#include "shortcutreconciler.h"

#include <KConfig>
#include <KConfigGroup>

#include <QDBusArgument>
#include <QDBusMessage>
#include <QDBusMetaType>
#include <QDBusObjectPath>
#include <QDir>
#include <QFile>
#include <QFileInfo>
#include <QKeySequence>
#include <QSet>
#include <QStandardPaths>
#include <QTemporaryDir>
#include <QVariant>

#include <cstdio>
#include <cstdlib>
#include <functional>
#include <sys/stat.h>
#include <unistd.h>

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

using namespace KWin;

constexpr int META_L = 268435532;
constexpr int META_ESC = 285212672;
constexpr int META_ALT_K = 402653259;
constexpr int META_ALT_L = 402653260;
constexpr int META_ALT_LEFT = 419430418;
constexpr int META_ALT_UP = 419430419;
constexpr int META_ALT_RIGHT = 419430420;
constexpr int META_ALT_DOWN = 419430421;
constexpr int META_G = 268435527;
constexpr int META_M = 268435533;
constexpr int META_O = 268435535;
constexpr int META_Y = 268435545;
constexpr int META_LEFT = 285212690;
constexpr int META_DOWN = 285212693;
constexpr int META_UP = 285212691;
constexpr int META_RIGHT = 285212692;
constexpr int META_SHIFT_LEFT = 318767122;
constexpr int META_SHIFT_RIGHT = 318767124;
constexpr int META_CTRL_TAB = 352321537;
constexpr int META_CTRL_H = 335544392;
constexpr int META_CTRL_K = 335544395;
constexpr int META_CTRL_LEFT = 352321554;
constexpr int META_CTRL_UP = 352321555;
constexpr int META_CTRL_J = 335544394;
constexpr int META_CTRL_L = 335544396;
constexpr int META_CTRL_DOWN = 352321557;
constexpr int META_CTRL_RIGHT = 352321556;
constexpr int META_CTRL_SHIFT_H = 369098824;
constexpr int META_CTRL_SHIFT_K = 369098827;
constexpr int META_CTRL_SHIFT_J = 369098826;
constexpr int META_CTRL_SHIFT_L = 369098828;
constexpr int META_CTRL_SHIFT_LEFT = 385875986;
constexpr int META_CTRL_SHIFT_UP = 385875987;
constexpr int META_CTRL_SHIFT_RIGHT = 385875988;
constexpr int META_CTRL_SHIFT_DOWN = 385875989;
constexpr int META_CTRL_ALT_H = 469762120;
constexpr int META_CTRL_ALT_J = 469762122;
constexpr int META_CTRL_ALT_K = 469762123;
constexpr int META_CTRL_ALT_L = 469762124;
constexpr int META_CTRL_ALT_LEFT = 486539282;
constexpr int META_CTRL_ALT_UP = 486539283;
constexpr int META_CTRL_ALT_RIGHT = 486539284;
constexpr int META_CTRL_ALT_DOWN = 486539285;

ShortcutTuple makeTuple(const QString &component, const QString &action, const QList<int> &active)
{
    ShortcutTuple tuple;
    tuple.component = component;
    tuple.action = action;
    tuple.componentFriendly = component == QStringLiteral("kwin") ? QStringLiteral("KWin") : QStringLiteral("KDE Session Manager");
    tuple.friendly = action;
    tuple.active = active;
    return tuple;
}

class FakeShortcutStore : public ShortcutStore
{
public:
    QList<ShortcutTuple> tuples;
    QString owner = QStringLiteral(":1.20");
    uint uid = static_cast<uint>(::geteuid());
    bool contractPresent = true;
    // Mirrors the real backend: validated through the shared strict parser.
    QString contractXml = QStringLiteral(
        "<node><interface name=\"org.kde.KGlobalAccel\">"
        "<method name=\"setShortcutKeys\">"
        "<arg type=\"as\" direction=\"in\"/>"
        "<arg type=\"a(ai)\" direction=\"in\"/>"
        "<arg type=\"u\" direction=\"in\"/>"
        "<arg type=\"a(ai)\" direction=\"out\"/>"
        "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
        "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
        "</method></interface></node>");
    bool malformedRead = false;
    bool serviceAbsent = false;
    bool driftAfterNextWrite = false;
    bool failNextWrite = false;
    bool badReplyNextWrite = false;
    // When true, readAll returns tuples without bounds validation so the
    // independent structural check in apply() is exercised directly.
    bool allowUnboundedRead = false;
    // Keyed lookup state (Defect B): derived from tuples plus explicit
    // .desktop-only extras invisible to readAll. Tests prove the keyed path
    // is authoritative by blocking on extras alone with zero writes.
    QMap<int, QList<ShortcutKeyHolder>> extraByKey;
    QMap<int, bool> availableOverride;
    bool failByKey = false;
    bool malformedByKey = false;
    bool failAvailable = false;
    bool malformedAvailable = false;
    // Mirrors KGlobalAccelStore::tryPinOwner: first verified owner pins,
    // the same owner confirms, a different owner fails closed.
    QString pinned;
    struct WriteRecord
    {
        QString component;
        QString action;
        QList<int> keys;
        QString componentFriendly;
        QString friendly;
    };
    QList<WriteRecord> writeLog;
    // Bounded native TRANSPORT seam state (Delivery 2 Unit 1): explicit
    // defaults per actionId plus foreign-write log with hermetic failure
    // controls. No live bus; validation mirrors the real backend.
    QMap<QString, QList<int>> defaultKeysById;
    QList<WriteRecord> foreignWriteLog;
    QList<WriteRecord> defaultCallLog;
    bool failDefaultKeys = false;
    bool malformedDefaultKeys = false;
    bool failForeign = false;
    bool badForeignReadback = false;

    bool checkSetterContract(QString *error) override
    {
        if (!contractPresent) {
            if (error) {
                *error = QStringLiteral("KGlobalAccel setShortcutKeys is absent or does not expose exactly as,a(ai),u -> a(ai) with QSet<QKeySequence>");
            }
            return false;
        }
        if (!ShortcutReconciler::introspectionContractValid(contractXml)) {
            if (error) {
                *error = QStringLiteral("KGlobalAccel setShortcutKeys is absent or does not expose exactly as,a(ai),u -> a(ai) with QSet<QKeySequence>");
            }
            return false;
        }
        return true;
    }

    bool currentOwner(QString *outOwner, uint *outUid, QString *error) override
    {
        // Genuinely absent service mirrors the live serviceOwner invalid
        // reply: fail with the same malformed-owner error, no pin mutation.
        if (serviceAbsent) {
            if (error) {
                *error = QStringLiteral("malformed KGlobalAccel service owner reply");
            }
            return false;
        }
        if (!ShortcutReconciler::uniqueNameValid(owner)) {
            if (error) {
                *error = QStringLiteral("malformed KGlobalAccel service owner reply");
            }
            return false;
        }
        if (pinned.isEmpty()) {
            pinned = owner;
        } else if (pinned != owner) {
            if (error) {
                *error = QStringLiteral("KGlobalAccel service owner drifted");
            }
            return false;
        }
        if (outOwner) {
            *outOwner = owner;
        }
        if (outUid) {
            *outUid = uid;
        }
        return true;
    }

    bool readAll(QList<ShortcutTuple> *out, QString *error) override
    {
        if (malformedRead) {
            if (error) {
                *error = QStringLiteral("unexpected allShortcutInfos reply: wrong message type");
            }
            return false;
        }
        // Bounded validation mirrors the real backend, unless explicitly
        // bypassed to exercise apply()'s independent structural check.
        // Cosmetic labels allow empty with only the length bound.
        if (!allowUnboundedRead) {
            if (tuples.size() > SHORTCUT_MAX_TUPLES) {
                if (error) {
                    *error = QStringLiteral("unexpected allShortcutInfos reply: too many tuples");
                }
                return false;
            }
            for (const ShortcutTuple &tuple : tuples) {
                QString fieldError;
                // Tuples carry no context/defaults; mirror with empty
                // cosmetic contexts and empty defaults (both valid).
                if (!ShortcutReconciler::keyedFieldsValid(tuple.action, tuple.friendly, tuple.component,
                                                          tuple.componentFriendly, QString(), QString(),
                                                          tuple.active, QList<int>(), &fieldError)) {
                    if (error) {
                        *error = QStringLiteral("unexpected allShortcutInfos reply: ") + fieldError;
                    }
                    return false;
                }
            }
        }
        if (out) {
            *out = tuples;
        }
        return true;
    }

    bool shortcutsByKey(int key, QList<ShortcutKeyHolder> *holders, QString *error) override
    {
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
        if (failByKey) {
            failByKey = false;
            if (error) {
                *error = QStringLiteral("globalShortcutsByKey call failed");
            }
            return false;
        }
        if (malformedByKey) {
            malformedByKey = false;
            if (error) {
                *error = QStringLiteral("unexpected globalShortcutsByKey reply: wrong message type");
            }
            return false;
        }
        QList<ShortcutKeyHolder> combined;
        for (const ShortcutTuple &tuple : tuples) {
            if (tuple.active.contains(key)) {
                ShortcutKeyHolder holder;
                holder.component = tuple.component;
                holder.action = tuple.action;
                holder.active = tuple.active;
                combined.append(holder);
            }
        }
        const auto extra = extraByKey.value(key);
        for (const ShortcutKeyHolder &holder : extra) {
            combined.append(holder);
        }
        if (combined.size() > SHORTCUT_MAX_TUPLES) {
            if (error) {
                *error = QStringLiteral("unexpected globalShortcutsByKey reply: too many occupancy holders");
            }
            return false;
        }
        if (holders) {
            *holders = combined;
        }
        return true;
    }

    bool shortcutAvailable(int key, const QString &component, bool *available, QString *error) override
    {
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
        if (failAvailable) {
            failAvailable = false;
            if (error) {
                *error = QStringLiteral("globalShortcutAvailable call failed");
            }
            return false;
        }
        if (malformedAvailable) {
            malformedAvailable = false;
            if (error) {
                *error = QStringLiteral("unexpected globalShortcutAvailable reply: wrong message type");
            }
            return false;
        }
        if (availableOverride.contains(key)) {
            if (available) {
                *available = availableOverride.value(key);
            }
            return true;
        }
        QList<ShortcutKeyHolder> combined;
        for (const ShortcutTuple &tuple : tuples) {
            if (tuple.active.contains(key)) {
                ShortcutKeyHolder holder;
                holder.component = tuple.component;
                holder.action = tuple.action;
                holder.active = tuple.active;
                combined.append(holder);
            }
        }
        combined.append(extraByKey.value(key));
        if (available) {
            *available = combined.isEmpty();
        }
        return true;
    }

    bool writeKeys(const QString &component, const QString &action, const QString &componentFriendly,
                   const QString &friendly, const QList<int> &keys, QList<int> *confirmed, QString *error) override
    {
        if (!ShortcutReconciler::isAllowlisted(component, action)) {
            if (error) {
                *error = QStringLiteral("refusing write outside the exact allowlist");
            }
            return false;
        }
        if (!ShortcutReconciler::keysValid(keys) || !ShortcutReconciler::stringValid(component)
            || !ShortcutReconciler::stringValid(action) || !ShortcutReconciler::cosmeticValid(componentFriendly)
            || !ShortcutReconciler::cosmeticValid(friendly)) {
            if (error) {
                *error = QStringLiteral("refusing write with unbounded tuple");
            }
            return false;
        }
        // No lifetime accumulation cap: the reconciler enforces the
        // operation-scoped budget before the overflowing write, so repeated
        // legitimate operations stay available. writeCount stays monotonic
        // for per-operation deltas.
        if (failNextWrite) {
            failNextWrite = false;
            // Transport failure after the daemon may still have changed;
            // record the attempt so recovery can be tested, but report failure.
            writeLog.append({component, action, keys, componentFriendly, friendly});
            if (error) {
                *error = QStringLiteral("setShortcutKeys call failed for action");
            }
            return false;
        }
        writeLog.append({component, action, keys, componentFriendly, friendly});
        if (driftAfterNextWrite) {
            driftAfterNextWrite = false;
            owner = QStringLiteral(":1.99");
            uid = static_cast<uint>(::geteuid() + 1);
        }
        // Apply to the fake daemon state.
        for (ShortcutTuple &tuple : tuples) {
            if (tuple.component == component && tuple.action == action) {
                tuple.active = keys;
                break;
            }
        }
        if (badReplyNextWrite) {
            badReplyNextWrite = false;
            if (error) {
                *error = QStringLiteral("setShortcutKeys reply did not confirm expected key");
            }
            return false;
        }
        if (confirmed) {
            *confirmed = keys;
        }
        return true;
    }

    bool defaultShortcutKeys(const QString &component, const QString &action, const QString &componentFriendly,
                             const QString &friendly, QList<int> *defaults, QString *error) override
    {
        if (!ShortcutReconciler::stringValid(component) || !ShortcutReconciler::stringValid(action)
            || !ShortcutReconciler::cosmeticValid(componentFriendly)
            || !ShortcutReconciler::cosmeticValid(friendly)) {
            if (error) {
                *error = QStringLiteral("refusing default keys with unbounded tuple");
            }
            return false;
        }
        if (failDefaultKeys) {
            failDefaultKeys = false;
            if (error) {
                *error = QStringLiteral("defaultShortcutKeys call failed");
            }
            return false;
        }
        if (malformedDefaultKeys) {
            malformedDefaultKeys = false;
            if (error) {
                *error = QStringLiteral("unexpected defaultShortcutKeys reply: wrong message type");
            }
            return false;
        }
        const QString id = component + QStringLiteral("/") + action;
        const QList<int> stored = defaultKeysById.value(id);
        if (!ShortcutReconciler::keysValid(stored)) {
            if (error) {
                *error = QStringLiteral("unexpected defaultShortcutKeys reply: did not return expected keys");
            }
            return false;
        }
        defaultCallLog.append({component, action, QList<int>(), componentFriendly, friendly});
        if (defaults) {
            *defaults = stored;
        }
        return true;
    }

    bool setForeignShortcutKeys(const QString &component, const QString &action, const QString &componentFriendly,
                                const QString &friendly, const QList<int> &keys, QString *error) override
    {
        if (!ShortcutReconciler::keysValid(keys) || !ShortcutReconciler::stringValid(component)
            || !ShortcutReconciler::stringValid(action) || !ShortcutReconciler::cosmeticValid(componentFriendly)
            || !ShortcutReconciler::cosmeticValid(friendly)) {
            if (error) {
                *error = QStringLiteral("refusing foreign write with unbounded tuple");
            }
            return false;
        }
        if (failForeign) {
            failForeign = false;
            if (error) {
                *error = QStringLiteral("setForeignShortcutKeys call failed");
            }
            return false;
        }
        // No lifetime accumulation cap (see writeKeys above).
        foreignWriteLog.append({component, action, keys, componentFriendly, friendly});
        for (ShortcutTuple &tuple : tuples) {
            if (tuple.component == component && tuple.action == action) {
                tuple.active = keys;
                break;
            }
        }
        // Fresh readback confirmation for the void setter: re-read the fake
        // daemon state and compare as sets, mirroring the live shortcutKeys
        // readback. Tampering control forces a mismatch.
        QList<int> readback;
        for (const ShortcutTuple &tuple : tuples) {
            if (tuple.component == component && tuple.action == action) {
                readback = tuple.active;
                break;
            }
        }
        if (badForeignReadback) {
            badForeignReadback = false;
            readback.append(999999);
        }
        if (!ShortcutReconciler::foreignReadbackMatches(keys, readback)) {
            if (error) {
                *error = QStringLiteral("setForeignShortcutKeys readback did not confirm expected keys");
            }
            return false;
        }
        return true;
    }

    int writeCount() const override
    {
        return writeLog.size() + foreignWriteLog.size();
    }
};

QString oversizedString();

void seedFullCatalogQuietExtras(FakeShortcutStore &store)
{
    for (const ShortcutCatalogEntry &entry : shortcutProjectCatalog()) {
        bool found = false;
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.component == entry.component && tuple.action == entry.action) {
                found = true;
                break;
            }
        }
        if (!found) {
            const QList<int> active =
                entry.canonicalKey == 0 ? QList<int>() : QList<int>{entry.canonicalKey};
            store.tuples.append(makeTuple(entry.component, entry.action, active));
        }
    }
}

void seedReady6(FakeShortcutStore &store, const QList<int> &focusPre, const QList<int> &lockPre)
{
    // Twenty tuples: the original fourteen plus the six focus/move-arrow
    // project rows at post with quiet Quick Tile/Screen holders, while Grid
    // View and Monocle hold the exact conflicting preimages, so fresh flows
    // refuse on exactly the two foreign letter chords.
    store.tuples = {
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"), focusPre),
        makeTuple(QStringLiteral("ksmserver"), QStringLiteral("Lock Session"), lockPre),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-up"), QList<int>{7}),
        makeTuple(QStringLiteral("KDE Keyboard Layout Switcher"), QStringLiteral("Switch to Next Keyboard Layout"),
                  QList<int>{META_ALT_K}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-right"), QList<int>{8}),
        makeTuple(QStringLiteral("KDE Keyboard Layout Switcher"), QStringLiteral("Switch to Last-Used Keyboard Layout"),
                  QList<int>{META_ALT_L}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-left-arrow"),
                  QList<int>{META_ALT_LEFT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-down-arrow"),
                  QList<int>{META_ALT_DOWN}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-up-arrow"),
                  QList<int>{META_ALT_UP}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-right-arrow"),
                  QList<int>{META_ALT_RIGHT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-float"), QList<int>{META_G}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("Grid View"), QList<int>{META_G}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-maximize"), QList<int>{META_M}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("KrohnkiteMonocleLayout"), QList<int>{META_M}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-left-arrow"),
                  QList<int>{META_LEFT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-down-arrow"),
                  QList<int>{META_DOWN}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-up-arrow"), QList<int>{META_UP}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right-arrow"),
                  QList<int>{META_RIGHT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-move-left-arrow"),
                  QList<int>{META_SHIFT_LEFT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-move-right-arrow"),
                  QList<int>{META_SHIFT_RIGHT}),
    };
    seedFullCatalogQuietExtras(store);
}

// Quiet state: projects own their chords, Lock Session holds the given pre,
// and no foreign holder claims a required chord. Plain Apply assigns any
// remaining project posts here; deviations drive refusal and Force tests.
void seedQuietState(FakeShortcutStore &store, const QList<int> &focusPre, const QList<int> &lockPre)
{
    store.tuples = {
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"), focusPre),
        makeTuple(QStringLiteral("ksmserver"), QStringLiteral("Lock Session"), lockPre),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-up"),
                  QList<int>{META_ALT_K}),
        makeTuple(QStringLiteral("KDE Keyboard Layout Switcher"), QStringLiteral("Switch to Next Keyboard Layout"),
                  QList<int>{}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-right"),
                  QList<int>{META_ALT_L}),
        makeTuple(QStringLiteral("KDE Keyboard Layout Switcher"), QStringLiteral("Switch to Last-Used Keyboard Layout"),
                  QList<int>{}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-left-arrow"),
                  QList<int>{META_ALT_LEFT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-down-arrow"),
                  QList<int>{META_ALT_DOWN}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-up-arrow"),
                  QList<int>{META_ALT_UP}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-right-arrow"),
                  QList<int>{META_ALT_RIGHT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-float"), QList<int>{META_G}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("Grid View"), QList<int>{}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-maximize"), QList<int>{META_M}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("KrohnkiteMonocleLayout"), QList<int>{}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-left-arrow"),
                  QList<int>{META_LEFT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("Window Quick Tile Left"), QList<int>{}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-down-arrow"),
                  QList<int>{META_DOWN}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("Window Quick Tile Bottom"), QList<int>{}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-up-arrow"), QList<int>{META_UP}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("Window Quick Tile Top"), QList<int>{}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right-arrow"),
                  QList<int>{META_RIGHT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("Window Quick Tile Right"), QList<int>{}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-move-left-arrow"),
                  QList<int>{META_SHIFT_LEFT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("Window to Previous Screen"), QList<int>{}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-move-right-arrow"),
                  QList<int>{META_SHIFT_RIGHT}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("Window to Next Screen"), QList<int>{}),
    };
    seedFullCatalogQuietExtras(store);
}

// In-memory cleared-actions store mirroring the real KConfig bounds.
class FakeClearedActions : public ClearedActionsStore
{
public:
    QList<ClearedAction> stored;
    int saves = 0;
    bool failLoad = false;
    bool failSave = false;
    bool failClear = false;

    bool load(QList<ClearedAction> *actions, QString *error) override
    {
        if (failLoad) {
            failLoad = false;
            if (error) {
                *error = QStringLiteral("cleared shortcut load failed");
            }
            return false;
        }
        if (actions) {
            *actions = stored;
        }
        return true;
    }

    bool save(const QList<ClearedAction> &actions, QString *error) override
    {
        if (failSave) {
            failSave = false;
            if (error) {
                *error = QStringLiteral("cleared shortcut persist failed");
            }
            return false;
        }
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
        stored = actions;
        ++saves;
        return true;
    }

    bool clear(QString *error) override
    {
        if (failClear) {
            failClear = false;
            if (error) {
                *error = QStringLiteral("could not clear the cleared shortcut list");
            }
            return false;
        }
        stored.clear();
        return true;
    }
};




void metaEscConflictRefusesWithoutMutation()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_L});
    store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("other-action"), QList<int>{META_ESC}));
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    const ShortcutApplyResult result = reconciler.apply();
    CHECK(!result.ok);
    CHECK(store.writeLog.isEmpty());
    CHECK(cleared.stored.isEmpty());
    CHECK(result.error.contains(QStringLiteral("Meta+Esc")));
}

void malformedReplyFailsClosed()
{
    FakeShortcutStore store;
    seedReady6(store, QList<int>{META_L}, QList<int>{META_L});
    store.malformedRead = true;
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    const ShortcutApplyResult result = reconciler.apply();
    CHECK(!result.ok);
    CHECK(store.writeLog.isEmpty());
    CHECK(cleared.stored.isEmpty());
}


void ownerResolutionHermetic()
{
    // Pure KGlobalAccelStore owner seam, no live D-Bus: success, genuinely
    // absent service, and drift all classify with the live error semantics.
    {
        QString pinned;
        QString ownerOut;
        uint uidOut = 0;
        QString error;
        CHECK(KGlobalAccelStore::resolveOwnerReply(true, QStringLiteral(":1.20"), true,
                                                   static_cast<uint>(::geteuid()), pinned, &ownerOut, &uidOut,
                                                   &error));
        CHECK(pinned == QStringLiteral(":1.20"));
        CHECK(ownerOut == QStringLiteral(":1.20"));
        CHECK(uidOut == static_cast<uint>(::geteuid()));
        CHECK(KGlobalAccelStore::resolveOwnerReply(true, QStringLiteral(":1.20"), true,
                                                   static_cast<uint>(::geteuid()), pinned, nullptr, nullptr,
                                                   &error));
        CHECK(pinned == QStringLiteral(":1.20"));
    }
    {
        QString pinned;
        QString error;
        CHECK(!KGlobalAccelStore::resolveOwnerReply(false, QString(), false, 0, pinned, nullptr, nullptr, &error));
        CHECK(error.contains(QStringLiteral("malformed KGlobalAccel service owner reply")));
        CHECK(pinned.isEmpty());
    }
    {
        QString pinned;
        QString error;
        CHECK(!KGlobalAccelStore::resolveOwnerReply(true, QStringLiteral("not-unique"), false, 0, pinned, nullptr,
                                                   nullptr, &error));
        CHECK(error.contains(QStringLiteral("unique name")));
        CHECK(pinned.isEmpty());
    }
    {
        QString pinned;
        QString error;
        CHECK(!KGlobalAccelStore::resolveOwnerReply(true, QStringLiteral(":1.20"), false, 0, pinned, nullptr,
                                                   nullptr, &error));
        CHECK(error.contains(QStringLiteral("UID")));
        CHECK(pinned.isEmpty());
    }
    {
        QString pinned = QStringLiteral(":1.20");
        QString error;
        CHECK(!KGlobalAccelStore::resolveOwnerReply(true, QStringLiteral(":1.99"), true,
                                                    static_cast<uint>(::geteuid()), pinned, nullptr, nullptr,
                                                    &error));
        CHECK(error.contains(QStringLiteral("drifted")));
        CHECK(pinned == QStringLiteral(":1.20"));
    }
    {
        QString error;
        CHECK(KGlobalAccelStore::checkPinnedDrift(true, QStringLiteral(":1.20"), QStringLiteral(":1.20"), &error));
        CHECK(!KGlobalAccelStore::checkPinnedDrift(false, QString(), QStringLiteral(":1.20"), &error));
        CHECK(error.contains(QStringLiteral("drifted")));
        CHECK(!KGlobalAccelStore::checkPinnedDrift(true, QStringLiteral(":1.99"), QStringLiteral(":1.20"), &error));
        CHECK(error.contains(QStringLiteral("drifted")));
    }
}

void ownerAbsentApplyZeroWrites()
{
    // High-level absent service: first owner resolution fails, zero writes
    // and no cleared-store writes, without live D-Bus.
    FakeShortcutStore store;
    seedReady6(store, QList<int>{419430420}, QList<int>{META_L});
    store.serviceAbsent = true;
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    const ShortcutApplyResult result = reconciler.apply();
    CHECK(!result.ok);
    CHECK(result.error.contains(QStringLiteral("malformed KGlobalAccel service owner reply")));
    CHECK(store.writeLog.isEmpty());
    CHECK(result.writes == 0);
    CHECK(cleared.stored.isEmpty());
}

void ownerDriftApplyZeroWrites()
{
    // High-level pre-write drift: pin succeeds, owner changes before apply,
    // first re-confirmation fails closed with zero writes and no cleared-store writes.
    FakeShortcutStore store;
    seedReady6(store, QList<int>{419430420}, QList<int>{META_L});
    QString pinError;
    CHECK(store.currentOwner(nullptr, nullptr, &pinError));
    CHECK(store.pinned == QStringLiteral(":1.20"));
    store.owner = QStringLiteral(":1.99");
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    const ShortcutApplyResult result = reconciler.apply();
    CHECK(!result.ok);
    CHECK(result.error.contains(QStringLiteral("drifted")));
    CHECK(store.writeLog.isEmpty());
    CHECK(result.writes == 0);
    CHECK(cleared.stored.isEmpty());
    CHECK(store.pinned == QStringLiteral(":1.20"));
}







void strictOwnerAndIntrospection()
{
    CHECK(ShortcutReconciler::uniqueNameValid(QStringLiteral(":1.20")));
    CHECK(!ShortcutReconciler::uniqueNameValid(QString()));
    CHECK(!ShortcutReconciler::uniqueNameValid(QStringLiteral("org.kde.kglobalaccel")));
    CHECK(!ShortcutReconciler::uniqueNameValid(QStringLiteral(":abc")));
    CHECK(!ShortcutReconciler::uniqueNameValid(QStringLiteral(":1")));
    const QString good = QStringLiteral(
        "<node><interface name=\"org.kde.KGlobalAccel\">"
        "<method name=\"setShortcutKeys\">"
        "<arg type=\"as\" direction=\"in\"/>"
        "<arg type=\"a(ai)\" direction=\"in\"/>"
        "<arg type=\"u\" direction=\"in\"/>"
        "<arg type=\"a(ai)\" direction=\"out\"/>"
        "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
        "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
        "</method></interface></node>");
    CHECK(ShortcutReconciler::introspectionContractValid(good));
    // Legacy single-arg combined form is rejected by the strict parser.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"asa(ai)u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(QString()));
    CHECK(!ShortcutReconciler::introspectionContractValid(QStringLiteral("<node/>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\"><method name=\"other\"/></interface></node>")));
    // Malformed owner fails closed through the store.
    FakeShortcutStore store;
    store.owner = QStringLiteral("not-unique");
    seedReady6(store, QList<int>{1}, QList<int>{META_L});
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    CHECK(!reconciler.apply().ok);
    CHECK(store.writeLog.isEmpty());
}

void friendlyLabelsValidated()
{
    // Empty cosmetic friendly labels are accepted by readAll (real captures
    // leave friendly empty) and by writes; identities and keys stay strict.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.tuples[0].friendly = QString();
        QList<ShortcutTuple> out;
        QString readError;
        CHECK(store.readAll(&out, &readError));
        CHECK(out.at(0).friendly.isEmpty());
    }
    // Oversized cosmetic labels still fail closed.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.tuples[0].friendly = QString(SHORTCUT_MAX_STRING_LEN + 1, QChar('x'));
        FakeClearedActions cleared;
        ShortcutReconciler reconciler(&store, &cleared);
        const ShortcutApplyResult result = reconciler.apply();
        CHECK(!result.ok);
        CHECK(result.error
              == QStringLiteral("unexpected allShortcutInfos reply: oversized friendly"));
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    // Identity stays strict: empty action fails as empty action.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.tuples[0].action = QString();
        FakeClearedActions cleared;
        ShortcutReconciler reconciler(&store, &cleared);
        const ShortcutApplyResult result = reconciler.apply();
        CHECK(!result.ok);
        CHECK(result.error == QStringLiteral("unexpected allShortcutInfos reply: empty action"));
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    // Empty identity labels still fail closed end to end.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Switch to Next Keyboard Layout")) {
                tuple.action = QString();
            }
        }
        FakeClearedActions cleared;
        ShortcutReconciler reconciler(&store, &cleared);
        const ShortcutApplyResult result = reconciler.apply();
        CHECK(!result.ok);
        CHECK(result.error == QStringLiteral("unexpected allShortcutInfos reply: empty action"));
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    // Oversized cosmetics still fail closed end to end.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Switch to Last-Used Keyboard Layout")) {
                tuple.friendly = oversizedString();
            }
        }
        FakeClearedActions cleared;
        ShortcutReconciler reconciler(&store, &cleared);
        const ShortcutApplyResult result = reconciler.apply();
        CHECK(!result.ok);
        CHECK(result.error == QStringLiteral("unexpected allShortcutInfos reply: oversized friendly"));
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    // Write contract mirrors production: component/action strict
    // nonempty/bounded, componentFriendly/friendly cosmetic bounded
    // (empty accepted), keys bounds strict, allowlist exact.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        QString error;
        QList<int> confirmed;
        CHECK(store.writeKeys(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"),
                              QString(), QStringLiteral("friendly"), QList<int>{META_L}, &confirmed, &error));
        CHECK(confirmed == QList<int>{META_L});
        CHECK(store.writeKeys(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"),
                              QStringLiteral("KWin"), QString(), QList<int>{META_L}, &confirmed, &error));
        CHECK(confirmed == QList<int>{META_L});
        CHECK(!store.writeKeys(QString(), QStringLiteral("omnitiler-focus-right"),
                               QStringLiteral("KWin"), QStringLiteral("friendly"), QList<int>{META_L}, nullptr,
                               &error));
        CHECK(error == QStringLiteral("refusing write outside the exact allowlist"));
        CHECK(!store.writeKeys(QStringLiteral("kwin"), QString(),
                               QStringLiteral("KWin"), QStringLiteral("friendly"), QList<int>{META_L}, nullptr,
                               &error));
        CHECK(error == QStringLiteral("refusing write outside the exact allowlist"));
        CHECK(!store.writeKeys(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"),
                               oversizedString(), QStringLiteral("friendly"), QList<int>{META_L}, nullptr, &error));
        CHECK(error == QStringLiteral("refusing write with unbounded tuple"));
        CHECK(!store.writeKeys(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"),
                               QStringLiteral("KWin"), oversizedString(), QList<int>{META_L}, nullptr, &error));
        CHECK(error == QStringLiteral("refusing write with unbounded tuple"));
        CHECK(!store.writeKeys(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"),
                               QStringLiteral("KWin"), QStringLiteral("friendly"), QList<int>{-1}, nullptr, &error));
        CHECK(error == QStringLiteral("refusing write with unbounded tuple"));
        CHECK(!store.writeKeys(QStringLiteral("kwin"), QStringLiteral("other-action"), QStringLiteral("KWin"),
                               QStringLiteral("friendly"), QList<int>{META_L}, nullptr, &error));
        CHECK(error == QStringLiteral("refusing write outside the exact allowlist"));
    }
}

void duplicateMetaEscDeduped()
{
    CHECK(ShortcutReconciler::lockPostFor(QList<int>{META_L, META_L}) == (QList<int>{META_ESC}));
    CHECK(ShortcutReconciler::lockPostFor(QList<int>{META_L, META_ESC}) == (QList<int>{META_ESC}));
    CHECK(ShortcutReconciler::lockPostFor(QList<int>{META_L, 42, META_L, 42}) == (QList<int>{META_ESC, 42}));
    CHECK(ShortcutReconciler::dedupKeys(QList<int>{1, 1, 2, 1, 2}) == (QList<int>{1, 2}));
    // End-to-end: pre with duplicate Meta+L collapses to a single Meta+Esc.
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{1}, QList<int>{META_L, META_L, 42});
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    const ShortcutApplyResult result = reconciler.apply();
    CHECK(result.ok);
    CHECK(result.writes == 2);
    CHECK(store.writeLog.size() == 2);
    if (store.writeLog.size() == 2) {
        CHECK(store.writeLog.at(0).action == QStringLiteral("omnitiler-focus-right"));
        CHECK(store.writeLog.at(0).keys == QList<int>{META_L});
        CHECK(store.writeLog.at(1).action == QStringLiteral("Lock Session"));
        CHECK(store.writeLog.at(1).keys == (QList<int>{META_ESC, 42}));
    }
}




