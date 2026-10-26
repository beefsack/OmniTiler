// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#pragma once

// Authoritative native gesture lifecycle store (Qt only, no KWin scene, no
// session bus, no Rust staticlib): one effect-owned lifecycle mints one
// immutable identity per native move/resize, freezes start facts at Started,
// owns the phase, and resolves Esc ONLY through the causal delivery fence.
//
// Source-traced fence contract (primary pin: KWin v6.7.4; the installed
// host 6.8.0 headers carry the identical contract, see below):
// - src/input.h processFilters: synchronous ordered loop; the first filter
//   returning true stops the dispatch. Spies always run before filters
//   (src/keyboard_input.cpp: processSpies then processFilters). No input
//   spy arms anything: there is no sticky prefilter arm. Only the passive
//   fence filter arms, and only when an Esc reaches it after all prior
//   consumers.
// - src/input.h InputFilterOrder: Effects immediately precedes
//   InteractiveMoveResize (installed 6.8.0 header: same order).
// - src/input.cpp installInputEventFilter: lower_bound on weight, so a
//   filter installed LATER with the same weight runs BEFORE the earlier
//   one. The builtin MoveResizeFilter is installed at InteractiveMoveResize
//   weight during setupWorkspace (before effects load); the effect's passive
//   fence installs later at the same weight, so it observes Esc after
//   earlier consumers (shortcuts, effects) and immediately before the
//   builtin consumer.
// - src/input.cpp MoveResizeFilter::keyboardKey: forwards the key to
//   Window::keyPressEvent, then consumes (returns true) whenever a
//   move/resize window exists, whether or not the mode ended.
// - src/window.cpp Window::keyPressEvent: Escape calls
//   finishInteractiveMoveResize(true) synchronously; finish emits
//   interactiveMoveResizeFinished synchronously, which the effect layer
//   re-emits directly (src/effect/effectwindow.cpp).
//
// The fence therefore arms ONLY in its own keyboardKey override on
// Escape+Pressed/Repeated (gestureEscArmsFilter), never consumes (always
// returns false), never calls finish, uses no timers. The finish handler
// consumes the arm synchronously in that same dispatch. An Esc consumed by
// an earlier shortcut/effect filter never reaches the fence: the later
// normal finish correctly reports Released. XWayland _NET_WM_MOVERESIZE
// cancel has no distinguishable effect signal and reports Released;
// COSMIC-style Esc-as-drop is not implemented.
//
// Identity and schema: {effect-instance UUID, serial}; serial is a nonzero
// decimal string ("0" names no gesture and is never acked). Beside the
// native identity rides a monotonic source revision (decimal string,
// transport-only): every material visible-snapshot change mints a new
// revision, and a Wait naming the same native identity with a stale
// revision completes immediately instead of holding past the update.
// Phases and kinds use the shared core wire vocabulary (started/updated/
// released/cancelled, move/resize) so the broker maps them mechanically to
// the existing GestureIngress/GestureSettled core wire (unchanged here).
// Frozen start facts (window, kind, start frame, start cursor, modal move)
// ride every snapshot; the core accepts the first terminal with them and
// derives the release frame itself from staged facts.

#include <QtCore/qnamespace.h>
#include <QPoint>
#include <QRect>
#include <QString>

#include <cstdint>
#include <deque>

