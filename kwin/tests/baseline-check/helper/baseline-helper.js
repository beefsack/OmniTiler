// omnitiler baseline helper: read-only KWin script (no Qt global).
// Logs window/workspace records under a per-run nonce; never writes window
// properties, never renders, never touches shortcuts/effects/config/fs.
//
// KWin v6.7.4 pins (sibling clone): window.h Q_PROPERTY QUuid internalId
// (String() it: QUuid is not a JS string), frameGeometry/clientGeometry,
// output/desktops/activities; workspace_wrapper.h windowList(),
// currentDesktop/currentDesktopForScreen(), activeWindow, cursorPos,
// screens/activeScreen, activities/currentActivity; core/output.h
// name/geometry; core/rect.h x/y/width/height; virtualdesktops.h
// id/x11DesktopNumber; scripting.cpp installs ConsoleExtension console.log.
(function () {
    var NONCE = "__BASELINE_NONCE__";
    var P = "omnitiler:baseline:" + NONCE + ":";
    var UUID = /^\{[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\}$/;
    function fail(m) { throw new Error("baseline-helper: " + m); }
    function str(v, w) { if (typeof v !== "string" || !v) fail(w + " must be a nonempty string"); return v; }
    function bl(v, w) { if (typeof v !== "boolean") fail(w + " must be boolean"); return v; }
    function int(v, w) { if (typeof v !== "number" || !isFinite(v) || Math.floor(v) !== v) fail(w + " must be int"); return v; }
    function rect(g, w) {
        if (!g || typeof g !== "object") fail(w + " must be an object");
        for (var k = 0; k < 4; k++) {
            var p = ["x", "y", "width", "height"][k];
            if (typeof g[p] !== "number" || !isFinite(g[p])) fail(w + "." + p + " must be finite");
        }
        return { x: g.x, y: g.y, w: g.width, h: g.height };
    }
    function uid(v, w) { var s = String(v); if (!UUID.test(s)) fail(w + " must be a braced UUID"); return s; }
    function win(w) {
        if (!w || typeof w !== "object") fail("window must be an object");
        var o = w.output;
        if (!o || typeof o !== "object") fail("output must be an object");
        var dl = w.desktops, des = [], acts = w.activities, i;
        if (!dl || typeof dl.length !== "number") fail("desktops must be a list");
        for (i = 0; i < dl.length; i++) des.push(str(dl[i].id, "desktops[" + i + "].id"));
        if (!acts || typeof acts.length !== "number") fail("activities must be a list");
        var al = [];
        for (i = 0; i < acts.length; i++) al.push(str(acts[i], "activities[" + i + "]"));
        if (typeof w.pid !== "number" || w.pid <= 0 || Math.floor(w.pid) !== w.pid) fail("pid must be a positive int");
        return {
            internalId: uid(w.internalId, "internalId"),
            resourceClass: str(w.resourceClass, "resourceClass"), resourceName: str(w.resourceName, "resourceName"),
            pid: w.pid, active: bl(w.active, "active"), minimized: bl(w.minimized, "minimized"),
            hidden: bl(w.hidden, "hidden"), fullScreen: bl(w.fullScreen, "fullScreen"),
            maximizeMode: int(w.maximizeMode, "maximizeMode"), onAllDesktops: bl(w.onAllDesktops, "onAllDesktops"),
            keepAbove: bl(w.keepAbove, "keepAbove"), keepBelow: bl(w.keepBelow, "keepBelow"),
            managed: bl(w.managed, "managed"), deleted: bl(w.deleted, "deleted"),
            normalWindow: bl(w.normalWindow, "normalWindow"), dialog: bl(w.dialog, "dialog"),
            splash: bl(w.splash, "splash"), utility: bl(w.utility, "utility"), dock: bl(w.dock, "dock"),
            frame: rect(w.frameGeometry, "frameGeometry"), client: rect(w.clientGeometry, "clientGeometry"),
            desktops: des, activities: al,
            output: { name: str(o.name, "output.name"), geom: rect(o.geometry, "output.geometry") }
        };
    }
    try {
        if (typeof workspace === "undefined" || !workspace) fail("workspace unavailable");
        if (typeof workspace.currentDesktopForScreen !== "function") fail("currentDesktopForScreen unavailable");
        var list = workspace.windowList(), wins = [], seen = {}, i;
        if (!list || typeof list.length !== "number") fail("windowList() must return a list");
        for (i = 0; i < list.length; i++) {
            var row = win(list[i]);
            if (seen[row.internalId]) fail("duplicate internalId " + row.internalId);
            seen[row.internalId] = true;
            wins.push(row);
        }
        if (!wins.length) fail("no windows enumerated");
        var dl = workspace.desktops, desks = [], sl = workspace.screens, screens = [], per = [], s;
        if (!dl || typeof dl.length !== "number" || !dl.length) fail("desktops must be nonempty");
        for (i = 0; i < dl.length; i++) desks.push({ id: str(dl[i].id, "desktop.id"), num: int(dl[i].x11DesktopNumber, "desktop.num") });
        if (!sl || typeof sl.length !== "number" || !sl.length) fail("screens must be nonempty");
        for (s = 0; s < sl.length; s++) {
            screens.push({ name: str(sl[s].name, "screen.name"), geom: rect(sl[s].geometry, "screen.geometry") });
            var perD = workspace.currentDesktopForScreen(sl[s]);
            if (!perD || typeof perD !== "object") fail("currentDesktopForScreen must return an object");
            per.push({ output: sl[s].name, desktop: str(perD.id, "perOutput desktop.id") });
        }
        var cur = workspace.currentDesktop, aw = workspace.activeWindow, active = null, cp = workspace.cursorPos;
        if (!cur || typeof cur !== "object") fail("currentDesktop must be an object");
        if (aw !== null && typeof aw !== "undefined") active = uid(aw.internalId, "activeWindow id");
        if (!cp || typeof cp !== "object") fail("cursorPos must be an object");
        console.log(P + "ready:count=" + wins.length);
        for (i = 0; i < wins.length; i++) console.log(P + "window:" + JSON.stringify(wins[i]));
        console.log(P + "workspace:" + JSON.stringify({
            desktops: desks, currentDesktop: str(cur.id, "currentDesktop.id"), perOutput: per, activeWindow: active,
            cursorPos: { x: int(cp.x, "cursorPos.x"), y: int(cp.y, "cursorPos.y") }, screens: screens,
            activeScreen: str(workspace.activeScreen.name, "activeScreen.name"),
            activities: (function (al) { var o = [], k; if (!al || typeof al.length !== "number") fail("activities must be a list"); for (k = 0; k < al.length; k++) o.push(str(al[k], "activities")); return o; })(workspace.activities),
            currentActivity: str(workspace.currentActivity, "currentActivity")
        }));
        console.log(P + "done:count=" + wins.length);
    } catch (e) {
        try { console.log(P + "error:" + ((e && e.message) || "unknown")); } catch (e2) {}
    }
})();
