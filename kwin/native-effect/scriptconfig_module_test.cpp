#include "scriptconfig_module.h"

#include <KConfigGroup>
#include <KPluginMetaData>
#include <KSharedConfig>

#include <QApplication>
#include <QClipboard>
#include <QComboBox>
#include <QDoubleSpinBox>
#include <QLabel>
#include <QMessageLogContext>
#include <QMimeData>
#include <QPushButton>
#include <QSpinBox>
#include <QStringList>
#include <QTemporaryDir>
#include <QWidget>

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

KConfigGroup scriptGroup()
{
    return KConfigGroup(KSharedConfig::openConfig(QStringLiteral("kwinrc")), QStringLiteral("Script-omnitiler-kwin"));
}

KConfigGroup borderGroup()
{
    return KConfigGroup(KSharedConfig::openConfig(QStringLiteral("kwinrc")), QStringLiteral("Effect-omnitiler-active-border"));
}

QComboBox *workspaceModeCombo(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QComboBox *>(QStringLiteral("workspaceModeCombo"));
}

QComboBox *sameAxisMoveCombo(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QComboBox *>(QStringLiteral("sameAxisMoveCombo"));
}

QComboBox *fixedSizePredicateCombo(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QComboBox *>(QStringLiteral("fixedSizePredicateCombo"));
}

QComboBox *bornMaximizedCombo(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QComboBox *>(QStringLiteral("bornMaximizedCombo"));
}

QComboBox *migrationSourceRefillCombo(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QComboBox *>(QStringLiteral("migrationSourceRefillCombo"));
}

QComboBox *shortcutProfileCombo(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QComboBox *>(QStringLiteral("shortcutProfileCombo"));
}

QSpinBox *innerGapSpinBox(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QSpinBox *>(QStringLiteral("innerGapSpinBox"));
}

QSpinBox *outerGapSpinBox(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QSpinBox *>(QStringLiteral("outerGapSpinBox"));
}

QDoubleSpinBox *borderWidthSpinBox(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QDoubleSpinBox *>(QStringLiteral("kcfg_BorderWidth"));
}

QLabel *scriptStatusLabel(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QLabel *>(QStringLiteral("scriptStatusLabel"));
}

QString storedWorkspaceMode()
{
    return scriptGroup().readEntry(QStringLiteral("workspaceMode"), QStringLiteral("per-output-local"));
}

QString storedSameAxisMove()
{
    return scriptGroup().readEntry(QStringLiteral("sameAxisMove"), QStringLiteral("group-with-neighbor"));
}

QString otherSameAxisMove(const QString &current)
{
    if (current == QStringLiteral("swap-with-neighbor")) {
        return QStringLiteral("group-with-neighbor");
    }
    return QStringLiteral("swap-with-neighbor");
}

QString storedFixedSizePredicate()
{
    return scriptGroup().readEntry(QStringLiteral("fixedSizePredicate"), QStringLiteral("both-axes-fixed"));
}

QString otherFixedSizePredicate(const QString &current)
{
    if (current == QStringLiteral("either-axis-fixed")) {
        return QStringLiteral("both-axes-fixed");
    }
    return QStringLiteral("either-axis-fixed");
}

QString storedBornMaximized()
{
    return scriptGroup().readEntry(QStringLiteral("bornMaximized"), QStringLiteral("tile-on-launch"));
}

QString otherBornMaximized(const QString &current)
{
    if (current == QStringLiteral("keep-maximized-overlay")) {
        return QStringLiteral("tile-on-launch");
    }
    return QStringLiteral("keep-maximized-overlay");
}

QString storedMigrationSourceRefill()
{
    return scriptGroup().readEntry(QStringLiteral("migrationSourceRefill"), QStringLiteral("last-remaining-workspace"));
}

QString otherMigrationSourceRefill(const QString &current)
{
    if (current == QStringLiteral("most-recently-used-workspace")) {
        return QStringLiteral("last-remaining-workspace");
    }
    return QStringLiteral("most-recently-used-workspace");
}

QString storedShortcutProfile()
{
    return scriptGroup().readEntry(QStringLiteral("shortcutProfile"), QStringLiteral("cosmic"));
}

QString otherWorkspaceMode(const QString &current)
{
    const QStringList candidates = {
        QStringLiteral("per-output-local"),
        QStringLiteral("shared"),
    };
    for (const QString &candidate : candidates) {
        if (candidate != current) {
            return candidate;
        }
    }
    return QStringLiteral("shared");
}

bool containsRestartRequirement(const QString &text)
{
    return text.contains(QStringLiteral("Session restart required"))
        || text.contains(QStringLiteral("session restart remains required"))
        || text.contains(QStringLiteral("Session restart remains required"));
}

bool containsAppliedClaim(const QString &text)
{
    return text.toLower().contains(QStringLiteral("applied"));
}

class CountingScriptModule : public KWin::ScriptConfigModule
{
public:
    using KWin::ScriptConfigModule::ScriptConfigModule;
    bool requestScriptReconfigure() override
    {
        ++scriptCalls;
        return scriptSucceed;
    }
    int scriptCalls = 0;
    bool scriptSucceed = false;
};

class CombinedReconfigureModule : public KWin::ScriptConfigModule
{
public:
    using KWin::ScriptConfigModule::ScriptConfigModule;
    bool requestScriptReconfigure() override
    {
        ++scriptCalls;
        return scriptSucceed;
    }
    bool requestEffectReconfigure() override
    {
        ++effectCalls;
        return effectSucceed;
    }
    int scriptCalls = 0;
    bool scriptSucceed = false;
    int effectCalls = 0;
    bool effectSucceed = false;
};

void scriptReconfigureTargetIsExact()
{
    CHECK(KWin::ScriptConfigModule::scriptService() == QStringLiteral("org.kde.KWin"));
    CHECK(KWin::ScriptConfigModule::scriptPath() == QStringLiteral("/KWin"));
    CHECK(KWin::ScriptConfigModule::scriptInterface() == QStringLiteral("org.kde.KWin"));
    CHECK(KWin::ScriptConfigModule::scriptMethod() == QStringLiteral("reconfigure"));
}

KConfigGroup windowsGroup()
{
    return KConfigGroup(KSharedConfig::openConfig(QStringLiteral("kwinrc")), QStringLiteral("Windows"));
}

QLabel *windowConflictStatusLabel(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QLabel *>(QStringLiteral("windowConflictStatusLabel"));
}

QLabel *windowConflictErrorLabel(KWin::ScriptConfigModule &module)
{
    return module.widget()->findChild<QLabel *>(QStringLiteral("windowConflictErrorLabel"));
}

QPushButton *windowButton(KWin::ScriptConfigModule &module, const char *name)
{
    return module.widget()->findChild<QPushButton *>(QString::fromUtf8(name));
}

QLabel *windowLabel(KWin::ScriptConfigModule &module, const char *name)
{
    return module.widget()->findChild<QLabel *>(QString::fromUtf8(name));
}

QWidget *windowRow(KWin::ScriptConfigModule &module, const char *name)
{
    return module.widget()->findChild<QWidget *>(QString::fromUtf8(name));
}

