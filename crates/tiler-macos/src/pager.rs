#![forbid(unsafe_code)]

//! Portable pager painter (macOS host styling, no AppKit).
//!
//! The core owns pager content: per-screen workspace order, ordinals,
//! current/occupied flags, the focused-workspace mark, and the ordered
//! miniature source rectangles ([`tiler_core::world_presentation`]). This
//! module styles that content with native host colours, exactly like the
//! Windows taskbar pager: the focused window's miniature carries the
//! independent pager highlight colour (full-strength border over a dimmer
//! fill of the same RGB; see [`crate::pager_highlight`]). Unfocused
//! miniatures stay plain gray. The menu-bar current-Space marker stays the
//! existing bordered bezel plus bold title (no accent fill is painted on
//! this surface).
//!
//! The live indicator tick consumes one applied reply presentation plus its
//! workspace bindings through [`indicator_miniatures`] per native Space and
//! draws the returned [`MiniatureDraw`] rects in the segment miniature
//! band. It never invents authority: a report with `paused` set
//! (focused-fullscreen-game lane) publishes no pager screens, so nothing
//! paints under the game; an empty or stale foreground token marks no
//! miniature; a focused mark needs BOTH the reply token equality and the
//! core `contains_focused` workspace flag, never the token alone (a stale
//! token over reordered content marks nothing until native observation
//! catches up); unmapped natives paint no miniatures. Renderer safety
//! vetoes stay: fullscreen/zoom/exclusion, floating/non-tiling membership,
//! session lock, lifetime, and Space fences are enforced by the core
//! content plus the native surface, never relaxed here. No underlay work
//! rides this path.

use tiler_core::world_presentation::WorldPresentation;

use crate::pager_highlight::{PagerHighlightStyle, dim_accent, effective_color};

/// Resolve the painter's highlight colour for one paint: the loop's
/// pager-highlight style plus the shared live accent snapshot. An explicit
/// border override never touches this (and vice versa): each side resolves
/// over its own style only.
#[must_use]
pub fn resolve_highlight(
    style: &PagerHighlightStyle,
    accent: Option<crate::active_border::SystemAccent>,
) -> (u8, u8, u8) {
    effective_color(style, accent)
}

/// One applied-reply workspace binding for the menu-bar pager: the stable
/// core logical workspace id plus the native desktop id the reply bound it
/// to (host-owned domains only; `None` in the default contract and for the
/// ordinary trailing spare). A current-reply live lookup, never a durable
/// host table: published per applied reply next to the presentation and
/// re-derived on every apply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PagerWorkspaceBinding {
    pub output: String,
    pub workspace: String,
    pub native: Option<String>,
}

/// Build the pager binding rows for one applied reply: every native domain
/// translated through the reply's `native_binding` rows (current AND hidden
/// domains — the indicator shows every Space, not just the actuated ones).
/// Unmapped natives (adopted later, never guessed) and the unbound trailing
/// spare (`None`) carry no row: their segments render exactly as today,
/// with no miniatures. Pure over the reply plus the domain list, so the
/// engine publish site and the offline tests share this one mapping.
#[must_use]
pub fn pager_bindings_for(
    domains: &[(String, String)],
    workspaces: &[tiler_core::world_report::WorkspaceDesire],
) -> Vec<PagerWorkspaceBinding> {
    let mut out = Vec::new();
    for (output, native) in domains {
        let Some(row) = workspaces.iter().find(|row| {
            row.output.0 == *output && row.native_binding.as_deref() == Some(native.as_str())
        }) else {
            continue;
        };
        out.push(PagerWorkspaceBinding {
            output: output.clone(),
            workspace: row.id.clone(),
            native: Some(native.clone()),
        });
    }
    out
}

/// Unfocused miniature fill: plain mid gray host styling (Windows "plain
/// gray" parity). The focused miniature instead carries the resolved
/// highlight border over the dimmer same-RGB fill.
pub const UNFOCUSED_MINIATURE_FILL: (u8, u8, u8) = (0x8c, 0x8c, 0x8c);

/// One drawable miniature for the menu-bar indicator: a point rectangle in
/// the segment's miniature band plus its resolved colours. The native
/// surface only fills/strokes these rects — all focus, layout, and colour
/// resolution lives here, so the offline tests prove exactly what the bar
/// draws. Focused miniatures carry the full-strength highlight border over
/// the dimmer same-RGB fill; unfocused miniatures carry the plain fill with
/// the border equal to the fill (never stroked).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MiniatureDraw {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
    pub focused: bool,
    pub border: (u8, u8, u8),
    pub fill: (u8, u8, u8),
}

