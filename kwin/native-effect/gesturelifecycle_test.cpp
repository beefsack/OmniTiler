// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
//
// Authoritative native gesture lifecycle offline tests (Qt only, no KWin
// scene, no session bus, no Rust staticlib): the effect-owned lifecycle
// that mints one immutable identity per native move/resize, freezes start
// facts at Started, owns the phase, and resolves Esc ONLY through the
// causal delivery fence (a passive InputEventFilter at
// InteractiveMoveResize that arms on Esc and is consumed synchronously by
// the finish handler). Geometry alone never proves Esc: the superseded
// drag oracle reports a no-change drag cancelled and a moved drag
// not-cancelled with no cancellation input at all.
//
// Discriminators (decision B):
// - fast start/Esc/finish before the first pull delivers Cancelled, never
//   a premature Released;
// - an Esc observed by the spy but consumed before the delivery fence,
//   followed by a normal finish, delivers Released;
// - a same-window newer start never adopts an unacked older terminal;
// - terminals survive delayed fetch and newer starts (bounded FIFO);
// - identity is immutable across updates and finish;
// - begun-while-open supersedes without inventing a terminal;
// - exact-identity ack only (wrong identity is a no-op);
// - serials are nonzero decimal strings; "0" names no gesture.

#include "gesture_lifecycle.h"

#include <QString>

#include <cstdio>
#include <cstdlib>

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

KWin::GestureLifecycleStore makeStore()
{
    KWin::GestureLifecycleStore store;
    store.setInstance(QStringLiteral("11111111-2222-3333-4444-555555555555"));
    return store;
}

QRect startRect()
{
    return QRect(10, 20, 300, 200);
}

void fastStartEscFinishBeforePullDeliversCancelled()
{
    // The fast path: Started, fence-armed Esc, and finish all land before
    // the script's first pull. The stored terminal is Cancelled with the
    // frozen start facts, never a premature Released.
    KWin::GestureLifecycleStore store = makeStore();
    const quint64 serial = store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(11, 21), true);
    CHECK(serial == 1);
    store.armEsc();
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(60, 70)) == KWin::GesturePhase::Cancelled);
    CHECK(store.terminalCount() == 1);
    const KWin::GestureSnapshot current = store.current();
    CHECK(current.phase == KWin::GesturePhase::Cancelled);
    CHECK(current.serial == serial);
    CHECK(current.instance == QStringLiteral("11111111-2222-3333-4444-555555555555"));
    CHECK(current.win == QStringLiteral("win-1"));
    CHECK(current.kind == QStringLiteral("move"));
    CHECK(current.startRect == startRect());
    CHECK(current.hasStartCursor && current.startCursor == QPoint(11, 21));
    CHECK(current.hasCursor && current.cursor == QPoint(60, 70));
    CHECK(current.modalMove);
    // The wire carries the causal terminal, not a release.
    const QString json = store.currentJson(false, false, true);
    CHECK(json.contains(QStringLiteral("\"serial\":\"1\"")));
    CHECK(json.contains(QStringLiteral("\"phase\":\"cancelled\"")));
    CHECK(!json.contains(QStringLiteral("\"phase\":\"released\"")));
}

void finishWithoutArmDeliversReleased()
{
    KWin::GestureLifecycleStore store = makeStore();
    const quint64 serial = store.beginGesture(QStringLiteral("win-1"), QStringLiteral("resize"), startRect(), false, QPoint(), false);
    CHECK(serial == 1);
    CHECK(store.updateCursor(QStringLiteral("win-1"), true, QPoint(15, 25)));
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(15, 25)) == KWin::GesturePhase::Released);
    CHECK(store.current().phase == KWin::GesturePhase::Released);
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"phase\":\"released\"")));
}

void consumedSpyEscLeavesLaterNormalFinishReleased()
{
    // An Esc observed by the passive pointer/keyboard spy but consumed by
    // an earlier filter never reaches the delivery fence, so no arm exists
    // and the later normal finish is Released. Only the fence arms: the
    // spy path has no store call. A stale arm without an open gesture is
    // dropped, and begin clears any arm, so neither poisons the finish.
    KWin::GestureLifecycleStore store = makeStore();
    store.armEsc(); // no open gesture: dropped, never retained
    CHECK(!store.escArmed());
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(11, 21), true) == 1);
    CHECK(!store.escArmed());
    // Spy-only observation: no armEsc call happens here by construction.
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(60, 70)) == KWin::GesturePhase::Released);
    CHECK(store.current().phase == KWin::GesturePhase::Released);
}

