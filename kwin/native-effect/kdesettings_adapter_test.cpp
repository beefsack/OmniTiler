// Targeted regression coverage for catalog-owned KDE settings descriptors.
//
// Pins the native adapter contract: every tab/field/widget mapping, visible
// strings/order/buddies, cross-tab draft Apply/Defaults/Cancel with exact
// saved keys and tolerant fallback, and Save isolation from the native
// operation panels (no Fix/Force/Revert, no shortcut mutation).

#include "kdesettings_adapter.h"
#include "scriptconfig_module.h"

#include <KColorButton>
#include <KConfigGroup>
#include <KPluginMetaData>
#include <KSharedConfig>

#include <QApplication>
#include <QCheckBox>
#include <QClipboard>
#include <QComboBox>
#include <QDoubleSpinBox>
#include <QLabel>
#include <QMimeData>
#include <QPushButton>
#include <QSpinBox>
#include <QTabWidget>
#include <QTemporaryDir>
#include <QWidget>

#include <cstdio>
#include <cstdlib>
#include <cstring>

namespace
{

// Changed-descriptor sample: content absent from the production tables that
// proves the render functions follow the passed descriptor instead of
// production literals.
constexpr OmnitilerKdeSettings::KdeEnumOptionDescriptor kSyntheticOptions[] = {
    {"synthetic-b", kli18n("Synthetic second"), kli18n("Synthetic tip B")},
    {"synthetic-a", kli18n("Synthetic first"), kli18n("")},
};
constexpr OmnitilerKdeSettings::KdeFieldDescriptor kSyntheticCombo = {
    "synthetic.field",
    "Script-omnitiler-kwin",
    "syntheticKey",
    "syntheticCombo",
    kli18n("Synthetic schema"),
    kli18n("Synthetic UI label:"),
    "Enum",
    "synthetic-a",
    "",
    "",
    kSyntheticOptions,
    2,
    "tiling-workspaces",
};
constexpr OmnitilerKdeSettings::KdeFieldDescriptor kSyntheticDouble = {
    "synthetic.double",
    "Effect-omnitiler-active-border",
    "SyntheticDouble",
    "syntheticDouble",
    kli18n("Synthetic schema"),
    kli18n("Synthetic double:"),
    "Double",
    "2.5",
    "0.5",
    "9.5",
    nullptr,
    0,
    "appearance",
};

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
    return KConfigGroup(KSharedConfig::openConfig(QStringLiteral("kwinrc")),
                        KWin::KdeSettingsAdapter::scriptConfigGroupName());
}

KConfigGroup windowsGroup()
{
    return KConfigGroup(KSharedConfig::openConfig(QStringLiteral("kwinrc")), QStringLiteral("Windows"));
}

QComboBox *combo(KWin::ScriptConfigModule &module, const char *name)
{
    return module.widget()->findChild<QComboBox *>(QString::fromUtf8(name));
}

QSpinBox *gapSpin(KWin::ScriptConfigModule &module, const char *name)
{
    return module.widget()->findChild<QSpinBox *>(QString::fromUtf8(name));
}

class CountingBothModule : public KWin::ScriptConfigModule
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