bool windowVisible(QWidget *widget)
{
    return widget != nullptr && !widget->isHidden();
}

void seedWindows(bool tiling, bool maximize, int borders)
{
    KConfigGroup group = windowsGroup();
    group.writeEntry(QStringLiteral("ElectricBorderTiling"), tiling);
    group.writeEntry(QStringLiteral("ElectricBorderMaximize"), maximize);
    group.writeEntry(QStringLiteral("ElectricBorders"), borders);
    group.sync();
    KSharedConfig::openConfig(QStringLiteral("kwinrc"))->sync();
}

void clearWindowsKeys()
{
    KConfigGroup group = windowsGroup();
    group.deleteEntry(QStringLiteral("ElectricBorderTiling"));
    group.deleteEntry(QStringLiteral("ElectricBorderMaximize"));
    group.deleteEntry(QStringLiteral("ElectricBorders"));
    group.sync();
    KSharedConfig::openConfig(QStringLiteral("kwinrc"))->sync();
}

void windowRowAndButtonStates()
{
    // Bool rows are always visible with exactly one button; the borders row
    // shows only when nonzero. Ordinary Save leaves [Windows] keys alone.
    seedWindows(true, true, 2);
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        module.load();
        CHECK(windowConflictStatusLabel(module) != nullptr);
        CHECK(windowConflictErrorLabel(module) != nullptr);
        CHECK(windowConflictErrorLabel(module)->text().isEmpty());
        CHECK(windowVisible(windowRow(module, "windowTilingRow")));
        CHECK(windowVisible(windowRow(module, "windowMaximizeRow")));
        CHECK(windowVisible(windowRow(module, "windowBordersRow")));
        CHECK(windowVisible(windowButton(module, "windowTilingFixButton")));
        CHECK(!windowVisible(windowButton(module, "windowTilingRevertButton")));
        CHECK(windowVisible(windowButton(module, "windowMaximizeFixButton")));
        CHECK(!windowVisible(windowButton(module, "windowMaximizeRevertButton")));
        CHECK(windowVisible(windowButton(module, "windowBordersFixButton")));
        CHECK(windowConflictStatusLabel(module)->text().contains(QStringLiteral("3")));
        CHECK(windowLabel(module, "windowTilingLabel")->text().contains(QStringLiteral("Tiling: on")));
        CHECK(windowLabel(module, "windowMaximizeLabel")->text().contains(QStringLiteral("Maximize: on")));
        CHECK(windowLabel(module, "windowBordersLabel")->text().contains(QStringLiteral("Borders: 2")));
        CHECK(!module.needsSave());
        module.scriptSucceed = true;
        module.save();
        CHECK(windowsGroup().readEntry(QStringLiteral("ElectricBorderTiling"), false));
        CHECK(windowsGroup().readEntry(QStringLiteral("ElectricBorderMaximize"), false));
        CHECK(windowsGroup().readEntry(QStringLiteral("ElectricBorders"), -1) == 2);
        CHECK(!module.needsSave());
    }
    // Tiler-friendly: bools offer only Revert, borders at default hides.
    seedWindows(false, false, 0);
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        module.load();
        CHECK(windowVisible(windowRow(module, "windowTilingRow")));
        CHECK(windowVisible(windowRow(module, "windowMaximizeRow")));
        CHECK(!windowVisible(windowRow(module, "windowBordersRow")));
        CHECK(!windowVisible(windowButton(module, "windowTilingFixButton")));
        CHECK(windowVisible(windowButton(module, "windowTilingRevertButton")));
        CHECK(!windowVisible(windowButton(module, "windowMaximizeFixButton")));
        CHECK(windowVisible(windowButton(module, "windowMaximizeRevertButton")));
        CHECK(windowConflictStatusLabel(module)->text().contains(QStringLiteral("No window edge conflicts")));
        CHECK(windowLabel(module, "windowTilingLabel")->text().contains(QStringLiteral("Tiling: off")));
        CHECK(windowLabel(module, "windowMaximizeLabel")->text().contains(QStringLiteral("Maximize: off")));
    }
    // Missing keys read back as the KDE defaults (true/true/0): the two
    // booleans conflict, the integer does not.
    clearWindowsKeys();
    {
        KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
        module.load();
        CHECK(windowVisible(windowButton(module, "windowTilingFixButton")));
        CHECK(!windowVisible(windowButton(module, "windowTilingRevertButton")));
        CHECK(windowVisible(windowButton(module, "windowMaximizeFixButton")));
        CHECK(!windowVisible(windowButton(module, "windowMaximizeRevertButton")));
        CHECK(!windowVisible(windowRow(module, "windowBordersRow")));
        CHECK(windowConflictStatusLabel(module)->text().contains(QStringLiteral("2")));
    }
    // Single conflict flips only its own button.
    seedWindows(false, true, 0);
    {
        KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
        module.load();
        CHECK(!windowVisible(windowButton(module, "windowTilingFixButton")));
        CHECK(windowVisible(windowButton(module, "windowTilingRevertButton")));
        CHECK(windowVisible(windowButton(module, "windowMaximizeFixButton")));
        CHECK(!windowVisible(windowButton(module, "windowMaximizeRevertButton")));
    }
    clearWindowsKeys();
}

void windowFixPersistsAndRevertDeletesKey()
{
    seedWindows(true, true, 0);
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    const bool needsSaveBefore = module.needsSave();
    module.scriptSucceed = true;

    module.requestWindowFix(QStringLiteral("ElectricBorderTiling"));
    CHECK(windowsGroup().readEntry(QStringLiteral("ElectricBorderTiling"), true) == false);
    CHECK(windowConflictErrorLabel(module)->text().isEmpty());
    CHECK(module.needsSave() == needsSaveBefore);
    CHECK(!windowVisible(windowButton(module, "windowTilingFixButton")));
    CHECK(windowVisible(windowButton(module, "windowTilingRevertButton")));
    CHECK(windowLabel(module, "windowTilingLabel")->text().contains(QStringLiteral("Tiling: off")));

    module.requestWindowFix(QStringLiteral("ElectricBorderMaximize"));
    CHECK(windowsGroup().readEntry(QStringLiteral("ElectricBorderMaximize"), true) == false);
    CHECK(windowConflictErrorLabel(module)->text().isEmpty());
    CHECK(windowConflictStatusLabel(module)->text().contains(QStringLiteral("No window edge conflicts")));
    CHECK(module.needsSave() == needsSaveBefore);

    // Readback into a fresh page reflects the persisted values.
    {
        KWin::ScriptConfigModule reloaded(nullptr, KPluginMetaData());
        reloaded.load();
        CHECK(!windowVisible(windowButton(reloaded, "windowTilingFixButton")));
        CHECK(windowVisible(windowButton(reloaded, "windowTilingRevertButton")));
        CHECK(windowLabel(reloaded, "windowTilingLabel")->text().contains(QStringLiteral("Tiling: off")));
    }

    // Revert deletes the local key so the KDE default (true) takes effect.
    module.requestWindowRevert(QStringLiteral("ElectricBorderTiling"));
    CHECK(!windowsGroup().hasKey(QStringLiteral("ElectricBorderTiling")));
    CHECK(windowsGroup().readEntry(QStringLiteral("ElectricBorderTiling"), true));
    CHECK(windowConflictErrorLabel(module)->text().isEmpty());
    CHECK(windowVisible(windowButton(module, "windowTilingFixButton")));
    CHECK(!windowVisible(windowButton(module, "windowTilingRevertButton")));
    CHECK(windowLabel(module, "windowTilingLabel")->text().contains(QStringLiteral("Tiling: on")));
    CHECK(module.needsSave() == needsSaveBefore);
    // The untouched key is preserved.
    CHECK(windowsGroup().readEntry(QStringLiteral("ElectricBorderMaximize"), true) == false);
    clearWindowsKeys();
}