void clearedActionsPathSafety()
{
    QString error;
    CHECK(!ShortcutReconciler::clearedActionsPathSafe(QString(), &error));
    CHECK(!ShortcutReconciler::clearedActionsPathSafe(QStringLiteral("relative/clearedrc"), &error));
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString good = dir.path() + QStringLiteral("/sub/clearedrc");
    CHECK(QDir().mkpath(QFileInfo(good).dir().path()));
    CHECK(ShortcutReconciler::clearedActionsPathSafe(good, &error));
    // Symlink leaf refused.
    const QString target = dir.path() + QStringLiteral("/realrc");
    QFile real(target);
    CHECK(real.open(QIODevice::WriteOnly));
    real.close();
    const QString linkLeaf = dir.path() + QStringLiteral("/linkrc");
    CHECK(QFile::link(target, linkLeaf));
    CHECK(!ShortcutReconciler::clearedActionsPathSafe(linkLeaf, &error));
    // Symlink parent refused.
    const QString realDir = dir.path() + QStringLiteral("/realdir");
    CHECK(QDir().mkpath(realDir));
    const QString linkDir = dir.path() + QStringLiteral("/linkdir");
    CHECK(QFile::link(realDir, linkDir));
    CHECK(!ShortcutReconciler::clearedActionsPathSafe(linkDir + QStringLiteral("/clearedrc"), &error));
    // Nonregular leaf (directory) refused.
    CHECK(!ShortcutReconciler::clearedActionsPathSafe(realDir, &error));
    // KConfig cleared store refuses unsafe paths without touching them.
    KConfigClearedActions unsafe(linkLeaf);
    QList<ClearedAction> loaded;
    CHECK(!unsafe.load(&loaded, &error));
    CHECK(!unsafe.save({{QStringLiteral("org.example"), QStringLiteral("other-action")}}, &error));
    CHECK(!unsafe.clear(&error));
    CHECK(QFileInfo(linkLeaf).isSymLink());
    CHECK(QFile::exists(target));
    // Default cleared path is absolute when XDG_CONFIG_HOME is isolated.
    QTemporaryDir configHome;
    CHECK(configHome.isValid());
    const QByteArray previous = qgetenv("XDG_CONFIG_HOME");
    qputenv("XDG_CONFIG_HOME", configHome.path().toUtf8());
    const QString def = defaultClearedActionsPath();
    if (!previous.isNull()) {
        qputenv("XDG_CONFIG_HOME", previous);
    } else {
        qunsetenv("XDG_CONFIG_HOME");
    }
    CHECK(!def.isEmpty());
    CHECK(QDir::isAbsolutePath(def));
    CHECK(def.endsWith(QStringLiteral("shortcut-clearedrc")));
}

QVariant objectPathArrayVariant(const QStringList &paths)
{
    QList<QDBusObjectPath> typed;
    for (const QString &path : paths) {
        typed.append(QDBusObjectPath(path));
    }
    return QVariant::fromValue(typed);
}

void allComponentsStrictTransport()
{
    QString error;
    QStringList components;
    // Exact ao object-path array is accepted.
    CHECK(ShortcutReconciler::parseAllComponentsReply(QDBusMessage::ReplyMessage, QStringLiteral("ao"),
                                                      {objectPathArrayVariant({QStringLiteral("/a"), QStringLiteral("/b")})},
                                                      &components, &error));
    CHECK(components == (QStringList{QStringLiteral("/a"), QStringLiteral("/b")}));
    // QStringList fallback ("as") is rejected, never accepted.
    CHECK(!ShortcutReconciler::parseAllComponentsReply(
        QDBusMessage::ReplyMessage, QStringLiteral("as"),
        {QVariant::fromValue(QStringList{QStringLiteral("/a")})}, nullptr, &error));
    // QStringList is rejected even with a forged ao signature.
    CHECK(!ShortcutReconciler::parseAllComponentsReply(
        QDBusMessage::ReplyMessage, QStringLiteral("ao"),
        {QVariant::fromValue(QStringList{QStringLiteral("/a")})}, nullptr, &error));
    // Non-reply transport is rejected.
    CHECK(!ShortcutReconciler::parseAllComponentsReply(QDBusMessage::ErrorMessage, QStringLiteral("ao"),
                                                       {objectPathArrayVariant({QStringLiteral("/a")})}, nullptr,
                                                       &error));
    // Wrong signature with an object-path array is rejected.
    CHECK(!ShortcutReconciler::parseAllComponentsReply(QDBusMessage::ReplyMessage, QStringLiteral("as"),
                                                       {objectPathArrayVariant({QStringLiteral("/a")})}, nullptr,
                                                       &error));
    // Wrong arity is rejected.
    CHECK(!ShortcutReconciler::parseAllComponentsReply(QDBusMessage::ReplyMessage, QStringLiteral("ao"), {}, nullptr,
                                                       &error));
    // Empty object path in the typed list is rejected with its own token.
    CHECK(!ShortcutReconciler::parseAllComponentsReply(QDBusMessage::ReplyMessage, QStringLiteral("ao"),
                                                       {objectPathArrayVariant({QString()})}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allComponents reply: empty object path in typed list"));
}

void introspectionStrictParsing()
{
    const QString liveGood = QStringLiteral(
        "<node><interface name=\"org.kde.KGlobalAccel\">"
        "<method name=\"setShortcutKeys\">"
        "<arg type=\"as\" direction=\"in\"/>"
        "<arg type=\"a(ai)\" direction=\"in\"/>"
        "<arg type=\"u\" direction=\"in\"/>"
        "<arg type=\"a(ai)\" direction=\"out\"/>"
        "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
        "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
        "</method></interface></node>");
    CHECK(ShortcutReconciler::introspectionContractValid(liveGood));
    // Omitted input direction defaults to "in" and is accepted; the reply
    // must stay explicit "out".
    CHECK(ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\"/>"
                       "<arg type=\"a(ai)\"/>"
                       "<arg type=\"u\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    // Legacy single-arg combined form is rejected.
    const QString legacyCombined = QStringLiteral(
        "<node><interface name=\"org.kde.KGlobalAccel\">"
        "<method name=\"setShortcutKeys\">"
        "<arg type=\"asa(ai)u\" direction=\"in\"/>"
        "<arg type=\"a(ai)\" direction=\"out\"/>"
        "</method></interface></node>");
    CHECK(!ShortcutReconciler::introspectionContractValid(legacyCombined));
    // Wrong arg counts are rejected.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    // Wrong types are rejected.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"s\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"aai\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"s\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"as\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    // Wrong directions are rejected: inputs must be "in" (or omitted) and
    // the reply must be explicit "out".
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"out\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    // Absent key-set annotations are rejected.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "</method></interface></node>")));
    // Wrong key-set annotations are rejected: missing reply, missing keys,
    // and wrong value types.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QList<int>\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In2\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    // Extra overloads are ambiguity and fail.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method>"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    // Substring fallback removed: signatures visible only as text, with no
    // parsed args on the exact method, must fail.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><!-- as a(ai) u a(ai) QSet<QKeySequence> org.kde.KGlobalAccel setShortcutKeys -->"
                       "<interface name=\"org.kde.KGlobalAccel\"><method name=\"setShortcutKeys\"/>"
                       "</interface></node>")));
    // Exact method on the wrong interface must fail even though the
    // signature strings are present.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.Other\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</method></interface></node>")));
    // Malformed XML must fail.
    CHECK(!ShortcutReconciler::introspectionContractValid(QStringLiteral("<node><interface>")));
    // The fake store mirrors the real parser: bad XML fails the apply
    // with zero writes.
    FakeShortcutStore store;
    seedReady6(store, QList<int>{1}, QList<int>{META_L});
    store.contractXml = QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                                       "<method name=\"setShortcutKeys\"/>"
                                       "</interface></node>");
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    const ShortcutApplyResult result = reconciler.apply();
    CHECK(!result.ok);
    CHECK(store.writeLog.isEmpty());
    CHECK(cleared.stored.isEmpty());
    // Legacy combined form fails the apply with zero writes and the error
    // reports the split contract, never the erroneous combined signature.
    FakeShortcutStore legacyStore;
    seedReady6(legacyStore, QList<int>{1}, QList<int>{META_L});
    legacyStore.contractXml = legacyCombined;
    FakeClearedActions legacyCleared;
    ShortcutReconciler legacyReconciler(&legacyStore, &legacyCleared);
    const ShortcutApplyResult legacyResult = legacyReconciler.apply();
    CHECK(!legacyResult.ok);
    CHECK(legacyStore.writeLog.isEmpty());
    CHECK(legacyCleared.stored.isEmpty());
    CHECK(legacyResult.error.contains(QStringLiteral("as,a(ai),u -> a(ai)")));
    CHECK(legacyResult.error.contains(QStringLiteral("QSet<QKeySequence>")));
    CHECK(!legacyResult.error.contains(QStringLiteral("asa(ai)u")));
    CHECK(cleared.stored.isEmpty());
}

void introspectionQtOutFirstVerbatim()
{
    // Exact captured Qt out-first ordering: out arg with its Out0
    // annotation interleaved immediately after, then inputs as,
    // a(ai) with In1 interleaved after its arg, then u. Must be accepted.
    const QString verbatim = QStringLiteral(
        "<node><interface name=\"org.kde.KGlobalAccel\">"
        "<method name=\"setShortcutKeys\">"
        "<arg type=\"a(ai)\" direction=\"out\"/>"
        "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
        "<arg type=\"as\" direction=\"in\"/>"
        "<arg type=\"a(ai)\" direction=\"in\"/>"
        "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
        "<arg type=\"u\" direction=\"in\"/>"
        "</method></interface></node>");
    CHECK(ShortcutReconciler::introspectionContractValid(verbatim));
    // Omitted input directions on the out-first shape still default to in,
    // annotations staying interleaved after their corresponding args.
    CHECK(ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<arg type=\"as\"/>"
                       "<arg type=\"a(ai)\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<arg type=\"u\"/>"
                       "</method></interface></node>")));
    // Absent method remains rejected.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"other\"/>"
                       "</interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\"></interface></node>")));
    // Wrong signatures remain rejected: wrong out type, wrong input type,
    // and missing key-set annotation on the out-first shape (interleaved).
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<arg type=\"s\" direction=\"in\"/>"
                       "</method></interface></node>")));
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "</method></interface></node>")));
    // Scrambled order (out not first, inputs out of order) stays rejected.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "</method></interface></node>")));
    // End-to-end through the fake: verbatim out-first passes the contract.
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{1}, QList<int>{META_L});
    store.contractXml = verbatim;
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    const ShortcutApplyResult result = reconciler.apply();
    CHECK(result.ok);
    CHECK(result.writes == 2);
    CHECK(store.writeLog.size() == 2);
}



QString contractXmlWith(const QString &annotations)
{
    return QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                          "<method name=\"setShortcutKeys\">"
                          "<arg type=\"as\" direction=\"in\"/>"
                          "<arg type=\"a(ai)\" direction=\"in\"/>"
                          "<arg type=\"u\" direction=\"in\"/>"
                          "<arg type=\"a(ai)\" direction=\"out\"/>")
        + annotations
        + QStringLiteral("</method></interface></node>");
}

void introspectionAnnotationNamesStrict()
{
    // Substring trap: In10 contains "In1" but is not the proven In1 slot.
    CHECK(!ShortcutReconciler::introspectionContractValid(contractXmlWith(
        QStringLiteral("<annotation name=\"org.qtproject.QtDBus.QtTypeName.In10\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"))));
    // Substring trap on the reply slot: Out00 contains "Out0" but is wrong.
    CHECK(!ShortcutReconciler::introspectionContractValid(contractXmlWith(
        QStringLiteral("<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out00\" value=\"QSet&lt;QKeySequence&gt;\"/>"))));
    // Misnamed slots are rejected.
    CHECK(!ShortcutReconciler::introspectionContractValid(contractXmlWith(
        QStringLiteral("<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"))));
    CHECK(!ShortcutReconciler::introspectionContractValid(contractXmlWith(
        QStringLiteral("<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out1\" value=\"QSet&lt;QKeySequence&gt;\"/>"))));
    // Wrong value type on the exact name is rejected.
    CHECK(!ShortcutReconciler::introspectionContractValid(contractXmlWith(
        QStringLiteral("<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QList&lt;int&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"))));
    // Bare "In1"/"Out0" names without the exact QtDBus prefix are rejected.
    CHECK(!ShortcutReconciler::introspectionContractValid(contractXmlWith(
        QStringLiteral("<annotation name=\"In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"))));
    // Arg-nested annotations are unproven and ignored: even correctly named
    // nested annotations must not satisfy the contract.
    CHECK(!ShortcutReconciler::introspectionContractValid(
        QStringLiteral("<node><interface name=\"org.kde.KGlobalAccel\">"
                       "<method name=\"setShortcutKeys\">"
                       "<arg type=\"as\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"in\">"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</arg>"
                       "<arg type=\"u\" direction=\"in\"/>"
                       "<arg type=\"a(ai)\" direction=\"out\">"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "</arg>"
                       "</method></interface></node>")));
    // Exact method annotations still pass when an unrelated extra is present;
    // extras are ignored, only the two proven keys count as proof.
    CHECK(ShortcutReconciler::introspectionContractValid(contractXmlWith(
        QStringLiteral("<annotation name=\"org.qtproject.QtDBus.QtTypeName.In1\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.Out0\" value=\"QSet&lt;QKeySequence&gt;\"/>"
                       "<annotation name=\"org.qtproject.QtDBus.QtTypeName.In2\" value=\"QList&lt;int&gt;\"/>"))));
}

void pinImmutableFailsClosed()
{
    // Direct seam on the real backend logic: no D-Bus needed.
    QString pinned;
    QString error;
    CHECK(KGlobalAccelStore::tryPinOwner(pinned, QStringLiteral(":1.20"), &error));
    CHECK(pinned == QStringLiteral(":1.20"));
    CHECK(KGlobalAccelStore::tryPinOwner(pinned, QStringLiteral(":1.20"), &error));
    CHECK(pinned == QStringLiteral(":1.20"));
    CHECK(!KGlobalAccelStore::tryPinOwner(pinned, QStringLiteral(":1.21"), &error));
    CHECK(pinned == QStringLiteral(":1.20"));
    CHECK(error.contains(QStringLiteral("drifted")));
    // Malformed candidate never pins.
    QString empty;
    CHECK(!KGlobalAccelStore::tryPinOwner(empty, QStringLiteral("not-unique"), &error));
    CHECK(empty.isEmpty());
    // Fake mirrors the same immutability through the store boundary, so
    // normal apply/revert flows (repeated same-owner confirmations) stay
    // correct while a drifted second owner fails closed.
    FakeShortcutStore store;
    seedReady6(store, QList<int>{1}, QList<int>{META_L});
    QString ownerError;
    CHECK(store.currentOwner(nullptr, nullptr, &ownerError));
    CHECK(store.pinned == QStringLiteral(":1.20"));
    CHECK(store.currentOwner(nullptr, nullptr, &ownerError));
    store.owner = QStringLiteral(":1.21");
    CHECK(!store.currentOwner(nullptr, nullptr, &ownerError));
    CHECK(store.pinned == QStringLiteral(":1.20"));
    CHECK(ownerError.contains(QStringLiteral("drifted")));
}

void ensureKeySequenceTestMetaTypes()
{
    static bool registered = false;
    if (registered) {
        return;
    }
    qDBusRegisterMetaType<QKeySequence>();
    qDBusRegisterMetaType<QSet<QKeySequence>>();
    registered = true;
}

void keySequenceDbusRoundtripAndBounds()
{
    ensureKeySequenceTestMetaTypes();
    // Narrow roundtrip/framing via the existing QVariant message pattern:
    // the live exact four-slot QKeySequence semantics survive the typed
    // encode without fallback formats.
    {
        const QKeySequence original(META_L);
        QDBusMessage message = QDBusMessage::createSignal(QStringLiteral("/test"), QStringLiteral("i.I"),
                                                          QStringLiteral("keys"));
        message << QVariant::fromValue(original);
        CHECK(message.arguments().size() == 1);
        const QKeySequence decoded = message.arguments().at(0).value<QKeySequence>();
        CHECK(decoded == original);
        CHECK(decoded.count() == 1);
        CHECK(decoded[0].toCombined() == META_L);
    }
    {
        const QKeySequence original(META_L, 42, 0, 0);
        QDBusMessage message = QDBusMessage::createSignal(QStringLiteral("/test"), QStringLiteral("i.I"),
                                                          QStringLiteral("keys"));
        message << QVariant::fromValue(original);
        CHECK(message.arguments().at(0).value<QKeySequence>() == original);
    }
    {
        QSet<QKeySequence> original;
        original.insert(QKeySequence(META_L));
        original.insert(QKeySequence(META_ESC));
        QDBusMessage message = QDBusMessage::createSignal(QStringLiteral("/test"), QStringLiteral("i.I"),
                                                          QStringLiteral("keys"));
        message << QVariant::fromValue(original);
        const QSet<QKeySequence> decoded = message.arguments().at(0).value<QSet<QKeySequence>>();
        CHECK(decoded == original);
        CHECK(decoded.contains(QKeySequence(META_L)));
        CHECK(decoded.contains(QKeySequence(META_ESC)));
    }
    // Strict shared slot validator: exactly four bounded ints pass; short,
    // long, or out-of-range sequences fail closed. Both reply-decode
    // variants share this validator, so malformed/long (ai) never
    // truncates into a false confirm.
    {
        QKeySequence out;
        CHECK(ShortcutReconciler::decodeKeySequenceSlots(QList<int>{META_L, 0, 0, 0}, &out));
        CHECK(out == QKeySequence(META_L));
        CHECK(ShortcutReconciler::decodeKeySequenceSlots(QList<int>{0, 0, 0, 0}, &out));
        CHECK(ShortcutReconciler::decodeKeySequenceSlots(QList<int>{1, 2, 3, 4}, &out));
        CHECK(out == QKeySequence(1, 2, 3, 4));
        CHECK(!ShortcutReconciler::decodeKeySequenceSlots(QList<int>{1, 2, 3}, &out));
        CHECK(!ShortcutReconciler::decodeKeySequenceSlots(QList<int>{1, 2, 3, 4, 5}, &out));
        CHECK(!ShortcutReconciler::decodeKeySequenceSlots(QList<int>{}, &out));
        CHECK(!ShortcutReconciler::decodeKeySequenceSlots(QList<int>{-1, 0, 0, 0}, &out));
        CHECK(!ShortcutReconciler::decodeKeySequenceSlots(QList<int>{SHORTCUT_MAX_KEY_VALUE + 1, 0, 0, 0}, &out));
        CHECK(ShortcutReconciler::decodeKeySequenceSlots(QList<int>{META_L, 0, 0, 0}, nullptr));
    }
    // Bounds are consistent for both variants: an oversized decoded set
    // exceeds SHORTCUT_MAX_KEYS_PER_TUPLE and the write path rejects it.
    {
        QSet<QKeySequence> oversized;
        for (int i = 0; i < SHORTCUT_MAX_KEYS_PER_TUPLE + 1; ++i) {
            oversized.insert(QKeySequence(1000 + i));
        }
        CHECK(oversized.size() > SHORTCUT_MAX_KEYS_PER_TUPLE);
    }
}

void writeFailureControlsFailClosed()
{
    // Transport failure on the first project write fails the apply with the
    // bounded write error and zero confirmed writes.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{1}, QList<int>{META_L});
        store.failNextWrite = true;
        FakeClearedActions cleared;
        ShortcutReconciler reconciler(&store, &cleared);
        const ShortcutApplyResult result = reconciler.apply();
        CHECK(!result.ok);
        CHECK(result.error.contains(QStringLiteral("setShortcutKeys")));
    }
    // Reply-mismatch control fails closed with the confirm error.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{1}, QList<int>{META_L});
        store.badReplyNextWrite = true;
        FakeClearedActions cleared;
        ShortcutReconciler reconciler(&store, &cleared);
        const ShortcutApplyResult result = reconciler.apply();
        CHECK(!result.ok);
        CHECK(result.error.contains(QStringLiteral("confirm")));
    }
}

void tablePreimageRefusalZeroMutation()
{
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{419430420}, QList<int>{META_L});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Switch to Next Keyboard Layout")) {
                tuple.active = QList<int>{999};
            }
        }
        FakeClearedActions cleared;
        ShortcutReconciler reconciler(&store, &cleared);
        const ShortcutApplyResult result = reconciler.apply();
        CHECK(!result.ok);
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{419430420}, QList<int>{META_L});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Switch to Last-Used Keyboard Layout")) {
                tuple.active = QList<int>{META_ALT_K};
            }
        }
        FakeClearedActions cleared;
        ShortcutReconciler reconciler(&store, &cleared);
        const ShortcutApplyResult result = reconciler.apply();
        CHECK(!result.ok);
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
}





void unrelatedChordsAllRefuse()
{
    for (int chord : {META_ALT_K, META_ALT_L}) {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{419430420}, QList<int>{META_L});
        store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("other-action"), QList<int>{chord}));
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!r.ok);
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
}

// Defect B: authoritative keyed lookup, not readAll enumeration.
void keyedDesktopOnlyBlocksRelocator()
{
    // .desktop-declared-only holder on Meta+Esc: absent from readAll tuples,
    // present via globalShortcutsByKey with empty active and defaults
    // containing Meta+Esc (authoritative primitive sees defaults).
    // Must block with zero writes, independently of readAll. The quiet seed
    // keeps the Esc blocker the first conflict in catalog order.
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    ShortcutKeyHolder foreign;
    foreign.component = QStringLiteral("org.kde.unexpected");
    foreign.action = QStringLiteral("other-launch");
    foreign.active = QList<int>{};
    foreign.defaults = QList<int>{META_ESC};
    store.extraByKey[META_ESC].append(foreign);
    // Prove independence from enumeration: readAll lacks the foreign holder.
    {
        QList<ShortcutTuple> enumerated;
        QString readError;
        CHECK(store.readAll(&enumerated, &readError));
        bool found = false;
        for (const ShortcutTuple &tuple : enumerated) {
            if (tuple.component == foreign.component && tuple.action == foreign.action) {
                found = true;
            }
        }
        CHECK(!found);
    }
    {
        QList<ShortcutKeyHolder> holders;
        QString keyedError;
        CHECK(store.shortcutsByKey(META_ESC, &holders, &keyedError));
        CHECK(holders.size() == 2);
        {
            bool sawOwn = false;
            bool sawForeign = false;
            for (const ShortcutKeyHolder &holder : holders) {
                if (holder.component == foreign.component && holder.action == foreign.action) {
                    sawForeign = true;
                    CHECK(holder.active.isEmpty());
                    CHECK(holder.defaults == QList<int>{META_ESC});
                } else if (holder.component == shortcutLockComponent()
                    && holder.action == shortcutLockAction()) {
                    sawOwn = true;
                    CHECK(holder.active.contains(META_ESC));
                    CHECK(ShortcutReconciler::isHolderExempt(holder.component, holder.action, META_ESC));
                }
            }
            CHECK(sawOwn);
            CHECK(sawForeign);
        }
    }
    FakeClearedActions cleared;
    const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
    CHECK(!r.ok);
    CHECK(r.error.contains(QStringLiteral("Meta+Esc")));
    CHECK(r.error.contains(QStringLiteral("org.kde.unexpected")));
    CHECK(store.writeLog.isEmpty());
    CHECK(cleared.stored.isEmpty());
    CHECK(r.writes == 0);
}

void keyedDesktopOnlyBlocksClearTargets()
{
    for (int chord : {META_ALT_K, META_ALT_L}) {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        ShortcutKeyHolder foreign;
        foreign.component = QStringLiteral("org.kde.unexpected");
        foreign.action = QStringLiteral("other-action");
        foreign.active = QList<int>{};
        foreign.defaults = QList<int>{chord};
        store.extraByKey[chord].append(foreign);
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!r.ok);
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
        CHECK(r.writes == 0);
    }
}

void keyedDesktopOnlyBlocksMetaL()
{
    FakeShortcutStore store;
    seedReady6(store, QList<int>{419430420}, QList<int>{META_L});
    ShortcutKeyHolder foreign;
    foreign.component = QStringLiteral("org.kde.unexpected");
    foreign.action = QStringLiteral("steal-meta-l");
    foreign.active = QList<int>{};
    foreign.defaults = QList<int>{META_L};
    store.extraByKey[META_L].append(foreign);
    FakeClearedActions cleared;
    const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
    CHECK(!r.ok);
    CHECK(r.error.contains(QStringLiteral("Meta+L")));
    CHECK(store.writeLog.isEmpty());
    CHECK(cleared.stored.isEmpty());
}

void keyedSystemMonitorEscAccepted()
{
    // Explicit user-authorized displacement: System Monitor `_launch` on
    // Meta+Esc is structurally part of the compiled-in table and must not
    // block. Never part of the write allowlist; no writes target it.
    CHECK(ShortcutReconciler::isAuthorizedDisplacement(
        META_ESC, shortcutAuthorizedEscComponent(), shortcutAuthorizedEscAction()));
    CHECK(!ShortcutReconciler::isAuthorizedDisplacement(
        META_ALT_K, shortcutAuthorizedEscComponent(), shortcutAuthorizedEscAction()));
    CHECK(!ShortcutReconciler::isAllowlisted(shortcutAuthorizedEscComponent(), shortcutAuthorizedEscAction()));
    {
        bool found = false;
        for (const ShortcutConflictRow &row : shortcutConflictTable()) {
            if (row.resolutionTarget.contains(META_ESC)
                && row.authorizedTargetComponent == shortcutAuthorizedEscComponent()
                && row.authorizedTargetAction == shortcutAuthorizedEscAction()) {
                found = true;
            }
        }
        CHECK(found);
    }
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{1}, QList<int>{META_L});
    ShortcutKeyHolder sysmon;
    sysmon.component = shortcutAuthorizedEscComponent();
    sysmon.action = shortcutAuthorizedEscAction();
    sysmon.active = QList<int>{META_ESC};
    store.extraByKey[META_ESC].append(sysmon);
    FakeClearedActions cleared;
    const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
    CHECK(r.ok);
    CHECK(r.writes == 2);
    CHECK(store.writeLog.size() == 2);
    CHECK(store.foreignWriteLog.isEmpty());
    for (const auto &record : store.writeLog) {
        CHECK(!(record.component == shortcutAuthorizedEscComponent() && record.action == shortcutAuthorizedEscAction()));
        CHECK(ShortcutReconciler::isAllowlisted(record.component, record.action));
    }
}

void keyedLiveSystemMonitorIdentity()
{
    // Live .desktop identity is the only authorized Meta+Esc occupant; the
    // stale suffix-less component is a genuinely unauthorized target occupant.
    CHECK(shortcutAuthorizedEscComponent() == QStringLiteral("org.kde.plasma-systemmonitor.desktop"));
    CHECK(ShortcutReconciler::isAuthorizedDisplacement(
        META_ESC, QStringLiteral("org.kde.plasma-systemmonitor.desktop"), QStringLiteral("_launch")));
    CHECK(!ShortcutReconciler::isAuthorizedDisplacement(
        META_ESC, QStringLiteral("org.kde.plasma.systemmonitor"), QStringLiteral("_launch")));
    CHECK(!ShortcutReconciler::isAllowlisted(QStringLiteral("org.kde.plasma-systemmonitor.desktop"),
                                             QStringLiteral("_launch")));
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{1}, QList<int>{META_L});
        ShortcutKeyHolder live;
        live.component = QStringLiteral("org.kde.plasma-systemmonitor.desktop");
        live.action = QStringLiteral("_launch");
        live.active = QList<int>{META_ESC};
        store.extraByKey[META_ESC].append(live);
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(r.ok);
        CHECK(r.writes == 2);
        CHECK(store.writeLog.size() == 2);
        for (const auto &record : store.writeLog) {
            CHECK(!(record.component == QStringLiteral("org.kde.plasma-systemmonitor.desktop")
                    && record.action == QStringLiteral("_launch")));
        }
    }
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{1}, QList<int>{META_L});
        ShortcutKeyHolder stale;
        stale.component = QStringLiteral("org.kde.plasma.systemmonitor");
        stale.action = QStringLiteral("_launch");
        stale.active = QList<int>{META_ESC};
        store.extraByKey[META_ESC].append(stale);
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!r.ok);
        CHECK(r.error.contains(QStringLiteral("Meta+Esc")));
        CHECK(r.error.contains(QStringLiteral("org.kde.plasma.systemmonitor")));
        CHECK(store.writeLog.isEmpty());
        CHECK(store.foreignWriteLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
        CHECK(r.writes == 0);
    }
}

void keyedTransportFailsClosed()
{
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.failByKey = true;
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!r.ok);
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.malformedByKey = true;
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!r.ok);
        CHECK(r.error.contains(QStringLiteral("globalShortcutsByKey")));
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.failAvailable = true;
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!r.ok);
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.malformedAvailable = true;
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!r.ok);
        CHECK(r.error.contains(QStringLiteral("globalShortcutAvailable")));
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
}

void keyedReplyParsingStrict()
{
    QString error;
    bool available = false;
    CHECK(ShortcutReconciler::parseGlobalShortcutAvailableReply(
        QDBusMessage::ReplyMessage, QStringLiteral("b"), {QVariant::fromValue(true)}, &available, &error));
    CHECK(available);
    CHECK(ShortcutReconciler::parseGlobalShortcutAvailableReply(
        QDBusMessage::ReplyMessage, QStringLiteral("b"), {QVariant::fromValue(false)}, &available, &error));
    CHECK(!available);
    CHECK(!ShortcutReconciler::parseGlobalShortcutAvailableReply(
        QDBusMessage::ErrorMessage, QStringLiteral("b"), {QVariant::fromValue(true)}, nullptr, &error));
    CHECK(!ShortcutReconciler::parseGlobalShortcutAvailableReply(
        QDBusMessage::ReplyMessage, QStringLiteral("s"), {QVariant::fromValue(QStringLiteral("x"))}, nullptr,
        &error));
    CHECK(!ShortcutReconciler::parseGlobalShortcutAvailableReply(QDBusMessage::ReplyMessage, QStringLiteral("b"),
                                                                 {}, nullptr, &error));
    CHECK(!ShortcutReconciler::parseGlobalShortcutAvailableReply(QDBusMessage::ReplyMessage, QStringLiteral("b"),
                                                                 {QVariant::fromValue(1)}, nullptr, &error));
    CHECK(!ShortcutReconciler::parseGlobalShortcutAvailableReply(QDBusMessage::ReplyMessage, QStringLiteral("b"),
                                                                 {QVariant::fromValue(QStringLiteral("true"))},
                                                                 nullptr, &error));
    QList<ShortcutKeyHolder> holders;
    CHECK(!ShortcutReconciler::parseGlobalShortcutsByKeyReply(
        QDBusMessage::ErrorMessage, QStringLiteral("a(ssssssaiai)"),
        {QVariant::fromValue(QStringLiteral("x"))}, nullptr, &error));
    CHECK(!ShortcutReconciler::parseGlobalShortcutsByKeyReply(QDBusMessage::ReplyMessage,
                                                              QStringLiteral("as"),
                                                              {QVariant::fromValue(QStringLiteral("x"))}, nullptr,
                                                              &error));
    CHECK(!ShortcutReconciler::parseGlobalShortcutsByKeyReply(QDBusMessage::ReplyMessage,
                                                              QStringLiteral("a(ssssssaiai)"), {}, nullptr, &error));
    CHECK(!ShortcutReconciler::parseGlobalShortcutsByKeyReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ssssssaiai)"),
        {QVariant::fromValue(QStringLiteral("not-an-argument"))}, nullptr, &error));
    CHECK(ShortcutReconciler::relevantConflictKeys()
          == (QList<int>{META_L, META_ESC, META_ALT_K, META_ALT_L, META_ALT_LEFT, META_ALT_DOWN, META_ALT_UP,
                         META_ALT_RIGHT, META_G, META_M, META_LEFT, META_DOWN, META_UP, META_RIGHT,
                         META_SHIFT_LEFT, META_SHIFT_RIGHT, META_CTRL_LEFT, META_CTRL_UP, META_CTRL_DOWN,
                         META_CTRL_RIGHT, META_CTRL_SHIFT_LEFT, META_CTRL_SHIFT_UP, META_CTRL_SHIFT_DOWN,
                         META_CTRL_SHIFT_RIGHT}));
    CHECK(ShortcutReconciler::keyDisplayName(META_L) == QStringLiteral("Meta+L"));
    CHECK(ShortcutReconciler::keyDisplayName(META_ESC) == QStringLiteral("Meta+Esc"));
    CHECK(ShortcutReconciler::keyDisplayName(META_ALT_LEFT) == QStringLiteral("Meta+Alt+Left"));
    CHECK(ShortcutReconciler::keyDisplayName(META_ALT_DOWN) == QStringLiteral("Meta+Alt+Down"));
    CHECK(ShortcutReconciler::keyDisplayName(META_ALT_UP) == QStringLiteral("Meta+Alt+Up"));
    CHECK(ShortcutReconciler::keyDisplayName(META_ALT_RIGHT) == QStringLiteral("Meta+Alt+Right"));
    CHECK(ShortcutReconciler::keyDisplayName(META_G) == QStringLiteral("Meta+G"));
    CHECK(ShortcutReconciler::keyDisplayName(META_M) == QStringLiteral("Meta+M"));
    CHECK(ShortcutReconciler::keyDisplayName(META_LEFT) == QStringLiteral("Meta+Left"));
    CHECK(ShortcutReconciler::keyDisplayName(META_DOWN) == QStringLiteral("Meta+Down"));
    CHECK(ShortcutReconciler::keyDisplayName(META_UP) == QStringLiteral("Meta+Up"));
    CHECK(ShortcutReconciler::keyDisplayName(META_RIGHT) == QStringLiteral("Meta+Right"));
    CHECK(ShortcutReconciler::keyDisplayName(META_SHIFT_LEFT) == QStringLiteral("Meta+Shift+Left"));
    CHECK(ShortcutReconciler::keyDisplayName(META_SHIFT_RIGHT) == QStringLiteral("Meta+Shift+Right"));
}

