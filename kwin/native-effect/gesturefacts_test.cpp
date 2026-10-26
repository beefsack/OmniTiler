// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
//
// Offline transport test for the authoritative gesture lifecycle source:
// the exact production GestureFactsSource class (no test fake) driven over
// a real private bus in the production topology (effect-owned dedicated
// connection plus the script's sessionBus caller). Proves immediate
// bootstrap, open hold plus cursor wake, fast terminal-before-pull
// delivery (Cancelled, never a premature Released), consumed-Esc Released,
// unacked-terminal retention across newer starts with exact-identity ack,
// abort without invented terminals, supersession, foreign-caller denial
// without displacement, malformed/oversize no-mutation, idle lease, and
// shutdown invalidation.
//
// The harness provides the private bus (run the binary under
// dbus-run-session, or any private bus via DBUS_SESSION_BUS_ADDRESS);
// no KWin. The only timers are the production waiter's own bounded lease
// and the test watchdog.

#include "gesturefacts.h"

#include <QCoreApplication>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusError>
#include <QDBusMessage>
#include <QDBusPendingCall>
#include <QDBusPendingCallWatcher>
#include <QDBusPendingReply>
#include <QDateTime>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QTimer>

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
    KWin::GestureFactsSource *source = nullptr;
    QString instance;
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
    QString path = QStringLiteral("/com/omnitiler/Gesture");
};

QDBusPendingCallWatcher *asyncWaitGesture(const Caller &caller, const QString &request)
{
    QDBusMessage msg = QDBusMessage::createMethodCall(caller.dest, caller.path,
        QStringLiteral("com.omnitiler.Gesture1"), QStringLiteral("WaitGesture"));
    msg.setArguments({request});
    QDBusPendingCall call = caller.conn.asyncCall(msg);
    return new QDBusPendingCallWatcher(call);
}

QDBusPendingCallWatcher *asyncLastGesture(const Caller &caller)
{
    QDBusMessage msg = QDBusMessage::createMethodCall(caller.dest, caller.path,
        QStringLiteral("com.omnitiler.Gesture1"), QStringLiteral("LastGesture"));
    QDBusPendingCall call = caller.conn.asyncCall(msg);
    return new QDBusPendingCallWatcher(call);
}

QDBusPendingCallWatcher *asyncAckGesture(const Caller &caller, const QString &request)
{
    QDBusMessage msg = QDBusMessage::createMethodCall(caller.dest, caller.path,
        QStringLiteral("com.omnitiler.Gesture1"), QStringLiteral("AckGesture"));
    msg.setArguments({request});
    QDBusPendingCall call = caller.conn.asyncCall(msg);
    return new QDBusPendingCallWatcher(call);
}

QString gestureRequest(const QString &instance, const QString &serial, int leaseMs)
{
    return QStringLiteral("{\"instance\":\"%1\",\"serial\":\"%2\",\"leaseMs\":%3}").arg(instance, serial).arg(leaseMs);
}

QString gestureRequestRev(const QString &instance, const QString &serial, const QString &revision, int leaseMs)
{
    return QStringLiteral("{\"instance\":\"%1\",\"serial\":\"%2\",\"revision\":\"%3\",\"leaseMs\":%4}")
        .arg(instance, serial, revision)
        .arg(leaseMs);
}

