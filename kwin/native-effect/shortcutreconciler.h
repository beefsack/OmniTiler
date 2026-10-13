#pragma once

#include <QDBusArgument>
#include <QDBusMessage>
#include <QKeySequence>
#include <QList>
#include <QLoggingCategory>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QVariant>

#include <functional>

// Exact (ai)/a(ai) framing for QKeySequence/QSet<QKeySequence>> matching the
// KF6 KGlobalAccel encoding: each sequence is a struct holding exactly four
// combined key ints. Declared here so tests exercise the same operators.
QDBusArgument &operator<<(QDBusArgument &argument, const QKeySequence &sequence);
const QDBusArgument &operator>>(const QDBusArgument &argument, QKeySequence &sequence);
const QDBusArgument &operator>>(const QDBusArgument &argument, QSet<QKeySequence> &set);

// Bounded KCM shortcut-override backend/state machine.
//
// Closed ordered conflict-resolution table (only):
//   row 0 relocate: kwin/omnitiler-focus-right -> Meta+L post;
//     ksmserver/Lock Session Meta+L replaced by Meta+Esc post
//   row 1 clear: kwin/omnitiler-resize-outwards-up -> Meta+Alt+K;
//     KDE Keyboard Layout Switcher/Switch to Next Keyboard Layout cleared
//   row 2 clear: kwin/omnitiler-resize-outwards-right -> Meta+Alt+L;
//     KDE Keyboard Layout Switcher/Switch to Last-Used Keyboard Layout cleared
//   row 3 clear: kwin/omnitiler-resize-outwards-left-arrow -> Meta+Alt+Left;
//     kwin/Switch Window Left cleared
//   row 4 clear: kwin/omnitiler-resize-outwards-down-arrow -> Meta+Alt+Down;
//     kwin/Switch Window Down cleared
//   row 5 clear: kwin/omnitiler-resize-outwards-up-arrow -> Meta+Alt+Up;
//     kwin/Switch Window Up cleared
//   row 6 clear: kwin/omnitiler-resize-outwards-right-arrow -> Meta+Alt+Right;
//     kwin/Switch Window Right cleared
//   row 7 clear: kwin/omnitiler-toggle-float -> Meta+G;
//     kwin/Grid View cleared
//   row 8 clear: kwin/omnitiler-toggle-maximize -> Meta+M;
//     kwin/KrohnkiteMonocleLayout cleared
//   row 9 clear: kwin/omnitiler-focus-left-arrow -> Meta+Left;
//     kwin/Window Quick Tile Left cleared
//   row 10 clear: kwin/omnitiler-focus-down-arrow -> Meta+Down;
//     kwin/Window Quick Tile Bottom cleared
//   row 11 clear: kwin/omnitiler-focus-up-arrow -> Meta+Up;
//     kwin/Window Quick Tile Top cleared
//   row 12 clear: kwin/omnitiler-focus-right-arrow -> Meta+Right;
//     kwin/Window Quick Tile Right cleared
//   row 13 clear: kwin/omnitiler-move-left-arrow -> Meta+Shift+Left;
//     kwin/Window to Previous Screen cleared
//   row 14 clear: kwin/omnitiler-move-right-arrow -> Meta+Shift+Right;
//     kwin/Window to Next Screen cleared
//   row 15 clear: kwin/omnitiler-workspace-prev-left-arrow -> Meta+Ctrl+Left;
//     kwin/Switch One Desktop to the Left cleared
//   row 16 clear: kwin/omnitiler-workspace-prev-up-arrow -> Meta+Ctrl+Up;
//     kwin/Switch One Desktop Up cleared
//   row 17 clear: kwin/omnitiler-workspace-next-down-arrow -> Meta+Ctrl+Down;
//     kwin/Switch One Desktop Down cleared
//   row 18 clear: kwin/omnitiler-workspace-next-right-arrow -> Meta+Ctrl+Right;
//     kwin/Switch One Desktop to the Right cleared
//   row 19 clear: kwin/omnitiler-send-prev-left-arrow -> Meta+Ctrl+Shift+Left;
//     kwin/Window One Desktop to the Left cleared
//   row 20 clear: kwin/omnitiler-send-prev-up-arrow -> Meta+Ctrl+Shift+Up;
//     kwin/Window One Desktop Up cleared
//   row 21 clear: kwin/omnitiler-send-next-down-arrow -> Meta+Ctrl+Shift+Down;
//     kwin/Window One Desktop Down cleared
//   row 22 clear: kwin/omnitiler-send-next-right-arrow -> Meta+Ctrl+Shift+Right;
//     kwin/Window One Desktop to the Right cleared
//
// Uses only the KGlobalAccel D-Bus APIs proven on live Plasma 6.7.4:
//   org.kde.kglobalaccel /kglobalaccel org.kde.KGlobalAccel
//     allComponents -> ao
//     setShortcutKeys as,a(ai),u -> a(ai) with QSet<QKeySequence>
//       annotations on the keys input and the reply; actionId is
//       [ComponentUnique, ActionUnique, ComponentFriendly, ActionFriendly]
//   org.kde.kglobalaccel.Component allShortcutInfos s default -> a(ssssssaiai)
//   org.freedesktop.DBus GetNameOwner / GetConnectionUnixUser for owner/UID.
// No shell, no guessed identities. All backends are injectable; tests use
// deterministic fakes with no live mutation.