void windowBordersFixDeletesKey()
{
    seedWindows(false, false, 4);
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    CHECK(windowVisible(windowRow(module, "windowBordersRow")));
    const bool needsSaveBefore = module.needsSave();
    module.scriptSucceed = true;
    module.requestWindowFix(QStringLiteral("ElectricBorders"));
    CHECK(!windowsGroup().hasKey(QStringLiteral("ElectricBorders")));
    CHECK(windowsGroup().readEntry(QStringLiteral("ElectricBorders"), 0) == 0);
    CHECK(windowConflictErrorLabel(module)->text().isEmpty());
    CHECK(!windowVisible(windowRow(module, "windowBordersRow")));
    CHECK(windowConflictStatusLabel(module)->text().contains(QStringLiteral("No window edge conflicts")));
    CHECK(module.needsSave() == needsSaveBefore);
    {
        KWin::ScriptConfigModule reloaded(nullptr, KPluginMetaData());
        reloaded.load();
        CHECK(!windowVisible(windowRow(reloaded, "windowBordersRow")));
    }
    clearWindowsKeys();
}

void windowFailedSendShowsError()
{
    seedWindows(true, false, 0);
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    module.scriptSucceed = false;
    module.requestWindowFix(QStringLiteral("ElectricBorderTiling"));
    CHECK(!windowConflictErrorLabel(module)->text().isEmpty());
    CHECK(windowConflictErrorLabel(module)->text().contains(QStringLiteral("ElectricBorderTiling")));
    CHECK(module.needsSave() == false);
    clearWindowsKeys();
}

QStringList capturedWindowConflictMessages;

void captureWindowConflictMessage(QtMsgType, const QMessageLogContext &, const QString &message)
{
    capturedWindowConflictMessages.append(message);
}

void windowOperationsLogOneLinePerOperation()
{
    seedWindows(true, false, 0);
    capturedWindowConflictMessages.clear();
    const QtMessageHandler previous = qInstallMessageHandler(captureWindowConflictMessage);
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        module.load();
        module.scriptSucceed = true;
        module.requestWindowFix(QStringLiteral("ElectricBorderTiling"));
        module.requestWindowRevert(QStringLiteral("ElectricBorderTiling"));
    }
    qInstallMessageHandler(previous);
    int fixLogs = 0;
    int revertLogs = 0;
    for (const QString &message : capturedWindowConflictMessages) {
        if (!message.contains(QStringLiteral("op="))) {
            continue;
        }
        if (message.contains(QStringLiteral("op=fix")) && message.contains(QStringLiteral("setting=ElectricBorderTiling"))) {
            ++fixLogs;
            CHECK(message.contains(QStringLiteral("outcome=ok")));
            CHECK(message.contains(QStringLiteral("reason=ok")));
        }
        if (message.contains(QStringLiteral("op=revert"))
            && message.contains(QStringLiteral("setting=ElectricBorderTiling"))) {
            ++revertLogs;
            CHECK(message.contains(QStringLiteral("outcome=ok")));
            CHECK(message.contains(QStringLiteral("reason=ok")));
        }
        // Short single line: no config paths leak into the log.
        CHECK(!message.contains(QStringLiteral("/home")));
        CHECK(!message.contains(QStringLiteral(".config")));
    }
    CHECK(fixLogs == 1);
    CHECK(revertLogs == 1);
    clearWindowsKeys();
}

void gapContractNormalizesBoundsAndPersists()
{
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("innerGap"), QStringLiteral("not-a-number"));
        group.writeEntry(QStringLiteral("outerGap"), QStringLiteral("-5"));
        group.sync();
    }

    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        QSpinBox *inner = innerGapSpinBox(module);
        QSpinBox *outer = outerGapSpinBox(module);
        CHECK(inner != nullptr);
        CHECK(outer != nullptr);
        module.load();
        if (inner) {
            CHECK(inner->value() == 8);
        }
        if (outer) {
            CHECK(outer->value() == 8);
        }
        module.scriptSucceed = true;
        module.save();
        CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == 8);
        CHECK(scriptGroup().readEntry(QStringLiteral("outerGap"), -1) == 8);
    }

    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("innerGap"), QStringLiteral("0"));
        group.writeEntry(QStringLiteral("outerGap"), QStringLiteral("64"));
        group.sync();
    }

    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        QSpinBox *inner = innerGapSpinBox(module);
        QSpinBox *outer = outerGapSpinBox(module);
        CHECK(inner != nullptr);
        CHECK(outer != nullptr);
        module.load();
        if (inner) {
            CHECK(inner->value() == 0);
        }
        if (outer) {
            CHECK(outer->value() == 64);
        }
        if (!inner || !outer) {
            return;
        }
        module.scriptSucceed = true;
        inner->setValue(64);
        outer->setValue(0);
        module.save();
        CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == 64);
        CHECK(scriptGroup().readEntry(QStringLiteral("outerGap"), -1) == 0);
        CHECK(module.scriptCalls == 1);
    }

    {
        KWin::ScriptConfigModule reloaded(nullptr, KPluginMetaData());
        reloaded.load();
        QSpinBox *inner = innerGapSpinBox(reloaded);
        QSpinBox *outer = outerGapSpinBox(reloaded);
        CHECK(inner != nullptr);
        CHECK(outer != nullptr);
        if (inner) {
            CHECK(inner->value() == 64);
        }
        if (outer) {
            CHECK(outer->value() == 0);
        }
    }

    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("innerGap"), QStringLiteral("65"));
        group.writeEntry(QStringLiteral("outerGap"), QStringLiteral("100"));
        group.sync();
    }

    {
        KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
        QSpinBox *inner = innerGapSpinBox(module);
        QSpinBox *outer = outerGapSpinBox(module);
        CHECK(inner != nullptr);
        CHECK(outer != nullptr);
        module.load();
        if (inner) {
            CHECK(inner->value() == 8);
        }
        if (outer) {
            CHECK(outer->value() == 8);
        }
    }

    {
        KConfigGroup group = scriptGroup();
        group.deleteEntry(QStringLiteral("innerGap"));
        group.deleteEntry(QStringLiteral("outerGap"));
        group.sync();
    }

    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        QSpinBox *inner = innerGapSpinBox(module);
        QSpinBox *outer = outerGapSpinBox(module);
        CHECK(inner != nullptr);
        CHECK(outer != nullptr);
        module.load();
        if (inner) {
            CHECK(inner->value() == 8);
        }
        if (outer) {
            CHECK(outer->value() == 8);
        }
        if (!inner || !outer) {
            return;
        }
        module.scriptSucceed = true;
        module.save();
        CHECK(!scriptGroup().hasKey(QStringLiteral("innerGap")));
        CHECK(!scriptGroup().hasKey(QStringLiteral("outerGap")));
        CHECK(module.scriptCalls == 0);

        inner->setValue(12);
        outer->setValue(20);
        module.save();
        CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == 12);
        CHECK(scriptGroup().readEntry(QStringLiteral("outerGap"), -1) == 20);

        module.defaults();
        CHECK(inner->value() == 8);
        CHECK(outer->value() == 8);
        module.save();
        CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == 8);
        CHECK(scriptGroup().readEntry(QStringLiteral("outerGap"), -1) == 8);
    }
}

