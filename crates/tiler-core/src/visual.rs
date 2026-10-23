//! Portable border, preview, group-underlay, and desired-presentation policy.

use crate::geometry::Rect;

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

/// Closed hide-reason vocabulary for the border presentation (adapter units).
/// Core owns eligibility reasons; the adapter keeps native identity, scope,
/// proof, and freshness vetoes (allowlist, scope-excluded, dpi-scale,
/// zero-width, present-failed) plus its own drawing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorderHideReason {
    Disabled,
    NoTarget,
    Minimized,
    Maximized,
    Fullscreen,
    Cloaked,
    HiddenWorkspace,
    Shell,
}

impl BorderHideReason {
    /// Closed log vocabulary for the hide reason.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::NoTarget => "no-target",
            Self::Minimized => "minimized",
            Self::Maximized => "maximized",
            Self::Fullscreen => "fullscreen",
            Self::Cloaked => "cloaked",
            Self::HiddenWorkspace => "hidden-workspace",
            Self::Shell => "shell",
        }
    }
}

/// Normalized border eligibility facts: the fresh foreground visibility and
/// exclusion state. No tiling/managed membership participates: ordinary
/// floats and scoped unmanaged foregrounds stay eligible.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderEligibilityFacts {
    pub enabled: bool,
    pub has_window: bool,
    pub minimized: bool,
    pub fullscreen: bool,
    pub maximized: bool,
    pub cloaked: bool,
    pub hidden_workspace: bool,
    pub shell: bool,
}

/// Normalized border facts in adapter units (Windows PMv2 physical): the
/// eligibility facts plus the already host-scaled physical style numbers
/// (scale before core geometry).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderFacts {
    pub eligibility: BorderEligibilityFacts,
    pub frame: Rect,
    pub gap_px: i32,
    pub width_px: i32,
}

/// Desired border presentation: the eligibility verdict plus the complete
/// inner/outer bounds in adapter units. `reason` is `Some` exactly when the
/// eligibility gate hides; a `None` reason with `visible == false` names a
/// failed bounds computation (the adapter logs its own geometry veto).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BorderPresentation {
    pub visible: bool,
    pub reason: Option<BorderHideReason>,
    pub inner: Option<Rect>,
    pub outer: Option<Rect>,
}

/// Border eligibility for one normalized foreground target, governed by
/// [`active_border_state`]. The core verdict is authoritative: reasons derive
/// after the verdict, never pre-returned ahead of it. `maximized` is the
/// single zoom axis (Windows maps its zoom axis here).
pub fn border_eligibility(facts: &BorderEligibilityFacts) -> Result<(), BorderHideReason> {
    if !facts.enabled {
        return Err(BorderHideReason::Disabled);
    }
    if !facts.has_window {
        return Err(BorderHideReason::NoTarget);
    }
    let maximized_core = active_border_is_maximized(facts.maximized, false);
    let state = active_border_state(
        facts.has_window,
        VisualRect {
            x: 0.0,
            y: 0.0,
            w: 1.0,
            h: 1.0,
        },
        /* deleted = */ false,
        facts.minimized,
        facts.fullscreen,
        maximized_core,
        facts.cloaked || facts.hidden_workspace || facts.shell,
    );
    if state.visible {
        return Ok(());
    }
    if facts.minimized {
        return Err(BorderHideReason::Minimized);
    }
    if facts.fullscreen {
        return Err(BorderHideReason::Fullscreen);
    }
    if facts.maximized {
        return Err(BorderHideReason::Maximized);
    }
    if facts.cloaked {
        return Err(BorderHideReason::Cloaked);
    }
    if facts.hidden_workspace {
        return Err(BorderHideReason::HiddenWorkspace);
    }
    if facts.shell {
        return Err(BorderHideReason::Shell);
    }
    Err(BorderHideReason::NoTarget)
}