namespace KWin
{

// Passive fence predicate: arm only on an Escape press edge. The caller
// passes true when the KWin key state is Pressed or Repeated (the builtin
// consumer treats both identically); releases and every other key never
// arm. Returning false (release/no-Esc) also covers the consumed-before-
// fence case: with no arm the later finish reports Released.
inline bool gestureEscArmsFilter(Qt::Key key, bool pressedOrRepeated)
{
    return pressedOrRepeated && key == Qt::Key_Escape;
}

enum class GesturePhase : int {
    Idle,
    Started,
    Updated,
    Released,
    Cancelled,
};

inline const char *gesturePhaseName(GesturePhase phase)
{
    switch (phase) {
    case GesturePhase::Idle:
        return "idle";
    case GesturePhase::Started:
        return "started";
    case GesturePhase::Updated:
        return "updated";
    case GesturePhase::Released:
        return "released";
    case GesturePhase::Cancelled:
        return "cancelled";
    }
    return "idle";
}

inline const char *gestureKindName(const QString &kind)
{
    if (kind == QStringLiteral("move")) {
        return "move";
    }
    if (kind == QStringLiteral("resize")) {
        return "resize";
    }
    return "";
}

// Carried-geometry bound (mirrors the presentation consumer and the drag
// oracle verdict): KWin logical pixels, fail closed outside.
inline constexpr int GESTURE_COORD_MIN = -16384;
inline constexpr int GESTURE_COORD_MAX = 16384;
inline constexpr int GESTURE_SIZE_MAX = 16384;

// Bounded FIFO of unacked terminals: a newer start never erases an older
// terminal. The script acks each terminal promptly, so steady-state depth
// is one; on overflow the oldest is evicted (liveness of newer evidence
// over ancient, never silent adoption: evicted identities simply vanish,
// they are never rebound to another gesture).
inline constexpr int GESTURE_TERMINAL_FIFO_CAPACITY = 8;

inline constexpr int GESTURE_MAX_ID_LEN = 128;

struct GestureSnapshot {
    QString instance;
    quint64 serial = 0;
    // Live source revision, stamped by current(): always the newest minted
    // revision, never a frozen terminal-time value. Terminals freeze only
    // the native identity and facts; the revision stays a transport clock.
    quint64 revision = 0;
    GesturePhase phase = GesturePhase::Idle;
    QString win;
    QString kind;
    QRect startRect;
    bool hasStartCursor = false;
    QPoint startCursor;
    bool hasCursor = false;
    QPoint cursor;
    bool modalMove = false;
};

namespace GestureLifecycleDetail
{

inline bool isOpaqueId(const QString &value)
{
    if (value.isEmpty() || value.size() > GESTURE_MAX_ID_LEN) {
        return false;
    }
    for (const QChar c : value) {
        const ushort u = c.unicode();
        const bool alnum = (u >= u'0' && u <= u'9') || (u >= u'A' && u <= u'Z') || (u >= u'a' && u <= u'z');
        if (!(alnum || u == u'-' || u == u'_' || u == u'.')) {
            return false;
        }
    }
    return true;
}

inline bool isValidRect(const QRect &rect)
{
    return rect.x() >= GESTURE_COORD_MIN && rect.x() <= GESTURE_COORD_MAX && rect.y() >= GESTURE_COORD_MIN
        && rect.y() <= GESTURE_COORD_MAX && rect.width() >= 1 && rect.width() <= GESTURE_SIZE_MAX
        && rect.height() >= 1 && rect.height() <= GESTURE_SIZE_MAX;
}

inline QString pointJson(bool has, const QPoint &point)
{
    if (!has) {
        return QStringLiteral("null");
    }
    return QStringLiteral("[%1,%2]").arg(point.x()).arg(point.y());
}

} // namespace GestureLifecycleDetail

class GestureLifecycleStore
{
public:
    void setInstance(const QString &instance)
    {
        m_instance = instance;
    }

    QString instance() const
    {
        return m_instance;
    }

    // Mints one immutable identity ({instance, serial}) and freezes the
    // start facts. Clears any fence arm (a stale arm never leaks into a new
    // gesture). An unfinished open is superseded WITHOUT recording a
    // terminal: a missed finish is an observation gap, never a fabricated
    // Released or Cancelled. Mints a new source revision on success.
    // Returns 0 when refused (empty instance, bad window identity, unknown
    // kind, out-of-bound frame, or serial exhaustion): no state changes on
    // refusal (no revision mint, the open stays exactly as it was).
    quint64 beginGesture(const QString &win, const QString &kind, const QRect &startRect, bool hasStartCursor,
        const QPoint &startCursor, bool modalMove)
    {
        if (m_instance.isEmpty()) {
            return 0;
        }
        if (!GestureLifecycleDetail::isOpaqueId(win)) {
            return 0;
        }
        if (kind != QStringLiteral("move") && kind != QStringLiteral("resize")) {
            return 0;
        }
        if (!GestureLifecycleDetail::isValidRect(startRect)) {
            return 0;
        }
        if (m_nextSerial == 0 || m_nextSerial == UINT64_MAX) {
            return 0;
        }
        m_armed = false;
        m_open = true;
        m_serial = m_nextSerial;
        m_nextSerial += 1;
        m_win = win;
        m_kind = kind;
        m_startRect = startRect;
        m_hasStartCursor = hasStartCursor;
        m_startCursor = startCursor;
        m_hasCursor = hasStartCursor;
        m_cursor = startCursor;
        m_modalMove = modalMove;
        m_updated = false;
        m_revision += 1;
        return m_serial;
    }