void sameAxisMoveContractDefaultsValidatesAndPersists()
{
    // Invalid stored values normalize to the group-with-neighbor default.
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("sameAxisMove"), QStringLiteral("bogus"));
        group.sync();
    }
    {
        KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
        QComboBox *combo = sameAxisMoveCombo(module);
        CHECK(combo != nullptr);
        module.load();
        if (combo) {
            CHECK(combo->currentData().toString() == QStringLiteral("group-with-neighbor"));
        }
    }
    // Missing keys stay missing through load and default back to group-with-neighbor.
    {
        KConfigGroup group = scriptGroup();
        group.deleteEntry(QStringLiteral("sameAxisMove"));
        group.sync();
    }
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        QComboBox *combo = sameAxisMoveCombo(module);
        CHECK(combo != nullptr);
        module.load();
        if (combo) {
            CHECK(combo->currentData().toString() == QStringLiteral("group-with-neighbor"));
        }
        module.scriptSucceed = true;
        module.save();
        CHECK(!scriptGroup().hasKey(QStringLiteral("sameAxisMove")));
        CHECK(module.scriptCalls == 0);
        CHECK(!module.isScriptRestartRequired());
    }
    // Retired pre-release tokens normalize to the default (no aliases).
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("sameAxisMove"), QStringLiteral("cosmic-wrap"));
        group.sync();
    }
    {
        KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
        QComboBox *combo = sameAxisMoveCombo(module);
        CHECK(combo != nullptr);
        module.load();
        if (combo) {
            CHECK(combo->currentData().toString() == QStringLiteral("group-with-neighbor"));
        }
    }
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("sameAxisMove"), QStringLiteral("flat-swap"));
        group.sync();
    }
    {
        KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
        QComboBox *combo = sameAxisMoveCombo(module);
        CHECK(combo != nullptr);
        module.load();
        if (combo) {
            CHECK(combo->currentData().toString() == QStringLiteral("group-with-neighbor"));
        }
    }
    {
        KConfigGroup group = scriptGroup();
        group.deleteEntry(QStringLiteral("sameAxisMove"));
        group.sync();
    }
    // A same-axis change persists, sends one live reconfigure, and never
    // requires a session restart.
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        module.load();
        QComboBox *combo = sameAxisMoveCombo(module);
        CHECK(combo != nullptr);
        if (!combo) {
            return;
        }
        const QString target = otherSameAxisMove(storedSameAxisMove());
        const int index = combo->findData(target);
        CHECK(index >= 0);
        combo->setCurrentIndex(index);
        CHECK(module.needsSave());
        module.scriptSucceed = true;
        module.save();
        CHECK(storedSameAxisMove() == target);
        CHECK(module.scriptCalls == 1);
        CHECK(!module.isScriptRestartRequired());
        CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
        CHECK(!containsAppliedClaim(module.scriptStatusText()));
        CHECK(!module.needsSave());
        // A follow-up unchanged save must not send again.
        module.save();
        CHECK(module.scriptCalls == 1);
        // Defaults restore group-with-neighbor.
        module.defaults();
        CHECK(combo->currentData().toString() == QStringLiteral("group-with-neighbor"));
        module.save();
        CHECK(storedSameAxisMove() == QStringLiteral("group-with-neighbor"));
    }
}

void fixedSizePredicateContractDefaultsValidatesAndPersists()
{
    // Invalid stored values normalize to the both-axes-fixed default.
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("fixedSizePredicate"), QStringLiteral("bogus"));
        group.sync();
    }
    {
        KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
        QComboBox *combo = fixedSizePredicateCombo(module);
        CHECK(combo != nullptr);
        module.load();
        if (combo) {
            CHECK(combo->currentData().toString() == QStringLiteral("both-axes-fixed"));
        }
    }
    // Missing keys stay missing through load and default back.
    {
        KConfigGroup group = scriptGroup();
        group.deleteEntry(QStringLiteral("fixedSizePredicate"));
        group.sync();
    }
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        QComboBox *combo = fixedSizePredicateCombo(module);
        CHECK(combo != nullptr);
        module.load();
        if (combo) {
            CHECK(combo->currentData().toString() == QStringLiteral("both-axes-fixed"));
        }
        module.scriptSucceed = true;
        module.save();
        CHECK(!scriptGroup().hasKey(QStringLiteral("fixedSizePredicate")));
        CHECK(module.scriptCalls == 0);
        CHECK(!module.isScriptRestartRequired());
    }
    // A predicate change persists, sends one live reconfigure, and never
    // requires a session restart.
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        module.load();
        QComboBox *combo = fixedSizePredicateCombo(module);
        CHECK(combo != nullptr);
        if (!combo) {
            return;
        }
        const QString target = otherFixedSizePredicate(storedFixedSizePredicate());
        const int index = combo->findData(target);
        CHECK(index >= 0);
        combo->setCurrentIndex(index);
        CHECK(module.needsSave());
        module.scriptSucceed = true;
        module.save();
        CHECK(storedFixedSizePredicate() == target);
        CHECK(module.scriptCalls == 1);
        CHECK(!module.isScriptRestartRequired());
        CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
        CHECK(module.scriptStatusText().contains(QStringLiteral("fixed-size")));
        CHECK(!containsAppliedClaim(module.scriptStatusText()));
        CHECK(!module.needsSave());
        module.save();
        CHECK(module.scriptCalls == 1);
        module.defaults();
        CHECK(combo->currentData().toString() == QStringLiteral("both-axes-fixed"));
        module.save();
        CHECK(storedFixedSizePredicate() == QStringLiteral("both-axes-fixed"));
    }
}

