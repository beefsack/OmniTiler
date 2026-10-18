#![forbid(unsafe_code)]
//! Portable Windows group-underlay policy, settings, and fill-raster math.
//!
//! KDE parity source: `kwin/native-effect/activeborderconfig.kcfg`
//! (underlay enabled, `#662a82da`, extension -1 resolves to border width).
//! Shared Engine geometry lives in `tiler-core::visual`
//! (`group_underlay_effective_extension`, `group_underlay_outer_rect`) and
//! `tiler-core::active_group` (immediate-parent projected union); the trigger
//! OR lives in `tiler-core::visual::group_underlay_trigger`. This module adds
//! only the Windows-side composition: options/flags parsing, hide reasons,
//! the native hit-test classification, modifier aggregation, and the
//! premultiplied-alpha fill raster. Native creation/presentation/placement
//! reuse the owned carrier in [`crate::active_border_sys`].

use tiler_core::directional::{OutputId, WindowId, WorkspaceId};
use tiler_core::geometry::Rect;
use tiler_core::seed::EngineWindow;
use tiler_core::size_hints::WindowSizeHints;
use tiler_core::visual::{VisualRect, group_focus_eligible};

/// Underlay default: on (mirrors the KDE highlight default).
pub const DEFAULT_ENABLED: bool = true;
/// KDE-parity default fill `#662a82da` (ARGB): alpha 0x66 over accent blue.
pub const DEFAULT_COLOR_ARGB: (u8, u8, u8, u8) = (0x66, 0x2a, 0x82, 0xda);
/// Extension sentinel: -1 follows the active-border width (KDE parity).
pub const DEFAULT_EXTENSION: f64 = -1.0;
/// Explicit extension range: the -1 sentinel or a 0..=32 logical width.
pub const MAX_EXTENSION: f64 = 32.0;

/// Configured underlay style. Colour is the KDE ARGB fill; extension is the
/// logical KDE-point outset beyond gap plus border width (-1 follows border).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupUnderlayStyle {
    pub color: (u8, u8, u8),
    pub alpha: u8,
    pub extension: f64,
}

impl Default for GroupUnderlayStyle {
    fn default() -> Self {
        Self {
            color: (
                DEFAULT_COLOR_ARGB.1,
                DEFAULT_COLOR_ARGB.2,
                DEFAULT_COLOR_ARGB.3,
            ),
            alpha: DEFAULT_COLOR_ARGB.0,
            extension: DEFAULT_EXTENSION,
        }
    }
}

/// Full underlay switch: default on, explicit `--no-group-underlay` off.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GroupUnderlayOptions {
    pub enabled: bool,
    pub style: GroupUnderlayStyle,
}

impl Default for GroupUnderlayOptions {
    fn default() -> Self {
        Self {
            enabled: DEFAULT_ENABLED,
            style: GroupUnderlayStyle::default(),
        }
    }
}

/// Parse `#aarrggbb` (case-insensitive, `#` required): the KDE ARGB config
/// form, alpha first. Six-digit `#rrggbb` is refused: a missing alpha must
/// never silently become opaque or transparent.
pub fn parse_color_argb(value: &str) -> Result<((u8, u8, u8), u8), String> {
    let usage = "refuse: --group-underlay-color needs #aarrggbb";
    let hex = value.strip_prefix('#').ok_or_else(|| usage.to_owned())?;
    if hex.len() != 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(usage.to_owned());
    }
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).map_err(|_| usage.to_owned())
    };
    Ok((
        (channel(2..4)?, channel(4..6)?, channel(6..8)?),
        channel(0..2)?,
    ))
}

/// Parse a logical extension: exactly -1 (follow border width) or 0..=32.
pub fn parse_extension(value: &str) -> Result<f64, String> {
    let usage = "refuse: --group-underlay-extension needs -1..=32";
    let parsed: f64 = value.parse().map_err(|_| usage.to_owned())?;
    if !parsed.is_finite() || (parsed != -1.0 && (parsed < 0.0 || parsed > MAX_EXTENSION)) {
        return Err(usage.to_owned());
    }
    Ok(parsed)
}

