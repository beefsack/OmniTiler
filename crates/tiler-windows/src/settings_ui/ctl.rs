#![forbid(unsafe_code)]
//! Win32 control helpers for the settings window.
//!
//! Layout descriptor, DPI scaling, and input caps stay here (pure policy).
//! Every text/check/enable/creation/font call delegates to
//! [`crate::ffi::settings_controls`]; no Win32 lives here.

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Graphics::Gdi::HFONT;

use super::{
    DynError, ID_BIND_CHORD, ID_BORDER_COLOR, ID_BORDER_GAP, ID_BORDER_RADIUS, ID_BORDER_WIDTH,
    ID_INNER_GAP, ID_OUTER_GAP, ID_UNDERLAY_COLOR, ID_UNDERLAY_EXTENSION, err,
};

pub(crate) use crate::ffi::settings_controls::{checked, enable, get_text, set_checked, set_text};

#[allow(dead_code)]
pub(super) fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain([0]).collect()
}

pub(crate) struct Ctl {
    pub(crate) id: u32,
    pub(crate) class: &'static str,
    pub(crate) text: String,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) w: i32,
    pub(crate) h: i32,
    pub(crate) style: u32,
}

pub(crate) fn px(value: i32, dpi: u32) -> i32 {
    ((i64::from(value) * i64::from(dpi) + 48) / 96) as i32
}

/// Create one child control with the DPI-scaled message font.
#[allow(clippy::too_many_arguments)]
pub(crate) fn create_ctl(
    parent: HWND,
    hinst: windows_sys::Win32::Foundation::HINSTANCE,
    ctl: &Ctl,
    dpi: u32,
    font: HFONT,
    out: &mut Vec<(u32, HWND)>,
) -> Result<HWND, DynError> {
    let hwnd = crate::ffi::settings_controls::create_child(
        parent,
        hinst,
        ctl.class,
        &ctl.text,
        ctl.id,
        ctl.style,
        px(ctl.x, dpi),
        px(ctl.y, dpi),
        px(ctl.w, dpi),
        px(ctl.h, dpi),
        font,
    );
    if hwnd.is_null() {
        return Err(err(format!("error: settings control {} failed", ctl.id)));
    }
    out.push((ctl.id, hwnd));
    Ok(hwnd)
}

/// Bounded native input length per edit control (the `get_text` 512 cap stays
/// as backstop). Group/label ids carry no limit.
pub(crate) fn edit_limit(id: u32) -> Option<usize> {
    match id {
        ID_INNER_GAP | ID_OUTER_GAP => Some(2),
        ID_BORDER_COLOR => Some(7),
        ID_BORDER_WIDTH | ID_BORDER_GAP | ID_BORDER_RADIUS => Some(5),
        ID_UNDERLAY_COLOR => Some(9),
        ID_UNDERLAY_EXTENSION => Some(5),
        ID_BIND_CHORD => Some(24),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn dialog_fits_1080p_work_area_at_100_and_150_percent() {
        for dpi in [96u32, 144u32] {
            assert!(super::px(924, dpi) + 32 <= 1920, "width at {dpi}");
            assert!(super::px(600, dpi) + 80 <= 1040, "height at {dpi}");
        }
        assert!(super::px(532, 144) <= super::px(600, 144));
        assert!(super::px(582, 144) <= super::px(600, 144));
    }
}