void descriptorTablesMatchNativeContract()
{
    using namespace OmnitilerKdeSettings;
    CHECK(kKdeTabCount == 4);
    CHECK(kKdeExposedFieldCount == 14);
    CHECK(kKdeNativePanelCount == 11);
    const QStringList wantedTabs = {
        QStringLiteral("tiling-workspaces"),
        QStringLiteral("appearance"),
        QStringLiteral("shortcuts"),
        QStringLiteral("desktop-integration"),
    };
    const QStringList wantedPages = {
        QStringLiteral("tilingTab"),
        QStringLiteral("appearanceTab"),
        QStringLiteral("shortcutsTab"),
        QStringLiteral("desktopTab"),
    };
    CHECK(sizeof(kKdeTabs) / sizeof(kKdeTabs[0]) == 4);
    for (int i = 0; i < 4; ++i) {
        CHECK(QString::fromUtf8(kKdeTabs[i].tab_id) == wantedTabs.at(i));
        CHECK(QString::fromUtf8(kKdeTabs[i].page_widget) == wantedPages.at(i));
        CHECK(!kKdeTabs[i].title.toString().isEmpty());
    }
    const QStringList wantedKeys = {
        QStringLiteral("workspaceMode"),
        QStringLiteral("sameAxisMove"),
        QStringLiteral("fixedSizePredicate"),
        QStringLiteral("bornMaximized"),
        QStringLiteral("migrationSourceRefill"),
        QStringLiteral("innerGap"),
        QStringLiteral("outerGap"),
    };
    CHECK(KWin::KdeSettingsAdapter::scriptKeysInRowOrder() == wantedKeys);
    const QVariantMap defaults = KWin::KdeSettingsAdapter::scriptDefaults();
    CHECK(defaults.value(QStringLiteral("workspaceMode")).toString() == QStringLiteral("per-output-local"));
    CHECK(defaults.value(QStringLiteral("sameAxisMove")).toString() == QStringLiteral("group-with-neighbor"));
    CHECK(defaults.value(QStringLiteral("fixedSizePredicate")).toString() == QStringLiteral("both-axes-fixed"));
    CHECK(defaults.value(QStringLiteral("bornMaximized")).toString() == QStringLiteral("tile-on-launch"));
    CHECK(defaults.value(QStringLiteral("migrationSourceRefill")).toString()
          == QStringLiteral("last-remaining-workspace"));
    CHECK(defaults.value(QStringLiteral("innerGap")).toInt() == 8);
    CHECK(defaults.value(QStringLiteral("outerGap")).toInt() == 8);
    CHECK(sizeof(kKdeFields) / sizeof(kKdeFields[0]) == 14);
    for (const auto &field : kKdeFields) {
        CHECK(KWin::KdeSettingsAdapter::findFieldById(field.field_id) == &field);
        CHECK(!QString::fromUtf8(field.key).isEmpty());
        CHECK(!QString::fromUtf8(field.widget).isEmpty());
        CHECK(!field.ui_label.toString().isEmpty());
        if (field.option_count > 0) {
            CHECK(field.options != nullptr);
            for (int i = 0; i < field.option_count; ++i) {
                CHECK(!QString::fromUtf8(field.options[i].token).isEmpty());
                CHECK(!field.options[i].label.toString().isEmpty());
            }
        }
    }
    CHECK(KWin::KdeSettingsAdapter::findFieldById("no.such.field") == nullptr);
    CHECK(KWin::KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("workspaceMode")) != nullptr);
    CHECK(KWin::KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("BorderWidth")) == nullptr);
    CHECK(KWin::KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("shortcutProfile")) == nullptr);
    CHECK(KWin::KdeSettingsAdapter::scriptConfigGroupName() == QStringLiteral("Script-omnitiler-kwin"));
}

