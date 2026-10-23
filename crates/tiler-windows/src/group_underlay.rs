#![forbid(unsafe_code)]
//! Portable Windows group-underlay policy, settings, and fill-raster math.
//!
//! KDE parity source: `kwin/native-effect/activeborderconfig.kcfg`
//! (underlay enabled, `#662a82da`, extension -1 resolves to border width).
//! Eligibility, trigger, union, and outer bounds live in `tiler-core`
//! (`visual::underlay_eligible`, `visual::group_underlay_trigger`,
//! `visual::group_underlay_outer_rect_px`, `active_group`, and the
//! read-only `world_presentation::WorldEngine::group_highlight` query).
//! This module adds only the Windows-side composition: options/flags
//! parsing, the native hit-test classification, modifier aggregation, and
//! the premultiplied-alpha fill raster. Native creation/presentation/
//! placement reuse the owned carrier in [`crate::active_border_sys`].

use tiler_core::geometry::Rect;

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

/// START probe outcome for one open move/size gesture: distinguishes an
/// unreadable cursor, an unavailable hit-test (hung window, timeout, or
/// failed send), and a successful raw `WM_NCHITTEST` result. Classification
/// still flows through `classify_hit_test` unchanged.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveSizeProbe {
    CursorUnavailable,
    HitTestUnavailable,
    Hit(u32),
}

/// One sampled START classification plus its probe facts: the single
/// cursor read and the single hit-test send that produced it, so the
/// `move-kind` diagnostic can attribute an `Unknown` without re-probing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MoveSizeStart {
    pub kind: MoveSizeKind,
    pub cursor: Option<(i32, i32)>,
    pub probe: MoveSizeProbe,
}

/// Pure START mapping over one sampled cursor/hit-test pair. The
/// classification is exactly `classify_hit_test` on a successful hit;
/// anything earlier is `Unknown` with the failing stage recorded. No
/// sampling here: the caller performs the single native sample once.
#[must_use]
pub fn classify_move_size_probe(cursor: Option<(i32, i32)>, hit: Option<u32>) -> MoveSizeStart {
    match (cursor, hit) {
        (None, _) => MoveSizeStart {
            kind: MoveSizeKind::Unknown,
            cursor: None,
            probe: MoveSizeProbe::CursorUnavailable,
        },
        (Some(point), None) => MoveSizeStart {
            kind: MoveSizeKind::Unknown,
            cursor: Some(point),
            probe: MoveSizeProbe::HitTestUnavailable,
        },
        (Some(point), Some(raw)) => MoveSizeStart {
            kind: classify_hit_test(raw),
            cursor: Some(point),
            probe: MoveSizeProbe::Hit(raw),
        },
    }
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
    fn move_size_probe_attributes_unknown_stage() {
        use MoveSizeKind::{Move, Resize, Unknown};
        use MoveSizeProbe::{CursorUnavailable, Hit, HitTestUnavailable};
        // No cursor: the sampler never sends, and the hit value (even a
        // would-be move) must not decide.
        for hit in [None, Some(2)] {
            assert_eq!(
                classify_move_size_probe(None, hit),
                MoveSizeStart {
                    kind: Unknown,
                    cursor: None,
                    probe: CursorUnavailable,
                },
                "hit={hit:?}",
            );
        }
        // Cursor without hit-test answer: Unknown with the failing stage.
        assert_eq!(
            classify_move_size_probe(Some((100, 200)), None),
            MoveSizeStart {
                kind: Unknown,
                cursor: Some((100, 200)),
                probe: HitTestUnavailable,
            },
        );
        // Successful hits keep the exact `classify_hit_test` verdict with
        // the raw value and cursor retained.
        assert_eq!(
            classify_move_size_probe(Some((100, 200)), Some(2)),
            MoveSizeStart {
                kind: Move,
                cursor: Some((100, 200)),
                probe: Hit(2),
            },
        );
        for hit in [10, 17] {
            assert_eq!(
                classify_move_size_probe(Some((0, 0)), Some(hit)),
                MoveSizeStart {
                    kind: Resize,
                    cursor: Some((0, 0)),
                    probe: Hit(hit),
                },
                "hit={hit}",
            );
        }
        for hit in [0, 1, 3, 18] {
            assert_eq!(
                classify_move_size_probe(Some((0, 0)), Some(hit)),
                MoveSizeStart {
                    kind: Unknown,
                    cursor: Some((0, 0)),
                    probe: Hit(hit),
                },
                "hit={hit}",
            );
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
    fn scaled_style_flows_into_core_outer() {
        // Windows seam: the -1 extension sentinel resolves against the
        // logical border width, scales to physical pixels, and the core
        // outer owns the union expansion. Union 100x80 at (10,20), logical
        // gap 2 / width 4 / extension -1 at 96 DPI scales to 2/4/4, and the
        // core outer is (0,10,120,100) at pad 10.
        use tiler_core::visual::{
            group_underlay_effective_extension, group_underlay_outer_rect_px,
        };
        let extension_logical = group_underlay_effective_extension(super::DEFAULT_EXTENSION, 4.0);
        assert_eq!(extension_logical, 4.0);
        let extension_px = crate::active_border::scale_to_physical(extension_logical, 96)
            .expect("extension scales");
        assert_eq!(extension_px, 4);
        assert_eq!(
            group_underlay_outer_rect_px(rect(10, 20, 100, 80), 2, 4, extension_px),
            Some(rect(0, 10, 120, 100))
        );
        // Zero pad is the identity; degenerate unions fail closed.
        assert_eq!(
            group_underlay_outer_rect_px(rect(10, 20, 100, 80), 0, 0, 0),
            Some(rect(10, 20, 100, 80))
        );
        assert_eq!(
            group_underlay_outer_rect_px(rect(10, 20, 0, 80), 2, 4, 4),
            None
        );
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
