#include "activeborderconfig_module.h"
#include "shortcutreconciler.h"

#include <KPluginMetaData>

#include <QApplication>
#include <QClipboard>
#include <QDialog>
#include <QDialogButtonBox>
#include <QDir>
#include <QEventLoop>
#include <QFile>
#include <QLabel>
#include <QListWidget>
#include <QListWidgetItem>
#include <QMimeData>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QTemporaryDir>
#include <QThread>
#include <QTimer>

#include <cstdio>
#include <cstdlib>
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
constexpr int META_LEFT = 285212690;
constexpr int META_DOWN = 285212693;
constexpr int META_UP = 285212691;
constexpr int META_RIGHT = 285212692;
constexpr int META_SHIFT_LEFT = 318767122;
constexpr int META_SHIFT_RIGHT = 318767124;
constexpr int META_CTRL_LEFT = 352321554;
constexpr int META_CTRL_UP = 352321555;
constexpr int META_CTRL_DOWN = 352321557;
constexpr int META_CTRL_RIGHT = 352321556;

ShortcutTuple makeTuple(const QString &component, const QString &action, const QList<int> &active)
{
    ShortcutTuple tuple;
    tuple.component = component;
    tuple.action = action;
    tuple.componentFriendly = component == QStringLiteral("kwin") ? QStringLiteral("KWin") : component;
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
    QMap<int, QList<ShortcutKeyHolder>> extraByKey;
    QMap<int, bool> availableOverride;
    bool failByKey = false;
    bool failAvailable = false;
    // Fail the Nth project write (0-based) to simulate a partial apply.
    int failWriteAt = -1;
    // Artificial latency for the guarded responsive-execution test: slept on
    // the calling thread inside the read paths below.
    int artificialReadMs = 0;
    int artificialByKeyMs = 0;
    // Artificial latency for mutation phases (project and foreign writes).
    int artificialWriteMs = 0;
    // Calling-thread identities for the worker-thread affinity assertions.
    QList<Qt::HANDLE> readThreadIds;
    QList<Qt::HANDLE> writeThreadIds;
    int readAllCalls = 0;
    struct WriteRecord
    {
        QString component;
        QString action;
        QList<int> keys;
    };
    QList<WriteRecord> writeLog;
    QMap<QString, QList<int>> defaultKeysById;
    QList<WriteRecord> foreignWriteLog;

    bool checkSetterContract(QString *error) override
    {
        if (!contractPresent) {
            if (error) {
                *error = QStringLiteral(
                    "KGlobalAccel setShortcutKeys is absent or does not expose exactly as,a(ai),u -> a(ai) with QSet<QKeySequence>");
            }
            return false;
        }
        if (!ShortcutReconciler::introspectionContractValid(contractXml)) {
            if (error) {
                *error = QStringLiteral(
                    "KGlobalAccel setShortcutKeys is absent or does not expose exactly as,a(ai),u -> a(ai) with QSet<QKeySequence>");
            }
            return false;
        }
        return true;
    }

    bool currentOwner(QString *outOwner, uint *outUid, QString *error) override
    {
        if (!ShortcutReconciler::uniqueNameValid(owner)) {
            if (error) {
                *error = QStringLiteral("owner reply is malformed");
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
        if (artificialReadMs > 0) {
            QThread::msleep(static_cast<unsigned long>(artificialReadMs));
        }
        ++readAllCalls;
        readThreadIds.append(QThread::currentThreadId());
        if (malformedRead) {
            if (error) {
                *error = QStringLiteral("unexpected reply");
            }
            return false;
        }
        for (const ShortcutTuple &tuple : tuples) {
            if (!ShortcutReconciler::keysValid(tuple.active) || !ShortcutReconciler::stringValid(tuple.component)
                || !ShortcutReconciler::stringValid(tuple.action) || !ShortcutReconciler::cosmeticValid(tuple.friendly)
                || !ShortcutReconciler::cosmeticValid(tuple.componentFriendly)) {
                if (error) {
                    *error = QStringLiteral("unexpected reply");
                }
                return false;
            }
        }
        if (tuples.size() > SHORTCUT_MAX_TUPLES) {
            if (error) {
                *error = QStringLiteral("unexpected reply");
            }
            return false;
        }
        if (out) {
            *out = tuples;
        }
        return true;
    }

    bool shortcutsByKey(int key, QList<ShortcutKeyHolder> *holders, QString *error) override
    {
        if (artificialByKeyMs > 0) {
            QThread::msleep(static_cast<unsigned long>(artificialByKeyMs));
        }
        if (failByKey) {
            if (error) {
                *error = QStringLiteral("globalShortcutsByKey call failed");
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
        combined.append(extraByKey.value(key));
        if (holders) {
            *holders = combined;
        }
        return true;
    }

    bool shortcutAvailable(int key, const QString &component, bool *available, QString *error) override
    {
        Q_UNUSED(component);
        if (failAvailable) {
            if (error) {
                *error = QStringLiteral("globalShortcutAvailable call failed");
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
        // legitimate operations stay available.
        if (failWriteAt >= 0 && writeLog.size() + foreignWriteLog.size() == failWriteAt) {
            if (error) {
                *error = QStringLiteral("simulated project write failure");
            }
            return false;
        }
        if (artificialWriteMs > 0) {
            QThread::msleep(static_cast<unsigned long>(artificialWriteMs));
        }
        writeThreadIds.append(QThread::currentThreadId());
        writeLog.append({component, action, keys});
        for (ShortcutTuple &tuple : tuples) {
            if (tuple.component == component && tuple.action == action) {
                tuple.active = keys;
                break;
            }
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
        const QString id = component + QStringLiteral("/") + action;
        const QList<int> stored = defaultKeysById.value(id);
        if (!ShortcutReconciler::keysValid(stored)) {
            if (error) {
                *error = QStringLiteral("unexpected defaultShortcutKeys reply: did not return expected keys");
            }
            return false;
        }
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
        // No lifetime accumulation cap (see writeKeys above).
        if (artificialWriteMs > 0) {
            QThread::msleep(static_cast<unsigned long>(artificialWriteMs));
        }
        writeThreadIds.append(QThread::currentThreadId());
        foreignWriteLog.append({component, action, keys});
        bool found = false;
        for (ShortcutTuple &tuple : tuples) {
            if (tuple.component == component && tuple.action == action) {
                tuple.active = keys;
                found = true;
                break;
            }
        }
        if (!found) {
            ShortcutTuple created = makeTuple(component, action, keys);
            created.componentFriendly = componentFriendly;
            created.friendly = friendly;
            tuples.append(created);
        }
        QList<int> readback;
        for (const ShortcutTuple &tuple : tuples) {
            if (tuple.component == component && tuple.action == action) {
                readback = tuple.active;
                break;
            }
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

    int totalWrites() const
    {
        return writeLog.size() + foreignWriteLog.size();
    }
};

class FakeClearedStore : public ClearedActionsStore
{
public:
    QList<ClearedAction> stored;
    int saves = 0;
    int clears = 0;
    bool failLoad = false;
    bool failSave = false;
    bool failClear = false;

    bool load(QList<ClearedAction> *actions, QString *error) override
    {
        if (failLoad) {
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
        stored = actions;
        ++saves;
        return true;
    }

    bool clear(QString *error) override
    {
        if (failClear) {
            if (error) {
                *error = QStringLiteral("could not clear the cleared shortcut list");
            }
            return false;
        }
        stored.clear();
        ++clears;
        return true;
    }
};

QPushButton *buttonByName(ActiveBorderConfigModule &module, const char *name)
{
    return module.widget()->findChild<QPushButton *>(QString::fromLocal8Bit(name));
}

QLabel *labelByName(ActiveBorderConfigModule &module, const char *name)
{
    return module.widget()->findChild<QLabel *>(QString::fromLocal8Bit(name));
}

void seedFullCatalogQuietExtras(FakeShortcutStore &store)
{
    // Full-catalog canonical quiet extras: every catalog row missing live is
    // appended at its canonical chord (empty for unbound rows) with no
    // foreign holders, so legacy 20-row seeds stay strict under
    // full-catalog missing-enabled rules while old oracles keep their
    // intended outcomes.
    for (const ShortcutCatalogEntry &entry : shortcutProjectCatalog()) {
        bool found = false;
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.component == entry.component && tuple.action == entry.action) {
                found = true;
                break;
            }
        }
        if (!found) {
            const QList<int> active = entry.canonicalKey == 0 ? QList<int>() : QList<int>{entry.canonicalKey};
            store.tuples.append(makeTuple(entry.component, entry.action, active));
        }
    }
}

void seedReady(FakeShortcutStore &store)
{
    // Project rows at non-post values with zero foreign holders, so Apply
    // succeeds. Full-catalog extras fill the remaining rows at canonical
    // (empty for unbound stay rows).
    store.tuples = {
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right"), QList<int>{419430420}),
        makeTuple(QStringLiteral("ksmserver"), QStringLiteral("Lock Session"), QList<int>{META_L}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-up"), QList<int>{7}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-right"), QList<int>{8}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-left-arrow"),
                  QList<int>{11}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-down-arrow"),
                  QList<int>{12}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-up-arrow"),
                  QList<int>{13}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-resize-outwards-right-arrow"),
                  QList<int>{14}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-float"), QList<int>{9}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-toggle-maximize"), QList<int>{10}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-left-arrow"), QList<int>{21}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-down-arrow"), QList<int>{22}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-up-arrow"), QList<int>{23}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-focus-right-arrow"), QList<int>{24}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-move-left-arrow"), QList<int>{25}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-move-right-arrow"), QList<int>{26}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-left-arrow"),
                  QList<int>{27}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-prev-up-arrow"),
                  QList<int>{28}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-down-arrow"),
                  QList<int>{29}),
        makeTuple(QStringLiteral("kwin"), QStringLiteral("omnitiler-workspace-next-right-arrow"),
                  QList<int>{30}),
    };
    seedFullCatalogQuietExtras(store);
}

void addForeignHolder(FakeShortcutStore &store, const QString &component, const QString &action,
                      const QList<int> &active, const QList<int> &defaults = {})
{
    store.tuples.append(makeTuple(component, action, active));
    if (!defaults.isEmpty()) {
        store.defaultKeysById[component + QStringLiteral("/") + action] = defaults;
    } else {
        store.defaultKeysById[component + QStringLiteral("/") + action] = QList<int>{7777};
    }
}

struct ForceCapture
{
    int calls = 0;
    bool enabled = false;
    QString text;
    bool allow = false;
};

void wireForceCapture(ActiveBorderConfigModule &module, ForceCapture &capture)
{
    module.setShortcutForcePromptHandler([&](const QString &, const QString &text, bool enabled) {
        ++capture.calls;
        capture.enabled = enabled;
        capture.text = text;
        return capture.allow;
    });
    module.setShortcutInformationHandler([](const QString &, const QString &) {});
    module.setShortcutErrorHandler([](const QString &, const QString &) {});
}

void wireQuietResults(ActiveBorderConfigModule &module)
{
    module.setShortcutInformationHandler([](const QString &, const QString &) {});
    module.setShortcutErrorHandler([](const QString &, const QString &) {});
}

void ordinarySettingsApplyNeverMutatesShortcuts()
{
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    int confirms = 0;
    module.setShortcutConfirmHandler([&](const QString &, const QString &) {
        ++confirms;
        return true;
    });
    module.setShortcutStores(&store, &cleared);
    module.load();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(cleared.clears == 0);
    module.refreshShortcutState();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(confirms == 0);
    module.save();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(cleared.clears == 0);
    CHECK(confirms == 0);
    module.defaults();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(confirms == 0);
    module.save();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(confirms == 0);
    module.refreshShortcutState();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(confirms == 0);
    QList<ClearedAction> loaded;
    QString error;
    CHECK(cleared.load(&loaded, &error));
    CHECK(loaded.isEmpty());
}

QListWidget *conflictListByModule(ActiveBorderConfigModule &module)
{
    return module.widget()->findChild<QListWidget *>(QStringLiteral("shortcutConflictList"));
}

QPushButton *presetButtonByModule(ActiveBorderConfigModule &module, const char *name)
{
    return module.widget()->findChild<QPushButton *>(QString::fromLocal8Bit(name));
}

void setRowDisabled(ActiveBorderConfigModule &module, const QString &id, bool disabled)
{
    QListWidget *list = conflictListByModule(module);
    CHECK(list != nullptr);
    if (list == nullptr) {
        return;
    }
    bool found = false;
    for (int i = 0; i < list->count(); ++i) {
        QListWidgetItem *item = list->item(i);
        if (item != nullptr && item->data(Qt::UserRole).toString() == id) {
            item->setCheckState(disabled ? Qt::Unchecked : Qt::Checked);
            found = true;
            break;
        }
    }
    CHECK(found);
    // Draft edits refresh through a queued state rebuild (the sender item
    // must survive its own change signal); pump it here for determinism.
    QApplication::processEvents();
}

void selectionPresetsAndDraft()
{
    // Compatible disables the 19 known-conflict rows plus live-colliding
    // rows in catalog order; Authentic resets to Keep. Staging only: zero
    // daemon or cleared-store writes either way.
    FakeShortcutStore store;
    seedReady(store);
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-ws"),
                                  QList<int>{SHORTCUT_META_1}));
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    CHECK(module.shortcutDisabledIds().isEmpty());
    CHECK(!module.shortcutAuthenticStaged());
    // Item 2 default-unbound rows are at canonical empty, not disabled: the
    // stay row starts enabled, present-empty, and rebindable through the
    // existing route, and load must not auto-stage it. Item 5 stay rows
    // share the same zero-key support.
    {
        const QString stayId = QStringLiteral("kwin/omnitiler-stay-workspace-1");
        CHECK(!module.shortcutDisabledIds().contains(stayId));
        bool stayFound = false;
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.component == QStringLiteral("kwin")
                && tuple.action == QStringLiteral("omnitiler-stay-workspace-1")) {
                stayFound = true;
                CHECK(tuple.active.isEmpty());
            }
        }
        CHECK(stayFound);
        QString writeError;
        QList<int> confirmed;
        CHECK(store.writeKeys(QStringLiteral("kwin"), QStringLiteral("omnitiler-stay-workspace-1"),
                              QStringLiteral("KWin"), QStringLiteral("friendly"), QList<int>{999}, &confirmed,
                              &writeError));
        CHECK(confirmed == QList<int>{999});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.component == QStringLiteral("kwin")
                && tuple.action == QStringLiteral("omnitiler-stay-workspace-1")) {
                tuple.active.clear();
            }
        }
        store.writeLog.clear();
        const QString outStayId = QStringLiteral("kwin/omnitiler-send-output-left-stay");
        CHECK(!module.shortcutDisabledIds().contains(outStayId));
        bool outStayFound = false;
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.component == QStringLiteral("kwin")
                && tuple.action == QStringLiteral("omnitiler-send-output-left-stay")) {
                outStayFound = true;
                CHECK(tuple.active.isEmpty());
            }
        }
        CHECK(outStayFound);
        CHECK(store.writeKeys(QStringLiteral("kwin"),
                              QStringLiteral("omnitiler-send-output-left-stay"),
                              QStringLiteral("KWin"), QStringLiteral("friendly"), QList<int>{999}, &confirmed,
                              &writeError));
        CHECK(confirmed == QList<int>{999});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.component == QStringLiteral("kwin")
                && tuple.action == QStringLiteral("omnitiler-send-output-left-stay")) {
                tuple.active.clear();
            }
        }
        store.writeLog.clear();
        const QString migrateId = QStringLiteral("kwin/omnitiler-migrate-workspace-left");
        CHECK(!module.shortcutDisabledIds().contains(migrateId));
        bool migrateFound = false;
        for (const ShortcutTuple &tuple : store.tuples) {
            if (tuple.component == QStringLiteral("kwin")
                && tuple.action == QStringLiteral("omnitiler-migrate-workspace-left")) {
                migrateFound = true;
                CHECK(tuple.active.isEmpty());
            }
        }
        CHECK(migrateFound);
        CHECK(store.writeKeys(QStringLiteral("kwin"),
                              QStringLiteral("omnitiler-migrate-workspace-left"),
                              QStringLiteral("KWin"), QStringLiteral("friendly"), QList<int>{999}, &confirmed,
                              &writeError));
        CHECK(confirmed == QList<int>{999});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.component == QStringLiteral("kwin")
                && tuple.action == QStringLiteral("omnitiler-migrate-workspace-left")) {
                tuple.active.clear();
            }
        }
        store.writeLog.clear();
    }
    QListWidget *list = conflictListByModule(module);
    CHECK(list != nullptr);
    if (list != nullptr) {
        CHECK(list->count() == 129);
    }
    QPushButton *compatible = presetButtonByModule(module, "shortcutCompatibleButton");
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(compatible != nullptr);
    CHECK(authentic != nullptr);
    if (compatible != nullptr) {
        compatible->click();
    }
    const QString focusId = QStringLiteral("kwin/omnitiler-focus-right");
    const QString ws1 = QStringLiteral("kwin/omnitiler-workspace-1");
    const QStringList draft = module.shortcutDisabledIds();
    CHECK(draft.size() == 24);
    CHECK(draft.contains(focusId));
    CHECK(draft.contains(ws1));
    CHECK(draft.indexOf(focusId) < draft.indexOf(ws1));
    // Item 1 Compatible: the four workspace arrow rows disable, letters and
    // the previous toggle stay Keep.
    CHECK(draft.contains(QStringLiteral("kwin/omnitiler-workspace-prev-left-arrow")));
    CHECK(draft.contains(QStringLiteral("kwin/omnitiler-workspace-prev-up-arrow")));
    CHECK(draft.contains(QStringLiteral("kwin/omnitiler-workspace-next-down-arrow")));
    CHECK(draft.contains(QStringLiteral("kwin/omnitiler-workspace-next-right-arrow")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-workspace-previous")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-workspace-prev-h")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-workspace-prev-k")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-workspace-next-j")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-workspace-next-l")));
    // Item 5 Compatible: the eight follow rows and four stay rows keep with
    // no stock KDE holder for the Meta+Ctrl+Alt arms.
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-left")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-left-arrow")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-left-stay")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-down")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-down-arrow")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-down-stay")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-up")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-up-arrow")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-up-stay")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-right")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-right-arrow")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-send-output-right-stay")));
    // R-WS-12 Compatible: the four unbound migrate rows keep with no stock
    // KDE holder.
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-migrate-workspace-left")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-migrate-workspace-right")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-migrate-workspace-up")));
    CHECK(!draft.contains(QStringLiteral("kwin/omnitiler-migrate-workspace-down")));
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(!module.shortcutAuthenticStaged());
    if (authentic != nullptr) {
        authentic->click();
    }
    CHECK(module.shortcutDisabledIds().isEmpty());
    CHECK(module.shortcutAuthenticStaged());
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    // Compatible resets the staged Authentic intent to Keep, then disables
    // the same conflicting rows again. Staging only either way.
    if (compatible != nullptr) {
        compatible->click();
    }
    CHECK(!module.shortcutAuthenticStaged());
    CHECK(module.shortcutDisabledIds().size() == 24);
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
}

