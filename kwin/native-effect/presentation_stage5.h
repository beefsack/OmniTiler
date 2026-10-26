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

// Per-route refresh set: which host surfaces the caller refreshes after the
// route runs. Every invalidation route refreshes all surfaces so stale items
// hide immediately until fresh covering paint arrives. Activation, endpoint,
// and reset routes refresh every surface; stale endpoint coverage refreshes
// nothing.
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
// injected host view. Every drawn surface additionally requires held
// coverage equal to the actual latest witness.
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
    // refreshes. Activation hides display-only then bumps every scope.
    // Tracked-frame, member-frame, fullscreen-mode, and gesture boundaries
    // bump their scope then refresh every surface (border, anchor,
    // underlay, preview) so no stale item remains visible.
    Stage5Refresh onActivation();
    Stage5Refresh onTrackedFrame();
    Stage5Refresh onMemberFrame();
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

    // Pure-renderer visibility decisions: the held accepted shapes show
    // only on the live foreground match, the geometry-fresh floors, held
    // coverage equal to the actual latest witness, the matching live
    // producer for the preview, plus the endpoint gate. No eligibility or
    // geometry derivation.
    Stage5Visibility decide(const Stage5HostView &host) const;

private:
    PresentationRouter *m_router = nullptr;
    PresentationFactsSource *m_facts = nullptr;
};

} // namespace KWin