void tolerantReadersPreserveFallback()
{
    using namespace KWin::KdeSettingsAdapter;
    {
        KConfigGroup group = scriptGroup();
        group.deleteEntry(QStringLiteral("sameAxisMove"));
        group.deleteEntry(QStringLiteral("innerGap"));
        group.sync();
    }
    const auto *move = findScriptFieldByKey(QStringLiteral("sameAxisMove"));
    const auto *inner = findScriptFieldByKey(QStringLiteral("innerGap"));
    CHECK(move != nullptr);
    CHECK(inner != nullptr);
    // Missing keys read as catalog defaults.
    CHECK(readEnumToken(scriptGroup(), move) == QStringLiteral("group-with-neighbor"));
    CHECK(readBoundedInt(scriptGroup(), inner) == 8);
    CHECK(isBoundedIntRawValid(scriptGroup(), inner));
    CHECK(intMinimum(inner) == 0);
    CHECK(intMaximum(inner) == 64);
    CHECK(intDefault(inner) == 8);
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("sameAxisMove"), QStringLiteral("swap-with-neighbor"));
        group.writeEntry(QStringLiteral("innerGap"), 64);
        group.sync();
    }
    CHECK(readEnumToken(scriptGroup(), move) == QStringLiteral("swap-with-neighbor"));
    CHECK(readBoundedInt(scriptGroup(), inner) == 64);
    CHECK(isBoundedIntRawValid(scriptGroup(), inner));
    // Invalid tokens fall back; the startup-only mode keeps the raw entry.
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("sameAxisMove"), QStringLiteral("bogus"));
        group.writeEntry(QStringLiteral("workspaceMode"), QStringLiteral("bogus-mode"));
        group.writeEntry(QStringLiteral("innerGap"), QStringLiteral("not-a-number"));
        group.sync();
    }
    CHECK(readEnumToken(scriptGroup(), move) == QStringLiteral("group-with-neighbor"));
    CHECK(readWorkspaceMode(scriptGroup()) == QStringLiteral("bogus-mode"));
    CHECK(readBoundedInt(scriptGroup(), inner) == 8);
    CHECK(!isBoundedIntRawValid(scriptGroup(), inner));
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("innerGap"), 65);
        group.sync();
    }
    CHECK(readBoundedInt(scriptGroup(), inner) == 8);
    CHECK(!isBoundedIntRawValid(scriptGroup(), inner));
    {
        KConfigGroup group = scriptGroup();
        group.deleteEntry(QStringLiteral("sameAxisMove"));
        group.deleteEntry(QStringLiteral("workspaceMode"));
        group.deleteEntry(QStringLiteral("innerGap"));
        group.sync();
    }
}

QString pageWidgetForTab(const char *tabId)
{
    for (const auto &tab : OmnitilerKdeSettings::kKdeTabs) {
        if (std::strcmp(tab.tab_id, tabId) == 0) {
            return QString::fromUtf8(tab.page_widget);
        }
    }
    return QString();
}

QString containingPage(QWidget *control, QTabWidget *tabs)
{
    QWidget *page = control;
    while (page != nullptr && tabs->indexOf(page) < 0) {
        page = page->parentWidget();
    }
    return page != nullptr ? page->objectName() : QString();
}