void selectionDisabledApplyIgnoresForeign()
{
    // A disabled row with a live foreign holder: Apply succeeds, clears
    // only the project's own assignment, and never writes the holder.
    FakeShortcutStore store;
    seedReady(store);
    store.tuples.append(makeTuple(QStringLiteral("org.example"), QStringLiteral("other-ws"),
                                  QList<int>{SHORTCUT_META_1}));
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    wireQuietResults(module);
    module.setShortcutStores(&store, &cleared);
    module.load();
    CHECK(module.shortcutStatusText().contains(QStringLiteral("Conflict")));
    setRowDisabled(module, QStringLiteral("kwin/omnitiler-workspace-1"), true);
    CHECK(module.shortcutDisabledIds() == QStringList{QStringLiteral("kwin/omnitiler-workspace-1")});
    CHECK(!module.shortcutStatusText().contains(QStringLiteral("Conflict")));
    module.requestShortcutApply();
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(store.foreignWriteLog.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-workspace-1")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("other-ws")) {
            CHECK(tuple.active == (QList<int>{SHORTCUT_META_1}));
        }
    }
    CHECK(cleared.saves == 0);
    CHECK(cleared.stored.isEmpty());
}

void selectionDraftEditCancelsPreview()
{
    // A declined conflict modal leaves zero writes and the refusal error;
    // a later draft edit only refreshes state, still with zero writes.
    FakeShortcutStore store;
    seedReady(store);
    store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("Grid View"), QList<int>{META_G}));
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture force;
    force.allow = false;
    wireForceCapture(module, force);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authenticForDraft = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authenticForDraft != nullptr);
    if (authenticForDraft != nullptr) {
        authenticForDraft->click();
    }
    module.requestShortcutApply();
    CHECK(force.calls == 1);
    CHECK(module.shortcutForcePreviewText().contains(QStringLiteral("Grid View")));
    CHECK(module.shortcutErrorText().contains(QStringLiteral("Meta+G")));
    setRowDisabled(module, QStringLiteral("kwin/omnitiler-workspace-1"), true);
    CHECK(force.calls == 1);
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
}

void selectionStaleDraftRefusesForce()
{
    // Declined modal, then a draft edit: the next Apply re-refuses under
    // the new draft with zero writes. The Apply refusal error is preserved.
    FakeShortcutStore store;
    seedReady(store);
    store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("Grid View"), QList<int>{META_G}));
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture force;
    force.allow = false;
    wireForceCapture(module, force);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authenticForStale = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authenticForStale != nullptr);
    if (authenticForStale != nullptr) {
        authenticForStale->click();
    }
    module.requestShortcutApply();
    CHECK(force.calls == 1);
    setRowDisabled(module, QStringLiteral("kwin/omnitiler-workspace-1"), true);
    module.requestShortcutApply();
    CHECK(force.calls == 2);
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(module.shortcutErrorText().contains(QStringLiteral("Meta+G")));
}

void selectionSaveIsolationWithDraft()
{
    // A staged non-empty draft never leaks into ordinary Save: zero
    // shortcut writes, zero cleared saves, draft retained.
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    setRowDisabled(module, QStringLiteral("kwin/omnitiler-workspace-1"), true);
    CHECK(module.shortcutDisabledIds().size() == 1);
    module.save();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(cleared.clears == 0);
    CHECK(module.shortcutDisabledIds().size() == 1);
    module.save();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
}

void selectionRevertKeepsOwnAssignment()
{
    // Accepted conflict modal clears the holder and assigns the project
    // row in one step; Revert restores the foreign default while the
    // project assignment stays in place.
    FakeShortcutStore store;
    seedReady(store);
    addForeignHolder(store, QStringLiteral("org.example"), QStringLiteral("other-launch"),
                     QList<int>{META_G, 999}, QList<int>{2222});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return true; });
    ForceCapture forceRevert;
    forceRevert.allow = true;
    wireForceCapture(module, forceRevert);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authenticForRevert = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authenticForRevert != nullptr);
    if (authenticForRevert != nullptr) {
        authenticForRevert->click();
    }
    module.requestShortcutApply();
    CHECK(forceRevert.calls == 1);
    CHECK(module.shortcutErrorText().isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("other-launch")) {
            CHECK(tuple.active == (QList<int>{999}));
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
    }
    module.requestShortcutRevert();
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(cleared.stored.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("other-launch")) {
            CHECK(tuple.active == (QList<int>{2222}));
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
    }
}

void selectionLiveDisabledReopen()
{
    // Reopening stages present-empty own assignments as Disable; missing
    // rows are not Disable.
    FakeShortcutStore store;
    seedReady(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-workspace-1")) {
            tuple.active.clear();
        }
    }
    for (int i = store.tuples.size() - 1; i >= 0; --i) {
        if (store.tuples.at(i).action == QStringLiteral("omnitiler-move-workspace-1")) {
            store.tuples.removeAt(i);
        }
    }
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    CHECK(module.shortcutDisabledIds().contains(QStringLiteral("kwin/omnitiler-workspace-1")));
    CHECK(!module.shortcutDisabledIds().contains(QStringLiteral("kwin/omnitiler-move-workspace-1")));
}

void selectionMissingEnabledStatus()
{
    // Missing enabled rows never claim success; missing disabled rows stay
    // allowed with an honest missing row.
    {
        FakeShortcutStore store;
        seedReady(store);
        for (int i = store.tuples.size() - 1; i >= 0; --i) {
            if (store.tuples.at(i).action == QStringLiteral("omnitiler-workspace-1")) {
                store.tuples.removeAt(i);
            }
        }
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        module.setShortcutStores(&store, &cleared);
        module.load();
        CHECK(module.shortcutStatusText().contains(QStringLiteral("unavailable"))
              || module.shortcutStatusText().contains(QStringLiteral("missing")));
        CHECK(!module.shortcutStatusText().contains(QStringLiteral("applied (129 rows")));
    }
    {
        FakeShortcutStore store;
        seedReady(store);
        for (int i = store.tuples.size() - 1; i >= 0; --i) {
            if (store.tuples.at(i).action == QStringLiteral("omnitiler-workspace-1")) {
                store.tuples.removeAt(i);
            }
        }
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        module.setShortcutStores(&store, &cleared);
        // Stage the absent row as disabled before load completes: absent
        // disabled is allowed, so seeding the draft directly then refreshing
        // must not claim missing success for enabled rows (none missing
        // enabled here because the only absent row is disabled).
        module.load();
        setRowDisabled(module, QStringLiteral("kwin/omnitiler-workspace-1"), true);
        CHECK(module.shortcutDisabledIds().contains(QStringLiteral("kwin/omnitiler-workspace-1")));
    }
}

