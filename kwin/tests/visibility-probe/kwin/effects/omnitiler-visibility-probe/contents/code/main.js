// omnitiler-visibility-probe: test-only scripted KWin effect.
//
// Reads EffectWindow.visible (plus pid/geometry/active join fields) for
// OmniTilerTestWindow fixture windows ONLY and emits one opaque journal line
// per fixture window on load and on visibility/geometry-relevant signals.
//
// What it NEVER does:
// - no paintScreen/postPaintScreen/animation overrides (no rendering)
// - no window property writes, no setData(), no refWindow/unrefWindow
// - never logs caption/title or any foreign-window data
//
// KWin source pins (sibling clone, 1f543de908 = v6.7.4):
// - visible semantics: src/effect/effectwindow.h:241,
//   src/effect/effectwindow.cpp:193-199
//   (visible = !minimized && !hidden && onCurrentDesktop && onCurrentActivity)
// - identity actually exposed to scripts (all Q_PROPERTY on EffectWindow):
//   windowClass = resourceName + ' ' + resourceClass
//   (effectwindow.h:64, effectwindow.cpp:331-334); the resourceClass token is
//   the LAST token. Case is client-verbatim at every setResourceClass call
//   site (window.cpp:175-179, x11window.cpp:2964, xdgshellwindow.cpp:857,
//   waylandwindow.cpp:167) despite the "always lowercase" comment in
//   window.h, so the match is case-insensitive but position-strict.
//   internalId() (effectwindow.h:652) and windowId() (effectwindow.h:643)
//   are plain C++ methods with NO Q_PROPERTY, so neither is script-visible;
//   windowId() is X11-only anyway (effectwindow.cpp:321-328, 0 on Wayland).
//   There is NO native-UUID join key available to a scripted effect.
// - geometry: Q_PROPERTY(KWin::RectF geometry READ frameGeometry)
//   (effectwindow.h:50); RectF is Q_GADGET with qreal x/y/width/height
//   (core/rect.h). Numeric components only, never stringified.
//   pid: Q_PROPERTY(pid_t pid) (effectwindow.h:310).
// - JOIN KEY (no oracle): pid + numeric frame geometry + resourceClass +
//   active, correlated live-side with fixture Describe/native records only
//   at unambiguous points. If two fixture siblings share pid AND identical
//   geometry, the lines are emitted but carry identical keys: the consumer
//   must NOT attribute vis to one caller id and must first disambiguate
//   (move/resize one window). Ambiguity is preserved, never resolved by
//   guessing.
// - enumeration/signals: effects.stackingOrder + activeWindow
//   (effecthandler.h:129,146), windowAdded/windowClosed/windowDeleted/
//   windowActivated/stackingOrderChanged/desktopChanged/
//   currentActivityChanged (effecthandler.h:824-969),
//   minimizedChanged/windowHiddenChanged/windowDesktopsChanged/
//   windowFrameGeometryChanged (effectwindow.h:767,746,823,828).
// - load/unload D-Bus: Q_CLASSINFO org.kde.kwin.Effects (effecthandler.h:121),
//   object /Effects (effecthandler.cpp:138), Q_SCRIPTABLE loadEffect/
//   unloadEffect/isEffectLoaded (effecthandler.h:1062-1069).
// - package layout + metadata: GenericDataLocation +
//   "kwin/effects/<pluginId>/contents/code/main.js"
//   (ScriptedEffect::create, src/scripting/scriptedeffect.cpp);
//   "KPackageStructure": "KWin/Effect" per bundled known-good example
//   examples/quick-effect/package/metadata.json at the same pin.
// - console.* logging: QJSEngine ConsoleExtension
//   (src/scripting/scriptedeffect.cpp, init)

