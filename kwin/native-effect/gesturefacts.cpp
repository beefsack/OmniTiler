// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#include "gesturefacts.h"

#include "activeborderlogic.h"

#include <QDBusConnectionInterface>
#include <QDBusError>
#include <QJsonDocument>
#include <QJsonObject>
#include <QUuid>

namespace KWin
{

GestureFactsSource::GestureFactsSource(const QDBusConnection &connection, QObject *parent)
    : QObject(parent)
    , m_connection(connection)
{
    m_store.setInstance(QUuid::createUuid().toString(QUuid::WithoutBraces));
    m_leaseTimer.setSingleShot(true);
    connect(&m_leaseTimer, &QTimer::timeout, this, [this]() {
        // Idle expiry is not teardown: the endpoint is alive with valid
        // (unchanged) state. Pass available:true.
        completePending(false, false, true);
    });
}

GestureFactsSource::~GestureFactsSource()
{
    // A held waiter must never outlive the endpoint: complete it as
    // explicitly unavailable so the script hides immediately instead of
    // holding a stale snapshot through its lease.
    completePending(false, false, false);
}

quint64 GestureFactsSource::gestureStarted(const QString &win, const QString &kind, const QRect &startRect,
    bool hasStartCursor, const QPoint &startCursor, bool modalMove)
{
    const QString before = m_store.currentJson(false, false, true);
    const quint64 serial = m_store.beginGesture(win, kind, startRect, hasStartCursor, startCursor, modalMove);
    if (serial != 0 && m_store.currentJson(false, false, true) != before) {
        completePending(true, false, true);
    }
    return serial;
}

bool GestureFactsSource::gestureCursor(const QString &win, bool hasCursor, const QPoint &cursor)
{
    const QString before = m_store.currentJson(false, false, true);
    const bool updated = m_store.updateCursor(win, hasCursor, cursor);
    if (updated && m_store.currentJson(false, false, true) != before) {
        completePending(true, false, true);
    }
    return updated;
}

GesturePhase GestureFactsSource::gestureFinished(const QString &win, bool hasCursor, const QPoint &cursor)
{
    const QString before = m_store.currentJson(false, false, true);
    const GesturePhase phase = m_store.finishGesture(win, hasCursor, cursor);
    if (phase != GesturePhase::Idle && m_store.currentJson(false, false, true) != before) {
        completePending(true, false, true);
    }
    return phase;
}

bool GestureFactsSource::gestureAborted(const QString &win)
{
    const QString before = m_store.currentJson(false, false, true);
    const bool dropped = m_store.abortGesture(win);
    if (dropped && m_store.currentJson(false, false, true) != before) {
        completePending(true, false, true);
    }
    return dropped;
}

void GestureFactsSource::armEsc()
{
    m_store.armEsc();
}

bool GestureFactsSource::hasOpen() const
{
    return m_store.hasOpen();
}

QString GestureFactsSource::currentSnapshot(bool changed, bool superseded, bool available) const
{
    return m_store.currentJson(changed, superseded, available);
}

void GestureFactsSource::completePending(bool changed, bool superseded, bool available)
{
    if (!m_waitArmed) {
        return;
    }
    m_waitArmed = false;
    QDBusMessage reply = m_waitMessage.createReply(currentSnapshot(changed, superseded, available));
    m_waitMessage = QDBusMessage();
    m_leaseTimer.stop();
    m_connection.send(reply);
}

void GestureFactsSource::armWaiter(const QDBusMessage &message, int leaseMs)
{
    // One waiter at a time: a newer Wait completes the pending one as
    // superseded first, so overlapping waiters never double-complete and
    // no reply is ever lost to a replaced slot.
    completePending(false, true, true);
    m_waitMessage = message;
    m_waitArmed = true;
    m_leaseTimer.start(presentationFactsLeaseMs(leaseMs));
}

bool GestureFactsSource::isOwnerCaller(const QDBusMessage &message) const
{
    // Only the current KWin bus owner may wait or ack: the script route runs
    // on KWin's own session connection, so its sender is the owner identity.
    // Any other sender (foreign same-user callers included) is denied with
    // no slot mutation. An unreadable owner fails closed.
    const QString sender = message.service();
    if (sender.isEmpty()) {
        return false;
    }
    const QString owner = m_connection.interface()->serviceOwner(QStringLiteral("org.kde.KWin"));
    return !owner.isEmpty() && sender == owner;
}

void GestureFactsSource::WaitGesture(const QString &request, const QDBusMessage &message)
{
    if (!calledFromDBus()) {
        return;
    }
    setDelayedReply(true);
    handleWait(request, message);
}

void GestureFactsSource::handleWait(const QString &request, const QDBusMessage &message)
{
    if (!isOwnerCaller(message)) {
        m_connection.send(message.createErrorReply(QDBusError::AccessDenied,
            QStringLiteral("gesture facts are only served to the KWin bus owner")));
        return;
    }
    if (request.size() > MAX_REQUEST_BYTES) {
        m_connection.send(message.createErrorReply(QDBusError::InvalidArgs,
            QStringLiteral("gesture facts request over bound")));
        return;
    }
    // Tolerant decode: any malformed identity counts as unknown and
    // completes immediately with the fresh snapshot (no mutation, no hold).
    // A stale revision completes immediately too, even when the native
    // identity matches: the update it missed already landed.
    QString knownInstance;
    QString knownSerial;
    QString knownRevision;
    int leaseMs = 20000;
    const QJsonDocument requestDoc = QJsonDocument::fromJson(request.toUtf8());
    if (requestDoc.isObject()) {
        const QJsonObject requestObject = requestDoc.object();
        knownInstance = requestObject.value(QStringLiteral("instance")).toString();
        knownSerial = requestObject.value(QStringLiteral("serial")).toString();
        knownRevision = requestObject.value(QStringLiteral("revision")).toString();
        const int requestedLease = requestObject.value(QStringLiteral("leaseMs")).toInt(20000);
        if (requestedLease > 0) {
            leaseMs = requestedLease;
        }
    }
    // Hold only while no unacked terminal waits and the request names the
    // exact current native identity AND the exact current source revision
    // (the open serial, or "0" when idle, plus the newest minted revision):
    // any unacked terminal, identity mismatch, or stale revision completes
    // immediately so a fast terminal or an update that landed while no
    // waiter was held is never missed. A matching open hold waits for the
    // next transition; a matching idle hold waits for the next begin.
    const QString holdSerial =
        m_store.hasOpen() ? QString::number(m_store.openSerial()) : QStringLiteral("0");
    const QString holdRevision = QString::number(m_store.revision());
    const bool holds = m_store.terminalCount() == 0 && knownInstance == m_store.instance()
        && knownSerial == holdSerial && knownRevision == holdRevision;
    if (!holds) {
        m_connection.send(message.createReply(currentSnapshot(true, false, true)));
        return;
    }
    armWaiter(message, leaseMs);
}

QString GestureFactsSource::LastGesture() const
{
    // Sticky pull: the oldest unacked terminal, else the open gesture, else
    // the idle snapshot. Never fabricates a terminal; never holds.
    return currentSnapshot(false, false, true);
}

QString GestureFactsSource::AckGesture(const QString &request, const QDBusMessage &message)
{
    if (!isOwnerCaller(message)) {
        m_connection.send(message.createErrorReply(QDBusError::AccessDenied,
            QStringLiteral("gesture facts are only served to the KWin bus owner")));
        return QString();
    }
    if (request.size() <= MAX_REQUEST_BYTES) {
        const QJsonDocument requestDoc = QJsonDocument::fromJson(request.toUtf8());
        if (requestDoc.isObject()) {
            const QJsonObject requestObject = requestDoc.object();
            const QString ackInstance = requestObject.value(QStringLiteral("instance")).toString();
            bool serialOk = false;
            const quint64 ackSerial = requestObject.value(QStringLiteral("serial")).toString().toULongLong(&serialOk);
            if (serialOk && ackSerial != 0) {
                // The ack mints a new revision (it exposes the next terminal,
                // the open gesture, or idle): wake any holder with the fresh
                // snapshot. No waiter can be armed here in practice (a hold
                // requires an empty FIFO), so this is normally a no-op.
                const QString before = m_store.currentJson(false, false, true);
                if (m_store.acknowledge(ackInstance, ackSerial)
                    && m_store.currentJson(false, false, true) != before) {
                    completePending(true, false, true);
                }
            }
        }
    }
    return currentSnapshot(false, false, true);
}

QString GestureFactsSource::instance() const
{
    return m_store.instance();
}

quint64 GestureFactsSource::revision() const
{
    return m_store.revision();
}

QString GestureFactsSource::openWin() const
{
    return m_store.openWin();
}

} // namespace KWin

#include "gesturefacts.moc"
