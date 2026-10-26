// SPDX-License-Identifier: GPL-2.0-or-later
// SPDX-FileCopyrightText: 2026 Michael Alexander
#include "activewindowborder.h"
#include "activeborderconfig.h"
#include "activeborderlogic.h"
#include "gesture_lifecycle.h"
#include "gesturefacts.h"
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
#include <workspace.h>

#include <QByteArray>
#include <QColor>
#include <QDBusConnection>
#include <QDBusContext>
#include <QDBusError>
#include <QDBusMessage>
#include <QImage>
#include <QLoggingCategory>
#include <QPalette>
#include <QTimer>
#include <QUuid>

#include <climits>
#include <cmath>
#include <cstddef>

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
    // Read-only presentation paint (Stage 5, pure renderer): one strict
    // JSON payload with accepted core shapes (see presentation_consumer.h).
    // Malformed payloads clear; same-generation older revisions never
    // resurrect. Never derives geometry, eligibility, or policy.
    Q_SCRIPTABLE void SetPresentation(const QString &payload)
    {
        if (m_effect) {
            m_effect->applyPresentation(payload);
        }
    }
    Q_SCRIPTABLE void ClearPresentation()
    {
        if (m_effect) {
            m_effect->clearPresentation();
        }
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

} // namespace

// Passive Esc delivery fence at InteractiveMoveResize weight (decision B).
// Installed after the builtin MoveResizeFilter exists, so the lower_bound
// install order (src/input.cpp, primary pin v6.7.4) places this fence
// immediately BEFORE the builtin consumer while staying AFTER earlier
// consumers (shortcuts at GlobalShortcut, effects at Effects): an Esc
// reaching this fence reaches the builtin in the same dispatch unless an
// earlier filter consumed it. No input spy arms anything: spies run before
// filters (src/keyboard_input.cpp, v6.7.4) and only observe, so there is no
// sticky prefilter arm; the fence below is the only arm site, and the
// effect verifies the exact native move/resize window before arming (see
// noteGestureEsc). Passive observer only: never consumes (always returns
// false), never calls finish, never patches KWin, no timers. It only
// records the arm, which the finish handler consumes synchronously.
//
// Source pins (primary: KWin v6.7.4; the installed host 6.8.0 headers carry
// the identical contract in input.h/input_event.h): processFilters is a
// synchronous ordered loop stopping at the first true; InputFilterOrder has
// Effects immediately before InteractiveMoveResize; installInputEventFilter
// inserts later same-weight filters before earlier ones; KeyboardKeyEvent
// carries a Qt::Key; the builtin MoveResizeFilter forwards to
// Window::keyPressEvent (Escape ends with finish(true), which emits
// interactiveMoveResizeFinished synchronously, re-emitted directly by
// src/effect/effectwindow.cpp) and consumes whenever a move/resize window
// exists.
class GestureEscFence : public InputEventFilter
{
public:
    explicit GestureEscFence(ActiveWindowBorderEffect *effect)
        : InputEventFilter(InputFilterOrder::InteractiveMoveResize)
        , m_effect(effect)
    {
    }

    bool keyboardKey(KeyboardKeyEvent *event) override
    {
        if (event != nullptr && m_effect != nullptr
            && gestureEscArmsFilter(event->key,
                   event->state == KeyboardKeyState::Pressed || event->state == KeyboardKeyState::Repeated)) {
            m_effect->noteGestureEsc();
        }
        // Never consume: the Esc must reach the builtin MoveResizeFilter in
        // this same dispatch for the arm to be causal.
        return false;
    }

private:
    ActiveWindowBorderEffect *m_effect = nullptr;
};

namespace
{

QRect quantizedGestureRect(const RectF &geometry)
{
    return QRect(static_cast<int>(std::lround(geometry.x())), static_cast<int>(std::lround(geometry.y())),
        static_cast<int>(std::lround(geometry.width())), static_cast<int>(std::lround(geometry.height())));
}

// Native start-geometry mechanics for the authoritative gesture lifecycle
// (read-only): the live move/resize frame at Started, quantized. Policy
// (move-vs-resize, targets, commits) lives in core; this only observes.
QRect gestureMoveResizeRect(EffectWindow *window)
{
    if (window == nullptr) {
        return QRect();
    }
    if (Window *inner = window->window()) {
        return quantizedGestureRect(inner->moveResizeGeometry());
    }
    return QRect();
}

// Live pointer proof for the authoritative gesture lifecycle (read-only):
// the global pointer quantized like the start rect, or absent when the
// input redirection is unavailable or the position is not finite. Never a
// fallback, never a gesture decision: absence rides as null cursor.
struct GesturePointer {
    bool has = false;
    QPoint point;
};

GesturePointer gesturePointerNow()
{
    InputRedirection *redir = input();
    if (redir == nullptr) {
        return GesturePointer{};
    }
    const QPointF pos = redir->globalPointer();
    if (!std::isfinite(pos.x()) || !std::isfinite(pos.y())) {
        return GesturePointer{};
    }
    const long x = std::lround(pos.x());
    const long y = std::lround(pos.y());
    if (x < INT_MIN || x > INT_MAX || y < INT_MIN || y > INT_MAX) {
        return GesturePointer{};
    }
    return GesturePointer{true, QPoint(static_cast<int>(x), static_cast<int>(y))};
}

// Native styling independence (Stage 5): the active ring keeps its own
// border color/theme tokens (default: live system accent). An explicit
// override on either side never touches the underlay or drop-preview
// colors. The alpha gate matches the retired shared predicate: a theme
// color is used only when theming is requested, the theme color is valid,
// and its quantized alpha is positive.
QColor presentationBorderColor(const QColor &themeColor, const QColor &fallbackColor, bool useThemeColor)
{
    // Preserve Qt's quantized alpha and original QColor representation.
    const bool themeValid = themeColor.isValid();
    const int themeAlpha = themeValid ? themeColor.alpha() : 0;
    if (useThemeColor && themeValid && themeAlpha > 0) {
        return themeColor;
    }
    return fallbackColor;
}

// Native ring artwork for one exact width (finding C): the color, theme
// selection, and corner radius stay live host styling; only the stroke
// comes from the published inner/outer pair (never live config width, so
// capture-vs-live config drift cannot move the drawn outer edge).
BorderOutline ringOutlineForWidth(double width)
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
    return BorderOutline(width, presentationBorderColor(themeColor, fallback, useThemeColor),
        BorderRadius(ActiveBorderConfig::borderRadius()));
}

