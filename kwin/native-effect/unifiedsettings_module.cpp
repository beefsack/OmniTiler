#include "unifiedsettings_module.h"
#include "activeborderconfig.h"
#include "kdesettings_adapter.h"

#include <KConfigGroup>
#include <KSharedConfig>
#include <KCMultiDialog>
#include <KLocalizedString>
#include <QLoggingCategory>

#include <QComboBox>
#include <QDBusConnection>
#include <QDBusInterface>
#include <QDBusMessage>
#include <QDialog>
#include <QDialogButtonBox>
#include <QEventLoop>
#include <QLabel>
#include <QListWidget>
#include <QListWidgetItem>
#include <QMessageBox>
#include <QMetaObject>
#include <QPlainTextEdit>
#include <QPushButton>
#include <QSpinBox>
#include <QVBoxLayout>

#include <memory>

Q_LOGGING_CATEGORY(lcScriptConfig, "omnitiler.script-config", QtInfoMsg)
Q_LOGGING_CATEGORY(lcWindowConflicts, "omnitiler.window-conflicts", QtInfoMsg)

namespace KWin
{

ShortcutTaskWorker::ShortcutTaskWorker(QObject *parent)
    : QObject(parent)
{
}

void ShortcutTaskWorker::execute(Task task, quint64 generation)
{
    if (task) {
        task();
    }
    Q_EMIT taskDone(generation);
}

namespace
{

QStringList liveChangedNames(bool gapChanged, bool sameAxisMoveChanged, bool fixedPredicateChanged, bool bornMaximizedChanged, bool anchorFallbackChanged, bool noSpaceChanged, bool refillChanged)
{
    QStringList names;
    if (gapChanged) {
        names.append(QStringLiteral("gaps"));
    }
    if (sameAxisMoveChanged) {
        names.append(QStringLiteral("same-axis move"));
    }
    if (fixedPredicateChanged) {
        names.append(QStringLiteral("fixed-size predicate"));
    }
    if (bornMaximizedChanged) {
        names.append(QStringLiteral("born-maximized"));
    }
    if (anchorFallbackChanged) {
        names.append(QStringLiteral("anchor fallback"));
    }
    if (noSpaceChanged) {
        names.append(QStringLiteral("no-space result"));
    }
    if (refillChanged) {
        names.append(QStringLiteral("migration source refill"));
    }
    return names;
}

void logScriptConfig(const char *operation, const char *stage, const char *outcome, const QString &detail)
{
    QString bounded = detail;
    if (bounded.size() > 256) {
        bounded.truncate(256);
    }
    qCInfo(lcScriptConfig).noquote() << QStringLiteral("omnitiler.script-config op=%1 stage=%2 outcome=%3 %4")
                                            .arg(QString::fromUtf8(operation), QString::fromUtf8(stage),
                                                 QString::fromUtf8(outcome), bounded);
}

struct WindowConflictState {
    bool tiling = true;
    bool maximize = true;
    int borders = 0;
};

WindowConflictState readWindowConflictState()
{
    const KSharedConfig::Ptr config = KSharedConfig::openConfig(QStringLiteral("kwinrc"));
    config->reparseConfiguration();
    const KConfigGroup group(config, QStringLiteral("Windows"));
    WindowConflictState state;
    state.tiling = group.readEntry(QStringLiteral("ElectricBorderTiling"), true);
    state.maximize = group.readEntry(QStringLiteral("ElectricBorderMaximize"), true);
    state.borders = group.readEntry(QStringLiteral("ElectricBorders"), 0);
    return state;
}

void logWindowConflict(const char *operation, const QString &setting, const char *outcome, const QString &reason)
{
    qCInfo(lcWindowConflicts).noquote() << QStringLiteral("op=%1 setting=%2 outcome=%3 reason=%4")
                                               .arg(QString::fromUtf8(operation), setting,
                                                    QString::fromUtf8(outcome), reason);
}

} // namespace

UnifiedSettingsModule::UnifiedSettingsModule(QObject *parent, const KPluginMetaData &data)
    : KCModule(parent, data)
{
    ActiveBorderConfig::instance(QStringLiteral("kwinrc"));
    m_ui.setupUi(widget());
    addConfig(ActiveBorderConfig::self(), widget());

    // Rendered native configuration comes from the generated
    // descriptors through one focused adapter loop; the Designer layout
    // keeps widget names, order, buddies and fallback text.
    KdeSettingsAdapter::configureNativeControls(widget());

    connect(m_ui.workspaceModeCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &UnifiedSettingsModule::updateScriptState);
    connect(m_ui.sameAxisMoveCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &UnifiedSettingsModule::updateScriptState);
    connect(m_ui.fixedSizePredicateCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &UnifiedSettingsModule::updateScriptState);
    connect(m_ui.bornMaximizedCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &UnifiedSettingsModule::updateScriptState);
    connect(m_ui.anchorFallbackCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &UnifiedSettingsModule::updateScriptState);
    connect(m_ui.noSpaceResultCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &UnifiedSettingsModule::updateScriptState);
    connect(m_ui.migrationSourceRefillCombo, QOverload<int>::of(&QComboBox::currentIndexChanged), this,
            &UnifiedSettingsModule::updateScriptState);
    connect(m_ui.innerGapSpinBox, QOverload<int>::of(&QSpinBox::valueChanged), this,
            &UnifiedSettingsModule::updateScriptState);
    connect(m_ui.outerGapSpinBox, QOverload<int>::of(&QSpinBox::valueChanged), this,
            &UnifiedSettingsModule::updateScriptState);

    m_shortcutStore = createLiveShortcutStore();
    // Project-owned cleared-actions config only. Constructing the
    // backends writes nothing. Opening, refreshing, previewing,
    // or cancelling never writes config.
    m_clearedStore = createLiveClearedActionsStore(defaultClearedActionsPath());
    m_ownsShortcutStores = true;
    // Static catalogs are read-only after first construction: touch them on
    // the UI thread before the worker can, so no first-use race remains.
    (void)shortcutProjectCatalog().size();
    (void)shortcutConflictTable().size();
    // Serialized worker thread for expensive shortcut work (production
    // live path runs async; injected hermetic stores stay synchronous).
    qRegisterMetaType<std::function<void()>>("std::function<void()>");
    m_shortcutThread = new QThread(this);
    m_shortcutWorker = new ShortcutTaskWorker();
    m_shortcutWorker->moveToThread(m_shortcutThread);
    connect(m_shortcutWorker, &ShortcutTaskWorker::taskDone, this, &UnifiedSettingsModule::onShortcutTaskDone,
            Qt::QueuedConnection);
    connect(this, &UnifiedSettingsModule::shortcutTaskRequested, m_shortcutWorker, &ShortcutTaskWorker::execute,
            Qt::QueuedConnection);
    // Canonical teardown: the worker deletes itself on its own thread when
    // the thread finishes. The destructor still blocks in quit()/wait()
    // first so an in-flight task (and the stores it uses) always outlives
    // the worker.
    connect(m_shortcutThread, &QThread::finished, m_shortcutWorker, &QObject::deleteLater);
    m_shortcutThread->start();
    m_shortcutAsync = true;
    connect(m_ui.shortcutApplyButton, &QPushButton::clicked, this, &UnifiedSettingsModule::requestShortcutApply);
    connect(m_ui.shortcutRevertButton, &QPushButton::clicked, this, &UnifiedSettingsModule::requestShortcutRevert);
    if (m_ui.shortcutAuthenticButton != nullptr) {
        connect(m_ui.shortcutAuthenticButton, &QPushButton::clicked, this,
                &UnifiedSettingsModule::requestShortcutPresetAuthentic);
    }
    if (m_ui.shortcutCompatibleButton != nullptr) {
        connect(m_ui.shortcutCompatibleButton, &QPushButton::clicked, this,
                &UnifiedSettingsModule::requestShortcutPresetCompatible);
    }
    if (m_ui.shortcutConflictList != nullptr) {
        connect(m_ui.shortcutConflictList, &QListWidget::itemChanged, this,
                &UnifiedSettingsModule::onShortcutDraftChanged);
    }
    // No discovery here: load() (framework) and setShortcutStores() (tests)
    // dispatch it explicitly. Dispatching a live task from the constructor
    // would also race the injected-store swap in every hermetic test.
    m_shortcutStatus = QStringLiteral("Working: discovering shortcut state.");
    updateShortcutPresentation();

    if (m_ui.windowTilingFixButton != nullptr) {
        connect(m_ui.windowTilingFixButton, &QPushButton::clicked, this,
                [this] { requestWindowFix(QStringLiteral("ElectricBorderTiling")); });
    }
    if (m_ui.windowTilingRevertButton != nullptr) {
        connect(m_ui.windowTilingRevertButton, &QPushButton::clicked, this,
                [this] { requestWindowRevert(QStringLiteral("ElectricBorderTiling")); });
    }
    if (m_ui.windowMaximizeFixButton != nullptr) {
        connect(m_ui.windowMaximizeFixButton, &QPushButton::clicked, this,
                [this] { requestWindowFix(QStringLiteral("ElectricBorderMaximize")); });
    }
    if (m_ui.windowMaximizeRevertButton != nullptr) {
        connect(m_ui.windowMaximizeRevertButton, &QPushButton::clicked, this,
                [this] { requestWindowRevert(QStringLiteral("ElectricBorderMaximize")); });
    }
    if (m_ui.windowBordersFixButton != nullptr) {
        connect(m_ui.windowBordersFixButton, &QPushButton::clicked, this,
                [this] { requestWindowFix(QStringLiteral("ElectricBorders")); });
    }
    refreshWindowConflicts();
    if (m_ui.virtualDesktopsOpenButton != nullptr) {
        connect(m_ui.virtualDesktopsOpenButton, &QPushButton::clicked, this,
                &UnifiedSettingsModule::requestOpenVirtualDesktops);
    }
    refreshVirtualDesktopWarning();

    m_scriptRestartRequired = false;
    m_scriptStatus = QStringLiteral("No pending script setting in this dialog.");
    if (m_ui.scriptStatusLabel != nullptr) {
        m_ui.scriptStatusLabel->setText(m_scriptStatus);
    }
    updateScriptState();
}

UnifiedSettingsModule::~UnifiedSettingsModule()
{
    m_shortcutDestroying = true;
    // Stop worker work before the stores are deleted: quit waits for the
    // in-flight task (if any) to finish while the stores are still alive,
    // then no store call can race destruction. Queued completions to this
    // dying object are discarded by Qt.
    if (m_shortcutThread != nullptr) {
        m_shortcutThread->quit();
        m_shortcutThread->wait();
    }
    m_shortcutWorker = nullptr;
    if (m_ownsShortcutStores) {
        delete m_shortcutStore;
        delete m_clearedStore;
    }
}

QString UnifiedSettingsModule::effectService()
{
    return QStringLiteral("org.kde.KWin");
}

QString UnifiedSettingsModule::effectPath()
{
    return QStringLiteral("/Effects");
}

QString UnifiedSettingsModule::effectInterface()
{
    return QStringLiteral("org.kde.kwin.Effects");
}

QString UnifiedSettingsModule::effectMethod()
{
    return QStringLiteral("reconfigureEffect");
}

QString UnifiedSettingsModule::effectName()
{
    return QStringLiteral("omnitiler-active-border");
}

bool UnifiedSettingsModule::isEffectReconfigureFailed(const QDBusMessage &reply)
{
    return reply.type() != QDBusMessage::ReplyMessage;
}

bool UnifiedSettingsModule::requestEffectReconfigure()
{
    QDBusInterface interface(effectService(), effectPath(), effectInterface(), QDBusConnection::sessionBus());
    const QDBusMessage reply = interface.call(effectMethod(), effectName());
    return !isEffectReconfigureFailed(reply);
}

QString UnifiedSettingsModule::scriptService()
{
    return QStringLiteral("org.kde.KWin");
}

QString UnifiedSettingsModule::scriptPath()
{
    return QStringLiteral("/KWin");
}

QString UnifiedSettingsModule::scriptInterface()
{
    return QStringLiteral("org.kde.KWin");
}

QString UnifiedSettingsModule::scriptMethod()
{
    return QStringLiteral("reconfigure");
}

bool UnifiedSettingsModule::requestScriptReconfigure()
{
    QDBusInterface interface(scriptService(), scriptPath(), scriptInterface(), QDBusConnection::sessionBus());
    if (!interface.isValid()) {
        return false;
    }
    return QDBusConnection::sessionBus().send(
        QDBusMessage::createMethodCall(scriptService(), scriptPath(), scriptInterface(), scriptMethod()));
}

void UnifiedSettingsModule::setShortcutStores(ShortcutStore *store, ClearedActionsStore *cleared)
{
    // Never swap the stores under in-flight work: the in-flight finish is
    // discarded via the generation bump and the flight itself is waited
    // out while its stores are still alive. Reentrant shortcut requests
    // during the wait are rejected by the busy guard.
    ++m_shortcutGeneration;
    if (m_shortcutBusy && m_shortcutWorker != nullptr) {
        QEventLoop loop;
        QObject::connect(m_shortcutWorker, &ShortcutTaskWorker::taskDone, &loop, &QEventLoop::quit);
        while (m_shortcutBusy) {
            loop.exec();
        }
        QObject::disconnect(m_shortcutWorker, &ShortcutTaskWorker::taskDone, &loop, &QEventLoop::quit);
    }
    m_shortcutBusy = false;
    m_shortcutFinish = nullptr;
    m_shortcutRefreshPending = false;
    // Injected hermetic stores stay synchronous for deterministic tests;
    // production live stores run on the worker thread. Tests opt into
    // async explicitly via setShortcutAsyncEnabled.
    m_shortcutAsync = false;
    if (m_ownsShortcutStores) {
        delete m_shortcutStore;
        delete m_clearedStore;
        m_shortcutStore = nullptr;
        m_clearedStore = nullptr;
        m_ownsShortcutStores = false;
    }
    m_shortcutStore = store;
    m_clearedStore = cleared;
    // Reopening stages live-disabled rows (present own assignments that are
    // empty restage Disable. Default-unbound rows (canonical key 0) are at
    // their canonical empty binding, not a disablement, so they never
    // auto-stage. Missing rows are not Disable; absence and read failure
    // stage Keep (Authentic intent cleared). Refresh never clobbers the
    // staged draft. Staging rides the refresh evidence (same tuples the
    // presentation uses); no separate store read. The restage claim is
    // bound to the bumped draft epoch (see requestRefreshShortcutState).
    m_shortcutDisabledDraft.clear();
    m_shortcutAuthenticStaged = false;
    m_shortcutRestageEpoch = ++m_shortcutDraftEpoch;
    refreshShortcutState();
}

void UnifiedSettingsModule::setShortcutAsyncEnabled(bool enabled)
{
    m_shortcutAsync = enabled;
}

bool UnifiedSettingsModule::isShortcutBusy() const
{
    return m_shortcutBusy;
}

void UnifiedSettingsModule::setShortcutControlsEnabled(bool enabled)
{
    if (m_ui.shortcutApplyButton != nullptr) {
        m_ui.shortcutApplyButton->setEnabled(enabled);
    }
    if (m_ui.shortcutRevertButton != nullptr) {
        m_ui.shortcutRevertButton->setEnabled(enabled);
    }
    if (m_ui.shortcutAuthenticButton != nullptr) {
        m_ui.shortcutAuthenticButton->setEnabled(enabled);
    }
    if (m_ui.shortcutCompatibleButton != nullptr) {
        m_ui.shortcutCompatibleButton->setEnabled(enabled);
    }
    if (m_ui.shortcutConflictList != nullptr) {
        m_ui.shortcutConflictList->setEnabled(enabled);
    }
}

void UnifiedSettingsModule::dispatchShortcutTask(const QString &busyStatus, std::function<void()> work,
                                                 std::function<void()> finish)
{
    if (m_shortcutDestroying) {
        return;
    }
    if (!m_shortcutAsync || m_shortcutWorker == nullptr || m_shortcutThread == nullptr) {
        // Synchronous hermetic path: identical behavior inline.
        if (work) {
            work();
        }
        if (finish) {
            finish();
        }
        return;
    }
    if (m_shortcutBusy) {
        return;
    }
    ++m_shortcutGeneration;
    const quint64 generation = m_shortcutGeneration;
    m_shortcutBusy = true;
    m_shortcutFinish = std::move(finish);
    m_shortcutStatus = busyStatus;
    updateShortcutPresentation();
    setShortcutControlsEnabled(false);
    Q_EMIT shortcutTaskRequested(work, generation);
}

void UnifiedSettingsModule::onShortcutTaskDone(quint64 generation)
{
    // At most one task is ever in flight (dispatch requires idle), so any
    // completion means the worker is idle again. A superseded generation
    // (e.g. stores swapped mid-flight) drops its finish and schedules
    // nothing: the swap path refreshes explicitly afterwards.
    if (generation != m_shortcutGeneration) {
        m_shortcutFinish = nullptr;
        m_shortcutBusy = false;
        setShortcutControlsEnabled(true);
        return;
    }
    std::function<void()> finish;
    finish.swap(m_shortcutFinish);
    m_shortcutBusy = false;
    setShortcutControlsEnabled(true);
    if (finish) {
        finish();
    }
    if (m_shortcutRefreshPending) {
        requestRefreshShortcutState();
    }
}

void UnifiedSettingsModule::setShortcutConfirmHandler(std::function<bool(const QString &, const QString &)> handler)
{
    m_shortcutConfirm = std::move(handler);
}

void UnifiedSettingsModule::setShortcutInformationHandler(
    std::function<void(const QString &, const QString &)> handler)
{
    m_shortcutInformation = std::move(handler);
}

void UnifiedSettingsModule::setShortcutErrorHandler(
    std::function<void(const QString &, const QString &)> handler)
{
    m_shortcutErrorHandler = std::move(handler);
}

void UnifiedSettingsModule::setShortcutForcePromptHandler(
    std::function<bool(const QString &, const QString &, bool)> handler)
{
    m_shortcutForcePrompt = std::move(handler);
}

QString UnifiedSettingsModule::shortcutStatusText() const
{
    return m_shortcutStatus;
}

QString UnifiedSettingsModule::shortcutErrorText() const
{
    return m_shortcutError;
}

QString UnifiedSettingsModule::shortcutForcePreviewText() const
{
    return m_forcePreviewText;
}

void UnifiedSettingsModule::showShortcutInformation(const QString &title, const QString &text)
{
    if (m_shortcutInformation) {
        m_shortcutInformation(title, text);
        return;
    }
    QMessageBox::information(widget(), title, text);
}

void UnifiedSettingsModule::showShortcutError(const QString &title, const QString &text)
{
    QString brief = text.simplified();
    if (brief.size() > 240) {
        brief.truncate(237);
        brief += QStringLiteral("...");
    }
    if (m_shortcutErrorHandler) {
        m_shortcutErrorHandler(title, brief);
        return;
    }
    QMessageBox::critical(widget(), title, brief);
}

bool UnifiedSettingsModule::askShortcutForce(const QString &title, const QString &text, bool forceEnabled)
{
    m_forcePreviewText = text;
    if (m_shortcutForcePrompt) {
        return m_shortcutForcePrompt(title, text, forceEnabled);
    }
    // One minimal dialog: read-only scrollable plain-text conflict list
    // with the buttons always visible. Blocked previews carry the reason
    // in the text with Force Apply disabled.
    QDialog dialog(widget());
    dialog.setObjectName(QStringLiteral("shortcutConflictDialog"));
    dialog.setWindowTitle(title);
    dialog.setModal(true);
    auto *layout = new QVBoxLayout(&dialog);
    auto *view = new QPlainTextEdit(&dialog);
    view->setObjectName(QStringLiteral("shortcutConflictText"));
    view->setPlainText(text);
    view->setReadOnly(true);
    view->setMinimumSize(560, 320);
    layout->addWidget(view);
    auto *buttons = new QDialogButtonBox(&dialog);
    buttons->setObjectName(QStringLiteral("shortcutConflictButtons"));
    QPushButton *force = buttons->addButton(QStringLiteral("Force Apply"), QDialogButtonBox::AcceptRole);
    force->setObjectName(QStringLiteral("shortcutConflictForceButton"));
    force->setEnabled(forceEnabled);
    QPushButton *cancel = buttons->addButton(QStringLiteral("Cancel"), QDialogButtonBox::RejectRole);
    cancel->setObjectName(QStringLiteral("shortcutConflictCancelButton"));
    layout->addWidget(buttons);
    QObject::connect(buttons, &QDialogButtonBox::accepted, &dialog, &QDialog::accept);
    QObject::connect(buttons, &QDialogButtonBox::rejected, &dialog, &QDialog::reject);
    if (!forceEnabled) {
        // A disabled Force button cannot be the default; Cancel takes it.
        cancel->setDefault(true);
    } else {
        force->setDefault(true);
    }
    const bool accepted = dialog.exec() == QDialog::Accepted;
    return accepted && forceEnabled;
}

QStringList UnifiedSettingsModule::shortcutDisabledIds() const
{
    QStringList out(m_shortcutDisabledDraft.begin(), m_shortcutDisabledDraft.end());
    out.sort();
    return out;
}

bool UnifiedSettingsModule::shortcutAuthenticStaged() const
{
    return m_shortcutAuthenticStaged;
}

void UnifiedSettingsModule::requestShortcutPresetAuthentic()
{
    // Authentic explicitly stages canonical assignment for every enabled
    // binding: the draft resets to Keep and the intent stages canonical,
    // committed only by a later confirmed Apply/Force. No store writes.
    if (m_shortcutBusy) {
        return;
    }
    m_shortcutDisabledDraft.clear();
    m_shortcutAuthenticStaged = true;
    ShortcutDiag::log(QtDebugMsg, "preset", "authentic", "staged", QStringLiteral("disabled=0 authentic=1"));
    refreshShortcutState();
}

void UnifiedSettingsModule::requestShortcutPresetCompatible()
{
    // Compatible resets the draft to Keep (clearing any staged Authentic
    // intent), then disables the known-conflicting canonical rows plus rows
    // whose canonical chord currently collides with a live holder or a live
    // foreign wire default (actives cleared but defaults still claim the
    // chord). Deterministic catalog order; the rest stay Keep at their
    // current assignments. Never writes the daemon and never invents
    // replacement chords. Any failed read/query aborts honestly with the
    // previous draft and intent kept.
    if (m_shortcutBusy) {
        return;
    }
    if (m_shortcutStore == nullptr || m_clearedStore == nullptr) {
        m_shortcutError = QStringLiteral("reconciler is not configured");
        updateShortcutPresentation();
        return;
    }
    ShortcutStore *store = m_shortcutStore;
    auto computed = std::make_shared<ShortcutCompatibleComputed>();
    dispatchShortcutTask(QStringLiteral("Working: computing compatible preset."),
                         [store, computed]() {
                             ShortcutCompatibleComputed out;
                             const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
                             QList<ShortcutTuple> tuples;
                             QString readError;
                             if (!store->readAll(&tuples, &readError)) {
                                 out.error = QStringLiteral("Compatible preset unavailable: %1").arg(
                                     readError.isEmpty() ? QStringLiteral("tuple query failed") : readError);
                                 *computed = out;
                                 return;
                             }
                             QSet<QString> colliding;
                             for (const ShortcutCatalogEntry &entry : catalog) {
                                 QList<ShortcutKeyHolder> holders;
                                 QString queryError;
                                 if (!store->shortcutsByKey(entry.canonicalKey, &holders, &queryError)) {
                                     out.error = QStringLiteral("Compatible preset unavailable: %1").arg(
                                         queryError.isEmpty() ? QStringLiteral("holder query failed") : queryError);
                                     *computed = out;
                                     return;
                                 }
                                 for (const ShortcutKeyHolder &holder : holders) {
                                     if (!ShortcutReconciler::isHolderExempt(holder.component, holder.action,
                                                                             entry.canonicalKey)) {
                                         colliding.insert(shortcutCatalogId(entry.component, entry.action));
                                         break;
                                     }
                                 }
                                 if (!colliding.contains(shortcutCatalogId(entry.component, entry.action))
                                     && !ShortcutReconciler::foreignDefaultIdsForKey(entry.canonicalKey, tuples)
                                             .isEmpty()) {
                                     colliding.insert(shortcutCatalogId(entry.component, entry.action));
                                 }
                             }
                             const QStringList disabled = presetCompatibleDisabledIds(
                                 catalog, shortcutKnownConflictIds(), colliding);
                             out.disabled = QSet<QString>(disabled.begin(), disabled.end());
                             *computed = out;
                         },
                         [this, computed]() {
                             finishCompatiblePreset(*computed);
                         });
}

void UnifiedSettingsModule::finishCompatiblePreset(const ShortcutCompatibleComputed &computed)
{
    if (!computed.error.isEmpty()) {
        m_shortcutError = computed.error;
        ShortcutDiag::log(QtWarningMsg, "preset", "compatible", "unavailable", m_shortcutError);
        updateShortcutPresentation();
        return;
    }
    m_shortcutDisabledDraft = computed.disabled;
    m_shortcutAuthenticStaged = false;
    ShortcutDiag::log(QtDebugMsg, "preset", "compatible", "staged",
                      QStringLiteral("disabled=%1 authentic=0").arg(computed.disabled.size()));
    refreshShortcutState();
}

void UnifiedSettingsModule::onShortcutDraftChanged()
{
    if (m_ui.shortcutConflictList == nullptr) {
        return;
    }
    QSet<QString> draft;
    for (int i = 0; i < m_ui.shortcutConflictList->count(); ++i) {
        QListWidgetItem *item = m_ui.shortcutConflictList->item(i);
        if (item == nullptr) {
            continue;
        }
        if (item->checkState() != Qt::Checked) {
            const QString id = item->data(Qt::UserRole).toString();
            if (!id.isEmpty()) {
                draft.insert(id);
            }
        }
    }
    if (draft == m_shortcutDisabledDraft) {
        return;
    }
    // The state refresh is queued: rebuilding the list here would
    // delete the edited item while its change signal is still on the stack.
    // Manual edits preserve the staged Authentic intent for the rows that
    // stay enabled. Conflict previews are built fresh on each Apply, so
    // there is no pending preview to cancel here. During a worker flight
    // the list is disabled, so edits cannot race store work; the staged
    // draft is still recorded and a refresh follows the flight. The
    // revision bumps the draft epoch so an outstanding restage claim from
    // an older flight can no longer stage over it.
    m_shortcutDisabledDraft = draft;
    ++m_shortcutDraftEpoch;
    ShortcutDiag::log(QtDebugMsg, "preset", "draft", "edited",
                      QStringLiteral("disabled=%1 authentic=%2")
                          .arg(draft.size())
                          .arg(m_shortcutAuthenticStaged ? 1 : 0));
    updateShortcutPresentation();
    if (m_shortcutBusy) {
        m_shortcutRefreshPending = true;
        return;
    }
    QMetaObject::invokeMethod(this, &UnifiedSettingsModule::refreshShortcutState, Qt::QueuedConnection);
}

void UnifiedSettingsModule::requestShortcutApply()
{
    runShortcutApply("apply");
}

bool UnifiedSettingsModule::confirmShortcutAction(const QString &title, const QString &text)
{
    if (m_shortcutConfirm) {
        return m_shortcutConfirm(title, text);
    }
    return QMessageBox::question(widget(), title, text, QMessageBox::Yes | QMessageBox::No, QMessageBox::No) == QMessageBox::Yes;
}

void UnifiedSettingsModule::requestShortcutRevert()
{
    runShortcutRevert("revert");
}

void UnifiedSettingsModule::applyConfirmedForcePreview(const ShortcutForcePreview &preview)
{
    // The modal already confirmed this exact preview. Revalidation inside
    // applyForcedSelected compares the full live snapshot (owner, images,
    // holder rows, blockers, lock relocation) and the staged draft/intent;
    // stale state fails with zero writes and a result error.
    if (!preview.forceable) {
        return;
    }
    if (m_shortcutBusy) {
        return;
    }
    if (m_shortcutStore == nullptr || m_clearedStore == nullptr) {
        m_shortcutError = QStringLiteral("reconciler is not configured");
        ShortcutDiag::log(QtWarningMsg, "force-apply", "result", "failed",
                          QStringLiteral("reason=reconciler-unconfigured writes=0"));
        showShortcutError(QStringLiteral("Force Apply Shortcuts"), m_shortcutError);
        updateShortcutPresentation();
        return;
    }
    if (!m_shortcutAsync) {
        ShortcutReconciler reconciler(m_shortcutStore, m_clearedStore);
        finishForceApplyResult(
            reconciler.applyForcedSelected(preview, m_shortcutDisabledDraft, m_shortcutAuthenticStaged));
        return;
    }
    ShortcutStore *store = m_shortcutStore;
    ClearedActionsStore *cleared = m_clearedStore;
    const QSet<QString> disabledDraft = m_shortcutDisabledDraft;
    const bool authenticStaged = m_shortcutAuthenticStaged;
    auto result = std::make_shared<ShortcutForceApplyResult>();
    dispatchShortcutTask(QStringLiteral("Working: force-applying shortcuts."),
                         [store, cleared, preview, disabledDraft, authenticStaged, result]() {
                             ShortcutReconciler reconciler(store, cleared);
                             *result = reconciler.applyForcedSelected(preview, disabledDraft, authenticStaged);
                         },
                         [this, result]() {
                             finishForceApplyResult(*result);
                         });
}

void UnifiedSettingsModule::finishForceApplyResult(const ShortcutForceApplyResult &result)
{
    if (result.ok) {
        m_shortcutError.clear();
    } else {
        m_shortcutError = result.error.isEmpty() ? QStringLiteral("Force Apply failed") : result.error;
    }
    ShortcutDiag::log(result.ok ? QtInfoMsg : QtWarningMsg, "force-apply", "result",
                      result.ok ? "ok" : "failed",
                      result.ok ? QStringLiteral("writes=%1 authentic=%2")
                                      .arg(result.writes)
                                      .arg(m_shortcutAuthenticStaged ? 1 : 0)
                                 : QStringLiteral("reason=%1 writes=%2 authentic=%3")
                                       .arg(result.error)
                                       .arg(result.writes)
                                       .arg(m_shortcutAuthenticStaged ? 1 : 0));
    // Staged Authentic reset intent is committed by a successful Apply/Force
    // and consumed here; failed or declined attempts retain it. Logging
    // above uses the original staged intent.
    if (result.ok) {
        m_shortcutAuthenticStaged = false;
    }
    refreshShortcutState();
    if (result.ok) {
        showShortcutInformation(
            QStringLiteral("Force Apply Shortcuts"),
            QStringLiteral("Shortcuts applied."));
    } else {
        showShortcutError(QStringLiteral("Force Apply Shortcuts"), m_shortcutError);
    }
}

static QString holderDisplayLabel(const QString &component, const QString &action,
                                   const QString &componentFriendly, const QString &friendly)
{
    const QString id = QStringLiteral("%1/%2").arg(component, action);
    const bool hasFriendly = !componentFriendly.isEmpty() || !friendly.isEmpty();
    QString base;
    if (!hasFriendly) {
        base = id;
    } else {
        const QString display = QStringLiteral("%1/%2").arg(
            componentFriendly.isEmpty() ? component : componentFriendly,
            friendly.isEmpty() ? action : friendly);
        base = (display == id) ? id : QStringLiteral("%1 (%2)").arg(id, display);
    }
    return base;
}

QString UnifiedSettingsModule::buildConflictDialogText(const ShortcutForcePreview &preview, bool authenticStaged)
{
    QStringList lines;
    lines.append(QStringLiteral("Apply found %1 conflicting binding(s) with %2 disabled binding(s) kept out of scope "
                                "while enabled bindings %3. "
                                "Each row shows the chord, the holder, the exact keys removed, and the keys kept. "
                                "Force Apply revalidates the same staged selection; stale state aborts without writes.")
                     .arg(preview.mismatches.size() + preview.blockers.size())
                     .arg(preview.disabledIds.size())
                     .arg(authenticStaged ? QStringLiteral("take their canonical chords")
                                          : QStringLiteral("stay at their current assignments")));
    // Reconciler-owned combined order: blockers first, then clear rows.
    for (const ShortcutOrderedConflict &entry : ShortcutReconciler::orderedConflicts(preview)) {
        if (entry.isBlocker) {
            const ShortcutForceBlocker &blocker = preview.blockers.at(entry.index);
            lines.append(QStringLiteral("- %1 chord %2: found %3; removes none and keeps %4 "
                                        "(default-only claimant with no required binding to clear; "
                                        "unbind it manually before forcing).")
                             .arg(holderDisplayLabel(blocker.component, blocker.action,
                                                     blocker.componentFriendly, blocker.friendly),
                                  ShortcutReconciler::keyDisplayName(blocker.key),
                                  ShortcutReconciler::keysDisplayNames(blocker.active),
                                  ShortcutReconciler::keysDisplayNames(blocker.active)));
            continue;
        }
        const ShortcutForceMismatch &mismatch = preview.mismatches.at(entry.index);
        const QString chord = ShortcutReconciler::keysDisplayNames(mismatch.expectedPre);
        lines.append(QStringLiteral("- %1 chord %2: found %3; will remove %4 and keep %5.")
                         .arg(holderDisplayLabel(mismatch.component, mismatch.action,
                                                 mismatch.componentFriendly, mismatch.friendly),
                              chord,
                              ShortcutReconciler::keysDisplayNames(mismatch.actual),
                              ShortcutReconciler::keysDisplayNames(mismatch.expectedPre),
                              ShortcutReconciler::keysDisplayNames(mismatch.post)));
    }
    // Exemptions exactly as the preview carries them: the in-scope lock
    // relocation only when the binding actually moves, or an honest
    // out-of-scope line. An already-relocated lock emits no exempt line.
    // Nothing is inferred here.
    if (preview.lock.inScope) {
        if (preview.lock.actual != preview.lock.post) {
            const QString actualNames = ShortcutReconciler::keysDisplayNames(preview.lock.actual);
            const QString postNames = ShortcutReconciler::keysDisplayNames(preview.lock.post);
            QString lockLine = QStringLiteral("Exempt: %1/%2 found %3 will move to %4.")
                                   .arg(preview.lock.component, preview.lock.action, actualNames, postNames);
            // Preserved alternates: keys the relocation keeps (extra chords
            // beyond the replaced preimage) are named distinctly instead of
            // being folded into the move.
            QList<int> kept;
            for (int key : preview.lock.actual) {
                if (preview.lock.post.contains(key) && !kept.contains(key)) {
                    kept.append(key);
                }
            }
            if (!kept.isEmpty()) {
                lockLine += QStringLiteral(" Preserves %1.")
                                .arg(ShortcutReconciler::keysDisplayNames(kept));
            }
            if (preview.lock.authorizedHolderPresent) {
                // Approved System Monitor Meta+Esc exception: retained but
                // displaced, preserved and never cleared. No claim about
                // physical key delivery is made here.
                lockLine += QStringLiteral(" Authorized %1 retains its binding %2; retained but displaced, "
                                           "preserved and not cleared.")
                                .arg(holderDisplayLabel(preview.lock.authorizedComponent,
                                                        preview.lock.authorizedAction,
                                                        preview.lock.authorizedComponentFriendly,
                                                        preview.lock.authorizedFriendly),
                                     ShortcutReconciler::keysDisplayNames(preview.lock.authorizedActive));
            }
            lines.append(lockLine);
        }
    } else {
        lines.append(QStringLiteral("Exempt: Lock Session untouched (focus-right is not taking canonical)."));
    }
    if (!preview.forceable && !preview.error.isEmpty()) {
        lines.append(QStringLiteral("Force unavailable: %1.").arg(preview.error));
    }
    return lines.join(QStringLiteral("\n"));
}

void UnifiedSettingsModule::runShortcutApply(const char *operation)
{
    // Direct Apply without a preliminary confirmation. Conflicts surface
    // once through the conflict modal (Force Apply/Cancel); that modal is
    // the sole Force confirmation and reuses the force revalidation path.
    if (m_shortcutBusy) {
        return;
    }
    m_forcePreviewText.clear();
    if (m_shortcutStore == nullptr || m_clearedStore == nullptr) {
        m_shortcutError = QStringLiteral("reconciler is not configured");
        ShortcutDiag::log(QtWarningMsg, operation, "result", "failed",
                          QStringLiteral("reason=reconciler-unconfigured writes=0"));
        updateShortcutPresentation();
        showShortcutError(QStringLiteral("Apply Shortcuts"), m_shortcutError);
        return;
    }
    if (!m_shortcutAsync) {
        runShortcutApplySync(operation);
        return;
    }
    ShortcutStore *store = m_shortcutStore;
    ClearedActionsStore *cleared = m_clearedStore;
    const QSet<QString> disabledDraft = m_shortcutDisabledDraft;
    const bool authenticStaged = m_shortcutAuthenticStaged;
    auto result = std::make_shared<ShortcutApplyResult>();
    dispatchShortcutTask(QStringLiteral("Working: applying shortcuts."),
                         [store, cleared, disabledDraft, authenticStaged, result]() {
                             ShortcutReconciler reconciler(store, cleared);
                             *result = reconciler.applySelected(disabledDraft, authenticStaged);
                         },
                         [this, result, operation]() {
                             if (!finishShortcutApplyResult(*result, operation)) {
                                 return;
                             }
                             ShortcutStore *previewStore = m_shortcutStore;
                             ClearedActionsStore *previewCleared = m_clearedStore;
                             const QSet<QString> previewDraft = m_shortcutDisabledDraft;
                             const bool previewIntent = m_shortcutAuthenticStaged;
                             auto preview = std::make_shared<ShortcutForcePreview>();
                             dispatchShortcutTask(QStringLiteral("Working: previewing shortcut conflicts."),
                                                  [previewStore, previewCleared, previewDraft, previewIntent,
                                                   preview]() {
                                                      ShortcutReconciler reconciler(previewStore, previewCleared);
                                                      *preview = reconciler.previewForceApplySelected(previewDraft,
                                                                                                      previewIntent);
                                                      ShortcutReconciler::logForcePreview("apply", *preview);
                                                  },
                                                  [this, preview, operation]() {
                                                      finishForcePreviewForApply(*preview, operation);
                                                  });
                         });
}

void UnifiedSettingsModule::runShortcutApplySync(const char *operation)
{
    ShortcutReconciler reconciler(m_shortcutStore, m_clearedStore);
    const ShortcutApplyResult result =
        reconciler.applySelected(m_shortcutDisabledDraft, m_shortcutAuthenticStaged);
    if (!finishShortcutApplyResult(result, operation)) {
        return;
    }
    const ShortcutForcePreview preview =
        reconciler.previewForceApplySelected(m_shortcutDisabledDraft, m_shortcutAuthenticStaged);
    ShortcutReconciler::logForcePreview(operation, preview);
    finishForcePreviewForApply(preview, operation);
}

// Shared UI-thread completion for one Apply result (both modes). Returns
// true when the conflict-modal path must follow with a fresh preview.
bool UnifiedSettingsModule::finishShortcutApplyResult(const ShortcutApplyResult &result, const char *operation)
{
    ShortcutDiag::log(result.ok ? QtInfoMsg : QtWarningMsg, operation, "result", result.ok ? "ok" : "failed",
                      result.ok ? QStringLiteral("writes=%1 authentic=%2")
                                      .arg(result.writes)
                                      .arg(m_shortcutAuthenticStaged ? 1 : 0)
                                 : QStringLiteral("reason=%1 writes=%2 authentic=%3")
                                       .arg(result.error)
                                       .arg(result.writes)
                                       .arg(m_shortcutAuthenticStaged ? 1 : 0));
    if (result.ok) {
        m_shortcutError.clear();
        // Staged Authentic reset intent is committed by a successful Apply.
        m_shortcutAuthenticStaged = false;
        refreshShortcutState();
        showShortcutInformation(
            QStringLiteral("Apply Shortcuts"),
            QStringLiteral("Shortcuts applied."));
        return false;
    }
    m_shortcutError = result.error.isEmpty() ? QStringLiteral("Apply failed") : result.error;
    if (result.conflictRefused && result.writes == 0) {
        return true;
    }
    // Operational or partial-apply failure: report the original error with
    // no further Force confirmation.
    refreshShortcutState();
    showShortcutError(QStringLiteral("Apply Shortcuts"), m_shortcutError);
    return false;
}

// Shared UI-thread conflict modal (both modes): the modal already runs on
// the UI thread, so no store work happens here. Accepting dispatches the
// confirmed force (async) or runs it inline (sync); either way the force
// itself revalidates the full live snapshot before any write.
void UnifiedSettingsModule::finishForcePreviewForApply(const ShortcutForcePreview &preview, const char *operation)
{
    Q_UNUSED(operation);
    if (preview.mismatches.isEmpty() && preview.blockers.isEmpty()) {
        // The holder vanished between refusal and preview: report the
        // original refusal with no empty conflict dialog.
        refreshShortcutState();
        showShortcutError(QStringLiteral("Apply Shortcuts"), m_shortcutError);
        return;
    }
    // Per-row reconstructible conflict logging is owned by the
    // reconciler; the dialog text below is the same combined order.
    const QString dialogText = buildConflictDialogText(preview, m_shortcutAuthenticStaged);
    // Serialize the modal confirmation with shortcut work: the busy guard
    // is held across the modal (nested loop in production, synchronous
    // prompt handler in tests), so a refresh requested from the prompt
    // defers to pending instead of dispatching under the modal. Accept
    // hands the guard directly to the confirmed force on this stack (no
    // event processing in between), so an accepted Force always yields a
    // result or an explicit stale refusal and never disappears. Cancel
    // releases the guard and refreshes.
    m_shortcutBusy = true;
    setShortcutControlsEnabled(false);
    bool accepted = false;
    if (!preview.forceable) {
        // Blocked: the modal itself carries the reason with Force Apply
        // disabled. No redundant error afterwards.
        askShortcutForce(QStringLiteral("Apply Shortcuts: conflicts found"), dialogText, false);
    } else {
        accepted = askShortcutForce(QStringLiteral("Apply Shortcuts: conflicts found"), dialogText, true);
    }
    m_shortcutBusy = false;
    setShortcutControlsEnabled(true);
    if (accepted) {
        applyConfirmedForcePreview(preview);
        return;
    }
    // Explicit Cancel: no writes, no followup error dialog. The status
    // label keeps showing the conflict honestly after refresh.
    refreshShortcutState();
}

QString UnifiedSettingsModule::buildConflictRowText(const ShortcutRowDisplay &row, bool disabled,
                                                      bool authenticStaged)
{
    const QString current = row.present ? ShortcutReconciler::keysDisplayNames(row.current)
                                        : QStringLiteral("missing");
    const QString ownDefault = row.present ? ShortcutReconciler::keysDisplayNames(row.projectDefaults)
                                           : QStringLiteral("missing");
    QString known;
    if (row.catalog.knownForeignComponent.isEmpty() && row.foreignDefaultIds.isEmpty()) {
        known = row.foreignDefaultsKnown ? QStringLiteral("none known") : QStringLiteral("defaults unavailable");
    } else if (!row.defaultsKnown || !row.foreignDefaultsKnown) {
        known = QStringLiteral("defaults unavailable");
    } else {
        QStringList defaults;
        if (!row.catalog.knownForeignComponent.isEmpty()) {
            defaults.append(QStringLiteral("%1/%2 defaults %3")
                                .arg(row.catalog.knownForeignComponent, row.catalog.knownForeignAction,
                                     ShortcutReconciler::keysDisplayNames(row.knownDefaults)));
        }
        if (!row.foreignDefaultIds.isEmpty()) {
            defaults.append(QStringLiteral("foreign defaults %1 claim %2")
                                .arg(row.foreignDefaultIds.join(QStringLiteral(", ")),
                                     ShortcutReconciler::keyDisplayName(row.catalog.canonicalKey)));
        }
        known = defaults.join(QStringLiteral("; "));
    }
    QString holders;
    if (!row.holdersKnown) {
        holders = QStringLiteral("holders unavailable");
    } else {
        QStringList own;
        QStringList foreign;
        for (const ShortcutKeyHolder &holder : row.holders) {
            const QString label = holderDisplayLabel(holder.component, holder.action, holder.componentFriendly,
                                                     holder.friendly);
            if (ShortcutReconciler::isHolderExempt(holder.component, holder.action, row.catalog.canonicalKey)) {
                if (holder.component == row.catalog.component && holder.action == row.catalog.action) {
                    own.append(QStringLiteral("own %1").arg(label));
                } else {
                    own.append(label);
                }
            } else {
                foreign.append(QStringLiteral("conflict %1").arg(label));
            }
        }
        // No truncation: every current holder appears in reconciler
        // order (already sorted by component/action in collectRowDisplays).
        QStringList parts;
        if (!own.isEmpty()) {
            parts.append(QStringLiteral("own [%1]").arg(own.join(QStringLiteral(", "))));
        }
        if (!foreign.isEmpty()) {
            parts.append(QStringLiteral("foreign [%1]").arg(foreign.join(QStringLiteral(", "))));
        }
        holders = parts.isEmpty() ? QStringLiteral("none") : parts.join(QStringLiteral("; "));
    }
    return QStringLiteral("%1 [%2]: canonical %3, current %4; own default %5; known KDE %6; holders %7.")
        .arg(row.catalog.action,
              disabled ? QStringLiteral("Disable")
                       : (authenticStaged ? QStringLiteral("Authentic") : QStringLiteral("Keep")),
             row.catalog.canonicalDisplay, current, ownDefault, known, holders);
}

void UnifiedSettingsModule::refreshShortcutConflictList(const QList<ShortcutRowDisplay> &rows)
{
    if (m_ui.shortcutConflictList == nullptr) {
        return;
    }
    m_ui.shortcutConflictList->blockSignals(true);
    m_ui.shortcutConflictList->clear();
    for (const ShortcutRowDisplay &row : rows) {
        const QString id = shortcutCatalogId(row.catalog.component, row.catalog.action);
        const bool disabled = m_shortcutDisabledDraft.contains(id);
        QListWidgetItem *item = new QListWidgetItem(buildConflictRowText(row, disabled, m_shortcutAuthenticStaged));
        item->setFlags(item->flags() | Qt::ItemIsUserCheckable);
        item->setCheckState(disabled ? Qt::Unchecked : Qt::Checked);
        item->setData(Qt::UserRole, id);
        m_ui.shortcutConflictList->addItem(item);
    }
    m_ui.shortcutConflictList->blockSignals(false);
}

void UnifiedSettingsModule::runShortcutRevert(const char *operation)
{
    if (m_shortcutBusy) {
        return;
    }
    QList<ClearedAction> cleared;
    QString clearedError;
    const bool clearedKnown = m_clearedStore != nullptr && m_clearedStore->load(&cleared, &clearedError);
    QString confirmText;
    if (clearedKnown && !cleared.isEmpty()) {
        confirmText = QStringLiteral("Restore KDE default shortcuts for %1 cleared binding(s) recorded by Force? "
                                     "Project shortcuts stay assigned. Custom cleared bindings are lost.")
                          .arg(cleared.size());
    } else {
        confirmText = QStringLiteral("Restore KDE default shortcuts for every binding recorded by Force? Project "
                                     "shortcuts stay assigned. Custom cleared bindings are lost.");
    }
    if (!confirmShortcutAction(QStringLiteral("Revert Shortcuts"), confirmText)) {
        return;
    }
    if (m_shortcutStore == nullptr || m_clearedStore == nullptr) {
        m_shortcutError = QStringLiteral("reconciler is not configured");
        ShortcutDiag::log(QtWarningMsg, operation, "result", "failed",
                          QStringLiteral("reason=reconciler-unconfigured writes=0"));
        updateShortcutPresentation();
        showShortcutError(QStringLiteral("Revert Shortcuts"), m_shortcutError);
        return;
    }
    if (!m_shortcutAsync) {
        runShortcutRevertSync(operation);
        return;
    }
    ShortcutStore *store = m_shortcutStore;
    ClearedActionsStore *clearedStore = m_clearedStore;
    auto result = std::make_shared<ShortcutRevertResult>();
    dispatchShortcutTask(QStringLiteral("Working: reverting shortcuts."),
                         [store, clearedStore, result]() {
                             ShortcutReconciler reconciler(store, clearedStore);
                             *result = reconciler.revert();
                         },
                         [this, result, operation]() {
                             finishShortcutRevertResult(*result, operation);
                         });
}

void UnifiedSettingsModule::runShortcutRevertSync(const char *operation)
{
    ShortcutReconciler reconciler(m_shortcutStore, m_clearedStore);
    finishShortcutRevertResult(reconciler.revert(), operation);
}

void UnifiedSettingsModule::finishShortcutRevertResult(const ShortcutRevertResult &result, const char *operation)
{
    if (result.ok) {
        m_shortcutError.clear();
    } else {
        m_shortcutError = result.error.isEmpty() ? QStringLiteral("Revert failed") : result.error;
    }
    ShortcutDiag::log(result.ok ? QtInfoMsg : QtWarningMsg, operation, "result", result.ok ? "ok" : "failed",
                      result.ok ? QStringLiteral("writes=%1").arg(result.writes)
                                : QStringLiteral("reason=%1 writes=%2")
                                      .arg(result.error)
                                      .arg(result.writes));
    refreshShortcutState();
    if (result.ok) {
        showShortcutInformation(
            QStringLiteral("Revert Shortcuts"),
            QStringLiteral("Shortcuts reverted to defaults."));
    } else {
        showShortcutError(QStringLiteral("Revert Shortcuts"), m_shortcutError);
    }
}

void UnifiedSettingsModule::updateShortcutPresentation()
{
    if (m_ui.shortcutStatusLabel != nullptr) {
        m_ui.shortcutStatusLabel->setText(m_shortcutStatus);
    }
    if (m_ui.shortcutErrorLabel != nullptr) {
        m_ui.shortcutErrorLabel->setText(m_shortcutError);
    }
}

void UnifiedSettingsModule::refreshShortcutState()
{
    requestRefreshShortcutState();
}

void UnifiedSettingsModule::requestRefreshShortcutState()
{
    if (m_shortcutDestroying) {
        return;
    }
    if (m_shortcutBusy) {
        m_shortcutRefreshPending = true;
        return;
    }
    // A direct dispatch supersedes any deferred one: single clear site.
    m_shortcutRefreshPending = false;
    // The restage claim and draft epoch are bound here, at dispatch. A
    // refresh finish stages only when its claim is still current, so older
    // evidence never stages after a newer load/default/draft revision.
    const quint64 restageEpoch = m_shortcutRestageEpoch;
    m_shortcutRestageEpoch = 0;
    const QSet<QString> disabledDraft = m_shortcutDisabledDraft;
    const bool authenticStaged = m_shortcutAuthenticStaged;
    ShortcutStore *store = m_shortcutStore;
    ClearedActionsStore *cleared = m_clearedStore;
    auto evidence = std::make_shared<ShortcutRefreshEvidence>();
    dispatchShortcutTask(QStringLiteral("Working: discovering shortcut state."),
                         [store, cleared, disabledDraft, authenticStaged, evidence]() {
                             collectRefreshEvidence(store, cleared, disabledDraft, authenticStaged, evidence.get());
                         },
                         [this, evidence, restageEpoch]() {
                             finishShortcutRefresh(*evidence, restageEpoch);
                         });
}

bool UnifiedSettingsModule::collectRefreshEvidence(ShortcutStore *store, ClearedActionsStore *cleared,
                                                   const QSet<QString> &disabledDraft, bool authenticStaged,
                                                   ShortcutRefreshEvidence *evidence)
{
    if (evidence == nullptr) {
        return false;
    }
    ShortcutRefreshEvidence out;
    if (store == nullptr || cleared == nullptr) {
        out.error = QStringLiteral("reconciler is not configured");
        *evidence = out;
        return false;
    }
    QString error;
    if (!cleared->load(&out.cleared, &error)) {
        out.error = error;
        *evidence = out;
        return false;
    }
    if (!store->checkSetterContract(&error)) {
        out.error = error;
        *evidence = out;
        return false;
    }
    QString owner;
    uint uid = 0;
    if (!store->currentOwner(&owner, &uid, &error)) {
        out.error = error;
        *evidence = out;
        return false;
    }
    if (!store->readAll(&out.tuples, &error)) {
        out.error = error;
        *evidence = out;
        return false;
    }
    out.tuplesOk = true;
    QList<ShortcutRowDisplay> displays;
    if (!ShortcutReconciler::collectRowDisplays(store, &displays, &error)) {
        out.displaysOk = false;
        out.displaysError = error;
    } else {
        out.displaysOk = true;
        out.displays = displays;
    }
    out.keyed = ShortcutReconciler::checkKeyedForeignOccupancyDetailedFor(store, disabledDraft, authenticStaged);
    *evidence = out;
    return true;
}

void UnifiedSettingsModule::finishShortcutRefresh(const ShortcutRefreshEvidence &evidence, quint64 restageEpoch)
{
    // Reopening/load restages live-disabled rows from the same evidence the
    // presentation below uses: present own assignments that are empty
    // restage Disable (canonical-unbound rows never do). Only the refresh
    // whose dispatch-time claim is still current stages, and it replaces
    // the draft from its own fresh tuples instead of unioning older
    // evidence over newer revisions.
    if (restageEpoch != 0 && restageEpoch == m_shortcutDraftEpoch && evidence.tuplesOk) {
        QSet<QString> staged;
        for (const ShortcutCatalogEntry &entry : shortcutProjectCatalog()) {
            if (entry.canonicalKey == 0) {
                continue;
            }
            for (const ShortcutTuple &tuple : evidence.tuples) {
                if (tuple.component == entry.component && tuple.action == entry.action && tuple.active.isEmpty()) {
                    staged.insert(shortcutCatalogId(entry.component, entry.action));
                    break;
                }
            }
        }
        m_shortcutDisabledDraft = staged;
    }
    presentRefreshEvidence(evidence);
}

void UnifiedSettingsModule::presentRefreshEvidence(const ShortcutRefreshEvidence &evidence)
{
    if (!evidence.tuplesOk) {
        m_shortcutStatus = QStringLiteral("Shortcut state unavailable: %1").arg(evidence.error);
        updateShortcutPresentation();
        return;
    }
    const QList<ClearedAction> cleared = evidence.cleared;
    const QList<ShortcutTuple> tuples = evidence.tuples;
    const QList<ShortcutConflictRow> &table = shortcutConflictTable();
    const QList<ShortcutCatalogEntry> &catalog = shortcutProjectCatalog();
    auto catalogPost = [&](const ShortcutCatalogEntry &entry, QList<int> *post) {
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
    QList<const ShortcutTuple *> projectCurrents;
    projectCurrents.reserve(table.size());
    for (int i = 0; i < table.size(); ++i) {
        projectCurrents.append(nullptr);
    }
    const ShortcutTuple *lockCurrent = nullptr;
    QList<int> projectMatches;
    projectMatches.reserve(table.size());
    for (int i = 0; i < table.size(); ++i) {
        projectMatches.append(0);
    }
    int lockMatches = 0;
    for (const ShortcutTuple &tuple : tuples) {
        for (int i = 0; i < table.size(); ++i) {
            if (tuple.component == table.at(i).projectComponent && tuple.action == table.at(i).projectAction) {
                ++projectMatches[i];
                projectCurrents[i] = &tuple;
            }
        }
        if (tuple.component == table.at(0).foreignComponent && tuple.action == table.at(0).foreignAction) {
            ++lockMatches;
            lockCurrent = &tuple;
        }
    }
    {
        QString idError;
        if (!ShortcutReconciler::disabledIdsValid(m_shortcutDisabledDraft, &idError)) {
            m_shortcutStatus = QStringLiteral("Shortcut state unavailable: %1").arg(idError);
            refreshShortcutConflictList(QList<ShortcutRowDisplay>());
            updateShortcutPresentation();
            return;
        }
    }
    // Missing disabled rows stay allowed (no assignment); duplicates fail
    // even when disabled. A Keep or disabled focus-right leaves the lock
    // out of scope entirely, so an absent lock still shows the row list.
    const QString refreshFocusId =
        shortcutCatalogId(table.at(0).projectComponent, table.at(0).projectAction);
    const bool refreshFocusEnabled = !m_shortcutDisabledDraft.contains(refreshFocusId);
    const bool refreshLockInScope = refreshFocusEnabled && m_shortcutAuthenticStaged;
    bool projectsMissing = false;
    if (refreshLockInScope && (lockMatches != 1 || lockCurrent == nullptr)) {
        projectsMissing = true;
    }
    for (int i = 0; i < table.size(); ++i) {
        const QString rowId =
            shortcutCatalogId(table.at(i).projectComponent, table.at(i).projectAction);
        if (projectMatches.at(i) > 1) {
            projectsMissing = true;
            break;
        }
        if (projectMatches.at(i) != 1 || projectCurrents.at(i) == nullptr) {
            if (!m_shortcutDisabledDraft.contains(rowId)) {
                projectsMissing = true;
                break;
            }
        }
    }
    if (projectsMissing) {
        m_shortcutStatus = QStringLiteral("Shortcut state unavailable: project bindings are missing.");
        refreshShortcutConflictList(QList<ShortcutRowDisplay>());
        updateShortcutPresentation();
        return;
    }
    for (const ShortcutTuple &tuple : tuples) {
        if (ShortcutReconciler::isProjectAction(tuple.component, tuple.action)
            || ShortcutReconciler::isLockAction(tuple.component, tuple.action)) {
            continue;
        }
        if (!ShortcutReconciler::keysValid(tuple.active)) {
            m_shortcutStatus = QStringLiteral("Shortcut state unavailable: unrelated tuple is unbounded.");
            refreshShortcutConflictList(QList<ShortcutRowDisplay>());
            updateShortcutPresentation();
            return;
        }
    }
    // Full-catalog conflict list: canonical chord, current assignment, and
    // known/current holders per row. Unavailable queries are reported
    // honestly per row; a failed collection leaves the list empty. Missing
    // enabled rows display honestly as missing and never count as applied.
    {
        if (!evidence.displaysOk) {
            m_shortcutStatus = QStringLiteral("Shortcut state unavailable: %1").arg(evidence.displaysError);
            refreshShortcutConflictList(QList<ShortcutRowDisplay>());
            updateShortcutPresentation();
            return;
        }
        const QList<ShortcutRowDisplay> displays = evidence.displays;
        refreshShortcutConflictList(displays);
        for (const ShortcutRowDisplay &row : displays) {
            const QString id = shortcutCatalogId(row.catalog.component, row.catalog.action);
            if (!row.present && !m_shortcutDisabledDraft.contains(id)) {
                m_shortcutStatus = QStringLiteral("Shortcut state unavailable: project bindings are missing.");
                updateShortcutPresentation();
                return;
            }
        }
    }
    // Keyed holder gate over the enabled chords only, never tuple
    // enumeration. Authentic-enabled rows contribute canonical chords, Keep
    // rows their live actual chords. Typed outcome keeps Conflict vs
    // unavailable semantics. A Keep or disabled focus-right excludes the
    // Meta+L/Meta+Esc chords here.
    {
        const KeyedOccupancyResult keyed = evidence.keyed;
        if (keyed.status != KeyedOccupancy::Clear) {
            if (keyed.status == KeyedOccupancy::Conflict) {
                m_shortcutStatus = QStringLiteral("Conflict: %1. Apply is refused.").arg(keyed.detail);
            } else {
                m_shortcutStatus = QStringLiteral("Shortcut state unavailable: %1").arg(keyed.detail);
            }
            updateShortcutPresentation();
            return;
        }
    }
    const QString focusId = shortcutCatalogId(table.at(0).projectComponent, table.at(0).projectAction);
    const bool focusEnabled = !m_shortcutDisabledDraft.contains(focusId);
    bool allAtPost = true;
    for (const ShortcutCatalogEntry &entry : catalog) {
        const QString id = shortcutCatalogId(entry.component, entry.action);
        const ShortcutTuple *current = nullptr;
        for (const ShortcutTuple &tuple : tuples) {
            if (tuple.component == entry.component && tuple.action == entry.action) {
                current = &tuple;
                break;
            }
        }
        if (current == nullptr) {
            // Enabled missing already returned unavailable above; disabled
            // missing stays out of scope here.
            continue;
        }
        if (m_shortcutDisabledDraft.contains(id)) {
            if (!current->active.isEmpty()) {
                allAtPost = false;
            }
            continue;
        }
        if (!m_shortcutAuthenticStaged) {
            // Keep preserves the current assignment by definition.
            continue;
        }
        QList<int> post;
        catalogPost(entry, &post);
        if (current->active != post) {
            allAtPost = false;
            break;
        }
    }
    bool lockHasPre = false;
    bool lockHasTarget = false;
    if (focusEnabled && m_shortcutAuthenticStaged && lockCurrent != nullptr) {
        for (int key : table.at(0).foreignExpectedPre) {
            if (lockCurrent->active.contains(key)) {
                lockHasPre = true;
            }
        }
        for (int key : table.at(0).resolutionTarget) {
            if (lockCurrent->active.contains(key)) {
                lockHasTarget = true;
            }
        }
    } else if (!focusEnabled) {
        lockHasTarget = true;
    } else {
        // Keep focus preserves its current assignment with Lock Session
        // untouched: the lock stays out of scope like a disabled focus.
        lockHasTarget = true;
    }
    QString clearedHint;
    if (!cleared.isEmpty()) {
        clearedHint = QStringLiteral(" %1 cleared binding(s) recorded; Revert restores KDE defaults.")
                          .arg(cleared.size());
    }
    QString draftHint;
    if (!m_shortcutDisabledDraft.isEmpty()) {
        draftHint = QStringLiteral(" %1 disabled binding(s) staged.").arg(m_shortcutDisabledDraft.size());
    }
    QString focusHint;
    if (!focusEnabled) {
        focusHint = QStringLiteral(", focus-right disabled");
    } else if (m_shortcutAuthenticStaged) {
        focusHint = QStringLiteral(", Lock Session owns Meta+Esc");
    } else {
        focusHint = QStringLiteral(", focus-right kept at its current assignment");
    }
    if (allAtPost && !lockHasPre && lockHasTarget) {
        if (m_shortcutAuthenticStaged) {
            m_shortcutStatus = QStringLiteral("Shortcuts applied (%1 rows%2): enabled bindings own their canonical "
                                               "chords%3.")
                                    .arg(catalog.size())
                                    .arg(m_shortcutDisabledDraft.isEmpty()
                                             ? QString()
                                             : QStringLiteral(", %1 disabled").arg(m_shortcutDisabledDraft.size()))
                                    .arg(focusHint)
                + clearedHint;
        } else {
            m_shortcutStatus = QStringLiteral("Shortcuts preserved (%1 rows%2): enabled bindings kept at their "
                                               "current assignments%3.")
                                    .arg(catalog.size())
                                    .arg(m_shortcutDisabledDraft.isEmpty()
                                             ? QString()
                                             : QStringLiteral(", %1 disabled").arg(m_shortcutDisabledDraft.size()))
                                    .arg(focusHint)
                + clearedHint;
        }
        updateShortcutPresentation();
        return;
    }
    if (m_shortcutAuthenticStaged) {
        m_shortcutStatus = QStringLiteral("Ready (%1 rows%2): Apply will assign enabled bindings to canonical and "
                                           "clear disabled own bindings%3.")
                                .arg(catalog.size())
                                .arg(m_shortcutDisabledDraft.isEmpty()
                                         ? QString()
                                         : QStringLiteral(", %1 disabled").arg(m_shortcutDisabledDraft.size()))
                                .arg(draftHint)
            + clearedHint;
    } else {
        m_shortcutStatus = QStringLiteral("Ready (%1 rows%2): Apply will preserve enabled bindings at current "
                                           "assignments and clear disabled own bindings%3.")
                                .arg(catalog.size())
                                .arg(m_shortcutDisabledDraft.isEmpty()
                                         ? QString()
                                         : QStringLiteral(", %1 disabled").arg(m_shortcutDisabledDraft.size()))
                                .arg(draftHint)
            + clearedHint;
    }
    updateShortcutPresentation();
}

QString UnifiedSettingsModule::scriptStatusText() const
{
    return m_scriptStatus;
}

bool UnifiedSettingsModule::isScriptRestartRequired() const
{
    return m_scriptRestartRequired;
}

bool UnifiedSettingsModule::isGapReconfigurePending() const
{
    return m_gapReconfigurePending;
}

QVariantMap UnifiedSettingsModule::currentScriptValues() const
{
    return {
        {QStringLiteral("workspaceMode"), m_ui.workspaceModeCombo->currentData()},
        {QStringLiteral("sameAxisMove"), m_ui.sameAxisMoveCombo->currentData()},
        {QStringLiteral("fixedSizePredicate"), m_ui.fixedSizePredicateCombo->currentData()},
        {QStringLiteral("bornMaximized"), m_ui.bornMaximizedCombo->currentData()},
        {QStringLiteral("anchorFallback"), m_ui.anchorFallbackCombo->currentData()},
        {QStringLiteral("noSpaceResult"), m_ui.noSpaceResultCombo->currentData()},
        {QStringLiteral("migrationSourceRefill"), m_ui.migrationSourceRefillCombo->currentData()},
        {QStringLiteral("innerGap"), m_ui.innerGapSpinBox->value()},
        {QStringLiteral("outerGap"), m_ui.outerGapSpinBox->value()},
    };
}

void UnifiedSettingsModule::updateScriptState()
{
    const QVariantMap current = currentScriptValues();
    // Catalog-owned defaults through the adapter; dirty tracking compares
    // the live widgets against the loaded snapshot below.
    const QVariantMap defaults = KdeSettingsAdapter::scriptDefaults();
    unmanagedWidgetChangeState(m_gapReconfigurePending
                               || (!m_loadedScriptValues.isEmpty() && current != m_loadedScriptValues));
    unmanagedWidgetDefaultState(current == defaults);
    // Read-only presentation (row visibility/label only, never a dirty
    // flag), so refreshes driven by the signals above stay clean.
    refreshVirtualDesktopWarning();
}

void UnifiedSettingsModule::load()
{
    KCModule::load();

    // Reopening stages live-disabled rows (present own assignments that
    // are empty); default-unbound rows (canonical key 0) are at canonical
    // empty, not a disablement. Missing rows are not Disable and read
    // failure stages Keep (Authentic intent cleared). Authentic/defaults
    // explicitly restage Keep; Compatible explicitly resets and recomputes.
    // Refresh never clobbers the draft.
    m_shortcutDisabledDraft.clear();
    m_shortcutAuthenticStaged = false;
    // Live-disabled staging rides the refresh evidence below (the same
    // tuples the presentation uses); no separate store read. The restage
    // claim is bound to the bumped draft epoch.
    m_shortcutRestageEpoch = ++m_shortcutDraftEpoch;
    refreshShortcutState();
    m_windowConflictError.clear();
    refreshWindowConflicts();
    refreshVirtualDesktopWarning();

    const KConfigGroup group(KSharedConfig::openConfig(QStringLiteral("kwinrc")),
                             KdeSettingsAdapter::scriptConfigGroupName());
    const auto select = [](QComboBox *combo, const QString &value, const QString &fallback) {
        KdeSettingsAdapter::selectEnumCombo(combo, value, fallback);
    };
    const QString workspaceMode = KdeSettingsAdapter::readWorkspaceMode(group);
    const QString sameAxisMove = KdeSettingsAdapter::readEnumToken(
        group, KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("sameAxisMove")));
    const QString fixedSizePredicate = KdeSettingsAdapter::readEnumToken(
        group, KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("fixedSizePredicate")));
    const QString bornMaximized = KdeSettingsAdapter::readEnumToken(
        group, KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("bornMaximized")));
    const QString anchorFallback = KdeSettingsAdapter::readEnumToken(
        group, KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("anchorFallback")));
    const QString noSpaceResult = KdeSettingsAdapter::readEnumToken(
        group, KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("noSpaceResult")));
    const QString migrationSourceRefill = KdeSettingsAdapter::readEnumToken(
        group, KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("migrationSourceRefill")));
    const int innerGap = KdeSettingsAdapter::readBoundedInt(
        group, KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("innerGap")));
    const int outerGap = KdeSettingsAdapter::readBoundedInt(
        group, KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("outerGap")));
    m_loadedInnerGapRawValid = KdeSettingsAdapter::isBoundedIntRawValid(
        group, KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("innerGap")));
    m_loadedOuterGapRawValid = KdeSettingsAdapter::isBoundedIntRawValid(
        group, KdeSettingsAdapter::findScriptFieldByKey(QStringLiteral("outerGap")));
    // Combo fallbacks are the catalog defaults: an unknown stored token
    // shows the default while the snapshot keeps the raw value.
    const QVariantMap catalogDefaults = KdeSettingsAdapter::scriptDefaults();
    select(m_ui.workspaceModeCombo, workspaceMode, catalogDefaults.value(QStringLiteral("workspaceMode")).toString());
    select(m_ui.sameAxisMoveCombo, sameAxisMove, catalogDefaults.value(QStringLiteral("sameAxisMove")).toString());
    select(m_ui.fixedSizePredicateCombo, fixedSizePredicate,
           catalogDefaults.value(QStringLiteral("fixedSizePredicate")).toString());
    select(m_ui.bornMaximizedCombo, bornMaximized, catalogDefaults.value(QStringLiteral("bornMaximized")).toString());
    select(m_ui.anchorFallbackCombo, anchorFallback, catalogDefaults.value(QStringLiteral("anchorFallback")).toString());
    select(m_ui.noSpaceResultCombo, noSpaceResult, catalogDefaults.value(QStringLiteral("noSpaceResult")).toString());
    select(m_ui.migrationSourceRefillCombo, migrationSourceRefill,
           catalogDefaults.value(QStringLiteral("migrationSourceRefill")).toString());
    m_ui.innerGapSpinBox->setValue(innerGap);
    m_ui.outerGapSpinBox->setValue(outerGap);
    m_loadedScriptValues = {
        {QStringLiteral("workspaceMode"), workspaceMode},
        {QStringLiteral("sameAxisMove"), sameAxisMove},
        {QStringLiteral("fixedSizePredicate"), fixedSizePredicate},
        {QStringLiteral("bornMaximized"), bornMaximized},
        {QStringLiteral("anchorFallback"), anchorFallback},
        {QStringLiteral("noSpaceResult"), noSpaceResult},
        {QStringLiteral("migrationSourceRefill"), migrationSourceRefill},
        {QStringLiteral("innerGap"), innerGap},
        {QStringLiteral("outerGap"), outerGap},
    };
    updateScriptState();
    m_scriptRestartRequired = false;
    m_gapReconfigurePending = false;
    updateScriptState();
    m_scriptStatus = QStringLiteral("No pending script setting in this dialog.");
    if (m_ui.scriptStatusLabel != nullptr) {
        m_ui.scriptStatusLabel->setText(m_scriptStatus);
    }
}

