#include "activeborderconfig.h"
#include "activeborderconfig_module.h"
#include "activeborderlogic.h"
#include "kdesettings_adapter.h"

#include <KColorButton>
#include <KConfigGroup>
#include <KPluginMetaData>
#include <KSharedConfig>

#include <QApplication>
#include <QCheckBox>
#include <QClipboard>
#include <QColor>
#include <QComboBox>
#include <QDBusMessage>
#include <QDoubleSpinBox>
#include <QLabel>
#include <QMimeData>
#include <QSpinBox>
#include <QTabWidget>
#include <QTemporaryDir>

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

KConfigGroup borderGroup()
{
    return KConfigGroup(KSharedConfig::openConfig(QStringLiteral("kwinrc")), QStringLiteral("Effect-omnitiler-active-border"));
}

QDoubleSpinBox *borderWidthSpinBox(KWin::ActiveBorderConfigModule &module)
{
    return module.widget()->findChild<QDoubleSpinBox *>(QStringLiteral("kcfg_BorderWidth"));
}

KColorButton *borderColorButton(KWin::ActiveBorderConfigModule &module)
{
    return module.widget()->findChild<KColorButton *>(QStringLiteral("kcfg_BorderColor"));
}

QCheckBox *useThemeColorCheckBox(KWin::ActiveBorderConfigModule &module)
{
    return module.widget()->findChild<QCheckBox *>(QStringLiteral("kcfg_UseThemeColor"));
}

KColorButton *dragPreviewColorButton(KWin::ActiveBorderConfigModule &module)
{
    return module.widget()->findChild<KColorButton *>(QStringLiteral("kcfg_DragPreviewColor"));
}

KColorButton *groupUnderlayColorButton(KWin::ActiveBorderConfigModule &module)
{
    return module.widget()->findChild<KColorButton *>(QStringLiteral("kcfg_GroupUnderlayColor"));
}

QDoubleSpinBox *groupUnderlayExtensionSpinBox(KWin::ActiveBorderConfigModule &module)
{
    return module.widget()->findChild<QDoubleSpinBox *>(QStringLiteral("kcfg_GroupUnderlayExtension"));
}

QColor defaultBorderColor()
{
    return QColor(0x2a, 0x82, 0xda);
}

QColor defaultDragPreviewColor()
{
    return QColor(0x2a, 0x82, 0xda, 64);
}

QColor defaultGroupUnderlayColor()
{
    return QColor(0x2a, 0x82, 0xda, 0x66);
}

void dbusTargetIsExact()
{
    CHECK(KWin::ActiveBorderConfigModule::effectService() == QStringLiteral("org.kde.KWin"));
    CHECK(KWin::ActiveBorderConfigModule::effectPath() == QStringLiteral("/Effects"));
    CHECK(KWin::ActiveBorderConfigModule::effectInterface() == QStringLiteral("org.kde.kwin.Effects"));
    CHECK(KWin::ActiveBorderConfigModule::effectInterface() != QStringLiteral("org.kde.KWin.Effects"));
    CHECK(KWin::ActiveBorderConfigModule::effectMethod() == QStringLiteral("reconfigureEffect"));
    CHECK(KWin::ActiveBorderConfigModule::effectName() == QStringLiteral("omnitiler-active-border"));
}