void sameWindowNewStartNeverAdoptsOldTerminal()
{
    // An unacked older terminal keeps its identity under a newer
    // same-window start; the newer start mints a fresh serial and never
    // inherits the old terminal.
    KWin::GestureLifecycleStore store = makeStore();
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(11, 21), true) == 1);
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(60, 70)) == KWin::GesturePhase::Released);
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(12, 22), true) == 2);
    CHECK(store.terminalCount() == 1);
    CHECK(store.current().phase == KWin::GesturePhase::Released);
    CHECK(store.current().serial == 1);
    CHECK(store.current().hasStartCursor && store.current().startCursor == QPoint(11, 21));
    // Exact-identity ack retires the old terminal and reveals the open one.
    CHECK(store.acknowledge(QStringLiteral("11111111-2222-3333-4444-555555555555"), 1));
    CHECK(store.terminalCount() == 0);
    const KWin::GestureSnapshot current = store.current();
    CHECK(current.phase == KWin::GesturePhase::Started);
    CHECK(current.serial == 2);
    CHECK(current.hasStartCursor && current.startCursor == QPoint(12, 22));
}

void terminalSurvivesDelayedFetchAndNewerStarts()
{
    // Terminals wait in the bounded FIFO for exact-identity ack: delayed
    // fetch and newer starts never erase or rebind them.
    KWin::GestureLifecycleStore store = makeStore();
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(11, 21), true) == 1);
    store.armEsc();
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(11, 21)) == KWin::GesturePhase::Cancelled);
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("resize"), QRect(0, 0, 100, 100), false, QPoint(), false) == 2);
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(5, 5)) == KWin::GesturePhase::Released);
    CHECK(store.terminalCount() == 2);
    // Head first: the older Cancelled terminal, with its frozen kind.
    CHECK(store.current().serial == 1);
    CHECK(store.current().phase == KWin::GesturePhase::Cancelled);
    CHECK(store.current().kind == QStringLiteral("move"));
    CHECK(store.acknowledge(QStringLiteral("11111111-2222-3333-4444-555555555555"), 1));
    CHECK(store.current().serial == 2);
    CHECK(store.current().phase == KWin::GesturePhase::Released);
    CHECK(store.current().kind == QStringLiteral("resize"));
    CHECK(store.acknowledge(QStringLiteral("11111111-2222-3333-4444-555555555555"), 2));
    // Both terminals acked, nothing open: idle names no gesture, never a
    // fabricated terminal.
    const KWin::GestureSnapshot idle = store.current();
    CHECK(idle.phase == KWin::GesturePhase::Idle);
    CHECK(idle.serial == 0);
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"serial\":\"0\"")));
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"phase\":\"idle\"")));
}

void identityIsImmutableAcrossUpdatesAndFinish()
{
    KWin::GestureLifecycleStore store = makeStore();
    CHECK(store.beginGesture(QStringLiteral("win-7"), QStringLiteral("move"), startRect(), true, QPoint(11, 21), true) == 1);
    CHECK(store.updateCursor(QStringLiteral("win-7"), true, QPoint(30, 40)));
    CHECK(store.updateCursor(QStringLiteral("win-7"), true, QPoint(50, 60)));
    const KWin::GestureSnapshot open = store.current();
    CHECK(open.phase == KWin::GesturePhase::Updated);
    CHECK(open.serial == 1);
    CHECK(open.win == QStringLiteral("win-7"));
    CHECK(open.kind == QStringLiteral("move"));
    CHECK(open.startRect == startRect());
    CHECK(open.hasStartCursor && open.startCursor == QPoint(11, 21));
    CHECK(open.hasCursor && open.cursor == QPoint(50, 60));
    CHECK(open.modalMove);
    CHECK(store.finishGesture(QStringLiteral("win-7"), true, QPoint(50, 60)) == KWin::GesturePhase::Released);
    const KWin::GestureSnapshot terminal = store.current();
    CHECK(terminal.serial == 1);
    CHECK(terminal.win == QStringLiteral("win-7"));
    CHECK(terminal.kind == QStringLiteral("move"));
    CHECK(terminal.startRect == startRect());
    CHECK(terminal.hasStartCursor && terminal.startCursor == QPoint(11, 21));
    CHECK(terminal.modalMove);
}

