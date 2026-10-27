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
// monotonic invalidation counter, the separately minted structural floor
// (monotonic value as of the last structural invalidation), the
// facts-instance namespace it was seeded with, and the exhaustion latch.
// The effect only passes observed facts (frames, activation, gesture
// boundaries, endpoint payloads) in; the router decides every
// witness/floor/latch/order transition. Native show predicates (foreground
// match, liveness, endpoint) stay in the effect, which additionally gates
// drawn borders/underlay on held coverage equal to the actual latest and
// the held preview on coverage at or past the structural floor (motion
// advances the monotonic witness without touching the floor, so the valid
// same-gesture preview persists while strict shapes hide).

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
    // namespace with the presentation-facts instance. Advances on every
    // native frame (including genuine mover motion), activation/focus,
    // fullscreen transitions, visibility changes, layout add/remove, and
    // gesture boundaries; the facts snapshot publishes it and captures
    // cover it. Structural routes additionally advance the floor below;
    // mover-motion routes do not. Never reset except by an explicit fresh
    // source instance (a new instance makes old counters incomparable).
    // Never established by arriving paint.
    quint64 counter = 0;
    // Witness source namespace the counter was seeded with. Empty until
    // the effect seeds it from the live facts object.
    QString instance;
    // Effect-owned structural floor (Stage 5): the monotonic counter value
    // as of the last structural invalidation (non-mover frames,
    // activation/focus, fullscreen transitions, gesture boundaries, and
    // layout add/remove). Genuine mover-motion frames advance the
    // monotonic witness without touching this floor. The renderer shows
    // the held same-gesture preview while its covered counter reaches at
    // least this floor; pre-event coverage hides it immediately with no
    // repaint. Same instance namespace as the counter; reset with it.
    quint64 structuralFloor = 0;
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
        structuralFloor = 0;
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
    // order/invalidation evidence, then the causal bump plus the
    // structural tick.
    void onActivation(const QString &inst)
    {
        clearPaintDisplay(&paint);
        invalidate(InvalidationScope::All, inst);
        tickStructural();
    }

    // Production tracked-frame route: the union scope (Border plus
    // Underlay) plus the causal bump and the structural tick. The tracked
    // window is always group-subscribed, so its frame would otherwise bump
    // twice (once here, once via the member route); the member handler
    // skips the tracked window and this single bump carries both floors,
    // exactly the floors the retired double bump set. One frame advances
    // the witness exactly once.
    void onTrackedFrame(const QString &inst)
    {
        invalidate(InvalidationScope::All, inst);
        tickStructural();
    }

    // Production member-frame route: underlay dirt plus the causal bump
    // and the structural tick, regardless of held display state (relevance
    // is observed facts, not visible paint: hidden members and
    // pre-first-paint geometry count).
    void onMemberFrame(const QString &inst)
    {
        invalidate(InvalidationScope::Underlay, inst);
        tickStructural();
    }

    // Active-gesture mover-motion route: the genuine live interactive
    // mover's own frame tracks the pointer. Exactly one monotonic bump
    // with the union scope (All for the tracked mover, Underlay for a
    // subscribed member mover) so strict border/underlay hide until fresh
    // covering paint; the structural floor is untouched, so the valid
    // same-gesture preview persists. The caller classifies genuine motion
    // by live gesture identity plus the interactive-move flag (never
    // timing, geometry, or cursor matching); anything else takes the
    // structural routes above. Publishes through the stage like every
    // other bump.
    void onMotionFrame(bool tracked, const QString &inst)
    {
        invalidate(tracked ? InvalidationScope::All : InvalidationScope::Underlay, inst);
    }

    // Structural tick: record the post-bump monotonic counter as the floor
    // the held preview coverage must reach. Called after invalidate() in
    // every structural route (never on the mover-motion route). The
    // namespace is already resolved by the preceding invalidate; an empty
    // or exhausted source records nothing.
    void tickStructural()
    {
        if (instance.isEmpty() || exhausted) {
            return;
        }
        structuralFloor = counter;
    }

    // Fullscreen-mode invalidation route: the tracked window's fullscreen
    // state flipped (enter or exit). No geometry moved, but every held
    // shape was computed pre-transition, so the causal witness advances
    // across all scopes (both floors) and no stale pre-enter/pre-exit
    // paint applies. No latch clear: a gesture may span the transition
    // and the producer match still gates the preview.
    void onFullscreenChanged(const QString &inst)
    {
        invalidate(InvalidationScope::All, inst);
        tickStructural();
    }

    // Member visibility route (minimize/restore, hide/show): membership
    // behind the underlay changes and a drop target may vanish with no
    // frame signal, so the causal witness advances across all scopes plus
    // the structural tick, unconditionally (observed facts, never held
    // membership: hidden members, cleared lists, and pre-first-paint
    // windows all count; a skipped bump would let an in-flight
    // older-covered paint resurrect a stale preview).
    void onVisibilityChanged(const QString &inst)
    {
        invalidate(InvalidationScope::All, inst);
        tickStructural();
    }

    // Layout add/remove route (independent target/layout change): the
    // membership behind the underlay and the drop target behind the
    // preview may have changed with no frame signal, so the causal
    // witness advances across all scopes plus the structural tick. The
    // open gesture (if any) still owns the preview through its producer
    // match, but pre-event coverage hides immediately with no repaint.
    void onLayoutChanged(const QString &inst)
    {
        invalidate(InvalidationScope::All, inst);
        tickStructural();
    }

    // Production gesture-boundary route: drop the held drop-preview latch
    // (it belongs to exactly one live gesture) plus the causal bump with
    // no floors. Cancellation/terminal semantics are untouched (no
    // terminal fabricated here - transitions only).
    void onGestureBoundary(const QString &inst)
    {
        clearPaintPreview(&paint);
        invalidate(InvalidationScope::None, inst);
        tickStructural();
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
    // witness plus the structural floor from effect authority (never from
    // arriving paint), refuse while exhausted or source-less, then
    // parse/apply. Paints prove against true live source plus counter
    // (exact cover) or against the seeded floor in the same instance
    // (structural path for pre-motion paint) even after a stream reset or
    // before any first paint.
    PresentationApply applyEndpointPaint(const QString &payload, const QString &liveGestureInstance,
        const QString &liveGestureSerial)
    {
        if (!instance.isEmpty()) {
            paint.latest = CoverWitness{instance, counter, true};
            paint.structuralFloor = CoverWitness{instance, structuralFloor, true};
        }
        if (exhausted) {
            return PresentationApply::StaleIgnored;
        }
        return applyPresentationPaint(&paint, payload, liveGestureInstance, liveGestureSerial);
    }
};

} // namespace KWin