void selectionDisabledLockAbsentShowsRows()
{
    // Focus disabled leaves the lock out of scope: an absent lock still
    // shows the full row list instead of an empty missing state.
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    // Lock vanishes live after open; disabling focus-right then leaves it
    // out of scope instead of failing the page.
    for (int i = static_cast<int>(store.tuples.size()) - 1; i >= 0; --i) {
        if (store.tuples.at(i).action == QStringLiteral("Lock Session")) {
            store.tuples.removeAt(i);
        }
    }
    setRowDisabled(module, QStringLiteral("kwin/omnitiler-focus-right"), true);
    CHECK(!module.shortcutStatusText().contains(QStringLiteral("project bindings are missing")));
    QListWidget *list = conflictListByModule(module);
    CHECK(list != nullptr);
    if (list != nullptr) {
        CHECK(list->count() == 129);
    }
}

void selectionCompatibleQueryFailurePreservesDraft()
{
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    CHECK(module.shortcutDisabledIds().isEmpty());
    store.failByKey = true;
    QPushButton *compatible = presetButtonByModule(module, "shortcutCompatibleButton");
    CHECK(compatible != nullptr);
    if (compatible != nullptr) {
        compatible->click();
    }
    CHECK(module.shortcutDisabledIds().isEmpty());
    CHECK(module.shortcutErrorText().contains(QStringLiteral("Compatible preset unavailable")));
}

void selectionForeignDefaultCompatibleAfterClear()
{
    // Noncompiled foreign default claims the chord after its active list is
    // cleared: Compatible still disables the row.
    FakeShortcutStore store;
    seedReady(store);
    ShortcutTuple foreign = makeTuple(QStringLiteral("org.example"), QStringLiteral("other-ws"), QList<int>());
    foreign.defaults = QList<int>{SHORTCUT_META_1};
    store.tuples.append(foreign);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    wireQuietResults(module);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *compatible = presetButtonByModule(module, "shortcutCompatibleButton");
    CHECK(compatible != nullptr);
    if (compatible != nullptr) {
        compatible->click();
    }
    CHECK(module.shortcutDisabledIds().contains(QStringLiteral("kwin/omnitiler-workspace-1")));
}

void selectionReadableRowText()
{
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QListWidget *list = conflictListByModule(module);
    CHECK(list != nullptr);
    if (list == nullptr) {
        return;
    }
    bool sawReadable = false;
    bool sawOwn = false;
    for (int i = 0; i < list->count(); ++i) {
        const QString text = list->item(i)->text();
        if (text.contains(QStringLiteral("Meta+1"))) {
            sawReadable = true;
        }
        if (text.contains(QStringLiteral("own ["))) {
            sawOwn = true;
        }
        CHECK(!text.contains(QStringLiteral("268435505")));
    }
    CHECK(sawReadable);
    CHECK(sawOwn);
}

void confirmationGatesEveryMutation()
{
    // Apply needs no preliminary confirmation; Revert keeps its data-loss
    // confirmation. Result dialogs are modal in both cases.
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    int confirms = 0;
    bool allow = false;
    ForceCapture force;
    wireForceCapture(module, force);
    int infos = 0;
    int errors = 0;
    module.setShortcutInformationHandler([&](const QString &, const QString &) { ++infos; });
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
    module.setShortcutConfirmHandler([&](const QString &title, const QString &text) {
        ++confirms;
        CHECK(!title.isEmpty());
        CHECK(!text.isEmpty());
        return allow;
    });
    module.setShortcutStores(&store, &cleared);
    module.load();
    // Direct Apply without conflicts: no confirmation of any kind, success
    // dialog, writes happen. Canonical assignment requires staged Authentic.
    allow = true;
    QPushButton *authenticForConfirm = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authenticForConfirm != nullptr);
    if (authenticForConfirm != nullptr) {
        authenticForConfirm->click();
    }
    CHECK(module.shortcutAuthenticStaged());
    module.requestShortcutApply();
    CHECK(confirms == 0);
    CHECK(force.calls == 0);
    CHECK(store.totalWrites() > 0);
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(infos == 1);
    CHECK(errors == 0);
    // Revert needs its own data-loss confirmation; decline first.
    allow = false;
    const int writesBeforeRevert = store.totalWrites();
    const int savesBeforeRevert = cleared.saves;
    module.requestShortcutRevert();
    CHECK(confirms == 1);
    CHECK(store.totalWrites() == writesBeforeRevert);
    CHECK(cleared.saves == savesBeforeRevert);
}

void stateAndErrorPresentation()
{
    // Preserved with zero holders: Keep is trivially satisfied, so a fresh
    // load with nothing staged reports preservation, never canonical
    // ownership.
    {
        FakeShortcutStore store;
        seedReady(store);
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        module.setShortcutStores(&store, &cleared);
        module.load();
        CHECK(module.shortcutStatusText().contains(QStringLiteral("preserved")));
        CHECK(module.shortcutStatusText().contains(QStringLiteral("129 rows")));
        CHECK(module.shortcutErrorText().isEmpty());
        CHECK(buttonByName(module, "shortcutFinishApplyButton") == nullptr);
        CHECK(buttonByName(module, "shortcutRestoreButton") == nullptr);
        CHECK(!module.shortcutStatusText().contains(QStringLiteral("journal")));
        CHECK(!module.shortcutStatusText().contains(QStringLiteral("Interrupted")));
        CHECK(!module.shortcutStatusText().contains(QStringLiteral("legacy")));
        CHECK(!module.shortcutStatusText().contains(QStringLiteral("migration")));
        QLabel *status = labelByName(module, "shortcutStatusLabel");
        QLabel *error = labelByName(module, "shortcutErrorLabel");
        CHECK(status != nullptr);
        CHECK(error != nullptr);
        if (status) {
            CHECK(status->text() == module.shortcutStatusText());
        }
        if (error) {
            CHECK(error->text() == module.shortcutErrorText());
        }
    }
    // Applied then consumed after an Authentic Apply: canonical chords are
    // committed and the staged reset intent is consumed, so the page reports
    // preservation for the next Apply.
    {
        FakeShortcutStore store;
        seedReady(store);
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        wireQuietResults(module);
        module.setShortcutStores(&store, &cleared);
        module.load();
        QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
        CHECK(authentic != nullptr);
        if (authentic != nullptr) {
            authentic->click();
        }
        module.requestShortcutApply();
        CHECK(module.shortcutErrorText().isEmpty());
        CHECK(!module.shortcutAuthenticStaged());
        CHECK(module.shortcutStatusText().contains(QStringLiteral("preserved")));
        CHECK(module.shortcutStatusText().contains(QStringLiteral("129 rows")));
    }
    // Conflict with an unknown foreign holder (Authentic stages the lock
    // relocation chords, so Meta+Esc is in scope).
    {
        FakeShortcutStore store;
        seedReady(store);
        store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("other-action"), QList<int>{META_ESC}));
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        ForceCapture forceEsc;
        forceEsc.allow = false;
        wireForceCapture(module, forceEsc);
        module.setShortcutStores(&store, &cleared);
        module.load();
        QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
        CHECK(authentic != nullptr);
        if (authentic != nullptr) {
            authentic->click();
        }
        CHECK(module.shortcutStatusText().contains(QStringLiteral("Conflict")));
        CHECK(module.shortcutStatusText().contains(QStringLiteral("Meta+Esc")));
        module.requestShortcutApply();
        CHECK(forceEsc.calls == 1);
        CHECK(store.writeLog.empty());
        CHECK(store.foreignWriteLog.empty());
        CHECK(!module.shortcutErrorText().isEmpty());
        CHECK(module.shortcutErrorText().contains(QStringLiteral("Meta+Esc")));
        const QString preserved = module.shortcutErrorText();
        module.refreshShortcutState();
        CHECK(module.shortcutErrorText() == preserved);
        CHECK(module.shortcutStatusText().contains(QStringLiteral("Conflict")));
    }
    // Transport failure surfaces as unavailable, never Conflict.
    {
        FakeShortcutStore store;
        seedReady(store);
        store.failByKey = true;
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        module.setShortcutStores(&store, &cleared);
        module.load();
        CHECK(!module.shortcutStatusText().contains(QStringLiteral("Conflict")));
        CHECK(module.shortcutStatusText().contains(QStringLiteral("unavailable")));
    }
    // Split-signature contract rejection, never the stale combined form.
    {
        FakeShortcutStore store;
        seedReady(store);
        store.contractXml = QStringLiteral(
            "<node><interface name=\"org.kde.KGlobalAccel\">"
            "<method name=\"setShortcutKeys\">"
            "<arg type=\"asa(ai)u\" direction=\"in\"/>"
            "<arg type=\"a(ai)\" direction=\"out\"/>"
            "</method></interface></node>");
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        int contractErrors = 0;
        module.setShortcutInformationHandler([](const QString &, const QString &) {});
        module.setShortcutErrorHandler([&](const QString &, const QString &) { ++contractErrors; });
        module.setShortcutStores(&store, &cleared);
        module.load();
        CHECK(module.shortcutStatusText().contains(QStringLiteral("as,a(ai),u -> a(ai)")));
        CHECK(!module.shortcutStatusText().contains(QStringLiteral("asa(ai)u")));
        module.requestShortcutApply();
        CHECK(contractErrors == 1);
        CHECK(store.totalWrites() == 0);
        CHECK(module.shortcutErrorText().contains(QStringLiteral("as,a(ai),u -> a(ai)")));
        CHECK(!module.shortcutErrorText().contains(QStringLiteral("asa(ai)u")));
    }
    // System Monitor Meta+Esc holder stays Ready and Apply avoids it.
    {
        FakeShortcutStore store;
        seedReady(store);
        ShortcutKeyHolder sysmon;
        sysmon.component = shortcutAuthorizedEscComponent();
        sysmon.action = shortcutAuthorizedEscAction();
        sysmon.active = QList<int>{META_ESC};
        sysmon.componentFriendly = QStringLiteral("System Monitor");
        sysmon.friendly = QStringLiteral("Launch System Monitor");
        store.extraByKey[META_ESC].append(sysmon);
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        wireQuietResults(module);
        module.setShortcutStores(&store, &cleared);
        module.load();
        // Stage Authentic before asserting: the authorized displacement is
        // in scope only while the lock relocation is staged.
        QPushButton *authenticForSysmon = presetButtonByModule(module, "shortcutAuthenticButton");
        CHECK(authenticForSysmon != nullptr);
        if (authenticForSysmon != nullptr) {
            authenticForSysmon->click();
        }
        CHECK(!module.shortcutStatusText().contains(QStringLiteral("Conflict")));
        CHECK(module.shortcutStatusText().contains(QStringLiteral("Ready")));
        module.requestShortcutApply();
        CHECK(!store.writeLog.empty());
        CHECK(module.shortcutErrorText().isEmpty());
        for (const auto &record : store.writeLog) {
            CHECK(!(record.component == shortcutAuthorizedEscComponent() && record.action == shortcutAuthorizedEscAction()));
        }
        for (const auto &record : store.foreignWriteLog) {
            CHECK(!(record.component == shortcutAuthorizedEscComponent() && record.action == shortcutAuthorizedEscAction()));
        }
    }
}

