// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
//
// Stage 5 publication/recovery and cross-surface refresh regressions: the
// exact production PresentationStage5 handlers (the same methods
// activewindowborder.cpp calls) driven with the real PresentationRouter and
// the real PresentationFactsSource over a private bus, fed the RECORDED
// cross-language fixture bytes from test-fixtures/stage5 (real broker
// output shaped by the real JS route: P5, pre-move P6, post-move covering
// paint, proven A-preview), and asserting on the stubbed item state the
// production refresh actually re-evaluates.
//
// Preserved deficiencies under test (RED: these fail, never fixed here):
// (D1-publication-omission) local counter bumps are never published after
// the seed bind, so the only post-bump-producible coverage is stale and no
// covering capture can exist to repaint the hidden surfaces;
// (D2-partial-refresh) tracked-frame refreshes the border only,
// member-frame the anchor plus the underlay only, and gesture boundaries
// the preview only, so the other currently-invalid surfaces stay visibly
// stale while the visibility decision already hides them.
//
// Every leg also proves what already works (GREEN): the renderer endpoint
// rejects stale/foreign/malformed coverage without touching the held
// display, fences survive activation and error hides, and a truly covering
// paint (the modeled fixed behavior) still recovers through the full
// refresh. The harness provides the private bus (run the binary under
// dbus-run-session); no KWin, no live state.
//
// Fixture provenance: envelope-*.json are real startWorldAdapterEntry
// DescribeWorld bytes over the verbatim facts-seed.json waiter reply;
// projection-*.json are the real WorldService outputs for those envelopes
// (asserted byte-equal by the broker leg); dispatch-*.json are the real
// formatPresentationPayload outputs for those projections (asserted
// byte-equal by the JS subscriber leg). Identity seam (disclosed): the
// native facts instance is a fresh random UUID per leg while the fixtures
// carry "effect-1", so each leg substitutes its live instance into the
// dispatch bytes before applying (opaque-id shape preserved, never a
// producer replacement).
//
// Fixture location is baked at build time via STAGE5_FIXTURE_DIR (source
// tree locally, Nix source store in the fileset build).

#include "presentation_stage5.h"

#include "activeborderlogic.h"
#include "presentationfacts.h"

#include <QCoreApplication>
#include <QDBusConnection>
#include <QFile>
#include <QString>

#include <cstdio>
#include <cstdlib>
#include <string>

#ifndef STAGE5_FIXTURE_DIR
#error "STAGE5_FIXTURE_DIR must be provided by the build"
#endif

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
// applied value, which is how the test observes stale items (not merely
// the refresh bit, but the actual observable item state).
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

KWin::Stage5HostView gestureHost(const char *instance, const char *serial)
{
    KWin::Stage5HostView host = allLiveHost();
    host.liveGestureInstance = QString::fromUtf8(instance);
    host.liveGestureSerial = QString::fromUtf8(serial);
    return host;
}

// One directed fixture read: the recorded bytes with the leg's live facts
// instance substituted for the recorded "effect-1" namespace.
QString dispatchBytes(const QString &fixtureDir, const char *name, const QString &liveInstance)
{
    QFile file(fixtureDir + QStringLiteral("/") + QString::fromUtf8(name));
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
        std::fprintf(stderr, "HARNESS-ERROR: cannot read fixture %s\n", name);
        std::exit(2);
    }
    QString bytes = QString::fromUtf8(file.readAll());
    bytes.replace(QStringLiteral("effect-1"), liveInstance);
    return bytes;
}

// The facts seed snapshot through the real serializer: live
// instance/revision/invalidation plus the test's own style/chord seed and
// the stale-identity waiter-reply args (changed:1, matching the immediate
// completion path the transport test pins).
QString seedSnapshotJson(KWin::PresentationFactsSource &facts)
{
    KWin::PresentationFactsSnapshot snapshot;
    snapshot.instance = facts.instance();
    snapshot.rev = facts.revision();
    snapshot.invalidation = facts.invalidation();
    snapshot.chord = true;
    snapshot.chordKnown = true;
    snapshot.gap = 0.0;
    snapshot.width = 4.0;
    snapshot.extension = 3.0;
    snapshot.changed = true;
    snapshot.superseded = false;
    snapshot.available = true;
    return KWin::presentationFactsJson(snapshot);
}

