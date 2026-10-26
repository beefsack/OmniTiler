// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
//
// Offline transport test for the versioned script facts waiter: the exact
// production PresentationFactsSource class (no test fake) driven over a
// real private bus. Proves the production topology (effect-owned dedicated
// connection plus the script's sessionBus caller), immediate bootstrap,
// change wake, idle lease, supersession, foreign-caller denial without
// displacement, malformed/oversize no-mutation, instance fencing, and
// shutdown invalidation.
//
// The harness provides the private bus (run the binary under
// dbus-run-session, or any private bus via DBUS_SESSION_BUS_ADDRESS);
// no KWin. The only timer is the production waiter's own bounded lease,
// exercised by the idle-lease leg.
//
// A separate same-connection leg pins the Qt local-loop limit explicitly:
// a held waiter with caller and owner on one connection is rejected
// outright (InternalError), which is why production keeps the dedicated
// effect connection. That leg never counts as acceptance.

#include "presentationfacts.h"

#include <QCoreApplication>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusError>
#include <QDBusMessage>
#include <QDBusPendingCall>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QDateTime>
#include <QJsonDocument>
#include <QJsonObject>
#include <QProcess>
#include <QTemporaryFile>
#include <QTextStream>
#include <QTimer>
#include <optional>

#include <cstdio>
#include <cstdlib>
#include <functional>
#include <vector>

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

QJsonObject parsePayload(const QString &payload)
{
    const QJsonDocument doc = QJsonDocument::fromJson(payload.toUtf8());
    CHECK(doc.isObject());
    return doc.object();
}

struct Fixture {
    QCoreApplication *app = nullptr;
    std::optional<QDBusConnection> effect;
    std::optional<QDBusConnection> script;
    std::optional<QDBusConnection> foreign;
    KWin::PresentationFactsSource *source = nullptr;
    QString instance;
    quint64 rev = 0;
    std::vector<std::function<void()>> steps;
    QTimer watchdog;

    void next()
    {
        if (steps.empty()) {
            app->quit();
            return;
        }
        auto step = steps.front();
        steps.erase(steps.begin());
        step();
    }

    void fail(const char *expression, const char *file, int line)
    {
        check(false, expression, file, line);
        app->exit(1);
    }
};