void bornMaximizedContractDefaultsValidatesAndPersists()
{
    // Invalid stored values normalize to the tile-on-launch default.
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("bornMaximized"), QStringLiteral("bogus"));
        group.sync();
    }
    {
        KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
        QComboBox *combo = bornMaximizedCombo(module);
        CHECK(combo != nullptr);
        module.load();
        if (combo) {
            CHECK(combo->currentData().toString() == QStringLiteral("tile-on-launch"));
        }
    }
    // Missing keys stay missing through load and default back.
    {
        KConfigGroup group = scriptGroup();
        group.deleteEntry(QStringLiteral("bornMaximized"));
        group.sync();
    }
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        QComboBox *combo = bornMaximizedCombo(module);
        CHECK(combo != nullptr);
        module.load();
        if (combo) {
            CHECK(combo->currentData().toString() == QStringLiteral("tile-on-launch"));
        }
        module.scriptSucceed = true;
        module.save();
        CHECK(!scriptGroup().hasKey(QStringLiteral("bornMaximized")));
        CHECK(module.scriptCalls == 0);
        CHECK(!module.isScriptRestartRequired());
    }
    // A born-maximized change persists, sends one live reconfigure, and
    // never requires a session restart.
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        module.load();
        QComboBox *combo = bornMaximizedCombo(module);
        CHECK(combo != nullptr);
        if (!combo) {
            return;
        }
        const QString target = otherBornMaximized(storedBornMaximized());
        const int index = combo->findData(target);
        CHECK(index >= 0);
        combo->setCurrentIndex(index);
        CHECK(module.needsSave());
        module.scriptSucceed = true;
        module.save();
        CHECK(storedBornMaximized() == target);
        CHECK(module.scriptCalls == 1);
        CHECK(!module.isScriptRestartRequired());
        CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
        CHECK(module.scriptStatusText().contains(QStringLiteral("born-maximized")));
        CHECK(!containsAppliedClaim(module.scriptStatusText()));
        CHECK(!module.needsSave());
        module.save();
        CHECK(module.scriptCalls == 1);
        module.defaults();
        CHECK(combo->currentData().toString() == QStringLiteral("tile-on-launch"));
        module.save();
        CHECK(storedBornMaximized() == QStringLiteral("tile-on-launch"));
    }
}

void migrationSourceRefillContractDefaultsValidatesAndPersists()
{
    // Invalid stored values normalize to the last-remaining-workspace default.
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("migrationSourceRefill"), QStringLiteral("bogus"));
        group.sync();
    }
    {
        KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
        QComboBox *combo = migrationSourceRefillCombo(module);
        CHECK(combo != nullptr);
        module.load();
        if (combo) {
            CHECK(combo->currentData().toString() == QStringLiteral("last-remaining-workspace"));
        }
    }
    // Missing keys stay missing through load and default back.
    {
        KConfigGroup group = scriptGroup();
        group.deleteEntry(QStringLiteral("migrationSourceRefill"));
        group.sync();
    }
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        QComboBox *combo = migrationSourceRefillCombo(module);
        CHECK(combo != nullptr);
        module.load();
        if (combo) {
            CHECK(combo->currentData().toString() == QStringLiteral("last-remaining-workspace"));
        }
        module.scriptSucceed = true;
        module.save();
        CHECK(!scriptGroup().hasKey(QStringLiteral("migrationSourceRefill")));
        CHECK(module.scriptCalls == 0);
        CHECK(!module.isScriptRestartRequired());
    }
    // A refill change persists, sends one live reconfigure, and never
    // requires a session restart.
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        module.load();
        QComboBox *combo = migrationSourceRefillCombo(module);
        CHECK(combo != nullptr);
        if (!combo) {
            return;
        }
        const QString target = otherMigrationSourceRefill(storedMigrationSourceRefill());
        const int index = combo->findData(target);
        CHECK(index >= 0);
        combo->setCurrentIndex(index);
        CHECK(module.needsSave());
        module.scriptSucceed = true;
        module.save();
        CHECK(storedMigrationSourceRefill() == target);
        CHECK(module.scriptCalls == 1);
        CHECK(!module.isScriptRestartRequired());
        CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
        CHECK(module.scriptStatusText().contains(QStringLiteral("migration source")));
        CHECK(!containsAppliedClaim(module.scriptStatusText()));
        CHECK(!module.needsSave());
        module.save();
        CHECK(module.scriptCalls == 1);
        module.defaults();
        CHECK(combo->currentData().toString() == QStringLiteral("last-remaining-workspace"));
        module.save();
        CHECK(storedMigrationSourceRefill() == QStringLiteral("last-remaining-workspace"));
    }
}

void unsupportedControlsAreAbsentAndLegacyValuesUntouched()
{
    KConfigGroup group = scriptGroup();
    group.writeEntry(QStringLiteral("tilingAlgorithm"), QStringLiteral("columns"));
    group.writeEntry(QStringLiteral("automaticSplitTarget"), QStringLiteral("active"));
    group.writeEntry(QStringLiteral("dropOutlinePreview"), true);
    group.sync();

    CountingScriptModule module(nullptr, KPluginMetaData());
    module.scriptSucceed = true;
    module.load();
    CHECK(module.widget()->findChild<QComboBox *>(QStringLiteral("tilingAlgorithmCombo")) == nullptr);
    CHECK(module.widget()->findChild<QComboBox *>(QStringLiteral("automaticSplitTargetCombo")) == nullptr);
    CHECK(module.widget()->findChild<QComboBox *>(QStringLiteral("dropOutlinePreviewCheckBox")) == nullptr);
    module.save();

    CHECK(group.readEntry(QStringLiteral("tilingAlgorithm"), QString()) == QStringLiteral("columns"));
    CHECK(group.readEntry(QStringLiteral("automaticSplitTarget"), QString()) == QStringLiteral("active"));
    CHECK(group.readEntry(QStringLiteral("dropOutlinePreview"), false));
}