/// Render a colour back to `#aarrggbb`.
#[must_use]
pub fn render_color_argb(((r, g, b), alpha): ((u8, u8, u8), u8)) -> String {
    format!("#{alpha:02x}{r:02x}{g:02x}{b:02x}")
}

/// Focused-only interactive move/size classification from the official
/// `WM_NCHITTEST` result sampled at move/size start. `HTCAPTION` starts a
/// move; sizing borders start a resize; anything else (client, menus,
/// nowhere, errors, timeouts) is unknown and never triggers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveSizeKind {
    Move,
    Resize,
    Unknown,
}

/// Classify one raw `WM_NCHITTEST` result (`HT*` codes are stable Win32 ABI:
/// `HTNOWHERE` 0, `HTCLIENT` 1, `HTCAPTION` 2, `HTSYSMENU` 3, sizing borders
/// 10..=17).
#[must_use]
pub fn classify_hit_test(hit: u32) -> MoveSizeKind {
    if hit == 2 {
        MoveSizeKind::Move
    } else if (10..=17).contains(&hit) {
        MoveSizeKind::Resize
    } else {
        MoveSizeKind::Unknown
    }
}

/// Fold raw modifier key states into the `(win, shift)` pair the shared chord
/// predicate consumes. Either side counts; extras are ignored (allowed), and
/// level sampling makes either press order work.
#[must_use]
pub const fn aggregate_chord_keys(
    win_l: bool,
    win_r: bool,
    shift: bool,
    shift_l: bool,
    shift_r: bool,
) -> (bool, bool) {
    (win_l || win_r, shift || shift_l || shift_r)
}

/// Closed hide-reason vocabulary for the underlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnderlayHideReason {
    Disabled,
    NoSubject,
    Maximized,
    Fullscreen,
    Floating,
    RootLeaf,
    NoGroup,
}

impl UnderlayHideReason {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NoSubject => "no-subject",
            Self::Maximized => "maximized",
            Self::Fullscreen => "fullscreen",
            Self::Floating => "floating",
            Self::RootLeaf => "root-leaf",
            Self::NoGroup => "no-group",
        }
    }
}

/// Underlay eligibility for one focused subject. `has_group` is the Engine
/// `ActiveGroup` verdict (false covers root-leaf focus); `floating` is a
/// focused window with no tiled membership. Maximized/fullscreen come from
/// the fresh foreground facts. Reasons derive after the verdict, mirroring
/// the border pattern.
pub fn underlay_eligible(
    enabled: bool,
    has_subject: bool,
    has_group: bool,
    maximized: bool,
    fullscreen: bool,
    floating: bool,
) -> Result<(), UnderlayHideReason> {
    if !enabled {
        return Err(UnderlayHideReason::Disabled);
    }
    if !has_subject {
        return Err(UnderlayHideReason::NoSubject);
    }
    if maximized {
        return Err(UnderlayHideReason::Maximized);
    }
    if fullscreen {
        return Err(UnderlayHideReason::Fullscreen);
    }
    if floating {
        return Err(UnderlayHideReason::Floating);
    }
    if !has_group {
        return Err(UnderlayHideReason::RootLeaf);
    }
    // The shared core focus gate agrees on the overlapping flags: a visible
    // non-deleted foreground subject is eligible exactly when neither
    // fullscreen nor maximized suppresses it.
    if !group_focus_eligible(true, false, false, fullscreen, false, maximized) {
        return Err(UnderlayHideReason::NoGroup);
    }
    Ok(())
}

fn visual_of(rect: Rect) -> VisualRect {
    VisualRect {
        x: rect.x as f64,
        y: rect.y as f64,
        w: rect.w as f64,
        h: rect.h as f64,
    }
}

