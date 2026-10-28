// Offline mocked regression tests for omnitiler-visibility-probe main.js.
// Mocks use ONLY actual EffectWindow Q_PROPERTY names from the pinned source
// (windowClass, visible, geometry{x,y,width,height}, pid) plus the actual
// signal names. `internalId`/`frameGeometry` are deliberately NOT mocked:
// neither is script-visible (plain C++ methods, no Q_PROPERTY). Run:
// node tests/run-tests.mjs
import { readFileSync } from "node:fs";
import vm from "node:vm";
import assert from "node:assert";
import { fileURLToPath } from "node:url";
import path from "node:path";

const here = path.dirname(fileURLToPath(import.meta.url));
const pkgDir = path.join(here, "..", "kwin", "effects", "omnitiler-visibility-probe");
const src = readFileSync(path.join(pkgDir, "contents", "code", "main.js"), "utf8");

function loadProbe(mockEffects) {
  const lines = [];
  const sandbox = {
    console: { log: (m) => lines.push(String(m)) },
    JSON,
    String,
    isFinite,
  };
  if (typeof mockEffects !== "undefined") {
    sandbox.effects = mockEffects;
    sandbox.console = mockEffects.__logger || sandbox.console;
  }
  vm.createContext(sandbox);
  vm.runInContext(src, sandbox, { filename: "main.js" });
  return { P: sandbox.OmnitilerVisibilityProbe, lines };
}

function mockSignal() {
  const handlers = [];
  return {
    handlers,
    connect(h) {
      handlers.push(h);
    },
    fire(...args) {
      for (const h of handlers) h(...args);
    },
  };
}

const WINDOW_SIGNALS = [
  "minimizedChanged",
  "windowHiddenChanged",
  "windowDesktopsChanged",
  "windowFrameGeometryChanged",
];

function mockWindow(o) {
  const calls = [];
  const w = {
    __calls: calls,
    windowClass: o.windowClass,
    visible: o.visible,
    geometry: o.geometry,
    pid: o.pid,
    caption: o.caption ?? "OmniTilerTestWindow:secret-title",
    setData() {
      calls.push("setData");
    },
  };
  for (const name of WINDOW_SIGNALS) {
    w[name] = mockSignal();
  }
  return w;
}

let pass = 0;
function ok(name, fn) {
  fn();
  pass++;
  console.log("ok - " + name);
}

// 0. Package metadata matches the proven KPackage layout
{
  const meta = JSON.parse(readFileSync(path.join(pkgDir, "metadata.json"), "utf8"));
  ok("metadata KPackageStructure KWin/Effect (per bundled quick-effect example)", () => {
    assert.equal(meta.KPackageStructure, "KWin/Effect");
  });
  ok("metadata javascript API, id matches dir, never auto-loads", () => {
    assert.equal(meta["X-Plasma-API"], "javascript");
    assert.equal(meta.KPlugin.Id, "omnitiler-visibility-probe");
    assert.equal(meta.KPlugin.EnabledByDefault, false);
  });
}

// 1. resourceClass-anchored filter (last token of windowClass)
{
  const { P } = loadProbe();
  ok("filter matches resourceClass token (either case)", () => {
    assert.equal(P.isFixtureWindow({ windowClass: "OmniTilerTestWindow OmniTilerTestWindow" }), true);
    assert.equal(P.isFixtureWindow({ windowClass: "omnitilertestwindow omnitilertestwindow" }), true);
  });
  ok("filter rejects resourceName-only squat (M1)", () => {
    assert.equal(P.isFixtureWindow({ windowClass: "OmniTilerTestWindow something-else" }), false);
  });
  ok("filter rejects foreign class", () => {
    assert.equal(P.isFixtureWindow({ windowClass: "plasmashell plasmashell" }), false);
  });
  ok("filter rejects substring squat", () => {
    assert.equal(P.isFixtureWindow({ windowClass: "x FakeOmniTilerTestWindow" }), false);
  });
  ok("filter rejects absent/non-string/blank class", () => {
    assert.equal(P.isFixtureWindow({}), false);
    assert.equal(P.isFixtureWindow({ windowClass: null }), false);
    assert.equal(P.isFixtureWindow({ windowClass: 42 }), false);
    assert.equal(P.isFixtureWindow({ windowClass: "   " }), false);
    assert.equal(P.isFixtureWindow(null), false);
  });
}