void dbusErrorClassification()
{
    const QDBusMessage methodCall = QDBusMessage::createMethodCall(
        KWin::ActiveBorderConfigModule::effectService(),
        KWin::ActiveBorderConfigModule::effectPath(),
        KWin::ActiveBorderConfigModule::effectInterface(),
        KWin::ActiveBorderConfigModule::effectMethod());
    const QDBusMessage reply = methodCall.createReply();
    CHECK(reply.type() == QDBusMessage::ReplyMessage);
    CHECK(!KWin::ActiveBorderConfigModule::isEffectReconfigureFailed(reply));
    const QDBusMessage error = QDBusMessage::createError(QStringLiteral("org.freedesktop.DBus.Error.ServiceUnknown"), QStringLiteral("not found"));
    CHECK(error.type() == QDBusMessage::ErrorMessage);
    CHECK(KWin::ActiveBorderConfigModule::isEffectReconfigureFailed(error));
    const QDBusMessage invalid;
    CHECK(invalid.type() == QDBusMessage::InvalidMessage);
    CHECK(KWin::ActiveBorderConfigModule::isEffectReconfigureFailed(invalid));
    CHECK(methodCall.type() == QDBusMessage::MethodCallMessage);
    CHECK(KWin::ActiveBorderConfigModule::isEffectReconfigureFailed(methodCall));
}

void reconfigureRequestFailsWithoutKwin()
{
    const QDBusMessage serviceUnknown = QDBusMessage::createError(QStringLiteral("org.freedesktop.DBus.Error.ServiceUnknown"), QStringLiteral("not found"));
    CHECK(KWin::ActiveBorderConfigModule::isEffectReconfigureFailed(serviceUnknown));
    const QDBusMessage unknownInterface = QDBusMessage::createError(QStringLiteral("org.freedesktop.DBus.Error.UnknownInterface"), QStringLiteral("no such interface"));
    CHECK(KWin::ActiveBorderConfigModule::isEffectReconfigureFailed(unknownInterface));
}

class FailingReconfigureModule : public KWin::ActiveBorderConfigModule
{
public:
    using KWin::ActiveBorderConfigModule::ActiveBorderConfigModule;
    bool requestEffectReconfigure() override
    {
        return false;
    }
};

class SucceedingReconfigureModule : public KWin::ActiveBorderConfigModule
{
public:
    using KWin::ActiveBorderConfigModule::ActiveBorderConfigModule;
    bool requestEffectReconfigure() override
    {
        return true;
    }
};

class CountingReconfigureModule : public KWin::ActiveBorderConfigModule
{
public:
    using KWin::ActiveBorderConfigModule::ActiveBorderConfigModule;
    bool requestEffectReconfigure() override
    {
        ++calls;
        return succeed;
    }
    int calls = 0;
    bool succeed = false;
};

void failedHotApplyKeepsNeedsSave()
{
    FailingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QDoubleSpinBox *width = borderWidthSpinBox(module);
    CHECK(width != nullptr);
    if (!width) {
        return;
    }
    CHECK(!module.needsSave());
    width->setValue(7.5);
    CHECK(module.needsSave());
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("BorderWidth"), 0.0) == 7.5);
    CHECK(module.needsSave());
}

void successfulHotApplyClearsNeedsSave()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QDoubleSpinBox *width = borderWidthSpinBox(module);
    CHECK(width != nullptr);
    if (!width) {
        return;
    }
    const double freshValue = (width->value() == 8.5) ? 6.5 : 8.5;
    width->setValue(freshValue);
    CHECK(module.needsSave());
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("BorderWidth"), 0.0) == freshValue);
    CHECK(!module.needsSave());
}

void cleanSaveClearsNeedsSave()
{
    KWin::ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.load();
    CHECK(!module.needsSave());
    module.save();
    CHECK(!module.needsSave());
}

void failedHotApplyRetriesOnSecondSaveWithoutEdit()
{
    CountingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QDoubleSpinBox *width = borderWidthSpinBox(module);
    CHECK(width != nullptr);
    if (!width) {
        return;
    }
    const double target = (width->value() == 7.5) ? 6.5 : 7.5;
    width->setValue(target);
    CHECK(module.needsSave());
    module.save();
    CHECK(module.calls == 1);
    CHECK(borderGroup().readEntry(QStringLiteral("BorderWidth"), 0.0) == target);
    CHECK(module.needsSave());
    module.save();
    CHECK(module.calls == 2);
    CHECK(module.needsSave());
    module.succeed = true;
    module.save();
    CHECK(module.calls == 3);
    CHECK(!module.needsSave());
    module.save();
    CHECK(module.calls == 3);
    CHECK(!module.needsSave());
}

