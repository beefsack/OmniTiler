#![forbid(unsafe_code)]

//! Native titlebar drag classifier (offline, parked).
//!
//! The production Meta+left pipeline ([`crate::drag_drop`]) observes its own
//! consumed press and moves the window through the shared Engine oracle. A
//! native titlebar source would instead passively observe an unconsumed host
//! left press, join the fresh scoped AX+CG target, classify titlebar versus
//! resize, and let the host move the window natively (no consume, no replay).
//!
//! No simple safe passive discriminator exists in the current AX/CG
//! vocabulary: the classifier below pins the affirmative bar (titlebar band
//! plus owned identity plus move-vs-resize travel), while the live arm stays
//! explicitly parked — arbitrary `AXMoved`/`AXWindowMoved` notifications are
//! never a drag, and a frame-only move would silently replace native
//! semantics. The Meta pipeline proceeds unblocked.
//!
//! Bounded survey (local upstream sources, no new reads): the role
//! vocabulary exists (`AXToolbar`, `AXGrowArea`, `AXCloseButton` /
//! `AXMinimizeButton` / `AXZoomButton` / `AXFullScreenButton` subroles per
//! AXSwift `Constants.swift`; hammerspoon `libaxuielement.m` names the same
//! toolbar roles), and this crate already wraps the read primitives
//! (`ax_element_at_position`, the `AXParent` chain, `ax_string_attribute`
//! for `AXRole`/`AXSubrole`, `ax_hit_test_owner`). What is concretely
//! missing for an affirmative passive classifier:
//!
//! 1. Role taxonomy: which `(role, subrole)` chains across AppKit /
//!    Catalyst / SwiftUI / Electron / Chromium are titlebar-drag chrome
//!    versus pressable controls versus content. No surveyed source
//!    (yabai, Rectangle, rift, glide, Silica, Swindler, Loop) classifies
//!    an interactive drag from AX roles; the traffic-light buttons live
//!    inside the titlebar band and must be excluded per element.
//! 2. Held-button proof during passive observation: the native arm never
//!    consumes, so proving the left button is still held while the origin
//!    travels needs a button-state read this crate does not wrap
//!    (`CGEventSource` button state has no safe wrapper here).
//! 3. Move-vs-system-move indistinguishability: size-invariant origin
//!    travel ([`confirm_native_move`]) also describes programmatic
//!    `AXSetPosition`, Space-switch, and system moves. Without (2), a
//!    frame-only observation would silently replace host-native semantics.
//!
//! Until (1)-(3) land with fixture proof, the arm stays parked and the
//! Meta arm carries production.

/// Why the live native titlebar arm stays parked.
pub const NATIVE_PARKED_REASON: &str = "parked: no affirmative passive titlebar-movement discriminator in the current AX/CG vocabulary; arbitrary AX moved/resized notifications never classify as an interactive drag, and frame-only movement would silently replace host-native semantics";

/// Live native titlebar state: the arm never leaves `Parked` in this unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativeArm {
    Parked,
}

/// Pure press classification over plain data (no OS touch).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NativePress {
    /// Affirmative titlebar-move candidate: press inside the titlebar band
    /// of the owned standard window with a fresh AX+CG join, no fullscreen,
    /// no modal/transient shape.
    TitlebarMove,
    /// Press classifies as resize (edge/corner): never a move source.
    Resize,
    /// Anything else: not a native drag source.
    Reject(&'static str),
}

/// Titlebar band height in points (top of the AX frame, AX top-left
/// origin). Conservative: only presses clearly inside the chrome count.
pub const TITLEBAR_BAND: f64 = 28.0;

/// Minimum owned-helper frame for a reliable titlebar observation (points).
pub const MIN_TITLEBAR_W: f64 = 60.0;
/// Minimum height carrying titlebar chrome (points).
pub const MIN_TITLEBAR_H: f64 = 24.0;

