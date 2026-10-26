import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

const read = (path: string): string => readFileSync(path, "utf8");

const effectHeader = read("native-effect/activewindowborder.h");
const effectImpl = read("native-effect/activewindowborder.cpp");
const factsHeader = read("native-effect/presentationfacts.h");
const factsImpl = read("native-effect/presentationfacts.cpp");
const logic = read("native-effect/activeborderlogic.h");
const consumer = read("native-effect/presentation_consumer.h");
const cmake = read("native-effect/CMakeLists.txt");
const validator = read("native-effect/validate-metadata.cmake");

function countMatches(body: string, pattern: RegExp): number {
    const matches = body.match(pattern);
    return matches === null ? 0 : matches.length;
}

function extractBody(body: string, marker: string): string {
    const start = body.indexOf(marker);
    assert.ok(start >= 0, marker);
    const end = body.indexOf("\n}\n", start);
    assert.ok(end > start, marker);
    return body.slice(start, end + 3);
}

describe("presentation native static contract", () => {
    it("exposes the paint setter, paint clear, and the held facts waiter through the effect-owned endpoint", () => {
        assert.equal(countMatches(effectImpl, /Q_SCRIPTABLE/g), 3);
        assert.match(effectImpl, /SetPresentation/);
        assert.match(effectImpl, /ClearPresentation/);
        assert.doesNotMatch(effectImpl, /LastVerdict/);
        assert.doesNotMatch(effectImpl, /SetGroupHighlight/);
        assert.doesNotMatch(effectImpl, /ClearGroupHighlight/);
        assert.doesNotMatch(effectImpl, /GetGroupHighlightStatus/);
        assert.doesNotMatch(effectImpl, /SetDragTargetPreview/);
        assert.doesNotMatch(effectImpl, /ClearDragTargetPreview/);
        assert.doesNotMatch(effectHeader, /SetGroupHighlight/);
        assert.doesNotMatch(effectHeader, /SetDragTargetPreview/);
        // Read-only versioned presentation facts for the script route
        // (Stage 3 ingress): instance/revision identity, native chord
        // bits, actual style numbers, and a bounded held-call lease. The
        // Qt-only PresentationFactsSource module owns the waiter state
        // machine (the same production class the offline transport test
        // drives over a real bus); the endpoint forwards to it, and the
        // effect only pushes observed chord/style levels.
        assert.match(effectImpl, /WaitPresentationFacts/);
        assert.match(effectImpl, /PresentationFactsSource/);
        assert.match(effectImpl, /m_facts->handleWait/);
        assert.match(effectImpl, /noteFactsChord/);
        assert.match(effectImpl, /noteFactsStyle/);
        assert.match(factsHeader, /class PresentationFactsSource/);
        assert.match(factsHeader, /QDBusContext/);
        assert.match(factsHeader, /MAX_REQUEST_BYTES/);
        assert.match(factsImpl, /serviceOwner\(QStringLiteral\("org\.kde\.KWin"\)\)/);
        assert.match(factsImpl, /QDBusError::AccessDenied/);
        assert.match(factsImpl, /QDBusError::InvalidArgs/);
        assert.match(factsImpl, /MAX_REQUEST_BYTES/);
        assert.match(factsImpl, /available && m_tripleKnown/);
        assert.match(factsImpl, /completePending\(false, true, true\)/);
        assert.match(logic, /presentationFactsJson/);
        assert.match(logic, /presentationFactsStale/);
        assert.match(logic, /presentationFactsLeaseMs/);
        assert.match(logic, /"instance"/);
        assert.match(logic, /"changed"/);
        assert.match(logic, /\\"superseded\\":%9/);
        assert.match(logic, /\\"available\\"/);
        // No script maximize handoff: observation seeds directly from the
        // native committed maximizeMode() for the hide veto only.
        assert.doesNotMatch(effectImpl, /SetInitialMaximizeState/);
        assert.doesNotMatch(effectImpl, /ClearInitialMaximizeState/);
        assert.doesNotMatch(effectImpl, /GetInitialMaximizeEpoch/);
        assert.doesNotMatch(effectImpl, /initial_maximize_/);
        assert.doesNotMatch(effectImpl, /InitialMaximize/);
        assert.match(effectImpl, /Q_CLASSINFO\("D-Bus Interface", "com\.omnitiler\.ActiveBorder1"\)/);
        // Effect-owned dedicated connection to the same session bus: Qt
        // forbids delayed replies on local-loop (same-connection)
        // messages, so the held waiter needs the endpoint on its own
        // connection while the script keeps calling via KWin's sessionBus.
        assert.match(effectImpl, /connectToBus\(QDBusConnection::SessionBus/);
        assert.match(effectImpl, /omnitiler-active-border-endpoint/);
        assert.match(effectImpl, /disconnectFromBus/);
        assert.match(effectImpl, /m_groupBus/);
        assert.match(effectImpl, /new GroupHighlightObject\(this, m_groupBus/);
        assert.match(effectImpl, /QDBusConnection::sessionBus/);
        assert.match(effectImpl, /registerService\(QStringLiteral\("com\.omnitiler\.ActiveBorder"\)\)/);
        assert.match(effectImpl, /registerObject\(QStringLiteral\("\/com\/omnitiler\/ActiveBorder"\)/);
        assert.match(effectImpl, /ExportScriptableContents/);
        assert.match(effectImpl, /unregisterObject\(QStringLiteral\("\/com\/omnitiler\/ActiveBorder"\)\)/);
        assert.match(effectImpl, /unregisterService\(QStringLiteral\("com\.omnitiler\.ActiveBorder"\)\)/);
        // The generic effect bus cannot dispatch effect-defined setters.
        assert.doesNotMatch(effectImpl, /\/Effects/);
        assert.doesNotMatch(effectImpl, /reconfigureEffect/);
        assert.doesNotMatch(effectImpl, /org\.kde\.kwin\.Effects/);
    });

    it("seeds fresh facts sources before first serve and recovers a dropped dedicated bus", () => {
        // A new group object (construction and reconnect rebind) is seeded
        // with the actual rendered style and the current observed chord
        // before registration/first call: a fresh instance must never
        // serve default zeros as available facts.
        const regBody = extractBody(effectImpl, "bool ActiveWindowBorderEffect::ensureGroupEndpointRegistered()");
        const createdAt = regBody.indexOf("new GroupHighlightObject(this, m_groupBus, this)");
        assert.ok(createdAt >= 0, "dedicated-connection object creation");
        assert.ok(regBody.indexOf("noteFactsStyle()", createdAt) > createdAt, "style seed after create");
        assert.ok(regBody.indexOf("noteFactsChord()", createdAt) > createdAt, "chord seed after create");
        // Healthy and connected returns without re-registering; a dropped
        // dedicated connection re-enters the reconnect path even while the
        // endpoint still reads available.
        assert.match(regBody, /m_groupDbusAvailable && m_groupDbusObject != nullptr/);
        const endpointsBody = extractBody(effectImpl, "void ActiveWindowBorderEffect::ensureEndpointsRegistered()");
        assert.match(endpointsBody, /!m_groupDbusAvailable \|\| !m_groupBus\.isConnected\(\)/);
    });

    it("keeps the existing active border and adds exactly one filled underlay plus the preview overlay", () => {
        assert.equal(countMatches(effectHeader, /OutlinedBorderItem/g), 1);
        assert.ok(countMatches(effectHeader, /ImageItem/g) >= 2);
        assert.match(effectHeader, /m_borderItem/);
        assert.match(effectHeader, /m_groupItem/);
        assert.match(effectHeader, /m_groupAnchor/);
        assert.match(effectHeader, /m_presentation/);
        assert.match(effectHeader, /m_dragPreviewRect/);
        assert.doesNotMatch(effectHeader, /m_groupMemberIds/);
        assert.doesNotMatch(effectHeader, /m_groupState/);
        assert.doesNotMatch(effectImpl, /m_groupItem\(RectF\(\), BorderOutline\(\)\)/);
        // No valid anchor detaches so the underlay can never draw as a
        // screen overlay; the drag preview keeps the overlay parent.
        assert.match(effectImpl, /m_groupItem\.setParentItem\(nullptr\)/);
        assert.doesNotMatch(effectImpl, /m_groupItem\.setParentItem\(effects->scene\(\)->overlayItem\(\)\)/);
        assert.match(effectImpl, /m_dragPreviewItem\.setParentItem\(effects->scene\(\)->overlayItem\(\)\)/);
        assert.match(effectImpl, /m_groupItem\.setImage\(/);
        assert.match(effectImpl, /m_groupItem\.setPosition\(/);
        assert.match(effectImpl, /m_groupItem\.setSize\(/);
        assert.match(effectImpl, /m_groupItem\.setVisible\(/);
        assert.doesNotMatch(effectImpl, /m_groupItem\.setOutline\(/);
        assert.doesNotMatch(effectImpl, /m_groupItem\.setInnerRect\(/);
        // Underlay anchors under the lowest-stacked member at Z=-2 so it
        // slides with the workspace and higher members occlude it.
        assert.match(effectImpl, /m_groupItem\.setZ\(-2\)/);
        assert.match(effectImpl, /updateGroupAnchorAndGeometry/);
        assert.match(effectImpl, /stackingOrderChanged/);
        // The current anchor's own frame move remaps the stored scene outer
        // without a fresh payload; the old anchor disconnects on change.
        assert.match(effectImpl, /windowFrameGeometryChanged/);
        assert.match(effectImpl, /disconnect\(oldAnchor, &EffectWindow::windowFrameGeometryChanged/);
        assert.match(effectImpl, /connect\(anchor, &EffectWindow::windowFrameGeometryChanged/);
        // The stored core outer rect is consumed verbatim (no second
        // outset): space translation only, via mapFromScene.
        assert.match(effectImpl, /mapFromScene/);
        assert.doesNotMatch(effectImpl, /groupUnderlayOuterRect/);
        assert.doesNotMatch(effectImpl, /groupUnderlayEffectiveExtension/);
        assert.doesNotMatch(logic, /groupUnderlayOuterRect/);
        assert.doesNotMatch(logic, /groupUnderlayEffectiveExtension/);
        // Accepted non-group overlap paints over the lower edge.
        assert.match(effectImpl, /painted over/);
        // The active border is local to its target, below target contents and
        // later-stacked windows rather than a global overlay.
        assert.match(effectImpl, /m_borderItem\.setZ\(-1\)/);
        assert.match(effectImpl, /m_borderItem\.setParentItem\(m_trackedWindow->windowItem\(\)\)/);
        assert.match(effectImpl, /m_borderItem\.setParentItem\(effects->scene\(\)->overlayItem\(\)\)/);
        // The held accepted inner rect maps into the tracked window item
        // without a second outset; visibility is core paint plus live
        // foreground plus hide-only vetoes, never derived eligibility.
        assert.match(effectImpl, /m_presentation\.borderInner/);
        assert.match(effectImpl, /m_borderItem\.setInnerRect\(window \? window->windowItem\(\)->mapFromScene\(innerRect\) : RectF\(\)\)/);
        assert.match(effectImpl, /const bool visible = hasPaint && foregroundMatches && liveEligible && strokeOk && fresh;/);
        // The ring replays the published outer exactly: its stroke is
        // derived strictly from the published inner/outer pair (half the
        // uniform inset), never live config width, and a set geometry
        // floor hides stale shapes until a strictly newer paint arrives.
        assert.match(effectImpl, /borderStrokeFromPublished\(m_presentation\.borderInner, m_presentation\.borderOuter\)/);
        assert.match(effectImpl, /presentationBorderFresh\(&m_presentation\)/);
        assert.match(effectImpl, /setOutline\(ringOutlineForWidth\(stroke\)\)/);
        assert.match(effectImpl, /onTrackedFrameChanged/);
        assert.match(effectImpl, /onGroupMemberFrameChanged/);
        assert.match(effectImpl, /markBorderGeometryDirty\(&m_presentation\)/);
        assert.match(effectImpl, /markUnderlayGeometryDirty\(&m_presentation\)/);
        assert.match(effectHeader, /onTrackedFrameChanged\(\);/);
        assert.match(effectHeader, /onGroupMemberFrameChanged\(EffectWindow \*window\);/);
        assert.doesNotMatch(effectImpl, /initialOk/);
        assert.match(effectImpl, /m_borderItem\.setVisible\(visible\)/);
    });

    it("observes the chord as raw ingress without deriving trigger gating", () => {
        assert.match(effectImpl, /mouseChanged/);
        assert.match(effectImpl, /Qt::MetaModifier/);
        assert.match(effectImpl, /Qt::ShiftModifier/);
        assert.match(effectImpl, /m_firstMouseSeen/);
        assert.match(effectImpl, /m_chordHeld/);
        // The core underlay Found already encodes the trigger: native code
        // performs no chord/eligibility/visibility derivation.
        assert.doesNotMatch(effectImpl, /groupUnderlayChordHeld/);
        assert.doesNotMatch(effectImpl, /group_highlight_is_visible/);
        assert.doesNotMatch(effectImpl, /group_highlight_focus_eligible/);
        assert.doesNotMatch(logic, /groupUnderlayChordHeld/);
        assert.match(effectImpl, /noteFactsChord/);
        // Fullscreen/minimized/hidden/deleted hide via owned tracked signals
        // without pointer movement.
        assert.match(effectImpl, /minimizedChanged/);
        assert.match(effectImpl, /windowFullScreenChanged/);
        assert.match(effectImpl, /windowClosed/);
        assert.match(effectImpl, /updateGroupVisibility/);
        assert.match(effectImpl, /foregroundMatchesNow/);
        assert.match(effectImpl, /isLiveEligibleNow/);
    });

    it("gates the preview on live gesture liveness without identity matching", () => {
        // Classification at Started via the underlying Window; resize alone
        // never arms and the chord stays independent.
        assert.match(effectImpl, /isInteractiveMove/);
        assert.match(effectImpl, /updateGroupMoveArm/);
        assert.match(effectImpl, /m_groupMoveWindow/);
        assert.match(effectImpl, /groupMoveLiveNow/);
        // The published preview win is the insertion neighbor, never the
        // mover: no identity match against the mover is performed anywhere.
        assert.doesNotMatch(effectImpl, /group_highlight_move_arm_matches/);
        assert.doesNotMatch(effectImpl, /groupMoveMatchesNow/);
        // End/cancel/removal clears the exact tracked move window only.
        assert.match(effectImpl, /m_groupMoveWindow == window/);
        assert.match(effectImpl, /m_groupMoveWindow\.clear\(\)/);
        // Finish clears the exact liveness first, from the authoritative
        // gesture callback, so preview hiding never depends on verdict
        // success or readable geometry.
        const finishBody = extractBody(effectImpl, "void ActiveWindowBorderEffect::onGestureDragFinish(");
        const armClear = finishBody.indexOf("m_groupMoveWindow.clear()");
        const terminal = finishBody.indexOf("gestureFinished");
        assert.ok(armClear >= 0 && terminal > armClear);
        // Same-window resize Started clears a stale own arm via the current
        // move classification.
        const armBody = extractBody(effectImpl, "void ActiveWindowBorderEffect::updateGroupMoveArm(");
        assert.match(armBody, /isInteractiveMove/);
        assert.match(armBody, /m_groupMoveWindow == window/);
        // The preview show gate is held paint plus liveness plus endpoint
        // only; the rect is consumed verbatim in overlay space.
        const previewBody = extractBody(effectImpl, "void ActiveWindowBorderEffect::updateDragPreview()");
        assert.match(previewBody, /m_presentation\.previewHas/);
        assert.match(previewBody, /groupMoveLiveNow/);
        assert.match(previewBody, /m_presentation\.previewRect/);
    });

    it("reports bounded paint setter and transition outcomes without raw identities", () => {
        assert.match(effectImpl, /presentation:setter outcome=%1 has=%2 bvis=%3 uvis=%4 pvis=%5 members=%6 anchor=%7 fmatch=%8 move=%9 elig=%10 ep=%11/);
        assert.match(effectImpl, /presentation:transition anchor=%1 members=%2 fmatch=%3 move=%4 elig=%5 ep=%6 vis=%7/);
        for (const outcome of ["accepted", "stale", "malformed", "endpoint-unavailable"]) {
            assert.match(effectImpl, new RegExp(`emitPresentationSetterDiag\\("${outcome}"\\)`));
        }
        for (const reason of ["no-presentation", "underlay-hidden", "no-members", "selected", "no-member-match", "all-items-hidden"]) {
            assert.ok(effectImpl.includes(`"${reason}"`), reason);
        }
        // The setter emitter carries bounded scalars only: fixed tokens and
        // bits, no identities, payload, geometry, or revision numbers.
        const emitter = extractBody(effectImpl, "void ActiveWindowBorderEffect::emitPresentationSetterDiag(");
        assert.doesNotMatch(emitter, /internalId|payloadBytes|frameGeometry/);
    });

    it("seeds maximize observation directly from committed mode for the hide veto only", () => {
        // Each observed window seeds m_maximizedWindows from window()->maximizeMode().
        assert.match(effectImpl, /maximizeMode\(\)/);
        assert.doesNotMatch(effectImpl, /activeBorderSeedMaximized/);
        assert.doesNotMatch(logic, /activeBorderSeedMaximized/);
        // Any maximize axis hides the paint; fullscreen stays hidden
        // independently via live isFullScreen().
        assert.match(effectImpl, /m_maximizedWindows\.insert\(window\)/);
        assert.match(effectImpl, /observe-seed/);
        // Native transition signals stay authoritative after the seed.
        assert.match(effectImpl, /windowMaximizedStateChanged/);
        assert.match(effectImpl, /windowMaximizedStateAboutToChange/);
        assert.match(effectImpl, /updateMaximizedState/);
        // No script handoff residue anywhere in the native contract.
        assert.doesNotMatch(effectHeader, /InitialMaximizeState/);
        assert.doesNotMatch(effectHeader, /m_initialEpoch/);
        assert.doesNotMatch(effectHeader, /initialMaximizeEpoch/);
        assert.doesNotMatch(effectHeader, /clearInitialGate/);
        assert.doesNotMatch(effectHeader, /isInitialConfirmedNormal/);
        // No script handoff residue anywhere in the maximize/diag paths:
        // the facts instance UUID lives only in the factored source module.
        // (QUuid::createUuid appears exactly once, minting the source
        // instance.)
        assert.equal(countMatches(factsImpl, /QUuid::createUuid/g), 1);
        assert.doesNotMatch(effectImpl, /QUuid::createUuid/);
        for (const body of [
            extractBody(effectImpl, "void ActiveWindowBorderEffect::subscribeMaximize("),
            extractBody(effectImpl, "void ActiveWindowBorderEffect::updateMaximizedState("),
            extractBody(effectImpl, "void ActiveWindowBorderEffect::emitActiveBorderVisible("),
        ]) {
            assert.doesNotMatch(body, /QUuid::createUuid/);
        }
        assert.doesNotMatch(effectImpl, /m_initialEpoch/);
        assert.doesNotMatch(effectImpl, /clearInitialGate/);
        assert.doesNotMatch(effectImpl, /isInitialConfirmedNormal/);
        assert.doesNotMatch(effectImpl, /emitActiveBorderApply/);
        // No Qt JSON in the implementation: parsing lives in the shared
        // consumer header (offline-tested); member matching uses bare-UUID
        // string compares against the held paint list.
        assert.doesNotMatch(effectImpl, /QJsonDocument/);
        assert.match(effectImpl, /m_presentation\.underlayMembers/);
    });

    it("binds the paint to the live foreground, clears on activation, and fails closed on endpoint loss", () => {
        // The held paint's foreground binds to the live active internalId;
        // a stale paint never renders under a new focus.
        assert.match(effectImpl, /internalId/);
        assert.match(effectImpl, /WithoutBraces/);
        assert.match(effectImpl, /foregroundMatchesNow/);
        assert.match(effectHeader, /PresentationPaint m_presentation/);
        // Focus activation clears immediately before async refresh.
        assert.match(effectImpl, /windowActivated/);
        assert.match(effectImpl, /clearPresentation\(\);/);
        // Registration failure fails closed with no retry. Focus clears
        // synchronously (no async settle); the only singleShot in the
        // effect is the facts-waiter lease, never the paint path.
        assert.match(effectHeader, /m_groupDbusAvailable/);
        assert.match(effectImpl, /m_groupDbusAvailable/);
        for (const body of [
            extractBody(effectImpl, "void ActiveWindowBorderEffect::applyPresentation("),
            extractBody(effectImpl, "void ActiveWindowBorderEffect::clearPresentation("),
        ]) {
            assert.doesNotMatch(body, /singleShot/);
        }
    });

    it("parses the strict paint in the shared consumer with fencing and no policy", () => {
        assert.match(consumer, /applyPresentationPaint/);
        assert.match(consumer, /clearPresentationPaint/);
        assert.match(consumer, /clearPaintDisplay/);
        assert.match(consumer, /clearPaintPreview/);
        assert.match(consumer, /StaleIgnored/);
        assert.match(consumer, /Malformed/);
        assert.match(consumer, /-16384/);
        assert.match(consumer, /16384/);
        assert.match(consumer, /members/);
        assert.match(consumer, /preview/);
        assert.match(consumer, /foreground/);
        assert.match(consumer, /"obs"/);
        // Finding B: value-based integer checks (Qt parses every number
        // as double, so lexical spelling is unobservable). The bounds and
        // finiteness check strictly precedes the cast.
        assert.match(consumer, /isJsonIntValue/);
        assert.doesNotMatch(consumer, /isStrictInt/);
        assert.match(consumer, /std::isfinite/);
        // Findings F/G: order fence independent of visible state, geometry
        // floors cleared only by strictly newer content, preview latch
        // cleared on gesture boundaries.
        assert.match(consumer, /PaintOrder/);
        assert.match(consumer, /paintOrderGreater/);
        assert.match(consumer, /markBorderGeometryDirty/);
        assert.match(consumer, /markUnderlayGeometryDirty/);
        assert.match(consumer, /presentationBorderFresh/);
        assert.match(consumer, /presentationUnderlayFresh/);
        // Finding C: the stroke derives strictly from the published pair.
        assert.match(consumer, /borderStrokeFromPublished/);
        // The consumer header holds Qt-only parse plus order fencing: no
        // KWin/effect types, no styling inputs, no derivation.
        assert.doesNotMatch(consumer, /EffectWindow/);
        assert.doesNotMatch(consumer, /group_highlight_/);
        assert.doesNotMatch(consumer, /visual_/);
        assert.doesNotMatch(consumer, /color/);
        assert.doesNotMatch(consumer, /Color/);
        for (const moved of [
            /parseGroupHighlightPayload/,
            /acceptGroupHighlightOrder/,
            /shouldShowGroupHighlight/,
            /groupFocusEligible/,
            /groupFocusMatches/,
            /isGroupEndpointUsable/,
        ]) {
            assert.doesNotMatch(effectHeader, moved);
            assert.doesNotMatch(effectImpl, moved);
            assert.doesNotMatch(logic, moved);
            assert.doesNotMatch(consumer, moved);
        }
    });

    it("forbids polling, grabs, interception, filters, shortcuts, and custom scene rendering", () => {
        for (const forbidden of [
            /startMousePolling/,
            /grabMouse/,
            /grabKeyboard/,
            /installEventFilter/,
            /installSceneFilter/,
            /registerShortcut/,
            /registerPropertyType/,
            /paintWindow/,
            /drawWindow/,
            /prePaintScreen/,
            /postPaintScreen/,
            /startTimer/,
            /QTimer::singleShot/,
        ]) {
            assert.doesNotMatch(effectImpl, forbidden);
            assert.doesNotMatch(effectHeader, forbidden);
        }
        // The effect header carries no QTimer at all: the single lease
        // timer lives in the factored facts module (pinned below).
        assert.doesNotMatch(effectHeader, /QTimer/);
    });

    it("gates the paintScreen override spelling on the KWin package version", () => {
        // Causal compile repair: KWin 6.8 changed Effect::paintScreen from
        // void to [[nodiscard]] bool (sibling v6.7.4 effect.h:709 vs
        // v6.8.0 :712; the effect API macro stayed 0.237, so no header
        // macro fences it). The package version fences the spelling: 6.7
        // keeps the void passthrough, 6.8 propagates the result. No
        // runtime route, no renderer policy.
        assert.match(cmake, /OMNITILER_KWIN_PAINTSCREEN_BOOL/);
        assert.match(cmake, /KWin_VERSION VERSION_GREATER_EQUAL "6\.8"/);
        assert.match(effectHeader, /#ifdef OMNITILER_KWIN_PAINTSCREEN_BOOL/);
        assert.match(effectHeader, /\[\[nodiscard\]\] bool paintScreen\(/);
        assert.match(effectImpl, /return effects->paintScreen\(renderTarget, viewport, mask, deviceRegion, screen\);/);
    });

    it("bounds the presentation-facts waiter to a single lease timer", () => {
        // The held WaitPresentationFacts call needs a bounded lease so a
        // wedged waiter recycles; this is the only timer in the facts
        // module: single-shot, waiter-scoped, never polling, never
        // rendering.
        assert.match(factsHeader, /QTimer m_leaseTimer/);
        const armBody = extractBody(factsImpl, "void PresentationFactsSource::armWaiter(");
        assert.ok(armBody.length > 0);
        assert.match(armBody, /m_leaseTimer\.start\(presentationFactsLeaseMs\(leaseMs\)\)/);
        const ctorBody = extractBody(factsImpl, "PresentationFactsSource::PresentationFactsSource(");
        assert.ok(ctorBody.length > 0);
        assert.match(ctorBody, /m_leaseTimer\.setSingleShot\(true\)/);
        assert.equal(countMatches(factsImpl, /new QTimer\(/g), 0);
        assert.doesNotMatch(factsImpl, /QTimer::singleShot/);
        assert.doesNotMatch(factsImpl, /\.start\([0-9]/);
        assert.doesNotMatch(effectImpl, /m_factsWaitTimer/);
        assert.doesNotMatch(effectImpl, /new QTimer/);
        assert.doesNotMatch(effectHeader, /QTimer/);
    });

    it("covers changed files in CMake and the source validator", () => {
        assert.match(cmake, /presentationconsumer_test\.cpp/);
        assert.match(cmake, /native-effect-presentation-consumer/);
        assert.match(cmake, /presentationfacts_test\.cpp/);
        assert.match(cmake, /presentation_consumer\.h/);
        assert.match(cmake, /native-effect-facts-transport/);
        assert.match(cmake, /presentationfacts\.h/);
        assert.match(cmake, /presentationfacts\.cpp/);
        assert.match(cmake, /OMNITILER_KWIN_PAINTSCREEN_BOOL/);
        assert.match(cmake, /PRES_HEADER/);
        assert.match(cmake, /PRES_IMPL/);
        assert.match(cmake, /PRES_LOGIC/);
        assert.match(cmake, /PRES_TEST/);
        assert.match(cmake, /PRES_CONSUMER/);
        assert.doesNotMatch(cmake, /activebordergroup_test\.cpp/);
        assert.doesNotMatch(cmake, /native-effect-group-highlight/);
        assert.doesNotMatch(cmake, /group-highlight-rs/);
        assert.match(cmake, /DRAG_HEADER/);
        assert.match(cmake, /DRAG_IMPL/);
        assert.match(cmake, /validate-unified-lifecycle\.cmake/);
        assert.match(validator, /PRES_HEADER/);
        assert.match(validator, /PRES_IMPL/);
        assert.match(validator, /PRES_LOGIC/);
        assert.match(validator, /PRES_CONSUMER/);
        assert.match(validator, /SetPresentation/);
        assert.match(validator, /ClearPresentation/);
        // The retired verdict endpoint is named only as forbidden residue;
        // the survivor must prove the gesture endpoint instead.
        assert.doesNotMatch(validator, /drag-oracle endpoint validation failed/);
        assert.doesNotMatch(validator, /oracleMoveResizeRect/);
        assert.match(validator, /gesture endpoint validation failed/);
        assert.match(validator, /gestureMoveResizeRect/);
        // The validator's deleted-method loop names the retired setters as
        // forbidden data; no wiring, FFI, or Rust reference to them remains.
        assert.match(validator, /PRES_DELETED_METHOD/);
        assert.doesNotMatch(validator, /GROUP_FFI/);
        assert.doesNotMatch(validator, /GROUP_RUST/);
        assert.match(validator, /mouseChanged/);
        assert.match(validator, /MetaModifier/);
        assert.match(validator, /OutlinedBorderItem/);
        assert.match(validator, /ImageItem/);
        assert.match(validator, /updateGroupAnchorAndGeometry/);
        assert.match(validator, /stackingOrderChanged/);
        assert.match(validator, /foregroundMatchesNow/);
        assert.match(validator, /mapFromScene/);
        assert.match(validator, /m_groupDbusAvailable/);
    });

    it("retires the drag oracle: gesture lifecycle owns preview with one shared lifecycle and no second plugin", () => {
        // The retired verdict endpoint, press spy, and FFI record calls
        // are gone; the core GestureIngress lane alone owns preview/drop.
        assert.doesNotMatch(effectImpl, /Q_CLASSINFO\("D-Bus Interface", "com\.omnitiler\.DragOracle1"\)/);
        assert.doesNotMatch(effectImpl, /com\.omnitiler\.DragOracle/);
        assert.doesNotMatch(effectImpl, /LastVerdict/);
        assert.doesNotMatch(effectImpl, /drag_oracle_last_copy/);
        assert.doesNotMatch(effectImpl, /drag_oracle_record/);
        assert.doesNotMatch(effectImpl, /OraclePress/);
        assert.doesNotMatch(effectImpl, /m_oracle/);
        assert.doesNotMatch(effectHeader, /m_oracle/);
        assert.doesNotMatch(effectHeader, /drag_oracle_ffi\.h/);
        assert.doesNotMatch(effectHeader, /OraclePressSpy/);
        // The authoritative gesture endpoint and lifecycle observation
        // survive: one shared window wiring, native start/step/finish.
        assert.match(effectImpl, /registerService\(QStringLiteral\("com\.omnitiler\.Gesture"\)\)/);
        assert.match(effectImpl, /registerObject\(QStringLiteral\("\/com\/omnitiler\/Gesture"\)/);
        assert.match(effectImpl, /unregisterObject\(QStringLiteral\("\/com\/omnitiler\/Gesture"\)\)/);
        assert.match(effectImpl, /unregisterService\(QStringLiteral\("com\.omnitiler\.Gesture"\)\)/);
        // Construction seed plus the presentation lowest-stacked anchor
        // share the lifecycle; stacking changes re-anchor the underlay.
        assert.equal(countMatches(effectImpl, /stackingOrder\(\)/g), 2);
        assert.match(effectImpl, /stackingOrderChanged/);
        assert.equal(countMatches(effectImpl, /EffectsHandler::windowAdded/g), 1);
        assert.equal(countMatches(effectImpl, /EffectsHandler::windowClosed/g), 1);
        assert.equal(countMatches(effectImpl, /EffectsHandler::windowDeleted/g), 1);
        assert.match(effectImpl, /attachGestureWindow/);
        assert.match(effectImpl, /forgetGestureWindow/);
        assert.match(effectImpl, /onGestureDragStart/);
        assert.match(effectImpl, /onGestureDragFinish/);
        assert.match(effectImpl, /windowStartUserMovedResized/);
        assert.match(effectImpl, /windowFinishUserMovedResized/);
        assert.match(effectImpl, /moveResizeGeometry/);
        assert.match(effectImpl, /gestureMoveResizeRect/);
        // The gesture observer was rendering-independent. The survivor must
        // attach it before the active-border-only OpenGL early return.
        const gestureSetup = effectImpl.indexOf("attachGestureWindow(window);");
        assert.ok(gestureSetup >= 0);
        assert.ok(effectImpl.indexOf("attachGestureWindow(window);", gestureSetup) < effectImpl.indexOf("m_borderItem.setZ(-1)", gestureSetup));
        // Live move start arms the latch; finish/removal clears the exact
        // tracked window, all from the authoritative gesture callbacks.
        assert.match(effectImpl, /updateGroupMoveArm\(window\)/);
        // No second plugin effect or factory.
        assert.doesNotMatch(effectImpl, /DragOracleEffect/);
        assert.doesNotMatch(effectImpl, /dragoracle-metadata\.json/);
        assert.doesNotMatch(effectImpl, /omnitiler-drag-oracle/);
        assert.doesNotMatch(effectHeader, /DragOracleEffect/);
        // Rendering: one active outline plus the filled underlay and
        // the drag preview fill.
        assert.equal(countMatches(effectHeader, /OutlinedBorderItem/g), 1);
        assert.ok(countMatches(effectHeader, /ImageItem/g) >= 2);
    });
});
describe("active-border visibility diagnostics", () => {
    function functionBody(body: string, marker: string): string {
        const start = body.indexOf(marker);
        assert.ok(start >= 0, marker);
        const end = body.indexOf("\n}\n", start);
        assert.ok(end > start, marker);
        return body.slice(start, end + 3);
    }

    it("emits bounded endpoint, observe-seed, and visible shapes from fixed sites", () => {
        assert.match(effectImpl, /Q_LOGGING_CATEGORY\(lcActiveBorder,\s*"omnitiler\.activeborder"\)/);
        assert.match(effectImpl, /qCInfo\(lcActiveBorder\)\.noquote\(\)/);
        assert.match(effectImpl, /omnitiler:active-border:endpoint available=/);
        assert.match(effectImpl, /omnitiler:active-border:observe-seed maximized=/);
        assert.match(effectImpl, /omnitiler:active-border:visible vis=%1 reason=%2 appletPopup=%3/);
        assert.match(effectImpl, /emitActiveBorderEndpoint\(\)/);
        assert.match(effectImpl, /emitActiveBorderVisible\(visible,/);
        assert.doesNotMatch(effectImpl, /initial-apply/);
        assert.doesNotMatch(effectImpl, /emitActiveBorderApply/);
        assert.match(effectHeader, /m_borderDiagEmitted/);
        assert.match(effectHeader, /m_borderDiagVisible/);
        assert.match(effectHeader, /emitActiveBorderVisible\(bool visible, const char \*reason, bool appletPopup\)/);
        for (const token of [
            "eligible",
            "no-presentation",
            "foreground-mismatch",
            "no-window",
            "deleted",
            "minimized",
            "fullscreen",
            "maximized",
            "applet-popup",
            "endpoint-unavailable",
        ]) {
            assert.ok(effectImpl.includes(`"${token}"`), token);
        }
        assert.ok(!effectImpl.includes('"initial-unconfirmed"'), "initial-unconfirmed");
        // Endpoint once after registration, seed inside subscribeMaximize,
        // visible inside updateBorder.
        const ctorEndpoint = effectImpl.indexOf("emitActiveBorderEndpoint();");
        assert.ok(ctorEndpoint > effectImpl.indexOf("m_groupDbusAvailable = true;"));
        const seedLog = effectImpl.indexOf("omnitiler:active-border:observe-seed");
        assert.ok(seedLog > effectImpl.indexOf("void ActiveWindowBorderEffect::subscribeMaximize("));
        const updateBorderBody = functionBody(effectImpl, "void ActiveWindowBorderEffect::updateBorder()");
        assert.match(updateBorderBody, /emitActiveBorderVisible\(visible,/);
    });

    it("emits visible edge-only on first evaluation and visibility flips", () => {
        assert.equal(countMatches(effectImpl, /omnitiler:active-border:visible/g), 1);
        assert.equal(countMatches(effectImpl, /emitActiveBorderVisible/g), 2);
        const visibleBody = functionBody(effectImpl, "void ActiveWindowBorderEffect::emitActiveBorderVisible(");
        assert.match(visibleBody, /m_borderDiagEmitted/);
        assert.match(visibleBody, /m_borderDiagVisible/);
        assert.match(visibleBody, /visible == m_borderDiagVisible/);
        // updateBorder delegates edge dedup to the emitter; it must call it
        // with the computed visibility but hold no ledger itself.
        const updateBorderBody = functionBody(effectImpl, "void ActiveWindowBorderEffect::updateBorder()");
        assert.match(updateBorderBody, /emitActiveBorderVisible\(visible,/);
        assert.doesNotMatch(updateBorderBody, /m_borderDiagVisible =/);
        // No timers, polling, or new registry/transport for diagnostics.
        for (const forbidden of [/QTimer/, /singleShot/, /startTimer/, /registerService/, /registerObject/]) {
            assert.doesNotMatch(visibleBody, forbidden);
        }
    });

    it("keeps diagnostics out of paint, mouse, and group update paths", () => {
        const paintBody = functionBody(effectImpl, "void ActiveWindowBorderEffect::paintScreen(");
        assert.doesNotMatch(paintBody, /active-border:/);
        assert.doesNotMatch(paintBody, /emitActiveBorder/);
        assert.doesNotMatch(paintBody, /lcActiveBorder/);
        const mouseBody = functionBody(effectImpl, "void ActiveWindowBorderEffect::onMouseChanged(");
        assert.doesNotMatch(mouseBody, /active-border:/);
        assert.doesNotMatch(mouseBody, /emitActiveBorder/);
        assert.doesNotMatch(mouseBody, /lcActiveBorder/);
        const groupBody = functionBody(effectImpl, "void ActiveWindowBorderEffect::updateGroupVisibility()");
        assert.doesNotMatch(groupBody, /active-border:/);
        assert.doesNotMatch(groupBody, /emitActiveBorder/);
        assert.doesNotMatch(groupBody, /lcActiveBorder/);
    });

    it("uses fixed bounded fields with no identity, payload, or geometry", () => {
        const seedMarker = "omnitiler:active-border:observe-seed";
        const seedAt = effectImpl.indexOf(seedMarker);
        assert.ok(seedAt >= 0, seedMarker);
        const subscribeBody = functionBody(effectImpl, "void ActiveWindowBorderEffect::subscribeMaximize(");
        assert.ok(subscribeBody.includes(seedMarker), seedMarker);
        assert.match(subscribeBody, /maximized=%1 fullscreen=%2/);
        const visibleBody = functionBody(effectImpl, "void ActiveWindowBorderEffect::emitActiveBorderVisible(");
        assert.match(visibleBody, /vis=%1 reason=%2/);
        const endpointBody = functionBody(effectImpl, "void ActiveWindowBorderEffect::emitActiveBorderEndpoint(");
        assert.match(endpointBody, /endpoint available=%1/);
        for (const body of [subscribeBody, visibleBody, endpointBody]) {
            for (const forbidden of [
                /internalId/,
                /WithoutBraces/,
                /payloadBytes/,
                /activeBytes/,
                /frameGeometry/,
                /windowItem/,
                /mapFromScene/,
                /maximize_mode/,
                /generation/,
                /QUuid::createUuid/,
                /QRectF/,
            ]) {
                assert.doesNotMatch(body, forbidden);
            }
        }
        // Logging failures are swallowed and never gate rendering decisions.
        const logBody = functionBody(effectImpl, "void ActiveWindowBorderEffect::logActiveBorderDiag(");
        assert.match(logBody, /catch \(\.\.\.\)/);
        assert.doesNotMatch(logBody, /setVisible/);
        assert.doesNotMatch(logBody, /addRepaintFull/);
        assert.doesNotMatch(logBody, /initial_maximize_/);
    });
});