void unchangedSaveSendsNothing()
{
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    CHECK(!module.isScriptRestartRequired());
    CHECK(scriptStatusLabel(module) != nullptr);
    CHECK(module.scriptStatusText().contains(QStringLiteral("No pending")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    module.scriptSucceed = true;
    module.save();
    CHECK(!module.isScriptRestartRequired());
    CHECK(!module.isGapReconfigurePending());
    CHECK(module.scriptCalls == 0);
    CHECK(!module.needsSave());
    CHECK(module.scriptStatusText().contains(QStringLiteral("No pending")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
}

void gapSavePersistsThenSendsUnconfirmed()
{
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    QSpinBox *inner = innerGapSpinBox(module);
    CHECK(inner != nullptr);
    if (!inner) {
        return;
    }
    const int gapTarget = (inner->value() == 12) ? 20 : 12;
    inner->setValue(gapTarget);
    CHECK(module.needsSave());
    module.scriptSucceed = true;
    module.save();
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == gapTarget);
    CHECK(module.scriptCalls == 1);
    CHECK(!module.isGapReconfigurePending());
    CHECK(!module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(!module.needsSave());
    // A follow-up unchanged save must not send again.
    module.save();
    CHECK(module.scriptCalls == 1);
}

void gapSaveSendFailureArmsRetryOnNextSave()
{
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    QSpinBox *inner = innerGapSpinBox(module);
    CHECK(inner != nullptr);
    if (!inner) {
        return;
    }
    const int gapTarget = (inner->value() == 12) ? 20 : 12;
    inner->setValue(gapTarget);
    module.scriptSucceed = false;
    module.save();
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == gapTarget);
    CHECK(module.scriptCalls == 1);
    CHECK(module.isGapReconfigurePending());
    CHECK(!module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("failed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("retry on the next save")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("saved to kwinrc")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    // Apply stays enabled for the retry even though widgets match storage.
    CHECK(module.needsSave());
    // A still-failing retry persists nothing further and must not claim the
    // retry saved anything.
    module.save();
    CHECK(module.scriptCalls == 2);
    CHECK(module.isGapReconfigurePending());
    CHECK(module.scriptStatusText().contains(QStringLiteral("failed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("saved nothing")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("saved to kwinrc")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(module.needsSave());
    // An unchanged follow-up save retries the request and clears the flag.
    module.scriptSucceed = true;
    module.save();
    CHECK(module.scriptCalls == 3);
    CHECK(!module.isGapReconfigurePending());
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == gapTarget);
    CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("saved nothing")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("saved to kwinrc")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(!module.needsSave());
}

void sameAxisMoveSaveFailureArmsRetryOnNextSave()
{
    // Mirror of gapSaveSendFailureArmsRetryOnNextSave for a same-axis-only
    // change: one queued reconfigure call per save, the saved mode retained,
    // and no session restart required at any point.
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    QComboBox *combo = sameAxisMoveCombo(module);
    CHECK(combo != nullptr);
    if (!combo) {
        return;
    }
    const QString target = otherSameAxisMove(storedSameAxisMove());
    const int index = combo->findData(target);
    CHECK(index >= 0);
    combo->setCurrentIndex(index);
    CHECK(module.needsSave());
    module.scriptSucceed = false;
    module.save();
    CHECK(storedSameAxisMove() == target);
    CHECK(module.scriptCalls == 1);
    CHECK(module.isGapReconfigurePending());
    CHECK(!module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("failed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("retry on the next save")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("saved to kwinrc")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    // Apply stays enabled for the retry even though widgets match storage.
    CHECK(module.needsSave());
    // A still-failing retry persists nothing further and must not claim the
    // retry saved anything.
    module.save();
    CHECK(module.scriptCalls == 2);
    CHECK(module.isGapReconfigurePending());
    CHECK(!module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("failed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("saved nothing")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("saved to kwinrc")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(module.needsSave());
    // An unchanged follow-up save retries the request and clears the flag.
    module.scriptSucceed = true;
    module.save();
    CHECK(module.scriptCalls == 3);
    CHECK(!module.isGapReconfigurePending());
    CHECK(!module.isScriptRestartRequired());
    CHECK(storedSameAxisMove() == target);
    CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("saved nothing")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("saved to kwinrc")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(!module.needsSave());
}

void combinedGapAndSameAxisMoveSaveSendsOnce()
{
    // One save carrying both a gap and a same-axis change persists both keys
    // with a single live reconfigure and no restart requirement.
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    QComboBox *combo = sameAxisMoveCombo(module);
    QSpinBox *inner = innerGapSpinBox(module);
    CHECK(combo != nullptr);
    CHECK(inner != nullptr);
    if (!combo || !inner) {
        return;
    }
    const QString axisTarget = otherSameAxisMove(storedSameAxisMove());
    const int axisIndex = combo->findData(axisTarget);
    CHECK(axisIndex >= 0);
    const int gapTarget = (inner->value() == 12) ? 20 : 12;
    combo->setCurrentIndex(axisIndex);
    inner->setValue(gapTarget);
    CHECK(module.needsSave());
    module.scriptSucceed = true;
    module.save();
    CHECK(storedSameAxisMove() == axisTarget);
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == gapTarget);
    CHECK(module.scriptCalls == 1);
    CHECK(!module.isGapReconfigurePending());
    CHECK(!module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("same-axis")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("gap")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(!module.needsSave());
    // A follow-up unchanged save must not send again.
    module.save();
    CHECK(module.scriptCalls == 1);
}

void combinedGapAndSameAxisMoveSaveFailureMentionsBothKeys()
{
    // A failing combined save persists both keys, arms the retry with one
    // queued call, and names both keys in the failure status.
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    QComboBox *combo = sameAxisMoveCombo(module);
    QSpinBox *inner = innerGapSpinBox(module);
    CHECK(combo != nullptr);
    CHECK(inner != nullptr);
    if (!combo || !inner) {
        return;
    }
    const QString axisTarget = otherSameAxisMove(storedSameAxisMove());
    const int axisIndex = combo->findData(axisTarget);
    CHECK(axisIndex >= 0);
    const int gapTarget = (inner->value() == 12) ? 20 : 12;
    combo->setCurrentIndex(axisIndex);
    inner->setValue(gapTarget);
    CHECK(module.needsSave());
    module.scriptSucceed = false;
    module.save();
    CHECK(storedSameAxisMove() == axisTarget);
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == gapTarget);
    CHECK(module.scriptCalls == 1);
    CHECK(module.isGapReconfigurePending());
    CHECK(!module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("failed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("retry on the next save")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("saved to kwinrc")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("same-axis")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("gap")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(module.needsSave());
    // Recovery sends once more and clears the retry.
    module.scriptSucceed = true;
    module.save();
    CHECK(module.scriptCalls == 2);
    CHECK(!module.isGapReconfigurePending());
    CHECK(!module.isScriptRestartRequired());
    CHECK(!module.needsSave());
}

void startupOnlySaveDisablesSendWithRestartMessage()
{
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    QComboBox *mode = workspaceModeCombo(module);
    CHECK(mode != nullptr);
    if (!mode) {
        return;
    }
    // Aligned KDE offers per-output-local and shared only: no global-unique.
    CHECK(mode->count() == 2);
    CHECK(mode->findData(QStringLiteral("global-unique")) < 0);
    CHECK(mode->findData(QStringLiteral("per-output-local")) >= 0);
    CHECK(mode->findData(QStringLiteral("shared")) >= 0);
    const QString modeTarget = otherWorkspaceMode(storedWorkspaceMode());
    const int modeIndex = mode->findData(modeTarget);
    CHECK(modeIndex >= 0);
    mode->setCurrentIndex(modeIndex);
    CHECK(module.needsSave());
    module.scriptSucceed = true;
    module.save();
    CHECK(scriptGroup().readEntry(QStringLiteral("workspaceMode"), QString()) == modeTarget);
    CHECK(module.scriptCalls == 0);
    CHECK(module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("Workspace mode")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("startup settings")));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(!module.needsSave());
    // An unchanged follow-up save must not send and must keep the restart flag.
    module.save();
    CHECK(module.scriptCalls == 0);
    CHECK(module.isScriptRestartRequired());
}

void hiddenShortcutProfileIsAbsentAndPreservedUntouched()
{
    // The profile control is hidden until distinct profiles exist, but any
    // saved kwinrc value must survive every dialog operation untouched.
    const QStringList profiles = {
        QStringLiteral("cosmic"),
        QStringLiteral("hyprland"),
        QStringLiteral("bspwm"),
    };
    for (const QString &seeded : profiles) {
        {
            KConfigGroup group = scriptGroup();
            group.writeEntry(QStringLiteral("shortcutProfile"), seeded);
            group.sync();
        }
        CountingScriptModule module(nullptr, KPluginMetaData());
        module.load();
        CHECK(shortcutProfileCombo(module) == nullptr);
        CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("label_shortcutProfile")) == nullptr);
        CHECK(storedShortcutProfile() == seeded);
        // Unchanged save: no send, no restart, value untouched.
        module.scriptSucceed = true;
        module.save();
        CHECK(storedShortcutProfile() == seeded);
        CHECK(module.scriptCalls == 0);
        CHECK(!module.isScriptRestartRequired());
        // Gap change: sends reconfigure, value untouched.
        QSpinBox *inner = innerGapSpinBox(module);
        CHECK(inner != nullptr);
        if (!inner) {
            return;
        }
        const int gapTarget = (inner->value() == 12) ? 20 : 12;
        inner->setValue(gapTarget);
        module.save();
        CHECK(storedShortcutProfile() == seeded);
        CHECK(module.scriptCalls == 1);
        CHECK(!module.isScriptRestartRequired());
        // Workspace-mode change: restart required, no send, value untouched.
        QComboBox *mode = workspaceModeCombo(module);
        CHECK(mode != nullptr);
        if (!mode) {
            return;
        }
        const QString modeTarget = otherWorkspaceMode(storedWorkspaceMode());
        const int modeIndex = mode->findData(modeTarget);
        CHECK(modeIndex >= 0);
        mode->setCurrentIndex(modeIndex);
        module.save();
        CHECK(storedShortcutProfile() == seeded);
        CHECK(scriptGroup().readEntry(QStringLiteral("workspaceMode"), QString()) == modeTarget);
        CHECK(module.scriptCalls == 1);
        CHECK(module.isScriptRestartRequired());
        CHECK(containsRestartRequirement(module.scriptStatusText()));
        // Defaults must not introduce or alter the hidden key either.
        module.defaults();
        module.save();
        CHECK(storedShortcutProfile() == seeded);
    }
    // A missing hidden key must stay missing through gap and mode saves.
    {
        KConfigGroup group = scriptGroup();
        group.deleteEntry(QStringLiteral("shortcutProfile"));
        group.sync();
    }
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        module.load();
        CHECK(shortcutProfileCombo(module) == nullptr);
        CHECK(!scriptGroup().hasKey(QStringLiteral("shortcutProfile")));
        QSpinBox *inner = innerGapSpinBox(module);
        CHECK(inner != nullptr);
        if (!inner) {
            return;
        }
        const int gapTarget = (inner->value() == 12) ? 20 : 12;
        inner->setValue(gapTarget);
        module.scriptSucceed = true;
        module.save();
        CHECK(!scriptGroup().hasKey(QStringLiteral("shortcutProfile")));
        QComboBox *mode = workspaceModeCombo(module);
        CHECK(mode != nullptr);
        if (!mode) {
            return;
        }
        const QString modeTarget = otherWorkspaceMode(storedWorkspaceMode());
        const int modeIndex = mode->findData(modeTarget);
        CHECK(modeIndex >= 0);
        mode->setCurrentIndex(modeIndex);
        module.save();
        CHECK(!scriptGroup().hasKey(QStringLiteral("shortcutProfile")));
        CHECK(module.isScriptRestartRequired());
    }
}

void combinedGapAndStartupSaveSendsWithResidualRestart()
{
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    QComboBox *mode = workspaceModeCombo(module);
    QSpinBox *inner = innerGapSpinBox(module);
    CHECK(mode != nullptr);
    CHECK(inner != nullptr);
    if (!mode || !inner) {
        return;
    }
    const QString modeTarget = otherWorkspaceMode(storedWorkspaceMode());
    const int modeIndex = mode->findData(modeTarget);
    CHECK(modeIndex >= 0);
    const int gapTarget = (inner->value() == 12) ? 20 : 12;
    mode->setCurrentIndex(modeIndex);
    inner->setValue(gapTarget);
    CHECK(module.needsSave());
    module.scriptSucceed = true;
    module.save();
    CHECK(scriptGroup().readEntry(QStringLiteral("workspaceMode"), QString()) == modeTarget);
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == gapTarget);
    CHECK(module.scriptCalls == 1);
    CHECK(module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("workspace mode")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("startup settings")));
    CHECK(containsRestartRequirement(module.scriptStatusText()));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(!module.needsSave());
    // A later gap-only save keeps the standing workspace-mode restart without
    // claiming workspace mode was just saved again.
    QSpinBox *laterGap = innerGapSpinBox(module);
    CHECK(laterGap != nullptr);
    if (!laterGap) {
        return;
    }
    const int laterGapTarget = (laterGap->value() == 12) ? 20 : 12;
    laterGap->setValue(laterGapTarget);
    module.save();
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == laterGapTarget);
    CHECK(module.scriptCalls == 2);
    CHECK(module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("workspace mode")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("startup settings")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("Workspace mode saved")));
    CHECK(containsRestartRequirement(module.scriptStatusText()));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(!module.needsSave());
}

void combinedGapAndStartupSendFailureKeepsResidualRestart()
{
    CountingScriptModule module(nullptr, KPluginMetaData());
    module.load();
    QComboBox *mode = workspaceModeCombo(module);
    QSpinBox *inner = innerGapSpinBox(module);
    CHECK(mode != nullptr);
    CHECK(inner != nullptr);
    if (!mode || !inner) {
        return;
    }
    const QString modeTarget = otherWorkspaceMode(storedWorkspaceMode());
    const int modeIndex = mode->findData(modeTarget);
    CHECK(modeIndex >= 0);
    const int gapTarget = (inner->value() == 12) ? 20 : 12;
    mode->setCurrentIndex(modeIndex);
    inner->setValue(gapTarget);
    module.scriptSucceed = false;
    module.save();
    CHECK(module.scriptCalls == 1);
    CHECK(module.isGapReconfigurePending());
    CHECK(module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("failed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("retry on the next save")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("saved to kwinrc")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("workspace mode")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("startup settings")));
    CHECK(containsRestartRequirement(module.scriptStatusText()));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(module.needsSave());
    // A still-failing retry saves nothing yet keeps the restart residual.
    module.save();
    CHECK(module.scriptCalls == 2);
    CHECK(module.isGapReconfigurePending());
    CHECK(module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("failed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("saved nothing")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("saved to kwinrc")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("workspace mode")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("startup settings")));
    CHECK(containsRestartRequirement(module.scriptStatusText()));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    // Retry success keeps the restart residual while clearing the pending flag.
    module.scriptSucceed = true;
    module.save();
    CHECK(module.scriptCalls == 3);
    CHECK(!module.isGapReconfigurePending());
    CHECK(module.isScriptRestartRequired());
    CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("saved nothing")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("saved to kwinrc")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("workspace mode")));
    CHECK(!module.scriptStatusText().contains(QStringLiteral("startup settings")));
    CHECK(containsRestartRequirement(module.scriptStatusText()));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    CHECK(!module.needsSave());
}

void combinedEffectAndScriptSavePreservesScriptOnEffectFailure()
{
    // Focused unified-page regression: one save carrying both a managed
    // border change and script gap/workspace changes persists the script
    // keys even when the effect reconfigure is disabled/missing, and keeps
    // the effect retry plus Apply enabled without disturbing script state.
    CombinedReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QComboBox *mode = workspaceModeCombo(module);
    QSpinBox *inner = innerGapSpinBox(module);
    QDoubleSpinBox *border = borderWidthSpinBox(module);
    CHECK(mode != nullptr);
    CHECK(inner != nullptr);
    CHECK(border != nullptr);
    if (!mode || !inner || !border) {
        return;
    }
    const QString modeTarget = otherWorkspaceMode(storedWorkspaceMode());
    const int modeIndex = mode->findData(modeTarget);
    CHECK(modeIndex >= 0);
    const int gapTarget = (inner->value() == 12) ? 20 : 12;
    const double borderTarget = (border->value() == 7.5) ? 6.5 : 7.5;
    mode->setCurrentIndex(modeIndex);
    inner->setValue(gapTarget);
    border->setValue(borderTarget);
    CHECK(module.needsSave());
    module.scriptSucceed = true;
    module.effectSucceed = false;
    module.save();
    CHECK(scriptGroup().readEntry(QStringLiteral("workspaceMode"), QString()) == modeTarget);
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == gapTarget);
    CHECK(borderGroup().readEntry(QStringLiteral("BorderWidth"), 0.0) == borderTarget);
    CHECK(module.scriptCalls == 1);
    CHECK(module.effectCalls == 1);
    CHECK(module.isScriptRestartRequired());
    CHECK(!module.isGapReconfigurePending());
    CHECK(module.scriptStatusText().contains(QStringLiteral("unconfirmed")));
    CHECK(module.scriptStatusText().contains(QStringLiteral("workspace mode")));
    CHECK(containsRestartRequirement(module.scriptStatusText()));
    CHECK(!containsAppliedClaim(module.scriptStatusText()));
    // Failed effect hot-apply keeps Apply enabled for the retry.
    CHECK(module.needsSave());
    // An unchanged save retries only the effect: script stays persisted with
    // no resend, restart residual and Apply preserved.
    module.save();
    CHECK(module.effectCalls == 2);
    CHECK(module.scriptCalls == 1);
    CHECK(scriptGroup().readEntry(QStringLiteral("workspaceMode"), QString()) == modeTarget);
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == gapTarget);
    CHECK(borderGroup().readEntry(QStringLiteral("BorderWidth"), 0.0) == borderTarget);
    CHECK(module.isScriptRestartRequired());
    CHECK(!module.isGapReconfigurePending());
    CHECK(module.needsSave());
    // Effect recovery clears Apply while keeping persisted script state and
    // the standing workspace-mode restart.
    module.effectSucceed = true;
    module.save();
    CHECK(module.effectCalls == 3);
    CHECK(module.scriptCalls == 1);
    CHECK(scriptGroup().readEntry(QStringLiteral("workspaceMode"), QString()) == modeTarget);
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == gapTarget);
    CHECK(module.isScriptRestartRequired());
    CHECK(!module.isGapReconfigurePending());
    CHECK(!module.needsSave());
}

void poisonedBusScriptSendFailsClosed()
{
    KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
    module.load();
    CHECK(!module.requestScriptReconfigure());
}

QStringList capturedScriptConfigMessages;

void captureScriptConfigMessage(QtMsgType, const QMessageLogContext &, const QString &message)
{
    capturedScriptConfigMessages.append(message);
}

QString startupRestartLog()
{
    for (const QString &message : capturedScriptConfigMessages) {
        if (message.contains(QStringLiteral("stage=startup"))) {
            return message;
        }
    }
    return QString();
}

void startupRestartLogEnumeratesOnlyChangedKeys()
{
    // Only workspaceMode can be changed from this dialog: the startup log
    // must list exactly that key and never the hidden shortcutProfile.
    capturedScriptConfigMessages.clear();
    const QtMessageHandler previous = qInstallMessageHandler(captureScriptConfigMessage);
    {
        CountingScriptModule module(nullptr, KPluginMetaData());
        module.load();
        QComboBox *mode = workspaceModeCombo(module);
        CHECK(mode != nullptr);
        if (mode) {
            const QString modeTarget = otherWorkspaceMode(storedWorkspaceMode());
            const int modeIndex = mode->findData(modeTarget);
            CHECK(modeIndex >= 0);
            mode->setCurrentIndex(modeIndex);
        }
        module.scriptSucceed = true;
        module.save();
    }
    qInstallMessageHandler(previous);
    const QString single = startupRestartLog();
    CHECK(!single.isEmpty());
    CHECK(single.contains(QStringLiteral("keys=workspaceMode")));
    CHECK(!single.contains(QStringLiteral("shortcutProfile")));
}

} // namespace

int main(int argc, char **argv)
{
    // Isolate from the live session bus before any
    // QDBusConnection::sessionBus() initialization.
    qputenv("DBUS_SESSION_BUS_ADDRESS", QByteArray("unix:path=/dev/null/omnitiler-scriptconfig-test-isolated-bus"));
    QTemporaryDir configHome;
    if (!configHome.isValid()) {
        std::fprintf(stderr, "failed to create temporary config directory\n");
        return EXIT_FAILURE;
    }
    qputenv("XDG_CONFIG_HOME", configHome.path().toUtf8());
    QApplication app(argc, argv);
    app.clipboard()->setMimeData(new QMimeData);

    if (argc != 2) {
        std::fprintf(stderr, "usage: %s dbus|gaps|save|windows\n", argv[0]);
        return EXIT_FAILURE;
    }

    const QString scenario = QString::fromLocal8Bit(argv[1]);
    if (scenario == QStringLiteral("dbus")) {
        scriptReconfigureTargetIsExact();
        poisonedBusScriptSendFailsClosed();
    } else if (scenario == QStringLiteral("gaps")) {
        gapContractNormalizesBoundsAndPersists();
        unsupportedControlsAreAbsentAndLegacyValuesUntouched();
    } else if (scenario == QStringLiteral("save")) {
        unchangedSaveSendsNothing();
        gapSavePersistsThenSendsUnconfirmed();
        gapSaveSendFailureArmsRetryOnNextSave();
        sameAxisMoveSaveFailureArmsRetryOnNextSave();
        combinedGapAndSameAxisMoveSaveSendsOnce();
        combinedGapAndSameAxisMoveSaveFailureMentionsBothKeys();
        sameAxisMoveContractDefaultsValidatesAndPersists();
        fixedSizePredicateContractDefaultsValidatesAndPersists();
        bornMaximizedContractDefaultsValidatesAndPersists();
        migrationSourceRefillContractDefaultsValidatesAndPersists();
        startupRestartLogEnumeratesOnlyChangedKeys();
        startupOnlySaveDisablesSendWithRestartMessage();
        hiddenShortcutProfileIsAbsentAndPreservedUntouched();
        combinedGapAndStartupSaveSendsWithResidualRestart();
        combinedGapAndStartupSendFailureKeepsResidualRestart();
        combinedEffectAndScriptSavePreservesScriptOnEffectFailure();
    } else if (scenario == QStringLiteral("windows")) {
        windowRowAndButtonStates();
        windowFixPersistsAndRevertDeletesKey();
        windowBordersFixDeletesKey();
        windowFailedSendShowsError();
        windowOperationsLogOneLinePerOperation();
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
