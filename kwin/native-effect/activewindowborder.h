// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#pragma once

#include "drag_oracle_ffi.h"
#include "group_highlight_ffi.h"

#include <effect/effect.h>
#include <effect/effectwindow.h>
#include <scene/imageitem.h>
#include <scene/outlinedborderitem.h>

#include <QByteArray>
#include <QDBusConnection>
#include <QHash>
#include <QPointF>
#include <QPointer>
#include <QRect>
#include <QRectF>
#include <QSet>
#include <QString>
#include <QStringList>

#include <chrono>
#include <cstdint>

namespace KWin
{

struct PointerButtonEvent;
class OraclePressSpy;

class ActiveWindowBorderEffect : public Effect
{
    Q_OBJECT

public:
    ActiveWindowBorderEffect();
    ~ActiveWindowBorderEffect() override;

    void applyGroupHighlight(const QString &payload);
    void clearGroupHighlight();
    QString groupHighlightStatus() const;
    // Versioned raw presentation facts for the script route (Stage 3
    // ingress, read-only): pushes the observed chord level and the actual
    // style configuration into the held facts source on the well-known
    // object. The source bumps its revision only on material change.
    void noteFactsChord();
    void noteFactsStyle();
    // Independent drag-target preview: screen-space filled rectangle above
    // windows, window-agnostic (no EffectWindow retained). Visible only
    // between a valid set and an explicit clear; never gated on Meta, focus,
    // or the group outline.
    void setDragTargetPreview(int x, int y, int w, int h);
    void clearDragTargetPreview();

private:
    friend class OraclePressSpy;
    void reconfigure(ReconfigureFlags flags) override;
    void setTrackedWindow(EffectWindow *window);
    void subscribeMaximize(EffectWindow *window);
    void unsubscribeMaximize(EffectWindow *window);
    // Group member visibility observation: every window subscribes its
    // existing public minimizedChanged/windowHiddenChanged signals once;
    // the handler re-anchors only for Rust-accepted member ids, so
    // minimizing/restoring a lower member preserves the underlay without
    // timers or polling. Precise disconnects never touch the tracked-window
    // signals sharing minimizedChanged.
    void subscribeGroupVisibility(EffectWindow *window);
    void unsubscribeGroupVisibility(EffectWindow *window);
    void onGroupMemberVisibilityChanged(EffectWindow *window);
    void attachOracleWindow(EffectWindow *window);
    void forgetOracleWindow(EffectWindow *window);
    void onOracleDragStart(EffectWindow *window);
    void onOracleDragFinish(EffectWindow *window);
    // Captures the exact interactive-move window at Started; display stays
    // gated on a fresh live-active plus accepted-subject match each refresh.
    void updateGroupMoveArm(EffectWindow *window);
    // Fresh move trigger: captured window revalidated against the live
    // active window and the accepted Rust subject on every call.
    bool groupMoveMatchesNow() const;
    // Passive press capture: a public InputEventSpy observes pointer presses
    // without grabbing or intercepting. A press matching the live configured
    // MouseUnrestrictedResize binding (or the Alt+Right source default while
    // the public options are unavailable) overwrites the single candidate;
    // any other press clears it. At drag start a candidate matching the exact
    // same effect window and identity within the bounded monotonic age moves
    // single-use into the per-window start slot, which the finish handler
    // passes to the verdict atomically. Move-vs-resize is never decided here.
    void noteOraclePointerPress(PointerButtonEvent *event);
    void emitOraclePressDiag(const char *outcome, bool configured);
    void updateMaximizedState(EffectWindow *window, bool maximized);
    void updateBorder();
    void updateOutline();
    void updateDragPreview();
    void updateDragPreviewFill();
    void updateGroupUnderlayFill();
    // Lowest-stacked renderable group member anchor: picks the first
    // stacking-order window whose bare-UUID internalId matches the
    // Rust-accepted member list and whose EffectWindow isVisible() plus
    // WindowItem effective visibility hold (skips minimized/hidden/
    // off-current members whose item would hide the underlay), reparents
    // the filled underlay there, and remaps the stored union outer rect
    // into the anchor item coordinates. Tracks the current anchor's
    // windowFrameGeometryChanged to remap on frame moves without a fresh
    // payload; disconnects the old anchor on change/clear. Member
    // minimizedChanged/windowHiddenChanged re-anchor via
    // onGroupMemberVisibilityChanged. No logging of ids.
    void updateGroupAnchorAndGeometry();
    // Compile-version gate (see CMakeLists OMNITILER_KWIN_PAINTSCREEN_BOOL):
    // KWin 6.8 changed Effect::paintScreen from void to [[nodiscard]] bool.
    // The 6.7 spelling keeps the void passthrough; the 6.8 spelling
    // propagates the painting result. Identical call, no policy.
#ifdef OMNITILER_KWIN_PAINTSCREEN_BOOL
    [[nodiscard]] bool paintScreen(const RenderTarget &renderTarget, const RenderViewport &viewport, int mask, const Region &deviceRegion, LogicalOutput *screen) override;
#else
    void paintScreen(const RenderTarget &renderTarget, const RenderViewport &viewport, int mask, const Region &deviceRegion, LogicalOutput *screen) override;
#endif
    void updateGroupVisibility();
    void onMouseChanged(const QPointF &pos, const QPointF &oldPos, Qt::MouseButtons buttons, Qt::MouseButtons oldButtons,
        Qt::KeyboardModifiers modifiers, Qt::KeyboardModifiers oldModifiers);
    bool isGroupFocusEligible() const;

