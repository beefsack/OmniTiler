// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#include "activewindowborder.h"
#include "activeborderconfig.h"
#include "activeborderlogic.h"
#include "drag_oracle_ffi.h"
#include "oraclepress.h"
#include "presentationfacts.h"

#include <KColorScheme>
#include <KSharedConfig>

#include <core/inputdevice.h>
#include <effect/effecthandler.h>
#include <input.h>
#include <input_event.h>
#include <input_event_spy.h>
#include <options.h>
#include <scene/workspacescene.h>
#include <scene/windowitem.h>
#include <window.h>

#include <QByteArray>
#include <QColor>
#include <QDBusConnection>
#include <QDBusContext>
#include <QDBusError>
#include <QDBusMessage>
#include <QImage>
#include <QJsonArray>
#include <QJsonDocument>
#include <QJsonObject>
#include <QJsonValue>
#include <QLoggingCategory>
#include <QPalette>
#include <QTimer>
#include <QUuid>

#include <chrono>
#include <cmath>
#include <cstddef>
#include <cstdint>

namespace KWin
{

Q_LOGGING_CATEGORY(lcActiveBorder, "omnitiler.activeborder");

namespace
{

class GroupHighlightObject : public QObject, protected QDBusContext
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "com.omnitiler.ActiveBorder1")

public:
    explicit GroupHighlightObject(ActiveWindowBorderEffect *effect, const QDBusConnection &connection,
        QObject *parent = nullptr)
        : QObject(parent)
        , m_effect(effect)
        , m_bus(connection)
        , m_facts(new PresentationFactsSource(connection, this))
    {
    }

    PresentationFactsSource *facts() const
    {
        return m_facts;
    }

public Q_SLOTS:
    Q_SCRIPTABLE void SetGroupHighlight(const QString &payload)
    {
        if (m_effect) {
            m_effect->applyGroupHighlight(payload);
        }
    }
    Q_SCRIPTABLE void ClearGroupHighlight()
    {
        if (m_effect) {
            m_effect->clearGroupHighlight();
        }
    }
    Q_SCRIPTABLE void SetDragTargetPreview(int x, int y, int w, int h)
    {
        if (m_effect) {
            m_effect->setDragTargetPreview(x, y, w, h);
        }
    }
    Q_SCRIPTABLE void ClearDragTargetPreview()
    {
        if (m_effect) {
            m_effect->clearDragTargetPreview();
        }
    }
    Q_SCRIPTABLE QString GetGroupHighlightStatus()
    {
        return m_effect ? m_effect->groupHighlightStatus() : QStringLiteral("v=1;rx=0;ok=0;parse_rej=0;focus_mm=0;stale=0;clr=0;has=0;ord=0;first=0;chord=0;move=0;foc=0;ep=0;gl=0;vis=0");
    }
    // Versioned raw presentation facts for the KWin script route (Stage 3
    // ingress, read-only, no policy). The request carries the
    // script-known identity as JSON
    // ({"instance":"...","rev":"...","leaseMs":20000}); a mismatched
    // identity completes immediately with the fresh snapshot, otherwise
    // the call holds until a fact changes or the bounded lease expires.
    // Delayed reply: the effect owns the pending message, never the bus.
    Q_SCRIPTABLE void WaitPresentationFacts(const QString &request, const QDBusMessage &message)
    {
        if (!calledFromDBus()) {
            return;
        }
        setDelayedReply(true);
        if (m_effect) {
            m_facts->handleWait(request, message);
            return;
        }
        m_bus.send(message.createErrorReply(QDBusError::InvalidArgs,
            QStringLiteral("presentation facts unavailable")));
    }

private:
    ActiveWindowBorderEffect *m_effect = nullptr;
    // Exact connection the ActiveBorder object is registered on (the
    // effect-owned dedicated connection): error replies route here, never
    // the shared sessionBus.
    QDBusConnection m_bus;
    // Versioned raw presentation facts for the script route (Stage 3
    // ingress, read-only): the same production class the offline transport
    // test drives, held on this well-known object.
    PresentationFactsSource *m_facts = nullptr;
};

class LastVerdictObject : public QObject
{
    Q_OBJECT
    Q_CLASSINFO("D-Bus Interface", "com.omnitiler.DragOracle1")

public:
    using QObject::QObject;

public Q_SLOTS:
    Q_SCRIPTABLE QString LastVerdict() const
    {
        // Copy the verdict before the D-Bus return so the reply never borrows
        // the oracle's process-static storage across threads or reentrancy.
        // The reply carries the exact existing fields plus, only when a
        // resize-binding press was verified for the same window, an optional
        // bounded "press" object (native f64 position, closed-vocabulary
        // binding token). No second call, service, or object.
        uint8_t copy[1024];
        const size_t taken = drag_oracle_last_copy(copy, sizeof(copy));
        if (taken == 0 || taken > sizeof(copy)) {
            return QStringLiteral("{\"v\":1,\"cancelled\":true,\"finalRect\":{\"x\":0,\"y\":0,\"w\":1,\"h\":1},\"windowIdentity\":\"\",\"correlation\":\"drag-0\",\"reason\":\"oracle-unavailable\"}");
        }
        return QString::fromUtf8(reinterpret_cast<const char *>(copy), static_cast<int>(taken));
    }
};

} // namespace

// Passive press observer at KWin scope (matching the header forward
// declaration): sees every pointer button event before filters but never
// consumes, grabs, or modifies anything. Forwards presses to the effect,
// which matches them against the resize binding and window.
class OraclePressSpy : public InputEventSpy
{
public:
    explicit OraclePressSpy(ActiveWindowBorderEffect *effect)
        : m_effect(effect)
    {
    }

    void pointerButton(PointerButtonEvent *event) override
    {
        if (m_effect != nullptr) {
            m_effect->noteOraclePointerPress(event);
        }
    }

private:
    ActiveWindowBorderEffect *m_effect = nullptr;
};

namespace
{

DragOracleRect toOraclePod(const QRect &rect)
{
    DragOracleRect out{};
    out.x = rect.x();
    out.y = rect.y();
    out.w = rect.width();
    out.h = rect.height();
    return out;
}

QRect quantizedOracleRect(const RectF &geometry)
{
    return QRect(static_cast<int>(std::lround(geometry.x())), static_cast<int>(std::lround(geometry.y())),
        static_cast<int>(std::lround(geometry.width())), static_cast<int>(std::lround(geometry.height())));
}

QRect oracleMoveResizeRect(EffectWindow *window)
{
    if (window == nullptr) {
        return QRect();
    }
    if (Window *inner = window->window()) {
        return quantizedOracleRect(inner->moveResizeGeometry());
    }
    return QRect();
}

// Single fixed reason token for the computed active-border visibility. Order
// matches updateBorder() evaluation so exactly one token distinguishes the
// first suppressing gate: endpoint, window presence, deleted, minimized,
// fullscreen, maximized, then applet popup.
const char *activeBorderDiagReason(bool hasWindow, bool deleted, bool minimized, bool fullScreen, bool nativeMaximized,
    bool appletPopup, bool dbusAvailable)
{
    if (!dbusAvailable) {
        return "endpoint-unavailable";
    }
    if (!hasWindow) {
        return "no-window";
    }
    if (deleted) {
        return "deleted";
    }
    if (minimized) {
        return "minimized";
    }
    if (fullScreen) {
        return "fullscreen";
    }
    if (nativeMaximized) {
        return "maximized";
    }
    if (appletPopup) {
        return "applet-popup";
    }
    return "eligible";
}

} // namespace