// Single fixed reason token for the computed active-border visibility.
// Order matches updateBorder() evaluation so exactly one token distinguishes
// the first suppressing gate: paint, endpoint, foreground match, window
// presence, deleted, minimized, fullscreen, maximized, then applet popup.
const char *activeBorderDiagReason(bool hasPaint, bool foregroundMatches, bool hasWindow, bool deleted, bool minimized, bool fullScreen, bool nativeMaximized,
    bool appletPopup, bool dbusAvailable)
{
    if (!dbusAvailable) {
        return "endpoint-unavailable";
    }
    if (!hasPaint) {
        return "no-presentation";
    }
    if (!foregroundMatches) {
        return "foreground-mismatch";
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

    clearPresentationPaint(&m_presentation);
    // Fail closed with no false endpoint expectation: the paint stays
    // unavailable/clear unless both the well-known service and object
    // register. A transient failure retries on later activation/reconfigure
    // events, never by timer. The ActiveBorder object binds the
    // effect-owned dedicated connection (cross-connection held replies).
    ensureEndpointsRegistered();
    if (!m_groupDbusAvailable) {
        clearPresentationPaint(&m_presentation);
    }

    // Gesture observation is independent of rendering. Keep this one shared
    // lifecycle hookup set active even when the border cannot render.
    connect(effects, &EffectsHandler::windowDeleted, this, [this](EffectWindow *window) {
        unsubscribeMaximize(window);
        unsubscribeGroupVisibility(window);
        forgetGestureWindow(window);
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
        emitPresentationTransitionDiag();
    });
    connect(effects, &EffectsHandler::windowClosed, this, [this](EffectWindow *window) {
        unsubscribeMaximize(window);
        unsubscribeGroupVisibility(window);
        forgetGestureWindow(window);
        m_maximizedWindows.remove(window);
        if (!m_isOpenGL) {
            return;
        }
        updateBorder();
        updateGroupAnchorAndGeometry();
        updateGroupVisibility();
        emitPresentationTransitionDiag();
    });
    // Global maximize tracking and the gesture lifecycle observe every
    // window, including when the active border cannot render. Each observed
    // window seeds its committed maximizeMode() directly: windows already
    // maximized before effect load emit no transition, so the seed is
    // authoritative until native transition signals update it.
    for (EffectWindow *window : effects->stackingOrder()) {
        subscribeMaximize(window);
        subscribeGroupVisibility(window);
        attachGestureWindow(window);
    }
    connect(effects, &EffectsHandler::windowAdded, this, [this](EffectWindow *window) {
        subscribeMaximize(window);
        subscribeGroupVisibility(window);
        attachGestureWindow(window);
        if (m_isOpenGL) {
            updateGroupAnchorAndGeometry();
            updateGroupVisibility();
            emitPresentationTransitionDiag();
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
        emitPresentationTransitionDiag();
    });
    // Activation recovery runs even when OpenGL rendering is off: the
    // gesture lifecycle stays useful without a visible border. Rendering
    // below stays OpenGL-gated; focus-clear ordering is preserved.
    connect(effects, &EffectsHandler::windowActivated, this, [this](EffectWindow *) {
        ensureEndpointsRegistered();
        if (!m_isOpenGL) {
            return;
        }
        // Focus activation clears the old paint immediately before any
        // asynchronous script refresh, so no stale paint renders under the
        // new focus. The subscriber repaints from the next accepted
        // projection.
        clearPresentation();
        setTrackedWindow(effects->activeWindow());
        updateBorder();
        updateGroupVisibility();
        emitPresentationTransitionDiag();
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
    // Shown only for a held accepted preview with live gesture liveness
    // and a fresh paint; never gated on Meta, focus, or the group underlay.
    // Z=10 keeps it above windows within the overlay.
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
    // The fence uninstalls itself on delete (InputEventFilter contract), so
    // no live dispatch can arm a dying source.
    delete m_gestureFence;
    m_gestureFence = nullptr;
    // Release only endpoints this effect registered, never an endpoint owned
    // elsewhere after a failed or partial registration. The ActiveBorder
    // endpoint tears down on its exact owned connection (unregister there,
    // then disconnect that named connection); the shared sessionBus is
    // untouched by the ActiveBorder teardown.
    // Gesture endpoint teardown on the same dedicated connection: release
    // the well-known name there, then destroy the source so a held waiter
    // completes explicitly unavailable instead of holding a stale snapshot.
    if (m_gestureDbusAvailable) {
        m_groupBus.unregisterObject(QStringLiteral("/com/omnitiler/Gesture"));
        m_groupBus.unregisterService(QStringLiteral("com.omnitiler.Gesture"));
    }
    m_gestureDbusAvailable = false;
    delete m_gestureSource;
    m_gestureSource = nullptr;
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
    m_borderItem.setOutline(ringOutlineForWidth(ActiveBorderConfig::borderWidth()));
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
    // Anchor inputs come from the held accepted paint: the member id list
    // chooses the lowest-stacked renderable member, and the stored core
    // outer rect is consumed verbatim (the gap/width/extension expansion
    // already happened core-side in KWin logical units). Item anchoring
    // translates coordinate spaces (scene to anchor-local) without a second
    // outset, DPI factor, or device scaling.
    const bool hasPaint = m_presentation.has && m_presentation.underlayFound && !m_presentation.underlayMembers.isEmpty()
        && m_groupDbusAvailable;
    EffectWindow *anchor = nullptr;
    bool sawMember = false;
    if (hasPaint) {
        // The order runs bottom-first: the first *renderable* member match
        // is the lowest-stacked painted member window. Bare-UUID string
        // compare only. WindowItem effective visibility covers
        // minimized/hidden members while preserving slide-painted
        // off-desktop windows. sawMember distinguishes no-member-match from
        // all-items-hidden.
        for (EffectWindow *candidate : effects->stackingOrder()) {
            if (candidate == nullptr || candidate->isDeleted()) {
                continue;
            }
            const QString bare = candidate->internalId().toString(QUuid::WithoutBraces);
            if (m_presentation.underlayMembers.contains(bare)) {
                sawMember = true;
            }
            WindowItem *candidateItem = candidate->windowItem();
            if (candidateItem == nullptr || !candidateItem->isVisible()) {
                continue;
            }
            if (m_presentation.underlayMembers.contains(bare)) {
                anchor = candidate;
                break;
            }
        }
    }
    m_presentationAnchorDiag = !m_presentation.has ? "no-presentation"
        : (!m_presentation.underlayFound ? "underlay-hidden"
            : (m_presentation.underlayMembers.isEmpty() ? "no-members"
                : (anchor != nullptr ? "selected" : (sawMember ? "all-items-hidden" : "no-member-match"))));
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
        // Remap the stored scene outer into the new anchor item coordinates
        // on frame moves without a fresh payload; the parented subtree still
        // slides with the workspace. Visibility stays gated by the existing
        // paint/foreground/veto flow. Connected even before the item check so
        // a later-mapped windowItem still remaps on its next frame change.
        connect(anchor, &EffectWindow::windowFrameGeometryChanged, this,
            &ActiveWindowBorderEffect::updateGroupAnchorAndGeometry);
        connect(anchor, &EffectWindow::windowFrameGeometryChanged, this,
            &ActiveWindowBorderEffect::updateGroupVisibility);
    }
    WindowItem *anchorItem = anchor ? anchor->windowItem() : nullptr;
    if (!hasPaint || anchor == nullptr || anchorItem == nullptr) {
        m_groupItem.setParentItem(nullptr);
        return;
    }
    // Consumed verbatim: the core outer rect already includes the border
    // gap, border width, and resolved extension in KWin logical units. Only
    // the space translation (scene to anchor-local) happens here.
    const QRectF outerRect(static_cast<qreal>(m_presentation.underlayOuter.x()),
        static_cast<qreal>(m_presentation.underlayOuter.y()), static_cast<qreal>(m_presentation.underlayOuter.width()),
        static_cast<qreal>(m_presentation.underlayOuter.height()));
    const RectF mapped = anchorItem->mapFromScene(outerRect);
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
            &ActiveWindowBorderEffect::onTrackedFrameChanged);
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
        // Frame moves dirty the border paint (finding F): the stored scene
        // rect may be stale, so the ring hides until a strictly newer
        // accepted paint arrives.
        connect(m_trackedWindow, &EffectWindow::windowFrameGeometryChanged, this, &ActiveWindowBorderEffect::onTrackedFrameChanged);
        connect(m_trackedWindow, &EffectWindow::minimizedChanged, this, &ActiveWindowBorderEffect::updateBorder);
        connect(m_trackedWindow, &EffectWindow::windowFullScreenChanged, this, &ActiveWindowBorderEffect::updateBorder);
        // Own active tracked signals update group visibility immediately so
        // hidden transitions hide while held without waiting for pointer
        // movement. Allowed effect conventions only: no polling, timers,
        // interception, or extra rendering.
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
            if (horizontal || vertical) {
                updateMaximizedState(changed, true);
            }
        });
    connect(window, &EffectWindow::windowMaximizedStateChanged, this,
        [this](EffectWindow *changed, bool horizontal, bool vertical) {
            updateMaximizedState(changed, horizontal || vertical);
        });
    // Direct-read seed from the native committed maximizeMode(): until a
    // Wayland client acknowledges its maximize configure it stays rendered
    // at normal geometry. Acknowledgement updates committed mode through the
    // changed signal; about-to-change can still hide earlier on a request.
    // A null inner window seeds normal.
    // Native transition signals stay authoritative after this seed. The seed
    // feeds the hide-only veto below, never a visibility derivation.
    bool seededMaximized = false;
    try {
        if (Window *inner = window->window()) {
            seededMaximized = inner->maximizeMode() != 0;
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
    // non-members via the held paint id list, so every window can stay
    // subscribed across paint changes (a restored lower member is no
    // longer the anchor but still re-anchors here). Frame changes are
    // observed on the same per-window set: any held member frame change
    // dirties the underlay (finding F), since the published outer depends
    // on every member, not just the anchor.
    connect(window, &EffectWindow::minimizedChanged, this, &ActiveWindowBorderEffect::onGroupMemberVisibilityChanged);
    connect(window, &EffectWindow::windowHiddenChanged, this, &ActiveWindowBorderEffect::onGroupMemberVisibilityChanged);
    connect(window, &EffectWindow::windowFrameGeometryChanged, this, &ActiveWindowBorderEffect::onGroupMemberFrameChanged);
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
    disconnect(window, &EffectWindow::windowFrameGeometryChanged, this, &ActiveWindowBorderEffect::onGroupMemberFrameChanged);
}

void ActiveWindowBorderEffect::onGroupMemberFrameChanged(EffectWindow *window)
{
    // Any held member frame change dirties the underlay paint: the stored
    // scene outer may be stale, so it hides until a strictly newer accepted
    // paint arrives. Remapping continues (hidden) so a fresh paint shows
    // coherent geometry immediately. Bare-UUID compare only against the
    // held paint list; never logged.
    if (!m_isOpenGL || window == nullptr || !m_presentation.has || m_presentation.underlayMembers.isEmpty()) {
        return;
    }
    const QString bare = window->internalId().toString(QUuid::WithoutBraces);
    if (!m_presentation.underlayMembers.contains(bare)) {
        return;
    }
    KWin::markUnderlayGeometryDirty(&m_presentation);
    updateGroupAnchorAndGeometry();
    updateGroupVisibility();
    emitPresentationTransitionDiag();
}

void ActiveWindowBorderEffect::onGroupMemberVisibilityChanged(EffectWindow *window)
{
    // Member minimize/restore or hide/show re-anchors to the lowest-stacked
    // renderable member, preserving the underlay without timers or polling.
    // Bare-UUID compare only against the held paint list; never logged.
    if (!m_isOpenGL || window == nullptr || m_presentation.underlayMembers.isEmpty()) {
        return;
    }
    const QString bare = window->internalId().toString(QUuid::WithoutBraces);
    if (!m_presentation.underlayMembers.contains(bare)) {
        return;
    }
    updateGroupAnchorAndGeometry();
    updateGroupVisibility();
    emitPresentationTransitionDiag();
}

void ActiveWindowBorderEffect::updateGroupMoveArm(EffectWindow *window)
{
    // Capture is move-vs-resize only: any interactive-move Started records
    // the exact window (latest gesture wins). Display stays gated on the
    // live liveness below plus the held accepted paint, so an ended gesture
    // never shows a stale preview. A same-window resize Started clears a
    // stale own arm instead of retaining it.
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
            updateDragPreview();
            emitPresentationTransitionDiag();
        }
        return;
    }
    m_groupMoveWindow = window;
    updateDragPreview();
    emitPresentationTransitionDiag();
}