/// Affirmative native-press classifier over plain data: the press point must
/// lie inside the titlebar band of the joined AX frame, the AX hit owner
/// must equal the exact held `(pid, wid)`, the CG owner join must hold, the
/// window must be a standard non-fullscreen non-minimized tile shape on the
/// normal layer, and the press must be an unmodified left press. Every miss
/// rejects with its leg (never guessed).
#[allow(clippy::too_many_arguments)]
#[must_use]
pub const fn classify_native_press(
    press_x: f64,
    press_y: f64,
    frame_x: f64,
    frame_y: f64,
    frame_w: f64,
    frame_h: f64,
    ax_hit_matches_owned: bool,
    cg_owned: bool,
    standard_tile_shape: bool,
    fullscreen: bool,
    minimized: bool,
    normal_layer: bool,
    left_unmodified: bool,
) -> NativePress {
    if !left_unmodified {
        return NativePress::Reject("modified-press");
    }
    if !cg_owned || !ax_hit_matches_owned {
        return NativePress::Reject("identity-mismatch");
    }
    if fullscreen || minimized {
        return NativePress::Reject("overlay-or-minimized");
    }
    if !normal_layer || !standard_tile_shape {
        return NativePress::Reject("non-tile-shape");
    }
    if frame_w < MIN_TITLEBAR_W || frame_h < MIN_TITLEBAR_H {
        return NativePress::Reject("frame-too-small");
    }
    if press_x < frame_x || press_x > frame_x + frame_w {
        return NativePress::Reject("outside-frame");
    }
    if press_y < frame_y || press_y > frame_y + TITLEBAR_BAND {
        // Below the band is client content (possible resize/content drag);
        // edges classify resize downstream, never a move here.
        if press_y > frame_y + frame_h {
            return NativePress::Reject("outside-frame");
        }
        return NativePress::Resize;
    }
    NativePress::TitlebarMove
}

/// Move-vs-resize confirmation over plain data (no OS touch): a host-native
/// move keeps the frame size while the origin travels; any size change is a
/// resize (or a system rescale) and never confirms a move. Zero travel
/// confirms nothing.
#[must_use]
pub const fn confirm_native_move(
    before_w: f64,
    before_h: f64,
    after_w: f64,
    after_h: f64,
    moved_origin: bool,
) -> bool {
    if !moved_origin {
        return false;
    }
    before_w == after_w && before_h == after_h
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(x: f64, y: f64) -> NativePress {
        classify_native_press(
            x, y, 100.0, 100.0, 480.0, 320.0, true, true, true, false, false, true, true,
        )
    }

    #[test]
    fn native_arm_stays_parked_with_reason() {
        assert_eq!(NativeArm::Parked, NativeArm::Parked);
        assert!(NATIVE_PARKED_REASON.contains("parked"));
        assert!(NATIVE_PARKED_REASON.contains("AX moved"));
    }

    #[test]
    fn titlebar_band_affirms_only_the_chrome() {
        assert_eq!(press(200.0, 108.0), NativePress::TitlebarMove);
        assert_eq!(press(200.0, 200.0), NativePress::Resize);
        assert_eq!(press(2000.0, 108.0), NativePress::Reject("outside-frame"));
    }

    #[test]
    fn identity_and_shape_refuse_before_geometry() {
        assert_eq!(
            classify_native_press(
                200.0, 108.0, 100.0, 100.0, 480.0, 320.0, false, true, true, false, false, true,
                true,
            ),
            NativePress::Reject("identity-mismatch")
        );
        assert_eq!(
            classify_native_press(
                200.0, 108.0, 100.0, 100.0, 480.0, 320.0, true, false, true, false, false, true,
                true,
            ),
            NativePress::Reject("identity-mismatch")
        );
        assert_eq!(
            classify_native_press(
                200.0, 108.0, 100.0, 100.0, 480.0, 320.0, true, true, true, true, false, true,
                true,
            ),
            NativePress::Reject("overlay-or-minimized")
        );
        assert_eq!(
            classify_native_press(
                200.0, 108.0, 100.0, 100.0, 480.0, 320.0, true, true, false, false, false, true,
                true,
            ),
            NativePress::Reject("non-tile-shape")
        );
        assert_eq!(
            classify_native_press(
                200.0, 108.0, 100.0, 100.0, 480.0, 320.0, true, true, true, false, false, true,
                false,
            ),
            NativePress::Reject("modified-press")
        );
        assert_eq!(
            classify_native_press(
                200.0, 108.0, 100.0, 100.0, 10.0, 320.0, true, true, true, false, false, true,
                true,
            ),
            NativePress::Reject("frame-too-small")
        );
    }

    #[test]
    fn move_confirmation_needs_travel_without_resize() {
        assert!(confirm_native_move(480.0, 320.0, 480.0, 320.0, true));
        assert!(!confirm_native_move(480.0, 320.0, 480.0, 320.0, false));
        assert!(!confirm_native_move(480.0, 320.0, 500.0, 320.0, true));
        assert!(!confirm_native_move(480.0, 320.0, 480.0, 400.0, true));
    }
}
