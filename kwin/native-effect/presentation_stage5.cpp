// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#include "presentation_stage5.h"

namespace KWin
{

PresentationStage5::PresentationStage5(PresentationRouter &router, PresentationFactsSource *facts)
    : m_router(&router)
    , m_facts(facts)
{
}

void PresentationStage5::bindFacts(PresentationFactsSource *facts)
{
    m_facts = facts;
    if (m_facts == nullptr) {
        return;
    }
    // Seed order matches the retired inline body exactly: publish the
    // current counter under the fresh instance first, then namespace the
    // router to it (which restarts the epoch). Later bumps publish through
    // publishInvalidation.
    m_facts->noteInvalidation(m_router->counter);
    m_router->setSourceInstance(m_facts->instance());
}

void PresentationStage5::publishInvalidation()
{
    if (m_facts == nullptr) {
        return;
    }
    m_facts->noteInvalidation(m_router->counter);
}

QString PresentationStage5::factsInstance() const
{
    if (m_facts == nullptr) {
        return QString();
    }
    return m_facts->instance();
}

Stage5Refresh PresentationStage5::onActivation()
{
    // Production activation route: display-only hide preserving
    // order/invalidation evidence, then the causal bump, then publish so
    // the next actual facts capture covers it. The caller refreshes every
    // surface (the tracked window plus visibility refresh, including the
    // drag preview, stay host-side).
    m_router->onActivation(factsInstance());
    publishInvalidation();
    Stage5Refresh refresh;
    refresh.border = true;
    refresh.anchor = true;
    refresh.underlay = true;
    refresh.preview = true;
    return refresh;
}

Stage5Refresh PresentationStage5::onTrackedFrame()
{
    // The tracked (active) window frame moved: exactly one causal bump
    // with the union scope (the tracked window is always group-subscribed,
    // so the member handler skips it on the same emission) plus the
    // structural tick, published so the next actual facts capture covers
    // it. The caller hides every surface until fresh covering paint
    // arrives. No live-frame comparison: a move that returns to its start
    // stays hidden without a fresh paint.
    m_router->onTrackedFrame(factsInstance());
    publishInvalidation();
    Stage5Refresh refresh;
    refresh.border = true;
    refresh.anchor = true;
    refresh.underlay = true;
    refresh.preview = true;
    return refresh;
}

Stage5Refresh PresentationStage5::onMemberFrame()
{
    // Any subscribed non-tracked window frame change bumps the causal
    // witness plus the structural tick: the published outer depends on
    // every member, so relevance is observed facts, never held display
    // state. Publish so the next actual facts capture covers it. The
    // caller remaps the anchor and re-evaluates every surface.
    m_router->onMemberFrame(factsInstance());
    publishInvalidation();
    Stage5Refresh refresh;
    refresh.border = true;
    refresh.anchor = true;
    refresh.underlay = true;
    refresh.preview = true;
    return refresh;
}

Stage5Refresh PresentationStage5::onMotionFrame(bool tracked)
{
    // Genuine mover motion (classified by the caller via live gesture
    // identity plus the interactive-move flag): exactly one monotonic bump
    // with the union scope, published so the next actual facts capture
    // covers it, WITHOUT the structural tick. Strict border/underlay hide
    // until fresh covering paint while the valid same-gesture preview
    // stays structurally covered. Refresh only the dirtied surfaces; the
    // preview is never refreshed here (it stays visible).
    m_router->onMotionFrame(tracked, factsInstance());
    publishInvalidation();
    Stage5Refresh refresh;
    refresh.border = tracked;
    refresh.underlay = true;
    return refresh;
}

Stage5Refresh PresentationStage5::onLayoutChanged()
{
    // Independent layout add/remove: membership behind the underlay and
    // the drop target behind the preview may have changed with no frame
    // signal. The causal witness advances across all scopes plus the
    // structural tick, published so the next actual facts capture covers
    // it; pre-event coverage hides immediately with no repaint. The caller
    // refreshes every surface.
    m_router->onLayoutChanged(factsInstance());
    publishInvalidation();
    Stage5Refresh refresh;
    refresh.border = true;
    refresh.anchor = true;
    refresh.underlay = true;
    refresh.preview = true;
    return refresh;
}

Stage5Refresh PresentationStage5::onVisibilityChanged(const QString &bareId)
{
    // Member minimize/restore or hide/show: membership behind the underlay
    // changes and a drop target may vanish with no frame signal, so the
    // causal witness advances across all scopes plus the structural tick,
    // unconditionally (hidden members and pre-first-paint windows count),
    // published so the next actual facts capture covers it. Border,
    // underlay, and preview refresh so stale shapes hide immediately; the
    // anchor still remaps only when the window names a held paint member
    // (extracted no-bump route below keeps its exact contract).
    m_router->onVisibilityChanged(factsInstance());
    publishInvalidation();
    Stage5Refresh refresh;
    refresh.border = true;
    refresh.underlay = true;
    refresh.preview = true;
    const Stage5Refresh member = onMemberVisibility(bareId);
    refresh.anchor = member.anchor;
    refresh.underlay = refresh.underlay || member.underlay;
    return refresh;
}

Stage5Refresh PresentationStage5::onFullscreenChanged()
{
    // Fullscreen transition (enter or exit): the causal witness advances
    // across all scopes (both floors) and publishes so the next actual
    // facts capture covers it, then the caller refreshes every surface
    // until fresh covering paint arrives. Pre-transition shapes must not
    // linger in either direction. The live fullscreen veto already hides
    // immediately; this rejects stale paints by order and coverage.
    m_router->onFullscreenChanged(factsInstance());
    publishInvalidation();
    Stage5Refresh refresh;
    refresh.border = true;
    refresh.anchor = true;
    refresh.underlay = true;
    refresh.preview = true;
    return refresh;
}

Stage5Refresh PresentationStage5::onGestureBoundary()
{
    // Gesture boundary: drop the held drop-preview latch (it belongs to
    // exactly one live gesture) plus the causal bump with no floors, then
    // publish so the next actual facts capture covers it. The caller
    // re-evaluates every surface.
    m_router->onGestureBoundary(factsInstance());
    publishInvalidation();
    Stage5Refresh refresh;
    refresh.border = true;
    refresh.anchor = true;
    refresh.underlay = true;
    refresh.preview = true;
    return refresh;
}

Stage5Refresh PresentationStage5::onMemberVisibility(const QString &bareId)
{
    // Member minimize/restore or hide/show re-anchors to the lowest-stacked
    // renderable member without timers or polling. Bare-UUID compare only
    // against the held paint list; a non-member refreshes nothing. No bump:
    // visibility alone is not an invalidation.
    Stage5Refresh refresh;
    if (bareId.isEmpty() || !m_router->paint.underlayMembers.contains(bareId)) {
        return refresh;
    }
    refresh.anchor = true;
    refresh.underlay = true;
    return refresh;
}

PresentationStage5::EndpointResult PresentationStage5::applyEndpoint(
    const QString &payload, const QString &liveGestureInstance, const QString &liveGestureSerial)
{
    // The router re-seeds the order-independent latest witness from effect
    // authority first, so no arriving paint can establish it. Older
    // revision, no live source, or coverage that is not exactly the live
    // source plus counter is ignored without touching the newer held
    // display (no refresh at all). Malformed clears display-only while
    // retaining the fence, witness, and floors.
    EndpointResult result;
    result.apply = m_router->applyEndpointPaint(payload, liveGestureInstance, liveGestureSerial);
    if (result.apply == PresentationApply::StaleIgnored) {
        return result;
    }
    result.refresh.border = true;
    result.refresh.anchor = true;
    result.refresh.underlay = true;
    result.refresh.preview = true;
    result.refresh.repaint = true;
    return result;
}

Stage5Refresh PresentationStage5::hideForEndpointLoss()
{
    const bool hadPaint = m_router->paint.has;
    m_router->onDisplayHide();
    Stage5Refresh refresh;
    refresh.border = true;
    refresh.anchor = true;
    refresh.underlay = true;
    refresh.preview = true;
    refresh.repaint = hadPaint;
    return refresh;
}

Stage5Refresh PresentationStage5::resetStream()
{
    // Explicit stream replacement: display, order, and floors reset; the
    // live source truth is NOT forgotten and is re-seeded from effect
    // authority at the next endpoint dispatch.
    const bool hadPaint = m_router->paint.has;
    m_router->onStreamReset();
    Stage5Refresh refresh;
    refresh.border = true;
    refresh.anchor = true;
    refresh.underlay = true;
    refresh.preview = true;
    refresh.repaint = hadPaint;
    return refresh;
}

Stage5Visibility PresentationStage5::decide(const Stage5HostView &host) const
{
    const PresentationPaint &paint = m_router->paint;
    const bool hasPaint = paint.has;
    bool strokeOk = false;
    if (hasPaint) {
        if (const auto derived = KWin::borderStrokeFromPublished(paint.borderInner, paint.borderOuter)) {
            strokeOk = true;
        }
    }
    const bool fresh = KWin::presentationBorderFresh(&paint);
    const bool underlayFresh = KWin::presentationUnderlayFresh(&paint);
    const bool current = KWin::presentationCoveredCurrent(&paint);
    // Structural currency for the preview (independent coverage lane):
    // the held paint covered at or past the structural floor in the same
    // instance. Motion advances the monotonic witness without touching the
    // floor, so the valid same-gesture preview persists while strict
    // borders hide; any structural invalidation moves the floor past
    // pre-event coverage, hiding the preview immediately with no repaint.
    const bool structuralCurrent = paint.covered.set && !m_router->instance.isEmpty()
        && paint.covered.instance == m_router->instance
        && paint.covered.counter >= m_router->structuralFloor;
    const bool producerLive =
        KWin::previewProducerMatchesLive(&paint, host.liveGestureInstance, host.liveGestureSerial);
    // Pure renderer: the held accepted border shape paints only on a live
    // foreground match, a fresh (geometry-clean) paint, held coverage equal
    // to the actual latest witness, plus the hide-only vetoes. The ring
    // replays the published stroke so the drawn outer edge equals the
    // published outer rect exactly; color and radius stay live host
    // styling. The underlay needs the same plus a valid anchor (the core
    // Found already encodes the trigger). The preview needs structurally
    // current coverage (computed above) plus a proven producer matching
    // the live open gesture and live move liveness.
    // Fullscreen presentation is owned elsewhere: every fullscreen
    // transition bumps all surfaces through onFullscreenChanged, and the
    // core omits the suspended output's preview from the published
    // projection. This decision adds no policy of its own.
    Stage5Visibility visibility;
    visibility.border = hasPaint && paint.borderVisible && host.endpointAvailable && host.foregroundMatches
        && host.liveEligible && strokeOk && fresh && current;
    visibility.underlay = hasPaint && paint.underlayFound && host.foregroundMatches && host.liveEligible
        && host.endpointAvailable && underlayFresh && current && host.anchorOk;
    visibility.preview = hasPaint && paint.previewHas && producerLive && host.moveLive && host.endpointAvailable
        && structuralCurrent;
    return visibility;
}

} // namespace KWin