void forcePreviewAcceptCancelRevert()
{
    FakeShortcutStore store;
    seedReady(store);
    addForeignHolder(store, QStringLiteral("org.kde.unknown"), QStringLiteral("other-launch"),
                     QList<int>{META_G, 999}, QList<int>{1111});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture force;
    force.allow = false;
    wireForceCapture(module, force);
    int infos = 0;
    int errors = 0;
    module.setShortcutInformationHandler([&](const QString &, const QString &) { ++infos; });
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authenticForForce = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authenticForForce != nullptr);
    if (authenticForForce != nullptr) {
        authenticForForce->click();
    }
    // Declined conflict modal: zero mutation, refusal kept, no followup
    // error dialog for the explicit Cancel.
    module.requestShortcutApply();
    CHECK(force.calls == 1);
    CHECK(force.enabled);
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(module.shortcutErrorText().contains(QStringLiteral("Meta+G")));
    CHECK(errors == 0);
    CHECK(infos == 0);
    CHECK(force.text.contains(QStringLiteral("org.kde.unknown")));
    CHECK(force.text.contains(QStringLiteral("other-launch")));
    CHECK(force.text.contains(ShortcutReconciler::keyDisplayName(META_G)));
    CHECK(force.text.contains(ShortcutReconciler::keyDisplayName(999)));
    CHECK(force.text.contains(QStringLiteral("will remove")));
    CHECK(force.text.contains(QStringLiteral("keep")));
    CHECK(module.shortcutForcePreviewText() == force.text);
    CHECK(labelByName(module, "shortcutForcePreviewLabel") == nullptr);
    CHECK(buttonByName(module, "shortcutForceApplyButton") == nullptr);
    CHECK(buttonByName(module, "shortcutForceCancelButton") == nullptr);
    // A second Apply shows the modal again; accepting forces in one step
    // with revalidation and a success dialog.
    module.requestShortcutApply();
    CHECK(force.calls == 2);
    force.allow = true;
    module.requestShortcutApply();
    CHECK(force.calls == 3);
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(cleared.saves == 1);
    CHECK(infos == 1);
    bool foundHolder = false;
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.component == QStringLiteral("org.kde.unknown") && tuple.action == QStringLiteral("other-launch")) {
            foundHolder = true;
            CHECK(tuple.active == (QList<int>{999}));
        }
    }
    CHECK(foundHolder);
    // Mutation while the accepted modal runs: the hook drifts the holder
    // after the preview is built, so revalidation fails stale with zero
    // further writes and a result error dialog. No Force runs outside the
    // accepted modal path.
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.component == QStringLiteral("org.kde.unknown")) {
            tuple.active = QList<int>{META_M, 5555};
        }
    }
    const int writesBeforeStale = store.totalWrites();
    const int savesBeforeStale = cleared.saves;
    const int errorsBeforeStale = errors;
    const int infosBeforeStale = infos;
    int modalRuns = 0;
    module.setShortcutForcePromptHandler([&](const QString &, const QString &text, bool enabled) {
        ++modalRuns;
        CHECK(enabled);
        CHECK(text.contains(QStringLiteral("other-launch")));
        // Drift the holder mid-modal: the confirmed preview no longer
        // matches live state.
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.component == QStringLiteral("org.kde.unknown")) {
                tuple.active = QList<int>{META_M, 6666};
            }
        }
        return true;
    });
    module.requestShortcutApply();
    CHECK(modalRuns == 1);
    CHECK(!module.shortcutErrorText().isEmpty());
    CHECK(module.shortcutErrorText().contains(QStringLiteral("stale")));
    CHECK(store.totalWrites() == writesBeforeStale);
    CHECK(cleared.saves == savesBeforeStale);
    CHECK(errors == errorsBeforeStale + 1);
    CHECK(infos == infosBeforeStale);
}

void recoveryVisibilityAndRouting()
{
    FakeShortcutStore store;
    seedReady(store);
    addForeignHolder(store, QStringLiteral("org.kde.unknown"), QStringLiteral("other-launch"),
                     QList<int>{META_G, 999}, QList<int>{2222});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    int confirms = 0;
    module.setShortcutConfirmHandler([&](const QString &, const QString &) {
        ++confirms;
        return true;
    });
    ForceCapture force;
    force.allow = true;
    wireForceCapture(module, force);
    int infos = 0;
    int errors = 0;
    module.setShortcutInformationHandler([&](const QString &, const QString &) { ++infos; });
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    CHECK(buttonByName(module, "shortcutFinishApplyButton") == nullptr);
    CHECK(buttonByName(module, "shortcutRestoreButton") == nullptr);
    CHECK(buttonByName(module, "shortcutForceApplyButton") == nullptr);
    CHECK(buttonByName(module, "shortcutForceCancelButton") == nullptr);
    QPushButton *authenticForRecovery = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authenticForRecovery != nullptr);
    if (authenticForRecovery != nullptr) {
        authenticForRecovery->click();
    }
    // Accepted conflict modal applies in one step with revalidation and a
    // success dialog; no second Force step exists.
    module.requestShortcutApply();
    CHECK(force.calls == 1);
    CHECK(confirms == 0);
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(infos == 1);
    CHECK(errors == 0);
    CHECK(cleared.stored.size() == 1);
    // Revert confirm mentions KDE defaults and reports via result dialog.
    bool revertTextSawDefaults = false;
    module.setShortcutConfirmHandler([&](const QString &title, const QString &text) {
        ++confirms;
        if (title.contains(QStringLiteral("Revert")) && text.contains(QStringLiteral("KDE default"))) {
            revertTextSawDefaults = true;
        }
        return true;
    });
    module.requestShortcutRevert();
    CHECK(confirms == 1);
    CHECK(revertTextSawDefaults);
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(infos == 2);
    CHECK(cleared.stored.isEmpty());
    bool foundHolder = false;
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.component == QStringLiteral("org.kde.unknown") && tuple.action == QStringLiteral("other-launch")) {
            foundHolder = true;
            CHECK(tuple.active == (QList<int>{2222}));
        }
    }
    CHECK(foundHolder);
    // Empty-list Revert is a no-op success with a result dialog.
    const int writesAfter = store.totalWrites();
    module.requestShortcutRevert();
    CHECK(confirms == 2);
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(infos == 3);
    CHECK(store.totalWrites() == writesAfter);
}

void legacyMigrationDeferredFromOpen()
{
    // The cleared config is the only durability: opening/refreshing never
    // writes the cleared store and the status never mentions retired
    // vocabulary or interrupted phases.
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    int confirms = 0;
    module.setShortcutConfirmHandler([&](const QString &, const QString &) {
        ++confirms;
        return true;
    });
    module.setShortcutStores(&store, &cleared);
    module.load();
    module.refreshShortcutState();
    CHECK(confirms == 0);
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(!module.shortcutStatusText().contains(QStringLiteral("journal")));
    CHECK(!module.shortcutStatusText().contains(QStringLiteral("legacy")));
    CHECK(!module.shortcutStatusText().contains(QStringLiteral("migration")));
    CHECK(!module.shortcutStatusText().contains(QStringLiteral("Interrupted")));
    CHECK(!module.shortcutStatusText().contains(QStringLiteral("Finish")));
    CHECK(!module.shortcutStatusText().contains(QStringLiteral("Restore")));
    const QString clearedPath = defaultClearedActionsPath();
    CHECK(!clearedPath.isEmpty());
    CHECK(QDir::isAbsolutePath(clearedPath));
    CHECK(clearedPath.endsWith(QStringLiteral("shortcut-clearedrc")));
    CHECK(clearedPath.contains(QStringLiteral("omnitiler")));
    CHECK(!QFile::exists(clearedPath));
}

void knownForeignStatusAlignsWithApply()
{
    // Status/backend alignment: a known compiled foreign holder (Grid
    // View on Meta+G) shows Conflict, never Ready, and Apply refuses
    // with zero writes while offering a Force preview.
    FakeShortcutStore store;
    seedReady(store);
    store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("Grid View"), QList<int>{META_G}));
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture forceKnown;
    forceKnown.allow = false;
    wireForceCapture(module, forceKnown);
    int knownErrors = 0;
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++knownErrors; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authenticForKnown = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authenticForKnown != nullptr);
    if (authenticForKnown != nullptr) {
        authenticForKnown->click();
    }
    CHECK(module.shortcutStatusText().contains(QStringLiteral("Conflict")));
    CHECK(module.shortcutStatusText().contains(QStringLiteral("Meta+G")));
    CHECK(!module.shortcutStatusText().contains(QStringLiteral("Ready")));
    module.requestShortcutApply();
    CHECK(forceKnown.calls == 1);
    CHECK(forceKnown.enabled);
    CHECK(store.writeLog.empty());
    CHECK(store.foreignWriteLog.empty());
    CHECK(!module.shortcutErrorText().isEmpty());
    CHECK(module.shortcutErrorText().contains(QStringLiteral("Grid View")));
    // Explicit Cancel shows no followup error dialog.
    CHECK(knownErrors == 0);
    CHECK(module.shortcutForcePreviewText().contains(QStringLiteral("Grid View")));
}

void growArrowStatusAlignsWithApply()
{
    // KCM status/backend alignment: a Switch Window holder on Meta+Alt+Left
    // shows Conflict with the 15-row state, Apply refuses with zero writes,
    // and Force preview lists the exact removal.
    FakeShortcutStore store;
    seedReady(store);
    store.tuples.append(
        makeTuple(QStringLiteral("kwin"), QStringLiteral("Switch Window Left"), QList<int>{META_ALT_LEFT}));
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture forceGrow;
    forceGrow.allow = false;
    wireForceCapture(module, forceGrow);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authenticForGrow = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authenticForGrow != nullptr);
    if (authenticForGrow != nullptr) {
        authenticForGrow->click();
    }
    CHECK(module.shortcutStatusText().contains(QStringLiteral("Conflict")));
    CHECK(module.shortcutStatusText().contains(QStringLiteral("Meta+Alt+Left")));
    CHECK(!module.shortcutStatusText().contains(QStringLiteral("Ready")));
    module.requestShortcutApply();
    CHECK(forceGrow.calls == 1);
    CHECK(store.writeLog.empty());
    CHECK(store.foreignWriteLog.empty());
    CHECK(!module.shortcutErrorText().isEmpty());
    CHECK(module.shortcutErrorText().contains(QStringLiteral("Switch Window Left")));
    CHECK(module.shortcutForcePreviewText().contains(QStringLiteral("Switch Window Left")));
    CHECK(module.shortcutForcePreviewText().contains(QStringLiteral("Meta+Alt+Left")));
}

void focusMoveArrowStatusAlignsWithApply()
{
    // KCM status/backend alignment for all six focus/move rows: each Quick
    // Tile / to-Screen holder shows Conflict, Apply refuses with zero
    // writes, and Force preview lists the exact removal.
    const QList<QString> actions = {
        QStringLiteral("Window Quick Tile Left"),
        QStringLiteral("Window Quick Tile Bottom"),
        QStringLiteral("Window Quick Tile Top"),
        QStringLiteral("Window Quick Tile Right"),
        QStringLiteral("Window to Previous Screen"),
        QStringLiteral("Window to Next Screen"),
    };
    const QList<QString> displays = {
        QStringLiteral("Meta+Left"),
        QStringLiteral("Meta+Down"),
        QStringLiteral("Meta+Up"),
        QStringLiteral("Meta+Right"),
        QStringLiteral("Meta+Shift+Left"),
        QStringLiteral("Meta+Shift+Right"),
    };
    const QList<int> keys = {META_LEFT, META_DOWN, META_UP, META_RIGHT, META_SHIFT_LEFT, META_SHIFT_RIGHT};
    for (int i = 0; i < actions.size(); ++i) {
        FakeShortcutStore store;
        seedReady(store);
        store.tuples.append(makeTuple(QStringLiteral("kwin"), actions.at(i), QList<int>{keys.at(i)}));
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        ForceCapture forceFocusMove;
        forceFocusMove.allow = false;
        wireForceCapture(module, forceFocusMove);
        module.setShortcutStores(&store, &cleared);
        module.load();
        QPushButton *authenticForFocusMove = presetButtonByModule(module, "shortcutAuthenticButton");
        CHECK(authenticForFocusMove != nullptr);
        if (authenticForFocusMove != nullptr) {
            authenticForFocusMove->click();
        }
        CHECK(module.shortcutStatusText().contains(QStringLiteral("Conflict")));
        CHECK(module.shortcutStatusText().contains(displays.at(i)));
        CHECK(!module.shortcutStatusText().contains(QStringLiteral("Ready")));
        module.requestShortcutApply();
        CHECK(forceFocusMove.calls == 1);
        CHECK(store.writeLog.empty());
        CHECK(store.foreignWriteLog.empty());
        CHECK(!module.shortcutErrorText().isEmpty());
        CHECK(module.shortcutErrorText().contains(actions.at(i)));
        CHECK(module.shortcutForcePreviewText().contains(actions.at(i)));
        CHECK(module.shortcutForcePreviewText().contains(displays.at(i)));
    }
}

void terminalFailureLogIncludesReason()
{
    FakeShortcutStore store;
    seedReady(store);
    addForeignHolder(store, QStringLiteral("org.kde.unknown"), QStringLiteral("other-launch"),
                     QList<int>{META_G, 999}, QList<int>{1111});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    wireQuietResults(module);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authenticForLog = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authenticForLog != nullptr);
    if (authenticForLog != nullptr) {
        authenticForLog->click();
    }
    QStringList messages;
    ShortcutDiag::setSink([&](QtMsgType, const QString &message) {
        messages.append(message);
    });
    // Drift inside the accepted modal: the preview is built, then live
    // state moves, so Force fails stale with zero writes.
    module.setShortcutForcePromptHandler([&](const QString &, const QString &, bool enabled) {
        CHECK(enabled);
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.component == QStringLiteral("org.kde.unknown")) {
                tuple.active = QList<int>{META_G, 1000};
            }
        }
        return true;
    });
    module.requestShortcutApply();
    ShortcutDiag::resetSink();
    bool sawTerminalFailure = false;
    bool sawConflictRow = false;
    bool sawStaleForceFailure = false;
    for (const QString &message : messages) {
        // Reconstructible diagnostics keep holder IDs plus chord images.
        if (message.contains(QStringLiteral("omnitiler.shortcut op=apply stage=result outcome=failed"))
            && message.contains(QStringLiteral("org.kde.unknown"))
            && message.contains(QStringLiteral("other-launch"))) {
            sawTerminalFailure = true;
        }
        if (message.contains(QStringLiteral("stage=conflict"))
            && message.contains(QStringLiteral("org.kde.unknown"))) {
            sawConflictRow = true;
        }
        if (message.contains(QStringLiteral("omnitiler.shortcut op=force-apply stage=result outcome=failed"))
            && message.contains(QStringLiteral("confirmed force image is stale"))
            && message.contains(QStringLiteral("writes=0"))) {
            sawStaleForceFailure = true;
        }
    }
    CHECK(sawTerminalFailure);
    CHECK(sawConflictRow);
    CHECK(sawStaleForceFailure);
    CHECK(cleared.saves == 0);
}

