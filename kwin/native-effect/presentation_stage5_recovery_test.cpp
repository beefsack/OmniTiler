// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
//
// Stage 5 publication/recovery and cross-surface refresh regressions: the
// exact production PresentationStage5 handlers (the same methods
// activewindowborder.cpp calls) driven with the real PresentationRouter and
// the real PresentationFactsSource over a private bus in the production
// cross-connection topology (effect-owned dedicated connection serving the
// facts object, script caller via the session bus holding org.kde.KWin), fed
// the RECORDED cross-language fixture bytes from test-fixtures/stage5 (real
// broker output over the real JS route: P5, pre-move P6, post-move covering
// paints, proven A preview and drains, valid-B repaint, and the rapid
// same-source serial-B covering capture), and asserting on
// the stubbed item state the production refresh actually re-evaluates.
//
// Publication is proven through the real waiter, never a getter: the first
// handler event runs while a waiter armed with the current instance/rev is
// held, and the test asserts the event publishes/wakes it with the advanced
// counter. Every other bump asserts the real post-event facts snapshot
// (unknown-identity Wait, immediate) equals the recorded advanced facts
// fixture. No explicit publish bypass exists: recovery paints are the
// recorded covering fixtures the real broker served for the published
// witness, so the same tests prove the actual route.
//
// Production behavior under test: every local counter bump publishes, so
// the post-bump facts equal the recorded advanced fixtures and the armed
// waiter wakes with the advanced counter; tracked-frame, member-frame, and
// gesture boundaries refresh every surface, so no stale item remains
// visible while the visibility decision already hides it.
//
// Every leg also proves what already works (GREEN): the seed facts equal
// the recorded bytes, fixture advanced facts equal the real class output
// (scratch source, no stage involved), the renderer endpoint rejects
// stale/foreign/malformed coverage without touching the held display,
// producer mismatch isolates to the preview while border/underlay still
// paint, fences survive activation and error hides, and truly covering
// paints (the recorded broker fixtures) recover through the full refresh.
// The harness provides the private bus (run the binary under
// dbus-run-session); no KWin, no live state. The only waits are the
// production waiter lease plus one same-connection Introspect delivery
// barrier per armed bump (bounded, failure-deadline watchdogs only): no
// product sleeps, settles, or polling.
//
// Fixture provenance: envelope-*.json are real startWorldAdapterEntry
// DescribeWorld bytes over the verbatim facts fixtures; projection-*.json
// are the real WorldService outputs for those envelopes (asserted
// byte-equal by the broker leg); dispatch-*.json are the real
// formatPresentationPayload outputs for those projections (asserted
// byte-equal by the JS subscriber leg); dispatch-a3-b.json is the real
// crossed-B subscriber render of projection-a3.json. Identity seams
// (disclosed): the native facts instance is a fresh random UUID per leg
// while the fixtures carry "effect-1", so each leg substitutes its live
// instance into the dispatch bytes before applying (opaque-id shape
// preserved, never a producer replacement); source/sourceSeq/nativeInvalid
// are per-run transport tokens normalized at record time. The gesture-B
// producer is a realistic UUID fixture identity.
//
// Fixture location is baked at build time via STAGE5_FIXTURE_DIR (source
// tree locally, Nix source store in the fileset build).

#include "presentation_stage5.h"

#include "activeborderlogic.h"
#include "presentationfacts.h"

#include <QCoreApplication>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDBusPendingCall>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QEventLoop>
#include <QFile>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTimer>

#include <cstdio>
#include <cstdlib>
#include <functional>
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

// Production cross-connection topology handles, set up once in main.
QDBusConnection g_effect(QStringLiteral("stage5-detached"));
QDBusConnection g_script(QStringLiteral("stage5-detached"));

// Send one WaitPresentationFacts request from the script connection to the
// effect connection's unique name and await the delayed reply through a
// nested loop. Empty on transport error or timeout (the caller fails).
QString waitFacts(const QString &objectPath, const QString &request, int timeoutMs)
{
    QDBusMessage msg = QDBusMessage::createMethodCall(g_effect.baseService(), objectPath,
        QStringLiteral("com.omnitiler.ActiveBorder1"), QStringLiteral("WaitPresentationFacts"));
    msg.setArguments({request});
    QDBusPendingCallWatcher watcher(g_script.asyncCall(msg));
    QEventLoop loop;
    QTimer timer;
    timer.setSingleShot(true);
    QObject::connect(&timer, &QTimer::timeout, &loop, &QEventLoop::quit);
    QObject::connect(&watcher, &QDBusPendingCallWatcher::finished, &loop, &QEventLoop::quit);
    timer.start(timeoutMs);
    loop.exec();
    if (!watcher.isFinished()) {
        return {};
    }
    QDBusPendingReply<QString> reply = watcher;
    if (!reply.isValid()) {
        return {};
    }
    return reply.value();
}

