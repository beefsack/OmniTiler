#pragma once

#include <KCModule>

#include <QDBusMessage>
#include <QSet>
#include <QString>
#include <QStringList>
#include <QThread>
#include <QVariantMap>

#include <functional>

#include "shortcutreconciler.h"
#include "ui_unifiedsettings.h"

Q_DECLARE_METATYPE(std::function<void()>)

namespace KWin
{

// Dedicated worker-thread host for expensive shortcut discovery and
// mutation. Tasks are plain functors executed on the worker thread; results
// travel back through taskDone (queued to the UI thread). The worker never
// touches widgets or module state: every task captures its stores and
// parameters by value and reports through the finish handler.
class ShortcutTaskWorker : public QObject
{
    Q_OBJECT

public:
    using Task = std::function<void()>;
    explicit ShortcutTaskWorker(QObject *parent = nullptr);

public Q_SLOTS:
    void execute(Task task, quint64 generation);

Q_SIGNALS:
    void taskDone(quint64 generation);
};

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
    // Async execution seam for the responsiveness tests: production live
    // stores run shortcut work on the worker thread, injected hermetic
    // stores stay synchronous for deterministic tests unless this is
    // enabled explicitly.
    void setShortcutAsyncEnabled(bool enabled);
    bool isShortcutBusy() const;
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

Q_SIGNALS:
    void shortcutTaskRequested(std::function<void()> task, quint64 generation);

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
    void runShortcutApplySync(const char *operation);
    // Shared UI-thread completion for one Apply result (both modes). On a
    // zero-write holder refusal it returns true so the caller builds a
    // fresh force preview (async: a second worker task) and runs the single
    // conflict modal there.
    bool finishShortcutApplyResult(const ShortcutApplyResult &result, const char *operation);
    void runShortcutRevert(const char *operation);
    void runShortcutRevertSync(const char *operation);
    void finishShortcutRevertResult(const ShortcutRevertResult &result, const char *operation);
    // Force runs only on the accepted-modal path with the exact preview the
    // modal showed; revalidation inside fails stale with zero writes.
    void applyConfirmedForcePreview(const ShortcutForcePreview &preview);
    void finishForceApplyResult(const ShortcutForceApplyResult &result);
    void finishForcePreviewForApply(const ShortcutForcePreview &preview, const char *operation);
    // Read-only evidence behind refreshShortcutState (no widgets, no module
    // state): safe to collect on the worker thread from captured stores.
    struct ShortcutRefreshEvidence {
        QString error;
        QList<ClearedAction> cleared;
        QList<ShortcutTuple> tuples;
        bool tuplesOk = false;
        QList<ShortcutRowDisplay> displays;
        bool displaysOk = false;
        QString displaysError;
        KeyedOccupancyResult keyed;
    };
    static bool collectRefreshEvidence(ShortcutStore *store, ClearedActionsStore *cleared,
                                       const QSet<QString> &disabledDraft, bool authenticStaged,
                                       ShortcutRefreshEvidence *evidence);
    void presentRefreshEvidence(const ShortcutRefreshEvidence &evidence);
    void requestRefreshShortcutState();
    void finishShortcutRefresh(const ShortcutRefreshEvidence &evidence, quint64 restageEpoch);
    // Compatible-preset computation (no widgets): safe for the worker.
    struct ShortcutCompatibleComputed {
        QString error;
        QSet<QString> disabled;
    };
    void finishCompatiblePreset(const ShortcutCompatibleComputed &computed);
    // Serialized async dispatch: work runs on the worker thread (async mode)
    // or inline (sync mode); finish always runs on the UI thread and is
    // dropped when its generation is stale.
    void dispatchShortcutTask(const QString &busyStatus, std::function<void()> work,
                              std::function<void()> finish);
    void onShortcutTaskDone(quint64 generation);
    void setShortcutControlsEnabled(bool enabled);
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
    // Serialized worker-thread execution for expensive shortcut work (see
    // ShortcutTaskWorker). m_shortcutBusy guards every shortcut entry
    // point against concurrent or reentrant execution; the staged draft
    // and intent are captured by value at dispatch, so edits during flight
    // only re-stage and schedule a refresh via m_shortcutRefreshPending.
    // Destruction stops the thread before the stores are deleted, so no
    // store call can race it.
    QThread *m_shortcutThread = nullptr;
    ShortcutTaskWorker *m_shortcutWorker = nullptr;
    bool m_shortcutBusy = false;
    quint64 m_shortcutGeneration = 0;
    bool m_shortcutRefreshPending = false;
    // Draft/restage epoch, separate from the task generation (a superseded
    // task must still complete so busy/pending stay consistent): load(),
    // setShortcutStores(), defaults(), and user draft revisions bump the
    // draft epoch; load()/setShortcutStores() additionally stage a restage
    // claim bound to the bumped epoch. A refresh finish stages only when
    // its dispatch-time claim is still current, so older evidence never
    // stages after a newer revision, and the claiming finish replaces (not
    // unions) the draft from its own fresh tuples.
    quint64 m_shortcutDraftEpoch = 0;
    quint64 m_shortcutRestageEpoch = 0;
    // Set at the top of the destructor: no new shortcut work may dispatch
    // once teardown has started, so a dialog resolving mid-destruction can
    // neither run store calls on freed stores nor resurrect the worker.
    bool m_shortcutDestroying = false;
    std::function<void()> m_shortcutFinish;
    bool m_shortcutAsync = false;

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