void selectionKeepPreservesCustomCanonicalEmpty()
{
    // Plain Keep Apply preserves every enabled row: a customized chord, a
    // canonical chord, and a present-empty assignment all stay, with zero
    // writes. Status reports preservation, never canonical ownership.
    FakeShortcutStore store;
    seedReady(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777001};
        }
        if (tuple.action == QStringLiteral("omnitiler-workspace-1")) {
            tuple.active.clear();
        }
    }
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    wireQuietResults(module);
    module.setShortcutStores(&store, &cleared);
    module.load();
    CHECK(!module.shortcutAuthenticStaged());
    // The live-empty row auto-stages Disable; restage it to Keep so empty
    // means leave empty.
    CHECK(module.shortcutDisabledIds().contains(QStringLiteral("kwin/omnitiler-workspace-1")));
    setRowDisabled(module, QStringLiteral("kwin/omnitiler-workspace-1"), false);
    CHECK(!module.shortcutDisabledIds().contains(QStringLiteral("kwin/omnitiler-workspace-1")));
    module.requestShortcutApply();
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{777001}));
        }
        if (tuple.action == QStringLiteral("omnitiler-workspace-1")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle-maximize")) {
            CHECK(tuple.active == (QList<int>{10}));
        }
        if (tuple.action == QStringLiteral("Lock Session")) {
            CHECK(tuple.active == (QList<int>{META_L}));
        }
    }
    CHECK(module.shortcutStatusText().contains(QStringLiteral("preserved")));
    CHECK(module.shortcutStatusText().contains(QStringLiteral("129 rows")));
}

void selectionAuthenticAssignsCanonical()
{
    // Staged Authentic commits canonical chords on confirmed Apply,
    // including the focus/lock relocation, without touching foreign
    // holders, and the staged reset intent is consumed there.
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    wireQuietResults(module);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authentic != nullptr);
    if (authentic != nullptr) {
        authentic->click();
    }
    CHECK(module.shortcutAuthenticStaged());
    CHECK(module.shortcutDisabledIds().isEmpty());
    module.requestShortcutApply();
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(!module.shortcutAuthenticStaged());
    CHECK(store.totalWrites() > 0);
    CHECK(store.foreignWriteLog.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
        if (tuple.action == QStringLiteral("omnitiler-resize-outwards-up")) {
            CHECK(tuple.active == (QList<int>{META_ALT_K}));
        }
        if (tuple.action == QStringLiteral("omnitiler-focus-right")) {
            CHECK(tuple.active == (QList<int>{META_L}));
        }
        if (tuple.action == QStringLiteral("Lock Session")) {
            CHECK(tuple.active == (QList<int>{META_ESC}));
        }
    }
    CHECK(module.shortcutStatusText().contains(QStringLiteral("preserved")));
}

void selectionCompatibleKeepsCustomNonconflict()
{
    // Compatible disables known/canonical conflicts while an unrelated
    // customized row stays Keep; Apply clears the disabled own rows and
    // leaves the custom chord and the foreign holder untouched.
    FakeShortcutStore store;
    seedReady(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-workspace-2")) {
            tuple.active = QList<int>{888001};
        }
    }
    store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("Grid View"), QList<int>{META_G}));
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    wireQuietResults(module);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *compatible = presetButtonByModule(module, "shortcutCompatibleButton");
    CHECK(compatible != nullptr);
    if (compatible != nullptr) {
        compatible->click();
    }
    CHECK(!module.shortcutAuthenticStaged());
    CHECK(module.shortcutDisabledIds().contains(QStringLiteral("kwin/omnitiler-toggle-float")));
    CHECK(!module.shortcutDisabledIds().contains(QStringLiteral("kwin/omnitiler-workspace-2")));
    module.requestShortcutApply();
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(store.foreignWriteLog.isEmpty());
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-workspace-2")) {
            CHECK(tuple.active == (QList<int>{888001}));
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("Grid View")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
    }
}

void selectionKeepCustomConflictPreviewForce()
{
    // A Keep-customized chord claimed by a foreign holder refuses Apply
    // naming the actual chord. Accepting the modal clears only the foreign
    // holder, preserving the custom chord with zero project writes.
    FakeShortcutStore store;
    seedReady(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777001};
        }
    }
    addForeignHolder(store, QStringLiteral("org.example"), QStringLiteral("other-app"), QList<int>{777001},
                    QList<int>{5555});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture force;
    force.allow = true;
    wireForceCapture(module, force);
    module.setShortcutStores(&store, &cleared);
    module.load();
    CHECK(!module.shortcutAuthenticStaged());
    module.requestShortcutApply();
    CHECK(force.calls == 1);
    CHECK(force.enabled);
    // The refusal names the live actual chord, never a silent canonical.
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(force.text.contains(QStringLiteral("other-app")));
    CHECK(force.text.contains(QStringLiteral("stay at their current assignments")));
    CHECK(store.writeLog.isEmpty());
    CHECK(!store.foreignWriteLog.isEmpty());
    CHECK(cleared.saves == 1);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("other-app")) {
            CHECK(tuple.active.isEmpty());
        }
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{777001}));
        }
    }
}

void selectionForceRevalidationKeepDrift()
{
    // A Keep-actual drift inside the accepted modal fails Force as stale
    // with zero writes and zero cleared-list persists. No Force runs
    // outside the accepted modal path.
    FakeShortcutStore store;
    seedReady(store);
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777001};
        }
    }
    addForeignHolder(store, QStringLiteral("org.example"), QStringLiteral("other-app"), QList<int>{777001},
                    QList<int>{5555});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    int errors = 0;
    module.setShortcutInformationHandler([](const QString &, const QString &) {});
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    // Drift the Keep row mid-modal: the conflicting chord stays claimed so
    // a preview still builds, but the live image differs and Force must
    // refuse as stale before any persist.
    module.setShortcutForcePromptHandler([&](const QString &, const QString &, bool enabled) {
        CHECK(enabled);
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
                tuple.active = QList<int>{777001, 888002};
            }
        }
        return true;
    });
    module.requestShortcutApply();
    CHECK(!module.shortcutErrorText().isEmpty());
    CHECK(module.shortcutErrorText().contains(QStringLiteral("stale")));
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(cleared.stored.isEmpty());
    CHECK(errors == 1);
}

void selectionSaveIsolationWithAuthentic()
{
    // A staged Authentic intent plus a disabled row never leaks into
    // ordinary Save: zero shortcut writes, intent and draft retained.
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    wireQuietResults(module);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authentic != nullptr);
    if (authentic != nullptr) {
        authentic->click();
    }
    setRowDisabled(module, QStringLiteral("kwin/omnitiler-workspace-1"), true);
    CHECK(module.shortcutAuthenticStaged());
    CHECK(module.shortcutDisabledIds().size() == 1);
    module.save();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(cleared.clears == 0);
    CHECK(module.shortcutAuthenticStaged());
    CHECK(module.shortcutDisabledIds().size() == 1);
    module.save();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(module.shortcutAuthenticStaged());
}

void selectionAuthenticApplyConsumesPreservesLaterCustom()
{
    // Lifecycle: staged Authentic is committed by a confirmed successful
    // Apply and consumed there. A later external customization in KDE
    // Shortcuts with the still-open KCM must survive the next confirmed
    // Apply (Keep preserves with no reset).
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    wireQuietResults(module);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authentic != nullptr);
    if (authentic != nullptr) {
        authentic->click();
    }
    CHECK(module.shortcutAuthenticStaged());
    module.requestShortcutApply();
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(!module.shortcutAuthenticStaged());
    CHECK(store.totalWrites() > 0);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
    }
    // Still-open KCM: the user customizes the current native chord in KDE
    // Shortcuts (external mutation, no KCM reload, draft untouched).
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777001};
        }
    }
    const int writesAfterFirst = store.totalWrites();
    CHECK(module.shortcutDisabledIds().isEmpty());
    CHECK(!module.shortcutAuthenticStaged());
    module.requestShortcutApply();
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(!module.shortcutAuthenticStaged());
    CHECK(store.totalWrites() == writesAfterFirst);
    CHECK(cleared.saves == 0);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{777001}));
        }
    }
    CHECK(module.shortcutStatusText().contains(QStringLiteral("preserved")));
}

void selectionAuthenticForceConsumesRetainsOnFailure()
{
    // Force lifecycle: a declined modal retains the staged Authentic
    // intent; an accepted modal consumes it on success, so a later
    // external customization with the still-open KCM is preserved by the
    // next Apply.
    FakeShortcutStore store;
    seedReady(store);
    addForeignHolder(store, QStringLiteral("org.example"), QStringLiteral("other-launch"),
                     QList<int>{META_G, 999}, QList<int>{1111});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture force;
    force.allow = false;
    wireForceCapture(module, force);
    int infos = 0;
    module.setShortcutInformationHandler([&](const QString &, const QString &) { ++infos; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    CHECK(authentic != nullptr);
    if (authentic != nullptr) {
        authentic->click();
    }
    CHECK(module.shortcutAuthenticStaged());
    module.requestShortcutApply();
    CHECK(force.calls == 1);
    CHECK(store.totalWrites() == 0);
    CHECK(!module.shortcutErrorText().isEmpty());
    CHECK(module.shortcutAuthenticStaged());
    // Accepted modal consumes the staged intent on success.
    force.allow = true;
    module.requestShortcutApply();
    CHECK(force.calls == 2);
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(!module.shortcutAuthenticStaged());
    CHECK(cleared.saves == 1);
    CHECK(infos == 1);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{META_G}));
        }
        if (tuple.action == QStringLiteral("other-launch")) {
            CHECK(tuple.active == (QList<int>{999}));
        }
    }
    // Still-open KCM: external customization then a Keep Apply preserves it
    // with no additional writes and no reset.
    for (ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            tuple.active = QList<int>{777002};
        }
    }
    const int writesAfterForce = store.totalWrites();
    module.requestShortcutApply();
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(!module.shortcutAuthenticStaged());
    CHECK(store.totalWrites() == writesAfterForce);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
            CHECK(tuple.active == (QList<int>{777002}));
        }
    }
}

void dialogApplySuccessNoPreliminaryConfirm()
{
    // Direct Apply without conflicts: no preliminary confirmation, short
    // success dialog with no Settings Apply/OK instructions.
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    int confirms = 0;
    module.setShortcutConfirmHandler([&](const QString &, const QString &) {
        ++confirms;
        return true;
    });
    ForceCapture force;
    wireForceCapture(module, force);
    int infos = 0;
    QString infoTitle;
    QString infoText;
    module.setShortcutInformationHandler([&](const QString &title, const QString &text) {
        ++infos;
        infoTitle = title;
        infoText = text;
    });
    module.setShortcutErrorHandler([](const QString &, const QString &) { CHECK(false); });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    if (authentic != nullptr) {
        authentic->click();
    }
    module.requestShortcutApply();
    CHECK(confirms == 0);
    CHECK(force.calls == 0);
    CHECK(infos == 1);
    CHECK(infoTitle.contains(QStringLiteral("Apply")));
    CHECK(infoText == QStringLiteral("Shortcuts applied."));
    CHECK(!infoText.contains(QStringLiteral("active immediately")));
    CHECK(!infoText.contains(QStringLiteral("Apply or OK")));
    CHECK(module.shortcutErrorText().isEmpty());
}

