// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#pragma once

// Production presentation routing (Stage 5, Qt-only, no KWin scene, no
// session bus): the exact production event/endpoint routing for the
// native paint state, factored so the offline consumer test drives it
// directly. The ActiveWindowBorderEffect owns one router and its signal
// handlers delegate paint-state transitions to it (thin delegation, no
// second policy owner, no new ordering authority).
//
// Single ownership: the router owns the held paint, the effect-owned
// monotonic invalidation counter, the facts-instance namespace it was
// seeded with, and the exhaustion latch. The effect only passes observed
// facts (frames, activation, gesture boundaries, endpoint payloads) in;
// the router decides every witness/floor/latch/order transition. Native
// show predicates (foreground match, liveness, endpoint) stay in the
// effect, which additionally gates every drawn surface on held coverage
// equal to the actual latest.

#include "presentation_consumer.h"

#include <QString>

namespace KWin
{

class PresentationRouter
{
public:
    // Held accepted presentation paint: complete core shapes plus the
    // serving generation/revision for order fencing and the causal
    // coverage witness. Never logged.
    PresentationPaint paint;
    // Effect-owned monotonic native-invalidation counter (Stage 5): one
    // namespace with the presentation-facts instance. Advances on
    // geometry, activation/focus, and gesture boundaries; the facts
    // snapshot publishes it and captures cover it. Never reset except by
    // an explicit fresh source instance (a new instance makes old
    // counters incomparable). Never established by arriving paint.
    quint64 counter = 0;
    // Witness source namespace the counter was seeded with. Empty until
    // the effect seeds it from the live facts object.
    QString instance;
    // Latched at the counter cap: no wrap, no pretend proof. Fail closed
    // (hide, record nothing further) until an explicit fresh source
    // instance recovers the namespace.
    bool exhausted = false;

    // Seed (or re-seed on proven replacement) the witness namespace from
    // the live facts object. A changed instance restarts the epoch
    // explicitly: the counter restarts, exhaustion clears, and stale
    // floors (incomparable across instances) drop. The subscriber-driven
    // stream reset already clears the paint on replacement; this never
    // paints, hides, or fabricates coverage.
    void setSourceInstance(const QString &inst)
    {
        if (inst.isEmpty() || inst == instance) {
            return;
        }
        instance = inst;
        counter = 0;
        exhausted = false;
        paint.borderFloor = CoverWitness{};
        paint.underlayFloor = CoverWitness{};
    }

    // Record one native invalidation for the affected scope and advance
    // the causal witness. Unknown source or exhaustion records nothing
    // (fail closed: hide already happened at the caller, never a wrapped
    // or pretend proof). An empty instance still advances the effect-owned
    // counter (matching previous behavior); the stored namespace is used
    // for the note so latest keeps tracking the counter.
    void invalidate(InvalidationScope scope, const QString &inst)
    {
        const QString use = inst.isEmpty() ? instance : inst;
        if (!use.isEmpty() && use != instance) {
            setSourceInstance(use);
        }
        if (instance.isEmpty() || exhausted) {
            return;
        }
        if (counter == UINT64_MAX) {
            exhausted = true;
            const CoverWitness fin{instance, counter, true};
            if (scope == InvalidationScope::Border || scope == InvalidationScope::All) {
                paint.borderFloor = fin;
            }
            if (scope == InvalidationScope::Underlay || scope == InvalidationScope::All) {
                paint.underlayFloor = fin;
            }
            return;
        }
        noteNativeInvalidation(&paint, instance, ++counter, scope);
    }

    // Production activation route: display-only hide preserving
    // order/invalidation evidence, then the causal bump.
    void onActivation(const QString &inst)
    {
        clearPaintDisplay(&paint);
        invalidate(InvalidationScope::All, inst);
    }

    // Production tracked-frame route: border dirt plus the causal bump.
    void onTrackedFrame(const QString &inst)
    {
        invalidate(InvalidationScope::Border, inst);
    }

    // Production member-frame route: underlay dirt plus the causal bump,
    // regardless of held display state (relevance is observed facts, not
    // visible paint: hidden members and pre-first-paint geometry count).
    void onMemberFrame(const QString &inst)
    {
        invalidate(InvalidationScope::Underlay, inst);
    }

    // Production gesture-boundary route: drop the held drop-preview latch
    // (it belongs to exactly one live gesture) plus the causal bump with
    // no floors. Cancellation/terminal semantics are untouched (no
    // terminal fabricated here - transitions only).
    void onGestureBoundary(const QString &inst)
    {
        clearPaintPreview(&paint);
        invalidate(InvalidationScope::None, inst);
    }

    // Explicit stream replacement: generation rotation, unbind, stop, or
    // proven effect-instance replacement. Resets display, order, and
    // floors; the live source truth (counter/namespace) is NOT forgotten
    // and is re-seeded at the next endpoint dispatch.
    void onStreamReset()
    {
        clearPresentationPaint(&paint);
    }

    // Display-only hide: clears the visible paint while retaining the
    // ordering fence, the causal witness, and the geometry floors.
    void onDisplayHide()
    {
        clearPaintDisplay(&paint);
    }

    // Production endpoint dispatch: re-seed the order-independent latest
    // witness from effect authority (never from arriving paint), refuse
    // while exhausted or source-less, then parse/apply. Paints prove
    // against true live source plus counter even after a stream reset or
    // before any first paint.
    PresentationApply applyEndpointPaint(const QString &payload, const QString &liveGestureInstance,
        const QString &liveGestureSerial)
    {
        if (!instance.isEmpty()) {
            paint.latest = CoverWitness{instance, counter, true};
        }
        if (exhausted) {
            return PresentationApply::StaleIgnored;
        }
        return applyPresentationPaint(&paint, payload, liveGestureInstance, liveGestureSerial);
    }
};

} // namespace KWin
