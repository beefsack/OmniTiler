// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
//
// Offline Stage 5 handler tests: the exact production PresentationStage5
// handlers (the same methods activewindowborder.cpp calls) driven with the
// real PresentationRouter and the real PresentationFactsSource plus
// fabricated host views and stubbed surfaces. Pins the current
// event-to-counter behavior, publication on every invalidation advance, the
// structural floor for preview continuity, and full-surface refresh
// (tracked-frame, member-frame, layout, and gesture routes refresh every
// surface so no stale item remains visible; the mover-motion route
// refreshes only its dirtied surfaces so the valid same-gesture preview
// persists).
//
// The harness provides the private bus (run the binary under
// dbus-run-session); no KWin, no live state. Payloads use the exact
// SetPresentation wire shape (same keys as presentationconsumer_test.cpp),
// so this route stays suited to the shared recorded fixture bytes.

#include "presentation_stage5.h"

#include "presentationfacts.h"

#include <QCoreApplication>
#include <QDBusConnection>
#include <QString>

#include <cstdio>
#include <cstdlib>
#include <string>

namespace
{

int failures = 0;

void check(bool condition, const char *expression, const char *file, int line)
{
    if (!condition) {
        std::fprintf(stderr, "FAIL: %s (%s:%d)\n", expression, file, line);
        ++failures;
    }
}

#define CHECK(expression) check(expression, #expression, __FILE__, __LINE__)

// Stubbed surface painting/visibility: only flagged surfaces re-evaluate,
// exactly like the production adapters. Unflagged surfaces keep their last
// applied value, which is how the test observes stale items.
struct SurfaceStub {
    bool border = false;
    bool underlay = false;
    bool preview = false;
    int repaints = 0;