QString currentRevision(KWin::GestureFactsSource *source)
{
    return QString::number(source->revision());
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
    f.watchdog.start(90000);

    // Production topology: the effect owns a named dedicated connection to
    // the same bus; the script calls via sessionBus, exactly like the KWin
    // script route. The well-known Gesture name registers on the effect
    // connection; org.kde.KWin registers on the script connection, so the
    // owner gate sees the script sender as the live KWin owner globally.
    f.steps.push_back([&]() {
        const QString address = QString::fromLocal8Bit(qgetenv("DBUS_SESSION_BUS_ADDRESS"));
        if (address.isEmpty()) {
            std::fprintf(stderr, "FAIL: test needs DBUS_SESSION_BUS_ADDRESS (run under dbus-run-session)\n");
            ++failures;
            app.exit(1);
            return;
        }
        f.effect.emplace(QDBusConnection::connectToBus(address, QStringLiteral("gesture-effect-dedicated")));
        f.script.emplace(QDBusConnection::sessionBus());
        f.foreign.emplace(QDBusConnection::connectToBus(address, QStringLiteral("gesture-foreign")));
        TCHECK(&f, f.effect->isConnected() && f.script->isConnected() && f.foreign->isConnected());
        TCHECK(&f, f.effect->name() != f.script->name());
        f.source = new KWin::GestureFactsSource(*f.effect, &app);
        f.instance = f.source->instance();
        TCHECK(&f, !f.instance.isEmpty());
        TCHECK(&f, f.effect->registerObject(QStringLiteral("/com/omnitiler/Gesture"), f.source,
                                            QDBusConnection::ExportScriptableContents));
        TCHECK(&f, f.effect->interface()->registerService(QStringLiteral("com.omnitiler.Gesture")));
        TCHECK(&f, f.script->interface()->registerService(QStringLiteral("org.kde.KWin")));
        f.next();
    });

    std::optional<Caller> scriptCaller;
    std::optional<Caller> foreignCaller;
    auto makeIfaces = [&]() {
        scriptCaller.emplace(Caller{*f.script, f.effect->baseService()});
        foreignCaller.emplace(Caller{*f.foreign, f.effect->baseService()});
    };

    // Step 1: immediate bootstrap on unknown identity: idle names no
    // gesture (serial "0"), never a fabricated terminal.
    f.steps.push_back([&]() {
        makeIfaces();
        auto *watcher = asyncWaitGesture(*scriptCaller, gestureRequest(QString(), QString(), 20000));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("v")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("superseded")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("available")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("instance")).toString() == f.instance);
            TCHECK(&f, body.value(QStringLiteral("serial")).toString() == QStringLiteral("0"));
            TCHECK(&f, body.value(QStringLiteral("revision")).toString() == QStringLiteral("0"));
            TCHECK(&f, body.value(QStringLiteral("phase")).toString() == QStringLiteral("idle"));
            TCHECK(&f, body.value(QStringLiteral("win")).toString().isEmpty());
            f.next();
        });
    });

    // Step 2: an open gesture with that exact identity and revision holds;
    // a cursor update wakes it with the updated snapshot.
    f.steps.push_back([&]() {
        TCHECK(&f, f.source->gestureStarted(QStringLiteral("win-1"), QStringLiteral("move"),
                                            QRect(10, 20, 300, 200), true, QPoint(11, 21), true) == 1);
        auto *watcher = asyncWaitGesture(*scriptCaller, gestureRequestRev(f.instance, QStringLiteral("1"), currentRevision(f.source), 20000));
        QTimer::singleShot(300, &app, [&, watcher]() {
            TCHECK(&f, !watcher->isFinished());
            TCHECK(&f, f.source->gestureCursor(QStringLiteral("win-1"), true, QPoint(60, 70)));
        });
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("serial")).toString() == QStringLiteral("1"));
            TCHECK(&f, body.value(QStringLiteral("revision")).toString() == currentRevision(f.source));
            TCHECK(&f, body.value(QStringLiteral("phase")).toString() == QStringLiteral("updated"));
            TCHECK(&f, body.value(QStringLiteral("win")).toString() == QStringLiteral("win-1"));
            TCHECK(&f, body.value(QStringLiteral("kind")).toString() == QStringLiteral("move"));
            f.next();
        });
    });

    // Step 3: fast start/Esc/finish before the first pull delivers
    // Cancelled with the frozen start facts, never a premature Released.
    // A Wait naming the open identity afterwards completes immediately
    // with the terminal (a terminal never holds).
    f.steps.push_back([&]() {
        f.source->armEsc();
        TCHECK(&f, f.source->gestureFinished(QStringLiteral("win-1"), true, QPoint(60, 70)) == KWin::GesturePhase::Cancelled);
        auto *pull = asyncLastGesture(*scriptCaller);
        QObject::connect(pull, &QDBusPendingCallWatcher::finished, &app, [&, pull]() {
            QDBusPendingReply<QString> reply = *pull;
            pull->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("serial")).toString() == QStringLiteral("1"));
            TCHECK(&f, body.value(QStringLiteral("phase")).toString() == QStringLiteral("cancelled"));
            TCHECK(&f, body.value(QStringLiteral("win")).toString() == QStringLiteral("win-1"));
            TCHECK(&f, body.value(QStringLiteral("kind")).toString() == QStringLiteral("move"));
            const QJsonArray startRect = body.value(QStringLiteral("startRect")).toArray();
            TCHECK(&f, startRect.size() == 4 && startRect.at(0).toInt(-1) == 10 && startRect.at(2).toInt(-1) == 300);
            auto *watcher = asyncWaitGesture(*scriptCaller, gestureRequestRev(f.instance, QStringLiteral("1"), currentRevision(f.source), 20000));
            QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
                QDBusPendingReply<QString> second = *watcher;
                watcher->deleteLater();
                TCHECK(&f, second.isValid());
                const QJsonObject secondBody = parsePayload(second.value());
                TCHECK(&f, secondBody.value(QStringLiteral("changed")).toInt(-1) == 1);
                TCHECK(&f, secondBody.value(QStringLiteral("serial")).toString() == QStringLiteral("1"));
                TCHECK(&f, secondBody.value(QStringLiteral("revision")).toString() == currentRevision(f.source));
                TCHECK(&f, secondBody.value(QStringLiteral("phase")).toString() == QStringLiteral("cancelled"));
                f.next();
            });
        });
    });

    // Step 4: an unacked terminal survives a newer same-window start and
    // blocks holds; only the exact-identity ack retires it.
    f.steps.push_back([&]() {
        TCHECK(&f, f.source->gestureStarted(QStringLiteral("win-1"), QStringLiteral("resize"),
                                            QRect(0, 0, 100, 100), false, QPoint(), false) == 2);
        auto *pull = asyncLastGesture(*scriptCaller);
        QObject::connect(pull, &QDBusPendingCallWatcher::finished, &app, [&, pull]() {
            QDBusPendingReply<QString> reply = *pull;
            pull->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("serial")).toString() == QStringLiteral("1"));
            TCHECK(&f, body.value(QStringLiteral("phase")).toString() == QStringLiteral("cancelled"));
            // A Wait naming the new open identity and current revision still
            // serves the unacked terminal immediately (never holds past it).
            auto *watcher = asyncWaitGesture(*scriptCaller, gestureRequestRev(f.instance, QStringLiteral("2"), currentRevision(f.source), 20000));
            QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
                QDBusPendingReply<QString> second = *watcher;
                watcher->deleteLater();
                TCHECK(&f, second.isValid());
                const QJsonObject secondBody = parsePayload(second.value());
                TCHECK(&f, secondBody.value(QStringLiteral("serial")).toString() == QStringLiteral("1"));
                TCHECK(&f, secondBody.value(QStringLiteral("phase")).toString() == QStringLiteral("cancelled"));
                // Wrong identities never retire: the head stays.
                auto *wrong = asyncAckGesture(*scriptCaller, gestureRequest(f.instance, QStringLiteral("2"), 20000));
                QObject::connect(wrong, &QDBusPendingCallWatcher::finished, &app, [&, wrong]() {
                    QDBusPendingReply<QString> wrongReply = *wrong;
                    wrong->deleteLater();
                    TCHECK(&f, wrongReply.isValid());
                    const QJsonObject wrongBody = parsePayload(wrongReply.value());
                    TCHECK(&f, wrongBody.value(QStringLiteral("serial")).toString() == QStringLiteral("1"));
                    auto *ack = asyncAckGesture(*scriptCaller, gestureRequest(f.instance, QStringLiteral("1"), 20000));
                    QObject::connect(ack, &QDBusPendingCallWatcher::finished, &app, [&, ack]() {
                        QDBusPendingReply<QString> ackReply = *ack;
                        ack->deleteLater();
                        TCHECK(&f, ackReply.isValid());
                        const QJsonObject ackBody = parsePayload(ackReply.value());
                        TCHECK(&f, ackBody.value(QStringLiteral("serial")).toString() == QStringLiteral("2"));
                        TCHECK(&f, ackBody.value(QStringLiteral("phase")).toString() == QStringLiteral("started"));
                        TCHECK(&f, f.source->gestureFinished(QStringLiteral("win-1"), true, QPoint(5, 5)) == KWin::GesturePhase::Released);
                        f.next();
                    });
                });
            });
        });
    });

    // Step 5: consumed-Esc finish reports Released (no arm reached the
    // fence); the exact ack then reveals idle, which holds on its identity.
    f.steps.push_back([&]() {
        auto *pull = asyncLastGesture(*scriptCaller);
        QObject::connect(pull, &QDBusPendingCallWatcher::finished, &app, [&, pull]() {
            QDBusPendingReply<QString> reply = *pull;
            pull->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("serial")).toString() == QStringLiteral("2"));
            TCHECK(&f, body.value(QStringLiteral("phase")).toString() == QStringLiteral("released"));
            auto *ack = asyncAckGesture(*scriptCaller, gestureRequest(f.instance, QStringLiteral("2"), 20000));
            QObject::connect(ack, &QDBusPendingCallWatcher::finished, &app, [&, ack]() {
                QDBusPendingReply<QString> ackReply = *ack;
                ack->deleteLater();
                TCHECK(&f, ackReply.isValid());
                const QJsonObject ackBody = parsePayload(ackReply.value());
                TCHECK(&f, ackBody.value(QStringLiteral("serial")).toString() == QStringLiteral("0"));
                TCHECK(&f, ackBody.value(QStringLiteral("phase")).toString() == QStringLiteral("idle"));
                // Idle holds on its exact identity and revision, then
                // completes unchanged on the bounded lease.
                const qint64 startMs = QDateTime::currentMSecsSinceEpoch();
                auto *watcher = asyncWaitGesture(*scriptCaller, gestureRequestRev(f.instance, QStringLiteral("0"), currentRevision(f.source), 1000));
                QTimer::singleShot(300, &app, [&, watcher]() {
                    if (watcher->isFinished()) {
                        f.fail("idle lease must hold the waiter", __FILE__, __LINE__);
                        return;
                    }
                });
                QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher, startMs]() {
                    QDBusPendingReply<QString> second = *watcher;
                    watcher->deleteLater();
                    TCHECK(&f, second.isValid());
                    const QJsonObject secondBody = parsePayload(second.value());
                    TCHECK(&f, secondBody.value(QStringLiteral("changed")).toInt(-1) == 0);
                    TCHECK(&f, secondBody.value(QStringLiteral("serial")).toString() == QStringLiteral("0"));
                    TCHECK(&f, QDateTime::currentMSecsSinceEpoch() - startMs >= 500);
                    f.next();
                });
            });
        });
    });

    // Step 6: supersession (a newer Wait completes the pending one as
    // superseded; the replacement wakes on the next transition).
    f.steps.push_back([&]() {
        TCHECK(&f, f.source->gestureStarted(QStringLiteral("win-3"), QStringLiteral("move"),
                                            QRect(1, 2, 30, 40), true, QPoint(3, 4), true) == 3);
        auto *first = asyncWaitGesture(*scriptCaller, gestureRequestRev(f.instance, QStringLiteral("3"), currentRevision(f.source), 20000));
        auto *second = asyncWaitGesture(*scriptCaller, gestureRequestRev(f.instance, QStringLiteral("3"), currentRevision(f.source), 20000));
        QObject::connect(first, &QDBusPendingCallWatcher::finished, &app, [&, first, second]() {
            QDBusPendingReply<QString> reply = *first;
            first->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("superseded")).toInt(-1) == 1);
            TCHECK(&f, !second->isFinished());
            TCHECK(&f, f.source->gestureCursor(QStringLiteral("win-3"), true, QPoint(9, 9)));
        });
        QObject::connect(second, &QDBusPendingCallWatcher::finished, &app, [&, second]() {
            QDBusPendingReply<QString> reply = *second;
            second->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("superseded")).toInt(-1) == 0);
            TCHECK(&f, body.value(QStringLiteral("phase")).toString() == QStringLiteral("updated"));
            TCHECK(&f, f.source->gestureFinished(QStringLiteral("win-3"), true, QPoint(9, 9)) == KWin::GesturePhase::Released);
            f.next();
        });
    });

    // Step 7: abort drops the open gesture without inventing a terminal.
    f.steps.push_back([&]() {
        auto *ack = asyncAckGesture(*scriptCaller, gestureRequest(f.instance, QStringLiteral("3"), 20000));
        QObject::connect(ack, &QDBusPendingCallWatcher::finished, &app, [&, ack]() {
            QDBusPendingReply<QString> ackReply = *ack;
            ack->deleteLater();
            TCHECK(&f, ackReply.isValid());
            TCHECK(&f, f.source->gestureStarted(QStringLiteral("win-9"), QStringLiteral("move"),
                                                QRect(0, 0, 50, 50), true, QPoint(1, 1), true) == 4);
            auto *watcher = asyncWaitGesture(*scriptCaller, gestureRequestRev(f.instance, QStringLiteral("4"), currentRevision(f.source), 20000));
            QTimer::singleShot(100, &app, [&, watcher]() {
                TCHECK(&f, !watcher->isFinished());
                TCHECK(&f, f.source->gestureAborted(QStringLiteral("win-9")));
            });
            QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
                QDBusPendingReply<QString> reply = *watcher;
                watcher->deleteLater();
                TCHECK(&f, reply.isValid());
                const QJsonObject body = parsePayload(reply.value());
                TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
                TCHECK(&f, body.value(QStringLiteral("serial")).toString() == QStringLiteral("0"));
                TCHECK(&f, body.value(QStringLiteral("phase")).toString() == QStringLiteral("idle"));
                auto *pull = asyncLastGesture(*scriptCaller);
                QObject::connect(pull, &QDBusPendingCallWatcher::finished, &app, [&, pull]() {
                    QDBusPendingReply<QString> pullReply = *pull;
                    pull->deleteLater();
                    TCHECK(&f, pullReply.isValid());
                    const QJsonObject pullBody = parsePayload(pullReply.value());
                    TCHECK(&f, pullBody.value(QStringLiteral("serial")).toString() == QStringLiteral("0"));
                    f.next();
                });
            });
        });
    });

    // Step 8: foreign caller denied on wait and ack without displacement.
    f.steps.push_back([&]() {
        TCHECK(&f, f.source->gestureStarted(QStringLiteral("win-5"), QStringLiteral("resize"),
                                            QRect(0, 0, 60, 60), false, QPoint(), false) == 5);
        auto *held = asyncWaitGesture(*scriptCaller, gestureRequestRev(f.instance, QStringLiteral("5"), currentRevision(f.source), 20000));
        auto *hostileWait = asyncWaitGesture(*foreignCaller, gestureRequest(QString(), QString(), 20000));
        QObject::connect(hostileWait, &QDBusPendingCallWatcher::finished, &app, [&, hostileWait, held]() {
            QDBusPendingReply<QString> reply = *hostileWait;
            hostileWait->deleteLater();
            TCHECK(&f, reply.isError());
            TCHECK(&f, reply.error().type() == QDBusError::AccessDenied);
            TCHECK(&f, !held->isFinished());
            auto *hostileAck = asyncAckGesture(*foreignCaller, gestureRequest(f.instance, QStringLiteral("5"), 20000));
            QObject::connect(hostileAck, &QDBusPendingCallWatcher::finished, &app, [&, hostileAck, held]() {
                QDBusPendingReply<QString> ackReply = *hostileAck;
                hostileAck->deleteLater();
                TCHECK(&f, ackReply.isError());
                TCHECK(&f, ackReply.error().type() == QDBusError::AccessDenied);
                TCHECK(&f, !held->isFinished());
                TCHECK(&f, f.source->gestureFinished(QStringLiteral("win-5"), true, QPoint(2, 2)) == KWin::GesturePhase::Released);
            });
        });
        QObject::connect(held, &QDBusPendingCallWatcher::finished, &app, [&, held]() {
            QDBusPendingReply<QString> reply = *held;
            held->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("serial")).toString() == QStringLiteral("5"));
            TCHECK(&f, body.value(QStringLiteral("phase")).toString() == QStringLiteral("released"));
            auto *ack = asyncAckGesture(*scriptCaller, gestureRequest(f.instance, QStringLiteral("5"), 20000));
            QObject::connect(ack, &QDBusPendingCallWatcher::finished, &app, [&, ack]() {
                QDBusPendingReply<QString> ackReply = *ack;
                ack->deleteLater();
                TCHECK(&f, ackReply.isValid());
                f.next();
            });
        });
    });

    // Step 9: malformed request completes immediately with no mutation.
    f.steps.push_back([&]() {
        auto *watcher = asyncWaitGesture(*scriptCaller, QStringLiteral("not json{{{"));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("serial")).toString() == QStringLiteral("0"));
            f.next();
        });
    });

    // Step 10: oversized request refused without mutation or hold.
    f.steps.push_back([&]() {
        auto *watcher = asyncWaitGesture(*scriptCaller, QString(5000, QLatin1Char('x')));
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isError());
            TCHECK(&f, reply.error().type() == QDBusError::InvalidArgs);
            auto *probe = asyncWaitGesture(*scriptCaller, gestureRequestRev(f.instance, QStringLiteral("0"), currentRevision(f.source), 20000));
            QTimer::singleShot(200, &app, [&, probe]() {
                TCHECK(&f, !probe->isFinished());
                probe->deleteLater();
                f.next();
            });
        });
    });

    // Step 11: lost-update interleaving: a cursor update lands while no
    // waiter is held, then the script waits with the same serial but a
    // stale (pre-update) revision. The waiter must complete immediately
    // with the fresh snapshot, never hold past the update.
    f.steps.push_back([&]() {
        const quint64 serial = f.source->gestureStarted(QStringLiteral("win-6"), QStringLiteral("move"),
            QRect(0, 0, 80, 80), true, QPoint(2, 2), true);
        TCHECK(&f, serial != 0);
        TCHECK(&f, f.source->gestureCursor(QStringLiteral("win-6"), true, QPoint(7, 7)));
        const QString serialText = QString::number(serial);
        const QString freshRevision = currentRevision(f.source);
        TCHECK(&f, freshRevision != QStringLiteral("0"));
        auto *watcher = asyncWaitGesture(*scriptCaller,
            QStringLiteral("{\"instance\":\"%1\",\"serial\":\"%2\",\"revision\":\"0\",\"leaseMs\":20000}")
                .arg(f.instance, serialText));
        QTimer::singleShot(400, &app, [&, watcher]() {
            if (!watcher->isFinished()) {
                f.fail("stale revision must complete immediately, not hold", __FILE__, __LINE__);
                return;
            }
        });
        QObject::connect(watcher, &QDBusPendingCallWatcher::finished, &app, [&, watcher, serialText, freshRevision]() {
            QDBusPendingReply<QString> reply = *watcher;
            watcher->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("changed")).toInt(-1) == 1);
            TCHECK(&f, body.value(QStringLiteral("serial")).toString() == serialText);
            TCHECK(&f, body.value(QStringLiteral("revision")).toString() == freshRevision);
            TCHECK(&f, body.value(QStringLiteral("phase")).toString() == QStringLiteral("updated"));
            TCHECK(&f, body.value(QStringLiteral("cursor")).toArray().at(0).toInt(-1) == 7);
            TCHECK(&f, f.source->gestureFinished(QStringLiteral("win-6"), true, QPoint(7, 7)) == KWin::GesturePhase::Released);
            auto *ack = asyncAckGesture(*scriptCaller, gestureRequest(f.instance, serialText, 20000));
            QObject::connect(ack, &QDBusPendingCallWatcher::finished, &app, [&, ack]() {
                QDBusPendingReply<QString> ackReply = *ack;
                ack->deleteLater();
                TCHECK(&f, ackReply.isValid());
                TCHECK(&f, parsePayload(ackReply.value()).value(QStringLiteral("serial")).toString() == QStringLiteral("0"));
                TCHECK(&f, parsePayload(ackReply.value()).value(QStringLiteral("phase")).toString() == QStringLiteral("idle"));
                f.next();
            });
        });
    });

    // Step 12: refused-start fail-closed: a refused new start for the same
    // window mints nothing, and the effect wiring drops the older open via
    // the production source abort (same call sequence as
    // onGestureDragStart). A later Esc never arms and the finish invents
    // no terminal: the snapshot reads idle.
    f.steps.push_back([&]() {
        const quint64 serial = f.source->gestureStarted(QStringLiteral("win-7"), QStringLiteral("move"),
            QRect(0, 0, 80, 80), true, QPoint(2, 2), true);
        TCHECK(&f, serial != 0);
        // Refused (out-of-bound frame): no state change, no revision mint.
        const QString beforeAbort = currentRevision(f.source);
        TCHECK(&f, f.source->gestureStarted(QStringLiteral("win-7"), QStringLiteral("move"),
                                            QRect(20000, 0, 10, 10), false, QPoint(), false) == 0);
        TCHECK(&f, currentRevision(f.source) == beforeAbort);
        TCHECK(&f, f.source->hasOpen());
        // Effect wiring drops the older same-window open without a
        // terminal; the abort mints a revision and wakes nobody invented.
        TCHECK(&f, f.source->gestureAborted(QStringLiteral("win-7")));
        TCHECK(&f, !f.source->hasOpen());
        f.source->armEsc();
        TCHECK(&f, f.source->gestureFinished(QStringLiteral("win-7"), true, QPoint(1, 1)) == KWin::GesturePhase::Idle);
        auto *pull = asyncLastGesture(*scriptCaller);
        QObject::connect(pull, &QDBusPendingCallWatcher::finished, &app, [&, pull]() {
            QDBusPendingReply<QString> reply = *pull;
            pull->deleteLater();
            TCHECK(&f, reply.isValid());
            const QJsonObject body = parsePayload(reply.value());
            TCHECK(&f, body.value(QStringLiteral("serial")).toString() == QStringLiteral("0"));
            TCHECK(&f, body.value(QStringLiteral("phase")).toString() == QStringLiteral("idle"));
            f.next();
        });
    });

    // Step 13: shutdown completes a pending waiter as explicitly
    // unavailable. Teardown mirrors production: the object and well-known
    // name release on the exact owned connection, then that named
    // connection disconnects; the script connection is never unregistered
    // from here.
    f.steps.push_back([&]() {
        auto *watcher = asyncWaitGesture(*scriptCaller, gestureRequestRev(f.instance, QStringLiteral("0"), currentRevision(f.source), 20000));
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
            f.effect->unregisterObject(QStringLiteral("/com/omnitiler/Gesture"));
            TCHECK(&f, f.effect->unregisterService(QStringLiteral("com.omnitiler.Gesture")));
            QDBusConnection::disconnectFromBus(QStringLiteral("gesture-effect-dedicated"));
            QDBusConnection::disconnectFromBus(QStringLiteral("gesture-foreign"));
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