QString waitRequest(const QString &instance, const QString &rev, int leaseMs)
{
    return QStringLiteral("{\"instance\":\"%1\",\"rev\":\"%2\",\"leaseMs\":%3}").arg(instance, rev).arg(leaseMs);
}

QJsonObject parseSnapshot(const QString &reply)
{
    const QJsonDocument doc = QJsonDocument::fromJson(reply.toUtf8());
    if (!doc.isObject()) {
        return {};
    }
    return doc.object();
}

// One directed fixture read: the recorded bytes verbatim.
QString fixtureBytes(const QString &fixtureDir, const char *name)
{
    QFile file(fixtureDir + QStringLiteral("/") + QString::fromUtf8(name));
    if (!file.open(QIODevice::ReadOnly | QIODevice::Text)) {
        std::fprintf(stderr, "HARNESS-ERROR: cannot read fixture %s\n", name);
        std::exit(2);
    }
    return QString::fromUtf8(file.readAll());
}

// The recorded dispatch bytes with the leg's live facts instance
// substituted for the recorded "effect-1" namespace.
QString dispatchBytes(const QString &fixtureDir, const char *name, const QString &liveInstance)
{
    QString bytes = fixtureBytes(fixtureDir, name);
    bytes.replace(QStringLiteral("effect-1"), liveInstance);
    return bytes;
}

// The live facts snapshot through the real waiter (unknown identity:
// completes immediately with the fresh snapshot, never holds).
QString immediateSnapshot(const QString &objectPath)
{
    return waitFacts(objectPath, waitRequest(QStringLiteral("unknown-stage5"), QStringLiteral("9"), 1000), 10000);
}

// Arm a waiter with the current instance/rev, prove the server processed
// the Wait through a same-connection Introspect barrier (D-Bus FIFO per
// sender: the barrier reply proves the Wait was dispatched, armed when
// identity/rev are current), fire the handler event, and await the
// publication wake. The event wakes it with the advanced counter
// (changed:1). earlyReply reports whether the
// Wait completed immediately at the barrier (must be false: the arm matched
// the live facts). The 10 s barrier bound is a failure deadline only, never
// an ordering oracle. No same-process introspection is consulted.
QString armedBumpWait(const QString &objectPath, const QString &instance, const QString &rev,
    const std::function<KWin::Stage5Refresh()> &event, bool *earlyReply)
{
    QDBusMessage msg = QDBusMessage::createMethodCall(g_effect.baseService(), objectPath,
        QStringLiteral("com.omnitiler.ActiveBorder1"), QStringLiteral("WaitPresentationFacts"));
    msg.setArguments({waitRequest(instance, rev, 1000)});
    auto *watcher = new QDBusPendingCallWatcher(g_script.asyncCall(msg));
    {
        QDBusMessage barrier = QDBusMessage::createMethodCall(g_effect.baseService(), objectPath,
            QStringLiteral("org.freedesktop.DBus.Introspectable"), QStringLiteral("Introspect"));
        QDBusPendingCallWatcher barrierWatcher(g_script.asyncCall(barrier));
        QEventLoop loop;
        QTimer timer;
        timer.setSingleShot(true);
        QObject::connect(&timer, &QTimer::timeout, &loop, &QEventLoop::quit);
        QObject::connect(&barrierWatcher, &QDBusPendingCallWatcher::finished, &loop, &QEventLoop::quit);
        timer.start(10000);
        loop.exec();
        if (!barrierWatcher.isFinished()) {
            std::fprintf(stderr, "HARNESS-ERROR: facts Wait delivery barrier timed out\n");
            std::exit(2);
        }
        {
            QDBusPendingReply<QString> barrierReply = barrierWatcher;
            if (!barrierReply.isValid() || barrierReply.value().isEmpty()) {
                std::fprintf(stderr, "HARNESS-ERROR: facts Wait delivery barrier rejected: %s\n",
                    barrierReply.error().message().toUtf8().constData());
                std::exit(2);
            }
        }
    }
    *earlyReply = watcher->isFinished();
    QString value;
    if (*earlyReply) {
        QDBusPendingReply<QString> reply = *watcher;
        if (reply.isValid()) {
            value = reply.value();
        }
        watcher->deleteLater();
        return value;
    }
    event();
    {
        QEventLoop loop;
        QTimer timer;
        timer.setSingleShot(true);
        QObject::connect(&timer, &QTimer::timeout, &loop, &QEventLoop::quit);
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &loop, &QEventLoop::quit);
        timer.start(10000);
        loop.exec();
    }
    if (watcher->isFinished()) {
        QDBusPendingReply<QString> reply = *watcher;
        if (reply.isValid()) {
            value = reply.value();
        }
    }
    watcher->deleteLater();
    return value;
}

