// Narrow KWin 6.7.5 scripting-surface subset, pinned to the KWin source
// fidelity reference recorded in
// docs/changes/custom-tile-vertical-slice/research/type-provenance.md
// (realized source tarball, sha256-23p9unGqyh5SGHM7gPkKmY2E4qs25NYtDj6gA3bFgC0=).
//
// Ambient and import-free so these declarations stay global. Strict controls:
// no `any`, `Function`, broad index signatures, unchecked casts, non-null
// assertions, or speculative KDE declarations.
//
// Output identity is session-local only: exact Output object reference
// equality plus VirtualDesktop id. KWin retains persistent topology; no stable
// identity survives restart or hotplug. Output.uuid is neither Q_PROPERTY nor
// Q_INVOKABLE and is not declared.

// src/scripting/scripting.h:
//     bool registerShortcut(const QString &, const QString &, const QString &,
//                           const QJSValue &)
declare function registerShortcut(
    name: string,
    text: string,
    sequence: string,
    callback: () => void,
): boolean;

// KWin scripting API -> Global -> Functions:
//     QVariant readConfig(QString key, QVariant defaultValue = QVariant())
// Reads the script's config value for key; without a default and with no stored
// value an undefined value is returned. Used here only for the selected
// shortcut-profile, workspace-mode, and bounded gap keys; no other script
// configuration is read.
declare function readConfig(key: string, defaultValue?: unknown): unknown;

// src/scripting/scripting.h:
//     Q_INVOKABLE void callDBus(const QString &service, const QString &path,
//                               const QString &interface, const QString &method,
//                               const QJSValue &arg1..arg9)
// Installed as a script global by src/scripting/scripting.cpp:237-251. The
// trailing argument may be a callable callback, invoked exactly once with the
// D-Bus reply arguments on a later event-loop turn (scripting.cpp:301-374); an
// error reply logs and never invokes the callback. Used here only as a
// guaranteed one-shot event-loop yield (ListNames), never for data transport.
declare function callDBus(
    service: string,
    path: string,
    dbusInterface: string,
    method: string,
    ...args: readonly unknown[]
): void;

// src/scripting/scripting.cpp:224-227 installs the KWin Options singleton as
// the script global `options` (CppOwnership). src/options.h declares the
// `void configChanged()` notify signal; src/options.cpp:657-662 emits it from
// updateSettings(), which src/workspace.cpp slotReconfigure() invokes after
// reparsing configuration for `org.kde.KWin /KWin reconfigure`
// (src/dbusinterface.cpp:64-67, src/workspace.cpp:998-1017). Declared unknown
// here: the controller only reads its `configChanged` signal surface through
// the feature-detecting capability seam, never option values.
declare const options: unknown;

// src/scripting/scripting.cpp installs QJSEngine::ConsoleExtension before
// evaluating the generated script.
interface Console {
    log(...values: readonly unknown[]): void;
}
declare var console: Console;

// QObject signal surface every KWin script signal exposes.
interface Signal {
    connect(callback: () => void): void;
    disconnect(callback: () => void): void;
}
interface Signal1<T> {
    connect(callback: (value: T) => void): void;
    disconnect(callback: (value: T) => void): void;
}
interface Signal3<T1, T2, T3> {
    connect(callback: (first: T1, second: T2, third: T3) => void): void;
    disconnect(callback: (first: T1, second: T2, third: T3) => void): void;
}

// Script geometry/size/point boundary values (QRect/QRectF, QSizeF, QPointF).
interface Rect {
    x: number;
    y: number;
    width: number;
    height: number;
}
interface Size {
    width: number;
    height: number;
}
interface Point {
    x: number;
    y: number;
}

// src/tiles/tile.h: Tile::LayoutDirection, values Floating=0, Horizontal=1,
// Vertical=2. Plain (non-const) enum keeps the runtime representation explicit.
declare enum LayoutDirection {
    Floating = 0,
    Horizontal = 1,
    Vertical = 2,
}

// src/virtualdesktops.h: virtual-desktop ID is a scripting-facing string
// property. x11DesktopNumber is the 1-based desktop number
// (src/virtualdesktops.h Q_PROPERTY(uint x11DesktopNumber READ
// x11DesktopNumber)); declared optional here so the boundary capability type
// stays structurally assignable, and read defensively only to order the live
// `workspace.desktops` list. The controller's 1-based navigation index is
// positional order, never this number.
interface VirtualDesktop {
    readonly id: string;
    readonly x11DesktopNumber?: number;
    // Writable: src/virtualdesktops.h at pinned v6.7.5 declares
    //     Q_PROPERTY(QString name READ name WRITE setName NOTIFY nameChanged)
    // Assigning renames the desktop. The controller writes only a
    // newly-created exactly-attributed desktop to its friendly `Desktop N`
    // name, never an adopted or existing desktop.
    name: string;
}

