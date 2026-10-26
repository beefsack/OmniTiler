// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#pragma once

#include "presentation_consumer.h"

#include <effect/effect.h>
#include <effect/effectwindow.h>
#include <scene/imageitem.h>
#include <scene/outlinedborderitem.h>

#include <QDBusConnection>
#include <QHash>
#include <QPointF>
#include <QPointer>
#include <QRect>
#include <QRectF>
#include <QSet>
#include <QString>


namespace KWin
{

class GestureEscFence;
class GestureFactsSource;

class ActiveWindowBorderEffect : public Effect
{
    Q_OBJECT

public:
    ActiveWindowBorderEffect();
    ~ActiveWindowBorderEffect() override;

    // Read-only presentation paint (Stage 5, pure renderer): replaces the
    // held paint with accepted core shapes only (border, underlay, drop
    // preview) plus the serving generation/revision for order fencing.
    // Malformed payloads clear the display; same-generation older revisions
    // never resurrect it. Never derives geometry, eligibility, or policy.
    void applyPresentation(const QString &payload);
    void clearPresentation();
    QString presentationStatus() const;
    // Versioned raw presentation facts for the script route (Stage 3
    // ingress, read-only): pushes the observed chord level and the actual
    // style configuration into the held facts source on the well-known
    // object. The source bumps its revision only on material change.
    void noteFactsChord();
    void noteFactsStyle();

private:
    friend class GestureEscFence;
    void reconfigure(ReconfigureFlags flags) override;
    void setTrackedWindow(EffectWindow *window);
    void subscribeMaximize(EffectWindow *window);
    void unsubscribeMaximize(EffectWindow *window);
    // Group member visibility observation: every window subscribes its
    // existing public minimizedChanged/windowHiddenChanged signals once;
    // the handler re-anchors only for presentation-held member ids, so
    // minimizing/restoring a lower member preserves the underlay without
    // timers or polling. Precise disconnects never touch the tracked-window
    // signals sharing minimizedChanged.
    void subscribeGroupVisibility(EffectWindow *window);
    void unsubscribeGroupVisibility(EffectWindow *window);
    void onGroupMemberVisibilityChanged(EffectWindow *window);
    // Authoritative gesture lifecycle observation (decision B): one
    // effect-owned lifecycle mints immutable {instance, serial} identity,
    // freezes start facts at Started, owns the phase, and resolves Esc
    // only through the causal delivery fence. The gesture endpoint
    // reports the terminal, never geometry inference: the retired oracle
    // verdict route is gone, and the core GestureIngress lane alone owns
    // preview/drop/resize/hover.
    void attachGestureWindow(EffectWindow *window);
    void forgetGestureWindow(EffectWindow *window);
    void onGestureDragStart(EffectWindow *window);
    void onGestureDragStep(EffectWindow *window);
    void onGestureDragFinish(EffectWindow *window);
    // Delivery-fence arm (passive only): called by the fence filter when an
    // Esc reaches InteractiveMoveResize. Arms only when the exact currently
    // active native move/resize window matches the tracked open native
    // identity in an interactive mode; records nothing by itself, and the
    // next finish consumes the arm synchronously.
    void noteGestureEsc();
    // Captures the exact interactive-move window at Started; the preview
    // stays gated on this live liveness plus the accepted paint each
    // refresh. The preview's published `win` is the insertion neighbor,
    // never the mover: no identity match against the mover is performed
    // here.
    void updateGroupMoveArm(EffectWindow *window);
    // Live interactive-move liveness for the drop preview: a non-deleted
    // tracked move window exists. No identity or policy decision.
    bool groupMoveLiveNow() const;
    void updateMaximizedState(EffectWindow *window, bool maximized);
    // Native geometry-freshness vetoes (findings F/G): the tracked frame
    // changing dirties the border paint, and any held member frame changing
    // dirties the underlay paint (the underlay depends on ALL members, not
    // just the anchor). Both stay hidden until a strictly newer accepted
    // paint arrives; no live-frame comparison, so same-window moves and
    // A->B->A races never resurrect stale shapes without a fresh paint.
    void onTrackedFrameChanged();
    void onGroupMemberFrameChanged(EffectWindow *window);
    // Pure-renderer paint: each shape renders the held accepted core rects
    // verbatim (KWin logical units; item anchoring translates spaces without
    // a second outset or scaling) gated on the live foreground match, the
    // geometry-freshness floors, plus immediate hide-only vetoes
    // (deleted/minimized/fullscreen/hidden/maximized/applet/endpoint). The
    // border ring replays the published outer exactly: its stroke is derived
    // strictly from the published inner/outer pair (half the uniform inset),
    // never from live config width. No eligibility, reason, or geometry
    // derivation.
    void updateBorder();
    void updateOutline();
    void updateDragPreview();
    void updateDragPreviewFill();
    void updateGroupUnderlayFill();
    // Lowest-stacked renderable presentation member anchor: picks the first
    // stacking-order window whose bare-UUID internalId matches the held
    // paint member list and whose EffectWindow isVisible() plus WindowItem
    // effective visibility hold (skips minimized/hidden/off-current members
    // whose item would hide the underlay), reparents the filled underlay
    // there, and remaps the stored core outer rect into the anchor item
    // coordinates. The stored outer rect is consumed verbatim: the
    // gap/width/extension expansion already happened core-side. Tracks the
    // current anchor's windowFrameGeometryChanged to remap on frame moves
    // without a fresh payload; disconnects the old anchor on change/clear.
    // Member minimizedChanged/windowHiddenChanged re-anchor via
    // onGroupMemberVisibilityChanged. No logging of ids.
    void updateGroupAnchorAndGeometry();
    // Existing live observation: the committed maximizeMode() seed plus the
    // native transition signals stay authoritative for the hide veto only;
    // they never derive visibility.
    bool nativeMaximizedNow(EffectWindow *window) const;
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
    // Live hide-only veto shared by every shape: a live active window that
    // is not deleted/minimized/fullscreen/hidden/maximized/applet. Core
    // visibility plus the foreground match decide; this only vetoes.
    bool isLiveEligibleNow() const;
    // Live foreground match against the held paint: the active window's
    // bare-UUID identity equals the published foreground token (non-empty).
    // Stale paints never render under a new focus.
    bool foregroundMatchesNow() const;