var OmnitilerVisibilityProbe = (function () {
    "use strict";

    var PREFIX = "omnitiler:visibility:";
    var FIXTURE_CLASS = "omnitilertestwindow"; // compared lower-cased

    var GLOBAL_SIGNALS = [
        "windowAdded",
        "windowClosed",
        "windowDeleted",
        "windowActivated",
        "stackingOrderChanged",
        "desktopChanged",
        "currentActivityChanged"
    ];

    var WINDOW_SIGNALS = [
        "minimizedChanged",
        "windowHiddenChanged",
        "windowDesktopsChanged",
        "windowFrameGeometryChanged"
    ];

    // Strict fixture filter: the resourceClass token is the LAST token of
    // windowClass ("resourceName resourceClass"). A resourceName-only match
    // (squat) or substring never matches. Case-insensitive because call sites
    // pass client-verbatim case (see header). Non-string/absent never match.
    function resourceClassOf(w) {
        if (!w) {
            return null;
        }
        var cls = w.windowClass;
        if (typeof cls === "function") {
            try {
                cls = cls.call(w);
            } catch (e) {
                return null;
            }
        }
        if (typeof cls !== "string") {
            return null;
        }
        var parts = cls.split(" ");
        for (var i = parts.length - 1; i >= 0; i--) {
            if (parts[i].length > 0) {
                return parts[i].toLowerCase();
            }
        }
        return null;
    }

    function isFixtureWindow(w) {
        return resourceClassOf(w) === FIXTURE_CLASS;
    }

    // Strict boolean only. Anything absent or non-boolean is unknown (null):
    // never fabricate true/false.
    function visibleState(w) {
        var v = w.visible;
        if (typeof v === "function") {
            try {
                v = v.call(w);
            } catch (e) {
                return null;
            }
        }
        if (v === true) {
            return true;
        }
        if (v === false) {
            return false;
        }
        return null;
    }

    function numOrNull(v) {
        if (typeof v === "function") {
            try {
                v = v.call(this);
            } catch (e) {
                return null;
            }
        }
        if (typeof v === "number" && isFinite(v)) {
            return v;
        }
        return null;
    }

    // Numeric frame geometry [x, y, width, height] from the w.geometry
    // Q_PROPERTY. Every component must be a finite number; otherwise the
    // whole rect is unknown (null). Never stringified, never partial.
    function geomRect(w) {
        var g = w.geometry;
        if (typeof g === "function") {
            try {
                g = g.call(w);
            } catch (e) {
                return null;
            }
        }
        if (!g || typeof g !== "object") {
            return null;
        }
        var vals = [g.x, g.y, g.width, g.height];
        for (var i = 0; i < vals.length; i++) {
            if (typeof vals[i] !== "number" || !isFinite(vals[i])) {
                return null;
            }
        }
        return vals;
    }

    // Join record for one fixture window. Returns null for non-fixture
    // windows. Never touches caption/title; carries no native UUID because
    // none is script-exposed (see header). Ambiguous twins keep identical
    // keys; attribution is the consumer's job at unambiguous points only.
    function snapshotFixture(w, activeWin) {
        if (!isFixtureWindow(w)) {
            return null;
        }
        var active = null;
        if (typeof activeWin !== "undefined") {
            active = (w === activeWin);
        }
        return {
            vis: visibleState(w),
            pid: numOrNull(w.pid),
            geom: geomRect(w),
            active: active
        };
    }

    // One checkpoint: read-only pass over effects.stackingOrder, one opaque
    // log line per fixture window via logger.log. Returns emitted count.
    function emitCheckpoint(fx, trigger, logger) {
        if (!fx || !logger || typeof logger.log !== "function") {
            return 0;
        }
        var order = fx.stackingOrder;
        if (typeof order === "function") {
            try {
                order = order.call(fx);
            } catch (e) {
                return 0;
            }
        }
        if (!order || typeof order.length !== "number") {
            return 0;
        }
        var hasActive = typeof fx.activeWindow !== "undefined";
        var activeWin = hasActive ? fx.activeWindow : undefined;
        if (typeof activeWin === "function") {
            try {
                activeWin = activeWin.call(fx);
            } catch (e) {
                activeWin = undefined;
            }
        }
        var n = 0;
        for (var i = 0; i < order.length; i++) {
            var s = snapshotFixture(order[i], activeWin);
            if (s) {
                s.trigger = trigger;
                logger.log(PREFIX + JSON.stringify(s));
                n++;
            }
        }
        return n;
    }

    // Connect global + per-window (fixture only) signals. seen[] avoids
    // duplicate per-window connections; wiredGlobals avoids duplicate
    // global connections on re-invocation. Returns connected global names.
    var wiredGlobals = false;
    function wireEffect(fx, logger, seen) {
        var connected = [];
        if (!fx) {
            return connected;
        }
        function checkpoint(trigger) {
            emitCheckpoint(fx, trigger, logger);
        }
        function hookWindow(w) {
            if (!isFixtureWindow(w)) {
                return;
            }
            for (var s = 0; s < seen.length; s++) {
                if (seen[s] === w) {
                    return;
                }
            }
            seen.push(w);
            for (var k = 0; k < WINDOW_SIGNALS.length; k++) {
                var sig = w[WINDOW_SIGNALS[k]];
                if (sig && typeof sig.connect === "function") {
                    try {
                        sig.connect(checkpoint.bind(null, WINDOW_SIGNALS[k]));
                    } catch (e) {
                        // Read-only probe: a failed hook changes nothing.
                    }
                }
            }
        }
        if (!wiredGlobals) {
            for (var i = 0; i < GLOBAL_SIGNALS.length; i++) {
                var g = fx[GLOBAL_SIGNALS[i]];
                if (g && typeof g.connect === "function") {
                    try {
                        g.connect(checkpoint.bind(null, GLOBAL_SIGNALS[i]));
                        connected.push(GLOBAL_SIGNALS[i]);
                    } catch (e) {
                        // Read-only probe: a failed hook changes nothing.
                    }
                }
            }
            var added = fx.windowAdded;
            if (added && typeof added.connect === "function") {
                try {
                    added.connect(function (w) {
                        hookWindow(w);
                    });
                } catch (e) {
                    // Read-only probe: ignore.
                }
            }
            wiredGlobals = true;
        }
        // Hook fixture windows already present; future ones are hooked via
        // the windowAdded handler above (cheap: seen[] guard, fixture-only).
        var order = fx.stackingOrder;
        if (typeof order === "function") {
            try {
                order = order.call(fx);
            } catch (e) {
                order = null;
            }
        }
        if (order && typeof order.length === "number") {
            for (var j = 0; j < order.length; j++) {
                hookWindow(order[j]);
            }
        }
        return connected;
    }

    return {
        PREFIX: PREFIX,
        FIXTURE_CLASS: FIXTURE_CLASS,
        GLOBAL_SIGNALS: GLOBAL_SIGNALS,
        WINDOW_SIGNALS: WINDOW_SIGNALS,
        resourceClassOf: resourceClassOf,
        isFixtureWindow: isFixtureWindow,
        visibleState: visibleState,
        geomRect: geomRect,
        snapshotFixture: snapshotFixture,
        emitCheckpoint: emitCheckpoint,
        wireEffect: wireEffect
    };
})();

// KWin entry point. In the offline test harness `effects` is undefined, so
// only the namespace above is exercised there.
if (typeof effects !== "undefined") {
    var __ovp_seen = [];
    OmnitilerVisibilityProbe.wireEffect(effects, console, __ovp_seen);
    OmnitilerVisibilityProbe.emitCheckpoint(effects, "load", console);
}