/// Whether one core pager window is the focused miniature: the core
/// focused-home flag for its workspace AND the reply token naming exactly
/// this window. A stale token over reordered content marks nothing.
#[must_use]
pub fn miniature_focused(contains_focused: bool, foreground: &str, window: &str) -> bool {
    contains_focused && !foreground.is_empty() && window == foreground
}

/// Lay out one workspace's miniature source rectangles into a segment
/// miniature band (`band_w` x `band_h` points): the union bounding box
/// aspect-fits centered, so relative positions and sizes survive while the
/// absolute screen origin does not travel to the menu bar. Empty input or a
/// degenerate union paints nothing, never a guess.
#[must_use]
pub fn layout_miniatures(
    windows: &[tiler_core::world_presentation::PagerWindow],
    foreground: &str,
    contains_focused: bool,
    highlight: (u8, u8, u8),
    band_w: f64,
    band_h: f64,
) -> Vec<MiniatureDraw> {
    if windows.is_empty()
        || !band_w.is_finite()
        || band_w <= 0.0
        || !band_h.is_finite()
        || band_h <= 0.0
    {
        return Vec::new();
    }
    let (mut min_x, mut min_y) = (i32::MAX, i32::MAX);
    let (mut max_r, mut max_b) = (i32::MIN, i32::MIN);
    for window in windows {
        min_x = min_x.min(window.frame.x);
        min_y = min_y.min(window.frame.y);
        max_r = max_r.max(window.frame.x + window.frame.w);
        max_b = max_b.max(window.frame.y + window.frame.h);
    }
    let (uw, uh) = (max_r - min_x, max_b - min_y);
    if uw <= 0 || uh <= 0 {
        return Vec::new();
    }
    let scale = (band_w / f64::from(uw)).min(band_h / f64::from(uh));
    let (used_w, used_h) = (f64::from(uw) * scale, f64::from(uh) * scale);
    let (off_x, off_y) = ((band_w - used_w) / 2.0, (band_h - used_h) / 2.0);
    let fill = dim_accent(highlight);
    windows
        .iter()
        .map(|window| {
            let focused = miniature_focused(contains_focused, foreground, &window.window.0);
            MiniatureDraw {
                x: off_x + f64::from(window.frame.x - min_x) * scale,
                y: off_y + f64::from(window.frame.y - min_y) * scale,
                w: (f64::from(window.frame.w) * scale).max(1.0),
                h: (f64::from(window.frame.h) * scale).max(1.0),
                focused,
                border: if focused {
                    highlight
                } else {
                    UNFOCUSED_MINIATURE_FILL
                },
                fill: if focused {
                    fill
                } else {
                    UNFOCUSED_MINIATURE_FILL
                },
            }
        })
        .collect()
}