void dialogConflictCompletenessAndOrder()
{
    // >5 holders across multiple chords all appear once in reconciler order
    // with chord, display names, removed/retained keys; status first matches
    // dialog first; no truncation.
    FakeShortcutStore store;
    seedReady(store);
    const QList<std::pair<QString, int>> holders = {
        {QStringLiteral("zeta-app"), META_G},
        {QStringLiteral("alpha-app"), META_M},
        {QStringLiteral("mid-app"), META_LEFT},
        {QStringLiteral("beta-app"), META_RIGHT},
        {QStringLiteral("gamma-app"), META_UP},
        {QStringLiteral("delta-app"), META_DOWN},
    };
    for (const auto &[action, key] : holders) {
        addForeignHolder(store, action, action, QList<int>{key, 9000});
        // Fix component to org.example for determinism.
        store.tuples.last().component = QStringLiteral("org.example");
    }
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture force;
    force.allow = false;
    wireForceCapture(module, force);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    if (authentic != nullptr) {
        authentic->click();
    }
    const QString statusBefore = module.shortcutStatusText();
    CHECK(statusBefore.contains(QStringLiteral("Conflict")));
    module.requestShortcutApply();
    CHECK(force.calls == 1);
    CHECK(force.enabled);
    // Every holder appears exactly once; dialog order is sorted.
    for (const auto &[action, key] : holders) {
        CHECK(force.text.contains(action));
        CHECK(force.text.contains(ShortcutReconciler::keyDisplayName(key)));
    }
    CHECK(force.text.contains(QStringLiteral("will remove")));
    CHECK(force.text.contains(QStringLiteral("keep")));
    CHECK(force.text.contains(ShortcutReconciler::keyDisplayName(9000)));
    QStringList ids;
    for (const auto &[action, key] : holders) {
        Q_UNUSED(key);
        ids.append(QStringLiteral("org.example/") + action);
    }
    // Sorted order: first dialog row matches status first conflict.
    QStringList sorted = ids;
    sorted.sort();
    const QString firstId = sorted.first();
    const QString firstAction = firstId.split(QStringLiteral("/")).last();
    CHECK(force.text.indexOf(firstAction) < force.text.indexOf(sorted.last().split(QStringLiteral("/")).last()));
    CHECK(statusBefore.contains(firstAction) || module.shortcutErrorText().contains(firstAction));
    // Row list shows every holder with no "(+N more)" truncation.
    QListWidget *list = conflictListByModule(module);
    CHECK(list != nullptr);
    if (list != nullptr) {
        bool sawTruncation = false;
        for (int i = 0; i < list->count(); ++i) {
            QListWidgetItem *item = list->item(i);
            if (item != nullptr && item->text().contains(QStringLiteral("more)"))) {
                sawTruncation = true;
            }
        }
        CHECK(!sawTruncation);
    }
}

void dialogBlockerForceDisabledWithReason()
{
    // Default-only claimant blocks Force: dialog still lists available
    // conflicts with Force disabled and an explicit reason.
    FakeShortcutStore store;
    seedReady(store);
    addForeignHolder(store, QStringLiteral("org.example"), QStringLiteral("clearable-app"),
                     QList<int>{META_G, 9000}, QList<int>{1111});
    // Default-only holder: active without the required chord, retaining
    // unrelated bindings the dialog must show as found and kept.
    ShortcutKeyHolder blocker;
    blocker.component = QStringLiteral("org.example");
    blocker.action = QStringLiteral("default-only");
    blocker.active = QList<int>{424242, 424243};
    store.extraByKey[META_CTRL_LEFT].append(blocker);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture force;
    force.allow = true;
    wireForceCapture(module, force);
    int errors = 0;
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    if (authentic != nullptr) {
        authentic->click();
    }
    module.requestShortcutApply();
    CHECK(force.calls == 1);
    CHECK(!force.enabled);
    CHECK(force.text.contains(QStringLiteral("clearable-app")));
    CHECK(force.text.contains(QStringLiteral("default-only")));
    CHECK(force.text.contains(QStringLiteral("removes none and keeps")));
    CHECK(force.text.contains(ShortcutReconciler::keyDisplayName(424242)));
    CHECK(force.text.contains(ShortcutReconciler::keyDisplayName(424243)));
    CHECK(force.text.contains(QStringLiteral("Force unavailable")));
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    // The blocked modal itself carries the reason; no redundant error.
    CHECK(errors == 0);
}

void dialogOperationalFailureErrorDialog()
{
    // Transport failure surfaces an error dialog with the reason, no force
    // prompt, zero writes.
    FakeShortcutStore store;
    seedReady(store);
    store.failByKey = true;
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture force;
    wireForceCapture(module, force);
    int errors = 0;
    QString errorText;
    module.setShortcutErrorHandler([&](const QString &, const QString &text) {
        ++errors;
        errorText = text;
    });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    if (authentic != nullptr) {
        authentic->click();
    }
    store.failByKey = true;
    module.requestShortcutApply();
    CHECK(errors == 1);
    CHECK(!errorText.isEmpty());
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
}

void dialogRevertResultDialogs()
{
    // Revert retains its data-loss confirmation, then reports success or
    // failure modally. Force runs only through the accepted conflict modal.
    FakeShortcutStore store;
    seedReady(store);
    addForeignHolder(store, QStringLiteral("org.example"), QStringLiteral("other-launch"),
                     QList<int>{META_G, 999}, QList<int>{2222});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    int confirms = 0;
    module.setShortcutConfirmHandler([&](const QString &title, const QString &) {
        ++confirms;
        CHECK(title.contains(QStringLiteral("Revert")));
        return true;
    });
    ForceCapture force;
    force.allow = true;
    wireForceCapture(module, force);
    int infos = 0;
    int errors = 0;
    QStringList infoTexts;
    module.setShortcutInformationHandler([&](const QString &, const QString &text) {
        ++infos;
        infoTexts.append(text);
    });
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    if (authentic != nullptr) {
        authentic->click();
    }
    module.requestShortcutApply();
    CHECK(force.calls == 1);
    CHECK(cleared.stored.size() == 1);
    CHECK(infos == 1);
    CHECK(infoTexts.last() == QStringLiteral("Shortcuts applied."));
    const int confirmsBeforeRevert = confirms;
    module.requestShortcutRevert();
    CHECK(confirms == confirmsBeforeRevert + 1);
    CHECK(infos == 2);
    CHECK(errors == 0);
    CHECK(infoTexts.last() == QStringLiteral("Shortcuts reverted to defaults."));
    CHECK(cleared.stored.isEmpty());
    // Failing revert reports an error dialog and retains the list.
    cleared.stored.append(ClearedAction{QStringLiteral("org.example"), QStringLiteral("missing-action")});
    module.requestShortcutRevert();
    CHECK(errors == 1);
    CHECK(!cleared.stored.isEmpty());
}

void dialogRealConflictModalForce()
{
    // Real scrollable conflict dialog offscreen: the list is read-only and
    // complete, Force Apply applies with revalidation, and the result is
    // the short success string.
    FakeShortcutStore store;
    seedReady(store);
    addForeignHolder(store, QStringLiteral("org.kde.unknown"), QStringLiteral("other-launch"),
                     QList<int>{META_G, 999}, QList<int>{1111});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    int infos = 0;
    QString infoText;
    module.setShortcutInformationHandler([&](const QString &, const QString &text) {
        ++infos;
        infoText = text;
    });
    module.setShortcutErrorHandler([](const QString &, const QString &) { CHECK(false); });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    if (authentic != nullptr) {
        authentic->click();
    }
    QTimer::singleShot(0, [&] {
        auto *dialog = module.widget()->findChild<QDialog *>(QStringLiteral("shortcutConflictDialog"));
        CHECK(dialog != nullptr);
        if (dialog == nullptr) {
            return;
        }
        auto *view = dialog->findChild<QPlainTextEdit *>(QStringLiteral("shortcutConflictText"));
        CHECK(view != nullptr);
        if (view != nullptr) {
            CHECK(view->isReadOnly());
            CHECK(view->toPlainText().contains(QStringLiteral("other-launch")));
            CHECK(view->toPlainText().contains(ShortcutReconciler::keyDisplayName(META_G)));
        }
        auto *force = dialog->findChild<QPushButton *>(QStringLiteral("shortcutConflictForceButton"));
        auto *cancel = dialog->findChild<QPushButton *>(QStringLiteral("shortcutConflictCancelButton"));
        CHECK(force != nullptr);
        CHECK(cancel != nullptr);
        if (force != nullptr) {
            CHECK(force->isEnabled());
            CHECK(force->text().contains(QStringLiteral("Force Apply")));
            force->click();
        }
    });
    module.requestShortcutApply();
    CHECK(infos == 1);
    CHECK(infoText == QStringLiteral("Shortcuts applied."));
    CHECK(!infoText.contains(QStringLiteral("active immediately")));
    CHECK(!infoText.contains(QStringLiteral("Apply or OK")));
    CHECK(module.shortcutErrorText().isEmpty());
    CHECK(cleared.saves == 1);
    for (const ShortcutTuple &tuple : store.tuples) {
        if (tuple.component == QStringLiteral("org.kde.unknown")) {
            CHECK(tuple.active == (QList<int>{999}));
        }
    }
}

void dialogRealConflictModalCancel()
{
    // Real dialog Cancel: no writes, no result dialogs either way.
    FakeShortcutStore store;
    seedReady(store);
    addForeignHolder(store, QStringLiteral("org.kde.unknown"), QStringLiteral("other-launch"),
                     QList<int>{META_G, 999}, QList<int>{1111});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    int infos = 0;
    int errors = 0;
    module.setShortcutInformationHandler([&](const QString &, const QString &) { ++infos; });
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    if (authentic != nullptr) {
        authentic->click();
    }
    QTimer::singleShot(0, [&] {
        auto *dialog = module.widget()->findChild<QDialog *>(QStringLiteral("shortcutConflictDialog"));
        CHECK(dialog != nullptr);
        if (dialog == nullptr) {
            return;
        }
        auto *cancel = dialog->findChild<QPushButton *>(QStringLiteral("shortcutConflictCancelButton"));
        CHECK(cancel != nullptr);
        if (cancel != nullptr) {
            cancel->click();
        }
    });
    module.requestShortcutApply();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(infos == 0);
    CHECK(errors == 0);
    // The refusal reason stays on the error label; the status still shows
    // the conflict honestly.
    CHECK(module.shortcutErrorText().contains(QStringLiteral("Meta+G")));
    CHECK(module.shortcutStatusText().contains(QStringLiteral("Conflict")));
}

void dialogRealBlockedModalForceDisabled()
{
    // Real blocked dialog: Force Apply is disabled with the reason in the
    // list; Cancel leaves no writes and no redundant error.
    FakeShortcutStore store;
    seedReady(store);
    addForeignHolder(store, QStringLiteral("org.example"), QStringLiteral("clearable-app"),
                     QList<int>{META_G, 9000}, QList<int>{1111});
    ShortcutKeyHolder blocker;
    blocker.component = QStringLiteral("org.example");
    blocker.action = QStringLiteral("default-only");
    blocker.active = QList<int>{424242};
    store.extraByKey[META_CTRL_LEFT].append(blocker);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    int infos = 0;
    int errors = 0;
    module.setShortcutInformationHandler([&](const QString &, const QString &) { ++infos; });
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    if (authentic != nullptr) {
        authentic->click();
    }
    QTimer::singleShot(0, [&] {
        auto *dialog = module.widget()->findChild<QDialog *>(QStringLiteral("shortcutConflictDialog"));
        CHECK(dialog != nullptr);
        if (dialog == nullptr) {
            return;
        }
        auto *view = dialog->findChild<QPlainTextEdit *>(QStringLiteral("shortcutConflictText"));
        CHECK(view != nullptr);
        if (view != nullptr) {
            CHECK(view->toPlainText().contains(QStringLiteral("clearable-app")));
            CHECK(view->toPlainText().contains(QStringLiteral("default-only")));
            CHECK(view->toPlainText().contains(QStringLiteral("Force unavailable")));
        }
        auto *force = dialog->findChild<QPushButton *>(QStringLiteral("shortcutConflictForceButton"));
        auto *cancel = dialog->findChild<QPushButton *>(QStringLiteral("shortcutConflictCancelButton"));
        CHECK(force != nullptr);
        CHECK(cancel != nullptr);
        if (force != nullptr) {
            CHECK(!force->isEnabled());
        }
        if (cancel != nullptr) {
            cancel->click();
        }
    });
    module.requestShortcutApply();
    CHECK(store.totalWrites() == 0);
    CHECK(cleared.saves == 0);
    CHECK(infos == 0);
    CHECK(errors == 0);
}

