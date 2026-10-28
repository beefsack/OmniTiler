// attribution native-roster helper: read-only KWin script (no Qt global).
// Logs every window as {internalId,resourceClass,caption,pid,frame}; caption
// only for the exact fixture resourceClass, null otherwise. KWin v6.7.4 pins:
// window.h resourceClass/caption Q_PROPERTY, internalId QUuid (String() it),
// frameGeometry; core/rect.h x/y/width/height; scripting.cpp ConsoleExtension;
// workspace_wrapper windowList().
(function () {
    var NONCE = "__ATTRIBUTION_NONCE__";
    var P = "omnitiler:attribution:" + NONCE + ":";
    var UUID = /^\{[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{4}-[0-9a-fA-F]{12}\}$/;
    var FIXTURE_CLASS = "OmniTilerTestWindow";
    function fail(m) { throw new Error("attribution-helper: " + m); }
    function win(w) {
        if (!w || typeof w !== "object") fail("window must be an object");
        var rc = w.resourceClass;
        if (typeof rc !== "string" || !rc) fail("resourceClass must be a nonempty string");
        var caption = null;
        if (rc === FIXTURE_CLASS) {
            if (typeof w.caption !== "string" || !w.caption) fail("fixture caption must be a nonempty string");
            caption = w.caption;
        }
        if (typeof w.pid !== "number" || w.pid <= 0 || Math.floor(w.pid) !== w.pid) fail("pid must be a positive int");
        var g = w.frameGeometry;
        if (!g || typeof g !== "object") fail("frameGeometry must be an object");
        var fr = { x: g.x, y: g.y, w: g.width, h: g.height };
        for (var k in fr) if (typeof fr[k] !== "number" || !isFinite(fr[k])) fail("frame." + k + " must be finite");
        if (fr.w <= 0 || fr.h <= 0) fail("frame size must be positive");
        var s = String(w.internalId);
        if (!UUID.test(s)) fail("internalId must be a braced UUID");
        return { internalId: s, resourceClass: rc, caption: caption, pid: w.pid, frame: fr };
    }
    try {
        if (typeof workspace === "undefined" || !workspace) fail("workspace unavailable");
        var list = workspace.windowList(), wins = [], seen = {}, i;
        if (!list || typeof list.length !== "number") fail("windowList() must return a list");
        for (i = 0; i < list.length; i++) {
            var row = win(list[i]);
            if (seen[row.internalId]) fail("duplicate internalId " + row.internalId);
            seen[row.internalId] = true;
            wins.push(row);
        }
        if (!wins.length) fail("no windows enumerated");
        console.log(P + "ready:count=" + wins.length);
        for (i = 0; i < wins.length; i++) console.log(P + "window:" + JSON.stringify(wins[i]));
        console.log(P + "done:count=" + wins.length);
    } catch (e) {
        try { console.log(P + "error:" + ((e && e.message) || "unknown")); } catch (e2) {}
    }
})();
