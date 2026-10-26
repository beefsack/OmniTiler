// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#pragma once

// Authoritative native gesture lifecycle D-Bus source (KWin script route,
// read-only): the effect-owned GestureLifecycleStore behind a held-waiter
// plus sticky-pull surface for the script/broker cutover. Qt only: no KWin
// headers, so the offline transport test drives this exact production class
// over a private bus with no test fake.
//
// The store owns identity {effect-instance UUID, serial}: serial is a
// nonzero decimal string, "0" names no gesture and is never acked. Beside
// the native identity rides a monotonic source revision (decimal string,
// transport-only): every material visible-snapshot change (begin, cursor
// update, finish, abort, and ack exposing the next terminal/open/idle)
// mints a new revision. The script holds one WaitGesture call per adapter
// generation; a mismatched identity OR a stale revision completes
// immediately with the fresh snapshot (changed:1), even when the native
// identity matches: an update that lands while no waiter is held (or while
// a prior reply is in transit) is never missed by the next wait. A hold
// requires the exact current identity AND the exact current revision, and
// only while an open gesture with that identity is live and no unacked
// terminal waits. A newer Wait supersedes a pending one, which completes
// unchanged with superseded:1. Only the current KWin bus owner
// (org.kde.KWin) may wait or ack: any other sender gets AccessDenied with
// no slot mutation. Malformed requests complete immediately with the
// current snapshot (no mutation, no hold); oversized requests are refused
// as InvalidArgs.
//
// Terminal delivery contract (decision B): a fenced Esc observed during an
// active native move/resize reports Cancelled only with causal delivery to
// that dispatch's synchronous finish; otherwise finish reports Released.
// Fast start/Esc/finish before the first waiter completes with the stored
// Cancelled terminal (never a premature Released). Newer starts never erase
// unacked older terminals (bounded FIFO, exact-identity ack).
//
// Snapshot wire JSON (v:1):
// {"v":1,"instance":"<uuid>","serial":"<n>","revision":"<m>",
//  "phase":"started|updated|released|cancelled|idle",
//  "win":"<id-or-empty>","kind":"move|resize|\"\"",
//  "startRect":[x,y,w,h]|null,"startCursor":[x,y]|null,
//  "cursor":[x,y]|null,"modalMove":0/1,
//  "changed":0/1,"superseded":0/1,"available":0/1}
// Serial "0" with phase "idle" names no gesture; revision "0" is the fresh
// source before any material change. Doubles never cross here
// (all coordinates are integral KWin logical pixels).
//
// Wait request wire JSON:
// {"instance":"<uuid>","serial":"<n>","revision":"<m>","leaseMs":20000}
// A missing or malformed identity/revision counts as stale and completes
// immediately. Ack requests name only the native identity
// ({"instance":"<uuid>","serial":"<n>",...}): exact {instance, serial}
// match retires the head terminal; the revision is never part of identity.

#include "gesture_lifecycle.h"

#include <QDBusConnection>
#include <QDBusContext>
#include <QDBusMessage>
#include <QObject>
#include <QPoint>
#include <QRect>
#include <QString>
#include <QTimer>

namespace KWin
{

class GestureFactsSource : public QObject, protected QDBusContext
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "com.omnitiler.Gesture1")

public:
    // Maximum accepted request payload: real requests are ~100 bytes;
    // anything past this bound is refused without parsing or state change.
    static constexpr int MAX_REQUEST_BYTES = 4096;

    explicit GestureFactsSource(const QDBusConnection &connection, QObject *parent = nullptr);
    ~GestureFactsSource() override;

    // Observation pushes from the effect (start/finish signals plus the
    // fence). Each push that changes the visible snapshot completes a held
    // waiter with the fresh snapshot; no-change pushes hold nothing.
    // gestureStarted returns the minted serial ("0" refused); gestureFinished
    // returns the recorded terminal phase (Idle when nothing was open).
    quint64 gestureStarted(const QString &win, const QString &kind, const QRect &startRect, bool hasStartCursor,
        const QPoint &startCursor, bool modalMove);
    bool gestureCursor(const QString &win, bool hasCursor, const QPoint &cursor);
    GesturePhase gestureFinished(const QString &win, bool hasCursor, const QPoint &cursor);
    // Drops an open gesture without recording a terminal (window removed
    // mid-gesture): no invented terminal, never a fabricated Released or
    // Cancelled. Returns true when an open gesture was dropped.
    bool gestureAborted(const QString &win);
    // Delivery-fence arm: records that an Esc reached the passive filter at
    // InteractiveMoveResize during an active gesture. Dropped when no
    // gesture is open; cleared by every begin; consumed synchronously by
    // the next finish. Never wakes a waiter by itself: arming is not
    // evidence until a finish consumes it.
    void armEsc();
    bool hasOpen() const;

    // Held waiter logic shared by the production forwarder. The caller owns
    // the delayed reply: every D-Bus slot must call setDelayedReply(true)
    // before forwarding here, so an armed hold never completes into a dead
    // call. LastGesture is a sticky pull (no hold, no owner gate);
    // AckGesture retires the head terminal on exact {instance, serial}
    // match and returns the fresh snapshot.
    void handleWait(const QString &request, const QDBusMessage &message);
    Q_SCRIPTABLE void WaitGesture(const QString &request, const QDBusMessage &message);
    Q_SCRIPTABLE QString LastGesture() const;
    Q_SCRIPTABLE QString AckGesture(const QString &request, const QDBusMessage &message);

    QString instance() const;
    quint64 revision() const;
    QString openWin() const;

private:
    QString currentSnapshot(bool changed, bool superseded, bool available) const;
    void completePending(bool changed, bool superseded, bool available);
    void armWaiter(const QDBusMessage &message, int leaseMs);
    bool isOwnerCaller(const QDBusMessage &message) const;

    QDBusConnection m_connection;
    GestureLifecycleStore m_store;
    QDBusMessage m_waitMessage;
    QTimer m_leaseTimer;
    bool m_waitArmed = false;
};

} // namespace KWin
