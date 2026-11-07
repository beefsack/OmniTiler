#include "presentation_stage5.h"

namespace KWin
{

namespace
{

// Border anchor facts for one endpoint/event from one injected host view:
// unknown unless every live identity/geometry/eligibility/scale fact is
// present. The router re-proves the known facts against the held
// descriptor on each use; underlay/preview never consult them.
KWin::BorderAnchorFacts anchorFactsFromHost(const KWin::Stage5HostView &host)
{
    KWin::BorderAnchorFacts facts;
    facts.known = !host.trackedId.isEmpty() && !host.liveActiveId.isEmpty() && !host.outputId.isEmpty()
        && host.trackedFrameKnown && host.liveEligibleKnown && host.outputScaleKnown;
    facts.trackedId = host.trackedId;
    facts.liveActiveId = host.liveActiveId;
    facts.outputId = host.outputId;
    facts.liveFrame = host.trackedFrame;
    facts.liveFrameKnown = host.trackedFrameKnown;
    facts.liveEligible = host.liveEligible;
    facts.liveEligibleKnown = host.liveEligibleKnown;
    facts.outputScale = host.outputScale;
    facts.outputScaleKnown = host.outputScaleKnown;
    return facts;
}

} // namespace

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

Stage5Refresh PresentationStage5::onTrackedFrame(const Stage5HostView &host)
{
    // The tracked (active) window frame moved: exactly one causal bump
    // with the union scope (the tracked window is always group-subscribed,
    // so the member handler skips it on the same emission) plus the
    // structural tick, published so the next actual facts capture covers
    // it. With live anchor facts the router advances the border chain by
    // event proof when the prior chain equals the actual pre-event cover
    // and identity/eligibility/output-scale are unchanged and known; a
    // missed gap, source change, output/scale change, or unknown facts
    // clear the chain while underlay and preview keep the existing global
    // coverage/floor gates. No live-frame comparison: a move that returns
    // to its start stays hidden without a fresh paint. No old-paint
    // re-arm: continuity proves only by the delivered event.
    m_router->onTrackedFrame(factsInstance(), anchorFactsFromHost(host));
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

Stage5Refresh PresentationStage5::onMotionFrame(bool tracked, const Stage5HostView &host)
{
    // Genuine mover motion (classified by the caller via live gesture
    // identity plus the interactive-move flag): exactly one monotonic bump
    // with the union scope, published so the next actual facts capture
    // covers it, WITHOUT the structural tick. With live anchor facts the
    // tracked motion advances the border chain by the same event proof as
    // the structural route; member motion never advances it, and unknown
    // facts clear it. Strict underlay hides until fresh covering paint
    // while the valid same-gesture preview stays structurally covered.
    // Refresh only the dirtied surfaces; the preview is never refreshed
    // here (it stays visible).
    m_router->onMotionFrame(tracked, factsInstance(), anchorFactsFromHost(host));
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

Stage5Refresh PresentationStage5::onGestureBoundary(const Stage5HostView &host)
{
    // Gesture boundary: drop the held drop-preview latch (it belongs to
    // exactly one live gesture) plus the causal bump and the structural
    // tick (no shape floors: the None scope snapshots none), then publish
    // so the next actual facts capture covers it. With same-anchor live
    // facts the border chain advances by the same event proof as frame
    // routes; unknown facts clear it. The caller re-evaluates every
    // surface.
    m_router->onGestureBoundary(factsInstance(), anchorFactsFromHost(host));
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
    const QString &payload, const QString &liveGestureInstance, const QString &liveGestureSerial,
    const Stage5HostView &hostForBorder)
{
    // The router re-seeds the order-independent latest witness from effect
    // authority first, so no arriving paint can establish it. Same-instance
    // older coverage below the structural floor, foreign-instance coverage,
    // future coverage, older order, no live source, or malformed shape is
    // rejected without touching the newer held display (no refresh at all).
    // Malformed clears display-only while retaining the fence, witness, and
    // floors, and clears the border chain. On a newly accepted paint the
    // router arms or retains the border chain (exact current coverage
    // clears first then re-proves with exact live anchor authority; older
    // structural coverage keeps the held descriptor and chain only on the
    // proven same anchor, otherwise clears with no new join) while
    // underlay/preview/order/coverage adopt the new paint.
    EndpointResult result;
    result.apply = m_router->applyEndpointPaint(payload, liveGestureInstance, liveGestureSerial);
    if (result.apply == PresentationApply::StaleIgnored) {
        return result;
    }
    if (result.apply == PresentationApply::Accepted) {
        m_router->tryArmBorderChain(anchorFactsFromHost(hostForBorder));
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
    // Pure renderer: the held accepted underlay paints only on a live
    // foreground match, a fresh (geometry-clean) paint, held coverage equal
    // to the actual latest witness, plus the hide-only vetoes and a valid
    // anchor (the core Found already encodes the trigger). The preview
    // needs structurally current coverage (computed above) plus a proven
    // producer matching the live open gesture and live move liveness. The
    // held accepted border paints only on the live foreground match, known
    // live eligibility, its anchor chain current, the complete held
    // descriptor (outsets, width, radius all present), exact live
    // tracked/active identity, the current live output identity and scale
    // matching the arm-time proof, and valid live geometry derived once
    // from the live frame plus held outsets/width (held radius for the
    // ring); width 0 or invalid metrics narrowly hide the border while
    // underlay/preview keep their gates. Fullscreen presentation is owned
    // elsewhere: every fullscreen transition bumps all surfaces through
    // onFullscreenChanged, and the core omits the suspended output's
    // preview from the published projection. This decision adds no policy
    // of its own.
    const bool chainCurrent = m_router->borderChainCurrent();
    bool borderAnchorOk = false;
    bool borderGeometryOk = false;
    if (hasPaint && paint.borderVisible && paint.borderHasWitness && paint.borderHasOutsets && paint.borderHasWidth
        && paint.borderHasRadius && chainCurrent && host.liveEligible && host.liveEligibleKnown
        && host.trackedFrameKnown && host.outputScaleKnown && std::isfinite(host.outputScale)
        && host.outputScale > 0.0 && !host.trackedId.isEmpty() && !host.liveActiveId.isEmpty()
        && !host.outputId.isEmpty()) {
        const QString subject = QString::fromUtf8(paint.borderSubject);
        const QString win = QString::fromUtf8(paint.witnessWin);
        const QString fg = QString::fromUtf8(paint.witnessForeground);
        const QString heldFg = QString::fromUtf8(paint.foreground);
        if (!subject.isEmpty() && host.trackedId == subject && host.liveActiveId == subject && host.trackedId == win
            && host.trackedId == fg && host.trackedId == heldFg
            && host.outputId == m_router->borderOutputId && host.outputScale == m_router->borderOutputScale) {
            borderAnchorOk = true;
            QRect outer;
            QRect inner;
            if (KWin::borderGeometryFromLiveFrame(host.trackedFrame, paint.borderOutsetLeft, paint.borderOutsetTop,
                    paint.borderOutsetRight, paint.borderOutsetBottom, paint.borderWidth, &outer, &inner)) {
                borderGeometryOk = true;
            }
        }
    }
    Stage5Visibility visibility;
    visibility.border = hasPaint && paint.borderVisible && host.endpointAvailable && host.foregroundMatches
        && host.liveEligible && host.liveEligibleKnown && chainCurrent && borderAnchorOk && borderGeometryOk;
    visibility.underlay = hasPaint && paint.underlayFound && host.foregroundMatches && host.liveEligible
        && host.endpointAvailable && underlayFresh && current && host.anchorOk;
    visibility.preview = hasPaint && paint.previewHas && producerLive && host.moveLive && host.endpointAvailable
        && structuralCurrent;
    return visibility;
}

} // namespace KWin