#define TCHECK(fixture, expression) \
    do { \
        if (!(expression)) { \
            (fixture)->fail(#expression, __FILE__, __LINE__); \
            return; \
        } \
    } while (0)

struct Caller {
    QDBusConnection conn;
    QString dest;
    QString path = QStringLiteral("/com/omnitiler/ActiveBorder");
};

QDBusPendingCallWatcher *asyncWait(const Caller &caller, const QString &request)
{
    // Raw dynamic call on purpose: QDBusInterface performs a blocking
    // introspection round-trip on first use, which stalls single-threaded
    // cross-connection tests for the 25 s call timeout. The production
    // caller (KWin JS callDBus) is dynamic too, so nothing is lost.
    QDBusMessage msg = QDBusMessage::createMethodCall(caller.dest, caller.path,
        QStringLiteral("com.omnitiler.ActiveBorder1"), QStringLiteral("WaitPresentationFacts"));
    msg.setArguments({request});
    QDBusPendingCall call = caller.conn.asyncCall(msg);
    auto *watcher = new QDBusPendingCallWatcher(call);
    return watcher;
}

QString waitRequest(const QString &instance, const QString &rev, int leaseMs)
{
    return QStringLiteral("{\"instance\":\"%1\",\"rev\":\"%2\",\"leaseMs\":%3}").arg(instance, rev).arg(leaseMs);
}

} // namespace

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    Fixture f;
    f.app = &app;

    f.watchdog.setSingleShot(true);
    QObject::connect(&f.watchdog, &QTimer::timeout, &app, [&]() {
        std::fprintf(stderr, "FAIL: transport test watchdog fired\n");
        ++failures;
        app.exit(1);
    });
    f.watchdog.start(60000);

    // Same-connection diagnostic (NOT the production topology): the Qt
    // local-loop rule forbids delayed replies when caller and owner share
    // one connection (org.qtproject.QtDBus.Error.InternalError), so a held
    // waiter is rejected outright here. Immediate replies work. This leg
    // pins the limitation only; acceptance comes from the dedicated-leg
    // wake/lease path below against the identical production class. The
    // held attempt fails fast (no lease wait), keeping the suite bounded.
    KWin::PresentationFactsSource *sharedSource = nullptr;
    QString sharedInstance;
    std::optional<Caller> sharedCaller;
    f.steps.push_back([&]() {
        auto session = QDBusConnection::sessionBus();
        TCHECK(&f, session.isConnected());
        TCHECK(&f, session.interface()->registerService(QStringLiteral("org.kde.KWin")));
        sharedSource = new KWin::PresentationFactsSource(session, &app);
        // Production seeds every fresh source before registration: the
        // stand-in real style triple here, like the effect's config read.
        sharedSource->noteStyle(0.0, 3.0, -1.0);
        TCHECK(&f, session.registerObject(QStringLiteral("/com/omnitiler/ActiveBorder"), sharedSource,
                                           QDBusConnection::ExportScriptableContents));
        sharedCaller.emplace(Caller{session, session.baseService()});
        auto *watcher = asyncWait(*sharedCaller, waitRequest(QString(), QString(), 20000));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("v")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("available")).toInt(-1) == 1);
            sharedInstance = body.value(QStringLiteral("instance")).toString();
            TCHECK(&f, !sharedInstance.isEmpty());
            TCHECK(&f, body.value(QStringLiteral("rev")).toString() == QStringLiteral("0"));
            f.next();
        });
    });
    f.steps.push_back([&]() {
        // A held waiter on the shared connection is rejected outright by
        // Qt: this is the local-loop limit, not a product decision. The
        // failed hold must mutate nothing: a follow-up unknown-identity
        // Wait still resolves against the unchanged instance and revision.
        auto *watcher = asyncWait(*sharedCaller, waitRequest(sharedInstance, QStringLiteral("0"), 20000));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, !reply.isValid());
            TCHECK(&f, reply.error().type() == QDBusError::InternalError);
            auto *probe = asyncWait(*sharedCaller, waitRequest(QStringLiteral("unknown-instance"), QStringLiteral("9"), 20000));
            QObject::connect(probe, &QDBusPendingCallWatcher::finished, &app, [&, probe]() {
                QDBusPendingReply<QString> probeReply = *probe;
                probe->deleteLater();
                TCHECK(&f, probeReply.isValid());
                const QJsonObject probeBody = parsePayload(probeReply.value());
                TCHECK(&f, probeBody.value(QStringLiteral("changed")).toInt(-1) == 1);
                TCHECK(&f, probeBody.value(QStringLiteral("instance")).toString() == sharedInstance);
                TCHECK(&f, probeBody.value(QStringLiteral("rev")).toString() == QStringLiteral("0"));
                auto session = QDBusConnection::sessionBus();
                session.unregisterObject(QStringLiteral("/com/omnitiler/ActiveBorder"));
                TCHECK(&f, session.unregisterService(QStringLiteral("org.kde.KWin")));
                delete sharedSource;
                sharedSource = nullptr;
                f.next();
            });
        });
    });

    // Step 0: production topology. The effect owns a named dedicated
    // connection to the same bus (like the effect's
    // omnitiler-active-border-endpoint handle); the script calls via
    // QDBusConnection::sessionBus, exactly like the KWin script route.
    // The well-known ActiveBorder name registers on the effect connection;
    // org.kde.KWin registers on the script (sessionBus) connection, so the
    // owner gate sees the script sender as the live KWin owner globally.
    f.steps.push_back([&]() {
        const QString address = QString::fromLocal8Bit(qgetenv("DBUS_SESSION_BUS_ADDRESS"));
        if (address.isEmpty()) {
            std::fprintf(stderr, "FAIL: test needs DBUS_SESSION_BUS_ADDRESS (run under dbus-run-session)\n");
            ++failures;
            app.exit(1);
            return;
        }
        f.effect.emplace(QDBusConnection::connectToBus(address, QStringLiteral("facts-effect-dedicated")));
        f.script.emplace(QDBusConnection::sessionBus());
        f.foreign.emplace(QDBusConnection::connectToBus(address, QStringLiteral("facts-foreign")));
TCHECK(&f, f.effect->isConnected() && f.script->isConnected() && f.foreign->isConnected());
        TCHECK(&f, f.effect->name() != f.script->name());
        f.source = new KWin::PresentationFactsSource(*f.effect, &app);
        // Production seeds every fresh source before registration (actual
        // config values in the effect; the stand-in real triple here), so
        // the first served snapshot is already available:true.
        f.source->noteStyle(0.0, 3.0, -1.0);
        TCHECK(&f, f.effect->registerObject(QStringLiteral("/com/omnitiler/ActiveBorder"), f.source,
                                            QDBusConnection::ExportScriptableContents));
        TCHECK(&f, f.effect->interface()->registerService(QStringLiteral("com.omnitiler.ActiveBorder")));
        // The script route runs on KWin's own session connection, which
        // owns the KWin name: the script peer takes that name here, exactly
        // like production, so its sender matches the live bus owner.
        TCHECK(&f, f.script->interface()->registerService(QStringLiteral("org.kde.KWin")));
        f.next();
    });

    std::optional<Caller> scriptCaller;
    std::optional<Caller> foreignCaller;
    auto makeIfaces = [&]() {
        scriptCaller.emplace(Caller{*f.script, f.effect->baseService()});
        foreignCaller.emplace(Caller{*f.foreign, f.effect->baseService()});
    };

    // Preseed leg: a fresh source with no style seed must not claim
    // available:true (no guessed defaults). The first real style push
    // flips it to available:true with no revision bump (baseline arm).
    KWin::PresentationFactsSource *probeSource = nullptr;
    std::optional<Caller> probeCaller;
    QString probeInstance;
    f.steps.push_back([&]() {
        makeIfaces();
        probeSource = new KWin::PresentationFactsSource(*f.effect, &app);
        TCHECK(&f, f.effect->registerObject(QStringLiteral("/com/omnitiler/ActiveBorderProbe"), probeSource,
                                            QDBusConnection::ExportScriptableContents));
        probeCaller.emplace(Caller{*f.script, f.effect->baseService(),
                                        QStringLiteral("/com/omnitiler/ActiveBorderProbe")});
        auto *watcher = asyncWait(*probeCaller, waitRequest(QString(), QString(), 20000));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("available")).toInt(-1) == 0);
            probeInstance = body.value(QStringLiteral("instance")).toString();
            TCHECK(&f, !probeInstance.isEmpty());
            TCHECK(&f, body.value(QStringLiteral("rev")).toString() == QStringLiteral("0"));
            probeSource->noteStyle(0.0, 3.0, -1.0);
            TCHECK(&f, probeSource->revision() == 0);
            f.next();
        });
    });
    f.steps.push_back([&]() {
        auto *watcher = asyncWait(*probeCaller, waitRequest(probeInstance, QStringLiteral("0"), 1000));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("available")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("rev")).toString() == QStringLiteral("0"));
            f.effect->unregisterObject(QStringLiteral("/com/omnitiler/ActiveBorderProbe"));
            delete probeSource;
            probeSource = nullptr;
            f.next();
        });
    });

    // Step 1: immediate bootstrap on unknown identity.

    f.steps.push_back([&]() {
        makeIfaces();
        auto *watcher = asyncWait(*scriptCaller, waitRequest(QString(), QString(), 20000));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("v")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("superseded")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("available")).toInt(-1) == 1);
            f.instance = body.value(QStringLiteral("instance")).toString();
            TCHECK(&f, !f.instance.isEmpty());
            TCHECK(&f, body.value(QStringLiteral("rev")).toString() == QStringLiteral("0"));
            // The fresh source starts with a zero invalidation witness: the
            // first capture covers it and paints.
            TCHECK(&f, body.value(QStringLiteral("invalidation")).toString() == QStringLiteral("0"));
            TCHECK(&f, body.value(QStringLiteral("chord")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("chordKnown")).toInt(-1) == 0);
            f.rev = 0;
            // Re-seed with the creation triple: no material change, so no
            // bump (later noteStyle calls with new values are the material
            // changes the wake legs prove).
            f.source->noteStyle(0.0, 3.0, -1.0);
            TCHECK(&f, f.source->revision() == 0);
            f.next();
        });
    });

    // Step 2: change wake (chord flip completes a held waiter, rev +1).
    f.steps.push_back([&]() {
        auto *watcher = asyncWait(*scriptCaller, waitRequest(f.instance, QStringLiteral("0"), 20000));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("superseded")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("rev")).toString() == QStringLiteral("1"));
            TCHECK(&f, body.value(QStringLiteral("chord")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("chordKnown")).toInt(-1) == 1);
            TCHECK(&f, f.source->revision() == 1);
            f.rev = 1;
            f.next();
        });
        QTimer::singleShot(100, &app, [&]() {
            f.source->noteChord(true, true);
        });
    });

    // Step 3: idle lease (matching identity holds, then completes
    // unchanged with the same rev). Idle expiry is not teardown: the
    // seeded endpoint still reads available.
    f.steps.push_back([&]() {
        const qint64 startMs = QDateTime::currentMSecsSinceEpoch();
        auto *watcher = asyncWait(*scriptCaller, waitRequest(f.instance, QStringLiteral("1"), 1000));
        QTimer::singleShot(300, &app, [&, watcher]() {
            if (watcher->isFinished()) {
                f.fail("lease must hold the waiter", __FILE__, __LINE__);
                return;
            }
        });
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher, startMs]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("superseded")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("available")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("rev")).toString() == QStringLiteral("1"));
            TCHECK(&f, QDateTime::currentMSecsSinceEpoch() - startMs >= 500);
            f.next();
        });
    });

    // Step 4: supersession (a newer Wait completes the pending one as
    // superseded; the replacement stays held, then wakes on change).
    f.steps.push_back([&]() {
        auto *first = asyncWait(*scriptCaller, waitRequest(f.instance, QStringLiteral("1"), 20000));
        auto *second = asyncWait(*scriptCaller, waitRequest(f.instance, QStringLiteral("1"), 20000));
        QObject::connect(first, &QDBusPendingCallWatcher::finished, &app, [&, first, second]() {
            QDBusPendingReply<QString> reply = *first;
            first->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("superseded")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("available")).toInt(-1) == 1);
            TCHECK(&f, !second->isFinished());
            f.source->noteStyle(0.0, 5.0, -1.0);
        });
        QObject::connect(second, &QDBusPendingCallWatcher::finished, &app, [&, second]() {
            QDBusPendingReply<QString> reply = *second;
            second->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("superseded")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("rev")).toString() == QStringLiteral("2"));
            TCHECK(&f, body.value(QStringLiteral("width")).toDouble(-1.0) == 5.0);
            TCHECK(&f, f.source->revision() == 2);
            f.rev = 2;
            f.next();
        });
    });

    // Step 5: foreign caller denied without displacement.
    f.steps.push_back([&]() {
        auto *held = asyncWait(*scriptCaller, waitRequest(f.instance, QStringLiteral("2"), 20000));
        auto *hostile = asyncWait(*foreignCaller, waitRequest(QString(), QString(), 20000));
        QObject::connect(hostile, &QDBusPendingCallWatcher::finished, &app, [&, hostile, held]() {
            QDBusPendingReply<QString> reply = *hostile;
            hostile->deleteLater();
            TCHECK(&f, reply.isError());
            TCHECK(&f, reply.error().type() == QDBusError::AccessDenied);
            TCHECK(&f, !held->isFinished());
            TCHECK(&f, f.source->revision() == 2);
            f.source->noteChord(false, true);
        });
        QObject::connect(held, &QDBusPendingCallWatcher::finished, &app, [&, held]() {
            QDBusPendingReply<QString> reply = *held;
            held->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("rev")).toString() == QStringLiteral("3"));
            f.rev = 3;
            f.next();
        });
    });

    // Step 5b: invalidation bump (Stage 5 witness): the effect advances its
    // monotonic counter on a native event; the held waiter completes with
    // the fresh witness (changed:1) plus a revision bump, so the script
    // carries the new covered value on its next capture.
    f.steps.push_back([&]() {
        auto *watcher = asyncWait(*scriptCaller, waitRequest(f.instance, QStringLiteral("3"), 20000));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("invalidation")).toString() == QStringLiteral("1"));
            TCHECK(&f, body.value(QStringLiteral("rev")).toString() == QStringLiteral("4"));
            TCHECK(&f, f.source->revision() == 4);
            f.rev = 4;
            f.next();
        });
        QTimer::singleShot(100, &app, [&]() {
            f.source->noteInvalidation(1);
        });
    });

    // Step 6: malformed request completes immediately with no mutation.
    f.steps.push_back([&]() {
        auto *watcher = asyncWait(*scriptCaller, QStringLiteral("not json{{{"));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("rev")).toString() == QStringLiteral("4"));
            TCHECK(&f, f.source->revision() == 4);
            f.next();
        });
    });

    // Step 7: oversized request refused without mutation or hold.
    f.steps.push_back([&]() {
        auto *watcher = asyncWait(*scriptCaller, QString(5000, QLatin1Char('x')));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isError());
            TCHECK(&f, reply.error().type() == QDBusError::InvalidArgs);
            TCHECK(&f, f.source->revision() == 4);
            // No waiter was held: a fresh matching Wait holds again.
            auto *probe = asyncWait(*scriptCaller, waitRequest(f.instance, QStringLiteral("4"), 20000));
            QTimer::singleShot(200, &app, [&, probe]() {
                TCHECK(&f, !probe->isFinished());
                probe->deleteLater();
                f.next();
            });
        });
    });

    // Step 8: shutdown completes a pending waiter as explicitly
    // unavailable; the client hides immediately instead of holding the
    // dead instance through its lease. Teardown mirrors production: the
    // object and well-known name release on the exact owned connection,
    // then that named connection disconnects; the script connection is
    // never unregistered from here.
    f.steps.push_back([&]() {
        auto *watcher = asyncWait(*scriptCaller, waitRequest(f.instance, QStringLiteral("4"), 20000));
        QTimer::singleShot(100, &app, [&, watcher]() {
            delete f.source;
            f.source = nullptr;
        });
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("superseded")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("available")).toInt(-1) == 0);
            f.effect->unregisterObject(QStringLiteral("/com/omnitiler/ActiveBorder"));
            TCHECK(&f, f.effect->unregisterService(QStringLiteral("com.omnitiler.ActiveBorder")));
            QDBusConnection::disconnectFromBus(QStringLiteral("facts-effect-dedicated"));
            QDBusConnection::disconnectFromBus(QStringLiteral("facts-foreign"));
            f.script->unregisterService(QStringLiteral("org.kde.KWin"));
            f.next();
        });
    });

    f.steps.push_back([&]() {
        TCHECK(&f, failures == 0);
        app.quit();
    });

    QTimer::singleShot(0, &app, [&]() {
        f.next();
    });
    const int code = app.exec();
    if (code != 0) {
        return code;
    }
    if (failures != 0) {
        std::fprintf(stderr, "%d check(s) failed\n", failures);
        return EXIT_FAILURE;
    }
    std::printf("all checks passed\n");
    return EXIT_SUCCESS;
}