// src/core/output.h: only scripting-exposed Q_PROPERTY members. uuid() is
// neither Q_PROPERTY nor Q_INVOKABLE and is excluded.
interface Output {
    readonly geometry: Rect;
    readonly name: string;
    readonly manufacturer: string;
    readonly model: string;
    readonly serialNumber: string;
}

// src/window.h: required Window properties and signals, including the
// interactive move signals. Pointer signal arguments are conservatively
// nullable at the JS boundary.
interface Window {
    readonly normalWindow: boolean;
    readonly managed: boolean;
    // src/window.h Q_PROPERTY `bool deleted READ isDeleted CONSTANT` at
    // pinned v6.7.4: whether this window is already deleted and only kept
    // for compositor animations. Read-only: deleted windows never ride the
    // raw capture lane.
    readonly deleted: boolean;
    // src/window.h Q_PROPERTY `bool wantsInput READ wantsInput` at pinned
    // v6.7.4: whether the window accepts input. Read-only: false maps to
    // the shared no-activate safety gate.
    readonly wantsInput: boolean;
    // src/window.h Q_PROPERTY `bool dialog READ isDialog CONSTANT` at
    // pinned v6.7.4: whether the window is a dialog. Read-only.
    readonly dialog: boolean;
    // src/window.h Q_PROPERTY `bool modal READ isModal` at pinned v6.7.4:
    // whether the window is modal. Read-only: modal counts as transient
    // for shared membership classification.
    readonly modal: boolean;
    readonly resizeable: boolean;
    // src/window.h Q_PROPERTY(QSizeF minSize READ minSize) and maxSize. These
    // are read only and used only by trace diagnostics for native constraints.
    readonly minSize: Size;
    readonly maxSize: Size;
    readonly appletPopup: boolean;
    // QList<VirtualDesktop *> has no established JavaScript marshalling contract.
    // Read-write in the official KWin scripting API (KWin::Window -> Read-write
    // Properties -> `desktopList`/`desktops`); a JS array assignment sets the
    // window's virtual-desktop membership. Written only through the guarded
    // boundary seam for workspace moves.
    desktops: unknown;
    readonly output: Output | null;
    // Writable: src/window.h at pinned v6.7.4 declares
    //     Q_PROPERTY(KWin::Tile *tile READ requestedTile WRITE
    //     setTileCompatibility NOTIFY tileChanged)
    // Assigning null detaches the window from its requested tile (unmanage)
    // and returns it to floating.
    tile: Tile | null;
    // Read-write: official KWin scripting API -> KWin::Window -> Read-write
    // Properties -> `QRectF frameGeometry`. Written only through the guarded
    // boundary seam for float geometry.
    frameGeometry: Rect;
    // Read-only in the official KWin scripting API (KWin::Window ->
    // Read-only Properties -> `clientGeometry`; src/window.h Q_PROPERTY
    // `KWin::RectF clientGeometry READ clientGeometry NOTIFY
    // clientGeometryChanged` at pinned v6.7.4): the geometry of the window
    // without frame borders, in global screen coordinates. Read-only: the
    // adapter captures it for client-to-decorated size-hint conversion,
    // never writes it.
    readonly clientGeometry: Rect;
    // Read-write: official KWin scripting API -> KWin::Window -> Read-write
    // Properties -> `bool onAllDesktops`. Written only through the guarded
    // boundary seam for sticky floating.
    onAllDesktops: boolean;
    // Read-write: src/window.h Q_PROPERTY `bool keepAbove READ keepAbove WRITE
    // setKeepAbove NOTIFY keepAboveChanged`. The float adapter records and
    // restores the prior value around its intentional floating state.
    keepAbove: boolean;
    // Read-write: src/window.h Q_PROPERTY `bool keepBelow READ keepBelow WRITE
    // setKeepBelow NOTIFY keepBelowChanged`. KWin maintains keep-above/below
    // exclusivity; the float adapter restores an initial keep-below state.
    keepBelow: boolean;
    // Read-write in the official KWin scripting API (KWin::Window -> Read-write
    // Properties -> `bool fullScreen`; https://develop.kde.org/docs/plasma/kwin/api/).
    // Declared writable here only for the explicit project fullscreen
    // toggle through the guarded boundary seam; observation paths read it
    // and cover-and-restore stays KWin-owned.
    fullScreen: boolean;
    // Read-only in the KWin scripting API: `KWin::Window.maximizeMode`
    // (Q_PROPERTY `KWin::MaximizeMode maximizeMode READ maximizeMode NOTIFY
    // maximizedChanged`, window.h; a read-only `KWin::MaximizeMode` enum:
    // 0=restore, 1=vertical, 2=horizontal, 3=full). The property itself is
    // read-only; restoration uses the separate Q_INVOKABLE `setMaximize`.
    readonly maximizeMode: number;
    // src/window.h Q_INVOKABLE `setMaximize(bool vertically, bool
    // horizontally, const RectF &restore = RectF())`. Calling false, false
    // restores every nonzero maximize mode.
    setMaximize(vertically: boolean, horizontally: boolean): void;
    // Read-only: official KWin scripting API -> KWin::Window -> Read-only
    // Properties -> `minimized`.
    // Declared read-only: adapters fail closed if this public property cannot
    // be observed while evaluating minimized-window exclusions.
    readonly minimized: boolean;
    // src/window.h Q_PROPERTY `bool transient READ isTransient NOTIFY
    // transientChanged` plus `Q_PROPERTY(Window *transientFor READ
    // transientFor NOTIFY transientChanged)` at pinned v6.7.4: whether this
    // window is a transient (dialog) and the window it is transient for
    // (null when independent). Read-only: the migration adapter tracks
    // transient descendants implicitly (no explicit setters) and gates on
    // protected transients; it never writes these properties.
    readonly transient: boolean;
    readonly transientFor: Window | null;
    // Read-only `QUuid internalId` (window.h Q_PROPERTY `QUuid internalId
    // READ internalId CONSTANT` at pinned v6.7.4; observed in scripting as
    // the opaque `String(Window.internalId)` identity used by proof scripts).
    // Declared read-only unknown here: the advisory observer only coerces it
    // via `String(...)` and validates the opaque result, never relying on a
    // string-typed binding.
    readonly internalId: unknown;
    // Documented Window property (KWin scripting API): the window's caption
    // (title) string. Read for snapshot observability only.
    readonly caption: string;
    // src/window.h exposes QML `move` / `resize`, backed by
    // isInteractiveMove() / isInteractiveResize(). Move and resize are
    // distinguished before drag state is captured.
    readonly move: boolean;
    readonly resize: boolean;
    readonly activeChanged: Signal;
    readonly desktopsChanged: Signal;
    // KWin emits the old output; re-read output for the current value.
    readonly outputChanged: Signal1<Output | null>;
    readonly tileChanged: Signal1<Tile | null>;
    readonly interactiveMoveResizeStarted: Signal;
    readonly interactiveMoveResizeStepped: Signal1<Rect>;
    readonly interactiveMoveResizeFinished: Signal;
    readonly moveResizedChanged: Signal;
    // src/window.h at pinned v6.7.4 declares
    //     Q_PROPERTY(KWin::RectF frameGeometry READ frameGeometry WRITE
    //     moveResize NOTIFY frameGeometryChanged)
    // with Q_SIGNALS `void frameGeometryChanged(const KWin::RectF &oldGeometry)`.
    // Programmatic frameGeometry writes via moveResize emit this when actual
    // geometry changes (src/waylandwindow.cpp); moveResizedChanged only mirrors
    // interactive start/finish (src/window.cpp connects it to
    // interactiveMoveResizeStarted/Finished). Geometry fences for programmatic
    // writes must bind here.
    readonly frameGeometryChanged: Signal1<Rect>;
    // Documented notify signal for the `fullScreen` property (KWin scripting
    // API -> KWin::Window -> Signals -> `fullScreenChanged()`). Attached via the
    // feature-detecting environment seam, never assumed present.
    readonly fullScreenChanged: Signal;
    // Documented notify signal for the `maximizeMode` property (KWin scripting
    // API -> KWin::Window -> Signals -> `maximizedChanged()`; the `maximizeMode`
    // Q_PROPERTY is `NOTIFY maximizedChanged`). Attached via the optional
    // feature-detecting environment seam, never assumed present.
    readonly maximizedChanged: Signal;
}