namespace KWin
{

inline constexpr int SHORTCUT_META_L = 268435532; // Meta+L (Qt Meta | Key_L)
inline constexpr int SHORTCUT_META_ESC = 285212672; // Meta+Esc (Qt Meta | Key_Escape)
inline constexpr int SHORTCUT_META_ALT_K = 402653259; // Meta+Alt+K catalog resize-outwards-up
inline constexpr int SHORTCUT_META_ALT_L = 402653260; // Meta+Alt+L catalog resize-outwards-right
inline constexpr int SHORTCUT_META_ALT_LEFT = 419430418; // Meta+Alt+Left catalog resize-outwards-left-arrow
inline constexpr int SHORTCUT_META_ALT_UP = 419430419; // Meta+Alt+Up catalog resize-outwards-up-arrow
inline constexpr int SHORTCUT_META_ALT_RIGHT = 419430420; // Meta+Alt+Right catalog resize-outwards-right-arrow
inline constexpr int SHORTCUT_META_ALT_DOWN = 419430421; // Meta+Alt+Down catalog resize-outwards-down-arrow
inline constexpr int SHORTCUT_META_G = 268435527; // Meta+G catalog toggle-float
inline constexpr int SHORTCUT_META_M = 268435533; // Meta+M catalog toggle-maximize
inline constexpr int SHORTCUT_META_LEFT = 285212690; // Meta+Left catalog focus-left-arrow
inline constexpr int SHORTCUT_META_DOWN = 285212693; // Meta+Down catalog focus-down-arrow
inline constexpr int SHORTCUT_META_UP = 285212691; // Meta+Up catalog focus-up-arrow
inline constexpr int SHORTCUT_META_RIGHT = 285212692; // Meta+Right catalog focus-right-arrow
inline constexpr int SHORTCUT_META_SHIFT_LEFT = 318767122; // Meta+Shift+Left catalog move-left-arrow
inline constexpr int SHORTCUT_META_SHIFT_RIGHT = 318767124; // Meta+Shift+Right catalog move-right-arrow
// Full project catalog chords (plan directional/toggle core plus workspace
// digits and shifted symbols). Combined Qt key ints: Meta 0x10000000,
// Shift 0x02000000, Alt 0x08000000 ORed with the Qt key code.
inline constexpr int SHORTCUT_META_H = 268435528; // Meta+H focus-left
inline constexpr int SHORTCUT_META_J = 268435530; // Meta+J focus-down
inline constexpr int SHORTCUT_META_K = 268435531; // Meta+K focus-up
inline constexpr int SHORTCUT_META_SHIFT_H = 301989960; // Meta+Shift+H move-left
inline constexpr int SHORTCUT_META_SHIFT_J = 301989962; // Meta+Shift+J move-down
inline constexpr int SHORTCUT_META_SHIFT_K = 301989963; // Meta+Shift+K move-up
inline constexpr int SHORTCUT_META_SHIFT_L = 301989964; // Meta+Shift+L move-right
inline constexpr int SHORTCUT_META_SHIFT_DOWN = 318767125; // Meta+Shift+Down move-down-arrow
inline constexpr int SHORTCUT_META_SHIFT_UP = 318767123; // Meta+Shift+Up move-up-arrow
inline constexpr int SHORTCUT_META_ALT_H = 402653256; // Meta+Alt+H resize-outwards-left
inline constexpr int SHORTCUT_META_ALT_J = 402653258; // Meta+Alt+J resize-outwards-down
inline constexpr int SHORTCUT_META_ALT_SHIFT_H = 436207688; // Meta+Alt+Shift+H resize-inwards-left
inline constexpr int SHORTCUT_META_ALT_SHIFT_J = 436207690; // Meta+Alt+Shift+J resize-inwards-down
inline constexpr int SHORTCUT_META_ALT_SHIFT_K = 436207691; // Meta+Alt+Shift+K resize-inwards-up
inline constexpr int SHORTCUT_META_ALT_SHIFT_L = 436207692; // Meta+Alt+Shift+L resize-inwards-right
inline constexpr int SHORTCUT_META_ALT_SHIFT_LEFT = 452984850; // Meta+Alt+Shift+Left resize-inwards-left-arrow
inline constexpr int SHORTCUT_META_ALT_SHIFT_DOWN = 452984853; // Meta+Alt+Shift+Down resize-inwards-down-arrow
inline constexpr int SHORTCUT_META_ALT_SHIFT_UP = 452984851; // Meta+Alt+Shift+Up resize-inwards-up-arrow
inline constexpr int SHORTCUT_META_ALT_SHIFT_RIGHT = 452984852; // Meta+Alt+Shift+Right resize-inwards-right-arrow
inline constexpr int SHORTCUT_META_SHIFT_G = 301989959; // Meta+Shift+G toggle-sticky
inline constexpr int SHORTCUT_META_F11 = 285212730; // Meta+F11 toggle-fullscreen
inline constexpr int SHORTCUT_META_O = 268435535; // Meta+O catalog toggle-orientation
inline constexpr int SHORTCUT_META_Y = 268435545; // Meta+Y catalog workspace-tiling toggle
inline constexpr int SHORTCUT_META_0 = 268435504; // Meta+0 workspace select trailing
inline constexpr int SHORTCUT_META_1 = 268435505; // Meta+1 workspace select 1
inline constexpr int SHORTCUT_META_2 = 268435506;
inline constexpr int SHORTCUT_META_3 = 268435507;
inline constexpr int SHORTCUT_META_4 = 268435508;
inline constexpr int SHORTCUT_META_5 = 268435509;
inline constexpr int SHORTCUT_META_6 = 268435510;
inline constexpr int SHORTCUT_META_7 = 268435511;
inline constexpr int SHORTCUT_META_8 = 268435512;
inline constexpr int SHORTCUT_META_9 = 268435513; // Meta+9 workspace select 9
inline constexpr int SHORTCUT_META_SHIFT_0 = 301989936; // Meta+Shift+0 move to trailing
inline constexpr int SHORTCUT_META_SHIFT_1 = 301989937; // Meta+Shift+1 move to workspace 1
inline constexpr int SHORTCUT_META_SHIFT_2 = 301989938;
inline constexpr int SHORTCUT_META_SHIFT_3 = 301989939;
inline constexpr int SHORTCUT_META_SHIFT_4 = 301989940;
inline constexpr int SHORTCUT_META_SHIFT_5 = 301989941;
inline constexpr int SHORTCUT_META_SHIFT_6 = 301989942;
inline constexpr int SHORTCUT_META_SHIFT_7 = 301989943;
inline constexpr int SHORTCUT_META_SHIFT_8 = 301989944;
inline constexpr int SHORTCUT_META_SHIFT_9 = 301989945; // Meta+Shift+9 move to workspace 9
inline constexpr int SHORTCUT_META_EXCLAM = 268435489; // Meta+! move-to-1 symbol alias
inline constexpr int SHORTCUT_META_AT = 268435520; // Meta+@ move-to-2 symbol alias
inline constexpr int SHORTCUT_META_NUMBERSIGN = 268435491; // Meta+# move-to-3 symbol alias
inline constexpr int SHORTCUT_META_DOLLAR = 268435492; // Meta+$ move-to-4 symbol alias
inline constexpr int SHORTCUT_META_PERCENT = 268435493; // Meta+% move-to-5 symbol alias
inline constexpr int SHORTCUT_META_ASCIICIRCUM = 268435550; // Meta+^ move-to-6 symbol alias
inline constexpr int SHORTCUT_META_AMPERSAND = 268435494; // Meta+& move-to-7 symbol alias
inline constexpr int SHORTCUT_META_ASTERISK = 268435498; // Meta+* move-to-8 symbol alias
inline constexpr int SHORTCUT_META_PARENLEFT = 268435496; // Meta+( move-to-9 symbol alias
inline constexpr int SHORTCUT_META_PARENRIGHT = 268435497; // Meta+) move-to-trailing symbol alias
inline constexpr int SHORTCUT_META_CTRL_TAB = 352321537; // Meta+Ctrl+Tab workspace previous toggle
inline constexpr int SHORTCUT_META_CTRL_H = 335544392; // Meta+Ctrl+H workspace prev-h
inline constexpr int SHORTCUT_META_CTRL_K = 335544395; // Meta+Ctrl+K workspace prev-k
inline constexpr int SHORTCUT_META_CTRL_LEFT = 352321554; // Meta+Ctrl+Left workspace prev-left-arrow
inline constexpr int SHORTCUT_META_CTRL_UP = 352321555; // Meta+Ctrl+Up workspace prev-up-arrow
inline constexpr int SHORTCUT_META_CTRL_J = 335544394; // Meta+Ctrl+J workspace next-j
inline constexpr int SHORTCUT_META_CTRL_L = 335544396; // Meta+Ctrl+L workspace next-l
inline constexpr int SHORTCUT_META_CTRL_DOWN = 352321557; // Meta+Ctrl+Down workspace next-down-arrow
inline constexpr int SHORTCUT_META_CTRL_RIGHT = 352321556; // Meta+Ctrl+Right workspace next-right-arrow
inline constexpr int SHORTCUT_META_CTRL_SHIFT_H = 369098824; // Meta+Ctrl+Shift+H send prev-h
inline constexpr int SHORTCUT_META_CTRL_SHIFT_K = 369098827; // Meta+Ctrl+Shift+K send prev-k
inline constexpr int SHORTCUT_META_CTRL_SHIFT_J = 369098826; // Meta+Ctrl+Shift+J send next-j
inline constexpr int SHORTCUT_META_CTRL_SHIFT_L = 369098828; // Meta+Ctrl+Shift+L send next-l
inline constexpr int SHORTCUT_META_CTRL_SHIFT_LEFT = 385875986; // Meta+Ctrl+Shift+Left send prev-left-arrow
inline constexpr int SHORTCUT_META_CTRL_SHIFT_UP = 385875987; // Meta+Ctrl+Shift+Up send prev-up-arrow
inline constexpr int SHORTCUT_META_CTRL_SHIFT_RIGHT = 385875988; // Meta+Ctrl+Shift+Right send next-right-arrow
inline constexpr int SHORTCUT_META_CTRL_SHIFT_DOWN = 385875989; // Meta+Ctrl+Shift+Down send next-down-arrow
inline constexpr int SHORTCUT_META_CTRL_ALT_H = 469762120; // Meta+Ctrl+Alt+H send-output left follow
inline constexpr int SHORTCUT_META_CTRL_ALT_J = 469762122; // Meta+Ctrl+Alt+J send-output down follow
inline constexpr int SHORTCUT_META_CTRL_ALT_K = 469762123; // Meta+Ctrl+Alt+K send-output up follow
inline constexpr int SHORTCUT_META_CTRL_ALT_L = 469762124; // Meta+Ctrl+Alt+L send-output right follow
inline constexpr int SHORTCUT_META_CTRL_ALT_LEFT = 486539282; // Meta+Ctrl+Alt+Left send-output left-arrow follow
inline constexpr int SHORTCUT_META_CTRL_ALT_UP = 486539283; // Meta+Ctrl+Alt+Up send-output up-arrow follow
inline constexpr int SHORTCUT_META_CTRL_ALT_RIGHT = 486539284; // Meta+Ctrl+Alt+Right send-output right-arrow follow
inline constexpr int SHORTCUT_META_CTRL_ALT_DOWN = 486539285; // Meta+Ctrl+Alt+Down send-output down-arrow follow
inline constexpr uint SHORTCUT_SET_FLAGS = 6; // SetPresent|NoAutoloading
inline constexpr int SHORTCUT_MAX_KEYS_PER_TUPLE = 16;
inline constexpr int SHORTCUT_MAX_TUPLES = 16384;
inline constexpr int SHORTCUT_MAX_STRING_LEN = 256;
inline constexpr int SHORTCUT_MAX_KEY_VALUE = 536870911;

inline const QString &shortcutService()
{
    static const QString value = QStringLiteral("org.kde.kglobalaccel");
    return value;
}
inline const QString &shortcutPath()
{
    static const QString value = QStringLiteral("/kglobalaccel");
    return value;
}
inline const QString &shortcutInterface()
{
    static const QString value = QStringLiteral("org.kde.KGlobalAccel");
    return value;
}
inline const QString &shortcutComponentInterface()
{
    static const QString value = QStringLiteral("org.kde.kglobalaccel.Component");
    return value;
}
inline const QString &shortcutSetMethod()
{
    static const QString value = QStringLiteral("setShortcutKeys");
    return value;
}
inline const QString &shortcutAllComponentsMethod()
{
    static const QString value = QStringLiteral("allComponents");
    return value;
}
inline const QString &shortcutAllInfosMethod()
{
    static const QString value = QStringLiteral("allShortcutInfos");
    return value;
}
inline const QString &shortcutByKeyMethod()
{
    static const QString value = QStringLiteral("globalShortcutsByKey");
    return value;
}
inline const QString &shortcutAvailableMethod()
{
    static const QString value = QStringLiteral("globalShortcutAvailable");
    return value;
}
inline const QString &shortcutDefaultKeysMethod()
{
    static const QString value = QStringLiteral("defaultShortcutKeys");
    return value;
}
inline const QString &shortcutSetForeignKeysMethod()
{
    static const QString value = QStringLiteral("setForeignShortcutKeys");
    return value;
}
inline const QString &shortcutActiveKeysMethod()
{
    static const QString value = QStringLiteral("shortcutKeys");
    return value;
}
// User-authorized explicit displacement (auditable): System Monitor `_launch`
// may hold Meta+Esc; Apply may displace it onto Lock Session without
// rebinding System Monitor itself. No other foreign occupier is authorized.
// Do not select a different target and do not write to System Monitor.
inline const QString &shortcutAuthorizedEscComponent()
{
    static const QString value = QStringLiteral("org.kde.plasma-systemmonitor.desktop");
    return value;
}
inline const QString &shortcutAuthorizedEscAction()
{
    static const QString value = QStringLiteral("_launch");
    return value;
}
inline constexpr int SHORTCUT_MATCH_EQUAL = 0;
inline const QString &shortcutFocusComponent()
{
    static const QString value = QStringLiteral("kwin");
    return value;
}
inline const QString &shortcutFocusAction()
{
    static const QString value = QStringLiteral("omnitiler-focus-right");
    return value;
}
inline const QString &shortcutLockComponent()
{
    static const QString value = QStringLiteral("ksmserver");
    return value;
}
inline const QString &shortcutLockAction()
{
    static const QString value = QStringLiteral("Lock Session");
    return value;
}
inline const QString &shortcutResizeUpComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutResizeUpAction() { static const QString v = QStringLiteral("omnitiler-resize-outwards-up"); return v; }
inline const QString &shortcutResizeRightComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutResizeRightAction() { static const QString v = QStringLiteral("omnitiler-resize-outwards-right"); return v; }
inline const QString &shortcutSwitchNextComponent() { static const QString v = QStringLiteral("KDE Keyboard Layout Switcher"); return v; }
inline const QString &shortcutSwitchNextAction() { static const QString v = QStringLiteral("Switch to Next Keyboard Layout"); return v; }
inline const QString &shortcutSwitchLastComponent() { static const QString v = QStringLiteral("KDE Keyboard Layout Switcher"); return v; }
inline const QString &shortcutSwitchLastAction() { static const QString v = QStringLiteral("Switch to Last-Used Keyboard Layout"); return v; }
inline const QString &shortcutResizeLeftArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutResizeLeftArrowAction() { static const QString v = QStringLiteral("omnitiler-resize-outwards-left-arrow"); return v; }
inline const QString &shortcutSwitchLeftComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutSwitchLeftAction() { static const QString v = QStringLiteral("Switch Window Left"); return v; }
inline const QString &shortcutResizeDownArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutResizeDownArrowAction() { static const QString v = QStringLiteral("omnitiler-resize-outwards-down-arrow"); return v; }
inline const QString &shortcutSwitchDownComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutSwitchDownAction() { static const QString v = QStringLiteral("Switch Window Down"); return v; }
inline const QString &shortcutResizeUpArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutResizeUpArrowAction() { static const QString v = QStringLiteral("omnitiler-resize-outwards-up-arrow"); return v; }
inline const QString &shortcutSwitchUpComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutSwitchUpAction() { static const QString v = QStringLiteral("Switch Window Up"); return v; }
inline const QString &shortcutResizeRightArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutResizeRightArrowAction() { static const QString v = QStringLiteral("omnitiler-resize-outwards-right-arrow"); return v; }
inline const QString &shortcutSwitchRightComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutSwitchRightAction() { static const QString v = QStringLiteral("Switch Window Right"); return v; }
inline const QString &shortcutFloatComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutFloatAction() { static const QString v = QStringLiteral("omnitiler-toggle-float"); return v; }
inline const QString &shortcutGridViewComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutGridViewAction() { static const QString v = QStringLiteral("Grid View"); return v; }
inline const QString &shortcutMaximizeComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutMaximizeAction() { static const QString v = QStringLiteral("omnitiler-toggle-maximize"); return v; }
inline const QString &shortcutMonocleComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutMonocleAction() { static const QString v = QStringLiteral("KrohnkiteMonocleLayout"); return v; }
inline const QString &shortcutFocusLeftArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutFocusLeftArrowAction() { static const QString v = QStringLiteral("omnitiler-focus-left-arrow"); return v; }
inline const QString &shortcutQuickTileLeftComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutQuickTileLeftAction() { static const QString v = QStringLiteral("Window Quick Tile Left"); return v; }
inline const QString &shortcutFocusDownArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutFocusDownArrowAction() { static const QString v = QStringLiteral("omnitiler-focus-down-arrow"); return v; }
inline const QString &shortcutQuickTileBottomComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutQuickTileBottomAction() { static const QString v = QStringLiteral("Window Quick Tile Bottom"); return v; }
inline const QString &shortcutFocusUpArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutFocusUpArrowAction() { static const QString v = QStringLiteral("omnitiler-focus-up-arrow"); return v; }
inline const QString &shortcutQuickTileTopComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutQuickTileTopAction() { static const QString v = QStringLiteral("Window Quick Tile Top"); return v; }
inline const QString &shortcutFocusRightArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutFocusRightArrowAction() { static const QString v = QStringLiteral("omnitiler-focus-right-arrow"); return v; }
inline const QString &shortcutQuickTileRightComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutQuickTileRightAction() { static const QString v = QStringLiteral("Window Quick Tile Right"); return v; }
inline const QString &shortcutMoveLeftArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutMoveLeftArrowAction() { static const QString v = QStringLiteral("omnitiler-move-left-arrow"); return v; }
inline const QString &shortcutToPrevScreenComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutToPrevScreenAction() { static const QString v = QStringLiteral("Window to Previous Screen"); return v; }
inline const QString &shortcutMoveRightArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutMoveRightArrowAction() { static const QString v = QStringLiteral("omnitiler-move-right-arrow"); return v; }
inline const QString &shortcutToNextScreenComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutToNextScreenAction() { static const QString v = QStringLiteral("Window to Next Screen"); return v; }
inline const QString &shortcutWorkspacePrevLeftArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutWorkspacePrevLeftArrowAction() { static const QString v = QStringLiteral("omnitiler-workspace-prev-left-arrow"); return v; }
inline const QString &shortcutSwitchOneDesktopLeftComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutSwitchOneDesktopLeftAction() { static const QString v = QStringLiteral("Switch One Desktop to the Left"); return v; }
inline const QString &shortcutWorkspacePrevUpArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutWorkspacePrevUpArrowAction() { static const QString v = QStringLiteral("omnitiler-workspace-prev-up-arrow"); return v; }
inline const QString &shortcutSwitchOneDesktopUpComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutSwitchOneDesktopUpAction() { static const QString v = QStringLiteral("Switch One Desktop Up"); return v; }
inline const QString &shortcutWorkspaceNextDownArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutWorkspaceNextDownArrowAction() { static const QString v = QStringLiteral("omnitiler-workspace-next-down-arrow"); return v; }
inline const QString &shortcutSwitchOneDesktopDownComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutSwitchOneDesktopDownAction() { static const QString v = QStringLiteral("Switch One Desktop Down"); return v; }
inline const QString &shortcutWorkspaceNextRightArrowComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutWorkspaceNextRightArrowAction() { static const QString v = QStringLiteral("omnitiler-workspace-next-right-arrow"); return v; }
inline const QString &shortcutSwitchOneDesktopRightComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutSwitchOneDesktopRightAction() { static const QString v = QStringLiteral("Switch One Desktop to the Right"); return v; }
inline const QString &shortcutWindowOneDesktopLeftComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutWindowOneDesktopLeftAction() { static const QString v = QStringLiteral("Window One Desktop to the Left"); return v; }
inline const QString &shortcutWindowOneDesktopUpComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutWindowOneDesktopUpAction() { static const QString v = QStringLiteral("Window One Desktop Up"); return v; }
inline const QString &shortcutWindowOneDesktopDownComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutWindowOneDesktopDownAction() { static const QString v = QStringLiteral("Window One Desktop Down"); return v; }
inline const QString &shortcutWindowOneDesktopRightComponent() { static const QString v = QStringLiteral("kwin"); return v; }
inline const QString &shortcutWindowOneDesktopRightAction() { static const QString v = QStringLiteral("Window One Desktop to the Right"); return v; }
inline const QString &shortcutResolutionRelocate() { static const QString v = QStringLiteral("relocate"); return v; }
inline const QString &shortcutResolutionClear() { static const QString v = QStringLiteral("clear"); return v; }

struct ShortcutTuple
{
    QString component;
    QString action;
    QString componentFriendly;
    QString friendly;
    QList<int> active;
    // Authoritative defaults carried on the wire (ssssss + ai + ai) but
    // previously dropped here. Kept so defaults stay visible even after
    // Force clears the active list; empty when the holder reports none.
    QList<int> defaults;
};

struct ShortcutConflictRow
{
    QString projectComponent;
    QString projectAction;
    QList<int> projectPost;
    QString foreignComponent;
    QString foreignAction;
    QList<int> foreignExpectedPre;
    QString resolution;
    QList<int> resolutionTarget;
    // Exact foreign holder permitted on this row's resolution target. It is
    // never writable and any other holder remains a conflict.
    QString authorizedTargetComponent;
    QString authorizedTargetAction;
    QString projectDisplay;
    QString targetDisplay;
    QString projectDiag;
};

const QList<ShortcutConflictRow> &shortcutConflictTable();
// Full project catalog: 37 directional/toggle rows from planShortcutCatalog
// (focus/move letters plus arrows, outwards/inwards resize letters plus
// arrows, five toggles) followed by 39 item-1 workspace rows from
// workspaceShortcutCatalog (Meta+0..9 select, Meta+Shift+0..9 move,
// shifted-symbol move aliases, Meta+Ctrl+Tab previous toggle plus
// Meta+Ctrl+H/J/K/L and Meta+Ctrl+arrows relative steps), followed by 36
// item-2 workspace rows (8 Meta+Ctrl+Shift+H/J/K/L and arrows relative
// send-and-follow, 20 unbound numbered send-and-stay with symbol aliases
// plus append rows, 8 unbound relative send-and-stay), followed by 12
// item-5 output rows from planOutputSendShortcutCatalog (8 Meta+Ctrl+Alt+H/J/K/L
// and arrows send-and-follow, 4 unbound directional send-and-stay),
// followed by 4 R-WS-12 migrate rows from
// planWorkspaceMigrateShortcutCatalog (unbound directional follow),
// followed by 1 workspace-tiling toggle row (Meta+Y bound by user decision
// 2026-10-10; no known stock KDE holder, so no conflict-table row).
// Deterministic order, unique action IDs (unbound rows share canonical key 0).
// kind is one of focus, move, resize, toggle, workspace-select,
// workspace-move, workspace-previous, workspace-relative,
// workspace-send-relative, workspace-move-stay, workspace-send-relative-stay,
// output-send, output-send-stay, workspace-migrate.
// knownForeign* is the compiled KDE conflict for the 23 conflict-table rows
// only; empty (key 0) means no known conflict and the row must report
// honestly instead of inventing one. Unbound rows carry canonical key 0
// and empty display; they stay rebindable through the existing KCM route
// and are skipped by required-key scans.
struct ShortcutCatalogEntry
{
    QString component;
    QString action;
    int canonicalKey = 0;
    QString canonicalDisplay;
    QString knownForeignComponent;
    QString knownForeignAction;
    int knownForeignKey = 0;
    QString kind;
};

const QList<ShortcutCatalogEntry> &shortcutProjectCatalog();
// "component/action" identity string for staged Keep/Disable drafts.
QString shortcutCatalogId(const QString &component, const QString &action);
bool shortcutCatalogContains(const QString &component, const QString &action);
// Canonical key for a catalog project action; *found false when absent.
int shortcutCatalogKeyFor(const QString &component, const QString &action, bool *found = nullptr);
// The 19 conflict-table project IDs: the known-conflicting canonical rows
// Compatible always disables.
QSet<QString> shortcutKnownConflictIds();
// Pure preset seam: union of known-conflict IDs and live-colliding IDs in
// catalog order. The caller supplies both sets from live queries.
QStringList presetCompatibleDisabledIds(const QList<ShortcutCatalogEntry> &catalog,
                                        const QSet<QString> &knownConflictIds,
                                        const QSet<QString> &collidingIds);
// Max project writes derived from the catalog: one write per catalog row
// plus the Lock Session relocation.
inline int shortcutMaxProjectWrites()
{
    return static_cast<int>(shortcutProjectCatalog().size()) + 1;
}
// Operation-scoped transport budgets (never a lifetime accumulation, so
// repeated legitimate Apply/Force/Revert stay available while each single
// operation stays bounded):
// - foreign clears in one Force: one per distinct cleared holder row,
//   bounded by the holder enumeration bound;
// - total writes in one Force: every cleared holder plus the full project
//   assignment above;
// - total writes in one Revert: one per persisted cleared ID (project-owned
//   IDs stay cleared and write nothing).
// The reconciler refuses before the write that would overflow the bound, so
// no operation ever exceeds it. writeCount() stays a monotonic total used
// only for per-operation deltas.
inline int shortcutMaxForeignClears()
{
    return SHORTCUT_MAX_TUPLES;
}
inline int shortcutMaxForceWrites(int foreignClears)
{
    return foreignClears + shortcutMaxProjectWrites();
}
inline int shortcutMaxRevertWrites(int clearedIds)
{
    return clearedIds;
}

// Typed keyed-occupancy outcome (no substring classification):
// Clear means no foreign occupancy, Conflict means an unexpected holder
// claims a relevant key, Unavailable means transport/parse/consistency
// failure. globalShortcutAvailable(key, "") is whole-key availability.
enum class KeyedOccupancy
{
    Clear,
    Conflict,
    Unavailable
};

struct KeyedOccupancyResult
{
    KeyedOccupancy status = KeyedOccupancy::Clear;
    QString detail;
};

struct ShortcutKeyHolder
{
    QString component;
    QString action;
    // Authoritative primitive sees defaults: a .desktop-declared-only
    // holder may have empty active with defaults containing the key.
    QList<int> active;
    QList<int> defaults;
    // Display labels carried on the wire (ssssss). Empty when the holder
    // reports none; never part of identity, never persisted.
    QString componentFriendly;
    QString friendly;
};

// Plain KGlobalShortcutInfo fields (ssssss + ai + ai) shared by the
// allShortcutInfos and globalShortcutsByKey reply parsers. The QDBusArgument
// versions only extract this list from the wire struct array, then delegate
// all field/bound mapping here, so hermetic tests cover the real field
// branches without a live bus and without duplicating parsing.
struct ShortcutInfoFields
{
    QString action;
    QString friendly;
    QString compUnique;
    QString compFriendly;
    QString contextUnique;
    QString contextFriendly;
    QList<int> active;
    QList<int> defaults;
};

// Exact (ssssssaiai) struct framing for KGlobalShortcutInfo: 6 strings plus
// active/default int arrays. In KWin namespace so ADL finds them from
// qDBusRegisterMetaType helpers; tests encode a write-mode array through them
// and assert the marshalled signature is exactly a(ssssssaiai).
QDBusArgument &operator<<(QDBusArgument &argument, const ShortcutInfoFields &info);
const QDBusArgument &operator>>(const QDBusArgument &argument, ShortcutInfoFields &info);

// QKeySequence D-Bus framing is (ai); MatchType is (i): a struct holding one
// int (0 == Equal). Registered for the keyed globalShortcutsByKey call.
struct ShortcutMatchType
{
    int value = SHORTCUT_MATCH_EQUAL;
};

struct ShortcutApplyResult
{
    bool ok = false;
    QString error;
    int writes = 0;
    // True only for the zero-write preflight conflict refusal (a holder
    // claims a required chord). Operational and partial-apply failures
    // leave this false so callers never offer Force for them.
    bool conflictRefused = false;
};

// Explicit confirmed override for holders of project-required chords.
// Produced by previewForceApply and consumed by applyForced; never
// constructed from arbitrary UI input. One row per cleared holder:
//   actual is the full live active list observed for the holder,
//   expectedPre carries precisely the conflicting required subset being
//     removed (actual intersected with the project-required chords),
//   post carries the remainder kept on the holder (actual minus the
//     conflicting subset, order-preserving; possibly empty).
// Revert does not consume preimages: it restores defaults.
struct ShortcutForceMismatch
{
    QString component;
    QString action;
    QList<int> expectedPre;
    QList<int> actual;
    QList<int> post;
    // Cosmetic actionId parts observed for the holder (empty allowed, e.g.
    // for .desktop-only holders). Transient preview labels only: never
    // persisted to the cleared-ID list; never part of identity.
    QString componentFriendly;
    QString friendly;
};

struct ShortcutForceBlocker
{
    QString component;
    QString action;
    int key = 0;
    // Full live active list observed for the holder. The required chord is
    // claimed without appearing here, so Force removes nothing and retains
    // all of these keys.
    QList<int> active;
    QString componentFriendly;
    QString friendly;
};

// Actual in-scope Lock Session relocation carried by the preview, so the
// dialog describes the exemption honestly without inferring policy.
struct ShortcutLockRelocation
{
    bool inScope = false;
    QString component;
    QString action;
    QList<int> actual;
    QList<int> post;
    bool hasPre = false;
    bool hasTarget = false;
    // Authorized displacement observed on the relocation target.
    bool authorizedHolderPresent = false;
    QString authorizedComponent;
    QString authorizedAction;
    QList<int> authorizedActive;
    QString authorizedComponentFriendly;
    QString authorizedFriendly;
};

// One reconciler-owned combined conflict entry: blockers first, then clear
// rows, each subsequence in component/action/key order. Status, refusal,
// dialog, and logs all share this order; the UI never re-sorts.
struct ShortcutOrderedConflict
{
    bool isBlocker = false;
    int index = -1;
};

struct ShortcutForcePreview
{
    bool forceable = false;
    QString error;
    QList<ShortcutForceMismatch> mismatches;
    // Default-only claimants with nothing Force can clear, in
    // component/action/key order. Populated even when forceable is false
    // so the conflict dialog can list every available conflict with Force
    // disabled and an explicit reason.
    QList<ShortcutForceBlocker> blockers;
    ShortcutLockRelocation lock;
    // Full bounded preflight image. Force accepts only this exact live image,
    // not merely the rows that initially needed clearing.
    QString owner;
    uint uid = 0;
    // Full bounded preflight image in catalog order (one active list per
    // catalog entry, empty list when the entry is missing live), followed
    // by the Lock Session image (empty when focus-right is disabled and the
    // lock is out of scope). Any drift fails the confirmation as stale.
    // livePresent binds presence per image (missing vs empty share the same
    // active image, so presence is compared too; last element is the lock
    // in-scope flag).
    QList<QList<int>> liveImages;
    QList<bool> livePresent;
    // Exact staged Keep/Disable draft this preview was built for, sorted.
    // Force applies only against the same draft; draft edits cancel the
    // pending preview and a mismatched draft fails as stale.
    QStringList disabledIds;
    // Exact staged Authentic intent this preview was built for. Force
    // applies only when the caller stages the same intent; a mismatch
    // fails as stale with zero writes.
    bool authenticStaged = false;
};

struct ShortcutForceApplyResult
{
    bool ok = false;
    QString error;
    int writes = 0;
};

// Bounded structured diagnostics through QLoggingCategory
// "omnitiler.shortcut" (visible in the kcmshell6/System Settings
// journald log). Only safe fields are ever logged: operation, stage,
// outcome, allowlisted component/action identity, and key images. Logging
// never affects behavior: the sink is void, exceptions are swallowed, and
// no caller branches on logging.
using ShortcutLogSink = std::function<void(QtMsgType, const QString &)>;
class ShortcutDiag
{
public:
    static void setSink(ShortcutLogSink sink);
    static void resetSink();
    static void log(QtMsgType type, const char *operation, const char *stage, const char *outcome,
                    const QString &detail);
};

struct ShortcutRevertResult
{
    bool ok = false;
    QString error;
    int writes = 0;
};

// One full-catalog row for the staged conflict list: canonical chord,
// current live assignment, known KDE default, current holders, and live
// foreign-default collisions from readAll tuple defaults.
// holdersKnown/defaultsKnown false means the keyed query failed and the
// row must report unavailable honestly instead of claiming clarity.
// foreignDefaultIds lists non-project foreign tuple IDs whose wire defaults
// contain the canonical key (own project defaults are never listed);
// foreignDefaultsKnown false means readAll failed and defaults are unknown.
struct ShortcutRowDisplay
{
    ShortcutCatalogEntry catalog;
    bool present = false;
    QList<int> current;
    QList<int> projectDefaults;
    QList<ShortcutKeyHolder> holders;
    bool holdersKnown = false;
    // Live foreign defaults for compiled known-conflict rows (empty plus
    // true means no known default to show; false means the defaults query
    // failed and the row must report unavailable).
    QList<int> knownDefaults;
    bool defaultsKnown = false;
    QStringList foreignDefaultIds;
    bool foreignDefaultsKnown = false;
};

class ShortcutStore
{
public:
    virtual ~ShortcutStore() = default;
    virtual bool checkSetterContract(QString *error) = 0;
    virtual bool currentOwner(QString *owner, uint *uid, QString *error) = 0;
    virtual bool readAll(QList<ShortcutTuple> *tuples, QString *error) = 0;
    // Authoritative keyed primitives (Defect B): globalShortcutsByKey and
    // globalShortcutAvailable. Foreign conflict detection must use these,
    // never tuple/config enumeration, so .desktop-only holders are visible.
    virtual bool shortcutsByKey(int key, QList<ShortcutKeyHolder> *holders, QString *error) = 0;
    virtual bool shortcutAvailable(int key, const QString &component, bool *available, QString *error) = 0;
    virtual bool writeKeys(const QString &component, const QString &action, const QString &componentFriendly,
                           const QString &friendly, const QList<int> &keys, QList<int> *confirmed, QString *error) = 0;
    // Bounded native TRANSPORT seam: exact KGlobalAccel
    // defaultShortcutKeys(as)->a(ai) read and setForeignShortcutKeys
    // (as,a(ai))->void write against the pinned org.kde.KGlobalAccel owner.
    // Policy/UI paths use writeKeys for project rows; these methods
    // establish the injectable seam for foreign defaults/clears.
    virtual bool defaultShortcutKeys(const QString &component, const QString &action,
                                     const QString &componentFriendly, const QString &friendly,
                                     QList<int> *defaults, QString *error) = 0;
    virtual bool setForeignShortcutKeys(const QString &component, const QString &action,
                                        const QString &componentFriendly, const QString &friendly,
                                        const QList<int> &keys, QString *error) = 0;
    virtual int writeCount() const = 0;
};

// Real D-Bus backend using QDBus with the exact observed contract.
class KGlobalAccelStore : public ShortcutStore
{
public:
    bool checkSetterContract(QString *error) override;
    bool currentOwner(QString *owner, uint *uid, QString *error) override;
    bool readAll(QList<ShortcutTuple> *tuples, QString *error) override;
    bool shortcutsByKey(int key, QList<ShortcutKeyHolder> *holders, QString *error) override;
    bool shortcutAvailable(int key, const QString &component, bool *available, QString *error) override;
    bool writeKeys(const QString &component, const QString &action, const QString &componentFriendly,
                   const QString &friendly, const QList<int> &keys, QList<int> *confirmed, QString *error) override;
    bool defaultShortcutKeys(const QString &component, const QString &action, const QString &componentFriendly,
                             const QString &friendly, QList<int> *defaults, QString *error) override;
    bool setForeignShortcutKeys(const QString &component, const QString &action, const QString &componentFriendly,
                                const QString &friendly, const QList<int> &keys, QString *error) override;
    int writeCount() const override
    {
        return m_writes;
    }
    // Pure pin transition tested without D-Bus: first verified owner sets
    // the pin, the same owner confirms it, a different owner fails closed.
    static bool tryPinOwner(QString &pinned, const QString &candidate, QString *error);
    // Pure daemon owner resolution tested without D-Bus: validates the
    // serviceOwner/serviceUid values, pins on first success, and fails
    // closed on absent service or drift. Live currentOwner() only fetches
    // the values via QDBusConnection::interface() and delegates here.
    static bool resolveOwnerReply(bool ownerValid, const QString &ownerValue, bool uidValid, uint uidValue,
                                  QString &pinned, QString *ownerOut, uint *uidOut, QString *error);
    // Pure pinned-owner drift check for the write path without D-Bus:
    // invalid live owner or any mismatch fails closed as drift.
    static bool checkPinnedDrift(bool liveValid, const QString &liveValue, const QString &pinned, QString *error);

private:
    int m_writes = 0;
    QString m_pinnedOwner;
};

// Minimal durable Force record: ONLY the component/action IDs Force
// cleared, in the project-owned config. Union-persisted BEFORE Force
// clearing so an interrupted Force stays revertible; emptied only after
// Revert restores every non-project entry. No cosmetic labels are
// persisted: Revert resolves each ID to its fresh current tuple from
// readAll to supply the current friendly labels for the 4-field actionId
// (KGlobalAccel 4-field semantics proven; 2-field daemon behavior
// unproven, so never assumed). Identity is component/action.
struct ClearedAction
{
    QString component;
    QString action;
};

inline bool operator==(const ClearedAction &a, const ClearedAction &b)
{
    return a.component == b.component && a.action == b.action;
}

inline bool clearedActionSameId(const ClearedAction &a, const ClearedAction &b)
{
    return a.component == b.component && a.action == b.action;
}

class ClearedActionsStore
{
public:
    virtual ~ClearedActionsStore() = default;
    // Absent config loads as an empty list (no error). Bounded: at most
    // SHORTCUT_MAX_TUPLES entries, strict component/action identity only.
    // Only Components+Actions are stored; legacy friendly keys are ignored.
    virtual bool load(QList<ClearedAction> *actions, QString *error) = 0;
    // Whole-list replace with write+sync+readback.
    virtual bool save(const QList<ClearedAction> &actions, QString *error) = 0;
    // Empty the list after a complete Revert.
    virtual bool clear(QString *error) = 0;
};

// Real KConfig backend rooted at an explicit project-owned file.
class KConfigClearedActions : public ClearedActionsStore
{
public:
    explicit KConfigClearedActions(const QString &filePath);
    bool load(QList<ClearedAction> *actions, QString *error) override;
    bool save(const QList<ClearedAction> &actions, QString *error) override;
    bool clear(QString *error) override;

private:
    QString m_filePath;
};

class ShortcutReconciler
{
public:
    explicit ShortcutReconciler(ShortcutStore *store, ClearedActionsStore *cleared = nullptr);
    ShortcutApplyResult apply();
    // Selection-aware Apply over an explicit staged draft. The single-arg
    // form stages Authentic for every enabled row (assigns canonical
    // chords, relocates Lock Session when focus-right is enabled:
    // historical behavior). The two-arg form adds the staged Authentic
    // intent: false preserves each enabled (Keep) row at its live
    // assignment (custom chords and empty assignments included) and leaves
    // Lock Session untouched. Disabled rows present live with a non-empty
    // active list are cleared to empty in both forms through the existing
    // own-action transport; they never scan, refuse, clear foreign holders,
    // or relocate the lock.
    ShortcutApplyResult applySelected(const QSet<QString> &disabledIds);
    ShortcutApplyResult applySelected(const QSet<QString> &disabledIds, bool authenticStaged);
    // Revert restores defaults for every non-project ID in the durable
    // cleared list (project-owned kwin/omnitiler-* IDs, including
    // legacy ones, stay cleared) and empties the list only after all of
    // them are restored. Each persisted ID is resolved to its fresh
    // current tuple from readAll to supply the current friendly labels
    // (empty allowed) for the 4-field actionId; absent or duplicate IDs
    // fail closed without writing unrelated actions and retain the list
    // for retry. Empty list is a no-op success.
    ShortcutRevertResult revert();
    // Read-only force preview: every actual holder of a project-required
    // chord (known, unknown, and legacy project IDs alike), except the
    // project actions themselves, Lock Session, and the authorized System
    // Monitor Meta+Esc holder. Never forceable when any store, ownership,
    // transport, or parsing check fails.
    ShortcutForcePreview previewForceApply();
    // Selection-aware Force preview bound to the exact draft and intent.
    // The single-arg form stages Authentic for every enabled row
    // (historical behavior); the two-arg form binds the staged intent.
    ShortcutForcePreview previewForceApplySelected(const QSet<QString> &disabledIds);
    ShortcutForcePreview previewForceApplySelected(const QSet<QString> &disabledIds, bool authenticStaged);
    // Confirmed force: revalidates the preview snapshot against a fresh live
    // read (stale snapshots fail closed with zero writes, including zero
    // cleared-list writes when stale before persist), persists the union of
    // cleared component/action IDs (no cosmetic labels) to the project
    // config BEFORE clearing, re-reads each
    // holder immediately before its foreign setter and aborts when its
    // active list changed (zero further KGlobalAccel writes), clears only
    // the conflicting keys from each holder (unrelated keys preserved),
    // then assigns the project keys and relocates Lock Session. Drift after
    // persist retains the persisted union (a superset when the drifted
    // holder was never cleared); a later Revert may therefore restore
    // defaults for an action Force never cleared.
    ShortcutForceApplyResult applyForced(const ShortcutForcePreview &confirmed);
    // Selection-aware confirmed Force: revalidates the preview snapshot and
    // the draft against fresh live state before any write. A draft or
    // Authentic-intent mismatch fails as stale with zero writes.
    ShortcutForceApplyResult applyForcedSelected(const ShortcutForcePreview &confirmed,
                                                 const QSet<QString> &disabledIds);
    ShortcutForceApplyResult applyForcedSelected(const ShortcutForcePreview &confirmed,
                                                 const QSet<QString> &disabledIds,
                                                 bool authenticStaged);