void dialogLockRelocationDescribed()
{
    // The dialog carries the actual in-scope lock relocation, the
    // authorized displacement when observed, and an honest out-of-scope
    // line otherwise. Nothing is inferred in the UI.
    {
        FakeShortcutStore store;
        seedReady(store);
        addForeignHolder(store, QStringLiteral("org.example"), QStringLiteral("other-launch"),
                         QList<int>{META_G, 999}, QList<int>{1111});
        ShortcutKeyHolder sysmon;
        sysmon.component = shortcutAuthorizedEscComponent();
        sysmon.action = shortcutAuthorizedEscAction();
        sysmon.active = QList<int>{META_ESC};
        sysmon.componentFriendly = QStringLiteral("System Monitor");
        sysmon.friendly = QStringLiteral("Launch System Monitor");
        store.extraByKey[META_ESC].append(sysmon);
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        ForceCapture force;
        force.allow = false;
        wireForceCapture(module, force);
        module.setShortcutStores(&store, &cleared);
        module.load();
        QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
        if (authentic != nullptr) {
            authentic->click();
        }
        module.requestShortcutApply();
        CHECK(force.calls == 1);
        CHECK(force.text.contains(QStringLiteral("Exempt: ksmserver/Lock Session found")));
        CHECK(force.text.contains(QStringLiteral("will move to")));
        CHECK(force.text.contains(ShortcutReconciler::keyDisplayName(META_ESC)));
        CHECK(force.text.contains(QStringLiteral("retains its binding")));
        CHECK(force.text.contains(QStringLiteral("retained but displaced")));
        CHECK(!force.text.contains(QStringLiteral("delivers to Lock Session")));
        CHECK(!force.text.contains(QStringLiteral("and stays")));
        CHECK(force.text.contains(QStringLiteral("System Monitor")));
    }
    {
        FakeShortcutStore store;
        seedReady(store);
        addForeignHolder(store, QStringLiteral("org.example"), QStringLiteral("other-launch"),
                         QList<int>{777001}, QList<int>{1111});
        for (ShortcutTuple &tuple : store.tuples) {
            if (tuple.action == QStringLiteral("omnitiler-toggle-float")) {
                tuple.active = QList<int>{777001};
            }
        }
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        ForceCapture force;
        force.allow = false;
        wireForceCapture(module, force);
        module.setShortcutStores(&store, &cleared);
        module.load();
        // Keep focus: the lock stays out of scope and the dialog says so.
        CHECK(!module.shortcutAuthenticStaged());
        module.requestShortcutApply();
        CHECK(force.calls == 1);
        CHECK(force.text.contains(QStringLiteral("Exempt: Lock Session untouched")));
    }
}

void dialogLockExemptWording()
{
    // Exempt lock wording is exact: an already-relocated lock reports
    // unchanged (never "will move to"), extra chords are named as preserved
    // alternates, and the authorized System Monitor holder is described as
    // retained but displaced without claiming physical key delivery.
    {
        ShortcutForcePreview preview;
        preview.forceable = true;
        preview.lock.inScope = true;
        preview.lock.component = QStringLiteral("ksmserver");
        preview.lock.action = QStringLiteral("Lock Session");
        preview.lock.actual = QList<int>{META_ESC};
        preview.lock.post = QList<int>{META_ESC};
        const QString text = ActiveBorderConfigModule::buildConflictDialogText(preview, true);
        CHECK(text.contains(QStringLiteral("already")));
        CHECK(text.contains(QStringLiteral("unchanged")));
        CHECK(!text.contains(QStringLiteral("will move to")));
    }
    {
        ShortcutForcePreview preview;
        preview.forceable = true;
        preview.lock.inScope = true;
        preview.lock.component = QStringLiteral("ksmserver");
        preview.lock.action = QStringLiteral("Lock Session");
        preview.lock.actual = QList<int>{META_L, 424242};
        preview.lock.post = QList<int>{META_ESC, 424242};
        const QString text = ActiveBorderConfigModule::buildConflictDialogText(preview, true);
        CHECK(text.contains(QStringLiteral("will move to")));
        CHECK(text.contains(QStringLiteral("Preserves")));
        CHECK(!text.contains(QStringLiteral("unchanged")));
    }
    {
        ShortcutForcePreview preview;
        preview.forceable = true;
        preview.lock.inScope = true;
        preview.lock.component = QStringLiteral("ksmserver");
        preview.lock.action = QStringLiteral("Lock Session");
        preview.lock.actual = QList<int>{META_L};
        preview.lock.post = QList<int>{META_ESC};
        preview.lock.authorizedHolderPresent = true;
        preview.lock.authorizedComponent = shortcutAuthorizedEscComponent();
        preview.lock.authorizedAction = shortcutAuthorizedEscAction();
        preview.lock.authorizedActive = QList<int>{META_ESC};
        preview.lock.authorizedComponentFriendly = QStringLiteral("System Monitor");
        preview.lock.authorizedFriendly = QStringLiteral("Launch System Monitor");
        const QString text = ActiveBorderConfigModule::buildConflictDialogText(preview, true);
        CHECK(text.contains(QStringLiteral("retains its binding")));
        CHECK(text.contains(QStringLiteral("retained but displaced")));
        CHECK(!text.contains(QStringLiteral("delivers to Lock Session")));
    }
}

void dialogKeyedHolderFriendlyLabels()
{
    // Keyed-only foreign holders expose wire display labels through the
    // store interface; the dialog shows them next to the IDs.
    FakeShortcutStore store;
    seedReady(store);
    ShortcutKeyHolder holder;
    holder.component = QStringLiteral("org.example");
    holder.action = QStringLiteral("keyed-app");
    holder.active = QList<int>{META_G, 9000};
    holder.componentFriendly = QStringLiteral("Example Component");
    holder.friendly = QStringLiteral("Keyed App");
    store.extraByKey[META_G].append(holder);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    ForceCapture force;
    force.allow = false;
    wireForceCapture(module, force);
    module.setShortcutStores(&store, &cleared);
    module.load();
    QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
    if (authentic != nullptr) {
        authentic->click();
    }
    module.requestShortcutApply();
    CHECK(force.calls == 1);
    CHECK(force.text.contains(QStringLiteral("org.example/keyed-app")));
    CHECK(force.text.contains(QStringLiteral("Example Component")));
    CHECK(force.text.contains(QStringLiteral("Keyed App")));
}

void dialogHistoricalIdentityLabel()
{
    // Previous OmniTiler identity (kwin/plasma-auto-tiler-*) is labeled
    // concisely in conflict rows; the exact ID and friendly info stay
    // readable and no stale-callback claim is made from the ID alone.
    {
        ShortcutForcePreview preview;
        preview.forceable = true;
        ShortcutForceMismatch mismatch;
        mismatch.component = QStringLiteral("kwin");
        mismatch.action = QStringLiteral("plasma-auto-tiler-toggle-float");
        mismatch.componentFriendly = QStringLiteral("KWin");
        mismatch.friendly = QStringLiteral("Toggle Float");
        mismatch.expectedPre = QList<int>{META_G};
        mismatch.actual = QList<int>{META_G, 9000};
        mismatch.post = QList<int>{9000};
        preview.mismatches.append(mismatch);
        const QString text = ActiveBorderConfigModule::buildConflictDialogText(preview, true);
        CHECK(text.contains(QStringLiteral("kwin/plasma-auto-tiler-toggle-float")));
        CHECK(text.contains(QStringLiteral("Toggle Float")));
        CHECK(text.contains(QStringLiteral("[previous OmniTiler identity]")));
        // The persisted ID alone implies no stale-callback status: the
        // historical row carries identity only (the header's generic
        // revalidation "stale state" line is not attached to the row).
        for (const QString &line : text.split(QStringLiteral("\n"))) {
            if (line.contains(QStringLiteral("plasma-auto-tiler-toggle-float"))) {
                CHECK(line.contains(QStringLiteral("[previous OmniTiler identity]")));
                CHECK(!line.contains(QStringLiteral("stale")));
                CHECK(!line.contains(QStringLiteral("callback")));
            }
        }
    }
    // Per-binding holder reporting reuses the same label through the row
    // list: a live historical holder of a required chord appears with the
    // previous-identity note and its exact ID.
    {
        FakeShortcutStore store;
        seedReady(store);
        ShortcutKeyHolder holder;
        holder.component = QStringLiteral("kwin");
        holder.action = QStringLiteral("plasma-auto-tiler-toggle-float");
        holder.active = QList<int>{META_G, 9000};
        holder.componentFriendly = QStringLiteral("KWin");
        holder.friendly = QStringLiteral("Toggle Float");
        store.extraByKey[META_G].append(holder);
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        ForceCapture force;
        force.allow = false;
        wireForceCapture(module, force);
        module.setShortcutStores(&store, &cleared);
        module.load();
        QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
        if (authentic != nullptr) {
            authentic->click();
        }
        module.requestShortcutApply();
        CHECK(force.calls == 1);
        CHECK(force.text.contains(QStringLiteral("kwin/plasma-auto-tiler-toggle-float")));
        CHECK(force.text.contains(QStringLiteral("[previous OmniTiler identity]")));
        for (const QString &line : force.text.split(QStringLiteral("\n"))) {
            if (line.contains(QStringLiteral("plasma-auto-tiler-toggle-float"))) {
                CHECK(!line.contains(QStringLiteral("stale")));
                CHECK(!line.contains(QStringLiteral("callback")));
            }
        }
        QListWidget *list = conflictListByModule(module);
        CHECK(list != nullptr);
        bool sawHistorical = false;
        if (list != nullptr) {
            for (int i = 0; i < list->count(); ++i) {
                QListWidgetItem *item = list->item(i);
                if (item != nullptr && item->text().contains(QStringLiteral("plasma-auto-tiler-toggle-float"))) {
                    sawHistorical = true;
                    CHECK(item->text().contains(QStringLiteral("[previous OmniTiler identity]")));
                }
            }
        }
        CHECK(sawHistorical);
    }
}

void dialogPartialApplyFailureReportsOriginal()
{
    // A failure after writes began (and an inconsistent availability
    // gate with holders present) reports the original error with no
    // Force confirmation, even though a later preview could list holders.
    {
        FakeShortcutStore store;
        seedReady(store);
        store.failWriteAt = 1;
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        ForceCapture force;
        wireForceCapture(module, force);
        int errors = 0;
        QString errorText;
        module.setShortcutErrorHandler([&](const QString &, const QString &text) {
            ++errors;
            errorText = text;
        });
        module.setShortcutStores(&store, &cleared);
        module.load();
        QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
        if (authentic != nullptr) {
            authentic->click();
        }
        module.requestShortcutApply();
        CHECK(force.calls == 0);
        CHECK(errors == 1);
        CHECK(errorText.contains(QStringLiteral("simulated project write failure")));
        CHECK(store.totalWrites() > 0);
        CHECK(cleared.saves == 0);
    }
    {
        // Holders exist but the availability gate fails: operational, not
        // a conflict refusal, so no Force modal follows.
        FakeShortcutStore store;
        seedReady(store);
        store.tuples.append(makeTuple(QStringLiteral("kwin"), QStringLiteral("Grid View"), QList<int>{META_G}));
        store.availableOverride[META_G] = true;
        FakeClearedStore cleared;
        ActiveBorderConfigModule module(nullptr, KPluginMetaData());
        module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
        ForceCapture force;
        wireForceCapture(module, force);
        int errors = 0;
        module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
        module.setShortcutStores(&store, &cleared);
        module.load();
        QPushButton *authentic = presetButtonByModule(module, "shortcutAuthenticButton");
        if (authentic != nullptr) {
            authentic->click();
        }
        module.requestShortcutApply();
        CHECK(force.calls == 0);
        CHECK(errors == 1);
        CHECK(store.totalWrites() == 0);
        CHECK(cleared.saves == 0);
    }
}

void waitForShortcutIdle(ActiveBorderConfigModule &module, int timeoutMs = 15000)
{
    QEventLoop loop;
    QTimer poll;
    poll.setInterval(5);
    QObject::connect(&poll, &QTimer::timeout, [&]() {
        if (!module.isShortcutBusy()) {
            loop.quit();
        }
    });
    QTimer watchdog;
    watchdog.setSingleShot(true);
    QObject::connect(&watchdog, &QTimer::timeout, &loop, &QEventLoop::quit);
    poll.start();
    watchdog.start(timeoutMs);
    if (module.isShortcutBusy()) {
        loop.exec();
    }
}

bool waitForStoreReads(FakeShortcutStore &store, int reads, int timeoutMs = 15000)
{
    QEventLoop loop;
    QTimer poll;
    poll.setInterval(5);
    QObject::connect(&poll, &QTimer::timeout, [&]() {
        if (store.readAllCalls >= reads) {
            loop.quit();
        }
    });
    QTimer watchdog;
    watchdog.setSingleShot(true);
    QObject::connect(&watchdog, &QTimer::timeout, &loop, &QEventLoop::quit);
    poll.start();
    watchdog.start(timeoutMs);
    if (store.readAllCalls < reads) {
        loop.exec();
    }
    return store.readAllCalls >= reads;
}

