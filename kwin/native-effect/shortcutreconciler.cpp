#include "shortcutreconciler.h"

#include <KConfig>
#include <KConfigGroup>

#include <QDBusArgument>
#include <QDBusConnection>
#include <QDBusConnectionInterface>
#include <QDBusMessage>
#include <QDBusMetaType>
#include <QDBusObjectPath>
#include <QDBusReply>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QKeySequence>
#include <QRegularExpression>
#include <QSet>
#include <QStandardPaths>
#include <QXmlStreamReader>

#include <cerrno>
#include <sys/stat.h>
#include <unistd.h>

#include <algorithm>

// QKeySequence D-Bus encoding is (ai): a struct holding an int array of up
// to four combined key values. QSet<QKeySequence> therefore encodes as
// a(ai). QList<QList<int>> would encode as aai and is wrong here. These
// operators match the KF6 KGlobalAccel encoding; the sets additionally need
// qDBusRegisterMetaType<QSet<QKeySequence>>() before use.
QDBusArgument &operator<<(QDBusArgument &argument, const QKeySequence &sequence)
{
    argument.beginStructure();
    argument.beginArray(qMetaTypeId<int>());
    for (int i = 0; i < 4; ++i) {
        const int value = (i < sequence.count()) ? sequence[static_cast<uint>(i)].toCombined() : 0;
        argument << value;
    }
    argument.endArray();
    argument.endStructure();
    return argument;
}

const QDBusArgument &operator>>(const QDBusArgument &argument, QKeySequence &sequence)
{
    // Read-mode guarded exact four-slot decode: a container must never
    // reach operator>>(int&). Check structure -> array -> basic before
    // each descent/read; any unexpected framing fails closed to empty.
    sequence = QKeySequence();
    if (argument.currentType() != QDBusArgument::StructureType) {
        return argument;
    }
    argument.beginStructure();
    if (argument.currentType() != QDBusArgument::ArrayType) {
        argument.endStructure();
        return argument;
    }
    argument.beginArray();
    QList<int> values;
    int innerCount = 0;
    bool bad = false;
    while (!argument.atEnd()) {
        if (argument.currentType() != QDBusArgument::BasicType) {
            bad = true;
            break;
        }
        int value = 0;
        argument >> value;
        if (innerCount < 4) {
            values.append(value);
        }
        ++innerCount;
    }
    argument.endArray();
    argument.endStructure();
    if (bad || innerCount != 4 || !KWin::ShortcutReconciler::decodeKeySequenceSlots(values, &sequence)) {
        sequence = QKeySequence();
        return argument;
    }
    return argument;
}

// Qt generic container extraction requires push_back, which QSet lacks
// (it has insert). This exact overload provides a(ai) framing for
// qDBusRegisterMetaType<QSet<QKeySequence>> and any direct extraction.
// Bounded: malformed or oversized input becomes an unmatchable sentinel so
// a clear-row reply cannot mistake it for a valid empty set. Read-mode
// guarded: a non-array or non-structure element never reaches basic reads.
const QDBusArgument &operator>>(const QDBusArgument &argument, QSet<QKeySequence> &set)
{
    set.clear();
    if (argument.currentType() != QDBusArgument::ArrayType) {
        return argument;
    }
    argument.beginArray();
    int count = 0;
    bool bad = false;
    while (!argument.atEnd()) {
        if (argument.currentType() != QDBusArgument::StructureType) {
            bad = true;
            break;
        }
        QKeySequence sequence;
        argument >> sequence;
        if (sequence.isEmpty()) {
            bad = true;
            break;
        }
        if (count <= KWin::SHORTCUT_MAX_KEYS_PER_TUPLE) {
            set.insert(sequence);
        }
        ++count;
        if (count > KWin::SHORTCUT_MAX_KEYS_PER_TUPLE) {
            while (!argument.atEnd()) {
                if (argument.currentType() != QDBusArgument::StructureType) {
                    bad = true;
                    break;
                }
                QKeySequence extra;
                argument >> extra;
                ++count;
            }
            break;
        }
    }
    argument.endArray();
    if (bad || count > KWin::SHORTCUT_MAX_KEYS_PER_TUPLE) {
        set.clear();
        // Preserve malformed/oversized wire input as unmatchable. An empty
        // set would otherwise falsely confirm a clear-row reply.
        set.insert(QKeySequence(KWin::SHORTCUT_MAX_KEY_VALUE + 1));
    }
    return argument;
}

// MatchType (i): single-int struct for globalShortcutsByKey Equal matching.
QDBusArgument &operator<<(QDBusArgument &argument, const KWin::ShortcutMatchType &match)
{
    argument.beginStructure();
    argument << match.value;
    argument.endStructure();
    return argument;
}

const QDBusArgument &operator>>(const QDBusArgument &argument, KWin::ShortcutMatchType &match)
{
    match.value = 0;
    if (argument.currentType() != QDBusArgument::StructureType) {
        return argument;
    }
    argument.beginStructure();
    if (argument.currentType() != QDBusArgument::BasicType) {
        argument.endStructure();
        return argument;
    }
    argument >> match.value;
    argument.endStructure();
    return argument;
}