void UnifiedSettingsModule::save()
{
    const bool borderChanged = managedWidgetChangeState();

    const QVariantMap current = currentScriptValues();
    const bool widgetsChanged =
        !m_loadedInnerGapRawValid || !m_loadedOuterGapRawValid || current != m_loadedScriptValues;
    const bool gapChanged = !m_loadedInnerGapRawValid || !m_loadedOuterGapRawValid
        || current.value(QStringLiteral("innerGap")) != m_loadedScriptValues.value(QStringLiteral("innerGap"))
        || current.value(QStringLiteral("outerGap")) != m_loadedScriptValues.value(QStringLiteral("outerGap"));
    // Per-key startup changes are captured before persistence refreshes the
    // loaded snapshot; comparing afterwards would always match and the
    // restart-required log must enumerate only the keys that changed.
    const bool workspaceModeChanged = current.value(QStringLiteral("workspaceMode"))
        != m_loadedScriptValues.value(QStringLiteral("workspaceMode"));
    const bool sameAxisMoveChanged = current.value(QStringLiteral("sameAxisMove"))
        != m_loadedScriptValues.value(QStringLiteral("sameAxisMove"));
    const bool fixedPredicateChanged = current.value(QStringLiteral("fixedSizePredicate"))
        != m_loadedScriptValues.value(QStringLiteral("fixedSizePredicate"));
    const bool bornMaximizedChanged = current.value(QStringLiteral("bornMaximized"))
        != m_loadedScriptValues.value(QStringLiteral("bornMaximized"));
    const bool anchorFallbackChanged = current.value(QStringLiteral("anchorFallback"))
        != m_loadedScriptValues.value(QStringLiteral("anchorFallback"));
    const bool noSpaceChanged = current.value(QStringLiteral("noSpaceResult"))
        != m_loadedScriptValues.value(QStringLiteral("noSpaceResult"));
    const bool refillChanged = current.value(QStringLiteral("migrationSourceRefill"))
        != m_loadedScriptValues.value(QStringLiteral("migrationSourceRefill"));
    const bool startupConsumedChanged = workspaceModeChanged;
    const bool scriptRetryArmed = m_gapReconfigurePending;

    KCModule::save();

    if (borderChanged) {
        m_effectReconfigurePending = true;
    }

    if (!widgetsChanged && !scriptRetryArmed) {
        updateScriptState();
    } else {
    // This module owns exactly workspaceMode, sameAxisMove,
    // fixedSizePredicate, bornMaximized, anchorFallback, noSpaceResult, migrationSourceRefill, innerGap, and outerGap. Any other key in this
    // group (including the hidden shortcutProfile) is never read here
    // beyond the group open and is never written; there is no migration. A
    // pure retry save (pending request, unchanged widgets) skips
    // persistence: the loaded values already match the widgets.
    KConfigGroup group(KSharedConfig::openConfig(QStringLiteral("kwinrc")),
                       KdeSettingsAdapter::scriptConfigGroupName());
    QStringList written;
    if (widgetsChanged) {
        if (workspaceModeChanged) {
            group.writeEntry(QStringLiteral("workspaceMode"), current.value(QStringLiteral("workspaceMode")).toString());
            written.append(QStringLiteral("workspaceMode"));
        }
        if (sameAxisMoveChanged) {
            group.writeEntry(QStringLiteral("sameAxisMove"), current.value(QStringLiteral("sameAxisMove")).toString());
            written.append(QStringLiteral("sameAxisMove"));
        }
        if (fixedPredicateChanged) {
            group.writeEntry(QStringLiteral("fixedSizePredicate"), current.value(QStringLiteral("fixedSizePredicate")).toString());
            written.append(QStringLiteral("fixedSizePredicate"));
        }
        if (bornMaximizedChanged) {
            group.writeEntry(QStringLiteral("bornMaximized"), current.value(QStringLiteral("bornMaximized")).toString());
            written.append(QStringLiteral("bornMaximized"));
        }
        if (anchorFallbackChanged) {
            group.writeEntry(QStringLiteral("anchorFallback"), current.value(QStringLiteral("anchorFallback")).toString());
            written.append(QStringLiteral("anchorFallback"));
        }
        if (noSpaceChanged) {
            group.writeEntry(QStringLiteral("noSpaceResult"), current.value(QStringLiteral("noSpaceResult")).toString());
            written.append(QStringLiteral("noSpaceResult"));
        }
        if (refillChanged) {
            group.writeEntry(QStringLiteral("migrationSourceRefill"), current.value(QStringLiteral("migrationSourceRefill")).toString());
            written.append(QStringLiteral("migrationSourceRefill"));
        }
        if (!m_loadedInnerGapRawValid
            || current.value(QStringLiteral("innerGap")) != m_loadedScriptValues.value(QStringLiteral("innerGap"))) {
            group.writeEntry(QStringLiteral("innerGap"), current.value(QStringLiteral("innerGap")).toInt());
            written.append(QStringLiteral("innerGap"));
        }
        if (!m_loadedOuterGapRawValid
            || current.value(QStringLiteral("outerGap")) != m_loadedScriptValues.value(QStringLiteral("outerGap"))) {
            group.writeEntry(QStringLiteral("outerGap"), current.value(QStringLiteral("outerGap")).toInt());
            written.append(QStringLiteral("outerGap"));
        }
        group.sync();
        m_loadedScriptValues = current;
        m_loadedInnerGapRawValid = true;
        m_loadedOuterGapRawValid = true;
        logScriptConfig("save", "persist", "ok", QStringLiteral("keys=%1").arg(written.join(QStringLiteral(","))));
    }
    if (startupConsumedChanged) {
        m_scriptRestartRequired = true;
        QStringList startupWritten;
        if (workspaceModeChanged) {
            startupWritten.append(QStringLiteral("workspaceMode"));
        }
        logScriptConfig("save", "startup", "restart-required",
                        QStringLiteral("keys=%1").arg(startupWritten.join(QStringLiteral(","))));
    }
    // Changed gaps, same-axis moves, fixed-size predicates, born-maximized
    // modes, anchor fallbacks, no-space results, and migration
    // source refills request one
    // typed KWin reconfigure after persistence whose pickup is the running
    // controller's Options configChanged live re-read (gaps re-resolve,
    // same-axis applies to subsequent moves with no tree rebuild,
    // fixed-size predicate applies to subsequent admissions with no
    // reclassification, born-maximized applies to subsequent admissions with
    // no rebuild or reclassification, anchor fallback and no-space result
    // apply to subsequent admission or actual capacity-repair decisions with
    // no reflow, source refill applies to subsequent
    // migrations with no destination/history/lifecycle work).
    // KWin's reconfigure is Q_NOREPLY, so a queued send never proves the
    // running script reread kwinrc. Success reports sent-but-unconfirmed and
    // clears any pending retry; failure arms a retry on the next save (an
    // unchanged save still sends while armed, and Apply stays enabled for it
    // through updateScriptState) and reports failed with the retry. A pure
    // retry save skips persistence, so its statuses must not claim anything
    // was saved by the retry. A queued send never clears a pending
    // session-restart requirement for the startup-consumed setting
    // (workspaceMode) and never claims the running tiler applied saved values.
    // Same-axis move, fixed-size predicate, born-maximized, anchor fallback,
    // no-space result, and migration
    // source refill
    // changes never set the restart
    // requirement: they apply to subsequent moves/admissions/migrations after the live
    // re-read.
    const bool liveChanged = gapChanged || sameAxisMoveChanged || fixedPredicateChanged || bornMaximizedChanged || anchorFallbackChanged || noSpaceChanged || refillChanged;
    const QString liveNames = liveChangedNames(gapChanged, sameAxisMoveChanged, fixedPredicateChanged, bornMaximizedChanged, anchorFallbackChanged, noSpaceChanged, refillChanged).join(QStringLiteral(", "));
    if (liveChanged || scriptRetryArmed) {
        if (requestScriptReconfigure()) {
            m_gapReconfigurePending = false;
            logScriptConfig("save", "reconfigure", "sent-unconfirmed", QStringLiteral("method=reconfigure"));
            if (!widgetsChanged) {
                if (m_scriptRestartRequired) {
                    m_scriptStatus = QStringLiteral(
                        "Reconfigure request sent for the pending live settings; application unconfirmed. This retry saved nothing; "
                        "persisted settings are unchanged. Session restart remains required for workspace mode. "
                        "Restart the session to guarantee pickup.");
                } else {
                    m_scriptStatus = QStringLiteral(
                        "Reconfigure request sent; application unconfirmed. This retry saved nothing; persisted "
                        "settings are unchanged. Restart the session to guarantee pickup.");
                }
            } else if (m_scriptRestartRequired) {
                if (refillChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request sent for live settings including %1; "
                        "application unconfirmed. Session restart remains required for workspace mode. Restart the "
                        "session to guarantee pickup.").arg(liveNames);
                } else if (fixedPredicateChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request sent for live settings including fixed-size predicate; "
                        "application unconfirmed. Session restart remains required for workspace mode. Restart the "
                        "session to guarantee pickup.");
                } else if (bornMaximizedChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request sent for live settings including born-maximized; "
                        "application unconfirmed. Session restart remains required for workspace mode. Restart the "
                        "session to guarantee pickup.");
                } else if (anchorFallbackChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request sent for live settings including anchor fallback; "
                        "application unconfirmed. Session restart remains required for workspace mode. Restart the "
                        "session to guarantee pickup.");
                } else if (noSpaceChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request sent for live settings including no-space result; "
                        "application unconfirmed. Session restart remains required for workspace mode. Restart the "
                        "session to guarantee pickup.");
                } else if (sameAxisMoveChanged && gapChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request sent for gaps and same-axis move; "
                        "application unconfirmed. Session restart remains required for workspace mode. Restart the "
                        "session to guarantee pickup.");
                } else if (sameAxisMoveChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request sent for same-axis move; "
                        "application unconfirmed. Session restart remains required for workspace mode. Restart the "
                        "session to guarantee pickup.");
                } else {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request sent for gaps; "
                        "application unconfirmed. Session restart remains required for workspace mode. Restart the "
                        "session to guarantee pickup.");
                }
            } else if (refillChanged) {
                m_scriptStatus = QStringLiteral(
                    "Live settings including %1 saved to kwinrc. Reconfigure request sent; application unconfirmed. Restart the "
                    "session to guarantee pickup.").arg(liveNames);
            } else if (fixedPredicateChanged) {
                m_scriptStatus = QStringLiteral(
                    "Live settings including fixed-size predicate saved to kwinrc. Reconfigure request sent; application unconfirmed. Restart the "
                    "session to guarantee pickup.");
            } else if (bornMaximizedChanged) {
                m_scriptStatus = QStringLiteral(
                    "Live settings including born-maximized saved to kwinrc. Reconfigure request sent; application unconfirmed. Restart the "
                    "session to guarantee pickup.");
            } else if (anchorFallbackChanged) {
                m_scriptStatus = QStringLiteral(
                    "Live settings including anchor fallback saved to kwinrc. Reconfigure request sent; application unconfirmed. Restart the "
                    "session to guarantee pickup.");
            } else if (noSpaceChanged) {
                m_scriptStatus = QStringLiteral(
                    "Live settings including no-space result saved to kwinrc. Reconfigure request sent; application unconfirmed. Restart the "
                    "session to guarantee pickup.");
            } else if (sameAxisMoveChanged && gapChanged) {
                m_scriptStatus = QStringLiteral(
                    "Tiling gaps and same-axis move saved to kwinrc. Reconfigure request sent; application unconfirmed. Restart the "
                    "session to guarantee pickup.");
            } else if (sameAxisMoveChanged && !gapChanged) {
                m_scriptStatus = QStringLiteral(
                    "Same-axis move saved to kwinrc. Reconfigure request sent; application unconfirmed. Restart the "
                    "session to guarantee pickup.");
            } else {
                m_scriptStatus = QStringLiteral(
                    "Tiling gaps saved to kwinrc. Reconfigure request sent; application unconfirmed. Restart the "
                    "session to guarantee pickup.");
            }
        } else {
            m_gapReconfigurePending = true;
            logScriptConfig("save", "reconfigure", "failed",
                            QStringLiteral("method=reconfigure retry-on-next-save"));
            if (!widgetsChanged) {
                if (m_scriptRestartRequired) {
                    m_scriptStatus = QStringLiteral(
                        "Reconfigure request failed; pickup of pending live settings is unconfirmed. This retry saved "
                        "nothing; the request will retry on the next save. Session restart remains required for workspace "
                        "mode.");
                } else {
                    m_scriptStatus = QStringLiteral(
                        "Reconfigure request failed; pickup of pending live settings is unconfirmed. This retry saved "
                        "nothing; the request will retry on the next save. Restart the session to guarantee pickup.");
                }
            } else if (m_scriptRestartRequired) {
                if (refillChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request failed; pickup of "
                        "live settings including %1 is unconfirmed. The request will retry on the next save. Session restart remains required for "
                        "workspace mode.").arg(liveNames);
                } else if (fixedPredicateChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request failed; pickup of "
                        "live settings including fixed-size predicate is unconfirmed. The request will retry on the next save. Session restart remains required for "
                        "workspace mode.");
                } else if (bornMaximizedChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request failed; pickup of "
                        "live settings including born-maximized is unconfirmed. The request will retry on the next save. Session restart remains required for "
                        "workspace mode.");
                } else if (anchorFallbackChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request failed; pickup of "
                        "live settings including anchor fallback is unconfirmed. The request will retry on the next save. Session restart remains required for "
                        "workspace mode.");
                } else if (noSpaceChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request failed; pickup of "
                        "live settings including no-space result is unconfirmed. The request will retry on the next save. Session restart remains required for "
                        "workspace mode.");
                } else if (sameAxisMoveChanged && gapChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request failed; pickup of "
                        "gaps and same-axis move is unconfirmed. The request will retry on the next save. Session restart remains required for "
                        "workspace mode.");
                } else if (sameAxisMoveChanged) {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request failed; pickup of "
                        "same-axis move is unconfirmed. The request will retry on the next save. Session restart remains required for "
                        "workspace mode.");
                } else {
                    m_scriptStatus = QStringLiteral(
                        "Settings saved to kwinrc. Reconfigure request failed; the running tiler still uses "
                        "startup gap values. The request will retry on the next save. Session restart remains required for "
                        "workspace mode.");
                }
            } else if (refillChanged) {
                m_scriptStatus = QStringLiteral(
                    "Live settings including %1 saved to kwinrc. Reconfigure request failed; pickup of live settings "
                    "is unconfirmed. The request will retry on the next save; restart the session to guarantee pickup.").arg(liveNames);
            } else if (fixedPredicateChanged) {
                m_scriptStatus = QStringLiteral(
                    "Live settings including fixed-size predicate saved to kwinrc. Reconfigure request failed; pickup of live settings "
                    "is unconfirmed. The request will retry on the next save; restart the session to guarantee pickup.");
            } else if (bornMaximizedChanged) {
                m_scriptStatus = QStringLiteral(
                    "Live settings including born-maximized saved to kwinrc. Reconfigure request failed; pickup of live settings "
                    "is unconfirmed. The request will retry on the next save; restart the session to guarantee pickup.");
            } else if (anchorFallbackChanged) {
                m_scriptStatus = QStringLiteral(
                    "Live settings including anchor fallback saved to kwinrc. Reconfigure request failed; pickup of live settings "
                    "is unconfirmed. The request will retry on the next save; restart the session to guarantee pickup.");
            } else if (noSpaceChanged) {
                m_scriptStatus = QStringLiteral(
                    "Live settings including no-space result saved to kwinrc. Reconfigure request failed; pickup of live settings "
                    "is unconfirmed. The request will retry on the next save; restart the session to guarantee pickup.");
            } else if (sameAxisMoveChanged && gapChanged) {
                m_scriptStatus = QStringLiteral(
                    "Tiling gaps and same-axis move saved to kwinrc. Reconfigure request failed; pickup of gaps and same-axis "
                    "move is unconfirmed. The request will retry on the next save; restart the session to guarantee pickup.");
            } else if (sameAxisMoveChanged && !gapChanged) {
                m_scriptStatus = QStringLiteral(
                    "Same-axis move saved to kwinrc. Reconfigure request failed; pickup of same-axis "
                    "move is unconfirmed. The request will retry on the next save; restart the session to guarantee pickup.");
            } else {
                m_scriptStatus = QStringLiteral(
                    "Tiling gaps saved to kwinrc. Reconfigure request failed; the running tiler still uses startup gap values. "
                    "The request will retry on the next save; restart the session to guarantee pickup.");
            }
        }
    } else if (m_scriptRestartRequired) {
        m_scriptStatus = QStringLiteral(
            "Workspace mode saved. Session restart required: the running tiler still uses startup values.");
    }
    updateScriptState();
    if (m_ui.scriptStatusLabel != nullptr) {
        m_ui.scriptStatusLabel->setText(m_scriptStatus);
    }
    }

    // Border hot-apply stays live through the native effect reconfigure and
    // runs after the script unmanaged state settles so a failed effect
    // reconfigure keeps Apply enabled via markAsChanged without being
    // cleared by updateScriptState.
    if (m_effectReconfigurePending) {
        if (requestEffectReconfigure()) {
            m_effectReconfigurePending = false;
        } else {
            markAsChanged();
        }
    }
}