void everyFieldTabAndWidgetMapping(KWin::ScriptConfigModule &module)
{
    using namespace OmnitilerKdeSettings;
    QTabWidget *tabs = module.widget()->findChild<QTabWidget *>(QStringLiteral("settingsTabs"));
    CHECK(tabs != nullptr);
    if (tabs) {
        CHECK(tabs->count() == kKdeTabCount);
        // Escaped ampersand survives verbatim: the descriptor carries the
        // double ampersand exactly as the Designer source does.
        CHECK(tabs->tabText(0) == QStringLiteral("Tiling && Workspaces"));
        for (int i = 0; i < kKdeTabCount && i < tabs->count(); ++i) {
            CHECK(tabs->tabText(i) == kKdeTabs[i].title.toString());
            CHECK(tabs->widget(i) != nullptr);
            if (tabs->widget(i)) {
                CHECK(tabs->widget(i)->objectName() == QString::fromUtf8(kKdeTabs[i].page_widget));
            }
        }
    }
    const QList<QLabel *> labels = module.widget()->findChildren<QLabel *>();
    for (const auto &field : kKdeFields) {
        QWidget *control = module.widget()->findChild<QWidget *>(QString::fromUtf8(field.widget));
        CHECK(control != nullptr);
        if (control == nullptr) {
            continue;
        }
        const QString schemaType = QString::fromUtf8(field.schema_type);
        CHECK(schemaType == QStringLiteral("Enum") || schemaType == QStringLiteral("Int")
              || schemaType == QStringLiteral("Double") || schemaType == QStringLiteral("Bool")
              || schemaType == QStringLiteral("Color"));
        if (schemaType == QStringLiteral("Enum")) {
            auto *box = qobject_cast<QComboBox *>(control);
            CHECK(box != nullptr);
            if (box) {
                CHECK(box->count() == field.option_count);
                for (int i = 0; i < field.option_count && i < box->count(); ++i) {
                    CHECK(box->itemText(i) == field.options[i].label.toString());
                    CHECK(box->itemData(i).toString() == QString::fromUtf8(field.options[i].token));
                    const QString tip = field.options[i].tip.toString();
                    if (tip.isEmpty()) {
                        CHECK(box->itemData(i, Qt::ToolTipRole).toString().isEmpty());
                    } else {
                        CHECK(box->itemData(i, Qt::ToolTipRole).toString() == tip);
                    }
                }
            }
        } else if (schemaType == QStringLiteral("Int")) {
            auto *spin = qobject_cast<QSpinBox *>(control);
            CHECK(spin != nullptr);
            if (spin) {
                CHECK(spin->minimum() == KWin::KdeSettingsAdapter::intMinimum(&field));
                CHECK(spin->maximum() == KWin::KdeSettingsAdapter::intMaximum(&field));
                CHECK(spin->suffix() == QStringLiteral(" px"));
            }
        } else if (schemaType == QStringLiteral("Double")) {
            auto *spin = qobject_cast<QDoubleSpinBox *>(control);
            CHECK(spin != nullptr);
            if (spin) {
                CHECK(spin->minimum() == KWin::KdeSettingsAdapter::doubleMinimum(&field));
                CHECK(spin->maximum() == KWin::KdeSettingsAdapter::doubleMaximum(&field));
                CHECK(spin->suffix() == QStringLiteral(" px"));
            }
        } else if (schemaType == QStringLiteral("Bool")) {
            auto *themeBox = qobject_cast<QCheckBox *>(control);
            CHECK(themeBox != nullptr);
            if (themeBox) {
                CHECK(themeBox->text() == field.ui_label.toString());
            }
        } else if (schemaType == QStringLiteral("Color")) {
            CHECK(qobject_cast<KColorButton *>(control) != nullptr);
        }
        // Dialog labels are descriptor-authoritative: buddy QLabel text for
        // every row control, checkbox text for the theme flag. Visible
        // labels keep their exact colon termination.
        const QString uiText = field.ui_label.toString();
        if (qobject_cast<QCheckBox *>(control) != nullptr) {
            CHECK(!uiText.endsWith(u':'));
        } else {
            CHECK(uiText.endsWith(u':'));
            QLabel *buddyLabel = nullptr;
            for (QLabel *label : labels) {
                if (label != nullptr && label->buddy() == control) {
                    buddyLabel = label;
                    break;
                }
            }
            CHECK(buddyLabel != nullptr);
            if (buddyLabel != nullptr) {
                CHECK(buddyLabel->text() == uiText);
            }
        }
        // Placement: each control lives on its descriptor tab's page, and
        // configuration adds no widget-level tooltips.
        if (tabs != nullptr) {
            CHECK(containingPage(control, tabs) == pageWidgetForTab(field.tab_id));
        }
        CHECK(control->toolTip().isEmpty());
    }
    // Native-only preview color keeps its retained label outside the
    // descriptors; managed kcfg bindings stay in place.
    auto *previewLabel = module.widget()->findChild<QLabel *>(QStringLiteral("label_DragPreviewColor"));
    CHECK(previewLabel != nullptr);
    if (previewLabel != nullptr) {
        CHECK(previewLabel->text() == QStringLiteral("Drop-target preview color:"));
    }
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("kcfg_DragPreviewColor")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("kcfg_BorderWidth")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("kcfg_GroupUnderlayColor")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("kcfg_GroupUnderlayExtension")) != nullptr);
    // Native operation panels remain explicit controls, never staged here.
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("shortcutApplyButton")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("shortcutRevertButton")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("windowTilingFixButton")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("windowBordersRow")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("scriptStatusGroup")) != nullptr);
}