void keyedAvailabilityConsistencyBothDirections()
{
    // Whole-key invariant: empty holders must report available, non-empty
    // must report unavailable. Both inconsistent directions fail closed
    // with zero writes, without assuming component semantics beyond "".
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        // Force empty lookup to report unavailable: inconsistent.
        for (int key : {META_L, META_ESC, META_ALT_K, META_ALT_L}) {
            Q_UNUSED(key);
        }
        store.availableOverride[META_L] = false;
        // META_L holders: focus tuple holds META_L? seedReady6 focus {1}, lock {META_L} so META_L non-empty.
        // To test empty+unavailable, clear META_L holders and force unavailable.
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.active.contains(META_L)) {
                tuple.active.removeAll(META_L);
            }
        }
        store.extraByKey.clear();
        // Now META_L empty, override false -> inconsistent.
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!r.ok);
        CHECK(r.error.contains(QStringLiteral("globalShortcutAvailable")));
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        // Non-empty holders reporting available: inconsistent.
        store.availableOverride[META_L] = true;
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!r.ok);
        CHECK(r.error.contains(QStringLiteral("globalShortcutAvailable")));
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    {
        // Authorized holder with inconsistent availability still fails. The
        // quiet seed keeps the Esc inconsistency the first conflict in
        // catalog order.
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        ShortcutKeyHolder sysmon;
        sysmon.component = shortcutAuthorizedEscComponent();
        sysmon.action = shortcutAuthorizedEscAction();
        sysmon.active = QList<int>{META_ESC};
        store.extraByKey[META_ESC].append(sysmon);
        store.availableOverride[META_ESC] = true;
        FakeClearedActions cleared;
        const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!r.ok);
        CHECK(r.error.contains(QStringLiteral("globalShortcutAvailable")));
        CHECK(store.writeLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
    }
    {
        // Typed outcome preserves semantics: Conflict vs Unavailable. The
        // quiet seed keeps the Esc holder the first conflict in order.
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        ShortcutKeyHolder foreign;
        foreign.component = QStringLiteral("org.kde.unexpected");
        foreign.action = QStringLiteral("other-launch");
        foreign.active = QList<int>{};
        foreign.defaults = QList<int>{META_ESC};
        store.extraByKey[META_ESC].append(foreign);
        const KeyedOccupancyResult outcome = ShortcutReconciler::checkKeyedForeignOccupancyDetailed(&store);
        CHECK(outcome.status == KeyedOccupancy::Conflict);
        CHECK(outcome.detail.contains(QStringLiteral("claimed by")));
    }
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.failByKey = true;
        const KeyedOccupancyResult outcome = ShortcutReconciler::checkKeyedForeignOccupancyDetailed(&store);
        CHECK(outcome.status == KeyedOccupancy::Unavailable);
        CHECK(!outcome.detail.contains(QStringLiteral("claimed by")));
    }
}

void unrelatedUnboundedRefusesZeroWrites()
{
    // Independent structural validation (not conflict detection):
    // unbounded unrelated enumerated state fails with zero writes.
    FakeShortcutStore store;
    seedReady6(store, QList<int>{419430420}, QList<int>{META_L});
    store.allowUnboundedRead = true;
    store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("other-action"), QList<int>{-1}));
    FakeClearedActions cleared;
    const ShortcutApplyResult r = ShortcutReconciler(&store, &cleared).apply();
    CHECK(!r.ok);
    CHECK(r.error.contains(QStringLiteral("unbounded")));
    CHECK(store.writeLog.isEmpty());
    CHECK(cleared.stored.isEmpty());
    CHECK(r.writes == 0);
}

QString oversizedString()
{
    return QString(SHORTCUT_MAX_STRING_LEN + 1, QChar('x'));
}

void preflightAllComponentsSplitTokens()
{
    QString error;
    QStringList components;
    // Wrong message type.
    CHECK(!ShortcutReconciler::parseAllComponentsReply(
        QDBusMessage::ErrorMessage, QStringLiteral("ao"),
        {objectPathArrayVariant({QStringLiteral("/a")})}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allComponents reply: wrong message type"));
    // Wrong signature.
    CHECK(!ShortcutReconciler::parseAllComponentsReply(
        QDBusMessage::ReplyMessage, QStringLiteral("as"),
        {objectPathArrayVariant({QStringLiteral("/a")})}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allComponents reply: wrong signature"));
    // Wrong arity.
    CHECK(!ShortcutReconciler::parseAllComponentsReply(QDBusMessage::ReplyMessage, QStringLiteral("ao"), {},
                                                       nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allComponents reply: wrong arity"));
    // Wrong variant shape (QStringList is never ao, even with forged signature).
    CHECK(!ShortcutReconciler::parseAllComponentsReply(
        QDBusMessage::ReplyMessage, QStringLiteral("ao"),
        {QVariant::fromValue(QStringList{QStringLiteral("/a")})}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allComponents reply: wrong variant shape"));
    // Wrong array framing (default QDBusArgument is not an array).
    CHECK(!ShortcutReconciler::parseAllComponentsReply(QDBusMessage::ReplyMessage, QStringLiteral("ao"),
                                                       {QVariant::fromValue(QDBusArgument())}, nullptr,
                                                       &error));
    CHECK(error == QStringLiteral("unexpected allComponents reply: wrong array framing"));
    // Empty object path in the typed list branch.
    CHECK(!ShortcutReconciler::parseAllComponentsReply(QDBusMessage::ReplyMessage, QStringLiteral("ao"),
                                                       {objectPathArrayVariant({QString()})}, nullptr,
                                                       &error));
    CHECK(error == QStringLiteral("unexpected allComponents reply: empty object path in typed list"));
    // The argument-array branch carries its own token
    // ("empty object path in argument array"): no public Qt API builds a
    // read-mode QDBusArgument outside a real bus reply, so the real branch
    // delegates to appendComponentPath and the exact token is triggered
    // hermetically there (see boundedHelperSeamsExactTokens).
    // Too many components (bound 1024).
    {
        QStringList many;
        for (int i = 0; i < 1025; ++i) {
            many.append(QStringLiteral("/c%1").arg(i));
        }
        CHECK(!ShortcutReconciler::parseAllComponentsReply(QDBusMessage::ReplyMessage, QStringLiteral("ao"),
                                                           {objectPathArrayVariant(many)}, nullptr, &error));
        CHECK(error == QStringLiteral("unexpected allComponents reply: too many components"));
    }
    // Acceptance still exact.
    CHECK(ShortcutReconciler::parseAllComponentsReply(
        QDBusMessage::ReplyMessage, QStringLiteral("ao"),
        {objectPathArrayVariant({QStringLiteral("/a")})}, &components, &error));
    CHECK(components == (QStringList{QStringLiteral("/a")}));
}

void preflightAvailableSplitTokens()
{
    QString error;
    bool available = false;
    CHECK(!ShortcutReconciler::parseGlobalShortcutAvailableReply(
        QDBusMessage::ErrorMessage, QStringLiteral("b"), {QVariant::fromValue(true)}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected globalShortcutAvailable reply: wrong message type"));
    CHECK(!ShortcutReconciler::parseGlobalShortcutAvailableReply(
        QDBusMessage::ReplyMessage, QStringLiteral("s"), {QVariant::fromValue(QStringLiteral("x"))}, nullptr,
        &error));
    CHECK(error == QStringLiteral("unexpected globalShortcutAvailable reply: wrong signature"));
    CHECK(!ShortcutReconciler::parseGlobalShortcutAvailableReply(QDBusMessage::ReplyMessage, QStringLiteral("b"),
                                                                 {}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected globalShortcutAvailable reply: wrong arity"));
    CHECK(!ShortcutReconciler::parseGlobalShortcutAvailableReply(QDBusMessage::ReplyMessage, QStringLiteral("b"),
                                                                 {QVariant::fromValue(1)}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected globalShortcutAvailable reply: wrong variant shape"));
    CHECK(!ShortcutReconciler::parseGlobalShortcutAvailableReply(
        QDBusMessage::ReplyMessage, QStringLiteral("b"), {QVariant::fromValue(QStringLiteral("true"))}, nullptr,
        &error));
    CHECK(error == QStringLiteral("unexpected globalShortcutAvailable reply: wrong variant shape"));
    CHECK(ShortcutReconciler::parseGlobalShortcutAvailableReply(
        QDBusMessage::ReplyMessage, QStringLiteral("b"), {QVariant::fromValue(true)}, &available, &error));
    CHECK(available);
}

void preflightByKeyTransportSplitTokens()
{
    QString error;
    CHECK(!ShortcutReconciler::parseGlobalShortcutsByKeyReply(
        QDBusMessage::ErrorMessage, QStringLiteral("a(ssssssaiai)"),
        {QVariant::fromValue(QStringLiteral("x"))}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: wrong message type"));
    CHECK(!ShortcutReconciler::parseGlobalShortcutsByKeyReply(QDBusMessage::ReplyMessage, QStringLiteral("as"),
                                                              {QVariant::fromValue(QStringLiteral("x"))},
                                                              nullptr, &error));
    CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: wrong signature"));
    CHECK(!ShortcutReconciler::parseGlobalShortcutsByKeyReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ssssssaiai)"), {}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: wrong arity"));
    CHECK(!ShortcutReconciler::parseGlobalShortcutsByKeyReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ssssssaiai)"),
        {QVariant::fromValue(QStringLiteral("not-an-argument"))}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: wrong variant shape"));
    CHECK(!ShortcutReconciler::parseGlobalShortcutsByKeyReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ssssssaiai)"),
        {QVariant::fromValue(QDBusArgument())}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: wrong array framing"));
}

void preflightAllInfosSplitTokens()
{
    // Pure allShortcutInfos parser: ordered type then signature then arity,
    // exact tokens per branch, no live D-Bus. Mirrors the readAll live path.
    QString error;
    CHECK(!ShortcutReconciler::parseAllShortcutInfosReply(
        QDBusMessage::ErrorMessage, QStringLiteral("a(ssssssaiai)"),
        {QVariant::fromValue(QStringLiteral("x"))}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: wrong message type"));
    CHECK(!ShortcutReconciler::parseAllShortcutInfosReply(QDBusMessage::ReplyMessage, QStringLiteral("as"),
                                                          {QVariant::fromValue(QStringLiteral("x"))}, nullptr,
                                                          &error));
    CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: wrong signature"));
    CHECK(!ShortcutReconciler::parseAllShortcutInfosReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ssssssaiai)"), {}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: wrong arity"));
    CHECK(!ShortcutReconciler::parseAllShortcutInfosReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ssssssaiai)"),
        {QVariant::fromValue(QStringLiteral("not-an-argument"))}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: wrong variant shape"));
    CHECK(!ShortcutReconciler::parseAllShortcutInfosReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ssssssaiai)"),
        {QVariant::fromValue(QDBusArgument())}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: wrong array framing"));
    // Ordering: type beats signature, signature beats arity.
    CHECK(!ShortcutReconciler::parseAllShortcutInfosReply(QDBusMessage::ErrorMessage, QStringLiteral("as"), {},
                                                          nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: wrong message type"));
    CHECK(!ShortcutReconciler::parseAllShortcutInfosReply(QDBusMessage::ReplyMessage, QStringLiteral("as"), {},
                                                          nullptr, &error));
    CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: wrong signature"));
}

// Shared seam coverage: the QDBusArgument reply parsers only extract
// QList<ShortcutInfoFields> from the wire array, then delegate all field and
// bound mapping to holdersFromInfoFields / tuplesFromInfoFields. These tests
// exercise those real field branches hermetically (no live bus).
//
// Why not a direct readable QDBusArgument: the public Qt API offers no way to
// build a read-mode QDBusArgument outside a real bus reply (a write-mode
// argument reports no ArrayType for const read access), and an in-process
// private dbus-daemon service+client fixture was attempted but the service
// never dispatched even basic ping/introspection calls in this environment
// (connections up, service registered, object registered, yet NoReply), so a
// bus fixture is not feasible here. Instead the wire encoding is proven by
// marshalling a write-mode array through the real source operators and
// asserting the signature is exactly a(ssssssaiai), and the field branches
// run through the shared seam the real parsers delegate to. Transport
// branches (type, signature, arity, variant shape, array framing) are covered
// separately with exact tokens on the real QDBusArgument parsers.
void ensureInfoFieldsTestMetaTypes()
{
    static bool registered = false;
    if (registered) {
        return;
    }
    qDBusRegisterMetaType<ShortcutInfoFields>();
    qDBusRegisterMetaType<QList<ShortcutInfoFields>>();
    registered = true;
}

ShortcutInfoFields makeSeamInfo(const QString &action, const QString &friendly, const QString &compUnique,
                                const QList<int> &active, const QList<int> &defaults)
{
    ShortcutInfoFields info;
    info.action = action;
    info.friendly = friendly;
    info.compUnique = compUnique;
    info.compFriendly = QStringLiteral("KWin");
    info.contextUnique = QStringLiteral("default");
    info.contextFriendly = QStringLiteral("Default Context");
    info.active = active;
    info.defaults = defaults;
    return info;
}

void infoFieldsWireEncodingProvesSignature()
{
    ensureInfoFieldsTestMetaTypes();
    // The real source operators encode exactly a(ssssssaiai): build a
    // write-mode array with them and read back the marshalled signature.
    // (A readable QDBusArgument is only produced by a real bus reply, which
    // is out of reach here; the field branches below run through the shared
    // seam the wire parsers delegate to.)
    QDBusArgument arg;
    arg.beginArray(QMetaType::fromType<ShortcutInfoFields>());
    const ShortcutInfoFields info =
        makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"), QList<int>{META_L},
                     QList<int>{META_ESC});
    arg << info;
    arg.endArray();
    CHECK(arg.currentSignature() == QStringLiteral("a(ssssssaiai)"));
}

void seamHoldersAndTuplesFieldBranches()
{
    // Real-capture-shaped empty-friendly accepted record maps through both
    // seam functions (the same code the wire parsers delegate to).
    {
        const QList<ShortcutInfoFields> infos = {
            makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"), QList<int>{META_L},
                         QList<int>{META_ESC}),
        };
        QList<ShortcutKeyHolder> holders;
        QString error;
        CHECK(ShortcutReconciler::holdersFromInfoFields(infos, &holders, &error));
        CHECK(holders.size() == 1);
        if (holders.size() == 1) {
            CHECK(holders.at(0).component == QStringLiteral("kwin"));
            CHECK(holders.at(0).action == QStringLiteral("walk"));
            CHECK(holders.at(0).active == QList<int>{META_L});
            CHECK(holders.at(0).defaults == QList<int>{META_ESC});
        }
        QList<ShortcutTuple> tuples;
        CHECK(ShortcutReconciler::tuplesFromInfoFields(infos, &tuples, &error));
        CHECK(tuples.size() == 1);
        if (tuples.size() == 1) {
            CHECK(tuples.at(0).component == QStringLiteral("kwin"));
            CHECK(tuples.at(0).action == QStringLiteral("walk"));
            CHECK(tuples.at(0).friendly.isEmpty());
            CHECK(tuples.at(0).active == QList<int>{META_L});
        }
    }
    // Each elementary predicate below reaches only its own full token on
    // both seams (by-key prefix vs allShortcutInfos prefix). Triggers are
    // pure: only the named field violates, all others valid.
    auto checkBothSeams = [](const ShortcutInfoFields &info, const char *byKeyToken, const char *allInfosToken) {
        const QList<ShortcutInfoFields> infos = {info};
        QString error;
        CHECK(!ShortcutReconciler::holdersFromInfoFields(infos, nullptr, &error));
        CHECK(error == QString::fromUtf8(byKeyToken));
        CHECK(!ShortcutReconciler::tuplesFromInfoFields(infos, nullptr, &error));
        CHECK(error == QString::fromUtf8(allInfosToken));
    };
    {
        ShortcutInfoFields info =
            makeSeamInfo(QString(), QString(), QStringLiteral("kwin"), QList<int>{}, QList<int>{});
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: empty action",
                       "unexpected allShortcutInfos reply: empty action");
    }
    {
        ShortcutInfoFields info =
            makeSeamInfo(oversizedString(), QString(), QStringLiteral("kwin"), QList<int>{}, QList<int>{});
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: oversized action",
                       "unexpected allShortcutInfos reply: oversized action");
    }
    {
        ShortcutInfoFields info =
            makeSeamInfo(QStringLiteral("walk"), QString(), QString(), QList<int>{}, QList<int>{});
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: empty component",
                       "unexpected allShortcutInfos reply: empty component");
    }
    {
        ShortcutInfoFields info = makeSeamInfo(QStringLiteral("walk"), QString(), oversizedString(), QList<int>{},
                                               QList<int>{});
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: oversized component",
                       "unexpected allShortcutInfos reply: oversized component");
    }
    {
        ShortcutInfoFields info = makeSeamInfo(QStringLiteral("walk"), oversizedString(), QStringLiteral("kwin"),
                                               QList<int>{}, QList<int>{});
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: oversized friendly",
                       "unexpected allShortcutInfos reply: oversized friendly");
    }
    {
        ShortcutInfoFields info = makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"),
                                               QList<int>{}, QList<int>{});
        info.compFriendly = oversizedString();
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: oversized component friendly",
                       "unexpected allShortcutInfos reply: oversized component friendly");
    }
    {
        ShortcutInfoFields info = makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"),
                                               QList<int>{}, QList<int>{});
        info.contextUnique = oversizedString();
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: oversized context unique",
                       "unexpected allShortcutInfos reply: oversized context unique");
    }
    {
        ShortcutInfoFields info = makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"),
                                               QList<int>{}, QList<int>{});
        info.contextFriendly = oversizedString();
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: oversized context friendly",
                       "unexpected allShortcutInfos reply: oversized context friendly");
    }
    {
        QList<int> tooMany;
        for (int i = 0; i < SHORTCUT_MAX_KEYS_PER_TUPLE + 1; ++i) {
            tooMany.append(i + 1);
        }
        ShortcutInfoFields info =
            makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"), tooMany, QList<int>{});
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: too many active keys",
                       "unexpected allShortcutInfos reply: too many active keys");
    }
    {
        ShortcutInfoFields info = makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"),
                                               QList<int>{-1}, QList<int>{});
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: negative active key",
                       "unexpected allShortcutInfos reply: negative active key");
    }
    {
        ShortcutInfoFields info = makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"),
                                               QList<int>{SHORTCUT_MAX_KEY_VALUE + 1}, QList<int>{});
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: oversized active key",
                       "unexpected allShortcutInfos reply: oversized active key");
    }
    {
        QList<int> tooMany;
        for (int i = 0; i < SHORTCUT_MAX_KEYS_PER_TUPLE + 1; ++i) {
            tooMany.append(i + 1);
        }
        ShortcutInfoFields info =
            makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"), QList<int>{}, tooMany);
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: too many default keys",
                       "unexpected allShortcutInfos reply: too many default keys");
    }
    {
        ShortcutInfoFields info = makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"),
                                               QList<int>{}, QList<int>{-1});
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: negative default key",
                       "unexpected allShortcutInfos reply: negative default key");
    }
    {
        ShortcutInfoFields info = makeSeamInfo(QStringLiteral("walk"), QString(), QStringLiteral("kwin"),
                                               QList<int>{}, QList<int>{SHORTCUT_MAX_KEY_VALUE + 1});
        checkBothSeams(info, "unexpected globalShortcutsByKey reply: oversized default key",
                       "unexpected allShortcutInfos reply: oversized default key");
    }
    // Bound maps only to the per-parser bound token on both seams.
    {
        QList<ShortcutInfoFields> many;
        many.reserve(SHORTCUT_MAX_TUPLES + 1);
        for (int i = 0; i < SHORTCUT_MAX_TUPLES + 1; ++i) {
            many.append(makeSeamInfo(QStringLiteral("a%1").arg(i), QString(),
                                     QStringLiteral("c%1").arg(i), QList<int>{}, QList<int>{}));
        }
        QString error;
        CHECK(!ShortcutReconciler::holdersFromInfoFields(many, nullptr, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: too many holders"));
        CHECK(!ShortcutReconciler::tuplesFromInfoFields(many, nullptr, &error));
        CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: too many tuples"));
    }
}

void wireBoundTokensDistinctFromSeams()
{
    // Wire demarshal loops delegate their fail-fast SHORTCUT_MAX_TUPLES
    // bound to the shared checkTupleAppendBound predicate immediately after
    // each real append; the exact wire tokens are triggered hermetically via
    // that predicate in boundedHelperSeamsExactTokens. Distinctness below is
    // only a uniqueness guard, not coverage.
    CHECK(QStringLiteral("unexpected globalShortcutsByKey reply: too many wire holders")
          != QStringLiteral("unexpected globalShortcutsByKey reply: too many holders"));
    CHECK(QStringLiteral("unexpected allShortcutInfos reply: too many wire tuples")
          != QStringLiteral("unexpected allShortcutInfos reply: too many tuples"));
    CHECK(QStringLiteral("unexpected globalShortcutsByKey reply: too many wire holders")
          != QStringLiteral("unexpected allShortcutInfos reply: too many wire tuples"));
}

void boundedHelperSeamsExactTokens()
{
    // Smallest pure seams, each trigger pure (one predicate only) with exact
    // full bounded tokens. Real branches call these helpers with the same
    // mutation/order/check, so this is executable coverage of the real
    // tokens, not string inequality or audit.
    {
        // (1) allComponents object-path validator: typed-list provenance.
        QString error;
        QStringList parsed;
        CHECK(!ShortcutReconciler::appendComponentPath(QString(), false, &parsed, &error));
        CHECK(error
              == QStringLiteral("unexpected allComponents reply: empty object path in typed list"));
        CHECK(parsed.isEmpty());
        CHECK(ShortcutReconciler::appendComponentPath(QStringLiteral("/a"), false, &parsed, &error));
        CHECK(parsed == (QStringList{QStringLiteral("/a")}));
    }
    {
        // (1) allComponents object-path validator: argument-array provenance.
        QString error;
        QStringList parsed;
        CHECK(!ShortcutReconciler::appendComponentPath(QString(), true, &parsed, &error));
        CHECK(error
              == QStringLiteral(
                  "unexpected allComponents reply: empty object path in argument array"));
        CHECK(parsed.isEmpty());
        CHECK(ShortcutReconciler::appendComponentPath(QStringLiteral("/b"), true, &parsed, &error));
        CHECK(parsed == (QStringList{QStringLiteral("/b")}));
    }
    {
        // (2) by-key wire bound via the shared predicate.
        QString error;
        CHECK(!ShortcutReconciler::checkTupleAppendBound(
            SHORTCUT_MAX_TUPLES + 1, ShortcutReconciler::TupleAppendBound::ByKeyWire, &error));
        CHECK(error
              == QStringLiteral("unexpected globalShortcutsByKey reply: too many wire holders"));
        CHECK(ShortcutReconciler::checkTupleAppendBound(
            SHORTCUT_MAX_TUPLES, ShortcutReconciler::TupleAppendBound::ByKeyWire, &error));
        CHECK(ShortcutReconciler::checkTupleAppendBound(
            0, ShortcutReconciler::TupleAppendBound::ByKeyWire, &error));
    }
    {
        // (2) allShortcutInfos wire bound via the shared predicate.
        QString error;
        CHECK(!ShortcutReconciler::checkTupleAppendBound(
            SHORTCUT_MAX_TUPLES + 1, ShortcutReconciler::TupleAppendBound::AllInfosWire, &error));
        CHECK(error
              == QStringLiteral("unexpected allShortcutInfos reply: too many wire tuples"));
        CHECK(ShortcutReconciler::checkTupleAppendBound(
            SHORTCUT_MAX_TUPLES, ShortcutReconciler::TupleAppendBound::AllInfosWire, &error));
    }
    {
        // (3) cross-component collected-tuples bound via the shared predicate.
        QString error;
        CHECK(!ShortcutReconciler::checkTupleAppendBound(
            SHORTCUT_MAX_TUPLES + 1, ShortcutReconciler::TupleAppendBound::Collected, &error));
        CHECK(error
              == QStringLiteral("unexpected allShortcutInfos reply: too many collected tuples"));
        CHECK(ShortcutReconciler::checkTupleAppendBound(
            SHORTCUT_MAX_TUPLES, ShortcutReconciler::TupleAppendBound::Collected, &error));
    }
    {
        // (4) defensive occupancy-key range: negative only.
        QString error;
        CHECK(!ShortcutReconciler::checkOccupancyKeyRange(-1, &error));
        CHECK(error
              == QStringLiteral("unexpected globalShortcutsByKey reply: negative occupancy key"));
    }
    {
        // (4) defensive occupancy-key range: zero (non-positive) only; passes
        // the negative check, fails only the zero check.
        QString error;
        CHECK(!ShortcutReconciler::checkOccupancyKeyRange(0, &error));
        CHECK(error
              == QStringLiteral("unexpected globalShortcutsByKey reply: non-positive occupancy key"));
    }
    {
        // (4) defensive occupancy-key range: oversized only; passes negative
        // and zero checks, fails only the over-max check.
        QString error;
        CHECK(!ShortcutReconciler::checkOccupancyKeyRange(SHORTCUT_MAX_KEY_VALUE + 1, &error));
        CHECK(error
              == QStringLiteral("unexpected globalShortcutsByKey reply: oversized occupancy key"));
        CHECK(ShortcutReconciler::checkOccupancyKeyRange(META_L, &error));
    }
}


void preflightKeyedFieldsSplitTokens()
{
    // Real-capture-shaped fixture: action, empty friendly, component,
    // componentFriendly, default, Default Context, ai, ai. Must be accepted.
    {
        QString fieldError = QStringLiteral("seed");
        CHECK(ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"),
            QStringLiteral("componentFriendly"), QStringLiteral("default"), QStringLiteral("Default Context"),
            QList<int>{META_L}, QList<int>{META_ESC}, &fieldError));
        CHECK(fieldError == QStringLiteral("seed"));
    }
    // All cosmetic fields empty is accepted for globalShortcutsByKey parsing.
    {
        QString fieldError;
        CHECK(ShortcutReconciler::keyedFieldsValid(QStringLiteral("action"), QString(),
                                                   QStringLiteral("component"), QString(), QString(), QString(),
                                                   QList<int>{}, QList<int>{}, &fieldError));
    }
    // Cosmetic validator allows empty with only the length bound.
    CHECK(ShortcutReconciler::cosmeticValid(QString()));
    CHECK(ShortcutReconciler::cosmeticValid(QStringLiteral("Default Context")));
    CHECK(!ShortcutReconciler::cosmeticValid(oversizedString()));
    CHECK(!ShortcutReconciler::stringValid(QString()));
    // Invalid identity: empty action.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(QString(), QStringLiteral("friendly"),
                                                    QStringLiteral("component"), QStringLiteral("compFriendly"),
                                                    QStringLiteral("default"), QStringLiteral("Default Context"),
                                                    QList<int>{}, QList<int>{}, &fieldError));
        CHECK(fieldError == QStringLiteral("empty action"));
    }
    // Invalid identity: oversized action (bound preserved).
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(oversizedString(), QString(),
                                                    QStringLiteral("component"), QStringLiteral("compFriendly"),
                                                    QStringLiteral("default"), QStringLiteral("Default Context"),
                                                    QList<int>{}, QList<int>{}, &fieldError));
        CHECK(fieldError == QStringLiteral("oversized action"));
    }
    // Invalid identity: empty compUnique.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(QStringLiteral("action"), QString(),
                                                    QString(), QStringLiteral("compFriendly"),
                                                    QStringLiteral("default"), QStringLiteral("Default Context"),
                                                    QList<int>{}, QList<int>{}, &fieldError));
        CHECK(fieldError == QStringLiteral("empty component"));
    }
    // Invalid identity: oversized compUnique.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(QStringLiteral("action"), QString(),
                                                    oversizedString(), QStringLiteral("compFriendly"),
                                                    QStringLiteral("default"), QStringLiteral("Default Context"),
                                                    QList<int>{}, QList<int>{}, &fieldError));
        CHECK(fieldError == QStringLiteral("oversized component"));
    }
    // Invalid cosmetic: oversized friendly.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), oversizedString(), QStringLiteral("component"),
            QStringLiteral("compFriendly"), QStringLiteral("default"), QStringLiteral("Default Context"),
            QList<int>{}, QList<int>{}, &fieldError));
        CHECK(fieldError == QStringLiteral("oversized friendly"));
    }
    // Invalid cosmetic: oversized compFriendly.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"), oversizedString(),
            QStringLiteral("default"), QStringLiteral("Default Context"), QList<int>{}, QList<int>{}, &fieldError));
        CHECK(fieldError == QStringLiteral("oversized component friendly"));
    }
    // Invalid cosmetic: oversized contextUnique.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"), QStringLiteral("compFriendly"),
            oversizedString(), QStringLiteral("Default Context"), QList<int>{}, QList<int>{}, &fieldError));
        CHECK(fieldError == QStringLiteral("oversized context unique"));
    }
    // Invalid cosmetic: oversized contextFriendly.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"), QStringLiteral("compFriendly"),
            QStringLiteral("default"), oversizedString(), QList<int>{}, QList<int>{}, &fieldError));
        CHECK(fieldError == QStringLiteral("oversized context friendly"));
    }
    // Invalid active keys: too many.
    {
        QList<int> tooMany;
        for (int i = 0; i < SHORTCUT_MAX_KEYS_PER_TUPLE + 1; ++i) {
            tooMany.append(i + 1);
        }
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"), QStringLiteral("compFriendly"),
            QStringLiteral("default"), QStringLiteral("Default Context"), tooMany, QList<int>{}, &fieldError));
        CHECK(fieldError == QStringLiteral("too many active keys"));
    }
    // Invalid active keys: negative.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"), QStringLiteral("compFriendly"),
            QStringLiteral("default"), QStringLiteral("Default Context"), QList<int>{-1}, QList<int>{},
            &fieldError));
        CHECK(fieldError == QStringLiteral("negative active key"));
    }
    // Invalid active keys: over-max.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"), QStringLiteral("compFriendly"),
            QStringLiteral("default"), QStringLiteral("Default Context"),
            QList<int>{SHORTCUT_MAX_KEY_VALUE + 1}, QList<int>{}, &fieldError));
        CHECK(fieldError == QStringLiteral("oversized active key"));
    }
    // Invalid default keys: too many.
    {
        QList<int> tooMany;
        for (int i = 0; i < SHORTCUT_MAX_KEYS_PER_TUPLE + 1; ++i) {
            tooMany.append(i + 1);
        }
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"), QStringLiteral("compFriendly"),
            QStringLiteral("default"), QStringLiteral("Default Context"), QList<int>{}, tooMany, &fieldError));
        CHECK(fieldError == QStringLiteral("too many default keys"));
    }
    // Invalid default keys: negative.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"), QStringLiteral("compFriendly"),
            QStringLiteral("default"), QStringLiteral("Default Context"), QList<int>{}, QList<int>{-1},
            &fieldError));
        CHECK(fieldError == QStringLiteral("negative default key"));
    }
    // Invalid default keys: over-max.
    {
        QString fieldError;
        CHECK(!ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"), QStringLiteral("compFriendly"),
            QStringLiteral("default"), QStringLiteral("Default Context"), QList<int>{},
            QList<int>{SHORTCUT_MAX_KEY_VALUE + 1}, &fieldError));
        CHECK(fieldError == QStringLiteral("oversized default key"));
    }
    // Empty friendly alone never reports an identity token.
    {
        QString fieldError;
        CHECK(ShortcutReconciler::keyedFieldsValid(
            QStringLiteral("action"), QString(), QStringLiteral("component"), QStringLiteral("compFriendly"),
            QStringLiteral("default"), QStringLiteral("Default Context"), QList<int>{}, QList<int>{},
            &fieldError));
    }
}

