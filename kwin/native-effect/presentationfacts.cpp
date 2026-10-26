// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#include "presentationfacts.h"

#include "activeborderlogic.h"

#include <QDBusConnectionInterface>
#include <QDBusError>
#include <QJsonDocument>
#include <QJsonObject>
#include <QUuid>

namespace KWin
{

PresentationFactsSource::PresentationFactsSource(const QDBusConnection &connection, QObject *parent)
    : QObject(parent)
    , m_connection(connection)
    , m_instance(QUuid::createUuid().toString(QUuid::WithoutBraces))
{
    m_leaseTimer.setSingleShot(true);
    connect(&m_leaseTimer, &QTimer::timeout, this, [this]() {
        // Idle expiry is not teardown: the endpoint is alive with valid
        // (unchanged) facts. Pass available:true and let currentSnapshot
        // apply the seed gate (unseeded still reads unavailable).
        completePending(false, false, true);
    });
}

PresentationFactsSource::~PresentationFactsSource()
{
    // A held waiter must never outlive the endpoint: complete it as
    // explicitly unavailable so the script hides immediately instead of
    // holding a stale snapshot through its lease.
    completePending(false, false, false);
}

void PresentationFactsSource::noteChord(bool held, bool known)
{
    if (m_chordKnown == known && m_chordHeld == held && known) {
        return;
    }
    m_chordHeld = held;
    m_chordKnown = known;
    bumpRevision();
}

void PresentationFactsSource::noteStyle(double gap, double width, double extension)
{
    if (m_tripleKnown && gap == m_gap && width == m_width && extension == m_extension) {
        return;
    }
    m_gap = gap;
    m_width = width;
    m_extension = extension;
    if (!m_tripleKnown) {
        m_tripleKnown = true;
        return;
    }
    bumpRevision();
}

void PresentationFactsSource::noteInvalidation(quint64 counter)
{
    if (m_invalidation == counter) {
        return;
    }
    m_invalidation = counter;
    bumpRevision();
}

void PresentationFactsSource::bumpRevision()
{
    ++m_rev;
    completePending(true, false, true);
}

QString PresentationFactsSource::currentSnapshot(bool changed, bool superseded, bool available) const
{
    PresentationFactsSnapshot facts;
    facts.instance = m_instance;
    facts.rev = m_rev;
    facts.invalidation = m_invalidation;
    facts.chord = m_chordHeld;
    facts.chordKnown = m_chordKnown;
    facts.gap = m_gap;
    facts.width = m_width;
    facts.extension = m_extension;
    facts.changed = changed;
    facts.superseded = superseded;
    // No guessed defaults: the source is unavailable until the first real
    // style push seeds it. A fresh instance serves default zeros only as
    // explicitly unavailable, so the script hides instead of rendering
    // never-configured numbers.
    facts.available = available && m_tripleKnown;
    return presentationFactsJson(facts);
}

bool PresentationFactsSource::isOwnerCaller(const QDBusMessage &message) const
{
    // Only the current KWin bus owner may wait: the script route runs on
    // KWin's own session connection, so its sender is the owner identity.
    // Any other sender (foreign same-user callers included) is denied with
    // no slot mutation. An unreadable owner fails closed.
    const QString sender = message.service();
    if (sender.isEmpty()) {
        return false;
    }
    const QString owner = m_connection.interface()->serviceOwner(QStringLiteral("org.kde.KWin"));
    return !owner.isEmpty() && sender == owner;
}

void PresentationFactsSource::completePending(bool changed, bool superseded, bool available)
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

void PresentationFactsSource::armWaiter(const QDBusMessage &message, int leaseMs)
{
    // One waiter at a time: a newer Wait completes the pending one as
    // superseded first, so overlapping waiters never double-complete and
    // no reply is ever lost to a replaced slot.
    completePending(false, true, true);
    m_waitMessage = message;
    m_waitArmed = true;
    m_leaseTimer.start(presentationFactsLeaseMs(leaseMs));
}

void PresentationFactsSource::WaitPresentationFacts(const QString &request, const QDBusMessage &message)
{
    if (!calledFromDBus()) {
        return;
    }
    setDelayedReply(true);
    handleWait(request, message);
}

void PresentationFactsSource::handleWait(const QString &request, const QDBusMessage &message)
{
    if (!isOwnerCaller(message)) {
        m_connection.send(message.createErrorReply(QDBusError::AccessDenied,
            QStringLiteral("presentation facts are only served to the KWin bus owner")));
        return;
    }
    if (request.size() > MAX_REQUEST_BYTES) {
        m_connection.send(message.createErrorReply(QDBusError::InvalidArgs,
            QStringLiteral("presentation facts request over bound")));
        return;
    }
    // Tolerant decode: any malformed identity counts as unknown and
    // completes immediately with the fresh snapshot (no mutation, no hold).
    QString knownInstance;
    QString knownRev;
    int leaseMs = 20000;
    const QJsonDocument requestDoc = QJsonDocument::fromJson(request.toUtf8());
    if (requestDoc.isObject()) {
        const QJsonObject requestObject = requestDoc.object();
        knownInstance = requestObject.value(QStringLiteral("instance")).toString();
        knownRev = requestObject.value(QStringLiteral("rev")).toString();
        const int requestedLease = requestObject.value(QStringLiteral("leaseMs")).toInt(20000);
        if (requestedLease > 0) {
            leaseMs = requestedLease;
        }
    }
    if (presentationFactsStale(knownInstance, knownRev, m_instance, m_rev)) {
        m_connection.send(message.createReply(currentSnapshot(true, false, true)));
        return;
    }
    armWaiter(message, leaseMs);
}

QString PresentationFactsSource::instance() const
{
    return m_instance;
}

quint64 PresentationFactsSource::revision() const
{
    return m_rev;
}

quint64 PresentationFactsSource::invalidation() const
{
    return m_invalidation;
}

} // namespace KWin

#include "presentationfacts.moc"