void UnifiedSettingsModule::defaults()
{
    KCModule::defaults();

    // Catalog-owned defaults through the adapter; shortcut staging is
    // restaged separately below and never persisted here.
    const QVariantMap catalogDefaults = KdeSettingsAdapter::scriptDefaults();
    KdeSettingsAdapter::selectEnumCombo(m_ui.workspaceModeCombo,
                                       catalogDefaults.value(QStringLiteral("workspaceMode")).toString(),
                                       catalogDefaults.value(QStringLiteral("workspaceMode")).toString());
    KdeSettingsAdapter::selectEnumCombo(m_ui.sameAxisMoveCombo,
                                       catalogDefaults.value(QStringLiteral("sameAxisMove")).toString(),
                                       catalogDefaults.value(QStringLiteral("sameAxisMove")).toString());
    KdeSettingsAdapter::selectEnumCombo(m_ui.fixedSizePredicateCombo,
                                       catalogDefaults.value(QStringLiteral("fixedSizePredicate")).toString(),
                                       catalogDefaults.value(QStringLiteral("fixedSizePredicate")).toString());
    KdeSettingsAdapter::selectEnumCombo(m_ui.bornMaximizedCombo,
                                       catalogDefaults.value(QStringLiteral("bornMaximized")).toString(),
                                       catalogDefaults.value(QStringLiteral("bornMaximized")).toString());
    KdeSettingsAdapter::selectEnumCombo(m_ui.anchorFallbackCombo,
                                       catalogDefaults.value(QStringLiteral("anchorFallback")).toString(),
                                       catalogDefaults.value(QStringLiteral("anchorFallback")).toString());
    KdeSettingsAdapter::selectEnumCombo(m_ui.noSpaceResultCombo,
                                       catalogDefaults.value(QStringLiteral("noSpaceResult")).toString(),
                                       catalogDefaults.value(QStringLiteral("noSpaceResult")).toString());
    KdeSettingsAdapter::selectEnumCombo(m_ui.migrationSourceRefillCombo,
                                       catalogDefaults.value(QStringLiteral("migrationSourceRefill")).toString(),
                                       catalogDefaults.value(QStringLiteral("migrationSourceRefill")).toString());
    m_ui.innerGapSpinBox->setValue(catalogDefaults.value(QStringLiteral("innerGap")).toInt());
    m_ui.outerGapSpinBox->setValue(catalogDefaults.value(QStringLiteral("outerGap")).toInt());
    // Defaults restage Keep for the shortcut draft with no preview;
    // persisting still never writes shortcuts. The revision invalidates
    // any outstanding restage claim (no new claim is staged).
    ++m_shortcutDraftEpoch;
    m_shortcutDisabledDraft.clear();
    m_shortcutAuthenticStaged = false;
    refreshShortcutState();
    updateScriptState();
}

