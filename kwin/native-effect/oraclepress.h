// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#pragma once

// Pure press-evidence predicates for the drag-oracle passive press capture.
// No KWin types cross in: callers pass the configured commandAll1/2/3 values
// plus Options::MouseUnrestrictedResize as plain ints, so this header stays
// testable with QtCore/QtGui only (see activeborderlogic_test.cpp). Policy:
// which pointer button the resize binding uses, the bounded monotonic press
// age, and the closed-vocabulary binding token echoed in the verdict.

#include <QtCore/qnamespace.h>

#include <cstdint>

namespace KWin
{

// Verified source defaults (Options::defaultCommandAll3() is
// MouseUnrestrictedResize, Options::defaultKeyCmdAllModKey() is Key_Alt):
// Alt+RightButton. Used only while the public options are unavailable.
inline Qt::MouseButton oracleDefaultResizeButton()
{
    return Qt::RightButton;
}

inline Qt::KeyboardModifier oracleDefaultResizeModifier()
{
    return Qt::AltModifier;
}

// Button slots map 1->Left, 2->Middle, 3->Right in slot order; the first
// slot carrying the resize command wins. Returns Qt::NoButton when no
// configured slot maps to resize (fail closed: no press can match).
inline Qt::MouseButton oracleResizeButton(int commandAll1, int commandAll2, int commandAll3, int resizeValue)
{
    if (commandAll1 == resizeValue) {
        return Qt::LeftButton;
    }
    if (commandAll2 == resizeValue) {
        return Qt::MiddleButton;
    }
    if (commandAll3 == resizeValue) {
        return Qt::RightButton;
    }
    return Qt::NoButton;
}

// Bounded monotonic age gate in milliseconds: the press must precede (or
// equal) the drag start and be at most 2s old. Steady-clock durations never
// go backwards, so a negative age reads as stale.
inline bool oraclePressAgeOk(int64_t ageMs)
{
    return ageMs >= 0 && ageMs <= 2000;
}

// Closed-vocabulary binding token echoed in the verdict press object. The
// script adapter chooses thirds handling by presence of the press object,
// never by this token's value beyond logging.
inline const char *oraclePressBindingName(bool configured)
{
    return configured ? "configured" : "default";
}

} // namespace KWin