void descriptorSampleDrivesConfiguration()
{
    using namespace KWin::KdeSettingsAdapter;
    // Enum rows follow the passed descriptor: production content first,
    // then the synthetic sample replaces it row for row.
    QComboBox scratch;
    const auto *move = findScriptFieldByKey(QStringLiteral("sameAxisMove"));
    CHECK(move != nullptr);
    if (move != nullptr) {
        populateEnumCombo(&scratch, move);
        CHECK(scratch.count() == move->option_count);
    }
    populateEnumCombo(&scratch, &kSyntheticCombo);
    CHECK(scratch.count() == 2);
    CHECK(scratch.itemText(0) == QStringLiteral("Synthetic second"));
    CHECK(scratch.itemData(0).toString() == QStringLiteral("synthetic-b"));
    CHECK(scratch.itemData(0, Qt::ToolTipRole).toString() == QStringLiteral("Synthetic tip B"));
    CHECK(scratch.itemText(1) == QStringLiteral("Synthetic first"));
    CHECK(scratch.itemData(1).toString() == QStringLiteral("synthetic-a"));
    CHECK(scratch.itemData(1, Qt::ToolTipRole).toString().isEmpty());
    selectEnumCombo(&scratch, QStringLiteral("synthetic-a"), QStringLiteral("synthetic-b"));
    CHECK(scratch.currentData().toString() == QStringLiteral("synthetic-a"));
    // Double bounds follow the passed descriptor, negatives included, then
    // the production rows reapply exactly.
    QDoubleSpinBox doubleScratch;
    configureDoubleSpinBox(&doubleScratch, &kSyntheticDouble);
    CHECK(doubleScratch.minimum() == 0.5);
    CHECK(doubleScratch.maximum() == 9.5);
    CHECK(doubleDefault(&kSyntheticDouble) == 2.5);
    const auto *width = findFieldById("border.width");
    CHECK(width != nullptr);
    if (width != nullptr) {
        configureDoubleSpinBox(&doubleScratch, width);
        CHECK(doubleScratch.minimum() == 0.0);
        CHECK(doubleScratch.maximum() == 32.0);
    }
    const auto *extension = findFieldById("underlay.extension");
    CHECK(extension != nullptr);
    if (extension != nullptr) {
        configureDoubleSpinBox(&doubleScratch, extension);
        CHECK(doubleScratch.minimum() == -1.0);
        CHECK(doubleScratch.maximum() == 32.0);
    }
    // Labels follow the passed descriptor through the buddy binding.
    QWidget root;
    auto *box = new QComboBox(&root);
    box->setObjectName(QStringLiteral("syntheticCombo"));
    auto *label = new QLabel(&root);
    label->setBuddy(box);
    applyFieldLabel(&root, &kSyntheticCombo);
    CHECK(label->text() == QStringLiteral("Synthetic UI label:"));
    CHECK(label->buddy() == box);
    // Tab titles follow the descriptor table without rearranging pages:
    // inserted last-first, each page still takes its own title.
    QWidget holderRoot;
    QTabWidget holder(&holderRoot);
    QWidget *desktop = new QWidget();
    desktop->setObjectName(QStringLiteral("desktopTab"));
    QWidget *shortcuts = new QWidget();
    shortcuts->setObjectName(QStringLiteral("shortcutsTab"));
    QWidget *appearance = new QWidget();
    appearance->setObjectName(QStringLiteral("appearanceTab"));
    QWidget *tiling = new QWidget();
    tiling->setObjectName(QStringLiteral("tilingTab"));
    holder.addTab(desktop, QStringLiteral("placeholder"));
    holder.addTab(shortcuts, QStringLiteral("placeholder"));
    holder.addTab(appearance, QStringLiteral("placeholder"));
    holder.addTab(tiling, QStringLiteral("placeholder"));
    configureTabTitles(&holderRoot);
    CHECK(holder.count() == 4);
    CHECK(holder.widget(0)->objectName() == QStringLiteral("desktopTab"));
    CHECK(holder.tabText(holder.indexOf(desktop)) == QStringLiteral("Desktop Integration"));
    CHECK(holder.tabText(holder.indexOf(shortcuts)) == QStringLiteral("Shortcuts"));
    CHECK(holder.tabText(holder.indexOf(appearance)) == QStringLiteral("Appearance"));
    CHECK(holder.tabText(holder.indexOf(tiling)) == QStringLiteral("Tiling && Workspaces"));
}