void UnifiedSettingsModule::refreshWindowConflicts()
{
    updateWindowConflictPresentation();
}

void UnifiedSettingsModule::requestWindowFix(const QString &key)
{
    runWindowConflictWrite(key, "fix");
}

void UnifiedSettingsModule::requestWindowRevert(const QString &key)
{
    runWindowConflictWrite(key, "revert");
}

void UnifiedSettingsModule::runWindowConflictWrite(const QString &key, const char *operation)
{
    const bool isFix = QString::fromUtf8(operation) == QStringLiteral("fix");
    const bool isBorders = key == QStringLiteral("ElectricBorders");

    const KSharedConfig::Ptr config = KSharedConfig::openConfig(QStringLiteral("kwinrc"));
    KConfigGroup group(config, QStringLiteral("Windows"));
    if (isFix && !isBorders) {
        group.writeEntry(key, false);
    } else {
        group.deleteEntry(key);
    }
    bool syncOk = group.sync();
    if (syncOk) {
        syncOk = config->sync();
    }
    bool sendOk = false;
    if (syncOk) {
        sendOk = requestScriptReconfigure();
    }

    config->reparseConfiguration();
    const KConfigGroup readback(config, QStringLiteral("Windows"));
    bool matches = false;
    if (isBorders) {
        matches = readback.readEntry(key, 0) == 0;
    } else if (isFix) {
        matches = !readback.readEntry(key, true);
    } else {
        matches = readback.readEntry(key, true);
    }

    const bool ok = syncOk && sendOk && matches;
    QString reason;
    if (!syncOk) {
        reason = QStringLiteral("write-failed");
    } else if (!sendOk) {
        reason = QStringLiteral("send-failed");
    } else if (!matches) {
        reason = QStringLiteral("readback-mismatch");
    } else {
        reason = QStringLiteral("ok");
    }
    logWindowConflict(operation, key, ok ? "ok" : "failed", reason);
    if (ok) {
        m_windowConflictError.clear();
    } else {
        const QString what = isFix ? QStringLiteral("Fix") : QStringLiteral("Revert");
        QString cause;
        if (!syncOk) {
            cause = QStringLiteral("the kwinrc write failed");
        } else if (!sendOk) {
            cause = QStringLiteral("the KWin reconfigure send failed");
        } else {
            cause = QStringLiteral("the re-read value did not match");
        }
        m_windowConflictError = QStringLiteral("%1 failed for %2: %3.").arg(what, key, cause);
    }
    updateWindowConflictPresentation();
}