    const bool m_isOpenGL;
    OutlinedBorderItem m_borderItem;
    // Filled-translucent group underlay below every group member (ImageItem
    // with a 1x1 solid configured-color image scaled to the outer rect).
    // Value member for auto-lifetime; visual parent is the lowest-stacked
    // renderable member WindowItem at Z=-2 so it slides with the workspace and higher
    // members occlude it normally. Detached (null parent) with no valid
    // anchor so it can never draw as a screen overlay; stays hidden there.
    ImageItem m_groupItem;
    // Rust-accepted member bare-UUID strings plus the lowest-stacked anchor.
    // Extracted native-side from the already-accepted payload with Qt JSON;
    // no new FFI storage. Never logged.
    QStringList m_groupMemberIds;
    QPointer<EffectWindow> m_groupAnchor;
    // Exact interactive-move window captured at Started (latest gesture
    // wins). Display needs a fresh live-active plus accepted-subject match,
    // so a stale pointer alone never shows. Cleared on Finish/cancel or
    // removal of that exact window; the chord stays independent.
    QPointer<EffectWindow> m_groupMoveWindow;
    // Filled-translucent preview above windows (ImageItem with a 1x1 solid
    // configured-color image scaled to the stored rect). Value member for
    // auto-lifetime with the effect; visual parent is the scene overlay.
    ImageItem m_dragPreviewItem;
    QRect m_dragPreviewRect;
    bool m_dragPreviewVisible = false;
    QPointer<EffectWindow> m_trackedWindow;
    QSet<EffectWindow *> m_maximizedWindows;
    QSet<EffectWindow *> m_maximizeSubscribed;
    QSet<EffectWindow *> m_groupVisibilitySubscribed;
    QObject *m_groupDbusObject = nullptr;
    // Effect-owned dedicated connection to the same session bus for the
    // ActiveBorder endpoint. Qt forbids delayed replies on local-loop
    // (same-connection) messages, so the held WaitPresentationFacts waiter
    // cannot complete when the endpoint and the script caller share
    // QDBusConnection::sessionBus. The dedicated connection reaches the
    // same bus (same service/path/interface/shape; the script still calls
    // via KWin's sessionBus), so held replies route cross-connection.
    // The DragOracle endpoint stays on the shared sessionBus (fire-and-read
    // verdict, no held waiter). Exact owned teardown only: unregister the
    // object/service on this connection, then disconnectFromBus(name);
    // never touches the shared sessionBus registrations.
    QString m_groupBusName;
    QDBusConnection m_groupBus;
    // Drag oracle state: inert read-only
    // observer state only (start rects plus the D-Bus object). The verdict
    // policy lives in the std-only Rust staticlib behind the POD C ABI.
    QObject *m_oracleDbusObject = nullptr;
    QHash<EffectWindow *, QRect> m_oracleStartRects;
    QSet<EffectWindow *> m_oracleAttached;
    // Single overwrite-on-press candidate plus one single-use press slot per
    // window pending between drag start and finish. Coordinates keep full
    // native precision (f64) so script thirds comparisons match exactly.
    struct OraclePressCandidate {
        QPointer<EffectWindow> window;
        QByteArray identity;
        double x = 0.0;
        double y = 0.0;
        bool configured = false;
        bool hasPress = false;
        std::chrono::steady_clock::time_point at{};
    };
    OraclePressCandidate m_oraclePress;
    QHash<EffectWindow *, DragOraclePress> m_oracleStartPress;
    OraclePressSpy *m_oraclePressSpy = nullptr;
    bool m_oraclePressDefaultLogged = false;
    // Pure group policy state lives in the std-only Rust staticlib; C++
    // holds it by value and forwards QString-to-UTF8 bytes plus POD
    // observer flags. Rendering reads the POD rect back out.
    GroupHighlightState m_groupState{};
    void logActiveBorderDiag(const QString &message);
    void emitActiveBorderEndpoint();
    void emitOracleEndpoint();
    // Bounded group-highlight observability only: setter receipt plus a
    // combined anchor/visibility transition. Fixed tokens and bits, no
    // identities, payload, or geometry. Failures never gate painting.
    void emitGroupSetterDiag(const char *outcome);
    void emitGroupTransitionDiag();
    void syncGroupTransitionDiag();
    // Shared idempotent D-Bus registration: only reports success when both
    // the service name and the object path register; on partial success
    // rolls back only what this attempt acquired. Never unregisters an
    // endpoint owned elsewhere. No timers or polling.
    bool ensureDbusEndpoint(const QString &service, const QString &path, QObject *object, bool *serviceOkOut, bool *objectOkOut);
    bool ensureDbusEndpointOn(QDBusConnection &bus, const QString &service, const QString &path, QObject *object,
        bool *serviceOkOut, bool *objectOkOut);
    // ActiveBorder endpoint on the effect-owned dedicated connection (same
    // bus, same service/path/interface/shape). Establishes the connection
    // once, binds the group object (and its facts source) to exactly that
    // connection, and registers there. Fail/recover on the existing
    // activation/reconfigure events; no retry polling.
    bool ensureGroupEndpointRegistered();
    void ensureOraclePressSpy();
    void ensureEndpointsRegistered();
    void emitActiveBorderVisible(bool visible, const char *reason, bool appletPopup);
    bool m_groupVisible = false;
    bool m_chordHeld = false;
    bool m_firstMouseSeen = false;
    bool m_groupDbusAvailable = false;
    bool m_oracleDbusAvailable = false;
    // Once-each redacted transition state: first partial/unavailable log per
    // endpoint and press spy, plus a single recovery log on each recovery.
    // No repeated registration while healthy, no repeated logging while ill.
    bool m_groupEndpointFailedLogged = false;
    bool m_oracleEndpointFailedLogged = false;
    bool m_pressSpyFailedLogged = false;
    // Bounded visibility diagnostic edge state only (two scalars, no ledger):
    // whether the first updateBorder() evaluation was emitted, and the last
    // emitted visibility. updateBorder() emits exactly on first evaluation
    // and then only when computed visibility flips.
    bool m_borderDiagEmitted = false;
    bool m_borderDiagVisible = false;
    // Current anchor outcome for diagnostics plus the last emitted
    // transition (anchor reason + visibility). Setter syncs without
    // emitting; non-setter paths emit only on material change.
    const char *m_groupAnchorDiag = "no-group";
    const char *m_groupTransDiagAnchor = "no-group";
    bool m_groupTransDiagVisible = false;
    bool m_groupTransDiagEmitted = true;
};

} // namespace KWin