void borderSerializationRoundtrip()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QDoubleSpinBox *width = borderWidthSpinBox(module);
    CHECK(width != nullptr);
    if (!width) {
        return;
    }
    width->setValue(9.5);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("BorderWidth"), 0.0) == 9.5);

    KWin::ActiveBorderConfigModule reloaded(nullptr, KPluginMetaData());
    reloaded.load();
    QDoubleSpinBox *reloadedWidth = borderWidthSpinBox(reloaded);
    CHECK(reloadedWidth != nullptr);
    if (reloadedWidth) {
        CHECK(reloadedWidth->value() == 9.5);
    }
}

void borderDefaultsReset()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QDoubleSpinBox *width = borderWidthSpinBox(module);
    CHECK(width != nullptr);
    if (!width) {
        return;
    }
    width->setValue(9.5);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("BorderWidth"), 0.0) == 9.5);
    module.defaults();
    CHECK(width->value() == 3.0);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("BorderWidth"), 3.0) == 3.0);
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::borderWidth() == 3.0);
}

void borderColorSerializationRoundtrip()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    KColorButton *color = borderColorButton(module);
    CHECK(color != nullptr);
    if (!color) {
        return;
    }
    const QColor target(0x11, 0x22, 0x33);
    color->setColor(target);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("BorderColor"), QColor()) == target);

    KWin::ActiveBorderConfigModule reloaded(nullptr, KPluginMetaData());
    reloaded.load();
    KColorButton *reloadedColor = borderColorButton(reloaded);
    CHECK(reloadedColor != nullptr);
    if (reloadedColor) {
        CHECK(reloadedColor->color() == target);
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::borderColor() == target);
}

void borderColorDefaultsReset()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    KColorButton *color = borderColorButton(module);
    CHECK(color != nullptr);
    if (!color) {
        return;
    }
    const QColor custom(0x11, 0x22, 0x33);
    color->setColor(custom);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("BorderColor"), QColor()) == custom);
    module.defaults();
    CHECK(color->color() == defaultBorderColor());
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("BorderColor"), defaultBorderColor()) == defaultBorderColor());
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::borderColor() == defaultBorderColor());
}

void effectConfigReloadReflectsStoredValues()
{
    KWin::ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.load();
    {
        KConfigGroup group = borderGroup();
        group.writeEntry(QStringLiteral("BorderWidth"), 12.5);
        group.sync();
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::borderWidth() == 12.5);
    {
        KConfigGroup group = borderGroup();
        group.writeEntry(QStringLiteral("BorderWidth"), 3.0);
        group.sync();
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::borderWidth() == 3.0);
}

void useThemeColorMissingKeyDefaultsToChecked()
{
    {
        KConfigGroup group = borderGroup();
        group.deleteEntry(QStringLiteral("UseThemeColor"));
        group.sync();
    }
    KWin::ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    QCheckBox *checkBox = useThemeColorCheckBox(module);
    CHECK(checkBox != nullptr);
    module.load();
    if (checkBox) {
        CHECK(checkBox->isChecked());
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::useThemeColor());
}

void useThemeColorSerializationRoundtrip()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QCheckBox *checkBox = useThemeColorCheckBox(module);
    CHECK(checkBox != nullptr);
    if (!checkBox) {
        return;
    }
    checkBox->setChecked(false);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("UseThemeColor"), true) == false);

    KWin::ActiveBorderConfigModule reloaded(nullptr, KPluginMetaData());
    reloaded.load();
    QCheckBox *reloadedCheckBox = useThemeColorCheckBox(reloaded);
    CHECK(reloadedCheckBox != nullptr);
    if (reloadedCheckBox) {
        CHECK(!reloadedCheckBox->isChecked());
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(!KWin::ActiveBorderConfig::useThemeColor());
}