ActiveWindowBorderEffect::ActiveWindowBorderEffect()
    : m_isOpenGL(effects->isOpenGLCompositing())
    , m_borderItem(RectF(), BorderOutline())
    , m_groupBusName(QStringLiteral("omnitiler-active-border-endpoint"))
    , m_groupBus(QDBusConnection::connectToBus(QDBusConnection::SessionBus, m_groupBusName))
{
    ActiveBorderConfig::instance(QStringLiteral("kwinrc"));
    updateOutline();
    updateGroupUnderlayFill();

    group_highlight_state_init(&m_groupState);
    // Fail closed with no false endpoint expectation: the group stays
    // unavailable/clear unless both the well-known service and object
    // register. A transient failure retries on later activation/reconfigure
    // events, never by timer. The ActiveBorder object binds the
    // effect-owned dedicated connection (cross-connection held replies);
    // the oracle stays on the shared sessionBus.
    m_oracleDbusObject = new LastVerdictObject(this);
    ensureEndpointsRegistered();
    if (!m_groupDbusAvailable) {
        group_highlight_clear(&m_groupState);
    }

    // Oracle observation is independent of rendering. Keep this one shared
    // lifecycle hookup set active even when the border cannot render.
    connect(effects, &EffectsHandler::windowDeleted, this, [this](EffectWindow *window) {
        unsubscribeMaximize(window);
        unsubscribeGroupVisibility(window);
        forgetOracleWindow(window);
        m_maximizedWindows.remove(window);
        if (!m_isOpenGL) {
            return;
        }
        if (m_trackedWindow == window) {
            setTrackedWindow(nullptr);
        }
        updateBorder();
        updateGroupAnchorAndGeometry();
        updateGroupVisibility();
        emitGroupTransitionDiag();
    });
    connect(effects, &EffectsHandler::windowClosed, this, [this](EffectWindow *window) {
        unsubscribeMaximize(window);
        unsubscribeGroupVisibility(window);
        forgetOracleWindow(window);
        m_maximizedWindows.remove(window);
        if (!m_isOpenGL) {
            return;
        }
        updateBorder();
        updateGroupAnchorAndGeometry();
        updateGroupVisibility();
        emitGroupTransitionDiag();
    });
    // Global maximize tracking and the oracle observe every window,
    // including when the active border cannot render. Each observed window
    // seeds its committed maximizeMode() directly: windows already maximized
    // before effect load emit no transition, so the seed is authoritative
    // until native transition signals update it.
    for (EffectWindow *window : effects->stackingOrder()) {
        subscribeMaximize(window);
        subscribeGroupVisibility(window);
        attachOracleWindow(window);
    }
    connect(effects, &EffectsHandler::windowAdded, this, [this](EffectWindow *window) {
        subscribeMaximize(window);
        subscribeGroupVisibility(window);
        attachOracleWindow(window);
        if (m_isOpenGL) {
            updateGroupAnchorAndGeometry();
            updateGroupVisibility();
            emitGroupTransitionDiag();
        }
    });
    // Lowest-stacked renderable anchor follows stacking and member
    // visibility changes; geometry remaps into the new anchor item
    // coordinates. Visibility gates stay unchanged.
    connect(effects, &EffectsHandler::stackingOrderChanged, this, [this]() {
        if (!m_isOpenGL) {
            return;
        }
        updateGroupAnchorAndGeometry();
        updateGroupVisibility();
        emitGroupTransitionDiag();
    });
    // Activation recovery runs even when OpenGL rendering is off: the oracle
    // endpoint and press spy stay useful without a visible border. Rendering
    // below stays OpenGL-gated; focus-clear ordering is preserved.
    connect(effects, &EffectsHandler::windowActivated, this, [this](EffectWindow *) {
        ensureEndpointsRegistered();
        if (!m_isOpenGL) {
            return;
        }
        // Focus activation clears the old group immediately before any
        // asynchronous script refresh, so no stale group renders under the
        // new active focus while Meta is held.
        clearGroupHighlight();
        setTrackedWindow(effects->activeWindow());
        updateBorder();
        updateGroupVisibility();
        emitGroupTransitionDiag();
    });

    if (!m_isOpenGL) {
        return;
    }

    // Keep the active outline in the target window subtree so higher windows
    // occlude it normally instead of treating it as a screen-wide overlay.
    m_borderItem.setZ(-1);
    // Group underlay below every group member: filled translucent rectangle
    // anchored under the lowest-stacked renderable member WindowItem at Z=-2 so it
    // slides with the workspace and higher members occlude it normally.
    // A lower non-group window edge may be painted over where the extension
    // overlaps it (accepted). Detached (null parent) with no anchor so it
    // can never draw as a screen overlay; stays hidden there.
    m_groupItem.setParentItem(nullptr);
    m_groupItem.setZ(-2);
    m_groupItem.setVisible(false);

    // Independent drag-target preview above windows: filled translucent
    // rectangle (1x1 solid config-color image scaled to the stored rect).
    // Shown only between a valid set and an explicit clear; never gated on
    // Meta, focus, or the group underlay. Z=10 keeps it above windows
    // within the overlay.
    updateDragPreviewFill();
    m_dragPreviewItem.setParentItem(effects->scene()->overlayItem());
    m_dragPreviewItem.setZ(10);
    m_dragPreviewItem.setVisible(false);

    connect(effects, &EffectsHandler::mouseChanged, this, &ActiveWindowBorderEffect::onMouseChanged);

    setTrackedWindow(effects->activeWindow());
    updateBorder();
    updateGroupAnchorAndGeometry();
    updateGroupVisibility();
}

ActiveWindowBorderEffect::~ActiveWindowBorderEffect()
{
    delete m_oraclePressSpy;
    m_oraclePressSpy = nullptr;
    // Release only endpoints this effect registered, never an endpoint owned
    // elsewhere after a failed or partial registration. The ActiveBorder
    // endpoint tears down on its exact owned connection (unregister there,
    // then disconnect that named connection); the shared sessionBus and the
    // oracle endpoint on it are untouched by the ActiveBorder teardown.
    QDBusConnection bus = QDBusConnection::sessionBus();
    if (m_oracleDbusAvailable) {
        bus.unregisterObject(QStringLiteral("/com/omnitiler/DragOracle"));
        bus.unregisterService(QStringLiteral("com.omnitiler.DragOracle"));
    }
    if (m_groupDbusAvailable) {
        m_groupBus.unregisterObject(QStringLiteral("/com/omnitiler/ActiveBorder"));
        m_groupBus.unregisterService(QStringLiteral("com.omnitiler.ActiveBorder"));
    }
    // A held script waiter is owned by the facts source on the well-known
    // object: destroying the object completes any pending waiter as
    // explicitly unavailable, so the script hides immediately instead of
    // holding a stale snapshot through its lease.
    delete m_groupDbusObject;
    m_groupDbusObject = nullptr;
    if (!m_groupBusName.isEmpty()) {
        QDBusConnection::disconnectFromBus(m_groupBusName);
    }
}

void ActiveWindowBorderEffect::reconfigure(ReconfigureFlags)
{
    ensureEndpointsRegistered();
    ActiveBorderConfig::self()->read();
    // A style configuration change is a published fact change: push the
    // live triple the effect renders with so a held waiter completes with
    // the same numbers. The source bumps only on material change.
    noteFactsStyle();
    updateOutline();
    updateDragPreviewFill();
    updateGroupUnderlayFill();
    updateBorder();
    updateGroupAnchorAndGeometry();
    updateGroupVisibility();
    updateDragPreview();
}

