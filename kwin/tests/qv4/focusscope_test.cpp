// Execute the exact shipped suppression wrappers in QV4. QObject::activated
// reenters the same engine, as the native KWin setter does. No compositor is
// needed; the fixture supplies the readback and the provisional event store.
#include <QCoreApplication>
#include <QFile>
#include <QJSEngine>
#include <QRegularExpression>
#include <QTextStream>

class NativeSetter final : public QObject
{
    Q_OBJECT
public:
    Q_INVOKABLE bool set(int kind)
    {
        Q_EMIT activated(kind);
        return kind == 1;
    }

Q_SIGNALS:
    void activated(int kind);
};

int main(int argc, char **argv)
{
    QCoreApplication app(argc, argv);
    QTextStream err(stderr);
    if (argc != 2) {
        err << "usage: omnitiler-focus-scope-test <built bundle>\n";
        return 2;
    }
    QFile bundle(QString::fromLocal8Bit(argv[1]));
    if (!bundle.open(QIODevice::ReadOnly)) {
        err << bundle.fileName() << ": " << bundle.errorString() << Qt::endl;
        return 2;
    }

    const auto source = QString::fromUtf8(bundle.readAll());
    const auto start = source.indexOf(QStringLiteral("withHomingSuppression ="));
    const auto end = source.indexOf(QRegularExpression(QStringLiteral(R"((?:const |, )commandWindowTarget =)")), start);
    if (start < 0 || end < start) {
        err << "suppression wrapper anchors missing in " << bundle.fileName() << Qt::endl;
        return 2;
    }
    // The release build combines adjacent const declarations with commas.
    const auto wrappers = QStringLiteral("const ") + source.mid(start, end - start) + QStringLiteral(";\n");
    // esbuild disambiguates imported readProp against unrelated bindings.
    const auto reader = QRegularExpression(QStringLiteral(R"(\b(readProp\d*)\(surface, "activeWindow"\))")).match(wrappers);
    if (!reader.hasMatch()) {
        err << "activeWindow readback missing in " << bundle.fileName() << Qt::endl;
        return 2;
    }

    const auto fixture = QStringLiteral(R"JS(
'use strict';
function assert(condition, label) { if (!condition) throw new Error(label); }
let viewFocusScope = null;
let ownScopeDepth = 0;
let drainKicks = 0, preemptions = 0;
const pendingQueue = ['cut-pinned-in-own-scope'];
const preemptDebounceForCommand = () => { preemptions++; };
const scheduleDrain = () => {
    assert(ownScopeDepth === 0 && viewFocusScope === null, 'filing can resume only after own-scope cleanup');
    assert(!events.some(event => typeof event === 'number' || event === 'view'), 'own evidence resolved before resuming');
    drainKicks++;
};
let homingSuppressDepth = 0, homingSuppressRef = null;
let liveView = 'src';
let activations = 0;
const target = { id: 'target', out: 'out', ws: 'dst' };
const surface = { activeWindow: null };
const events = ['unrelated-before'];
let invalidations = 0;
const state = { currentInvalid: '19', focusChanges: {
    prove(serial) {
        const at = events.indexOf(serial);
        assert(at >= 0, 'proof must name an existing exact serial');
        events.splice(at, 1);
    },
    leaveUnproved(serial) {
        assert(!events.includes(serial), 'proved evidence removed before finalizing its status');
    },
} };
const worldTraceProbes = new Map();
const applyHostBase = {
    readLiveView() { return liveView; },
    readLiveMembership(ref) { return ref; },
};
const outputNameOf = ref => ref.out;
const desktopIdOf = ref => ref.ws;
const readNativeViews = () => [{ out: 'out', native: liveView }];
const focusEvidenceId = ref => ref.id;
const onNativeInvalidation = () => invalidations++;
nativeSetter.activated.connect(function(kind) {
    if (kind === 0) {
        assert(homingSuppressRef === target && homingSuppressDepth === 1, 'homing scope visible to signal');
        return;
    }
    activations++;
    assert(ownScopeDepth === 1 && drainKicks === activations - 1, 'nested shortcut must not file in the setter');
    assert(viewFocusScope !== null, 'scope visible to synchronous signal');
    surface.activeWindow = target;
    viewFocusScope.activationAt = '19';
    viewFocusScope.activationEvidence = activations;
    events.push(activations);
    if (kind === 1) {
        liveView = 'dst';
        viewFocusScope.activatedRef = target;
        viewFocusScope.activatedId = target.id;
        viewFocusScope.viewAt = '19';
        viewFocusScope.viewEvidence = ['view'];
        events.push('view');
    }
    events.push('unrelated-' + activations);
});
)JS");
    const auto assertions = QStringLiteral(R"JS(
function checkSplice(label, count) {
    assert(viewFocusScope === null && !invalidations && activations === count, label + ': scope cleanup');
    assert(ownScopeDepth === 0 && drainKicks === count && preemptions === count, label + ': resume exactly once');
    const expected = ['unrelated-before'];
    for (let i = 1; i <= count; i++) expected.push('unrelated-' + i);
    assert(events.join(',') === expected.join(','), label + ': exact provisional event splice (got ' + events + ')');
}
function view(run) { return withViewSuppression(target, target, run); }
function focus(run) { return withFocusSuppression(target, target.id, target.out, target.ws, 'dst', run); }
function checkThrow(wrap, kind, label) {
    const sentinel = new Error(label);
    let caught = null;
    try {
        wrap(() => { nativeSetter.set(kind); throw sentinel; });
    } catch (error) {
        caught = error;
    }
    assert(caught === sentinel, label + ': original exception identity');
}
// Start with throws: these must finish the splice and reach the successful
// return cases, which detect the Qt 6.12.0 truncation on an unfixed bundle.
checkThrow(view, 1, 'throwing view');
checkSplice('throwing view', 1);
checkThrow(focus, 2, 'throwing focus');
checkSplice('throwing focus', 2);
assert(view(() => nativeSetter.set(1)) === true, 'accepted view return');
checkSplice('accepted view', 3);
assert(focus(() => nativeSetter.set(2)) === false, 'refused focus return');
checkSplice('refused focus', 4);
assert(withHomingSuppression(target, () => nativeSetter.set(0)) === false && homingSuppressDepth === 0 && homingSuppressRef === null,
    'homing return and cleanup');
)JS");

    NativeSetter setter;
    QJSEngine engine;
    QJSEngine::setObjectOwnership(&setter, QJSEngine::CppOwnership);
    engine.globalObject().setProperty(QStringLiteral("nativeSetter"), engine.newQObject(&setter));
    const auto readProp = QStringLiteral("const %1 = (ref, key) => ref[key];\n").arg(reader.captured(1));
    const auto result = engine.evaluate(fixture + readProp + wrappers + assertions, bundle.fileName());
    if (result.isError()) {
        err << result.toString() << '\n' << result.property(QStringLiteral("stack")).toString() << Qt::endl;
        return 1;
    }
    QTextStream(stdout) << "Qt " << qVersion() << ": shipped scope splices, return values and exception identity passed\n";
    return 0;
}

#include "focusscope_test.moc"