void scriptIdentitySharesDescriptorDrivenControls()
{
    KWin::ScriptConfigModule module(nullptr, KPluginMetaData());
    module.load();
    everyFieldTabAndWidgetMapping(module);
}

void crossTabDraftApplyDefaultsCancelAndExactKeys()
{
    // Seed: valid enum, invalid gap (repairs on save), hidden + foreign
    // keys that must survive every operation untouched.
    {
        KConfigGroup group = scriptGroup();
        group.writeEntry(QStringLiteral("sameAxisMove"), QStringLiteral("swap-with-neighbor"));
        group.writeEntry(QStringLiteral("innerGap"), QStringLiteral("not-a-number"));
        group.writeEntry(QStringLiteral("shortcutProfile"), QStringLiteral("hyprland"));
        group.writeEntry(QStringLiteral("tilingAlgorithm"), QStringLiteral("columns"));
        group.sync();
    }
    windowsGroup().writeEntry(QStringLiteral("ElectricBorderTiling"), true);
    windowsGroup().sync();
    KSharedConfig::openConfig(QStringLiteral("kwinrc"))->sync();

    CountingBothModule module(nullptr, KPluginMetaData());
    module.load();
    QComboBox *mode = combo(module, "workspaceModeCombo");
    QComboBox *axis = combo(module, "sameAxisMoveCombo");
    QSpinBox *inner = gapSpin(module, "innerGapSpinBox");
    QDoubleSpinBox *borderWidth =
        module.widget()->findChild<QDoubleSpinBox *>(QStringLiteral("kcfg_BorderWidth"));
    CHECK(mode != nullptr);
    CHECK(axis != nullptr);
    CHECK(inner != nullptr);
    CHECK(borderWidth != nullptr);
    if (!mode || !axis || !inner || !borderWidth) {
        return;
    }
    // Tolerant load: invalid gap shows the catalog default, valid enum stays.
    CHECK(inner->value() == 8);
    CHECK(axis->currentData().toString() == QStringLiteral("swap-with-neighbor"));
    // Cross-tab draft: tiling mode + gap plus an Appearance border change.
    const int modeIndex = mode->findData(QStringLiteral("shared"));
    CHECK(modeIndex >= 0);
    mode->setCurrentIndex(modeIndex);
    const int axisIndex = axis->findData(QStringLiteral("group-with-neighbor"));
    CHECK(axisIndex >= 0);
    axis->setCurrentIndex(axisIndex);
    inner->setValue(12);
    const double borderTarget = (borderWidth->value() == 7.5) ? 6.5 : 7.5;
    borderWidth->setValue(borderTarget);
    CHECK(module.needsSave());
    module.scriptSucceed = true;
    module.effectSucceed = true;
    module.save();
    // Exact saved keys: changed script keys plus the managed border key.
    CHECK(scriptGroup().readEntry(QStringLiteral("workspaceMode"), QString()) == QStringLiteral("shared"));
    CHECK(scriptGroup().readEntry(QStringLiteral("sameAxisMove"), QString()) == QStringLiteral("group-with-neighbor"));
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == 12);
    CHECK(!scriptGroup().hasKey(QStringLiteral("outerGap")));
    // Untouched keys stay untouched: hidden profile, foreign key, windows.
    CHECK(scriptGroup().readEntry(QStringLiteral("shortcutProfile"), QString()) == QStringLiteral("hyprland"));
    CHECK(scriptGroup().readEntry(QStringLiteral("tilingAlgorithm"), QString()) == QStringLiteral("columns"));
    CHECK(windowsGroup().readEntry(QStringLiteral("ElectricBorderTiling"), false));
    CHECK(module.scriptCalls == 1);
    CHECK(module.effectCalls == 1);
    CHECK(!module.needsSave());
    // Defaults restores catalog values across tabs; a follow-up save
    // persists exactly those keys.
    module.defaults();
    CHECK(mode->currentData().toString() == QStringLiteral("per-output-local"));
    CHECK(axis->currentData().toString() == QStringLiteral("group-with-neighbor"));
    CHECK(inner->value() == 8);
    module.save();
    CHECK(scriptGroup().readEntry(QStringLiteral("workspaceMode"), QString())
          == QStringLiteral("per-output-local"));
    CHECK(scriptGroup().readEntry(QStringLiteral("sameAxisMove"), QString())
          == QStringLiteral("group-with-neighbor"));
    CHECK(scriptGroup().readEntry(QStringLiteral("innerGap"), -1) == 8);
    CHECK(scriptGroup().readEntry(QStringLiteral("shortcutProfile"), QString()) == QStringLiteral("hyprland"));
    // Cancel (load) discards an unapplied draft without persisting.
    mode->setCurrentIndex(mode->findData(QStringLiteral("global-unique")));
    inner->setValue(20);
    CHECK(module.needsSave());
    module.load();
    CHECK(mode->currentData().toString() == QStringLiteral("per-output-local"));
    CHECK(inner->value() == 8);
    CHECK(scriptGroup().readEntry(QStringLiteral("workspaceMode"), QString())
          == QStringLiteral("per-output-local"));
    // Cleanup seeded keys for other scenarios.
    {
        KConfigGroup group = scriptGroup();
        group.deleteEntry(QStringLiteral("workspaceMode"));
        group.deleteEntry(QStringLiteral("sameAxisMove"));
        group.deleteEntry(QStringLiteral("innerGap"));
        group.deleteEntry(QStringLiteral("shortcutProfile"));
        group.deleteEntry(QStringLiteral("tilingAlgorithm"));
        group.sync();
        KConfigGroup windows = windowsGroup();
        windows.deleteEntry(QStringLiteral("ElectricBorderTiling"));
        windows.sync();
    }
}

} // namespace