void ActiveWindowBorderEffect::updateOutline()
{
    const QColor fallback = ActiveBorderConfig::borderColor();
    const bool useThemeColor = ActiveBorderConfig::useThemeColor();
    QColor themeColor;
    if (useThemeColor) {
        const KSharedConfigPtr colorConfig = KSharedConfig::openConfig(QStringLiteral("kdeglobals"));
        themeColor = KColorScheme::isColorSetSupported(colorConfig, KColorScheme::Selection)
            ? KColorScheme(QPalette::Active, KColorScheme::Selection, colorConfig).background().color()
            : QColor();
    }
    const BorderOutline outline = BorderOutline(
        ActiveBorderConfig::borderWidth(),
        activeBorderColor(themeColor, fallback, useThemeColor),
        BorderRadius(ActiveBorderConfig::borderRadius()));
    m_borderItem.setOutline(outline);
}

void ActiveWindowBorderEffect::updateGroupUnderlayFill()
{
    QImage underlayImage(1, 1, QImage::Format_ARGB32);
    underlayImage.fill(ActiveBorderConfig::groupUnderlayColor());
    m_groupItem.setImage(underlayImage);
}

void ActiveWindowBorderEffect::updateGroupAnchorAndGeometry()
{
    if (!m_isOpenGL) {
        return;
    }
    GroupHighlightRect rect{};
    const bool hasGroup = group_highlight_rect(&m_groupState, &rect) == 1 && !m_groupMemberIds.isEmpty();
    EffectWindow *anchor = nullptr;
    bool sawMember = false;
    if (hasGroup) {
        // The order runs bottom-first: the first *renderable* member match
        // is the lowest-stacked painted group window. Bare-UUID string
        // compare only; the member list came from the already-Rust-accepted
        // payload. WindowItem effective visibility covers minimized/hidden
        // members while preserving slide-painted off-desktop windows.
        // sawMember distinguishes no-member-match from all-items-hidden.
        for (EffectWindow *candidate : effects->stackingOrder()) {
            if (candidate == nullptr || candidate->isDeleted()) {
                continue;
            }
            const QString bare = candidate->internalId().toString(QUuid::WithoutBraces);
            if (m_groupMemberIds.contains(bare)) {
                sawMember = true;
            }
            WindowItem *candidateItem = candidate->windowItem();
            if (candidateItem == nullptr || !candidateItem->isVisible()) {
                continue;
            }
            if (m_groupMemberIds.contains(bare)) {
                anchor = candidate;
                break;
            }
        }
    }
    m_groupAnchorDiag = !hasGroup ? "no-group" : (anchor != nullptr ? "selected" : (sawMember ? "all-items-hidden" : "no-member-match"));
    EffectWindow *oldAnchor = m_groupAnchor;
    if (oldAnchor != anchor && oldAnchor != nullptr) {
        // The current anchor's own frame move remaps below; the old anchor
        // stops remapping on change/clear. QObject destruction auto-drops
        // its connections, so a nulled QPointer needs no manual disconnect.
        disconnect(oldAnchor, &EffectWindow::windowFrameGeometryChanged, this,
            &ActiveWindowBorderEffect::updateGroupAnchorAndGeometry);
        disconnect(oldAnchor, &EffectWindow::windowFrameGeometryChanged, this,
            &ActiveWindowBorderEffect::updateGroupVisibility);
    }
    m_groupAnchor = anchor;
    if (oldAnchor != anchor && anchor != nullptr) {
        // Remap the stored scene union into the new anchor item coordinates
        // on frame moves without a fresh payload; the parented subtree still
        // slides with the workspace. Visibility stays gated by the existing
        // Meta/focus/endpoint flow. Connected even before the item check so
        // a later-mapped windowItem still remaps on its next frame change.
        connect(anchor, &EffectWindow::windowFrameGeometryChanged, this,
            &ActiveWindowBorderEffect::updateGroupAnchorAndGeometry);
        connect(anchor, &EffectWindow::windowFrameGeometryChanged, this,
            &ActiveWindowBorderEffect::updateGroupVisibility);
    }
    WindowItem *anchorItem = anchor ? anchor->windowItem() : nullptr;
    if (!hasGroup || anchor == nullptr || anchorItem == nullptr) {
        m_groupItem.setParentItem(nullptr);
        return;
    }
    // Outer = union + border gap + border width + resolved extension beyond
    // the border outer edge. Sentinel -1 resolves to the current border
    // width; explicit values (including 0) render as-is.
    const QRectF unionRect(static_cast<qreal>(rect.x), static_cast<qreal>(rect.y),
        static_cast<qreal>(rect.w), static_cast<qreal>(rect.h));
    const double extension = groupUnderlayEffectiveExtension(
        ActiveBorderConfig::groupUnderlayExtension(), ActiveBorderConfig::borderWidth());
    const QRectF outer = groupUnderlayOuterRect(unionRect, ActiveBorderConfig::borderGap(),
        ActiveBorderConfig::borderWidth(), extension);
    const RectF mapped = anchorItem->mapFromScene(outer);
    m_groupItem.setParentItem(anchorItem);
    m_groupItem.setZ(-2);
    m_groupItem.setPosition(QPointF(mapped.x(), mapped.y()));
    m_groupItem.setSize(QSizeF(mapped.width(), mapped.height()));
}

void ActiveWindowBorderEffect::setTrackedWindow(EffectWindow *window)
{
    if (m_trackedWindow == window) {
        return;
    }
    if (m_trackedWindow) {
        // Disconnect only the tracked-window signals. Global maximize
        // subscriptions stay connected across tracked switches.
        disconnect(m_trackedWindow, &EffectWindow::windowFrameGeometryChanged, this,
            &ActiveWindowBorderEffect::updateBorder);
        disconnect(m_trackedWindow, &EffectWindow::minimizedChanged, this, &ActiveWindowBorderEffect::updateBorder);
        disconnect(m_trackedWindow, &EffectWindow::windowFullScreenChanged, this,
            &ActiveWindowBorderEffect::updateBorder);
        disconnect(m_trackedWindow, &EffectWindow::minimizedChanged, this,
            &ActiveWindowBorderEffect::updateGroupVisibility);
        disconnect(m_trackedWindow, &EffectWindow::windowFullScreenChanged, this,
            &ActiveWindowBorderEffect::updateGroupVisibility);
    }
    m_trackedWindow = window;
    if (m_trackedWindow) {
        m_borderItem.setParentItem(m_trackedWindow->windowItem());
        connect(m_trackedWindow, &EffectWindow::windowFrameGeometryChanged, this, &ActiveWindowBorderEffect::updateBorder);
        connect(m_trackedWindow, &EffectWindow::minimizedChanged, this, &ActiveWindowBorderEffect::updateBorder);
        connect(m_trackedWindow, &EffectWindow::windowFullScreenChanged, this, &ActiveWindowBorderEffect::updateBorder);
        // Own active tracked signals update group visibility immediately so
        // fullscreen/minimized transitions hide while Meta is held without
        // waiting for pointer movement. Allowed effect conventions only: no
        // polling, timers, interception, or extra rendering.
        connect(m_trackedWindow, &EffectWindow::minimizedChanged, this, &ActiveWindowBorderEffect::updateGroupVisibility);
        connect(m_trackedWindow, &EffectWindow::windowFullScreenChanged, this, &ActiveWindowBorderEffect::updateGroupVisibility);
    } else {
        m_borderItem.setParentItem(effects->scene()->overlayItem());
    }
}