    static bool isAllowlisted(const QString &component, const QString &action);
    // Current project-owned action: any full-catalog project row. Never
    // cleared by Force, never restored by Revert.
    static bool isProjectAction(const QString &component, const QString &action);
    // Any own-prefix action: kwin/omnitiler-*, covering the current
    // project rows and legacy IDs (e.g. omnitiler-float-toggle,
    // omnitiler-toggle), plus the previous OmniTiler identity
    // kwin/plasma-auto-tiler-* (pre-rename catalog). Revert-only skip:
    // Revert leaves them cleared. Force still lists them as conflicts
    // via isProjectAction/isHolderExempt (never silently exempt).
    static bool isProjectOwned(const QString &component, const QString &action);
    // Previous OmniTiler identity: kwin/plasma-auto-tiler-* (the entire
    // pre-rename catalog). Reporting-only distinction; Revert skips it
    // via isProjectOwned, Force still clears it on confirmation.
    static bool isHistoricalProjectOwned(const QString &component, const QString &action);
    static bool isLockAction(const QString &component, const QString &action);
    // Required-chord subset of a live active list (the keys Force removes),
    // order-preserving. Empty means the holder claims no required chord.
    static QList<int> conflictingKeys(const QList<int> &active);
    // Remainder kept on a cleared holder (active minus required chords),
    // order-preserving.
    static QList<int> remainderAfterClear(const QList<int> &active);
    // Selection-scoped variants over the enabled catalog chords only. A
    // disabled focus-right additionally drops the Meta+L/Meta+Esc chords.
    // Two-arg forms stage Authentic for every enabled row (canonical chords,
    // historical behavior). Four-arg forms add the staged Authentic intent
    // plus the live Keep actuals (keepKeys, catalog-ordered, zero-filtered
    // by the implementation): authenticStaged true ignores keepKeys and
    // behaves like the two-arg form; false requires the authentic canonical
    // chords plus the Keep live chords (Lock Session relocation chords only
    // while focus-right itself is Authentic-enabled).
    static QList<int> conflictingKeysFor(const QList<int> &active, const QSet<QString> &disabledIds);
    static QList<int> conflictingKeysFor(const QList<int> &active, const QSet<QString> &disabledIds,
                                         bool authenticStaged, const QList<int> &keepKeys);
    static QList<int> remainderAfterClearFor(const QList<int> &active, const QSet<QString> &disabledIds);
    static QList<int> remainderAfterClearFor(const QList<int> &active, const QSet<QString> &disabledIds,
                                             bool authenticStaged, const QList<int> &keepKeys);
    // Required-key image over an explicit holder-active intersection.
    static QList<int> conflictingKeysIn(const QList<int> &active, const QList<int> &required);
    static QList<int> remainderAfterClearIn(const QList<int> &active, const QList<int> &required);
    static QList<int> lockPostFor(const QList<int> &lockPre);
    static QList<int> dedupKeys(const QList<int> &keys);
    static bool keysValid(const QList<int> &keys);
    static bool stringValid(const QString &value);
    // Cosmetic labels/contexts: empty allowed (real captures leave friendly
    // empty), only the length bound applies. Identity (action/compUnique)
    // and keys stay strict via stringValid/keysValid.
    static bool cosmeticValid(const QString &value);
    // Pure field-category validator shared by allShortcutInfos and
    // globalShortcutsByKey parsing (and mirrored by fakes): identity stays
    // strict, cosmetic labels/contexts allow empty with only the length
    // bound, active/default keys stay strict. Returns true when valid;
    // otherwise sets *fieldError to a bounded non-reflective suffix for the
    // caller to prefix. Each independently producing elementary predicate
    // has its own suffix, in fixed order: "empty action",
    // "oversized action", "empty component", "oversized component",
    // "oversized friendly", "oversized component friendly",
    // "oversized context unique", "oversized context friendly",
    // "too many active keys", "negative active key",
    // "oversized active key", "too many default keys",
    // "negative default key", "oversized default key". The key classifier
    // is one ordered scan (too-many, then negative, then oversized), so a
    // list with both a negative and an over-max key reports negative.
    // Full tokens add the per-parser prefix
    // ("unexpected globalShortcutsByKey reply: " or
    // "unexpected allShortcutInfos reply: "), so the same suffix under two
    // prefixes is two distinct full tokens, each covered on both seams.
    static bool keyedFieldsValid(const QString &action, const QString &friendly, const QString &compUnique,
                                 const QString &compFriendly, const QString &contextUnique,
                                 const QString &contextFriendly, const QList<int> &active,
                                 const QList<int> &defaults, QString *fieldError);
    static bool uniqueNameValid(const QString &owner);
    static bool introspectionContractValid(const QString &xml);
    static bool parseAllComponentsReply(QDBusMessage::MessageType replyType, const QString &replySignature,
                                        const QList<QVariant> &replyArgs, QStringList *components, QString *error);
    // Pure strict (ai) slot validation tested without D-Bus: exactly four
    // ints in [0, SHORTCUT_MAX_KEY_VALUE]; the live encoding always writes
    // four slots with zero padding. Both reply-decode variants share it.
    static bool decodeKeySequenceSlots(const QList<int> &slotValues, QKeySequence *out);
    // Pure setter-reply slot-set validation used by the real setShortcutKeys
    // decoder: each group must be exactly four bounded slots; the outer
    // count must not exceed SHORTCUT_MAX_KEYS_PER_TUPLE. Empty accepted.
    static bool decodeSetterReplySlotSets(const QList<QList<int>> &slotGroups, QSet<QKeySequence> *out);
    // Bounded native TRANSPORT pure seams (Delivery 2 Unit 1, no live bus):
    // exact defaultShortcutKeys(as)->a(ai) reply decode into primitive
    // defaults (flattened distinct non-zero slots, sorted for determinism),
    // exact setForeignShortcutKeys(as,a(ai))->void reply validation (empty
    // void reply only), and order-insensitive set comparison for the void
    // setter fresh readback. Ordered type then signature then arity.
    static bool parseDefaultShortcutKeysReply(QDBusMessage::MessageType replyType, const QString &replySignature,
                                              const QList<QVariant> &replyArgs, QList<int> *defaults,
                                              QString *error);
    static bool parseSetForeignShortcutKeysReply(QDBusMessage::MessageType replyType,
                                                 const QString &replySignature, const QList<QVariant> &replyArgs,
                                                 QString *error);
    static bool foreignReadbackMatches(const QList<int> &expected, const QList<int> &actual);
    static bool clearedActionsPathSafe(const QString &path, QString *error);
    // Defect B keyed conflict detection (authoritative, not enumeration).
    static QList<int> relevantConflictKeys();
    // Selection-scoped required chords: canonical keys of Authentic-enabled
    // catalog rows, plus the caller-supplied live Keep chords (keepKeys),
    // plus the Lock Session relocation chords only when focus-right is
    // Authentic-enabled. Catalog order, deduplicated. The single-arg form
    // stages Authentic for every enabled row (historical behavior).
    static QList<int> enabledRequiredKeys(const QSet<QString> &disabledIds);
    static QList<int> enabledRequiredKeys(const QSet<QString> &disabledIds, bool authenticStaged,
                                          const QList<int> &keepKeys);
    // Live Keep chords in catalog order from an explicit per-catalog active
    // image (missing rows contribute nothing). Empty unless preserving.
    static QList<int> keepKeysForImages(const QList<QList<int>> &liveImages, const QSet<QString> &disabledIds,
                                        bool authenticStaged);
    static QString keyDisplayName(int key);
    // Bounded safe key-list image for diagnostics/preview: "none" for empty,
    // otherwise comma-joined ints. Only ever called with validated key lists.
    static QString keysDisplay(const QList<int> &keys);
    // Human-readable chord image for UI rows: "none" for empty, otherwise
    // comma-joined keyDisplayName values. Diagnostics keep keysDisplay raw.
    static QString keysDisplayNames(const QList<int> &keys);
    // Combined blocker-first conflict order for one preview. Both lists
    // are already sorted; this only concatenates blockers then mismatches
    // so status, refusal, dialog, and logs cannot diverge.
    static QList<ShortcutOrderedConflict> orderedConflicts(const ShortcutForcePreview &preview);
    // Per-row reconstructible conflict logging owned here: one summary plus
    // one bounded line per conflict row (chunked when a row exceeds the
    // bound), Info when forceable, Warning otherwise. IDs plus raw key
    // images only; never friendly labels.
    static void logForcePreview(const char *operation, const ShortcutForcePreview &preview);
    // Unknown disabled IDs fail closed before any write.
    static bool disabledIdsValid(const QSet<QString> &disabledIds, QString *error);
    // Non-project foreign tuple IDs whose wire defaults contain the key.
    // Own project defaults are never listed; Lock Session is exempt.
    static QStringList foreignDefaultIdsForKey(int key, const QList<ShortcutTuple> &tuples);
    static bool isAuthorizedDisplacement(int key, const QString &component, const QString &action);
    static bool parseGlobalShortcutsByKeyReply(QDBusMessage::MessageType replyType, const QString &replySignature,
                                               const QList<QVariant> &replyArgs,
                                               QList<ShortcutKeyHolder> *holders, QString *error);
    // Pure allShortcutInfos reply parser shared with KGlobalAccelStore::readAll
    // (no live D-Bus): exact ReplyMessage with signature "a(ssssssaiai)".
    // Ordered type then signature then arity; field validation shares
    // keyedFieldsValid; bound SHORTCUT_MAX_TUPLES. Lets hermetic tests cover
    // the live read path without session services and without duplicating
    // parsing in the backend.
    static bool parseAllShortcutInfosReply(QDBusMessage::MessageType replyType, const QString &replySignature,
                                           const QList<QVariant> &replyArgs, QList<ShortcutTuple> *tuples,
                                           QString *error);
    // Shared pure field-to-output mapping (no D-Bus): validates each fields
    // record via keyedFieldsValid and maps to holders/tuples with the exact
    // per-parser error prefix and bound ("too many holders" / "too many
    // tuples"). The QDBusArgument reply parsers enforce their own independent
    // fail-fast wire bound in the demarshal loop ("too many wire holders" /
    // "too many wire tuples") immediately after every append, before further
    // payload allocation, then delegate here, so hermetic tests of these functions cover the real field
    // branches (a direct readable QDBusArgument is not constructible via the
    // public Qt API outside a real bus reply).
    static bool holdersFromInfoFields(const QList<ShortcutInfoFields> &infos,
                                      QList<ShortcutKeyHolder> *holders, QString *error);
    static bool tuplesFromInfoFields(const QList<ShortcutInfoFields> &infos, QList<ShortcutTuple> *tuples,
                                     QString *error);
    // Smallest pure seams covering otherwise unreachable wire/collected and
    // defensive branches (no new capability, same tokens/bounds/order):
    // appendComponentPath validates one ao path and appends on success,
    // parameterized by the two actual representation provenances (false is
    // the typed QList<QDBusObjectPath> list, true is the read-mode
    // QDBusArgument array); checkTupleAppendBound is the shared size
    // predicate called immediately after each real wire append and each
    // real cross-component collected append; checkOccupancyKeyRange is the
    // single-key range helper called by occupancy. Each returns false with
    // the exact full bounded token for its failing predicate only.
    enum class TupleAppendBound
    {
        ByKeyWire,
        AllInfosWire,
        Collected
    };
    static bool appendComponentPath(const QString &path, bool fromArgumentArray, QStringList *parsed,
                                    QString *error);
    static bool checkTupleAppendBound(int size, TupleAppendBound kind, QString *error);
    static bool checkOccupancyKeyRange(int key, QString *error);
    static bool parseGlobalShortcutAvailableReply(QDBusMessage::MessageType replyType,
                                                  const QString &replySignature, const QList<QVariant> &replyArgs,
                                                  bool *available, QString *error);
    // Calls shortcutsByKey + shortcutAvailable for every relevant key.
    // Skips only the project actions, Lock Session, and the explicit
    // System Monitor Meta+Esc displacement (shared with the backend
    // holder scan); any other holder, known or unknown, fails closed
    // with a "claimed by" error before any write. Availability
    // consistency fails closed in both directions: holders empty must
    // report available, holders non-empty must report unavailable
    // (whole-key semantics).
    static KeyedOccupancyResult checkKeyedForeignOccupancyDetailed(ShortcutStore *store);
    static bool checkKeyedForeignOccupancy(ShortcutStore *store, QString *error);
    // Selection-scoped occupancy gate over the enabled required chords.
    // Two-arg form stages Authentic for every enabled row (historical
    // behavior); three-arg form binds the staged Authentic intent, with
    // Keep rows contributing their live actual chords.
    static KeyedOccupancyResult checkKeyedForeignOccupancyDetailedFor(ShortcutStore *store,
                                                                      const QSet<QString> &disabledIds);
    static KeyedOccupancyResult checkKeyedForeignOccupancyDetailedFor(ShortcutStore *store,
                                                                      const QSet<QString> &disabledIds,
                                                                      bool authenticStaged);
    // Full-catalog row displays in catalog order: one keyed holder query
    // per row for current holders (canonical chord plus live actual chords,
    // so Keep-customized rows report their actual conflicts), plus the known
    // compiled default and the live project defaults from readAll. Any store
    // failure fails closed with the row marked unknown; never invents
    // holders or defaults.
    static bool collectRowDisplays(ShortcutStore *store, QList<ShortcutRowDisplay> *rows, QString *error);
    // Shared occupancy exemption behind the status check and the backend
    // holder scan: project actions own their chords, Lock Session owns
    // its chord, and the explicit System Monitor Meta+Esc holder is
    // user-authorized. Every other holder, including the compiled
    // foreign rows (Grid View, Switcher, Monocle), is a conflict.
    static bool isHolderExempt(const QString &component, const QString &action, int key);

private:
    ShortcutStore *m_store = nullptr;
    // Durable cleared-ID list in the project config. Not owned.
    // Force persists the union here before clearing; Revert consumes it.
    ClearedActionsStore *m_cleared = nullptr;