/// Complete border bounds in adapter units: the visible frame expanded by the
/// physical gap (inner), then by the physical border width on every side
/// (outer, the KDE `BorderOutline` ring outside the inner rect). `None` on
/// negative style numbers, degenerate frames, overflow, or degenerate output.
#[must_use]
pub fn border_geometry(frame: Rect, gap_px: i32, width_px: i32) -> Option<(Rect, Rect)> {
    if gap_px < 0 || width_px < 0 || frame.w <= 0 || frame.h <= 0 {
        return None;
    }
    let gap = i64::from(gap_px);
    let inner = Rect {
        x: i64::from(frame.x).checked_sub(gap)?.try_into().ok()?,
        y: i64::from(frame.y).checked_sub(gap)?.try_into().ok()?,
        w: i64::from(frame.w).checked_add(2 * gap)?.try_into().ok()?,
        h: i64::from(frame.h).checked_add(2 * gap)?.try_into().ok()?,
    };
    if inner.w <= 0 || inner.h <= 0 {
        return None;
    }
    let width = i64::from(width_px);
    let outer = Rect {
        x: i64::from(inner.x).checked_sub(width)?.try_into().ok()?,
        y: i64::from(inner.y).checked_sub(width)?.try_into().ok()?,
        w: i64::from(inner.w).checked_add(2 * width)?.try_into().ok()?,
        h: i64::from(inner.h).checked_add(2 * width)?.try_into().ok()?,
    };
    if outer.w <= 0 || outer.h <= 0 {
        return None;
    }
    Some((inner, outer))
}

/// Desired border presentation from normalized facts: eligibility first, then
/// the complete bounds. Eligible but uncomputable bounds hide with no reason
/// (the adapter logs its own geometry veto); the style/scale vetoes
/// (dpi-scale, zero-width) stay adapter-side ahead of this call.
#[must_use]
pub fn border_presentation(facts: &BorderFacts) -> BorderPresentation {
    match border_eligibility(&facts.eligibility) {
        Err(reason) => BorderPresentation {
            visible: false,
            reason: Some(reason),
            inner: None,
            outer: None,
        },
        Ok(()) => match border_geometry(facts.frame, facts.gap_px, facts.width_px) {
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
        },
    }
}

/// Closed hide-reason vocabulary for the group-underlay presentation.
/// Core owns eligibility reasons; the adapter keeps native identity, scope,
/// proof, liveness, and freshness vetoes (no-target, allowlist-changed,
/// identity-changed, scope-excluded, observe-failed, dpi-scale,
/// anchor-missing, present-failed) plus z-anchor/topmost and drawing.
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
    /// Closed log vocabulary for the hide reason.
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

/// Underlay eligibility for one focused subject. `has_group` is the resolved
/// immediate-parent verdict (false covers root-leaf focus); `floating` is a
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