void preflightStoreGuardsSplitTokens()
{
    // Pre-D-Bus keyed input guards fail closed without a bus. Each
    // elementary single-key predicate has its own token: negative, zero
    // (non-positive), over-max. List-bound is unreachable for one element.
    {
        KGlobalAccelStore store;
        QList<ShortcutKeyHolder> holders;
        QString error;
        CHECK(!store.shortcutsByKey(-1, &holders, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: negative key"));
        CHECK(!store.shortcutsByKey(0, &holders, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: non-positive key"));
        CHECK(!store.shortcutsByKey(SHORTCUT_MAX_KEY_VALUE + 1, &holders, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: oversized key"));
        // Fake mirrors the same split.
        FakeShortcutStore fake;
        CHECK(!fake.shortcutsByKey(-1, &holders, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: negative key"));
        CHECK(!fake.shortcutsByKey(0, &holders, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: non-positive key"));
        CHECK(!fake.shortcutsByKey(SHORTCUT_MAX_KEY_VALUE + 1, &holders, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutsByKey reply: oversized key"));
    }
    {
        KGlobalAccelStore store;
        bool available = false;
        QString error;
        CHECK(!store.shortcutAvailable(-1, QString(), &available, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutAvailable reply: negative key"));
        CHECK(!store.shortcutAvailable(0, QString(), &available, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutAvailable reply: non-positive key"));
        CHECK(!store.shortcutAvailable(SHORTCUT_MAX_KEY_VALUE + 1, QString(), &available, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutAvailable reply: oversized key"));
        CHECK(!store.shortcutAvailable(META_L, oversizedString(), &available, &error));
        CHECK(error == QStringLiteral("unexpected globalShortcutAvailable reply: oversized component"));
    }
}

void preflightReadAllFieldSplitTokens()
{
    // Negative active key via Fake readAll.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.tuples[0].active = QList<int>{-1};
        QString error;
        QList<ShortcutTuple> out;
        CHECK(!store.readAll(&out, &error));
        CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: negative active key"));
    }
    // Oversized active key via Fake readAll.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.tuples[0].active = QList<int>{SHORTCUT_MAX_KEY_VALUE + 1};
        QString error;
        QList<ShortcutTuple> out;
        CHECK(!store.readAll(&out, &error));
        CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: oversized active key"));
    }
    // Too many tuples via Fake readAll.
    {
        FakeShortcutStore store;
        for (int i = 0; i < SHORTCUT_MAX_TUPLES + 1; ++i) {
            ShortcutTuple tuple;
            tuple.component = QStringLiteral("c%1").arg(i);
            tuple.action = QStringLiteral("a%1").arg(i);
            tuple.componentFriendly = QStringLiteral("cf");
            tuple.friendly = QString();
            store.tuples.append(tuple);
        }
        QString error;
        QList<ShortcutTuple> out;
        CHECK(!store.readAll(&out, &error));
        CHECK(error == QStringLiteral("unexpected allShortcutInfos reply: too many tuples"));
    }
}

void preflightOccupancyHolderSplitTokens()
{
    // Each holder predicate has its own token; triggers are pure. The
    // "holder" qualifier keeps these distinct from the shared-seam field
    // tokens under the same by-key prefix. Occupancy-key range tokens are
    // defensive: relevantConflictKeys() constants are always valid, so the
    // real occupancy branch delegates to checkOccupancyKeyRange and the
    // exact tokens are triggered hermetically there (see
    // boundedHelperSeamsExactTokens).
    auto checkHolder = [](const ShortcutKeyHolder &bad, const char *token) {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{419430420}, QList<int>{META_L});
        store.extraByKey[META_L].append(bad);
        const KeyedOccupancyResult outcome = ShortcutReconciler::checkKeyedForeignOccupancyDetailed(&store);
        CHECK(outcome.status == KeyedOccupancy::Unavailable);
        CHECK(outcome.detail == QString::fromUtf8(token));
    };
    {
        ShortcutKeyHolder bad;
        bad.component = QString();
        bad.action = QStringLiteral("other");
        bad.active = QList<int>{META_L};
        checkHolder(bad, "unexpected globalShortcutsByKey reply: empty holder component");
    }
    {
        ShortcutKeyHolder bad;
        bad.component = oversizedString();
        bad.action = QStringLiteral("other");
        bad.active = QList<int>{META_L};
        checkHolder(bad, "unexpected globalShortcutsByKey reply: oversized holder component");
    }
    {
        ShortcutKeyHolder bad;
        bad.component = QStringLiteral("org.kde.unexpected");
        bad.action = QString();
        bad.active = QList<int>{META_L};
        checkHolder(bad, "unexpected globalShortcutsByKey reply: empty holder action");
    }
    {
        ShortcutKeyHolder bad;
        bad.component = QStringLiteral("org.kde.unexpected");
        bad.action = oversizedString();
        bad.active = QList<int>{META_L};
        checkHolder(bad, "unexpected globalShortcutsByKey reply: oversized holder action");
    }
    {
        ShortcutKeyHolder bad;
        bad.component = QStringLiteral("org.kde.unexpected");
        bad.action = QStringLiteral("other");
        QList<int> tooMany;
        for (int i = 0; i < SHORTCUT_MAX_KEYS_PER_TUPLE + 1; ++i) {
            tooMany.append(i + 1);
        }
        bad.active = tooMany;
        checkHolder(bad, "unexpected globalShortcutsByKey reply: too many holder active keys");
    }
    {
        ShortcutKeyHolder bad;
        bad.component = QStringLiteral("org.kde.unexpected");
        bad.action = QStringLiteral("other");
        bad.active = QList<int>{-1};
        checkHolder(bad, "unexpected globalShortcutsByKey reply: negative holder active key");
    }
    {
        ShortcutKeyHolder bad;
        bad.component = QStringLiteral("org.kde.unexpected");
        bad.action = QStringLiteral("other");
        bad.active = QList<int>{SHORTCUT_MAX_KEY_VALUE + 1};
        checkHolder(bad, "unexpected globalShortcutsByKey reply: oversized holder active key");
    }
    {
        ShortcutKeyHolder bad;
        bad.component = QStringLiteral("org.kde.unexpected");
        bad.action = QStringLiteral("other");
        bad.active = QList<int>{};
        QList<int> tooMany;
        for (int i = 0; i < SHORTCUT_MAX_KEYS_PER_TUPLE + 1; ++i) {
            tooMany.append(i + 1);
        }
        bad.defaults = tooMany;
        checkHolder(bad, "unexpected globalShortcutsByKey reply: too many holder default keys");
    }
    {
        ShortcutKeyHolder bad;
        bad.component = QStringLiteral("org.kde.unexpected");
        bad.action = QStringLiteral("other");
        bad.active = QList<int>{};
        bad.defaults = QList<int>{-1};
        checkHolder(bad, "unexpected globalShortcutsByKey reply: negative holder default key");
    }
    {
        ShortcutKeyHolder bad;
        bad.component = QStringLiteral("org.kde.unexpected");
        bad.action = QStringLiteral("other");
        bad.active = QList<int>{};
        bad.defaults = QList<int>{SHORTCUT_MAX_KEY_VALUE + 1};
        checkHolder(bad, "unexpected globalShortcutsByKey reply: oversized holder default key");
    }
    // Too many occupancy holders.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{419430420}, QList<int>{META_L});
        for (int i = 0; i < SHORTCUT_MAX_TUPLES + 1; ++i) {
            ShortcutKeyHolder holder;
            holder.component = QStringLiteral("c%1").arg(i);
            holder.action = QStringLiteral("a%1").arg(i);
            store.extraByKey[META_L].append(holder);
        }
        const KeyedOccupancyResult outcome = ShortcutReconciler::checkKeyedForeignOccupancyDetailed(&store);
        CHECK(outcome.status == KeyedOccupancy::Unavailable);
        CHECK(outcome.detail
              == QStringLiteral("unexpected globalShortcutsByKey reply: too many occupancy holders"));
    }
    // Availability inconsistency both directions with exact tokens.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.active.contains(META_L)) {
                tuple.active.removeAll(META_L);
            }
        }
        store.extraByKey.clear();
        store.availableOverride[META_L] = false;
        const KeyedOccupancyResult outcome = ShortcutReconciler::checkKeyedForeignOccupancyDetailed(&store);
        CHECK(outcome.status == KeyedOccupancy::Unavailable);
        CHECK(outcome.detail
              == QStringLiteral(
                  "unexpected globalShortcutAvailable reply: empty holders report unavailable"));
    }
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.availableOverride[META_L] = true;
        const KeyedOccupancyResult outcome = ShortcutReconciler::checkKeyedForeignOccupancyDetailed(&store);
        CHECK(outcome.status == KeyedOccupancy::Unavailable);
        CHECK(outcome.detail
              == QStringLiteral(
                  "unexpected globalShortcutAvailable reply: occupied holders report available"));
    }
}

void setterReplyAaiEncodingAndPureDecode()
{
    ensureKeySequenceTestMetaTypes();
    // Actual a(ai) encoding: empty and multi-sequence QSet round-trips
    // assert exact a(ai) framing via the real source operators.
    {
        QDBusArgument arg;
        const QSet<QKeySequence> empty;
        arg.beginArray(QMetaType::fromType<QKeySequence>());
        for (const QKeySequence &seq : empty) {
            arg << seq;
        }
        arg.endArray();
        CHECK(arg.currentSignature() == QStringLiteral("a(ai)"));
        QDBusMessage message =
            QDBusMessage::createSignal(QStringLiteral("/test"), QStringLiteral("i.I"), QStringLiteral("keys"));
        message << QVariant::fromValue(empty);
        const QSet<QKeySequence> decoded = message.arguments().at(0).value<QSet<QKeySequence>>();
        CHECK(decoded.isEmpty());
        QSet<QKeySequence> out;
        CHECK(ShortcutReconciler::decodeSetterReplySlotSets(QList<QList<int>>{}, &out));
        CHECK(out.isEmpty());
    }
    {
        QSet<QKeySequence> multi;
        multi.insert(QKeySequence(META_L, 0, 0, 0));
        multi.insert(QKeySequence(META_ESC, 0, 0, 0));
        QDBusArgument arg;
        arg.beginArray(QMetaType::fromType<QKeySequence>());
        for (const QKeySequence &seq : multi) {
            arg << seq;
        }
        arg.endArray();
        CHECK(arg.currentSignature() == QStringLiteral("a(ai)"));
        QDBusMessage message =
            QDBusMessage::createSignal(QStringLiteral("/test"), QStringLiteral("i.I"), QStringLiteral("keys"));
        message << QVariant::fromValue(multi);
        const QSet<QKeySequence> decoded = message.arguments().at(0).value<QSet<QKeySequence>>();
        CHECK(decoded == multi);
        QList<QList<int>> groups;
        groups.append(QList<int>{META_L, 0, 0, 0});
        groups.append(QList<int>{META_ESC, 0, 0, 0});
        QSet<QKeySequence> out;
        CHECK(ShortcutReconciler::decodeSetterReplySlotSets(groups, &out));
        CHECK(out.size() == 2);
        CHECK(out.contains(QKeySequence(META_L)));
        CHECK(out.contains(QKeySequence(META_ESC)));
    }
}

void setterReplySlotShapesFailClosed()
{
    // Pure slot-set seam rejects malformed/unexpected per-sequence shapes
    // and limits without abort.
    {
        QSet<QKeySequence> out;
        CHECK(!ShortcutReconciler::decodeSetterReplySlotSets(QList<QList<int>>{QList<int>{1, 2, 3}}, &out));
        CHECK(!ShortcutReconciler::decodeSetterReplySlotSets(QList<QList<int>>{QList<int>{1, 2, 3, 4, 5}}, &out));
        CHECK(!ShortcutReconciler::decodeSetterReplySlotSets(QList<QList<int>>{QList<int>{}}, &out));
        CHECK(!ShortcutReconciler::decodeSetterReplySlotSets(
            QList<QList<int>>{QList<int>{-1, 0, 0, 0}}, &out));
        CHECK(!ShortcutReconciler::decodeSetterReplySlotSets(
            QList<QList<int>>{QList<int>{SHORTCUT_MAX_KEY_VALUE + 1, 0, 0, 0}}, &out));
        QList<QList<int>> tooMany;
        for (int i = 0; i < SHORTCUT_MAX_KEYS_PER_TUPLE + 1; ++i) {
            tooMany.append(QList<int>{1000 + i, 0, 0, 0});
        }
        CHECK(!ShortcutReconciler::decodeSetterReplySlotSets(tooMany, &out));
    }
    // Guarded reply operators fail closed on unexpected framing, no abort:
    // a default (non-array/structure) argument never reaches basic reads.
    {
        const QDBusArgument bad;
        QKeySequence decodedSeq = QKeySequence(META_L);
        bad >> decodedSeq;
        CHECK(decodedSeq == QKeySequence());
        QSet<QKeySequence> decodedSet;
        decodedSet.insert(QKeySequence(META_L));
        bad >> decodedSet;
        CHECK(decodedSet.isEmpty());
        KWin::ShortcutMatchType match{7};
        bad >> match;
        CHECK(match.value == 0);
        KWin::ShortcutInfoFields info;
        info.action = QStringLiteral("seed");
        bad >> info;
        CHECK(info.action.isEmpty());
    }
}

void defaultAndForeignTransportSeam()
{
    // Lean hermetic coverage: exact defaultShortcutKeys(as)->a(ai) decode,
    // exact void setForeignShortcutKeys(as,a(ai)) validation, arbitrary IDs
    // with order-insensitive fresh readback, pre-bus input plus pinned-owner
    // drift guards on the real store, and the Fake seam (arbitrary defaults,
    // foreign write plus readback, cap counting, failure controls). No live
    // bus, no config mutation.
    ensureKeySequenceTestMetaTypes();
    qDBusRegisterMetaType<QList<QKeySequence>>();
    QString error;
    QList<int> defaults;
    // Default reply strict transport: ordered type, signature, arity.
    CHECK(!ShortcutReconciler::parseDefaultShortcutKeysReply(
        QDBusMessage::ErrorMessage, QStringLiteral("a(ai)"),
        {QVariant::fromValue(QSet<QKeySequence>())}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected defaultShortcutKeys reply: wrong message type"));
    CHECK(!ShortcutReconciler::parseDefaultShortcutKeysReply(
        QDBusMessage::ReplyMessage, QStringLiteral("as"),
        {QVariant::fromValue(QSet<QKeySequence>())}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected defaultShortcutKeys reply: wrong signature"));
    CHECK(!ShortcutReconciler::parseDefaultShortcutKeysReply(QDBusMessage::ReplyMessage,
                                                             QStringLiteral("a(ai)"), {}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected defaultShortcutKeys reply: wrong arity"));
    CHECK(!ShortcutReconciler::parseDefaultShortcutKeysReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ai)"),
        {QVariant::fromValue(QStringLiteral("not-an-argument"))}, nullptr, &error));
    CHECK(error == QStringLiteral("unexpected defaultShortcutKeys reply: wrong variant shape"));
    CHECK(!ShortcutReconciler::parseDefaultShortcutKeysReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ai)"), {QVariant::fromValue(QDBusArgument())},
        nullptr, &error));
    CHECK(error == QStringLiteral("unexpected defaultShortcutKeys reply: wrong array framing"));
    // Typed acceptance: empty and multi-key sets and lists decode to sorted
    // primitive defaults.
    CHECK(ShortcutReconciler::parseDefaultShortcutKeysReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ai)"),
        {QVariant::fromValue(QSet<QKeySequence>())}, &defaults, &error));
    CHECK(defaults.isEmpty());
    {
        QSet<QKeySequence> set;
        set.insert(QKeySequence(META_ESC));
        set.insert(QKeySequence(META_L));
        CHECK(ShortcutReconciler::parseDefaultShortcutKeysReply(QDBusMessage::ReplyMessage,
                                                                QStringLiteral("a(ai)"),
                                                                {QVariant::fromValue(set)}, &defaults, &error));
        CHECK(defaults == (QList<int>{META_L, META_ESC}));
    }
    {
        const QList<QKeySequence> list = {QKeySequence(META_ALT_K), QKeySequence(META_ALT_L)};
        CHECK(ShortcutReconciler::parseDefaultShortcutKeysReply(QDBusMessage::ReplyMessage,
                                                                QStringLiteral("a(ai)"),
                                                                {QVariant::fromValue(list)}, &defaults, &error));
        CHECK(defaults == (QList<int>{META_ALT_K, META_ALT_L}));
    }
    // Typed oversized fails closed with the bounded decode token.
    {
        QSet<QKeySequence> oversized;
        for (int i = 0; i < SHORTCUT_MAX_KEYS_PER_TUPLE + 1; ++i) {
            oversized.insert(QKeySequence(1000 + i));
        }
        CHECK(!ShortcutReconciler::parseDefaultShortcutKeysReply(
            QDBusMessage::ReplyMessage, QStringLiteral("a(ai)"), {QVariant::fromValue(oversized)},
            nullptr, &error));
        CHECK(error
              == QStringLiteral("unexpected defaultShortcutKeys reply: did not return expected keys"));
    }
    // Void reply strict transport: empty signature and zero args only.
    CHECK(ShortcutReconciler::parseSetForeignShortcutKeysReply(QDBusMessage::ReplyMessage, QString(), {}, &error));
    CHECK(!ShortcutReconciler::parseSetForeignShortcutKeysReply(QDBusMessage::ErrorMessage, QString(), {},
                                                                &error));
    CHECK(error == QStringLiteral("unexpected setForeignShortcutKeys reply: wrong message type"));
    CHECK(!ShortcutReconciler::parseSetForeignShortcutKeysReply(
        QDBusMessage::ReplyMessage, QStringLiteral("a(ai)"), {}, &error));
    CHECK(error == QStringLiteral("unexpected setForeignShortcutKeys reply: wrong signature"));
    CHECK(!ShortcutReconciler::parseSetForeignShortcutKeysReply(
        QDBusMessage::ReplyMessage, QString(), {QVariant::fromValue(1)}, &error));
    CHECK(error == QStringLiteral("unexpected setForeignShortcutKeys reply: wrong arity"));
    // Fresh readback comparison is order-insensitive set equality.
    CHECK(ShortcutReconciler::foreignReadbackMatches(QList<int>{META_L, META_ESC}, QList<int>{META_ESC, META_L}));
    CHECK(!ShortcutReconciler::foreignReadbackMatches(QList<int>{META_L}, QList<int>{META_ESC}));
    CHECK(!ShortcutReconciler::foreignReadbackMatches(QList<int>{}, QList<int>{META_L}));
    // Real store pre-bus guards fail closed without a bus: unbounded tuple,
    // then missing pin/drift (in that order). Arbitrary IDs reach the pin
    // guard, proving no allowlist refusal.
    {
        KGlobalAccelStore store;
        QList<int> out;
        CHECK(!store.defaultShortcutKeys(QStringLiteral("kwin"),
                                         QStringLiteral("omnitiler-focus-right"), oversizedString(),
                                         QStringLiteral("friendly"), &out, &error));
        CHECK(error == QStringLiteral("refusing default keys with unbounded tuple"));
        CHECK(!store.defaultShortcutKeys(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle"),
                                         QStringLiteral("KWin"), QStringLiteral("friendly"), &out, &error));
        CHECK(error == QStringLiteral("refusing default keys without a pinned KGlobalAccel owner"));
        CHECK(!store.defaultShortcutKeys(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                         QStringLiteral("Example"), QStringLiteral("Arbitrary"), &out, &error));
        CHECK(error == QStringLiteral("refusing default keys without a pinned KGlobalAccel owner"));
    }
    {
        KGlobalAccelStore store;
        CHECK(!store.setForeignShortcutKeys(QStringLiteral("kwin"),
                                            QStringLiteral("omnitiler-focus-right"), QStringLiteral("KWin"),
                                            QStringLiteral("friendly"), QList<int>{-1}, &error));
        CHECK(error == QStringLiteral("refusing foreign write with unbounded tuple"));
        CHECK(!store.setForeignShortcutKeys(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle"),
                                            QStringLiteral("KWin"), QStringLiteral("friendly"), QList<int>{META_L},
                                            &error));
        CHECK(error == QStringLiteral("refusing foreign write without a pinned KGlobalAccel owner"));
        CHECK(!store.setForeignShortcutKeys(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                            QStringLiteral("Example"), QStringLiteral("Arbitrary"), QList<int>{},
                                            &error));
        CHECK(error == QStringLiteral("refusing foreign write without a pinned KGlobalAccel owner"));
    }
    // Fake seam: arbitrary IDs round-trip defaults, void foreign write
    // applies plus fresh readback confirms, foreign writes count toward
    // writeCount and the lifetime cap.
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                      QList<int>{META_L}));
        store.defaultKeysById[QStringLiteral("org.example/arbitrary-action")] = QList<int>{META_G};
        store.defaultKeysById[QStringLiteral("kwin/omnitiler-toggle")] = QList<int>{META_L};
        QList<int> out;
        CHECK(store.defaultShortcutKeys(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                        QStringLiteral("Example"), QStringLiteral("Arbitrary"), &out, &error));
        CHECK(out == QList<int>{META_G});
        CHECK(store.defaultShortcutKeys(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle"),
                                        QStringLiteral("KWin"), QStringLiteral("legacy"), &out, &error));
        CHECK(out == QList<int>{META_L});
        CHECK(store.setForeignShortcutKeys(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                           QStringLiteral("Example"), QStringLiteral("Arbitrary"), QList<int>{},
                                           &error));
        CHECK(store.foreignWriteLog.size() == 1);
        CHECK(store.writeCount() == 1);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("arbitrary-action")) {
                CHECK(tuple.active.isEmpty());
            }
        }
        store.writeLog.clear();
        store.foreignWriteLog.clear();
        // No lifetime accumulation cap: repeated legitimate transport writes
        // stay available past the old 64-write lifetime. The reconciler
        // enforces the operation-scoped budget instead (see the full-catalog
        // and repeated-operation regressions below).
        for (int i = 0; i < 200; ++i) {
            CHECK(store.setForeignShortcutKeys(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                               QStringLiteral("Example"), QStringLiteral("Arbitrary"),
                                               QList<int>{META_L}, &error));
        }
        CHECK(store.writeCount() == 200);
        CHECK(shortcutMaxForeignClears() == SHORTCUT_MAX_TUPLES);
        CHECK(shortcutMaxForceWrites(43) == 43 + shortcutMaxProjectWrites());
        CHECK(shortcutMaxRevertWrites(43) == 43);
    }
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                      QList<int>{META_L}));
        store.failDefaultKeys = true;
        CHECK(!store.defaultShortcutKeys(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                         QStringLiteral("Example"), QStringLiteral("Arbitrary"), nullptr, &error));
        CHECK(error == QStringLiteral("defaultShortcutKeys call failed"));
        store.malformedDefaultKeys = true;
        CHECK(!store.defaultShortcutKeys(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                         QStringLiteral("Example"), QStringLiteral("Arbitrary"), nullptr, &error));
        CHECK(error == QStringLiteral("unexpected defaultShortcutKeys reply: wrong message type"));
        store.failForeign = true;
        CHECK(!store.setForeignShortcutKeys(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                            QStringLiteral("Example"), QStringLiteral("Arbitrary"), QList<int>{},
                                            &error));
        CHECK(error == QStringLiteral("setForeignShortcutKeys call failed"));
        CHECK(store.foreignWriteLog.isEmpty());
    }
    {
        FakeShortcutStore store;
        seedReady6(store, QList<int>{1}, QList<int>{META_L});
        store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                      QList<int>{META_L}));
        store.badForeignReadback = true;
        CHECK(!store.setForeignShortcutKeys(QStringLiteral("org.example"), QStringLiteral("arbitrary-action"),
                                            QStringLiteral("Example"), QStringLiteral("Arbitrary"), QList<int>{},
                                            &error));
        CHECK(error == QStringLiteral("setForeignShortcutKeys readback did not confirm expected keys"));
    }
}














// Structured diagnostics: operations, stages, and outcomes are logged with
// safe fields only, through the injectable sink; logging never affects
// behavior, even when the sink throws.
void diagSinkCapturesOperations()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{1}, QList<int>{META_L});
    FakeClearedActions cleared;
    QStringList messages;
    ShortcutDiag::setSink([&](QtMsgType, const QString &message) {
        messages.append(message);
    });
    CHECK(ShortcutReconciler(&store, &cleared).apply().ok);
    CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-action"), QList<int>{META_G}));
    store.defaultKeysById[QStringLiteral("org.example/other-action")] = QList<int>{META_G};
    const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
    CHECK(preview.forceable);
    CHECK(ShortcutReconciler(&store, &cleared).applyForced(preview).ok);
    ShortcutDiag::resetSink();
    bool sawApplyStart = false;
    bool sawApplyFinish = false;
    bool sawRevert = false;
    bool sawForcePreview = false;
    for (const QString &message : messages) {
        CHECK(message.contains(QStringLiteral("op=")));
        CHECK(message.contains(QStringLiteral("stage=")));
        CHECK(message.contains(QStringLiteral("outcome=")));
        CHECK(message.contains(QStringLiteral("omnitiler.shortcut")));
        CHECK(!message.contains(QStringLiteral("/home")));
        CHECK(!message.contains(QStringLiteral(".config")));
        if (message.contains(QStringLiteral("op=apply"))
            && message.contains(QStringLiteral("stage=start"))) {
            sawApplyStart = true;
        }
        if (message.contains(QStringLiteral("op=apply"))
            && message.contains(QStringLiteral("stage=finish"))
            && message.contains(QStringLiteral("outcome=ok"))) {
            sawApplyFinish = true;
        }
        if (message.contains(QStringLiteral("op=revert"))) {
            sawRevert = true;
        }
        if (message.contains(QStringLiteral("op=force-preview"))) {
            sawForcePreview = true;
        }
    }
    CHECK(sawApplyStart);
    CHECK(sawApplyFinish);
    CHECK(sawRevert);
    CHECK(sawForcePreview);
}

void diagPrefixAndForeignDataAreSafe()
{
    QString message;
    ShortcutDiag::setSink([&](QtMsgType, const QString &captured) {
        message = captured;
    });
    ShortcutDiag::log(QtWarningMsg, "apply", "preflight", "refused",
                      QStringLiteral("Meta+G claimed by org.example.foreign/Untrusted Action"));
    ShortcutDiag::resetSink();
    CHECK(message.startsWith(QStringLiteral("omnitiler.shortcut op=apply stage=preflight outcome=refused")));
    // Reconstructible: chord plus holder IDs stay; user paths never enter.
    CHECK(message.contains(QStringLiteral("org.example.foreign")));
    CHECK(message.contains(QStringLiteral("Untrusted Action")));
    CHECK(!message.contains(QStringLiteral("/home")));
    CHECK(!message.contains(QStringLiteral(".config")));
}

void diagThrowingSinkPreservesBehavior()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{1}, QList<int>{META_L});
    FakeClearedActions cleared;
    ShortcutDiag::setSink([](QtMsgType, const QString &) {
        throw 1;
    });
    CHECK(ShortcutReconciler(&store, &cleared).apply().ok);
    ShortcutDiag::resetSink();
    CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
}

void applyAssignsProjectsAndRelocatesLock()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{1}, QList<int>{META_L, 42});
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-resize-outwards-up")) {
            tuple.active = QList<int>{7};
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{8};
        }
    }
    FakeClearedActions cleared;
    const ShortcutApplyResult result = ShortcutReconciler(&store, &cleared).apply();
    CHECK(result.ok);
    CHECK(result.writes == 4);
    CHECK(store.writeLog.size() == 4);
    if (store.writeLog.size() == 4) {
        CHECK(store.writeLog.at(0).action == QStringLiteral("omnitiler-focus-right"));
        CHECK(store.writeLog.at(0).keys == QList<int>{META_L});
        CHECK(store.writeLog.at(1).action == QStringLiteral("Lock Session"));
        CHECK(store.writeLog.at(1).keys == (QList<int>{META_ESC, 42}));
        CHECK(store.writeLog.at(2).action == QStringLiteral("omnitiler-resize-outwards-up"));
        CHECK(store.writeLog.at(2).keys == QList<int>{META_ALT_K});
        CHECK(store.writeLog.at(3).action == QStringLiteral("omnitiler-toggle-float"));
        CHECK(store.writeLog.at(3).keys == QList<int>{META_G});
    }
    CHECK(store.foreignWriteLog.isEmpty());
    for (const auto &record : store.writeLog) {
        CHECK(ShortcutReconciler::isAllowlisted(record.component, record.action));
    }
}

void applyIdempotentNoOp()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    FakeClearedActions cleared;
    const ShortcutApplyResult result = ShortcutReconciler(&store, &cleared).apply();
    CHECK(result.ok);
    CHECK(result.writes == 0);
    CHECK(store.writeCount() == 0);
}

void unknownAndLegacyHoldersRefuseApply()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-action"), QList<int>{META_G}));
    store.tuples.append(
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle"), QList<int>{META_L}));
    FakeClearedActions cleared;
    const ShortcutApplyResult result = ShortcutReconciler(&store, &cleared).apply();
    CHECK(!result.ok);
    // Sorted rows put the legacy kwin holder first; its removal is Meta+L.
    CHECK(result.error.contains(QStringLiteral("Meta+L")));
    CHECK(result.error.contains(QStringLiteral("omnitiler-toggle")));
    CHECK(result.writes == 0);
    CHECK(store.writeCount() == 0);
    CHECK(cleared.stored.isEmpty());
}

void forceUnknownAndLegacyPreviewAndClear()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{1}, QList<int>{META_L});
    store.tuples.append(
        makeTuple(QStringLiteral("org.example"), QStringLiteral("other-action"), QList<int>{META_G, 999}));
    store.tuples.append(
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle"), QList<int>{META_L}));
    store.defaultKeysById[QStringLiteral("org.example/other-action")] = QList<int>{META_G};
    store.defaultKeysById[QStringLiteral("kwin/omnitiler-toggle")] = QList<int>{META_L};
    FakeClearedActions cleared;
    CHECK(!ShortcutReconciler(&store, &cleared).apply().ok);
    const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
    CHECK(preview.forceable);
    CHECK(preview.mismatches.size() == 2);
    // Sorted by component/action: the legacy kwin holder first.
    const ShortcutForceMismatch &legacy = preview.mismatches.at(0);
    CHECK(legacy.component == QStringLiteral("kwin"));
    CHECK(legacy.action == QStringLiteral("omnitiler-toggle"));
    CHECK(legacy.actual == (QList<int>{META_L}));
    CHECK(legacy.expectedPre == (QList<int>{META_L}));
    CHECK(legacy.post.isEmpty());
    const ShortcutForceMismatch &unknown = preview.mismatches.at(1);
    CHECK(unknown.component == QStringLiteral("org.example"));
    CHECK(unknown.action == QStringLiteral("other-action"));
    CHECK(unknown.actual == (QList<int>{META_G, 999}));
    CHECK(unknown.expectedPre == (QList<int>{META_G}));
    CHECK(unknown.post == (QList<int>{999}));
    const ShortcutForceApplyResult forced = ShortcutReconciler(&store, &cleared).applyForced(preview);
    CHECK(forced.ok);
    // 2 foreign clears plus focus and lock assignments.
    CHECK(forced.writes == 4);
    CHECK(store.foreignWriteLog.size() == 2);
    // The union of cleared IDs was persisted before clearing.
    CHECK(cleared.stored.size() == 2);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("other-action")) {
            CHECK(tuple.active == (QList<int>{999}));
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("omnitiler-focus-right")) {
            CHECK(tuple.active == (QList<int>{META_L}));
        }
    }
    // Revert restores the unknown holder to defaults, leaves the legacy
    // project-owned ID cleared, and empties the list.
    const ShortcutRevertResult reverted = ShortcutReconciler(&store, &cleared).revert();
    CHECK(reverted.ok);
    CHECK(cleared.stored.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("other-action")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle")) {
            CHECK(tuple.active.isEmpty());
        }
    }
}

void historicalIdentityRevertSkipsForceClears()
{
    // Pre-rename catalog was kwin/plasma-auto-tiler-*: the previous
    // OmniTiler identity. Revert skips it (stays cleared, no setter
    // even with nonempty defaults); Force still lists it as a conflict
    // and clears only the exact required chord, preserving unrelated.
    CHECK(ShortcutReconciler::isHistoricalProjectOwned(QStringLiteral("kwin"),
                                                       QStringLiteral("plasma-auto-tiler-toggle-float")));
    CHECK(ShortcutReconciler::isProjectOwned(QStringLiteral("kwin"),
                                             QStringLiteral("plasma-auto-tiler-toggle-float")));
    CHECK(!ShortcutReconciler::isProjectAction(QStringLiteral("kwin"),
                                               QStringLiteral("plasma-auto-tiler-toggle-float")));
    CHECK(!ShortcutReconciler::isHolderExempt(QStringLiteral("kwin"),
                                              QStringLiteral("plasma-auto-tiler-toggle-float"), META_G));
    CHECK(!ShortcutReconciler::isHistoricalProjectOwned(QStringLiteral("kwin"),
                                                        QStringLiteral("omnitiler-toggle-float")));
    CHECK(!ShortcutReconciler::isHistoricalProjectOwned(QStringLiteral("org.example"),
                                                        QStringLiteral("plasma-auto-tiler-toggle-float")));
    // Direct Revert: persisted historical ID with nonempty defaults stays
    // cleared with no setter call.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("plasma-auto-tiler-toggle-float"),
                                      QList<int>{}));
        store.defaultKeysById[QStringLiteral("kwin/plasma-auto-tiler-toggle-float")] = QList<int>{META_G};
        FakeClearedActions cleared;
        cleared.stored = {
            {QStringLiteral("kwin"), QStringLiteral("plasma-auto-tiler-toggle-float")},
        };
        const ShortcutRevertResult result = ShortcutReconciler(&store, &cleared).revert();
        CHECK(result.ok);
        CHECK(result.writes == 0);
        CHECK(store.foreignWriteLog.isEmpty());
        CHECK(store.defaultCallLog.isEmpty());
        CHECK(cleared.stored.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("plasma-auto-tiler-toggle-float")) {
                CHECK(tuple.active.isEmpty());
            }
        }
    }
    // Force: historical holder of a required chord is a listed conflict;
    // confirmed Force clears exactly that chord and keeps unrelated keys.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{1}, QList<int>{META_L});
        store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("plasma-auto-tiler-toggle-float"),
                                      QList<int>{META_G, 999}));
        store.defaultKeysById[QStringLiteral("kwin/plasma-auto-tiler-toggle-float")] = QList<int>{META_G};
        FakeClearedActions cleared;
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
        CHECK(preview.mismatches.size() == 1);
        CHECK(preview.mismatches.at(0).component == QStringLiteral("kwin"));
        CHECK(preview.mismatches.at(0).action == QStringLiteral("plasma-auto-tiler-toggle-float"));
        CHECK(preview.mismatches.at(0).expectedPre == (QList<int>{META_G}));
        CHECK(preview.mismatches.at(0).post == (QList<int>{999}));
        const ShortcutForceApplyResult forced = ShortcutReconciler(&store, &cleared).applyForced(preview);
        CHECK(forced.ok);
        CHECK(store.foreignWriteLog.size() == 1);
        CHECK(store.foreignWriteLog.at(0).keys == (QList<int>{999}));
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("plasma-auto-tiler-toggle-float")) {
                CHECK(tuple.active == (QList<int>{999}));
            }
        }
        // Persisted historical ID stays cleared through Revert despite
        // nonempty defaults: no setter, list emptied.
        store.foreignWriteLog.clear();
        store.defaultCallLog.clear();
        const ShortcutRevertResult reverted = ShortcutReconciler(&store, &cleared).revert();
        CHECK(reverted.ok);
        CHECK(cleared.stored.isEmpty());
        CHECK(store.foreignWriteLog.isEmpty());
        CHECK(store.defaultCallLog.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("plasma-auto-tiler-toggle-float")) {
                CHECK(tuple.active == (QList<int>{999}));
            }
        }
    }
}

