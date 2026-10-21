#![deny(unsafe_code)]

pub mod active_border;
#[cfg(windows)]
pub mod active_border_sys;
pub mod cloak;
#[cfg(windows)]
pub mod ffi;
pub mod gaming;
pub mod gaming_settings;
pub mod group_underlay;
pub mod input_handoff;
pub mod input_health;
pub mod input_pump;
pub mod lifecycle;
pub mod model;
pub mod mouse_snap;
#[cfg(windows)]
pub mod native;
pub mod native_guards;
pub mod pager;
pub mod pager_icons;
#[cfg(windows)]
pub mod pager_icons_sys;
#[cfg(windows)]
pub mod pager_sys;
pub mod panic_diag;
pub mod product_hide;
pub mod render_handoff;
pub mod render_pump;
pub mod settings;
#[cfg(windows)]
pub mod settings_ui;
pub mod snapkey;
pub mod storage;
pub mod test_window;
pub mod tiling;
#[cfg(windows)]
pub mod tiling_sys;
pub mod tray;
pub mod tray_pager;
#[cfg(windows)]
pub mod tray_sys;
pub mod winarrow;