void useThemeColorDefaultsReset()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QCheckBox *checkBox = useThemeColorCheckBox(module);
    CHECK(checkBox != nullptr);
    if (!checkBox) {
        return;
    }
    checkBox->setChecked(false);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("UseThemeColor"), true) == false);
    module.defaults();
    CHECK(checkBox->isChecked());
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("UseThemeColor"), true) == true);
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::useThemeColor());
}

void useThemeColorToggleMarksDirty()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QCheckBox *checkBox = useThemeColorCheckBox(module);
    CHECK(checkBox != nullptr);
    if (!checkBox) {
        return;
    }
    CHECK(!module.needsSave());
    checkBox->setChecked(!checkBox->isChecked());
    CHECK(module.needsSave());
}

void useThemeColorOnlyHotApplyRetry()
{
    CountingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QCheckBox *checkBox = useThemeColorCheckBox(module);
    CHECK(checkBox != nullptr);
    if (!checkBox) {
        return;
    }
    checkBox->setChecked(false);
    CHECK(module.needsSave());
    module.save();
    CHECK(module.calls == 1);
    CHECK(borderGroup().readEntry(QStringLiteral("UseThemeColor"), true) == false);
    CHECK(module.needsSave());
    module.save();
    CHECK(module.calls == 2);
    CHECK(module.needsSave());
    module.succeed = true;
    module.save();
    CHECK(module.calls == 3);
    CHECK(!module.needsSave());
}

void descriptorDrivenControlsMatchScriptIdentity()
{
    // Effect identity renders the same descriptor-driven page: tab titles
    // and page order, enum rows with tooltips, gap bounds and dialog
    // labels with buddy bindings. (Full cross-tab persistence lives in the
    // descriptor test on the script identity; both share one page.)
    using namespace OmnitilerKdeSettings;
    KWin::ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.load();
    QTabWidget *tabs = module.widget()->findChild<QTabWidget *>(QStringLiteral("settingsTabs"));
    CHECK(tabs != nullptr);
    if (tabs) {
        CHECK(tabs->count() == kKdeTabCount);
        for (int i = 0; i < kKdeTabCount && i < tabs->count(); ++i) {
            CHECK(tabs->tabText(i) == kKdeTabs[i].title.toString());
            CHECK(tabs->widget(i) != nullptr);
            if (tabs->widget(i)) {
                CHECK(tabs->widget(i)->objectName() == QString::fromUtf8(kKdeTabs[i].page_widget));
            }
        }
    }
    QComboBox *axis = module.widget()->findChild<QComboBox *>(QStringLiteral("sameAxisMoveCombo"));
    CHECK(axis != nullptr);
    if (axis) {
        const auto *field = KWin::KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("sameAxisMove"));
        CHECK(field != nullptr);
        if (field) {
            CHECK(axis->count() == field->option_count);
            for (int i = 0; i < field->option_count && i < axis->count(); ++i) {
                CHECK(axis->itemText(i) == field->options[i].label.toString());
                CHECK(axis->itemData(i).toString() == QString::fromUtf8(field->options[i].token));
            }
        }
    }
    const auto *inner = KWin::KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("innerGap"));
    auto *spin = module.widget()->findChild<QSpinBox *>(QStringLiteral("innerGapSpinBox"));
    auto *label = module.widget()->findChild<QLabel *>(QStringLiteral("label_innerGap"));
    CHECK(inner != nullptr);
    CHECK(spin != nullptr);
    CHECK(label != nullptr);
    if (spin && inner) {
        CHECK(spin->minimum() == KWin::KdeSettingsAdapter::intMinimum(inner));
        CHECK(spin->maximum() == KWin::KdeSettingsAdapter::intMaximum(inner));
    }
    if (label && inner) {
        CHECK(label->text() == inner->ui_label.toString());
        CHECK(label->buddy() != nullptr);
    }
    // Appearance numerics and the theme checkbox render from the same
    // descriptors: double ranges, exact dialog text, escaped tab ampersand.
    const auto *width = KWin::KdeSettingsAdapter::findFieldById("border.width");
    auto *widthSpin = module.widget()->findChild<QDoubleSpinBox *>(QStringLiteral("kcfg_BorderWidth"));
    auto *widthLabel = module.widget()->findChild<QLabel *>(QStringLiteral("label_BorderWidth"));
    CHECK(width != nullptr);
    CHECK(widthSpin != nullptr);
    CHECK(widthLabel != nullptr);
    if (widthSpin && width) {
        CHECK(widthSpin->minimum() == KWin::KdeSettingsAdapter::doubleMinimum(width));
        CHECK(widthSpin->maximum() == KWin::KdeSettingsAdapter::doubleMaximum(width));
    }
    if (widthLabel && width) {
        CHECK(widthLabel->text() == width->ui_label.toString());
    }
    const auto *theme = KWin::KdeSettingsAdapter::findFieldById("border.use-theme");
    auto *themeCheck = module.widget()->findChild<QCheckBox *>(QStringLiteral("kcfg_UseThemeColor"));
    CHECK(theme != nullptr);
    CHECK(themeCheck != nullptr);
    if (themeCheck && theme) {
        CHECK(themeCheck->text() == theme->ui_label.toString());
    }
    if (tabs) {
        CHECK(tabs->tabText(0) == QStringLiteral("Tiling && Workspaces"));
    }
}