// src/tiles/tile.h: tile tree (tiles/windows), geometry, layout, and
// manage/unmanage. bool manage(Window *); bool unmanage(Window *).
interface Tile {
    // src/tiles/tile.h: Q_PROPERTY(KWin::RectF relativeGeometry READ
    // relativeGeometry WRITE setRelativeGeometry NOTIFY relativeGeometryChanged).
    // Writable: assigning dispatches to Tile::setRelativeGeometry (CustomTile
    // overrides it and adjusts sibling tiles at the changed shared edges). The
    // value is in screen-relative [0,1] units. The neighbor-adjusting detail is
    // source-derived and not live-proven here.
    relativeGeometry: Rect;
    // Writable: src/tiles/tile.h exposes the uniform logical-pixel padding
    // property used by Custom Tile roots. KWin applies output scaling and
    // computes the resulting outer and adjacent spacing.
    padding: number;
    readonly absoluteGeometry: Rect;
    readonly parent: Tile | null;
    // QList QObject boundaries require runtime decoding before iteration.
    readonly tiles: unknown;
    readonly windows: unknown;
    readonly isLayout: boolean;
    readonly canBeRemoved: boolean;
    manage(window: Window): boolean;
    unmanage(window: Window): boolean;
    readonly relativeGeometryChanged: Signal;
    readonly absoluteGeometryChanged: Signal;
    readonly windowGeometryChanged: Signal;
    readonly paddingChanged: Signal1<number>;
    readonly minimumSizeChanged: Signal1<Size>;
    readonly rowChanged: Signal1<number>;
    readonly isLayoutChanged: Signal1<boolean>;
    readonly childTilesChanged: Signal;
    readonly windowAdded: Signal1<Window>;
    readonly windowRemoved: Signal1<Window>;
    readonly windowsChanged: Signal;
}