int main(int argc, char **argv)
{
    qputenv("DBUS_SESSION_BUS_ADDRESS", QByteArray("unix:path=/dev/null/omnitiler-descriptor-test-isolated-bus"));
    QTemporaryDir configHome;
    if (!configHome.isValid()) {
        std::fprintf(stderr, "failed to create temporary config directory\n");
        return EXIT_FAILURE;
    }
    qputenv("XDG_CONFIG_HOME", configHome.path().toUtf8());
    QApplication app(argc, argv);
    app.clipboard()->setMimeData(new QMimeData);

    if (argc != 2) {
        std::fprintf(stderr, "usage: %s descriptors\n", argv[0]);
        return EXIT_FAILURE;
    }
    const QString scenario = QString::fromLocal8Bit(argv[1]);
    if (scenario != QStringLiteral("descriptors")) {
        std::fprintf(stderr, "unknown scenario: %s\n", argv[1]);
        return EXIT_FAILURE;
    }

    descriptorTablesMatchNativeContract();
    tolerantReadersPreserveFallback();
    descriptorSampleDrivesConfiguration();
    scriptIdentitySharesDescriptorDrivenControls();
    crossTabDraftApplyDefaultsCancelAndExactKeys();

    if (failures != 0) {
        std::fprintf(stderr, "%d check(s) failed\n", failures);
        return EXIT_FAILURE;
    }
    std::printf("all checks passed\n");
    return EXIT_SUCCESS;
}