void ActiveWindowBorderEffect::subscribeMaximize(EffectWindow *window)
{
    if (window == nullptr || m_maximizeSubscribed.contains(window)) {
        return;
    }
    m_maximizeSubscribed.insert(window);
    connect(window, &EffectWindow::windowMaximizedStateAboutToChange, this,
        [this](EffectWindow *changed, bool horizontal, bool vertical) {
            // KWin supplies the target state before geometry changes. Hide
            // before entering maximize, but defer restoration until the
            // changed signal so the border uses the restored frame.
            if (activeBorderIsMaximized(horizontal, vertical)) {
                updateMaximizedState(changed, true);
            }
        });
    connect(window, &EffectWindow::windowMaximizedStateChanged, this,
        [this](EffectWindow *changed, bool horizontal, bool vertical) {
            updateMaximizedState(changed, activeBorderIsMaximized(horizontal, vertical));
        });
    // Direct-read seed from the native committed maximizeMode(): until a
    // Wayland client acknowledges its maximize configure it stays rendered
    // at normal geometry. Acknowledgement updates committed mode through the
    // changed signal; about-to-change can still hide earlier on a request.
    // A null inner window seeds normal.
    // Native transition signals stay authoritative after this seed.
    bool seededMaximized = false;
    try {
        if (Window *inner = window->window()) {
            seededMaximized = activeBorderSeedMaximized(static_cast<int>(inner->maximizeMode()));
        }
    } catch (...) {
        seededMaximized = false;
    }
    if (seededMaximized) {
        m_maximizedWindows.insert(window);
    } else {
        m_maximizedWindows.remove(window);
    }
    bool seedFullScreen = false;
    try {
        seedFullScreen = window->isFullScreen();
    } catch (...) {
        seedFullScreen = false;
    }
    // Bounded per-window observation seed log: two scalars only, no native
    // identifiers, geometry, or payload.
    try {
        logActiveBorderDiag(QStringLiteral("omnitiler:active-border:observe-seed maximized=%1 fullscreen=%2")
                .arg(seededMaximized ? 1 : 0)
                .arg(seedFullScreen ? 1 : 0));
    } catch (...) {
    }
}

void ActiveWindowBorderEffect::unsubscribeMaximize(EffectWindow *window)
{
    if (window == nullptr || !m_maximizeSubscribed.remove(window)) {
        return;
    }
    disconnect(window, &EffectWindow::windowMaximizedStateAboutToChange, this, nullptr);
    disconnect(window, &EffectWindow::windowMaximizedStateChanged, this, nullptr);
}

void ActiveWindowBorderEffect::subscribeGroupVisibility(EffectWindow *window)
{
    if (window == nullptr || m_groupVisibilitySubscribed.contains(window)) {
        return;
    }
    m_groupVisibilitySubscribed.insert(window);
    // Existing public visibility signals only. The handler ignores
    // non-members via the Rust-accepted id list, so every window can stay
    // subscribed across member-list changes (a restored lower member is no
    // longer the anchor but still re-anchors here).
    connect(window, &EffectWindow::minimizedChanged, this, &ActiveWindowBorderEffect::onGroupMemberVisibilityChanged);
    connect(window, &EffectWindow::windowHiddenChanged, this, &ActiveWindowBorderEffect::onGroupMemberVisibilityChanged);
}

void ActiveWindowBorderEffect::unsubscribeGroupVisibility(EffectWindow *window)
{
    if (window == nullptr || !m_groupVisibilitySubscribed.remove(window)) {
        return;
    }
    // Precise method-pointer disconnects: minimizedChanged is shared with
    // the tracked-window signals, which must survive.
    disconnect(window, &EffectWindow::minimizedChanged, this, &ActiveWindowBorderEffect::onGroupMemberVisibilityChanged);
    disconnect(window, &EffectWindow::windowHiddenChanged, this, &ActiveWindowBorderEffect::onGroupMemberVisibilityChanged);
}

void ActiveWindowBorderEffect::onGroupMemberVisibilityChanged(EffectWindow *window)
{
    // Member minimize/restore or hide/show re-anchors to the lowest-stacked
    // renderable member, preserving the underlay without timers or polling.
    // Bare-UUID compare only against the Rust-accepted list; never logged.
    if (!m_isOpenGL || window == nullptr || m_groupMemberIds.isEmpty()) {
        return;
    }
    const QString bare = window->internalId().toString(QUuid::WithoutBraces);
    if (!m_groupMemberIds.contains(bare)) {
        return;
    }
    updateGroupAnchorAndGeometry();
    updateGroupVisibility();
    emitGroupTransitionDiag();
}

void ActiveWindowBorderEffect::attachOracleWindow(EffectWindow *window)
{
    if (window == nullptr || m_oracleAttached.contains(window)) {
        return;
    }
    m_oracleAttached.insert(window);
    connect(window, &EffectWindow::windowStartUserMovedResized, this, [this](EffectWindow *moved) {
        onOracleDragStart(moved);
    });
    connect(window, &EffectWindow::windowFinishUserMovedResized, this, [this](EffectWindow *moved) {
        onOracleDragFinish(moved);
    });
}

void ActiveWindowBorderEffect::forgetOracleWindow(EffectWindow *window)
{
    if (window == nullptr) {
        return;
    }
    m_oracleStartRects.remove(window);
    m_oracleStartPress.remove(window);
    if (m_oraclePress.window == window) {
        m_oraclePress = OraclePressCandidate{};
    }
    // Removal clears the exact tracked window arm; any other window's arm
    // (or none) is untouched. Callers refresh visibility after this.
    if (m_groupMoveWindow == window) {
        m_groupMoveWindow.clear();
    }
    m_oracleAttached.remove(window);
}

void ActiveWindowBorderEffect::emitOraclePressDiag(const char *outcome, bool configured)
{
    // Redacted transition diagnostic only: outcome plus the closed-vocabulary
    // binding source. Never carries coordinates, identities, or geometry.
    try {
        logActiveBorderDiag(QStringLiteral("omnitiler:drag-oracle:press outcome=%1 binding=%2")
                .arg(QString::fromUtf8(outcome))
                .arg(QString::fromUtf8(oraclePressBindingName(configured))));
    } catch (...) {
    }
}

void ActiveWindowBorderEffect::noteOraclePointerPress(PointerButtonEvent *event)
{
    // Passive observer only: the event is never consumed, grabbed, filtered,
    // or modified. Only press edges can start a candidate; every other event
    // leaves the current candidate untouched.
    if (event == nullptr || event->state != PointerButtonState::Pressed) {
        return;
    }
    // Single overwrite-on-press candidate: each press replaces the previous
    // one, so at most one press is ever pending.
    m_oraclePress = OraclePressCandidate{};
    if (!std::isfinite(event->position.x()) || !std::isfinite(event->position.y())) {
        return;
    }
    // Identify the actual configured MouseUnrestrictedResize binding. While
    // the public options are unavailable, fall back to the verified source
    // default (Alt+RightButton) and log that default once per effect
    // instance. A missing resize slot fails closed with no candidate.
    Qt::MouseButton resizeButton = oracleDefaultResizeButton();
    Qt::KeyboardModifier resizeModifier = oracleDefaultResizeModifier();
    bool configured = false;
    if (options != nullptr) {
        resizeButton = oracleResizeButton(static_cast<int>(options->commandAll1()), static_cast<int>(options->commandAll2()),
            static_cast<int>(options->commandAll3()), static_cast<int>(Options::MouseUnrestrictedResize));
        resizeModifier = options->commandAllModifier();
        configured = true;
    } else if (!m_oraclePressDefaultLogged) {
        m_oraclePressDefaultLogged = true;
        emitOraclePressDiag("default-binding", false);
    }
    if (resizeButton == Qt::NoButton || event->button != resizeButton || !event->modifiers.testFlag(resizeModifier)) {
        return;
    }
    InputRedirection *redir = input();
    if (redir == nullptr) {
        return;
    }
    Window *top = redir->findToplevel(event->position);
    if (top == nullptr || top->isDeleted()) {
        return;
    }
    // Associate the toplevel with its effect window through the public
    // internal-id lookup, then verify the exact same live window object.
    EffectWindow *found = effects->findWindow(top->internalId());
    if (found == nullptr || found->isDeleted() || found->window() != top) {
        return;
    }
    OraclePressCandidate next;
    next.window = found;
    next.identity = found->internalId().toString(QUuid::WithoutBraces).toUtf8();
    next.x = event->position.x();
    next.y = event->position.y();
    next.configured = configured;
    next.hasPress = true;
    next.at = std::chrono::steady_clock::now();
    m_oraclePress = next;
    emitOraclePressDiag("captured", configured);
}