    bool hasOpen() const
    {
        return m_open;
    }

    quint64 openSerial() const
    {
        return m_open ? m_serial : 0;
    }

    QString openWin() const
    {
        return m_open ? m_win : QString();
    }

    quint64 revision() const
    {
        return m_revision;
    }

    // Refreshes only the live cursor of the matching open gesture. Frozen
    // facts (identity, window, kind, start frame, start cursor, modal move)
    // are never touched: the first update also flips Started to Updated.
    // Mints a new source revision only on a material change (phase flip or
    // a different cursor); a no-change refresh mints nothing.
    // Returns false when no open gesture names this window.
    bool updateCursor(const QString &win, bool hasCursor, const QPoint &cursor)
    {
        if (!m_open || win != m_win) {
            return false;
        }
        if (m_updated && m_hasCursor == hasCursor && m_cursor == cursor) {
            return true;
        }
        m_hasCursor = hasCursor;
        m_cursor = cursor;
        m_updated = true;
        m_revision += 1;
        return true;
    }

    // Delivery-fence arm: records that an Esc reached the passive filter at
    // InteractiveMoveResize. Dropped when no gesture is open (a spy-only
    // observation or a stray key never arms a future gesture).
    void armEsc()
    {
        if (m_open) {
            m_armed = true;
        }
    }

    bool escArmed() const
    {
        return m_armed;
    }

    // Records the terminal for the matching open gesture and consumes the
    // arm synchronously: armed Esc becomes Cancelled, otherwise Released.
    // The terminal freezes the start facts plus the last live cursor and
    // waits in the bounded FIFO for exact-identity ack. Mints a new source
    // revision. Returns Idle and records nothing when no open gesture names
    // this window: finishes never invent terminals.
    GesturePhase finishGesture(const QString &win, bool hasCursor, const QPoint &cursor)
    {
        if (!m_open || win != m_win) {
            return GesturePhase::Idle;
        }
        const GesturePhase phase = m_armed ? GesturePhase::Cancelled : GesturePhase::Released;
        GestureSnapshot terminal;
        terminal.instance = m_instance;
        terminal.serial = m_serial;
        terminal.phase = phase;
        terminal.win = m_win;
        terminal.kind = m_kind;
        terminal.startRect = m_startRect;
        terminal.hasStartCursor = m_hasStartCursor;
        terminal.startCursor = m_startCursor;
        terminal.hasCursor = hasCursor;
        terminal.cursor = cursor;
        terminal.modalMove = m_modalMove;
        if (static_cast<int>(m_terminals.size()) >= GESTURE_TERMINAL_FIFO_CAPACITY) {
            m_terminals.pop_front();
        }
        m_terminals.push_back(terminal);
        m_open = false;
        m_armed = false;
        m_revision += 1;
        return phase;
    }

    // Drops the open gesture naming this window without recording a
    // terminal (window removed mid-gesture): the gesture vanished, so
    // neither Released nor Cancelled may be asserted. Mints a new source
    // revision. Returns false when nothing was open.
    bool abortGesture(const QString &win)
    {
        if (!m_open || win != m_win) {
            return false;
        }
        m_open = false;
        m_armed = false;
        m_revision += 1;
        return true;
    }

    int terminalCount() const
    {
        return static_cast<int>(m_terminals.size());
    }

    // Retires the head terminal on exact {instance, serial} match only.
    // Wrong identity, serial "0", or an empty FIFO is a no-op: unacked
    // terminals are never skipped or rebound. Mints a new source revision
    // on success: the ack exposes the next terminal, the open gesture, or
    // idle, and a waiter holding the pre-ack revision must wake.
    bool acknowledge(const QString &instance, quint64 serial)
    {
        if (m_terminals.empty() || serial == 0) {
            return false;
        }
        const GestureSnapshot &head = m_terminals.front();
        if (head.instance != instance || head.serial != serial) {
            return false;
        }
        m_terminals.pop_front();
        m_revision += 1;
        return true;
    }

