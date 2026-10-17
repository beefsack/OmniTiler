#pragma once

// Focused native adapter over the generated KDE settings descriptors.
//
// Owns the KDE tab/field/enum presentation contract the KCMs render: tab
// order and page widgets, control bindings in native row order, dialog
// labels, enum rows with tooltips, numeric bounds/defaults, and the
// tolerant storage readers behind load()/save()/defaults(). Values
// (defaults, bounds, tokens, order) originate in the shared catalog and
// arrive here through kwin/native-effect/generated/kde_settings_descriptors.h;
// this adapter only renders them onto the retained Designer layout.
//
// User-visible strings translate on use (descriptor.lazy.toString()), so no
// global QString is ever constructed here. Ordinary field staging and Save
// must never invoke the native operation panels (shortcut Apply/Force/Revert,
// window-edge Fix/Revert): this adapter stages no values and exposes no
// operation setters.

#include <KConfigGroup>
#include <QString>
#include <QStringList>
#include <QVariantMap>

#include "generated/kde_settings_descriptors.h"

class QCheckBox;
class QComboBox;
class QDoubleSpinBox;
class QLabel;
class QSpinBox;
class QTabWidget;
class QWidget;

namespace KWin
{
namespace KdeSettingsAdapter
{

/// kwinrc group owning the seven script settings (workspace mode, move,
/// admission, refill, gaps). Effect keys stay KConfigXT-managed.
QString scriptConfigGroupName();

/// Descriptor lookups over the generated tables (nullptr when absent; the
/// caller falls back to the catalog default, never to an empty control).
const OmnitilerKdeSettings::KdeFieldDescriptor *findFieldById(const char *fieldId);
const OmnitilerKdeSettings::KdeFieldDescriptor *findScriptFieldByKey(const QString &key);

/// Tolerant storage readers preserving the native fallback contract:
/// - readEnumToken returns the stored token when it names a generated
///   option, else the descriptor default (missing keys read as default).
/// - readWorkspaceMode returns the stored entry verbatim (startup-only key:
///   an unknown token stays staged dirty so the next save repairs it).
/// - readBoundedInt clamps to [min, max] with the descriptor default for
///   missing/non-numeric/out-of-range entries; isBoundedIntRawValid reports
///   whether the stored entry needs that repair.
QString readEnumToken(const KConfigGroup &group,
                      const OmnitilerKdeSettings::KdeFieldDescriptor *field);
QString readWorkspaceMode(const KConfigGroup &group);
int readBoundedInt(const KConfigGroup &group,
                   const OmnitilerKdeSettings::KdeFieldDescriptor *field);
bool isBoundedIntRawValid(const KConfigGroup &group,
                          const OmnitilerKdeSettings::KdeFieldDescriptor *field);

int intMinimum(const OmnitilerKdeSettings::KdeFieldDescriptor *field);
int intMaximum(const OmnitilerKdeSettings::KdeFieldDescriptor *field);
int intDefault(const OmnitilerKdeSettings::KdeFieldDescriptor *field);

double doubleMinimum(const OmnitilerKdeSettings::KdeFieldDescriptor *field);
double doubleMaximum(const OmnitilerKdeSettings::KdeFieldDescriptor *field);
double doubleDefault(const OmnitilerKdeSettings::KdeFieldDescriptor *field);

/// Thin control configuration over the retained Designer layout (no layout,
/// field, order or binding changes): enum rows with translated labels plus
/// tooltips where the descriptor carries one, combo selection with catalog
/// fallback, int/double spin bounds from the descriptor (value, decimals,
/// suffix and special-value text stay in .ui), dialog labels from the
/// descriptor (buddy QLabel text, checkbox text) and tab titles in
/// descriptor order. Native-only rows (preview color), status text and
/// operation panels are never touched here.
void populateEnumCombo(QComboBox *combo,
                       const OmnitilerKdeSettings::KdeFieldDescriptor *field);
void selectEnumCombo(QComboBox *combo, const QString &value, const QString &fallback);
void configureBoundedSpinBox(QSpinBox *spinBox,
                            const OmnitilerKdeSettings::KdeFieldDescriptor *field);
void configureDoubleSpinBox(QDoubleSpinBox *spinBox,
                           const OmnitilerKdeSettings::KdeFieldDescriptor *field);
void applyFieldLabel(QWidget *root,
                     const OmnitilerKdeSettings::KdeFieldDescriptor *field);
void configureTabTitles(QWidget *root);
/// Rendered configuration for every exposed field and tab: one descriptor
/// loop over the retained widgets. Called once before the module connects
/// its change signals; ordinary staging and Save never call it again.
void configureNativeControls(QWidget *root);

/// Script keys in descriptor row order and their catalog defaults, for the
/// draft snapshot, dirty tracking and Defaults. Hidden and foreign keys are
/// never listed here and never written through this adapter.
QStringList scriptKeysInRowOrder();
QVariantMap scriptDefaults();

} // namespace KdeSettingsAdapter
} // namespace KWin