void ActiveWindowBorderEffect::updateGroupMoveArm(EffectWindow *window)
{
    // Capture is move-vs-resize only: any interactive-move Started records
    // the exact window (latest gesture wins), even before the group's async
    // payload arrives. Display stays gated on the fresh match below, so an
    // unfocused or not-yet-accepted drag shows nothing. A same-window
    // resize Started clears a stale own arm instead of retaining it.
    bool isMove = false;
    try {
        if (Window *inner = window->window()) {
            isMove = inner->isInteractiveMove();
        }
    } catch (...) {
        isMove = false;
    }
    if (!isMove) {
        if (m_groupMoveWindow == window) {
            m_groupMoveWindow.clear();
            updateGroupVisibility();
            emitGroupTransitionDiag();
        }
        return;
    }
    m_groupMoveWindow = window;
    updateGroupVisibility();
    emitGroupTransitionDiag();
}

bool ActiveWindowBorderEffect::groupMoveMatchesNow() const
{
    // Start-time matches do not persist: the captured window revalidates
    // against the live active window and the accepted Rust subject here, so
    // a focus change plus an unrelated new group can never ride an old drag.
    EffectWindow *dragged = m_groupMoveWindow;
    if (dragged == nullptr || dragged->isDeleted()) {
        return false;
    }
    EffectWindow *active = effects->activeWindow();
    if (active == nullptr) {
        return false;
    }
    const QByteArray draggedId = dragged->internalId().toString(QUuid::WithoutBraces).toUtf8();
    const QByteArray activeId = active->internalId().toString(QUuid::WithoutBraces).toUtf8();
    const uint8_t *draggedPtr = draggedId.isEmpty() ? nullptr : reinterpret_cast<const uint8_t *>(draggedId.constData());
    const uint8_t *activePtr = activeId.isEmpty() ? nullptr : reinterpret_cast<const uint8_t *>(activeId.constData());
    return group_highlight_move_arm_matches(&m_groupState, draggedPtr, static_cast<size_t>(draggedId.size()), activePtr,
               static_cast<size_t>(activeId.size()))
        != 0;
}

void ActiveWindowBorderEffect::onOracleDragStart(EffectWindow *window)
{
    if (window == nullptr || window->isDeleted()) {
        return;
    }
    updateGroupMoveArm(window);
    m_oracleStartRects.insert(window, oracleMoveResizeRect(window));
    // A repeated start supersedes any prior unconsumed slot for this window:
    // drop it before evaluating the candidate so a stale slot can never leak
    // into a later finish. A stale candidate likewise drops on any start via
    // the exact freshness match below instead of lingering for an unrelated
    // later window start; only a fresh candidate for another window stays
    // pending (bounded by age and overwrite-on-press).
    m_oracleStartPress.remove(window);
    // Single-use press evidence: a candidate for the exact same effect window
    // and identity within the bounded monotonic age moves into this window's
    // start slot. Stale or orphaned candidates drop; a candidate for another
    // window stays pending (bounded by age and overwrite-on-press). Whether
    // this drag is a move or a resize is not decided here: the matched press
    // is recorded and the script adapter gates on its own start.resize.
    if (m_oraclePress.hasPress) {
        const auto now = std::chrono::steady_clock::now();
        const int64_t ageMs =
            std::chrono::duration_cast<std::chrono::milliseconds>(now - m_oraclePress.at).count();
        const QByteArray identity = window->internalId().toString(QUuid::WithoutBraces).toUtf8();
        if (m_oraclePress.window.isNull() || !oraclePressAgeOk(ageMs)) {
            const bool wasConfigured = m_oraclePress.configured;
            m_oraclePress = OraclePressCandidate{};
            emitOraclePressDiag("stale", wasConfigured);
        } else if (m_oraclePress.window == window && m_oraclePress.identity == identity) {
            DragOraclePress pod{};
            pod.hasPress = 1;
            pod.binding = m_oraclePress.configured ? 1 : 0;
            pod.x = m_oraclePress.x;
            pod.y = m_oraclePress.y;
            m_oracleStartPress.insert(window, pod);
            const bool wasConfigured = m_oraclePress.configured;
            m_oraclePress = OraclePressCandidate{};
            emitOraclePressDiag("attached", wasConfigured);
        }
    }
}

void ActiveWindowBorderEffect::onOracleDragFinish(EffectWindow *window)
{
    // Finish/cancel clears the exact tracked window arm first, before any
    // oracle-state read: move cleanup never depends on verdict success or
    // readable geometry. The held chord stays independent.
    if (window != nullptr && m_groupMoveWindow == window) {
        m_groupMoveWindow.clear();
        updateGroupVisibility();
        emitGroupTransitionDiag();
    }
    if (window == nullptr || window->isDeleted()) {
        return;
    }
    const QRect finalRect = oracleMoveResizeRect(window);
    const QRect startRect = m_oracleStartRects.take(window);
    // Single-use: the start slot is consumed with this verdict, so the press
    // evidence correlates atomically with the final verdict in one reply.
    const DragOraclePress press = m_oracleStartPress.take(window);
    const QByteArray identity = window->internalId().toString(QUuid::WithoutBraces).toUtf8();
    drag_oracle_record(toOraclePod(startRect), toOraclePod(finalRect), reinterpret_cast<const uint8_t *>(identity.constData()),
        static_cast<size_t>(identity.size()), press);
}

void ActiveWindowBorderEffect::updateMaximizedState(EffectWindow *window, bool maximized)
{
    if (window == nullptr) {
        return;
    }
    if (maximized) {
        m_maximizedWindows.insert(window);
    } else {
        m_maximizedWindows.remove(window);
    }
    if (m_trackedWindow == window) {
        updateBorder();
        // A displayed chord/move-held group must hide immediately on
        // maximize and may only return after the restore transition; the
        // focus-eligibility gate above keeps a maximized-before-chord window
        // from ever showing it.
        updateGroupVisibility();
    }
}

void ActiveWindowBorderEffect::logActiveBorderDiag(const QString &message)
{
    // Diagnostic path only: swallow every failure and never branch caller
    // behavior on logging. Bounded fixed-token message built by callers.
    try {
        qCInfo(lcActiveBorder).noquote() << message;
    } catch (...) {
    }
}

void ActiveWindowBorderEffect::emitActiveBorderEndpoint()
{
    try {
        logActiveBorderDiag(QStringLiteral("omnitiler:active-border:endpoint available=%1")
                .arg(m_groupDbusAvailable ? 1 : 0));
    } catch (...) {
    }
}

void ActiveWindowBorderEffect::emitOracleEndpoint()
{
    try {
        logActiveBorderDiag(QStringLiteral("omnitiler:drag-oracle:endpoint available=%1")
                .arg(m_oracleDbusAvailable ? 1 : 0));
    } catch (...) {
    }
}

bool ActiveWindowBorderEffect::ensureDbusEndpoint(
    const QString &service, const QString &path, QObject *object, bool *serviceOkOut, bool *objectOkOut)
{
    QDBusConnection bus = QDBusConnection::sessionBus();
    return ensureDbusEndpointOn(bus, service, path, object, serviceOkOut, objectOkOut);
}