namespace KWin
{

QDBusArgument &operator<<(QDBusArgument &argument, const ShortcutInfoFields &info)
{
    argument.beginStructure();
    argument << info.action << info.friendly << info.compUnique << info.compFriendly << info.contextUnique
             << info.contextFriendly << info.active << info.defaults;
    argument.endStructure();
    return argument;
}

const QDBusArgument &operator>>(const QDBusArgument &argument, ShortcutInfoFields &info)
{
    // Read-mode guarded: structure framing checked before descent; each
    // basic string checked as BasicType and each int array checked as
    // ArrayType with BasicType elements, so malformed containers fail
    // closed instead of reaching basic extraction.
    info = ShortcutInfoFields();
    if (argument.currentType() != QDBusArgument::StructureType) {
        return argument;
    }
    argument.beginStructure();
    auto readString = [&](QString *out) {
        if (argument.currentType() != QDBusArgument::BasicType) {
            return false;
        }
        QString value;
        argument >> value;
        if (out) {
            *out = value;
        }
        return true;
    };
    auto readIntList = [&](QList<int> *out) {
        if (argument.currentType() != QDBusArgument::ArrayType) {
            return false;
        }
        argument.beginArray();
        QList<int> values;
        while (!argument.atEnd()) {
            if (argument.currentType() != QDBusArgument::BasicType) {
                argument.endArray();
                return false;
            }
            int value = 0;
            argument >> value;
            values.append(value);
        }
        argument.endArray();
        if (out) {
            *out = values;
        }
        return true;
    };
    QString action;
    QString friendly;
    QString compUnique;
    QString compFriendly;
    QString contextUnique;
    QString contextFriendly;
    QList<int> active;
    QList<int> defaults;
    if (!readString(&action) || !readString(&friendly) || !readString(&compUnique) || !readString(&compFriendly)
        || !readString(&contextUnique) || !readString(&contextFriendly) || !readIntList(&active)
        || !readIntList(&defaults)) {
        argument.endStructure();
        info = ShortcutInfoFields();
        return argument;
    }
    argument.endStructure();
    info.action = action;
    info.friendly = friendly;
    info.compUnique = compUnique;
    info.compFriendly = compFriendly;
    info.contextUnique = contextUnique;
    info.contextFriendly = contextFriendly;
    info.active = active;
    info.defaults = defaults;
    return argument;
}

namespace
{

bool findAllowlisted(const QList<ShortcutTuple> &tuples, const QString &component, const QString &action,
                     ShortcutTuple *out, QString *error)
{
    int matches = 0;
    ShortcutTuple found;
    for (const ShortcutTuple &tuple : tuples) {
        if (tuple.component == component && tuple.action == action) {
            ++matches;
            found = tuple;
        }
    }
    if (matches != 1) {
        if (error) {
            *error = QStringLiteral("allowlisted tuple %1/%2 has %3 records, expected exactly one")
                         .arg(component, action)
                         .arg(matches);
        }
        return false;
    }
    if (out) {
        *out = found;
    }
    return true;
}

// Bounded key-list classifier without duplicate validation: single ordered
// scan distinguishing the three elementary range failures. Too-many is
// checked first, then any negative element, then any over-max element. A
// list containing both a negative and an over-max key reports negative.
enum class KeyListFailure
{
    None,
    TooMany,
    Negative,
    Oversized
};

KeyListFailure classifyKeyList(const QList<int> &keys)
{
    if (keys.size() > SHORTCUT_MAX_KEYS_PER_TUPLE) {
        return KeyListFailure::TooMany;
    }
    for (int key : keys) {
        if (key < 0) {
            return KeyListFailure::Negative;
        }
    }
    for (int key : keys) {
        if (key > SHORTCUT_MAX_KEY_VALUE) {
            return KeyListFailure::Oversized;
        }
    }
    return KeyListFailure::None;
}

bool ensurePrivateDir(const QString &dirPath, QString *error)
{
    QDir dir;
    if (!dir.mkpath(dirPath)) {
        if (error) {
            *error = QStringLiteral("could not create the cleared shortcut directory");
        }
        return false;
    }
    // Owner-safe private permissions: user-owned mode 0700 exactly.
    const QByteArray encoded = dirPath.toLocal8Bit();
    struct stat st = {};
    if (::stat(encoded.constData(), &st) != 0) {
        if (error) {
            *error = QStringLiteral("could not stat the cleared shortcut directory");
        }
        return false;
    }
    if (st.st_uid != static_cast<uid_t>(::geteuid())) {
        if (error) {
            *error = QStringLiteral("cleared shortcut directory is not owned by this user");
        }
        return false;
    }
    if ((st.st_mode & 0077) != 0) {
        if (::chmod(encoded.constData(), 0700) != 0) {
            if (error) {
                *error = QStringLiteral("cleared shortcut directory is not private");
            }
            return false;
        }
    }
    return true;
}

bool ensurePrivateFile(const QString &filePath, QString *error)
{
    const QByteArray encoded = filePath.toLocal8Bit();
    struct stat st = {};
    if (::stat(encoded.constData(), &st) != 0) {
        return true; // absent leaf needs no chmod yet
    }
    if (st.st_uid != static_cast<uid_t>(::geteuid())) {
        if (error) {
            *error = QStringLiteral("cleared shortcut file is not owned by this user");
        }
        return false;
    }
    if ((st.st_mode & 0077) != 0) {
        if (::chmod(encoded.constData(), 0600) != 0) {
            if (error) {
                *error = QStringLiteral("cleared shortcut file is not private");
            }
            return false;
        }
    }
    return true;
}

void ensureKeySequenceMetaTypes()
{
    static bool registered = false;
    if (registered) {
        return;
    }
    qDBusRegisterMetaType<QKeySequence>();
    qDBusRegisterMetaType<QSet<QKeySequence>>();
    qDBusRegisterMetaType<QList<QKeySequence>>();
    qDBusRegisterMetaType<ShortcutMatchType>();
    registered = true;
}

QSet<QKeySequence> keySetFromInts(const QList<int> &keys)
{
    // Intentional order is retained by the caller in QList form; the QSet
    // itself is unordered by definition.
    QSet<QKeySequence> out;
    for (int key : keys) {
        out.insert(QKeySequence(key, 0, 0, 0));
    }
    return out;
}

QList<QKeySequence> keyListFromInts(const QList<int> &keys)
{
    // Installed kf6_org.kde.KGlobalAccel.xml annotates the v2 key inputs as
    // QList<QKeySequence> (a(ai) framing); the QSet form encodes identically
    // but the list matches the pinned interface annotation.
    QList<QKeySequence> out;
    out.reserve(keys.size());
    for (int key : keys) {
        out.append(QKeySequence(key, 0, 0, 0));
    }
    return out;
}

QSet<int> intSetFromKeySet(const QSet<QKeySequence> &keys)
{
    QSet<int> out;
    for (const QKeySequence &sequence : keys) {
        for (int i = 0; i < sequence.count(); ++i) {
            out.insert(sequence[static_cast<uint>(i)].toCombined());
        }
    }
    return out;
}

QSet<int> intSetFromKeyList(const QList<QKeySequence> &keys)
{
    QSet<int> out;
    for (const QKeySequence &sequence : keys) {
        for (int i = 0; i < sequence.count(); ++i) {
            out.insert(sequence[static_cast<uint>(i)].toCombined());
        }
    }
    return out;
}

QList<int> sortedIntsFromSet(const QSet<int> &set)
{
    QList<int> out(set.begin(), set.end());
    std::sort(out.begin(), out.end());
    return out;
}

} // namespace

const QList<ShortcutConflictRow> &shortcutConflictTable()
{
    static const QList<ShortcutConflictRow> table = {
        {shortcutFocusComponent(), shortcutFocusAction(), {SHORTCUT_META_L}, shortcutLockComponent(),
         shortcutLockAction(), {SHORTCUT_META_L}, shortcutResolutionRelocate(), {SHORTCUT_META_ESC},
         shortcutAuthorizedEscComponent(), shortcutAuthorizedEscAction(), QStringLiteral("Meta+L"),
         QStringLiteral("Meta+Esc"), QStringLiteral("focus")},
        {shortcutResizeUpComponent(), shortcutResizeUpAction(), {SHORTCUT_META_ALT_K},
          shortcutSwitchNextComponent(), shortcutSwitchNextAction(), {SHORTCUT_META_ALT_K},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Alt+K"), {}, QStringLiteral("resize-up")},
        {shortcutResizeRightComponent(), shortcutResizeRightAction(), {SHORTCUT_META_ALT_L},
          shortcutSwitchLastComponent(), shortcutSwitchLastAction(), {SHORTCUT_META_ALT_L},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Alt+L"), {}, QStringLiteral("resize-right")},
        {shortcutResizeLeftArrowComponent(), shortcutResizeLeftArrowAction(), {SHORTCUT_META_ALT_LEFT},
          shortcutSwitchLeftComponent(), shortcutSwitchLeftAction(), {SHORTCUT_META_ALT_LEFT},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Alt+Left"), {},
          QStringLiteral("resize-left-arrow")},
        {shortcutResizeDownArrowComponent(), shortcutResizeDownArrowAction(), {SHORTCUT_META_ALT_DOWN},
          shortcutSwitchDownComponent(), shortcutSwitchDownAction(), {SHORTCUT_META_ALT_DOWN},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Alt+Down"), {},
          QStringLiteral("resize-down-arrow")},
        {shortcutResizeUpArrowComponent(), shortcutResizeUpArrowAction(), {SHORTCUT_META_ALT_UP},
          shortcutSwitchUpComponent(), shortcutSwitchUpAction(), {SHORTCUT_META_ALT_UP},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Alt+Up"), {},
          QStringLiteral("resize-up-arrow")},
        {shortcutResizeRightArrowComponent(), shortcutResizeRightArrowAction(), {SHORTCUT_META_ALT_RIGHT},
          shortcutSwitchRightComponent(), shortcutSwitchRightAction(), {SHORTCUT_META_ALT_RIGHT},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Alt+Right"), {},
          QStringLiteral("resize-right-arrow")},
        {shortcutFloatComponent(), shortcutFloatAction(), {SHORTCUT_META_G},
          shortcutGridViewComponent(), shortcutGridViewAction(), {SHORTCUT_META_G},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+G"), {}, QStringLiteral("float")},
        {shortcutMaximizeComponent(), shortcutMaximizeAction(), {SHORTCUT_META_M},
          shortcutMonocleComponent(), shortcutMonocleAction(), {SHORTCUT_META_M},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+M"), {}, QStringLiteral("maximize")},
        {shortcutFocusLeftArrowComponent(), shortcutFocusLeftArrowAction(), {SHORTCUT_META_LEFT},
          shortcutQuickTileLeftComponent(), shortcutQuickTileLeftAction(), {SHORTCUT_META_LEFT},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Left"), {},
          QStringLiteral("focus-left-arrow")},
        {shortcutFocusDownArrowComponent(), shortcutFocusDownArrowAction(), {SHORTCUT_META_DOWN},
          shortcutQuickTileBottomComponent(), shortcutQuickTileBottomAction(), {SHORTCUT_META_DOWN},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Down"), {},
          QStringLiteral("focus-down-arrow")},
        {shortcutFocusUpArrowComponent(), shortcutFocusUpArrowAction(), {SHORTCUT_META_UP},
          shortcutQuickTileTopComponent(), shortcutQuickTileTopAction(), {SHORTCUT_META_UP},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Up"), {},
          QStringLiteral("focus-up-arrow")},
        {shortcutFocusRightArrowComponent(), shortcutFocusRightArrowAction(), {SHORTCUT_META_RIGHT},
          shortcutQuickTileRightComponent(), shortcutQuickTileRightAction(), {SHORTCUT_META_RIGHT},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Right"), {},
          QStringLiteral("focus-right-arrow")},
        {shortcutMoveLeftArrowComponent(), shortcutMoveLeftArrowAction(), {SHORTCUT_META_SHIFT_LEFT},
          shortcutToPrevScreenComponent(), shortcutToPrevScreenAction(), {SHORTCUT_META_SHIFT_LEFT},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Shift+Left"), {},
          QStringLiteral("move-left-arrow")},
        {shortcutMoveRightArrowComponent(), shortcutMoveRightArrowAction(), {SHORTCUT_META_SHIFT_RIGHT},
          shortcutToNextScreenComponent(), shortcutToNextScreenAction(), {SHORTCUT_META_SHIFT_RIGHT},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Shift+Right"), {},
          QStringLiteral("move-right-arrow")},
        {shortcutWorkspacePrevLeftArrowComponent(), shortcutWorkspacePrevLeftArrowAction(),
          {SHORTCUT_META_CTRL_LEFT}, shortcutSwitchOneDesktopLeftComponent(), shortcutSwitchOneDesktopLeftAction(),
          {SHORTCUT_META_CTRL_LEFT}, shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Ctrl+Left"), {},
          QStringLiteral("workspace-prev-left-arrow")},
        {shortcutWorkspacePrevUpArrowComponent(), shortcutWorkspacePrevUpArrowAction(), {SHORTCUT_META_CTRL_UP},
          shortcutSwitchOneDesktopUpComponent(), shortcutSwitchOneDesktopUpAction(), {SHORTCUT_META_CTRL_UP},
          shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Ctrl+Up"), {},
          QStringLiteral("workspace-prev-up-arrow")},
        {shortcutWorkspaceNextDownArrowComponent(), shortcutWorkspaceNextDownArrowAction(),
          {SHORTCUT_META_CTRL_DOWN}, shortcutSwitchOneDesktopDownComponent(), shortcutSwitchOneDesktopDownAction(),
          {SHORTCUT_META_CTRL_DOWN}, shortcutResolutionClear(), {}, {}, {}, QStringLiteral("Meta+Ctrl+Down"), {},
          QStringLiteral("workspace-next-down-arrow")},
        {shortcutWorkspaceNextRightArrowComponent(), shortcutWorkspaceNextRightArrowAction(),
          {SHORTCUT_META_CTRL_RIGHT}, shortcutSwitchOneDesktopRightComponent(),
          shortcutSwitchOneDesktopRightAction(), {SHORTCUT_META_CTRL_RIGHT}, shortcutResolutionClear(), {}, {}, {},
          QStringLiteral("Meta+Ctrl+Right"), {}, QStringLiteral("workspace-next-right-arrow")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-left-arrow"),
          {SHORTCUT_META_CTRL_SHIFT_LEFT}, shortcutWindowOneDesktopLeftComponent(),
          shortcutWindowOneDesktopLeftAction(), {SHORTCUT_META_CTRL_SHIFT_LEFT}, shortcutResolutionClear(), {}, {},
          {}, QStringLiteral("Meta+Ctrl+Shift+Left"), {}, QStringLiteral("send-prev-left-arrow")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-up-arrow"),
          {SHORTCUT_META_CTRL_SHIFT_UP}, shortcutWindowOneDesktopUpComponent(),
          shortcutWindowOneDesktopUpAction(), {SHORTCUT_META_CTRL_SHIFT_UP}, shortcutResolutionClear(), {}, {}, {},
          QStringLiteral("Meta+Ctrl+Shift+Up"), {}, QStringLiteral("send-prev-up-arrow")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-down-arrow"),
          {SHORTCUT_META_CTRL_SHIFT_DOWN}, shortcutWindowOneDesktopDownComponent(),
          shortcutWindowOneDesktopDownAction(), {SHORTCUT_META_CTRL_SHIFT_DOWN}, shortcutResolutionClear(), {}, {},
          {}, QStringLiteral("Meta+Ctrl+Shift+Down"), {}, QStringLiteral("send-next-down-arrow")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-right-arrow"),
          {SHORTCUT_META_CTRL_SHIFT_RIGHT}, shortcutWindowOneDesktopRightComponent(),
          shortcutWindowOneDesktopRightAction(), {SHORTCUT_META_CTRL_SHIFT_RIGHT}, shortcutResolutionClear(), {},
          {}, {}, QStringLiteral("Meta+Ctrl+Shift+Right"), {}, QStringLiteral("send-next-right-arrow")},
    };
    return table;
}

const QList<ShortcutCatalogEntry> &shortcutProjectCatalog()
{
    // 37 plan rows then 39 item-1 workspace rows then 36 item-2 workspace
    // rows (8 bound relative send-and-follow, 20 unbound numbered
    // send-and-stay with symbol aliases plus append rows, 8 unbound
    // relative send-and-stay) then 12 item-5 output rows (8 bound
    // Meta+Ctrl+Alt+H/J/K/L and arrows send-and-follow, 4 unbound
    // directional send-and-stay in exact planOutputSendShortcutCatalog
    // order) then 4 R-WS-12 migrate rows (unbound directional follow in
    // exact planWorkspaceMigrateShortcutCatalog order) then 1
    // workspace-tiling toggle row (Meta+Y bound by user decision
    // 2026-10-10; no known stock KDE holder). Bound canonical
    // keys stay unique across the
    // catalog; unbound rows share key 0 with empty display. knownForeign*
    // mirrors the conflict table for its 23 project actions; every other
    // row carries empty known foreign state.
    static const QList<ShortcutCatalogEntry> catalog = {
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-left"), SHORTCUT_META_H,
         QStringLiteral("Meta+H"), {}, {}, 0, QStringLiteral("focus")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-left-arrow"), SHORTCUT_META_LEFT,
         QStringLiteral("Meta+Left"), shortcutQuickTileLeftComponent(), shortcutQuickTileLeftAction(),
         SHORTCUT_META_LEFT, QStringLiteral("focus")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-down"), SHORTCUT_META_J,
         QStringLiteral("Meta+J"), {}, {}, 0, QStringLiteral("focus")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-down-arrow"), SHORTCUT_META_DOWN,
         QStringLiteral("Meta+Down"), shortcutQuickTileBottomComponent(), shortcutQuickTileBottomAction(),
         SHORTCUT_META_DOWN, QStringLiteral("focus")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-up"), SHORTCUT_META_K,
         QStringLiteral("Meta+K"), {}, {}, 0, QStringLiteral("focus")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-up-arrow"), SHORTCUT_META_UP,
         QStringLiteral("Meta+Up"), shortcutQuickTileTopComponent(), shortcutQuickTileTopAction(), SHORTCUT_META_UP,
         QStringLiteral("focus")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"), SHORTCUT_META_L,
         QStringLiteral("Meta+L"), shortcutLockComponent(), shortcutLockAction(), SHORTCUT_META_L,
         QStringLiteral("focus")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right-arrow"), SHORTCUT_META_RIGHT,
         QStringLiteral("Meta+Right"), shortcutQuickTileRightComponent(), shortcutQuickTileRightAction(),
         SHORTCUT_META_RIGHT, QStringLiteral("focus")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-left"), SHORTCUT_META_SHIFT_H,
         QStringLiteral("Meta+Shift+H"), {}, {}, 0, QStringLiteral("move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-left-arrow"), SHORTCUT_META_SHIFT_LEFT,
         QStringLiteral("Meta+Shift+Left"), shortcutToPrevScreenComponent(), shortcutToPrevScreenAction(),
         SHORTCUT_META_SHIFT_LEFT, QStringLiteral("move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-down"), SHORTCUT_META_SHIFT_J,
         QStringLiteral("Meta+Shift+J"), {}, {}, 0, QStringLiteral("move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-down-arrow"), SHORTCUT_META_SHIFT_DOWN,
         QStringLiteral("Meta+Shift+Down"), {}, {}, 0, QStringLiteral("move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-up"), SHORTCUT_META_SHIFT_K,
         QStringLiteral("Meta+Shift+K"), {}, {}, 0, QStringLiteral("move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-up-arrow"), SHORTCUT_META_SHIFT_UP,
         QStringLiteral("Meta+Shift+Up"), {}, {}, 0, QStringLiteral("move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-right"), SHORTCUT_META_SHIFT_L,
         QStringLiteral("Meta+Shift+L"), {}, {}, 0, QStringLiteral("move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-right-arrow"), SHORTCUT_META_SHIFT_RIGHT,
         QStringLiteral("Meta+Shift+Right"), shortcutToNextScreenComponent(), shortcutToNextScreenAction(),
         SHORTCUT_META_SHIFT_RIGHT, QStringLiteral("move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-left"), SHORTCUT_META_ALT_H,
         QStringLiteral("Meta+Alt+H"), {}, {}, 0, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-left-arrow"),
         SHORTCUT_META_ALT_LEFT, QStringLiteral("Meta+Alt+Left"), shortcutSwitchLeftComponent(),
         shortcutSwitchLeftAction(), SHORTCUT_META_ALT_LEFT, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-down"), SHORTCUT_META_ALT_J,
         QStringLiteral("Meta+Alt+J"), {}, {}, 0, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-down-arrow"),
         SHORTCUT_META_ALT_DOWN, QStringLiteral("Meta+Alt+Down"), shortcutSwitchDownComponent(),
         shortcutSwitchDownAction(), SHORTCUT_META_ALT_DOWN, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-up"), SHORTCUT_META_ALT_K,
         QStringLiteral("Meta+Alt+K"), shortcutSwitchNextComponent(), shortcutSwitchNextAction(), SHORTCUT_META_ALT_K,
         QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-up-arrow"), SHORTCUT_META_ALT_UP,
         QStringLiteral("Meta+Alt+Up"), shortcutSwitchUpComponent(), shortcutSwitchUpAction(), SHORTCUT_META_ALT_UP,
         QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-right"), SHORTCUT_META_ALT_L,
         QStringLiteral("Meta+Alt+L"), shortcutSwitchLastComponent(), shortcutSwitchLastAction(), SHORTCUT_META_ALT_L,
         QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-right-arrow"),
         SHORTCUT_META_ALT_RIGHT, QStringLiteral("Meta+Alt+Right"), shortcutSwitchRightComponent(),
         shortcutSwitchRightAction(), SHORTCUT_META_ALT_RIGHT, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-inwards-left"), SHORTCUT_META_ALT_SHIFT_H,
         QStringLiteral("Meta+Alt+Shift+H"), {}, {}, 0, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-inwards-left-arrow"),
         SHORTCUT_META_ALT_SHIFT_LEFT, QStringLiteral("Meta+Alt+Shift+Left"), {}, {}, 0, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-inwards-down"), SHORTCUT_META_ALT_SHIFT_J,
         QStringLiteral("Meta+Alt+Shift+J"), {}, {}, 0, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-inwards-down-arrow"),
         SHORTCUT_META_ALT_SHIFT_DOWN, QStringLiteral("Meta+Alt+Shift+Down"), {}, {}, 0, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-inwards-up"), SHORTCUT_META_ALT_SHIFT_K,
         QStringLiteral("Meta+Alt+Shift+K"), {}, {}, 0, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-inwards-up-arrow"),
         SHORTCUT_META_ALT_SHIFT_UP, QStringLiteral("Meta+Alt+Shift+Up"), {}, {}, 0, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-inwards-right"), SHORTCUT_META_ALT_SHIFT_L,
         QStringLiteral("Meta+Alt+Shift+L"), {}, {}, 0, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-inwards-right-arrow"),
         SHORTCUT_META_ALT_SHIFT_RIGHT, QStringLiteral("Meta+Alt+Shift+Right"), {}, {}, 0, QStringLiteral("resize")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-float"), SHORTCUT_META_G,
         QStringLiteral("Meta+G"), shortcutGridViewComponent(), shortcutGridViewAction(), SHORTCUT_META_G,
         QStringLiteral("toggle")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-sticky"), SHORTCUT_META_SHIFT_G,
         QStringLiteral("Meta+Shift+G"), {}, {}, 0, QStringLiteral("toggle")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-maximize"), SHORTCUT_META_M,
         QStringLiteral("Meta+M"), shortcutMonocleComponent(), shortcutMonocleAction(), SHORTCUT_META_M,
         QStringLiteral("toggle")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-fullscreen"), SHORTCUT_META_F11,
         QStringLiteral("Meta+F11"), {}, {}, 0, QStringLiteral("toggle")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-orientation"), SHORTCUT_META_O,
         QStringLiteral("Meta+O"), {}, {}, 0, QStringLiteral("toggle")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-1"), SHORTCUT_META_1,
         QStringLiteral("Meta+1"), {}, {}, 0, QStringLiteral("workspace-select")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-2"), SHORTCUT_META_2,
         QStringLiteral("Meta+2"), {}, {}, 0, QStringLiteral("workspace-select")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-3"), SHORTCUT_META_3,
         QStringLiteral("Meta+3"), {}, {}, 0, QStringLiteral("workspace-select")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-4"), SHORTCUT_META_4,
         QStringLiteral("Meta+4"), {}, {}, 0, QStringLiteral("workspace-select")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-5"), SHORTCUT_META_5,
         QStringLiteral("Meta+5"), {}, {}, 0, QStringLiteral("workspace-select")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-6"), SHORTCUT_META_6,
         QStringLiteral("Meta+6"), {}, {}, 0, QStringLiteral("workspace-select")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-7"), SHORTCUT_META_7,
         QStringLiteral("Meta+7"), {}, {}, 0, QStringLiteral("workspace-select")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-8"), SHORTCUT_META_8,
         QStringLiteral("Meta+8"), {}, {}, 0, QStringLiteral("workspace-select")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-9"), SHORTCUT_META_9,
         QStringLiteral("Meta+9"), {}, {}, 0, QStringLiteral("workspace-select")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-0"), SHORTCUT_META_0,
         QStringLiteral("Meta+0"), {}, {}, 0, QStringLiteral("workspace-select")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-1"), SHORTCUT_META_SHIFT_1,
         QStringLiteral("Meta+Shift+1"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-1-symbol"), SHORTCUT_META_EXCLAM,
         QStringLiteral("Meta+!"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-2"), SHORTCUT_META_SHIFT_2,
         QStringLiteral("Meta+Shift+2"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-2-symbol"), SHORTCUT_META_AT,
         QStringLiteral("Meta+@"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-3"), SHORTCUT_META_SHIFT_3,
         QStringLiteral("Meta+Shift+3"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-3-symbol"),
         SHORTCUT_META_NUMBERSIGN, QStringLiteral("Meta+#"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-4"), SHORTCUT_META_SHIFT_4,
         QStringLiteral("Meta+Shift+4"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-4-symbol"), SHORTCUT_META_DOLLAR,
         QStringLiteral("Meta+$"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-5"), SHORTCUT_META_SHIFT_5,
         QStringLiteral("Meta+Shift+5"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-5-symbol"), SHORTCUT_META_PERCENT,
         QStringLiteral("Meta+%"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-6"), SHORTCUT_META_SHIFT_6,
         QStringLiteral("Meta+Shift+6"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-6-symbol"),
         SHORTCUT_META_ASCIICIRCUM, QStringLiteral("Meta+^"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-7"), SHORTCUT_META_SHIFT_7,
         QStringLiteral("Meta+Shift+7"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-7-symbol"),
         SHORTCUT_META_AMPERSAND, QStringLiteral("Meta+&"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-8"), SHORTCUT_META_SHIFT_8,
         QStringLiteral("Meta+Shift+8"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-8-symbol"),
         SHORTCUT_META_ASTERISK, QStringLiteral("Meta+*"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-9"), SHORTCUT_META_SHIFT_9,
         QStringLiteral("Meta+Shift+9"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-9-symbol"),
         SHORTCUT_META_PARENLEFT, QStringLiteral("Meta+("), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-append"), SHORTCUT_META_SHIFT_0,
         QStringLiteral("Meta+Shift+0"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-move-workspace-append-symbol"),
          SHORTCUT_META_PARENRIGHT, QStringLiteral("Meta+)"), {}, {}, 0, QStringLiteral("workspace-move")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-previous"), SHORTCUT_META_CTRL_TAB,
          QStringLiteral("Meta+Ctrl+Tab"), {}, {}, 0, QStringLiteral("workspace-previous")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-h"), SHORTCUT_META_CTRL_H,
          QStringLiteral("Meta+Ctrl+H"), {}, {}, 0, QStringLiteral("workspace-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-k"), SHORTCUT_META_CTRL_K,
          QStringLiteral("Meta+Ctrl+K"), {}, {}, 0, QStringLiteral("workspace-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-left-arrow"),
          SHORTCUT_META_CTRL_LEFT, QStringLiteral("Meta+Ctrl+Left"), shortcutSwitchOneDesktopLeftComponent(),
          shortcutSwitchOneDesktopLeftAction(), SHORTCUT_META_CTRL_LEFT, QStringLiteral("workspace-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-up-arrow"), SHORTCUT_META_CTRL_UP,
          QStringLiteral("Meta+Ctrl+Up"), shortcutSwitchOneDesktopUpComponent(), shortcutSwitchOneDesktopUpAction(),
          SHORTCUT_META_CTRL_UP, QStringLiteral("workspace-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-j"), SHORTCUT_META_CTRL_J,
          QStringLiteral("Meta+Ctrl+J"), {}, {}, 0, QStringLiteral("workspace-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-l"), SHORTCUT_META_CTRL_L,
          QStringLiteral("Meta+Ctrl+L"), {}, {}, 0, QStringLiteral("workspace-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-down-arrow"),
          SHORTCUT_META_CTRL_DOWN, QStringLiteral("Meta+Ctrl+Down"), shortcutSwitchOneDesktopDownComponent(),
          shortcutSwitchOneDesktopDownAction(), SHORTCUT_META_CTRL_DOWN, QStringLiteral("workspace-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-right-arrow"),
          SHORTCUT_META_CTRL_RIGHT, QStringLiteral("Meta+Ctrl+Right"), shortcutSwitchOneDesktopRightComponent(),
          shortcutSwitchOneDesktopRightAction(), SHORTCUT_META_CTRL_RIGHT, QStringLiteral("workspace-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-h"), SHORTCUT_META_CTRL_SHIFT_H,
          QStringLiteral("Meta+Ctrl+Shift+H"), {}, {}, 0, QStringLiteral("workspace-send-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-k"), SHORTCUT_META_CTRL_SHIFT_K,
          QStringLiteral("Meta+Ctrl+Shift+K"), {}, {}, 0, QStringLiteral("workspace-send-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-left-arrow"),
          SHORTCUT_META_CTRL_SHIFT_LEFT, QStringLiteral("Meta+Ctrl+Shift+Left"),
          shortcutWindowOneDesktopLeftComponent(), shortcutWindowOneDesktopLeftAction(),
          SHORTCUT_META_CTRL_SHIFT_LEFT, QStringLiteral("workspace-send-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-up-arrow"), SHORTCUT_META_CTRL_SHIFT_UP,
          QStringLiteral("Meta+Ctrl+Shift+Up"), shortcutWindowOneDesktopUpComponent(),
          shortcutWindowOneDesktopUpAction(), SHORTCUT_META_CTRL_SHIFT_UP,
          QStringLiteral("workspace-send-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-j"), SHORTCUT_META_CTRL_SHIFT_J,
          QStringLiteral("Meta+Ctrl+Shift+J"), {}, {}, 0, QStringLiteral("workspace-send-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-l"), SHORTCUT_META_CTRL_SHIFT_L,
          QStringLiteral("Meta+Ctrl+Shift+L"), {}, {}, 0, QStringLiteral("workspace-send-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-down-arrow"),
          SHORTCUT_META_CTRL_SHIFT_DOWN, QStringLiteral("Meta+Ctrl+Shift+Down"),
          shortcutWindowOneDesktopDownComponent(), shortcutWindowOneDesktopDownAction(),
          SHORTCUT_META_CTRL_SHIFT_DOWN, QStringLiteral("workspace-send-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-right-arrow"),
          SHORTCUT_META_CTRL_SHIFT_RIGHT, QStringLiteral("Meta+Ctrl+Shift+Right"),
          shortcutWindowOneDesktopRightComponent(), shortcutWindowOneDesktopRightAction(),
          SHORTCUT_META_CTRL_SHIFT_RIGHT, QStringLiteral("workspace-send-relative")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-1"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-1-symbol"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-2"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-2-symbol"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-3"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-3-symbol"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-4"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-4-symbol"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-5"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-5-symbol"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-6"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-6-symbol"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-7"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-7-symbol"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-8"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-8-symbol"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-9"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-9-symbol"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-append"), 0, QStringLiteral(""), {},
          {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-append-symbol"), 0,
          QStringLiteral(""), {}, {}, 0, QStringLiteral("workspace-move-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-stay-prev-h"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-send-relative-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-stay-prev-k"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-send-relative-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-stay-prev-left-arrow"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-send-relative-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-stay-prev-up-arrow"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-send-relative-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-stay-next-j"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-send-relative-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-stay-next-l"), 0, QStringLiteral(""), {}, {},
          0, QStringLiteral("workspace-send-relative-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-stay-next-down-arrow"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-send-relative-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-stay-next-right-arrow"), 0,
          QStringLiteral(""), {}, {}, 0, QStringLiteral("workspace-send-relative-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-left"), SHORTCUT_META_CTRL_ALT_H,
          QStringLiteral("Meta+Ctrl+Alt+H"), {}, {}, 0, QStringLiteral("output-send")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-left-arrow"),
          SHORTCUT_META_CTRL_ALT_LEFT, QStringLiteral("Meta+Ctrl+Alt+Left"), {}, {}, 0,
          QStringLiteral("output-send")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-left-stay"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("output-send-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-down"), SHORTCUT_META_CTRL_ALT_J,
          QStringLiteral("Meta+Ctrl+Alt+J"), {}, {}, 0, QStringLiteral("output-send")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-down-arrow"),
          SHORTCUT_META_CTRL_ALT_DOWN, QStringLiteral("Meta+Ctrl+Alt+Down"), {}, {}, 0,
          QStringLiteral("output-send")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-down-stay"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("output-send-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-up"), SHORTCUT_META_CTRL_ALT_K,
          QStringLiteral("Meta+Ctrl+Alt+K"), {}, {}, 0, QStringLiteral("output-send")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-up-arrow"),
          SHORTCUT_META_CTRL_ALT_UP, QStringLiteral("Meta+Ctrl+Alt+Up"), {}, {}, 0,
          QStringLiteral("output-send")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-up-stay"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("output-send-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-right"), SHORTCUT_META_CTRL_ALT_L,
          QStringLiteral("Meta+Ctrl+Alt+L"), {}, {}, 0, QStringLiteral("output-send")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-right-arrow"),
          SHORTCUT_META_CTRL_ALT_RIGHT, QStringLiteral("Meta+Ctrl+Alt+Right"), {}, {}, 0,
          QStringLiteral("output-send")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-right-stay"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("output-send-stay")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-migrate-workspace-left"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-migrate")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-migrate-workspace-right"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-migrate")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-migrate-workspace-up"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-migrate")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-migrate-workspace-down"), 0, QStringLiteral(""),
          {}, {}, 0, QStringLiteral("workspace-migrate")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-workspace-tiling"), SHORTCUT_META_Y,
          QStringLiteral("Meta+Y"), {}, {}, 0, QStringLiteral("toggle")},
    };
    return catalog;
}

QString shortcutCatalogId(const QString &component, const QString &action)
{
    return component + QStringLiteral("/") + action;
}

bool shortcutCatalogContains(const QString &component, const QString &action)
{
    for (const ShortcutCatalogEntry &entry : shortcutProjectCatalog()) {
        if (entry.component == component && entry.action == action) {
            return true;
        }
    }
    return false;
}

int shortcutCatalogKeyFor(const QString &component, const QString &action, bool *found)
{
    for (const ShortcutCatalogEntry &entry : shortcutProjectCatalog()) {
        if (entry.component == component && entry.action == action) {
            if (found) {
                *found = true;
            }
            return entry.canonicalKey;
        }
    }
    if (found) {
        *found = false;
    }
    return 0;
}

QSet<QString> shortcutKnownConflictIds()
{
    QSet<QString> out;
    for (const ShortcutConflictRow &row : shortcutConflictTable()) {
        out.insert(shortcutCatalogId(row.projectComponent, row.projectAction));
    }
    return out;
}

QStringList presetCompatibleDisabledIds(const QList<ShortcutCatalogEntry> &catalog,
                                        const QSet<QString> &knownConflictIds,
                                        const QSet<QString> &collidingIds)
{
    QStringList out;
    for (const ShortcutCatalogEntry &entry : catalog) {
        const QString id = shortcutCatalogId(entry.component, entry.action);
        if (knownConflictIds.contains(id) || collidingIds.contains(id)) {
            out.append(id);
        }
    }
    return out;
}

bool ShortcutReconciler::isAllowlisted(const QString &component, const QString &action)
{
    // Project writes cover the full catalog; foreign writes stay on the
    // exact conflict-table foreign rows (Lock Session relocation path).
    if (shortcutCatalogContains(component, action)) {
        return true;
    }
    for (const ShortcutConflictRow &row : shortcutConflictTable()) {
        if ((component == row.projectComponent && action == row.projectAction)
            || (component == row.foreignComponent && action == row.foreignAction)) {
            return true;
        }
    }
    return false;
}

bool ShortcutReconciler::isProjectAction(const QString &component, const QString &action)
{
    return shortcutCatalogContains(component, action);
}

bool ShortcutReconciler::isHistoricalProjectOwned(const QString &component, const QString &action)
{
    // Previous OmniTiler identity: the entire pre-rename catalog was
    // kwin/plasma-auto-tiler-*. Identity only; says nothing about live
    // callback status.
    return component == shortcutFocusComponent() && action.startsWith(QStringLiteral("plasma-auto-tiler-"));
}

bool ShortcutReconciler::isProjectOwned(const QString &component, const QString &action)
{
    // Own prefix, current and legacy alike (e.g. omnitiler-toggle,
    // omnitiler-float-toggle), plus the previous pre-rename identity
    // kwin/plasma-auto-tiler-*. Revert-only skip; isProjectAction and
    // isHolderExempt stay narrow so Force still lists and clears the
    // historical IDs on confirmation. Lock Session and foreign
    // components never match.
    if (component != shortcutFocusComponent()) {
        return false;
    }
    return action.startsWith(QStringLiteral("omnitiler-")) || action.startsWith(QStringLiteral("plasma-auto-tiler-"));
}

bool ShortcutReconciler::isLockAction(const QString &component, const QString &action)
{
    return component == shortcutLockComponent() && action == shortcutLockAction();
}

QList<int> ShortcutReconciler::conflictingKeys(const QList<int> &active)
{
    const QList<int> required = relevantConflictKeys();
    QList<int> out;
    out.reserve(active.size());
    for (int key : active) {
        if (required.contains(key) && !out.contains(key)) {
            out.append(key);
        }
    }
    return out;
}

QList<int> ShortcutReconciler::conflictingKeysFor(const QList<int> &active, const QSet<QString> &disabledIds)
{
    return conflictingKeysFor(active, disabledIds, true, QList<int>());
}

QList<int> ShortcutReconciler::conflictingKeysFor(const QList<int> &active, const QSet<QString> &disabledIds,
                                                 bool authenticStaged, const QList<int> &keepKeys)
{
    return conflictingKeysIn(active, enabledRequiredKeys(disabledIds, authenticStaged, keepKeys));
}

QList<int> ShortcutReconciler::conflictingKeysIn(const QList<int> &active, const QList<int> &required)
{
    QList<int> out;
    out.reserve(active.size());
    for (int key : active) {
        if (required.contains(key) && !out.contains(key)) {
            out.append(key);
        }
    }
    return out;
}

QList<int> ShortcutReconciler::remainderAfterClear(const QList<int> &active)
{
    const QList<int> required = relevantConflictKeys();
    QList<int> out;
    out.reserve(active.size());
    for (int key : active) {
        if (!required.contains(key)) {
            out.append(key);
        }
    }
    return out;
}

QList<int> ShortcutReconciler::remainderAfterClearFor(const QList<int> &active, const QSet<QString> &disabledIds)
{
    return remainderAfterClearFor(active, disabledIds, true, QList<int>());
}

QList<int> ShortcutReconciler::remainderAfterClearFor(const QList<int> &active, const QSet<QString> &disabledIds,
                                                     bool authenticStaged, const QList<int> &keepKeys)
{
    return remainderAfterClearIn(active, enabledRequiredKeys(disabledIds, authenticStaged, keepKeys));
}

QList<int> ShortcutReconciler::remainderAfterClearIn(const QList<int> &active, const QList<int> &required)
{
    QList<int> out;
    out.reserve(active.size());
    for (int key : active) {
        if (!required.contains(key)) {
            out.append(key);
        }
    }
    return out;
}

QList<int> ShortcutReconciler::lockPostFor(const QList<int> &lockPre)
{
    const ShortcutConflictRow &row = shortcutConflictTable().at(0);
    QList<int> replaced;
    replaced.reserve(lockPre.size());
    for (int key : lockPre) {
        if (row.foreignExpectedPre.contains(key)) {
            for (int t : row.resolutionTarget) replaced.append(t);
        } else {
            replaced.append(key);
        }
    }
    return dedupKeys(replaced);
}
QList<int> ShortcutReconciler::dedupKeys(const QList<int> &keys)
{
    QList<int> deduped;
    deduped.reserve(keys.size());
    for (int key : keys) {
        if (!deduped.contains(key)) {
            deduped.append(key);
        }
    }
    return deduped;
}

bool ShortcutReconciler::keysValid(const QList<int> &keys)
{
    if (keys.size() > SHORTCUT_MAX_KEYS_PER_TUPLE) {
        return false;
    }
    for (int key : keys) {
        if (key < 0 || key > SHORTCUT_MAX_KEY_VALUE) {
            return false;
        }
    }
    return true;
}

bool ShortcutReconciler::stringValid(const QString &value)
{
    return !value.isEmpty() && value.size() <= SHORTCUT_MAX_STRING_LEN;
}

bool ShortcutReconciler::cosmeticValid(const QString &value)
{
    return value.size() <= SHORTCUT_MAX_STRING_LEN;
}

bool ShortcutReconciler::keyedFieldsValid(const QString &action, const QString &friendly, const QString &compUnique,
                                          const QString &compFriendly, const QString &contextUnique,
                                          const QString &contextFriendly, const QList<int> &active,
                                          const QList<int> &defaults, QString *fieldError)
{
    auto fail = [&](const QString &suffix) {
        if (fieldError) {
            *fieldError = suffix;
        }
        return false;
    };
    // Identity strict, each elementary predicate distinct: empty vs
    // oversized per field, checked action then component. Direct
    // isEmpty/size checks (not stringValid) so the branch identifies the
    // field variation without duplicate validation.
    if (action.isEmpty()) {
        return fail(QStringLiteral("empty action"));
    }
    if (action.size() > SHORTCUT_MAX_STRING_LEN) {
        return fail(QStringLiteral("oversized action"));
    }
    if (compUnique.isEmpty()) {
        return fail(QStringLiteral("empty component"));
    }
    if (compUnique.size() > SHORTCUT_MAX_STRING_LEN) {
        return fail(QStringLiteral("oversized component"));
    }
    // Cosmetic empty allowed (real captures leave friendly empty); only the
    // length bound applies, one distinct token per field in fixed order.
    if (friendly.size() > SHORTCUT_MAX_STRING_LEN) {
        return fail(QStringLiteral("oversized friendly"));
    }
    if (compFriendly.size() > SHORTCUT_MAX_STRING_LEN) {
        return fail(QStringLiteral("oversized component friendly"));
    }
    if (contextUnique.size() > SHORTCUT_MAX_STRING_LEN) {
        return fail(QStringLiteral("oversized context unique"));
    }
    if (contextFriendly.size() > SHORTCUT_MAX_STRING_LEN) {
        return fail(QStringLiteral("oversized context friendly"));
    }
    // Keys strict, each elementary range failure distinct via the single
    // ordered classifier (too-many, then negative, then oversized).
    switch (classifyKeyList(active)) {
    case KeyListFailure::TooMany:
        return fail(QStringLiteral("too many active keys"));
    case KeyListFailure::Negative:
        return fail(QStringLiteral("negative active key"));
    case KeyListFailure::Oversized:
        return fail(QStringLiteral("oversized active key"));
    case KeyListFailure::None:
        break;
    }
    switch (classifyKeyList(defaults)) {
    case KeyListFailure::TooMany:
        return fail(QStringLiteral("too many default keys"));
    case KeyListFailure::Negative:
        return fail(QStringLiteral("negative default key"));
    case KeyListFailure::Oversized:
        return fail(QStringLiteral("oversized default key"));
    case KeyListFailure::None:
        break;
    }
    return true;
}

bool ShortcutReconciler::appendComponentPath(const QString &path, bool fromArgumentArray, QStringList *parsed,
                                             QString *error)
{
    if (path.isEmpty()) {
        if (error) {
            *error = fromArgumentArray
                ? QStringLiteral("unexpected allComponents reply: empty object path in argument array")
                : QStringLiteral("unexpected allComponents reply: empty object path in typed list");
        }
        return false;
    }
    if (parsed) {
        parsed->append(path);
    }
    return true;
}

bool ShortcutReconciler::checkTupleAppendBound(int size, TupleAppendBound kind, QString *error)
{
    if (size <= SHORTCUT_MAX_TUPLES) {
        return true;
    }
    if (error) {
        switch (kind) {
        case TupleAppendBound::ByKeyWire:
            *error = QStringLiteral("unexpected globalShortcutsByKey reply: too many wire holders");
            break;
        case TupleAppendBound::AllInfosWire:
            *error = QStringLiteral("unexpected allShortcutInfos reply: too many wire tuples");
            break;
        case TupleAppendBound::Collected:
            *error = QStringLiteral("unexpected allShortcutInfos reply: too many collected tuples");
            break;
        }
    }
    return false;
}

bool ShortcutReconciler::checkOccupancyKeyRange(int key, QString *error)
{
    if (key < 0) {
        if (error) {
            *error = QStringLiteral("unexpected globalShortcutsByKey reply: negative occupancy key");
        }
        return false;
    }
    if (key == 0) {
        if (error) {
            *error = QStringLiteral("unexpected globalShortcutsByKey reply: non-positive occupancy key");
        }
        return false;
    }
    if (key > SHORTCUT_MAX_KEY_VALUE) {
        if (error) {
            *error = QStringLiteral("unexpected globalShortcutsByKey reply: oversized occupancy key");
        }
        return false;
    }
    return true;
}

bool ShortcutReconciler::decodeKeySequenceSlots(const QList<int> &slotValues, QKeySequence *out)
{
    if (slotValues.size() != 4) {
        return false;
    }
    for (int slot : slotValues) {
        if (slot < 0 || slot > SHORTCUT_MAX_KEY_VALUE) {
            return false;
        }
    }
    if (out) {
        *out = QKeySequence(slotValues.at(0), slotValues.at(1), slotValues.at(2), slotValues.at(3));
    }
    return true;
}

bool ShortcutReconciler::decodeSetterReplySlotSets(const QList<QList<int>> &slotGroups, QSet<QKeySequence> *out)
{
    if (slotGroups.size() > SHORTCUT_MAX_KEYS_PER_TUPLE) {
        return false;
    }
    QSet<QKeySequence> set;
    for (const QList<int> &group : slotGroups) {
        QKeySequence decoded;
        if (!decodeKeySequenceSlots(group, &decoded)) {
            return false;
        }
        set.insert(decoded);
    }
    if (out) {
        *out = set;
    }
    return true;
}

bool ShortcutReconciler::parseDefaultShortcutKeysReply(QDBusMessage::MessageType replyType,
                                                       const QString &replySignature,
                                                       const QList<QVariant> &replyArgs, QList<int> *defaults,
                                                       QString *error)
{
    auto fail = [&](const QString &message) {
        if (error) {
            *error = message;
        }
        return false;
    };
    // Exact transport only (installed kf6_org.kde.KGlobalAccel.xml:123-127):
    // ReplyMessage with signature "a(ai)" carrying the default key list.
    // Ordered type then signature then arity; any other shape fails closed.
    if (replyType != QDBusMessage::ReplyMessage) {
        return fail(QStringLiteral("unexpected defaultShortcutKeys reply: wrong message type"));
    }
    if (replySignature != QStringLiteral("a(ai)")) {
        return fail(QStringLiteral("unexpected defaultShortcutKeys reply: wrong signature"));
    }
    if (replyArgs.size() != 1) {
        return fail(QStringLiteral("unexpected defaultShortcutKeys reply: wrong arity"));
    }
    const QVariant first = replyArgs.at(0);
    QSet<int> parsed;
    if (first.userType() == qMetaTypeId<QSet<QKeySequence>>()) {
        const QSet<QKeySequence> set = first.value<QSet<QKeySequence>>();
        if (set.size() > SHORTCUT_MAX_KEYS_PER_TUPLE) {
            return fail(QStringLiteral("unexpected defaultShortcutKeys reply: did not return expected keys"));
        }
        parsed = intSetFromKeySet(set);
    } else if (first.userType() == qMetaTypeId<QList<QKeySequence>>()) {
        const QList<QKeySequence> list = first.value<QList<QKeySequence>>();
        if (list.size() > SHORTCUT_MAX_KEYS_PER_TUPLE) {
            return fail(QStringLiteral("unexpected defaultShortcutKeys reply: did not return expected keys"));
        }
        parsed = intSetFromKeyList(list);
    } else if (first.canConvert<QDBusArgument>()) {
        const QDBusArgument arg = first.value<QDBusArgument>();
        if (arg.currentType() != QDBusArgument::ArrayType) {
            return fail(QStringLiteral("unexpected defaultShortcutKeys reply: wrong array framing"));
        }
        QList<QList<int>> slotGroups;
        bool framingOk = true;
        arg.beginArray();
        while (!arg.atEnd() && framingOk) {
            if (arg.currentType() != QDBusArgument::StructureType) {
                framingOk = false;
                break;
            }
            arg.beginStructure();
            if (arg.currentType() != QDBusArgument::ArrayType) {
                framingOk = false;
                arg.endStructure();
                break;
            }
            arg.beginArray();
            QList<int> values;
            int innerCount = 0;
            while (!arg.atEnd()) {
                if (arg.currentType() != QDBusArgument::BasicType) {
                    framingOk = false;
                    break;
                }
                int value = 0;
                arg >> value;
                if (innerCount < 5) {
                    values.append(value);
                }
                ++innerCount;
            }
            if (!framingOk) {
                arg.endArray();
                arg.endStructure();
                break;
            }
            if (innerCount != 4) {
                framingOk = false;
                arg.endArray();
                arg.endStructure();
                break;
            }
            slotGroups.append(values);
            if (slotGroups.size() > SHORTCUT_MAX_KEYS_PER_TUPLE) {
                framingOk = false;
                arg.endArray();
                arg.endStructure();
                break;
            }
            arg.endArray();
            arg.endStructure();
        }
        arg.endArray();
        if (!framingOk) {
            return fail(QStringLiteral("unexpected defaultShortcutKeys reply: did not return expected keys"));
        }
        QSet<QKeySequence> decoded;
        if (!decodeSetterReplySlotSets(slotGroups, &decoded)) {
            return fail(QStringLiteral("unexpected defaultShortcutKeys reply: did not return expected keys"));
        }
        parsed = intSetFromKeySet(decoded);
    } else {
        return fail(QStringLiteral("unexpected defaultShortcutKeys reply: wrong variant shape"));
    }
    const QList<int> out = sortedIntsFromSet(parsed);
    if (!keysValid(out)) {
        return fail(QStringLiteral("unexpected defaultShortcutKeys reply: did not return expected keys"));
    }
    if (defaults) {
        *defaults = out;
    }
    return true;
}

bool ShortcutReconciler::parseSetForeignShortcutKeysReply(QDBusMessage::MessageType replyType,
                                                          const QString &replySignature,
                                                          const QList<QVariant> &replyArgs, QString *error)
{
    auto fail = [&](const QString &message) {
        if (error) {
            *error = message;
        }
        return false;
    };
    // Exact void transport only (installed kf6_org.kde.KGlobalAccel.xml:145-149):
    // ReplyMessage with empty signature and zero args. Any payload fails closed.
    if (replyType != QDBusMessage::ReplyMessage) {
        return fail(QStringLiteral("unexpected setForeignShortcutKeys reply: wrong message type"));
    }
    if (replySignature != QString()) {
        return fail(QStringLiteral("unexpected setForeignShortcutKeys reply: wrong signature"));
    }
    if (!replyArgs.isEmpty()) {
        return fail(QStringLiteral("unexpected setForeignShortcutKeys reply: wrong arity"));
    }
    return true;
}

bool ShortcutReconciler::foreignReadbackMatches(const QList<int> &expected, const QList<int> &actual)
{
    // Void setter has no reply keys: the fresh shortcutKeys readback must
    // confirm the exact set. Order-insensitive set comparison; duplicates
    // collapse as the live QSet reply has no order.
    return QSet<int>(expected.begin(), expected.end()) == QSet<int>(actual.begin(), actual.end());
}

bool ShortcutReconciler::uniqueNameValid(const QString &owner)
{
    // Strict D-Bus unique name: colon followed by dot-separated integers.
    static const QRegularExpression pattern(QStringLiteral("^:[0-9]+(\\.[0-9]+)+$"));
    if (owner.size() > SHORTCUT_MAX_STRING_LEN) {
        return false;
    }
    return pattern.match(owner).hasMatch();
}

bool ShortcutReconciler::introspectionContractValid(const QString &xml)
{
    if (xml.isEmpty() || xml.size() > 1048576) {
        return false;
    }
    // Live Plasma 6.7.4 contract: exactly one
    // org.kde.KGlobalAccel/setShortcutKeys method with four args.
    // Qt D-Bus introspection emits the reply first (document order) with
    // method-level annotations interleaved after their corresponding args:
    //   a(ai) reply (out) + Out0, as actionId (in),
    //   a(ai) keys (in) + In1, u flags (in).
    // The logical in-last order (as, a(ai), u, then a(ai) out, annotations
    // trailing) is also accepted for compatibility; any other permutation
    // is rejected.
    // Omitted input direction is accepted only because the D-Bus
    // introspection default is "in"; the reply must be explicit "out".
    // Proven only as exact method-level annotations
    // org.qtproject.QtDBus.QtTypeName.In1 and ...Out0 with
    // QSet<QKeySequence>: arg-nested annotations are unproven and ignored,
    // and substring key matches (e.g. In10) never count.
    // No substring fallback: the structured parse is authoritative.
    // Extra overloads are ambiguity and fail.
    struct ParsedArg
    {
        QString type;
        QString direction;
    };
    QXmlStreamReader reader(xml);
    bool inTargetInterface = false;
    bool inTargetMethod = false;
    bool inMethodArg = false;
    int methodCount = 0;
    QList<ParsedArg> firstArgs;
    QMap<QString, QString> firstMethodAnnotations;
    QList<ParsedArg> curArgs;
    QMap<QString, QString> curMethodAnnotations;
    ParsedArg curArg;
    while (!reader.atEnd()) {
        reader.readNext();
        if (reader.isStartElement()) {
            if (reader.name() == QStringLiteral("interface")
                && reader.attributes().value(QStringLiteral("name")) == shortcutInterface()) {
                inTargetInterface = true;
            } else if (inTargetInterface && reader.name() == QStringLiteral("method")
                       && reader.attributes().value(QStringLiteral("name")) == shortcutSetMethod()) {
                inTargetMethod = true;
                inMethodArg = false;
                curArgs.clear();
                curMethodAnnotations.clear();
                curArg = ParsedArg{};
            } else if (inTargetMethod && reader.name() == QStringLiteral("arg")) {
                curArg.type = reader.attributes().value(QStringLiteral("type")).toString();
                curArg.direction = reader.attributes().value(QStringLiteral("direction")).toString();
                inMethodArg = true;
            } else if (inTargetMethod && !inMethodArg && reader.name() == QStringLiteral("annotation")) {
                const QString name = reader.attributes().value(QStringLiteral("name")).toString();
                const QString value = reader.attributes().value(QStringLiteral("value")).toString();
                curMethodAnnotations.insert(name, value);
            }
        } else if (reader.isEndElement()) {
            if (reader.name() == QStringLiteral("arg") && inTargetMethod && inMethodArg) {
                curArgs.append(curArg);
                curArg = ParsedArg{};
                inMethodArg = false;
            } else if (reader.name() == QStringLiteral("method") && inTargetMethod) {
                ++methodCount;
                if (methodCount == 1) {
                    firstArgs = curArgs;
                    firstMethodAnnotations = curMethodAnnotations;
                }
                inTargetMethod = false;
                inMethodArg = false;
            } else if (reader.name() == QStringLiteral("interface") && inTargetInterface) {
                inTargetInterface = false;
            }
        }
    }
    if (reader.hasError()) {
        return false;
    }
    if (methodCount != 1) {
        return false;
    }
    if (firstArgs.size() != 4) {
        return false;
    }
    auto isIn = [](const QString &direction) {
        return direction == QStringLiteral("in") || direction.isEmpty();
    };
    auto isOut = [](const QString &direction) {
        return direction == QStringLiteral("out");
    };
    // D-Bus default direction is "in", so an omitted input direction is
    // that default; anything else on an input is malformed. The reply
    // must be explicit "out". Exactly two document orders are accepted:
    // Qt out-first (live) and logical in-last (compatible).
    const bool inLastOrder = firstArgs.at(0).type == QStringLiteral("as") && isIn(firstArgs.at(0).direction)
        && firstArgs.at(1).type == QStringLiteral("a(ai)") && isIn(firstArgs.at(1).direction)
        && firstArgs.at(2).type == QStringLiteral("u") && isIn(firstArgs.at(2).direction)
        && firstArgs.at(3).type == QStringLiteral("a(ai)") && isOut(firstArgs.at(3).direction);
    const bool outFirstOrder = firstArgs.at(0).type == QStringLiteral("a(ai)") && isOut(firstArgs.at(0).direction)
        && firstArgs.at(1).type == QStringLiteral("as") && isIn(firstArgs.at(1).direction)
        && firstArgs.at(2).type == QStringLiteral("a(ai)") && isIn(firstArgs.at(2).direction)
        && firstArgs.at(3).type == QStringLiteral("u") && isIn(firstArgs.at(3).direction);
    if (!inLastOrder && !outFirstOrder) {
        return false;
    }
    auto hasExactKeySet = [&](const QString &name) {
        const auto it = firstMethodAnnotations.constFind(name);
        return it != firstMethodAnnotations.constEnd() && *it == QStringLiteral("QSet<QKeySequence>");
    };
    if (!hasExactKeySet(QStringLiteral("org.qtproject.QtDBus.QtTypeName.In1"))
        || !hasExactKeySet(QStringLiteral("org.qtproject.QtDBus.QtTypeName.Out0"))) {
        return false;
    }
    return true;
}

bool ShortcutReconciler::parseAllComponentsReply(QDBusMessage::MessageType replyType, const QString &replySignature,
                                                 const QList<QVariant> &replyArgs, QStringList *components,
                                                 QString *error)
{
    auto fail = [&](const QString &message) {
        if (error) {
            *error = message;
        }
        return false;
    };
    // Exact transport only: a ReplyMessage with signature "ao" carrying an
    // object-path array. Both Qt representations of the exact ao array are
    // accepted: a read-mode QDBusArgument array of object paths (as produced
    // by live D-Bus demarshalling) or a typed QList<QDBusObjectPath>.
    // QStringList ("as") or any other shape is rejected; no fallback.
    if (replyType != QDBusMessage::ReplyMessage) {
        return fail(QStringLiteral("unexpected allComponents reply: wrong message type"));
    }
    if (replySignature != QStringLiteral("ao")) {
        return fail(QStringLiteral("unexpected allComponents reply: wrong signature"));
    }
    if (replyArgs.size() != 1) {
        return fail(QStringLiteral("unexpected allComponents reply: wrong arity"));
    }
    const QVariant first = replyArgs.at(0);
    QStringList parsed;
    if (first.userType() == qMetaTypeId<QList<QDBusObjectPath>>()) {
        const QList<QDBusObjectPath> paths = first.value<QList<QDBusObjectPath>>();
        for (const QDBusObjectPath &path : paths) {
            QString pathError;
            if (!appendComponentPath(path.path(), false, &parsed, &pathError)) {
                return fail(pathError);
            }
        }
    } else if (first.canConvert<QDBusArgument>()) {
        const QDBusArgument arg = first.value<QDBusArgument>();
        if (arg.currentType() != QDBusArgument::ArrayType) {
            return fail(QStringLiteral("unexpected allComponents reply: wrong array framing"));
        }
        arg.beginArray();
        while (!arg.atEnd()) {
            QDBusObjectPath path;
            arg >> path;
            QString pathError;
            if (!appendComponentPath(path.path(), true, &parsed, &pathError)) {
                return fail(pathError);
            }
        }
        arg.endArray();
    } else {
        return fail(QStringLiteral("unexpected allComponents reply: wrong variant shape"));
    }
    if (parsed.size() > 1024) {
        return fail(QStringLiteral("unexpected allComponents reply: too many components"));
    }
    if (components) {
        *components = parsed;
    }
    return true;
}

QList<int> ShortcutReconciler::relevantConflictKeys()
{
    QList<int> out;
    for (const ShortcutConflictRow &row : shortcutConflictTable()) {
        for (int key : row.projectPost) {
            if (!out.contains(key)) {
                out.append(key);
            }
        }
        for (int key : row.resolutionTarget) {
            if (!out.contains(key)) {
                out.append(key);
            }
        }
        for (int key : row.foreignExpectedPre) {
            if (!out.contains(key)) {
                out.append(key);
            }
        }
    }
    return out;
}

QList<int> ShortcutReconciler::enabledRequiredKeys(const QSet<QString> &disabledIds)
{
    return enabledRequiredKeys(disabledIds, true, QList<int>());
}

QList<int> ShortcutReconciler::keepKeysForImages(const QList<QList<int>> &liveImages,
                                                 const QSet<QString> &disabledIds, bool authenticStaged)
{
    // Live Keep chords in catalog order from an explicit per-catalog active
    // image. Bounded: at most SHORTCUT_MAX_TUPLES keys even from a forged
    // image; forged content still fails the full live revalidation later.
    QList<int> out;
    if (authenticStaged) {
        return out;
    }
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    const int rows = qMin(catalog.size(), liveImages.size());
    for (int i = 0; i < rows; ++i) {
        const ShortcutCatalogEntry &entry = catalog.at(i);
        if (disabledIds.contains(shortcutCatalogId(entry.component, entry.action))) {
            continue;
        }
        for (int key : liveImages.at(i)) {
            if (out.size() >= SHORTCUT_MAX_TUPLES) {
                return out;
            }
            out.append(key);
        }
    }
    return out;
}

QList<int> ShortcutReconciler::enabledRequiredKeys(const QSet<QString> &disabledIds, bool authenticStaged,
                                                  const QList<int> &keepKeys)
{
    QList<int> out;
    if (authenticStaged) {
        for (const ShortcutCatalogEntry &entry : shortcutProjectCatalog()) {
            if (disabledIds.contains(shortcutCatalogId(entry.component, entry.action))) {
                continue;
            }
            if (entry.canonicalKey == 0) {
                continue;
            }
            if (!out.contains(entry.canonicalKey)) {
                out.append(entry.canonicalKey);
            }
        }
    } else {
        for (int key : keepKeys) {
            if (key != 0 && !out.contains(key)) {
                out.append(key);
            }
        }
    }
    // The Lock Session relocation chords stay required only while focus-right
    // itself is Authentic-enabled; a Keep or disabled focus-right never
    // scans Meta+Esc.
    const bool focusEnabled =
        !disabledIds.contains(shortcutCatalogId(shortcutFocusComponent(), shortcutFocusAction()));
    if (focusEnabled && authenticStaged) {
        const ShortcutConflictRow &lockRow = shortcutConflictTable().at(0);
        for (int key : lockRow.resolutionTarget) {
            if (!out.contains(key)) {
                out.append(key);
            }
        }
        for (int key : lockRow.foreignExpectedPre) {
            if (!out.contains(key)) {
                out.append(key);
            }
        }
    }
    return out;
}

QString ShortcutReconciler::keyDisplayName(int key)
{
    if (key == 0) {
        return QStringLiteral("none");
    }
    for (const ShortcutCatalogEntry &entry : shortcutProjectCatalog()) {
        if (entry.canonicalKey == key && !entry.canonicalDisplay.isEmpty()) {
            return entry.canonicalDisplay;
        }
    }
    for (const ShortcutConflictRow &row : shortcutConflictTable()) {
        if (row.projectPost.contains(key) && !row.projectDisplay.isEmpty()) {
            return row.projectDisplay;
        }
        if (row.resolutionTarget.contains(key) && !row.targetDisplay.isEmpty()) {
            return row.targetDisplay;
        }
    }
    for (const ShortcutConflictRow &row : shortcutConflictTable()) {
        if (row.foreignExpectedPre.contains(key) && !row.projectDisplay.isEmpty()) {
            return row.projectDisplay;
        }
    }
    // Custom/unrelated chords still render readably through QKeySequence;
    // only an invalid or empty rendering falls back to the raw key int.
    const QString portable = QKeySequence(key, 0, 0, 0).toString(QKeySequence::PortableText);
    if (!portable.isEmpty()) {
        return portable;
    }
    return QStringLiteral("key %1").arg(key);
}

QString ShortcutReconciler::keysDisplay(const QList<int> &keys)
{
    if (keys.isEmpty()) {
        return QStringLiteral("none");
    }
    QStringList parts;
    parts.reserve(keys.size());
    for (int key : keys) {
        parts.append(QString::number(key));
    }
    return parts.join(QStringLiteral(","));
}

QString ShortcutReconciler::keysDisplayNames(const QList<int> &keys)
{
    if (keys.isEmpty()) {
        return QStringLiteral("none");
    }
    QStringList parts;
    parts.reserve(keys.size());
    for (int key : keys) {
        parts.append(keyDisplayName(key));
    }
    return parts.join(QStringLiteral(", "));
}

bool ShortcutReconciler::disabledIdsValid(const QSet<QString> &disabledIds, QString *error)
{
    for (const QString &id : disabledIds) {
        bool found = false;
        const int slash = id.indexOf(QChar::fromLatin1('/'));
        if (slash > 0) {
            const QString component = id.left(slash);
            const QString action = id.mid(slash + 1);
            if (!component.isEmpty() && !action.isEmpty() && shortcutCatalogContains(component, action)) {
                found = true;
            }
        }
        if (!found) {
            if (error) {
                *error = QStringLiteral("unknown disabled binding; refusing apply");
            }
            return false;
        }
    }
    return true;
}

QStringList ShortcutReconciler::foreignDefaultIdsForKey(int key, const QList<ShortcutTuple> &tuples)
{
    QStringList out;
    if (key == 0) {
        return out;
    }
    for (const ShortcutTuple &tuple : tuples) {
        if (isProjectAction(tuple.component, tuple.action) || isLockAction(tuple.component, tuple.action)) {
            continue;
        }
        if (tuple.defaults.contains(key)) {
            const QString id = shortcutCatalogId(tuple.component, tuple.action);
            if (!out.contains(id)) {
                out.append(id);
            }
        }
    }
    out.sort();
    return out;
}

bool ShortcutReconciler::isAuthorizedDisplacement(int key, const QString &component, const QString &action)
{
    // Row-owned target exception: never part of the write allowlist and
    // never a permission to rebind System Monitor itself.
    for (const ShortcutConflictRow &row : shortcutConflictTable()) {
        if (row.resolutionTarget.contains(key) && component == row.authorizedTargetComponent
            && action == row.authorizedTargetAction) {
            return true;
        }
    }
    return false;
}

bool ShortcutReconciler::isHolderExempt(const QString &component, const QString &action, int key)
{
    // Single shared policy for the status keyed check and the backend
    // holder scan. Project rows and Lock Session own their chords; the
    // authorized System Monitor Meta+Esc displacement is user-approved.
    // Known compiled foreign holders (Grid View, Switcher, Monocle) and
    // unknown holders are never exempt.
    if (isProjectAction(component, action) || isLockAction(component, action)) {
        return true;
    }
    return isAuthorizedDisplacement(key, component, action);
}

bool ShortcutReconciler::holdersFromInfoFields(const QList<ShortcutInfoFields> &infos,
                                               QList<ShortcutKeyHolder> *holders, QString *error)
{
    auto fail = [&](const QString &message) {
        if (error) {
            *error = message;
        }
        return false;
    };
    QList<ShortcutKeyHolder> parsed;
    for (const ShortcutInfoFields &info : infos) {
        // Shared pure field validator: identity strict, cosmetic allows
        // empty with length bound, keys strict.
        QString fieldError;
        if (!keyedFieldsValid(info.action, info.friendly, info.compUnique, info.compFriendly, info.contextUnique,
                               info.contextFriendly, info.active, info.defaults, &fieldError)) {
            return fail(QStringLiteral("unexpected globalShortcutsByKey reply: ") + fieldError);
        }
        ShortcutKeyHolder holder;
        holder.component = info.compUnique;
        holder.action = info.action;
        holder.active = info.active;
        holder.defaults = info.defaults;
        holder.componentFriendly = info.compFriendly;
        holder.friendly = info.friendly;
        parsed.append(holder);
        if (parsed.size() > SHORTCUT_MAX_TUPLES) {
            return fail(QStringLiteral("unexpected globalShortcutsByKey reply: too many holders"));
        }
    }
    if (holders) {
        *holders = parsed;
    }
    return true;
}

bool ShortcutReconciler::tuplesFromInfoFields(const QList<ShortcutInfoFields> &infos, QList<ShortcutTuple> *tuples,
                                              QString *error)
{
    auto fail = [&](const QString &message) {
        if (error) {
            *error = message;
        }
        return false;
    };
    QList<ShortcutTuple> parsed;
    for (const ShortcutInfoFields &info : infos) {
        QString fieldError;
        if (!keyedFieldsValid(info.action, info.friendly, info.compUnique, info.compFriendly, info.contextUnique,
                               info.contextFriendly, info.active, info.defaults, &fieldError)) {
            return fail(QStringLiteral("unexpected allShortcutInfos reply: ") + fieldError);
        }
        ShortcutTuple tuple;
        tuple.component = info.compUnique;
        tuple.action = info.action;
        tuple.componentFriendly = info.compFriendly;
        tuple.friendly = info.friendly;
        tuple.active = info.active;
        tuple.defaults = info.defaults;
        parsed.append(tuple);
        if (parsed.size() > SHORTCUT_MAX_TUPLES) {
            return fail(QStringLiteral("unexpected allShortcutInfos reply: too many tuples"));
        }
    }
    if (tuples) {
        *tuples = parsed;
    }
    return true;
}

bool ShortcutReconciler::parseGlobalShortcutsByKeyReply(QDBusMessage::MessageType replyType,
                                                        const QString &replySignature,
                                                        const QList<QVariant> &replyArgs,
                                                        QList<ShortcutKeyHolder> *holders, QString *error)
{
    auto fail = [&](const QString &message) {
        if (error) {
            *error = message;
        }
        return false;
    };
    // Exact transport only: ReplyMessage with signature "a(ssssssaiai)"
    // carrying KGlobalShortcutInfo structs (6 strings + ai + ai). Any other
    // type, signature, arity, or shape fails closed; no fallback.
    if (replyType != QDBusMessage::ReplyMessage) {
        return fail(QStringLiteral("unexpected globalShortcutsByKey reply: wrong message type"));
    }
    if (replySignature != QStringLiteral("a(ssssssaiai)")) {
        return fail(QStringLiteral("unexpected globalShortcutsByKey reply: wrong signature"));
    }
    if (replyArgs.size() != 1) {
        return fail(QStringLiteral("unexpected globalShortcutsByKey reply: wrong arity"));
    }
    const QVariant first = replyArgs.at(0);
    if (!first.canConvert<QDBusArgument>()) {
        return fail(QStringLiteral("unexpected globalShortcutsByKey reply: wrong variant shape"));
    }
    // Const read-mode access: non-const beginStructure/endStructure would
    // select the write overloads and fail on a read-only argument.
    const QDBusArgument arg = first.value<QDBusArgument>();
    if (arg.currentType() != QDBusArgument::ArrayType) {
        return fail(QStringLiteral("unexpected globalShortcutsByKey reply: wrong array framing"));
    }
    QList<ShortcutInfoFields> infos;
    arg.beginArray();
    while (!arg.atEnd()) {
        ShortcutInfoFields info;
        arg >> info;
        infos.append(info);
        // Fail-fast wire bound before further payload allocation: shared
        // predicate retains the exact wire token for direct use.
        QString boundError;
        if (!checkTupleAppendBound(infos.size(), TupleAppendBound::ByKeyWire, &boundError)) {
            return fail(boundError);
        }
    }
    arg.endArray();
    return holdersFromInfoFields(infos, holders, error);
}

bool ShortcutReconciler::parseGlobalShortcutAvailableReply(QDBusMessage::MessageType replyType,
                                                           const QString &replySignature,
                                                           const QList<QVariant> &replyArgs, bool *available,
                                                           QString *error)
{
    auto fail = [&](const QString &message) {
        if (error) {
            *error = message;
        }
        return false;
    };
    // Exact transport only: ReplyMessage with signature "b" carrying one bool.
    if (replyType != QDBusMessage::ReplyMessage) {
        return fail(QStringLiteral("unexpected globalShortcutAvailable reply: wrong message type"));
    }
    if (replySignature != QStringLiteral("b")) {
        return fail(QStringLiteral("unexpected globalShortcutAvailable reply: wrong signature"));
    }
    if (replyArgs.size() != 1) {
        return fail(QStringLiteral("unexpected globalShortcutAvailable reply: wrong arity"));
    }
    const QVariant first = replyArgs.at(0);
    if (first.userType() != QMetaType::Bool) {
        return fail(QStringLiteral("unexpected globalShortcutAvailable reply: wrong variant shape"));
    }
    if (available) {
        *available = first.toBool();
    }
    return true;
}

KeyedOccupancyResult ShortcutReconciler::checkKeyedForeignOccupancyDetailed(ShortcutStore *store)
{
    // Legacy entry point: full catalog with an empty (Authentic) draft.
    return checkKeyedForeignOccupancyDetailedFor(store, QSet<QString>());
}

KeyedOccupancyResult ShortcutReconciler::checkKeyedForeignOccupancyDetailedFor(ShortcutStore *store,
                                                                              const QSet<QString> &disabledIds)
{
    return checkKeyedForeignOccupancyDetailedFor(store, disabledIds, true);
}

KeyedOccupancyResult ShortcutReconciler::checkKeyedForeignOccupancyDetailedFor(ShortcutStore *store,
                                                                              const QSet<QString> &disabledIds,
                                                                              bool authenticStaged)
{
    KeyedOccupancyResult result;
    result.status = KeyedOccupancy::Clear;
    auto unavailable = [&](const QString &message) {
        result.status = KeyedOccupancy::Unavailable;
        result.detail = message;
        return result;
    };
    auto conflict = [&](const QString &message) {
        result.status = KeyedOccupancy::Conflict;
        result.detail = message;
        return result;
    };
    if (!store) {
        return unavailable(QStringLiteral("reconciler is not configured"));
    }
    // Single reconciler-owned collection/ordering below the UI: the same
    // holder snapshot (sorted blockers then sorted rows) drives both this
    // status check and the apply refusal, so the first conflict matches.
    ShortcutReconciler reconciler(store, nullptr);
    HolderSnapshot snap;
    QString error;
    if (!reconciler.collectHolderSnapshotFor(&snap, disabledIds, authenticStaged, &error)) {
        return unavailable(error.isEmpty() ? QStringLiteral("allShortcutInfos call failed") : error);
    }
    if (!snap.blocked.isEmpty()) {
        const Blocker &first = snap.blocked.first();
        return conflict(QStringLiteral("refusing to apply: %1 is claimed by %2/%3")
                            .arg(keyDisplayName(first.key), first.component, first.action));
    }
    if (!snap.rows.isEmpty()) {
        const ClearRow &first = snap.rows.first();
        const int key = first.removals.isEmpty() ? 0 : first.removals.first();
        if (key == 0) {
            return unavailable(QStringLiteral("holder state is inconsistent"));
        }
        return conflict(QStringLiteral("refusing to apply: %1 is claimed by %2/%3")
                            .arg(keyDisplayName(key), first.component, first.action));
    }
    return result;
}

bool ShortcutReconciler::checkKeyedForeignOccupancy(ShortcutStore *store, QString *error)
{
    const KeyedOccupancyResult detailed = checkKeyedForeignOccupancyDetailed(store);
    if (detailed.status != KeyedOccupancy::Clear) {
        if (error) {
            *error = detailed.detail;
        }
        return false;
    }
    return true;
}

bool ShortcutReconciler::collectRowDisplays(ShortcutStore *store, QList<ShortcutRowDisplay> *rows, QString *error)
{
    if (!store) {
        if (error) {
            *error = QStringLiteral("reconciler is not configured");
        }
        return false;
    }
    if (!store->checkSetterContract(error)) {
        return false;
    }
    QString owner;
    uint uid = 0;
    if (!store->currentOwner(&owner, &uid, error)) {
        return false;
    }
    QList<ShortcutTuple> tuples;
    if (!store->readAll(&tuples, error)) {
        return false;
    }
    QList<ShortcutRowDisplay> out;
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    out.reserve(catalog.size());
    for (const ShortcutCatalogEntry &entry : catalog) {
        ShortcutRowDisplay row;
        row.catalog = entry;
        for (const ShortcutTuple &tuple : tuples) {
            if (tuple.component == entry.component && tuple.action == entry.action) {
                if (row.present) {
                    if (error) {
                        *error = QStringLiteral("allowlisted tuple %1/%2 has multiple records, expected exactly one")
                                     .arg(entry.component, entry.action);
                    }
                    return false;
                }
                row.present = true;
                row.current = tuple.active;
                row.projectDefaults = tuple.defaults;
            }
        }
        // Current holders via the authoritative keyed queries: the canonical
        // chord plus the live actual chords, so a Keep-customized row reports
        // its actual conflicts instead of only the canonical image. A failed
        // query marks this row unknown; other rows still report. Unbound
        // rows (key 0) with an empty assignment never query: they report
        // empty holders as known so the existing KCM rebinding route stays
        // usable with an empty default.
        QList<int> queryKeys;
        if (entry.canonicalKey != 0) {
            queryKeys.append(entry.canonicalKey);
        }
        if (row.present) {
            for (int key : row.current) {
                if (key != 0 && !queryKeys.contains(key)) {
                    queryKeys.append(key);
                }
            }
        }
        QList<ShortcutKeyHolder> holders;
        QString holderError;
        if (queryKeys.isEmpty()) {
            row.holdersKnown = true;
            row.holders = holders;
        } else {
            QSet<QString> seenHolderIds;
            bool queriesOk = true;
            for (int queryKey : queryKeys) {
                QList<ShortcutKeyHolder> one;
                if (!store->shortcutsByKey(queryKey, &one, &holderError)) {
                    queriesOk = false;
                    break;
                }
                for (const ShortcutKeyHolder &holder : one) {
                    const QString id = shortcutCatalogId(holder.component, holder.action);
                    if (!seenHolderIds.contains(id)) {
                        seenHolderIds.insert(id);
                        holders.append(holder);
                    }
                }
            }
            row.holdersKnown = queriesOk;
            if (queriesOk) {
                // Reconciler-owned deterministic order: every current holder
                // appears sorted by component/action so status, error, and
                // row text share one ordering with the snapshot scan.
                std::sort(holders.begin(), holders.end(),
                          [](const ShortcutKeyHolder &a, const ShortcutKeyHolder &b) {
                              if (a.component != b.component) {
                                  return a.component < b.component;
                              }
                              return a.action < b.action;
                          });
                row.holders = holders;
            }
        }
        // Known KDE default for the compiled rows, read live so a cleared
        // foreign holder still shows its default. Failures mark the known
        // default unknown rather than inventing one.
        if (!entry.knownForeignComponent.isEmpty()) {
            QList<int> defaults;
            QString defaultError;
            ShortcutTuple foreignCurrent;
            bool foreignFound = false;
            for (const ShortcutTuple &tuple : tuples) {
                if (tuple.component == entry.knownForeignComponent
                    && tuple.action == entry.knownForeignAction) {
                    foreignCurrent = tuple;
                    foreignFound = true;
                    break;
                }
            }
            if (foreignFound
                && store->defaultShortcutKeys(entry.knownForeignComponent, entry.knownForeignAction,
                                              foreignCurrent.componentFriendly, foreignCurrent.friendly, &defaults,
                                              &defaultError)) {
                row.knownDefaults = defaults;
                row.defaultsKnown = true;
            } else {
                row.defaultsKnown = false;
            }
        } else {
            row.defaultsKnown = true;
            row.knownDefaults = QList<int>();
        }
        // Live foreign wire defaults from readAll for every queried chord
        // (canonical plus live actuals; compiled table stays the fallback
        // above). Own project defaults and Lock Session are never listed.
        QStringList foreignIds = foreignDefaultIdsForKey(entry.canonicalKey, tuples);
        if (row.present) {
            for (int key : row.current) {
                if (key == 0 || key == entry.canonicalKey) {
                    continue;
                }
                for (const QString &id : foreignDefaultIdsForKey(key, tuples)) {
                    if (!foreignIds.contains(id)) {
                        foreignIds.append(id);
                    }
                }
            }
        }
        foreignIds.sort();
        row.foreignDefaultIds = foreignIds;
        row.foreignDefaultsKnown = true;
        out.append(row);
    }
    if (rows) {
        *rows = out;
    }
    return true;
}

bool ShortcutReconciler::clearedActionsPathSafe(const QString &path, QString *error)
{
    if (path.isEmpty()) {
        if (error) {
            *error = QStringLiteral("cleared shortcut path is empty; refusing CWD fallback");
        }
        return false;
    }
    if (!QDir::isAbsolutePath(path)) {
        if (error) {
            *error = QStringLiteral("cleared shortcut path must be absolute; refusing CWD fallback");
        }
        return false;
    }
    // Refuse symlink or nonregular leaf without following it.
    {
        struct stat st = {};
        const QByteArray encoded = path.toLocal8Bit();
        if (::lstat(encoded.constData(), &st) == 0) {
            if (S_ISLNK(st.st_mode)) {
                if (error) {
                    *error = QStringLiteral("cleared shortcut path must not be a symlink");
                }
                return false;
            }
            if (!S_ISREG(st.st_mode)) {
                if (error) {
                    *error = QStringLiteral("cleared shortcut path must be a regular file");
                }
                return false;
            }
            if (st.st_uid != static_cast<uid_t>(::geteuid())) {
                if (error) {
                    *error = QStringLiteral("cleared shortcut file is not owned by this user");
                }
                return false;
            }
            if ((st.st_mode & 0077) != 0) {
                if (error) {
                    *error = QStringLiteral("cleared shortcut file is not private");
                }
                return false;
            }
        }
    }
    // The direct parent directory is the writable trust boundary. It cannot
    // be foreign-owned or writable by group/other; all ancestors are still
    // checked for symlinks below.
    QDir parent = QFileInfo(path).dir();
    {
        struct stat st = {};
        const QByteArray encoded = parent.path().toLocal8Bit();
        if (::lstat(encoded.constData(), &st) == 0) {
            if (S_ISLNK(st.st_mode) || !S_ISDIR(st.st_mode) || st.st_uid != static_cast<uid_t>(::geteuid())
                || (st.st_mode & 0022) != 0) {
                if (error) {
                    *error = QStringLiteral("cleared shortcut parent directory is unsafe");
                }
                return false;
            }
        } else if (errno != ENOENT) {
            if (error) {
                *error = QStringLiteral("cleared shortcut parent directory is unavailable");
            }
            return false;
        }
    }
    // Refuse unsafe symlink in any parent component (no following).
    while (!parent.path().isEmpty() && parent.path() != QStringLiteral("/") && parent.path() != QStringLiteral(".")) {
        struct stat st = {};
        const QByteArray encoded = parent.path().toLocal8Bit();
        if (::lstat(encoded.constData(), &st) == 0) {
            if (S_ISLNK(st.st_mode)) {
                if (error) {
                    *error = QStringLiteral("cleared shortcut parent path must not contain a symlink");
                }
                return false;
            }
        }
        const QString next = QFileInfo(parent.path()).dir().path();
        if (next == parent.path()) {
            break;
        }
        parent = QDir(next);
    }
    return true;
}

bool KGlobalAccelStore::checkSetterContract(QString *error)
{
    // Introspection-proven exact setter contract on live Plasma 6.7.4:
    // method setShortcutKeys with split args as,a(ai),u -> a(ai) where the
    // keys input and the reply carry QSet<QKeySequence> annotations.
    QDBusMessage call = QDBusMessage::createMethodCall(
        shortcutService(), shortcutPath(), QStringLiteral("org.freedesktop.DBus.Introspectable"), QStringLiteral("Introspect"));
    const QDBusMessage reply = QDBusConnection::sessionBus().call(call);
    if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().size() != 1) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel introspection did not return a strict reply");
        }
        return false;
    }
    const QString xml = reply.arguments().at(0).toString();
    if (!ShortcutReconciler::introspectionContractValid(xml)) {
        if (error) {
            *error = QStringLiteral(
                "KGlobalAccel setShortcutKeys is absent or does not expose exactly as,a(ai),u -> a(ai) with QSet<QKeySequence>");
        }
        return false;
    }
    return true;
}

bool KGlobalAccelStore::tryPinOwner(QString &pinned, const QString &candidate, QString *error)
{
    if (!ShortcutReconciler::uniqueNameValid(candidate)) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel owner is not a unique name");
        }
        return false;
    }
    if (pinned.isEmpty()) {
        pinned = candidate;
        return true;
    }
    if (pinned != candidate) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel service owner drifted");
        }
        return false;
    }
    return true;
}

bool KGlobalAccelStore::resolveOwnerReply(bool ownerValid, const QString &ownerValue, bool uidValid, uint uidValue,
                                           QString &pinned, QString *ownerOut, uint *uidOut, QString *error)
{
    if (!ownerValid) {
        if (error) {
            *error = QStringLiteral("malformed KGlobalAccel service owner reply");
        }
        return false;
    }
    if (!ShortcutReconciler::uniqueNameValid(ownerValue)) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel owner is not a unique name");
        }
        return false;
    }
    if (!uidValid) {
        if (error) {
            *error = QStringLiteral("malformed KGlobalAccel owner UID reply");
        }
        return false;
    }
    // Verified pin is immutable: the first verified unique owner sets it,
    // the same owner confirms it, and a subsequent different owner fails
    // closed without clobbering the pin.
    if (!tryPinOwner(pinned, ownerValue, error)) {
        return false;
    }
    if (ownerOut) {
        *ownerOut = ownerValue;
    }
    if (uidOut) {
        *uidOut = uidValue;
    }
    return true;
}

bool KGlobalAccelStore::checkPinnedDrift(bool liveValid, const QString &liveValue, const QString &pinned,
                                         QString *error)
{
    if (!liveValid || liveValue != pinned) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel service owner drifted");
        }
        return false;
    }
    return true;
}

bool KGlobalAccelStore::currentOwner(QString *owner, uint *uid, QString *error)
{
    // Robust daemon owner resolution without dynamic QDBusInterface: Qt 6
    // dynamic QDBusInterface performs owner tracking/introspection at
    // construction, which fails implicit owner resolution against the
    // current bus daemon. QDBusConnection::interface()->serviceOwner/
    // serviceUid avoids that construction entirely.
    const QDBusConnection connection = QDBusConnection::sessionBus();
    if (!connection.isConnected()) {
        if (error) {
            *error = QStringLiteral("D-Bus daemon interface is invalid");
        }
        return false;
    }
    QDBusConnectionInterface *bus = connection.interface();
    if (!bus) {
        if (error) {
            *error = QStringLiteral("D-Bus daemon interface is invalid");
        }
        return false;
    }
    const QDBusReply<QString> nameOwner = bus->serviceOwner(shortcutService());
    const bool ownerValid = nameOwner.isValid();
    const QString ownerValue = ownerValid ? nameOwner.value() : QString();
    bool uidValid = false;
    uint uidValue = 0;
    if (ownerValid && ShortcutReconciler::uniqueNameValid(ownerValue)) {
        const QDBusReply<uint> ownerUid = bus->serviceUid(ownerValue);
        uidValid = ownerUid.isValid();
        if (uidValid) {
            uidValue = ownerUid.value();
        }
    }
    return resolveOwnerReply(ownerValid, ownerValue, uidValid, uidValue, m_pinnedOwner, owner, uid, error);
}

bool ShortcutReconciler::parseAllShortcutInfosReply(QDBusMessage::MessageType replyType,
                                                     const QString &replySignature,
                                                     const QList<QVariant> &replyArgs,
                                                     QList<ShortcutTuple> *tuples, QString *error)
{
    auto fail = [&](const QString &message) {
        if (error) {
            *error = message;
        }
        return false;
    };
    // Exact transport only: ReplyMessage with signature "a(ssssssaiai)"
    // carrying KGlobalShortcutInfo structs (6 strings + ai + ai). Ordered
    // type then signature then arity; any other shape fails closed.
    if (replyType != QDBusMessage::ReplyMessage) {
        return fail(QStringLiteral("unexpected allShortcutInfos reply: wrong message type"));
    }
    if (replySignature != QStringLiteral("a(ssssssaiai)")) {
        return fail(QStringLiteral("unexpected allShortcutInfos reply: wrong signature"));
    }
    if (replyArgs.size() != 1) {
        return fail(QStringLiteral("unexpected allShortcutInfos reply: wrong arity"));
    }
    const QVariant first = replyArgs.at(0);
    if (!first.canConvert<QDBusArgument>()) {
        return fail(QStringLiteral("unexpected allShortcutInfos reply: wrong variant shape"));
    }
    const QDBusArgument arg = first.value<QDBusArgument>();
    if (arg.currentType() != QDBusArgument::ArrayType) {
        return fail(QStringLiteral("unexpected allShortcutInfos reply: wrong array framing"));
    }
    QList<ShortcutInfoFields> infos;
    arg.beginArray();
    while (!arg.atEnd()) {
        ShortcutInfoFields info;
        arg >> info;
        infos.append(info);
        // Fail-fast wire bound before further payload allocation: shared
        // predicate retains the exact wire token for direct use.
        QString boundError;
        if (!checkTupleAppendBound(infos.size(), TupleAppendBound::AllInfosWire, &boundError)) {
            return fail(boundError);
        }
    }
    arg.endArray();
    return tuplesFromInfoFields(infos, tuples, error);
}

bool KGlobalAccelStore::readAll(QList<ShortcutTuple> *tuples, QString *error)
{
    if (!tuples) {
        return false;
    }
    // Raw method calls without dynamic QDBusInterface construction: the
    // Qt 6 dynamic interface performs owner tracking/introspection at
    // construction and fails implicit owner resolution here.
    if (!QDBusConnection::sessionBus().isConnected()) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel interface is invalid");
        }
        return false;
    }
    const QDBusMessage compsCall = QDBusMessage::createMethodCall(shortcutService(), shortcutPath(),
                                                                 shortcutInterface(), shortcutAllComponentsMethod());
    const QDBusMessage compsReply = QDBusConnection::sessionBus().call(compsCall);
    QStringList components;
    if (!ShortcutReconciler::parseAllComponentsReply(compsReply.type(), compsReply.signature(), compsReply.arguments(),
                                                     &components, error)) {
        return false;
    }
    QList<ShortcutTuple> collected;
    for (const QString &componentPath : components) {
        QDBusMessage infosCall = QDBusMessage::createMethodCall(shortcutService(), componentPath,
                                                               shortcutComponentInterface(), shortcutAllInfosMethod());
        infosCall.setArguments({QStringLiteral("default")});
        const QDBusMessage infosReply = QDBusConnection::sessionBus().call(infosCall);
        QList<ShortcutTuple> batch;
        if (!ShortcutReconciler::parseAllShortcutInfosReply(infosReply.type(), infosReply.signature(),
                                                            infosReply.arguments(), &batch, error)) {
            return false;
        }
        for (const ShortcutTuple &tuple : batch) {
            collected.append(tuple);
            QString boundError;
            if (!ShortcutReconciler::checkTupleAppendBound(collected.size(),
                                                           ShortcutReconciler::TupleAppendBound::Collected,
                                                           &boundError)) {
                if (error) {
                    *error = boundError;
                }
                return false;
            }
        }
    }
    *tuples = collected;
    return true;
}

bool KGlobalAccelStore::shortcutsByKey(int key, QList<ShortcutKeyHolder> *holders, QString *error)
{
    if (!holders) {
        return false;
    }
    // Disjoint single-key guards without duplicate validation: negative,
    // then zero as non-positive, then over-max. List-bound is unreachable
    // for one element; over-max uses the direct bound check.
    if (key < 0) {
        if (error) {
            *error = QStringLiteral("unexpected globalShortcutsByKey reply: negative key");
        }
        return false;
    }
    if (key == 0) {
        if (error) {
            *error = QStringLiteral("unexpected globalShortcutsByKey reply: non-positive key");
        }
        return false;
    }
    if (key > SHORTCUT_MAX_KEY_VALUE) {
        if (error) {
            *error = QStringLiteral("unexpected globalShortcutsByKey reply: oversized key");
        }
        return false;
    }
    if (!QDBusConnection::sessionBus().isConnected()) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel interface is invalid");
        }
        return false;
    }
    ensureKeySequenceMetaTypes();
    const QKeySequence sequence(key, 0, 0, 0);
    const ShortcutMatchType match{SHORTCUT_MATCH_EQUAL};
    QDBusMessage call = QDBusMessage::createMethodCall(shortcutService(), shortcutPath(), shortcutInterface(),
                                                      shortcutByKeyMethod());
    call.setArguments({QVariant::fromValue(sequence), QVariant::fromValue(match)});
    const QDBusMessage reply = QDBusConnection::sessionBus().call(call);
    return ShortcutReconciler::parseGlobalShortcutsByKeyReply(reply.type(), reply.signature(), reply.arguments(),
                                                              holders, error);
}

bool KGlobalAccelStore::shortcutAvailable(int key, const QString &component, bool *available, QString *error)
{
    if (!available) {
        return false;
    }
    if (key < 0) {
        if (error) {
            *error = QStringLiteral("unexpected globalShortcutAvailable reply: negative key");
        }
        return false;
    }
    if (key == 0) {
        if (error) {
            *error = QStringLiteral("unexpected globalShortcutAvailable reply: non-positive key");
        }
        return false;
    }
    if (key > SHORTCUT_MAX_KEY_VALUE) {
        if (error) {
            *error = QStringLiteral("unexpected globalShortcutAvailable reply: oversized key");
        }
        return false;
    }
    if (component.size() > SHORTCUT_MAX_STRING_LEN) {
        if (error) {
            *error = QStringLiteral("unexpected globalShortcutAvailable reply: oversized component");
        }
        return false;
    }
    if (!QDBusConnection::sessionBus().isConnected()) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel interface is invalid");
        }
        return false;
    }
    ensureKeySequenceMetaTypes();
    const QKeySequence sequence(key, 0, 0, 0);
    QDBusMessage call = QDBusMessage::createMethodCall(shortcutService(), shortcutPath(), shortcutInterface(),
                                                      shortcutAvailableMethod());
    call.setArguments({QVariant::fromValue(sequence), QVariant::fromValue(component)});
    const QDBusMessage reply = QDBusConnection::sessionBus().call(call);
    return ShortcutReconciler::parseGlobalShortcutAvailableReply(reply.type(), reply.signature(), reply.arguments(),
                                                                 available, error);
}

bool KGlobalAccelStore::writeKeys(const QString &component, const QString &action, const QString &componentFriendly,
                                  const QString &friendly, const QList<int> &keys, QList<int> *confirmed, QString *error)
{
    if (!ShortcutReconciler::isAllowlisted(component, action)) {
        if (error) {
            *error = QStringLiteral("refusing write outside the exact allowlist");
        }
        return false;
    }
    if (!ShortcutReconciler::keysValid(keys) || !ShortcutReconciler::stringValid(component) || !ShortcutReconciler::stringValid(action)
        || !ShortcutReconciler::cosmeticValid(componentFriendly) || !ShortcutReconciler::cosmeticValid(friendly)) {
        if (error) {
            *error = QStringLiteral("refusing write with unbounded tuple");
        }
        return false;
    }
    // No lifetime accumulation cap here: each operation carries its own
    // bounded budget enforced by ShortcutReconciler (per-operation deltas
    // from writeCount), so repeated legitimate Apply/Force/Revert stay
    // available while a single operation can never exceed its bound.
    if (m_pinnedOwner.isEmpty() || !ShortcutReconciler::uniqueNameValid(m_pinnedOwner)) {
        if (error) {
            *error = QStringLiteral("refusing write without a pinned KGlobalAccel owner");
        }
        return false;
    }
    // Re-resolve without clobbering the pin: the write must target the
    // verified captured unique owner, never a re-resolved well-known name.
    // Robust daemon query without dynamic QDBusInterface construction.
    {
        const QDBusConnection connection = QDBusConnection::sessionBus();
        if (!connection.isConnected()) {
            if (error) {
                *error = QStringLiteral("D-Bus daemon interface is invalid");
            }
            return false;
        }
        QDBusConnectionInterface *bus = connection.interface();
        if (!bus) {
            if (error) {
                *error = QStringLiteral("D-Bus daemon interface is invalid");
            }
            return false;
        }
        const QDBusReply<QString> liveOwner = bus->serviceOwner(shortcutService());
        const bool liveValid = liveOwner.isValid();
        const QString liveValue = liveValid ? liveOwner.value() : QString();
        if (!checkPinnedDrift(liveValid, liveValue, m_pinnedOwner, error)) {
            return false;
        }
    }
    ensureKeySequenceMetaTypes();
    // Typed QSet<QKeySequence> value: each active int becomes one sequence.
    // The QList target order is intentional (lock-key preservation); the
    // QSet itself is unordered and replies compare as sets.
    const QSet<QKeySequence> keySet = keySetFromInts(keys);
    const QSet<int> expectedSet(keys.begin(), keys.end());
    // Action ID is exactly [ComponentUnique, ActionUnique,
    // ComponentFriendly, ActionFriendly] from the live read tuples.
    const QStringList actionId{component, action, componentFriendly, friendly};
    if (!QDBusConnection::sessionBus().isConnected()) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel interface is invalid");
        }
        return false;
    }
    QDBusMessage setCall = QDBusMessage::createMethodCall(m_pinnedOwner, shortcutPath(), shortcutInterface(),
                                                         shortcutSetMethod());
    setCall.setArguments({actionId, QVariant::fromValue(keySet), QVariant::fromValue(SHORTCUT_SET_FLAGS)});
    const QDBusMessage reply = QDBusConnection::sessionBus().call(setCall);
    if (reply.type() != QDBusMessage::ReplyMessage || reply.arguments().size() != 1
        || reply.signature() != QStringLiteral("a(ai)")) {
        if (error) {
            *error = QStringLiteral("setShortcutKeys reply did not confirm expected key");
        }
        return false;
    }
    QSet<QKeySequence> replySet;
    const QVariant replyVariant = reply.arguments().at(0);
    if (replyVariant.userType() == qMetaTypeId<QSet<QKeySequence>>()) {
        replySet = replyVariant.value<QSet<QKeySequence>>();
        // Same bound as the QDBusArgument framing below.
        if (replySet.size() > SHORTCUT_MAX_KEYS_PER_TUPLE) {
            if (error) {
                *error = QStringLiteral("setShortcutKeys reply did not confirm expected key");
            }
            return false;
        }
    } else if (replyVariant.canConvert<QDBusArgument>()) {
        // Read-mode a(ai) decode: const access selects the read overloads.
        // Guarded array -> structure -> inner array -> basic before each
        // descent/read so a container can never reach operator>>(int&);
        // every unexpected framing fails closed with the bounded error.
        const QDBusArgument arg = replyVariant.value<QDBusArgument>();
        if (arg.currentType() != QDBusArgument::ArrayType) {
            if (error) {
                *error = QStringLiteral("setShortcutKeys reply did not confirm expected key");
            }
            return false;
        }
        QList<QList<int>> slotGroups;
        bool framingOk = true;
        arg.beginArray();
        while (!arg.atEnd() && framingOk) {
            if (arg.currentType() != QDBusArgument::StructureType) {
                framingOk = false;
                break;
            }
            arg.beginStructure();
            if (arg.currentType() != QDBusArgument::ArrayType) {
                framingOk = false;
                arg.endStructure();
                break;
            }
            arg.beginArray();
            QList<int> values;
            int innerCount = 0;
            while (!arg.atEnd()) {
                if (arg.currentType() != QDBusArgument::BasicType) {
                    framingOk = false;
                    break;
                }
                int value = 0;
                arg >> value;
                if (innerCount < 5) {
                    values.append(value);
                }
                ++innerCount;
            }
            if (!framingOk) {
                arg.endArray();
                arg.endStructure();
                break;
            }
            // Bound inner growth before validation: more than five slots is
            // already long; drain state stays consistent via atEnd loop above.
            if (innerCount != 4) {
                framingOk = false;
                arg.endArray();
                arg.endStructure();
                break;
            }
            slotGroups.append(values);
            if (slotGroups.size() > SHORTCUT_MAX_KEYS_PER_TUPLE) {
                framingOk = false;
                arg.endArray();
                arg.endStructure();
                break;
            }
            arg.endArray();
            arg.endStructure();
        }
        arg.endArray();
        if (!framingOk) {
            if (error) {
                *error = QStringLiteral("setShortcutKeys reply did not confirm expected key");
            }
            return false;
        }
        if (!ShortcutReconciler::decodeSetterReplySlotSets(slotGroups, &replySet)) {
            if (error) {
                *error = QStringLiteral("setShortcutKeys reply did not confirm expected key");
            }
            return false;
        }
    } else {
        if (error) {
            *error = QStringLiteral("setShortcutKeys reply did not confirm expected key");
        }
        return false;
    }
    // A QSet reply has no meaningful order: compare as sets.
    if (intSetFromKeySet(replySet) != expectedSet) {
        if (error) {
            *error = QStringLiteral("setShortcutKeys reply did not confirm expected key");
        }
        return false;
    }
    ++m_writes;
    if (confirmed) {
        *confirmed = keys;
    }
    return true;
}

bool KGlobalAccelStore::defaultShortcutKeys(const QString &component, const QString &action,
                                            const QString &componentFriendly, const QString &friendly,
                                            QList<int> *defaults, QString *error)
{
    // Bounded native TRANSPORT seam: exact defaultShortcutKeys(as)->a(ai)
    // against the pinned owner. Arbitrary component/action IDs allowed:
    // identity strict, cosmetic bounded, pin plus drift enforced before the
    // call; reply strictly validated. No policy/UI effect; writeKeys
    // remains the project-row path.
    if (!ShortcutReconciler::stringValid(component) || !ShortcutReconciler::stringValid(action)
        || !ShortcutReconciler::cosmeticValid(componentFriendly) || !ShortcutReconciler::cosmeticValid(friendly)) {
        if (error) {
            *error = QStringLiteral("refusing default keys with unbounded tuple");
        }
        return false;
    }
    if (m_pinnedOwner.isEmpty() || !ShortcutReconciler::uniqueNameValid(m_pinnedOwner)) {
        if (error) {
            *error = QStringLiteral("refusing default keys without a pinned KGlobalAccel owner");
        }
        return false;
    }
    {
        const QDBusConnection connection = QDBusConnection::sessionBus();
        if (!connection.isConnected()) {
            if (error) {
                *error = QStringLiteral("D-Bus daemon interface is invalid");
            }
            return false;
        }
        QDBusConnectionInterface *bus = connection.interface();
        if (!bus) {
            if (error) {
                *error = QStringLiteral("D-Bus daemon interface is invalid");
            }
            return false;
        }
        const QDBusReply<QString> liveOwner = bus->serviceOwner(shortcutService());
        const bool liveValid = liveOwner.isValid();
        const QString liveValue = liveValid ? liveOwner.value() : QString();
        if (!checkPinnedDrift(liveValid, liveValue, m_pinnedOwner, error)) {
            return false;
        }
    }
    ensureKeySequenceMetaTypes();
    const QStringList actionId{component, action, componentFriendly, friendly};
    if (!QDBusConnection::sessionBus().isConnected()) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel interface is invalid");
        }
        return false;
    }
    QDBusMessage call =
        QDBusMessage::createMethodCall(m_pinnedOwner, shortcutPath(), shortcutInterface(), shortcutDefaultKeysMethod());
    call.setArguments({actionId});
    const QDBusMessage reply = QDBusConnection::sessionBus().call(call);
    return ShortcutReconciler::parseDefaultShortcutKeysReply(reply.type(), reply.signature(), reply.arguments(),
                                                             defaults, error);
}

bool KGlobalAccelStore::setForeignShortcutKeys(const QString &component, const QString &action,
                                               const QString &componentFriendly, const QString &friendly,
                                               const QList<int> &keys, QString *error)
{
    // Bounded native TRANSPORT seam: exact setForeignShortcutKeys
    // (as,a(ai))->void against the pinned owner with strict void validation,
    // owner drift checks, and fresh shortcutKeys readback confirmation. The
    // void reply carries no keys, so confirmation reads back active keys for
    // the same actionId and compares as sets. Arbitrary IDs allowed; successful
    // foreign writes count against the same lifetime cap as writeKeys.
    if (!ShortcutReconciler::keysValid(keys) || !ShortcutReconciler::stringValid(component)
        || !ShortcutReconciler::stringValid(action) || !ShortcutReconciler::cosmeticValid(componentFriendly)
        || !ShortcutReconciler::cosmeticValid(friendly)) {
        if (error) {
            *error = QStringLiteral("refusing foreign write with unbounded tuple");
        }
        return false;
    }
    if (m_pinnedOwner.isEmpty() || !ShortcutReconciler::uniqueNameValid(m_pinnedOwner)) {
        if (error) {
            *error = QStringLiteral("refusing foreign write without a pinned KGlobalAccel owner");
        }
        return false;
    }
    // No lifetime accumulation cap here: the per-operation budget is
    // enforced by ShortcutReconciler before the write that would overflow
    // it (see shortcutMaxForceWrites); successful foreign writes still
    // count in the monotonic total used for per-operation deltas.
    auto driftCheck = [&](QString *driftError) {
        const QDBusConnection connection = QDBusConnection::sessionBus();
        if (!connection.isConnected()) {
            if (driftError) {
                *driftError = QStringLiteral("D-Bus daemon interface is invalid");
            }
            return false;
        }
        QDBusConnectionInterface *bus = connection.interface();
        if (!bus) {
            if (driftError) {
                *driftError = QStringLiteral("D-Bus daemon interface is invalid");
            }
            return false;
        }
        const QDBusReply<QString> liveOwner = bus->serviceOwner(shortcutService());
        const bool liveValid = liveOwner.isValid();
        const QString liveValue = liveValid ? liveOwner.value() : QString();
        return checkPinnedDrift(liveValid, liveValue, m_pinnedOwner, driftError);
    };
    if (!driftCheck(error)) {
        return false;
    }
    ensureKeySequenceMetaTypes();
    const QStringList actionId{component, action, componentFriendly, friendly};
    const QList<QKeySequence> keyList = keyListFromInts(keys);
    if (!QDBusConnection::sessionBus().isConnected()) {
        if (error) {
            *error = QStringLiteral("KGlobalAccel interface is invalid");
        }
        return false;
    }
    QDBusMessage setCall = QDBusMessage::createMethodCall(m_pinnedOwner, shortcutPath(), shortcutInterface(),
                                                          shortcutSetForeignKeysMethod());
    setCall.setArguments({actionId, QVariant::fromValue(keyList)});
    const QDBusMessage setReply = QDBusConnection::sessionBus().call(setCall);
    if (!ShortcutReconciler::parseSetForeignShortcutKeysReply(setReply.type(), setReply.signature(),
                                                              setReply.arguments(), error)) {
        return false;
    }
    if (!driftCheck(error)) {
        return false;
    }
    // Fresh readback confirmation: the void setter returns nothing, so read
    // active keys for the same actionId and compare as sets.
    QDBusMessage getCall =
        QDBusMessage::createMethodCall(m_pinnedOwner, shortcutPath(), shortcutInterface(), shortcutActiveKeysMethod());
    getCall.setArguments({actionId});
    const QDBusMessage getReply = QDBusConnection::sessionBus().call(getCall);
    QList<int> readback;
    if (!ShortcutReconciler::parseDefaultShortcutKeysReply(getReply.type(), getReply.signature(),
                                                           getReply.arguments(), &readback, nullptr)) {
        if (error) {
            *error = QStringLiteral("setForeignShortcutKeys readback did not confirm expected keys");
        }
        return false;
    }
    if (!ShortcutReconciler::foreignReadbackMatches(keys, readback)) {
        if (error) {
            *error = QStringLiteral("setForeignShortcutKeys readback did not confirm expected keys");
        }
        return false;
    }
    ++m_writes;
    return true;
}

ShortcutReconciler::ShortcutReconciler(ShortcutStore *store, ClearedActionsStore *cleared)
    : m_store(store)
    , m_cleared(cleared)
{
}

ShortcutStore *createLiveShortcutStore()
{
    return new KGlobalAccelStore;
}

namespace
{

// Bounded cleared-entry validation shared by load and save: strict
// component/action identity only, at most SHORTCUT_MAX_TUPLES entries.
// No cosmetic labels are stored.
bool clearedEntriesValid(const QList<ClearedAction> &actions, QString *error)
{
    if (actions.size() > SHORTCUT_MAX_TUPLES) {
        if (error) {
            *error = QStringLiteral("cleared shortcut list is unbounded");
        }
        return false;
    }
    for (const ClearedAction &entry : actions) {
        if (!ShortcutReconciler::stringValid(entry.component) || !ShortcutReconciler::stringValid(entry.action)) {
            if (error) {
                *error = QStringLiteral("cleared shortcut entry is unbounded");
            }
            return false;
        }
    }
    return true;
}

} // namespace

KConfigClearedActions::KConfigClearedActions(const QString &filePath)
    : m_filePath(filePath)
{
}

bool KConfigClearedActions::load(QList<ClearedAction> *actions, QString *error)
{
    if (!ShortcutReconciler::clearedActionsPathSafe(m_filePath, error)) {
        return false;
    }
    KConfig config(m_filePath, KConfig::SimpleConfig);
    const KConfigGroup group = config.group(QStringLiteral("ClearedActions"));
    const QStringList components = group.readEntry(QStringLiteral("Components"), QStringList());
    const QStringList acts = group.readEntry(QStringLiteral("Actions"), QStringList());
    // IDs only: legacy ComponentFriendlies/Friendlies keys are ignored, never
    // read, never required.
    if (components.size() != acts.size()) {
        if (error) {
            *error = QStringLiteral("cleared shortcut list is malformed");
        }
        return false;
    }
    QList<ClearedAction> loaded;
    loaded.reserve(components.size());
    for (int i = 0; i < components.size(); ++i) {
        ClearedAction entry;
        entry.component = components.at(i);
        entry.action = acts.at(i);
        loaded.append(entry);
    }
    if (!clearedEntriesValid(loaded, error)) {
        return false;
    }
    if (actions) {
        *actions = loaded;
    }
    return true;
}

bool KConfigClearedActions::save(const QList<ClearedAction> &actions, QString *error)
{
    if (!clearedEntriesValid(actions, error)) {
        return false;
    }
    if (!ShortcutReconciler::clearedActionsPathSafe(m_filePath, error)) {
        return false;
    }
    const QString parentPath = QFileInfo(m_filePath).dir().path();
    if (!ensurePrivateDir(parentPath, error)) {
        return false;
    }
    {
        KConfig config(m_filePath, KConfig::SimpleConfig);
        KConfigGroup group = config.group(QStringLiteral("ClearedActions"));
        QStringList components;
        QStringList acts;
        components.reserve(actions.size());
        acts.reserve(actions.size());
        for (const ClearedAction &entry : actions) {
            components.append(entry.component);
            acts.append(entry.action);
        }
        group.writeEntry(QStringLiteral("Components"), components);
        group.writeEntry(QStringLiteral("Actions"), acts);
        // IDs only: never write cosmetic labels. Remove any legacy
        // friendly keys so the persisted shape stays Components+Actions.
        group.deleteEntry(QStringLiteral("ComponentFriendlies"));
        group.deleteEntry(QStringLiteral("Friendlies"));
        config.sync();
    }
    if (!ensurePrivateFile(m_filePath, error)) {
        return false;
    }
    // Write+sync+readback before any daemon mutation.
    QList<ClearedAction> readback;
    if (!load(&readback, nullptr)) {
        if (error) {
            *error = QStringLiteral("cleared shortcut readback failed");
        }
        return false;
    }
    if (readback != actions) {
        if (error) {
            *error = QStringLiteral("cleared shortcut readback mismatch");
        }
        return false;
    }
    return true;
}

bool KConfigClearedActions::clear(QString *error)
{
    if (!ShortcutReconciler::clearedActionsPathSafe(m_filePath, error)) {
        return false;
    }
    if (!QFile::exists(m_filePath)) {
        return true;
    }
    // Only removes the project-owned cleared-actions file, never global config.
    if (!QFile::remove(m_filePath)) {
        if (error) {
            *error = QStringLiteral("could not clear the cleared shortcut list");
        }
        return false;
    }
    return true;
}

ClearedActionsStore *createLiveClearedActionsStore(const QString &filePath)
{
    return new KConfigClearedActions(filePath);
}

QString defaultClearedActionsPath()
{
    // Host-independent project location for the durable cleared-ID
    // list, shared by every KCM host. Never falls back to CWD.
    const QString base = QStandardPaths::writableLocation(QStandardPaths::GenericConfigLocation);
    if (base.isEmpty() || !QDir::isAbsolutePath(base)) {
        return QString();
    }
    return base + QStringLiteral("/omnitiler/shortcut-clearedrc");
}

Q_LOGGING_CATEGORY(lcShortcut, "omnitiler.shortcut");

namespace
{

ShortcutLogSink g_shortcutLogSink;

} // namespace

void ShortcutDiag::setSink(ShortcutLogSink sink)
{
    g_shortcutLogSink = std::move(sink);
}

void ShortcutDiag::resetSink()
{
    g_shortcutLogSink = nullptr;
}

void ShortcutDiag::log(QtMsgType type, const char *operation, const char *stage, const char *outcome,
                       const QString &detail)
{
    // Logging never affects behavior: void return, bounded message, all
    // exceptions swallowed, no caller branches on this path.
    try {
        // Reconstructible conflict diagnostics: callers pass only bounded
        // component/action IDs and key images (raw ints or display names).
        // Friendly labels and user paths never enter here; truncate only.
        QString bounded = detail;
        if (bounded.size() > 512) {
            bounded.truncate(512);
        }
        const QString message = QStringLiteral("omnitiler.shortcut op=%1 stage=%2 outcome=%3 %4")
                                    .arg(QString::fromUtf8(operation), QString::fromUtf8(stage),
                                         QString::fromUtf8(outcome), bounded);
        if (g_shortcutLogSink) {
            g_shortcutLogSink(type, message);
            return;
        }
        switch (type) {
        case QtDebugMsg:
            qCDebug(lcShortcut).noquote() << message;
            break;
        case QtInfoMsg:
            qCInfo(lcShortcut).noquote() << message;
            break;
        case QtWarningMsg:
            qCWarning(lcShortcut).noquote() << message;
            break;
        default:
            qCCritical(lcShortcut).noquote() << message;
            break;
        }
    } catch (...) {
    }
}

bool ShortcutReconciler::collectHolderSnapshot(HolderSnapshot *snapshot, QString *error)
{
    return collectHolderSnapshotFor(snapshot, QSet<QString>(), error);
}

bool ShortcutReconciler::collectHolderSnapshotFor(HolderSnapshot *snapshot, const QSet<QString> &disabledIds,
                                                  QString *error)
{
    return collectHolderSnapshotFor(snapshot, disabledIds, true, error);
}

bool ShortcutReconciler::collectHolderSnapshotFor(HolderSnapshot *snapshot, const QSet<QString> &disabledIds,
                                                  bool authenticStaged, QString *error)
{
    if (!m_store) {
        if (error) {
            *error = QStringLiteral("reconciler is not configured");
        }
        return false;
    }
    if (!disabledIdsValid(disabledIds, error)) {
        return false;
    }
    if (!m_store->checkSetterContract(error)) {
        return false;
    }
    HolderSnapshot snap;
    if (!m_store->currentOwner(&snap.owner, &snap.uid, error)) {
        return false;
    }
    QList<ShortcutTuple> tuples;
    if (!m_store->readAll(&tuples, error)) {
        return false;
    }
    if (tuples.size() > SHORTCUT_MAX_TUPLES) {
        if (error) {
            *error = QStringLiteral("tuple enumeration is unbounded");
        }
        return false;
    }
    const QList<ShortcutConflictRow> &table = shortcutConflictTable();
    const QString focusId = shortcutCatalogId(table.at(0).projectComponent, table.at(0).projectAction);
    const bool focusEnabled = !disabledIds.contains(focusId);
    // The lock resolves only while focus-right itself is Authentic-enabled:
    // a Keep focus preserves its current assignment and a disabled focus is
    // cleared, so neither relocates Lock Session.
    const bool focusAuthentic = focusEnabled && authenticStaged;
    snap.projects.reserve(table.size());
    if (focusAuthentic) {
        ShortcutTuple focusCurrent;
        if (!findAllowlisted(tuples, table.at(0).projectComponent, table.at(0).projectAction, &focusCurrent,
                             error)) {
            return false;
        }
        if (!findAllowlisted(tuples, table.at(0).foreignComponent, table.at(0).foreignAction, &snap.lock,
                             error)) {
            return false;
        }
        snap.lockResolved = true;
        snap.projects.append(focusCurrent);
    } else {
        // A Keep or disabled focus-right leaves the lock out of scope: no
        // resolution, no relocation, and no Meta+L/Meta+Esc holder scan
        // below.
        snap.projects.append(ShortcutTuple());
    }
    for (int i = 1; i < table.size(); ++i) {
        const QString rowId =
            shortcutCatalogId(table.at(i).projectComponent, table.at(i).projectAction);
        if (disabledIds.contains(rowId)) {
            // Disabled table rows stay allowed when absent live (no
            // assignment); duplicates still fail closed even when disabled.
            int matches = 0;
            ShortcutTuple found;
            for (const ShortcutTuple &tuple : tuples) {
                if (tuple.component == table.at(i).projectComponent
                    && tuple.action == table.at(i).projectAction) {
                    ++matches;
                    found = tuple;
                }
            }
            if (matches > 1) {
                if (error) {
                    *error = QStringLiteral("allowlisted tuple %1/%2 has %3 records, expected exactly one")
                                 .arg(table.at(i).projectComponent, table.at(i).projectAction)
                                 .arg(matches);
                }
                return false;
            }
            if (matches == 0) {
                snap.projects.append(ShortcutTuple());
                continue;
            }
            snap.projects.append(found);
            continue;
        }
        ShortcutTuple current;
        if (!findAllowlisted(tuples, table.at(i).projectComponent, table.at(i).projectAction, &current, error)) {
            return false;
        }
        snap.projects.append(current);
    }
    if (focusAuthentic && (!keysValid(snap.projects.at(0).active) || !keysValid(snap.lock.active))) {
        if (error) {
            *error = QStringLiteral("allowlisted tuple is unbounded");
        }
        return false;
    }
    for (int i = 1; i < snap.projects.size(); ++i) {
        if (!keysValid(snap.projects.at(i).active)) {
            if (error) {
                *error = QStringLiteral("allowlisted tuple is unbounded");
            }
            return false;
        }
    }
    // Full-catalog image in catalog order for the confirmed Force snapshot
    // and the disable-clear phase. Missing enabled rows fail closed with
    // zero writes; missing disabled rows stay allowed (no assignment).
    // Duplicates always fail closed.
    {
        const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
        snap.catalogActives.reserve(catalog.size());
        snap.catalogPresent.reserve(catalog.size());
        for (const ShortcutCatalogEntry &entry : catalog) {
            const QString id = shortcutCatalogId(entry.component, entry.action);
            int matches = 0;
            ShortcutTuple found;
            for (const ShortcutTuple &tuple : tuples) {
                if (tuple.component == entry.component && tuple.action == entry.action) {
                    ++matches;
                    found = tuple;
                }
            }
            if (matches > 1) {
                if (error) {
                    *error = QStringLiteral("allowlisted tuple %1/%2 has %3 records, expected exactly one")
                                 .arg(entry.component, entry.action)
                                 .arg(matches);
                }
                return false;
            }
            if (matches == 0) {
                if (!disabledIds.contains(id)) {
                    if (error) {
                        *error = QStringLiteral("allowlisted tuple %1/%2 has 0 records, expected exactly one")
                                     .arg(entry.component, entry.action);
                    }
                    return false;
                }
                snap.catalogActives.append(QList<int>());
                snap.catalogPresent.append(false);
                continue;
            }
            if (!keysValid(found.active)) {
                if (error) {
                    *error = QStringLiteral("allowlisted tuple is unbounded");
                }
                return false;
            }
            snap.catalogActives.append(found.active);
            snap.catalogPresent.append(true);
        }
    }
    // Independent structural validation of enumerated state: unbounded
    // unrelated tuples fail closed with zero writes.
    QMap<QString, QPair<QString, QString>> cosmetics;
    for (const ShortcutTuple &tuple : tuples) {
        if (isProjectAction(tuple.component, tuple.action) || isLockAction(tuple.component, tuple.action)) {
            continue;
        }
        if (!keysValid(tuple.active)) {
            if (error) {
                *error = QStringLiteral("unrelated tuple is unbounded");
            }
            return false;
        }
        cosmetics.insert(tuple.component + QStringLiteral("/") + tuple.action,
                         qMakePair(tuple.componentFriendly, tuple.friendly));
    }
    // Authoritative per-required-key holder scan over the enabled chords:
    // Authentic-enabled rows contribute their canonical chords, Keep rows
    // contribute their live actual chords (customized chords included), and
    // the lock chords join only while focus-right is Authentic-enabled.
    // Project actions and Lock Session own their chords; the explicit
    // System Monitor Meta+Esc holder is user-authorized. Every other actual
    // holder becomes a clear row, whatever its identity (known, unknown, or
    // legacy project IDs). Disabled rows are never scanned and never become
    // rows or blockers.
    // Transport, parsing, and consistency failures sort before the lock
    // gate below, matching the historical refusal precedence.
    const QList<int> keepKeys = keepKeysForImages(snap.catalogActives, disabledIds, authenticStaged);
    const QList<int> required = enabledRequiredKeys(disabledIds, authenticStaged, keepKeys);
    QMap<QString, int> rowIndex;
    for (int key : required) {
        QString keyError;
        if (!checkOccupancyKeyRange(key, &keyError)) {
            if (error) {
                *error = keyError;
            }
            return false;
        }
        QList<ShortcutKeyHolder> holders;
        QString storeError;
        if (!m_store->shortcutsByKey(key, &holders, &storeError)) {
            if (error) {
                *error = storeError.isEmpty() ? QStringLiteral("globalShortcutsByKey call failed") : storeError;
            }
            return false;
        }
        bool available = false;
        if (!m_store->shortcutAvailable(key, QString(), &available, &storeError)) {
            if (error) {
                *error = storeError.isEmpty() ? QStringLiteral("globalShortcutAvailable call failed") : storeError;
            }
            return false;
        }
        if (holders.size() > SHORTCUT_MAX_TUPLES) {
            if (error) {
                *error = QStringLiteral("unexpected globalShortcutsByKey reply: too many occupancy holders");
            }
            return false;
        }
        for (const ShortcutKeyHolder &holder : holders) {
            if (holder.component.isEmpty()) {
                if (error) {
                    *error = QStringLiteral("unexpected globalShortcutsByKey reply: empty holder component");
                }
                return false;
            }
            if (holder.component.size() > SHORTCUT_MAX_STRING_LEN) {
                if (error) {
                    *error = QStringLiteral("unexpected globalShortcutsByKey reply: oversized holder component");
                }
                return false;
            }
            if (holder.action.isEmpty()) {
                if (error) {
                    *error = QStringLiteral("unexpected globalShortcutsByKey reply: empty holder action");
                }
                return false;
            }
            if (holder.action.size() > SHORTCUT_MAX_STRING_LEN) {
                if (error) {
                    *error = QStringLiteral("unexpected globalShortcutsByKey reply: oversized holder action");
                }
                return false;
            }
            switch (classifyKeyList(holder.active)) {
            case KeyListFailure::TooMany:
                if (error) {
                    *error = QStringLiteral("unexpected globalShortcutsByKey reply: too many holder active keys");
                }
                return false;
            case KeyListFailure::Negative:
                if (error) {
                    *error = QStringLiteral("unexpected globalShortcutsByKey reply: negative holder active key");
                }
                return false;
            case KeyListFailure::Oversized:
                if (error) {
                    *error = QStringLiteral("unexpected globalShortcutsByKey reply: oversized holder active key");
                }
                return false;
            case KeyListFailure::None:
                break;
            }
            switch (classifyKeyList(holder.defaults)) {
            case KeyListFailure::TooMany:
                if (error) {
                    *error = QStringLiteral("unexpected globalShortcutsByKey reply: too many holder default keys");
                }
                return false;
            case KeyListFailure::Negative:
                if (error) {
                    *error = QStringLiteral("unexpected globalShortcutsByKey reply: negative holder default key");
                }
                return false;
            case KeyListFailure::Oversized:
                if (error) {
                    *error = QStringLiteral("unexpected globalShortcutsByKey reply: oversized holder default key");
                }
                return false;
            case KeyListFailure::None:
                break;
            }
            if (isHolderExempt(holder.component, holder.action, key)) {
                // Record the authorized relocation-target displacement while
                // in scope so the preview describes it honestly. Project and
                // lock holders stay silent; nothing here is writable.
                if (snap.lockResolved && isAuthorizedDisplacement(key, holder.component, holder.action)
                    && shortcutConflictTable().at(0).resolutionTarget.contains(key)
                    && !snap.authorizedTargetPresent) {
                    snap.authorizedTargetPresent = true;
                    snap.authorizedTargetComponent = holder.component;
                    snap.authorizedTargetAction = holder.action;
                    snap.authorizedTargetActive = holder.active;
                    snap.authorizedTargetComponentFriendly = holder.componentFriendly;
                    snap.authorizedTargetFriendly = holder.friendly;
                }
                continue;
            }
            const QString id = holder.component + QStringLiteral("/") + holder.action;
            const auto existing = rowIndex.find(id);
            if (existing == rowIndex.end()) {
                const QList<int> removals = conflictingKeysIn(holder.active, required);
                if (removals.isEmpty()) {
                    // Listed for the key but holds no required chord in its
                    // active list: claims the chord (e.g. via a
                    // .desktop-declared default) with nothing to clear.
                    Blocker blocker;
                    blocker.component = holder.component;
                    blocker.action = holder.action;
                    blocker.key = key;
                    blocker.active = holder.active;
                    if (!holder.componentFriendly.isEmpty() || !holder.friendly.isEmpty()) {
                        blocker.componentFriendly = holder.componentFriendly;
                        blocker.friendly = holder.friendly;
                    } else {
                        const auto cosmeticBlock = cosmetics.find(id);
                        if (cosmeticBlock != cosmetics.end()) {
                            blocker.componentFriendly = cosmeticBlock->first;
                            blocker.friendly = cosmeticBlock->second;
                        }
                    }
                    snap.blocked.append(blocker);
                    if (snap.blocked.size() > SHORTCUT_MAX_TUPLES) {
                        if (error) {
                            *error = QStringLiteral(
                                "unexpected globalShortcutsByKey reply: too many occupancy holders");
                        }
                        return false;
                    }
                    continue;
                }
                ClearRow row;
                row.component = holder.component;
                row.action = holder.action;
                if (!holder.componentFriendly.isEmpty() || !holder.friendly.isEmpty()) {
                    row.componentFriendly = holder.componentFriendly;
                    row.friendly = holder.friendly;
                } else {
                    const auto cosmetic = cosmetics.find(id);
                    if (cosmetic != cosmetics.end()) {
                        row.componentFriendly = cosmetic->first;
                        row.friendly = cosmetic->second;
                    }
                }
                row.active = holder.active;
                row.removals = removals;
                row.remainder = remainderAfterClearIn(holder.active, required);
                rowIndex.insert(id, snap.rows.size());
                snap.rows.append(row);
                if (snap.rows.size() > SHORTCUT_MAX_TUPLES) {
                    if (error) {
                        *error = QStringLiteral("unexpected globalShortcutsByKey reply: too many occupancy holders");
                    }
                    return false;
                }
            } else {
                ClearRow &row = snap.rows[existing.value()];
                if (row.active != holder.active) {
                    if (error) {
                        *error = QStringLiteral(
                            "unexpected globalShortcutsByKey reply: inconsistent holder for the requested key");
                    }
                    return false;
                }
            }
        }
        // Whole-key consistency, fail closed both directions: empty holders
        // must report available and non-empty holders must report
        // unavailable. Skipped project/lock/authorized holders still occupy
        // the key and keep the non-empty/unavailable expectation.
        if (holders.isEmpty() != available) {
            if (error) {
                *error = holders.isEmpty()
                    ? QStringLiteral("unexpected globalShortcutAvailable reply: empty holders report unavailable")
                    : QStringLiteral("unexpected globalShortcutAvailable reply: occupied holders report available");
            }
            return false;
        }
    }
    // Deterministic row/blocker order regardless of daemon reply order,
    // so status, error, preview, and re-preview share one ordering and a
    // fresh re-preview compares exactly against the confirmed snapshot.
    std::sort(snap.rows.begin(), snap.rows.end(), [](const ClearRow &a, const ClearRow &b) {
        if (a.component != b.component) {
            return a.component < b.component;
        }
        return a.action < b.action;
    });
    std::sort(snap.blocked.begin(), snap.blocked.end(), [](const Blocker &a, const Blocker &b) {
        if (a.component != b.component) {
            return a.component < b.component;
        }
        if (a.action != b.action) {
            return a.action < b.action;
        }
        return a.key < b.key;
    });
    // Lock preconditions are relocate logic and apply only while focus-right
    // is Authentic-enabled: without the preimage to replace and without the
    // target already held there is nothing safe to do. A Keep or disabled
    // focus-right skips this gate entirely with the lock out of scope.
    if (focusAuthentic) {
        const ShortcutConflictRow &lockRow = shortcutConflictTable().at(0);
        bool hasPre = false;
        bool hasTarget = false;
        for (int key : lockRow.foreignExpectedPre) {
            if (snap.lock.active.contains(key)) {
                hasPre = true;
            }
        }
        for (int key : lockRow.resolutionTarget) {
            if (snap.lock.active.contains(key)) {
                hasTarget = true;
            }
        }
        if (!hasPre && !hasTarget) {
            if (error) {
                *error = QStringLiteral("refusing to apply: lock binding has no Meta+L to replace");
            }
            return false;
        }
    }
    if (snapshot) {
        *snapshot = snap;
    }
    return true;
}

ShortcutApplyResult ShortcutReconciler::writeProjectKeys(const char *operation)
{
    return writeProjectKeysFor(operation, QSet<QString>(), true);
}

ShortcutApplyResult ShortcutReconciler::writeProjectKeysFor(const char *operation, const QSet<QString> &disabledIds)
{
    return writeProjectKeysFor(operation, disabledIds, true);
}

ShortcutApplyResult ShortcutReconciler::writeProjectKeysFor(const char *operation, const QSet<QString> &disabledIds,
                                                            bool authenticStaged)
{
    ShortcutApplyResult result;
    if (!m_store) {
        result.error = QStringLiteral("reconciler is not configured");
        return result;
    }
    const QList<ShortcutConflictRow> &table = shortcutConflictTable();
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    ShortcutDiag::log(QtDebugMsg, operation, "start", "running",
                      QStringLiteral("rows=%1 disabled=%2 authentic=%3")
                          .arg(catalog.size())
                          .arg(disabledIds.size())
                          .arg(authenticStaged ? 1 : 0));
    const int startWrites = m_store->writeCount();
    auto usedWrites = [&]() {
        return m_store->writeCount() - startWrites;
    };
    // Operation-scoped budget: at most one write per catalog row plus the
    // lock relocation. Checked before every write so the overflowing write
    // never happens; the post-hoc check below stays as defense in depth.
    const int maxProjectWrites = shortcutMaxProjectWrites();
    QString error;
    HolderSnapshot snap;
    if (!collectHolderSnapshotFor(&snap, disabledIds, authenticStaged, &error)) {
        result.error = error;
        ShortcutDiag::log(QtWarningMsg, operation, "preflight", "refused", error);
        return result;
    }
    if (!snap.rows.isEmpty() || !snap.blocked.isEmpty()) {
        if (!snap.blocked.isEmpty()) {
            const Blocker &first = snap.blocked.first();
            result.error = QStringLiteral("refusing to apply: %1 is claimed by %2/%3")
                               .arg(keyDisplayName(first.key), first.component, first.action);
        } else {
            const ClearRow &first = snap.rows.first();
            const int key = first.removals.isEmpty() ? 0 : first.removals.first();
            if (key == 0) {
                result.error = QStringLiteral("refusing to apply: holder state is inconsistent");
            } else {
                result.error = QStringLiteral("refusing to apply: %1 is claimed by %2/%3")
                                   .arg(keyDisplayName(key), first.component, first.action);
            }
        }
        result.conflictRefused = true;
        ShortcutDiag::log(QtWarningMsg, operation, "preflight", "refused", result.error);
        return result;
    }
    const QString owner = snap.owner;
    const uint uid = snap.uid;
    auto checkOwner = [&](QString *ownerError) {
        QString liveOwner;
        uint liveUid = 0;
        if (!m_store->currentOwner(&liveOwner, &liveUid, ownerError)) {
            return false;
        }
        if (liveOwner != owner || liveUid != uid) {
            if (ownerError) {
                *ownerError = QStringLiteral("KGlobalAccel service owner drifted");
            }
            return false;
        }
        return true;
    };
    auto writeOne = [&](const QString &component, const QString &action, const QString &componentFriendly,
                        const QString &friendly, const QList<int> &keys, const char *what, QString *writeError) {
        if (!checkOwner(writeError)) {
            return false;
        }
        if (usedWrites() >= maxProjectWrites) {
            if (writeError) {
                *writeError = QStringLiteral("tuple writes exceed the exact %1 writes max").arg(maxProjectWrites);
            }
            return false;
        }
        QList<int> confirmed;
        if (!m_store->writeKeys(component, action, componentFriendly, friendly, keys, &confirmed, writeError)) {
            if (writeError && writeError->isEmpty()) {
                *writeError =
                    QStringLiteral("setShortcutKeys call failed for %1").arg(QString::fromUtf8(what));
            }
            return false;
        }
        if (confirmed != keys) {
            if (writeError) {
                *writeError = QStringLiteral("setShortcutKeys reply did not confirm expected key");
            }
            return false;
        }
        return checkOwner(writeError);
    };
    auto isTableProject = [&](const QString &component, const QString &action) {
        for (const ShortcutConflictRow &row : table) {
            if (row.projectComponent == component && row.projectAction == action) {
                return true;
            }
        }
        return false;
    };
    auto expectedPost = [&](const ShortcutCatalogEntry &entry, QList<int> *post) {
        for (const ShortcutConflictRow &row : table) {
            if (row.projectComponent == entry.component && row.projectAction == entry.action) {
                *post = row.projectPost;
                return;
            }
        }
        if (entry.canonicalKey == 0) {
            *post = QList<int>();
            return;
        }
        *post = QList<int>{entry.canonicalKey};
    };
    auto whatFor = [&](const ShortcutCatalogEntry &entry) {
        for (const ShortcutConflictRow &row : table) {
            if (row.projectComponent == entry.component && row.projectAction == entry.action
                && !row.projectDiag.isEmpty()) {
                return row.projectDiag;
            }
        }
        return entry.action;
    };
    const QString focusId = shortcutCatalogId(table.at(0).projectComponent, table.at(0).projectAction);
    const bool focusEnabled = !disabledIds.contains(focusId);
    // Per-binding outcome diagnostics: identity only (catalog ID), never
    // key payloads. Kinds: kept (Keep skip), assigned (Authentic write),
    // already-assigned (Authentic no-op), cleared (Disable clear),
    // already-cleared (Disable no-op), relocated/already-relocated (lock).
    auto logRow = [&](const char *outcome, const QString &id) {
        ShortcutDiag::log(QtDebugMsg, operation, "write", outcome, id);
    };
    // Phase 1: focus must own its post before lock drops the preimage.
    // Runs only while focus-right itself is Authentic-enabled; a Keep or
    // disabled focus leaves the lock untouched.
    if (snap.lockResolved) {
        const QList<int> focusPost = table.at(0).projectPost;
        const ShortcutTuple &focusSnap = snap.projects.at(0);
        if (focusSnap.active != focusPost) {
            if (!writeOne(focusSnap.component, focusSnap.action, focusSnap.componentFriendly, focusSnap.friendly,
                           focusPost, "focus", &error)) {
                result.error = error;
                result.writes = usedWrites();
                return result;
            }
            logRow("assigned", focusId);
        } else {
            logRow("already-assigned", focusId);
        }
        // Phase 2: lock relocation preserving exact other keys/order, gated
        // on a fresh read showing focus still owning its post.
        {
            QList<ShortcutTuple> tuples;
            if (!m_store->readAll(&tuples, &error)) {
                result.error = error;
                result.writes = usedWrites();
                return result;
            }
            ShortcutTuple focusGate;
            ShortcutTuple lockGate;
            if (!findAllowlisted(tuples, table.at(0).projectComponent, table.at(0).projectAction, &focusGate,
                                 &error)
                || !findAllowlisted(tuples, table.at(0).foreignComponent, table.at(0).foreignAction, &lockGate,
                                     &error)) {
                result.error = error;
                result.writes = usedWrites();
                return result;
            }
            if (focusGate.active != focusPost) {
                result.error = QStringLiteral("refusing the lock write while focus does not own Meta+L");
                result.writes = usedWrites();
                return result;
            }
            const QList<int> lockPost = lockPostFor(lockGate.active);
            const QString lockId = shortcutCatalogId(lockGate.component, lockGate.action);
            if (lockGate.active != lockPost) {
                if (!writeOne(lockGate.component, lockGate.action, lockGate.componentFriendly, lockGate.friendly,
                               lockPost, "lock", &error)) {
                    result.error = error;
                    result.writes = usedWrites();
                    return result;
                }
                logRow("relocated", lockId);
            } else {
                logRow("already-relocated", lockId);
            }
        }
    }
    // Phase 3: Authentic-enabled project assignments from a fresh read, in
    // catalog order. Keep rows are preserved untouched. Missing enabled
    // rows fail closed; missing disabled rows stay allowed (no assignment).
    // Phase 4 clears disabled own rows to empty through the existing
    // transport; foreign holders are never touched.
    {
        QList<ShortcutTuple> tuples;
        if (!m_store->readAll(&tuples, &error)) {
            result.error = error;
            result.writes = usedWrites();
            return result;
        }
        struct Need
        {
            ShortcutTuple current;
            QList<int> post;
            QString what;
        };
        QList<Need> needs;
        struct Clear
        {
            ShortcutTuple current;
            QString what;
        };
        QList<Clear> clears;
        for (const ShortcutCatalogEntry &entry : catalog) {
            const QString id = shortcutCatalogId(entry.component, entry.action);
            if (id == focusId && focusEnabled && authenticStaged) {
                continue; // phase 1 owns the Authentic-enabled focus row
            }
            ShortcutTuple current;
            const bool tableRow = isTableProject(entry.component, entry.action);
            if (tableRow) {
                if (disabledIds.contains(id)) {
                    int matches = 0;
                    ShortcutTuple found;
                    for (const ShortcutTuple &tuple : tuples) {
                        if (tuple.component == entry.component && tuple.action == entry.action) {
                            ++matches;
                            found = tuple;
                        }
                    }
                    if (matches > 1) {
                        result.error =
                            QStringLiteral("allowlisted tuple %1/%2 has %3 records, expected exactly one")
                                .arg(entry.component, entry.action)
                                .arg(matches);
                        result.writes = usedWrites();
                        return result;
                    }
                    if (matches == 0) {
                        continue;
                    }
                    current = found;
                } else if (!findAllowlisted(tuples, entry.component, entry.action, &current, &error)) {
                    result.error = error;
                    result.writes = usedWrites();
                    return result;
                }
            } else {
                int matches = 0;
                ShortcutTuple found;
                for (const ShortcutTuple &tuple : tuples) {
                    if (tuple.component == entry.component && tuple.action == entry.action) {
                        ++matches;
                        found = tuple;
                    }
                }
                if (matches > 1) {
                    result.error =
                        QStringLiteral("allowlisted tuple %1/%2 has %3 records, expected exactly one")
                            .arg(entry.component, entry.action)
                            .arg(matches);
                    result.writes = usedWrites();
                    return result;
                }
                if (matches == 0) {
                    if (!disabledIds.contains(id)) {
                        result.error =
                            QStringLiteral("allowlisted tuple %1/%2 has 0 records, expected exactly one")
                                .arg(entry.component, entry.action);
                        result.writes = usedWrites();
                        return result;
                    }
                    continue;
                }
                current = found;
            }
            if (disabledIds.contains(id)) {
                if (!current.active.isEmpty()) {
                    clears.append({current, whatFor(entry)});
                } else {
                    logRow("already-cleared", id);
                }
                continue;
            }
            if (!authenticStaged) {
                // Keep preserves the current live assignment, including
                // customized chords and empty assignments: never assigned.
                // Presence is already bound by the preflight snapshot.
                logRow("kept", id);
                continue;
            }
            QList<int> post;
            expectedPost(entry, &post);
            if (current.active != post) {
                needs.append({current, post, whatFor(entry)});
            } else {
                logRow("already-assigned", id);
            }
        }
        for (const Need &need : needs) {
            const QByteArray whatBytes = need.what.toUtf8();
            if (!writeOne(need.current.component, need.current.action, need.current.componentFriendly,
                           need.current.friendly, need.post, whatBytes.constData(), &error)) {
                result.error = error;
                result.writes = usedWrites();
                return result;
            }
            logRow("assigned", shortcutCatalogId(need.current.component, need.current.action));
        }
        for (const Clear &clear : clears) {
            const QByteArray whatBytes = clear.what.toUtf8();
            if (!writeOne(clear.current.component, clear.current.action, clear.current.componentFriendly,
                           clear.current.friendly, QList<int>(), whatBytes.constData(), &error)) {
                result.error = error;
                result.writes = usedWrites();
                return result;
            }
            logRow("cleared", shortcutCatalogId(clear.current.component, clear.current.action));
        }
    }
    // Finish: only the expected image is permitted. Authentic-enabled rows
    // sit at their posts, Keep rows must still match the preflight image
    // (custom, canonical, or empty: any drift fails instead of claiming a
    // preserve), disabled rows are empty, missing disabled rows are out of
    // scope, missing enabled rows fail, duplicates fail, and the lock is
    // verified only while it is in scope (Authentic focus).
    {
        QList<ShortcutTuple> finalTuples;
        if (!m_store->readAll(&finalTuples, &error)) {
            result.error = error;
            result.writes = usedWrites();
            return result;
        }
        bool verified = true;
        for (int catalogIndex = 0; catalogIndex < catalog.size(); ++catalogIndex) {
            const ShortcutCatalogEntry &entry = catalog.at(catalogIndex);
            const QString id = shortcutCatalogId(entry.component, entry.action);
            if (id == focusId) {
                if (!focusEnabled) {
                    // Focus disabled leaves the focus row out of scope when
                    // absent; duplicates still fail closed.
                    int matches = 0;
                    ShortcutTuple found;
                    for (const ShortcutTuple &tuple : finalTuples) {
                        if (tuple.component == entry.component && tuple.action == entry.action) {
                            ++matches;
                            found = tuple;
                        }
                    }
                    if (matches > 1) {
                        result.error =
                            QStringLiteral("allowlisted tuple %1/%2 has %3 records, expected exactly one")
                                .arg(entry.component, entry.action)
                                .arg(matches);
                        result.writes = usedWrites();
                        return result;
                    }
                    if (matches == 0) {
                        continue;
                    }
                    if (!found.active.isEmpty()) {
                        verified = false;
                    }
                    continue;
                }
                ShortcutTuple current;
                if (!findAllowlisted(finalTuples, entry.component, entry.action, &current, &error)) {
                    result.error = error;
                    result.writes = usedWrites();
                    return result;
                }
                if (authenticStaged) {
                    QList<int> post;
                    expectedPost(entry, &post);
                    if (current.active != post) {
                        verified = false;
                    }
                } else if (current.active != snap.catalogActives.at(catalogIndex)) {
                    // Keep must still match the preflight image: drift fails
                    // instead of claiming a preserve.
                    verified = false;
                }
                continue;
            }
            const bool tableRow = isTableProject(entry.component, entry.action);
            ShortcutTuple current;
            bool present = false;
            if (tableRow) {
                if (disabledIds.contains(id)) {
                    int matches = 0;
                    for (const ShortcutTuple &tuple : finalTuples) {
                        if (tuple.component == entry.component && tuple.action == entry.action) {
                            ++matches;
                            current = tuple;
                            present = true;
                        }
                    }
                    if (matches > 1) {
                        result.error =
                            QStringLiteral("allowlisted tuple %1/%2 has %3 records, expected exactly one")
                                .arg(entry.component, entry.action)
                                .arg(matches);
                        result.writes = usedWrites();
                        return result;
                    }
                    if (!present) {
                        continue;
                    }
                } else {
                    if (!findAllowlisted(finalTuples, entry.component, entry.action, &current, &error)) {
                        result.error = error;
                        result.writes = usedWrites();
                        return result;
                    }
                    present = true;
                }
            } else {
                int matches = 0;
                for (const ShortcutTuple &tuple : finalTuples) {
                    if (tuple.component == entry.component && tuple.action == entry.action) {
                        ++matches;
                        current = tuple;
                        present = true;
                    }
                }
                if (matches > 1) {
                    result.error =
                        QStringLiteral("allowlisted tuple %1/%2 has %3 records, expected exactly one")
                            .arg(entry.component, entry.action)
                            .arg(matches);
                    result.writes = usedWrites();
                    return result;
                }
                if (!present) {
                    if (!disabledIds.contains(id)) {
                        verified = false;
                    }
                    continue;
                }
            }
            if (disabledIds.contains(id)) {
                if (!current.active.isEmpty()) {
                    verified = false;
                }
                continue;
            }
            if (authenticStaged) {
                QList<int> post;
                expectedPost(entry, &post);
                if (current.active != post) {
                    verified = false;
                }
            } else if (current.active != snap.catalogActives.at(catalogIndex)) {
                // Keep must still match the preflight image: drift fails
                // instead of claiming a preserve.
                verified = false;
            }
        }
        if (verified && snap.lockResolved) {
            ShortcutTuple lockFinal;
            if (!findAllowlisted(finalTuples, table.at(0).foreignComponent, table.at(0).foreignAction, &lockFinal,
                                 &error)) {
                result.error = error;
                result.writes = usedWrites();
                return result;
            }
            const ShortcutConflictRow &lockRow = table.at(0);
            for (int key : lockRow.foreignExpectedPre) {
                if (lockFinal.active.contains(key)) {
                    verified = false;
                    break;
                }
            }
            if (verified) {
                for (int key : lockRow.resolutionTarget) {
                    if (!lockFinal.active.contains(key)) {
                        verified = false;
                        break;
                    }
                }
            }
        }
        if (!verified) {
            result.error = QStringLiteral("finish-apply verification failed: live state differs from the expected image");
            result.writes = usedWrites();
            return result;
        }
    }
    const int maxWrites = shortcutMaxProjectWrites();
    if (usedWrites() > maxWrites) {
        result.error = QStringLiteral("tuple writes exceed the exact %1 writes max").arg(maxWrites);
        result.writes = usedWrites();
        return result;
    }
    if (!checkOwner(&error)) {
        result.error = error;
        result.writes = usedWrites();
        return result;
    }
    result.ok = true;
    result.writes = usedWrites();
    ShortcutDiag::log(QtInfoMsg, operation, "finish", "ok", QStringLiteral("writes=%1").arg(result.writes));
    return result;
}

ShortcutApplyResult ShortcutReconciler::apply()
{
    return writeProjectKeys("apply");
}

ShortcutApplyResult ShortcutReconciler::applySelected(const QSet<QString> &disabledIds)
{
    return writeProjectKeysFor("apply", disabledIds, true);
}

ShortcutApplyResult ShortcutReconciler::applySelected(const QSet<QString> &disabledIds, bool authenticStaged)
{
    return writeProjectKeysFor("apply", disabledIds, authenticStaged);
}

ShortcutRevertResult ShortcutReconciler::revert()
{
    ShortcutRevertResult result;
    if (!m_store || !m_cleared) {
        result.error = QStringLiteral("reconciler is not configured");
        return result;
    }
    ShortcutDiag::log(QtDebugMsg, "revert", "start", "running", QStringLiteral("cleared=list"));
    QString error;
    if (!m_store->checkSetterContract(&error)) {
        result.error = error;
        return result;
    }
    QString owner;
    uint uid = 0;
    if (!m_store->currentOwner(&owner, &uid, &error)) {
        result.error = error;
        return result;
    }
    auto checkOwner = [&](QString *ownerError) {
        QString liveOwner;
        uint liveUid = 0;
        if (!m_store->currentOwner(&liveOwner, &liveUid, ownerError)) {
            return false;
        }
        if (liveOwner != owner || liveUid != uid) {
            if (ownerError) {
                *ownerError = QStringLiteral("KGlobalAccel service owner drifted");
            }
            return false;
        }
        return true;
    };
    QList<ClearedAction> existing;
    if (!m_cleared->load(&existing, &error)) {
        result.error = error;
        return result;
    }
    if (existing.isEmpty()) {
        result.ok = true;
        ShortcutDiag::log(QtInfoMsg, "revert", "finish", "ok", QStringLiteral("cleared=empty writes=0"));
        return result;
    }
    const int startWrites = m_store->writeCount();
    auto usedWrites = [&]() {
        return m_store->writeCount() - startWrites;
    };
    // Operation-scoped budget: one write per persisted cleared ID at most
    // (project-owned IDs stay cleared and write nothing). Checked before
    // every foreign write so the overflowing write never happens.
    const int maxRevertWrites = shortcutMaxRevertWrites(existing.size());
    // Idempotent per-entry restore: resolve each persisted ID to its fresh
    // current tuple from readAll to supply the current friendly labels
    // (empty allowed) for the 4-field actionId. Never assume 2-field
    // daemon semantics. Project-owned IDs (current and legacy) stay
    // cleared. Absent or duplicate IDs fail closed without writing
    // unrelated actions; any failure retains the list for resume.
    QList<ShortcutTuple> fresh;
    if (!m_store->readAll(&fresh, &error)) {
        result.error = error.isEmpty() ? QStringLiteral("cleared shortcut resolve failed") : error;
        result.writes = usedWrites();
        return result;
    }
    for (const ClearedAction &entry : existing) {
        if (!stringValid(entry.component) || !stringValid(entry.action)) {
            result.error = QStringLiteral("cleared shortcut entry is unbounded");
            result.writes = usedWrites();
            return result;
        }
        if (isProjectOwned(entry.component, entry.action)) {
            continue;
        }
        int matches = 0;
        ShortcutTuple current;
        for (const ShortcutTuple &tuple : fresh) {
            if (tuple.component == entry.component && tuple.action == entry.action) {
                ++matches;
                current = tuple;
            }
        }
        if (matches != 1) {
            result.error = matches == 0
                ? QStringLiteral("cleared shortcut action is absent; retaining list for retry")
                : QStringLiteral("cleared shortcut action is ambiguous; retaining list for retry");
            result.writes = usedWrites();
            return result;
        }
        // Fresh labels only; empty friendlies allowed, length bound applies.
        if (!cosmeticValid(current.componentFriendly) || !cosmeticValid(current.friendly)) {
            result.error = QStringLiteral("cleared shortcut entry is unbounded");
            result.writes = usedWrites();
            return result;
        }
        QList<int> defaults;
        if (!m_store->defaultShortcutKeys(entry.component, entry.action, current.componentFriendly,
                                          current.friendly, &defaults, &error)) {
            result.error = error.isEmpty() ? QStringLiteral("defaultShortcutKeys call failed") : error;
            result.writes = usedWrites();
            return result;
        }
        if (!checkOwner(&error)) {
            result.error = error;
            result.writes = usedWrites();
            return result;
        }
        if (usedWrites() >= maxRevertWrites) {
            result.error =
                QStringLiteral("tuple writes exceed the exact %1 writes max").arg(maxRevertWrites);
            result.writes = usedWrites();
            return result;
        }
        if (!m_store->setForeignShortcutKeys(entry.component, entry.action, current.componentFriendly,
                                             current.friendly, defaults, &error)) {
            result.error = error.isEmpty() ? QStringLiteral("setForeignShortcutKeys call failed") : error;
            result.writes = usedWrites();
            return result;
        }
    }
    if (!m_cleared->clear(&error)) {
        result.error = error.isEmpty() ? QStringLiteral("could not clear the cleared shortcut list") : error;
        result.writes = usedWrites();
        return result;
    }
    result.ok = true;
    result.writes = usedWrites();
    ShortcutDiag::log(QtInfoMsg, "revert", "finish", "ok", QStringLiteral("writes=%1").arg(result.writes));
    return result;
}

bool ShortcutReconciler::forceMismatchFromRow(const ClearRow &row, ShortcutForceMismatch *out)
{
    if (!out) {
        return false;
    }
    ShortcutForceMismatch mismatch;
    mismatch.component = row.component;
    mismatch.action = row.action;
    mismatch.expectedPre = row.removals;
    mismatch.actual = row.active;
    mismatch.post = row.remainder;
    mismatch.componentFriendly = row.componentFriendly;
    mismatch.friendly = row.friendly;
    *out = mismatch;
    return true;
}

ShortcutForcePreview ShortcutReconciler::buildForcePreview()
{
    return buildForcePreviewFor(QSet<QString>(), true);
}

ShortcutForcePreview ShortcutReconciler::buildForcePreviewFor(const QSet<QString> &disabledIds)
{
    return buildForcePreviewFor(disabledIds, true);
}

ShortcutForcePreview ShortcutReconciler::buildForcePreviewFor(const QSet<QString> &disabledIds,
                                                              bool authenticStaged)
{
    ShortcutForcePreview preview;
    auto refuse = [&](const QString &message) {
        preview.forceable = false;
        preview.error = message;
        return preview;
    };
    if (!m_store) {
        return refuse(QStringLiteral("reconciler is not configured"));
    }
    HolderSnapshot snap;
    QString error;
    if (!collectHolderSnapshotFor(&snap, disabledIds, authenticStaged, &error)) {
        return refuse(error);
    }
    // Lock preconditions are relocate logic, never forceable: even with
    // clearable holders, a lock binding with nothing to replace refuses the
    // preview up front instead of failing mid-force after clearing. Out of
    // scope while focus-right is Keep or disabled.
    if (snap.lockResolved) {
        const ShortcutConflictRow &lockRow = shortcutConflictTable().at(0);
        bool hasPre = false;
        bool hasTarget = false;
        for (int key : lockRow.foreignExpectedPre) {
            if (snap.lock.active.contains(key)) {
                hasPre = true;
            }
        }
        for (int key : lockRow.resolutionTarget) {
            if (snap.lock.active.contains(key)) {
                hasTarget = true;
            }
        }
        if (!hasPre && !hasTarget) {
            return refuse(QStringLiteral("refusing to apply: lock binding has no Meta+L to replace"));
        }
    }
    preview.owner = snap.owner;
    preview.uid = snap.uid;
    // Full fresh image: every catalog active in catalog order (empty when
    // the entry is missing live), then the lock image (empty while the
    // lock is out of scope). Presence is bound too: missing vs empty share
    // the same active image.
    preview.liveImages.reserve(snap.catalogActives.size() + 1);
    for (const QList<int> &active : snap.catalogActives) {
        preview.liveImages.append(active);
    }
    preview.liveImages.append(snap.lockResolved ? snap.lock.active : QList<int>());
    preview.livePresent.reserve(snap.catalogPresent.size() + 1);
    for (bool present : snap.catalogPresent) {
        preview.livePresent.append(present);
    }
    preview.livePresent.append(snap.lockResolved);
    preview.disabledIds = QStringList(disabledIds.begin(), disabledIds.end());
    preview.disabledIds.sort();
    preview.authenticStaged = authenticStaged;
    preview.lock.inScope = snap.lockResolved;
    if (snap.lockResolved) {
        const ShortcutConflictRow &lockRow = shortcutConflictTable().at(0);
        preview.lock.component = lockRow.foreignComponent;
        preview.lock.action = lockRow.foreignAction;
        preview.lock.actual = snap.lock.active;
        preview.lock.post = lockPostFor(snap.lock.active);
        for (int key : lockRow.foreignExpectedPre) {
            if (snap.lock.active.contains(key)) {
                preview.lock.hasPre = true;
            }
        }
        for (int key : lockRow.resolutionTarget) {
            if (snap.lock.active.contains(key)) {
                preview.lock.hasTarget = true;
            }
        }
        preview.lock.authorizedHolderPresent = snap.authorizedTargetPresent;
        preview.lock.authorizedComponent = snap.authorizedTargetComponent;
        preview.lock.authorizedAction = snap.authorizedTargetAction;
        preview.lock.authorizedActive = snap.authorizedTargetActive;
        preview.lock.authorizedComponentFriendly = snap.authorizedTargetComponentFriendly;
        preview.lock.authorizedFriendly = snap.authorizedTargetFriendly;
    }
    for (const ClearRow &row : snap.rows) {
        ShortcutForceMismatch mismatch;
        forceMismatchFromRow(row, &mismatch);
        preview.mismatches.append(mismatch);
    }
    for (const Blocker &blocker : snap.blocked) {
        ShortcutForceBlocker out;
        out.component = blocker.component;
        out.action = blocker.action;
        out.key = blocker.key;
        out.active = blocker.active;
        out.componentFriendly = blocker.componentFriendly;
        out.friendly = blocker.friendly;
        preview.blockers.append(out);
    }
    if (!snap.blocked.isEmpty()) {
        const Blocker &first = snap.blocked.first();
        preview.forceable = false;
        preview.error = QStringLiteral("refusing force: %1 is claimed by %2/%3 with no active binding to clear")
                            .arg(keyDisplayName(first.key), first.component, first.action);
        return preview;
    }
    if (snap.rows.isEmpty()) {
        return refuse(QStringLiteral("no forced override applies: no holder claims a project-required key"));
    }
    preview.forceable = true;
    return preview;
}

QList<ShortcutOrderedConflict> ShortcutReconciler::orderedConflicts(const ShortcutForcePreview &preview)
{
    QList<ShortcutOrderedConflict> out;
    out.reserve(preview.blockers.size() + preview.mismatches.size());
    for (int i = 0; i < preview.blockers.size(); ++i) {
        ShortcutOrderedConflict entry;
        entry.isBlocker = true;
        entry.index = i;
        out.append(entry);
    }
    for (int i = 0; i < preview.mismatches.size(); ++i) {
        ShortcutOrderedConflict entry;
        entry.isBlocker = false;
        entry.index = i;
        out.append(entry);
    }
    return out;
}

namespace
{

// Bounded per-row emission: one line when the full row fits, otherwise one
// line per key image with the shared index prefix so the tail is never
// dropped. IDs plus raw key ints only; never friendly labels.
void logConflictRow(QtMsgType type, const char *operation, const QString &prefix, const QString &row)
{
    constexpr int kChunkBound = 400;
    if (row.size() <= kChunkBound) {
        ShortcutDiag::log(type, operation, "conflict", "row", prefix + row);
        return;
    }
    int start = 0;
    int chunk = 0;
    while (start < row.size()) {
        const QString part = row.mid(start, kChunkBound);
        ShortcutDiag::log(type, operation, "conflict", "row",
                          QStringLiteral("%1 part=%2/%3 %4")
                              .arg(prefix)
                              .arg(chunk + 1)
                              .arg((row.size() + kChunkBound - 1) / kChunkBound)
                              .arg(part));
        start += kChunkBound;
        ++chunk;
    }
}

} // namespace

void ShortcutReconciler::logForcePreview(const char *operation, const ShortcutForcePreview &preview)
{
    const QtMsgType rowType = preview.forceable ? QtInfoMsg : QtWarningMsg;
    const char *outcome = preview.forceable ? "forceable" : "not-forceable";
    const int total = preview.mismatches.size() + preview.blockers.size();
    ShortcutDiag::log(rowType, operation, "preview", outcome,
                      QStringLiteral("conflicts=%1 blocked=%2 clearable=%3 disabled=%4 authentic=%5")
                          .arg(total)
                          .arg(preview.blockers.size())
                          .arg(preview.mismatches.size())
                          .arg(preview.disabledIds.size())
                          .arg(preview.authenticStaged ? 1 : 0));
    int at = 0;
    for (const ShortcutOrderedConflict &entry : orderedConflicts(preview)) {
        ++at;
        const QString prefix =
            QStringLiteral("i=%1/%2 ").arg(at).arg(total);
        if (entry.isBlocker) {
            const ShortcutForceBlocker &blocker = preview.blockers.at(entry.index);
            logConflictRow(rowType, operation, prefix,
                           QStringLiteral("%1/%2 blocker-key=%3 found=%4 remove=none keep=%5")
                               .arg(blocker.component, blocker.action,
                                    keysDisplay(QList<int>{blocker.key}),
                                    keysDisplay(blocker.active),
                                    keysDisplay(blocker.active)));
        } else {
            const ShortcutForceMismatch &mismatch = preview.mismatches.at(entry.index);
            logConflictRow(rowType, operation, prefix,
                           QStringLiteral("%1/%2 actual=%3 remove=%4 keep=%5")
                               .arg(mismatch.component, mismatch.action,
                                    keysDisplay(mismatch.actual),
                                    keysDisplay(mismatch.expectedPre),
                                    keysDisplay(mismatch.post)));
        }
    }
    if (!preview.forceable && !preview.error.isEmpty()) {
        ShortcutDiag::log(rowType, operation, "preview", outcome, preview.error);
    }
    if (preview.lock.inScope) {
        ShortcutDiag::log(rowType, operation, "lock", "relocate",
                          QStringLiteral("%1/%2 actual=%3 post=%4 pre=%5 target=%6 authorized=%7")
                              .arg(preview.lock.component, preview.lock.action,
                                   keysDisplay(preview.lock.actual),
                                   keysDisplay(preview.lock.post))
                              .arg(preview.lock.hasPre ? 1 : 0)
                              .arg(preview.lock.hasTarget ? 1 : 0)
                              .arg(preview.lock.authorizedHolderPresent
                                       ? preview.lock.authorizedComponent + QStringLiteral("/")
                                           + preview.lock.authorizedAction
                                       : QStringLiteral("none")));
    }
}

ShortcutForcePreview ShortcutReconciler::previewForceApply()
{
    ShortcutForcePreview preview = buildForcePreview();
    logForcePreview("force-preview", preview);
    return preview;
}

ShortcutForcePreview ShortcutReconciler::previewForceApplySelected(const QSet<QString> &disabledIds)
{
    return previewForceApplySelected(disabledIds, true);
}

ShortcutForcePreview ShortcutReconciler::previewForceApplySelected(const QSet<QString> &disabledIds,
                                                                  bool authenticStaged)
{
    ShortcutForcePreview preview = buildForcePreviewFor(disabledIds, authenticStaged);
    logForcePreview("force-preview", preview);
    return preview;
}

ShortcutForceApplyResult ShortcutReconciler::applyForced(const ShortcutForcePreview &confirmed)
{
    return applyForcedSelected(confirmed, QSet<QString>(), true);
}

ShortcutForceApplyResult ShortcutReconciler::applyForcedSelected(const ShortcutForcePreview &confirmed,
                                                                 const QSet<QString> &disabledIds)
{
    return applyForcedSelected(confirmed, disabledIds, true);
}

ShortcutForceApplyResult ShortcutReconciler::applyForcedSelected(const ShortcutForcePreview &confirmed,
                                                                 const QSet<QString> &disabledIds,
                                                                 bool authenticStaged)
{
    ShortcutForceApplyResult result;
    auto fail = [&](const QString &message, const char *outcome) {
        result.ok = false;
        result.error = message;
        result.writes = 0;
        ShortcutDiag::log(QtWarningMsg, "force-apply", "confirm", outcome, message);
        return result;
    };
    if (!m_store || !m_cleared) {
        if (!m_store) {
            return fail(QStringLiteral("reconciler is not configured"), "unconfigured");
        }
        return fail(QStringLiteral("cleared shortcut store is not configured"), "unconfigured");
    }
    {
        QString idError;
        if (!disabledIdsValid(disabledIds, &idError)) {
            return fail(idError, "unbounded");
        }
    }
    if (!confirmed.forceable) {
        return fail(QStringLiteral("no confirmed forced override to apply"), "no-confirmation");
    }
    if (confirmed.mismatches.isEmpty() || confirmed.mismatches.size() > SHORTCUT_MAX_TUPLES) {
        return fail(QStringLiteral("forced override is unbounded"), "unbounded");
    }
    if (!confirmed.blockers.isEmpty()) {
        return fail(confirmed.error.isEmpty()
                        ? QStringLiteral("refusing force: default-only claimant blocks force")
                        : confirmed.error,
                    "blocked");
    }
    // The preview binds its exact staged draft and Authentic intent: a
    // draft edit cancels the pending preview, and any residual mismatch
    // fails here as stale with zero writes.
    {
        QStringList current = QStringList(disabledIds.begin(), disabledIds.end());
        current.sort();
        if (current != confirmed.disabledIds) {
            return fail(QStringLiteral("confirmed force image is stale; re-preview before forcing"),
                        "stale-draft");
        }
        if (confirmed.authenticStaged != authenticStaged) {
            return fail(QStringLiteral("confirmed force image is stale; re-preview before forcing"),
                        "stale-intent");
        }
    }
    // No arbitrary action/key inputs: every confirmed row must carry a
    // bounded identity with removals/remainder exactly derived from its
    // actuals under the confirmed draft and intent. Anything forged fails
    // closed here.
    const QList<int> keepKeys = keepKeysForImages(confirmed.liveImages, disabledIds, authenticStaged);
    QStringList seen;
    for (const ShortcutForceMismatch &mismatch : confirmed.mismatches) {
        if (!stringValid(mismatch.component) || !stringValid(mismatch.action)
            || !cosmeticValid(mismatch.componentFriendly) || !cosmeticValid(mismatch.friendly)
            || !keysValid(mismatch.actual) || !keysValid(mismatch.expectedPre) || !keysValid(mismatch.post)
            || mismatch.actual.isEmpty() || mismatch.expectedPre.isEmpty()) {
            return fail(QStringLiteral("forced override is unbounded"), "unbounded");
        }
        const QString id = mismatch.component + QStringLiteral("/") + mismatch.action;
        if (seen.contains(id)) {
            return fail(QStringLiteral("forced override is unbounded"), "duplicate");
        }
        seen.append(id);
        if (mismatch.expectedPre != conflictingKeysFor(mismatch.actual, disabledIds, authenticStaged, keepKeys)
            || mismatch.post != remainderAfterClearFor(mismatch.actual, disabledIds, authenticStaged, keepKeys)) {
            return fail(QStringLiteral("forced override is unbounded"), "image-mismatch");
        }
    }
    // Revalidation after confirmation, before any write (including
    // cleared-list writes): recompute the full preflight under the same
    // draft and intent and require the exact confirmed snapshot.
    const ShortcutForcePreview current = buildForcePreviewFor(disabledIds, authenticStaged);
    if (!current.forceable) {
        return fail(current.error.isEmpty() ? QStringLiteral("confirmed force image is stale; re-preview before forcing")
                                            : current.error,
                    "stale");
    }
    if (current.owner != confirmed.owner || current.uid != confirmed.uid
        || current.liveImages != confirmed.liveImages || current.livePresent != confirmed.livePresent
        || current.authenticStaged != confirmed.authenticStaged
        || current.mismatches.size() != confirmed.mismatches.size()
        || current.blockers.size() != confirmed.blockers.size()) {
        return fail(QStringLiteral("confirmed force image is stale; re-preview before forcing"), "stale");
    }
    for (int i = 0; i < current.mismatches.size(); ++i) {
        const ShortcutForceMismatch &a = current.mismatches.at(i);
        const ShortcutForceMismatch &b = confirmed.mismatches.at(i);
        if (a.component != b.component || a.action != b.action || a.expectedPre != b.expectedPre
            || a.actual != b.actual || a.post != b.post || a.componentFriendly != b.componentFriendly
            || a.friendly != b.friendly) {
            return fail(QStringLiteral("confirmed force image is stale; re-preview before forcing"), "stale");
        }
    }
    for (int i = 0; i < current.blockers.size(); ++i) {
        const ShortcutForceBlocker &a = current.blockers.at(i);
        const ShortcutForceBlocker &b = confirmed.blockers.at(i);
        if (a.component != b.component || a.action != b.action || a.key != b.key || a.active != b.active) {
            return fail(QStringLiteral("confirmed force image is stale; re-preview before forcing"), "stale");
        }
    }
    {
        const ShortcutLockRelocation &a = current.lock;
        const ShortcutLockRelocation &b = confirmed.lock;
        if (a.inScope != b.inScope || a.actual != b.actual || a.post != b.post || a.hasPre != b.hasPre
            || a.hasTarget != b.hasTarget || a.authorizedHolderPresent != b.authorizedHolderPresent
            || a.authorizedComponent != b.authorizedComponent || a.authorizedAction != b.authorizedAction
            || a.authorizedActive != b.authorizedActive
            || a.authorizedComponentFriendly != b.authorizedComponentFriendly
            || a.authorizedFriendly != b.authorizedFriendly) {
            return fail(QStringLiteral("confirmed force image is stale; re-preview before forcing"), "stale");
        }
    }
    const int startWrites = m_store->writeCount();
    auto usedWrites = [&]() {
        return m_store->writeCount() - startWrites;
    };
    // Operation-scoped budget: every confirmed cleared holder plus the full
    // project assignment that follows. Checked before every foreign write
    // so the overflowing write never happens.
    const int forceBudget = shortcutMaxForceWrites(confirmed.mismatches.size());
    // Durable union BEFORE clearing: an interrupted Force stays revertible.
    QList<ClearedAction> existing;
    QString storeError;
    if (!m_cleared->load(&existing, &storeError)) {
        return fail(storeError.isEmpty() ? QStringLiteral("cleared shortcut load failed") : storeError,
                    "persist-failed");
    }
    QList<ClearedAction> merged = existing;
    for (const ShortcutForceMismatch &mismatch : confirmed.mismatches) {
        ClearedAction entry;
        entry.component = mismatch.component;
        entry.action = mismatch.action;
        // Union by ID only: transient preview labels are never persisted.
        bool found = false;
        for (const ClearedAction &prior : merged) {
            if (clearedActionSameId(prior, entry)) {
                found = true;
                break;
            }
        }
        if (!found) {
            merged.append(entry);
        }
    }
    if (!m_cleared->save(merged, &storeError)) {
        return fail(storeError.isEmpty() ? QStringLiteral("cleared shortcut persist failed") : storeError,
                    "persist-failed");
    }
    const QString owner = current.owner;
    const uint uid = current.uid;
    auto checkOwner = [&](QString *ownerError) {
        QString liveOwner;
        uint liveUid = 0;
        if (!m_store->currentOwner(&liveOwner, &liveUid, ownerError)) {
            return false;
        }
        if (liveOwner != owner || liveUid != uid) {
            if (ownerError) {
                *ownerError = QStringLiteral("KGlobalAccel service owner drifted");
            }
            return false;
        }
        return true;
    };
    // Clear only the conflicting keys from each holder; unrelated keys stay.
    // Fresh per-holder re-read immediately before each foreign setter: the
    // confirmed post image is stale, so abort when the live active list no
    // longer matches the confirmed actual. This keeps a concurrent
    // unrelated-key change from being erased. Owner is re-checked first.
    // The persisted union is already written here, so this abort retains
    // the list (a superset when the drifted holder was never cleared);
    // Revert may then restore an action Force never cleared.
    for (const ShortcutForceMismatch &mismatch : confirmed.mismatches) {
        if (!checkOwner(&storeError)) {
            result.ok = false;
            result.error = storeError;
            result.writes = usedWrites();
            ShortcutDiag::log(QtWarningMsg, "force-apply", "clear", "drifted", storeError);
            return result;
        }
        {
            const int probeKey = mismatch.expectedPre.isEmpty() ? 0 : mismatch.expectedPre.first();
            QList<ShortcutKeyHolder> liveHolders;
            QString keyError;
            if (probeKey == 0 || !m_store->shortcutsByKey(probeKey, &liveHolders, &keyError)) {
                result.ok = false;
                result.error = keyError.isEmpty()
                    ? QStringLiteral("confirmed force image is stale; re-preview before forcing")
                    : keyError;
                result.writes = usedWrites();
                ShortcutDiag::log(QtWarningMsg, "force-apply", "clear", "stale", result.error);
                return result;
            }
            bool found = false;
            for (const ShortcutKeyHolder &live : liveHolders) {
                if (live.component == mismatch.component && live.action == mismatch.action) {
                    found = true;
                    if (live.active != mismatch.actual) {
                        result.ok = false;
                        result.error =
                            QStringLiteral("confirmed force image is stale; re-preview before forcing");
                        result.writes = usedWrites();
                        ShortcutDiag::log(QtWarningMsg, "force-apply", "clear", "stale", result.error);
                        return result;
                    }
                    break;
                }
            }
            if (!found) {
                result.ok = false;
                result.error = QStringLiteral("confirmed force image is stale; re-preview before forcing");
                result.writes = usedWrites();
                ShortcutDiag::log(QtWarningMsg, "force-apply", "clear", "stale", result.error);
                return result;
            }
        }
        if (usedWrites() >= forceBudget) {
            result.ok = false;
            result.error = QStringLiteral("tuple writes exceed the exact %1 writes max").arg(forceBudget);
            result.writes = usedWrites();
            ShortcutDiag::log(QtWarningMsg, "force-apply", "clear", "failed", result.error);
            return result;
        }
        if (!m_store->setForeignShortcutKeys(mismatch.component, mismatch.action, mismatch.componentFriendly,
                                             mismatch.friendly, mismatch.post, &storeError)) {
            result.ok = false;
            result.error =
                storeError.isEmpty() ? QStringLiteral("setForeignShortcutKeys call failed") : storeError;
            result.writes = usedWrites();
            ShortcutDiag::log(QtWarningMsg, "force-apply", "clear", "failed", result.error);
            return result;
        }
    }
    // Project assignments from a fresh read now that holders are cleared.
    const ShortcutApplyResult projects = writeProjectKeysFor("force-apply", disabledIds, authenticStaged);
    if (!projects.ok) {
        result.ok = false;
        result.error = projects.error;
        result.writes = usedWrites();
        ShortcutDiag::log(QtWarningMsg, "force-apply", "finish", "failed",
                          QStringLiteral("writes=%1").arg(result.writes));
        return result;
    }
    // Honest Keep check against the confirmed image: a Keep assignment that
    // changed during clearing or project writes fails here without reset
    // (no corrective writes). The project-phase snapshot cannot see drift
    // that landed before it was taken.
    {
        QList<ShortcutTuple> fresh;
        if (!m_store->readAll(&fresh, &storeError)) {
            result.ok = false;
            result.error = storeError.isEmpty() ? QStringLiteral("allShortcutInfos call failed") : storeError;
            result.writes = usedWrites();
            ShortcutDiag::log(QtWarningMsg, "force-apply", "finish", "failed", result.error);
            return result;
        }
        const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
        const int rows = qMin(catalog.size(), confirmed.liveImages.size());
        for (int i = 0; i < rows; ++i) {
            const ShortcutCatalogEntry &entry = catalog.at(i);
            const QString id = shortcutCatalogId(entry.component, entry.action);
            if (disabledIds.contains(id) || authenticStaged) {
                continue; // cleared/canonical images already verified above
            }
            int matches = 0;
            QList<int> live;
            for (const ShortcutTuple &tuple : fresh) {
                if (tuple.component == entry.component && tuple.action == entry.action) {
                    ++matches;
                    live = tuple.active;
                }
            }
            if (matches != 1 || live != confirmed.liveImages.at(i)) {
                result.ok = false;
                result.error = QStringLiteral(
                    "finish-apply verification failed: live state differs from the expected image");
                result.writes = usedWrites();
                ShortcutDiag::log(QtWarningMsg, "force-apply", "finish", "failed", result.error);
                return result;
            }
        }
    }
    result.ok = true;
    result.error.clear();
    result.writes = usedWrites();
    ShortcutDiag::log(QtInfoMsg, "force-apply", "finish", "ok",
                      QStringLiteral("writes=%1").arg(result.writes));
    return result;
}

} // namespace KWin
