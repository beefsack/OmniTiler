use super::*;

#[test]
fn accent_dirty_flag_is_sticky_until_consumed() {
    // WndProc broadcast path without a live window: marks coalesce into
    // one refresh, and consumption resets exactly once.
    assert!(!take_accent_dirty());
    mark_accent_dirty();
    mark_accent_dirty();
    assert!(take_accent_dirty());
    assert!(!take_accent_dirty());
}

#[test]
fn z_verdict_repairs_underlay_above_but_preserves_border() {
    use ZRelation::{Above, AdjacentBelow, Below};
    // Adjacent-below is the only acceptable cached placement for both.
    assert!(!z_needs_reassert(Some(AdjacentBelow), false));
    assert!(!z_needs_reassert(Some(AdjacentBelow), true));
    // Far-below and unreadable probes reassert for both carriers.
    assert!(z_needs_reassert(Some(Below), false));
    assert!(z_needs_reassert(Some(Below), true));
    assert!(z_needs_reassert(None, false));
    assert!(z_needs_reassert(None, true));
    // Above-anchor: border preserves its foreground-below assumption,
    // the underlay (lowest-member anchor) must never accept it.
    assert!(!z_needs_reassert(Some(Above), false));
    assert!(z_needs_reassert(Some(Above), true));
}

#[test]
fn preview_default_matches_kde_argb402a82da() {
    // KDE parity: `#2A82DA` at alpha 64, i.e. ARGB `#402A82DA`.
    assert_eq!(PREVIEW_DEFAULT_ARGB, (0x40, 0x2A, 0x82, 0xDA));
    assert_eq!(preview_default_fill(), ((0x2A, 0x82, 0xDA), 0x40));
    assert_eq!(
        crate::group_underlay::render_color_argb(((0x2A, 0x82, 0xDA), 0x40)),
        "#402a82da"
    );
    // Distinct carrier class: inspect and residue audits count each
    // surface exactly; the preview never shares the border/underlay
    // below-target plane.
    assert_eq!(PREVIEW_CLASS, "OmniTilerDropPreview");
    assert_ne!(PREVIEW_CLASS, OVERLAY_CLASS);
    assert_ne!(PREVIEW_CLASS, UNDERLAY_CLASS);
}

#[test]
fn preview_fill_raster_is_premultiplied_kde_blue() {
    // Straight-to-premultiplied with rounding: (c * 0x40 + 127) / 255.
    // 0x2A*0x40/255 is 0x0B, 0x82*0x40/255 is 0x21, 0xDA*0x40/255 is
    // 0x37, over BGRA order with the alpha byte itself.
    let ((r, g, b), alpha) = preview_default_fill();
    let mut pixels = vec![0u8; 2 * 2 * 4];
    let checksum = crate::group_underlay::paint_fill_argb(&mut pixels, 2, 2, (r, g, b), alpha);
    assert_eq!(
        &pixels[0..4],
        &[0x37, 0x21, 0x0B, 0x40],
        "BGRA premultiplied"
    );
    assert_eq!(pixels.len(), 16);
    let mut other = vec![0u8; 2 * 2 * 4];
    assert_eq!(
        crate::group_underlay::paint_fill_argb(&mut other, 2, 2, (r, g, b), alpha),
        checksum
    );
}

#[test]
fn preview_degenerate_bounds_gate() {
    // Non-positive or absurd surfaces never present: the caller hides.
    let rect = |x: i32, y: i32, w: i32, h: i32| Rect { x, y, w, h };
    assert!(preview_bounds_valid(rect(0, 0, 100, 80)));
    assert!(!preview_bounds_valid(rect(0, 0, 0, 80)));
    assert!(!preview_bounds_valid(rect(0, 0, 100, 0)));
    assert!(!preview_bounds_valid(rect(0, 0, -5, 80)));
    assert!(!preview_bounds_valid(rect(0, 0, 16385, 80)));
    assert!(!preview_bounds_valid(rect(0, 0, 100, 16385)));
    // A fresh overlay hides closed on a degenerate show: no surface,
    // no panic, exactly one counted failure.
    let mut overlay = PreviewOverlay::default();
    assert_eq!(
        overlay.show_fill_above(rect(0, 0, 0, 80), PREVIEW_DEFAULT_ARGB, "w1", 7),
        OverlayOutcome::Hidden
    );
    assert!(!overlay.is_visible());
    assert!(overlay.hwnd().is_none());
}

