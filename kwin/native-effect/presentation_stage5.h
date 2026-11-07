#pragma once

// Production Stage 5 invalidation/coverage/surface-visibility orchestration
// (Qt-only, KWin-independent): the exact handler bodies the KWin effect
// delegates to for native-invalidation counter advances (geometry,
// activation/focus, gesture boundaries), publication through the real
// PresentationFactsSource, coverage of accepted presentation, and every
// surface's refresh/visibility decision. No KWin headers: live host signal
// delivery, reads, and painting arrive as injected inputs, and the offline
// stage test drives these exact handlers with fabricated host views and
// stubbed surfaces.
//
// Every local counter advance publishes through the bound facts source, so
// the next actual facts capture covers it. Every invalidation route
// refreshes all surfaces (border, anchor, underlay, preview) so no stale
// item remains visible while the visibility decision already hides it.

#include "presentation_router.h"
#include "presentationfacts.h"

#include <QString>

namespace KWin
{

// Genuine mover-motion classification (never timing, geometry, cursor
// position, or pointer matching): the changed window names the live open
// gesture window, that gesture is open, and the live interactive-move
// flag is set. A same-window resize drag, scripted move, post-finish
// stray, or non-interactive frame is structural, never motion. All inputs
// are live reads supplied by the caller (identities plus the flag); the
// offline test pins the full matrix so a weaker check cannot return.
inline bool classifyMoverMotion(const QString &changedBareId, const QString &openWinId, bool gestureOpen,
    bool interactiveMove)
{
    return gestureOpen && !changedBareId.isEmpty() && changedBareId == openWinId && interactiveMove;
}

// Visible-to-visible preview-rect advance: the accepted preview rect may
// advance while shown (motion continuity delivers newer completed
// previews for the same gesture; covering captures replace older ones).
// Consume it verbatim (KWin logical units, no outset or scaling) instead
// of pinning the first rect. Returns true when the item must move/resize.
inline bool advanceDragPreviewRect(QRect &current, const QRect &incoming)
{
    if (current == incoming) {
        return false;
    }
    current = incoming;
    return true;
}

// Per-route refresh set: which host surfaces the caller refreshes after the
// route runs. Structural invalidation routes refresh all surfaces so stale
// items hide immediately until fresh covering paint arrives (the
// mover-motion route refreshes only its dirtied border/underlay: the
// structurally covered preview stays visible without refresh). Activation,
// endpoint, and reset routes refresh every surface; stale endpoint coverage
// refreshes nothing.
struct Stage5Refresh {
    bool border = false;
    bool anchor = false;
    bool underlay = false;
    bool preview = false;
    bool repaint = false;
};

// Live host reads for one visibility evaluation. Every field is a plain
// injected input: the effect supplies them from live window/scene state,
// the offline test fabricates them. No identity, geometry, or policy
// derivation happens here; the foreground match and liveness arrive
// precomputed from live reads. Border anchor authority (tracked/active
// identity, output identity, frame, eligibility-known, output scale)
// rides the same seam for the actual endpoint/event path; underlay/preview
// never consult it.
struct Stage5HostView {
    bool endpointAvailable = false;
    bool foregroundMatches = false;
    bool liveEligible = false;
    bool liveEligibleKnown = false;
    bool anchorOk = false;
    bool moveLive = false;
    QString liveGestureInstance;
    QString liveGestureSerial = QStringLiteral("0");
    QString trackedId;
    // Actual live active-window identity (bare UUID): the arm/advance/draw
    // proof requires subject == witness == held foreground == tracked ==
    // live-active, never the derived foreground-match flag alone.
    QString liveActiveId;
    // Tracked output identity (live output name) plus its scale: the draw
    // gate requires the current live facts to match the output identity
    // and scale proven at arm time.
    QString outputId;
    QRect trackedFrame;
    bool trackedFrameKnown = false;
    qreal outputScale = 0.0;
    bool outputScaleKnown = false;
};

// Decided visibility per surface, from the held accepted paint plus one
// injected host view. Drawn underlay requires held coverage equal to the
// actual latest witness; the drawn preview requires coverage at or past
// the structural floor instead (same instance). The drawn border needs its
// anchor chain current (router-owned, advanced only on delivered
// same-anchor frame or lifecycle-boundary events with live proof) plus
// live vetoes and valid live geometry
// (outer from live frame plus held outsets, inner inset by held width,
// held radius for the ring); unknown frame/subject/eligibility/scale hides
// only the border.
struct Stage5Visibility {
    bool border = false;
    bool underlay = false;
    bool preview = false;
};

// Single-owner Stage 5 orchestration over the caller's router. The router
// storage stays with the caller (the effect keeps its member); this class
// owns every counter/publication/coverage/refresh transition on it. The
// facts source stays caller-owned (the well-known D-Bus object); this
// class publishes through it at the seed bind and on every later bump.
class PresentationStage5
{
public:
    explicit PresentationStage5(PresentationRouter &router, PresentationFactsSource *facts = nullptr);

