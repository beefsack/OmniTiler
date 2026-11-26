import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { describe, it } from "node:test";

const read = (path: string): string => readFileSync(path, "utf8");

const effectHeader = read("native-effect/activewindowborder.h");
const effectImpl = read("native-effect/activewindowborder.cpp");
const cmake = read("native-effect/CMakeLists.txt");

function extractBody(body: string, marker: string): string {
    const start = body.indexOf(marker);
    const end = body.indexOf("\n}\n", start);
    if (start < 0 || end <= start) {
        throw new Error(`Cannot locate native body: ${marker}`);
    }
    return body.slice(start, end + 3);
}

describe("presentation native static contract", () => {
    it("exports the presentation setter and keeps display hide separate from stream reset", () => {
        // docs/decisions/kde.md#native-effect-packaging-and-version-pin;
        // docs/spec/reference-outcomes.md:6942, Focus clear races old native paint.
        assert.match(effectImpl, /\bQ_SCRIPTABLE void SetPresentation\(const QString\s*&\s*\w+\)/);
        for (const slot of ["ClearPresentation", "HidePresentation"]) {
            assert.match(effectImpl, new RegExp(`\\bQ_SCRIPTABLE void ${slot}\\(\\)`));
        }
        const hide = extractBody(effectImpl, "void ActiveWindowBorderEffect::hidePresentation()");
        assert.doesNotMatch(hide, /\b(?:clearPresentation|resetStream)\(/);
    });

    it("registers the script-facing presentation endpoint after seeding the held facts source", () => {
        // The script-facing wire names and fresh-source integration are
        // unpinned by decisions/reference rows. presentationfacts_test.cpp
        // exercises authorization, snapshots, leasing and replacement on a
        // private bus, not this effect's registration and reconnect wiring.
        const registration = extractBody(effectImpl, "bool ActiveWindowBorderEffect::ensureGroupEndpointRegistered()");
        const registered = /\bensureDbusEndpointOn\(\s*m_groupBus,\s*QStringLiteral\("com\.omnitiler\.ActiveBorder"\),\s*QStringLiteral\("\/com\/omnitiler\/ActiveBorder"\)/.exec(registration);
        assert.ok(registered, "register the well-known name and object, not just unregister them");
        const busRegistration = extractBody(effectImpl, "bool ActiveWindowBorderEffect::ensureDbusEndpointOn(");
        assert.match(busRegistration, /\bbus\.registerService\(service\)/);
        assert.match(busRegistration, /\bbus\.registerObject\(path,\s*object,\s*QDBusConnection::ExportScriptableContents\)/);
        assert.match(effectImpl, /Q_CLASSINFO\("D-Bus Interface", "com\.omnitiler\.ActiveBorder1"\)/);
        assert.match(effectImpl, /\bQ_SCRIPTABLE void WaitPresentationFacts\(/);
        assert.match(registration, /QDBusConnection::connectToBus\(QDBusConnection::SessionBus,/);
        const beforeRegistration = registration.slice(0, registered.index);
        for (const seed of ["noteFactsStyle()", "noteFactsChord()", "m_stage.bindFacts("]) {
            assert.ok(beforeRegistration.includes(seed), `seed ${seed} before registration`);
        }
        const recovery = extractBody(effectImpl, "void ActiveWindowBorderEffect::ensureEndpointsRegistered()");
        assert.match(recovery, /!m_groupDbusAvailable \|\| !m_groupBus\.isConnected\(\)/);
    });

    it("parents the border and underlay locally at their decided stack positions", () => {
        // docs/decisions/visuals.md#group-underlay-rendering-and-stacking:
        // the border belongs to the target; the filled underlay belongs to the
        // lowest painted member and translates the core rectangle, unscaled.
        assert.match(effectImpl, /m_borderItem\.setZ\(-1\)/);
        assert.match(effectImpl, /m_borderItem\.setParentItem\(m_trackedWindow->windowItem\(\)\)/);
        const anchor = extractBody(effectImpl, "void ActiveWindowBorderEffect::updateGroupAnchorAndGeometry()");
        assert.match(anchor, /for \(EffectWindow \*candidate : effects->stackingOrder\(\)\)[\s\S]*?candidateItem->isVisible\(\)[\s\S]*?underlayMembers\.contains\(bare\)[\s\S]*?anchor = candidate;\s*break;/);
        assert.match(anchor, /m_groupItem\.setParentItem\(anchorItem\);\s*m_groupItem\.setZ\(-2\)/);
        assert.match(anchor, /m_groupItem\.setParentItem\(nullptr\)/);
        assert.doesNotMatch(effectImpl, /m_groupItem\.setParentItem\([^;]*overlayItem\(/);
        assert.match(effectImpl, /m_groupItem\.setImage\(/);
        assert.match(anchor, /m_router\.paint\.underlayOuter[\s\S]*anchorItem->mapFromScene\(outerRect\)/);
        assert.match(anchor, /\bconnect\(anchor, &EffectWindow::windowFrameGeometryChanged/);
        assert.match(effectImpl, /\bconnect\(effects, &EffectsHandler::stackingOrderChanged/);
    });

    it("replays the accepted border descriptor against live window geometry", () => {
        // docs/decisions/contract.md#anchored-border-r20: live frame, held
        // outsets/width/radius, no absolute border geometry from the wire.
        const border = extractBody(effectImpl, "void ActiveWindowBorderEffect::updateBorder()");
        assert.match(border, /borderGeometryFromLiveFrame\(host\.trackedFrame,\s*m_router\.paint\.borderOutsetLeft,[\s\S]*?m_router\.paint\.borderWidth,/);
        assert.match(border, /ringOutlineForWidth\([^;]*m_router\.paint\.borderWidth[^;]*m_router\.paint\.borderRadius\)/);
        assert.match(border, /m_borderItem\.setInnerRect\([^;]*->mapFromScene\(innerRect\)/);
    });

    it("routes native focus, frame and structural invalidations into presentation", () => {
        // docs/decisions/world-rendering.md#kde-renderer-freshness:
        // native publications must refresh/hide without waiting for new paint.
        // KWin v6.8.1 src/effect/effectwindow.h:746,767,806,823.
        for (const [signal, slot, stage] of [
            ["windowFrameGeometryChanged", "onTrackedFrameChanged", "onTrackedFrame"],
            ["windowFrameGeometryChanged", "onGroupMemberFrameChanged", "onMemberFrame"],
            ["minimizedChanged", "onGroupMemberVisibilityChanged", "onVisibilityChanged"],
            ["windowFullScreenChanged", "onTrackedFullscreenChanged", "onFullscreenChanged"],
            ["windowFullScreenChanged", "onGroupMemberFullscreenChanged", "onFullscreenChanged"],
            ["windowDesktopsChanged", "onGroupMemberDesktopsChanged", "onLayoutChanged"],
        ]) {
            assert.match(effectImpl, new RegExp(`\\bconnect\\(\\w+,\\s*&EffectWindow::${signal},\\s*this,\\s*&ActiveWindowBorderEffect::${slot}\\)`));
            const callback = extractBody(effectImpl, `void ActiveWindowBorderEffect::${slot}(`);
            assert.match(callback, new RegExp(`\\bm_stage\\.${stage}\\([\\s\\S]*\\bapplyStageRefresh\\(`));
        }
        for (const signal of ["screenAdded", "screenRemoved", "desktopAdded", "desktopRemoved", "desktopChanged",
            "showingDesktopChanged", "virtualScreenGeometryChanged"]) {
            assert.match(effectImpl, new RegExp(`\\bconnect\\(effects,\\s*&EffectsHandler::${signal}\\b[^;]+onWorkspaceStructureChanged\\(`));
        }
        // docs/spec/reference-outcomes.md:6942: activation hides display
        // immediately without replacing the stream or discarding order.
        assert.match(effectImpl, /\bconnect\(effects,\s*&EffectsHandler::windowActivated[\s\S]*?\n\s*onWindowActivated\(\);\s*\}\);/);
        const activation = extractBody(effectImpl, "void ActiveWindowBorderEffect::onWindowActivated()");
        assert.match(activation, /\bm_stage\.onActivation\(\)[\s\S]*\bapplyStageRefresh\(/);
        assert.doesNotMatch(activation, /\b(?:clearPresentation|resetStream)\(/);
    });

    it("observes Meta+Shift passively and publishes the level as a fact", () => {
        // docs/decisions/visuals.md#group-underlay-style-and-trigger;
        // docs/decisions/engine.md#shared-presentation-and-cursor-authority.
        // KWin v6.8.1 src/effect/effecthandler.cpp:207–235 delivers
        // mouseChanged on input and modifier changes, without polling.
        assert.match(effectImpl, /\bconnect\(effects,\s*&EffectsHandler::mouseChanged/);
        const chord = extractBody(effectImpl, "void ActiveWindowBorderEffect::onMouseChanged(");
        assert.match(chord, /\btestFlag\(Qt::MetaModifier\)\s*&&\s*modifiers\.testFlag\(Qt::ShiftModifier\)/);
        assert.match(chord, /\bnoteFactsChord\(\)/);
    });

    it("uses the live producing gesture for the core preview and invalidates it at gesture boundaries", () => {
        // docs/decisions/world-rendering.md#active-drag-preview-continuity
        // and reference-outcomes.md:6936,6941: use live producer identity,
        // not neighbor identity, and keep same-gesture motion continuity.
        const host = extractBody(effectImpl, "Stage5HostView ActiveWindowBorderEffect::currentHostView(");
        assert.match(host, /host\.liveGestureInstance\s*=\s*m_gestureSource->instance\(\)/);
        assert.match(host, /host\.liveGestureSerial\s*=\s*m_gestureSource->openSerial\(\)/);
        // KWin v6.8.1 src/window.h:1346 supplies the move-only native flag.
        const motion = extractBody(effectImpl, "bool ActiveWindowBorderEffect::isGenuineMoverFrame(");
        assert.match(motion, /inner->isInteractiveMove\(\)/);
        assert.match(effectImpl, /m_dragPreviewItem\.setParentItem\(effects->scene\(\)->overlayItem\(\)\)/);
        const preview = extractBody(effectImpl, "void ActiveWindowBorderEffect::updateDragPreview()");
        assert.match(preview, /m_stage\.decide\(currentHostView\(\)\)\.preview[\s\S]*m_router\.paint\.previewRect/);
        for (const marker of ["onGestureDragStart(", "onGestureDragFinish(", "forgetGestureWindow("]) {
            const route = extractBody(effectImpl, `void ActiveWindowBorderEffect::${marker}`);
            assert.match(route, /\bapplyStageRefresh\(m_stage\.onGestureBoundary\(/);
        }
    });

    it("does not source raw native IDs or payloads for presentation diagnostics", () => {
        // docs/decisions/observability.md#cross-component-diagnostics.
        for (const marker of ["emitPresentationSetterDiag(", "emitPresentationTransitionDiag("]) {
            const emitter = extractBody(effectImpl, `void ActiveWindowBorderEffect::${marker}`);
            assert.doesNotMatch(emitter, /\b(?:internalId|payloadBytes|activeBytes)\b/);
        }
    });

    it("seeds the maximize hide veto from committed state and observes native transitions", () => {
        // docs/decisions/kde.md#kde-fullscreen-presentation-and-host-integration;
        // KWin v6.8.1 src/window.h:1271 and src/effect/effectwindow.h:723,739.
        const seed = extractBody(effectImpl, "void ActiveWindowBorderEffect::subscribeMaximize(");
        assert.match(seed, /inner->maximizeMode\(\)\s*!=\s*0/);
        for (const signal of ["windowMaximizedStateAboutToChange", "windowMaximizedStateChanged"]) {
            assert.match(seed, new RegExp(`\\bconnect\\(window,\\s*&EffectWindow::${signal}\\b`));
        }
    });

    it("binds the accepted foreground to the live window identity and fails closed on endpoint loss", () => {
        // docs/decisions/contract.md#anchored-border-r20; docs/decisions/kde.md
        // #native-effect-packaging-and-version-pin. KWin v6.8.1
        // src/window.h:1116–1122 exposes the window lifetime ID.
        const foreground = extractBody(effectImpl, "bool ActiveWindowBorderEffect::foregroundMatchesNow()");
        assert.match(foreground, /effects->activeWindow\(\)[\s\S]*?->internalId\(\)\.toString\(QUuid::WithoutBraces\)/);
        assert.match(foreground, /==\s*m_router\.paint\.foreground/);
        const setter = extractBody(effectImpl, "void ActiveWindowBorderEffect::applyPresentation(");
        assert.match(setter, /if\s*\(\s*!m_groupDbusAvailable\s*\)\s*\{\s*?(?:\/\/[^\n]*\n\s*)*hidePresentation\(/);
    });

    it("uses passive input and event-driven painting", () => {
        // docs/decisions/visuals.md#group-underlay-style-and-trigger and
        // #group-underlay-rendering-and-stacking: no polling, timed paint,
        // input takeover, or custom scene renderer. KWin v6.8.1
        // src/effect/effecthandler.h:178–208 describes the paint and grab APIs.
        // The passive Esc delivery fence is not an input grab:
        // docs/spec/reference-outcomes.md#kde-cross-output-drag-native-scope-discriminator.
        const forbidden = /\b(?:startMousePolling|grabKeyboard|startMouseInterception|(?:pre|post)PaintScreen|prePaintWindow|paintWindow|drawWindow|startTimer|singleShot)\s*\(|\bQTimer\s*[*&\w]|\bnew\s+QTimer\b/;
        assert.doesNotMatch(effectImpl, forbidden);
        assert.doesNotMatch(effectHeader, forbidden);
    });

    it("passes the paint result through on KWin 6.8 and keeps the earlier void override", () => {
        // KWin src/effect/effect.h: v6.6.0:611 and v6.7.4:709 return void;
        // v6.8.0 and v6.8.1:709–712 return bool and require failure to
        // propagate. src/effect/effecthandler.h:179 has the matching
        // v6.8.1 handler signature. The API macro is 0.237 on both sides
        // (effect.h:110–113), so the build selects by package version.
        const gate = cmake.match(/if\(KWin_VERSION VERSION_GREATER_EQUAL "6\.8"\)\s+target_compile_definitions\(omnitiler-active-border PRIVATE (\w+)\)/)?.[1];
        assert.ok(gate, "select the bool paintScreen ABI starting at KWin 6.8");
        assert.match(effectHeader, new RegExp(
            `#ifdef ${gate}\\s+(?:\\[\\[nodiscard\\]\\]\\s+)?bool paintScreen\\([^;]+\\) override;\\s+#else\\s+void paintScreen\\([^;]+\\) override;`,
        ));
        const boolPaint = extractBody(effectImpl, "bool ActiveWindowBorderEffect::paintScreen(");
        assert.match(boolPaint, /\breturn\s+effects->paintScreen\(/);
    });

    it("hooks native start, step and finish to the one effect-owned gesture lifecycle", () => {
        // docs/decisions/engine.md#gesture-lifecycle-and-native-transport;
        // KWin v6.8.1 src/effect/effectwindow.h:674–710 declares the three
        // signals; src/window.h:816 exposes native move/resize geometry.
        // The exact script-facing service/path names are unpinned.
        const registration = extractBody(effectImpl, "bool ActiveWindowBorderEffect::ensureGestureEndpointRegistered()");
        assert.match(registration, /\bensureDbusEndpointOn\(\s*m_groupBus,\s*QStringLiteral\("com\.omnitiler\.Gesture"\),\s*QStringLiteral\("\/com\/omnitiler\/Gesture"\),\s*m_gestureSource\b/);
        const hookup = extractBody(effectImpl, "void ActiveWindowBorderEffect::attachGestureWindow(");
        for (const signal of ["windowStartUserMovedResized", "windowStepUserMovedResized", "windowFinishUserMovedResized"]) {
            assert.match(hookup, new RegExp(`\\bconnect\\(window,\\s*&EffectWindow::${signal}\\b`));
        }
        assert.match(effectImpl, /->moveResizeGeometry\(\)/);
    });
});
describe("active-border visibility diagnostics", () => {
    // docs/decisions/observability.md#cross-component-diagnostics:
    // native window IDs and raw payloads are redacted; normal diagnostics
    // must not add frame or pointer-step noise.
    it("does not source raw native IDs or payloads for active-border diagnostics", () => {
        for (const marker of [
            "void ActiveWindowBorderEffect::subscribeMaximize(",
            "void ActiveWindowBorderEffect::emitActiveBorderEndpoint(",
            "void ActiveWindowBorderEffect::emitActiveBorderVisible(",
        ]) {
            assert.doesNotMatch(extractBody(effectImpl, marker), /\b(?:internalId|payloadBytes|activeBytes)\b/);
        }
    });

    it("does not log active-border diagnostics from paint or pointer callbacks", () => {
        for (const marker of [
            "bool ActiveWindowBorderEffect::paintScreen(",
            "void ActiveWindowBorderEffect::paintScreen(",
            "void ActiveWindowBorderEffect::onMouseChanged(",
        ]) {
            assert.doesNotMatch(extractBody(effectImpl, marker), /\b(?:logActiveBorderDiag|emitActiveBorder\w*|qCInfo)\b|omnitiler:active-border:/);
        }
    });
});