/// Verbatim pre-optimization exhaustive painter: evaluates `ring_covers`
/// and mixes four bytes per pixel. Test oracle only (no Win32 calls);
/// `paint_ring` must stay byte- and checksum-identical to this.
fn paint_ring_oracle(
    pixels: &mut [u8],
    w: i32,
    h: i32,
    width_px: i32,
    radius_px: i32,
    color: (u8, u8, u8),
) -> u64 {
    let (r, g, b) = color;
    let mut hash: u64 = 0xcbf29ce484222325;
    let mut mix = |byte: u8| {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    };
    for y in 0..h {
        for x in 0..w {
            let idx = (y as usize * w as usize + x as usize) * 4;
            let (bb, gg, rr, aa) = if ring_covers(x, y, w, h, width_px, radius_px) {
                (b, g, r, 255)
            } else {
                (0, 0, 0, 0)
            };
            pixels[idx] = bb;
            pixels[idx + 1] = gg;
            pixels[idx + 2] = rr;
            pixels[idx + 3] = aa;
            mix(bb);
            mix(gg);
            mix(rr);
            mix(aa);
        }
    }
    hash
}

fn check_paint_ring_matches_oracle(
    w: i32,
    h: i32,
    width_px: i32,
    radius_px: i32,
    color: (u8, u8, u8),
) {
    // Dirty nonzero start: a missed clear shows up as a byte mismatch.
    let total = w.max(0) as usize * h.max(0) as usize * 4;
    let mut got = vec![0xA5u8; total + 16];
    let mut want = vec![0xA5u8; total + 16];
    let got_sum = paint_ring(&mut got, w, h, width_px, radius_px, color);
    let want_sum = paint_ring_oracle(&mut want, w, h, width_px, radius_px, color);
    assert_eq!(
        got_sum, want_sum,
        "checksum {w}x{h} bw{width_px} r{radius_px}"
    );
    assert_eq!(got, want, "bytes {w}x{h} bw{width_px} r{radius_px}");
}

#[test]
fn paint_ring_matches_exhaustive_oracle_small_matrix() {
    // Broad small matrix: degenerate dims, border width 0 / negative /
    // thick (no inner hole), radius 0 / negative / oversized (clamped),
    // narrow surfaces, and color variations.
    let dims = [-1, 0, 1, 2, 3, 4, 7, 8, 11, 16];
    let borders = [-1, 0, 1, 2, 3, 5, 8, 20];
    let radii = [-2, 0, 1, 6, 40];
    let colors = [(0u8, 0u8, 0u8), (42, 130, 218), (255, 255, 255)];
    for w in dims {
        for h in dims {
            for width_px in borders {
                for radius_px in radii {
                    for color in colors {
                        check_paint_ring_matches_oracle(w, h, width_px, radius_px, color);
                    }
                }
            }
        }
    }
}

#[test]
fn paint_ring_matches_oracle_at_typical_sizes() {
    // Sensible larger surfaces incl. defaults (width 3, radius 6).
    for (w, h, width_px, radius_px) in [
        (320, 200, 3, 6),
        (320, 200, 1, 0),
        (800, 600, 3, 6),
        (800, 600, 8, 12),
        (1920, 1080, 3, 6),
        (1920, 1080, 1, 0),
        (320, 200, 200, 6),
    ] {
        check_paint_ring_matches_oracle(w, h, width_px, radius_px, (42, 130, 218));
    }
}

/// Offline microbenchmark: old exhaustive vs new banded painter over
/// identical dirty buffers. Ignored by default; run explicitly in dev
/// and release with `-- --ignored --nocapture`. No Win32 calls, no
/// asserted timing thresholds; prints wall time only.
#[test]
#[ignore]
fn paint_ring_bench_old_vs_new() {
    use std::hint::black_box;
    use std::time::Instant;
    const REPS: u32 = 3;
    let color = (42u8, 130u8, 218u8);
    for (label, w, h, width_px, radius_px) in
        [("medium", 800, 600, 3, 6), ("large", 1920, 1080, 3, 6)]
    {
        let total = w as usize * h as usize * 4;
        let dirt = vec![0xA5u8; total];
        let mut new_elapsed = std::time::Duration::ZERO;
        let mut old_elapsed = std::time::Duration::ZERO;
        for _ in 0..REPS {
            let mut buf = dirt.clone();
            let start = Instant::now();
            let new_sum = paint_ring(
                black_box(buf.as_mut_slice()),
                w,
                h,
                width_px,
                radius_px,
                color,
            );
            new_elapsed += start.elapsed();
            black_box(new_sum);
            let mut expect = dirt.clone();
            let start = Instant::now();
            let old_sum = paint_ring_oracle(
                black_box(expect.as_mut_slice()),
                w,
                h,
                width_px,
                radius_px,
                color,
            );
            old_elapsed += start.elapsed();
            black_box(old_sum);
            assert_eq!(new_sum, old_sum, "bench checksum {label}");
            assert_eq!(buf, expect, "bench bytes {label}");
        }
        println!(
            "paint_ring {label} {w}x{h} bw{width_px} r{radius_px}: new {:?}/rep old {:?}/rep ({REPS} reps)",
            new_elapsed / REPS,
            old_elapsed / REPS,
        );
    }
}
