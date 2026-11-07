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
// facts-instance namespace it was seeded with, the exhaustion latch, and
// the BORDER-ONLY accepted anchor chain (no duplicate descriptor fields:
// the chain reuses the held border descriptor plus one cover plus the
// anchor/output-scale authority). The effect only passes observed facts
// (frames, activation, gesture boundaries, endpoint payloads) in; the
// router decides every witness/floor/latch/order/chain transition. Native
// show predicates (foreground match, liveness, endpoint) stay in the
// effect, which additionally gates drawn underlay on held coverage equal
// to the actual latest and the held preview on coverage at or past the
// structural floor (motion advances the monotonic witness without touching
// the floor, so the valid same-gesture preview persists while strict
// underlay hides). The drawn border needs its anchor chain current plus
// live vetoes instead of the global coverage/floor gates.

#include "presentation_consumer.h"

#include <QRect>
#include <QString>

namespace KWin
{

// Live border-anchor facts for one endpoint/event (Qt-only seam input):
// the actual live tracked/active/output identity plus the quantized live
// frame, eligibility, and output scale observed around the delivered
// event. Unknown by default (which clears the chain); the router
// re-proves every field against the held descriptor on each use and
// stores only the output identity/scale captured at arm time.
struct BorderAnchorFacts {
    bool known = false;
    QString trackedId;
    QString liveActiveId;
    QString outputId;
    QRect liveFrame;
    bool liveFrameKnown = false;
    bool liveEligible = false;
    bool liveEligibleKnown = false;
    qreal outputScale = 0.0;
    bool outputScaleKnown = false;
};

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
    // BORDER-ONLY accepted anchor chain: the proven native coverage for
    // the held border descriptor (same instance namespace as the counter).
    // Armed only on a newly accepted v2 endpoint with exact current native
    // coverage plus exact live anchor authority; advanced only on a
    // delivered same-anchor event (frame or gesture-lifecycle boundary with
    // live same-window proof) whose prior chain equals the actual pre-event
    // native cover; cleared on any missed gap, unproven event,
    // source/clear/reset/endpoint-loss/malformed, exhaustion, output-scale
    // change, or unknown eligibility. Reuses the
    // held descriptor (no duplicate descriptor fields) plus the proven
    // output identity/scale. Never mutates paint.covered; the chain is
    // the border's own current-proof.
    CoverWitness borderChain;
    // Proven output identity/scale captured at arm time. The draw gate
    // requires the current live facts to match these exactly; any output
    // or scale change arrives as a structural event that clears the chain.
    QString borderOutputId;
    qreal borderOutputScale = 0.0;
    bool borderOutputScaleKnown = false;

    // Seed (or re-seed on proven replacement) the witness namespace from
    // the live facts object. A changed instance restarts the epoch
    // explicitly: the counter restarts, exhaustion clears, and stale
    // floors (incomparable across instances) drop. The subscriber-driven
    // stream reset already clears the paint on replacement; this never
    // paints, hides, or fabricates coverage.
    void clearBorderChain()
    {
        borderChain = CoverWitness{};
        borderOutputId.clear();
        borderOutputScaleKnown = false;
    }

