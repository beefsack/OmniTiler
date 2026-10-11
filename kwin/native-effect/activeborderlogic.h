// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#pragma once

#include <QColor>
#include <QRectF>

#include <cstdint>

#include "visual_policy_ffi.h"

namespace KWin
{

inline VisualPolicyRect visualPolicyRectFromQRectF(const QRectF &rect)
{
    return {rect.x(), rect.y(), rect.width(), rect.height()};
}

inline QRectF qrectFFromVisualPolicyRect(const VisualPolicyRect &rect)
{
    return {rect.x, rect.y, rect.w, rect.h};
}

inline QColor activeBorderColor(const QColor &themeColor, const QColor &fallbackColor, bool useThemeColor)
{
    // Preserve Qt's quantized alpha and original QColor representation.
    const bool themeValid = themeColor.isValid();
    const int themeAlpha = themeValid ? themeColor.alpha() : 0;
    if (visual_border_use_theme(useThemeColor ? 1 : 0, themeValid ? 1 : 0, static_cast<int32_t>(themeAlpha)) != 0) {
        return themeColor;
    }
    return fallbackColor;
}

inline QRectF activeBorderInnerRect(const QRectF &frameGeometry, double gap)
{
    return qrectFFromVisualPolicyRect(visual_border_inner_rect(visualPolicyRectFromQRectF(frameGeometry), gap));
}

struct ActiveBorderState
{
    bool visible;
    QRectF innerRect;
};

inline bool activeBorderIsMaximized(bool horizontal, bool vertical)
{
    return visual_border_is_maximized(horizontal ? 1 : 0, vertical ? 1 : 0) != 0;
}

// Direct-read observation seed from the native committed maximizeMode()
// (KWin::MaximizeMode: 0 restore, 1 vertical, 2 horizontal, 3 full). Any
// nonzero axis suppresses both borders; fullscreen stays suppressed
// independently via the live isFullScreen() observation. No script handoff,
// no epoch, no polling. Host-specific decoding stays in C++.
inline bool activeBorderSeedMaximized(int maximizeMode)
{
    return maximizeMode != 0;
}

inline ActiveBorderState activeBorderState(bool hasWindow, const QRectF &frameGeometry, bool deleted, bool minimized, bool fullScreen, bool maximized,
    bool appletPopup)
{
    const VisualPolicyBorderState state = visual_border_state(hasWindow ? 1 : 0, visualPolicyRectFromQRectF(frameGeometry),
        deleted ? 1 : 0, minimized ? 1 : 0, fullScreen ? 1 : 0, maximized ? 1 : 0, appletPopup ? 1 : 0);
    return {state.visible != 0, qrectFFromVisualPolicyRect(state.inner)};
}

// Independent drag-target preview policy (screen-space, window-agnostic):
// valid only for carried-geometry bounds (x/y in -16384..16384,
// w/h in 1..16384, matching the group-highlight carried bound). Invalid
// rects fail closed (never display). Visibility is solely "set versus
// explicitly cleared": never gated on Meta, focus, or the group outline.
inline bool dragPreviewRectValid(int x, int y, int w, int h)
{
    return visual_drag_preview_rect_valid(
        static_cast<int32_t>(x), static_cast<int32_t>(y), static_cast<int32_t>(w), static_cast<int32_t>(h))
        != 0;
}

// Group underlay extension default: sentinel -1 means "match the current
// configured border width". Explicit values (including 0) render as-is.
inline double groupUnderlayEffectiveExtension(double extension, double borderWidth)
{
    return visual_group_underlay_effective_extension(extension, borderWidth);
}

// Group underlay outer geometry: the Rust-carried union bounds expanded by
// the border gap plus the border width plus the resolved extension beyond
// the border outer edge. Pure so the effect remap after re-anchor and
// reconfigure stays testable without KWin.
inline QRectF groupUnderlayOuterRect(const QRectF &unionRect, double gap, double borderWidth, double extension)
{
    return qrectFFromVisualPolicyRect(
        visual_group_underlay_outer_rect(visualPolicyRectFromQRectF(unionRect), gap, borderWidth, extension));
}

// Movement-only group-underlay chord: both Win and Shift held. Extra
// modifiers are allowed (callers fold only these two), and either press
// order works because this is level-observed, never edge-sequenced. The
// predicate lives in Rust; this is only the native observation fold.
inline bool groupUnderlayChordHeld(bool winHeld, bool shiftHeld)
{
    return visual_group_underlay_chord_held(winHeld ? 1 : 0, shiftHeld ? 1 : 0) != 0;
}

} // namespace KWin