// src/tiles/customtile.h: CustomTile layout direction, layoutModified(), and
// QList<CustomTile *> split(LayoutDirection).
interface CustomTile extends Tile {
    readonly layoutDirection: LayoutDirection;
    readonly layoutDirectionChanged: Signal1<LayoutDirection>;
    readonly layoutModified: Signal;
    // QList<CustomTile *> requires runtime decoding before use.
    split(direction: LayoutDirection): unknown;
    // src/tiles/customtile.h: `Q_INVOKABLE void remove()`. The void return is
    // not an acknowledgement: callers must verify the root topology afterwards.
    remove(): void;
}

// Opaque tile model exposed by RootTile; no scripting-exposed members exist.
interface TileModel {}

// src/tiles/customtile.h: RootTile::model() and RootTile::pick(QPointF)
// returning Tile *.
interface RootTile extends CustomTile {
    readonly model: TileModel;
    pick(point: Point): Tile | null;
}

// src/scripting/scripting.h: ScriptTimer (QTimer subclass with a
// Q_INVOKABLE constructor), exposed by src/scripting/scripting.cpp as the
// global `QTimer` metaobject so scripts can do `new QTimer()`. Only the
// narrow singleShot/interval/timeout/start/stop surface this project uses
// is declared.
interface QTimer {
    interval: number;
    singleShot: boolean;
    readonly timeout: Signal;
    start(): void;
    stop(): void;
}
declare const QTimer: {
    new (): QTimer;
};