// 2. visible: absent/unknown never fabricated
{
  const { P } = loadProbe();
  ok("visible true/false pass through", () => {
    assert.equal(P.visibleState({ visible: true }), true);
    assert.equal(P.visibleState({ visible: false }), false);
  });
  ok("visible absent/null/non-bool is unknown", () => {
    assert.equal(P.visibleState({}), null);
    assert.equal(P.visibleState({ visible: null }), null);
    assert.equal(P.visibleState({ visible: undefined }), null);
    assert.equal(P.visibleState({ visible: 1 }), null);
    assert.equal(P.visibleState({ visible: "true" }), null);
  });
}

// 3. Numeric geometry only (M2)
{
  const { P } = loadProbe();
  ok("geometry numerics pass through as [x,y,w,h]", () => {
    assert.deepEqual(P.geomRect({ geometry: { x: 10, y: 20, width: 800, height: 600 } }), [10, 20, 800, 600]);
  });
  ok("geometry absent/non-object/partial/non-finite is unknown (never partial)", () => {
    assert.equal(P.geomRect({}), null);
    assert.equal(P.geomRect({ geometry: null }), null);
    assert.equal(P.geomRect({ geometry: "frame(1,2,3,4)" }), null);
    assert.equal(P.geomRect({ geometry: { x: 10, y: 20, width: 800 } }), null);
    assert.equal(P.geomRect({ geometry: { x: 10, y: NaN, width: 800, height: 600 } }), null);
    assert.equal(P.geomRect({ geometry: { x: 10, y: 20, width: Infinity, height: 600 } }), null);
  });
}

// 4. Join record: vis/pid/geom/active only, no native UUID (H2)
{
  const { P } = loadProbe();
  const fx = {
    windowClass: "OmniTilerTestWindow OmniTilerTestWindow",
    visible: true,
    geometry: { x: 1, y: 2, width: 3, height: 4 },
    pid: 1234,
  };
  const other = { windowClass: "OmniTilerTestWindow OmniTilerTestWindow", visible: false };
  ok("snapshot joins vis/pid/geom/active, carries no id field", () => {
    const s = P.snapshotFixture(fx, other);
    assert.deepEqual(s, { vis: true, pid: 1234, geom: [1, 2, 3, 4], active: false });
    assert.ok(!("id" in s), "must not carry a fabricated id");
    assert.equal(P.snapshotFixture(fx, fx).active, true);
  });
  ok("snapshot active unknown when no active info", () => {
    assert.equal(P.snapshotFixture(fx, undefined).active, null);
  });
  ok("snapshot non-fixture is null", () => {
    assert.equal(P.snapshotFixture({ windowClass: "dolphin dolphin", visible: true }, null), null);
  });
  ok("snapshot absent pid/geom stay null, never fabricated", () => {
    const s = P.snapshotFixture({ windowClass: "OmniTilerTestWindow OmniTilerTestWindow", visible: false }, null);
    assert.deepEqual(s, { vis: false, pid: null, geom: null, active: false });
  });
  ok("snapshot exposes no internalId/windowId/frameGeometry readers", () => {
    assert.equal(typeof P.opaqueId, "undefined");
    assert.equal(typeof P.internalId, "undefined");
  });
}

// 5. No side effects, no foreign leaks
{
  const logs = [];
  const logger = { log: (m) => logs.push(String(m)) };
  const secret = "SECRET-TITLE-do-not-log";
  const fx1 = mockWindow({
    windowClass: "OmniTilerTestWindow OmniTilerTestWindow",
    visible: true,
    geometry: { x: 0, y: 0, width: 100, height: 100 },
    pid: 111,
    caption: secret,
  });
  const foreign = mockWindow({
    windowClass: "dolphin dolphin",
    visible: true,
    geometry: { x: 0, y: 0, width: 100, height: 100 },
    pid: 222,
    caption: "FOREIGN-" + secret,
  });
  const fxObj = { stackingOrder: [fx1, foreign], activeWindow: fx1 };
  const { P } = loadProbe();
  ok("checkpoint emits fixture only, numeric join fields", () => {
    const n = P.emitCheckpoint(fxObj, "test", logger);
    assert.equal(n, 1);
    assert.equal(logs.length, 1);
    assert.ok(logs[0].startsWith(P.PREFIX));
    const body = JSON.parse(logs[0].slice(P.PREFIX.length));
    assert.deepEqual(body, {
      vis: true,
      pid: 111,
      geom: [0, 0, 100, 100],
      active: true,
      trigger: "test",
    });
  });
  ok("no titles/captions/foreign data leak", () => {
    for (const l of logs) {
      assert.ok(!l.includes(secret), "leak: " + l);
      assert.ok(!l.includes("dolphin"), "leak: " + l);
      assert.ok(!l.includes("caption"), "leak: " + l);
    }
  });
  ok("no writes, no setData, no setters invoked", () => {
    assert.deepEqual(fx1.__calls, []);
    assert.deepEqual(foreign.__calls, []);
  });
  ok("ambiguous twins keep identical keys (no attribution by guessing)", () => {
    logs.length = 0;
    const twin = mockWindow({
      windowClass: "OmniTilerTestWindow OmniTilerTestWindow",
      visible: false,
      geometry: { x: 0, y: 0, width: 100, height: 100 },
      pid: 111,
    });
    const n = P.emitCheckpoint({ stackingOrder: [fx1, twin], activeWindow: null }, "amb", logger);
    assert.equal(n, 2);
    const a = JSON.parse(logs[0].slice(P.PREFIX.length));
    const b = JSON.parse(logs[1].slice(P.PREFIX.length));
    assert.deepEqual([a.pid, a.geom], [b.pid, b.geom]);
    // vis differs here, but with identical keys AND identical vis the consumer
    // must treat caller-id attribution as unknown and disambiguate first.
  });
}