void beginWhileOpenSupersedesWithoutTerminal()
{
    // A missed finish (no terminal) never fabricates one: the newer begin
    // supersedes the unfinished open silently and owns the next terminal.
    KWin::GestureLifecycleStore store = makeStore();
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(11, 21), true) == 1);
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(12, 22), true) == 2);
    CHECK(store.terminalCount() == 0);
    CHECK(store.current().phase == KWin::GesturePhase::Started);
    CHECK(store.current().serial == 2);
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(12, 22)) == KWin::GesturePhase::Released);
    CHECK(store.terminalCount() == 1);
    CHECK(store.current().serial == 2);
}

void exactIdentityAckOnly()
{
    KWin::GestureLifecycleStore store = makeStore();
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(11, 21), true) == 1);
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(60, 70)) == KWin::GesturePhase::Released);
    CHECK(!store.acknowledge(QStringLiteral("11111111-2222-3333-4444-555555555555"), 2));
    CHECK(!store.acknowledge(QStringLiteral("99999999-9999-9999-9999-999999999999"), 1));
    CHECK(!store.acknowledge(QStringLiteral("11111111-2222-3333-4444-555555555555"), 0));
    CHECK(store.terminalCount() == 1);
    CHECK(store.acknowledge(QStringLiteral("11111111-2222-3333-4444-555555555555"), 1));
    CHECK(store.terminalCount() == 0);
}

void finishWithoutOpenInventsNothing()
{
    KWin::GestureLifecycleStore store = makeStore();
    CHECK(store.finishGesture(QStringLiteral("win-9"), true, QPoint(1, 1)) == KWin::GesturePhase::Idle);
    CHECK(store.terminalCount() == 0);
    CHECK(store.current().phase == KWin::GesturePhase::Idle);
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(11, 21), true) == 1);
    // A finish naming another window never touches the open gesture.
    CHECK(store.finishGesture(QStringLiteral("win-2"), true, QPoint(1, 1)) == KWin::GesturePhase::Idle);
    CHECK(store.terminalCount() == 0);
    CHECK(store.hasOpen());
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(11, 21)) == KWin::GesturePhase::Released);
}

void badBeginRefusesWithoutMutation()
{
    KWin::GestureLifecycleStore store = makeStore();
    CHECK(store.beginGesture(QString(), QStringLiteral("move"), startRect(), false, QPoint(), false) == 0);
    CHECK(store.beginGesture(QStringLiteral("win\";drop"), QStringLiteral("move"), startRect(), false, QPoint(), false) == 0);
    CHECK(store.beginGesture(QString(200, u'a'), QStringLiteral("move"), startRect(), false, QPoint(), false) == 0);
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("drag"), startRect(), false, QPoint(), false) == 0);
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), QRect(0, 0, 0, 0), false, QPoint(), false) == 0);
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), QRect(20000, 0, 10, 10), false, QPoint(), false) == 0);
    CHECK(!store.hasOpen());
    CHECK(store.terminalCount() == 0);
    // An empty effect instance fails closed: no gesture can be minted.
    KWin::GestureLifecycleStore unseeded;
    CHECK(unseeded.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), false, QPoint(), false) == 0);
}

void escFencePredicateArmsOnlyOnEscPress()
{
    CHECK(KWin::gestureEscArmsFilter(Qt::Key_Escape, true));
    CHECK(!KWin::gestureEscArmsFilter(Qt::Key_Escape, false));
    CHECK(!KWin::gestureEscArmsFilter(Qt::Key_Enter, true));
    CHECK(!KWin::gestureEscArmsFilter(Qt::Key_Return, true));
    CHECK(!KWin::gestureEscArmsFilter(Qt::Key_Space, true));
    CHECK(!KWin::gestureEscArmsFilter(Qt::Key_A, true));
    CHECK(!KWin::gestureEscArmsFilter(Qt::Key_Left, true));
}