struct Leg {
    Leg(const Leg &) = delete;
    Leg &operator=(const Leg &) = delete;
    Leg(Leg &&) = delete;
    Leg &operator=(Leg &&) = delete;
    Leg() = default;
    KWin::PresentationRouter router;
    KWin::PresentationFactsSource *facts = nullptr;
    KWin::PresentationStage5 *stage = nullptr;
    QString path;
    QString instance;
};

// Construct one leg in place: the caller's Leg never moves, so the
// stage's pointer to the caller-owned router stays valid. (Returning the
// owning value would depend on optional copy elision to keep that
// pointer live.)
void initLeg(Leg &leg, const QString &path)
{
    leg.path = path;
    leg.facts = new KWin::PresentationFactsSource(g_effect, QCoreApplication::instance());
    if (!g_effect.registerObject(path, leg.facts, QDBusConnection::ExportScriptableContents)) {
        std::fprintf(stderr, "HARNESS-ERROR: cannot register facts object %s\n", qPrintable(path));
        std::exit(2);
    }
    leg.stage = new KWin::PresentationStage5(leg.router);
    leg.instance = leg.facts->instance();
}

void destroyLeg(Leg &leg)
{
    g_effect.unregisterObject(leg.path);
    delete leg.stage;
    leg.stage = nullptr;
    delete leg.facts;
    leg.facts = nullptr;
}

// Production seed order: style, chord, then the one-time bind publish
// (counter still zero) plus the router namespacing.
void seed(Leg &leg)
{
    leg.facts->noteStyle(0.0, 4.0, 3.0);
    leg.facts->noteChord(true, true);
    leg.stage->bindFacts(leg.facts);
}

KWin::PresentationStage5::EndpointResult applyStr(
    Leg &leg, const QString &payload, const char *liveInstance, const char *liveSerial)
{
    return leg.stage->applyEndpoint(payload, QString::fromUtf8(liveInstance), QString::fromUtf8(liveSerial));
}

