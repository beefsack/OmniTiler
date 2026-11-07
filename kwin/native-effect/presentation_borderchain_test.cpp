//
// BORDER-ONLY anchor-chain regression (Qt-only, no fixtures, no bus): the
// exact production PresentationStage5/PresentationRouter seam driven with
// a hand-authored strict SetPresentation v2 payload. Pins arm-on-accept
// with exact current coverage plus live anchor authority, advance across
// a delivered same-anchor frame by event proof (the pre-clear defect:
// clearing the anchor before the bump made every continuation fail),
// no advance across an event gap, and clears on output/scale change,
// unknown eligibility, plus the missing-radius narrow hide. Never mutates
// paint.covered; the chain alone moves. Also pins the older-coverage
// structural retention: arm at cover7, one proven motion to 8, then an
// order-newer cover7 paint with the same anchor keeps the held descriptor
// (original witness frame/obs) and the prior chain, while a changed
// anchor input clears with no new join. Also pins the gesture-boundary
// preservation: the genuine boundary route with same-anchor live facts
// keeps the chain while dropping the preview latch, with the bump and
// floors continuing; unknown/foreign/changed-output/ineligible/gapped
// boundaries clear.
//

#include "presentation_stage5.h"

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

std::string borderPayload(const std::string &coveredInstance, quint64 coveredInvalid, const std::string &rev,
    const std::string &obs, bool withRadius)
{
    return "{\"v\":2,\"gen\":\"gen-1\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs
        + "\",\"coveredInstance\":\"" + coveredInstance + "\",\"coveredInvalid\":\""
        + std::to_string(coveredInvalid) + "\",\"foreground\":\"win-1\","
          "\"border\":{\"visible\":true,\"reason\":\"\",\"subject\":\"win-1\","
          "\"outsets\":[3,3,3,3],\"width\":3,\"radius\":"
        + (withRadius ? std::string("6") : std::string("null"))
        + ",\"witness\":{\"win\":\"win-1\",\"obs\":\"7\",\"frame\":[404,8,388,584],\"foreground\":\"win-1\"}},"
          "\"underlay\":{\"status\":\"hidden\"},\"preview\":null}";
}

std::string replaceOnce(std::string base, const std::string &from, const std::string &to)
{
    const size_t pos = base.find(from);
    if (pos != std::string::npos) {
        base.replace(pos, from.size(), to);
    }
    return base;
}

std::string chainStructuralPayload(const std::string &rev, const std::string &obs, int frameX, bool withPreview,
    const std::string &covered = "7", const std::string &witnessObs = "7", const std::string &gen = "gen-1")
{
    // Older-coverage structural replacement payload: same window/output
    // IDs as anchorHost, but a configurable witness frame x, a found
    // underlay, and an optionally proven preview. Lets the replacement
    // regressions below prove the held descriptor is RETAINED on the
    // structural path while the prior chain survives.
    return "{\"v\":2,\"gen\":\"" + gen + "\",\"rev\":\"" + rev + "\",\"obs\":\"" + obs
        + "\",\"coveredInstance\":\"inst-1\",\"coveredInvalid\":\"" + covered + "\",\"foreground\":\"win-1\","
          "\"border\":{\"visible\":true,\"reason\":\"\",\"subject\":\"win-1\","
          "\"outsets\":[3,3,3,3],\"width\":3,\"radius\":6,"
          "\"witness\":{\"win\":\"win-1\",\"obs\":\"" + witnessObs + "\",\"frame\":[" + std::to_string(frameX)
        + ",8,388,584],\"foreground\":\"win-1\"}},"
          "\"underlay\":{\"status\":\"found\",\"win\":\"win-1\",\"outer\":[0,0,1200,800],"
          "\"members\":[\"win-1\"]},"
        + (withPreview
            ? std::string("\"preview\":{\"rect\":[0,0,396,600],\"sourceOut\":\"out-1\",\"sourceWs\":\"ws-1\",\"win\":\"win-a\","
                  "\"producerInstance\":\"g-1\",\"producerSerial\":\"7\"}}")
            : std::string("\"preview\":null}"));
}

KWin::Stage5HostView anchorHost()
{
    KWin::Stage5HostView host;
    host.endpointAvailable = true;
    host.foregroundMatches = true;
    host.liveEligible = true;
    host.liveEligibleKnown = true;
    host.trackedId = QStringLiteral("win-1");
    host.liveActiveId = QStringLiteral("win-1");
    host.outputId = QStringLiteral("out-1");
    host.trackedFrame = QRect(404, 8, 388, 584);
    host.trackedFrameKnown = true;
    host.outputScale = 1.0;
    host.outputScaleKnown = true;
    return host;
}

} // namespace