void wireSchemaIsExact()
{
    // The native identity is {instance, serial}: serial is a nonzero
    // decimal string, never zero-padded; phase and kind use the shared
    // core wire vocabulary so the broker maps them mechanically.
    KWin::GestureLifecycleStore store = makeStore();
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("resize"), startRect(), false, QPoint(), false) == 1);
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), false, QPoint(), true) == 2);
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(9, 9)) == KWin::GesturePhase::Released);
    const QString json = store.currentJson(false, false, true);
    CHECK(json.startsWith(QStringLiteral("{\"v\":1,")));
    CHECK(json.contains(QStringLiteral("\"instance\":\"11111111-2222-3333-4444-555555555555\"")));
    CHECK(json.contains(QStringLiteral("\"serial\":\"2\"")));
    CHECK(json.contains(QStringLiteral("\"phase\":\"released\"")));
    CHECK(json.contains(QStringLiteral("\"win\":\"win-1\"")));
    CHECK(json.contains(QStringLiteral("\"kind\":\"move\"")));
    CHECK(json.contains(QStringLiteral("\"startRect\":[10,20,300,200]")));
    CHECK(json.contains(QStringLiteral("\"modalMove\":1")));
    CHECK(json.contains(QStringLiteral("\"available\":1")));
    CHECK(!json.contains(QStringLiteral("\"serial\":\"02\"")));
}

void openPhasesTrackProgress()
{
    KWin::GestureLifecycleStore store = makeStore();
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(11, 21), true) == 1);
    CHECK(store.current().phase == KWin::GesturePhase::Started);
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"phase\":\"started\"")));
    CHECK(store.updateCursor(QStringLiteral("win-1"), true, QPoint(12, 22)));
    CHECK(store.current().phase == KWin::GesturePhase::Updated);
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"phase\":\"updated\"")));
}

void sourceRevisionFencesStaleSnapshots()
{
    // Lost-update fence: every material visible-snapshot change mints a
    // new monotonic source revision (decimal string); a Wait naming the
    // same native identity with a stale revision completes immediately
    // instead of holding past the update. Native identity stays
    // {instance, serial}; the revision is transport-only.
    KWin::GestureLifecycleStore store = makeStore();
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"revision\":\"0\"")));
    CHECK(store.beginGesture(QStringLiteral("win-1"), QStringLiteral("move"), startRect(), true, QPoint(11, 21), true) == 1);
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"revision\":\"1\"")));
    // Arming is not evidence: no revision mint.
    store.armEsc();
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"revision\":\"1\"")));
    CHECK(store.updateCursor(QStringLiteral("win-1"), true, QPoint(60, 70)));
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"revision\":\"2\"")));
    // A no-change cursor refresh mints nothing.
    CHECK(store.updateCursor(QStringLiteral("win-1"), true, QPoint(60, 70)));
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"revision\":\"2\"")));
    CHECK(store.finishGesture(QStringLiteral("win-1"), true, QPoint(60, 70)) == KWin::GesturePhase::Cancelled);
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"revision\":\"3\"")));
    // Ack exposing the next state (idle here) mints too.
    CHECK(store.acknowledge(QStringLiteral("11111111-2222-3333-4444-555555555555"), 1));
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"revision\":\"4\"")));
    // Refusals and misses mint nothing.
    CHECK(store.beginGesture(QStringLiteral("win\";drop"), QStringLiteral("move"), startRect(), false, QPoint(), false) == 0);
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"revision\":\"4\"")));
    CHECK(!store.acknowledge(QStringLiteral("11111111-2222-3333-4444-555555555555"), 9));
    CHECK(store.currentJson(false, false, true).contains(QStringLiteral("\"revision\":\"4\"")));
}

} // namespace

int main()
{
    fastStartEscFinishBeforePullDeliversCancelled();
    finishWithoutArmDeliversReleased();
    consumedSpyEscLeavesLaterNormalFinishReleased();
    sameWindowNewStartNeverAdoptsOldTerminal();
    terminalSurvivesDelayedFetchAndNewerStarts();
    identityIsImmutableAcrossUpdatesAndFinish();
    beginWhileOpenSupersedesWithoutTerminal();
    exactIdentityAckOnly();
    finishWithoutOpenInventsNothing();
    badBeginRefusesWithoutMutation();
    escFencePredicateArmsOnlyOnEscPress();
    wireSchemaIsExact();
    openPhasesTrackProgress();
    sourceRevisionFencesStaleSnapshots();
    if (failures != 0) {
        std::fprintf(stderr, "gesturelifecycle_test: %d failures\n", failures);
        return EXIT_FAILURE;
    }
    std::printf("gesturelifecycle_test: all passed\n");
    return EXIT_SUCCESS;
}