// Real post-event facts must equal the recorded advanced fixture after the
// disclosed instance normalization.
void checkAdvancedFacts(const QString &reply, const QString &liveInstance, const QString &fixtureDir,
    const char *fixtureName, const char *wantedRev, const char *wantedInvalid)
{
    CHECK(!reply.isEmpty());
    if (reply.isEmpty()) {
        return;
    }
    const QJsonObject body = parseSnapshot(reply);
    CHECK(body.value(QStringLiteral("changed")).toInt(-1) == 1);
    CHECK(body.value(QStringLiteral("rev")).toString() == QString::fromUtf8(wantedRev));
    CHECK(body.value(QStringLiteral("invalidation")).toString() == QString::fromUtf8(wantedInvalid));
    QString normalized = reply;
    normalized.replace(liveInstance, QStringLiteral("effect-1"));
    CHECK(normalized == fixtureBytes(fixtureDir, fixtureName));
}

} // namespace

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    const QString fixtureDir = QString::fromUtf8(STAGE5_FIXTURE_DIR);

    QTimer watchdog;
    watchdog.setSingleShot(true);
    QObject::connect(&watchdog, &QTimer::timeout, &app, []() {
        std::fprintf(stderr, "FAIL: stage5 recovery test watchdog fired\n");
        std::exit(1);
    });
    watchdog.start(60000);

    // Production topology: the effect owns a named dedicated connection to
    // the bus; the script calls via QDBusConnection::sessionBus, exactly
    // like the KWin script route. org.kde.KWin registers on the script
    // connection, so the owner gate sees the script sender as the live KWin
    // owner globally.
    const QString address = QString::fromLocal8Bit(qgetenv("DBUS_SESSION_BUS_ADDRESS"));
    if (address.isEmpty()) {
        std::fprintf(stderr, "FAIL: test needs DBUS_SESSION_BUS_ADDRESS (run under dbus-run-session)\n");
        return 1;
    }
    g_effect = QDBusConnection::connectToBus(address, QStringLiteral("stage5-effect"));
    g_script = QDBusConnection::sessionBus();
    CHECK(g_effect.isConnected() && g_script.isConnected());
    CHECK(g_effect.name() != g_script.name());
    CHECK(g_script.interface()->registerService(QStringLiteral("org.kde.KWin")));

    // F0: the recorded advanced facts fixtures are exact real-producer
    // bytes. A scratch source (no stage involved) seeded like production
    // and advanced through the real noteInvalidation path emits exactly
    // them (instance aside). GREEN now and post-fix: the class behavior
    // under test is already correct; only the stage publication is not.
    {
        Leg leg;
        initLeg(leg, QStringLiteral("/com/omnitiler/ActiveBorderProof"));
        seed(leg);
        QString avanc0 = immediateSnapshot(leg.path);
        {
            QString normalized = avanc0;
            normalized.replace(leg.instance, QStringLiteral("effect-1"));
            CHECK(normalized == fixtureBytes(fixtureDir, "facts-seed.json"));
        }
        leg.facts->noteInvalidation(1);
        checkAdvancedFacts(immediateSnapshot(leg.path), leg.instance, fixtureDir, "facts-cover.json", "2", "1");
        leg.facts->noteInvalidation(2);
        checkAdvancedFacts(immediateSnapshot(leg.path), leg.instance, fixtureDir, "facts-inv2.json", "3", "2");
        leg.facts->noteInvalidation(3);
        checkAdvancedFacts(immediateSnapshot(leg.path), leg.instance, fixtureDir, "facts-inv3.json", "4", "3");
        destroyLeg(leg);
    }

    // L1 tracked double-frame with the preview stale path: P5 displayed,
    // first bump publishes, the still-newer pre-move P6
    // captured before the invalidation is delivered must not paint
    // (coverage, never mere revision: P6 rev/obs is newer than the P5
    // fence), the covering capture restores border+underlay, the proven A
    // preview shows, the second bump hides every surface and the
    // production refresh re-evaluates every surface, and the recorded
    // witness-2 A
    // drain (real broker bytes, producer A, coverage current) recovers all
    // three through the full endpoint refresh.
    {
        Leg leg;
        initLeg(leg, QStringLiteral("/com/omnitiler/ActiveBorder1"));
        SurfaceStub stub;
        seed(leg);
        CHECK(leg.router.counter == 0);
        CHECK(leg.facts->invalidation() == 0);
        CHECK(leg.facts->revision() == 1);
        // L0-in-leg: the recorded seed snapshot is exactly what the real
        // class serves for the production seed over the real waiter.
        {
            QString normalized = immediateSnapshot(leg.path);
            normalized.replace(leg.instance, QStringLiteral("effect-1"));
            CHECK(normalized == fixtureBytes(fixtureDir, "facts-seed.json"));
        }
        const QString inst = leg.instance;
        const auto p5 = applyStr(leg, dispatchBytes(fixtureDir, "dispatch-p5.json", inst), "", "0");
        CHECK(p5.apply == KWin::PresentationApply::Accepted);
        stub.apply(p5.refresh, leg.stage->decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);

        // First tracked bump under the armed waiter: the event must publish
        // and wake it with the advanced counter.
        {
            const QString cur = immediateSnapshot(leg.path);
            CHECK(parseSnapshot(cur).value(QStringLiteral("rev")).toString() == QStringLiteral("1"));
            bool earlyReply = true;
            const QString woken = armedBumpWait(leg.path, inst,
                QStringLiteral("1"),
                [&leg, &stub]() {
                    const KWin::Stage5Refresh refresh = leg.stage->onTrackedFrame();
                    stub.apply(refresh, leg.stage->decide(allLiveHost()));
                    return refresh;
                },
                &earlyReply);
            CHECK(!earlyReply);
            CHECK(leg.router.counter == 1);
            checkAdvancedFacts(woken, inst, fixtureDir, "facts-cover.json", "2", "1");
            CHECK(!stub.border);
            CHECK(!stub.underlay);
        }
        // In-transit pre-move P6 (rev/obs newer than the displayed P5
        // fence, same old coverage) completes after the native move, before
        // any post-move capture is accepted. Coverage rejects it: no stale
        // paint, no display touch, fence retained for the covering capture.
        {
            const auto stale = applyStr(leg, dispatchBytes(fixtureDir, "dispatch-p6.json", inst), "", "0");
            CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
            CHECK(!stale.refresh.border);
            CHECK(!stale.refresh.repaint);
            CHECK(!stub.border);
            CHECK(!stub.underlay);
        }
        const auto cover =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-cover.json", inst), "", "0");
        CHECK(cover.apply == KWin::PresentationApply::Accepted);
        stub.apply(cover.refresh, leg.stage->decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
        const auto preview =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-preview.json", inst), "gesture-1", "1");
        CHECK(preview.apply == KWin::PresentationApply::Accepted);
        {
            const KWin::Stage5Visibility live = leg.stage->decide(gestureHost("gesture-1", "1"));
            CHECK(live.border);
            CHECK(live.underlay);
            CHECK(live.preview);
            stub.apply(preview.refresh, live);
            CHECK(stub.border);
            CHECK(stub.underlay);
            CHECK(stub.preview);
        }

        // Second tracked bump: every surface hides in the decision, and the
        // production refresh re-evaluates every surface. The bump publishes.
        const KWin::Stage5Refresh bump = leg.stage->onTrackedFrame();
        CHECK(leg.router.counter == 2);
        checkAdvancedFacts(immediateSnapshot(leg.path), inst, fixtureDir, "facts-inv2.json", "3", "2");
        const KWin::Stage5Visibility hidden = leg.stage->decide(allLiveHost());
        CHECK(!hidden.border);
        CHECK(!hidden.underlay);
        CHECK(!hidden.preview);
        stub.apply(bump, hidden);
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(!stub.preview);

        // The recorded witness-2 A drain (real broker bytes for the
        // published witness) recovers all three surfaces through the full
        // endpoint refresh: the endpoint machinery itself is intact.
        const auto drain =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-a2.json", inst), "gesture-1", "1");
        CHECK(drain.apply == KWin::PresentationApply::Accepted);
        stub.apply(drain.refresh, leg.stage->decide(gestureHost("gesture-1", "1")));
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(stub.preview);
        destroyLeg(leg);
    }

    // L2 member double-frame with the original geometry race and the preview
    // stale path: P5 displayed, first member bump publishes,
    // the still-newer pre-move P6 captured before the invalidation is
    // delivered must not paint (coverage, never mere revision), the covering
    // capture restores, the proven A preview shows, the second member bump
    // hides every surface and the production refresh re-evaluates every
    // surface, and the recorded witness-2 A drain repaints.
    {
        Leg leg;
        initLeg(leg, QStringLiteral("/com/omnitiler/ActiveBorder2"));
        SurfaceStub stub;
        seed(leg);
        const QString inst = leg.instance;
        const auto p5 = applyStr(leg, dispatchBytes(fixtureDir, "dispatch-p5.json", inst), "", "0");
        CHECK(p5.apply == KWin::PresentationApply::Accepted);
        stub.apply(p5.refresh, leg.stage->decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);

        const KWin::Stage5Refresh bump1 = leg.stage->onMemberFrame();
        CHECK(leg.router.counter == 1);
        checkAdvancedFacts(immediateSnapshot(leg.path), inst, fixtureDir, "facts-cover.json", "2", "1");
        stub.apply(bump1, leg.stage->decide(allLiveHost()));
        CHECK(!stub.underlay);
        // In-transit pre-move P6 (rev/obs newer than the displayed P5
        // fence, same old coverage) completes after the member move, before
        // any post-move capture is accepted. Coverage rejects it: no stale
        // paint, no resurrection of the hidden underlay.
        {
            const auto stale = applyStr(leg, dispatchBytes(fixtureDir, "dispatch-p6.json", inst), "", "0");
            CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
            CHECK(!stale.refresh.border);
            CHECK(!stale.refresh.repaint);
            CHECK(!stub.underlay);
        }
        const auto cover =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-cover.json", inst), "", "0");
        CHECK(cover.apply == KWin::PresentationApply::Accepted);
        stub.apply(cover.refresh, leg.stage->decide(allLiveHost()));
        const auto preview =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-preview.json", inst), "gesture-1", "1");
        CHECK(preview.apply == KWin::PresentationApply::Accepted);
        stub.apply(preview.refresh, leg.stage->decide(gestureHost("gesture-1", "1")));
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(stub.preview);

        const KWin::Stage5Refresh bump2 = leg.stage->onMemberFrame();
        CHECK(leg.router.counter == 2);
        checkAdvancedFacts(immediateSnapshot(leg.path), inst, fixtureDir, "facts-inv2.json", "3", "2");
        const KWin::Stage5Visibility hidden = leg.stage->decide(allLiveHost());
        CHECK(!hidden.border);
        CHECK(!hidden.underlay);
        CHECK(!hidden.preview);
        stub.apply(bump2, hidden);
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(!stub.preview);

        // The later recorded drain repaints (recovery leg).
        const auto drain =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-a2.json", inst), "gesture-1", "1");
        CHECK(drain.apply == KWin::PresentationApply::Accepted);
        stub.apply(drain.refresh, leg.stage->decide(gestureHost("gesture-1", "1")));
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(stub.preview);

        // Member visibility routing stays bump-free and member-gated.
        CHECK(leg.router.counter == 2);
        const KWin::Stage5Refresh member = leg.stage->onMemberVisibility(QStringLiteral("win-a"));
        CHECK(member.anchor);
        CHECK(member.underlay);
        CHECK(leg.router.counter == 2);
        const KWin::Stage5Refresh foreign = leg.stage->onMemberVisibility(QStringLiteral("zzz"));
        CHECK(!foreign.anchor);
        CHECK(leg.router.counter == 2);
        destroyLeg(leg);
    }

    // L3 gesture boundary with the original cross-gesture race, both
    // boundaries, and the valid-B repaint: the proven A preview shows while
    // A is live; finish-A drops the latch and bumps with no floors, so the
    // decision hides every surface and the production refresh re-evaluates
    // every surface. The already-sent A paint
    // is rejected inside B; the recorded witness-2 A drain still applies
    // (coverage current) but its producer mismatch isolates to the preview
    // while border/underlay repaint; start-B bumps again; and the
    // recorded valid-B capture (real broker bytes, UUID producer, coverage
    // current) repaints all three.
    {
        static constexpr char kBInstance[] = "9d4e6a2c-1b3f-4d8e-9f0a-2c5e7a1b3d4f";
        Leg leg;
        initLeg(leg, QStringLiteral("/com/omnitiler/ActiveBorder3"));
        SurfaceStub stub;
        seed(leg);
        const QString inst = leg.instance;
        const auto p5 = applyStr(leg, dispatchBytes(fixtureDir, "dispatch-p5.json", inst), "", "0");
        CHECK(p5.apply == KWin::PresentationApply::Accepted);
        stub.apply(p5.refresh, leg.stage->decide(allLiveHost()));
        // The covering capture for the earlier tracked move lands first.
        const auto track = leg.stage->onTrackedFrame();
        CHECK(leg.router.counter == 1);
        stub.apply(track, leg.stage->decide(allLiveHost()));
        checkAdvancedFacts(immediateSnapshot(leg.path), inst, fixtureDir, "facts-cover.json", "2", "1");
        const auto cover =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-cover.json", inst), "", "0");
        CHECK(cover.apply == KWin::PresentationApply::Accepted);
        stub.apply(cover.refresh, leg.stage->decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);

        // Gesture A opens: the proven preview (live gesture-1/1) shows
        // alongside the border and the underlay.
        const auto preview =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-preview.json", inst), "gesture-1", "1");
        CHECK(preview.apply == KWin::PresentationApply::Accepted);
        {
            const KWin::Stage5Visibility live = leg.stage->decide(gestureHost("gesture-1", "1"));
            CHECK(live.border);
            CHECK(live.underlay);
            CHECK(live.preview);
            stub.apply(preview.refresh, live);
            CHECK(stub.border);
            CHECK(stub.underlay);
            CHECK(stub.preview);
        }

        // Finish A: the boundary drops the latch and bumps with no floors.
        // Every surface is invalid now, and every surface re-evaluates.
        const KWin::Stage5Refresh finishA = leg.stage->onGestureBoundary();
        CHECK(leg.router.counter == 2);
        checkAdvancedFacts(immediateSnapshot(leg.path), inst, fixtureDir, "facts-inv2.json", "3", "2");
        {
            const KWin::Stage5Visibility insideB = leg.stage->decide(gestureHost(kBInstance, "1"));
            CHECK(!insideB.border);
            CHECK(!insideB.underlay);
            CHECK(!insideB.preview);
            stub.apply(finishA, insideB);
            CHECK(!stub.border);
            CHECK(!stub.underlay);
            CHECK(!stub.preview);
        }

        // Already-sent A paint re-applied inside B: stale coverage, never
        // paints in B.
        {
            const auto crossed =
                applyStr(leg, dispatchBytes(fixtureDir, "dispatch-preview.json", inst), kBInstance, "1");
            CHECK(crossed.apply == KWin::PresentationApply::StaleIgnored);
            CHECK(!crossed.refresh.preview);
            CHECK(!crossed.refresh.repaint);
            CHECK(!stub.preview);
        }

        // Newly-drained A paint read inside B with current coverage
        // (witness 2 == latest 2): the producer mismatch isolates to the
        // preview. Border and underlay still repaint from the accepted
        // projection; the preview never arms for the foreign producer.
        {
            const auto drain =
                applyStr(leg, dispatchBytes(fixtureDir, "dispatch-a2.json", inst), kBInstance, "1");
            CHECK(drain.apply == KWin::PresentationApply::Accepted);
            CHECK(drain.refresh.border);
            CHECK(drain.refresh.preview);
            stub.apply(drain.refresh, leg.stage->decide(gestureHost(kBInstance, "1")));
            CHECK(stub.border);
            CHECK(stub.underlay);
            CHECK(!stub.preview);
        }

        // Start B: the second boundary bumps again.
        const KWin::Stage5Refresh startB = leg.stage->onGestureBoundary();
        CHECK(leg.router.counter == 3);
        checkAdvancedFacts(immediateSnapshot(leg.path), inst, fixtureDir, "facts-inv3.json", "4", "3");
        {
            const KWin::Stage5Visibility insideB = leg.stage->decide(gestureHost(kBInstance, "1"));
            CHECK(!insideB.border);
            CHECK(!insideB.underlay);
            CHECK(!insideB.preview);
            stub.apply(startB, insideB);
            CHECK(!stub.border);
            CHECK(!stub.underlay);
            CHECK(!stub.preview);
        }

        // Newly-drained A paint read inside B with current coverage
        // (witness 3 == latest 3): still accepted for border/underlay with
        // the preview isolated.
        {
            const auto drain =
                applyStr(leg, dispatchBytes(fixtureDir, "dispatch-a3.json", inst), kBInstance, "1");
            CHECK(drain.apply == KWin::PresentationApply::Accepted);
            stub.apply(drain.refresh, leg.stage->decide(gestureHost(kBInstance, "1")));
            CHECK(stub.border);
            CHECK(stub.underlay);
            CHECK(!stub.preview);
        }

        // Valid-B repaint: the recorded witness-3 capture with the UUID
        // producer matching the live B gesture repaints all three.
        {
            const auto valid =
                applyStr(leg, dispatchBytes(fixtureDir, "dispatch-preview-b.json", inst), kBInstance, "1");
            CHECK(valid.apply == KWin::PresentationApply::Accepted);
            stub.apply(valid.refresh, leg.stage->decide(gestureHost(kBInstance, "1")));
            CHECK(stub.border);
            CHECK(stub.underlay);
            CHECK(stub.preview);
        }
        destroyLeg(leg);
    }

    // L4 focus clear with the original in-transit race, the return-to-A
    // second activation, and the error-hide fence: show A, activate B
    // (display-only hide plus the all-scope bump, every surface refreshed),
    // deliver the in-transit old A read before the post-activation capture
    // (must not resurrect), return to A through the recorded witness-2
    // covering capture (repaints), then drive the error-hide path (malformed
    // hides display-only with the fence retained; a further old A paint
    // still cannot resurrect). Both activation bumps publish.
    {
        Leg leg;
        initLeg(leg, QStringLiteral("/com/omnitiler/ActiveBorder4"));
        SurfaceStub stub;
        seed(leg);
        const QString inst = leg.instance;
        const auto p5 = applyStr(leg, dispatchBytes(fixtureDir, "dispatch-p5.json", inst), "", "0");
        CHECK(p5.apply == KWin::PresentationApply::Accepted);
        stub.apply(p5.refresh, leg.stage->decide(allLiveHost()));
        CHECK(stub.border);

        const KWin::Stage5Refresh activation = leg.stage->onActivation();
        CHECK(leg.router.counter == 1);
        checkAdvancedFacts(immediateSnapshot(leg.path), inst, fixtureDir, "facts-cover.json", "2", "1");
        CHECK(!leg.router.paint.has);
        CHECK(leg.router.paint.fence.set);
        stub.apply(activation, leg.stage->decide(allLiveHost()));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        CHECK(!stub.preview);

        // In-transit old A paint (newer rev/obs than P5, stale coverage)
        // delivered after the activation, before any post-activation
        // capture: must not resurrect the cleared paint.
        const auto stale = applyStr(leg, dispatchBytes(fixtureDir, "dispatch-p6.json", inst), "", "0");
        CHECK(stale.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stale.refresh.border);
        CHECK(!stale.refresh.repaint);
        CHECK(!stub.border);

        // First covering capture repaints; the fence advances to it.
        const auto cover =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-cover.json", inst), "", "0");
        CHECK(cover.apply == KWin::PresentationApply::Accepted);
        stub.apply(cover.refresh, leg.stage->decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(leg.router.paint.fence.set);
        CHECK(leg.router.paint.fence.rev == 3);

        // Return to A: the second activation hides display-only again plus
        // the all-scope bump (published); only the recorded
        // witness-2 covering capture repaints.
        const KWin::Stage5Refresh activation2 = leg.stage->onActivation();
        CHECK(leg.router.counter == 2);
        checkAdvancedFacts(immediateSnapshot(leg.path), inst, fixtureDir, "facts-inv2.json", "3", "2");
        stub.apply(activation2, leg.stage->decide(allLiveHost()));
        CHECK(!stub.border);
        CHECK(!stub.underlay);
        // In-transit pre-return paint (rev/obs newer than the cover fence,
        // stale coverage) delivered after the second activation, before the
        // return capture: must not resurrect the cleared paint.
        const auto stale2 = applyStr(leg, dispatchBytes(fixtureDir, "dispatch-preview.json", inst), "", "0");
        CHECK(stale2.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stale2.refresh.border);
        CHECK(!stale2.refresh.repaint);
        CHECK(!stub.border);
        const auto cover2 =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-cover2.json", inst), "", "0");
        CHECK(cover2.apply == KWin::PresentationApply::Accepted);
        stub.apply(cover2.refresh, leg.stage->decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(leg.router.paint.fence.set);
        CHECK(leg.router.paint.fence.rev == 5);

        // Error-hide path: malformed clears display-only with a full
        // refresh while the fence survives; a further old A paint still
        // cannot resurrect.
        const auto malformed = applyStr(leg, QStringLiteral("{}"), "", "0");
        CHECK(malformed.apply == KWin::PresentationApply::Malformed);
        CHECK(malformed.refresh.border);
        CHECK(leg.router.paint.fence.set);
        CHECK(leg.router.paint.fence.rev == 5);
        stub.apply(malformed.refresh, leg.stage->decide(allLiveHost()));
        CHECK(!stub.border);
        const auto afterError = applyStr(leg, dispatchBytes(fixtureDir, "dispatch-p6.json", inst), "", "0");
        CHECK(afterError.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stub.border);

        // A foreign witness instance never applies (reused counters across
        // instances never compare).
        QString foreign = dispatchBytes(fixtureDir, "dispatch-cover2.json", inst);
        foreign.replace(inst, QStringLiteral("other-instance"));
        const auto foreignCover = applyStr(leg, foreign, "", "0");
        CHECK(foreignCover.apply == KWin::PresentationApply::StaleIgnored);
        CHECK(!stub.border);
        destroyLeg(leg);
    }

    // L5 rapid same-source gesture serial handover: the proven A preview
    // shows while A (gesture-1/1) is live; B (gesture-1/2, same native
    // source) goes live with no covering capture between them. The
    // already-produced A paint delivered while B is live must not show its
    // preview (coverage is still current: the serial alone isolates it),
    // while border and underlay still repaint from the accepted projection.
    // The start-B boundary drops the latch and bumps with no floors, so the
    // decision hides every surface and the production refresh re-evaluates
    // every surface. The recorded serial-B
    // covering capture (real broker bytes for the published witness, B
    // producer, coverage current) repaints all three through the full
    // endpoint refresh.
    {
        Leg leg;
        initLeg(leg, QStringLiteral("/com/omnitiler/ActiveBorder5"));
        SurfaceStub stub;
        seed(leg);
        const QString inst = leg.instance;
        const auto p5 = applyStr(leg, dispatchBytes(fixtureDir, "dispatch-p5.json", inst), "", "0");
        CHECK(p5.apply == KWin::PresentationApply::Accepted);
        stub.apply(p5.refresh, leg.stage->decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);

        // First tracked bump publishes; the covering capture
        // restores, and the proven A preview shows while A is live.
        const KWin::Stage5Refresh bump1 = leg.stage->onTrackedFrame();
        CHECK(leg.router.counter == 1);
        checkAdvancedFacts(immediateSnapshot(leg.path), inst, fixtureDir, "facts-cover.json", "2", "1");
        stub.apply(bump1, leg.stage->decide(allLiveHost()));
        CHECK(!stub.border);
        const auto cover =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-cover.json", inst), "", "0");
        CHECK(cover.apply == KWin::PresentationApply::Accepted);
        stub.apply(cover.refresh, leg.stage->decide(allLiveHost()));
        CHECK(stub.border);
        CHECK(stub.underlay);
        const auto preview =
            applyStr(leg, dispatchBytes(fixtureDir, "dispatch-preview.json", inst), "gesture-1", "1");
        CHECK(preview.apply == KWin::PresentationApply::Accepted);
        stub.apply(preview.refresh, leg.stage->decide(gestureHost("gesture-1", "1")));
        CHECK(stub.border);
        CHECK(stub.underlay);
        CHECK(stub.preview);

        // Rapid handover: B (same instance, serial 2) is live before any
        // covering capture. Re-delivering the proven A paint (coverage
        // still current, equal order re-applies) must not show the A
        // preview: the serial alone isolates it, while border and underlay
        // still repaint from the accepted projection.
        {
            const auto crossed =
                applyStr(leg, dispatchBytes(fixtureDir, "dispatch-preview.json", inst), "gesture-1", "2");
            CHECK(crossed.apply == KWin::PresentationApply::Accepted);
            CHECK(crossed.refresh.preview);
            stub.apply(crossed.refresh, leg.stage->decide(gestureHost("gesture-1", "2")));
            CHECK(stub.border);
            CHECK(stub.underlay);
            CHECK(!stub.preview);
        }

        // Start B: the boundary drops the latch and bumps with no floors
        // (published). Every surface is invalid now, and every surface
        // re-evaluates.
        const KWin::Stage5Refresh startB = leg.stage->onGestureBoundary();
        CHECK(leg.router.counter == 2);
        checkAdvancedFacts(immediateSnapshot(leg.path), inst, fixtureDir, "facts-inv2.json", "3", "2");
        {
            const KWin::Stage5Visibility insideB = leg.stage->decide(gestureHost("gesture-1", "2"));
            CHECK(!insideB.border);
            CHECK(!insideB.underlay);
            CHECK(!insideB.preview);
            stub.apply(startB, insideB);
            CHECK(!stub.border);
            CHECK(!stub.underlay);
            CHECK(!stub.preview);
        }

        // Covering serial-B capture (real broker bytes for the published
        // witness, B producer): repaints all three through the full
        // endpoint refresh.
        {
            const auto valid =
                applyStr(leg, dispatchBytes(fixtureDir, "dispatch-s2.json", inst), "gesture-1", "2");
            CHECK(valid.apply == KWin::PresentationApply::Accepted);
            stub.apply(valid.refresh, leg.stage->decide(gestureHost("gesture-1", "2")));
            CHECK(stub.border);
            CHECK(stub.underlay);
            CHECK(stub.preview);
        }
        destroyLeg(leg);
    }

    g_script.unregisterService(QStringLiteral("org.kde.KWin"));
    QDBusConnection::disconnectFromBus(QStringLiteral("stage5-effect"));

    if (failures != 0) {
        std::fprintf(stderr, "stage5 recovery test: %d failure(s)\n", failures);
        return 1;
    }
    std::printf("stage5 recovery test: all checks passed\n");
    return 0;
}