void UnifiedSettingsModule::updateWindowConflictPresentation()
{
    const WindowConflictState state = readWindowConflictState();
    const int conflicts = (state.tiling ? 1 : 0) + (state.maximize ? 1 : 0) + (state.borders != 0 ? 1 : 0);

    if (m_ui.windowConflictStatusLabel != nullptr) {
        m_ui.windowConflictStatusLabel->setText(
            conflicts > 0 ? QStringLiteral("Found %1 conflicting edge setting(s). Use Fix on each shown row.")
                                        .arg(conflicts)
                          : QStringLiteral("No window edge conflicts."));
    }
    if (m_ui.windowTilingLabel != nullptr) {
        m_ui.windowTilingLabel->setText(QStringLiteral("ElectricBorderTiling: %1. Edge drag tiles windows over our preview.")
                                            .arg(state.tiling ? QStringLiteral("on") : QStringLiteral("off")));
    }
    if (m_ui.windowMaximizeLabel != nullptr) {
        m_ui.windowMaximizeLabel->setText(QStringLiteral("ElectricBorderMaximize: %1. Top-edge drag maximizes windows over our preview.")
                                               .arg(state.maximize ? QStringLiteral("on") : QStringLiteral("off")));
    }
    if (m_ui.windowBordersLabel != nullptr) {
        m_ui.windowBordersLabel->setText(
            QStringLiteral("ElectricBorders: %1. Switching desktops at edges interrupts a window drag.").arg(state.borders));
    }
    if (m_ui.windowTilingFixButton != nullptr) {
        m_ui.windowTilingFixButton->setVisible(state.tiling);
    }
    if (m_ui.windowTilingRevertButton != nullptr) {
        m_ui.windowTilingRevertButton->setVisible(!state.tiling);
    }
    if (m_ui.windowMaximizeFixButton != nullptr) {
        m_ui.windowMaximizeFixButton->setVisible(state.maximize);
    }
    if (m_ui.windowMaximizeRevertButton != nullptr) {
        m_ui.windowMaximizeRevertButton->setVisible(!state.maximize);
    }
    if (m_ui.windowBordersRow != nullptr) {
        m_ui.windowBordersRow->setVisible(state.borders != 0);
    }
    if (m_ui.windowConflictErrorLabel != nullptr) {
        m_ui.windowConflictErrorLabel->setText(m_windowConflictError);
    }
}

