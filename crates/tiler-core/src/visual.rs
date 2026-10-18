//! Portable border, preview and group-underlay policy.

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VisualRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VisualBorderState {
    pub visible: bool,
    pub inner: VisualRect,
}

/// Alpha is the host's quantized value, not a floating-point alpha.
#[must_use]
pub fn active_border_use_theme(use_theme: bool, theme_valid: bool, theme_alpha: i32) -> bool {
    use_theme && theme_valid && theme_alpha > 0
}

#[must_use]
pub fn active_border_is_maximized(horizontal: bool, vertical: bool) -> bool {
    horizontal || vertical
}

#[must_use]
pub fn active_border_state(
    has_window: bool,
    frame: VisualRect,
    deleted: bool,
    minimized: bool,
    fullscreen: bool,
    maximized: bool,
    excluded: bool,
) -> VisualBorderState {
    if has_window && !deleted && !minimized && !fullscreen && !maximized && !excluded {
        VisualBorderState {
            visible: true,
            inner: frame,
        }
    } else {
        VisualBorderState {
            visible: false,
            inner: VisualRect::default(),
        }
    }
}

/// Expand the frame by `gap` on every side.
#[must_use]
pub fn active_border_inner_rect(frame: VisualRect, gap: f64) -> VisualRect {
    VisualRect {
        x: frame.x - gap,
        y: frame.y - gap,
        w: frame.w + 2.0 * gap,
        h: frame.h + 2.0 * gap,
    }
}

#[must_use]
pub fn drag_preview_rect_valid(x: i32, y: i32, w: i32, h: i32) -> bool {
    const BOUND: i32 = 16384;
    if w < 1 || h < 1 || w > BOUND || h > BOUND {
        return false;
    }
    (-BOUND..=BOUND).contains(&x) && (-BOUND..=BOUND).contains(&y)
}

/// Underlay extension default: any negative sentinel follows the current
/// border width.
#[must_use]
pub fn group_underlay_effective_extension(extension: f64, border_width: f64) -> f64 {
    if extension < 0.0 {
        border_width
    } else {
        extension
    }
}

/// The extension is already resolved.
#[must_use]
pub fn group_underlay_outer_rect(
    union_rect: VisualRect,
    gap: f64,
    border_width: f64,
    resolved_extension: f64,
) -> VisualRect {
    let pad = gap + border_width + resolved_extension;
    VisualRect {
        x: union_rect.x - pad,
        y: union_rect.y - pad,
        w: union_rect.w + 2.0 * pad,
        h: union_rect.h + 2.0 * pad,
    }
}

#[must_use]
pub fn group_focus_eligible(
    has_window: bool,
    deleted: bool,
    minimized: bool,
    fullscreen: bool,
    hidden: bool,
    maximized: bool,
) -> bool {
    has_window && !deleted && !minimized && !fullscreen && !hidden && !maximized
}

#[must_use]
pub fn group_visible(has_group: bool, modifier_held: bool, focus_eligible: bool) -> bool {
    has_group && modifier_held && focus_eligible
}

/// Movement-only underlay chord: both Win and Shift held. Extra modifiers are
/// allowed (callers fold only these two), and either press order works because
/// this is level-observed, never edge-sequenced.
#[must_use]
pub fn group_underlay_chord_held(win_held: bool, shift_held: bool) -> bool {
    win_held && shift_held
}

