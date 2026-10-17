#![allow(unsafe_code)]

use tiler_core::visual as core;

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VisualPolicyRect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct VisualPolicyBorderState {
    pub visible: u8,
    pub inner: VisualPolicyRect,
}

impl From<VisualPolicyRect> for core::VisualRect {
    fn from(value: VisualPolicyRect) -> Self {
        core::VisualRect {
            x: value.x,
            y: value.y,
            w: value.w,
            h: value.h,
        }
    }
}

impl From<core::VisualRect> for VisualPolicyRect {
    fn from(value: core::VisualRect) -> Self {
        VisualPolicyRect {
            x: value.x,
            y: value.y,
            w: value.w,
            h: value.h,
        }
    }
}

impl From<core::VisualBorderState> for VisualPolicyBorderState {
    fn from(value: core::VisualBorderState) -> Self {
        VisualPolicyBorderState {
            visible: u8::from(value.visible),
            inner: value.inner.into(),
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn visual_border_use_theme(
    use_theme: u8,
    theme_valid: u8,
    quantized_alpha: i32,
) -> u8 {
    match std::panic::catch_unwind(|| {
        u8::from(core::active_border_use_theme(
            use_theme != 0,
            theme_valid != 0,
            quantized_alpha,
        ))
    }) {
        Ok(value) => value,
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn visual_border_is_maximized(horizontal: u8, vertical: u8) -> u8 {
    match std::panic::catch_unwind(|| {
        u8::from(core::active_border_is_maximized(
            horizontal != 0,
            vertical != 0,
        ))
    }) {
        Ok(value) => value,
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn visual_border_state(
    has_window: u8,
    frame: VisualPolicyRect,
    deleted: u8,
    minimized: u8,
    fullscreen: u8,
    maximized: u8,
    excluded: u8,
) -> VisualPolicyBorderState {
    match std::panic::catch_unwind(|| {
        VisualPolicyBorderState::from(core::active_border_state(
            has_window != 0,
            frame.into(),
            deleted != 0,
            minimized != 0,
            fullscreen != 0,
            maximized != 0,
            excluded != 0,
        ))
    }) {
        Ok(state) => state,
        Err(_) => VisualPolicyBorderState::default(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn visual_border_inner_rect(frame: VisualPolicyRect, gap: f64) -> VisualPolicyRect {
    match std::panic::catch_unwind(|| {
        VisualPolicyRect::from(core::active_border_inner_rect(frame.into(), gap))
    }) {
        Ok(rect) => rect,
        Err(_) => VisualPolicyRect::default(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn visual_drag_preview_rect_valid(x: i32, y: i32, w: i32, h: i32) -> u8 {
    match std::panic::catch_unwind(|| u8::from(core::drag_preview_rect_valid(x, y, w, h))) {
        Ok(value) => value,
        Err(_) => 0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn visual_group_underlay_effective_extension(
    extension: f64,
    border_width: f64,
) -> f64 {
    match std::panic::catch_unwind(|| {
        core::group_underlay_effective_extension(extension, border_width)
    }) {
        Ok(value) => value,
        Err(_) => 0.0,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn visual_group_underlay_outer_rect(
    union_rect: VisualPolicyRect,
    gap: f64,
    border_width: f64,
    resolved_extension: f64,
) -> VisualPolicyRect {
    match std::panic::catch_unwind(|| {
        VisualPolicyRect::from(core::group_underlay_outer_rect(
            union_rect.into(),
            gap,
            border_width,
            resolved_extension,
        ))
    }) {
        Ok(rect) => rect,
        Err(_) => VisualPolicyRect::default(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn visual_group_underlay_chord_held(win_held: u8, shift_held: u8) -> u8 {
    match std::panic::catch_unwind(|| {
        u8::from(core::group_underlay_chord_held(
            win_held != 0,
            shift_held != 0,
        ))
    }) {
        Ok(value) => value,
        Err(_) => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pod_layout_matches_ffi_header() {
        assert_eq!(std::mem::size_of::<VisualPolicyRect>(), 32);
        assert_eq!(std::mem::size_of::<VisualPolicyBorderState>(), 40);
    }
}