/// Production pager input for one indicator segment: resolve one native
/// Space to its drawable miniatures from one applied reply presentation
/// plus its workspace bindings. This is the exact function the live
/// indicator tick calls per segment (same highlight resolution, same
/// mapping, same layout): the offline wiring test calls it with
/// production-built inputs, so disconnecting the tick from this path fails
/// that test. Unmapped natives, empty workspaces, and the paused game lane
/// (no pager screens) all paint nothing.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn indicator_miniatures(
    presentation: &WorldPresentation,
    bindings: &[PagerWorkspaceBinding],
    native_sid: u64,
    highlight: (u8, u8, u8),
    band_w: f64,
    band_h: f64,
) -> Vec<MiniatureDraw> {
    if presentation.pager.is_empty() {
        return Vec::new();
    }
    let key = crate::topology::workspace_key(native_sid);
    let Some(binding) = bindings
        .iter()
        .find(|b| b.native.as_deref() == Some(key.as_str()))
    else {
        return Vec::new();
    };
    let Some(workspace) = presentation
        .pager
        .iter()
        .flat_map(|screen| screen.workspaces.iter())
        .find(|workspace| workspace.id == binding.workspace)
    else {
        return Vec::new();
    };
    layout_miniatures(
        &workspace.windows,
        &presentation.foreground,
        workspace.contains_focused,
        highlight,
        band_w,
        band_h,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_border::SystemAccent;
    use tiler_core::directional::{OutputId, WindowId};
    use tiler_core::geometry::Rect;
    use tiler_core::world_engine::{WorldEngine, WorldOutcome};
    use tiler_core::world_report::{
        PresentationIngress, ScreenReport, WORLD_CONTRACT_VERSION, WindowFact, WorldReply,
        WorldReport,
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
            hints: tiler_core::size_hints::WindowSizeHints::none(),
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
            underlay_enabled: false,
            border_gap_px: 0,
            border_width_px: 3,
            underlay_extension_px: 3,
            chord_held: false,
            ..PresentationIngress::disabled()
        }
    }

    fn live_frame(window: &str) -> tiler_core::world_report::PresentationPagerFrame {
        tiler_core::world_report::PresentationPagerFrame {
            window: WindowId(window.to_owned()),
            live: Some(rect(0, 0, 40, 90)),
            float_frame: None,
            tile_frame: None,
        }
    }

    fn report(
        request_seq: u64,
        observation_seq: u64,
        windows: Vec<WindowFact>,
        focused: &str,
        ingress: PresentationIngress,
    ) -> WorldReport {
        let mut ingress = ingress;
        ingress.pager_frames = windows
            .iter()
            .filter_map(|w| w.rect.map(|_| live_frame(&w.window.0)))
            .collect();
        ingress.pager_order = windows.iter().map(|w| w.window.clone()).collect();
        WorldReport {
            contract_version: WORLD_CONTRACT_VERSION,
            owner: "owner-a".to_owned(),
            adapter_incarnation: "adapter-1".to_owned(),
            core_generation: "gen-1".to_owned(),
            observation_seq,
            request_seq,
            request_id: format!("req-{request_seq}"),
            command_seq: None,
            command: None,
            workspace_mode: tiler_core::workspace_scope::WorkspaceMode::PerOutputLocal,
            screens: vec![screen("out-a", 0)],
            windows,
            views: Vec::new(),
            closed: vec![],
            host_owned_workspace_domains: false,
            equal_workspace_count: false,
            birth_clear: false,
            native: Vec::new(),
            fixed_size_predicate: tiler_core::size_hints::FixedSizePredicate::BothAxes,
            migration_source_refill: tiler_core::output_selection::MigrationSourceRefill::default(),
            anchor_fallback: tiler_core::admission::AnchorFallback::default(),
            no_space_result: tiler_core::admission::NoSpaceResult::default(),
            born_maximized: tiler_core::born_maximized::BornMaximized::default(),
            switch_target: tiler_core::cursor_targeting::SwitchTarget::default(),
            fresh_placement: tiler_core::cursor_targeting::FreshPlacement::default(),
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
            gesture: None,
            monotonic_ms: 0,
            frames: Vec::new(),
            member_attempts: Vec::new(),
            view_attempts: Vec::new(),
            clear_attempts: Vec::new(),
            creation_attempts: Vec::new(),
            presentation: ingress,
        }
    }

    fn accepted(engine: &mut WorldEngine, report: &WorldReport) -> WorldPresentation {
        match engine.handle_report(report) {
            WorldOutcome::Accepted(reply) => reply.presentation,
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    fn admitted_pair() -> (WorldEngine, WorldPresentation) {
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let members = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
        accepted(
            &mut engine,
            &report(1, 1, members.clone(), "win-a", ingress("win-a")),
        );
        // Second identical report lets managed focus latch behind the
        // newcomer before any content reads retained focus.
        accepted(
            &mut engine,
            &report(2, 2, members, "win-a", ingress("win-a")),
        );
        // Third report carries retained focus: the current workspace marks
        // the focused miniature.
        let presentation = accepted(
            &mut engine,
            &report(
                3,
                3,
                vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)],
                "win-a",
                ingress("win-a"),
            ),
        );
        (engine, presentation)
    }

    #[test]
    fn focused_miniature_carries_full_border_over_dimmer_same_rgb_fill() {
        let highlight = (0xbd, 0x93, 0xf9);
        let (_, presentation) = admitted_pair();
        let workspace = presentation
            .pager
            .iter()
            .flat_map(|s| &s.workspaces)
            .find(|w| w.contains_focused)
            .expect("exactly one workspace marks focused");
        // Layout through the production function the tick calls: the
        // focused miniature carries the full-strength border over the
        // dimmer same-RGB fill, unfocused miniatures stay plain.
        let draws = layout_miniatures(
            &workspace.windows,
            &presentation.foreground,
            workspace.contains_focused,
            highlight,
            26.0,
            8.0,
        );
        assert_eq!(draws.len(), workspace.windows.len());
        let focused: Vec<_> = draws.iter().filter(|d| d.focused).collect();
        assert_eq!(focused.len(), 1, "exactly one miniature is focused");
        assert_eq!(focused[0].border, highlight);
        assert_eq!(focused[0].fill, dim_accent(highlight));
        assert_ne!(focused[0].fill, focused[0].border);
        for draw in &draws {
            if !draw.focused {
                assert_eq!(draw.fill, UNFOCUSED_MINIATURE_FILL);
                assert_eq!(draw.border, UNFOCUSED_MINIATURE_FILL);
            }
        }
        // Geometry is real: every draw lands inside the band with a
        // positive size.
        for draw in &draws {
            assert!(draw.x >= 0.0 && draw.y >= 0.0);
            assert!(draw.x + draw.w <= 26.0 && draw.y + draw.h <= 8.0);
            assert!(draw.w >= 1.0 && draw.h >= 1.0);
        }
    }

    #[test]
    fn independent_highlight_override_never_touches_the_ring() {
        // A pager-highlight override changes the focused miniature while the
        // border style resolves unchanged, and vice versa.
        let accent = Some(SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        });
        let highlight_override = PagerHighlightStyle {
            color: (0xbd, 0x93, 0xf9),
            use_theme: false,
        };
        let border_style = crate::active_border::ActiveBorderStyle::default();
        assert_eq!(
            resolve_highlight(&highlight_override, accent),
            (0xbd, 0x93, 0xf9)
        );
        assert_eq!(
            crate::active_border::effective_color(&border_style, accent),
            (0xaa, 0xbb, 0xcc)
        );
        // Live accent change flips both independently: themed highlight
        // follows, explicit highlight stays, and the ring follows its own
        // switch only.
        let themed = PagerHighlightStyle::default();
        let accent_b = Some(SystemAccent {
            color: (0x11, 0x22, 0x33),
            alpha: 0xC4,
        });
        assert_eq!(resolve_highlight(&themed, accent), (0xaa, 0xbb, 0xcc));
        assert_eq!(resolve_highlight(&themed, accent_b), (0x11, 0x22, 0x33));
        assert_eq!(
            resolve_highlight(&highlight_override, accent_b),
            (0xbd, 0x93, 0xf9)
        );
        let explicit_border = crate::active_border::ActiveBorderStyle {
            use_theme: false,
            color: (0xff, 0x3b, 0x30),
            ..crate::active_border::ActiveBorderStyle::default()
        };
        assert_eq!(
            crate::active_border::effective_color(&explicit_border, accent_b),
            (0xff, 0x3b, 0x30)
        );
    }

    #[test]
    fn unknown_focus_marks_no_miniature() {
        let (_, presentation) = admitted_pair();
        let workspace = presentation
            .pager
            .iter()
            .flat_map(|s| &s.workspaces)
            .find(|w| w.contains_focused)
            .expect("focused workspace present");
        // Empty foreground focuses nothing, never a guess.
        let draws = layout_miniatures(&workspace.windows, "", true, (0xbd, 0x93, 0xf9), 26.0, 8.0);
        assert!(
            draws.iter().all(|d| !d.focused),
            "empty token marks no miniature"
        );
        // A stale token naming no member marks nothing even where the core
        // flag still holds the workspace.
        let draws = layout_miniatures(
            &workspace.windows,
            "ghost",
            true,
            (0xbd, 0x93, 0xf9),
            26.0,
            8.0,
        );
        assert!(
            draws.iter().all(|d| !d.focused),
            "stale token marks no miniature"
        );
    }

    #[test]
    fn highlight_colour_change_repaints_identical_geometry() {
        // The highlight colour rides the draw: identical content with a
        // different resolved colour paints different rects, so a
        // live-accent change repaints without any geometry move — and only
        // the focused styling moves, never the layout.
        let (_, presentation) = admitted_pair();
        let workspace = presentation
            .pager
            .iter()
            .flat_map(|s| &s.workspaces)
            .find(|w| w.contains_focused)
            .expect("focused workspace present");
        let first = layout_miniatures(
            &workspace.windows,
            &presentation.foreground,
            true,
            (0xbd, 0x93, 0xf9),
            26.0,
            8.0,
        );
        let second = layout_miniatures(
            &workspace.windows,
            &presentation.foreground,
            true,
            (0x11, 0x22, 0x33),
            26.0,
            8.0,
        );
        assert_eq!(first.len(), second.len());
        assert_ne!(first, second, "highlight change repaints");
        for (a, b) in first.iter().zip(second.iter()) {
            assert_eq!(
                (a.x, a.y, a.w, a.h),
                (b.x, b.y, b.w, b.h),
                "layout never moves"
            );
            assert_eq!(a.focused, b.focused);
        }
    }

    #[test]
    fn paused_game_lane_paints_no_screens() {
        // The focused-fullscreen-game lane publishes no pager screens, so no
        // miniature (and no highlight cost) runs under the game. A real
        // report with `paused` through the core proves the empty content.
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let members = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
        let mut pausing = ingress("win-a");
        pausing.paused = true;
        let presentation = accepted(&mut engine, &report(1, 1, members, "win-a", pausing));
        assert!(presentation.pager.is_empty(), "paused publishes no screens");
        // No pager screens means the tick resolves no miniatures for any
        // Space: nothing draws under the game.
        assert!(
            indicator_miniatures(&presentation, &[], 1, (0xbd, 0x93, 0xf9), 26.0, 8.0).is_empty()
        );
    }

    #[test]
    fn fullscreen_focus_hides_border_but_keeps_truthful_miniature() {
        // Eligibility contrast: a focused native fullscreen window hides the
        // active border (core fullscreen veto) while the pager still paints
        // its truthful member miniature. No positively grounded game
        // classification exists, so `paused` stays false and the game-lane
        // hide never fires for ordinary fullscreen.
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let mut members = vec![fact("win-a", "out-a", 0), fact("win-b", "out-a", 100)];
        members[0].fullscreen = true;
        let presentation = accepted(
            &mut engine,
            &report(1, 1, members, "win-a", {
                let mut styled = ingress("win-a");
                styled.foreground_fullscreen = true;
                styled
            }),
        );
        assert!(
            !presentation.border.visible,
            "fullscreen focus hides the ring"
        );
        assert!(!presentation.pager.is_empty(), "pager keeps member content");
        assert!(
            presentation
                .pager
                .iter()
                .flat_map(|s| &s.workspaces)
                .any(|w| w.contains_focused),
            "focused workspace still marks focused"
        );
    }

    fn native_fact(window: &str, native_ws: &str, x: i32) -> WindowFact {
        let mut row = fact(window, "out-a", x);
        row.native_workspace = Some(native_ws.to_owned());
        row
    }

    fn host_report(
        request_seq: u64,
        observation_seq: u64,
        windows: Vec<WindowFact>,
        focused: &str,
        ingress: PresentationIngress,
    ) -> WorldReport {
        let mut base = report(request_seq, observation_seq, windows, focused, ingress);
        base.host_owned_workspace_domains = true;
        base.native = vec![tiler_core::world_report::NativeObservation {
            output: OutputId("out-a".to_owned()),
            native_order: vec!["space-1".to_owned(), "space-2".to_owned()],
            current_native: Some("space-1".to_owned()),
        }];
        base
    }

    fn accepted_reply(engine: &mut WorldEngine, report: &WorldReport) -> WorldReply {
        match engine.handle_report(report) {
            WorldOutcome::Accepted(reply) => reply,
            other => panic!("expected accepted, got {other:?}"),
        }
    }

    #[test]
    fn indicator_path_consumes_live_reply_bindings_styles_and_accent() {
        // Production-path wiring: a host-owned report through the real
        // engine, bindings through the production mapper the dispatch site
        // publishes, highlight through the production settings resolver,
        // miniatures through the exact function the live tick calls per
        // segment. Disconnecting the tick from this path changes this
        // output and fails this test.
        use crate::dev_selection::{PagerHighlightCli, resolve_pager_highlight_config};
        use crate::workspace_indicator::segment_column_width;
        let mut engine = WorldEngine::new("gen-1").expect("generation");
        let members = || {
            vec![
                native_fact("win-a", "space-1", 0),
                native_fact("win-b", "space-2", 100),
            ]
        };
        accepted_reply(
            &mut engine,
            &host_report(1, 1, members(), "win-a", ingress("win-a")),
        );
        // Second identical report lets managed focus latch behind the
        // newcomer before any content reads retained focus.
        accepted_reply(
            &mut engine,
            &host_report(2, 2, members(), "win-a", ingress("win-a")),
        );
        let reply = accepted_reply(
            &mut engine,
            &host_report(3, 3, members(), "win-a", ingress("win-a")),
        );
        // Bindings through the production mapper over both native domains
        // (current AND hidden): each names the adopted logical workspace
        // from the reply's own native_binding rows.
        let bindings = pager_bindings_for(
            &[
                ("out-a".to_owned(), "space-1".to_owned()),
                ("out-a".to_owned(), "space-2".to_owned()),
                ("out-a".to_owned(), "space-9".to_owned()),
            ],
            &reply.workspaces,
        );
        assert_eq!(bindings.len(), 2, "unmapped natives carry no row");
        let adopted: Vec<&str> = bindings.iter().map(|b| b.workspace.as_str()).collect();
        assert_ne!(adopted[0], adopted[1], "distinct Spaces bind distinctly");
        // Highlight through the production settings resolver: explicit CLI
        // colour wins, theme defaults on.
        let style = resolve_pager_highlight_config(
            &PagerHighlightCli {
                color: Some("#bd93f9".to_owned()),
                use_theme: None,
            },
            None,
        )
        .expect("resolves");
        assert_eq!(style.color, (0xbd, 0x93, 0xf9));
        assert!(style.use_theme);
        // The live accent resolves per paint: a themed style follows it,
        // so an accent change propagates to drawable output.
        let accent_a = Some(crate::active_border::SystemAccent {
            color: (0xaa, 0xbb, 0xcc),
            alpha: 0xC4,
        });
        let accent_b = Some(crate::active_border::SystemAccent {
            color: (0x11, 0x22, 0x33),
            alpha: 0xC4,
        });
        let highlight_a = resolve_highlight(&style, accent_a);
        let highlight_b = resolve_highlight(&style, accent_b);
        assert_eq!(highlight_a, (0xaa, 0xbb, 0xcc));
        assert_eq!(highlight_b, (0x11, 0x22, 0x33));
        // Miniatures through the exact tick function, laid out in the
        // shared column width: Space 1 paints the focused miniature with
        // the full-strength border over the dimmer same-RGB fill, Space 2
        // paints its plain member, an unmapped Space paints nothing.
        let band_w = segment_column_width(false, false);
        let band_h = crate::workspace_indicator::MINIATURE_BAND_H;
        let focused = indicator_miniatures(
            &reply.presentation,
            &bindings,
            1,
            highlight_a,
            band_w,
            band_h,
        );
        assert_eq!(focused.len(), 1);
        assert!(focused[0].focused);
        assert_eq!(focused[0].border, highlight_a);
        assert_eq!(focused[0].fill, dim_accent(highlight_a));
        assert_ne!(focused[0].fill, focused[0].border);
        let plain = indicator_miniatures(
            &reply.presentation,
            &bindings,
            2,
            highlight_a,
            band_w,
            band_h,
        );
        assert_eq!(plain.len(), 1);
        assert!(!plain[0].focused);
        assert_eq!(plain[0].fill, UNFOCUSED_MINIATURE_FILL);
        assert!(
            indicator_miniatures(
                &reply.presentation,
                &bindings,
                9,
                highlight_a,
                band_w,
                band_h
            )
            .is_empty()
        );
        // Accent-change propagation: identical content with a new accent
        // repaints the focused miniature (border and fill move together),
        // while the unfocused fill stays put.
        let focused_b = indicator_miniatures(
            &reply.presentation,
            &bindings,
            1,
            highlight_b,
            band_w,
            band_h,
        );
        assert_ne!(focused, focused_b, "accent change repaints");
        assert_eq!(focused_b[0].border, highlight_b);
        let plain_b = indicator_miniatures(
            &reply.presentation,
            &bindings,
            2,
            highlight_b,
            band_w,
            band_h,
        );
        assert_eq!(plain, plain_b, "unfocused miniatures ignore the accent");
        // Ring independence: an explicit border override never touches the
        // highlight, and an explicit highlight never touches the ring.
        let explicit_border = crate::active_border::ActiveBorderStyle {
            use_theme: false,
            color: (0xff, 0x3b, 0x30),
            ..crate::active_border::ActiveBorderStyle::default()
        };
        assert_eq!(
            crate::active_border::effective_color(&explicit_border, accent_a),
            (0xff, 0x3b, 0x30)
        );
        let explicit_highlight = crate::pager_highlight::PagerHighlightStyle {
            color: (0xbd, 0x93, 0xf9),
            use_theme: false,
        };
        assert_eq!(
            resolve_highlight(&explicit_highlight, accent_b),
            (0xbd, 0x93, 0xf9)
        );
    }
}