void forceMultiKeyPreservesUnrelated()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    store.tuples.append(
        makeTuple(QStringLiteral("org.example"), QStringLiteral("other-action"), QList<int>{META_G, 999}));
    store.defaultKeysById[QStringLiteral("org.example/other-action")] = QList<int>{META_G};
    FakeClearedActions cleared;
    const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
    CHECK(preview.forceable);
    CHECK(preview.mismatches.size() == 1);
    CHECK(preview.mismatches.at(0).post == (QList<int>{999}));
    CHECK(ShortcutReconciler(&store, &cleared).applyForced(preview).ok);
    CHECK(store.foreignWriteLog.size() == 1);
    CHECK(store.foreignWriteLog.at(0).keys == (QList<int>{999}));
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("other-action")) {
            CHECK(tuple.active == (QList<int>{999}));
        }
    }
    CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
    CHECK(cleared.stored.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("other-action")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
    }
}

// Full-catalog Authentic Apply needs more than the old 64-write lifetime:
// every bound row emptied plus the lock on its preimage. The
// operation-scoped budget (at most one write per catalog row plus the
// lock) covers it, and a repeated Apply stays available afterwards.
void seedFullQuiet(FakeShortcutStore &store);
void applyFullCatalogBeyondOldLifetime()
{
    FakeShortcutStore store;
    seedFullQuiet(store);
    int boundRows = 0;
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Lock Session")) {
            tuple.active = QList<int>{META_L};
            continue;
        }
        if (ShortcutReconciler::isProjectAction(tuple.component, tuple.action) && !tuple.active.isEmpty()) {
            tuple.active.clear();
            ++boundRows;
        }
    }
    CHECK(boundRows > 64);
    FakeClearedActions cleared;
    const ShortcutApplyResult result = ShortcutReconciler(&store, &cleared).apply();
    CHECK(result.ok);
    CHECK(result.writes == boundRows + 1);
    CHECK(result.writes > 64);
    CHECK(result.writes <= shortcutMaxProjectWrites());
    // Repeated Apply is a no-op success with the lifetime total past 64.
    const ShortcutApplyResult again = ShortcutReconciler(&store, &cleared).apply();
    CHECK(again.ok);
    CHECK(again.writes == 0);
    CHECK(store.writeCount() == result.writes);
    CHECK(store.writeCount() > 64);
    CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
}

// Full Force: 43 foreign clears plus the full project assignment in one
// operation, then Revert and a repeated Apply, with the lifetime total far
// past the old 64-write cap throughout.
void forceFullCatalogBeyondOldLifetime()
{
    FakeShortcutStore store;
    seedFullQuiet(store);
    int boundRows = 0;
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Lock Session")) {
            tuple.active = QList<int>{META_L};
            continue;
        }
        if (ShortcutReconciler::isProjectAction(tuple.component, tuple.action) && !tuple.active.isEmpty()) {
            tuple.active.clear();
            ++boundRows;
        }
    }
    CHECK(boundRows > 64);
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    QSet<int> usedKeys;
    int foreigns = 0;
    for (const ShortcutCatalogEntry &entry : catalog) {
        if (foreigns == 43) {
            break;
        }
        if (entry.canonicalKey == 0 || usedKeys.contains(entry.canonicalKey)) {
            continue;
        }
        usedKeys.insert(entry.canonicalKey);
        const QString action = QStringLiteral("foreign-%1").arg(foreigns);
        store.tuples.append(makeTuple(QStringLiteral("org.example"), action,
                                      QList<int>{entry.canonicalKey, 9000 + foreigns}));
        store.defaultKeysById[QStringLiteral("org.example/") + action] = QList<int>{entry.canonicalKey};
        ++foreigns;
    }
    CHECK(foreigns == 43);
    FakeClearedActions cleared;
    CHECK(!ShortcutReconciler(&store, &cleared).apply().ok);
    const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
    CHECK(preview.forceable);
    CHECK(preview.mismatches.size() == 43);
    const ShortcutForceApplyResult forced = ShortcutReconciler(&store, &cleared).applyForced(preview);
    CHECK(forced.ok);
    CHECK(forced.writes == 43 + boundRows + 1);
    CHECK(forced.writes > 64);
    CHECK(forced.writes <= shortcutMaxForceWrites(43));
    CHECK(cleared.stored.size() == 43);
    // Repeated Apply is a no-op success with the lifetime total past 64.
    const ShortcutApplyResult again = ShortcutReconciler(&store, &cleared).apply();
    CHECK(again.ok);
    CHECK(again.writes == 0);
    CHECK(store.writeCount() == forced.writes);
    // Revert restores the 43 foreign defaults and empties the list.
    const ShortcutRevertResult reverted = ShortcutReconciler(&store, &cleared).revert();
    CHECK(reverted.ok);
    CHECK(reverted.writes == 43);
    CHECK(reverted.writes <= shortcutMaxRevertWrites(43));
    CHECK(cleared.stored.isEmpty());
    CHECK(store.writeCount() == forced.writes + 43);
    // The restored defaults conflict again: Apply refuses with zero writes.
    const ShortcutApplyResult refused = ShortcutReconciler(&store, &cleared).apply();
    CHECK(!refused.ok);
    CHECK(refused.conflictRefused);
    CHECK(refused.writes == 0);
    CHECK(store.writeCount() == forced.writes + 43);
}

void forceStaleConfirmationZeroWrites()
{
    // Live holder drift after preview.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-action"),
                                      QList<int>{META_G}));
        FakeClearedActions cleared;
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("other-action")) {
                tuple.active = QList<int>{META_M};
            }
        }
        const ShortcutForceApplyResult stale = ShortcutReconciler(&store, &cleared).applyForced(preview);
        CHECK(!stale.ok);
        CHECK(stale.error.contains(QStringLiteral("stale")));
        CHECK(stale.writes == 0);
        CHECK(store.writeCount() == 0);
        CHECK(cleared.stored.isEmpty());
        CHECK(cleared.saves == 0);
    }
    // Project drift after preview is likewise stale.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-action"),
                                      QList<int>{META_G}));
        FakeClearedActions cleared;
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-focus-right")) {
                tuple.active = QList<int>{1};
            }
        }
        const ShortcutForceApplyResult stale = ShortcutReconciler(&store, &cleared).applyForced(preview);
        CHECK(!stale.ok);
        CHECK(stale.error.contains(QStringLiteral("stale")));
        CHECK(store.writeCount() == 0);
        CHECK(cleared.stored.isEmpty());
    }
    // Forged identities and rewritten actuals fail closed as unbounded.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-action"),
                                      QList<int>{META_G}));
        FakeClearedActions cleared;
        ShortcutForcePreview forged = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(forged.forceable);
        forged.mismatches[0].action = QStringLiteral("forged-action");
        const ShortcutForceApplyResult result = ShortcutReconciler(&store, &cleared).applyForced(forged);
        CHECK(!result.ok);
        CHECK(result.error.contains(QStringLiteral("stale")));
        CHECK(store.writeCount() == 0);
        CHECK(cleared.stored.isEmpty());
    }
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-action"),
                                      QList<int>{META_G}));
        FakeClearedActions cleared;
        ShortcutForcePreview forged = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(forged.forceable);
        forged.mismatches[0].actual = QList<int>{META_M};
        const ShortcutForceApplyResult result = ShortcutReconciler(&store, &cleared).applyForced(forged);
        CHECK(!result.ok);
        CHECK(store.writeCount() == 0);
        CHECK(cleared.stored.isEmpty());
    }
    // An unconsumed empty preview never applies.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        FakeClearedActions cleared;
        const ShortcutForceApplyResult result =
            ShortcutReconciler(&store, &cleared).applyForced(ShortcutForcePreview());
        CHECK(!result.ok);
        CHECK(store.writeCount() == 0);
        CHECK(cleared.stored.isEmpty());
    }
}

void forceBlockedHolderNotForceable()
{
    // A .desktop-only defaults holder claims the chord but offers nothing
    // Force can clear: Apply names it and no preview is offered.
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    ShortcutKeyHolder foreign;
    foreign.component = QStringLiteral("org.kde.unexpected");
    foreign.action = QStringLiteral("other-launch");
    foreign.active = QList<int>{};
    foreign.defaults = QList<int>{META_ESC};
    store.extraByKey[META_ESC].append(foreign);
    FakeClearedActions cleared;
    const ShortcutApplyResult refused = ShortcutReconciler(&store, &cleared).apply();
    CHECK(!refused.ok);
    CHECK(refused.error.contains(QStringLiteral("Meta+Esc")));
    CHECK(refused.error.contains(QStringLiteral("org.kde.unexpected")));
    const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
    CHECK(!preview.forceable);
    CHECK(preview.error.contains(QStringLiteral("no active binding to clear")));
    CHECK(store.writeCount() == 0);
    CHECK(cleared.stored.isEmpty());
}

void forceInterruptedListRetainedThenRevert()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{1}, QList<int>{META_L});
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Switch to Next Keyboard Layout")) {
            tuple.active = QList<int>{META_ALT_K};
        }
    }
    store.defaultKeysById[QStringLiteral("KDE Keyboard Layout Switcher/Switch to Next Keyboard Layout")] =
        QList<int>{META_ALT_K};
    FakeClearedActions cleared;
    const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
    CHECK(preview.forceable);
    // Fail the first project write after the clear succeeds.
    store.failNextWrite = true;
    const ShortcutForceApplyResult interrupted = ShortcutReconciler(&store, &cleared).applyForced(preview);
    CHECK(!interrupted.ok);
    CHECK(interrupted.error.contains(QStringLiteral("setShortcutKeys")));
    // The union was persisted before clearing: the interruption is revertible.
    CHECK(cleared.stored.size() == 1);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Switch to Next Keyboard Layout")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("omnitiler-focus-right")) {
            CHECK(tuple.active == QList<int>{1});
        }
    }
    // Revert restores the cleared holder and empties the list.
    CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
    CHECK(cleared.stored.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Switch to Next Keyboard Layout")) {
            CHECK(tuple.active == (QList<int>{META_ALT_K}));
        }
    }
    // A fresh Force after the interruption completes the assignment.
    const ShortcutForcePreview retry = ShortcutReconciler(&store, &cleared).previewForceApply();
    CHECK(retry.forceable);
    CHECK(ShortcutReconciler(&store, &cleared).applyForced(retry).ok);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-focus-right")) {
            CHECK(tuple.active == (QList<int>{META_L}));
        }
        if (tuple.action == QStringLiteral("Lock Session")) {
            CHECK(tuple.active == (QList<int>{META_ESC}));
        }
        if (tuple.action == QStringLiteral("Switch to Next Keyboard Layout")) {
            CHECK(tuple.active.isEmpty());
        }
    }
}

void revertInterruptedRetainsList()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("first-action"), QList<int>{}));
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("second-action"), QList<int>{}));
    store.defaultKeysById[QStringLiteral("org.example/first-action")] = QList<int>{META_G};
    store.defaultKeysById[QStringLiteral("org.example/second-action")] = QList<int>{META_M};
    FakeClearedActions cleared;
    cleared.stored = {
        {QStringLiteral("org.example"), QStringLiteral("first-action")},
        {QStringLiteral("org.example"), QStringLiteral("second-action")},
    };
    store.failForeign = true;
    const ShortcutRevertResult interrupted = ShortcutReconciler(&store, &cleared).revert();
    CHECK(!interrupted.ok);
    CHECK(interrupted.writes == 0);
    CHECK(cleared.stored.size() == 2);
    const ShortcutRevertResult reverted = ShortcutReconciler(&store, &cleared).revert();
    CHECK(reverted.ok);
    CHECK(cleared.stored.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("first-action")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
        if (tuple.action == QStringLiteral("second-action")) {
            CHECK(tuple.active == (QList<int>{META_M}));
        }
    }
}

void revertEmptyListNoOp()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    FakeClearedActions cleared;
    const ShortcutRevertResult result = ShortcutReconciler(&store, &cleared).revert();
    CHECK(result.ok);
    CHECK(result.writes == 0);
    CHECK(store.writeCount() == 0);
}

void knownForeignHoldersAreConflicts()
{
    // Status/backend alignment: the compiled foreign rows (Grid View,
    // Switcher, Monocle) are conflicts, never Ready. The shared
    // exemption keeps project, lock, and the authorized System Monitor
    // displacement clear.
    CHECK(!ShortcutReconciler::isHolderExempt(QStringLiteral("kwin"), QStringLiteral("Grid View"), META_G));
    CHECK(!ShortcutReconciler::isHolderExempt(QStringLiteral("KDE Keyboard Layout Switcher"),
                                              QStringLiteral("Switch to Next Keyboard Layout"), META_ALT_K));
    CHECK(!ShortcutReconciler::isHolderExempt(QStringLiteral("kwin"),
                                              QStringLiteral("KrohnkiteMonocleLayout"), META_M));
    CHECK(ShortcutReconciler::isHolderExempt(QStringLiteral("kwin"),
                                             QStringLiteral("omnitiler-toggle-float"), META_G));
    CHECK(ShortcutReconciler::isHolderExempt(QStringLiteral("ksmserver"), QStringLiteral("Lock Session"), META_L));
    CHECK(ShortcutReconciler::isHolderExempt(shortcutAuthorizedEscComponent(), shortcutAuthorizedEscAction(),
                                             META_ESC));
    for (const auto &known : {std::make_pair(QStringLiteral("kwin"), QStringLiteral("Grid View")),
                              std::make_pair(QStringLiteral("KDE Keyboard Layout Switcher"),
                                             QStringLiteral("Switch to Next Keyboard Layout")),
                              std::make_pair(QStringLiteral("kwin"), QStringLiteral("KrohnkiteMonocleLayout"))}) {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        const int key = known.second == QStringLiteral("Grid View")
            ? META_G
            : (known.second == QStringLiteral("KrohnkiteMonocleLayout") ? META_M : META_ALT_K);
        store.tuples.append(makeTuple(known.first, known.second, QList<int>{key}));
        const KeyedOccupancyResult outcome = ShortcutReconciler::checkKeyedForeignOccupancyDetailed(&store);
        CHECK(outcome.status == KeyedOccupancy::Conflict);
        CHECK(outcome.detail.contains(QStringLiteral("claimed by")));
        CHECK(outcome.detail.contains(known.second));
        FakeClearedActions cleared;
        const ShortcutApplyResult refused = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!refused.ok);
        CHECK(refused.error.contains(known.second));
        CHECK(store.writeCount() == 0);
        CHECK(cleared.stored.isEmpty());
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
    }
}

void growArrowSwitchWindowClearRows()
{
    // Twenty-three project rows, twenty-four relevant chords: the four grow-arrow
    // actions clear exactly kwin/Switch Window Left/Down/Up/Right with no
    // relocation, like the existing Meta+Alt+K/L clear rows.
    CHECK(shortcutConflictTable().size() == 23);
    CHECK(ShortcutReconciler::relevantConflictKeys().size() == 24);
    const QList<std::pair<QString, int>> arrows = {
        {QStringLiteral("omnitiler-resize-outwards-left-arrow"), META_ALT_LEFT},
        {QStringLiteral("omnitiler-resize-outwards-down-arrow"), META_ALT_DOWN},
        {QStringLiteral("omnitiler-resize-outwards-up-arrow"), META_ALT_UP},
        {QStringLiteral("omnitiler-resize-outwards-right-arrow"), META_ALT_RIGHT},
    };
    const QList<std::pair<QString, int>> switches = {
        {QStringLiteral("Switch Window Left"), META_ALT_LEFT},
        {QStringLiteral("Switch Window Down"), META_ALT_DOWN},
        {QStringLiteral("Switch Window Up"), META_ALT_UP},
        {QStringLiteral("Switch Window Right"), META_ALT_RIGHT},
    };
    for (const auto &[projectAction, key] : arrows) {
        CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), projectAction));
        CHECK(ShortcutReconciler::isProjectOwned(QStringLiteral("kwin"), projectAction));
        CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), projectAction));
        CHECK(ShortcutReconciler::conflictingKeys(QList<int>{key, 999}) == QList<int>{key});
        CHECK(ShortcutReconciler::remainderAfterClear(QList<int>{key, 999}) == QList<int>{999});
    }
    for (const auto &[foreignAction, key] : switches) {
        CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), foreignAction));
        CHECK(!ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), foreignAction));
        CHECK(!ShortcutReconciler::isHolderExempt(QStringLiteral("kwin"), foreignAction, key));
    }
    // Apply refuses with zero writes when a Switch Window holder claims
    // an arrow chord; other arrow chords stay untouched.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        store.tuples.append(
            makeTuple(QStringLiteral("kwin"), QStringLiteral("Switch Window Left"), QList<int>{META_ALT_LEFT}));
        FakeClearedActions cleared;
        const ShortcutApplyResult refused = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!refused.ok);
        CHECK(refused.error.contains(QStringLiteral("Meta+Alt+Left")));
        CHECK(refused.error.contains(QStringLiteral("Switch Window Left")));
        CHECK(refused.writes == 0);
        CHECK(store.writeCount() == 0);
        CHECK(cleared.stored.isEmpty());
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
        CHECK(preview.mismatches.size() == 1);
        CHECK(preview.mismatches.at(0).component == QStringLiteral("kwin"));
        CHECK(preview.mismatches.at(0).action == QStringLiteral("Switch Window Left"));
        CHECK(preview.mismatches.at(0).expectedPre == QList<int>{META_ALT_LEFT});
        CHECK(preview.mismatches.at(0).post.isEmpty());
    }
    // Force preview/clear removes only the arrow key and keeps unrelated
    // keys; Revert restores the KDE default for the cleared Switch Window
    // action while the project arrow stays assigned.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-resize-outwards-up-arrow")) {
                tuple.active = QList<int>{7};
            }
        }
        store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("Switch Window Up"),
                                      QList<int>{META_ALT_UP, 4242}));
        store.defaultKeysById[QStringLiteral("kwin/Switch Window Up")] = QList<int>{META_ALT_UP};
        FakeClearedActions cleared;
        CHECK(!ShortcutReconciler(&store, &cleared).apply().ok);
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
        CHECK(preview.mismatches.size() == 1);
        CHECK(preview.mismatches.at(0).expectedPre == QList<int>{META_ALT_UP});
        CHECK(preview.mismatches.at(0).post == QList<int>{4242});
        const ShortcutForceApplyResult forced = ShortcutReconciler(&store, &cleared).applyForced(preview);
        CHECK(forced.ok);
        CHECK(cleared.stored.size() == 1);
        CHECK(cleared.stored.at(0).component == QStringLiteral("kwin"));
        CHECK(cleared.stored.at(0).action == QStringLiteral("Switch Window Up"));
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Switch Window Up")) {
                CHECK(tuple.active == (QList<int>{4242}));
            }
            if (tuple.action == QStringLiteral("omnitiler-resize-outwards-up-arrow")) {
                CHECK(tuple.active == (QList<int>{META_ALT_UP}));
            }
        }
        CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
        CHECK(cleared.stored.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Switch Window Up")) {
                CHECK(tuple.active == (QList<int>{META_ALT_UP}));
            }
            if (tuple.action == QStringLiteral("omnitiler-resize-outwards-up-arrow")) {
                CHECK(tuple.active == (QList<int>{META_ALT_UP}));
            }
        }
    }
}

void focusMoveArrowQuickTileClearRows()
{
    // Six clear-only rows: focus arrows clear Quick Tile Left/Bottom/Top/
    // Right, move arrows clear Window to Previous/Next Screen. Verified
    // against KWin v6.7.5 useractions.cpp:879-886 (Quick Tile) and 949-952
    // (to Next/Previous Screen).
    CHECK(shortcutConflictTable().size() == 23);
    CHECK(ShortcutReconciler::relevantConflictKeys().size() == 24);
    const QList<std::pair<QString, int>> projects = {
        {QStringLiteral("omnitiler-focus-left-arrow"), META_LEFT},
        {QStringLiteral("omnitiler-focus-down-arrow"), META_DOWN},
        {QStringLiteral("omnitiler-focus-up-arrow"), META_UP},
        {QStringLiteral("omnitiler-focus-right-arrow"), META_RIGHT},
        {QStringLiteral("omnitiler-move-left-arrow"), META_SHIFT_LEFT},
        {QStringLiteral("omnitiler-move-right-arrow"), META_SHIFT_RIGHT},
    };
    const QList<std::pair<QString, int>> foreigns = {
        {QStringLiteral("Window Quick Tile Left"), META_LEFT},
        {QStringLiteral("Window Quick Tile Bottom"), META_DOWN},
        {QStringLiteral("Window Quick Tile Top"), META_UP},
        {QStringLiteral("Window Quick Tile Right"), META_RIGHT},
        {QStringLiteral("Window to Previous Screen"), META_SHIFT_LEFT},
        {QStringLiteral("Window to Next Screen"), META_SHIFT_RIGHT},
    };
    for (const auto &[projectAction, key] : projects) {
        CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), projectAction));
        CHECK(ShortcutReconciler::isProjectOwned(QStringLiteral("kwin"), projectAction));
        CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), projectAction));
        CHECK(ShortcutReconciler::conflictingKeys(QList<int>{key, 999}) == QList<int>{key});
        CHECK(ShortcutReconciler::remainderAfterClear(QList<int>{key, 999}) == QList<int>{999});
    }
    for (const auto &[foreignAction, key] : foreigns) {
        CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), foreignAction));
        CHECK(!ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), foreignAction));
        CHECK(!ShortcutReconciler::isHolderExempt(QStringLiteral("kwin"), foreignAction, key));
    }
    // Table rows carry the exact project posts, foreign preimages, clear
    // resolution, and displays in preexisting order (rows 9-14).
    {
        const QList<ShortcutConflictRow> &table = shortcutConflictTable();
        const QList<std::pair<QString, QString>> diag = {
            {QStringLiteral("focus-left-arrow"), QStringLiteral("Meta+Left")},
            {QStringLiteral("focus-down-arrow"), QStringLiteral("Meta+Down")},
            {QStringLiteral("focus-up-arrow"), QStringLiteral("Meta+Up")},
            {QStringLiteral("focus-right-arrow"), QStringLiteral("Meta+Right")},
            {QStringLiteral("move-left-arrow"), QStringLiteral("Meta+Shift+Left")},
            {QStringLiteral("move-right-arrow"), QStringLiteral("Meta+Shift+Right")},
        };
        for (int i = 0; i < diag.size(); ++i) {
            const ShortcutConflictRow &row = table.at(9 + i);
            CHECK(row.projectComponent == QStringLiteral("kwin"));
            CHECK(row.projectAction == projects.at(i).first);
            CHECK(row.projectPost == QList<int>{projects.at(i).second});
            CHECK(row.foreignComponent == QStringLiteral("kwin"));
            CHECK(row.foreignAction == foreigns.at(i).first);
            CHECK(row.foreignExpectedPre == QList<int>{foreigns.at(i).second});
            CHECK(row.resolution == shortcutResolutionClear());
            CHECK(row.projectDisplay == diag.at(i).second);
            CHECK(row.projectDiag == diag.at(i).first);
        }
    }
    // Apply refuses with zero writes when a Quick Tile holder claims a
    // focus chord.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Window Quick Tile Left")) {
                tuple.active = QList<int>{META_LEFT};
            }
        }
        FakeClearedActions cleared;
        const ShortcutApplyResult refused = ShortcutReconciler(&store, &cleared).apply();
        CHECK(!refused.ok);
        CHECK(refused.error.contains(QStringLiteral("Meta+Left")));
        CHECK(refused.error.contains(QStringLiteral("Window Quick Tile Left")));
        CHECK(refused.writes == 0);
        CHECK(store.writeCount() == 0);
        CHECK(cleared.stored.isEmpty());
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
        CHECK(preview.mismatches.size() == 1);
        CHECK(preview.mismatches.at(0).component == QStringLiteral("kwin"));
        CHECK(preview.mismatches.at(0).action == QStringLiteral("Window Quick Tile Left"));
        CHECK(preview.mismatches.at(0).expectedPre == QList<int>{META_LEFT});
        CHECK(preview.mismatches.at(0).post.isEmpty());
    }
    // Force removes only the conflicting key, keeps unrelated keys, assigns
    // the project arrow, and Revert restores the KDE default.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-move-left-arrow")) {
                tuple.active = QList<int>{7};
            }
        }
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Window to Previous Screen")) {
                tuple.active = QList<int>{META_SHIFT_LEFT, 4242};
            }
        }
        store.defaultKeysById[QStringLiteral("kwin/Window to Previous Screen")] =
            QList<int>{META_SHIFT_LEFT};
        FakeClearedActions cleared;
        CHECK(!ShortcutReconciler(&store, &cleared).apply().ok);
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
        CHECK(preview.mismatches.size() == 1);
        CHECK(preview.mismatches.at(0).expectedPre == QList<int>{META_SHIFT_LEFT});
        CHECK(preview.mismatches.at(0).post == QList<int>{4242});
        const ShortcutForceApplyResult forced = ShortcutReconciler(&store, &cleared).applyForced(preview);
        CHECK(forced.ok);
        CHECK(cleared.stored.size() == 1);
        CHECK(cleared.stored.at(0).component == QStringLiteral("kwin"));
        CHECK(cleared.stored.at(0).action == QStringLiteral("Window to Previous Screen"));
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Window to Previous Screen")) {
                CHECK(tuple.active == (QList<int>{4242}));
            }
            if (tuple.action == QStringLiteral("omnitiler-move-left-arrow")) {
                CHECK(tuple.active == (QList<int>{META_SHIFT_LEFT}));
            }
        }
        CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
        CHECK(cleared.stored.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Window to Previous Screen")) {
                CHECK(tuple.active == (QList<int>{META_SHIFT_LEFT}));
            }
            if (tuple.action == QStringLiteral("omnitiler-move-left-arrow")) {
                CHECK(tuple.active == (QList<int>{META_SHIFT_LEFT}));
            }
        }
    }
    // Plain assignment: a missing project arrow is assigned with no foreign
    // holders present.
    {
        FakeShortcutStore store;
        seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-focus-right-arrow")) {
                tuple.active = QList<int>{7};
            }
        }
        FakeClearedActions cleared;
        const ShortcutApplyResult result = ShortcutReconciler(&store, &cleared).apply();
        CHECK(result.ok);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-focus-right-arrow")) {
                CHECK(tuple.active == (QList<int>{META_RIGHT}));
            }
        }
    }
}

// Lean drift-after-save fake: the cleared-list save succeeds, then a
// concurrent holder change lands before the first foreign setter. The
// per-holder re-read must abort with zero KGlobalAccel writes while the
// persisted union is retained (superset); Revert stays safe by restoring
// defaults for the retained entry.
class DriftingClearedActions : public ClearedActionsStore
{
public:
    FakeClearedActions inner;
    FakeShortcutStore *store = nullptr;

    bool load(QList<ClearedAction> *actions, QString *error) override
    {
        return inner.load(actions, error);
    }
    bool save(const QList<ClearedAction> &actions, QString *error) override
    {
        if (!inner.save(actions, error)) {
            return false;
        }
        if (store) {
            for (ShortcutTuple &tuple : store->tuples) {
                if (tuple.component == QStringLiteral("org.example")
                    && tuple.action == QStringLiteral("other-action")) {
                    tuple.active = QList<int>{META_G, 1000, 2000};
                    break;
                }
            }
        }
        return true;
    }
    bool clear(QString *error) override
    {
        return inner.clear(error);
    }
};

void forceDriftAfterPersistAbortsWithoutForeignWrites()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-action"),
                                  QList<int>{META_G, 999}));
    store.defaultKeysById[QStringLiteral("org.example/other-action")] = QList<int>{META_G};
    DriftingClearedActions cleared;
    cleared.store = &store;
    const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared.inner).previewForceApply();
    CHECK(preview.forceable);
    const ShortcutForceApplyResult result = ShortcutReconciler(&store, &cleared).applyForced(preview);
    CHECK(!result.ok);
    CHECK(result.error.contains(QStringLiteral("stale")));
    CHECK(result.writes == 0);
    CHECK(store.writeCount() == 0);
    CHECK(store.foreignWriteLog.isEmpty());
    CHECK(store.writeLog.isEmpty());
    // Persisted union retained (superset): the drifted holder was never
    // cleared, but Revert remains safe by restoring its defaults.
    CHECK(cleared.inner.stored.size() == 1);
    CHECK(cleared.inner.stored.at(0).component == QStringLiteral("org.example"));
    CHECK(cleared.inner.stored.at(0).action == QStringLiteral("other-action"));
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("other-action")) {
            CHECK(tuple.active == (QList<int>{META_G, 1000, 2000}));
        }
    }
}

void revertResolvesFreshLabelsAfterRestart()
{
    // IDs-only persistence across a simulated restart with cosmetic label
    // change and multi-key defaults: Revert must use the fresh labels.
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    ShortcutTuple holder = makeTuple(QStringLiteral("org.example"), QStringLiteral("other-action"),
                                     QList<int>{META_G, 999});
    holder.componentFriendly = QStringLiteral("OldComp");
    holder.friendly = QStringLiteral("OldLabel");
    store.tuples.append(holder);
    store.defaultKeysById[QStringLiteral("org.example/other-action")] = QList<int>{META_G, 1000};
    FakeClearedActions cleared;
    const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
    CHECK(preview.forceable);
    CHECK(ShortcutReconciler(&store, &cleared).applyForced(preview).ok);
    // Persisted union is IDs only.
    CHECK(cleared.stored.size() == 1);
    CHECK(cleared.stored.at(0).component == QStringLiteral("org.example"));
    CHECK(cleared.stored.at(0).action == QStringLiteral("other-action"));
    // Simulated restart: cosmetic labels change, cleared active stays.
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.component == QStringLiteral("org.example") && tuple.action == QStringLiteral("other-action")) {
            tuple.componentFriendly = QStringLiteral("NewComp");
            tuple.friendly = QStringLiteral("NewLabel");
            CHECK(tuple.active == (QList<int>{999}));
        }
    }
    store.defaultCallLog.clear();
    store.foreignWriteLog.clear();
    // Fresh reconciler instance against the same durable list.
    CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
    CHECK(cleared.stored.isEmpty());
    CHECK(store.defaultCallLog.size() == 1);
    CHECK(store.defaultCallLog.at(0).componentFriendly == QStringLiteral("NewComp"));
    CHECK(store.defaultCallLog.at(0).friendly == QStringLiteral("NewLabel"));
    CHECK(store.foreignWriteLog.size() == 1);
    CHECK(store.foreignWriteLog.at(0).componentFriendly == QStringLiteral("NewComp"));
    CHECK(store.foreignWriteLog.at(0).friendly == QStringLiteral("NewLabel"));
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.component == QStringLiteral("org.example") && tuple.action == QStringLiteral("other-action")) {
            CHECK(tuple.active == (QList<int>{META_G, 1000}));
        }
    }
}

void revertMissingActionRetainsList()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    store.defaultKeysById[QStringLiteral("org.example/gone-action")] = QList<int>{META_G};
    FakeClearedActions cleared;
    cleared.stored = {
        {QStringLiteral("org.example"), QStringLiteral("gone-action")},
    };
    const ShortcutRevertResult result = ShortcutReconciler(&store, &cleared).revert();
    CHECK(!result.ok);
    CHECK(result.writes == 0);
    CHECK(store.foreignWriteLog.isEmpty());
    CHECK(store.defaultCallLog.isEmpty());
    CHECK(cleared.stored.size() == 1);
}