/// Outer rectangle: the Engine-projected group union expanded by the physical
/// gap, border width, and resolved extension on every side, via the shared
/// `tiler-core::visual` expansion. `None` on overflow or degenerate output.
#[must_use]
pub fn underlay_outer_rect(
    union_rect: Rect,
    gap_px: i32,
    width_px: i32,
    extension_px: i32,
) -> Option<Rect> {
    if gap_px < 0 || width_px < 0 || extension_px < 0 {
        return None;
    }
    // Physical pixels throughout: the -1 extension sentinel is resolved
    // against the border width in logical units and scaled before this call,
    // so the resolved outset here is literal.
    let outer = tiler_core::visual::group_underlay_outer_rect(
        visual_of(union_rect),
        f64::from(gap_px),
        f64::from(width_px),
        f64::from(extension_px),
    );
    let x = outer.x.round() as i64;
    let y = outer.y.round() as i64;
    let w = outer.w.round() as i64;
    let h = outer.h.round() as i64;
    if w <= 0
        || h <= 0
        || w > i64::from(i32::MAX)
        || h > i64::from(i32::MAX)
        || x < i64::from(i32::MIN)
        || x > i64::from(i32::MAX)
        || y < i64::from(i32::MIN)
        || y > i64::from(i32::MAX)
    {
        return None;
    }
    if union_rect.w <= 0 || union_rect.h <= 0 {
        return None;
    }
    Some(Rect {
        x: x as i32,
        y: y as i32,
        w: w as i32,
        h: h as i32,
    })
}

/// Bounded geometry diagnostics for shown/redrew `group-underlay` log lines.
///
/// Pure payload construction only: no behavior change, no new log lines, and
/// no user content. All identities are opaque Engine tokens/ids; all geometry
/// is numeric. Lets the next live trace separate a stale Engine union from a
/// wrong immediate-parent subtree from adapter-side pad/DPI/domain math.
/// Input bundle for [`underlay_geometry_debug`]: keeps the diagnostics call
/// to one argument (strict Clippy) while staying a pure payload with opaque
/// ids and numeric geometry only.
pub struct UnderlayGeometryReport<'a> {
    pub union_rect: Rect,
    pub group: &'a str,
    pub focused_leaf: &'a str,
    pub member_windows: &'a [String],
    pub member_rects: &'a [Rect],
    pub dpi: u32,
    pub gap_px: i32,
    pub width_px: i32,
    pub extension_px: i32,
    pub domain_bounds: Rect,
    pub domain_gap: i32,
    pub base_revision: u64,
}

#[must_use]
pub fn underlay_geometry_debug(report: UnderlayGeometryReport<'_>) -> serde_json::Value {
    serde_json::json!({
        "union": [report.union_rect.x, report.union_rect.y, report.union_rect.w, report.union_rect.h],
        "group": report.group,
        "focused_leaf": report.focused_leaf,
        "member_windows": report.member_windows,
        "member_rects": report.member_rects
            .iter()
            .map(|rect| [rect.x, rect.y, rect.w, rect.h])
            .collect::<Vec<[i32; 4]>>(),
        "dpi": report.dpi,
        "gap_px": report.gap_px,
        "width_px": report.width_px,
        "extension_px": report.extension_px,
        "domain_bounds": [
            report.domain_bounds.x,
            report.domain_bounds.y,
            report.domain_bounds.w,
            report.domain_bounds.h
        ],
        "domain_gap": report.domain_gap,
        "base_revision": report.base_revision,
    })
}
/// Fill a BGRA buffer with the premultiplied-alpha underlay colour: straight
/// RGB scaled by `alpha/255` (rounded) with the alpha byte itself, over the
/// whole surface (the DIB is exactly the outer rect). Returns the FNV-1a
/// checksum over the owned bytes in the same pass. Non-premultiplied input
/// would render wrong under `ULW_ALPHA`: layered composition requires
/// premultiplied source.
pub fn paint_fill_argb(pixels: &mut [u8], w: i32, h: i32, color: (u8, u8, u8), alpha: u8) -> u64 {
    let (r, g, b) = color;
    // Straight-to-premultiplied with rounding: (c * a + 127) / 255.
    let premultiplied = |c: u8| ((u16::from(c) * u16::from(alpha) + 127) / 255) as u8;
    let (pr, pg, pb) = (premultiplied(r), premultiplied(g), premultiplied(b));
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    };
    for y in 0..h {
        for x in 0..w {
            let idx = (y as usize * w as usize + x as usize) * 4;
            if idx + 3 >= pixels.len() {
                continue;
            }
            pixels[idx] = pb;
            pixels[idx + 1] = pg;
            pixels[idx + 2] = pr;
            pixels[idx + 3] = alpha;
            mix(pb);
            mix(pg);
            mix(pr);
            mix(alpha);
        }
    }
    hash
}

