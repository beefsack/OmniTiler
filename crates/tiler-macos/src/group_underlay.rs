#![forbid(unsafe_code)]

//! Portable macOS group-underlay policy (pure, no AppKit, no window-server).
//!
//! Shared KDE/Windows-adjacent semantics live in `tiler-core::visual`
//! (`group_underlay_effective_extension`, `group_underlay_outer_rect`);
//! this module is the thin macOS-local layer: the shared default fill
//! `#662a82da` (with alpha), the closed parse, the union helper over member
//! frames, and the carrier outer-rect math in AppKit points. No radius
//! setting exists for the underlay (it fills, never strokes a ring).

use tiler_core::visual::VisualRect;

/// Shared default underlay fill (`#662a82da` in `#aarrggbb`: alpha `0x66`,
/// RGB `0x2a,0x82,0xda`).
pub const DEFAULT_FILL_HEX: &str = "#662a82da";
/// Same default as raw channels in RGBA order (alpha last).
pub const DEFAULT_FILL_RGBA: (u8, u8, u8, u8) = (0x2a, 0x82, 0xda, 0x66);
/// Fixture border gap applied before the underlay extension (matches the
/// active-border default gap).
pub const DEFAULT_GAP: f64 = 0.0;
/// Border width the `-1` extension sentinel follows in the fixture.
pub const DEFAULT_BORDER_WIDTH: f64 = 3.0;
/// Configured extension sentinel: `-1` follows the border width.
pub const DEFAULT_EXTENSION: f64 = -1.0;

/// Parse exactly `#aarrggbb` (case-insensitive, `#` required), the shared
/// `ColorArgb` grammar. The returned channels are RGBA-ordered
/// `(r, g, b, a)`. Six-digit `#rrggbb` and any other length refuse (never
/// an opaque guess).
pub fn parse_fill(value: &str) -> Result<(u8, u8, u8, u8), String> {
    let usage = "refuse: underlay fill needs #aarrggbb";
    let hex = value.strip_prefix('#').ok_or_else(|| usage.to_owned())?;
    if hex.len() != 8 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(usage.to_owned());
    }
    let channel = |range: std::ops::Range<usize>| {
        u8::from_str_radix(&hex[range], 16).map_err(|_| usage.to_owned())
    };
    Ok((
        channel(2..4)?,
        channel(4..6)?,
        channel(6..8)?,
        channel(0..2)?,
    ))
}

/// Render RGBA channels back to shared `#aarrggbb` (alpha first).
#[must_use]
pub fn render_fill((r, g, b, a): (u8, u8, u8, u8)) -> String {
    format!("#{a:02x}{r:02x}{g:02x}{b:02x}")
}

/// Bounding union of member frames. `None` on empty input or any
/// non-finite member (never guessed).
#[must_use]
pub fn union_rect(members: &[VisualRect]) -> Option<VisualRect> {
    if members.is_empty() {
        return None;
    }
    let mut iter = members.iter();
    let first = iter.next()?;
    for v in [first.x, first.y, first.w, first.h] {
        if !v.is_finite() || first.w <= 0.0 || first.h <= 0.0 {
            return None;
        }
    }
    let (mut x0, mut y0, mut x1, mut y1) = (first.x, first.y, first.x + first.w, first.y + first.h);
    for m in iter {
        for v in [m.x, m.y, m.w, m.h] {
            if !v.is_finite() {
                return None;
            }
        }
        if m.w <= 0.0 || m.h <= 0.0 {
            return None;
        }
        x0 = x0.min(m.x);
        y0 = y0.min(m.y);
        x1 = x1.max(m.x + m.w);
        y1 = y1.max(m.y + m.h);
    }
    if !(x1 > x0 && y1 > y0) || ![x0, y0, x1, y1].iter().all(|v| v.is_finite()) {
        return None;
    }
    Some(VisualRect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    })
}

/// Carrier outer rect for a member union: resolve the extension sentinel
/// against `border_width`, then expand by `gap + border_width + extension`
/// via the shared core helper. `None` fails closed (never clamped).
#[must_use]
pub fn underlay_outer_rect(
    union: VisualRect,
    gap: f64,
    border_width: f64,
    extension: f64,
) -> Option<VisualRect> {
    if !gap.is_finite() || !border_width.is_finite() || !extension.is_finite() {
        return None;
    }
    for v in [union.x, union.y, union.w, union.h] {
        if !v.is_finite() {
            return None;
        }
    }
    if union.w <= 0.0 || union.h <= 0.0 {
        return None;
    }
    let resolved = tiler_core::visual::group_underlay_effective_extension(extension, border_width);
    let outer = tiler_core::visual::group_underlay_outer_rect(union, gap, border_width, resolved);
    if ![outer.x, outer.y, outer.w, outer.h]
        .iter()
        .all(|v| v.is_finite())
        || outer.w <= 0.0
        || outer.h <= 0.0
    {
        return None;
    }
    const BOUND: f64 = 1_000_000.0;
    if outer.x.abs() > BOUND || outer.y.abs() > BOUND || outer.w > BOUND || outer.h > BOUND {
        return None;
    }
    Some(outer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> VisualRect {
        VisualRect { x, y, w, h }
    }

    #[test]
    fn default_fill_matches_shared_value() {
        // Shared `#aarrggbb`: alpha FIRST. `#662a82da` is a=`0x66`,
        // RGB `0x2a,0x82,0xda`; the tuple stays RGBA-ordered (alpha last).
        assert_eq!(DEFAULT_FILL_HEX, "#662a82da");
        assert_eq!(DEFAULT_FILL_RGBA, (0x2a, 0x82, 0xda, 0x66));
        assert_eq!(render_fill(DEFAULT_FILL_RGBA), "#662a82da");
        assert_eq!(parse_fill("#662a82da"), Ok(DEFAULT_FILL_RGBA));
        assert_eq!(parse_fill("#662A82DA"), Ok(DEFAULT_FILL_RGBA));
        assert_eq!(parse_fill("#ff2a82da"), Ok((0x2a, 0x82, 0xda, 0xff)));
        assert!(parse_fill("662a82da").is_err());
        assert!(parse_fill("#2a82da").is_err());
        assert!(parse_fill("#fff").is_err());
        assert!(parse_fill("#662a82d").is_err());
        assert!(parse_fill("#662a82dag").is_err());
    }

    #[test]
    fn union_covers_members() {
        let union = union_rect(&[
            rect(10.0, 20.0, 100.0, 80.0),
            rect(150.0, 40.0, 60.0, 200.0),
        ])
        .expect("union");
        assert_eq!(union, rect(10.0, 20.0, 200.0, 220.0));
        assert!(union_rect(&[]).is_none());
        assert!(union_rect(&[rect(0.0, 0.0, 0.0, 10.0)]).is_none());
        assert!(union_rect(&[rect(f64::NAN, 0.0, 10.0, 10.0)]).is_none());
    }

    #[test]
    fn outer_expands_gap_border_extension() {
        // Union 100x80 at (10,20), gap 0, border 3, extension -1 -> 3:
        // pad 6 -> (4,14,112,92).
        let union = rect(10.0, 20.0, 100.0, 80.0);
        assert_eq!(
            underlay_outer_rect(union, 0.0, 3.0, -1.0),
            Some(rect(4.0, 14.0, 112.0, 92.0))
        );
        assert!(underlay_outer_rect(union, f64::NAN, 3.0, -1.0).is_none());
        assert!(underlay_outer_rect(rect(0.0, 0.0, 0.0, 10.0), 0.0, 3.0, -1.0).is_none());
    }
}