KWin::PresentationStage5::EndpointResult applyStr(KWin::PresentationStage5 &stage, const QString &payload,
    const char *liveInstance = "", const char *liveSerial = "0")
{
    return stage.applyEndpoint(payload, QString::fromUtf8(liveInstance), QString::fromUtf8(liveSerial));
}

// Production seed order: style, chord, then the one-time bind publish
// (counter still zero) plus the router namespacing.
void seed(KWin::PresentationStage5 &stage, KWin::PresentationFactsSource &facts)
{
    facts.noteStyle(0.0, 4.0, 3.0);
    facts.noteChord(true, true);
    stage.bindFacts(&facts);
}

} // namespace

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    const QString fixtureDir = QString::fromUtf8(STAGE5_FIXTURE_DIR);

    // L0: the recorded seed snapshot is exactly what the real class
    // publishes for the production seed (instance aside): same keys, same
    // revision counting (style seeds without bumping, chord bumps once),
    // same zero invalidation after the bind.
    {
        KWin::PresentationRouter router;
        KWin::PresentationFactsSource facts(QDBusConnection::sessionBus(), &app);
        KWin::PresentationStage5 stage(router);
        seed(stage, facts);
        const QString inst = facts.instance();
        CHECK(router.counter == 0);
        CHECK(facts.invalidation() == 0);
        CHECK(facts.revision() == 1);
        QFile seedFile(fixtureDir + QStringLiteral("/facts-seed.json"));
        CHECK(seedFile.open(QIODevice::ReadOnly | QIODevice::Text));
        QString recorded = QString::fromUtf8(seedFile.readAll());
        recorded.replace(QStringLiteral("effect-1"), inst);
        CHECK(seedSnapshotJson(facts) == recorded);
    }

    // L1 tracked-frame: the border floor plus the coverage mismatch hide
    // both surfaces in the decision, but the production refresh
    // re-evaluates the border only (D2: the underlay stub stays visibly
    // stale). The bump is never published (D1), so the only
    // post-bump-producible coverage (the still-published 0, e.g. P6) is
    // rejected and nothing can repaint until a covering capture exists.
    {
        KWin::PresentationRouter router;
        KWin::PresentationFactsSource facts(QDBusConnection::sessionBus(), &app);
        KWin::PresentationStage5 stage(router);
        SurfaceStub stub;
        seed(stage, facts);
        const QString inst = facts.instance();
        const auto p5 = applyStr(stage, dispatchBytes(fixtureDir, "dispatch-p5.json", inst));
        CHECK(p5.apply == KWin::PresentationApply::Accepted);
        stub.apply(p5.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);

        const KWin::Stage5Refresh bump = stage.onTrackedFrame();
        CHECK(router.counter == 1);
        CHECK(facts.invalidation() == 1); // D1-publication-omission (actual: 0).
        const KWin::Stage5Visibility hidden = stage.decide(allLiveHost());
        CHECK(!hidden.border);
        CHECK(!hidden.underlay);
        stub.apply(bump, hidden);
        CHECK(!stub.border);
        CHECK(!stub.underlay); // D2-partial-refresh (actual: still true).

        // Stale pre-move P6 (newer rev/obs, same old coverage) is rejected
        // without touching the held display: no stale paint.
        const auto stale =
            applyStr(stage, dispatchBytes(fixtureDir, "dispatch-p6.json", inst));
        CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stale.refresh.border);
        CHECK(!stale.refresh.repaint);

        // The explicit publish path produces exactly the modeled advanced
        // witness the cover fixture assumes (rev 2, invalidation 1).
        stage.publishInvalidation();
        CHECK(facts.invalidation() == 1);
        CHECK(facts.revision() == 2);

        // A truly covering post-move capture recovers through the full
        // refresh: the endpoint machinery itself is intact.
        const auto cover =
            applyStr(stage, dispatchBytes(fixtureDir, "dispatch-cover.json", inst));
        CHECK(cover.apply == KWin::PresentationApply::Accepted);
        stub.apply(cover.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
    }

    // L2 member-frame with the original geometry race: P5 displayed, pre-move
    // P6 (newer snapshot, old coverage) delivered after the member move must
    // not paint, and only the later covering capture repaints. The
    // production refresh re-evaluates the anchor plus the underlay only
    // (D2: the border stub stays visibly stale); the bump is never
    // published (D1).
    {
        KWin::PresentationRouter router;
        KWin::PresentationFactsSource facts(QDBusConnection::sessionBus(), &app);
        KWin::PresentationStage5 stage(router);
        SurfaceStub stub;
        seed(stage, facts);
        const QString inst = facts.instance();
        const auto p5 = applyStr(stage, dispatchBytes(fixtureDir, "dispatch-p5.json", inst));
        CHECK(p5.apply == KWin::PresentationApply::Accepted);
        stub.apply(p5.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);

        const KWin::Stage5Refresh bump = stage.onMemberFrame();
        CHECK(router.counter == 1);
        CHECK(facts.invalidation() == 1); // D1-publication-omission (actual: 0).
        const KWin::Stage5Visibility hidden = stage.decide(allLiveHost());
        CHECK(!hidden.border);
        CHECK(!hidden.underlay);
        stub.apply(bump, hidden);
        CHECK(!stub.border); // D2-partial-refresh (actual: still true).
        CHECK(!stub.underlay);

        // Race step: the in-flight pre-move P6 (rev/obs newer than P5, same
        // stale coverage) completes after the move, before any post-move
        // capture is accepted. It must not paint.
        const auto stale =
            applyStr(stage, dispatchBytes(fixtureDir, "dispatch-p6.json", inst));
        CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stale.refresh.border);
        CHECK(!stale.refresh.repaint);
        CHECK(!stub.underlay);

        // The later covering capture repaints (recovery leg).
        const auto cover =
            applyStr(stage, dispatchBytes(fixtureDir, "dispatch-cover.json", inst));
        CHECK(cover.apply == KWin::PresentationApply::Accepted);
        stub.apply(cover.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);

        // Member visibility routing stays bump-free and member-gated.
        CHECK(router.counter == 1);
        const KWin::Stage5Refresh member = stage.onMemberVisibility(QStringLiteral("win-a"));
        CHECK(member.anchor);
        CHECK(member.underlay);
        CHECK(router.counter == 1);
        const KWin::Stage5Refresh foreign = stage.onMemberVisibility(QStringLiteral("zzz"));
        CHECK(!foreign.anchor);
        CHECK(router.counter == 1);
    }

    // L3 gesture boundary with the original cross-gesture race: the proven
    // A preview shows while A is live; the finish-A/start-B boundary drops
    // the latch and bumps with no floors, so the decision hides every
    // surface but the production refresh re-evaluates the preview only
    // (D2: border and underlay stubs stay visibly stale). The bump is never
    // published (D1). The stale A preview delivered entirely inside B
    // (plus an already-sent A paint) is rejected; a valid B preview would
    // need the unpublished covering witness.
    {
        KWin::PresentationRouter router;
        KWin::PresentationFactsSource facts(QDBusConnection::sessionBus(), &app);
        KWin::PresentationStage5 stage(router);
        SurfaceStub stub;
        seed(stage, facts);
        const QString inst = facts.instance();
        const auto p5 = applyStr(stage, dispatchBytes(fixtureDir, "dispatch-p5.json", inst));
        CHECK(p5.apply == KWin::PresentationApply::Accepted);
        stub.apply(p5.refresh, stage.decide(allLiveHost()));
        // The covering capture for the earlier tracked move lands first.
        const auto track = stage.onTrackedFrame();
        CHECK(router.counter == 1);
        stub.apply(track, stage.decide(allLiveHost()));
        const auto cover =
            applyStr(stage, dispatchBytes(fixtureDir, "dispatch-cover.json", inst));
        CHECK(cover.apply == KWin::PresentationApply::Accepted);
        stub.apply(cover.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);

        // Gesture A opens: the proven preview (producer gesture-1/1, live
        // gesture-1/1) shows alongside the border and the underlay. This is
        // the faithful production combination the broker actually emits.
        const auto preview =
            applyStr(stage, dispatchBytes(fixtureDir, "dispatch-preview.json", inst), "gesture-1", "1");
        CHECK(preview.apply == KWin::PresentationApply::Accepted);
        const KWin::Stage5Visibility live = stage.decide(gestureHost("gesture-1", "1"));
        CHECK(live.border);
        CHECK(live.underlay);
        CHECK(live.preview);
        stub.apply(preview.refresh, live);
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(stub.preview);

        // Finish A / start B: the boundary drops the latch and bumps with
        // no floors. Every surface is invalid now, but only the preview
        // re-evaluates.
        const KWin::Stage5Refresh boundary = stage.onGestureBoundary();
        CHECK(router.counter == 2);
        CHECK(facts.invalidation() == 2); // D1-publication-omission (actual: 0).
        const KWin::Stage5Visibility insideB = stage.decide(gestureHost("gesture-1", "2"));
        CHECK(!insideB.border);
        CHECK(!insideB.underlay);
        CHECK(!insideB.preview);
        stub.apply(boundary, insideB);
        CHECK(!stub.border); // D2-partial-refresh (actual: still true).
        CHECK(!stub.underlay); // D2-partial-refresh (actual: still true).
        CHECK(!stub.preview);

        // Race step: the stale A preview is drained, published, and read
        // entirely inside B (newer rev/obs, old coverage, foreign producer
        // for the live B). It must never paint in B.
        const auto crossed =
            applyStr(stage, dispatchBytes(fixtureDir, "dispatch-preview.json", inst), "gesture-1", "2");
        CHECK(crossed.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!crossed.refresh.preview);
        CHECK(!crossed.refresh.repaint);
        CHECK(!stub.preview);
    }

    // L4 focus clear with the original in-transit race: show A, activate B
    // (display-only hide plus the all-scope bump, every surface refreshed),
    // deliver the in-transit old A read before the post-activation capture
    // (must not resurrect), return to A through the covering capture
    // (repaints), then drive the error-hide path (malformed hides
    // display-only with the fence retained; a further old A paint still
    // cannot resurrect). The activation bump is never published (D1).
    {
        KWin::PresentationRouter router;
        KWin::PresentationFactsSource facts(QDBusConnection::sessionBus(), &app);
        KWin::PresentationStage5 stage(router);
        SurfaceStub stub;
        seed(stage, facts);
        const QString inst = facts.instance();
        const auto p5 = applyStr(stage, dispatchBytes(fixtureDir, "dispatch-p5.json", inst));
        CHECK(p5.apply == KWin::PresentationApply::Accepted);
        stub.apply(p5.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);

        const KWin::Stage5Refresh activation = stage.onActivation();
        CHECK(router.counter == 1);
        CHECK(facts.invalidation() == 1); // D1-publication-omission (actual: 0).
        CHECK(!router.paint.has);
        CHECK(router.paint.fence.set);
        stub.apply(activation, stage.decide(allLiveHost()));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(!stub.preview);

        // In-transit old A paint (newer rev/obs than P5, stale coverage)
        // delivered after the activation, before any post-activation
        // capture: must not resurrect the cleared paint.
        const auto stale =
            applyStr(stage, dispatchBytes(fixtureDir, "dispatch-p6.json", inst));
        CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stale.refresh.border);
        CHECK(!stale.refresh.repaint);
        CHECK(!stub.border);

        // Return to A: the covering capture repaints (recovery leg); the
        // fence advanced to it.
        const auto cover =
            applyStr(stage, dispatchBytes(fixtureDir, "dispatch-cover.json", inst));
        CHECK(cover.apply == KWin::PresentationApply::Accepted);
        stub.apply(cover.refresh, stage.decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(router.paint.fence.set);
        CHECK(router.paint.fence.rev == 3);

        // Error-hide path: malformed clears display-only with a full
        // refresh while the fence survives; a further old A paint still
        // cannot resurrect.
        const auto malformed = applyStr(stage, QStringLiteral("{}"));
        CHECK(malformed.apply == KWin::PresentationApply::Malformed);
        CHECK(malformed.refresh.border);
        CHECK(router.paint.fence.set);
        CHECK(router.paint.fence.rev == 3);
        stub.apply(malformed.refresh, stage.decide(allLiveHost()));
        CHECK(!stub.border);
        const auto afterError =
            applyStr(stage, dispatchBytes(fixtureDir, "dispatch-p6.json", inst));
        CHECK(afterError.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stub.border);

        // A foreign witness instance never applies (reused counters across
        // instances never compare).
        QString foreign = dispatchBytes(fixtureDir, "dispatch-cover.json", inst);
        foreign.replace(inst, QStringLiteral("other-instance"));
        const auto foreignCover = applyStr(stage, foreign);
        CHECK(foreignCover.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stub.border);
    }

    if (failures != 0) {
        std::fprintf(stderr, "stage5 recovery test: %d failure(s)\n", failures);
        return 1;
    }
    std::printf("stage5 recovery test: all checks passed\n");
    return 0;
}