    const bool m_isOpenGL;
    OutlinedBorderItem m_borderItem;
    // Filled-translucent group underlay below every group member (ImageItem
    // with a 1x1 solid configured-color image scaled to the outer rect).
    // Value member for auto-lifetime; visual parent is the lowest-stacked
    // renderable member WindowItem at Z=-2 so it slides with the workspace and higher
    // members occlude it normally. Detached (null parent) with no valid
    // anchor so it can never draw as a screen overlay; stays hidden there.
    ImageItem m_groupItem;
    // Held accepted presentation paint: complete core shapes plus the
    // serving generation/revision for order fencing. The member id list
    // feeds the lowest-stacked anchor choice only; geometry is consumed
    // verbatim. Never logged.
    PresentationPaint m_presentation;
    QPointer<EffectWindow> m_groupAnchor;
    // Exact interactive-move window captured at Started (latest gesture
    // wins). The preview needs this liveness plus the held paint, so a
    // stale preview alone never shows. Cleared on Finish/cancel or removal
    // of that exact window. Never compared against the published
    // insertion-neighbor identity.
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
    // Exact owned teardown only: unregister the object/service on this
    // connection, then disconnectFromBus(name); never touches the shared
    // sessionBus registrations.
    QString m_groupBusName;
    QDBusConnection m_groupBus;
    // Authoritative gesture lifecycle (decision B): the effect-owned
    // D-Bus source plus the passive Esc fence. The source registers on the
    // effect-owned dedicated connection (same bus as the ActiveBorder
    // endpoint), so held WaitGesture replies route cross-connection. The
    // fence installs at InteractiveMoveResize weight after the builtin
    // consumer exists, so lower_bound ordering places it immediately before
    // the builtin: it observes Esc after earlier consumers and never
    // consumes. Both tear down with the effect exactly.
    QSet<EffectWindow *> m_gestureAttached;
    QHash<EffectWindow *, QString> m_gestureWinIds;
    GestureFactsSource *m_gestureSource = nullptr;
    GestureEscFence *m_gestureFence = nullptr;
    bool m_gestureDbusAvailable = false;
    bool m_gestureEndpointFailedLogged = false;
    bool ensureGestureEndpointRegistered();
    void ensureGestureFence();
    void emitGestureDiag(const char *outcome, const char *phase);
    void emitGestureEndpoint();
    void logActiveBorderDiag(const QString &message);
    void emitActiveBorderEndpoint();
    // Bounded presentation observability only: setter receipt plus a
    // combined anchor/visibility transition. Fixed tokens and bits, no
    // identities, payload, geometry, or revision numbers. Failures never
    // gate painting.
    void emitPresentationSetterDiag(const char *outcome);
    void emitPresentationTransitionDiag();
    void syncPresentationTransitionDiag();
    // Shared idempotent D-Bus registration: only reports success when both
    // the well-known service name and the object path register; on partial
    // success rolls back only what this attempt acquired. Never unregisters
    // an endpoint owned elsewhere. No timers or polling.
    bool ensureDbusEndpoint(const QString &service, const QString &path, QObject *object, bool *serviceOkOut, bool *objectOkOut);
    bool ensureDbusEndpointOn(QDBusConnection &bus, const QString &service, const QString &path, QObject *object,
        bool *serviceOkOut, bool *objectOkOut);
    // ActiveBorder endpoint on the effect-owned dedicated connection (same
    // bus, same service/path/interface/shape). Establishes the connection
    // once, binds the group object (and its facts source) to exactly that
    // connection, and registers there. Fail/recover on the existing
    // activation/reconfigure events; no retry polling.
    bool ensureGroupEndpointRegistered();
    void ensureEndpointsRegistered();
    void emitActiveBorderVisible(bool visible, const char *reason, bool appletPopup);
    bool m_groupVisible = false;
    bool m_chordHeld = false;
    bool m_firstMouseSeen = false;
    bool m_groupDbusAvailable = false;
    // Once-each redacted transition state: first partial/unavailable log per
    // endpoint and press spy, plus a single recovery log on each recovery.
    // No repeated registration while healthy, no repeated logging while ill.
    bool m_groupEndpointFailedLogged = false;
    // Bounded visibility diagnostic edge state (two scalars, no ledger):
    // whether the first updateBorder() evaluation was emitted, and the last
    // emitted visibility. updateBorder() emits exactly on first evaluation
    // and then only when computed visibility flips.
    bool m_borderDiagEmitted = false;
    bool m_borderDiagVisible = false;
    // Current anchor outcome for diagnostics plus the last emitted
    // transition (anchor reason + visibility). Setter syncs without
    // emitting; non-setter paths emit only on material change.
    const char *m_presentationAnchorDiag = "no-presentation";
    const char *m_presentationTransDiagAnchor = "no-presentation";
    bool m_presentationTransDiagVisible = false;
    bool m_presentationTransDiagEmitted = true;
};

} // namespace KWin