    void apply(const KWin::Stage5Refresh &refresh, const KWin::Stage5Visibility &visibility)
    {
        if (refresh.border) {
            border = visibility.border;
        }
        if (refresh.underlay) {
            underlay = visibility.underlay;
        }
        if (refresh.preview) {
            preview = visibility.preview;
        }
        if (refresh.repaint) {
            ++repaints;
        }
    }
};

KWin::Stage5HostView allLiveHost()
{
    KWin::Stage5HostView host;
    host.endpointAvailable = true;
    host.foregroundMatches = true;
    host.liveEligible = true;
    host.anchorOk = true;
    host.moveLive = true;
    return host;
}

std::string foundPayload(const std::string &coveredInstance, quint64 coveredInvalid, const std::string &rev = "4",
    const std::string &obs = "9", bool withPreview = false)
{
    return "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs + "\",\"coveredInstance\":\""
        + coveredInstance + "\",\"coveredInvalid\":\"" + std::to_string(coveredInvalid)
        + "\",\"foreground\":\"win-2\","
          "\"border\":{\"visible\":1,\"inner\":[10,10,300,300],\"outer\":[8,8,304,304]},"
          "\"underlay\":{\"status\":\"found\",\"win\":\"win-2\",\"outer\":[0,0,1200,800],"
          "\"members\":[\"win-1\",\"win-2\"]},"
        + (withPreview
            ? std::string("\"preview\":{\"rect\":[0,0,396,600],\"sourceOut\":\"out-1\",\"sourceWs\":\"ws-1\",\"win\":\"win-a\","
                  "\"producerInstance\":\"gesture-9\",\"producerSerial\":\"7\"}}")
            : std::string("\"preview\":null}"));
}

std::string previewPayload(const std::string &coveredInstance, quint64 coveredInvalid, const std::string &rev = "5",
    const std::string &obs = "1")
{
    return "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs + "\",\"coveredInstance\":\""
        + coveredInstance + "\",\"coveredInvalid\":\"" + std::to_string(coveredInvalid)
        + "\",\"foreground\":\"win-2\","
          "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
          "\"underlay\":{\"status\":\"hidden\"},"
          "\"preview\":{\"rect\":[0,0,396,600],\"sourceOut\":\"out-1\",\"sourceWs\":\"ws-1\",\"win\":\"win-a\","
          "\"producerInstance\":\"gesture-9\",\"producerSerial\":\"7\"}}";
}

std::string hiddenPayload(const std::string &coveredInstance, quint64 coveredInvalid, const std::string &rev,
    const std::string &obs)
{
    return "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs + "\",\"coveredInstance\":\""
        + coveredInstance + "\",\"coveredInvalid\":\"" + std::to_string(coveredInvalid)
        + "\",\"foreground\":\"win-2\","
          "\"border\":{\"visible\":0,\"inner\":null,\"outer\":null},"
          "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}";
}

KWin::PresentationStage5::EndpointResult applyStr(KWin::PresentationStage5 &stage, const std::string &payload,
    const char *liveInstance = "", const char *liveSerial = "0")
{
    return stage.applyEndpoint(QString::fromStdString(payload), QString::fromUtf8(liveInstance),
        QString::fromUtf8(liveSerial));
}

} // namespace

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);

    KWin::PresentationRouter router;
    KWin::PresentationFactsSource facts(QDBusConnection::sessionBus(), &app);
    KWin::PresentationStage5 stage(router);
    SurfaceStub stub;

    // Seed bind: the one-time publish of the current counter under the
    // fresh instance (a no-op while the counter is still zero, exactly
    // like the retired inline body), then the router is namespaced to it.
    stage.bindFacts(&facts);
    const std::string inst = facts.instance().toStdString();
    CHECK(router.instance == facts.instance());
    CHECK(router.counter == 0);
    CHECK(facts.invalidation() == 0);
    CHECK(facts.revision() == 0);
    CHECK(stage.factsInstance() == facts.instance());

    // Accepted coverage with a fabricated all-live host: border and
    // underlay show, the unproven preview stays hidden.
    {
        const auto result = applyStr(stage, foundPayload(inst, 0));
        CHECK(result.apply == KWin::PresentationApply::Accepted);
        CHECK(result.refresh.border);
        CHECK(result.refresh.anchor);
        CHECK(result.refresh.underlay);
        CHECK(result.refresh.preview);
        CHECK(result.refresh.repaint);
        const KWin::Stage5Visibility visibility = stage.decide(allLiveHost());
        CHECK(visibility.border);
        CHECK(visibility.underlay);
        CHECK(!visibility.preview);
        stub.apply(result.refresh, visibility);
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(!stub.preview);
        CHECK(stub.repaints == 1);
    }

    // Tracked-frame route: union-scope bump (Border plus Underlay: the
    // tracked window is always group-subscribed, so the member handler
    // skips it on the same emission), published so the next actual facts
    // capture covers it, plus the structural tick. One frame advances the
    // witness exactly once, setting both floors at the same pre-bump
    // point. Every surface refreshed; the decision already hides border
    // and underlay; the stub follows.
    {
        const KWin::Stage5Refresh refresh = stage.onTrackedFrame();
        CHECK(router.counter == 1);
        CHECK(router.structuralFloor == 1);
        CHECK(refresh.border);
        CHECK(refresh.anchor);
        CHECK(refresh.underlay);
        CHECK(refresh.preview);
        CHECK(!refresh.repaint);
        CHECK(facts.invalidation() == 1);
        CHECK(facts.revision() == 1);
        CHECK(router.paint.borderFloor.set);
        CHECK(router.paint.borderFloor.counter == 0);
        CHECK(router.paint.borderFloor.instance == facts.instance());
        CHECK(router.paint.underlayFloor.set);
        CHECK(router.paint.underlayFloor.counter == 0);
        CHECK(router.paint.underlayFloor.instance == facts.instance());
        const KWin::Stage5Visibility visibility = stage.decide(allLiveHost());
        CHECK(!visibility.border);
        CHECK(!visibility.underlay);
        stub.apply(refresh, visibility);
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(stub.repaints == 1);
    }

    // Fresh covering paint recovers the ring and clears the floor.
    {
        const auto result = applyStr(stage, foundPayload(inst, 1, "4", "10"));
        CHECK(result.apply == KWin::PresentationApply::Accepted);
        CHECK(!router.paint.borderFloor.set);
        const KWin::Stage5Visibility visibility = stage.decide(allLiveHost());
        CHECK(visibility.border);
        CHECK(visibility.underlay);
        stub.apply(result.refresh, visibility);
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(stub.repaints == 2);
    }

    // Member-frame route: underlay scope bump plus the structural tick,
    // published so the next actual facts capture covers it, and every
    // surface refreshed. The decision already hides border and underlay;
    // the stub follows.
    {
        const KWin::Stage5Refresh refresh = stage.onMemberFrame();
        CHECK(router.counter == 2);
        CHECK(router.structuralFloor == 2);
        CHECK(refresh.border);
        CHECK(refresh.anchor);
        CHECK(refresh.underlay);
        CHECK(refresh.preview);
        CHECK(!refresh.repaint);
        CHECK(facts.invalidation() == 2);
        CHECK(facts.revision() == 2);
        CHECK(router.paint.underlayFloor.set);
        CHECK(router.paint.underlayFloor.counter == 1);
        const KWin::Stage5Visibility visibility = stage.decide(allLiveHost());
        CHECK(!visibility.border);
        CHECK(!visibility.underlay);
        stub.apply(refresh, visibility);
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(stub.repaints == 2);
    }

    // Member-visibility routing: members re-anchor with no bump,
    // non-members refresh nothing and bump nothing.
    {
        const KWin::Stage5Refresh member = stage.onMemberVisibility(QStringLiteral("win-1"));
        CHECK(!member.border);
        CHECK(member.anchor);
        CHECK(member.underlay);
        CHECK(!member.preview);
        CHECK(router.counter == 2);
        const KWin::Stage5Refresh foreign = stage.onMemberVisibility(QStringLiteral("zzz"));
        CHECK(!foreign.border);
        CHECK(!foreign.anchor);
        CHECK(!foreign.underlay);
        CHECK(!foreign.preview);
        CHECK(router.counter == 2);
        CHECK(facts.invalidation() == 2);
    }

    // Gesture boundary with a proven preview: the latch drops, the witness
    // advances with no floors, publishes so the next actual facts capture
    // covers it, and every surface refreshes.
    {
        const auto accept = applyStr(stage, previewPayload(inst, 2), "gesture-9", "7");
        CHECK(accept.apply == KWin::PresentationApply::Accepted);
        CHECK(router.paint.previewHas);
        KWin::Stage5HostView host = allLiveHost();
        host.liveGestureInstance = QStringLiteral("gesture-9");
        host.liveGestureSerial = QStringLiteral("7");
        CHECK(stage.decide(host).preview);
        stub.apply(accept.refresh, stage.decide(host));
        CHECK(stub.preview);
        const KWin::Stage5Refresh refresh = stage.onGestureBoundary();
        CHECK(router.counter == 3);
        CHECK(router.structuralFloor == 3);
        CHECK(refresh.border);
        CHECK(refresh.anchor);
        CHECK(refresh.underlay);
        CHECK(refresh.preview);
        CHECK(!refresh.repaint);
        CHECK(!router.paint.previewHas);
        CHECK(!router.paint.borderFloor.set);
        CHECK(!router.paint.underlayFloor.set);
        CHECK(facts.invalidation() == 3);
        CHECK(facts.revision() == 3);
        stub.apply(refresh, stage.decide(host));
        CHECK(!stub.preview);
    }

    // Activation: display-only hide plus the all-scope bump, published so the
    // next actual facts capture covers it, every surface refreshed (no
    // repaint flag), fences retained.
    {
        const KWin::Stage5Refresh refresh = stage.onActivation();
        CHECK(router.counter == 4);
        CHECK(router.structuralFloor == 4);
        CHECK(refresh.border);
        CHECK(refresh.anchor);
        CHECK(refresh.underlay);
        CHECK(refresh.preview);
        CHECK(!refresh.repaint);
        CHECK(!router.paint.has);
        CHECK(router.paint.fence.set);
        CHECK(router.paint.borderFloor.set);
        CHECK(router.paint.underlayFloor.set);
        CHECK(facts.invalidation() == 4);
        CHECK(facts.revision() == 4);
        stub.apply(refresh, stage.decide(allLiveHost()));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(!stub.preview);
        CHECK(stub.repaints == 3);
    }

    // Endpoint-loss hide: full refresh with repaint exactly when paint was
    // held; a second hide with no paint requests no repaint.
    {
        const auto accept = applyStr(stage, foundPayload(inst, 4, "6", "1"));
        CHECK(accept.apply == KWin::PresentationApply::Accepted);
        stub.apply(accept.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        const int repaintsBeforeHide = stub.repaints;
        const KWin::Stage5Refresh hide = stage.hideForEndpointLoss();
        CHECK(!router.paint.has);
        CHECK(router.paint.fence.set);
        CHECK(hide.border);
        CHECK(hide.anchor);
        CHECK(hide.underlay);
        CHECK(hide.preview);
        CHECK(hide.repaint);
        stub.apply(hide, stage.decide(allLiveHost()));
        CHECK(!stub.border);
        CHECK(stub.repaints == repaintsBeforeHide + 1);
        const KWin::Stage5Refresh hideEmpty = stage.hideForEndpointLoss();
        CHECK(!hideEmpty.repaint);
    }

    // Stream reset: display, order, and floors reset while the live source
    // truth (counter and namespace) is not forgotten; the next endpoint
    // dispatch re-seeds latest from effect authority.
    {
        const auto accept = applyStr(stage, foundPayload(inst, 4, "6", "2"));
        CHECK(accept.apply == KWin::PresentationApply::Accepted);
        const KWin::Stage5Refresh reset = stage.resetStream();
        CHECK(!router.paint.has);
        CHECK(!router.paint.fence.set);
        CHECK(!router.paint.borderFloor.set);
        CHECK(!router.paint.underlayFloor.set);
        CHECK(router.counter == 4);
        CHECK(router.structuralFloor == 4);
        CHECK(router.instance == facts.instance());
        CHECK(reset.border);
        CHECK(reset.repaint);
        stub.apply(reset, stage.decide(allLiveHost()));
        const auto reseeded = applyStr(stage, foundPayload(inst, 4, "6", "3"));
        CHECK(reseeded.apply == KWin::PresentationApply::Accepted);
    }

    // Stale coverage and stale order never touch the held display and
    // refresh nothing; malformed clears display-only with a full refresh.
    {
        const auto fresh = applyStr(stage, foundPayload(inst, 4, "7", "2"));
        CHECK(fresh.apply == KWin::PresentationApply::Accepted);
        stub.apply(fresh.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        const auto older = applyStr(stage, foundPayload(inst, 4, "6", "9"));
        CHECK(older.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!older.refresh.border);
        CHECK(!older.refresh.anchor);
        CHECK(!older.refresh.underlay);
        CHECK(!older.refresh.preview);
        CHECK(!older.refresh.repaint);
        CHECK(router.paint.has);
        CHECK(router.paint.rev == 7);
        CHECK(facts.invalidation() == 4);
        const std::string foreign = foundPayload("other-instance", 4, "8", "1");
        const auto foreignCover = applyStr(stage, foreign);
        CHECK(foreignCover.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(router.paint.has);
        const auto malformed = applyStr(stage, "{}");
        CHECK(malformed.apply == KWin::PresentationApply::Malformed);
        CHECK(malformed.refresh.border);
        CHECK(malformed.refresh.anchor);
        CHECK(malformed.refresh.underlay);
        CHECK(malformed.refresh.preview);
        CHECK(malformed.refresh.repaint);
        CHECK(!router.paint.has);
    }

    // Re-publishing the unchanged counter is a no-op in the facts source:
    // it carries the live counter with no revision bump.
    stage.publishInvalidation();
    CHECK(facts.invalidation() == router.counter);
    CHECK(facts.revision() == 4);

    KWin::Stage5HostView fullscreenHost = allLiveHost();
    fullscreenHost.liveGestureInstance = QStringLiteral("gesture-9");
    fullscreenHost.liveGestureSerial = QStringLiteral("7");
    const auto beforeEnter = applyStr(stage, previewPayload(inst, 4, "8", "9"), "gesture-9", "7");
    CHECK(beforeEnter.apply == KWin::PresentationApply::Accepted);
    stub.apply(beforeEnter.refresh, stage.decide(fullscreenHost));
    CHECK(stub.preview);

    // Fullscreen-mode transition ENTER: all-scope bump published so the
    // next actual facts capture covers it, every surface refreshed (no
    // repaint flag), and both floors set so no stale pre-enter paint
    // applies. A pre-transition covering paint is stale-ignored and
    // refreshes nothing; a covering ENTER payload with hidden shapes stays
    // hidden after it applies.
    {
        const KWin::Stage5Refresh refresh = stage.onFullscreenChanged();
        CHECK(router.counter == 5);
        CHECK(router.structuralFloor == 5);
        CHECK(refresh.border);
        CHECK(refresh.anchor);
        CHECK(refresh.underlay);
        CHECK(refresh.preview);
        CHECK(!refresh.repaint);
        CHECK(router.paint.borderFloor.set);
        CHECK(router.paint.borderFloor.counter == 4);
        CHECK(router.paint.underlayFloor.set);
        CHECK(router.paint.underlayFloor.counter == 4);
        CHECK(facts.invalidation() == 5);
        CHECK(facts.revision() == 5);
        stub.apply(refresh, stage.decide(fullscreenHost));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(!stub.preview);
        // Pre-transition coverage (equal to the dirt point) is stale:
        // display, order, and floors preserved, nothing refreshed.
        const auto stale = applyStr(stage, foundPayload(inst, 4, "9", "9"));
        CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stale.refresh.border);
        CHECK(!stale.refresh.anchor);
        CHECK(!stale.refresh.underlay);
        CHECK(!stale.refresh.preview);
        CHECK(!stale.refresh.repaint);
        CHECK(router.paint.borderFloor.set);
        CHECK(router.paint.underlayFloor.set);
        // Covering ENTER paint carries hidden shapes: it applies and clears
        // the floors, and the held display remains hidden.
        const auto covering = applyStr(stage, hiddenPayload(inst, 5, "9", "3"));
        CHECK(covering.apply == KWin::PresentationApply::Accepted);
        CHECK(!router.paint.borderFloor.set);
        CHECK(!router.paint.underlayFloor.set);
        stub.apply(covering.refresh, stage.decide(allLiveHost()));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(!stub.preview);
    }

    // Fullscreen-mode transition EXIT: the same all-scope bump in the
    // other direction. The ENTER covering paint is now stale-ignored, and
    // a covering EXIT paint with visible shapes restores the display.
    {
        const KWin::Stage5Refresh refresh = stage.onFullscreenChanged();
        CHECK(router.counter == 6);
        CHECK(router.structuralFloor == 6);
        CHECK(refresh.border);
        CHECK(refresh.anchor);
        CHECK(refresh.underlay);
        CHECK(refresh.preview);
        CHECK(!refresh.repaint);
        CHECK(router.paint.borderFloor.set);
        CHECK(router.paint.borderFloor.counter == 5);
        CHECK(router.paint.underlayFloor.set);
        CHECK(router.paint.underlayFloor.counter == 5);
        CHECK(facts.invalidation() == 6);
        CHECK(facts.revision() == 6);
        stub.apply(refresh, stage.decide(allLiveHost()));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(!stub.preview);
        // The ENTER covering paint is now stale: nothing refreshed, held
        // hidden display preserved.
        const auto stale = applyStr(stage, hiddenPayload(inst, 5, "9", "4"));
        CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stale.refresh.border);
        CHECK(!stale.refresh.anchor);
        CHECK(!stale.refresh.underlay);
        CHECK(!stale.refresh.preview);
        CHECK(!stale.refresh.repaint);
        // Covering EXIT paint carries visible shapes: it applies, clears
        // the floors, and restores the display.
        const auto covering = applyStr(stage, foundPayload(inst, 6, "9", "4"));
        CHECK(covering.apply == KWin::PresentationApply::Accepted);
        CHECK(!router.paint.borderFloor.set);
        CHECK(!router.paint.underlayFloor.set);
        stub.apply(covering.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
        const auto preview = applyStr(stage, previewPayload(inst, 6, "9", "5"), "gesture-9", "7");
        CHECK(preview.apply == KWin::PresentationApply::Accepted);
        stub.apply(preview.refresh, stage.decide(fullscreenHost));
        CHECK(stub.preview);
    }

    // Genuine mover-motion classification: the full matrix. Only the live
    // open gesture window with the interactive-move flag held is motion;
    // a mismatched or empty identity, a closed gesture, or a
    // non-interactive frame is structural. Never timing, geometry, or
    // cursor matching.
    {
        CHECK(KWin::classifyMoverMotion(QStringLiteral("w"), QStringLiteral("w"), true, true));
        CHECK(!KWin::classifyMoverMotion(QStringLiteral("w"), QStringLiteral("x"), true, true));
        CHECK(!KWin::classifyMoverMotion(QString(), QStringLiteral("w"), true, true));
        CHECK(!KWin::classifyMoverMotion(QStringLiteral("w"), QString(), true, true));
        CHECK(!KWin::classifyMoverMotion(QString(), QString(), true, true));
        CHECK(!KWin::classifyMoverMotion(QStringLiteral("w"), QStringLiteral("w"), false, true));
        CHECK(!KWin::classifyMoverMotion(QStringLiteral("w"), QStringLiteral("w"), true, false));
        CHECK(!KWin::classifyMoverMotion(QStringLiteral("w"), QStringLiteral("w"), false, false));
    }

    // Visible-to-visible preview-rect advance: an advancing rect consumes
    // verbatim (exact KWin logical values, no outset or scaling); an equal
    // rect moves nothing.
    {
        QRect held(1, 2, 3, 4);
        CHECK(KWin::advanceDragPreviewRect(held, QRect(5, 6, 7, 8)));
        CHECK(held == QRect(5, 6, 7, 8));
        CHECK(!KWin::advanceDragPreviewRect(held, QRect(5, 6, 7, 8)));
        CHECK(held == QRect(5, 6, 7, 8));
        QRect zero;
        CHECK(KWin::advanceDragPreviewRect(zero, QRect(0, 0, 396, 600)));
        CHECK(zero == QRect(0, 0, 396, 600));
    }

    // Layout add/remove: independent target/layout change with no frame
    // signal advances the causal witness across all scopes plus the
    // structural tick, publishes, refreshes every surface, and hides the
    // held display until the covering paint lands.
    {
        const auto accept = applyStr(stage, foundPayload(inst, 6, "10", "1"));
        CHECK(accept.apply == KWin::PresentationApply::Accepted);
        stub.apply(accept.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
        const KWin::Stage5Refresh refresh = stage.onLayoutChanged();
        CHECK(router.counter == 7);
        CHECK(router.structuralFloor == 7);
        CHECK(refresh.border);
        CHECK(refresh.anchor);
        CHECK(refresh.underlay);
        CHECK(refresh.preview);
        CHECK(!refresh.repaint);
        CHECK(facts.invalidation() == 7);
        CHECK(facts.revision() == 7);
        CHECK(router.paint.borderFloor.set);
        CHECK(router.paint.borderFloor.counter == 6);
        CHECK(router.paint.underlayFloor.set);
        CHECK(router.paint.underlayFloor.counter == 6);
        stub.apply(refresh, stage.decide(allLiveHost()));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        const auto covering = applyStr(stage, foundPayload(inst, 7, "10", "2"));
        CHECK(covering.apply == KWin::PresentationApply::Accepted);
        stub.apply(covering.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
    }

    // Active-drag mover motion with all three shapes visible: exactly one
    // monotonic bump with the union scope (both floors set at the same
    // pre-bump point), published, WITHOUT the structural tick. Strict
    // border/underlay hide while the structurally covered preview persists.
    // A newer-order paint captured pre-motion is accepted on the structural
    // path yet keeps both visible strict shapes hidden (its cover predates
    // the bump); only the exact post-motion cover restores them. A later
    // structural frame hides everything immediately and pre-event paint
    // cannot repaint it.
    {
        KWin::Stage5HostView host = allLiveHost();
        host.liveGestureInstance = QStringLiteral("gesture-9");
        host.liveGestureSerial = QStringLiteral("7");
        const auto accept = applyStr(stage, foundPayload(inst, 7, "11", "1", true), "gesture-9", "7");
        CHECK(accept.apply == KWin::PresentationApply::Accepted);
        CHECK(router.paint.previewHas);
        CHECK(router.paint.borderVisible);
        CHECK(router.paint.underlayFound);
        stub.apply(accept.refresh, stage.decide(host));
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(stub.preview);
        const KWin::Stage5Refresh motion = stage.onMotionFrame(true);
        CHECK(router.counter == 8);
        CHECK(router.structuralFloor == 7);
        CHECK(facts.invalidation() == 8);
        CHECK(facts.revision() == 8);
        CHECK(motion.border);
        CHECK(!motion.anchor);
        CHECK(motion.underlay);
        CHECK(!motion.preview);
        CHECK(!motion.repaint);
        CHECK(router.paint.borderFloor.set);
        CHECK(router.paint.borderFloor.counter == 7);
        CHECK(router.paint.underlayFloor.set);
        CHECK(router.paint.underlayFloor.counter == 7);
        CHECK(router.paint.previewHas);
        stub.apply(motion, stage.decide(host));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(stub.preview);
        // Newer-order pre-motion paint with visible strict shapes: accepted
        // on the structural path (cover reaches the untouched floor) yet
        // both shapes stay hidden (cover predates the bump) while the
        // same-gesture preview remains. The held paint still carries the
        // visible shapes; only the gates hide them.
        const auto older = applyStr(stage, foundPayload(inst, 7, "11", "2", true), "gesture-9", "7");
        CHECK(older.apply == KWin::PresentationApply::Accepted);
        CHECK(router.paint.borderVisible);
        CHECK(router.paint.underlayFound);
        CHECK(router.paint.previewHas);
        stub.apply(older.refresh, stage.decide(host));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(stub.preview);
        // Post-motion exact cover restores both strict shapes.
        const auto covering = applyStr(stage, foundPayload(inst, 8, "11", "3", true), "gesture-9", "7");
        CHECK(covering.apply == KWin::PresentationApply::Accepted);
        CHECK(!router.paint.borderFloor.set);
        CHECK(!router.paint.underlayFloor.set);
        stub.apply(covering.refresh, stage.decide(host));
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(stub.preview);
        // Structural frame: the floor moves past pre-event coverage, so all
        // shapes hide immediately and pre-event paint (even with visible
        // strict shapes and a live preview) cannot repaint them.
        const KWin::Stage5Refresh structural = stage.onTrackedFrame();
        CHECK(router.counter == 9);
        CHECK(router.structuralFloor == 9);
        CHECK(facts.invalidation() == 9);
        stub.apply(structural, stage.decide(host));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(!stub.preview);
        const auto stale = applyStr(stage, foundPayload(inst, 8, "11", "4", true), "gesture-9", "7");
        CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stale.refresh.border);
        CHECK(!stale.refresh.anchor);
        CHECK(!stale.refresh.underlay);
        CHECK(!stale.refresh.preview);
        CHECK(!stale.refresh.repaint);
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(!stub.preview);
    }

    // Member motion: underlay scope only, still exactly one bump, still no
    // structural tick. A non-interactive frame on the same window would
    // take the structural route above (classified by the caller).
    {
        const KWin::Stage5Refresh motion = stage.onMotionFrame(false);
        CHECK(router.counter == 10);
        CHECK(router.structuralFloor == 9);
        CHECK(facts.invalidation() == 10);
        CHECK(facts.revision() == 10);
        CHECK(!motion.border);
        CHECK(!motion.anchor);
        CHECK(motion.underlay);
        CHECK(!motion.preview);
        CHECK(!motion.repaint);
        CHECK(router.paint.borderFloor.set);
        CHECK(router.paint.borderFloor.counter == 8);
        CHECK(router.paint.underlayFloor.set);
        CHECK(router.paint.underlayFloor.counter == 9);
    }

    // Member hide/minimize: the union scope plus the structural tick hide
    // held shapes immediately (a drop target may have vanished with no
    // frame signal), published; the anchor still remaps only for held
    // members via the extracted no-bump route, and a non-member visibility
    // change bumps the witness without re-anchoring. Pre-event coverage
    // cannot repaint hidden shapes.
    {
        const auto accept = applyStr(stage, foundPayload(inst, 10, "12", "1"));
        CHECK(accept.apply == KWin::PresentationApply::Accepted);
        stub.apply(accept.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
        const KWin::Stage5Refresh refresh = stage.onVisibilityChanged(QStringLiteral("win-1"));
        CHECK(router.counter == 11);
        CHECK(router.structuralFloor == 11);
        CHECK(refresh.border);
        CHECK(refresh.anchor);
        CHECK(refresh.underlay);
        CHECK(refresh.preview);
        CHECK(!refresh.repaint);
        CHECK(facts.invalidation() == 11);
        CHECK(facts.revision() == 11);
        CHECK(router.paint.borderFloor.set);
        CHECK(router.paint.underlayFloor.set);
        stub.apply(refresh, stage.decide(allLiveHost()));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        const auto covering = applyStr(stage, foundPayload(inst, 11, "12", "2"));
        CHECK(covering.apply == KWin::PresentationApply::Accepted);
        CHECK(!router.paint.borderFloor.set);
        CHECK(!router.paint.underlayFloor.set);
        stub.apply(covering.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
        // A non-member visibility change still bumps (no membership gate
        // on the witness) without re-anchoring.
        const KWin::Stage5Refresh foreign = stage.onVisibilityChanged(QStringLiteral("zzz"));
        CHECK(router.counter == 12);
        CHECK(router.structuralFloor == 12);
        CHECK(foreign.border);
        CHECK(!foreign.anchor);
        CHECK(foreign.underlay);
        CHECK(foreign.preview);
        CHECK(facts.invalidation() == 12);
    }

    // Visibility with a proven same-gesture preview: the preview hides
    // immediately with the tick and pre-event paint cannot repaint it;
    // the covering paint restores it.
    {
        KWin::Stage5HostView host = allLiveHost();
        host.liveGestureInstance = QStringLiteral("gesture-9");
        host.liveGestureSerial = QStringLiteral("7");
        const auto accept = applyStr(stage, previewPayload(inst, 12, "13", "1"), "gesture-9", "7");
        CHECK(accept.apply == KWin::PresentationApply::Accepted);
        stub.apply(accept.refresh, stage.decide(host));
        CHECK(stub.preview);
        const KWin::Stage5Refresh refresh = stage.onVisibilityChanged(QStringLiteral("win-1"));
        CHECK(router.counter == 13);
        CHECK(router.structuralFloor == 13);
        stub.apply(refresh, stage.decide(host));
        CHECK(!stub.preview);
        const auto stale = applyStr(stage, previewPayload(inst, 12, "13", "2"), "gesture-9", "7");
        CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stub.preview);
        const auto covering = applyStr(stage, previewPayload(inst, 13, "13", "3"), "gesture-9", "7");
        CHECK(covering.apply == KWin::PresentationApply::Accepted);
        stub.apply(covering.refresh, stage.decide(host));
        CHECK(stub.preview);
    }

    if (failures != 0) {
        std::fprintf(stderr, "stage5 handler test: %d failure(s)\n", failures);
        return 1;
    }
    std::printf("stage5 handler test: all checks passed\n");
    return 0;
}