    // One observed holder row needing clearance.
    struct ClearRow
    {
        QString component;
        QString action;
        QString componentFriendly;
        QString friendly;
        QList<int> active;
        QList<int> removals;
        QList<int> remainder;
    };
    // A non-exempt holder listed for a required key without any required
    // key in its active list (e.g. a .desktop-declared default). It claims
    // the chord but offers nothing Force can clear: Apply refuses and Force
    // is not available until it is unbound manually.
    struct Blocker
    {
        QString component;
        QString action;
        int key = 0;
        QList<int> active;
        QString componentFriendly;
        QString friendly;
    };
    struct HolderSnapshot
    {
        QString owner;
        uint uid = 0;
        ShortcutTuple lock;
        bool lockResolved = false;
        // Authorized displacement observed on the relocation target while
        // in scope (never writable, never cleared).
        bool authorizedTargetPresent = false;
        QString authorizedTargetComponent;
        QString authorizedTargetAction;
        QList<int> authorizedTargetActive;
        QString authorizedTargetComponentFriendly;
        QString authorizedTargetFriendly;
        QList<ShortcutTuple> projects;
        // Full-catalog actives in catalog order (empty when the entry is
        // missing live). The confirmed Force image binds this snapshot.
        QList<QList<int>> catalogActives;
        QList<bool> catalogPresent;
        QList<ClearRow> rows;
        QList<Blocker> blocked;
    };
    // Shared fresh read for apply and force preview: setter contract, owner,
    // project/lock tuple resolution with key bounds, and the per-required-key
    // holder scan. Zero writes. Fails closed on any transport, parsing,
    // consistency, or lock-precondition failure. Disabled catalog rows are
    // out of scope: their chords are never scanned and their holders never
    // become clear rows or blockers. Keep (non-Authentic) rows contribute
    // their live actual chords to the scan and are never assigned; the lock
    // resolves only while focus-right itself is Authentic-enabled (a Keep or
    // disabled focus-right leaves the lock out of scope with no
    // Meta+L/Meta+Esc scan, resolution, or relocation).
    bool collectHolderSnapshot(HolderSnapshot *snapshot, QString *error);
    bool collectHolderSnapshotFor(HolderSnapshot *snapshot, const QSet<QString> &disabledIds, QString *error);
    bool collectHolderSnapshotFor(HolderSnapshot *snapshot, const QSet<QString> &disabledIds,
                                  bool authenticStaged, QString *error);
    // Assigns project posts and relocates Lock Session from a fresh read.
    // Used by both apply (no holders present) and the second half of force
    // (holders just cleared). Owner-pinned with confirmed replies. Keep rows
    // are verified present but never assigned; the lock relocates only while
    // focus-right is Authentic-enabled.
    ShortcutApplyResult writeProjectKeys(const char *operation);
    ShortcutApplyResult writeProjectKeysFor(const char *operation, const QSet<QString> &disabledIds);
    ShortcutApplyResult writeProjectKeysFor(const char *operation, const QSet<QString> &disabledIds,
                                            bool authenticStaged);
    // Fresh preview builder behind previewForceApply and the force
    // revalidation inside applyForced.
    ShortcutForcePreview buildForcePreview();
    ShortcutForcePreview buildForcePreviewFor(const QSet<QString> &disabledIds);
    ShortcutForcePreview buildForcePreviewFor(const QSet<QString> &disabledIds, bool authenticStaged);
    static bool forceMismatchFromRow(const ClearRow &row, ShortcutForceMismatch *out);
};

// Live backend factories for KCM integration. The KCM calls only these and
// the allowlisted reconciler API; no other identities are exposed here.
ShortcutStore *createLiveShortcutStore();
ClearedActionsStore *createLiveClearedActionsStore(const QString &filePath);
// Host-independent project config path (GenericConfigLocation,
// project-owned) for the durable cleared-ID list.
QString defaultClearedActionsPath();



} // namespace KWin
Q_DECLARE_METATYPE(KWin::ShortcutMatchType)
Q_DECLARE_METATYPE(KWin::ShortcutInfoFields)
Q_DECLARE_METATYPE(QList<KWin::ShortcutInfoFields>)
QDBusArgument &operator<<(QDBusArgument &argument, const KWin::ShortcutMatchType &match);
const QDBusArgument &operator>>(const QDBusArgument &argument, KWin::ShortcutMatchType &match);