bool ActiveWindowBorderEffect::ensureDbusEndpointOn(
    QDBusConnection &bus, const QString &service, const QString &path, QObject *object, bool *serviceOkOut, bool *objectOkOut)
{
    bool serviceOk = false;
    bool objectOk = false;
    if (bus.isConnected() && object != nullptr) {
        serviceOk = bus.registerService(service);
        objectOk = bus.registerObject(path, object, QDBusConnection::ExportScriptableContents);
        if (serviceOk && objectOk) {
            if (serviceOkOut != nullptr) {
                *serviceOkOut = true;
            }
            if (objectOkOut != nullptr) {
                *objectOkOut = true;
            }
            return true;
        }
        // Partial success rolls back only what this attempt acquired, so an
        // endpoint owned elsewhere is never unregistered here.
        if (objectOk) {
            bus.unregisterObject(path);
        }
        if (serviceOk) {
            bus.unregisterService(service);
        }
    }
    if (serviceOkOut != nullptr) {
        *serviceOkOut = serviceOk;
    }
    if (objectOkOut != nullptr) {
        *objectOkOut = objectOk;
    }
    return false;
}

bool ActiveWindowBorderEffect::ensureGroupEndpointRegistered()
{
    // Establish the effect-owned dedicated connection once: a distinct
    // connection to the same session bus (unique internal name, never the
    // shared sessionBus handle), so held WaitPresentationFacts replies
    // route cross-connection instead of hitting the local-loop limit.
    if (!m_groupBus.isConnected()) {
        // Stale named handle: drop it before a fresh connect so the name
        // is never double-registered.
        if (!m_groupBusName.isEmpty()) {
            QDBusConnection::disconnectFromBus(m_groupBusName);
            m_groupBus = QDBusConnection::connectToBus(QDBusConnection::SessionBus, m_groupBusName);
        }
        if (!m_groupBus.isConnected()) {
            return false;
        }
        // A (re)connected bus invalidates the previous binding: the facts
        // source holds exactly the registering connection, so rebind the
        // object to the current one. The instance identity resets, which
        // the script treats as unknown (hide) until fresh facts arrive.
        delete m_groupDbusObject;
        m_groupDbusObject = nullptr;
        m_groupDbusAvailable = false;
    } else if (m_groupDbusAvailable && m_groupDbusObject != nullptr) {
        // Healthy and connected: never re-register while well.
        return true;
    }
    if (m_groupDbusObject == nullptr) {
        m_groupDbusObject = new GroupHighlightObject(this, m_groupBus, this);
        // Seed the fresh source before registration/first call: the actual
        // rendered style triple and the current observed chord. Config is
        // instantiated before construction ensures endpoints; an unseeded
        // source serves available:false (no guessed defaults).
        noteFactsStyle();
        noteFactsChord();
    }
    bool serviceOk = false;
    bool objectOk = false;
    if (ensureDbusEndpointOn(m_groupBus, QStringLiteral("com.omnitiler.ActiveBorder"),
            QStringLiteral("/com/omnitiler/ActiveBorder"), m_groupDbusObject, &serviceOk, &objectOk)) {
        m_groupDbusAvailable = true;
        // Transition diagnostic only: initial success or recovery once.
        // Never affects gate, visibility, or repaint decisions.
        emitActiveBorderEndpoint();
        return true;
    }
    if (!m_groupEndpointFailedLogged) {
        m_groupEndpointFailedLogged = true;
        logActiveBorderDiag(QStringLiteral("omnitiler:active-border:endpoint stage=failed service=%1 object=%2")
                .arg(serviceOk ? 1 : 0)
                .arg(objectOk ? 1 : 0));
        emitActiveBorderEndpoint();
    }
    return false;
}

void ActiveWindowBorderEffect::ensureOraclePressSpy()
{
    // Late install when input redirection was null at construction. Passive
    // observer only: never grabs, consumes, or modifies events.
    if (m_oraclePressSpy != nullptr) {
        return;
    }
    if (input() == nullptr) {
        return;
    }
    m_oraclePressSpy = new OraclePressSpy(this);
    input()->installInputEventSpy(m_oraclePressSpy);
    if (m_pressSpyFailedLogged) {
        logActiveBorderDiag(QStringLiteral("omnitiler:drag-oracle:press-spy available=1"));
    }
}

void ActiveWindowBorderEffect::ensureEndpointsRegistered()
{
    // Idempotent: never re-registers while healthy, never polls. Called from
    // construction plus the existing activation and reconfigure events.
    // The ActiveBorder endpoint registers on the effect-owned dedicated
    // connection (same bus, same service/path/interface/shape); the
    // DragOracle endpoint stays on the shared sessionBus. A dropped
    // dedicated connection recovers even while the endpoint still reads
    // available: the bus check below re-enters the reconnect path.
    if (!m_groupDbusAvailable || !m_groupBus.isConnected()) {
        ensureGroupEndpointRegistered();
    }
    // Drag oracle endpoint stays independent of the ActiveBorder outcome, so
    // one failure never hides the other service.
    if (!m_oracleDbusAvailable) {
        bool serviceOk = false;
        bool objectOk = false;
        if (ensureDbusEndpoint(QStringLiteral("com.omnitiler.DragOracle"),
                QStringLiteral("/com/omnitiler/DragOracle"), m_oracleDbusObject, &serviceOk, &objectOk)) {
            m_oracleDbusAvailable = true;
            emitOracleEndpoint();
        } else if (!m_oracleEndpointFailedLogged) {
            m_oracleEndpointFailedLogged = true;
            logActiveBorderDiag(QStringLiteral("omnitiler:drag-oracle:endpoint stage=failed service=%1 object=%2")
                    .arg(serviceOk ? 1 : 0)
                    .arg(objectOk ? 1 : 0));
            emitOracleEndpoint();
        }
    }
    if (m_oraclePressSpy == nullptr && input() == nullptr) {
        if (!m_pressSpyFailedLogged) {
            m_pressSpyFailedLogged = true;
            logActiveBorderDiag(QStringLiteral("omnitiler:drag-oracle:press-spy available=0"));
        }
        return;
    }
    ensureOraclePressSpy();
}

void ActiveWindowBorderEffect::emitActiveBorderVisible(bool visible, const char *reason, bool appletPopup)
{
    // Edge only: first evaluation plus visibility flips. Fixed scalars, no ledger.
    try {
        if (m_borderDiagEmitted && visible == m_borderDiagVisible) {
            return;
        }
        m_borderDiagEmitted = true;
        m_borderDiagVisible = visible;
        logActiveBorderDiag(QStringLiteral("omnitiler:active-border:visible vis=%1 reason=%2 appletPopup=%3")
                .arg(visible ? 1 : 0)
                .arg(QString::fromUtf8(reason))
                .arg(appletPopup ? 1 : 0));
    } catch (...) {
    }
}

void ActiveWindowBorderEffect::emitGroupSetterDiag(const char *outcome)
{
    // Every SetGroupHighlight receipt: Rust outcome plus bounded scalars.
    try {
        const int members = m_groupMemberIds.size() > 9999 ? 9999 : static_cast<int>(m_groupMemberIds.size());
        logActiveBorderDiag(QStringLiteral("omnitiler:group-highlight:setter outcome=%1 members=%2 anchor=%3 first=%4 chord=%5 move=%6 foc=%7 ep=%8 vis=%9")
                .arg(QString::fromUtf8(outcome))
                .arg(members)
                .arg(QString::fromUtf8(m_groupAnchorDiag))
                .arg(m_firstMouseSeen ? 1 : 0)
                .arg(m_chordHeld ? 1 : 0)
                .arg(groupMoveMatchesNow() ? 1 : 0)
                .arg(isGroupFocusEligible() ? 1 : 0)
                .arg(m_groupDbusAvailable ? 1 : 0)
                .arg(m_groupVisible ? 1 : 0));
    } catch (...) {
    }
}