void unifiedControlsArePresent()
{
    KWin::ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.load();
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("workspaceModeCombo")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("innerGapSpinBox")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("outerGapSpinBox")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("scriptStatusLabel")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("kcfg_BorderWidth")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("kcfg_DragPreviewColor")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("kcfg_GroupUnderlayColor")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("kcfg_GroupUnderlayExtension")) != nullptr);
    CHECK(module.widget()->findChild<QWidget *>(QStringLiteral("shortcutApplyButton")) != nullptr);
}

void translucentColorsAlphaChannelEnabled()
{
    KWin::ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    module.load();
    KColorButton *preview = dragPreviewColorButton(module);
    CHECK(preview != nullptr);
    KColorButton *underlay = groupUnderlayColorButton(module);
    CHECK(underlay != nullptr);
    if (preview) {
        CHECK(preview->isAlphaChannelEnabled());
    }
    if (underlay) {
        CHECK(underlay->isAlphaChannelEnabled());
    }
}

void translucentColorsMissingKeysDefaultToTranslucent()
{
    {
        KConfigGroup group = borderGroup();
        group.deleteEntry(QStringLiteral("DragPreviewColor"));
        group.deleteEntry(QStringLiteral("GroupUnderlayColor"));
        group.sync();
    }
    KWin::ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    KColorButton *preview = dragPreviewColorButton(module);
    KColorButton *underlay = groupUnderlayColorButton(module);
    CHECK(preview != nullptr);
    CHECK(underlay != nullptr);
    module.load();
    if (preview) {
        CHECK(preview->color() == defaultDragPreviewColor());
    }
    if (underlay) {
        CHECK(underlay->color() == defaultGroupUnderlayColor());
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::dragPreviewColor() == defaultDragPreviewColor());
    CHECK(KWin::ActiveBorderConfig::groupUnderlayColor() == defaultGroupUnderlayColor());
}