bool ActiveWindowBorderEffect::groupMoveLiveNow() const
{
    const EffectWindow *dragged = m_groupMoveWindow;
    if (dragged == nullptr || dragged->isDeleted()) {
        return false;
    }
    return true;
}

void ActiveWindowBorderEffect::attachGestureWindow(EffectWindow *window)
{
    if (window == nullptr || m_gestureAttached.contains(window)) {
        return;
    }
    m_gestureAttached.insert(window);
    connect(window, &EffectWindow::windowStartUserMovedResized, this, [this](EffectWindow *moved) {
        onGestureDragStart(moved);
    });
    connect(window, &EffectWindow::windowStepUserMovedResized, this, [this](EffectWindow *moved) {
        onGestureDragStep(moved);
    });
    connect(window, &EffectWindow::windowFinishUserMovedResized, this, [this](EffectWindow *moved) {
        onGestureDragFinish(moved);
    });
}

void ActiveWindowBorderEffect::forgetGestureWindow(EffectWindow *window)
{
    if (window == nullptr) {
        return;
    }
    // Window removed mid-gesture: drop the open gesture by its tracked
    // identity (never read a possibly deleted window) without inventing a
    // terminal. No-op when nothing was open. Removal also clears the exact
    // tracked move window plus the held drop-preview latch, so no later
    // paint may arrive to clear a stale preview for a dead window.
    if (m_groupMoveWindow == window) {
        m_groupMoveWindow.clear();
        KWin::clearPaintPreview(&m_presentation);
        updateDragPreview();
        emitPresentationTransitionDiag();
    }
    if (m_gestureSource != nullptr) {
        const QString id = m_gestureWinIds.take(window);
        if (!id.isEmpty() && m_gestureSource->gestureAborted(id)) {
            emitGestureDiag("aborted", "dropped");
        }
    } else {
        m_gestureWinIds.remove(window);
    }
    m_gestureAttached.remove(window);
}