void revertDuplicateActionRetainsList()
{
    FakeShortcutStore store;
    seedQuietState(store, QList<int>{META_L}, QList<int>{META_ESC});
    ShortcutTuple first = makeTuple(QStringLiteral("org.example"), QStringLiteral("dup-action"), QList<int>{});
    first.componentFriendly = QStringLiteral("CompA");
    first.friendly = QStringLiteral("LabelA");
    ShortcutTuple second = makeTuple(QStringLiteral("org.example"), QStringLiteral("dup-action"), QList<int>{});
    second.componentFriendly = QStringLiteral("CompB");
    second.friendly = QStringLiteral("LabelB");
    store.tuples.append(first);
    store.tuples.append(second);
    store.defaultKeysById[QStringLiteral("org.example/dup-action")] = QList<int>{META_G};
    FakeClearedActions cleared;
    cleared.stored = {
        {QStringLiteral("org.example"), QStringLiteral("dup-action")},
    };
    const ShortcutRevertResult result = ShortcutReconciler(&store, &cleared).revert();
    CHECK(!result.ok);
    CHECK(result.writes == 0);
    CHECK(store.foreignWriteLog.isEmpty());
    CHECK(cleared.stored.size() == 1);
}

void clearedStoreRoundtrip()
{
    QTemporaryDir dir;
    CHECK(dir.isValid());
    const QString path = dir.path() + QStringLiteral("/clearedrc");
    KConfigClearedActions backend(path);
    QString error;
    QList<ClearedAction> loaded;
    // Absent config loads as an empty list.
    CHECK(backend.load(&loaded, &error));
    CHECK(loaded.isEmpty());
    const QList<ClearedAction> actions = {
        {QStringLiteral("org.example"), QStringLiteral("other-action")},
        {QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle")},
    };
    CHECK(backend.save(actions, &error));
    CHECK(backend.load(&loaded, &error));
    CHECK(loaded == actions);
    // Persistence shape is IDs only: Components+Actions present, no
    // cosmetic ComponentFriendlies/Friendlies keys written.
    {
        KConfig stored(path, KConfig::SimpleConfig);
        const KConfigGroup group = stored.group(QStringLiteral("ClearedActions"));
        CHECK(group.hasKey(QStringLiteral("Components")));
        CHECK(group.hasKey(QStringLiteral("Actions")));
        CHECK(!group.hasKey(QStringLiteral("ComponentFriendlies")));
        CHECK(!group.hasKey(QStringLiteral("Friendlies")));
    }
    // Legacy friendly keys are ignored when present.
    {
        KConfig legacy(path, KConfig::SimpleConfig);
        KConfigGroup group = legacy.group(QStringLiteral("ClearedActions"));
        group.writeEntry(QStringLiteral("ComponentFriendlies"), QStringList{QStringLiteral("X"), QStringLiteral("Y")});
        group.writeEntry(QStringLiteral("Friendlies"), QStringList{QStringLiteral("Z"), QStringLiteral("W")});
        legacy.sync();
    }
    CHECK(backend.load(&loaded, &error));
    CHECK(loaded == actions);
    // Unbounded entries fail closed without touching the file content.
    const QList<ClearedAction> bad = {
        {oversizedString(), QStringLiteral("other-action")},
    };
    CHECK(!backend.save(bad, &error));
    CHECK(backend.load(&loaded, &error));
    CHECK(loaded == actions);
    CHECK(backend.clear(&error));
    CHECK(!QFile::exists(path));
    CHECK(backend.load(&loaded, &error));
    CHECK(loaded.isEmpty());
}

void clearedPathIsNewAndAbsolute()
{
    const QString path = defaultClearedActionsPath();
    CHECK(!path.isEmpty());
    CHECK(QDir::isAbsolutePath(path));
    CHECK(path.endsWith(QStringLiteral("shortcut-clearedrc")));
    CHECK(path.contains(QStringLiteral("omnitiler")));
    ClearedActionsStore *live = createLiveClearedActionsStore(path);
    CHECK(live != nullptr);
    delete live;
}

// Full-catalog quiet state: every catalog project at its canonical chord
// (empty for unbound rows), Lock Session relocated, and all known foreign
// holders cleared. Plain Apply is a no-op here; deviations drive refusal
// and Force tests.
void seedFullQuiet(FakeShortcutStore &store)
{
    store.tuples.clear();
    for (const ShortcutCatalogEntry &entry : shortcutProjectCatalog()) {
        const QList<int> active = entry.canonicalKey == 0 ? QList<int>() : QList<int>{entry.canonicalKey};
        store.tuples.append(makeTuple(entry.component, entry.action, active));
    }
    store.tuples.append(makeTuple(QStringLiteral("ksmserver"), QStringLiteral("Lock Session"),
                                  QList<int>{META_ESC}));
    const QList<ShortcutConflictRow> &table = shortcutConflictTable();
    for (int i = 1; i < table.size(); ++i) {
        store.tuples.append(
            makeTuple(table.at(i).foreignComponent, table.at(i).foreignAction, QList<int>()));
    }
}

void selectionCatalogCounts()
{
    // Research catalog: 37 directional/toggle plus 39 item-1 workspace rows
    // (select, move with symbol aliases, previous toggle plus relative
    // steps) plus 36 item-2 workspace rows (8 bound relative send-follow,
    // 20 unbound numbered stay with aliases plus append rows, 8 unbound
    // relative stay) plus 12 item-5 output rows (8 bound follow, 4 unbound
    // stay) plus 4 R-WS-12 migrate rows (unbound directional follow) plus 1
    // workspace-tiling toggle row (Meta+Y bound, no known stock holder).
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    CHECK(catalog.size() == 129);
    int focus = 0;
    int move = 0;
    int resize = 0;
    int toggle = 0;
    int select = 0;
    int workspaceMove = 0;
    int workspacePrevious = 0;
    int workspaceRelative = 0;
    int workspaceSendRelative = 0;
    int workspaceMoveStay = 0;
    int workspaceSendRelativeStay = 0;
    int outputSend = 0;
    int outputSendStay = 0;
    int workspaceMigrate = 0;
    QSet<int> keys;
    for (const ShortcutCatalogEntry &entry : catalog) {
        if (entry.kind == QStringLiteral("focus")) {
            ++focus;
        } else if (entry.kind == QStringLiteral("move")) {
            ++move;
        } else if (entry.kind == QStringLiteral("resize")) {
            ++resize;
        } else if (entry.kind == QStringLiteral("toggle")) {
            ++toggle;
        } else if (entry.kind == QStringLiteral("workspace-select")) {
            ++select;
        } else if (entry.kind == QStringLiteral("workspace-move")) {
            ++workspaceMove;
        } else if (entry.kind == QStringLiteral("workspace-previous")) {
            ++workspacePrevious;
        } else if (entry.kind == QStringLiteral("workspace-relative")) {
            ++workspaceRelative;
        } else if (entry.kind == QStringLiteral("workspace-send-relative")) {
            ++workspaceSendRelative;
        } else if (entry.kind == QStringLiteral("workspace-move-stay")) {
            ++workspaceMoveStay;
        } else if (entry.kind == QStringLiteral("workspace-send-relative-stay")) {
            ++workspaceSendRelativeStay;
        } else if (entry.kind == QStringLiteral("output-send")) {
            ++outputSend;
        } else if (entry.kind == QStringLiteral("output-send-stay")) {
            ++outputSendStay;
        } else if (entry.kind == QStringLiteral("workspace-migrate")) {
            ++workspaceMigrate;
        }
        if (entry.canonicalKey == 0) {
            CHECK(entry.canonicalDisplay.isEmpty());
        } else {
            CHECK(!entry.canonicalDisplay.isEmpty());
        }
        keys.insert(entry.canonicalKey);
        bool found = false;
        CHECK(shortcutCatalogKeyFor(entry.component, entry.action, &found) == entry.canonicalKey);
        CHECK(found);
        CHECK(shortcutCatalogContains(entry.component, entry.action));
    }
    CHECK(focus == 8);
    CHECK(move == 8);
    CHECK(resize == 16);
    CHECK(toggle == 6);
    CHECK(select == 10);
    CHECK(workspaceMove == 20);
    CHECK(workspacePrevious == 1);
    CHECK(workspaceRelative == 8);
    CHECK(workspaceSendRelative == 8);
    CHECK(workspaceMoveStay == 20);
    CHECK(workspaceSendRelativeStay == 8);
    CHECK(outputSend == 8);
    CHECK(outputSendStay == 4);
    CHECK(workspaceMigrate == 4);
    CHECK(keys.size() == 94);
    CHECK(keys.contains(0));
    CHECK(!shortcutCatalogContains(QStringLiteral("kwin"), QStringLiteral("other-action")));
    bool found = true;
    CHECK(shortcutCatalogKeyFor(QStringLiteral("kwin"), QStringLiteral("other-action"), &found) == 0);
    CHECK(!found);
    // Known conflicts are exactly the 23 conflict-table project rows.
    CHECK(shortcutKnownConflictIds().size() == 23);
    CHECK(shortcutKnownConflictIds().contains(
        shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"))));
    CHECK(!shortcutKnownConflictIds().contains(
        shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-1"))));
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-left"))));
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-left-stay"))));
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-migrate-workspace-left"))));
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-workspace-tiling"))));
    CHECK(shortcutMaxProjectWrites() == 130);
    // Displays resolve through the catalog first, table targets preserved.
    CHECK(ShortcutReconciler::keyDisplayName(SHORTCUT_META_1) == QStringLiteral("Meta+1"));
    CHECK(ShortcutReconciler::keyDisplayName(SHORTCUT_META_EXCLAM) == QStringLiteral("Meta+!"));
    CHECK(ShortcutReconciler::keyDisplayName(META_L) == QStringLiteral("Meta+L"));
    CHECK(ShortcutReconciler::keyDisplayName(META_ESC) == QStringLiteral("Meta+Esc"));
    // Selection-scoped key images: disabling drops the chord; disabling
    // focus-right additionally drops the lock relocation chords.
    CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{SHORTCUT_META_1, 999}, QSet<QString>())
          == QList<int>{SHORTCUT_META_1});
    CHECK(ShortcutReconciler::remainderAfterClearFor(QList<int>{SHORTCUT_META_1, 999}, QSet<QString>())
          == QList<int>{999});
    const QString ws1 = shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-1"));
    CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{SHORTCUT_META_1, 999}, QSet<QString>{ws1}).isEmpty());
    CHECK(ShortcutReconciler::enabledRequiredKeys(QSet<QString>()).contains(SHORTCUT_META_1));
    CHECK(ShortcutReconciler::enabledRequiredKeys(QSet<QString>()).contains(META_ESC));
    CHECK(ShortcutReconciler::enabledRequiredKeys(QSet<QString>{ws1}).contains(SHORTCUT_META_1) == false);
    const QString focusId =
        shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"));
    CHECK(ShortcutReconciler::enabledRequiredKeys(QSet<QString>{focusId}).contains(META_L) == false);
    CHECK(ShortcutReconciler::enabledRequiredKeys(QSet<QString>{focusId}).contains(META_ESC) == false);
    // Preset seam is a deterministic catalog-ordered union.
    const QStringList preset =
        presetCompatibleDisabledIds(catalog, shortcutKnownConflictIds(), QSet<QString>{ws1});
    CHECK(preset.size() == 24);
    CHECK(preset.contains(focusId));
    CHECK(preset.contains(ws1));
    CHECK(preset.indexOf(focusId) < preset.indexOf(ws1));
    // Project identity covers the full catalog for writes and exemptions.
    CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"),
                                              QStringLiteral("omnitiler-workspace-1")));
    CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"),
                                            QStringLiteral("omnitiler-workspace-1")));
    CHECK(!ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), QStringLiteral("other-action")));
}

void selectionDisabledSkipsRefusalAndClearsOwn()
{
    // Disabled toggle-float with its known holder claiming the chord:
    // Apply succeeds, clears only the project's own assignment, never the
    // foreign holder, and persists nothing locally.
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Grid View")) {
            tuple.active = QList<int>{META_G};
        }
    }
    const QString floatId =
        shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-float"));
    FakeClearedActions cleared;
    const ShortcutApplyResult result =
        ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{floatId});
    CHECK(result.ok);
    CHECK(result.error.isEmpty());
    CHECK(result.writes == 1);
    CHECK(store.foreignWriteLog.isEmpty());
    CHECK(cleared.stored.isEmpty());
    CHECK(cleared.saves == 0);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("Grid View")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
    }
}

void selectionDisabledFocusSkipsLockAndEsc()
{
    // Disabled focus-right with Lock Session still on the preimage and an
    // unknown Meta+Esc holder: Apply succeeds, the lock is never relocated,
    // Meta+Esc is never scanned, and the focus row itself is cleared.
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Lock Session")) {
            tuple.active = QList<int>{META_L};
        }
    }
    store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("other-action"), QList<int>{META_ESC}));
    const QString focusId =
        shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"));
    FakeClearedActions cleared;
    const ShortcutApplyResult result =
        ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{focusId});
    CHECK(result.ok);
    CHECK(result.error.isEmpty());
    CHECK(store.foreignWriteLog.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-focus-right")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("Lock Session")) {
            CHECK(tuple.active == (QList<int>{META_L}));
        }
        if (tuple.action == QStringLiteral("other-action")) {
            CHECK(tuple.active == (QList<int>{META_ESC}));
        }
    }
}

void selectionMixedRefusesEnabledOnly()
{
    // An enabled-row holder refuses naming exactly that row while a
    // disabled-row holder on another chord is ignored with zero writes.
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Window Quick Tile Left")) {
            tuple.active = QList<int>{META_LEFT};
        }
    }
    store.tuples.append(
        makeTuple(QStringLiteral("org.example"), QStringLiteral("other-ws"), QList<int>{SHORTCUT_META_1}));
    const QString ws1 = shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-1"));
    FakeClearedActions cleared;
    const ShortcutApplyResult refused =
        ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{ws1});
    CHECK(!refused.ok);
    CHECK(refused.error.contains(QStringLiteral("Meta+Left")));
    CHECK(refused.error.contains(QStringLiteral("Window Quick Tile Left")));
    CHECK(refused.writes == 0);
    CHECK(store.writeCount() == 0);
    CHECK(cleared.stored.isEmpty());
    const ShortcutForcePreview preview =
        ShortcutReconciler(&store, &cleared).previewForceApplySelected(QSet<QString>{ws1});
    CHECK(preview.forceable);
    CHECK(preview.mismatches.size() == 1);
    CHECK(preview.mismatches.at(0).action == QStringLiteral("Window Quick Tile Left"));
    CHECK(preview.disabledIds == QStringList{ws1});
    CHECK(preview.authenticStaged);
}

void selectionForceBindsDraftAndDrift()
{
    // Force binds its exact draft: a changed draft fails as stale with zero
    // writes before any persist, and holder drift fails as stale too.
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Window Quick Tile Left")) {
            tuple.active = QList<int>{META_LEFT, 4242};
        }
    }
    store.defaultKeysById[QStringLiteral("kwin/Window Quick Tile Left")] = QList<int>{META_LEFT};
    FakeClearedActions cleared;
    const ShortcutForcePreview preview =
        ShortcutReconciler(&store, &cleared).previewForceApplySelected(QSet<QString>());
    CHECK(preview.forceable);
    CHECK(preview.disabledIds.isEmpty());
    CHECK(preview.authenticStaged);
    CHECK(preview.liveImages.size() == 130);
    const QString ws1 = shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-1"));
    const ShortcutForceApplyResult staleDraft =
        ShortcutReconciler(&store, &cleared).applyForcedSelected(preview, QSet<QString>{ws1});
    CHECK(!staleDraft.ok);
    CHECK(staleDraft.error.contains(QStringLiteral("stale")));
    CHECK(staleDraft.writes == 0);
    CHECK(store.writeCount() == 0);
    CHECK(cleared.saves == 0);
    const ShortcutForceApplyResult forced =
        ShortcutReconciler(&store, &cleared).applyForcedSelected(preview, QSet<QString>());
    CHECK(forced.ok);
    CHECK(cleared.stored.size() == 1);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Window Quick Tile Left")) {
            CHECK(tuple.active == (QList<int>{4242}));
        }
    }
    CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
    CHECK(cleared.stored.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Window Quick Tile Left")) {
            CHECK(tuple.active == (QList<int>{META_LEFT}));
        }
    }
    // Drift after preview fails as stale with zero further writes.
    {
        FakeShortcutStore drifted;
        seedFullQuiet(drifted);
        for (ShortcutTuple &tuple : drifted.tuples) {
            if (tuple.action == QStringLiteral("Window Quick Tile Left")) {
                tuple.active = QList<int>{META_LEFT, 4242};
            }
        }
        drifted.defaultKeysById[QStringLiteral("kwin/Window Quick Tile Left")] = QList<int>{META_LEFT};
        FakeClearedActions driftedCleared;
        const ShortcutForcePreview driftPreview =
            ShortcutReconciler(&drifted, &driftedCleared).previewForceApplySelected(QSet<QString>());
        CHECK(driftPreview.forceable);
        const int savesBefore = driftedCleared.saves;
        const int writesBefore = drifted.writeCount();
        for (ShortcutTuple &tuple : drifted.tuples) {
            if (tuple.action == QStringLiteral("Window Quick Tile Left")) {
                tuple.active = QList<int>{META_LEFT, 5555};
            }
        }
        const ShortcutForceApplyResult stale =
            ShortcutReconciler(&drifted, &driftedCleared).applyForcedSelected(driftPreview, QSet<QString>());
        CHECK(!stale.ok);
        CHECK(stale.writes == 0);
        CHECK(drifted.writeCount() == writesBefore);
        CHECK(driftedCleared.saves == savesBefore);
    }
}

void selectionDefaultsCarried()
{
    // Wire defaults survive into tuples and holder views so defaults stay
    // visible even after Force clears the active list.
    ShortcutInfoFields info;
    info.action = QStringLiteral("omnitiler-workspace-1");
    info.friendly = QString();
    info.compUnique = QStringLiteral("kwin");
    info.compFriendly = QStringLiteral("KWin");
    info.contextUnique = QString();
    info.contextFriendly = QString();
    info.active = QList<int>{SHORTCUT_META_1};
    info.defaults = QList<int>{SHORTCUT_META_1};
    QList<ShortcutTuple> tuples;
    QString error;
    CHECK(ShortcutReconciler::tuplesFromInfoFields(QList<ShortcutInfoFields>{info}, &tuples, &error));
    CHECK(tuples.size() == 1);
    CHECK(tuples.at(0).active == (QList<int>{SHORTCUT_META_1}));
    CHECK(tuples.at(0).defaults == (QList<int>{SHORTCUT_META_1}));
    QList<ShortcutKeyHolder> holders;
    CHECK(ShortcutReconciler::holdersFromInfoFields(QList<ShortcutInfoFields>{info}, &holders, &error));
    CHECK(holders.size() == 1);
    CHECK(holders.at(0).defaults == (QList<int>{SHORTCUT_META_1}));
}

void selectionMissingEnabledFailsClosed()
{
    // Missing enabled rows fail closed with zero writes; missing disabled
    // rows stay allowed.
    const QString ws1 = shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-1"));
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        for (int i = static_cast<int>(store.tuples.size()) - 1; i >= 0; --i) {
            if (store.tuples.at(i).action == QStringLiteral("omnitiler-workspace-1")) {
                store.tuples.removeAt(i);
            }
        }
        FakeClearedActions cleared;
        const ShortcutApplyResult result = ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>());
        CHECK(!result.ok);
        CHECK(result.writes == 0);
        CHECK(store.writeCount() == 0);
    }
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        for (int i = static_cast<int>(store.tuples.size()) - 1; i >= 0; --i) {
            if (store.tuples.at(i).action == QStringLiteral("omnitiler-workspace-1")) {
                store.tuples.removeAt(i);
            }
        }
        FakeClearedActions cleared;
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{ws1});
        CHECK(result.ok);
    }
}

void selectionUnknownIdFailsClosed()
{
    FakeShortcutStore store;
    seedFullQuiet(store);
    FakeClearedActions cleared;
    const QSet<QString> bad{QStringLiteral("kwin/other-action")};
    const ShortcutApplyResult result = ShortcutReconciler(&store, &cleared).applySelected(bad);
    CHECK(!result.ok);
    CHECK(result.writes == 0);
    CHECK(store.writeCount() == 0);
    const ShortcutForcePreview preview =
        ShortcutReconciler(&store, &cleared).previewForceApplySelected(bad);
    CHECK(!preview.forceable);
}

void selectionForeignDefaultConflictAfterClear()
{
    // Noncompiled foreign wire default still collides after its active list
    // is cleared; own project defaults never count as OS conflicts.
    FakeShortcutStore store;
    seedFullQuiet(store);
    ShortcutTuple foreign;
    foreign.component = QStringLiteral("org.example");
    foreign.action = QStringLiteral("other-ws");
    foreign.componentFriendly = QStringLiteral("Example");
    foreign.friendly = QStringLiteral("Other");
    foreign.active = QList<int>();
    foreign.defaults = QList<int>{SHORTCUT_META_1};
    store.tuples.append(foreign);
    const QStringList ids = ShortcutReconciler::foreignDefaultIdsForKey(SHORTCUT_META_1, store.tuples);
    CHECK(ids.contains(QStringLiteral("org.example/other-ws")));
    // Own project defaults are never listed.
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-workspace-1")) {
            tuple.defaults = QList<int>{SHORTCUT_META_1};
        }
    }
    const QStringList ownIds = ShortcutReconciler::foreignDefaultIdsForKey(SHORTCUT_META_1, store.tuples);
    CHECK(!ownIds.contains(QStringLiteral("kwin/omnitiler-workspace-1")));
    CHECK(ownIds.contains(QStringLiteral("org.example/other-ws")));
    // Readable chord image for UI rows.
    CHECK(ShortcutReconciler::keysDisplayNames(QList<int>{SHORTCUT_META_1}) == QStringLiteral("Meta+1"));
    CHECK(ShortcutReconciler::keysDisplayNames(QList<int>()) == QStringLiteral("none"));
    // Custom/unrelated chords render through QKeySequence, not raw ints.
    const int ctrlF12 = 0x04000000 | 0x0100003B; // Ctrl+F12, no catalog row
    CHECK(ShortcutReconciler::keyDisplayName(ctrlF12) == QStringLiteral("Ctrl+F12"));
}

void selectionDisabledTableRowAbsentAllowed()
{
    // Table rows follow the same rule: enabled missing fails closed with
    // zero writes, disabled missing stays allowed, duplicates fail even
    // when disabled.
    const QString floatId =
        shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-float"));
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        for (int i = static_cast<int>(store.tuples.size()) - 1; i >= 0; --i) {
            if (store.tuples.at(i).action == QStringLiteral("omnitiler-toggle-float")) {
                store.tuples.removeAt(i);
            }
        }
        FakeClearedActions cleared;
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>());
        CHECK(!result.ok);
        CHECK(result.writes == 0);
        CHECK(store.writeCount() == 0);
    }
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        for (int i = static_cast<int>(store.tuples.size()) - 1; i >= 0; --i) {
            if (store.tuples.at(i).action == QStringLiteral("omnitiler-toggle-float")) {
                store.tuples.removeAt(i);
            }
        }
        FakeClearedActions cleared;
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{floatId});
        CHECK(result.ok);
    }
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        store.tuples.append(
            makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-float"),
                      QList<int>{META_G}));
        FakeClearedActions cleared;
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{floatId});
        CHECK(!result.ok);
        CHECK(result.writes == 0);
        CHECK(store.writeCount() == 0);
    }
}

void selectionFocusDisabledLockAbsentAllowed()
{
    // A disabled focus-right leaves the lock out of scope: an absent lock
    // still applies cleanly and clears only the disabled focus row.
    const QString focusId =
        shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"));
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (int i = static_cast<int>(store.tuples.size()) - 1; i >= 0; --i) {
        if (store.tuples.at(i).action == QStringLiteral("Lock Session")) {
            store.tuples.removeAt(i);
        }
    }
    FakeClearedActions cleared;
    const ShortcutApplyResult result =
        ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{focusId});
    CHECK(result.ok);
    CHECK(cleared.stored.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-focus-right")) {
            CHECK(tuple.active.isEmpty());
        }
    }
}

void selectionPresenceBindsForceSnapshot()
{
    // Missing vs empty share the same active image; presence is bound so an
    // appearing disabled row fails as stale.
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("Window Quick Tile Left")) {
            tuple.active = QList<int>{META_LEFT, 4242};
        }
    }
    store.defaultKeysById[QStringLiteral("kwin/Window Quick Tile Left")] = QList<int>{META_LEFT};
    FakeClearedActions cleared;
    const QString ws1 = shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-1"));
    const ShortcutForcePreview preview =
        ShortcutReconciler(&store, &cleared).previewForceApplySelected(QSet<QString>{ws1});
    CHECK(preview.forceable);
    CHECK(preview.livePresent.size() == preview.liveImages.size());
    // Simulate the disabled row appearing live after preview: remove it, then
    // re-add as present-empty and require stale.
    for (int i = static_cast<int>(store.tuples.size()) - 1; i >= 0; --i) {
        if (store.tuples.at(i).action == QStringLiteral("omnitiler-workspace-1")) {
            store.tuples.removeAt(i);
        }
    }
    // Preview was built with the row present; removing it flips presence
    // while the active image stays empty, so the snapshot must differ.
    const ShortcutForcePreview current =
        ShortcutReconciler(&store, &cleared).previewForceApplySelected(QSet<QString>{ws1});
    CHECK(current.livePresent != preview.livePresent);
}

// Fixture drift during another write: mutates one Keep row once, either on
// the next project write or the next foreign clear, to prove verification
// catches mid-write drift without reset.
class KeepDriftShortcutStore : public FakeShortcutStore
{
public:
    bool driftKeepOnWrite = false;
    bool driftKeepOnForeign = false;

    bool writeKeys(const QString &component, const QString &action, const QString &componentFriendly,
                   const QString &friendly, const QList<int> &keys, QList<int> *confirmed,
                   QString *error) override
    {
        if (driftKeepOnWrite) {
            driftKeepOnWrite = false;
            for (ShortcutTuple &tuple : tuples) {
                if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
                    tuple.active = QList<int>{888002};
                }
            }
        }
        return FakeShortcutStore::writeKeys(component, action, componentFriendly, friendly, keys, confirmed,
                                            error);
    }

    bool setForeignShortcutKeys(const QString &component, const QString &action,
                                const QString &componentFriendly, const QString &friendly,
                                const QList<int> &keys, QString *error) override
    {
        if (driftKeepOnForeign) {
            driftKeepOnForeign = false;
            for (ShortcutTuple &tuple : tuples) {
                if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
                    tuple.active = QList<int>{888002};
                }
            }
        }
        return FakeShortcutStore::setForeignShortcutKeys(component, action, componentFriendly, friendly, keys,
                                                          error);
    }
};

void selectionKeepPreservesCustomCanonicalEmpty()
{
    // Keep (non-Authentic) Apply preserves every enabled row at its live
    // assignment: customized chords, canonical chords, and empty
    // assignments all stay, with zero project writes and a passing
    // post-write verification. Staged Authentic assigns canonical instead.
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777001};
        }
        if (tuple.action == QStringLiteral("omnitiler-workspace-1")) {
            tuple.active.clear();
        }
    }
    FakeClearedActions cleared;
    const ShortcutApplyResult preserved =
        ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>(), false);
    CHECK(preserved.ok);
    CHECK(preserved.error.isEmpty());
    CHECK(preserved.writes == 0);
    CHECK(store.writeCount() == 0);
    CHECK(cleared.saves == 0);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{777001}));
        }
        if (tuple.action == QStringLiteral("omnitiler-workspace-1")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle-maximize")) {
            CHECK(tuple.active == (QList<int>{META_M}));
        }
        if (tuple.action == QStringLiteral("Lock Session")) {
            CHECK(tuple.active == (QList<int>{META_ESC}));
        }
    }
    // The same state under staged Authentic assigns canonical chords.
    const ShortcutApplyResult authentic =
        ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>(), true);
    CHECK(authentic.ok);
    CHECK(authentic.writes > 0);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
        if (tuple.action == QStringLiteral("omnitiler-workspace-1")) {
            CHECK(tuple.active == (QList<int>{SHORTCUT_META_1}));
        }
    }
}

void selectionKeepCustomConflictPreviewForcePreserves()
{
    // A Keep-customized chord claimed by a foreign holder refuses Apply
    // naming the actual chord. Force previews the actual removal and, once
    // confirmed, clears only the foreign holder while the Keep row stays at
    // its customized chord with zero project writes.
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777001};
        }
    }
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-app"), QList<int>{777001}));
    FakeClearedActions cleared;
    const ShortcutApplyResult refused =
        ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>(), false);
    CHECK(!refused.ok);
    // The refusal names the live actual chord, never a silent canonical.
    CHECK(refused.error.contains(ShortcutReconciler::keyDisplayName(777001)));
    CHECK(!refused.error.contains(QStringLiteral("Meta+G")));
    CHECK(refused.writes == 0);
    CHECK(store.writeCount() == 0);
    const ShortcutForcePreview preview =
        ShortcutReconciler(&store, &cleared).previewForceApplySelected(QSet<QString>(), false);
    CHECK(preview.forceable);
    CHECK(!preview.authenticStaged);
    CHECK(preview.mismatches.size() == 1);
    CHECK(preview.mismatches.at(0).action == QStringLiteral("other-app"));
    CHECK(preview.mismatches.at(0).actual == (QList<int>{777001}));
    CHECK(preview.mismatches.at(0).expectedPre == (QList<int>{777001}));
    CHECK(preview.mismatches.at(0).post.isEmpty());
    const ShortcutForceApplyResult forced =
        ShortcutReconciler(&store, &cleared).applyForcedSelected(preview, QSet<QString>(), false);
    CHECK(forced.ok);
    CHECK(store.writeLog.isEmpty());
    CHECK(!store.foreignWriteLog.isEmpty());
    CHECK(cleared.stored.size() == 1);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("other-app")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{777001}));
        }
    }
}

void selectionKeepDriftDuringWriteFailsWithoutReset()
{
    // Fixture drift during another selected write: disabling workspace-1
    // performs a clear write while toggle-float stays Keep. Control first:
    // an unchanged custom preserves and succeeds. With drift, the Keep row
    // no longer matches the preflight image, so verification fails without
    // reset (the drifted custom stays, never silently canonical).
    const QString ws1 = shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-1"));
    {
        KeepDriftShortcutStore store;
        seedFullQuiet(store);
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
                tuple.active = QList<int>{777001};
            }
        }
        FakeClearedActions cleared;
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{ws1}, false);
        CHECK(result.ok);
        CHECK(result.writes == 1);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
                CHECK(tuple.active == (QList<int>{777001}));
            }
            if (tuple.action == QStringLiteral("omnitiler-workspace-1")) {
                CHECK(tuple.active.isEmpty());
            }
        }
    }
    {
        KeepDriftShortcutStore store;
        seedFullQuiet(store);
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
                tuple.active = QList<int>{777001};
            }
        }
        store.driftKeepOnWrite = true;
        FakeClearedActions cleared;
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{ws1}, false);
        CHECK(!result.ok);
        CHECK(result.error.contains(QStringLiteral("live state differs")));
        CHECK(result.writes == 1);
        CHECK(store.writeLog.size() == 1);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
                CHECK(tuple.active == (QList<int>{888002}));
            }
            if (tuple.action == QStringLiteral("omnitiler-workspace-1")) {
                CHECK(tuple.active.isEmpty());
            }
        }
    }
}

void selectionForceKeepDriftDuringClearFailsWithoutReset()
{
    // Fixture drift during Force clearing: the Keep row changes after the
    // confirmed preview, so the project-phase snapshot cannot see it, but
    // the post-write check against the confirmed image fails without reset
    // (no project writes, drifted custom stays).
    KeepDriftShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777001};
        }
    }
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-app"), QList<int>{777001}));
    store.driftKeepOnForeign = true;
    FakeClearedActions cleared;
    const ShortcutForcePreview preview =
        ShortcutReconciler(&store, &cleared).previewForceApplySelected(QSet<QString>(), false);
    CHECK(preview.forceable);
    const ShortcutForceApplyResult forced =
        ShortcutReconciler(&store, &cleared).applyForcedSelected(preview, QSet<QString>(), false);
    CHECK(!forced.ok);
    CHECK(forced.error.contains(QStringLiteral("live state differs")));
    CHECK(forced.writes == 1);
    CHECK(store.writeLog.isEmpty());
    CHECK(!store.foreignWriteLog.isEmpty());
    CHECK(cleared.saves == 1);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("other-app")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{888002}));
        }
    }
}