/// Complete group-underlay outer bounds in adapter units: the projected group
/// union expanded by the physical gap, border width, and resolved extension
/// (the -1 sentinel resolves against the logical border width and scales
/// before this call, so the outset here is literal) on every side. `None` on
/// negative style numbers, degenerate unions, overflow, or degenerate output.
#[must_use]
pub fn group_underlay_outer_rect_px(
    union_rect: Rect,
    gap_px: i32,
    width_px: i32,
    extension_px: i32,
) -> Option<Rect> {
    if gap_px < 0 || width_px < 0 || extension_px < 0 {
        return None;
    }
    if union_rect.w <= 0 || union_rect.h <= 0 {
        return None;
    }
    let pad = i64::from(gap_px)
        .checked_add(i64::from(width_px))?
        .checked_add(i64::from(extension_px))?;
    let outer = Rect {
        x: i64::from(union_rect.x).checked_sub(pad)?.try_into().ok()?,
        y: i64::from(union_rect.y).checked_sub(pad)?.try_into().ok()?,
        w: i64::from(union_rect.w)
            .checked_add(2 * pad)?
            .try_into()
            .ok()?,
        h: i64::from(union_rect.h)
            .checked_add(2 * pad)?
            .try_into()
            .ok()?,
    };
    if outer.w <= 0 || outer.h <= 0 {
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

    #[allow(clippy::too_many_arguments)]
    fn border_facts(
        enabled: bool,
        has_window: bool,
        minimized: bool,
        fullscreen: bool,
        maximized: bool,
        cloaked: bool,
        hidden_workspace: bool,
        shell: bool,
    ) -> BorderEligibilityFacts {
        BorderEligibilityFacts {
            enabled,
            has_window,
            minimized,
            fullscreen,
            maximized,
            cloaked,
            hidden_workspace,
            shell,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn styled_facts(
        enabled: bool,
        has_window: bool,
        minimized: bool,
        fullscreen: bool,
        maximized: bool,
        cloaked: bool,
        hidden_workspace: bool,
        shell: bool,
    ) -> BorderFacts {
        BorderFacts {
            eligibility: border_facts(
                enabled,
                has_window,
                minimized,
                fullscreen,
                maximized,
                cloaked,
                hidden_workspace,
                shell,
            ),
            frame: Rect {
                x: 10,
                y: 20,
                w: 320,
                h: 200,
            },
            gap_px: 2,
            width_px: 4,
        }
    }

    #[test]
    fn border_eligibility_suppression_matrix() {
        // Ordinary floats and scoped unmanaged foregrounds carry no tiled
        // token, so eligibility never consults one: any observed foreground
        // with a live frame is eligible.
        assert!(
            border_eligibility(&border_facts(
                true, true, false, false, false, false, false, false
            ))
            .is_ok()
        );
        assert_eq!(
            border_eligibility(&border_facts(
                false, true, false, false, false, false, false, false
            )),
            Err(BorderHideReason::Disabled)
        );
        assert_eq!(
            border_eligibility(&border_facts(
                true, false, false, false, false, false, false, false
            )),
            Err(BorderHideReason::NoTarget)
        );
        for (min, fs, max, cloak, hidden, shell, want) in [
            (
                true,
                false,
                false,
                false,
                false,
                false,
                BorderHideReason::Minimized,
            ),
            (
                false,
                true,
                false,
                false,
                false,
                false,
                BorderHideReason::Fullscreen,
            ),
            (
                false,
                false,
                true,
                false,
                false,
                false,
                BorderHideReason::Maximized,
            ),
            (
                false,
                false,
                false,
                true,
                false,
                false,
                BorderHideReason::Cloaked,
            ),
            (
                false,
                false,
                false,
                false,
                true,
                false,
                BorderHideReason::HiddenWorkspace,
            ),
            (
                false,
                false,
                false,
                false,
                false,
                true,
                BorderHideReason::Shell,
            ),
        ] {
            assert_eq!(
                border_eligibility(&border_facts(
                    true, true, min, fs, max, cloak, hidden, shell
                )),
                Err(want)
            );
        }
        // Priority follows the suppression order: minimized beats every
        // other suppressor, and any excluded analogue beats no-target.
        assert_eq!(
            border_eligibility(&border_facts(
                true, true, true, true, true, true, true, true
            )),
            Err(BorderHideReason::Minimized)
        );
        assert_eq!(
            border_eligibility(&border_facts(
                true, true, false, true, true, true, true, true
            )),
            Err(BorderHideReason::Fullscreen)
        );
    }

    #[test]
    fn border_eligibility_agrees_with_core_verdict() {
        // The portable verdict is authoritative: for every flag combination
        // the eligibility matches the core visibility, with reasons derived
        // after the verdict.
        for bits in 0..64u32 {
            let minimized = bits & 1 != 0;
            let fullscreen = bits & 2 != 0;
            let maximized = bits & 4 != 0;
            let cloaked = bits & 8 != 0;
            let hidden = bits & 16 != 0;
            let shell = bits & 32 != 0;
            let maximized_core = active_border_is_maximized(maximized, false);
            let core = active_border_state(
                true,
                VisualRect {
                    x: 10.0,
                    y: 20.0,
                    w: 320.0,
                    h: 200.0,
                },
                false,
                minimized,
                fullscreen,
                maximized_core,
                cloaked || hidden || shell,
            );
            let verdict = border_eligibility(&border_facts(
                true, true, minimized, fullscreen, maximized, cloaked, hidden, shell,
            ));
            assert_eq!(verdict.is_ok(), core.visible, "bits={bits:06b}");
        }
    }

    #[test]
    fn border_geometry_expands_gap_then_width() {
        // Visible 100x80 at (10,20), gap 2, width 4:
        // inner (8,18,104,84), outer (4,14,112,92).
        let frame = Rect {
            x: 10,
            y: 20,
            w: 100,
            h: 80,
        };
        assert_eq!(
            border_geometry(frame, 2, 4),
            Some((
                Rect {
                    x: 8,
                    y: 18,
                    w: 104,
                    h: 84
                },
                Rect {
                    x: 4,
                    y: 14,
                    w: 112,
                    h: 92
                }
            ))
        );
        // Zero gap/width is the identity for inner, width-only for outer.
        assert_eq!(
            border_geometry(frame, 0, 3),
            Some((
                frame,
                Rect {
                    x: 7,
                    y: 17,
                    w: 106,
                    h: 86
                }
            ))
        );
    }

    #[test]
    fn border_geometry_fails_closed() {
        let frame = Rect {
            x: 10,
            y: 20,
            w: 100,
            h: 80,
        };
        assert_eq!(border_geometry(frame, -1, 4), None);
        assert_eq!(border_geometry(frame, 2, -1), None);
        assert_eq!(
            border_geometry(
                Rect {
                    x: 10,
                    y: 20,
                    w: 0,
                    h: 80
                },
                2,
                4
            ),
            None
        );
        // Overflow at the i32 edge fails closed instead of wrapping.
        assert_eq!(
            border_geometry(
                Rect {
                    x: 0,
                    y: 0,
                    w: i32::MAX,
                    h: 100,
                },
                2,
                4
            ),
            None
        );
        assert_eq!(
            border_geometry(
                Rect {
                    x: i32::MIN,
                    y: 0,
                    w: 100,
                    h: 100,
                },
                2,
                4
            ),
            None
        );
    }

    #[test]
    fn border_presentation_combines_verdict_and_bounds() {
        // Eligible with computable bounds: visible with both rects.
        let shown = border_presentation(&styled_facts(
            true, true, false, false, false, false, false, false,
        ));
        assert!(shown.visible);
        assert_eq!(shown.reason, None);
        assert_eq!(
            shown.inner,
            Some(Rect {
                x: 8,
                y: 18,
                w: 324,
                h: 204
            })
        );
        assert_eq!(
            shown.outer,
            Some(Rect {
                x: 4,
                y: 14,
                w: 332,
                h: 212
            })
        );
        // Ineligible: hidden with the eligibility reason, no bounds.
        let hidden = border_presentation(&styled_facts(
            true, true, false, true, false, false, false, false,
        ));
        assert!(!hidden.visible);
        assert_eq!(hidden.reason, Some(BorderHideReason::Fullscreen));
        assert_eq!(hidden.inner, None);
        assert_eq!(hidden.outer, None);
        // Eligible but overflowing: hidden with no reason (the adapter logs
        // its own geometry veto), never a mislabeled eligibility reason.
        let mut facts = styled_facts(true, true, false, false, false, false, false, false);
        facts.frame = Rect {
            x: 0,
            y: 0,
            w: i32::MAX,
            h: 100,
        };
        let overflow = border_presentation(&facts);
        assert!(!overflow.visible);
        assert_eq!(overflow.reason, None);
        assert_eq!(overflow.inner, None);
        assert_eq!(overflow.outer, None);
    }

    #[test]
    fn underlay_eligibility_suppression_matrix() {
        assert!(underlay_eligible(true, true, true, false, false, false).is_ok());
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
    fn underlay_eligibility_agrees_with_core_focus_gate() {
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
    fn underlay_outer_px_expands_union_by_gap_width_extension() {
        // Union 100x80 at (10,20), gap 2, width 4, extension 3: pad 9.
        assert_eq!(
            group_underlay_outer_rect_px(
                Rect {
                    x: 10,
                    y: 20,
                    w: 100,
                    h: 80
                },
                2,
                4,
                3
            ),
            Some(Rect {
                x: 1,
                y: 11,
                w: 118,
                h: 98
            })
        );
        // Zero pad is the identity.
        let union = Rect {
            x: 10,
            y: 20,
            w: 100,
            h: 80,
        };
        assert_eq!(group_underlay_outer_rect_px(union, 0, 0, 0), Some(union));
        // Degenerate, negative, or overflowing inputs fail closed.
        assert_eq!(
            group_underlay_outer_rect_px(
                Rect {
                    x: 10,
                    y: 20,
                    w: 0,
                    h: 80
                },
                2,
                4,
                3
            ),
            None
        );
        assert_eq!(group_underlay_outer_rect_px(union, -1, 4, 3), None);
        assert_eq!(group_underlay_outer_rect_px(union, 2, 4, -1), None);
        assert_eq!(
            group_underlay_outer_rect_px(
                Rect {
                    x: 0,
                    y: 0,
                    w: i32::MAX,
                    h: 100,
                },
                2,
                4,
                3
            ),
            None
        );
    }
}