void ActiveWindowBorderEffect::onGestureDragStart(EffectWindow *window)
{
    if (window == nullptr || window->isDeleted() || m_gestureSource == nullptr) {
        return;
    }
    Window *inner = window->window();
    if (inner == nullptr || inner->isDeleted()) {
        return;
    }
    // Native move-vs-resize classification from the live gesture flags
    // (never assumed): a started move loop carries modal-move truth.
    const bool isMove = inner->isInteractiveMove();
    if (!isMove && !inner->isInteractiveResize()) {
        // Not a tracked move/resize start: a refused/unsupported new start
        // must not leave an older same-window open lifecycle to receive a
        // later Esc/finish. Drop it without a terminal: the missed finish
        // is an observation gap, never a fabricated outcome.
        const QString unsupportedId =
            QString::fromUtf8(window->internalId().toString(QUuid::WithoutBraces).toUtf8());
        if (!unsupportedId.isEmpty() && m_gestureSource->gestureAborted(unsupportedId)) {
            emitGestureDiag("refused", "idle");
        }
        return;
    }
    // A new gesture drops any held drop-preview latch immediately: the
    // latch belongs to exactly one live gesture, and a late
    // previous-gesture paint must not reseed the new one.
    KWin::clearPaintPreview(&m_presentation);
    updateDragPreview();
    updateGroupMoveArm(window);
    const QString id =
        QString::fromUtf8(window->internalId().toString(QUuid::WithoutBraces).toUtf8());
    m_gestureWinIds.insert(window, id);
    const GesturePointer pointer = gesturePointerNow();
    const QString kind = isMove ? QStringLiteral("move") : QStringLiteral("resize");
    const quint64 serial =
        m_gestureSource->gestureStarted(id, kind, gestureMoveResizeRect(window), pointer.has, pointer.point, isMove);
    if (serial == 0) {
        // Refused (bad window identity, unknown kind, out-of-bound frame,
        // or serial exhaustion): same fail-closed drop of the older
        // same-window open lifecycle, never a terminal.
        m_gestureSource->gestureAborted(id);
        m_gestureWinIds.remove(window);
        emitGestureDiag("refused", "idle");
        return;
    }
    emitGestureDiag("started", isMove ? "move" : "resize");
}