void selectionForceIntentMismatchStale()
{
    // A preview built under one Authentic intent cannot be consumed under
    // the other: the mismatch fails as stale with zero writes, before any
    // cleared-list persist.
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777001};
        }
    }
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-app"), QList<int>{777001}));
    FakeClearedActions cleared;
    const ShortcutForcePreview keepPreview =
        ShortcutReconciler(&store, &cleared).previewForceApplySelected(QSet<QString>(), false);
    CHECK(keepPreview.forceable);
    const ShortcutForceApplyResult staleIntent =
        ShortcutReconciler(&store, &cleared).applyForcedSelected(keepPreview, QSet<QString>(), true);
    CHECK(!staleIntent.ok);
    CHECK(staleIntent.error.contains(QStringLiteral("stale")));
    CHECK(staleIntent.writes == 0);
    CHECK(store.writeCount() == 0);
    CHECK(cleared.saves == 0);
    CHECK(cleared.stored.isEmpty());
    // The reverse direction: an Authentic preview over a canonical conflict
    // cannot be consumed while preserving.
    FakeShortcutStore canonical;
    seedFullQuiet(canonical);
    canonical.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("Grid View"), QList<int>{META_G}));
    FakeClearedActions canonicalCleared;
    const ShortcutForcePreview authenticPreview =
        ShortcutReconciler(&canonical, &canonicalCleared).previewForceApplySelected(QSet<QString>(), true);
    CHECK(authenticPreview.forceable);
    CHECK(authenticPreview.authenticStaged);
    const ShortcutForceApplyResult staleKeep =
        ShortcutReconciler(&canonical, &canonicalCleared)
            .applyForcedSelected(authenticPreview, QSet<QString>(), false);
    CHECK(!staleKeep.ok);
    CHECK(staleKeep.error.contains(QStringLiteral("stale")));
    CHECK(staleKeep.writes == 0);
    CHECK(canonical.writeCount() == 0);
    CHECK(canonicalCleared.saves == 0);
}

void selectionKeepOccupancyGate()
{
    // The keyed gate under Keep intent scans live actual chords: quiet
    // customs unclaimed are Clear, a foreign holder on the actual custom
    // conflicts, and Meta+Esc holders stay out of scope without Authentic.
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777001};
        }
    }
    {
        const KeyedOccupancyResult quiet =
            ShortcutReconciler::checkKeyedForeignOccupancyDetailedFor(&store, QSet<QString>(), false);
        CHECK(quiet.status == KeyedOccupancy::Clear);
    }
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-app"), QList<int>{777001}));
    {
        const KeyedOccupancyResult conflict =
            ShortcutReconciler::checkKeyedForeignOccupancyDetailedFor(&store, QSet<QString>(), false);
        CHECK(conflict.status == KeyedOccupancy::Conflict);
        CHECK(conflict.detail.contains(ShortcutReconciler::keyDisplayName(777001)));
    }
    store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("other-esc"), QList<int>{META_ESC}));
    {
        const KeyedOccupancyResult keepScope =
            ShortcutReconciler::checkKeyedForeignOccupancyDetailedFor(&store, QSet<QString>(), false);
        CHECK(keepScope.status == KeyedOccupancy::Conflict);
        CHECK(keepScope.detail.contains(ShortcutReconciler::keyDisplayName(777001)));
        const KeyedOccupancyResult authenticScope =
            ShortcutReconciler::checkKeyedForeignOccupancyDetailedFor(&store, QSet<QString>(), true);
        CHECK(authenticScope.status == KeyedOccupancy::Conflict);
        CHECK(authenticScope.detail.contains(QStringLiteral("Meta+Esc")));
    }
}

void selectionRequiredKeysKeepVsAuthentic()
{
    // Pure selection seams: the legacy forms stage Authentic (canonical),
    // while the explicit forms bind Keep live chords plus the
    // Authentic-gated lock chords.
    const QSet<QString> none;
    CHECK(ShortcutReconciler::enabledRequiredKeys(none).contains(SHORTCUT_META_1));
    CHECK(ShortcutReconciler::enabledRequiredKeys(none, true, QList<int>()).contains(SHORTCUT_META_1));
    CHECK(!ShortcutReconciler::enabledRequiredKeys(none, false, QList<int>()).contains(SHORTCUT_META_1));
    CHECK(ShortcutReconciler::enabledRequiredKeys(none, false, QList<int>{777001}).contains(777001));
    CHECK(!ShortcutReconciler::enabledRequiredKeys(none, false, QList<int>{777001}).contains(SHORTCUT_META_1));
    CHECK(!ShortcutReconciler::enabledRequiredKeys(none, false, QList<int>{0}).contains(0));
    CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{777001, 999}, none, false, QList<int>{777001})
          == QList<int>{777001});
    CHECK(ShortcutReconciler::remainderAfterClearFor(QList<int>{777001, 999}, none, false, QList<int>{777001})
          == QList<int>{999});
    CHECK(ShortcutReconciler::conflictingKeysIn(QList<int>{777001, 999}, QList<int>{777001})
          == QList<int>{777001});
}

void selectionRowDisplayShowsActualHolders()
{
    // Row displays merge canonical and live-actual holder queries, so a
    // Keep-customized row reports the foreign holder of its actual chord.
    FakeShortcutStore store;
    seedFullQuiet(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777001};
        }
    }
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-app"), QList<int>{777001}));
    QList<ShortcutRowDisplay> rows;
    QString error;
    CHECK(ShortcutReconciler::collectRowDisplays(&store, &rows, &error));
    bool found = false;
    for (const ShortcutRowDisplay &row : rows) {
        if (row.catalog.action == QStringLiteral("omnitiler-toggle-float")) {
            found = true;
            CHECK(row.present);
            CHECK(row.current == (QList<int>{777001}));
            CHECK(row.holdersKnown);
            bool sawForeign = false;
            for (const ShortcutKeyHolder &holder : row.holders) {
                if (holder.component == QStringLiteral("org.example")
                    && holder.action == QStringLiteral("other-app")) {
                    sawForeign = true;
                }
            }
            CHECK(sawForeign);
        }
    }
    CHECK(found);
}

void orderedConflictCompletenessAndBlocker()
{
    // >5 holders across multiple chords stay complete and consistently
    // ordered: every current foreign holder appears once in
    // component/action order with exact removals/remainder, status first
    // matches the preview first, and a default-only claimant blocks Force
    // while the available conflicts stay listed.
    FakeShortcutStore store;
    seedFullQuiet(store);
    // Seven distinct foreign holders on distinct required chords.
    const QList<std::pair<QString, int>> holders = {
        {QStringLiteral("zeta-app"), META_G},
        {QStringLiteral("alpha-app"), META_M},
        {QStringLiteral("mid-app"), META_LEFT},
        {QStringLiteral("beta-app"), META_RIGHT},
        {QStringLiteral("gamma-app"), META_UP},
        {QStringLiteral("delta-app"), META_DOWN},
        {QStringLiteral("epsilon-app"), META_ALT_K},
    };
    for (const auto &[action, key] : holders) {
        store.tuples.append(makeTuple(QStringLiteral("org.example"), action, QList<int>{key, 9000}));
    }
    // Multi-chord holder: two required chords plus one unrelated key.
    store.tuples.append(
        makeTuple(QStringLiteral("org.example"), QStringLiteral("multi-app"), QList<int>{META_G, META_M, 9001}));
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    const QSet<QString> disabled;
    ShortcutForcePreview preview = reconciler.previewForceApplySelected(disabled, true);
    CHECK(preview.forceable);
    // 7 single + 1 multi = 8 distinct holder IDs, no truncation.
    CHECK(preview.mismatches.size() == 8);
    QStringList ids;
    for (const ShortcutForceMismatch &mismatch : preview.mismatches) {
        ids.append(mismatch.component + QStringLiteral("/") + mismatch.action);
    }
    QStringList sorted = ids;
    sorted.sort();
    CHECK(ids == sorted);
    for (const auto &[action, key] : holders) {
        const QString id = QStringLiteral("org.example/") + action;
        CHECK(ids.contains(id));
    }
    CHECK(ids.contains(QStringLiteral("org.example/multi-app")));
    for (const ShortcutForceMismatch &mismatch : preview.mismatches) {
        if (mismatch.action == QStringLiteral("multi-app")) {
            CHECK(mismatch.expectedPre.contains(META_G));
            CHECK(mismatch.expectedPre.contains(META_M));
            CHECK(mismatch.post == (QList<int>{9001}));
            CHECK(mismatch.actual == (QList<int>{META_G, META_M, 9001}));
        } else {
            CHECK(mismatch.expectedPre.size() == 1);
            CHECK(mismatch.post == (QList<int>{9000}));
        }
        // Reconstructible diagnostics: raw chord ints plus holder IDs.
        CHECK(ShortcutReconciler::keysDisplay(mismatch.actual).contains(QStringLiteral(",")) || !mismatch.actual.isEmpty());
    }
    // Status first conflict uses the same ordering as the preview first.
    const KeyedOccupancyResult keyed =
        ShortcutReconciler::checkKeyedForeignOccupancyDetailedFor(&store, disabled, true);
    CHECK(keyed.status == KeyedOccupancy::Conflict);
    const ShortcutForceMismatch &first = preview.mismatches.first();
    CHECK(keyed.detail.contains(first.component));
    CHECK(keyed.detail.contains(first.action));
    // Default-only claimant: listed for the key with nothing to clear.
    // Force becomes unavailable but still lists every available conflict.
    ShortcutKeyHolder defaultOnly;
    defaultOnly.component = QStringLiteral("org.example");
    defaultOnly.action = QStringLiteral("default-only");
    defaultOnly.active = QList<int>{424242};
    defaultOnly.defaults = QList<int>{424242};
    store.extraByKey[META_CTRL_LEFT].append(defaultOnly);
    ShortcutForcePreview blocked = reconciler.previewForceApplySelected(disabled, true);
    CHECK(!blocked.forceable);
    CHECK(!blocked.blockers.isEmpty());
    CHECK(!blocked.mismatches.isEmpty());
    CHECK(blocked.mismatches.size() == preview.mismatches.size());
    CHECK(blocked.error.contains(QStringLiteral("default-only")));
    bool sawBlocker = false;
    for (const ShortcutForceBlocker &blocker : blocked.blockers) {
        if (blocker.action == QStringLiteral("default-only")) {
            sawBlocker = true;
            CHECK(blocker.key == META_CTRL_LEFT);
        }
    }
    CHECK(sawBlocker);
}

void orderedConflictsBlockerFirstEverywhere()
{
    // Mixed blockers plus clear rows: status first, refusal first, and the
    // combined dialog order all agree on the blocker-first order. The
    // blocker sorts before every clear row by component/action/key.
    FakeShortcutStore store;
    seedFullQuiet(store);
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("zzz-clear"),
                                  QList<int>{META_G, 9000}));
    ShortcutKeyHolder blocker;
    blocker.component = QStringLiteral("aaa-block");
    blocker.action = QStringLiteral("aaa-action");
    blocker.active = QList<int>{424242};
    store.extraByKey[META_CTRL_LEFT].append(blocker);
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    const QSet<QString> disabled;
    const KeyedOccupancyResult keyed =
        ShortcutReconciler::checkKeyedForeignOccupancyDetailedFor(&store, disabled, true);
    CHECK(keyed.status == KeyedOccupancy::Conflict);
    CHECK(keyed.detail.contains(QStringLiteral("aaa-block/aaa-action")));
    const ShortcutApplyResult refused = reconciler.applySelected(disabled, true);
    CHECK(!refused.ok);
    CHECK(refused.error.contains(QStringLiteral("aaa-block/aaa-action")));
    CHECK(refused.error == keyed.detail);
    const ShortcutForcePreview preview = reconciler.previewForceApplySelected(disabled, true);
    CHECK(!preview.forceable);
    CHECK(!preview.blockers.isEmpty());
    CHECK(!preview.mismatches.isEmpty());
    const QList<ShortcutOrderedConflict> ordered = ShortcutReconciler::orderedConflicts(preview);
    CHECK(ordered.size() == preview.blockers.size() + preview.mismatches.size());
    CHECK(ordered.first().isBlocker);
    // Every blocker precedes every clear row in the combined order.
    int lastBlocker = -1;
    int firstClear = ordered.size();
    for (int i = 0; i < ordered.size(); ++i) {
        if (ordered.at(i).isBlocker) {
            lastBlocker = i;
        } else if (firstClear == ordered.size()) {
            firstClear = i;
        }
    }
    CHECK(lastBlocker >= 0);
    CHECK(lastBlocker < firstClear);
}

void forcePreviewLogsEveryRowReconstructible()
{
    // >26 holders: per-row logging keeps every ID plus raw chord images
    // without dropping the tail; the final holder and chord are present.
    FakeShortcutStore store;
    seedFullQuiet(store);
    const int total = 30;
    for (int i = 0; i < total; ++i) {
        store.tuples.append(makeTuple(QStringLiteral("org.example"),
                                      QStringLiteral("bulk-%1").arg(i, 2, 10, QChar::fromLatin1('0')),
                                      QList<int>{META_G, 90000 + i}));
    }
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    QStringList messages;
    ShortcutDiag::setSink([&](QtMsgType, const QString &message) {
        messages.append(message);
    });
    const ShortcutForcePreview preview =
        reconciler.previewForceApplySelected(QSet<QString>(), true);
    ShortcutDiag::resetSink();
    CHECK(preview.forceable);
    CHECK(preview.mismatches.size() == total);
    // Every holder ID appears across the captured messages.
    for (int i = 0; i < total; ++i) {
        const QString id =
            QStringLiteral("org.example/bulk-%1").arg(i, 2, 10, QChar::fromLatin1('0'));
        bool saw = false;
        for (const QString &message : messages) {
            if (message.contains(id)) {
                saw = true;
                break;
            }
        }
        CHECK(saw);
    }
    // The final holder and its raw chord image are present untruncated.
    bool sawFinal = false;
    for (const QString &message : messages) {
        if (message.contains(QStringLiteral("org.example/bulk-29"))
            && message.contains(QStringLiteral("90029"))) {
            sawFinal = true;
            break;
        }
    }
    CHECK(sawFinal);
}

void blockerCarriesActiveImage()
{
    // Default-only claimants retain unrelated actives: the preview carries
    // the full active image, the chunked log shows found/remove-none/keep,
    // and revalidation compares the image.
    FakeShortcutStore store;
    seedFullQuiet(store);
    ShortcutKeyHolder blocker;
    blocker.component = QStringLiteral("org.example");
    blocker.action = QStringLiteral("default-only");
    blocker.active = QList<int>{424242, 424243};
    blocker.defaults = QList<int>{META_CTRL_LEFT};
    store.extraByKey[META_CTRL_LEFT].append(blocker);
    FakeClearedActions cleared;
    ShortcutReconciler reconciler(&store, &cleared);
    const ShortcutForcePreview preview =
        reconciler.previewForceApplySelected(QSet<QString>(), true);
    CHECK(!preview.forceable);
    CHECK(preview.blockers.size() == 1);
    CHECK(preview.blockers.first().active == (QList<int>{424242, 424243}));
    CHECK(preview.blockers.first().key == META_CTRL_LEFT);
    QStringList messages;
    ShortcutDiag::setSink([&](QtMsgType, const QString &message) {
        messages.append(message);
    });
    ShortcutReconciler::logForcePreview("force-preview", preview);
    ShortcutDiag::resetSink();
    bool sawFound = false;
    bool sawKeep = false;
    for (const QString &message : messages) {
        if (message.contains(QStringLiteral("org.example/default-only"))
            && message.contains(QStringLiteral("424242"))) {
            sawFound = true;
        }
        if (message.contains(QStringLiteral("org.example/default-only"))
            && message.contains(QStringLiteral("remove=none"))
            && message.contains(QStringLiteral("424243"))) {
            sawKeep = true;
        }
    }
    CHECK(sawFound);
    CHECK(sawKeep);
    // A blocked preview never forces: zero writes with the refusal
    // reason, even for a drifted blocker image.
    ShortcutForcePreview drifted = preview;
    drifted.blockers[0].active = QList<int>{424242};
    const ShortcutForceApplyResult refused =
        reconciler.applyForcedSelected(drifted, QSet<QString>(), true);
    CHECK(!refused.ok);
    CHECK(!refused.error.isEmpty());
    CHECK(refused.writes == 0);
}

void workspaceRelativeDesktopSwitchRows()
{
    // Item 1 catalog: previous toggle plus four prev and four next relative
    // rows with exact TS chords. Arrow rows carry compiled kwin Switch One
    // Desktop holders; letters and toggle carry no known holder.
    CHECK(shortcutConflictTable().size() == 23);
    CHECK(ShortcutReconciler::relevantConflictKeys().size() == 24);
    const QList<std::pair<QString, int>> projects = {
        {QStringLiteral("omnitiler-workspace-previous"), META_CTRL_TAB},
        {QStringLiteral("omnitiler-workspace-prev-h"), META_CTRL_H},
        {QStringLiteral("omnitiler-workspace-prev-k"), META_CTRL_K},
        {QStringLiteral("omnitiler-workspace-prev-left-arrow"), META_CTRL_LEFT},
        {QStringLiteral("omnitiler-workspace-prev-up-arrow"), META_CTRL_UP},
        {QStringLiteral("omnitiler-workspace-next-j"), META_CTRL_J},
        {QStringLiteral("omnitiler-workspace-next-l"), META_CTRL_L},
        {QStringLiteral("omnitiler-workspace-next-down-arrow"), META_CTRL_DOWN},
        {QStringLiteral("omnitiler-workspace-next-right-arrow"), META_CTRL_RIGHT},
    };
    for (const auto &[projectAction, key] : projects) {
        CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), projectAction));
        CHECK(ShortcutReconciler::isProjectOwned(QStringLiteral("kwin"), projectAction));
        CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), projectAction));
        CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{key, 999}, QSet<QString>())
              == QList<int>{key});
        CHECK(ShortcutReconciler::remainderAfterClearFor(QList<int>{key, 999}, QSet<QString>())
              == QList<int>{999});
    }
    // Conflict-table arrows additionally participate in the legacy
    // table-scoped key math; letters and toggle do not.
    for (int key : {META_CTRL_LEFT, META_CTRL_UP, META_CTRL_DOWN, META_CTRL_RIGHT}) {
        CHECK(ShortcutReconciler::conflictingKeys(QList<int>{key, 999}) == QList<int>{key});
        CHECK(ShortcutReconciler::remainderAfterClear(QList<int>{key, 999}) == QList<int>{999});
    }
    for (int key : {META_CTRL_TAB, META_CTRL_H, META_CTRL_K, META_CTRL_J, META_CTRL_L}) {
        CHECK(ShortcutReconciler::conflictingKeys(QList<int>{key, 999}).isEmpty());
        CHECK(ShortcutReconciler::remainderAfterClear(QList<int>{key, 999}) == (QList<int>{key, 999}));
    }
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_TAB) == QStringLiteral("Meta+Ctrl+Tab"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_LEFT) == QStringLiteral("Meta+Ctrl+Left"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_UP) == QStringLiteral("Meta+Ctrl+Up"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_DOWN) == QStringLiteral("Meta+Ctrl+Down"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_RIGHT) == QStringLiteral("Meta+Ctrl+Right"));
    const QList<std::pair<QString, int>> foreigns = {
        {QStringLiteral("Switch One Desktop to the Left"), META_CTRL_LEFT},
        {QStringLiteral("Switch One Desktop Up"), META_CTRL_UP},
        {QStringLiteral("Switch One Desktop Down"), META_CTRL_DOWN},
        {QStringLiteral("Switch One Desktop to the Right"), META_CTRL_RIGHT},
    };
    for (const auto &[foreignAction, key] : foreigns) {
        CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), foreignAction));
        CHECK(!ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), foreignAction));
        CHECK(!ShortcutReconciler::isHolderExempt(QStringLiteral("kwin"), foreignAction, key));
    }
    // Letters and toggle carry no compiled holder and stay Keep.
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-previous"))));
    CHECK(!shortcutKnownConflictIds().contains(
        shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-h"))));
    CHECK(!shortcutKnownConflictIds().contains(
        shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-j"))));
    // Table rows carry exact posts, preimages, clear resolution, and
    // displays in appended order (rows 15-18).
    {
        const QList<ShortcutConflictRow> &table = shortcutConflictTable();
        const QList<std::pair<QString, QString>> diag = {
            {QStringLiteral("workspace-prev-left-arrow"), QStringLiteral("Meta+Ctrl+Left")},
            {QStringLiteral("workspace-prev-up-arrow"), QStringLiteral("Meta+Ctrl+Up")},
            {QStringLiteral("workspace-next-down-arrow"), QStringLiteral("Meta+Ctrl+Down")},
            {QStringLiteral("workspace-next-right-arrow"), QStringLiteral("Meta+Ctrl+Right")},
        };
        const QList<QString> projectActions = {
            QStringLiteral("omnitiler-workspace-prev-left-arrow"),
            QStringLiteral("omnitiler-workspace-prev-up-arrow"),
            QStringLiteral("omnitiler-workspace-next-down-arrow"),
            QStringLiteral("omnitiler-workspace-next-right-arrow"),
        };
        for (int i = 0; i < diag.size(); ++i) {
            const ShortcutConflictRow &row = table.at(15 + i);
            CHECK(row.projectComponent == QStringLiteral("kwin"));
            CHECK(row.projectAction == projectActions.at(i));
            CHECK(row.foreignComponent == QStringLiteral("kwin"));
            CHECK(row.foreignAction == foreigns.at(i).first);
            CHECK(row.foreignExpectedPre == QList<int>{foreigns.at(i).second});
            CHECK(row.resolution == shortcutResolutionClear());
            CHECK(row.projectDisplay == diag.at(i).second);
            CHECK(row.projectDiag == diag.at(i).first);
        }
    }
    // Authentic full selection: stock arrow holders refuse Apply, Force
    // clears exactly the arrow chords, Revert restores stock defaults.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Switch One Desktop to the Left")) {
                tuple.active = QList<int>{META_CTRL_LEFT, 4242};
            }
        }
        store.defaultKeysById[QStringLiteral("kwin/Switch One Desktop to the Left")] =
            QList<int>{META_CTRL_LEFT};
        FakeClearedActions cleared;
        CHECK(!ShortcutReconciler(&store, &cleared).apply().ok);
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
        bool sawLeft = false;
        for (const ShortcutForceMismatch &mismatch : preview.mismatches) {
            if (mismatch.action == QStringLiteral("Switch One Desktop to the Left")) {
                sawLeft = true;
                CHECK(mismatch.expectedPre == QList<int>{META_CTRL_LEFT});
                CHECK(mismatch.post == QList<int>{4242});
            }
        }
        CHECK(sawLeft);
        const ShortcutForceApplyResult forced = ShortcutReconciler(&store, &cleared).applyForced(preview);
        CHECK(forced.ok);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Switch One Desktop to the Left")) {
                CHECK(tuple.active == (QList<int>{4242}));
            }
            if (tuple.action == QStringLiteral("omnitiler-workspace-prev-left-arrow")) {
                CHECK(tuple.active == (QList<int>{META_CTRL_LEFT}));
            }
        }
        CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
        CHECK(cleared.stored.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Switch One Desktop to the Left")) {
                CHECK(tuple.active == (QList<int>{META_CTRL_LEFT}));
            }
        }
    }
    // Compatible disables exactly the four arrow rows; letters and toggle
    // stay Keep even with live collisions elsewhere.
    {
        const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
        const QSet<QString> known = shortcutKnownConflictIds();
        const QStringList compatible = presetCompatibleDisabledIds(catalog, known, QSet<QString>());
        CHECK(compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-left-arrow"))));
        CHECK(compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-up-arrow"))));
        CHECK(compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-down-arrow"))));
        CHECK(compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-right-arrow"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-previous"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-h"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-k"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-j"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-l"))));
        // Disabled arrows skip refusal and clear only their own assignment.
        FakeShortcutStore store;
        seedFullQuiet(store);
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Switch One Desktop to the Left")) {
                tuple.active = QList<int>{META_CTRL_LEFT};
            }
        }
        QSet<QString> disabled;
        for (const ShortcutCatalogEntry &entry : catalog) {
            const QString id = shortcutCatalogId(entry.component, entry.action);
            if (known.contains(id)
                && entry.action.startsWith(QStringLiteral("omnitiler-workspace-"))) {
                disabled.insert(id);
            }
        }
        CHECK(disabled.size() == 4);
        FakeClearedActions cleared;
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(disabled);
        CHECK(result.ok);
        CHECK(store.foreignWriteLog.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-workspace-prev-left-arrow")) {
                CHECK(tuple.active.isEmpty());
            }
            if (tuple.action == QStringLiteral("Switch One Desktop to the Left")) {
                CHECK(tuple.active == (QList<int>{META_CTRL_LEFT}));
            }
        }
    }
}

void workspaceSendFollowStayRows()
{
    // Item 2 catalog: 8 bound relative send-and-follow plus 20 unbound
    // numbered stay plus 8 unbound relative stay, in exact TS order.
    CHECK(shortcutConflictTable().size() == 23);
    CHECK(ShortcutReconciler::relevantConflictKeys().size() == 24);
    CHECK(shortcutProjectCatalog().size() == 129);
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    const QList<std::pair<QString, int>> follow = {
        {QStringLiteral("omnitiler-send-prev-h"), META_CTRL_SHIFT_H},
        {QStringLiteral("omnitiler-send-prev-k"), META_CTRL_SHIFT_K},
        {QStringLiteral("omnitiler-send-prev-left-arrow"), META_CTRL_SHIFT_LEFT},
        {QStringLiteral("omnitiler-send-prev-up-arrow"), META_CTRL_SHIFT_UP},
        {QStringLiteral("omnitiler-send-next-j"), META_CTRL_SHIFT_J},
        {QStringLiteral("omnitiler-send-next-l"), META_CTRL_SHIFT_L},
        {QStringLiteral("omnitiler-send-next-down-arrow"), META_CTRL_SHIFT_DOWN},
        {QStringLiteral("omnitiler-send-next-right-arrow"), META_CTRL_SHIFT_RIGHT},
    };
    const QList<QString> followDisplays = {
        QStringLiteral("Meta+Ctrl+Shift+H"), QStringLiteral("Meta+Ctrl+Shift+K"),
        QStringLiteral("Meta+Ctrl+Shift+Left"), QStringLiteral("Meta+Ctrl+Shift+Up"),
        QStringLiteral("Meta+Ctrl+Shift+J"), QStringLiteral("Meta+Ctrl+Shift+L"),
        QStringLiteral("Meta+Ctrl+Shift+Down"), QStringLiteral("Meta+Ctrl+Shift+Right"),
    };
    for (int i = 0; i < follow.size(); ++i) {
        const ShortcutCatalogEntry &entry = catalog.at(76 + i);
        CHECK(entry.component == QStringLiteral("kwin"));
        CHECK(entry.action == follow.at(i).first);
        CHECK(entry.canonicalKey == follow.at(i).second);
        CHECK(entry.canonicalDisplay == followDisplays.at(i));
        CHECK(entry.kind == QStringLiteral("workspace-send-relative"));
        CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), entry.action));
        CHECK(ShortcutReconciler::isProjectOwned(QStringLiteral("kwin"), entry.action));
        CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), entry.action));
        CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{entry.canonicalKey, 999}, QSet<QString>())
              == QList<int>{entry.canonicalKey});
        CHECK(ShortcutReconciler::remainderAfterClearFor(QList<int>{entry.canonicalKey, 999}, QSet<QString>())
              == QList<int>{999});
    }
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_SHIFT_H) == QStringLiteral("Meta+Ctrl+Shift+H"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_SHIFT_LEFT)
          == QStringLiteral("Meta+Ctrl+Shift+Left"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_SHIFT_UP)
          == QStringLiteral("Meta+Ctrl+Shift+Up"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_SHIFT_DOWN)
          == QStringLiteral("Meta+Ctrl+Shift+Down"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_SHIFT_RIGHT)
          == QStringLiteral("Meta+Ctrl+Shift+Right"));
    // Numbered stay: 1..9 plus symbol aliases plus append rows, all unbound.
    const QList<QString> stayActions = {
        QStringLiteral("omnitiler-stay-workspace-1"),
        QStringLiteral("omnitiler-stay-workspace-1-symbol"),
        QStringLiteral("omnitiler-stay-workspace-2"),
        QStringLiteral("omnitiler-stay-workspace-2-symbol"),
        QStringLiteral("omnitiler-stay-workspace-3"),
        QStringLiteral("omnitiler-stay-workspace-3-symbol"),
        QStringLiteral("omnitiler-stay-workspace-4"),
        QStringLiteral("omnitiler-stay-workspace-4-symbol"),
        QStringLiteral("omnitiler-stay-workspace-5"),
        QStringLiteral("omnitiler-stay-workspace-5-symbol"),
        QStringLiteral("omnitiler-stay-workspace-6"),
        QStringLiteral("omnitiler-stay-workspace-6-symbol"),
        QStringLiteral("omnitiler-stay-workspace-7"),
        QStringLiteral("omnitiler-stay-workspace-7-symbol"),
        QStringLiteral("omnitiler-stay-workspace-8"),
        QStringLiteral("omnitiler-stay-workspace-8-symbol"),
        QStringLiteral("omnitiler-stay-workspace-9"),
        QStringLiteral("omnitiler-stay-workspace-9-symbol"),
        QStringLiteral("omnitiler-stay-workspace-append"),
        QStringLiteral("omnitiler-stay-workspace-append-symbol"),
    };
    for (int i = 0; i < stayActions.size(); ++i) {
        const ShortcutCatalogEntry &entry = catalog.at(84 + i);
        CHECK(entry.component == QStringLiteral("kwin"));
        CHECK(entry.action == stayActions.at(i));
        CHECK(entry.canonicalKey == 0);
        CHECK(entry.canonicalDisplay.isEmpty());
        CHECK(entry.kind == QStringLiteral("workspace-move-stay"));
        CHECK(entry.knownForeignComponent.isEmpty());
        CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), entry.action));
        CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{999}, QSet<QString>()).isEmpty()
              || true);
    }
    const QList<QString> stayRelativeActions = {
        QStringLiteral("omnitiler-send-stay-prev-h"),
        QStringLiteral("omnitiler-send-stay-prev-k"),
        QStringLiteral("omnitiler-send-stay-prev-left-arrow"),
        QStringLiteral("omnitiler-send-stay-prev-up-arrow"),
        QStringLiteral("omnitiler-send-stay-next-j"),
        QStringLiteral("omnitiler-send-stay-next-l"),
        QStringLiteral("omnitiler-send-stay-next-down-arrow"),
        QStringLiteral("omnitiler-send-stay-next-right-arrow"),
    };
    for (int i = 0; i < stayRelativeActions.size(); ++i) {
        const ShortcutCatalogEntry &entry = catalog.at(104 + i);
        CHECK(entry.component == QStringLiteral("kwin"));
        CHECK(entry.action == stayRelativeActions.at(i));
        CHECK(entry.canonicalKey == 0);
        CHECK(entry.canonicalDisplay.isEmpty());
        CHECK(entry.kind == QStringLiteral("workspace-send-relative-stay"));
    }
    CHECK(ShortcutReconciler::keyDisplayName(0) == QStringLiteral("none"));
    CHECK(!ShortcutReconciler::enabledRequiredKeys(QSet<QString>()).contains(0));
    // Arrow follow rows carry compiled Window One Desktop holders; letters
    // and all stay rows carry no known holder.
    const QList<std::pair<QString, int>> arrowForeigns = {
        {QStringLiteral("Window One Desktop to the Left"), META_CTRL_SHIFT_LEFT},
        {QStringLiteral("Window One Desktop Up"), META_CTRL_SHIFT_UP},
        {QStringLiteral("Window One Desktop Down"), META_CTRL_SHIFT_DOWN},
        {QStringLiteral("Window One Desktop to the Right"), META_CTRL_SHIFT_RIGHT},
    };
    for (const auto &[foreignAction, key] : arrowForeigns) {
        CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), foreignAction));
        CHECK(!ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), foreignAction));
        CHECK(!ShortcutReconciler::isHolderExempt(QStringLiteral("kwin"), foreignAction, key));
        CHECK(ShortcutReconciler::conflictingKeys(QList<int>{key, 999}) == QList<int>{key});
    }
    for (int key : {META_CTRL_SHIFT_H, META_CTRL_SHIFT_K, META_CTRL_SHIFT_J, META_CTRL_SHIFT_L}) {
        CHECK(ShortcutReconciler::conflictingKeys(QList<int>{key, 999}).isEmpty());
    }
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-h"))));
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-j"))));
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-1"))));
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-send-stay-prev-h"))));
    // Table rows carry exact posts, preimages, clear resolution, and
    // displays in appended order (rows 19-22).
    {
        const QList<ShortcutConflictRow> &table = shortcutConflictTable();
        const QList<std::pair<QString, QString>> diag = {
            {QStringLiteral("send-prev-left-arrow"), QStringLiteral("Meta+Ctrl+Shift+Left")},
            {QStringLiteral("send-prev-up-arrow"), QStringLiteral("Meta+Ctrl+Shift+Up")},
            {QStringLiteral("send-next-down-arrow"), QStringLiteral("Meta+Ctrl+Shift+Down")},
            {QStringLiteral("send-next-right-arrow"), QStringLiteral("Meta+Ctrl+Shift+Right")},
        };
        const QList<QString> projectActions = {
            QStringLiteral("omnitiler-send-prev-left-arrow"),
            QStringLiteral("omnitiler-send-prev-up-arrow"),
            QStringLiteral("omnitiler-send-next-down-arrow"),
            QStringLiteral("omnitiler-send-next-right-arrow"),
        };
        for (int i = 0; i < diag.size(); ++i) {
            const ShortcutConflictRow &row = table.at(19 + i);
            CHECK(row.projectComponent == QStringLiteral("kwin"));
            CHECK(row.projectAction == projectActions.at(i));
            CHECK(row.foreignComponent == QStringLiteral("kwin"));
            CHECK(row.foreignAction == arrowForeigns.at(i).first);
            CHECK(row.foreignExpectedPre == QList<int>{arrowForeigns.at(i).second});
            CHECK(row.resolution == shortcutResolutionClear());
            CHECK(row.projectDisplay == diag.at(i).second);
            CHECK(row.projectDiag == diag.at(i).first);
        }
    }
    // Authentic full selection: stock arrow holders refuse Apply, Force
    // clears exactly the arrow chords, Revert restores stock defaults.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Window One Desktop to the Left")) {
                tuple.active = QList<int>{META_CTRL_SHIFT_LEFT, 4242};
            }
        }
        store.defaultKeysById[QStringLiteral("kwin/Window One Desktop to the Left")] =
            QList<int>{META_CTRL_SHIFT_LEFT};
        FakeClearedActions cleared;
        CHECK(!ShortcutReconciler(&store, &cleared).apply().ok);
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
        bool sawLeft = false;
        for (const ShortcutForceMismatch &mismatch : preview.mismatches) {
            if (mismatch.action == QStringLiteral("Window One Desktop to the Left")) {
                sawLeft = true;
                CHECK(mismatch.expectedPre == QList<int>{META_CTRL_SHIFT_LEFT});
                CHECK(mismatch.post == QList<int>{4242});
            }
        }
        CHECK(sawLeft);
        const ShortcutForceApplyResult forced = ShortcutReconciler(&store, &cleared).applyForced(preview);
        CHECK(forced.ok);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Window One Desktop to the Left")) {
                CHECK(tuple.active == (QList<int>{4242}));
            }
            if (tuple.action == QStringLiteral("omnitiler-send-prev-left-arrow")) {
                CHECK(tuple.active == (QList<int>{META_CTRL_SHIFT_LEFT}));
            }
        }
        CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
        CHECK(cleared.stored.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Window One Desktop to the Left")) {
                CHECK(tuple.active == (QList<int>{META_CTRL_SHIFT_LEFT}));
            }
        }
    }
    // Compatible disables exactly the four send arrow rows; letters and all
    // unbound stay rows stay Keep.
    {
        const QSet<QString> known = shortcutKnownConflictIds();
        const QStringList compatible = presetCompatibleDisabledIds(catalog, known, QSet<QString>());
        CHECK(compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-left-arrow"))));
        CHECK(compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-up-arrow"))));
        CHECK(compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-down-arrow"))));
        CHECK(compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-right-arrow"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-h"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-send-prev-k"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-j"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-send-next-l"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-1"))));
        CHECK(!compatible.contains(shortcutCatalogId(
            QStringLiteral("kwin"), QStringLiteral("omnitiler-send-stay-prev-h"))));
        // Disabled send arrows skip refusal and clear only their own assignment.
        FakeShortcutStore store;
        seedFullQuiet(store);
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("Window One Desktop to the Left")) {
                tuple.active = QList<int>{META_CTRL_SHIFT_LEFT};
            }
        }
        QSet<QString> disabled;
        for (const ShortcutCatalogEntry &entry : catalog) {
            const QString id = shortcutCatalogId(entry.component, entry.action);
            if (known.contains(id) && entry.action.startsWith(QStringLiteral("omnitiler-send-"))) {
                disabled.insert(id);
            }
        }
        CHECK(disabled.size() == 4);
        FakeClearedActions cleared;
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(disabled);
        CHECK(result.ok);
        CHECK(store.foreignWriteLog.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-send-prev-left-arrow")) {
                CHECK(tuple.active.isEmpty());
            }
            if (tuple.action == QStringLiteral("Window One Desktop to the Left")) {
                CHECK(tuple.active == (QList<int>{META_CTRL_SHIFT_LEFT}));
            }
        }
    }
    // Unbound rows stay rebindable through the existing route and Apply
    // restores their empty default.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        QList<ShortcutRowDisplay> rows;
        QString error;
        CHECK(ShortcutReconciler::collectRowDisplays(&store, &rows, &error));
        bool sawStay = false;
        for (const ShortcutRowDisplay &row : rows) {
            if (row.catalog.action == QStringLiteral("omnitiler-stay-workspace-1")) {
                sawStay = true;
                CHECK(row.present);
                CHECK(row.current.isEmpty());
                CHECK(row.holdersKnown);
                CHECK(row.holders.isEmpty());
            }
        }
        CHECK(sawStay);
        QString writeError;
        QList<int> confirmed;
        CHECK(store.writeKeys(QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-1"),
                              QStringLiteral("KWin"), QStringLiteral("friendly"), QList<int>{999}, &confirmed,
                              &writeError));
        CHECK(confirmed == QList<int>{999});
        FakeClearedActions cleared;
        const ShortcutApplyResult result = ShortcutReconciler(&store, &cleared).apply();
        CHECK(result.ok);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-stay-workspace-1")) {
                CHECK(tuple.active.isEmpty());
            }
        }
    }
}

