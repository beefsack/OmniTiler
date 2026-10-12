#pragma once

#include <KCModule>

#include <QDBusMessage>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QVariantMap>

#include <functional>

#include "shortcutreconciler.h"
#include "ui_unifiedsettings.h"

namespace KWin
{

class UnifiedSettingsModule : public KCModule
{
    Q_OBJECT

public:
    explicit UnifiedSettingsModule(QObject *parent, const KPluginMetaData &data);
    ~UnifiedSettingsModule() override;

    static QString effectService();
    static QString effectPath();
    static QString effectInterface();
    static QString effectMethod();
    static QString effectName();
    static bool isEffectReconfigureFailed(const QDBusMessage &reply);
    virtual bool requestEffectReconfigure();

    static QString scriptService();
    static QString scriptPath();
    static QString scriptInterface();
    static QString scriptMethod();
    virtual bool requestScriptReconfigure();

    void setShortcutStores(ShortcutStore *store, ClearedActionsStore *cleared);
    void setShortcutConfirmHandler(std::function<bool(const QString &, const QString &)> handler);
    // Modal dialog hooks below the widget layer (tests inject these;
    // production shows the scrollable conflict QDialog and QMessageBox
    // result dialogs). Force prompt returns true for Force Apply, false
    // for Cancel; forceEnabled false renders Force disabled.
    void setShortcutInformationHandler(std::function<void(const QString &, const QString &)> handler);
    void setShortcutErrorHandler(std::function<void(const QString &, const QString &)> handler);
    void setShortcutForcePromptHandler(std::function<bool(const QString &, const QString &, bool)> handler);
    QString shortcutStatusText() const;
    QString shortcutErrorText() const;
    // Last conflict dialog text (empty when no conflicts). This is the
    // exact plain-text list shown in the scrollable conflict dialog.
    QString shortcutForcePreviewText() const;
    // Staged Keep/Disable draft as sorted catalog IDs ("component/action").
    // Empty means Keep every binding. The Authentic preset additionally
    // stages canonical assignment for every enabled binding; otherwise
    // enabled bindings are preserved at their current assignments.
    QStringList shortcutDisabledIds() const;
    bool shortcutAuthenticStaged() const;
    void refreshShortcutState();
    virtual bool confirmShortcutAction(const QString &title, const QString &text);

    // Ordered conflict dialog text over the reconciler-owned combined
    // order (blockers first, then clear rows). Lists every holder with
    // chord, display names, removed/retained keys, and the actual lock
    // relocation carried by the preview.
    static QString buildConflictDialogText(const ShortcutForcePreview &preview, bool authenticStaged);
    virtual void showShortcutInformation(const QString &title, const QString &text);
    virtual void showShortcutError(const QString &title, const QString &text);
    virtual bool askShortcutForce(const QString &title, const QString &text, bool forceEnabled);

    QString scriptStatusText() const;
    bool isScriptRestartRequired() const;
    bool isGapReconfigurePending() const;

    void refreshWindowConflicts();
    void requestWindowFix(const QString &key);
    void requestWindowRevert(const QString &key);

public Q_SLOTS:
    void load() override;
    void save() override;
    void defaults() override;
    void requestShortcutApply();
    void requestShortcutRevert();
    void requestShortcutPresetAuthentic();
    void requestShortcutPresetCompatible();

private:
    void runShortcutApply(const char *operation);
    void runShortcutRevert(const char *operation);
    // Force runs only on the accepted-modal path with the exact preview the
    // modal showed; revalidation inside fails stale with zero writes.
    void applyConfirmedForcePreview(const ShortcutForcePreview &preview);
    static QString buildConflictRowText(const ShortcutRowDisplay &row, bool disabled, bool authenticStaged);
    void updateShortcutPresentation();
    void refreshShortcutConflictList(const QList<ShortcutRowDisplay> &rows);
    void onShortcutDraftChanged();

    QVariantMap currentScriptValues() const;
    void updateScriptState();

    void runWindowConflictWrite(const QString &key, const char *operation);
    void updateWindowConflictPresentation();

    ::Ui::UnifiedSettings m_ui;
    bool m_effectReconfigurePending = false;
    ShortcutStore *m_shortcutStore = nullptr;
    // Durable cleared-ID list in the project-owned config. Never
    // read/written/removed except via confirmed Force
    // (union-persist before clearing) and Revert (consume then empty).
    ClearedActionsStore *m_clearedStore = nullptr;
    bool m_ownsShortcutStores = false;
    std::function<bool(const QString &, const QString &)> m_shortcutConfirm;
    std::function<void(const QString &, const QString &)> m_shortcutInformation;
    std::function<void(const QString &, const QString &)> m_shortcutErrorHandler;
    std::function<bool(const QString &, const QString &, bool)> m_shortcutForcePrompt;
    QString m_shortcutStatus;
    QString m_shortcutError;
    // Last conflict dialog text shown in the scrollable modal.
    QString m_forcePreviewText;
    // Staged Keep/Disable draft as catalog IDs ("component/action").
    // Checked list rows are kept; unchecked rows are disabled. Staged only:
    // ordinary Save, startup, and installation never apply it.
    QSet<QString> m_shortcutDisabledDraft;
    // Staged Authentic intent for every enabled binding: assignment to the
    // canonical chords on the next confirmed Apply/Force. False preserves
    // each enabled binding at its current assignment (customized chords and
    // empty assignments included). Staged only by the Authentic preset;
    // Compatible, load, and defaults restage Keep (false).
    bool m_shortcutAuthenticStaged = false;

    QVariantMap m_loadedScriptValues;
    bool m_loadedInnerGapRawValid = true;
    bool m_loadedOuterGapRawValid = true;
    bool m_scriptRestartRequired = false;
    // A failed gap reconfigure send arms a retry on the next save: an
    // unchanged save still sends while this is set, and Apply stays enabled
    // for it through updateScriptState. Cleared on a sent request or load.
    bool m_gapReconfigurePending = false;
    QString m_scriptStatus;

    QString m_windowConflictError;
};

} // namespace KWin