    // The servable snapshot: the oldest unacked terminal first (a newer
    // start never adopts it), else the open gesture, else idle. Never a
    // premature Released: an open gesture serves Started/Updated only. The
    // served snapshot always stamps the newest minted revision.
    GestureSnapshot current() const
    {
        if (!m_terminals.empty()) {
            GestureSnapshot terminal = m_terminals.front();
            terminal.revision = m_revision;
            return terminal;
        }
        if (m_open) {
            GestureSnapshot snapshot;
            snapshot.instance = m_instance;
            snapshot.serial = m_serial;
            snapshot.revision = m_revision;
            snapshot.phase = m_updated ? GesturePhase::Updated : GesturePhase::Started;
            snapshot.win = m_win;
            snapshot.kind = m_kind;
            snapshot.startRect = m_startRect;
            snapshot.hasStartCursor = m_hasStartCursor;
            snapshot.startCursor = m_startCursor;
            snapshot.hasCursor = m_hasCursor;
            snapshot.cursor = m_cursor;
            snapshot.modalMove = m_modalMove;
            return snapshot;
        }
        GestureSnapshot idle;
        idle.instance = m_instance;
        idle.revision = m_revision;
        return idle;
    }

    // Wire JSON for the current snapshot. Field order is pinned: instance,
    // serial (nonzero decimal; "0" names no gesture), revision (monotonic
    // decimal source clock, "0" before any material change), phase, win,
    // kind, frozen start facts, live/terminal cursor, modal truth, then the
    // waiter transport lanes (changed/superseded/available).
    QString currentJson(bool changed, bool superseded, bool available) const
    {
        const GestureSnapshot snapshot = current();
        const bool idle = snapshot.phase == GesturePhase::Idle;
        const QString kind = idle ? QString() : snapshot.kind;
        const QString rect = idle
            ? QStringLiteral("null")
            : QStringLiteral("[%1,%2,%3,%4]")
                  .arg(snapshot.startRect.x())
                  .arg(snapshot.startRect.y())
                  .arg(snapshot.startRect.width())
                  .arg(snapshot.startRect.height());
        return QStringLiteral(
            "{\"v\":1,\"instance\":\"%1\",\"serial\":\"%2\",\"revision\":\"%3\",\"phase\":\"%4\",\"win\":\"%5\",\"kind\":\"%6\","
            "\"startRect\":%7,\"startCursor\":%8,\"cursor\":%9,\"modalMove\":%10,"
            "\"changed\":%11,\"superseded\":%12,\"available\":%13}")
            .arg(snapshot.instance)
            .arg(QString::number(snapshot.serial))
            .arg(QString::number(snapshot.revision))
            .arg(QString::fromUtf8(gesturePhaseName(snapshot.phase)))
            .arg(snapshot.win)
            .arg(kind)
            .arg(rect)
            .arg(idle ? QStringLiteral("null")
                      : GestureLifecycleDetail::pointJson(snapshot.hasStartCursor, snapshot.startCursor))
            .arg(idle ? QStringLiteral("null")
                      : GestureLifecycleDetail::pointJson(snapshot.hasCursor, snapshot.cursor))
            .arg(snapshot.modalMove ? 1 : 0)
            .arg(changed ? 1 : 0)
            .arg(superseded ? 1 : 0)
            .arg(available ? 1 : 0);
    }

private:
    QString m_instance;
    quint64 m_nextSerial = 1;
    quint64 m_revision = 0;
    bool m_open = false;
    quint64 m_serial = 0;
    QString m_win;
    QString m_kind;
    QRect m_startRect;
    bool m_hasStartCursor = false;
    QPoint m_startCursor;
    bool m_hasCursor = false;
    QPoint m_cursor;
    bool m_modalMove = false;
    bool m_updated = false;
    bool m_armed = false;
    std::deque<GestureSnapshot> m_terminals;
};

} // namespace KWin
