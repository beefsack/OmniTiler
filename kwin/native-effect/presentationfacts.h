// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#pragma once

#include <QDBusConnection>
#include <QDBusContext>
#include <QDBusMessage>
#include <QObject>
#include <QString>
#include <QTimer>

#include <cstdint>

namespace KWin
{

// Read-only versioned presentation facts for the KWin script route (Stage 3
// ingress, no policy): the native chord level plus the actual style
// configuration the effect renders with. Qt-only: no KWin headers, so the
// offline transport test drives this exact production class over a private
// bus with no test fake.
//
// Protocol: the script holds one WaitPresentationFacts call. A mismatched
// identity completes immediately with the fresh snapshot (changed:1);
// otherwise the call holds until a fact changes (changed:1) or the bounded
// lease expires (changed:0). A newer Wait supersedes a pending one, which
// completes unchanged with superseded:1 so the script stands down to its
// host deadline instead of ping-ponging. Only the current KWin bus owner
// (org.kde.KWin) may wait: any other sender gets AccessDenied with no slot
// mutation. Malformed requests complete immediately with the current
// snapshot (no mutation); oversized requests are refused as InvalidArgs.
class PresentationFactsSource : public QObject, protected QDBusContext
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "com.omnitiler.ActiveBorder1")

public:
    // Maximum accepted request payload: real requests are ~50 bytes; anything
    // past this bound is refused without parsing or state change.
    static constexpr int MAX_REQUEST_BYTES = 4096;

    explicit PresentationFactsSource(const QDBusConnection &connection, QObject *parent = nullptr);
    ~PresentationFactsSource() override;

    // Fact pushes from the effect (level observations, never verdicts). Each
    // material change bumps the revision and completes a held waiter with
    // the fresh snapshot. The invalidation counter is the Stage 5 causal
    // witness: the effect advances it on geometry, activation/focus, and
    // gesture boundaries, and the script carries each capture's covered
    // value so the renderer only draws what covers the latest.
    void noteChord(bool held, bool known);
    void noteStyle(double gap, double width, double extension);
    void noteInvalidation(quint64 counter);

    // Held waiter logic shared by the production forwarder and test-owned
    // objects. The caller owns the delayed reply: every D-Bus slot must
    // call setDelayedReply(true) before forwarding here, so an armed hold
    // never completes into a dead call.
    void handleWait(const QString &request, const QDBusMessage &message);
    Q_SCRIPTABLE void WaitPresentationFacts(const QString &request, const QDBusMessage &message);

    QString instance() const;
    quint64 revision() const;
    quint64 invalidation() const;

private:
    void bumpRevision();
    void completePending(bool changed, bool superseded, bool available);
    void armWaiter(const QDBusMessage &message, int leaseMs);
    bool isOwnerCaller(const QDBusMessage &message) const;
    QString currentSnapshot(bool changed, bool superseded, bool available) const;

    QDBusConnection m_connection;
    const QString m_instance;
    quint64 m_rev = 0;
    quint64 m_invalidation = 0;
    bool m_chordHeld = false;
    bool m_chordKnown = false;
    double m_gap = 0.0;
    double m_width = 0.0;
    double m_extension = 0.0;
    bool m_tripleKnown = false;
    QDBusMessage m_waitMessage;
    QTimer m_leaseTimer;
    bool m_waitArmed = false;
};

} // namespace KWin
