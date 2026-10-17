#include "kdesettings_adapter.h"

#include <QCheckBox>
#include <QComboBox>
#include <QDoubleSpinBox>
#include <QLabel>
#include <QSpinBox>
#include <QTabWidget>
#include <QWidget>

#include <cstring>

namespace KWin
{
namespace KdeSettingsAdapter
{

namespace
{

constexpr char kScriptGroup[] = "Script-omnitiler-kwin";

bool isScriptField(const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    return field != nullptr && std::strcmp(field->group, kScriptGroup) == 0;
}

QString fieldKey(const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    return QString::fromUtf8(field->key);
}

QString fieldDefaultToken(const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    return QString::fromUtf8(field->native_default);
}

} // namespace

QString scriptConfigGroupName()
{
    return QString::fromUtf8(kScriptGroup);
}

const OmnitilerKdeSettings::KdeFieldDescriptor *findFieldById(const char *fieldId)
{
    if (fieldId == nullptr) {
        return nullptr;
    }
    for (const auto &field : OmnitilerKdeSettings::kKdeFields) {
        if (std::strcmp(field.field_id, fieldId) == 0) {
            return &field;
        }
    }
    return nullptr;
}

const OmnitilerKdeSettings::KdeFieldDescriptor *findScriptFieldByKey(const QString &key)
{
    const QByteArray wanted = key.toUtf8();
    for (const auto &field : OmnitilerKdeSettings::kKdeFields) {
        if (isScriptField(&field) && wanted == field.key) {
            return &field;
        }
    }
    return nullptr;
}

QString readEnumToken(const KConfigGroup &group,
                      const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (field == nullptr || field->option_count <= 0 || field->options == nullptr) {
        return QString();
    }
    const QString fallback = fieldDefaultToken(field);
    const QString value = group.readEntry(fieldKey(field), fallback);
    for (int i = 0; i < field->option_count; ++i) {
        if (value == QString::fromUtf8(field->options[i].token)) {
            return value;
        }
    }
    return fallback;
}

QString readWorkspaceMode(const KConfigGroup &group)
{
    const auto *field = findScriptFieldByKey(QStringLiteral("workspaceMode"));
    if (field == nullptr) {
        return QStringLiteral("per-output-local");
    }
    // Unvalidated by design: an unknown startup-only token stays staged
    // dirty so the next save repairs it (combo falls back visually).
    return group.readEntry(fieldKey(field), fieldDefaultToken(field));
}

int readBoundedInt(const KConfigGroup &group,
                   const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (field == nullptr) {
        return 0;
    }
    const int fallback = intDefault(field);
    if (!group.hasKey(fieldKey(field))) {
        return fallback;
    }
    bool ok = false;
    const int parsed = group.readEntry(fieldKey(field), QString()).toInt(&ok);
    if (ok && parsed >= intMinimum(field) && parsed <= intMaximum(field)) {
        return parsed;
    }
    return fallback;
}

bool isBoundedIntRawValid(const KConfigGroup &group,
                          const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (field == nullptr) {
        return true;
    }
    if (!group.hasKey(fieldKey(field))) {
        return true;
    }
    bool ok = false;
    const int parsed = group.readEntry(fieldKey(field), QString()).toInt(&ok);
    return ok && parsed >= intMinimum(field) && parsed <= intMaximum(field);
}

int intMinimum(const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (field == nullptr) {
        return 0;
    }
    return QString::fromUtf8(field->min_value).toInt();
}

int intMaximum(const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (field == nullptr) {
        return 0;
    }
    return QString::fromUtf8(field->max_value).toInt();
}

int intDefault(const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (field == nullptr) {
        return 0;
    }
    return QString::fromUtf8(field->native_default).toInt();
}

void populateEnumCombo(QComboBox *combo,
                       const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (combo == nullptr || field == nullptr || field->options == nullptr) {
        return;
    }
    combo->clear();
    for (int i = 0; i < field->option_count; ++i) {
        const auto &option = field->options[i];
        combo->addItem(option.label.toString(), QString::fromUtf8(option.token));
        const QString tip = option.tip.toString();
        if (!tip.isEmpty()) {
            combo->setItemData(i, tip, Qt::ToolTipRole);
        }
    }
}

void selectEnumCombo(QComboBox *combo, const QString &value, const QString &fallback)
{
    if (combo == nullptr) {
        return;
    }
    const int index = combo->findData(value);
    const int fallbackIndex = combo->findData(fallback);
    combo->setCurrentIndex(index >= 0 ? index : fallbackIndex);
}

void configureBoundedSpinBox(QSpinBox *spinBox,
                            const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (spinBox == nullptr || field == nullptr) {
        return;
    }
    spinBox->setMinimum(intMinimum(field));
    spinBox->setMaximum(intMaximum(field));
}

double doubleMinimum(const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (field == nullptr) {
        return 0.0;
    }
    return QString::fromUtf8(field->min_value).toDouble();
}

double doubleMaximum(const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (field == nullptr) {
        return 0.0;
    }
    return QString::fromUtf8(field->max_value).toDouble();
}

double doubleDefault(const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (field == nullptr) {
        return 0.0;
    }
    return QString::fromUtf8(field->native_default).toDouble();
}

void configureDoubleSpinBox(QDoubleSpinBox *spinBox,
                           const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (spinBox == nullptr || field == nullptr) {
        return;
    }
    spinBox->setMinimum(doubleMinimum(field));
    spinBox->setMaximum(doubleMaximum(field));
}

void applyFieldLabel(QWidget *root,
                     const OmnitilerKdeSettings::KdeFieldDescriptor *field)
{
    if (root == nullptr || field == nullptr) {
        return;
    }
    QWidget *control = root->findChild<QWidget *>(QString::fromUtf8(field->widget));
    if (control == nullptr) {
        return;
    }
    const QString text = field->ui_label.toString();
    if (auto *check = qobject_cast<QCheckBox *>(control)) {
        check->setText(text);
        return;
    }
    const QList<QLabel *> labels = root->findChildren<QLabel *>();
    for (QLabel *label : labels) {
        if (label != nullptr && label->buddy() == control) {
            label->setText(text);
            return;
        }
    }
}

void configureTabTitles(QWidget *root)
{
    if (root == nullptr) {
        return;
    }
    // Pages are reparented under the tab widget's internal stack on add,
    // so locate the holder by membership rather than by parent widget.
    const QList<QTabWidget *> holders = root->findChildren<QTabWidget *>();
    for (const auto &tab : OmnitilerKdeSettings::kKdeTabs) {
        QWidget *page = root->findChild<QWidget *>(QString::fromUtf8(tab.page_widget));
        if (page == nullptr) {
            continue;
        }
        for (QTabWidget *tabs : holders) {
            const int index = tabs != nullptr ? tabs->indexOf(page) : -1;
            if (index >= 0) {
                tabs->setTabText(index, tab.title.toString());
                break;
            }
        }
    }
}

void configureNativeControls(QWidget *root)
{
    if (root == nullptr) {
        return;
    }
    configureTabTitles(root);
    for (const auto &field : OmnitilerKdeSettings::kKdeFields) {
        QWidget *control = root->findChild<QWidget *>(QString::fromUtf8(field.widget));
        if (auto *box = qobject_cast<QComboBox *>(control)) {
            populateEnumCombo(box, &field);
        } else if (auto *spin = qobject_cast<QSpinBox *>(control)) {
            configureBoundedSpinBox(spin, &field);
        } else if (auto *doubleSpin = qobject_cast<QDoubleSpinBox *>(control)) {
            configureDoubleSpinBox(doubleSpin, &field);
        }
        applyFieldLabel(root, &field);
    }
}

QStringList scriptKeysInRowOrder()
{
    QStringList keys;
    for (const auto &field : OmnitilerKdeSettings::kKdeFields) {
        if (isScriptField(&field)) {
            keys.append(QString::fromUtf8(field.key));
        }
    }
    return keys;
}

QVariantMap scriptDefaults()
{
    QVariantMap defaults;
    for (const auto &field : OmnitilerKdeSettings::kKdeFields) {
        if (!isScriptField(&field)) {
            continue;
        }
        if (field.option_count > 0 && field.options != nullptr) {
            defaults.insert(QString::fromUtf8(field.key), fieldDefaultToken(&field));
        } else {
            defaults.insert(QString::fromUtf8(field.key), intDefault(&field));
        }
    }
    return defaults;
}

} // namespace KdeSettingsAdapter
} // namespace KWin