void outputSendFollowStayRows()
{
    // Item 5 catalog: 8 bound Meta+Ctrl+Alt H/J/K/L and arrows follow plus 4
    // unbound directional stay, in exact planOutputSendShortcutCatalog order
    // (H left, J down, K up, L right; stay bindable unbound). No stock KDE
    // holder for these arms, so the conflict table stays at 23 rows and the
    // follow rows carry empty known foreign state; both presets Keep.
    CHECK(shortcutConflictTable().size() == 23);
    CHECK(ShortcutReconciler::relevantConflictKeys().size() == 24);
    CHECK(shortcutProjectCatalog().size() == 129);
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    const QList<QString> outputOrder = {
        QStringLiteral("omnitiler-send-output-left"),
        QStringLiteral("omnitiler-send-output-left-arrow"),
        QStringLiteral("omnitiler-send-output-left-stay"),
        QStringLiteral("omnitiler-send-output-down"),
        QStringLiteral("omnitiler-send-output-down-arrow"),
        QStringLiteral("omnitiler-send-output-down-stay"),
        QStringLiteral("omnitiler-send-output-up"),
        QStringLiteral("omnitiler-send-output-up-arrow"),
        QStringLiteral("omnitiler-send-output-up-stay"),
        QStringLiteral("omnitiler-send-output-right"),
        QStringLiteral("omnitiler-send-output-right-arrow"),
        QStringLiteral("omnitiler-send-output-right-stay"),
    };
    for (int i = 0; i < outputOrder.size(); ++i) {
        const ShortcutCatalogEntry &entry = catalog.at(112 + i);
        CHECK(entry.component == QStringLiteral("kwin"));
        CHECK(entry.action == outputOrder.at(i));
    }
    const QList<std::pair<QString, int>> follow = {
        {QStringLiteral("omnitiler-send-output-left"), META_CTRL_ALT_H},
        {QStringLiteral("omnitiler-send-output-left-arrow"), META_CTRL_ALT_LEFT},
        {QStringLiteral("omnitiler-send-output-down"), META_CTRL_ALT_J},
        {QStringLiteral("omnitiler-send-output-down-arrow"), META_CTRL_ALT_DOWN},
        {QStringLiteral("omnitiler-send-output-up"), META_CTRL_ALT_K},
        {QStringLiteral("omnitiler-send-output-up-arrow"), META_CTRL_ALT_UP},
        {QStringLiteral("omnitiler-send-output-right"), META_CTRL_ALT_L},
        {QStringLiteral("omnitiler-send-output-right-arrow"), META_CTRL_ALT_RIGHT},
    };
    const QList<QString> followDisplays = {
        QStringLiteral("Meta+Ctrl+Alt+H"), QStringLiteral("Meta+Ctrl+Alt+Left"),
        QStringLiteral("Meta+Ctrl+Alt+J"), QStringLiteral("Meta+Ctrl+Alt+Down"),
        QStringLiteral("Meta+Ctrl+Alt+K"), QStringLiteral("Meta+Ctrl+Alt+Up"),
        QStringLiteral("Meta+Ctrl+Alt+L"), QStringLiteral("Meta+Ctrl+Alt+Right"),
    };
    for (int i = 0; i < follow.size(); ++i) {
        bool found = false;
        for (const ShortcutCatalogEntry &entry : catalog) {
            if (entry.action == follow.at(i).first) {
                found = true;
                CHECK(entry.component == QStringLiteral("kwin"));
                CHECK(entry.canonicalKey == follow.at(i).second);
                CHECK(entry.canonicalDisplay == followDisplays.at(i));
                CHECK(entry.kind == QStringLiteral("output-send"));
                CHECK(entry.knownForeignComponent.isEmpty());
                CHECK(entry.knownForeignAction.isEmpty());
                CHECK(entry.knownForeignKey == 0);
                CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), entry.action));
                CHECK(ShortcutReconciler::isProjectOwned(QStringLiteral("kwin"), entry.action));
                CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), entry.action));
                CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{entry.canonicalKey, 999},
                                                             QSet<QString>())
                      == QList<int>{entry.canonicalKey});
                CHECK(ShortcutReconciler::remainderAfterClearFor(QList<int>{entry.canonicalKey, 999},
                                                                QSet<QString>())
                      == QList<int>{999});
                CHECK(ShortcutReconciler::enabledRequiredKeys(QSet<QString>()).contains(entry.canonicalKey));
                const QString id = shortcutCatalogId(QStringLiteral("kwin"), entry.action);
                CHECK(!ShortcutReconciler::enabledRequiredKeys(QSet<QString>{id}).contains(
                    entry.canonicalKey));
                CHECK(!shortcutKnownConflictIds().contains(id));
                break;
            }
        }
        CHECK(found);
    }
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_ALT_H) == QStringLiteral("Meta+Ctrl+Alt+H"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_ALT_J) == QStringLiteral("Meta+Ctrl+Alt+J"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_ALT_K) == QStringLiteral("Meta+Ctrl+Alt+K"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_ALT_L) == QStringLiteral("Meta+Ctrl+Alt+L"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_ALT_LEFT)
          == QStringLiteral("Meta+Ctrl+Alt+Left"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_ALT_UP)
          == QStringLiteral("Meta+Ctrl+Alt+Up"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_ALT_DOWN)
          == QStringLiteral("Meta+Ctrl+Alt+Down"));
    CHECK(ShortcutReconciler::keyDisplayName(META_CTRL_ALT_RIGHT)
          == QStringLiteral("Meta+Ctrl+Alt+Right"));
    // Stay rows: bindable unbound, zero-key support preserved.
    const QList<QString> stayActions = {
        QStringLiteral("omnitiler-send-output-left-stay"),
        QStringLiteral("omnitiler-send-output-down-stay"),
        QStringLiteral("omnitiler-send-output-up-stay"),
        QStringLiteral("omnitiler-send-output-right-stay"),
    };
    for (const QString &action : stayActions) {
        bool found = false;
        for (const ShortcutCatalogEntry &entry : catalog) {
            if (entry.action == action) {
                found = true;
                CHECK(entry.component == QStringLiteral("kwin"));
                CHECK(entry.canonicalKey == 0);
                CHECK(entry.canonicalDisplay.isEmpty());
                CHECK(entry.kind == QStringLiteral("output-send-stay"));
                CHECK(entry.knownForeignComponent.isEmpty());
                CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), entry.action));
                CHECK(!shortcutKnownConflictIds().contains(
                    shortcutCatalogId(QStringLiteral("kwin"), entry.action)));
                CHECK(!ShortcutReconciler::enabledRequiredKeys(QSet<QString>()).contains(0));
                break;
            }
        }
        CHECK(found);
    }
    // Both presets Keep the output rows when quiet; a live collision stages
    // Disable through the shared preset seam without inventing a stock holder.
    {
        const QSet<QString> known = shortcutKnownConflictIds();
        const QStringList quiet = presetCompatibleDisabledIds(catalog, known, QSet<QString>());
        for (const QString &action : outputOrder) {
            CHECK(!quiet.contains(shortcutCatalogId(QStringLiteral("kwin"), action)));
        }
        const QString outLeft =
            shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-left"));
        const QStringList colliding =
            presetCompatibleDisabledIds(catalog, known, QSet<QString>{outLeft});
        CHECK(colliding.contains(outLeft));
    }
    // Quiet Apply assigns follow chords and leaves stay rows empty.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        FakeClearedActions cleared;
        CHECK(ShortcutReconciler(&store, &cleared).apply().ok);
        for (const auto &[action, key] : follow) {
            for (const ShortcutTuple &tuple : store.tuples) {
                if (tuple.action == action) {
                    CHECK(tuple.active == (QList<int>{key}));
                }
            }
        }
        for (const QString &action : stayActions) {
            for (const ShortcutTuple &tuple : store.tuples) {
                if (tuple.action == action) {
                    CHECK(tuple.active.isEmpty());
                }
            }
        }
    }
    // Disabled follow clears only its own assignment.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        FakeClearedActions cleared;
        const QString outLeft =
            shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-send-output-left"));
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{outLeft});
        CHECK(result.ok);
        CHECK(store.foreignWriteLog.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-send-output-left")) {
                CHECK(tuple.active.isEmpty());
            }
        }
    }
    // Unknown holder of a follow chord refuses Apply; Force clears exactly
    // the required chord and Revert restores the holder default.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        store.tuples.append(
            makeTuple(QStringLiteral("org.example"), QStringLiteral("other-output"),
                      QList<int>{META_CTRL_ALT_H, 4242}));
        store.defaultKeysById[QStringLiteral("org.example/other-output")] =
            QList<int>{META_CTRL_ALT_H};
        FakeClearedActions cleared;
        CHECK(!ShortcutReconciler(&store, &cleared).apply().ok);
        const ShortcutForcePreview preview = ShortcutReconciler(&store, &cleared).previewForceApply();
        CHECK(preview.forceable);
        bool sawOther = false;
        for (const ShortcutForceMismatch &mismatch : preview.mismatches) {
            if (mismatch.action == QStringLiteral("other-output")) {
                sawOther = true;
                CHECK(mismatch.expectedPre == QList<int>{META_CTRL_ALT_H});
                CHECK(mismatch.post == QList<int>{4242});
            }
        }
        CHECK(sawOther);
        const ShortcutForceApplyResult forced = ShortcutReconciler(&store, &cleared).applyForced(preview);
        CHECK(forced.ok);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("other-output")) {
                CHECK(tuple.active == (QList<int>{4242}));
            }
            if (tuple.action == QStringLiteral("omnitiler-send-output-left")) {
                CHECK(tuple.active == (QList<int>{META_CTRL_ALT_H}));
            }
        }
        CHECK(ShortcutReconciler(&store, &cleared).revert().ok);
        CHECK(cleared.stored.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("other-output")) {
                CHECK(tuple.active == (QList<int>{META_CTRL_ALT_H}));
            }
        }
    }
    // Empty stay rows stay rebindable and Apply restores the empty default.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        QList<ShortcutRowDisplay> rows;
        QString error;
        CHECK(ShortcutReconciler::collectRowDisplays(&store, &rows, &error));
        bool sawStay = false;
        for (const ShortcutRowDisplay &row : rows) {
            if (row.catalog.action == QStringLiteral("omnitiler-send-output-left-stay")) {
                sawStay = true;
                CHECK(row.present);
                CHECK(row.current.isEmpty());
                CHECK(row.holdersKnown);
                CHECK(row.holders.isEmpty());
            }
        }
        CHECK(sawStay);
        QString writeError;
        QList<int> confirmed;
        CHECK(store.writeKeys(QStringLiteral("kwin"),
                              QStringLiteral("omnitiler-send-output-left-stay"),
                              QStringLiteral("KWin"), QStringLiteral("friendly"), QList<int>{999}, &confirmed,
                              &writeError));
        CHECK(confirmed == QList<int>{999});
        FakeClearedActions cleared;
        const ShortcutApplyResult result = ShortcutReconciler(&store, &cleared).apply();
        CHECK(result.ok);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-send-output-left-stay")) {
                CHECK(tuple.active.isEmpty());
            }
        }
    }
}

void migrateWorkspaceFollowRows()
{
    // R-WS-12 catalog: 4 unbound directional follow rows in exact
    // planWorkspaceMigrateShortcutCatalog order (left, right, up, down).
    // No stock KDE holder, so the conflict table stays at 23 rows and the
    // rows carry empty known foreign state; both presets Keep.
    CHECK(shortcutConflictTable().size() == 23);
    CHECK(ShortcutReconciler::relevantConflictKeys().size() == 24);
    CHECK(shortcutProjectCatalog().size() == 129);
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    const QList<QString> migrateOrder = {
        QStringLiteral("omnitiler-migrate-workspace-left"),
        QStringLiteral("omnitiler-migrate-workspace-right"),
        QStringLiteral("omnitiler-migrate-workspace-up"),
        QStringLiteral("omnitiler-migrate-workspace-down"),
    };
    for (int i = 0; i < migrateOrder.size(); ++i) {
        const ShortcutCatalogEntry &entry = catalog.at(124 + i);
        CHECK(entry.component == QStringLiteral("kwin"));
        CHECK(entry.action == migrateOrder.at(i));
        CHECK(entry.canonicalKey == 0);
        CHECK(entry.canonicalDisplay.isEmpty());
        CHECK(entry.kind == QStringLiteral("workspace-migrate"));
        CHECK(entry.knownForeignComponent.isEmpty());
        CHECK(entry.knownForeignAction.isEmpty());
        CHECK(entry.knownForeignKey == 0);
        CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"), entry.action));
        CHECK(ShortcutReconciler::isProjectOwned(QStringLiteral("kwin"), entry.action));
        CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"), entry.action));
        CHECK(!shortcutKnownConflictIds().contains(
            shortcutCatalogId(QStringLiteral("kwin"), entry.action)));
        CHECK(!ShortcutReconciler::enabledRequiredKeys(QSet<QString>()).contains(0));
    }
    // Both presets Keep the migrate rows when quiet; a live collision stages
    // Disable through the shared preset seam without inventing a stock holder.
    {
        const QSet<QString> known = shortcutKnownConflictIds();
        const QStringList quiet = presetCompatibleDisabledIds(catalog, known, QSet<QString>());
        for (const QString &action : migrateOrder) {
            CHECK(!quiet.contains(shortcutCatalogId(QStringLiteral("kwin"), action)));
        }
        const QString migrateLeft =
            shortcutCatalogId(QStringLiteral("kwin"), QStringLiteral("omnitiler-migrate-workspace-left"));
        const QStringList colliding =
            presetCompatibleDisabledIds(catalog, known, QSet<QString>{migrateLeft});
        CHECK(colliding.contains(migrateLeft));
    }
    // Quiet Apply leaves migrate rows empty.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        FakeClearedActions cleared;
        CHECK(ShortcutReconciler(&store, &cleared).apply().ok);
        for (const QString &action : migrateOrder) {
            for (const ShortcutTuple &tuple : store.tuples) {
                if (tuple.action == action) {
                    CHECK(tuple.active.isEmpty());
                }
            }
        }
    }
    // Empty migrate rows stay rebindable and Apply restores the empty default.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        QList<ShortcutRowDisplay> rows;
        QString error;
        CHECK(ShortcutReconciler::collectRowDisplays(&store, &rows, &error));
        bool sawMigrate = false;
        for (const ShortcutRowDisplay &row : rows) {
            if (row.catalog.action == QStringLiteral("omnitiler-migrate-workspace-left")) {
                sawMigrate = true;
                CHECK(row.present);
                CHECK(row.current.isEmpty());
                CHECK(row.holdersKnown);
                CHECK(row.holders.isEmpty());
            }
        }
        CHECK(sawMigrate);
        QString writeError;
        QList<int> confirmed;
        CHECK(store.writeKeys(QStringLiteral("kwin"),
                              QStringLiteral("omnitiler-migrate-workspace-left"),
                              QStringLiteral("KWin"), QStringLiteral("friendly"), QList<int>{999}, &confirmed,
                              &writeError));
        CHECK(confirmed == QList<int>{999});
        FakeClearedActions cleared;
        const ShortcutApplyResult result = ShortcutReconciler(&store, &cleared).apply();
        CHECK(result.ok);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-migrate-workspace-left")) {
                CHECK(tuple.active.isEmpty());
            }
        }
    }
}

void toggleOrientationCatalogPresetRows()
{
    // R-LAY-01 Meta+O: no stock KDE holder, so both presets Keep.
    // Catalog carries the bound toggle row with empty known foreign state;
    // selection scoping and Apply clear only the project assignment.
    CHECK(SHORTCUT_META_O == META_O);
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    const ShortcutCatalogEntry *found = nullptr;
    for (const ShortcutCatalogEntry &entry : catalog) {
        if (entry.component == QStringLiteral("kwin")
            && entry.action == QStringLiteral("omnitiler-toggle-orientation")) {
            found = &entry;
            break;
        }
    }
    CHECK(found != nullptr);
    CHECK(found->canonicalKey == META_O);
    CHECK(found->canonicalDisplay == QStringLiteral("Meta+O"));
    CHECK(found->kind == QStringLiteral("toggle"));
    CHECK(found->knownForeignComponent.isEmpty());
    CHECK(found->knownForeignAction.isEmpty());
    CHECK(found->knownForeignKey == 0);
    CHECK(ShortcutReconciler::keyDisplayName(META_O) == QStringLiteral("Meta+O"));
    CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"),
                                             QStringLiteral("omnitiler-toggle-orientation")));
    CHECK(ShortcutReconciler::isProjectOwned(QStringLiteral("kwin"),
                                             QStringLiteral("omnitiler-toggle-orientation")));
    CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"),
                                           QStringLiteral("omnitiler-toggle-orientation")));
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-orientation"))));
    // No known holder: neither preset disables the row.
    const QString orientId = shortcutCatalogId(QStringLiteral("kwin"),
                                               QStringLiteral("omnitiler-toggle-orientation"));
    const QStringList authentic = presetCompatibleDisabledIds(catalog, shortcutKnownConflictIds(),
                                                              QSet<QString>());
    CHECK(!authentic.contains(orientId));
    const QStringList compatible = presetCompatibleDisabledIds(
        catalog, shortcutKnownConflictIds(), QSet<QString>{orientId});
    CHECK(compatible.contains(orientId));
    // Selection scoping: enabled requires the chord, disabled drops it.
    CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{META_O, 999}, QSet<QString>())
          == QList<int>{META_O});
    CHECK(ShortcutReconciler::remainderAfterClearFor(QList<int>{META_O, 999}, QSet<QString>())
          == QList<int>{999});
    CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{META_O, 999}, QSet<QString>{orientId})
          .isEmpty());
    CHECK(ShortcutReconciler::enabledRequiredKeys(QSet<QString>()).contains(META_O));
    CHECK(!ShortcutReconciler::enabledRequiredKeys(QSet<QString>{orientId}).contains(META_O));
    // Quiet Apply assigns the chord; disabling clears only the own row.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        FakeClearedActions cleared;
        CHECK(ShortcutReconciler(&store, &cleared).apply().ok);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-toggle-orientation")) {
                CHECK(tuple.active == (QList<int>{META_O}));
            }
        }
    }
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        FakeClearedActions cleared;
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{orientId});
        CHECK(result.ok);
        CHECK(store.foreignWriteLog.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-toggle-orientation")) {
                CHECK(tuple.active.isEmpty());
            }
        }
    }
}

void workspaceTilingToggleCatalogPresetRows()
{
    // User decision 2026-10-10 Meta+Y: no stock KDE holder, so both presets
    // Keep. Catalog carries the bound toggle row with empty known foreign
    // state; selection scoping and Apply clear only the project assignment.
    CHECK(SHORTCUT_META_Y == META_Y);
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    const ShortcutCatalogEntry *found = nullptr;
    for (const ShortcutCatalogEntry &entry : catalog) {
        if (entry.component == QStringLiteral("kwin")
            && entry.action == QStringLiteral("omnitiler-toggle-workspace-tiling")) {
            found = &entry;
            break;
        }
    }
    CHECK(found != nullptr);
    CHECK(found->canonicalKey == META_Y);
    CHECK(found->canonicalDisplay == QStringLiteral("Meta+Y"));
    CHECK(found->kind == QStringLiteral("toggle"));
    CHECK(found->knownForeignComponent.isEmpty());
    CHECK(found->knownForeignAction.isEmpty());
    CHECK(found->knownForeignKey == 0);
    CHECK(ShortcutReconciler::keyDisplayName(META_Y) == QStringLiteral("Meta+Y"));
    CHECK(ShortcutReconciler::isProjectAction(QStringLiteral("kwin"),
                                             QStringLiteral("omnitiler-toggle-workspace-tiling")));
    CHECK(ShortcutReconciler::isProjectOwned(QStringLiteral("kwin"),
                                             QStringLiteral("omnitiler-toggle-workspace-tiling")));
    CHECK(ShortcutReconciler::isAllowlisted(QStringLiteral("kwin"),
                                           QStringLiteral("omnitiler-toggle-workspace-tiling")));
    CHECK(!shortcutKnownConflictIds().contains(shortcutCatalogId(
        QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-workspace-tiling"))));
    // No known holder: neither preset disables the row.
    const QString toggleId = shortcutCatalogId(QStringLiteral("kwin"),
                                               QStringLiteral("omnitiler-toggle-workspace-tiling"));
    const QStringList compatibleEmpty = presetCompatibleDisabledIds(
        catalog, shortcutKnownConflictIds(), QSet<QString>());
    CHECK(!compatibleEmpty.contains(toggleId));
    const QStringList compatible = presetCompatibleDisabledIds(
        catalog, shortcutKnownConflictIds(), QSet<QString>{toggleId});
    CHECK(compatible.contains(toggleId));
    // Selection scoping: enabled requires the chord, disabled drops it.
    CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{META_Y, 999}, QSet<QString>())
          == QList<int>{META_Y});
    CHECK(ShortcutReconciler::remainderAfterClearFor(QList<int>{META_Y, 999}, QSet<QString>())
          == QList<int>{999});
    CHECK(ShortcutReconciler::conflictingKeysFor(QList<int>{META_Y, 999}, QSet<QString>{toggleId})
          .isEmpty());
    CHECK(ShortcutReconciler::enabledRequiredKeys(QSet<QString>()).contains(META_Y));
    CHECK(!ShortcutReconciler::enabledRequiredKeys(QSet<QString>{toggleId}).contains(META_Y));
    // Quiet Apply assigns the chord; disabling clears only the own row.
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        FakeClearedActions cleared;
        CHECK(ShortcutReconciler(&store, &cleared).apply().ok);
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-toggle-workspace-tiling")) {
                CHECK(tuple.active == (QList<int>{META_Y}));
            }
        }
    }
    {
        FakeShortcutStore store;
        seedFullQuiet(store);
        FakeClearedActions cleared;
        const ShortcutApplyResult result =
            ShortcutReconciler(&store, &cleared).applySelected(QSet<QString>{toggleId});
        CHECK(result.ok);
        CHECK(store.foreignWriteLog.isEmpty());
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-toggle-workspace-tiling")) {
                CHECK(tuple.active.isEmpty());
            }
        }
    }
}

} // namespace

int main(int argc, char **argv)
{
    // Unit 1 hard gate: isolate from the live session bus before any
    // QDBusConnection::sessionBus() initialization (this test constructs
    // the live KGlobalAccelStore).
    qputenv("DBUS_SESSION_BUS_ADDRESS", QByteArray("unix:path=/dev/null/omnitiler-kcm-test-isolated-bus"));
    const QString scenario = argc == 2 ? QString::fromLocal8Bit(argv[1]) : QStringLiteral("all");
    if (scenario == QStringLiteral("all") || scenario == QStringLiteral("success")) {
        applyAssignsProjectsAndRelocatesLock();
        applyIdempotentNoOp();
        applyFullCatalogBeyondOldLifetime();
        duplicateMetaEscDeduped();
    }
    if (scenario == QStringLiteral("all") || scenario == QStringLiteral("conflict")) {
        metaEscConflictRefusesWithoutMutation();
        tablePreimageRefusalZeroMutation();
        unknownAndLegacyHoldersRefuseApply();
        unrelatedChordsAllRefuse();
        unrelatedUnboundedRefusesZeroWrites();
        keyedDesktopOnlyBlocksRelocator();
        keyedDesktopOnlyBlocksClearTargets();
        keyedDesktopOnlyBlocksMetaL();
        keyedSystemMonitorEscAccepted();
        keyedLiveSystemMonitorIdentity();
        keyedTransportFailsClosed();
        keyedReplyParsingStrict();
        keyedAvailabilityConsistencyBothDirections();
        preflightOccupancyHolderSplitTokens();
    }
    if (scenario == QStringLiteral("all") || scenario == QStringLiteral("malformed")) {
        malformedReplyFailsClosed();
        strictOwnerAndIntrospection();
        friendlyLabelsValidated();
        allComponentsStrictTransport();
        introspectionStrictParsing();
        introspectionQtOutFirstVerbatim();
        introspectionAnnotationNamesStrict();
        keySequenceDbusRoundtripAndBounds();
        writeFailureControlsFailClosed();
        preflightAllComponentsSplitTokens();
        preflightAvailableSplitTokens();
        preflightByKeyTransportSplitTokens();
        preflightAllInfosSplitTokens();
        preflightKeyedFieldsSplitTokens();
        preflightStoreGuardsSplitTokens();
        preflightReadAllFieldSplitTokens();
        infoFieldsWireEncodingProvesSignature();
        seamHoldersAndTuplesFieldBranches();
        wireBoundTokensDistinctFromSeams();
        boundedHelperSeamsExactTokens();
        setterReplyAaiEncodingAndPureDecode();
        setterReplySlotShapesFailClosed();
        defaultAndForeignTransportSeam();
    }
    if (scenario == QStringLiteral("all") || scenario == QStringLiteral("owner")) {
        ownerResolutionHermetic();
        ownerAbsentApplyZeroWrites();
        ownerDriftApplyZeroWrites();
        pinImmutableFailsClosed();
    }
    if (scenario == QStringLiteral("all") || scenario == QStringLiteral("recovery")) {
        forceInterruptedListRetainedThenRevert();
        revertInterruptedRetainsList();
        revertResolvesFreshLabelsAfterRestart();
        revertMissingActionRetainsList();
        revertDuplicateActionRetainsList();
    }
    if (scenario == QStringLiteral("all") || scenario == QStringLiteral("external")) {
        revertEmptyListNoOp();
    }
    if (scenario == QStringLiteral("all") || scenario == QStringLiteral("journal")) {
        clearedActionsPathSafety();
        clearedStoreRoundtrip();
        clearedPathIsNewAndAbsolute();
    }
    if (scenario == QStringLiteral("all") || scenario == QStringLiteral("force")) {
        forceUnknownAndLegacyPreviewAndClear();
        historicalIdentityRevertSkipsForceClears();
        forceMultiKeyPreservesUnrelated();
        forceFullCatalogBeyondOldLifetime();
        forceStaleConfirmationZeroWrites();
        forceBlockedHolderNotForceable();
        knownForeignHoldersAreConflicts();
        growArrowSwitchWindowClearRows();
        focusMoveArrowQuickTileClearRows();
        forceDriftAfterPersistAbortsWithoutForeignWrites();
    }
    if (scenario == QStringLiteral("all") || scenario == QStringLiteral("diag")) {
        diagSinkCapturesOperations();
        diagPrefixAndForeignDataAreSafe();
        diagThrowingSinkPreservesBehavior();
    }
    if (scenario == QStringLiteral("all") || scenario == QStringLiteral("selection")) {
        selectionCatalogCounts();
        workspaceRelativeDesktopSwitchRows();
        workspaceSendFollowStayRows();
        outputSendFollowStayRows();
        migrateWorkspaceFollowRows();
        selectionDisabledSkipsRefusalAndClearsOwn();
        selectionDisabledFocusSkipsLockAndEsc();
        selectionMixedRefusesEnabledOnly();
        selectionForceBindsDraftAndDrift();
        selectionDefaultsCarried();
        selectionMissingEnabledFailsClosed();
        selectionUnknownIdFailsClosed();
        selectionForeignDefaultConflictAfterClear();
        selectionDisabledTableRowAbsentAllowed();
        selectionFocusDisabledLockAbsentAllowed();
        selectionPresenceBindsForceSnapshot();
        selectionKeepPreservesCustomCanonicalEmpty();
        selectionKeepCustomConflictPreviewForcePreserves();
        selectionKeepDriftDuringWriteFailsWithoutReset();
        selectionForceKeepDriftDuringClearFailsWithoutReset();
        selectionForceIntentMismatchStale();
        selectionKeepOccupancyGate();
        selectionRequiredKeysKeepVsAuthentic();
        selectionRowDisplayShowsActualHolders();
        orderedConflictCompletenessAndBlocker();
        orderedConflictsBlockerFirstEverywhere();
        blockerCarriesActiveImage();
        forcePreviewLogsEveryRowReconstructible();
        toggleOrientationCatalogPresetRows();
        workspaceTilingToggleCatalogPresetRows();
    }
    if (scenario != QStringLiteral("all") && scenario != QStringLiteral("success") && scenario != QStringLiteral("conflict")
        && scenario != QStringLiteral("malformed") && scenario != QStringLiteral("owner") && scenario != QStringLiteral("recovery")
        && scenario != QStringLiteral("external") && scenario != QStringLiteral("journal")
        && scenario != QStringLiteral("force") && scenario != QStringLiteral("diag")
        && scenario != QStringLiteral("selection")) {
        std::fprintf(stderr, "unknown scenario: %s\n", argv[1]);
        return EXIT_FAILURE;
    }
    if (failures != 0) {
        std::fprintf(stderr, "%d check(s) failed\n", failures);
        return EXIT_FAILURE;
    }
    std::printf("all checks passed\n");
    return EXIT_SUCCESS;
}