void ActiveWindowBorderEffect::onGestureDragStep(EffectWindow *window)
{
    if (window == nullptr || window->isDeleted() || m_gestureSource == nullptr) {
        return;
    }
    const QString id = m_gestureWinIds.value(window);
    if (id.isEmpty()) {
        return;
    }
    const GesturePointer pointer = gesturePointerNow();
    m_gestureSource->gestureCursor(id, pointer.has, pointer.point);
}

void ActiveWindowBorderEffect::onGestureDragFinish(EffectWindow *window)
{
    // Finish/cancel clears the exact tracked move window plus the held
    // drop-preview latch first, from the authoritative gesture callback:
    // preview hiding never depends on verdict success or readable
    // geometry.
    if (window != nullptr && m_groupMoveWindow == window) {
        m_groupMoveWindow.clear();
        KWin::clearPaintPreview(&m_presentation);
        updateDragPreview();
        emitPresentationTransitionDiag();
    }
    if (window == nullptr || window->isDeleted() || m_gestureSource == nullptr) {
        return;
    }
    const QString id = m_gestureWinIds.value(window);
    if (id.isEmpty()) {
        return;
    }
    // The fence arm (if an Esc reached InteractiveMoveResize during this
    // gesture) is consumed synchronously here, in the same dispatch the
    // builtin filter ended: armed finishes Cancelled, all others Released.
    const GesturePointer pointer = gesturePointerNow();
    const GesturePhase phase = m_gestureSource->gestureFinished(id, pointer.has, pointer.point);
    if (phase != GesturePhase::Idle) {
        emitGestureDiag("finished", gesturePhaseName(phase));
    }
}

void ActiveWindowBorderEffect::noteGestureEsc()
{
    // Fence arm only, and only when the exact currently active native
    // move/resize window matches the tracked open native identity in an
    // interactive mode: a stray Esc (no gesture, another window, or a mode
    // that already ended) never arms a future or foreign lifecycle. The
    // store additionally drops the arm when nothing is open and every
    // begin clears it, so a refused or unsupported start can never leave an
    // older same-window open armed for a later Esc.
    if (m_gestureSource == nullptr || !m_gestureSource->hasOpen()) {
        return;
    }
    Window *native = nullptr;
    try {
        if (workspace() != nullptr) {
            native = workspace()->moveResizeWindow();
        }
    } catch (...) {
        return;
    }
    if (native == nullptr || native->isDeleted()) {
        return;
    }
    try {
        if (!native->isInteractiveMove() && !native->isInteractiveResize()) {
            return;
        }
    } catch (...) {
        return;
    }
    QString nativeId;
    try {
        nativeId = QString::fromUtf8(native->internalId().toString(QUuid::WithoutBraces).toUtf8());
    } catch (...) {
        return;
    }
    if (nativeId.isEmpty() || nativeId != m_gestureSource->openWin()) {
        return;
    }
    m_gestureSource->armEsc();
}