QString UnifiedSettingsModule::virtualDesktopsModuleId()
{
    return QStringLiteral("kcm_kwin_virtualdesktops");
}

bool UnifiedSettingsModule::needsVirtualDesktopWarning(const QString &workspaceMode, bool perOutputKnown,
                                                       bool perOutputEnabled)
{
    const bool separate = workspaceMode == QStringLiteral("per-output-local")
        || workspaceMode == QStringLiteral("global-unique");
    return separate && perOutputKnown && !perOutputEnabled;
}

bool UnifiedSettingsModule::readPerOutputVirtualDesktops(bool *known)
{
    // Read-only following the window-conflict seam: reparse so an external
    // change (e.g. the Virtual Desktops page) is picked up while this page
    // is open. A missing key is the KDE default (off), never unknown.
    const KSharedConfig::Ptr config = KSharedConfig::openConfig(QStringLiteral("kwinrc"));
    config->reparseConfiguration();
    const KConfigGroup group(config, QStringLiteral("Windows"));
    if (known != nullptr) {
        *known = true;
    }
    return group.readEntry(QStringLiteral("PerOutputVirtualDesktops"), false);
}

void UnifiedSettingsModule::setVirtualDesktopsLauncherHandler(std::function<void(const QString &)> handler)
{
    m_virtualDesktopsLauncher = std::move(handler);
}