    bool borderChainCurrent() const
    {
        return borderChain.set && paint.latest.set && borderChain.instance == paint.latest.instance
            && borderChain.counter == paint.latest.counter;
    }

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
        clearBorderChain();
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
            clearBorderChain();
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
        clearBorderChain();
        invalidate(InvalidationScope::All, inst);
        tickStructural();
    }

    // Production tracked-frame route: the union scope (Border plus
    // Underlay) plus the causal bump and the structural tick. The tracked
    // window is always group-subscribed, so its frame would otherwise bump
    // twice (once here, once via the member route); the member handler
    // skips the tracked window and this single bump carries both floors,
    // exactly the floors the retired double bump set. One frame advances
    // the witness exactly once. With live anchor facts the border chain
    // advances by event proof when the prior chain equals the actual
    // pre-event native cover and identity/eligibility/output-scale are
    // unchanged and known; unknown facts (the default for callers without
    // live authority) clear the chain while the global accounting below
    // stays identical for the other surfaces.
    void onTrackedFrame(const QString &inst, const BorderAnchorFacts &facts = BorderAnchorFacts{})
    {
        const CoverWitness preChain = borderChain;
        const QString preInstance = instance;
        const quint64 preCounter = counter;
        invalidate(InvalidationScope::All, inst);
        tickStructural();
        if (!facts.known) {
            clearBorderChain();
            return;
        }
        advanceBorderChain(preChain, preInstance, preCounter, facts);
    }

    // Production member-frame route: underlay dirt plus the causal bump
    // and the structural tick, regardless of held display state (relevance
    // is observed facts, not visible paint: hidden members and
    // pre-first-paint geometry count).
    void onMemberFrame(const QString &inst)
    {
        clearBorderChain();
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
    // other bump. Member motion (tracked false) never advances the border
    // chain; tracked motion advances it only with live anchor facts under
    // the same gap/identity/eligibility/scale rules as the structural
    // route above.
    void onMotionFrame(bool tracked, const QString &inst, const BorderAnchorFacts &facts = BorderAnchorFacts{})
    {
        const CoverWitness preChain = borderChain;
        const QString preInstance = instance;
        const quint64 preCounter = counter;
        invalidate(tracked ? InvalidationScope::All : InvalidationScope::Underlay, inst);
        if (!tracked || !facts.known) {
            clearBorderChain();
            return;
        }
        advanceBorderChain(preChain, preInstance, preCounter, facts);
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
        clearBorderChain();
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
        clearBorderChain();
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
        clearBorderChain();
        invalidate(InvalidationScope::All, inst);
        tickStructural();
    }

    // Production gesture-boundary route: drop the held drop-preview latch
    // (it belongs to exactly one live gesture) plus the causal bump and
    // the structural tick (no shape floors: the None scope snapshots none;
    // the preview latch clears separately and the producer match gates it).
    // With live anchor facts for the same tracked window the border chain
    // advances by the same event proof as frame routes; unknown facts (the
    // default for callers without same-window proof) clear the chain while
    // the global bump/tick stays identical.
    // Cancellation/terminal semantics are untouched (no terminal
    // fabricated here - transitions only).
    void onGestureBoundary(const QString &inst, const BorderAnchorFacts &facts = BorderAnchorFacts{})
    {
        const CoverWitness preChain = borderChain;
        const QString preInstance = instance;
        const quint64 preCounter = counter;
        clearPaintPreview(&paint);
        invalidate(InvalidationScope::None, inst);
        tickStructural();
        if (!facts.known) {
            clearBorderChain();
            return;
        }
        advanceBorderChain(preChain, preInstance, preCounter, facts);
    }

    // Explicit stream replacement: generation rotation, unbind, stop, or
    // proven effect-instance replacement. Resets display, order, and
    // floors; the live source truth (counter/namespace) is NOT forgotten
    // and is re-seeded at the next endpoint dispatch.
    void onStreamReset()
    {
        clearPresentationPaint(&paint);
        clearBorderChain();
    }

    // Display-only hide: clears the visible paint while retaining the
    // ordering fence, the causal witness, and the geometry floors.
    void onDisplayHide()
    {
        clearPaintDisplay(&paint);
        clearBorderChain();
    }

    // BORDER-ONLY arm: called by the stage after a newly accepted v2
    // endpoint with the live anchor facts for the same endpoint path.
    // Reuses the held descriptor (no duplicate descriptor fields) and
    // stores only the proven output identity/scale. Re-arms only on exact
    // current coverage (clearing first); a structurally accepted paint
    // with older coverage is handled inside applyEndpointPaint below
    // (retention by prior event proof), so this safely no-ops there.
    // Returns true when armed; unknown facts or any
    // identity/eligibility/frame/scale gap leave the chain cleared so only
    // the border hides.
    bool tryArmBorderChain(const BorderAnchorFacts &facts)
    {
        // Only exact-current coverage re-arms: a structurally accepted
        // paint whose coverage predates the actual latest preserves a
        // proven chain (continuity by prior event proof), so the anchored
        // border persists while the floored underlay hides. Exact-current
        // coverage clears first, then re-proves below.
        if (!paint.covered.set || !paint.latest.set || paint.covered.instance != paint.latest.instance
            || paint.covered.counter != paint.latest.counter) {
            return false;
        }
        clearBorderChain();
        if (exhausted || instance.isEmpty()) {
            return false;
        }
        if (!paint.has || !paint.borderVisible || !paint.borderHasWitness) {
            return false;
        }
        if (paint.covered.instance != instance) {
            return false;
        }
        if (!proveBorderAnchor(facts, true)) {
            return false;
        }
        borderChain = paint.covered;
        borderOutputId = facts.outputId;
        borderOutputScale = facts.outputScale;
        borderOutputScaleKnown = true;
        return true;
    }

    // Production endpoint dispatch: re-seed the order-independent latest
    // witness plus the structural floor from effect authority (never from
    // arriving paint), refuse while exhausted or source-less, then
    // parse/apply. Paints prove against true live source plus counter
    // (exact cover) or against the seeded floor in the same instance
    // (structural path for pre-motion paint) even after a stream reset or
    // before any first paint. A structurally accepted older-coverage paint
    // is never a new join: the held border descriptor (kept verbatim) and
    // the chain survive only when the chain is already current, the stream
    // generation matches, and every anchor input equals the held one; any
    // mismatch clears the chain with no new join while
    // underlay/preview/order/coverage adopt the new paint. The new witness
    // obs/frame authorize nothing and are restored to the held values.
    PresentationApply applyEndpointPaint(const QString &payload, const QString &liveGestureInstance,
        const QString &liveGestureSerial)
    {
        if (!instance.isEmpty()) {
            paint.latest = CoverWitness{instance, counter, true};
            paint.structuralFloor = CoverWitness{instance, structuralFloor, true};
        }
        if (exhausted) {
            clearBorderChain();
            return PresentationApply::StaleIgnored;
        }
        const PresentationPaint pre = paint;
        const bool preCurrent = borderChainCurrent();
        const PresentationApply result = applyPresentationPaint(&paint, payload, liveGestureInstance, liveGestureSerial);
        if (result == PresentationApply::Malformed) {
            clearBorderChain();
            return result;
        }
        if (result == PresentationApply::StaleIgnored) {
            return result;
        }
        const bool exactCurrent = paint.covered.set && paint.latest.set
            && paint.covered.instance == paint.latest.instance && paint.covered.counter == paint.latest.counter;
        if (!exactCurrent
            && (!preCurrent || !paint.has || !pre.has || paint.gen.isEmpty() || paint.gen != pre.gen
                || !sameBorderAnchor(pre, paint))) {
            clearBorderChain();
            return result;
        }
        if (!exactCurrent) {
            paint.witnessObs = pre.witnessObs;
            paint.witnessObsValue = pre.witnessObsValue;
            paint.witnessFrame = pre.witnessFrame;
            if (!borderChainCurrent()) {
                clearBorderChain();
            }
        }
        return result;
    }

private:
    // Same-anchor predicate for structural retention (witness obs/frame
    // excluded: the new witness authorizes nothing and is restored to the
    // held values by the caller). All other border anchor inputs must match
    // exactly, including has-flag/null presence.
    static bool sameBorderAnchor(const PresentationPaint &held, const PresentationPaint &incoming)
    {
        return held.borderVisible && incoming.borderVisible && held.borderReason == incoming.borderReason
            && !held.borderSubject.isEmpty() && held.borderSubject == incoming.borderSubject
            && held.borderHasOutsets && incoming.borderHasOutsets
            && held.borderOutsetLeft == incoming.borderOutsetLeft
            && held.borderOutsetTop == incoming.borderOutsetTop
            && held.borderOutsetRight == incoming.borderOutsetRight
            && held.borderOutsetBottom == incoming.borderOutsetBottom && held.borderHasWidth
            && incoming.borderHasWidth && held.borderWidth == incoming.borderWidth && held.borderHasRadius
            && incoming.borderHasRadius && held.borderRadius == incoming.borderRadius && held.borderHasWitness
            && incoming.borderHasWitness && !held.witnessWin.isEmpty() && held.witnessWin == incoming.witnessWin
            && !held.witnessForeground.isEmpty() && held.witnessForeground == incoming.witnessForeground
            && !held.foreground.isEmpty() && held.foreground == incoming.foreground;
    }
    // Shared live-anchor proof against the held descriptor: every identity
    // (tracked, live-active, subject, witness window/foreground, held
    // foreground) names the same window, eligibility/scale are known live
    // facts, the scale is finite positive, and the full border descriptor
    // (outsets, width, radius) is present. The arm path additionally
    // requires the current live frame to equal the witness frame; the
    // advance path proves continuity by the delivered event itself, never
    // by re-reading the frame.
    bool proveBorderAnchor(const BorderAnchorFacts &facts, bool requireFrame) const
    {
        if (!facts.known) {
            return false;
        }
        if (facts.trackedId.isEmpty() || facts.liveActiveId.isEmpty() || facts.outputId.isEmpty()) {
            return false;
        }
        if (!facts.liveFrameKnown || !facts.liveEligibleKnown || !facts.liveEligible) {
            return false;
        }
        if (!facts.outputScaleKnown || !std::isfinite(facts.outputScale) || !(facts.outputScale > 0.0)) {
            return false;
        }
        if (!paint.has || !paint.borderVisible || !paint.borderHasWitness) {
            return false;
        }
        if (!paint.borderHasOutsets || !paint.borderHasWidth || !paint.borderHasRadius) {
            return false;
        }
        const QString subject = QString::fromUtf8(paint.borderSubject);
        const QString win = QString::fromUtf8(paint.witnessWin);
        const QString fg = QString::fromUtf8(paint.witnessForeground);
        const QString heldFg = QString::fromUtf8(paint.foreground);
        if (subject.isEmpty() || win.isEmpty() || fg.isEmpty() || heldFg.isEmpty()) {
            return false;
        }
        if (facts.trackedId != subject || facts.liveActiveId != subject) {
            return false;
        }
        if (facts.trackedId != win || facts.trackedId != fg || facts.trackedId != heldFg) {
            return false;
        }
        if (requireFrame && facts.liveFrame != paint.witnessFrame) {
            return false;
        }
        return true;
    }

    // BORDER-ONLY advance on a delivered same-anchor geometry event: the
    // caller already ran the existing global bump exactly once. Advances
    // only when the prior chain equals the actual pre-event instance and
    // counter (no missed gap, no source change), the post-event latest is
    // exactly that one bump, and the live anchor still proves the held
    // descriptor on the same output and scale. Never mutates
    // paint.covered; the chain takes the post-event latest by event proof.
    bool advanceBorderChain(
        const CoverWitness &preChain, const QString &preInstance, quint64 preCounter, const BorderAnchorFacts &facts)
    {
        if (exhausted || instance.isEmpty()) {
            clearBorderChain();
            return false;
        }
        if (!preChain.set || preChain.instance != preInstance || preChain.counter != preCounter) {
            clearBorderChain();
            return false;
        }
        if (preInstance != instance || counter != preCounter + 1) {
            clearBorderChain();
            return false;
        }
        if (!paint.latest.set || paint.latest.instance != instance || paint.latest.counter != counter) {
            clearBorderChain();
            return false;
        }
        if (!proveBorderAnchor(facts, false)) {
            clearBorderChain();
            return false;
        }
        if (!borderOutputScaleKnown || facts.outputId != borderOutputId || facts.outputScale != borderOutputScale) {
            clearBorderChain();
            return false;
        }
        borderChain = paint.latest;
        return true;
    }
};

} // namespace KWin