void ActiveWindowBorderEffect::onTrackedFrameChanged()
{
    // The tracked (active) window frame moved: the stored border scene
    // rect may be stale, so the ring hides until a strictly newer accepted
    // paint arrives. No live-frame comparison: a move that returns to its
    // start stays hidden without a fresh paint.
    if (!m_isOpenGL) {
        return;
    }
    KWin::markBorderGeometryDirty(&m_presentation);
    updateBorder();
    emitPresentationTransitionDiag();
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
        // A displayed paint must hide immediately on maximize and may only
        // return on the restore transition; the live veto below keeps a
        // maximized-before-paint window from ever showing it.
        updateGroupVisibility();
        updateDragPreview();
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

void ActiveWindowBorderEffect::emitGestureDiag(const char *outcome, const char *detail)
{
    // Redacted transition diagnostic only: outcome plus a closed-vocabulary
    // detail (kind or phase). Never carries coordinates, identities, serials,
    // or geometry.
    try {
        logActiveBorderDiag(QStringLiteral("omnitiler:drag-gesture:gesture outcome=%1 detail=%2")
                .arg(QString::fromUtf8(outcome))
                .arg(QString::fromUtf8(detail)));
    } catch (...) {
    }
}

void ActiveWindowBorderEffect::emitGestureEndpoint()
{
    try {
        logActiveBorderDiag(QStringLiteral("omnitiler:drag-gesture:endpoint available=%1")
                .arg(m_gestureDbusAvailable ? 1 : 0));
    } catch (...) {
    }
}

bool ActiveWindowBorderEffect::ensureGestureEndpointRegistered()
{
    // Gesture endpoint on the effect-owned dedicated connection (same bus,
    // same topology as the ActiveBorder endpoint), so held WaitGesture
    // replies route cross-connection instead of hitting the local-loop
    // limit. Independent of the ActiveBorder endpoint outcome, like the
    // group endpoint is of the gesture outcome.
    if (!m_groupBus.isConnected()) {
        if (!m_groupBusName.isEmpty()) {
            QDBusConnection::disconnectFromBus(m_groupBusName);
            m_groupBus = QDBusConnection::connectToBus(QDBusConnection::SessionBus, m_groupBusName);
        }
        if (!m_groupBus.isConnected()) {
            return false;
        }
        // A (re)connected bus invalidates the previous binding: the source
        // holds exactly the registering connection, so rebind and re-mint
        // the instance (the script treats it as unknown until fresh facts).
        delete m_gestureSource;
        m_gestureSource = nullptr;
        m_gestureDbusAvailable = false;
    } else if (m_gestureDbusAvailable && m_gestureSource != nullptr) {
        return true;
    }
    if (m_gestureSource == nullptr) {
        m_gestureSource = new GestureFactsSource(m_groupBus, this);
    }
    bool serviceOk = false;
    bool objectOk = false;
    if (ensureDbusEndpointOn(m_groupBus, QStringLiteral("com.omnitiler.Gesture"),
            QStringLiteral("/com/omnitiler/Gesture"), m_gestureSource, &serviceOk, &objectOk)) {
        m_gestureDbusAvailable = true;
        emitGestureEndpoint();
        return true;
    }
    if (!m_gestureEndpointFailedLogged) {
        m_gestureEndpointFailedLogged = true;
        logActiveBorderDiag(QStringLiteral("omnitiler:drag-gesture:endpoint stage=failed service=%1 object=%2")
                .arg(serviceOk ? 1 : 0)
                .arg(objectOk ? 1 : 0));
        emitGestureEndpoint();
    }
    return false;
}

void ActiveWindowBorderEffect::ensureGestureFence()
{
    // Late install when input redirection was null at construction. Passive
    // observer only: never grabs, consumes, or modifies events.
    if (m_gestureFence != nullptr) {
        return;
    }
    if (input() == nullptr) {
        return;
    }
    m_gestureFence = new GestureEscFence(this);
    input()->installInputEventFilter(m_gestureFence);
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

void ActiveWindowBorderEffect::ensureEndpointsRegistered()
{
    // Idempotent: never re-registers while healthy, never polls. Called from
    // construction plus the existing activation and reconfigure events.
    // The ActiveBorder endpoint registers on the effect-owned dedicated
    // connection (same bus, same service/path/interface/shape). The
    // Gesture endpoint registers beside the ActiveBorder object on the
    // dedicated connection (cross-connection held replies); a dropped
    // dedicated connection recovers even while an endpoint still reads
    // available: the bus check below re-enters the reconnect path.
    if (!m_groupDbusAvailable || !m_groupBus.isConnected()) {
        ensureGroupEndpointRegistered();
    }
    // Gesture lifecycle endpoint stays independent of the other outcomes,
    // so one failure never hides another service.
    if (!m_gestureDbusAvailable || !m_groupBus.isConnected()) {
        ensureGestureEndpointRegistered();
    }
    ensureGestureFence();
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

void ActiveWindowBorderEffect::emitPresentationSetterDiag(const char *outcome)
{
    // Every SetPresentation receipt: outcome plus bounded scalars. Fixed
    // tokens and bits, no identities, payload, geometry, or revision.
    try {
        const int members = m_presentation.underlayMembers.size() > 9999 ? 9999 : static_cast<int>(m_presentation.underlayMembers.size());
        logActiveBorderDiag(QStringLiteral("omnitiler:presentation:setter outcome=%1 has=%2 bvis=%3 uvis=%4 pvis=%5 members=%6 anchor=%7 fmatch=%8 move=%9 elig=%10 ep=%11")
                .arg(QString::fromUtf8(outcome))
                .arg(m_presentation.has ? 1 : 0)
                .arg((m_presentation.has && m_presentation.borderVisible) ? 1 : 0)
                .arg((m_presentation.has && m_presentation.underlayFound) ? 1 : 0)
                .arg((m_presentation.has && m_presentation.previewHas) ? 1 : 0)
                .arg(members)
                .arg(QString::fromUtf8(m_presentationAnchorDiag))
                .arg(foregroundMatchesNow() ? 1 : 0)
                .arg(groupMoveLiveNow() ? 1 : 0)
                .arg(isLiveEligibleNow() ? 1 : 0)
                .arg(m_groupDbusAvailable ? 1 : 0));
    } catch (...) {
    }
}

void ActiveWindowBorderEffect::syncPresentationTransitionDiag()
{
    m_presentationTransDiagAnchor = m_presentationAnchorDiag;
    m_presentationTransDiagVisible = m_groupVisible;
    m_presentationTransDiagEmitted = true;
}

void ActiveWindowBorderEffect::emitPresentationTransitionDiag()
{
    // Combined anchor/visibility line only on material change.
    try {
        const QString curAnchor = QString::fromUtf8(m_presentationAnchorDiag);
        const QString lastAnchor = QString::fromUtf8(m_presentationTransDiagAnchor);
        if (m_presentationTransDiagEmitted && lastAnchor == curAnchor && m_presentationTransDiagVisible == m_groupVisible) {
            return;
        }
        m_presentationTransDiagAnchor = m_presentationAnchorDiag;
        m_presentationTransDiagVisible = m_groupVisible;
        m_presentationTransDiagEmitted = true;
        const int members = m_presentation.underlayMembers.size() > 9999 ? 9999 : static_cast<int>(m_presentation.underlayMembers.size());
        logActiveBorderDiag(QStringLiteral("omnitiler:presentation:transition anchor=%1 members=%2 fmatch=%3 move=%4 elig=%5 ep=%6 vis=%7")
                .arg(curAnchor)
                .arg(members)
                .arg(foregroundMatchesNow() ? 1 : 0)
                .arg(groupMoveLiveNow() ? 1 : 0)
                .arg(isLiveEligibleNow() ? 1 : 0)
                .arg(m_groupDbusAvailable ? 1 : 0)
                .arg(m_groupVisible ? 1 : 0));
    } catch (...) {
    }
}

bool ActiveWindowBorderEffect::nativeMaximizedNow(EffectWindow *window) const
{
    if (window == nullptr) {
        return false;
    }
    return m_maximizedWindows.contains(window);
}

bool ActiveWindowBorderEffect::foregroundMatchesNow() const
{
    // Stale paints never render under a new focus: the live active
    // window's bare-UUID identity must equal the published foreground
    // token, and the token must be non-empty.
    if (!m_presentation.has || m_presentation.foreground.isEmpty()) {
        return false;
    }
    EffectWindow *active = effects->activeWindow();
    if (active == nullptr || active->isDeleted()) {
        return false;
    }
    const QByteArray activeId = active->internalId().toString(QUuid::WithoutBraces).toUtf8();
    return !activeId.isEmpty() && activeId == m_presentation.foreground;
}

bool ActiveWindowBorderEffect::isLiveEligibleNow() const
{
    // Hide-only veto: a live active window that is not deleted, minimized,
    // fullscreen, hidden, maximized, or an applet popup. Core visibility
    // plus the foreground match decide; this only vetoes.
    EffectWindow *window = effects->activeWindow();
    if (window == nullptr) {
        return false;
    }
    if (window->isDeleted() || window->isMinimized() || window->isFullScreen() || window->isHidden()
        || m_maximizedWindows.contains(window) || window->isAppletPopup()) {
        return false;
    }
    return true;
}

void ActiveWindowBorderEffect::updateBorder()
{
    if (!m_isOpenGL) {
        return;
    }

    // Pure renderer: the held accepted border shape paints only on a live
    // foreground match, a fresh (geometry-clean) paint, plus the hide-only
    // vetoes. No eligibility or geometry derivation: inner/outer arrive
    // complete in KWin logical units. The ring replays the published outer
    // exactly: its stroke is derived strictly from the published pair
    // (finding C), so capture-vs-live config drift cannot move the drawn
    // edge; color and radius stay live host styling.
    const bool hasPaint = m_presentation.has && m_presentation.borderVisible && m_groupDbusAvailable;
    EffectWindow *window = effects->activeWindow();
    const bool foregroundMatches = foregroundMatchesNow();
    const bool nativeMaximized = window ? m_maximizedWindows.contains(window) : false;
    const bool fullScreen = window ? window->isFullScreen() : false;
    const bool appletPopup = window ? window->isAppletPopup() : false;
    const bool liveEligible = isLiveEligibleNow();
    double stroke = 0.0;
    bool strokeOk = false;
    if (hasPaint) {
        if (const auto derived = KWin::borderStrokeFromPublished(m_presentation.borderInner, m_presentation.borderOuter)) {
            stroke = *derived;
            strokeOk = true;
        }
    }
    const bool fresh = KWin::presentationBorderFresh(&m_presentation);
    const bool visible = hasPaint && foregroundMatches && liveEligible && strokeOk && fresh;
    // Transition diagnostic only: first evaluation plus visibility flips.
    // Never affects the border or repaint decision below.
    emitActiveBorderVisible(visible,
        activeBorderDiagReason(hasPaint, foregroundMatches, window != nullptr, window ? window->isDeleted() : false,
            window ? window->isMinimized() : false, fullScreen, nativeMaximized, appletPopup, m_groupDbusAvailable),
        appletPopup);
    if (visible) {
        // Item-local anchoring translation only: the stored inner rect
        // (scene space, KWin logical pixels) maps into the tracked window
        // item. This is a space translation, not a geometric outset: no
        // gap, width, DPI, or device scaling is applied here. The outline
        // replays the published stroke so the drawn outer edge equals the
        // published outer rect exactly.
        const QRectF innerRect(static_cast<qreal>(m_presentation.borderInner.x()), static_cast<qreal>(m_presentation.borderInner.y()),
            static_cast<qreal>(m_presentation.borderInner.width()), static_cast<qreal>(m_presentation.borderInner.height()));
        m_borderItem.setOutline(ringOutlineForWidth(stroke));
        m_borderItem.setInnerRect(window ? window->windowItem()->mapFromScene(innerRect) : RectF());
    }
    m_borderItem.setVisible(visible);
    effects->addRepaintFull();
}

void ActiveWindowBorderEffect::applyPresentation(const QString &payload)
{
    // Unavailable endpoint never displays: fail closed.
    if (!m_groupDbusAvailable) {
        clearPresentation();
        emitPresentationSetterDiag("endpoint-unavailable");
        syncPresentationTransitionDiag();
        return;
    }
    const PresentationApply result = applyPresentationPaint(&m_presentation, payload);
    if (result == PresentationApply::StaleIgnored) {
        // Same-generation older revision: ignored without touching the
        // newer held display.
        emitPresentationSetterDiag("stale");
        syncPresentationTransitionDiag();
        return;
    }
    if (result == PresentationApply::Malformed) {
        // Leave m_groupAnchor for updateGroupAnchorAndGeometry so the old
        // anchor disconnects its frame remap; the update nulls it when no
        // paint stays.
        updateGroupAnchorAndGeometry();
        updateGroupVisibility();
        updateBorder();
        updateDragPreview();
        if (m_isOpenGL) {
            effects->addRepaintFull();
        }
        emitPresentationSetterDiag("malformed");
        syncPresentationTransitionDiag();
        return;
    }
    updateGroupAnchorAndGeometry();
    updateGroupVisibility();
    updateBorder();
    updateDragPreview();
    if (m_isOpenGL) {
        effects->addRepaintFull();
    }
    emitPresentationSetterDiag("accepted");
    syncPresentationTransitionDiag();
}

void ActiveWindowBorderEffect::clearPresentation()
{
    // The held paint clears (generation rotation, focus activation,
    // malformed reads, or an explicit clear). Order fencing lives in the
    // held (gen, rev): a cleared paint accepts the next newer-or-equal
    // revision, never resurrecting older state through a late callback
    // (the subscriber fences requests; native fences revisions).
    const bool hadPaint = m_presentation.has;
    clearPresentationPaint(&m_presentation);
    // Leave m_groupAnchor for updateGroupAnchorAndGeometry so the old anchor
    // disconnects its frame remap; the update nulls it.
    updateGroupAnchorAndGeometry();
    updateGroupVisibility();
    updateBorder();
    updateDragPreview();
    if (hadPaint && m_isOpenGL) {
        effects->addRepaintFull();
        emitPresentationTransitionDiag();
    }
}

QString ActiveWindowBorderEffect::presentationStatus() const
{
    return QStringLiteral("v=1;has=%1;bvis=%2;uvis=%3;pvis=%4;ep=%5;gl=%6;vis=%7")
        .arg(m_presentation.has ? 1 : 0)
        .arg((m_presentation.has && m_presentation.borderVisible) ? 1 : 0)
        .arg((m_presentation.has && m_presentation.underlayFound) ? 1 : 0)
        .arg((m_presentation.has && m_presentation.previewHas) ? 1 : 0)
        .arg(m_groupDbusAvailable ? 1 : 0)
        .arg(m_isOpenGL ? 1 : 0)
        .arg(m_groupVisible ? 1 : 0);
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
    // Movement-only chord observation (both Win and Shift held; extras
    // allowed, either press order): folded here only as the Stage 3 raw
    // ingress fact. Native visibility no longer gates on the chord: the
    // core underlay Found already encodes the trigger.
    const bool wasHeld = m_chordHeld;
    m_chordHeld = modifiers.testFlag(Qt::MetaModifier) && modifiers.testFlag(Qt::ShiftModifier);
    // A chord level/known flip is a published fact change: push the observed
    // level into the versioned script facts so a held waiter completes with
    // the fresh snapshot instead of holding stale identity.
    if (!wasKnown || wasHeld != m_chordHeld) {
        noteFactsChord();
    }
    updateGroupVisibility();
    emitPresentationTransitionDiag();
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
    // Pure renderer: the held accepted underlay Found paints only on a live
    // foreground match, a fresh (geometry-clean) paint, plus the hide-only
    // vetoes. The core Found already encodes the trigger (chord or matching
    // move); native code performs no trigger, eligibility, or geometry
    // derivation. A missing anchor (no live member window) keeps the
    // underlay hidden. The underlay depends on ALL held members, not just
    // the anchor: any member frame change dirties it via
    // onGroupMemberFrameChanged.
    const bool underlayShow = m_presentation.has && m_presentation.underlayFound && foregroundMatchesNow() && isLiveEligibleNow()
        && m_groupDbusAvailable && KWin::presentationUnderlayFresh(&m_presentation);
    const bool anchorOk = !m_groupAnchor.isNull() && m_groupAnchor->windowItem() != nullptr && !m_presentation.underlayMembers.isEmpty();
    const bool show = underlayShow && anchorOk;
    if (show != m_groupVisible) {
        m_groupVisible = show;
        m_groupItem.setVisible(show);
        effects->addRepaintFull();
    } else {
        m_groupItem.setVisible(show);
    }
}

void ActiveWindowBorderEffect::updateDragPreview()
{
    if (!m_isOpenGL) {
        return;
    }
    // Pure renderer: the held accepted preview rect paints only with live
    // interactive-move liveness (a gesture is actually in flight) plus the
    // endpoint gate. The published `win` is the insertion neighbor, never
    // the mover, so no identity match is performed: liveness plus the
    // subscriber's generation/request fencing (and the Finish-time liveness
    // drop above) keep stale previews from ever showing.
    const bool show = m_presentation.has && m_presentation.previewHas && groupMoveLiveNow() && m_groupDbusAvailable;
    if (show != m_dragPreviewVisible) {
        m_dragPreviewVisible = show;
        if (show) {
            // Consumed verbatim in overlay (scene) space: KWin logical
            // pixels, no outset or scaling.
            m_dragPreviewRect = m_presentation.previewRect;
            m_dragPreviewItem.setPosition(
                QPointF(static_cast<qreal>(m_dragPreviewRect.x()), static_cast<qreal>(m_dragPreviewRect.y())));
            m_dragPreviewItem.setSize(
                QSizeF(static_cast<qreal>(m_dragPreviewRect.width()), static_cast<qreal>(m_dragPreviewRect.height())));
        }
        m_dragPreviewItem.setVisible(show);
        effects->addRepaintFull();
    } else {
        if (show) {
            m_dragPreviewItem.setVisible(show);
        } else {
            m_dragPreviewItem.setVisible(false);
        }
        m_dragPreviewVisible = show;
    }
}

void ActiveWindowBorderEffect::updateDragPreviewFill()
{
    QImage previewImage(1, 1, QImage::Format_ARGB32);
    previewImage.fill(ActiveBorderConfig::dragPreviewColor());
    m_dragPreviewItem.setImage(previewImage);
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