void ActiveWindowBorderEffect::syncGroupTransitionDiag()
{
    m_groupTransDiagAnchor = m_groupAnchorDiag;
    m_groupTransDiagVisible = m_groupVisible;
    m_groupTransDiagEmitted = true;
}

void ActiveWindowBorderEffect::emitGroupTransitionDiag()
{
    // Combined anchor/visibility line only on material change.
    try {
        const QString curAnchor = QString::fromUtf8(m_groupAnchorDiag);
        const QString lastAnchor = QString::fromUtf8(m_groupTransDiagAnchor);
        if (m_groupTransDiagEmitted && lastAnchor == curAnchor && m_groupTransDiagVisible == m_groupVisible) {
            return;
        }
        m_groupTransDiagAnchor = m_groupAnchorDiag;
        m_groupTransDiagVisible = m_groupVisible;
        m_groupTransDiagEmitted = true;
        const int members = m_groupMemberIds.size() > 9999 ? 9999 : static_cast<int>(m_groupMemberIds.size());
        logActiveBorderDiag(QStringLiteral("omnitiler:group-highlight:transition anchor=%1 members=%2 first=%3 chord=%4 move=%5 foc=%6 ep=%7 vis=%8")
                .arg(curAnchor)
                .arg(members)
                .arg(m_firstMouseSeen ? 1 : 0)
                .arg(m_chordHeld ? 1 : 0)
                .arg(groupMoveMatchesNow() ? 1 : 0)
                .arg(isGroupFocusEligible() ? 1 : 0)
                .arg(m_groupDbusAvailable ? 1 : 0)
                .arg(m_groupVisible ? 1 : 0));
    } catch (...) {
    }
}

void ActiveWindowBorderEffect::updateBorder()
{
    if (!m_isOpenGL) {
        return;
    }

    EffectWindow *window = effects->activeWindow();
    const bool nativeMaximized = window ? m_maximizedWindows.contains(window) : false;
    const bool fullScreen = window ? window->isFullScreen() : false;
    const bool appletPopup = window ? window->isAppletPopup() : false;
    const ActiveBorderState state = activeBorderState(
        window != nullptr,
        window ? static_cast<QRectF>(window->frameGeometry()) : QRectF(),
        window ? window->isDeleted() : false,
        window ? window->isMinimized() : false,
        fullScreen,
        nativeMaximized,
        appletPopup);
    // Native maximize/fullscreen signals stay authoritative: any seeded or
    // transitioned maximize axis plus live fullscreen suppresses the border.
    const bool visible = state.visible;
    // Transition diagnostic only: first evaluation plus visibility flips.
    // Never affects the border or repaint decision below.
    emitActiveBorderVisible(visible,
        activeBorderDiagReason(window != nullptr, window ? window->isDeleted() : false, window ? window->isMinimized() : false,
            fullScreen, nativeMaximized, appletPopup, m_groupDbusAvailable),
        appletPopup);
    const qreal gap = ActiveBorderConfig::borderGap();
    const QRectF innerRect = activeBorderInnerRect(state.innerRect, gap);
    m_borderItem.setInnerRect(window ? window->windowItem()->mapFromScene(innerRect) : RectF());
    m_borderItem.setVisible(visible);
    effects->addRepaintFull();
}

void ActiveWindowBorderEffect::applyGroupHighlight(const QString &payload)
{
    // Unavailable endpoint never displays: fail closed.
    if (!m_groupDbusAvailable) {
        clearGroupHighlight();
        emitGroupSetterDiag("endpoint-unavailable");
        syncGroupTransitionDiag();
        return;
    }
    // QObject/D-Bus boundary: QString payload and native active identity to
    // UTF-8 bytes. All parsing, ordering, and focus policy lives in Rust.
    const QByteArray payloadBytes = payload.toUtf8();
    EffectWindow *active = effects->activeWindow();
    QByteArray activeBytes;
    if (active != nullptr) {
        activeBytes = active->internalId().toString(QUuid::WithoutBraces).toUtf8();
    }
    const bool hadDisplayed = m_groupState.has_group != 0;
    const uint8_t *payloadPtr = payloadBytes.isEmpty() ? nullptr : reinterpret_cast<const uint8_t *>(payloadBytes.constData());
    const uint8_t *activePtr = activeBytes.isEmpty() ? nullptr : reinterpret_cast<const uint8_t *>(activeBytes.constData());
    const int32_t code = group_highlight_apply(&m_groupState, payloadPtr, static_cast<size_t>(payloadBytes.size()), activePtr,
        static_cast<size_t>(activeBytes.size()));
    if (code == 2) {
        // Stale/out-of-order versus the newer displayed highlight is ignored
        // without destroying it.
        emitGroupSetterDiag("stale");
        syncGroupTransitionDiag();
        return;
    }
    if (code == 1) {
        // Membership comes from the already-Rust-accepted payload via Qt
        // JSON only: plain id list, no new FFI storage. Never logged.
        m_groupMemberIds.clear();
        const QJsonDocument document = QJsonDocument::fromJson(payloadBytes);
        if (document.isObject()) {
            const QJsonArray members = document.object().value(QStringLiteral("members")).toArray();
            for (const QJsonValue &entry : members) {
                if (entry.isString()) {
                    m_groupMemberIds.append(entry.toString());
                }
            }
        }
        updateGroupAnchorAndGeometry();
        updateGroupVisibility();
        if (m_isOpenGL) {
            effects->addRepaintFull();
        }
        emitGroupSetterDiag("accepted");
        syncGroupTransitionDiag();
        return;
    }
    m_groupMemberIds.clear();
    // Leave m_groupAnchor for updateGroupAnchorAndGeometry so the old anchor
    // disconnects its frame remap; the update nulls it when no member stays.
    updateGroupAnchorAndGeometry();
    updateGroupVisibility();
    if (hadDisplayed && m_isOpenGL) {
        effects->addRepaintFull();
    }
    emitGroupSetterDiag(code == 3 ? "focus-mismatch" : "parse-rejected");
    syncGroupTransitionDiag();
}

void ActiveWindowBorderEffect::clearGroupHighlight()
{
    // Rust preserves the order within the stream; only the display clears.
    const int32_t hadGroup = group_highlight_clear(&m_groupState);
    m_groupMemberIds.clear();
    // Leave m_groupAnchor for updateGroupAnchorAndGeometry so the old anchor
    // disconnects its frame remap; the update nulls it.
    updateGroupAnchorAndGeometry();
    updateGroupVisibility();
    if (hadGroup == 1 && m_isOpenGL) {
        effects->addRepaintFull();
        emitGroupTransitionDiag();
    }
}

void ActiveWindowBorderEffect::setDragTargetPreview(int x, int y, int w, int h)
{
    // Fail closed while the shared ActiveBorder endpoint is unavailable.
    if (!m_groupDbusAvailable) {
        clearDragTargetPreview();
        return;
    }
    if (!dragPreviewRectValid(x, y, w, h)) {
        clearDragTargetPreview();
        return;
    }
    m_dragPreviewRect = QRect(x, y, w, h);
    m_dragPreviewVisible = true;
    updateDragPreview();
    if (m_isOpenGL) {
        effects->addRepaintFull();
    }
}