QString UnifiedSettingsModule::virtualDesktopWarningText() const
{
    return i18n("Separate workspaces need KWin's per-screen desktops (off). Open Virtual Desktops to turn "
                "on 'Switch desktops independently for each screen'.");
}

void UnifiedSettingsModule::refreshVirtualDesktopWarning()
{
    // Presentation only (row visibility and label text): never marks the
    // page dirty, so load()/updateScriptState() refreshes stay clean.
    const QString mode = m_ui.workspaceModeCombo != nullptr ? m_ui.workspaceModeCombo->currentData().toString()
                                                            : QString();
    bool known = false;
    const bool enabled = readPerOutputVirtualDesktops(&known);
    if (m_ui.virtualDesktopsWarningRow != nullptr) {
        m_ui.virtualDesktopsWarningRow->setVisible(needsVirtualDesktopWarning(mode, known, enabled));
    }
    if (m_ui.virtualDesktopsWarningLabel != nullptr) {
        m_ui.virtualDesktopsWarningLabel->setText(virtualDesktopWarningText());
    }
}

void UnifiedSettingsModule::requestOpenVirtualDesktops()
{
    const QString id = virtualDesktopsModuleId();
    if (m_virtualDesktopsLauncher) {
        m_virtualDesktopsLauncher(id);
        // The opened page may have changed the host setting (the user's
        // own action, never our write): re-read on return.
        refreshVirtualDesktopWarning();
        return;
    }
    // Safe open only: the fixed Virtual Desktops page in a modal dialog.
    // Never writes the host setting and never shells out.
    const KPluginMetaData metaData =
        KPluginMetaData::findPluginById(QStringLiteral("plasma/kcms/systemsettings"), id);
    if (!metaData.isValid()) {
        return;
    }
    KCMultiDialog dialog(widget());
    dialog.addModule(metaData);
    dialog.exec();
    refreshVirtualDesktopWarning();
}

} // namespace KWin