int main()
{
    KWin::PresentationRouter router;
    KWin::PresentationStage5 stage(router, nullptr);
    const KWin::Stage5HostView host = anchorHost();

    router.setSourceInstance(QStringLiteral("inst-1"));
    stage.onLayoutChanged();
    CHECK(router.counter == 1);

    // Arm on a newly accepted endpoint with exact current coverage.
    KWin::PresentationStage5::EndpointResult accepted =
        stage.applyEndpoint(QString::fromStdString(borderPayload("inst-1", 1, "4", "9", true)),
            QStringLiteral("g-1"), QStringLiteral("7"), host);
    CHECK(accepted.apply == KWin::PresentationApply::Accepted);
    CHECK(router.borderChainCurrent());
    CHECK(stage.decide(host).border);
    CHECK(router.paint.covered.set && router.paint.covered.counter == 1);

    // Continue across a delivered same-anchor frame: exactly one bump,
    // chain follows, held coverage never mutates.
    stage.onTrackedFrame(host);
    CHECK(router.counter == 2);
    CHECK(router.borderChainCurrent());
    CHECK(router.borderChain.counter == 2);
    CHECK(router.paint.covered.set && router.paint.covered.counter == 1);
    CHECK(stage.decide(host).border);

    // Continue across genuine mover motion without the structural tick.
    stage.onMotionFrame(true, host);
    CHECK(router.counter == 3);
    CHECK(router.borderChainCurrent());
    CHECK(stage.decide(host).border);

    // An event gap (unknown facts) clears; a later anchored event must not
    // re-advance from the cleared chain.
    stage.onTrackedFrame();
    CHECK(router.counter == 4);
    CHECK(!router.borderChainCurrent());
    CHECK(!stage.decide(host).border);
    stage.onTrackedFrame(host);
    CHECK(router.counter == 5);
    CHECK(!router.borderChainCurrent());
    CHECK(!stage.decide(host).border);

    // Re-arm, then prove an output/scale change clears structurally.
    KWin::PresentationStage5::EndpointResult rearmed =
        stage.applyEndpoint(QString::fromStdString(borderPayload("inst-1", 5, "5", "1", true)),
            QStringLiteral("g-1"), QStringLiteral("7"), host);
    CHECK(rearmed.apply == KWin::PresentationApply::Accepted);
    CHECK(stage.decide(host).border);
    stage.onLayoutChanged();
    CHECK(router.counter == 6);
    CHECK(!router.borderChainCurrent());
    CHECK(!stage.decide(host).border);

    // Re-arm, then prove unknown eligibility clears on the next event.
    KWin::PresentationStage5::EndpointResult eligible =
        stage.applyEndpoint(QString::fromStdString(borderPayload("inst-1", 6, "6", "1", true)),
            QStringLiteral("g-1"), QStringLiteral("7"), host);
    CHECK(eligible.apply == KWin::PresentationApply::Accepted);
    CHECK(stage.decide(host).border);
    KWin::Stage5HostView unknownEligible = host;
    unknownEligible.liveEligibleKnown = false;
    stage.onTrackedFrame(unknownEligible);
    CHECK(router.counter == 7);
    CHECK(!stage.decide(host).border);

    // Re-arm, then prove known-ineligible authority (the maximize-hide
    // analog) refuses the arm while the paint stays accepted: only the
    // border hides, and no stale chain rearms when eligibility returns
    // without a fresh covering paint.
    KWin::PresentationStage5::EndpointResult rearmedEligible =
        stage.applyEndpoint(QString::fromStdString(borderPayload("inst-1", 7, "8", "1", true)),
            QStringLiteral("g-1"), QStringLiteral("7"), host);
    CHECK(rearmedEligible.apply == KWin::PresentationApply::Accepted);
    CHECK(stage.decide(host).border);
    KWin::Stage5HostView ineligible = host;
    ineligible.liveEligible = false;
    KWin::PresentationStage5::EndpointResult ineligiblePaint =
        stage.applyEndpoint(QString::fromStdString(borderPayload("inst-1", 7, "8", "2", true)),
            QStringLiteral("g-1"), QStringLiteral("7"), ineligible);
    CHECK(ineligiblePaint.apply == KWin::PresentationApply::Accepted);
    CHECK(!router.borderChainCurrent());
    CHECK(!stage.decide(ineligible).border);
    CHECK(!stage.decide(host).border);

    // A missing required descriptor part stays a valid shape (accepted)
    // yet narrowly hides only the border.
    KWin::PresentationStage5::EndpointResult noRadius =
        stage.applyEndpoint(QString::fromStdString(borderPayload("inst-1", 7, "8", "3", false)),
            QStringLiteral("g-1"), QStringLiteral("7"), host);
    CHECK(noRadius.apply == KWin::PresentationApply::Accepted);
    CHECK(!stage.decide(host).border);

    // Structural-path descriptor REPLACEMENT control (passes): arm at
    // cover7, one proven motion to 8, then accept an order-newer cover7
    // paint with the IDENTICAL descriptor. The structural path accepts
    // (older coverage preserved, never resurrected), the replacement is
    // the already-proven descriptor, the prior chain still authorizes it,
    // and the floored underlay stays hidden while the same-gesture
    // preview persists.
    {
        KWin::PresentationRouter router2;
        KWin::PresentationStage5 stage2(router2, nullptr);
        KWin::Stage5HostView host2 = anchorHost();
        host2.anchorOk = true;
        host2.liveGestureInstance = QStringLiteral("g-1");
        host2.liveGestureSerial = QStringLiteral("7");
        host2.moveLive = true;
        router2.setSourceInstance(QStringLiteral("inst-1"));
        for (int i = 0; i < 7; ++i) {
            stage2.onLayoutChanged();
        }
        CHECK(router2.counter == 7);
        CHECK(router2.structuralFloor == 7);
        KWin::PresentationStage5::EndpointResult first = stage2.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "9", 404, true)),
            QStringLiteral("g-1"), QStringLiteral("7"), host2);
        CHECK(first.apply == KWin::PresentationApply::Accepted);
        CHECK(router2.borderChainCurrent());
        CHECK(stage2.decide(host2).border);
        CHECK(stage2.decide(host2).underlay);
        CHECK(stage2.decide(host2).preview);
        // One proven motion: the chain follows to 8 by event proof.
        stage2.onMotionFrame(true, host2);
        CHECK(router2.counter == 8);
        CHECK(router2.structuralFloor == 7);
        CHECK(router2.borderChainCurrent());
        CHECK(router2.borderChain.counter == 8);
        CHECK(stage2.decide(host2).border);
        CHECK(!stage2.decide(host2).underlay);
        CHECK(stage2.decide(host2).preview);
        // Identical-descriptor structural replacement: accepted, chain
        // preserved, border persists, underlay fence and preview unchanged.
        KWin::PresentationStage5::EndpointResult same = stage2.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "10", 404, true)),
            QStringLiteral("g-1"), QStringLiteral("7"), host2);
        CHECK(same.apply == KWin::PresentationApply::Accepted);
        CHECK(router2.paint.covered.set && router2.paint.covered.counter == 7);
        CHECK(router2.paint.witnessFrame == QRect(404, 8, 388, 584));
        CHECK(router2.borderChainCurrent());
        CHECK(router2.borderChain.counter == 8);
        CHECK(stage2.decide(host2).border);
        CHECK(!stage2.decide(host2).underlay);
        CHECK(stage2.decide(host2).preview);
    }

    // Older-coverage structural paint is never a new join: same
    // arm-then-motion sequence, but the order-newer cover7 paint carries
    // the same anchor IDs with a CHANGED witness frame. The structural
    // path accepts, keeps the held descriptor (original arming witness
    // frame/obs), and keeps the chain, so the border stays visible while
    // underlay/preview/order/coverage adopt the new paint.
    {
        KWin::PresentationRouter router3;
        KWin::PresentationStage5 stage3(router3, nullptr);
        KWin::Stage5HostView host3 = anchorHost();
        host3.anchorOk = true;
        host3.liveGestureInstance = QStringLiteral("g-1");
        host3.liveGestureSerial = QStringLiteral("7");
        host3.moveLive = true;
        router3.setSourceInstance(QStringLiteral("inst-1"));
        for (int i = 0; i < 7; ++i) {
            stage3.onLayoutChanged();
        }
        KWin::PresentationStage5::EndpointResult first = stage3.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "9", 404, true)),
            QStringLiteral("g-1"), QStringLiteral("7"), host3);
        CHECK(first.apply == KWin::PresentationApply::Accepted);
        CHECK(router3.borderChainCurrent());
        stage3.onMotionFrame(true, host3);
        CHECK(router3.counter == 8);
        CHECK(router3.borderChainCurrent());
        CHECK(router3.borderChain.counter == 8);
        CHECK(stage3.decide(host3).border);
        // Order-newer, older-coverage, changed-frame replacement: Accepted
        // on the structural path, held descriptor retained, chain kept.
        KWin::PresentationStage5::EndpointResult changed = stage3.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "10", 500, true)),
            QStringLiteral("g-1"), QStringLiteral("7"), host3);
        CHECK(changed.apply == KWin::PresentationApply::Accepted);
        CHECK(router3.paint.witnessFrame == QRect(404, 8, 388, 584));
        CHECK(router3.paint.witnessObs == QStringLiteral("7"));
        CHECK(router3.borderChainCurrent());
        CHECK(router3.borderChain.counter == 8);
        CHECK(stage3.decide(host3).border);
        CHECK(!stage3.decide(host3).underlay);
        CHECK(stage3.decide(host3).preview);
        // Underlay/preview/order/coverage adopt the new paint even while
        // the border stays held.
        CHECK(router3.paint.covered.set && router3.paint.covered.counter == 7);
        CHECK(router3.paint.fence.set && router3.paint.fence.rev == 4 && router3.paint.fence.obs == 10);
        CHECK(router3.paint.previewHas);
        CHECK(router3.paint.underlayFound);
    }

    // Witness obs-only, frame-only, and combined changes all retain the
    // held descriptor; identical redelivery does too.
    {
        auto retainedAfter = [](const std::string &payload) {
            KWin::PresentationRouter router;
            KWin::PresentationStage5 stage(router, nullptr);
            KWin::Stage5HostView host = anchorHost();
            host.anchorOk = true;
            host.liveGestureInstance = QStringLiteral("g-1");
            host.liveGestureSerial = QStringLiteral("7");
            host.moveLive = true;
            router.setSourceInstance(QStringLiteral("inst-1"));
            for (int i = 0; i < 7; ++i) {
                stage.onLayoutChanged();
            }
            KWin::PresentationStage5::EndpointResult first = stage.applyEndpoint(
                QString::fromStdString(chainStructuralPayload("4", "9", 404, true)),
                QStringLiteral("g-1"), QStringLiteral("7"), host);
            if (first.apply != KWin::PresentationApply::Accepted || !router.borderChainCurrent()) {
                return false;
            }
            stage.onMotionFrame(true, host);
            if (!router.borderChainCurrent() || router.borderChain.counter != 8) {
                return false;
            }
            KWin::PresentationStage5::EndpointResult next = stage.applyEndpoint(
                QString::fromStdString(payload), QStringLiteral("g-1"), QStringLiteral("7"), host);
            if (next.apply != KWin::PresentationApply::Accepted) {
                return false;
            }
            return router.borderChainCurrent() && router.paint.witnessFrame == QRect(404, 8, 388, 584)
                && router.paint.witnessObs == QStringLiteral("7") && stage.decide(host).border;
        };
        CHECK(retainedAfter(chainStructuralPayload("4", "10", 404, true, "7", "8")));
        CHECK(retainedAfter(chainStructuralPayload("4", "10", 500, true, "7", "7")));
        CHECK(retainedAfter(chainStructuralPayload("4", "10", 500, true, "7", "9")));
        CHECK(retainedAfter(chainStructuralPayload("4", "10", 404, true, "7", "7")));
    }

    // Any anchor input change, hidden descriptor, or missing required
    // part clears with no new join.
    {
        auto clearsAfter = [](const std::string &payload) {
            KWin::PresentationRouter router;
            KWin::PresentationStage5 stage(router, nullptr);
            KWin::Stage5HostView host = anchorHost();
            host.anchorOk = true;
            host.liveGestureInstance = QStringLiteral("g-1");
            host.liveGestureSerial = QStringLiteral("7");
            host.moveLive = true;
            router.setSourceInstance(QStringLiteral("inst-1"));
            for (int i = 0; i < 7; ++i) {
                stage.onLayoutChanged();
            }
            KWin::PresentationStage5::EndpointResult first = stage.applyEndpoint(
                QString::fromStdString(chainStructuralPayload("4", "9", 404, true)),
                QStringLiteral("g-1"), QStringLiteral("7"), host);
            if (first.apply != KWin::PresentationApply::Accepted || !router.borderChainCurrent()) {
                return false;
            }
            stage.onMotionFrame(true, host);
            if (!router.borderChainCurrent()) {
                return false;
            }
            KWin::PresentationStage5::EndpointResult next = stage.applyEndpoint(
                QString::fromStdString(payload), QStringLiteral("g-1"), QStringLiteral("7"), host);
            if (next.apply != KWin::PresentationApply::Accepted) {
                return false;
            }
            return !router.borderChainCurrent() && !stage.decide(host).border;
        };
        const std::string base = chainStructuralPayload("4", "10", 404, true);
        CHECK(clearsAfter(replaceOnce(base, "\"subject\":\"win-1\"", "\"subject\":\"win-2\"")));
        CHECK(clearsAfter(replaceOnce(base, "\"width\":3", "\"width\":4")));
        CHECK(clearsAfter(replaceOnce(base, "\"radius\":6", "\"radius\":7")));
        CHECK(clearsAfter(replaceOnce(base, "\"outsets\":[3,3,3,3]", "\"outsets\":[4,3,3,3]")));
        CHECK(clearsAfter(replaceOnce(base, "\"witness\":{\"win\":\"win-1\"", "\"witness\":{\"win\":\"win-2\"")));
        CHECK(clearsAfter(replaceOnce(base, "\"frame\":[404,8,388,584],\"foreground\":\"win-1\"",
            "\"frame\":[404,8,388,584],\"foreground\":\"win-2\"")));
        CHECK(clearsAfter(replaceOnce(base, "\"foreground\":\"win-1\",\"border\"",
            "\"foreground\":\"win-9\",\"border\"")));
        CHECK(clearsAfter(replaceOnce(base, "\"visible\":true", "\"visible\":false")));
        CHECK(clearsAfter(replaceOnce(base, "\"outsets\":[3,3,3,3]", "\"outsets\":null")));
        CHECK(clearsAfter(replaceOnce(base, "\"width\":3", "\"width\":null")));
        CHECK(clearsAfter(replaceOnce(base, "\"radius\":6", "\"radius\":null")));
        CHECK(clearsAfter(replaceOnce(base,
            "\"witness\":{\"win\":\"win-1\",\"obs\":\"7\",\"frame\":[404,8,388,584],\"foreground\":\"win-1\"}",
            "\"witness\":null")));
    }

    // No current chain must not borrow: arm fails on ineligible authority
    // (no chain), motion keeps it cleared, then a structurally accepted
    // same-anchor paint must not create a chain.
    {
        KWin::PresentationRouter router;
        KWin::PresentationStage5 stage(router, nullptr);
        KWin::Stage5HostView host = anchorHost();
        host.anchorOk = true;
        host.liveGestureInstance = QStringLiteral("g-1");
        host.liveGestureSerial = QStringLiteral("7");
        host.moveLive = true;
        KWin::Stage5HostView ineligible = host;
        ineligible.liveEligible = false;
        router.setSourceInstance(QStringLiteral("inst-1"));
        for (int i = 0; i < 7; ++i) {
            stage.onLayoutChanged();
        }
        KWin::PresentationStage5::EndpointResult first = stage.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "9", 404, true)), QStringLiteral("g-1"),
            QStringLiteral("7"), ineligible);
        CHECK(first.apply == KWin::PresentationApply::Accepted);
        CHECK(!router.borderChainCurrent());
        stage.onMotionFrame(true, host);
        CHECK(!router.borderChainCurrent());
        KWin::PresentationStage5::EndpointResult noborrow = stage.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "10", 404, true)), QStringLiteral("g-1"),
            QStringLiteral("7"), host);
        CHECK(noborrow.apply == KWin::PresentationApply::Accepted);
        CHECK(!router.borderChainCurrent());
        CHECK(!stage.decide(host).border);
    }
    {
        KWin::PresentationRouter router;
        KWin::PresentationStage5 stage(router, nullptr);
        KWin::Stage5HostView host = anchorHost();
        host.anchorOk = true;
        host.liveGestureInstance = QStringLiteral("g-1");
        host.liveGestureSerial = QStringLiteral("7");
        host.moveLive = true;
        router.setSourceInstance(QStringLiteral("inst-1"));
        for (int i = 0; i < 7; ++i) {
            stage.onLayoutChanged();
        }
        KWin::PresentationStage5::EndpointResult first = stage.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "9", 404, true)), QStringLiteral("g-1"),
            QStringLiteral("7"), host);
        CHECK(first.apply == KWin::PresentationApply::Accepted);
        stage.onMotionFrame(true, host);
        CHECK(router.borderChainCurrent());
        KWin::PresentationStage5::EndpointResult genchange = stage.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("5", "1", 404, true, "7", "7", "gen-2")),
            QStringLiteral("g-1"), QStringLiteral("7"), host);
        CHECK(genchange.apply == KWin::PresentationApply::Accepted);
        CHECK(!router.borderChainCurrent());
        CHECK(!stage.decide(host).border);
    }

    // Exact-current replacement re-proves the witness frame; stale
    // coverage below the floor stays ignored with no mutation.
    {
        KWin::PresentationRouter router;
        KWin::PresentationStage5 stage(router, nullptr);
        KWin::Stage5HostView host = anchorHost();
        host.anchorOk = true;
        host.liveGestureInstance = QStringLiteral("g-1");
        host.liveGestureSerial = QStringLiteral("7");
        host.moveLive = true;
        router.setSourceInstance(QStringLiteral("inst-1"));
        for (int i = 0; i < 7; ++i) {
            stage.onLayoutChanged();
        }
        KWin::PresentationStage5::EndpointResult first = stage.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "9", 404, true)), QStringLiteral("g-1"),
            QStringLiteral("7"), host);
        CHECK(first.apply == KWin::PresentationApply::Accepted);
        stage.onMotionFrame(true, host);
        CHECK(router.counter == 8);
        CHECK(router.borderChainCurrent());
        // Structural bump clears the chain so the exact-current paint
        // below is a genuine re-proof, not a borrow.
        stage.onLayoutChanged();
        CHECK(router.counter == 9);
        CHECK(!router.borderChainCurrent());
        KWin::PresentationStage5::EndpointResult exact = stage.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "11", 404, true, "9")), QStringLiteral("g-1"),
            QStringLiteral("7"), host);
        CHECK(exact.apply == KWin::PresentationApply::Accepted);
        CHECK(router.paint.covered.set && router.paint.covered.counter == 9);
        CHECK(router.borderChainCurrent());
        CHECK(router.borderChain.counter == 9);
        CHECK(stage.decide(host).border);
        KWin::PresentationStage5::EndpointResult stale = stage.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "12", 404, true, "7")), QStringLiteral("g-1"),
            QStringLiteral("7"), host);
        CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(router.paint.witnessFrame == QRect(404, 8, 388, 584));
        CHECK(router.borderChainCurrent());
        CHECK(stage.decide(host).border);
    }

    // Gesture-boundary anchor preservation: the same arm, then the
    // genuine boundary route with same-anchor live facts, then continuous
    // same-anchor motion (translated and resized live frames) with only
    // older-coverage paints whose witness obs/frame differ keeps the border
    // visible on the prior chain with the original held witness while the
    // preview latch drops and the underlay/preview floors continue
    // correctly. A finish-side same-anchor boundary keeps the chain while
    // dropping the preview; an unknown close clears it. Caller-side
    // same-window identity is proven by passing the live host only for the
    // tracked window; foreground facts alone from an unrelated gesture
    // clear.
    {
        KWin::PresentationRouter router4;
        KWin::PresentationStage5 stage4(router4, nullptr);
        KWin::Stage5HostView host4 = anchorHost();
        host4.anchorOk = true;
        host4.liveGestureInstance = QStringLiteral("g-1");
        host4.liveGestureSerial = QStringLiteral("7");
        host4.moveLive = true;
        router4.setSourceInstance(QStringLiteral("inst-1"));
        for (int i = 0; i < 7; ++i) {
            stage4.onLayoutChanged();
        }
        CHECK(router4.counter == 7);
        CHECK(router4.structuralFloor == 7);
        KWin::PresentationStage5::EndpointResult armed = stage4.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "9", 404, true)),
            QStringLiteral("g-1"), QStringLiteral("7"), host4);
        CHECK(armed.apply == KWin::PresentationApply::Accepted);
        CHECK(router4.borderChainCurrent());
        CHECK(stage4.decide(host4).border);
        CHECK(stage4.decide(host4).underlay);
        CHECK(stage4.decide(host4).preview);
        // Genuine gesture boundary for the tracked window (drag-start seam):
        // the preview latch drops and the witness bumps plus the
        // structural tick, while the same-anchor chain survives by event
        // proof.
        stage4.onGestureBoundary(host4);
        CHECK(router4.counter == 8);
        CHECK(router4.structuralFloor == 8);
        CHECK(router4.paint.covered.set && router4.paint.covered.counter == 7);
        CHECK(!router4.paint.previewHas);
        CHECK(!stage4.decide(host4).preview);
        CHECK(!stage4.decide(host4).underlay);
        CHECK(router4.borderChainCurrent());
        CHECK(router4.borderChain.counter == 8);
        CHECK(stage4.decide(host4).border);
        // Continuous same-anchor motion after the boundary.
        stage4.onMotionFrame(true, host4);
        CHECK(router4.counter == 9);
        CHECK(router4.structuralFloor == 8);
        CHECK(router4.borderChainCurrent());
        CHECK(router4.borderChain.counter == 9);
        CHECK(stage4.decide(host4).border);
        CHECK(!stage4.decide(host4).preview);
        // Older coverage below the post-boundary floor stays stale with no
        // mutation; it must not resurrect the preview or disturb the chain.
        KWin::PresentationStage5::EndpointResult stale = stage4.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "10", 404, true)),
            QStringLiteral("g-1"), QStringLiteral("7"), host4);
        CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!router4.paint.previewHas);
        CHECK(router4.borderChainCurrent());
        CHECK(stage4.decide(host4).border);
        // Older coverage reaching the floor is accepted on the structural
        // path: the new underlay/preview/order/coverage is adopted while
        // the chain stays on the prior proof, so the strict underlay
        // hides and the preview returns.
        KWin::PresentationStage5::EndpointResult covered = stage4.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "11", 404, true, "8")),
            QStringLiteral("g-1"), QStringLiteral("7"), host4);
        CHECK(covered.apply == KWin::PresentationApply::Accepted);
        CHECK(router4.paint.covered.set && router4.paint.covered.counter == 8);
        CHECK(router4.paint.previewHas);
        CHECK(router4.borderChainCurrent());
        CHECK(router4.borderChain.counter == 9);
        CHECK(stage4.decide(host4).border);
        CHECK(!stage4.decide(host4).underlay);
        CHECK(stage4.decide(host4).preview);
        // Start->continuous-motion path: translated live frame motion keeps
        // the chain by event proof with no floor move.
        host4.trackedFrame = QRect(420, 16, 388, 584);
        stage4.onMotionFrame(true, host4);
        CHECK(router4.counter == 10);
        CHECK(router4.structuralFloor == 8);
        CHECK(router4.borderChainCurrent());
        CHECK(router4.borderChain.counter == 10);
        CHECK(router4.paint.witnessFrame == QRect(404, 8, 388, 584));
        CHECK(router4.paint.witnessObs == QStringLiteral("7"));
        CHECK(stage4.decide(host4).border);
        // Only-older cover8 paint with differing witness obs/frame: accepted
        // structurally, held witness retained, new order/coverage adopted.
        KWin::PresentationStage5::EndpointResult older12 = stage4.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "12", 520, true, "8", "9")),
            QStringLiteral("g-1"), QStringLiteral("7"), host4);
        CHECK(older12.apply == KWin::PresentationApply::Accepted);
        CHECK(router4.paint.covered.set && router4.paint.covered.counter == 8);
        CHECK(router4.paint.witnessFrame == QRect(404, 8, 388, 584));
        CHECK(router4.paint.witnessObs == QStringLiteral("7"));
        CHECK(router4.paint.fence.set && router4.paint.fence.obs == 12);
        CHECK(router4.paint.previewHas);
        CHECK(router4.borderChainCurrent());
        CHECK(router4.borderChain.counter == 10);
        CHECK(stage4.decide(host4).border);
        CHECK(!stage4.decide(host4).underlay);
        CHECK(stage4.decide(host4).preview);
        // Resized live frame motion: same proof, live geometry follows.
        host4.trackedFrame = QRect(420, 16, 400, 600);
        stage4.onMotionFrame(true, host4);
        CHECK(router4.counter == 11);
        CHECK(router4.structuralFloor == 8);
        CHECK(router4.borderChainCurrent());
        CHECK(router4.borderChain.counter == 11);
        CHECK(router4.paint.witnessFrame == QRect(404, 8, 388, 584));
        CHECK(router4.paint.witnessObs == QStringLiteral("7"));
        CHECK(stage4.decide(host4).border);
        KWin::PresentationStage5::EndpointResult older13 = stage4.applyEndpoint(
            QString::fromStdString(chainStructuralPayload("4", "13", 540, true, "8", "11")),
            QStringLiteral("g-1"), QStringLiteral("7"), host4);
        CHECK(older13.apply == KWin::PresentationApply::Accepted);
        CHECK(router4.paint.witnessFrame == QRect(404, 8, 388, 584));
        CHECK(router4.paint.witnessObs == QStringLiteral("7"));
        CHECK(router4.paint.fence.set && router4.paint.fence.obs == 13);
        CHECK(router4.borderChainCurrent());
        CHECK(router4.borderChain.counter == 11);
        CHECK(stage4.decide(host4).border);
        CHECK(!stage4.decide(host4).underlay);
        CHECK(stage4.decide(host4).preview);
        // Finish-side same-anchor boundary: preview latch drops, chain kept.
        stage4.onGestureBoundary(host4);
        CHECK(router4.counter == 12);
        CHECK(router4.structuralFloor == 12);
        CHECK(!router4.paint.previewHas);
        CHECK(router4.borderChainCurrent());
        CHECK(router4.borderChain.counter == 12);
        CHECK(stage4.decide(host4).border);
        CHECK(!stage4.decide(host4).preview);
        CHECK(!stage4.decide(host4).underlay);
        // Unknown close: clears with no new join.
        stage4.onGestureBoundary();
        CHECK(router4.counter == 13);
        CHECK(router4.structuralFloor == 13);
        CHECK(!router4.borderChainCurrent());
        CHECK(!stage4.decide(host4).border);
    }

    // Gesture-boundary negative proof: unknown, foreign, changed-output,
    // ineligible, unknown-frame, and gap-before-boundary chains all clear
    // with no new join, while the preview latch still drops and the
    // bump/floor continue.
    {
        auto boundaryClears = [](const KWin::Stage5HostView &boundaryHost, bool gapFirst = false) {
            KWin::PresentationRouter router;
            KWin::PresentationStage5 stage(router, nullptr);
            KWin::Stage5HostView host = anchorHost();
            host.anchorOk = true;
            host.liveGestureInstance = QStringLiteral("g-1");
            host.liveGestureSerial = QStringLiteral("7");
            host.moveLive = true;
            router.setSourceInstance(QStringLiteral("inst-1"));
            for (int i = 0; i < 7; ++i) {
                stage.onLayoutChanged();
            }
            KWin::PresentationStage5::EndpointResult armed = stage.applyEndpoint(
                QString::fromStdString(chainStructuralPayload("4", "9", 404, true)), QStringLiteral("g-1"),
                QStringLiteral("7"), host);
            if (armed.apply != KWin::PresentationApply::Accepted || !router.borderChainCurrent()) {
                return false;
            }
            if (gapFirst) {
                stage.onTrackedFrame();
                if (router.borderChainCurrent()) {
                    return false;
                }
            }
            const quint64 before = router.counter;
            const quint64 floorBefore = router.structuralFloor;
            stage.onGestureBoundary(boundaryHost);
            if (router.counter != before + 1 || router.structuralFloor != before + 1
                || floorBefore + 1 != router.structuralFloor) {
                return false;
            }
            if (router.paint.previewHas) {
                return false;
            }
            return !router.borderChainCurrent() && !stage.decide(host).border;
        };
        KWin::Stage5HostView same = anchorHost();
        same.anchorOk = true;
        same.liveGestureInstance = QStringLiteral("g-1");
        same.liveGestureSerial = QStringLiteral("7");
        same.moveLive = true;
        CHECK(boundaryClears(KWin::Stage5HostView{}));
        KWin::Stage5HostView foreign = same;
        foreign.trackedId = QStringLiteral("win-2");
        foreign.liveActiveId = QStringLiteral("win-2");
        CHECK(boundaryClears(foreign));
        KWin::Stage5HostView changedOut = same;
        changedOut.outputId = QStringLiteral("DP-2");
        CHECK(boundaryClears(changedOut));
        KWin::Stage5HostView changedScale = same;
        changedScale.outputScale = 2.0;
        CHECK(boundaryClears(changedScale));
        KWin::Stage5HostView unknownEligible = same;
        unknownEligible.liveEligibleKnown = false;
        CHECK(boundaryClears(unknownEligible));
        KWin::Stage5HostView ineligible = same;
        ineligible.liveEligible = false;
        CHECK(boundaryClears(ineligible));
        KWin::Stage5HostView unknownFrame = same;
        unknownFrame.trackedFrameKnown = false;
        CHECK(boundaryClears(unknownFrame));
        CHECK(boundaryClears(same, true));
    }

    if (failures == 0) {
        std::printf("borderchain: all checks passed\n");
        return EXIT_SUCCESS;
    }
    std::fprintf(stderr, "borderchain: %d checks failed\n", failures);
    return EXIT_FAILURE;
}
