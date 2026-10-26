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
    // router to it (which restarts the epoch). Later bumps never publish.
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
    // order/invalidation evidence, then the causal bump, then the caller
    // refreshes every surface (the tracked window plus visibility refresh,
    // including the drag preview, stay host-side).
    m_router->onActivation(factsInstance());
    Stage5Refresh refresh;
    refresh.border = true;
    refresh.underlay = true;
    refresh.preview = true;
    return refresh;
}

Stage5Refresh PresentationStage5::onTrackedFrame()
{
    // The tracked (active) window frame moved: bump the causal witness
    // first (observed facts, never display state), then the caller hides
    // the ring until fresh covering paint arrives. Underlay and preview
    // stay unrefreshed (deficiency 2: stale items remain visible).
    m_router->onTrackedFrame(factsInstance());
    Stage5Refresh refresh;
    refresh.border = true;
    return refresh;
}

Stage5Refresh PresentationStage5::onMemberFrame()
{
    // Any subscribed window frame change bumps the causal witness: the
    // published outer depends on every member, so relevance is observed
    // facts, never held display state. The caller remaps the anchor and
    // re-evaluates the underlay only (deficiency 2: the border and the
    // preview stay unrefreshed).
    m_router->onMemberFrame(factsInstance());
    Stage5Refresh refresh;
    refresh.anchor = true;
    refresh.underlay = true;
    return refresh;
}

Stage5Refresh PresentationStage5::onGestureBoundary()
{
    // Gesture boundary: drop the held drop-preview latch (it belongs to
    // exactly one live gesture) plus the causal bump with no floors. The
    // caller re-evaluates the preview only (deficiency 2: the border and
    // the underlay stay unrefreshed).
    m_router->onGestureBoundary(factsInstance());
    Stage5Refresh refresh;
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
    const bool producerLive =
        KWin::previewProducerMatchesLive(&paint, host.liveGestureInstance, host.liveGestureSerial);
    // Pure renderer: the held accepted border shape paints only on a live
    // foreground match, a fresh (geometry-clean) paint, held coverage equal
    // to the actual latest witness, plus the hide-only vetoes. The ring
    // replays the published stroke so the drawn outer edge equals the
    // published outer rect exactly; color and radius stay live host
    // styling. The underlay needs the same plus a valid anchor (the core
    // Found already encodes the trigger). The preview needs a proven
    // producer matching the live open gesture plus live move liveness.
    Stage5Visibility visibility;
    visibility.border = hasPaint && paint.borderVisible && host.endpointAvailable && host.foregroundMatches
        && host.liveEligible && strokeOk && fresh && current;
    visibility.underlay = hasPaint && paint.underlayFound && host.foregroundMatches && host.liveEligible
        && host.endpointAvailable && underlayFresh && current && host.anchorOk;
    visibility.preview = hasPaint && paint.previewHas && producerLive && host.moveLive && host.endpointAvailable
        && current;
    return visibility;
}

} // namespace KWin
