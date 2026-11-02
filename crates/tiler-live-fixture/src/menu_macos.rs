#![allow(unsafe_code)]
//! macOS-only standard Window menu for the live fixture.
//!
//! Sole `unsafe` owner in this crate on macOS (mirrors `pipe_win.rs` on
//! Windows): every AppKit call lives here; `win.rs` uses this module through
//! the safe [`ensure_window_menu`] entry, which reports installed/refused as
//! a bool and never hands out raw pointers or validity obligations.
//!
//! Source-inspected need (winit 0.30.13,
//! `src/platform_impl/macos/menu.rs`): the default `menu::initialize` builds
//! the app menu only (About/Services/Hide/Quit) and calls `setMainMenu` - it
//! never calls `setWindowsMenu`. Without a `windowsMenu`, AppKit shows no
//! Window menu and synthesizes no Zoom/Fill/tile slots, so the
//! `fixture-menu-press` inventory has nothing to join. This module appends
//! one Window menu item holding a single `Zoom` slot (`performZoom:` on the
//! responder chain, i.e. the key window) to the existing main menu and
//! registers it with `setWindowsMenu`; AppKit is then expected to populate
//! the Fill, tile, and Return to Previous Size slots around it at display
//! time (not runtime-proven here; the live `fixture-menu-press` inventory
//! readback confirms what AppKit actually installed).
//!
//! Bounds: main thread only (winit owns the loop thread; the single call
//! site is window creation on that thread). Idempotent: a present
//! `windowsMenu` is left untouched and reports installed. Fail-closed: off
//! the main thread, or with no main menu yet, nothing is created and the
//! caller gets `false`. The existing main menu is preserved, never replaced;
//! no delegate, no custom frame, no other menu surface is touched.

/// Install the minimal standard Window menu if absent. Returns `true` when a
/// `windowsMenu` is present afterwards (already installed or just
/// installed), `false` when refused (off-main-thread call, or no main menu
/// to extend yet). Safe to call on every window creation: the present-menu
/// fast path is one AppKit read.
#[cfg(target_os = "macos")]
pub fn ensure_window_menu() -> bool {
    use objc2::MainThreadMarker;
    use objc2_app_kit::{NSApplication, NSMenu, NSMenuItem};
    use objc2_foundation::NSString;

    let Some(mtm) = MainThreadMarker::new() else {
        return false;
    };
    let app = NSApplication::sharedApplication(mtm);
    if app.windowsMenu().is_some() {
        return true;
    }
    let Some(main_menu) = app.mainMenu() else {
        return false;
    };
    // Single standard slot; AppKit is expected to fill Fill/tile/Return
    // slots around the registered windowsMenu at display time (live
    // inventory readback confirms). Nil target routes
    // `performZoom:` through the responder chain to the key window.
    // SAFETY: `initWithTitle` family on live objects with a valid marker;
    // the retained menus/items outlive this call via the menubar.
    let (window_menu, holder) = unsafe {
        use objc2::sel;
        let menu: objc2::rc::Retained<NSMenu> =
            NSMenu::initWithTitle(mtm.alloc::<NSMenu>(), &NSString::from_str("Window"));
        let zoom: objc2::rc::Retained<NSMenuItem> = NSMenuItem::initWithTitle_action_keyEquivalent(
            mtm.alloc::<NSMenuItem>(),
            &NSString::from_str("Zoom"),
            Some(sel!(performZoom:)),
            &NSString::from_str(""),
        );
        menu.addItem(&zoom);
        let holder = NSMenuItem::new(mtm);
        holder.setSubmenu(Some(&menu));
        (menu, holder)
    };
    main_menu.addItem(&holder);
    app.setWindowsMenu(Some(&window_menu));
    app.windowsMenu().is_some()
}

#[cfg(test)]
mod tests {
    #[test]
    fn refuses_off_the_main_thread() {
        // Unit tests run off the main thread: the installer must fail
        // closed (no menu, `false`) rather than touch AppKit.
        assert!(
            main_thread_marker_is_absent_here(),
            "test precondition: unit tests run off the main thread"
        );
        assert!(!super::ensure_window_menu());
    }

    fn main_thread_marker_is_absent_here() -> bool {
        #[cfg(target_os = "macos")]
        {
            objc2::MainThreadMarker::new().is_none()
        }
        #[cfg(not(target_os = "macos"))]
        {
            true
        }
    }
}
