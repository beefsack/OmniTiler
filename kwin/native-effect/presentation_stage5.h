// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
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
// precomputed from live reads.
struct Stage5HostView {
    bool endpointAvailable = false;
    bool foregroundMatches = false;
    bool liveEligible = false;
    bool anchorOk = false;
    bool moveLive = false;
    QString liveGestureInstance;
    QString liveGestureSerial = QStringLiteral("0");
};

// Decided visibility per surface, from the held accepted paint plus one
// injected host view. Drawn borders/underlay additionally require held
// coverage equal to the actual latest witness; the drawn preview requires
// coverage at or past the structural floor instead (same instance).
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
    // the preview is never refreshed there (it stays visible).
    Stage5Refresh onActivation();
    Stage5Refresh onTrackedFrame();
    Stage5Refresh onMemberFrame();
    Stage5Refresh onMotionFrame(bool tracked);
    Stage5Refresh onLayoutChanged();
    // Member visibility route (minimize/restore, hide/show): union scope
    // plus the structural tick, unconditionally, then border/underlay/
    // preview refresh; the anchor still remaps only for held members via
    // onMemberVisibility below.
    Stage5Refresh onVisibilityChanged(const QString &bareId);
    Stage5Refresh onFullscreenChanged();
    Stage5Refresh onGestureBoundary();
    // Member visibility change: no bump, re-anchor plus underlay only, and
    // only when the bare id names a held paint member (empty refresh
    // otherwise: no refresh, no transition).
    Stage5Refresh onMemberVisibility(const QString &bareId);

    struct EndpointResult {
        PresentationApply apply = PresentationApply::StaleIgnored;
        Stage5Refresh refresh;
    };
    // Accepted-presentation coverage for one SetPresentation payload with
    // the exact live open native gesture identity at the endpoint. The
    // caller gates endpoint loss before this (display-only hide there).
    // Accepted/malformed refresh every surface (plus repaint); stale
    // coverage refreshes nothing and preserves the held display.
    EndpointResult applyEndpoint(const QString &payload, const QString &liveGestureInstance,
        const QString &liveGestureSerial);
    // Endpoint-loss hide (display-only, retaining order/invalidation
    // evidence) and stream-resetting clear. Both refresh every surface;
    // repaint is requested exactly when paint was held.
    Stage5Refresh hideForEndpointLoss();
    Stage5Refresh resetStream();

    // Pure-renderer visibility decisions: the held accepted border and
    // underlay shapes show only on the live foreground match, the
    // geometry-fresh floors, held coverage equal to the actual latest
    // witness, plus the hide-only vetoes. The underlay needs the same plus
    // a valid anchor (the core Found already encodes the trigger). The
    // preview needs structurally current coverage (covered at or past the
    // structural floor in the same instance: motion bumps the monotonic
    // witness without touching the floor, so the valid same-gesture
    // preview persists while strict borders hide) plus the matching live
    // producer, live move liveness, and the endpoint gate. No eligibility
    // or geometry derivation.
    Stage5Visibility decide(const Stage5HostView &host) const;

private:
    PresentationRouter *m_router = nullptr;
    PresentationFactsSource *m_facts = nullptr;
};

} // namespace KWin