void dragPreviewColorSaveReadbackAndDefaultsReset()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    KColorButton *preview = dragPreviewColorButton(module);
    CHECK(preview != nullptr);
    if (!preview) {
        return;
    }
    const QColor target(0x11, 0x22, 0x33, 128);
    preview->setColor(target);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("DragPreviewColor"), QColor()) == target);

    KWin::ActiveBorderConfigModule reloaded(nullptr, KPluginMetaData());
    reloaded.load();
    KColorButton *reloadedPreview = dragPreviewColorButton(reloaded);
    CHECK(reloadedPreview != nullptr);
    if (reloadedPreview) {
        CHECK(reloadedPreview->color() == target);
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::dragPreviewColor() == target);

    module.defaults();
    CHECK(preview->color() == defaultDragPreviewColor());
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("DragPreviewColor"), defaultDragPreviewColor()) == defaultDragPreviewColor());
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::dragPreviewColor() == defaultDragPreviewColor());
}

void groupUnderlayColorSaveReadbackAndDefaultsReset()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    KColorButton *underlay = groupUnderlayColorButton(module);
    CHECK(underlay != nullptr);
    if (!underlay) {
        return;
    }
    const QColor target(0x11, 0x22, 0x33, 128);
    underlay->setColor(target);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("GroupUnderlayColor"), QColor()) == target);

    KWin::ActiveBorderConfigModule reloaded(nullptr, KPluginMetaData());
    reloaded.load();
    KColorButton *reloadedUnderlay = groupUnderlayColorButton(reloaded);
    CHECK(reloadedUnderlay != nullptr);
    if (reloadedUnderlay) {
        CHECK(reloadedUnderlay->color() == target);
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::groupUnderlayColor() == target);

    module.defaults();
    CHECK(underlay->color() == defaultGroupUnderlayColor());
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("GroupUnderlayColor"), defaultGroupUnderlayColor()) == defaultGroupUnderlayColor());
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::groupUnderlayColor() == defaultGroupUnderlayColor());
}

void dragPreviewColorOnlyHotApplyRetry()
{
    CountingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    KColorButton *preview = dragPreviewColorButton(module);
    CHECK(preview != nullptr);
    if (!preview) {
        return;
    }
    const QColor target = (preview->color() == QColor(0x11, 0x22, 0x33, 128))
        ? QColor(0x44, 0x55, 0x66, 200)
        : QColor(0x11, 0x22, 0x33, 128);
    preview->setColor(target);
    CHECK(module.needsSave());
    module.save();
    CHECK(module.calls == 1);
    CHECK(borderGroup().readEntry(QStringLiteral("DragPreviewColor"), QColor()) == target);
    CHECK(module.needsSave());
    module.succeed = true;
    module.save();
    CHECK(module.calls == 2);
    CHECK(!module.needsSave());
}

void groupUnderlayExtensionDefaultsToBorderWidth()
{
    {
        KConfigGroup group = borderGroup();
        group.deleteEntry(QStringLiteral("GroupUnderlayExtension"));
        group.deleteEntry(QStringLiteral("BorderWidth"));
        group.sync();
    }
    KWin::ActiveBorderConfigModule module(nullptr, KPluginMetaData());
    QDoubleSpinBox *extension = groupUnderlayExtensionSpinBox(module);
    CHECK(extension != nullptr);
    module.load();
    if (extension) {
        CHECK(extension->value() == -1.0);
        CHECK(extension->minimum() == -1.0);
        CHECK(extension->decimals() == 0);
        CHECK(extension->specialValueText() == QStringLiteral("Match border width"));
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::groupUnderlayExtension() == -1.0);
    // Effect FFI policy: the default sentinel follows the current border
    // width. Gated out of settings-only builds (no Cargo staticlib there);
    // the full effect build keeps these gates.
#ifdef OMNITILER_HAVE_EFFECT_FFI
    // Default sentinel follows the current border width: changing the
    // border width to 5 while the extension stays at default resolves to 5.
    CHECK(KWin::groupUnderlayEffectiveExtension(KWin::ActiveBorderConfig::groupUnderlayExtension(), 3.0) == 3.0);
    {
        KConfigGroup group = borderGroup();
        group.writeEntry(QStringLiteral("BorderWidth"), 5.0);
        group.sync();
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::borderWidth() == 5.0);
    CHECK(KWin::ActiveBorderConfig::groupUnderlayExtension() == -1.0);
    CHECK(KWin::groupUnderlayEffectiveExtension(KWin::ActiveBorderConfig::groupUnderlayExtension(), KWin::ActiveBorderConfig::borderWidth()) == 5.0);
    // Explicit 0 renders as-is and does not follow the border width.
    {
        KConfigGroup group = borderGroup();
        group.writeEntry(QStringLiteral("GroupUnderlayExtension"), 0.0);
        group.sync();
    }
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::groupUnderlayEffectiveExtension(KWin::ActiveBorderConfig::groupUnderlayExtension(), KWin::ActiveBorderConfig::borderWidth()) == 0.0);
#endif
    {
        KConfigGroup group = borderGroup();
        group.deleteEntry(QStringLiteral("GroupUnderlayExtension"));
        group.writeEntry(QStringLiteral("BorderWidth"), 3.0);
        group.sync();
    }
    KWin::ActiveBorderConfig::self()->read();
}