void asyncResponsiveGuardedExecution()
{
    // Production async path with a slow store: the UI event loop stays
    // responsive during discovery, a concurrent shortcut operation is
    // rejected (never queued or interleaved), a draft edit during flight
    // stages without racing store work, and destruction waits out the
    // flight instead of racing it.
    FakeShortcutStore store;
    seedReady(store);
    store.artificialReadMs = 100;
    store.artificialByKeyMs = 2;
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    int infos = 0;
    int errors = 0;
    module.setShortcutInformationHandler([&](const QString &, const QString &) { ++infos; });
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
    module.setShortcutStores(&store, &cleared);
    module.setShortcutAsyncEnabled(true);
    CHECK(!module.isShortcutBusy());
    module.refreshShortcutState();
    CHECK(module.isShortcutBusy());
    // The event loop processes timers while the worker flight runs.
    bool timerFired = false;
    QTimer probe;
    probe.setSingleShot(true);
    QObject::connect(&probe, &QTimer::timeout, [&]() { timerFired = true; });
    probe.start(25);
    // A concurrent Apply during the flight is rejected: no writes, no
    // dialogs, and the flight continues undisturbed.
    const int writesBefore = store.totalWrites();
    module.requestShortcutApply();
    CHECK(store.totalWrites() == writesBefore);
    CHECK(infos == 0);
    CHECK(errors == 0);
    CHECK(module.isShortcutBusy());
    // A draft edit during the flight stages without racing store work: the
    // list is disabled, but the staged draft is still recorded and a
    // refresh follows the flight.
    QString editedId;
    if (auto *list = module.widget()->findChild<QListWidget *>(QStringLiteral("shortcutConflictList"))) {
        if (list->count() > 0) {
            QListWidgetItem *item = list->item(0);
            editedId = item->data(Qt::UserRole).toString();
            item->setCheckState(Qt::Unchecked);
        }
    }
    CHECK(!editedId.isEmpty());
    CHECK(module.shortcutDisabledIds().contains(editedId));
    waitForShortcutIdle(module);
    CHECK(timerFired);
    CHECK(!module.isShortcutBusy());
    CHECK(!module.shortcutStatusText().startsWith(QStringLiteral("Working:")));
    CHECK(module.shortcutStatusText().contains(QStringLiteral("129 rows")));
    CHECK(module.shortcutDisabledIds().contains(editedId));
    // Destruction during a flight waits out the worker instead of racing
    // it: deleting here must not crash.
    {
        FakeShortcutStore slow;
        seedReady(slow);
        slow.artificialReadMs = 500;
        FakeClearedStore slowCleared;
        auto *dying = new ActiveBorderConfigModule(nullptr, KPluginMetaData());
        dying->setShortcutStores(&slow, &slowCleared);
        dying->setShortcutAsyncEnabled(true);
        dying->refreshShortcutState();
        CHECK(dying->isShortcutBusy());
        delete dying;
    }
    CHECK(true);
}

void asyncDoubleLoadRestageEpochFreshOnly()
{
    // Double load with inter-read drift: load#1 (idle) dispatches R1 with
    // a restage claim bound to its epoch; load#2 during R1 clears the draft
    // and re-claims. The row empty in R1's image is rebound before R2
    // reads, and a different row is emptied. The final draft must be
    // exactly the fresh set: R1's superseded claim stages nothing and R2
    // replaces (never unions) the old Disable.
    FakeShortcutStore store;
    seedReady(store);
    store.artificialByKeyMs = 5; // slow the holder-scan phase, not the first read
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutStores(&store, &cleared);
    module.setShortcutAsyncEnabled(true);
    CHECK(module.shortcutDisabledIds().isEmpty());
    // Two distinct bound project rows with canonical chords.
    QStringList boundIds;
    for (const ShortcutCatalogEntry &entry : shortcutProjectCatalog()) {
        if (entry.component == QStringLiteral("kwin") && entry.action.startsWith(QStringLiteral("omnitiler-"))
            && entry.canonicalKey != 0) {
            boundIds.append(shortcutCatalogId(entry.component, entry.action));
            if (boundIds.size() == 2) {
                break;
            }
        }
    }
    CHECK(boundIds.size() == 2);
    const QString idA = boundIds.at(0);
    const QString idB = boundIds.at(1);
    QList<int> activeA;
    for (const ShortcutTuple &tuple : store.tuples) {
        if (shortcutCatalogId(tuple.component, tuple.action) == idA) {
            activeA = tuple.active;
        }
    }
    CHECK(!activeA.isEmpty());
    for (ShortcutTuple &tuple : store.tuples) {
        if (shortcutCatalogId(tuple.component, tuple.action) == idA) {
            tuple.active.clear();
        }
    }
    const int readsBefore = store.readAllCalls;
    module.load(); // load#1, idle: clears the draft, claims, dispatches R1
    CHECK(module.isShortcutBusy());
    // Wait until R1 has captured its tuple image (A empty, B bound), then
    // drift: rebind A, empty B. Only the post-load#2 refresh may stage.
    CHECK(waitForStoreReads(store, readsBefore + 1));
    for (ShortcutTuple &tuple : store.tuples) {
        if (shortcutCatalogId(tuple.component, tuple.action) == idA) {
            tuple.active = activeA;
        }
        if (shortcutCatalogId(tuple.component, tuple.action) == idB) {
            tuple.active.clear();
        }
    }
    module.load(); // load#2, during R1: clears the draft, re-claims, defers
    CHECK(module.isShortcutBusy());
    waitForShortcutIdle(module);
    CHECK(!module.isShortcutBusy());
    CHECK(module.shortcutDisabledIds() == QStringList(idB));
}

void asyncApplyModalQueuedRefreshThenForce()
{
    // Async Apply -> conflict preview modal -> refresh requested from the
    // prompt -> accepted Force must still run to a result (never silently
    // dropped), with reads and mutation phases on the worker thread.
    FakeShortcutStore store;
    seedReady(store);
    store.artificialReadMs = 20;
    store.artificialByKeyMs = 1;
    store.artificialWriteMs = 10;
    addForeignHolder(store, QStringLiteral("org.example"), QStringLiteral("other-launch"),
                     QList<int>{META_G, 999}, QList<int>{META_G});
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    int infos = 0;
    int errors = 0;
    module.setShortcutInformationHandler([&](const QString &, const QString &) { ++infos; });
    module.setShortcutErrorHandler([&](const QString &, const QString &) { ++errors; });
    bool promptSawBusy = false;
    module.setShortcutForcePromptHandler([&](const QString &, const QString &, bool enabled) {
        promptSawBusy = module.isShortcutBusy();
        CHECK(enabled);
        // A refresh requested from the prompt must defer to pending while
        // the modal holds the guard; it must not preempt the accepted Force.
        module.refreshShortcutState();
        return true; // accept Force
    });
    module.setShortcutStores(&store, &cleared);
    // Stage Authentic synchronously so the foreign Meta+G holder conflicts.
    if (auto *authentic = module.widget()->findChild<QPushButton *>(QStringLiteral("shortcutAuthenticButton"))) {
        authentic->click();
    }
    CHECK(module.shortcutAuthenticStaged());
    module.setShortcutAsyncEnabled(true);
    // Scope the affinity assertions to the async flights below: the
    // synchronous staging above ran on the UI thread by design.
    store.readThreadIds.clear();
    store.writeThreadIds.clear();
    const Qt::HANDLE uiThread = QThread::currentThreadId();
    module.requestShortcutApply();
    CHECK(module.isShortcutBusy());
    bool timerFired = false;
    QTimer probe;
    probe.setSingleShot(true);
    QObject::connect(&probe, &QTimer::timeout, [&]() { timerFired = true; });
    probe.start(25);
    waitForShortcutIdle(module);
    CHECK(timerFired);
    CHECK(promptSawBusy);
    // Accepted Force ran to a result: one foreign clear plus the project
    // assignment, cleared-ID persisted, success dialog shown.
    CHECK(infos == 1);
    CHECK(errors == 0);
    CHECK(store.foreignWriteLog.size() == 1);
    CHECK(store.foreignWriteLog.at(0).keys == (QList<int>{999}));
    CHECK(cleared.stored.size() == 1);
    CHECK(!store.writeThreadIds.isEmpty());
    for (Qt::HANDLE thread : store.writeThreadIds) {
        CHECK(thread != uiThread);
    }
    CHECK(!store.readThreadIds.isEmpty());
    for (Qt::HANDLE thread : store.readThreadIds) {
        CHECK(thread != uiThread);
    }
    CHECK(!module.isShortcutBusy());
    CHECK(module.shortcutStatusText().contains(QStringLiteral("129 rows")));
}

void dialogErrorBriefSingleLineCapped()
{
    // Error rendering boundary: a multiline long reason reaches the
    // handler as one single-line string capped at 240 characters.
    FakeShortcutStore store;
    seedReady(store);
    FakeClearedStore cleared;
    ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.setShortcutConfirmHandler([](const QString &, const QString &) { return false; });
    module.setShortcutStores(&store, &cleared);
    module.load();
    QString seen;
    module.setShortcutErrorHandler([&](const QString &, const QString &text) { seen = text; });
    QString longReason = QStringLiteral("first line\nsecond\tline  with   extra   spaces\n");
    while (longReason.size() < 400) {
        longReason += QStringLiteral("0123456789 detail ");
    }
    module.showShortcutError(QStringLiteral("Apply Shortcuts"), longReason);
    CHECK(!seen.isEmpty());
    CHECK(!seen.contains(QStringLiteral("\n")));
    CHECK(!seen.contains(QStringLiteral("\t")));
    CHECK(!seen.contains(QStringLiteral("  ")));
    CHECK(seen.size() <= 240);
    CHECK(seen.endsWith(QStringLiteral("...")));
    // Short reasons pass through simplified but untruncated.
    module.showShortcutError(QStringLiteral("Apply Shortcuts"), QStringLiteral("reconciler is not configured"));
    CHECK(seen == QStringLiteral("reconciler is not configured"));
}

} // namespace

int main(int argc, char **argv)
{
    qputenv("DBUS_SESSION_BUS_ADDRESS", QByteArray("unix:path=/dev/null/omnitiler-kcm-test-isolated-bus"));
    QTemporaryDir configHome;
    if (!configHome.isValid()) {
        std::fprintf(stderr, "failed to create temporary config directory\n");
        return EXIT_FAILURE;
    }
    qputenv("XDG_CONFIG_HOME", configHome.path().toUtf8());
    QApplication app(argc, argv);
    app.clipboard()->setMimeData(new QMimeData);

    if (argc != 2) {
        std::fprintf(stderr, "usage: %s ordinary|recovery|confirm|state|force|migration|selection\n", argv[0]);
        return EXIT_FAILURE;
    }

    const QString scenario = QString::fromLocal8Bit(argv[1]);
    if (scenario == QStringLiteral("ordinary")) {
        ordinarySettingsApplyNeverMutatesShortcuts();
    } else if (scenario == QStringLiteral("recovery")) {
        recoveryVisibilityAndRouting();
    } else if (scenario == QStringLiteral("confirm")) {
        confirmationGatesEveryMutation();
    } else if (scenario == QStringLiteral("force")) {
        forcePreviewAcceptCancelRevert();
        terminalFailureLogIncludesReason();
    } else if (scenario == QStringLiteral("migration")) {
        legacyMigrationDeferredFromOpen();
    } else if (scenario == QStringLiteral("state")) {
        stateAndErrorPresentation();
        knownForeignStatusAlignsWithApply();
        growArrowStatusAlignsWithApply();
        focusMoveArrowStatusAlignsWithApply();
    } else if (scenario == QStringLiteral("selection")) {
        selectionPresetsAndDraft();
        selectionDisabledApplyIgnoresForeign();
        selectionDraftEditCancelsPreview();
        selectionStaleDraftRefusesForce();
        selectionSaveIsolationWithDraft();
        selectionRevertKeepsOwnAssignment();
        selectionLiveDisabledReopen();
        selectionMissingEnabledStatus();
        selectionDisabledLockAbsentShowsRows();
        selectionCompatibleQueryFailurePreservesDraft();
        selectionForeignDefaultCompatibleAfterClear();
        selectionReadableRowText();
        selectionKeepPreservesCustomCanonicalEmpty();
        selectionAuthenticAssignsCanonical();
        selectionCompatibleKeepsCustomNonconflict();
        selectionKeepCustomConflictPreviewForce();
        selectionForceRevalidationKeepDrift();
        selectionSaveIsolationWithAuthentic();
        selectionAuthenticApplyConsumesPreservesLaterCustom();
        selectionAuthenticForceConsumesRetainsOnFailure();
        asyncResponsiveGuardedExecution();
        asyncDoubleLoadRestageEpochFreshOnly();
        asyncApplyModalQueuedRefreshThenForce();
        dialogApplySuccessNoPreliminaryConfirm();
        dialogConflictCompletenessAndOrder();
        dialogBlockerForceDisabledWithReason();
        dialogOperationalFailureErrorDialog();
        dialogRevertResultDialogs();
        dialogRealConflictModalForce();
        dialogRealConflictModalCancel();
        dialogRealBlockedModalForceDisabled();
        dialogPartialApplyFailureReportsOriginal();
        dialogLockRelocationDescribed();
        dialogLockExemptWording();
        dialogKeyedHolderFriendlyLabels();
        dialogHistoricalIdentityLabel();
        dialogErrorBriefSingleLineCapped();
    } else {
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