// 6. Wiring: signals connected, load checkpoint, dedupe, geometry hook (M4)
{
  const logs = [];
  const logger = { log: (m) => logs.push(String(m)) };
  const fxObj = { __logger: logger, activeWindow: null };
  for (const name of [
    "windowAdded",
    "windowClosed",
    "windowDeleted",
    "windowActivated",
    "stackingOrderChanged",
    "desktopChanged",
    "currentActivityChanged",
  ]) {
    fxObj[name] = mockSignal();
  }
  const w1 = mockWindow({
    windowClass: "OmniTilerTestWindow OmniTilerTestWindow",
    visible: false,
    geometry: { x: 5, y: 5, width: 50, height: 50 },
    pid: 7,
  });
  fxObj.stackingOrder = [w1];
  const { P, lines } = loadProbe(fxObj);
  ok("load checkpoint emitted at init via entry point", () => {
    const all = lines.concat(logs);
    assert.equal(all.length, 1);
    assert.ok(all[0].startsWith(P.PREFIX));
    assert.ok(all[0].includes('"trigger":"load"'));
  });
  ok("per-window signals hooked once (incl geometryChanged); re-wire does not duplicate", () => {
    assert.equal(w1.minimizedChanged.handlers.length, 1);
    assert.equal(w1.windowFrameGeometryChanged.handlers.length, 1);
    const again = P.wireEffect(fxObj, logger, [w1]); // already seen+wired
    assert.deepEqual(again, []);
    assert.equal(w1.minimizedChanged.handlers.length, 1);
    assert.equal(w1.windowFrameGeometryChanged.handlers.length, 1);
    assert.equal(fxObj.stackingOrderChanged.handlers.length, 1);
  });
  ok("geometry signal fire produces fresh numeric checkpoint (M4)", () => {
    const n0 = logs.length + lines.length;
    w1.windowFrameGeometryChanged.fire(w1, { x: 0, y: 0, width: 1, height: 1 });
    const all = lines.concat(logs);
    assert.equal(all.length, n0 + 1);
    assert.ok(all[all.length - 1].includes('"trigger":"windowFrameGeometryChanged"'));
    const body = JSON.parse(all[all.length - 1].slice(P.PREFIX.length));
    assert.deepEqual(body.geom, [5, 5, 50, 50]);
  });
  ok("foreign window added hooks nothing and logs nothing", () => {
    const n0 = lines.concat(logs).length;
    const foreign = mockWindow({ windowClass: "xterm xterm", visible: true });
    fxObj.stackingOrder.push(foreign);
    fxObj.windowAdded.fire(foreign);
    assert.equal(foreign.minimizedChanged.handlers.length, 0);
    assert.equal(foreign.windowFrameGeometryChanged.handlers.length, 0);
    const afterAdd = lines.concat(logs);
    assert.equal(afterAdd.length, n0 + 1);
    assert.ok(!afterAdd[afterAdd.length - 1].includes("xterm"));
  });
  ok("absent stackingOrder/activeWindow tolerated (unknown, no throw)", () => {
    assert.equal(P.emitCheckpoint({}, "t", logger), 0);
    assert.equal(P.emitCheckpoint({ stackingOrder: [], activeWindow: null }, "t", logger), 0);
    assert.deepEqual(P.wireEffect(null, logger, []), []);
  });
}

console.log(`\nAll ${pass} mocked checks passed.`);