/// One member's hint contribution to an ActiveGroup query: opaque identities
/// plus the retained advisory minimums. No geometry, no topology.
pub struct UnderlayHintMember<'a> {
    pub token: &'a str,
    pub output: &'a str,
    pub workspace: &'a str,
    pub hints: WindowSizeHints,
}

/// Hints-only ActiveGroup query entries for one domain's members: the same
/// retained per-window minimums the ordinary plan for this state used, so the
/// Engine projects the group union through the identical hints-aware
/// projector. Rectangles are empty on purpose: the resolver reads window and
/// hints alone and never consults carried geometry or membership. No window,
/// hook, or placement effect; pure data assembly.
#[must_use]
pub fn underlay_hint_windows(members: &[UnderlayHintMember<'_>]) -> Vec<EngineWindow> {
    members
        .iter()
        .map(|member| EngineWindow {
            window: WindowId(member.token.to_owned()),
            output: OutputId(member.output.to_owned()),
            workspace: WorkspaceId(member.workspace.to_owned()),
            rect: Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0,
            },
            floating: false,
            fit_excluded: false,
            fullscreen: false,
            maximized: false,
            sticky: false,
            fixed_auto: false,
            fixed_suppress: false,
            hints: member.hints,
        })
        .collect()
}

/// Echo of parsed underlay flags for argv-consistency verification.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UnderlayArgvEcho {
    pub no_group_underlay: bool,
    pub color: Option<String>,
    pub extension: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect {
        Rect { x, y, w, h }
    }

    #[test]
    fn defaults_match_kde_parity() {
        let options = GroupUnderlayOptions::default();
        assert!(options.enabled);
        assert_eq!(
            (options.style.color, options.style.alpha),
            ((0x2a, 0x82, 0xda), 0x66)
        );
        assert_eq!(options.style.extension, -1.0);
        assert_eq!(
            render_color_argb((options.style.color, options.style.alpha)),
            "#662a82da"
        );
        // Explicitly saved colors still parse, including the superseded
        // default and the dogfood report value: no migration, schema v1.
        assert_eq!(
            parse_color_argb("#40808080"),
            Ok(((0x80, 0x80, 0x80), 0x40))
        );
        assert_eq!(
            parse_color_argb("#50909090"),
            Ok(((0x90, 0x90, 0x90), 0x50))
        );
    }

    #[test]
    fn color_requires_aarrggbb() {
        assert_eq!(
            parse_color_argb("#662a82da"),
            Ok(((0x2a, 0x82, 0xda), 0x66))
        );
        assert_eq!(
            parse_color_argb("#40808080"),
            Ok(((0x80, 0x80, 0x80), 0x40))
        );
        assert_eq!(
            parse_color_argb("#FF112233"),
            Ok(((0x11, 0x22, 0x33), 0xff))
        );
        assert!(parse_color_argb("#808080").is_err());
        assert!(parse_color_argb("40808080").is_err());
        assert!(parse_color_argb("#4080808").is_err());
        assert!(parse_color_argb("#408080800").is_err());
        assert!(parse_color_argb("#zzzzzzzz").is_err());
    }

    #[test]
    fn extension_accepts_sentinel_or_bounded_width() {
        assert_eq!(parse_extension("-1"), Ok(-1.0));
        assert_eq!(parse_extension("0"), Ok(0.0));
        assert_eq!(parse_extension("32"), Ok(32.0));
        assert!(parse_extension("-2").is_err());
        assert!(parse_extension("-0.5").is_err());
        assert!(parse_extension("32.1").is_err());
        assert!(parse_extension("nan").is_err());
    }

    #[test]
    fn hit_test_classifies_move_vs_resize() {
        use MoveSizeKind::{Move, Resize, Unknown};
        assert_eq!(classify_hit_test(2), Move);
        for hit in [10, 11, 12, 13, 14, 15, 16, 17] {
            assert_eq!(classify_hit_test(hit), Resize, "hit={hit}");
        }
        for hit in [0, 1, 3, 4, 5, 9, 18, 0xffff] {
            assert_eq!(classify_hit_test(hit), Unknown, "hit={hit}");
        }
    }

    #[test]
    fn modifier_aggregation_either_side_extras_ignored() {
        // (win_l, win_r, shift, shift_l, shift_r) -> (win, shift).
        for (keys, want) in [
            ((false, false, false, false, false), (false, false)),
            ((true, false, false, false, false), (true, false)),
            ((false, true, false, false, false), (true, false)),
            ((false, false, true, false, false), (false, true)),
            ((false, false, false, true, false), (false, true)),
            ((false, false, false, false, true), (false, true)),
            ((true, false, false, true, false), (true, true)),
            ((false, true, true, false, false), (true, true)),
            ((true, true, true, true, true), (true, true)),
        ] {
            let (wl, wr, s, sl, sr) = keys;
            assert_eq!(aggregate_chord_keys(wl, wr, s, sl, sr), want);
        }
    }

    #[test]
    fn eligibility_suppression_matrix() {
        let frame_ok = || underlay_eligible(true, true, true, false, false, false);
        assert!(frame_ok().is_ok());
        assert_eq!(
            underlay_eligible(false, true, true, false, false, false),
            Err(UnderlayHideReason::Disabled)
        );
        assert_eq!(
            underlay_eligible(true, false, true, false, false, false),
            Err(UnderlayHideReason::NoSubject)
        );
        assert_eq!(
            underlay_eligible(true, true, true, true, false, false),
            Err(UnderlayHideReason::Maximized)
        );
        assert_eq!(
            underlay_eligible(true, true, true, false, true, false),
            Err(UnderlayHideReason::Fullscreen)
        );
        assert_eq!(
            underlay_eligible(true, true, true, false, false, true),
            Err(UnderlayHideReason::Floating)
        );
        assert_eq!(
            underlay_eligible(true, true, false, false, false, false),
            Err(UnderlayHideReason::RootLeaf)
        );
    }

    #[test]
    fn eligibility_agrees_with_core_focus_gate() {
        // Whenever the underlay verdict is Ok, the shared core focus gate on
        // the overlapping flags agrees; whenever core suppresses, so do we.
        for bits in 0..4u32 {
            let maximized = bits & 1 != 0;
            let fullscreen = bits & 2 != 0;
            let core = group_focus_eligible(true, false, false, fullscreen, false, maximized);
            let verdict = underlay_eligible(true, true, true, maximized, fullscreen, false).is_ok();
            assert_eq!(verdict, core, "bits={bits:02b}");
        }
    }

    #[test]
    fn outer_expands_union_by_gap_width_extension() {
        // Union 100x80 at (10,20), gap 2, width 4, extension 3: pad 9.
        assert_eq!(
            underlay_outer_rect(rect(10, 20, 100, 80), 2, 4, 3),
            Some(rect(1, 11, 118, 98))
        );
        // Zero pad is the identity.
        assert_eq!(
            underlay_outer_rect(rect(10, 20, 100, 80), 0, 0, 0),
            Some(rect(10, 20, 100, 80))
        );
        // Degenerate or negative inputs fail closed.
        assert_eq!(underlay_outer_rect(rect(10, 20, 0, 80), 2, 4, 3), None);
        assert_eq!(underlay_outer_rect(rect(10, 20, 100, 80), -1, 4, 3), None);
    }

    #[test]
    fn geometry_debug_carries_opaque_ids_and_numeric_inputs() {
        // Regression for the short-underlay trace: the next live line must
        // carry the Engine union, immediate-parent group/leaf, member rects,
        // DPI/pad inputs, and domain/revision so a partial outer can be
        // attributed without new behavior. Pure payload, opaque ids only.
        let debug = underlay_geometry_debug(UnderlayGeometryReport {
            union_rect: rect(8, 496, 2544, 876),
            group: "inner",
            focused_leaf: "b-leaf",
            member_windows: &["win-2".to_owned(), "win-3".to_owned()],
            member_rects: &[rect(8, 496, 2544, 750), rect(8, 1254, 2544, 118)],
            dpi: 120,
            gap_px: 0,
            width_px: 4,
            extension_px: 4,
            domain_bounds: rect(0, 0, 2560, 1380),
            domain_gap: 8,
            base_revision: 42,
        });
        assert_eq!(debug["union"], serde_json::json!([8, 496, 2544, 876]));
        assert_eq!(debug["group"], serde_json::json!("inner"));
        assert_eq!(debug["focused_leaf"], serde_json::json!("b-leaf"));
        assert_eq!(
            debug["member_windows"],
            serde_json::json!(["win-2", "win-3"])
        );
        assert_eq!(
            debug["member_rects"],
            serde_json::json!([[8, 496, 2544, 750], [8, 1254, 2544, 118]])
        );
        assert_eq!(debug["dpi"], serde_json::json!(120));
        assert_eq!(debug["gap_px"], serde_json::json!(0));
        assert_eq!(debug["width_px"], serde_json::json!(4));
        assert_eq!(debug["extension_px"], serde_json::json!(4));
        assert_eq!(
            debug["domain_bounds"],
            serde_json::json!([0, 0, 2560, 1380])
        );
        assert_eq!(debug["domain_gap"], serde_json::json!(8));
        assert_eq!(debug["base_revision"], serde_json::json!(42));
    }

    #[test]
    fn active_group_union_without_hints_replays_tick136_short_outer() {
        // Tick-136 replay from the short-underlay trace: one retained share
        // state, two projections. The hinted Engine plan must equal the
        // logged tick-136 plan exactly, while the hint-free ActiveGroup union
        // of the focused bottom pair must equal the logged short outer's
        // source union ([8,785,2544,587] -> [0,777,2560,603] at pad 8).
        // Pure Engine replay, no windows, no writes.
        use std::collections::BTreeMap;
        use tiler_core::active_group::describe_active_group;
        use tiler_core::directional::{Axis, Node, NodeId, WindowId};
        use tiler_core::size_hints::{WindowSizeHints, project_with_hints};

        fn leaf(id: &str) -> Node {
            Node::Leaf {
                id: NodeId::from(id),
            }
        }
        fn hints(min_w: i32, min_h: i32) -> WindowSizeHints {
            WindowSizeHints {
                min_w: Some(min_w),
                min_h: Some(min_h),
                max_w: None,
                max_h: None,
            }
        }

        // Nested vertical stack with the retained shares that the hinted
        // plan below implies (the logged members:2 already proves the
        // focused pair sits in an inner group); domain bounds are the
        // 8px-inset work area.
        let tree = Node::Group {
            id: NodeId::from("root"),
            axis: Axis::Vertical,
            children: vec![
                leaf("w25-leaf"),
                Node::Group {
                    id: NodeId::from("inner"),
                    axis: Axis::Vertical,
                    children: vec![leaf("w24-leaf"), leaf("w23-leaf")],
                    shares: vec![461, 117],
                },
            ],
            shares: vec![765, 583],
        };
        let map: BTreeMap<NodeId, WindowId> = [
            ("w25-leaf", "w25"),
            ("w24-leaf", "w24"),
            ("w23-leaf", "w23"),
        ]
        .into_iter()
        .map(|(leaf, window)| (NodeId::from(leaf), WindowId::from(window)))
        .collect();
        let bounds = rect(8, 8, 2544, 1364);
        let resolve = |leaf: &NodeId| match leaf.0.as_str() {
            "w25-leaf" => hints(401, 246),
            "w24-leaf" => hints(1263, 750),
            "w23-leaf" => hints(582, 118),
            _ => WindowSizeHints::none(),
        };
        // The regular Reconcile/move path honors minimums: byte-exact tick-136 plan.
        let hinted = project_with_hints(&tree, bounds, 8, &resolve).expect("hinted projection");
        assert!(hinted.overconstrained.is_empty());
        let hinted_rects: Vec<Rect> = hinted.leaves.iter().map(|leaf| leaf.rect).collect();
        assert_eq!(
            hinted_rects,
            vec![
                rect(8, 8, 2544, 480),
                rect(8, 496, 2544, 750),
                rect(8, 1254, 2544, 118),
            ],
            "hinted plan must equal the logged tick-136 plan"
        );
        // The ActiveGroup path ignores minimums: the focused bottom pair
        // unions to the short source behind the logged outer.
        let group = describe_active_group(&tree, bounds, 8, &NodeId::from("w24-leaf"), &map)
            .expect("immediate parent resolves");
        assert_eq!(group.group, NodeId::from("inner"));
        assert_eq!(group.members.len(), 2);
        assert_eq!(group.bounds, rect(8, 785, 2544, 587));
        assert_eq!(
            underlay_outer_rect(group.bounds, 0, 4, 4),
            Some(rect(0, 777, 2560, 603)),
            "hint-free union must yield the logged tick-136 outer"
        );
        // The same members under hints cover the full pair: the 289px top
        // delta is exactly w24's hint deficit (750 - 461).
        assert_eq!(
            hinted_rects[1].y.min(hinted_rects[2].y),
            496,
            "hinted pair top follows the enforced w24 minimum"
        );
        assert_eq!(group.bounds.y, 785);
        assert_eq!(785 - 496, 289);
    }

    #[test]
    fn active_group_union_without_hints_shorts_nested_parent_top() {
        // Nested V[W1 H[W2 W3]] replay with the trace's min sizes: the H
        // union is split-invariant, but the H rectangle itself sits lower
        // without hints because the outer split ignores the H subtree
        // minimum. Result is bottom-anchored and short at the top, the
        // reported symptom shape. Pure Engine replay, no windows, no writes.
        use std::collections::BTreeMap;
        use tiler_core::active_group::describe_active_group;
        use tiler_core::directional::{Axis, Node, NodeId, WindowId};
        use tiler_core::size_hints::{WindowSizeHints, project_with_hints};

        fn leaf(id: &str) -> Node {
            Node::Leaf {
                id: NodeId::from(id),
            }
        }
        fn hints(min_w: i32, min_h: i32) -> WindowSizeHints {
            WindowSizeHints {
                min_w: Some(min_w),
                min_h: Some(min_h),
                max_w: None,
                max_h: None,
            }
        }

        let tree = Node::Group {
            id: NodeId::from("root"),
            axis: Axis::Vertical,
            children: vec![
                leaf("w1-leaf"),
                Node::Group {
                    id: NodeId::from("inner"),
                    axis: Axis::Horizontal,
                    children: vec![leaf("w2-leaf"), leaf("w3-leaf")],
                    shares: vec![1268, 1268],
                },
            ],
            shares: vec![800, 548],
        };
        let map: BTreeMap<NodeId, WindowId> =
            [("w1-leaf", "w1"), ("w2-leaf", "w2"), ("w3-leaf", "w3")]
                .into_iter()
                .map(|(leaf, window)| (NodeId::from(leaf), WindowId::from(window)))
                .collect();
        let bounds = rect(8, 8, 2544, 1364);
        let resolve = |leaf: &NodeId| match leaf.0.as_str() {
            "w1-leaf" => hints(401, 246),
            "w2-leaf" => hints(1263, 750),
            "w3-leaf" => hints(582, 118),
            _ => WindowSizeHints::none(),
        };
        let hinted = project_with_hints(&tree, bounds, 8, &resolve).expect("hinted projection");
        assert!(hinted.overconstrained.is_empty());
        // Child order: W1, then H's W2, W3. The bottom pair matches the live
        // side-by-side layout shape (1268-wide cells, full H height).
        assert_eq!(
            hinted
                .leaves
                .iter()
                .map(|leaf| leaf.rect)
                .collect::<Vec<Rect>>(),
            vec![
                rect(8, 8, 2544, 606),
                rect(8, 622, 1268, 750),
                rect(1284, 622, 1268, 750),
            ]
        );
        let group = describe_active_group(&tree, bounds, 8, &NodeId::from("w2-leaf"), &map)
            .expect("immediate parent resolves");
        assert_eq!(group.group, NodeId::from("inner"));
        assert_eq!(group.members.len(), 2);
        assert_eq!(group.bounds, rect(8, 820, 2544, 552));
        assert_eq!(
            underlay_outer_rect(group.bounds, 0, 4, 4),
            Some(rect(0, 812, 2560, 568)),
            "hint-free H union stays bottom-anchored but short at the top"
        );
    }

    #[test]
    fn hint_windows_carry_hints_without_geometry() {
        // The ActiveGroup query attaches retained minimums only: opaque
        // window identity plus hints, an empty rectangle, and no flags, so
        // the resolver cannot read geometry or membership from the carrier.
        let members = [
            UnderlayHintMember {
                token: "w24",
                output: "o1",
                workspace: "ws1",
                hints: WindowSizeHints {
                    min_w: Some(1263),
                    min_h: Some(750),
                    max_w: None,
                    max_h: None,
                },
            },
            UnderlayHintMember {
                token: "w25",
                output: "o1",
                workspace: "ws1",
                hints: WindowSizeHints::none(),
            },
        ];
        let windows = underlay_hint_windows(&members);
        assert_eq!(windows.len(), 2);
        assert_eq!(windows[0].window.0, "w24");
        assert_eq!(windows[0].hints.min_h, Some(750));
        assert_eq!(
            windows[0].rect,
            Rect {
                x: 0,
                y: 0,
                w: 0,
                h: 0
            }
        );
        assert!(!windows[0].floating);
        assert_eq!(windows[1].hints, WindowSizeHints::none());
        assert!(underlay_hint_windows(&[]).is_empty());
    }

    #[test]
    fn fill_raster_is_premultiplied() {
        // Grey at 0x40 alpha: 0x80*0x40/255 rounds to 0x20.
        let mut pixels = vec![0u8; 2 * 2 * 4];
        let checksum = paint_fill_argb(&mut pixels, 2, 2, (0x80, 0x80, 0x80), 0x40);
        assert_eq!(
            &pixels[0..4],
            &[0x20, 0x20, 0x20, 0x40],
            "BGRA premultiplied"
        );
        assert_eq!(pixels.len(), 16);
        // Checksum is stable and content-sensitive.
        let mut other = vec![0u8; 2 * 2 * 4];
        let same = paint_fill_argb(&mut other, 2, 2, (0x80, 0x80, 0x80), 0x40);
        assert_eq!(same, checksum);
        let mut opaque = vec![0u8; 2 * 2 * 4];
        let different = paint_fill_argb(&mut opaque, 2, 2, (0x80, 0x80, 0x80), 0xff);
        assert_ne!(different, checksum);
        assert_eq!(&opaque[0..4], &[0x80, 0x80, 0x80, 0xff]);
        // Candidate-C default accent blue at 0x66 alpha premultiplies to
        // BGRA [0x57, 0x34, 0x11, 0x66].
        let mut accent = vec![0u8; 2 * 2 * 4];
        paint_fill_argb(&mut accent, 2, 2, (0x2a, 0x82, 0xda), 0x66);
        assert_eq!(&accent[0..4], &[0x57, 0x34, 0x11, 0x66]);
    }
}
