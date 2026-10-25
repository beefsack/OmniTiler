import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

const read = (path: string): string => readFileSync(path, "utf8");

const effectHeader = read("native-effect/activewindowborder.h");
const effectImpl = read("native-effect/activewindowborder.cpp");
const factsHeader = read("native-effect/presentationfacts.h");
const factsImpl = read("native-effect/presentationfacts.cpp");
const logic = read("native-effect/activeborderlogic.h");
const cmake = read("native-effect/CMakeLists.txt");
const validator = read("native-effect/validate-metadata.cmake");
const ffi = read("native-effect/group_highlight_ffi.h");
const rust = read("../crates/tiler-kwin-effect-ffi/src/group_highlight.rs");

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

describe("active-group native static contract", () => {
    it("exposes setter, clear, a read-only status query, and the held facts waiter through the effect-owned endpoint", () => {
        assert.equal(countMatches(effectImpl, /Q_SCRIPTABLE/g), 7);
        assert.match(effectImpl, /SetGroupHighlight/);
        assert.match(effectImpl, /ClearGroupHighlight/);
        assert.match(effectImpl, /GetGroupHighlightStatus/);
        assert.match(effectImpl, /LastVerdict/);
        assert.match(effectImpl, /SetDragTargetPreview/);
        assert.match(effectImpl, /ClearDragTargetPreview/);
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
        assert.match(factsImpl, /QDBusError::InvalidArgs/);
        assert.match(factsImpl, /completePending\(false, true, true\)/);
        assert.match(logic, /presentationFactsJson/);
        assert.match(logic, /presentationFactsStale/);
        assert.match(logic, /presentationFactsLeaseMs/);
        assert.match(logic, /"instance"/);
        assert.match(logic, /"changed"/);
        assert.match(logic, /\\"superseded\\":%9/);
        assert.match(logic, /\\"available\\"/);
        // No script maximize handoff: observation seeds directly from the
        // native committed maximizeMode().
        assert.doesNotMatch(effectImpl, /SetInitialMaximizeState/);
        assert.doesNotMatch(effectImpl, /ClearInitialMaximizeState/);
        assert.doesNotMatch(effectImpl, /GetInitialMaximizeEpoch/);
        assert.doesNotMatch(effectImpl, /initial_maximize_/);
        assert.doesNotMatch(effectImpl, /InitialMaximize/);
        assert.match(effectHeader, /groupHighlightStatus\(\) const/);
        assert.match(effectImpl, /Q_CLASSINFO\("D-Bus Interface", "com\.omnitiler\.ActiveBorder1"\)/);
        // Effect-owned dedicated connection to the same session bus: Qt
        // forbids delayed replies on local-loop (same-connection)
        // messages, so the held waiter needs the endpoint on its own
        // connection while the script keeps calling via KWin's sessionBus.
        // Same service/path/interface/shape; the oracle stays on sessionBus.
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

    it("keeps the existing active border and adds exactly one filled group underlay", () => {
        assert.equal(countMatches(effectHeader, /OutlinedBorderItem/g), 1);
        assert.ok(countMatches(effectHeader, /ImageItem/g) >= 2);
        assert.match(effectHeader, /m_borderItem/);
        assert.match(effectHeader, /m_groupItem/);
        assert.match(effectHeader, /m_groupMemberIds/);
        assert.match(effectHeader, /m_groupAnchor/);
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
        // The current anchor's own frame move remaps the stored scene union
        // without a fresh payload; the old anchor disconnects on change.
        assert.match(effectImpl, /windowFrameGeometryChanged/);
        assert.match(effectImpl, /disconnect\(oldAnchor, &EffectWindow::windowFrameGeometryChanged/);
        assert.match(effectImpl, /connect\(anchor, &EffectWindow::windowFrameGeometryChanged/);
        assert.match(effectImpl, /groupUnderlayOuterRect/);
        assert.match(logic, /groupUnderlayOuterRect/);
        // Default extension sentinel follows the current border width;
        // explicit 0 renders as-is.
        assert.match(logic, /groupUnderlayEffectiveExtension/);
        assert.match(effectImpl, /groupUnderlayEffectiveExtension\(\s*ActiveBorderConfig::groupUnderlayExtension\(\),\s*ActiveBorderConfig::borderWidth\(\)\)/);
        // Accepted non-group overlap paints over the lower edge.
        assert.match(effectImpl, /painted over/);
        // The active border is local to its target, below target contents and
        // later-stacked windows rather than a global overlay.
        assert.match(effectImpl, /m_borderItem\.setZ\(-1\)/);
        assert.match(effectImpl, /m_borderItem\.setParentItem\(m_trackedWindow->windowItem\(\)\)/);
        assert.match(effectImpl, /m_borderItem\.setParentItem\(effects->scene\(\)->overlayItem\(\)\)/);
        assert.match(effectImpl, /const QRectF innerRect = activeBorderInnerRect\(state\.innerRect, gap\)/);
        assert.match(effectImpl, /m_borderItem\.setInnerRect\(window \? window->windowItem\(\)->mapFromScene\(innerRect\) : RectF\(\)\)/);
        assert.match(effectImpl, /const bool visible = state\.visible;/);
        assert.doesNotMatch(effectImpl, /initialOk/);
        assert.match(effectImpl, /m_borderItem\.setVisible\(visible\)/);
    });

    it("gates group visibility on the passive Meta+Shift chord with unknown-before-first-signal invisible", () => {
        assert.match(effectImpl, /mouseChanged/);
        assert.match(effectImpl, /Qt::MetaModifier/);
        assert.match(effectImpl, /Qt::ShiftModifier/);
        assert.match(effectImpl, /m_firstMouseSeen/);
        assert.match(effectImpl, /m_chordHeld/);
        assert.match(effectImpl, /groupUnderlayChordHeld/);
        assert.match(effectImpl, /group_highlight_is_visible/);
        assert.match(effectImpl, /group_highlight_focus_eligible/);
        assert.match(rust, /tiler_core::visual::group_underlay_trigger\(chord_ready, matching_move_active\)/);
        assert.match(rust, /tiler_core::visual::group_focus_eligible\(/);
        // Fullscreen/minimized/hidden/deleted hide via owned tracked signals
        // without pointer movement.
        assert.match(effectImpl, /minimizedChanged/);
        assert.match(effectImpl, /windowFullScreenChanged/);
        assert.match(effectImpl, /windowClosed/);
        assert.match(effectImpl, /updateGroupVisibility/);
    });

    it("arms a matching focused interactive move without modifier observation", () => {
        // Classification at Started via the underlying Window; resize alone
        // never arms and the chord stays independent.
        assert.match(effectImpl, /isInteractiveMove/);
        assert.match(effectImpl, /updateGroupMoveArm/);
        assert.match(effectImpl, /m_groupMoveWindow/);
        assert.match(effectImpl, /group_highlight_move_arm_matches/);
        assert.match(ffi, /group_highlight_move_arm_matches/);
        assert.match(rust, /group_highlight_move_arm_matches/);
        // The chord fold stays shared policy in logic; the trigger OR lives
        // in the group FFI policy (should_show), with no native wrapper.
        assert.match(logic, /groupUnderlayChordHeld/);
        assert.doesNotMatch(logic, /groupUnderlayTrigger/);
        // End/cancel/removal clears the exact tracked window arm only.
        assert.match(effectImpl, /m_groupMoveWindow == window/);
        assert.match(effectImpl, /m_groupMoveWindow\.clear\(\)/);
        // The move flag reaches visibility; the first-signal gate stays on
        // the chord branch only.
        assert.match(effectImpl, /matchingMove/);
        assert.match(rust, /first_signal_seen && chord_held/);
        // Visibility revalidates through the matcher each refresh: the body
        // of updateGroupVisibility must route through groupMoveMatchesNow,
        // never a bare pointer check, and that helper must call the actual
        // Rust subject matcher against the live active window.
        const bodyOf = (marker: string): string => {
            const start = effectImpl.indexOf(marker);
            assert.ok(start >= 0, marker);
            const end = effectImpl.indexOf("\n}\n", start);
            assert.ok(end > start, marker);
            return effectImpl.slice(start, end + 3);
        };
        const visBody = bodyOf("void ActiveWindowBorderEffect::updateGroupVisibility()");
        assert.match(visBody, /groupMoveMatchesNow/);
        assert.doesNotMatch(visBody, /!m_groupMoveWindow\.isNull\(\)/);
        const freshBody = bodyOf("bool ActiveWindowBorderEffect::groupMoveMatchesNow()");
        assert.match(freshBody, /group_highlight_move_arm_matches/);
        assert.match(freshBody, /activeWindow/);
        // Finish clears the exact arm before any oracle-state read, so move
        // cleanup never depends on verdict success or readable geometry.
        const finishBody = bodyOf("void ActiveWindowBorderEffect::onOracleDragFinish(");
        const armClear = finishBody.indexOf("m_groupMoveWindow.clear()");
        const verdict = finishBody.indexOf("drag_oracle_record");
        assert.ok(armClear >= 0 && verdict > armClear);
        // Same-window resize Started clears a stale own arm via the current
        // move classification.
        const armBody = bodyOf("void ActiveWindowBorderEffect::updateGroupMoveArm(");
        assert.match(armBody, /isInteractiveMove/);
        assert.match(armBody, /m_groupMoveWindow == window/);
    });

    it("reports bounded native setter and anchor outcomes without raw identities", () => {
        assert.match(effectImpl, /group-highlight:setter outcome=%1 members=%2 anchor=%3 first=%4 chord=%5 move=%6 foc=%7 ep=%8 vis=%9/);
        assert.match(effectImpl, /group-highlight:transition anchor=%1 members=%2 first=%3 chord=%4 move=%5 foc=%6 ep=%7 vis=%8/);
        for (const outcome of ["accepted", "stale", "endpoint-unavailable"]) {
            assert.match(effectImpl, new RegExp(`emitGroupSetterDiag\\("${outcome}"\\)`));
        }
        assert.match(effectImpl, /emitGroupSetterDiag\(code == 3 \? "focus-mismatch" : "parse-rejected"\)/);
        for (const reason of ["selected", "no-group", "no-member-match", "all-items-hidden"]) {
            assert.match(effectImpl, new RegExp(`"${reason}"`));
        }
        const emitter = effectImpl.split("void ActiveWindowBorderEffect::emitGroupSetterDiag")[1]?.split("void ActiveWindowBorderEffect::updateBorder")[0];
        assert.ok(emitter !== undefined);
        assert.doesNotMatch(emitter, /internalId|m_groupMemberIds\.at|payloadBytes|frameGeometry/);
    });

    it("seeds maximize observation directly from committed mode with transitions authoritative", () => {
        // Each observed window seeds m_maximizedWindows from window()->maximizeMode().
        assert.match(effectImpl, /maximizeMode\(\)/);
        assert.match(effectImpl, /activeBorderSeedMaximized/);
        assert.match(logic, /activeBorderSeedMaximized/);
        assert.doesNotMatch(logic, /activeBorderInitialGate/);
        // Any maximize axis suppresses both borders; fullscreen stays
        // suppressed independently via live isFullScreen().
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
        // Qt JSON reads only the already-Rust-accepted plain member list
        // for the lowest-stacked anchor; policy stays in Rust.
        assert.match(effectImpl, /QJsonDocument/);
        assert.match(effectImpl, /members/);
        assert.match(effectImpl, /m_groupMemberIds/);
        assert.doesNotMatch(ffi, /InitialMaximizeState/);
        assert.doesNotMatch(ffi, /initial_maximize_/);
        assert.doesNotMatch(rust, /initial_maximize_/);
        assert.doesNotMatch(rust, /InitialMaximize/);
        assert.doesNotMatch(rust, /maximize_mode/);
    });

    it("binds focus, clears on activation, and fails closed on endpoint loss", () => {
        // Accepted payload focused_window binds to the live active internalId.
        assert.match(effectImpl, /internalId/);
        assert.match(effectImpl, /WithoutBraces/);
        assert.match(effectImpl, /group_highlight_apply/);
        assert.match(effectHeader, /GroupHighlightState m_groupState/);
        // Focus activation clears immediately before async refresh.
        assert.match(effectImpl, /windowActivated/);
        assert.match(effectImpl, /clearGroupHighlight/);
        // Registration failure fails closed with no retry. Focus clears
        // synchronously (no async settle); the only singleShot in the
        // effect is the facts-waiter lease, never the highlight path.
        assert.match(effectHeader, /m_groupDbusAvailable/);
        assert.match(effectImpl, /m_groupDbusAvailable/);
        for (const body of [
            extractBody(effectImpl, "void ActiveWindowBorderEffect::applyGroupHighlight("),
            extractBody(effectImpl, "void ActiveWindowBorderEffect::clearGroupHighlight("),
        ]) {
            assert.doesNotMatch(body, /singleShot/);
        }
    });

    it("parses strict bounded JSON with identity ordering and renders only supplied union bounds", () => {
        assert.match(effectImpl, /toUtf8/);
        assert.match(effectImpl, /group_highlight_apply/);
        assert.match(effectImpl, /group_highlight_rect/);
        assert.match(effectImpl, /group_highlight_status/);
        assert.match(ffi, /GroupHighlightState/);
        assert.match(ffi, /group_highlight_apply/);
        assert.match(ffi, /GroupHighlightStatus/);
        assert.match(ffi, /group_highlight_status/);
        assert.match(rust, /fn parse_payload/);
        assert.match(rust, /fn correlation_is_newer/);
        assert.match(rust, /-16384/);
        assert.match(rust, /16384/);
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
        assert.match(cmake, /activebordergroup_test\.cpp/);
        assert.match(cmake, /native-effect-group-highlight/);
        assert.match(cmake, /presentationfacts_test\.cpp/);
        assert.match(cmake, /native-effect-facts-transport/);
        assert.match(cmake, /presentationfacts\.h/);
        assert.match(cmake, /presentationfacts\.cpp/);
        assert.match(cmake, /OMNITILER_KWIN_PAINTSCREEN_BOOL/);
        assert.match(cmake, /GROUP_HEADER/);
        assert.match(cmake, /GROUP_IMPL/);
        assert.match(cmake, /GROUP_LOGIC/);
        assert.match(cmake, /GROUP_FFI/);
        assert.match(cmake, /GROUP_RUST/);
        assert.match(cmake, /DRAG_HEADER/);
        assert.match(cmake, /DRAG_IMPL/);
        assert.match(cmake, /DRAG_FFI/);
        assert.match(cmake, /DRAG_RUST/);
        assert.match(cmake, /validate-unified-lifecycle\.cmake/);
        assert.match(validator, /GROUP_HEADER/);
        assert.match(validator, /GROUP_IMPL/);
        assert.match(validator, /GROUP_LOGIC/);
        assert.match(validator, /SetGroupHighlight/);
        assert.match(validator, /ClearGroupHighlight/);
        assert.match(validator, /LastVerdict/);
        assert.doesNotMatch(validator, /SetInitialMaximizeState/);
        assert.doesNotMatch(validator, /ClearInitialMaximizeState/);
        assert.doesNotMatch(validator, /GetInitialMaximizeEpoch/);
        assert.doesNotMatch(validator, /initial_maximize_/);
        assert.match(validator, /mouseChanged/);
        assert.match(validator, /MetaModifier/);
        assert.match(validator, /OutlinedBorderItem/);
        assert.match(validator, /ImageItem/);
        assert.match(validator, /updateGroupAnchorAndGeometry/);
        assert.match(validator, /stackingOrderChanged/);
        assert.match(validator, /group_highlight_focus_matches/);
        assert.match(validator, /group_highlight_is_visible/);
        assert.match(validator, /m_groupDbusAvailable/);
    });

    it("folds the drag oracle into the survivor with one shared lifecycle and no second plugin", () => {
        // Oracle D-Bus endpoint and LastVerdict contract survive unchanged.
        assert.match(effectImpl, /Q_CLASSINFO\("D-Bus Interface", "com\.omnitiler\.DragOracle1"\)/);
        assert.match(effectImpl, /registerService\(QStringLiteral\("com\.omnitiler\.DragOracle"\)\)/);
        assert.match(effectImpl, /registerObject\(QStringLiteral\("\/com\/omnitiler\/DragOracle"\)/);
        assert.match(effectImpl, /unregisterObject\(QStringLiteral\("\/com\/omnitiler\/DragOracle"\)\)/);
        assert.match(effectImpl, /unregisterService\(QStringLiteral\("com\.omnitiler\.DragOracle"\)\)/);
        // Copied verdict before the D-Bus return; never a borrowed view.
        assert.match(effectImpl, /drag_oracle_last_copy/);
        assert.doesNotMatch(effectImpl, /drag_oracle_last\(/);
        assert.match(effectImpl, /drag_oracle_record/);
        // Construction seed plus the group-underlay lowest-stacked anchor
        // share the lifecycle; stacking changes re-anchor the underlay.
        assert.equal(countMatches(effectImpl, /stackingOrder\(\)/g), 2);
        assert.match(effectImpl, /stackingOrderChanged/);
        assert.equal(countMatches(effectImpl, /EffectsHandler::windowAdded/g), 1);
        assert.equal(countMatches(effectImpl, /EffectsHandler::windowClosed/g), 1);
        assert.equal(countMatches(effectImpl, /EffectsHandler::windowDeleted/g), 1);
        assert.match(effectImpl, /attachOracleWindow/);
        assert.match(effectImpl, /forgetOracleWindow/);
        assert.match(effectImpl, /onOracleDragStart/);
        assert.match(effectImpl, /onOracleDragFinish/);
        assert.match(effectImpl, /windowStartUserMovedResized/);
        assert.match(effectImpl, /windowFinishUserMovedResized/);
        assert.match(effectImpl, /moveResizeGeometry/);
        assert.match(effectImpl, /oracleMoveResizeRect/);
        // The original observer was rendering-independent. The survivor must
        // attach it before the active-border-only OpenGL early return.
        const oracleSetup = effectImpl.indexOf("m_oracleDbusObject =");
        assert.ok(oracleSetup >= 0);
        assert.ok(effectImpl.indexOf("attachOracleWindow(window);", oracleSetup) < effectImpl.indexOf("m_borderItem.setZ(-1)", oracleSetup));
        // No second plugin effect or factory.
        assert.doesNotMatch(effectImpl, /DragOracleEffect/);
        assert.doesNotMatch(effectImpl, /dragoracle-metadata\.json/);
        assert.doesNotMatch(effectImpl, /omnitiler-drag-oracle/);
        assert.doesNotMatch(effectHeader, /DragOracleEffect/);
        assert.match(effectHeader, /m_oracleDbusObject/);
        assert.match(effectHeader, /m_oracleStartRects/);
        assert.match(effectHeader, /drag_oracle_ffi\.h/);
        // Rendering: one active outline plus the filled group underlay and
        // the drag preview fill.
        assert.equal(countMatches(effectHeader, /OutlinedBorderItem/g), 1);
        assert.ok(countMatches(effectHeader, /ImageItem/g) >= 2);
        // Oracle Rust FFI and pull protocol surface stay intact.
        const oracleFfi = read("native-effect/drag_oracle_ffi.h");
        const oracleRust = read("../crates/tiler-kwin-effect-ffi/src/drag_oracle.rs");
        assert.match(oracleFfi, /DragOracleRect/);
        assert.match(oracleFfi, /drag_oracle_record/);
        assert.match(oracleFfi, /drag_oracle_last_copy/);
        assert.match(oracleRust, /drag_oracle_last_copy/);
        assert.match(oracleRust, /correlation/);
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
        assert.ok(ctorEndpoint > effectImpl.indexOf("m_groupDbusAvailable = groupRegistered"));
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