void groupUnderlayExtensionDefaultsReset()
{
    SucceedingReconfigureModule module(nullptr, KPluginMetaData());
    module.load();
    QDoubleSpinBox *extension = groupUnderlayExtensionSpinBox(module);
    CHECK(extension != nullptr);
    if (!extension) {
        return;
    }
    extension->setValue(7.0);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("GroupUnderlayExtension"), -1.0) == 7.0);
    module.defaults();
    CHECK(extension->value() == -1.0);
    module.save();
    CHECK(borderGroup().readEntry(QStringLiteral("GroupUnderlayExtension"), -1.0) == -1.0);
    KWin::ActiveBorderConfig::self()->read();
    CHECK(KWin::ActiveBorderConfig::groupUnderlayExtension() == -1.0);
}

} // namespace

int main(int argc, char **argv)
{
    // Unit 1 hard gate: isolate from the live session bus before any
    // QDBusConnection::sessionBus() initialization (ActiveBorderConfigModule
    // creates the live KGlobalAccelStore in its constructor).
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
        std::fprintf(stderr, "usage: %s dbus|hotapply|border|config\n", argv[0]);
        return EXIT_FAILURE;
    }

    const QString scenario = QString::fromLocal8Bit(argv[1]);
    if (scenario == QStringLiteral("dbus")) {
        dbusTargetIsExact();
        dbusErrorClassification();
        reconfigureRequestFailsWithoutKwin();
    } else if (scenario == QStringLiteral("hotapply")) {
        cleanSaveClearsNeedsSave();
        failedHotApplyKeepsNeedsSave();
        successfulHotApplyClearsNeedsSave();
        failedHotApplyRetriesOnSecondSaveWithoutEdit();
    } else if (scenario == QStringLiteral("border")) {
        useThemeColorMissingKeyDefaultsToChecked();
        borderSerializationRoundtrip();
        borderDefaultsReset();
        borderColorSerializationRoundtrip();
        borderColorDefaultsReset();
        useThemeColorSerializationRoundtrip();
        useThemeColorDefaultsReset();
        useThemeColorToggleMarksDirty();
        useThemeColorOnlyHotApplyRetry();
        dragPreviewColorSaveReadbackAndDefaultsReset();
        dragPreviewColorOnlyHotApplyRetry();
        groupUnderlayColorSaveReadbackAndDefaultsReset();
        groupUnderlayExtensionDefaultsToBorderWidth();
        groupUnderlayExtensionDefaultsReset();
        translucentColorsAlphaChannelEnabled();
        translucentColorsMissingKeysDefaultToTranslucent();
        unifiedControlsArePresent();
        descriptorDrivenControlsMatchScriptIdentity();
    } else if (scenario == QStringLiteral("config")) {
        effectConfigReloadReflectsStoredValues();
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
