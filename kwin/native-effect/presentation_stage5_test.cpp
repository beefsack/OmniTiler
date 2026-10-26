// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
//
// Offline Stage 5 handler tests: the exact production PresentationStage5
// handlers (the same methods activewindowborder.cpp calls) driven with the
// real PresentationRouter and the real PresentationFactsSource plus
// fabricated host views and stubbed surfaces. Pins the current
// event-to-counter behavior, publication on every invalidation advance, and
// full-surface refresh (tracked-frame, member-frame, and gesture routes
// refresh every surface so no stale item remains visible).
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
    const std::string &obs = "9")
{
    return "{\"v\":1,\"gen\":\"gen-1\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs + "\",\"coveredInstance\":\""
        + coveredInstance + "\",\"coveredInvalid\":\"" + std::to_string(coveredInvalid)
        + "\",\"foreground\":\"win-2\","
          "\"border\":{\"visible\":1,\"inner\":[10,10,300,300],\"outer\":[8,8,304,304]},"
          "\"underlay\":{\"status\":\"found\",\"win\":\"win-2\",\"outer\":[0,0,1200,800],"
          "\"members\":[\"win-1\",\"win-2\"]},\"preview\":null}";
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

    // Tracked-frame route: border scope bump, published so the next actual
    // facts capture covers it, and every surface refreshed. The decision
    // already hides border and underlay; the stub follows.
    {
        const KWin::Stage5Refresh refresh = stage.onTrackedFrame();
        CHECK(router.counter == 1);
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

    // Member-frame route: underlay scope bump, published so the next actual
    // facts capture covers it, and every surface refreshed. The decision
    // already hides border and underlay; the stub follows.
    {
        const KWin::Stage5Refresh refresh = stage.onMemberFrame();
        CHECK(router.counter == 2);
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

    if (failures != 0) {
        std::fprintf(stderr, "stage5 handler test: %d failure(s)\n", failures);
        return 1;
    }
    std::printf("stage5 handler test: all checks passed\n");
    return 0;
}