/// Movement-only underlay trigger: the observed Win+Shift chord OR a matching
/// focused-window interactive move. Interactive resize alone never triggers;
/// the chord stays independent and can still show the underlay during resize.
/// Move-start evidence never depends on modifier observation: the move arm is
/// a separate input.
#[must_use]
pub fn group_underlay_trigger(chord_held: bool, matching_move_active: bool) -> bool {
    chord_held || matching_move_active
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rect(x: f64, y: f64, w: f64, h: f64) -> VisualRect {
        VisualRect { x, y, w, h }
    }

    #[test]
    fn visibility_suppression_matrix() {
        // (has, deleted, minimized, fullscreen, maximized, popup) -> visible.
        let cases = [
            ((true, false, false, false, false, false), true),
            ((false, false, false, false, false, false), false),
            ((true, true, false, false, false, false), false),
            ((true, false, true, false, false, false), false),
            ((true, false, false, true, false, false), false),
            ((true, false, false, false, true, false), false),
            ((true, false, false, false, false, true), false),
        ];
        for ((has, deleted, min, fs, max, popup), want) in cases {
            let frame = rect(10.0, 20.0, 320.0, 200.0);
            let state = active_border_state(has, frame, deleted, min, fs, max, popup);
            assert_eq!(state.visible, want);
            assert_eq!(
                state.inner,
                if want { frame } else { VisualRect::default() }
            );
        }
    }

    #[test]
    fn maximize_axis_or() {
        assert!(!active_border_is_maximized(false, false));
        assert!(active_border_is_maximized(true, false));
        assert!(active_border_is_maximized(false, true));
        assert!(active_border_is_maximized(true, true));
    }

    #[test]
    fn theme_gate_needs_request_valid_and_positive_alpha() {
        // (use_theme, valid, alpha) -> use theme.
        let cases = [
            ((true, true, 1), true),
            ((true, true, 255), true),
            ((true, true, 0), false),
            ((true, false, 255), false),
            ((false, true, 255), false),
            ((false, false, 0), false),
        ];
        for ((use_theme, valid, alpha), want) in cases {
            assert_eq!(active_border_use_theme(use_theme, valid, alpha), want);
        }
    }

    #[test]
    fn inner_rect_ordinary_and_fractional_gap() {
        let frame = rect(10.0, 20.0, 320.0, 200.0);
        assert_eq!(active_border_inner_rect(frame, 0.0), frame);
        assert_eq!(
            active_border_inner_rect(frame, 5.0),
            rect(5.0, 15.0, 330.0, 210.0)
        );
        assert_eq!(
            active_border_inner_rect(frame, 2.5),
            rect(7.5, 17.5, 325.0, 205.0)
        );
    }

    #[test]
    fn preview_inclusive_boundaries() {
        let valid = [
            (0, 0, 1, 1),
            (0, 0, 1200, 800),
            (-16384, -16384, 16384, 16384),
            (16384, 16384, 1, 1),
            (-16384, 16384, 1, 1),
        ];
        for (x, y, w, h) in valid {
            assert!(
                drag_preview_rect_valid(x, y, w, h),
                "want valid {x} {y} {w} {h}"
            );
        }
        let invalid = [
            (0, 0, 0, 1),
            (0, 0, 1, 0),
            (0, 0, 16385, 1),
            (0, 0, 1, 16385),
            (16385, 0, 10, 10),
            (-16385, 0, 10, 10),
            (0, 16385, 10, 10),
            (0, -16385, 10, 10),
        ];
        for (x, y, w, h) in invalid {
            assert!(
                !drag_preview_rect_valid(x, y, w, h),
                "want invalid {x} {y} {w} {h}"
            );
        }
    }

    #[test]
    fn underlay_extension_sentinel() {
        assert_eq!(group_underlay_effective_extension(-1.0, 3.0), 3.0);
        assert_eq!(group_underlay_effective_extension(0.0, 5.0), 0.0);
        assert_eq!(group_underlay_effective_extension(7.5, 5.0), 7.5);
    }

    #[test]
    fn underlay_outer_fixtures() {
        let union = rect(10.0, 20.0, 100.0, 80.0);
        for (extension, expected) in [
            (-1.0, rect(2.0, 12.0, 116.0, 96.0)),
            (0.0, rect(5.0, 15.0, 110.0, 90.0)),
        ] {
            let extension = group_underlay_effective_extension(extension, 3.0);
            assert_eq!(
                group_underlay_outer_rect(union, 2.0, 3.0, extension),
                expected
            );
        }
    }

    #[test]
    fn group_focus_eligibility() {
        // (has, deleted, minimized, fullscreen, hidden, maximized) -> eligible.
        let cases = [
            ((true, false, false, false, false, false), true),
            ((false, false, false, false, false, false), false),
            ((true, true, false, false, false, false), false),
            ((true, false, true, false, false, false), false),
            ((true, false, false, true, false, false), false),
            ((true, false, false, false, true, false), false),
            ((true, false, false, false, false, true), false),
        ];
        for ((has, deleted, min, fs, hidden, max), want) in cases {
            assert_eq!(
                group_focus_eligible(has, deleted, min, fs, hidden, max),
                want
            );
        }
    }

    #[test]
    fn group_visibility() {
        assert!(group_visible(true, true, true));
        assert!(!group_visible(false, true, true));
        assert!(!group_visible(true, false, true));
        assert!(!group_visible(true, true, false));
    }

    #[test]
    fn underlay_chord_needs_both_win_and_shift() {
        // (win, shift) -> chord. Extras/order live outside this predicate.
        for (win, shift, want) in [
            (false, false, false),
            (true, false, false),
            (false, true, false),
            (true, true, true),
        ] {
            assert_eq!(group_underlay_chord_held(win, shift), want);
        }
    }

    #[test]
    fn underlay_trigger_is_chord_or_matching_move() {
        // (chord, matching_move) -> trigger. Resize alone feeds
        // matching_move=false, so it never triggers; a held chord still does.
        for (chord, moving, want) in [
            (false, false, false),
            (true, false, true),
            (false, true, true),
            (true, true, true),
        ] {
            assert_eq!(group_underlay_trigger(chord, moving), want);
        }
    }
}