    // Seed bind at endpoint creation: publishes the current counter once
    // under the fresh instance, then namespaces the router to it (which
    // restarts the epoch). Later bumps publish through publishInvalidation.
    void bindFacts(PresentationFactsSource *facts);
    // Explicit publish of the current counter (seed path plus every later
    // invalidation advance).
    void publishInvalidation();
    QString factsInstance() const;

    // Counter-advance routes; each returns the surfaces the caller
    // refreshes. The tracked-frame route bumps once with the union scope
    // (the member handler skips the tracked window on the same emission),
    // member-frame, fullscreen-mode, layout, visibility, and gesture
    // boundaries bump their scope plus the structural tick, then refresh
    // all surfaces so no stale item remains visible. The mover-motion
    // route is the exception: it bumps monotonically with the union scope
    // but WITHOUT the structural tick (the valid same-gesture preview
    // persists) and refreshes only its dirtied border/underlay surfaces;
    // the preview is never refreshed there (it stays visible). The
    // tracked-frame and tracked-motion routes accept optional live anchor
    // facts (unknown by default): with them the border chain advances by
    // event proof, without them it clears, while the global
    // bump/tick/publish accounting stays identical for the other surfaces.
    // The gesture-boundary route takes the same optional same-anchor facts:
    // same-window proof advances the chain by the same one-bump rule,
    // unknown or mismatched facts clear it with no new join.
    Stage5Refresh onActivation();
    Stage5Refresh onTrackedFrame(const Stage5HostView &host = Stage5HostView{});
    Stage5Refresh onMemberFrame();
    Stage5Refresh onMotionFrame(bool tracked, const Stage5HostView &host = Stage5HostView{});
    Stage5Refresh onLayoutChanged();
    // Member visibility route (minimize/restore, hide/show): union scope
    // plus the structural tick, unconditionally, then border/underlay/
    // preview refresh; the anchor still remaps only for held members via
    // onMemberVisibility below.
    Stage5Refresh onVisibilityChanged(const QString &bareId);
    Stage5Refresh onFullscreenChanged();
    Stage5Refresh onGestureBoundary(const Stage5HostView &host = Stage5HostView{});
    // Member visibility change: no bump, re-anchor plus underlay only, and
    // only when the bare id names a held paint member (empty refresh
    // otherwise: no refresh, no transition).
    Stage5Refresh onMemberVisibility(const QString &bareId);

    struct EndpointResult {
        PresentationApply apply = PresentationApply::StaleIgnored;
        Stage5Refresh refresh;
    };
    // Accepted-presentation coverage for one SetPresentation payload with
    // the exact live open native gesture identity at the endpoint plus the
    // live border anchor facts for the same endpoint path. The caller
    // gates endpoint loss before this (display-only hide there).
    // Accepted/malformed refresh every surface (plus repaint); stale
    // coverage refreshes nothing and preserves the held display. The
    // border chain re-proves only on newly accepted exact-current coverage
    // with exact live anchor authority (clearing first); a structurally
    // accepted paint with older coverage preserves a proven chain by prior
    // event proof, so only the floored shapes hide. Without a proven
    // chain only the border hides. The anchor facts default to unknown
    // for callers without live authority.
    EndpointResult applyEndpoint(const QString &payload, const QString &liveGestureInstance,
        const QString &liveGestureSerial, const Stage5HostView &hostForBorder = Stage5HostView{});
    // Endpoint-loss hide (display-only, retaining order/invalidation
    // evidence) and stream-resetting clear. Both refresh every surface;
    // repaint is requested exactly when paint was held.
    Stage5Refresh hideForEndpointLoss();
    Stage5Refresh resetStream();

    // Pure-renderer visibility decisions: the held accepted underlay
    // shows only on the live foreground match, the geometry-fresh floor,
    // held coverage equal to the actual latest witness, plus the hide-only
    // vetoes and a valid anchor (the core Found already encodes the
    // trigger). The preview needs structurally current coverage (covered
    // at or past the structural floor in the same instance: motion bumps
    // the monotonic witness without touching the floor, so the valid
    // same-gesture preview persists while strict underlay hides) plus the
    // matching live producer, live move liveness, and the endpoint gate.
    // The held accepted border shows only on the live foreground match,
    // live eligibility (known), its anchor chain current, the complete
    // held descriptor (outsets, width, radius all present), exact live
    // tracked/active identity against subject/witness/held foreground, the
    // current live output identity and scale matching the arm-time proof,
    // and valid live geometry (live frame plus held outsets/width produce
    // a non-empty outer/inner); underlay/preview keep the existing global
    // coverage/floor gates. No eligibility or geometry derivation.
    Stage5Visibility decide(const Stage5HostView &host) const;

private:
    PresentationRouter *m_router = nullptr;
    PresentationFactsSource *m_facts = nullptr;
};

} // namespace KWin
