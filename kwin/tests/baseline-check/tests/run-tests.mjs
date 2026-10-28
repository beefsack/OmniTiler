// Hermetic offline tests: every external call/read is stubbed through the
// injected io; the only real fs use is temp-dir fixtures plus the actual
// helper template. Run: node --test tests/run-tests.mjs (from baseline-check/).
import { test } from "node:test";
import assert from "node:assert/strict";
import { existsSync, mkdtempSync, readFileSync, rmSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import vm from "node:vm";
import { fileURLToPath } from "node:url";
import { AuditError, captureDir, childNames, compareDirs, validateDir } from "../baseline.mjs";
const NONCE = "ab".repeat(16), PLUGIN = `omnitiler-audit-${NONCE}`;
const UUID1 = "{11111111-1111-1111-1111-111111111111}";
const THIS = path.dirname(fileURLToPath(import.meta.url));
const HELPER_SRC = readFileSync(path.join(THIS, "..", "helper", "baseline-helper.js"), "utf8");
const J = (v) => JSON.stringify(v);
const enoent = (message = "ENOENT") => Object.assign(new Error(message), { code: "ENOENT" });
const eacces = (message = "EACCES") => Object.assign(new Error(message), { code: "EACCES" });
const win = (over = {}) => ({ internalId: UUID1, resourceClass: "rc", resourceName: "rn", pid: 42, active: false, minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false, keepAbove: false, keepBelow: false, managed: true, deleted: false, normalWindow: true, dialog: false, splash: false, utility: false, dock: false, frame: { x: 0, y: 0, w: 10, h: 10 }, client: { x: 0, y: 0, w: 10, h: 10 }, desktops: ["d1"], activities: [], output: { name: "HDMI-1", geom: { x: 0, y: 0, w: 10, h: 10 } }, ...over });
const ws = (over = {}) => ({ desktops: [{ id: "d1", num: 1 }], currentDesktop: "d1", perOutput: [{ output: "HDMI-1", desktop: "d1" }], activeWindow: null, cursorPos: { x: 1, y: 2 }, screens: [{ name: "HDMI-1", geom: { x: 0, y: 0, w: 10, h: 10 } }], activeScreen: "HDMI-1", activities: [], currentActivity: "act", ...over });
const comps20 = () => ["/component/kwin", ...Array.from({ length: 19 }, (_, i) => `/component/app${i}`)];
const infoOk = () => ({ type: "a(ssssssaiai)", data: [[["a", "b", "c", "d", "e", "f", [1], [2]]]] });
const jl = (nonce, m) => J({ MESSAGE: `js: omnitiler:baseline:${nonce}:${m}` });
const journalOk = (w = win(), s = ws()) => [jl(NONCE, "ready:count=1"), jl(NONCE, `window:${J(w)}`), jl(NONCE, `workspace:${J(s)}`), jl(NONCE, "done:count=1")].join("\n") + "\n";
const kids = (names) => `<node>${names.map((n) => `<node name="${n}"/>`).join("")}</node>`;
function fake(o = {}) {
  const calls = [];
  const ok = (v) => ({ status: 0, stdout: typeof v === "string" ? v : J(v), stderr: "" });
  let li = 0, ii = 0, ownerCalls = 0, procReads = 0;
  const loads = o.loads ?? [false, true, false], intros = o.intros ?? [kids(["Script0"]), kids(["Script0", "Script3"]), kids(["Script0"])];
  const files = new Map(Object.entries(o.files ?? {}));
  const stat = (kind, p) => {
    const k = `${kind}:${p}`;
    if (files.has(k)) {
      const v = files.get(k);
      if (v instanceof Error) throw v;
      return v;
    }
    if (kind === "lstat" && o.dirs && p in o.dirs) { // project dir presence via dirs map
      const v = o.dirs[p];
      if (v instanceof Error) throw v;
      return { mode: 0o755, size: 0, isFile: false, isDir: true, isSymlink: false };
    }
    throw new Error(`unstubbed ${kind}: ${p}`);
  };
  const exec = (argv) => {
    calls.push(argv);
    const s = argv.join(" ");
    if (o.throwSub && s.includes(o.throwSub)) throw Object.assign(new Error("injected exec throw"), { status: o.throwStatus ?? 1, stdout: "", stderr: "injected boom" });
    if (s.includes("GetNameOwner")) {
      assert.deepEqual(argv.slice(0, 8), ["busctl", "--user", "--json=short", "call", "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetNameOwner"]);
      const n = ownerCalls++;
      if (o.ownerAfter !== undefined && n >= (o.driftAt ?? 1)) return ok({ type: "s", data: [o.ownerAfter] });
      if (o.ownerBad !== undefined && n >= (o.ownerBadAt ?? 1)) return ok(o.ownerBad);
      return ok({ type: "s", data: [o.owner ?? ":1.7"] });
    }
    if (s.includes("GetConnectionUnixProcessID")) return ok({ type: "u", data: [o.pid ?? 7] });
    if (s.includes("org.freedesktop.DBus.Properties")) {
      assert.equal(argv[7], "Get");
      if (s.includes("org.kde.kwin.Effects")) {
        const prop = argv[argv.length - 1];
        assert.deepEqual(argv, ["busctl", "--user", "--json=short", "call", "org.kde.KWin", "/Effects", "org.freedesktop.DBus.Properties", "Get", "ss", "org.kde.kwin.Effects", prop]);
        if (prop === "loadedEffects") return ok(o.effLoaded ?? { type: "v", data: [{ type: "as", data: ["blur"] }] });
        return ok(o.effActive ?? { type: "v", data: [{ type: "as", data: [] }] });
      }
      if (s.includes("VirtualDesktopManager")) {
        assert.deepEqual(argv, ["busctl", "--user", "--json=short", "call", "org.kde.KWin", "/VirtualDesktopManager", "org.freedesktop.DBus.Properties", "Get", "ss", "org.kde.KWin.VirtualDesktopManager", "desktops"]);
        return ok(o.desktops ?? { type: "v", data: [{ type: "a(uss)", data: [[0, "d1", "Desk 1"]] }] });
      }
      throw new Error(`unstubbed Properties.Get: ${s}`);
    }
    if (s.includes("allComponents")) {
      assert.deepEqual(argv, ["busctl", "--user", "--json=short", "call", "org.kde.kglobalaccel", "/kglobalaccel", "org.kde.KGlobalAccel", "allComponents"]);
      return ok(o.comps ?? { type: "ao", data: [comps20()] });
    }
    if (s.includes("allShortcutInfos")) {
      assert.deepEqual(argv.slice(0, 5), ["busctl", "--user", "--json=short", "call", "org.kde.kglobalaccel"]);
      assert.ok(comps20().includes(argv[5]));
      assert.equal(argv[6], "org.kde.kglobalaccel.Component");
      assert.equal(argv[argv.length - 2], "s"); assert.equal(argv[argv.length - 1], "default");
      if ((o.missingInfo ?? []).includes(argv[5])) return { status: 1, stdout: "", stderr: "unknown component" };
      return ok(infoOk());
    }
    if (s.includes("Introspect")) {
      assert.deepEqual(argv, ["busctl", "--user", "--json=short", "call", o.owner ?? ":1.7", "/Scripting", "org.freedesktop.DBus.Introspectable", "Introspect"]);
      if (o.introspectThrowAt !== undefined && ii === o.introspectThrowAt) { ii++; throw Object.assign(new Error("injected exec throw"), { status: 1, stdout: "", stderr: "injected boom" }); }
      return ok({ type: "s", data: [intros[Math.min(ii++, intros.length - 1)] ?? o.introBad ?? kids([])] });
    }
    if (s.includes("isScriptLoaded")) {
      assert.deepEqual(argv, ["busctl", "--user", "--json=short", "call", o.owner ?? ":1.7", "/Scripting", "org.kde.kwin.Scripting", "isScriptLoaded", "s", PLUGIN]);
      return ok({ type: "b", data: [loads[Math.min(li++, loads.length - 1)]] });
    }
    if (s.includes("unloadScript")) {
      assert.deepEqual(argv, ["busctl", "--user", "--json=short", "call", o.owner ?? ":1.7", "/Scripting", "org.kde.kwin.Scripting", "unloadScript", "s", PLUGIN]);
      if (o.unloadStatus) return { status: o.unloadStatus, stdout: "", stderr: "unload boom" };
      return ok(o.unload ?? { type: "b", data: [true] });
    }
    if (s.includes("loadScript")) {
      assert.deepEqual(argv.slice(0, 9), ["busctl", "--user", "--json=short", "call", o.owner ?? ":1.7", "/Scripting", "org.kde.kwin.Scripting", "loadScript", "ss"]);
      assert.ok(argv[9].endsWith("helper.source.js")); assert.equal(argv[10], PLUGIN);
      if (o.loadStatus !== undefined) return { status: o.loadStatus, stdout: "", stderr: "load boom" };
      if (o.loadRaw !== undefined) return { status: 0, stdout: o.loadRaw, stderr: "" };
      return ok(o.load ?? { type: "i", data: [3] });
    }
    if (s.includes("org.kde.kwin.Script") && argv[argv.length - 1] === "run") {
      assert.deepEqual(argv, ["busctl", "--user", "call", o.owner ?? ":1.7", "/Scripting/Script3", "org.kde.kwin.Script", "run"]);
      return { status: o.runStatus ?? 0, stdout: "", stderr: o.runStatus ? "run boom" : "" };
    }
    if (argv[0] === "journalctl" && s.includes("--show-cursor")) return ok("x\n-- cursor: cur1\n");
    if (s.includes("--after-cursor")) {
      assert.ok(argv.includes("-o") && argv.includes("json") && argv.includes(`_PID=${o.pid ?? 7}`));
      if (o.journalStatus) return { status: o.journalStatus, stdout: "", stderr: "journal boom" };
      return ok(o.journal ?? journalOk());
    }
    throw new Error(`unstubbed exec: ${s}`);
  };
  return {
    calls,
    runner: {
      env: o.env ?? { XDG_CONFIG_HOME: "/cfg", HOME: "/home/u", XDG_RUNTIME_DIR: "/run" },
      exec,
      readFile: (p) => {
        if (o.helperPath) { if (p === o.helperPath) return readFileSync(p); }
        else if (path.basename(p) === "helper.source.js") return readFileSync(p); // real temp-fixture write from CLI
        if (p.startsWith("/proc/") && p.endsWith("/stat")) {
          const n = procReads++;
          if (o.tickAfter !== undefined && n >= (o.tickDriftAt ?? 1)) return Buffer.from(PROC(o.tickAfter));
        }
        if (files.has(p)) {
          const v = files.get(p);
          if (v instanceof Error) throw v;
          return v;
        }
        throw new Error(`unstubbed read: ${p}`);
      },
      lstat: (p) => stat("lstat", p), stat: (p) => stat("stat", p),
      listDir: (p) => {
        if (o.dirs && p in o.dirs) {
          const v = o.dirs[p];
          if (v instanceof Error) throw v;
          return v;
        }
        throw new Error(`unstubbed listDir: ${p}`);
      },
    },
  };
}
const PROC = (tick = "12345") => `7 (kwin_wayland) S ${[...Array(18).fill("1"), tick, "0"].join(" ")}`;
const baseFiles = (extra = {}, pid = 7, tick = "12345") => ({
  [`/proc/${pid}/stat`]: Buffer.from(PROC(tick)),
  "lstat:/cfg/kwinrc": { mode: 0o644, size: 3, isFile: true, isSymlink: false },
  "stat:/cfg/kwinrc": { mode: 0o644, size: 3, isFile: true, isSymlink: false },
  "/cfg/kwinrc": Buffer.from("abc"),
  "lstat:/run/omnitiler/float-intent.json": enoent(),
  ...extra,
});
const cap = (tag, o = {}) => {
  const dir = path.join(mkdtempSync(path.join(os.tmpdir(), `bl-${tag}-`)), "cap");
  const f = fake({ ...o, helperPath: path.join(dir, "helper.source.js") });
  captureDir(dir, f.runner, { nonce: NONCE });
  return { dir, f };
};
const capThrows = (tag, o = {}) => {
  const tmp = path.join(mkdtempSync(path.join(os.tmpdir(), `bl-${tag}-`)), "cap");
  const f = fake({ ...o, helperPath: path.join(tmp, "helper.source.js") });
  assert.throws(() => captureDir(tmp, f.runner, { nonce: NONCE }), AuditError);
  assert.ok(!existsSync(path.join(tmp, "manifest.json")), "no complete manifest on failure");
  assert.ok(existsSync(path.join(tmp, "failure.json")), "failure record saved outside repo");
  assert.ok(existsSync(path.join(tmp, "commands.jsonl")), "raw command log kept on failure");
  return { tmp, f };
};
const unloads = (f) => f.calls.filter((c) => Array.isArray(c) && c.join(" ").includes("unloadScript")).length;
const runs = (f) => f.calls.filter((c) => Array.isArray(c) && c[c.length - 1] === "run").length;
// Every post-load failure must unload the unique plugin NAME exactly once:
// never a numeric Script<ID> target, never a retry, run only the returned id.
const assertSingleNamedUnload = (f, tag) => {
  assert.equal(unloads(f), 1, `${tag}: exactly one unload-by-name`);
  for (const c of f.calls) {
    if (!Array.isArray(c)) continue;
    if (c.join(" ").includes("unloadScript")) assert.equal(c[c.length - 1], PLUGIN, `${tag}: unload by name only`);
    assert.ok(!c.some((x, i) => i < c.length - 1 && /^Script\d+$/.test(String(x))), `${tag}: never numeric unload arg`);
  }
  for (const c of f.calls) {
    if (Array.isArray(c) && c[c.length - 1] === "run") assert.ok(c.includes("/Scripting/Script3"), `${tag}: run only returned Script3`);
  }
  assert.ok(!f.calls.some((c) => Array.isArray(c) && c.includes("start") && c.join(" ").includes("Scripting")), `${tag}: never Scripting.start`);
};
const assertRawKept = (tmp, tag) => {
  const cmds = readFileSync(path.join(tmp, "commands.jsonl"), "utf8");
  assert.ok(cmds.includes("loadScript"), `${tag}: raw load receipt kept`);
  assert.ok(cmds.includes("unloadScript"), `${tag}: raw unload receipt kept`);
};
const assertOwnerTargeted = (f, tag, owner = ":1.7") => {
  let scriptingCalls = 0;
  for (const c of f.calls) {
    if (!Array.isArray(c)) continue;
    const ci = c.indexOf("call");
    if (ci < 0) continue;
    const dest = c[ci + 1], obj = c[ci + 2];
    if (typeof obj === "string" && obj.startsWith("/Scripting")) {
      scriptingCalls++;
      assert.equal(dest, owner, `${tag}: /Scripting call targets original owner ${owner}, got ${dest}`);
    }
  }
  assert.ok(scriptingCalls > 0, `${tag}: expected owner-targeted /Scripting calls`);
};
const cleanup = (d) => rmSync(path.dirname(d), { recursive: true, force: true });
test("success: complete manifest, exact argv, identical compare matches", () => {
  const { dir, f } = cap("ok", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() } });
  const m = validateDir(dir).manifest;
  assert.equal(m.complete, true); assert.equal(m.plugin, PLUGIN);
  assert.equal(m.counts.components, 20); assert.equal(m.counts.infos, 20);
  assert.equal(JSON.parse(readFileSync(path.join(dir, "kwinrc.json"), "utf8")).mode, "0644");
  const eff = f.calls.find((c) => Array.isArray(c) && c.join(" ").includes("loadedEffects"));
  assert.deepEqual(eff, ["busctl", "--user", "--json=short", "call", "org.kde.KWin", "/Effects", "org.freedesktop.DBus.Properties", "Get", "ss", "org.kde.kwin.Effects", "loadedEffects"]);
  assert.ok(f.calls.some((c) => Array.isArray(c) && c.includes("allShortcutInfos") && c[c.length - 2] === "s" && c[c.length - 1] === "default"));
  assert.ok(!f.calls.some((c) => Array.isArray(c) && c.includes("start")), "never Scripting.start");
  assert.equal(unloads(f), 1); assert.equal(runs(f), 1);
  assertOwnerTargeted(f, "ok");
  assertSingleNamedUnload(f, "ok");
  const again = cap("ok2", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() } });
  const { lines, mismatch } = compareDirs(dir, again.dir);
  assert.equal(mismatch, 0);
  assert.ok(lines.some((l) => l.startsWith("MATCH kwin identity")));
  assert.ok(lines.some((l) => l.startsWith("MATCH window ")));
  cleanup(dir); cleanup(again.dir);
});
test("setuid/sticky bits preserved in recorded mode", () => {
  const { dir } = cap("mode441", { files: baseFiles({ "lstat:/cfg/kwinrc": { mode: 0o4644, size: 3, isFile: true, isSymlink: false }, "stat:/cfg/kwinrc": { mode: 0o4644, size: 3, isFile: true, isSymlink: false } }), dirs: { "/cfg/omnitiler": enoent() } });
  assert.equal(JSON.parse(readFileSync(path.join(dir, "kwinrc.json"), "utf8")).mode, "4644");
  cleanup(dir);
});
test("error-as-data fails; method-shaped envelope fails; no manifest", () => {
  capThrows("err", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, effLoaded: "Call failed: no such property" });
  capThrows("mshape", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, effLoaded: { type: "s", data: ["x"] } });
  const { f } = capThrows("mshape2", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, effLoaded: { type: "v", data: [{ type: "s", data: ["x"] }] } });
  assert.equal(unloads(f), 0, "fails before load: no unload owed");
});
test("missing shortcut component body fails before load", () => {
  const { f } = capThrows("mi", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, missingInfo: ["/component/app0"] });
  assert.equal(unloads(f), 0); assert.equal(runs(f), 0);
});
test("malformed mode via actual stat fails (not hash mutation)", () => {
  capThrows("badmode", { files: baseFiles({ "stat:/cfg/kwinrc": { mode: 0o100644, size: 3, isFile: true, isSymlink: false } }), dirs: { "/cfg/omnitiler": enoent() } });
});
test("store symlink refused; EACCES (even mentioning ENOENT) is not absence", () => {
  capThrows("sym", { files: baseFiles({ "lstat:/run/omnitiler/float-intent.json": { mode: 0o600, size: 1, isFile: false, isSymlink: true } }), dirs: { "/cfg/omnitiler": enoent() } });
  const { f } = capThrows("acc", { files: baseFiles({ "lstat:/run/omnitiler/float-intent.json": { mode: 0o600, size: 5, isFile: true, isSymlink: false }, "stat:/run/omnitiler/float-intent.json": eacces("ENOENT-looking EACCES") }), dirs: { "/cfg/omnitiler": enoent() } });
  assert.equal(unloads(f), 0, "config failure precedes lifecycle");
});
test("missing runtime dir fails (not absence); project dir symlink refused", () => {
  capThrows("nortd", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, env: { XDG_CONFIG_HOME: "/cfg", HOME: "/home/u" } });
  capThrows("pdsym", { files: baseFiles({ "lstat:/cfg/omnitiler": { mode: 0o755, size: 0, isFile: false, isDir: false, isSymlink: true } }), dirs: {} });
});
test("name already loaded: no load, no unload", () => {
  const { f } = capThrows("nl", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, loads: [true] });
  assert.equal(f.calls.filter((c) => Array.isArray(c) && c.join(" ").includes("loadScript")).length, 0);
  assert.equal(unloads(f), 0);
});
test("load -1 and malformed load fail closed: unload once, never run", () => {
  for (const [tag, o] of [["m1", { load: { type: "i", data: [-1] } }], ["m2", { loadRaw: "garbage" }]]) {
    const { tmp, f } = capThrows(tag, { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, loads: [false, true, false], intros: [kids(["Script0"]), kids(["Script0"])], ...o });
    assert.equal(runs(f), 0, `${tag}: never run`);
    assertSingleNamedUnload(f, tag);
    assertOwnerTargeted(f, tag);
    assertRawKept(tmp, tag);
  }
});
test("preexisting Script<ID>: no run, unload NAME once, lifecycle failure", () => {
  const { tmp, f } = capThrows("pc", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, loads: [false, false], intros: [kids(["Script0", "Script3"]), kids(["Script0", "Script3"]), kids(["Script0", "Script3"])] });
  assert.equal(runs(f), 0);
  assertSingleNamedUnload(f, "pc");
  assertOwnerTargeted(f, "pc");
  assertRawKept(tmp, "pc");
});
test("name false before run: no run, unload once; malformed introspect XML fails", () => {
  const { tmp, f } = capThrows("nr", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, loads: [false, false, false] });
  assert.equal(runs(f), 0);
  assertSingleNamedUnload(f, "nr");
  assertOwnerTargeted(f, "nr");
  assertRawKept(tmp, "nr");
  const b = capThrows("badxml", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, intros: ["not xml at all"] });
  assert.equal(unloads(b.f), 0, "pre-load introspect failure: no unload owed");
  assert.equal(runs(b.f), 0);
});
test("unload false and unload failure: reported once, never retried", () => {
  for (const [tag, o] of [["uf", { unload: { type: "b", data: [false] } }], ["ux", { unloadStatus: 1 }]]) {
    const { tmp, f } = capThrows(tag, { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, ...o });
    assertSingleNamedUnload(f, tag);
    assertOwnerTargeted(f, tag);
    assertRawKept(tmp, tag);
    assert.ok(JSON.parse(readFileSync(path.join(tmp, "failure.json"), "utf8")).error.length > 0);
  }
});
test("stale/foreign nonce and duplicate/out-of-order journal rejected", () => {
  const foreign = journalOk().replaceAll(NONCE, "00".repeat(16));
  for (const [tag, j] of [["stale", foreign], ["dup", journalOk().replace("done:count=1", "ready:count=1\ndone:count=1")]]) {
    const { tmp, f } = capThrows(tag, { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, journal: j });
    assertSingleNamedUnload(f, tag);
    assertOwnerTargeted(f, tag);
    assertRawKept(tmp, tag);
  }
  const nojs = journalOk().replaceAll("js: ", "");
  const { dir } = cap("nojs", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, journal: nojs });
  assert.equal(validateDir(dir).manifest.counts.windows, 1, "optional js: prefix accepted");
  cleanup(dir);
});
test("compare mismatch: per-item lines incl kwin identity; bodies compared by hash", () => {
  const a = cap("cma", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() } });
  const b = cap("cmb", { files: baseFiles({}, 7, "99999"), dirs: { "/cfg/omnitiler": enoent() }, journal: journalOk(win({ pid: 43 })) });
  const { lines, mismatch } = compareDirs(a.dir, b.dir);
  assert.ok(mismatch > 0);
  assert.ok(lines.some((l) => l.startsWith("MISMATCH kwin identity")), "identity compares as item");
  assert.ok(lines.some((l) => l.startsWith("MISMATCH window ")));
  assert.ok(lines.some((l) => l.startsWith("MATCH workspace.")));
  cleanup(a.dir); cleanup(b.dir);
});
const runHelper = (workspace) => {
  assert.ok(HELPER_SRC.includes("__BASELINE_NONCE__"), "template keeps placeholder");
  assert.ok(!/Qt\./.test(HELPER_SRC), "helper never uses Qt global");
  const logs = [];
  const ctx = { workspace, console: { log: (m) => logs.push(String(m)) }, JSON, String, Math, Error, RegExp, Object, Array, Number, isFinite };
  vm.createContext(ctx);
  vm.runInContext(HELPER_SRC.replaceAll("__BASELINE_NONCE__", NONCE), ctx);
  return logs.filter((l) => l.includes(`omnitiler:baseline:${NONCE}:`)).map((l) => l.slice(l.indexOf(`omnitiler:baseline:${NONCE}:`) + 20 + NONCE.length));
};
const kwsWin = (over = {}) => ({
  internalId: { toString: () => UUID1 }, // QUuid object: helper must String() it
  resourceClass: "rc", resourceName: "rn", pid: 42, active: false, minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false, keepAbove: false, keepBelow: false, managed: true, deleted: false, normalWindow: true, dialog: false, splash: false, utility: false, dock: false,
  frameGeometry: { x: 0, y: 0, width: 10, height: 10 }, clientGeometry: { x: 0, y: 0, width: 10, height: 10 },
  desktops: [{ id: "d1" }], activities: [], output: { name: "HDMI-1", geometry: { x: 0, y: 0, width: 10, height: 10 } }, ...over,
});
const kws = (over = {}) => ({
  windowList: () => [kwsWin()],
  desktops: [{ id: "d1", x11DesktopNumber: 1 }], currentDesktop: { id: "d1" },
  currentDesktopForScreen: (s) => ({ id: "d1", _for: s.name }),
  activeWindow: null, cursorPos: { x: 1, y: 2 },
  screens: [{ name: "HDMI-1", geometry: { x: 0, y: 0, width: 10, height: 10 } }], activeScreen: { name: "HDMI-1" },
  activities: [], currentActivity: "act",
  setCurrentDesktop() { throw new Error("helper must not write"); },
  ...over,
});
test("helper vm: read-only, QUuid String conversion, nonce + per-output views", () => {
  const out = runHelper(kws());
  assert.equal(out[0], "ready:count=1");
  assert.deepEqual(JSON.parse(out[1].slice(7)).internalId, UUID1);
  const w = JSON.parse(out[2].slice(10));
  assert.deepEqual(w.perOutput, [{ output: "HDMI-1", desktop: "d1" }]);
  assert.equal(out[3], "done:count=1");
  assert.equal(out.length, 4, "exactly ready/window/workspace/done");
});
test("helper vm: missing fields fail closed to a single error line", () => {
  const out = runHelper(kws({ windowList: () => [kwsWin({ pid: undefined })] }));
  assert.equal(out.length, 1);
  assert.ok(out[0].startsWith("error:"), "single error line, no payload");
});
test("post-load output failures: single named unload, raw receipts, never numeric", () => {
  const foreign = journalOk().replaceAll(NONCE, "00".repeat(16));
  const cases = [
    ["run-fail", { runStatus: 1 }],
    ["journal-cmd-fail", { journalStatus: 1 }],
    ["foreign-nonce", { journal: foreign }],
    ["bad-window", { journal: journalOk(win({ pid: -1 })) }],
    ["bad-workspace", { journal: journalOk(win(), ws({ desktops: [] })) }],
  ];
  for (const [tag, o] of cases) {
    const { tmp, f } = capThrows(tag, { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, ...o });
    assertSingleNamedUnload(f, tag);
    assert.equal(runs(f), 1, `${tag}: run attempted exactly once`);
    assertRawKept(tmp, tag);
  }
});
test("stale/foreign nonce after load unloads NAME once", () => {
  const foreign = journalOk().replaceAll(NONCE, "00".repeat(16));
  const { tmp, f } = capThrows("stale-unload", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, journal: foreign });
  assertSingleNamedUnload(f, "stale-unload");
  assertRawKept(tmp, "stale-unload");
});
test("journal failure with failing teardown: one unload, teardown reported, no retry", () => {
  const foreign = journalOk().replaceAll(NONCE, "00".repeat(16));
  const tmp = path.join(mkdtempSync(path.join(os.tmpdir(), "bl-jt-")), "cap");
  const f = fake({ files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, journal: foreign, unload: { type: "b", data: [false] }, helperPath: path.join(tmp, "helper.source.js") });
  assert.throws(() => captureDir(tmp, f.runner, { nonce: NONCE }), (e) => e instanceof AuditError && /teardown/.test(e.message));
  assert.ok(!existsSync(path.join(tmp, "manifest.json")), "no complete manifest on failure");
  assert.ok(existsSync(path.join(tmp, "failure.json")), "failure record saved outside repo");
  assertSingleNamedUnload(f, "jt");
  assertOwnerTargeted(f, "jt");
  assertRawKept(tmp, "jt");
});
test("post-load malformed introspect unloads NAME once, no retry", () => {
  const { tmp, f } = capThrows("pmi", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, intros: [kids(["Script0"]), "not xml at all", kids(["Script0"])] });
  assertSingleNamedUnload(f, "pmi");
  assertRawKept(tmp, "pmi");
});
test("strict introspection XML: malformed docs rejected, valid parsed", () => {
  const DOCTYPE = `<!DOCTYPE node PUBLIC "-//freedesktop//DTD D-BUS Object Introspection 1.0//EN" "http://www.freedesktop.org/standards/dbus/1.0/introspect.dtd">`;
  const good = { type: "s", data: [`<node><interface name="org.kde.kwin.Scripting"><method name="loadScript"><arg name="a" type="s" direction="in"/></method></interface><node name="Script0"/><node name="Script3"/></node>`] };
  assert.deepEqual([...childNames(good, "t")].sort(), ["Script0", "Script3"]);
  const full = { type: "s", data: [`<?xml version="1.0" encoding="UTF-8"?>${DOCTYPE}<node name="/Scripting"><!-- ok --><interface name="org.kde.kwin.Scripting"><method name="loadScript"><arg type="s" direction="in"/><arg name="b" type="i" direction="out"/><annotation name="org.test.Annotation" value="true"/></method><signal name="testSignal"><arg type="s"/></signal><property name="prop" type="s" access="readwrite"><annotation name="a" value="b"/></property></interface><node name="Script0"></node><!-- tail --></node>`] };
  assert.deepEqual([...childNames(full, "full")], ["Script0"]);
  const ent = { type: "s", data: [`<node><node name="A&#x30;"/></node>`] };
  assert.deepEqual([...childNames(ent, "ent")], ["A0"]);
  assert.deepEqual([...childNames({ type: "s", data: ["<node>\t<node name=\"Script3\"/>\n</node>"] }, "tab-ws")], ["Script3"]);
  assert.deepEqual([...childNames({ type: "s", data: [`<node><interface name="org.a.B"><annotation name="a" value="&#9;"/></interface></node>`] }, "tab-ref")], []);
  for (const [tag, xml] of [
    ["plain", "not xml at all"],
    ["text-outside", `hello <node></node>`],
    ["text-inside", `<node>hello</node>`],
    ["ws-text", `<node>  <node name="Script3"/> tail </node>`],
    ["unclosed", `<node><node name="Script3"></node>`],
    ["single-quote", `<node><node name='Script3'/></node>`],
    ["unquoted", `<node><node name=Script3/></node>`],
    ["multi-root", `<node></node><node></node>`],
    ["stray", `<node>oops <</node>`],
    ["self-root", `<node/>`],
    ["extra-attr", `<node><node name="Script3" extra="1"/></node>`],
    ["dup-attr", `<node><node name="a" name="b"/></node>`],
    ["dup-child", `<node><node name="Script3"/><node name="Script3"/></node>`],
    ["nested-node", `<node><node name="a"><node name="b"/></node></node>`],
    ["nested-iface-node", `<node><interface name="org.a.B"><node name="x"/></interface></node>`],
    ["bad-child-name", `<node><node name="a/b"/></node>`],
    ["child-no-name", `<node><node/></node>`],
    ["root-attr", `<node foo="1"></node>`],
    ["iface-no-dot", `<node><interface name="plain"/></node>`],
    ["method-misplaced", `<node><method name="m"/></node>`],
    ["arg-misplaced", `<node><interface name="org.a.B"><arg type="s"/></interface></node>`],
    ["arg-no-type", `<node><interface name="org.a.B"><method name="m"><arg name="a"/></method></interface></node>`],
    ["arg-bad-dir", `<node><interface name="org.a.B"><method name="m"><arg type="s" direction="sideways"/></method></interface></node>`],
    ["prop-bad-access", `<node><interface name="org.a.B"><property name="p" type="s" access="admin"/></interface></node>`],
    ["prop-missing", `<node><interface name="org.a.B"><property name="p" type="s"/></interface></node>`],
    ["annotation-misplaced", `<node><annotation name="a" value="b"/></node>`],
    ["unknown-tag", `<node><frob name="x"/></node>`],
    ["pi-hidden", `<node><?evil ?><node name="Script3"/></node>`],
    ["pi-prolog", `<?evil?><node></node>`],
    ["comment-unclosed", `<node><!-- oops <node name="Script3"/></node>`],
    ["comment-dash", `<node><!-- a--b --><node name="Script3"/></node>`],
    ["cdata", `<node><![CDATA[x]]><node name="Script3"/></node>`],
    ["doctype-bad", `<!DOCTYPE node SYSTEM "x"><node></node>`],
    ["doctype-subset", `${DOCTYPE.slice(0, -1)} [ <!ENTITY x "y"> ]><node></node>`],
    ["doctype-trailing", `<node></node>${DOCTYPE}`],
    ["entity-bad", `<node><node name="a&amp"/></node>`],
    ["entity-raw-amp", `<node><node name="a&b"/></node>`],
    ["entity-raw-lt", `<node><node name="a<b"/></node>`],
    ["entity-unknown", `<node><node name="a&foo;"/></node>`],
    ["decoded-name", `<node><node name="a&amp;b"/></node>`],
    ["annotation-surrogate", `<node><interface name="org.a.B"><annotation name="a" value="&#xD800;"/></interface></node>`],
    ["annotation-8", `<node><interface name="org.a.B"><annotation name="a" value="&#8;"/></interface></node>`],
    ["annotation-FFFF", `<node><interface name="org.a.B"><annotation name="a" value="&#xFFFF;"/></interface></node>`],
    ["raw-control", "<node>\u0008<node name=\"Script3\"/></node>"],
    ["lone-surrogate", "<node>\uD800<node name=\"Script3\"/></node>"],
    ["close-attr", `<node></node foo="1">`],
    ["unbalanced", `<node><interface name="org.a.B"></node></interface>`],
  ]) assert.throws(() => childNames({ type: "s", data: [xml] }, tag), AuditError, tag);
});
test("io.exec throw keeps receipt, cleanup single-shot still unloads", () => {
  for (const [tag, o] of [
    ["throw-journal", { throwSub: "--after-cursor" }],
    ["throw-run", { throwSub: "org.kde.kwin.Script run" }],
    ["throw-post-intro", { introspectThrowAt: 1 }],
  ]) {
    const { tmp, f } = capThrows(tag, { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, ...o });
    assertSingleNamedUnload(f, tag);
    assertOwnerTargeted(f, tag);
    const cmds = readFileSync(path.join(tmp, "commands.jsonl"), "utf8");
    assert.ok(cmds.includes("loadScript"), `${tag}: load receipt kept despite throw`);
    assert.ok(cmds.includes("unloadScript"), `${tag}: unload receipt kept despite throw`);
  }
  const pre = capThrows("throw-pre-intro", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, introspectThrowAt: 0 });
  assert.equal(unloads(pre.f), 0, "pre-load throw: no unload owed");
  assert.equal(runs(pre.f), 0);
});
test("load throw with plugin present still unloads; absent load throw never unloads", () => {
  const present = capThrows("throw-load-present", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, throwSub: "loadScript", loads: [false, true, false] });
  assertSingleNamedUnload(present.f, "throw-load-present");
  assertOwnerTargeted(present.f, "throw-load-present");
  assertRawKept(present.tmp, "throw-load-present");
  const absent = capThrows("throw-load-absent", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, throwSub: "loadScript", loads: [false, false, false] });
  assert.equal(unloads(absent.f), 0, "load throw, plugin absent: no unload owed");
  assert.equal(runs(absent.f), 0);
});
test("load command failure and ambiguity fail closed", () => {
  const fail = capThrows("load-status", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, loadStatus: 1, loads: [false, false, false] });
  assert.equal(unloads(fail.f), 0, "load status failure, absent: no unload");
  assert.equal(runs(fail.f), 0);
  const dirty = capThrows("load-status-dirty", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, loadStatus: 1, loads: [false, true, false] });
  assertSingleNamedUnload(dirty.f, "load-status-dirty");
  assertOwnerTargeted(dirty.f, "load-status-dirty");
  assertRawKept(dirty.tmp, "load-status-dirty");
});
test("final verification failures unload NAME once, never retry", () => {
  for (const [tag, o] of [
    ["final-true", { loads: [false, true, true] }],
    ["final-node-present", { intros: [kids(["Script0"]), kids(["Script0", "Script3"]), kids(["Script0", "Script3"])] }],
    ["final-xml-bad", { intros: [kids(["Script0"]), kids(["Script0", "Script3"]), "not xml at all"] }],
  ]) {
    const { tmp, f } = capThrows(tag, { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, ...o });
    assertSingleNamedUnload(f, tag);
    assertOwnerTargeted(f, tag);
    assertRawKept(tmp, tag);
    assert.ok(/teardown|still/.test(JSON.parse(readFileSync(path.join(tmp, "failure.json"), "utf8")).error), `${tag}: teardown reported`);
  }
});
test("identity drift before run unloads original NAME once, never touches new owner", () => {
  const { tmp, f } = capThrows("drift-owner", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, ownerAfter: ":1.99", driftAt: 2 });
  assertSingleNamedUnload(f, "drift-owner");
  assertOwnerTargeted(f, "drift-owner", ":1.7");
  assert.ok(!f.calls.some((c) => Array.isArray(c) && c.includes(":1.99") && c.some((x) => typeof x === "string" && x.startsWith("/Scripting"))), "never address new owner /Scripting");
  assertRawKept(tmp, "drift-owner");
});
test("identity tick drift at completion fails closed with single unload", () => {
  const { tmp, f } = capThrows("drift-tick", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, tickAfter: "99999", tickDriftAt: 4 });
  assertSingleNamedUnload(f, "drift-tick");
  assertOwnerTargeted(f, "drift-tick", ":1.7");
  assertRawKept(tmp, "drift-tick");
});
test("malformed GetNameOwner shape is identity change, never touches new", () => {
  const { tmp, f } = capThrows("owner-bad", { files: baseFiles(), dirs: { "/cfg/omnitiler": enoent() }, ownerBad: { type: "s", data: ["org.kde.KWin"] }, ownerBadAt: 2 });
  assertSingleNamedUnload(f, "owner-bad");
  assertOwnerTargeted(f, "owner-bad", ":1.7");
  assertRawKept(tmp, "owner-bad");
});