void ActiveWindowBorderEffect::clearDragTargetPreview()
{
    const bool wasVisible = m_dragPreviewVisible;
    m_dragPreviewVisible = false;
    m_dragPreviewRect = QRect();
    updateDragPreview();
    if (wasVisible && m_isOpenGL) {
        effects->addRepaintFull();
    }
}

void ActiveWindowBorderEffect::updateDragPreviewFill()
{
    QImage previewImage(1, 1, QImage::Format_ARGB32);
    previewImage.fill(ActiveBorderConfig::dragPreviewColor());
    m_dragPreviewItem.setImage(previewImage);
}

void ActiveWindowBorderEffect::updateDragPreview()
{
    if (!m_isOpenGL) {
        return;
    }
    if (m_dragPreviewVisible) {
        m_dragPreviewItem.setPosition(QPointF(static_cast<qreal>(m_dragPreviewRect.x()), static_cast<qreal>(m_dragPreviewRect.y())));
        m_dragPreviewItem.setSize(QSizeF(static_cast<qreal>(m_dragPreviewRect.width()), static_cast<qreal>(m_dragPreviewRect.height())));
    }
    m_dragPreviewItem.setVisible(m_dragPreviewVisible);
}

QString ActiveWindowBorderEffect::groupHighlightStatus() const
{
    GroupHighlightStatus status{};
    if (group_highlight_status(&m_groupState, &status) != 0) {
        status = GroupHighlightStatus{};
    }
    const bool focusEligible = isGroupFocusEligible();
    return QStringLiteral("v=1;rx=%1;ok=%2;parse_rej=%3;focus_mm=%4;stale=%5;clr=%6;has=%7;ord=%8;first=%9;chord=%10;move=%11;foc=%12;ep=%13;gl=%14;vis=%15")
        .arg(QString::number(status.receipts))
        .arg(QString::number(status.accepted))
        .arg(QString::number(status.parse_rejected))
        .arg(QString::number(status.focus_mismatch))
        .arg(QString::number(status.stale_ignored))
        .arg(QString::number(status.clear_requests))
        .arg(status.has_group != 0 ? 1 : 0)
        .arg(status.order_initialized != 0 ? 1 : 0)
        .arg(m_firstMouseSeen ? 1 : 0)
        .arg(m_chordHeld ? 1 : 0)
        .arg(groupMoveMatchesNow() ? 1 : 0)
        .arg(focusEligible ? 1 : 0)
        .arg(m_groupDbusAvailable ? 1 : 0)
        .arg(m_isOpenGL ? 1 : 0)
        .arg(m_groupVisible ? 1 : 0);
}

bool ActiveWindowBorderEffect::isGroupFocusEligible() const
{
    EffectWindow *window = effects->activeWindow();
    if (window == nullptr) {
        return false;
    }
    return group_highlight_focus_eligible(1, window->isDeleted() ? 1 : 0, window->isMinimized() ? 1 : 0,
               window->isFullScreen() ? 1 : 0, window->isHidden() ? 1 : 0,
               m_maximizedWindows.contains(window) ? 1 : 0)
        != 0;
}

void ActiveWindowBorderEffect::onMouseChanged(const QPointF &pos, const QPointF &oldPos, Qt::MouseButtons buttons, Qt::MouseButtons oldButtons,
    Qt::KeyboardModifiers modifiers, Qt::KeyboardModifiers oldModifiers)
{
    Q_UNUSED(pos);
    Q_UNUSED(oldPos);
    Q_UNUSED(buttons);
    Q_UNUSED(oldButtons);
    Q_UNUSED(oldModifiers);
    const bool wasKnown = m_firstMouseSeen;
    m_firstMouseSeen = true;
    // Movement-only chord: both Win and Shift held (extras allowed, either
    // press order). The predicate lives in Rust; only these two level bits
    // fold here. Unknown before the first signal hides via the first-seen
    // gate in updateGroupVisibility.
    const bool wasHeld = m_chordHeld;
    m_chordHeld = groupUnderlayChordHeld(
        modifiers.testFlag(Qt::MetaModifier), modifiers.testFlag(Qt::ShiftModifier));
    // A chord level/known flip is a published fact change: push the observed
    // level into the versioned script facts so a held waiter completes with
    // the fresh snapshot instead of holding stale identity.
    if (!wasKnown || wasHeld != m_chordHeld) {
        noteFactsChord();
    }
    updateGroupVisibility();
    emitGroupTransitionDiag();
}

// Versioned raw presentation facts for the script route (Stage 3 ingress,
// read-only): pushes the observed chord level and the actual style
// configuration into the held facts source on the well-known object. The
// source bumps its revision only on material change; the script fences
// snapshots on (instance, rev).
void ActiveWindowBorderEffect::noteFactsChord()
{
    if (auto *dbusObject = static_cast<GroupHighlightObject *>(m_groupDbusObject)) {
        if (PresentationFactsSource *facts = dbusObject->facts()) {
            facts->noteChord(m_chordHeld, m_firstMouseSeen);
        }
    }
}

void ActiveWindowBorderEffect::noteFactsStyle()
{
    if (auto *dbusObject = static_cast<GroupHighlightObject *>(m_groupDbusObject)) {
        if (PresentationFactsSource *facts = dbusObject->facts()) {
            facts->noteStyle(ActiveBorderConfig::borderGap(), ActiveBorderConfig::borderWidth(),
                ActiveBorderConfig::groupUnderlayExtension());
        }
    }
}

void ActiveWindowBorderEffect::updateGroupVisibility()
{
    if (!m_isOpenGL) {
        return;
    }
    // Fail-closed endpoint gate plus the passive Win+Shift chord gate (or a
    // matching focused-window interactive move, which needs no modifier
    // observation) plus live focus eligibility (fullscreen/minimized/hidden/
    // deleted/maximized hide immediately via the tracked-signal connections
    // above). Member validity is never derived native-side beyond the
    // Rust-accepted list used for the lowest-stacked anchor; the carried
    // union outer rect renders.
    // Policy lives in Rust; C++ supplies POD observer flags and renders.
    // A missing anchor (no live member window) keeps the underlay hidden.
    const bool matchingMove = groupMoveMatchesNow();
    const bool groupShow = group_highlight_is_visible(&m_groupState, m_chordHeld ? 1 : 0, m_firstMouseSeen ? 1 : 0,
                               isGroupFocusEligible() ? 1 : 0, m_groupDbusAvailable ? 1 : 0, matchingMove ? 1 : 0)
        == 1;
    const bool anchorOk = !m_groupAnchor.isNull() && m_groupAnchor->windowItem() != nullptr && !m_groupMemberIds.isEmpty();
    const bool show = groupShow && anchorOk;
    if (show != m_groupVisible) {
        m_groupVisible = show;
        m_groupItem.setVisible(show);
        effects->addRepaintFull();
    } else {
        m_groupItem.setVisible(show);
    }
}

#ifdef OMNITILER_KWIN_PAINTSCREEN_BOOL
bool ActiveWindowBorderEffect::paintScreen(const RenderTarget &renderTarget, const RenderViewport &viewport, int mask, const Region &deviceRegion, LogicalOutput *screen)
{
    return effects->paintScreen(renderTarget, viewport, mask, deviceRegion, screen);
}
#else
void ActiveWindowBorderEffect::paintScreen(const RenderTarget &renderTarget, const RenderViewport &viewport, int mask, const Region &deviceRegion, LogicalOutput *screen)
{
    effects->paintScreen(renderTarget, viewport, mask, deviceRegion, screen);
}
#endif

KWIN_EFFECT_FACTORY(ActiveWindowBorderEffect, "metadata.json")

} // namespace KWin

#include "activewindowborder.moc"
