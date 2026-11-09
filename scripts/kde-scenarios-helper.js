// scripts/kde-scenarios-helper.js: KWin-script template for the KDE scenario runner.
//
// Run-time substitution (plain string replace by the runner, once per call):
//   __KDE_SCENARIOS_NONCE__ -> JSON string, fresh 32-hex run nonce for this call
//   __KDE_SCENARIOS_OP__    -> JSON object {op,scope,...}, one narrowly approved op
// Ops: read | append | setCurrent | activate | remove.
// Non-module ES2017 IIFE. No Qt global, no imports, no optional catch binding.
// Emits exactly one console.log line: "omnitiler:native:<nonce>:" + JSON.
// Foreign (non-scope) windows never leave this script: only a foreign count.
(function () {
  var NONCE = __KDE_SCENARIOS_NONCE__;
  var OP = __KDE_SCENARIOS_OP__;
  var PREFIX = "omnitiler:native:" + NONCE + ":";
  var MAX_LIST = 4096;
  var GEO_BOUND = 16384;
  function fail(message) { throw new Error("native-helper: " + message); }
  function out(obj) { console.log(PREFIX + JSON.stringify(obj)); }
  function needStr(value, what) {
    if (typeof value !== "string" || !value) { fail(what + " must be a nonempty string"); }
    return value;
  }
  function needInt(value) {
    if (typeof value !== "number" || !isFinite(value)) { return null; }
    var r = Math.round(value);
    if (Math.abs(r) > 9007199254740991) { return null; }
    return r;
  }
  // QML array-likes are not JS arrays: decode by length with a hard cap.
  function decodeList(value) {
    if (Array.isArray(value)) { return value.slice(); }
    if (value === null || typeof value !== "object") { return null; }
    var len;
    try { len = value.length; } catch (e) { return null; }
    if (typeof len !== "number" || len < 0 || len > MAX_LIST || Math.floor(len) !== len) { return null; }
    var result = [];
    for (var i = 0; i < len; i++) {
      var item;
      try { item = value[i]; } catch (e) { return null; }
      if (item === undefined) { return null; }
      result.push(item);
    }
    return result;
  }
  function readRect(geom) {
    if (geom === null || typeof geom !== "object") { return null; }
    var x = needInt(geom.x);
    var y = needInt(geom.y);
    var w = needInt(geom.width !== undefined ? geom.width : geom.w);
    var h = needInt(geom.height !== undefined ? geom.height : geom.h);
    if (x === null || y === null || w === null || h === null) { return null; }
    if (w <= 0 || h <= 0) { return null; }
    if (x < -GEO_BOUND || x > GEO_BOUND || y < -GEO_BOUND || y > GEO_BOUND) { return null; }
    if (w > GEO_BOUND || h > GEO_BOUND) { return null; }
    return { x: x, y: y, w: w, h: h };
  }
  function readRawRect(geom) {
    if (geom === null || typeof geom !== "object") { return null; }
    var x = geom.x;
    var y = geom.y;
    var w = geom.width !== undefined ? geom.width : geom.w;
    var h = geom.height !== undefined ? geom.height : geom.h;
    if (typeof x !== "number" || !isFinite(x)) { return null; }
    if (typeof y !== "number" || !isFinite(y)) { return null; }
    if (typeof w !== "number" || !isFinite(w)) { return null; }
    if (typeof h !== "number" || !isFinite(h)) { return null; }
    return { x: x, y: y, w: w, h: h };
  }
  function readScale(output) {
    var s;
    try { s = output.devicePixelRatio; } catch (e) { return null; }
    if (typeof s !== "number" || !isFinite(s) || s <= 0) { return null; }
    return s;
  }
  // Pointer position from the KWin scripting API (QPoint
  // Workspace::cursorPos at the pinned revision: a QPoint-like {x, y}).
  // Integer-normalised via needInt; unreadable/missing reads null (unknown).
  function readCursor(ws) {
    var cp;
    try { cp = ws.cursorPos; } catch (e) { return null; }
    if (cp === null || typeof cp !== "object") { return null; }
    var cx, cy;
    try { cx = cp.x; cy = cp.y; } catch (e) { return null; }
    cx = needInt(cx);
    cy = needInt(cy);
    if (cx === null || cy === null) { return null; }
    return { x: cx, y: cy };
  }
  function workspaceOf() {
    if (typeof workspace === "undefined" || !workspace) { fail("workspace unavailable"); }
    return workspace;
  }
  function desktopObjs(ws) {
    var raw;
    try { raw = ws.desktops; } catch (e) { fail("desktops unreadable"); }
    var list = decodeList(raw);
    if (list === null) { fail("desktops undecodable"); }
    return list;
  }
  function desktopId(desk) {
    var id;
    try { id = desk.id; } catch (e) { fail("desktop id unreadable"); }
    return needStr(id, "desktop.id");
  }
  function findDesktop(ws, id) {
    var all = desktopObjs(ws);
    for (var i = 0; i < all.length; i++) {
      try {
        if (String(all[i].id) === id) { return all[i]; }
      } catch (e) { /* keep looking */ }
    }
    fail("desktop " + id + " not found");
    return null;
  }
  function windowObjs(ws) {
    var fn;
    try { fn = ws.windowList; } catch (e) { fail("windowList unreadable"); }
    if (typeof fn !== "function") { fail("windowList unavailable"); }
    var raw;
    try { raw = fn.call(ws); } catch (e) { fail("windowList threw"); }
    var list = decodeList(raw);
    if (list === null) { fail("windowList undecodable"); }
    return list;
  }
  try {
    if (typeof OP !== "object" || OP === null) { fail("op missing"); }
    var scope = needStr(OP.scope, "scope");
    var ws = workspaceOf();
    if (OP.op === "read") {
      var desks = desktopObjs(ws);
      var ids = [];
      for (var di = 0; di < desks.length; di++) { ids.push(desktopId(desks[di])); }
      var current = null;
      try {
        if (ws.currentDesktop) { current = desktopId(ws.currentDesktop); }
      } catch (e) { current = null; }
      if (!current) { fail("current desktop unreadable"); }
      var active = null;
      try {
        if (ws.activeWindow) { active = String(ws.activeWindow.internalId); }
      } catch (e) { active = null; }
      var wins = windowObjs(ws);
      var seen = {};
      var owned = [];
      var foreign = 0;
      for (var wi = 0; wi < wins.length; wi++) {
        var w = wins[wi];
        var uuid;
        try { uuid = String(w.internalId); } catch (e) { fail("window internalId unreadable"); }
        if (!uuid) { fail("window internalId empty"); }
        if (seen[uuid]) { fail("duplicate native UUID " + uuid); }
        seen[uuid] = 1;
        var cls = "";
        try { cls = String(w.resourceClass); } catch (e) { cls = ""; }
        var pid = -1;
        try { pid = w.pid; } catch (e) { pid = -1; }
        if (typeof pid !== "number" || Math.floor(pid) !== pid) { pid = -1; }
        var title = "";
        // KWin caption() is captionNormal + captionSuffix: report the stable
        // captionNormal first, falling back to caption only when captionNormal
        // is not a nonempty string.
        try {
          var cn = w.captionNormal;
          if (typeof cn === "string" && cn) { title = String(cn); }
          else { title = String(w.caption || w.resourceName || ""); }
        } catch (e) { title = ""; }
        if (cls === scope && pid > 0 && title) {
          var frame = null;
          var frameRaw = null;
          try { frame = readRect(w.frameGeometry); } catch (e) { frame = null; }
          try { frameRaw = readRawRect(w.frameGeometry); } catch (e) { frameRaw = null; }
          var outName = null;
          var outRect = null;
          var outScale = null;
          try {
            if (w.output) {
              outName = String(w.output.name || "");
              if (!outName) { outName = null; }
              outRect = readRect(w.output.geometry);
              outScale = readScale(w.output);
            }
          } catch (e) { outName = null; outRect = null; outScale = null; }
          var members = null;
          try {
            var mraw = decodeList(w.desktops);
            if (mraw !== null) {
              members = [];
              for (var mi = 0; mi < mraw.length; mi++) { members.push(desktopId(mraw[mi])); }
            }
          } catch (e) { members = null; }
          owned.push({ uuid: uuid, class: cls, pid: pid, title: title, frame: frame, frameRaw: frameRaw, out: outName, outRect: outRect, outScale: outScale, desks: members });
        } else {
          foreign++;
        }
      }
      out({ op: "read", desktops: ids, current: current, count: ids.length, active: active, owned: owned, foreignCount: foreign, cursor: readCursor(ws) });
    } else if (OP.op === "append") {
      var name = needStr(OP.name, "append.name");
      var before = desktopObjs(ws);
      var have = {};
      for (var bi = 0; bi < before.length; bi++) { have[desktopId(before[bi])] = 1; }
      try { ws.createDesktop(before.length, name); } catch (e) { fail("createDesktop threw"); }
      var after = desktopObjs(ws);
      var fresh = [];
      for (var ai = 0; ai < after.length; ai++) {
        var nid = desktopId(after[ai]);
        if (!have[nid]) { fresh.push(nid); }
      }
      if (after.length !== before.length + 1 || fresh.length !== 1) { fail("append uncorrelated"); }
      out({ op: "append", id: fresh[0], before: before.length, after: after.length });
    } else if (OP.op === "setCurrent") {
      var target = findDesktop(ws, needStr(OP.id, "setCurrent.id"));
      try { ws.currentDesktop = target; } catch (e) { fail("currentDesktop write threw"); }
      var now = null;
      try {
        if (ws.currentDesktop) { now = desktopId(ws.currentDesktop); }
      } catch (e) { now = null; }
      if (now !== OP.id) { fail("currentDesktop did not take effect"); }
      out({ op: "setCurrent", id: OP.id });
    } else if (OP.op === "activate") {
      var auuid = needStr(OP.uuid, "activate.uuid");
      var all = windowObjs(ws);
      var hit = null;
      for (var hi = 0; hi < all.length; hi++) {
        try {
          if (String(all[hi].internalId) === auuid) { hit = all[hi]; break; }
        } catch (e) { /* keep looking */ }
      }
      if (!hit) { fail("activate target absent"); }
      try { ws.activeWindow = hit; } catch (e) { fail("activeWindow write threw"); }
      out({ op: "activate", uuid: auuid, ok: true });
    } else if (OP.op === "remove") {
      var victim = findDesktop(ws, needStr(OP.id, "remove.id"));
      try { ws.removeDesktop(victim); } catch (e) { fail("removeDesktop threw"); }
      out({ op: "remove", id: OP.id, removed: true });
    } else {
      fail("unsupported op");
    }
  } catch (e) {
    try { out({ error: String((e && e.message) || e) }); } catch (x) { /* never throw past the boundary */ }
  }
})();