// src/scripting/workspace_wrapper.h: current-workspace/output surface.
interface Workspace {
    // QList<LogicalOutput *> requires runtime decoding before iteration.
    readonly screens: unknown;
    // Read-only: official KWin scripting API -> Workspace Scripting -> Output
    // properties -> `activeScreen`. The output that currently has keyboard
    // focus; used as the active output when no window is focused (spec D).
    readonly activeScreen: Output | null;
    readonly cursorPos: Point;
    // QList<VirtualDesktop *> requires runtime decoding before iteration.
    readonly desktops: unknown;
    // Read-write: src/scripting/workspace_wrapper.h declares the WRITE setter
    //     Q_PROPERTY(KWin::VirtualDesktop *currentDesktop READ currentDesktop
    //     WRITE setCurrentDesktop NOTIFY currentDesktopChanged)
    // at pinned v6.7.5. Assigning switches the current virtual desktop.
    currentDesktop: VirtualDesktop | null;
    // src/scripting/workspace_wrapper.h at pinned v6.7.5 source tarball
    // SHA-256 6baa910b732d93c48c90f9c1cc685cc93d0b8de0cdf138c24192c045bc3a48e2:
    //     Q_SCRIPTABLE void createDesktop(int position, const QString &name)
    //     const
    // Creates a desktop at the zero-based position in [0, count] and
    // returns nothing. The controller re-enumerates `desktops` (id plus
    // native name) to resolve the new desktop and never relies on a
    // return value.
    createDesktop(position: number, name: string): void;
    // src/scripting/workspace_wrapper.h:
    //     Q_INVOKABLE void removeDesktop(VirtualDesktop *desktop)
    removeDesktop(desktop: VirtualDesktop): void;
    // src/scripting/workspace_wrapper.h:
    //     Q_INVOKABLE void setCurrentDesktopForScreen(VirtualDesktop *desktop,
    //                                                 Output *output)
    // KWin 6.7+ only: absent on KWin 6.6, where the shared currentDesktop is
    // current on every output. Runtime code must go through the
    // workspace-native capability seam (supportsPerOutputDesktops and
    // friends), never this declaration directly.
    setCurrentDesktopForScreen(desktop: VirtualDesktop, output: Output): void;
    // KWin 6.7.5 src/scripting/workspace_wrapper.h:614 declares the public
    // Q_SLOT `sendClientToScreen(KWin::Window *, KWin::LogicalOutput *)`.
    // The call initiates transfer only; callers must prove the resulting output,
    // desktop membership, geometry, and focus asynchronously before commit.
    sendClientToScreen(client: Window, output: Output): void;
    // src/scripting/workspace_wrapper.h `public Q_SLOTS` at pinned v6.7.4:
    // directional output switches routing to the native useractions
    // switchToOutput/setActiveOutput. They move keyboard focus to the
    // adjacent output without touching windows, desktops, or focus targets.
    // All four return void: callers must read back `activeScreen`. Used
    // only by whole-workspace migration follow for empty/sticky-active
    // routes when the target output is not already active; never retried.
    slotSwitchToLeftScreen(): void;
    slotSwitchToRightScreen(): void;
    slotSwitchToAboveScreen(): void;
    slotSwitchToBelowScreen(): void;
    // Writable: src/scripting/workspace_wrapper.h declares the WRITE setter
    //     Q_PROPERTY(KWin::Window *activeWindow READ activeWindow WRITE
    //     setActiveWindow NOTIFY windowActivated)
    // at pinned v6.7.4 source tarball
    // sha256-23p9unGqyh5SGHM7gPkKmY2E4qs25NYtDj6gA3bFgC0=.
    activeWindow: Window | null;
    // KWin 6.7+ only: absent on KWin 6.6. Same capability-seam rule as the
    // setter above.
    currentDesktopForScreen(output: Output): VirtualDesktop | null;
    // Script-visible hit test, verified at the pinned v6.7.4
    // (src/scripting/workspace_wrapper.h:483-495,
    // src/scripting/workspace_wrapper.cpp:369-395): up to count clients at
    // a point, topmost first, skipping deleted, off-activity,
    // off-desktop, minimized, show-desktop-hidden and non-hit windows.
    // Availability on any other KWin version is unproven: callers must
    // feature-detect the function itself (never assume presence), and no
    // current path calls it (Stage 6 cursor work consumes it). Cursor-warp
    // stays unsupported: no warp API exists on either the JS or the
    // effect side.
    windowAt(pos: Point, count?: number): unknown;
    rootTile(output: Output, desktop: VirtualDesktop): Tile | null;
    // src/scripting/workspace_wrapper.h: Q_SCRIPTABLE QRectF clientArea(
    //     ClientAreaOption option, Output *output, VirtualDesktop *desktop)
    // const. Returns the geometry for the requested ClientAreaOption (panel
    // struts accounted per option). PlacementArea is 0: the per-output usable
    // area for the given output and desktop. WorkArea is 5: the whole work
    // area across all screens together, never a per-output bound.
    clientArea(option: number, output: Output, desktop: VirtualDesktop): Rect;
    // The JavaScript-only QList<Window *> boundary requires runtime decoding.
    windowList(): unknown;
    readonly windowActivated: Signal1<Window | null>;
    readonly windowAdded: Signal1<Window>;
    readonly windowRemoved: Signal1<Window>;
    readonly screensChanged: Signal;
    readonly desktopsChanged: Signal;
    readonly currentDesktopChanged: Signal3<VirtualDesktop | null, VirtualDesktop | null, Output | null>;
    // Geometry-only outline rectangle slots (src/scripting/workspace_wrapper.h,
    // `showOutline(QRect)` / `showOutline(x, y, w, h)` / `hideOutline()`).
    showOutline(x: number, y: number, w: number, h: number): void;
    hideOutline(): void;
}

// src/scripting/workspace_wrapper.h: the scripting workspace singleton.
declare const workspace: Workspace;
