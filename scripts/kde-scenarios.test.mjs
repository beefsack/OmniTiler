// scripts/kde-scenarios.test.mjs: hermetic tests for the KDE scenario runner.
// Run with exactly: node --test scripts/kde-scenarios.test.mjs
// OFFLINE ONLY: every test runs against a stateful FAKE process boundary io.
// The fake KWin executes the real helper source (post-substitution) in
// node:vm; journal stubs replay VM-produced rows. The fake planner computes
// allocations from fake windows; fake KWin frames can be perturbed.
import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath } from "node:url";
import {
  SCOPE_CLASS, PLUGIN_ID, PLANNER_BUS, PLANNER_OBJECT, PLANNER_IFACE, PLANNER_METHOD,
  WORLD_CONTRACT_VERSION, HELPER_PREFIX,
  ScenarioError, createMutOnce, parseArgs, checkEvidenceDir,
  isOpaqueId, isCanonicalU64, normalizeNativeId, isCarriedRect, parseDevStatusMode,
  frameOf, frameEqual, frameDiff, frameInside, frameArea, parseReadiness, readinessPass,
  parsePresentation, parseWindowInfo, devStatusCmd, devOffCmd, devOnCmd,
  fixtureServeCmd, fixtureSendCmd, ydotooldCmd, presentationReq, plannerReadCmd,
  windowInfoCmd, runScenario, liveMain, parseProcStat, createLiveIo,
} from "./kde-scenarios.mjs";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const TEMPLATE = path.join(HERE, "kde-scenarios-helper.js");
const ROOT = "/repo";
const EVID = "/tmp/kde-evidence-1";
const ENV = { PATH: "/usr/bin", XDG_RUNTIME_DIR: "/tmp/fake-rt" };
const J = (v) => JSON.stringify(v);
const UUID_A = "{11111111-1111-1111-1111-111111111111}";
const UUID_B = "{22222222-2222-2222-2222-222222222222}";
const UUID_F = "{33333333-3333-3333-3333-333333333333}";
const UUID_X = "{44444444-4444-4444-4444-444444444444}";
const FIX_NONCE = "ab".repeat(32);
const OUT_GEOM = { x: 0, y: 0, w: 1920, h: 1080 };
const BARE = (u) => u.slice(1, 37);

// Real busctl --user --json=short envelope shapes, captured read-only.
// Every a{sv} value is {type,data}: scalar data is the bare value, list data
// is an array. Replies below are built with wrap() so the fake replays
// reality instead of agreeing with the parser on an invented shape.
const SHAPES = JSON.parse(fs.readFileSync(path.join(HERE, "kde-scenarios-fixtures", "busctl-shapes.json"), "utf8"));
const STAT = JSON.parse(fs.readFileSync(path.join(HERE, "kde-scenarios-fixtures", "proc-stat-shapes.json"), "utf8")).procStat;
const wrap = (type, data) => ({ type, data });

// ---------------------------------------------------------------------------
// Fake process boundary
// ---------------------------------------------------------------------------

function makeFake(opts) {
  opts = opts || {};
  const F = {
    opts,
    execCalls: [], spawnCalls: [], kills: [],
    files: new Map(), // path -> {data, mode}
    dirs: new Set([EVID]),
    procs: new Map(), // pid -> {tick, alive, name}
    pidSeq: 70011,
    kwinPid: 70011, kwinTick: "424242", kwinOwner: ":1.77",
    devUp: false, plannerPid: null, plannerTick: null, scriptId: 7,
    fixture: null, cargoPid: null, fixtureUp: false, spawnedAB: false, closedB: false,
    activated: false, appendedCurrent: false, homeDesktop: "d1", hostSkip: [],
    sawRemove: false, postRemoveRuns: 0,
    desktops: [{ id: "d1", name: "Desktop 1" }, { id: "d2", name: "Desktop 2" }],
    current: "d1", appended: null, activeUuid: null,
    wins: [],
    journal: [], cursorSeq: 0,
    loaded: new Map(), // plugin -> id
    scriptSeq: 41,
    runCount: new Map(), // semantic mutation counts
    devOnCount: 0, devOffCount: 0, spawnCount: 0, closeCount: 0, shutdownCount: 0,
    fail: opts.fail || {}, // step -> mode
    framePatch: opts.framePatch || null, // fn(wins) applied before each native read
    allocPatch: opts.allocPatch || null, // fn(windows) applied to presentation rows
    leave: opts.leave || {}, // cleanup verifications to sabotage
    rmdirErr: opts.rmdirErr || null,
  };
  const newPid = (name) => {
    const pid = F.pidSeq;
    F.pidSeq += 17;
    F.procs.set(pid, { tick: String(100000 + pid), alive: true, name });
    return pid;
  };
  F.procs.set(F.kwinPid, { tick: F.kwinTick, alive: true, name: "kwin" });
  F.pidSeq = 70101;
  const mkWin = (uuid, cls, pid, title, frame, desks) => ({
    internalId: uuid, resourceClass: cls, pid, caption: title, captionNormal: title,
    frameGeometry: { x: frame.x, y: frame.y, width: frame.w, height: frame.h },
    output: { name: "HDMI-1", geometry: { x: OUT_GEOM.x, y: OUT_GEOM.y, width: OUT_GEOM.w, height: OUT_GEOM.h } },
    desktops: desks.map((id) => ({ id })),
  });
  F.mkWin = mkWin;
  F.resetWins = () => {
    const fp = F.fixture ? F.fixture.pid : 70231;
    F.wins = [
      mkWin(UUID_A, SCOPE_CLASS, fp, "OmniTilerTestWindow:a:tag9", { x: 0, y: 0, w: 960, h: 1080 }, ["d1"]),
      mkWin(UUID_B, SCOPE_CLASS, fp, "OmniTilerTestWindow:b:tag9", { x: 960, y: 0, w: 960, h: 1080 }, ["d1"]),
      mkWin(UUID_F, "org.kde.dolphin", 1234, "Foreign Files - Do Not Touch", { x: 0, y: 0, w: 400, h: 300 }, ["d1"]),
    ];
  };
  F.resetWins();
  // Window A carries a non-empty caption suffix (caption = captionNormal +
  // suffix, as KWin reports it), so the captionNormal-first join stays live.
  F.wins[0].caption = F.wins[0].captionNormal + " - Suffix";
  if (F.fail.readEmpty) { F.desktops = []; }
  F.newCursor = () => "CUR" + (F.cursorSeq++);
  F.journalPush = (pid, msg) => { F.journal.push({ cursor: F.newCursor(), pid, msg }); };
  const count = (k) => { F.runCount.set(k, (F.runCount.get(k) || 0) + 1); };
  F.count = count;
  // Guard: killing anything not receipted by this run is a violation.
  F.checkKill = (pid) => {
    const receipted = new Set();
    if (F.cargoPid) { receipted.add(F.cargoPid); }
    if (F.fixture) { receipted.add(F.fixture.pid); }
    if (F.ydPid) { receipted.add(F.ydPid); }
    if (F.plannerPid) { receipted.add(F.plannerPid); }
    if (!receipted.has(pid)) { throw new ScenarioError("VIOLATION: kill of unreceipted pid " + pid); }
  };

  // Execute the REAL helper source (post-substitution) on the fake workspace.
  F.execHelper = (src) => {
    const m = /var NONCE = "([0-9a-f]{32})"/.exec(src);
    if (!m) { throw new ScenarioError("helper source lost its nonce"); }
    const nonce = m[1];
    // The substituted op is the single-line `var OP = {...};` statement.
    // (Compact `"op":"x"` never appears in template code, which uses spaces.)
    let opName = null, opTarget = null;
    const opLine = /var OP = (\{[^\n]*\});/.exec(src);
    if (opLine) {
      try { const o = JSON.parse(opLine[1]); opName = o.op; opTarget = o.id || null; } catch (e) { /* leave null */ }
    }
    const deskObjs = F.desktops.map((d) => ({ id: d.id, name: d.name }));
    const findDesk = (id) => deskObjs.find((d) => d.id === id);
    const ws = {
      get desktops() { return F.desktops.map((d) => ({ id: d.id, name: d.name })); },
      get currentDesktop() { const d = findDesk(F.current); return d || null; },
      set currentDesktop(v) {
        if (typeof v === "string" || v === null || typeof v !== "object") { throw new Error("currentDesktop needs a desktop object"); }
        if (!F.desktops.some((d) => d.id === v.id)) { throw new Error("currentDesktop unknown object"); }
        F.current = v.id;
      },
      get activeWindow() { return F.wins.find((w) => w.internalId === F.activeUuid) || null; },
      set activeWindow(v) {
        if (v === null || typeof v !== "object") { throw new Error("activeWindow needs a window object"); }
        if (F.wins.indexOf(v) < 0) { throw new Error("activeWindow unknown object"); }
        F.activeUuid = v.internalId;
      },
      windowList() { return F.wins; },
      createDesktop(pos, name) {
        if (pos !== F.desktops.length) { throw new Error("createDesktop position must be beyond the roster"); }
        const id = "desk-new";
        F.desktops.push({ id, name });
      },
      removeDesktop(obj) {
        if (typeof obj === "string" || obj === null || typeof obj !== "object") { throw new Error("removeDesktop needs a desktop object"); }
        const i = F.desktops.findIndex((d) => d.id === obj.id);
        if (i < 0) { throw new Error("removeDesktop unknown object"); }
        F.desktops.splice(i, 1);
        if (F.current === obj.id) { F.current = F.desktops.length ? F.desktops[0].id : null; }
      },
    };
    const logs = [];
    const sandbox = { workspace: ws, console: { log: (...a) => logs.push(a.join(" ")) } };
    vm.createContext(sandbox);
    const prevCurrent = F.current;
    const prevDesks = F.desktops.map((d) => ({ id: d.id, name: d.name }));
    vm.runInContext(src, sandbox, { timeout: 2000 });
    // Fault injection on executed ops (post-effect, so the helper's own
    // verification sees the unpatched world first).
    if (opName === "setCurrent" && (F.fail.setCurrentWrong || (F.fail.restoreStuck && opTarget === F.homeDesktop))) {
      F.current = prevCurrent;
    }
    if (opName === "remove" && F.fail.removeStuck) {
      F.desktops.length = 0;
      for (const d of prevDesks) { F.desktops.push(d); }
    }
    if (opName === "append" && F.fail.appendAmbig) {
      F.desktops.push({ id: "desk-extra", name: "extra" });
    }
    if (opName === "activate" && F.fail.activateUnverified) {
      F.activeUuid = null;
    }
    // Ordering facts derived from execution: hosting becomes true only once
    // current actually flips to the appended workspace, activation only once
    // the activate op actually ran.
    if (opName === "remove") { F.sawRemove = true; }
    else if (F.sawRemove && opName) { F.postRemoveRuns++; }
    if (F.current === "desk-new" && !F.appendedCurrent) {
      F.appendedCurrent = true;
      F.hostFixtures();
    }
    if (opName === "activate") {
      F.activated = true;
    }
    const prefix = "omnitiler:native:" + nonce + ":";
    for (const l of logs) { F.journalPush(F.kwinPid, (l.indexOf("js:") === 0 ? "" : "js: ") + l); }
    return { nonce, logs };
  };

  const busErr = (what) => { throw new ScenarioError("VIOLATION: " + what); };
  F.ioExec = async (argv, o) => {
    F.execCalls.push(argv.map(String).join(" "));
    const s = argv.map(String).join(" ");
    if (argv[0] === "just" && argv[1] === "dev-status") {
      if (F.fail.devStatus) { return { status: 1, stdout: "", stderr: "boom" }; }
      return { status: 0, stdout: F.devUp ? "dev mode: UP (planner verified, controller loaded)" : "dev mode: DOWN (planner unowned, controller unloaded)", stderr: "" };
    }
    if (argv[0] === "just" && argv[1] === "dev-on") {
      F.devOnCount++;
      count("dev-on");
      if (F.fail.devOn) { return { status: 1, stdout: "", stderr: "boom" }; }
      if (F.devOnCount > 1) { busErr("dev-on relaunch (second start)"); }
      F.devUp = true;
      F.plannerPid = 70317;
      F.plannerTick = String(100000 + F.plannerPid);
      F.procs.set(F.plannerPid, { tick: F.plannerTick, alive: true, name: "planner" });
      if (F.fail.readinessPart !== "no-ready") {
        F.journalPush(F.kwinPid, "js: omnitiler:world:ready gen=g9 source=local-dev");
      }
      if (F.fail.readinessPart !== "no-discovered") {
        F.journalPush(F.kwinPid, "js: omnitiler:world:discovered gen=g9");
      }
      // Production shapes: ready carries source=<rev-hex|local-dev>, dispatch
      // carries src=<synthetic per-dispatch id>; readiness is presence of all
      // four line kinds, never an equality.
      if (!F.fail.readinessDrop && F.fail.readinessPart !== "no-dispatch") {
        F.journalPush(F.kwinPid, "js: omnitiler:world:dispatch src=src-1-abc nwinv=1 cmd=observe");
      }
      if (!F.fail.readinessDrop && F.fail.readinessPart !== "no-applied") {
        F.journalPush(F.kwinPid, "js: omnitiler:world:applied rev=1 geom=2 member=2 views=0 mk=1 rm=0 fx=0 held=0");
      }
      return { status: 0, stdout: "dev-on: planner pid 70317, script id 7\n", stderr: "" };
    }
    if (argv[0] === "just" && argv[1] === "dev-off") {
      F.devOffCount++;
      count("dev-off");
      if (F.devOffCount > 1) { busErr("dev-off repeated"); }
      if (F.fail.devOff) { return { status: 1, stdout: "", stderr: "boom" }; }
      if (!F.fail.devUpStuck) { F.devUp = false; }
      if (F.plannerPid && !F.leave.planner) { F.procs.get(F.plannerPid).alive = false; }
      return { status: 0, stdout: "dev-off: done\n", stderr: "" };
    }
    if (argv[0] === "cargo" && argv.indexOf("serve") >= 0) {
      busErr("serve via blocking exec (servers must use spawnBackground)");
    }
    if (argv[0] === "cargo" && argv.indexOf("send") >= 0) {
      const payloads = argv.filter((a) => a.indexOf("{") === 0);
      for (const p of payloads) {
        if (p.indexOf("spawn-ab") >= 0) {
          F.spawnCount++;
          count("fixture-spawn");
          if (F.spawnCount > 1) { busErr("fixture spawn repeated"); }
          if (!F.appendedCurrent) { busErr("fixture spawn before appended workspace made current"); }
          if (F.fail.spawn) { return { status: 1, stdout: "", stderr: "boom" }; }
          F.spawnedAB = true;
          if (F.fail.spawnMalformed) { return { status: 0, stdout: "garbage-line\n", stderr: "" }; }
          return { status: 0, stdout: J({ ok: true, result: {} }) + "\n", stderr: "" };
        }
        if (p.indexOf("close-b") >= 0) {
          F.closeCount++;
          count("fixture-close");
          if (F.closeCount > 1) { busErr("fixture close repeated"); }
          if (!F.activated) { busErr("fixture close before activation"); }
          const b = F.wins.find((w) => w.internalId === UUID_B);
          if (b && !F.leave.bClosed) {
            if (!F.optsKeepB) { F.wins.splice(F.wins.indexOf(b), 1); }
            const a = F.wins.find((w) => w.internalId === UUID_A);
            if (a && !F.optsFreezeA) { a.frameGeometry = { x: 0, y: 0, width: 1920, height: 1080 }; }
          }
          F.closedB = true;
          // A stray managed window appearing only after the close: the reflow
          // exact-set checks (presentation and native) must refuse it.
          if (F.fail.extraReflow && !F.extraReflowAdded) {
            F.extraReflowAdded = true;
            F.wins.push(mkWin(UUID_X, SCOPE_CLASS, F.fixture.pid, "OmniTilerTestWindow:x:tag9",
              { x: 0, y: 0, w: 100, h: 100 }, ["desk-new"]));
          }
          return { status: 0, stdout: J({ ok: true, result: {} }) + "\n", stderr: "" };
        }
        if (p.indexOf("shutdown") >= 0) {
          F.shutdownCount++;
          count("fixture-shutdown");
          if (F.shutdownCount > 1) { busErr("fixture shutdown repeated"); }
          return { status: 0, stdout: J({ ok: true, result: {} }) + "\n", stderr: "" };
        }
      }
      if (payloads.some((p) => p.indexOf("get-ab") >= 0 || (p.indexOf('"op":"get"') >= 0))) {
        const wins = [];
        if (F.spawnedAB && !F.fail.getEmpty) {
          wins.push({ id: "a", ok: true, snapshot: { id: "a", pid: F.fixture.pid, title: "OmniTilerTestWindow:a:tag9" } });
          if (!F.closedB) { wins.push({ id: "b", ok: true, snapshot: { id: "b", pid: F.fixture.pid, title: "OmniTilerTestWindow:b:tag9" } }); }
        }
        return { status: 0, stdout: J({ ok: true, request_id: "get-ab", result: { windows: wins } }) + "\n", stderr: "" };
      }
      return { status: 0, stdout: J({ ok: true, result: {} }) + "\n", stderr: "" };
    }
    if (argv[0] === "ydotoold") { busErr("ydotoold via blocking exec (must use spawnBackground)"); }
    if (argv[0] === "busctl") {
      if (s.indexOf("GetNameOwner") >= 0 && s.indexOf(KWIN_BUS_FAKE) >= 0) {
        if (F.fail.kwinOwner) { return { status: 0, stdout: J({ type: "s", data: ["kwin-not-unique"] }) + "\n", stderr: "" }; }
        return { status: 0, stdout: J({ type: "s", data: [F.kwinOwner] }) + "\n", stderr: "" };
      }
      if (s.indexOf("GetNameOwner") >= 0 && s.indexOf(PLANNER_BUS) >= 0) {
        if (!F.devUp || !F.plannerPid) { return { status: 4, stdout: "", stderr: "unowned" }; }
        return { status: 0, stdout: J({ type: "s", data: [":1.99"] }) + "\n", stderr: "" };
      }
      if (s.indexOf("GetConnectionUnixProcessID") >= 0) {
        const last = argv[argv.length - 1];
        if (last === F.kwinOwner) { return { status: 0, stdout: J({ type: "u", data: [F.kwinPid] }) + "\n", stderr: "" }; }
        if (last === ":1.99") { return { status: 0, stdout: J({ type: "u", data: [F.plannerPid] }) + "\n", stderr: "" }; }
        return { status: 1, stdout: "", stderr: "unknown" };
      }
      if (s.indexOf("Introspect") >= 0) {
        const names = [...F.loaded.values()].map((id) => '<node name="Script' + id + '"/>').join("");
        return { status: 0, stdout: J({ type: "s", data: ["<node>" + names + "</node>"] }) + "\n", stderr: "" };
      }
      if (s.indexOf("isScriptLoaded") >= 0) {
        const plugin = argv[argv.length - 1];
        const v = String(plugin).indexOf(HELPER_PREFIX) === 0 ? F.loaded.has(plugin) : F.devUp;
        return { status: 0, stdout: J({ type: "b", data: [v] }) + "\n", stderr: "" };
      }
      if (s.indexOf("unloadScript") >= 0) {
        const plugin = argv[argv.length - 1];
        if (F.fail.unload) { return { status: 0, stdout: J({ type: "b", data: [false] }) + "\n", stderr: "" }; }
        // Verification-phase-only unload failures: every scenario- and
        // cleanup-path unload succeeds; only the post-cleanup verification
        // reads (runs after the remove op plus its proof read) report, so the
        // second teardown drain stays live.
        if (F.fail.unloadVerifyOnly && F.postRemoveRuns >= 2) {
          return { status: 0, stdout: J({ type: "b", data: [false] }) + "\n", stderr: "" };
        }
        F.loaded.delete(plugin);
        return { status: 0, stdout: J({ type: "b", data: [true] }) + "\n", stderr: "" };
      }
      if (s.indexOf("loadScript") >= 0) {
        const plugin = argv[argv.length - 1];
        if (F.fail.load) { return { status: 0, stdout: J({ type: "i", data: [-1] }) + "\n", stderr: "" }; }
        const id = F.scriptSeq++;
        F.loaded.set(plugin, id);
        F.pendingPlugin = plugin;
        return { status: 0, stdout: J({ type: "i", data: [id] }) + "\n", stderr: "" };
      }
      if (argv.indexOf("run") >= 0 && /Script[0-9]+/.test(s)) {
        const m = /Script([0-9]+)/.exec(s);
        const id = Number(m[1]);
        if (![...F.loaded.values()].some((v) => v === id)) { busErr("helper run without load/introspect"); }
        const src = F.pendingSrc;
        if (!src) { busErr("helper run with no pending source"); }
        if (F.fail.noNonceLine) {
          F.journalPush(F.kwinPid, "js: some unrelated line");
        } else {
          F.execHelper(src);
        }
        F.pendingSrc = null;
        return { status: 0, stdout: "", stderr: "" };
      }
      if (s.indexOf("getWindowInfo") >= 0) {
        const uuid = argv[argv.length - 1];
        const w = F.wins.find((x) => x.internalId === uuid);
        if (!w) { return { status: 0, stdout: J({ type: "a{sv}", data: [{}] }) + "\n", stderr: "" }; }
        F.winfoCount = (F.winfoCount || 0) + 1;
        const cls = F.fail.classMismatch ? "SomethingElse" : w.resourceClass;
        // Production getWindowInfo reports captionNormal, never the suffixed
        // caption: mirror that so the suffixed fake caption stays live.
        const cap = (typeof w.captionNormal === "string" && w.captionNormal) || w.caption;
        const fg = w.frameGeometry || { x: 0, y: 0, width: 0, height: 0 };
        // Real-shaped a{sv}: every value wrapped with its KWin-source
        // signature (strings s, desktops as, pid i, geometry d).
        const desks = (F.fail.moveOffAppend && F.winfoCount > 2) ? ["d1"] : w.desktops.map((d) => d.id);
        return {
          status: 0,
          stdout: J({
            type: "a{sv}",
            data: [{
              resourceClass: wrap("s", cls),
              resourceName: wrap("s", "x"),
              caption: wrap("s", cap),
              x: wrap("d", fg.x),
              y: wrap("d", fg.y),
              width: wrap("d", fg.width),
              height: wrap("d", fg.height),
              desktops: wrap("as", desks),
              uuid: wrap("s", w.internalId),
              pid: wrap("i", w.pid),
            }],
          }) + "\n",
          stderr: "",
        };
      }
      if (s.indexOf(PLANNER_METHOD) >= 0) {
        if (!F.devUp) { busErr("planner read before dev-on"); }
        const m = /"req":"([^"]+)"/.exec(s);
        const req = m ? m[1] : "scen-x";
        const reqOut = F.fail.presentMismatch ? "some-other-req" : req;
        const rows = [];
        const a = F.wins.find((w) => w.internalId === UUID_A);
        const b = F.wins.find((w) => w.internalId === UUID_B);
        const win = (w, alloc) => ({
          win: BARE(w.internalId), out: "out-1", ws: F.fail.splitWs ? (w.internalId === UUID_A ? "ws-1" : "ws-2") : "ws-1",
          alloc, frame: null, frame_auth: "controlled", frame_hold: "",
          member_auth: "controlled", member_hold: "", floating: false, float_origin: "",
          cmdSeq: null, visibility: null, visibility_auth: "controlled", visibility_hold: "", state: [],
        });
        if (F.closedB) {
          const g = F.framePatch && F.framePatch.grown === false ? [0, 0, 960, 1080] : [0, 0, 1920, 1080];
          rows.push(win(a, F.allocPatch ? F.allocPatch("a1", g) : g));
        } else {
          const ga = [0, 0, 960, 1080];
          let gb = [960, 0, 960, 1080];
          F.presentCount = (F.presentCount || 0) + 1;
          // First-divergence probes: a one-time bad allocation resolves on the
          // next poll (transient), while admitDrift keeps disagreeing with a
          // different allocation every later read (persistent).
          if (F.fail.flakyAdmitOnce && F.presentCount === 1) { gb = [0, 0, 10, 10]; }
          else if (F.fail.admitDrift) { gb = F.presentCount === 1 ? [0, 0, 10, 10] : [0, 0, 20, 20]; }
          if (F.allocPatch) { rows.push(win(a, F.allocPatch("a0", ga))); rows.push(win(b, F.allocPatch("b0", gb))); }
          else { rows.push(win(a, ga)); rows.push(win(b, gb)); }
        }
        // Stray managed windows: any other scope-class window present in the
        // fake workspace is also presented, so the exact-set oracle checks
        // (exactly {A,B} at admission, exactly {A} after reflow) stay live.
        for (const w of F.wins) {
          if (w.internalId !== UUID_A && w.internalId !== UUID_B &&
            w.resourceClass === SCOPE_CLASS && typeof w.pid === "number" && w.pid > 0) {
            rows.push(win(w, [0, 0, 100, 100]));
          }
        }
        const body = {
          v: 4, outcome: "presentation", gen: "g9", req: reqOut, rev: "78", obs: "91",
          coveredInstance: null, coveredInvalid: null, scopes: [], workspaces: [], outputs: [],
          windows: rows, presentation: {}, preview: null,
        };
        return { status: 0, stdout: J({ type: "s", data: [J(body)] }) + "\n", stderr: "" };
      }
      throw new ScenarioError("fake: unstubbed busctl: " + s.slice(0, 120));
    }
    if (argv[0] === "journalctl") {
      if (s.indexOf("--show-cursor") >= 0) {
        if (F.fail.cursorEmpty) { return { status: 0, stdout: "nothing here\n", stderr: "" }; }
        return { status: 0, stdout: "-- cursor: CUR" + F.cursorSeq + "\n", stderr: "" };
      }
      if (s.indexOf("--after-cursor") >= 0) {
        const ci = argv.indexOf("--after-cursor");
        const cur = argv[ci + 1];
        const n = Number(String(cur).replace("CUR", ""));
        const pidM = /_PID=([0-9]+)/.exec(s);
        const pid = pidM ? Number(pidM[1]) : null;
        // Journal cursor from the UNFILTERED stream is the sequence length.
        const lines = F.journal.filter((e, ix) => ix >= n && (pid === null || e.pid === pid))
          .map((e) => J({ MESSAGE: e.msg }));
        return { status: 0, stdout: lines.join("\n") + (lines.length ? "\n" : ""), stderr: "" };
      }
    }
    throw new ScenarioError("fake: unstubbed exec: " + s.slice(0, 140));
  };
  const KWIN_BUS_FAKE = "org.kde.KWin";
  F.ioSpawn = async (argv, o) => {
    F.spawnCalls.push(argv.map(String).join(" "));
    const s = argv.map(String).join(" ");
    if (s.indexOf("tiler-live-fixture") >= 0 && s.indexOf("serve") >= 0) {
      const di = argv.indexOf("--dir");
      const dir = argv[di + 1];
      const sock = dir + "/fixture.sock";
      const cargoPid = newPid("cargo");
      F.cargoPid = cargoPid;
      if (F.fail.announce) {
        let dead = false;
        return {
          pid: cargoPid,
          waitLine: async () => { throw new Error("no match"); },
          exited: async (t) => (dead ? { code: 0, signal: null } : null),
          kill: async (sig) => {
            F.kills.push([cargoPid, sig]);
            F.checkKill(cargoPid);
            dead = true;
            F.procs.get(cargoPid).alive = false;
          },
        };
      }
      const fp = 70231;
      F.procs.set(fp, { tick: String(100000 + fp), alive: true, name: "fixture" });
      F.fixture = { socket: sock, pid: fp, nonce: FIX_NONCE, dir };
      // No resetWins() here: the serve starts the fixture, but test-modelled
      // native deviations are set on F.wins before the run and must survive.
      F.files.set(sock, { data: "sock", mode: 0 });
      let exited = null;
      return {
        pid: cargoPid,
        waitLine: async (pred, t) => {
          const line = J({ socket: sock, pid: fp, nonce: FIX_NONCE });
          if (pred(line)) { return line; }
          throw new Error("no match");
        },
        exited: async (t) => {
          if (F.shutdownCount > 0 && !F.leave.fixtureProc) {
            F.procs.get(fp).alive = false;
            F.procs.get(cargoPid).alive = false;
            F.files.delete(sock);
            try { F.dirs.delete(dir); } catch (e) { /* keep */ }
            return { code: 0, signal: null };
          }
          return exited;
        },
        kill: async (sig) => { F.kills.push([cargoPid, sig]); F.checkKill(cargoPid); },
      };
    }
    if (argv[0] === "ydotoold") {
      const yp = newPid("ydotoold");
      F.ydPid = yp;
      const sock = argv.filter((a) => a.indexOf("--socket-path=") === 0)[0].slice("--socket-path=".length);
      if (!F.fail.ydotooldSocket) { F.files.set(sock, { data: "sock", mode: 0 }); }
      return {
        pid: yp,
        waitLine: async () => { throw new Error("no lines"); },
        exited: async (t) => {
          if (F.ydDead) { return { code: 0, signal: null }; }
          return null;
        },
        kill: async (sig) => {
          F.kills.push([yp, sig]);
          F.checkKill(yp);
          if (!F.leave.ydotoold) {
            F.ydDead = true;
            F.procs.get(yp).alive = false;
            if (!F.leave.ydSock) { F.files.delete(sock); }
          }
        },
      };
    }
    throw new ScenarioError("fake: unstubbed spawn: " + s.slice(0, 120));
  };
  const rt = "/tmp/fake-rt/omnitiler-dev";
  F.io = {
    exec: F.ioExec,
    spawnBackground: F.ioSpawn,
    readFile: async (p) => {
      if (String(p).endsWith("kde-scenarios-helper.js")) { return fs.readFileSync(TEMPLATE, "utf8"); }
      if (p === "/tmp/fake-rt/omnitiler-dev/planner-pid") { return String(F.plannerPid) + "\n"; }
      if (p === "/tmp/fake-rt/omnitiler-dev/controller-receipt-path") { return rt + "/receipt\n"; }
      if (p === rt + "/receipt") { return J({ script_id: F.scriptId }) + "\n"; }
      if (F.files.has(p)) { return F.files.get(p).data; }
      throw new Error("fake: unstubbed read " + p);
    },
    writeFile: async (p, d, m) => {
      F.files.set(p, { data: String(d), mode: m });
      if (String(d).indexOf("omnitiler:native:") >= 0) { F.pendingSrc = String(d); }
    },
    mkdir: async (d, m) => { F.dirs.add(d); },
    rmdir: async (d) => {
      if (F.rmdirErr) {
        // Real fs errors carry .code; the fixture removes its own run dir on exit.
        if (F.rmdirErr === "ENOENT") { F.dirs.delete(d); }
        throw Object.assign(new Error(F.rmdirErr + ": rmdir " + d), { code: F.rmdirErr });
      }
      if (F.leave.rundir) { throw new Error("not empty"); }
      F.dirs.delete(d);
    },
    rm: async (p) => {
      // leave.helperSrc models a helper source file the rm cannot remove, so
      // the runner's post-rm existence check stays live.
      if (F.leave.helperSrc && String(p).indexOf("/helper-") >= 0) { return; }
      F.files.delete(p);
    },
    exists: async (p) => {
      if (F.leave.fixtureSock && String(p).endsWith("fixture.sock")) { return true; }
      if (F.leave.rundir && p === EVID + "/fixture-run") { return true; }
      if (String(p).endsWith("ydotool.sock")) { return F.files.has(p); }
      if (String(p).endsWith("fixture.sock")) { return F.files.has(p); }
      return F.files.has(p) || F.dirs.has(p);
    },
    // The fake renders a real-shaped /proc/<pid>/stat (trailing newline and
    // all, from the captured shape) and parses it with the production parser.
    procIdentity: async (pid) => {
      const e = F.procs.get(pid);
      if (!e || !e.alive) { return null; }
      const text = STAT.plain.replace(/^4242 /, pid + " ").replace(/ 2241 /, " " + e.tick + " ");
      return parseProcStat(text, pid);
    },
    sleep: async (ms) => {},
    now: (() => { let t = 1000; return () => (t += 5); })(),
  };
  // Hosting hook: after the appended workspace exists and is current, fixture
  // windows land on it. Windows in F.hostSkip keep their desks, so tests can
  // model a native side that misreports placement.
  F.hostFixtures = () => {
    for (const w of F.wins) {
      if ((w.internalId === UUID_A || w.internalId === UUID_B) && F.hostSkip.indexOf(w.internalId) < 0) {
        w.desktops = [{ id: "desk-new" }];
      }
    }
  };
  return F;
}

const baseOpts = () => ({ io: null, root: ROOT, env: ENV, evidenceDir: EVID, templatePath: TEMPLATE });

// ---------------------------------------------------------------------------
// A. Happy path
// ---------------------------------------------------------------------------

test("A: happy path ordered events, exit 0, no nonce/foreign leakage, real pids", async () => {
  const f = makeFake();
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, checks: res.checks }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
  const ev = res.events;
  const seq = ["preflight", "kwin-identity", "fixture-serve", "journal-cursor",
    "workspace-read", "workspace-append", "workspace-reread", "workspace-setcurrent",
    "dev-on", "readiness", "fixture-spawn", "native-read", "join", "activate",
    "scenario-admission", "scenario-close-b", "scenario-done", "cleanup",
    "cleanup-fixture-shutdown", "cleanup-workspace-restore", "cleanup-dev-off",
    "cleanup-workspace-remove", "verify-fixture-pid-gone", "verify-cargo-pid-gone",
    "verify-fixture-socket-absent", "verify-rundir-removed", "verify-planner-pid-gone",
    "verify-current-restored", "verify-appended-absent", "verify-dev-down"];
  assert.deepEqual(ev, seq);
  assert.equal(res.receipts.kwin.pid, 70011);
  assert.equal(res.receipts.fixture.pid, 70231);
  assert.notEqual(res.receipts.cargo.pid, res.receipts.fixture.pid);
  assert.equal(res.receipts.planner.pid, 70317);
  // No-leak scope is the runner's published evidence plus persisted file
  // contents. Raw argv transcripts carry nonces by construction (fixture send
  // needs --nonce; loadScript names the nonce plugin) and are excluded.
  const blob = J(res) +
    [...f.files.values()].map((x) => String(x.data)).join("\n");
  assert.ok(!blob.includes(FIX_NONCE), "fixture nonce leaked");
  assert.ok(!/helper-[0-9a-f]{32}/.test(J(res)), "helper nonce leaked into evidence");
  assert.ok(!blob.includes("Foreign Files"), "foreign caption leaked");
  assert.ok(!blob.includes("org.kde.dolphin"), "foreign class leaked");
  assert.equal(res.counts.foreign, 1);
  assert.equal(f.devOnCount, 1);
  assert.equal(f.spawnCount, 1);
  assert.equal(f.closeCount, 1);
  assert.equal(f.shutdownCount, 1);
  assert.equal(f.devOffCount, 1);
});

test("SHAPE: fake busctl replies conform to the captured real envelopes", async () => {
  const f = makeFake();
  const call = async (argv) => JSON.parse((await f.io.exec(argv, {})).stdout);
  const ownerR = await call(["busctl", "--user", "--json=short", "call",
    "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus", "GetNameOwner", "s", "org.kde.KWin"]);
  assert.equal(ownerR.type, SHAPES.getNameOwner.type);
  assert.equal(Array.isArray(ownerR.data), Array.isArray(SHAPES.getNameOwner.data));
  assert.equal(ownerR.data.length, SHAPES.getNameOwner.data.length);
  const pidR = await call(["busctl", "--user", "--json=short", "call",
    "org.freedesktop.DBus", "/org/freedesktop/DBus", "org.freedesktop.DBus",
    "GetConnectionUnixProcessID", "s", f.kwinOwner]);
  assert.equal(pidR.type, SHAPES.getConnectionUnixProcessID.type);
  assert.equal(Array.isArray(pidR.data), Array.isArray(SHAPES.getConnectionUnixProcessID.data));
  assert.equal(typeof pidR.data[0], typeof SHAPES.getConnectionUnixProcessID.data[0]);
  const loadedR = await call(["busctl", "--user", "--json=short", "call",
    f.kwinOwner, "/Scripting", "org.kde.kwin.Scripting", "isScriptLoaded", "s", HELPER_PREFIX + "probe"]);
  assert.equal(loadedR.type, SHAPES.isScriptLoaded.type);
  assert.equal(Array.isArray(loadedR.data), Array.isArray(SHAPES.isScriptLoaded.data));
  assert.equal(typeof loadedR.data[0], typeof SHAPES.isScriptLoaded.data[0]);
  const unknownR = await call(windowInfoCmd(UUID_X));
  assert.equal(unknownR.type, SHAPES.getWindowInfoUnknown.type);
  assert.deepEqual(unknownR.data, SHAPES.getWindowInfoUnknown.data);
  // A known window replays the variant-dict convention: every value wrapped
  // as {type,data}, scalar data bare, list data an array. Bare values fail.
  const knownR = await call(windowInfoCmd(UUID_A));
  assert.equal(knownR.type, "a{sv}");
  const body = knownR.data[0];
  assert.ok(body && typeof body === "object" && Object.keys(body).length > 0);
  for (const k of Object.keys(body)) {
    const v = body[k];
    assert.ok(v && typeof v === "object" && !Array.isArray(v), "value wrapped as {type,data}: " + k);
    assert.equal(typeof v.type, "string", "wrapper type string: " + k);
    if (v.type === "as" || v.type === "au") { assert.ok(Array.isArray(v.data), "list data is an array: " + k); }
    else { assert.ok(!Array.isArray(v.data), "scalar data is the bare value: " + k); }
  }
  assert.equal(body.resourceClass.type, "s");
  assert.equal(body.caption.type, "s");
  assert.equal(body.uuid.type, "s");
  assert.equal(body.pid.type, "i");
  assert.equal(body.desktops.type, "as");
  assert.ok(["i", "d"].includes(body.x.type) && ["i", "d"].includes(body.width.type));
});

// ---------------------------------------------------------------------------
// B. Oracle wrong-value / failure cases
// ---------------------------------------------------------------------------

test("B: 1px frame perturbation fails naming first divergence, cleanup verified", async () => {
  const f = makeFake();
  const b = f.wins.find((w) => w.internalId === UUID_B);
  b.frameGeometry = { x: 960, y: 0, width: 961, height: 1080 };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "FAIL");
  assert.match(res.divergence, /tile B frame mismatch/);
  assert.ok(res.events.includes("cleanup-fixture-shutdown"));
  assert.ok(res.events.includes("verify-dev-down"));
});

test("FRAME: within-output 1px shrink of B fails naming the exact field", async () => {
  const f = makeFake();
  const b = f.wins.find((w) => w.internalId === UUID_B);
  b.frameGeometry = { x: 960, y: 0, width: 959, height: 1080 };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "FAIL");
  assert.match(res.divergence, /tile B frame mismatch \(w\)/);
});

test("FRAME: within-output 1px shrink of A fails naming the exact field", async () => {
  const f = makeFake();
  const a = f.wins.find((w) => w.internalId === UUID_A);
  a.frameGeometry = { x: 0, y: 0, width: 959, height: 1080 };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "FAIL");
  assert.match(res.divergence, /tile A frame mismatch \(w\)/);
});

test("FRAME: exactness is pinned (unit level)", () => {
  assert.equal(frameEqual({ x: 0, y: 0, w: 960, h: 1080 }, { x: 0, y: 0, w: 960, h: 1080 }), true);
  assert.equal(frameEqual({ x: 0, y: 0, w: 959, h: 1080 }, { x: 0, y: 0, w: 960, h: 1080 }), false);
  assert.deepEqual(frameDiff({ x: 0, y: 0, w: 959, h: 1080 }, { x: 0, y: 0, w: 960, h: 1080 }), ["w"]);
  assert.deepEqual(frameDiff({ x: 0, y: 0, w: 960, h: 1080 }, { x: 0, y: 0, w: 960, h: 1080 }), []);
});

test("FIRST: transient early divergence that later resolves does not fail", async () => {
  const f = makeFake({ fail: { flakyAdmitOnce: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, checks: res.checks }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
});

test("FIRST: persistent divergence reports the first observation, not the last", async () => {
  const f = makeFake({ fail: { admitDrift: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "FAIL");
  assert.match(res.divergence, /tile B frame mismatch/);
  const fi = res.divergence.indexOf('"w":10');
  const li = res.divergence.indexOf('"w":20');
  assert.ok(fi >= 0, "first allocation visible: " + res.divergence);
  assert.ok(li < 0 || fi < li, "first observation is primary, last only labelled: " + res.divergence);
});

test("B: unknown (null) native frame is PENDING, never PASS", async () => {
  const f = makeFake();
  const b = f.wins.find((w) => w.internalId === UUID_B);
  b.frameGeometry = null;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "PENDING");
});

test("B: allocation outside output fails", async () => {
  const f = makeFake();
  f.allocPatch = (which, g) => (which === "b0" ? [1800, 0, 960, 1080] : g);
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence, /mismatch|outside/);
});

test("B: overlapping allocations fail", async () => {
  const f = makeFake();
  f.allocPatch = (which, g) => (which === "b0" ? [500, 0, 960, 1080] : g);
  const b = f.wins.find((w) => w.internalId === UUID_B);
  b.frameGeometry = { x: 500, y: 0, width: 960, height: 1080 };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence, /overlap/);
});

test("B: reflowed A unchanged fails; B lingering fails; wrong ws fails; A-only reflow mismatch fails", async () => {
  const f1 = makeFake();
  f1.framePatch = { grown: false };
  const r1 = await runScenario(Object.assign(baseOpts(), { io: f1.io }));
  assert.equal(r1.rc, 1);
  assert.match(r1.divergence, /unchanged|mismatch/);

  const f2 = makeFake();
  f2.optsKeepB = true;
  const r2 = await runScenario(Object.assign(baseOpts(), { io: f2.io }));
  assert.equal(r2.rc, 1);
  assert.match(r2.divergence, /still lists closed b|still in the native roster/);

  const f3 = makeFake();
  // Workspace ids are planner-opaque: the runner's checkable property is that
  // A and B share one presentation workspace, so split them to force it.
  f3.fail.splitWs = true;
  const r3 = await runScenario(Object.assign(baseOpts(), { io: f3.io }));
  assert.equal(r3.rc, 1);
  assert.match(r3.divergence, /different presentation workspaces/);

  const f4 = makeFake();
  f4.optsFreezeA = true;
  const r4 = await runScenario(Object.assign(baseOpts(), { io: f4.io }));
  assert.equal(r4.rc, 1);
  assert.match(r4.divergence, /reflow A frame mismatch/);
});

test("ORACLE: stray extra managed window fails admission (window set not exactly A,B)", async () => {
  const f = makeFake();
  f.wins.push(f.mkWin(UUID_X, SCOPE_CLASS, 70231, "OmniTilerTestWindow:x:tag9",
    { x: 0, y: 0, w: 100, h: 100 }, ["d1"]));
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /not exactly \{A,B\}/);
  assert.ok(res.events.includes("cleanup"));
});

test("ORACLE: stray extra managed window at reflow fails close/reflow (not exactly A)", async () => {
  const f = makeFake({ fail: { extraReflow: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /not exactly \{A\}/);
  assert.ok(res.events.includes("cleanup"));
});

// ---------------------------------------------------------------------------
// C. Join safety
// ---------------------------------------------------------------------------

test("C: absent join fails closed with cleanup", async () => {
  const f = makeFake();
  f.wins = f.wins.filter((w) => w.internalId !== UUID_B);
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence, /join for 'b' is absent/);
  assert.ok(res.events.includes("cleanup-dev-off"));
});

test("C: duplicate caption, pid mismatch, desktops mismatch, class mismatch, duplicate UUID fail closed", async () => {
  const dup = makeFake();
  dup.wins.push(Object.assign({}, dup.wins[0]));
  const rDup = await runScenario(Object.assign(baseOpts(), { io: dup.io }));
  assert.equal(rDup.rc, 1);
  assert.match(rDup.divergence, /ambiguous|duplicate/);

  const pid = makeFake();
  pid.wins.find((w) => w.internalId === UUID_A).pid = 99999;
  const rPid = await runScenario(Object.assign(baseOpts(), { io: pid.io }));
  assert.equal(rPid.rc, 1);
  assert.match(rPid.divergence, /absent/);

  const desk = makeFake();
  // Keep A pinned off the appended workspace so the join must fail closed:
  // hosting skips it, preserving the deliberately wrong placement.
  desk.hostSkip = [UUID_A];
  desk.wins.find((w) => w.internalId === UUID_A).desktops = [{ id: "d1" }];
  const rDesk = await runScenario(Object.assign(baseOpts(), { io: desk.io }));
  assert.equal(rDesk.rc, 1);
  assert.match(rDesk.divergence, /desktops mismatch|appended workspace/);

  const cls = makeFake();
  cls.fail.classMismatch = true;
  const rCls = await runScenario(Object.assign(baseOpts(), { io: cls.io }));
  assert.equal(rCls.rc, 1);
  assert.match(rCls.divergence, /class mismatch|absent/);
});

// ---------------------------------------------------------------------------
// D. Lifecycle fault injection (parameterized)
// ---------------------------------------------------------------------------

const CLEANUP_ORDER = {
  "cleanup-fixture-shutdown": 10, "cleanup-workspace-restore": 20,
  "cleanup-dev-off": 30, "cleanup-workspace-remove": 40, "cleanup-ydotoold-stop": 50,
};

const LIFECYCLE_CASES = [
  { name: "devstatus-nonzero", fail: { devStatus: true }, match: /dev-status failed/, absent: ["fixture-serve", "dev-on", "fixture-spawn", "scenario-done"] },
  { name: "kwin-owner-malformed", fail: { kwinOwner: true }, match: /KWin owner is not a unique name/, absent: ["fixture-serve", "dev-on", "fixture-spawn", "scenario-done"] },
  { name: "announce-malformed", fail: { announce: true }, match: /announcement/, absent: ["dev-on", "fixture-spawn", "scenario-done"] },
  { name: "journal-cursor-empty", fail: { cursorEmpty: true }, match: /empty journal cursor/, absent: ["workspace-read", "workspace-append", "dev-on", "scenario-done"] },
  { name: "workspace-read-empty", fail: { readEmpty: true }, match: /unreadable|empty desktop roster/, absent: ["workspace-append", "dev-on", "scenario-done"] },
  { name: "helper-load-minus1", fail: { load: true }, match: /bad script id|load failure/, absent: ["workspace-append", "dev-on", "scenario-done"] },
  { name: "helper-run-no-nonce-line", fail: { noNonceLine: true }, match: /no nonce result line/, absent: ["workspace-append", "dev-on", "scenario-done"] },
  { name: "append-ambiguous", fail: { appendAmbig: true }, match: /exactly once/, absent: ["workspace-setcurrent", "dev-on", "fixture-spawn", "scenario-done"] },
  { name: "setcurrent-wrong", fail: { setCurrentWrong: true }, match: /did not take effect|not the appended/, absent: ["dev-on", "fixture-spawn", "scenario-done"] },
  { name: "devon-nonzero", fail: { devOn: true }, match: /dev-on failed/, absent: ["fixture-spawn", "scenario-done"] },
  { name: "readiness-missing", fail: { readinessDrop: true }, match: /readiness gate/, absent: ["fixture-spawn", "scenario-done"] },
  { name: "fixture-spawn-nonzero", fail: { spawn: true }, match: /spawn failed/, absent: ["activate", "scenario-admission", "scenario-done"] },
  { name: "fixture-spawn-malformed", fail: { spawnMalformed: true }, match: /unparsable fixture reply/, absent: ["activate", "scenario-admission", "scenario-done"] },
  { name: "fixture-get-timeout", fail: { getEmpty: true }, match: /never returned both/, absent: ["activate", "scenario-admission", "scenario-done"] },
  { name: "presentation-correlation", fail: { presentMismatch: true }, match: /correlation/, absent: ["scenario-close-b", "scenario-done"] },
  { name: "activate-unverified", fail: { activateUnverified: true }, match: /unverified/, absent: ["scenario-admission", "scenario-close-b", "scenario-done"] },
  { name: "helper-unload-fails", fail: { unload: true }, match: null, absent: [] }, // cleanup failure path
  { name: "devoff-nonzero", fail: { devOff: true }, match: null, absent: [] }, // cleanup failure path
];

test("D: every lifecycle fault stops later steps, runs cleanup subset, exits nonzero", async () => {
  for (const c of LIFECYCLE_CASES) {
    const f = makeFake({ fail: c.fail });
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
    assert.equal(res.rc, 1, c.name);
    if (c.match) {
      assert.match(res.divergence || res.cleanupFailed.join(";"), c.match, c.name);
    }
    assert.ok(res.events[0] === "preflight", c.name + ": starts at preflight");
    for (const late of c.absent) {
      assert.ok(!res.events.includes(late), c.name + ": later step not executed: " + late);
    }
    assert.ok(res.events.includes("cleanup"), c.name + ": cleanup ran");
    assert.ok(res.events.includes("verify-dev-down"), c.name + ": final verify ran");
    // Cleanup subset runs in declared order (never reversed, never repeated).
    const got = res.events.filter((e) => CLEANUP_ORDER[e] !== undefined).map((e) => CLEANUP_ORDER[e]);
    const sorted = got.slice().sort((a, b) => a - b);
    assert.deepEqual(got, sorted, c.name + ": cleanup in declared order");
    assert.equal(new Set(got).size, got.length, c.name + ": no cleanup repeated");
    // No mutating call repeated, no relaunch.
    assert.equal(f.devOnCount <= 1, true, c.name + ": no relaunch");
    assert.equal(f.devOffCount <= 1, true, c.name + ": no dev-off repeat");
    assert.equal(f.spawnCount <= 1, true, c.name + ": no spawn repeat");
    assert.equal(f.closeCount <= 1, true, c.name + ": no close repeat");
    assert.equal(f.shutdownCount <= 1, true, c.name + ": no shutdown repeat");
  }
});

test("D: ambiguous append reports a cleanup ambiguity and removes nothing", async () => {
  const f = makeFake({ fail: { appendAmbig: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /exactly once/);
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("append cleanup ambiguous") >= 0), J(res.cleanupFailed));
});

test("D: dev-on nonzero still invokes dev-off exactly once", async () => {
  const f = makeFake({ fail: { devOn: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.equal(f.devOffCount, 1);
  assert.ok(res.checks.some((c) => c.name === "cleanup-dev-down"));
});

test("D: unload failure is reported as a cleanup failure (nonzero even on PASS path)", async () => {
  const f = makeFake({ fail: { unload: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.category, "PASS", J(res.divergence));
  assert.equal(res.rc, 1);
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("helper unload") >= 0), J(res.cleanupFailed));
});

test("D: verification-phase helper unload failure is reported even though the scenario PASSed", async () => {
  const f = makeFake({ fail: { unloadVerifyOnly: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.category, "PASS", J(res.divergence));
  assert.equal(res.rc, 1);
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("helper unload") >= 0), J(res.cleanupFailed));
});

test("D: dev-off failure on the PASS path reports a cleanup failure with category PASS", async () => {
  const f = makeFake({ fail: { devOff: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.category, "PASS", J(res.divergence));
  assert.equal(res.rc, 1);
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("dev-off") >= 0), J(res.cleanupFailed));
});

test("D: setCurrent ambiguity stops the scenario", async () => {
  const f = makeFake();
  f.desktops.push({ id: "desk-new", name: "Collision" });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
});

test("ORPHAN: malformed announcement terminates the cargo child once with a tick check", async () => {
  const f = makeFake({ fail: { announce: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /announcement/);
  assert.deepEqual(f.kills.filter((k) => k[0] === f.cargoPid).map((k) => k[1]), ["SIGTERM"]);
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("fixture-shutdown") >= 0 &&
    m.indexOf("no announcement arrived") >= 0), J(res.cleanupFailed));
  assert.ok(res.events.includes("cleanup-fixture-shutdown"));
});

test("ORPHAN: cargo pid reuse (tick changed) sends no signal and is reported", async () => {
  const f = makeFake({ fail: { announce: true } });
  const origSpawn = f.io.spawnBackground;
  f.io.spawnBackground = async (argv, o) => {
    const c = await origSpawn(argv, o);
    const origWait = c.waitLine;
    c.waitLine = async (...a) => {
      const e = f.procs.get(c.pid);
      if (e) { e.tick = "999991"; }
      return origWait(...a);
    };
    return c;
  };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.ok(!f.kills.some((k) => k[0] === f.cargoPid), "no signal to a reused pid: " + J(f.kills));
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("fixture-shutdown") >= 0 &&
    m.indexOf("identity changed") >= 0), J(res.cleanupFailed));
});

test("ORPHAN: ydotoold socket timeout terminates the daemon, rc 1", async () => {
  const f = makeFake({ fail: { ydotooldSocket: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /ydotoold socket never appeared/);
  assert.deepEqual(f.kills.filter((k) => k[0] === f.ydPid).map((k) => k[1]), ["SIGTERM"]);
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("ydotoold-stop") >= 0), J(res.cleanupFailed));
});

test("YDPIN: ydotoold pid reaped/replaced before cleanup sends no signal and fails cleanup", async () => {
  const f = makeFake();
  const orig = f.io.exec;
  let fired = false;
  f.io.exec = async (argv, o) => {
    const s = argv.map(String).join(" ");
    if (!fired && s.indexOf("close-b") >= 0) {
      fired = true;
      const e = f.procs.get(f.ydPid);
      if (e) { e.tick = "999992"; }
    }
    return orig(argv, o);
  };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true }));
  assert.equal(res.rc, 1);
  assert.ok(!f.kills.some((k) => k[0] === f.ydPid), "no signal to a replaced pid: " + J(f.kills));
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("ydotoold-stop") >= 0 &&
    m.indexOf("identity changed") >= 0), J(res.cleanupFailed));
});

test("YDSOCK: ydotoold pid gone but its socket remains fails cleanup naming the socket", async () => {
  const f = makeFake({ leave: { ydSock: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true }));
  assert.equal(res.rc, 1);
  assert.deepEqual(f.kills.filter((k) => k[0] === f.ydPid).map((k) => k[1]), ["SIGTERM"]);
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("ydotoold-stop") >= 0 &&
    m.indexOf("socket still present") >= 0), J(res.cleanupFailed));
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("ydotoold-gone") >= 0 || m.indexOf("ydotoold") >= 0));
});

test("PROCSTAT: production parser accepts the real stat shape (trailing newline) and fails closed", () => {
  assert.equal(parseProcStat(STAT.plain, 4242), "2241");
  assert.equal(parseProcStat(STAT.plain.slice(0, -1), 4242), "2241");
  assert.equal(parseProcStat(STAT.commWithParens, 4243), "2241");
  assert.equal(parseProcStat(STAT.plain, 4243), null, "pid mismatch");
  assert.equal(parseProcStat(STAT.plain + "\n", 4242), null, "second newline");
  assert.equal(parseProcStat(STAT.plain.replace(" 2241 ", " 0 "), 4242), null, "zero starttime");
  assert.equal(parseProcStat(STAT.plain.replace(" 2241 ", " x "), 4242), null, "non-numeric starttime");
  assert.equal(parseProcStat("4242 (short) S 1 2\n", 4242), null, "truncated");
  assert.equal(parseProcStat("", 4242), null);
});

test("PROCSTAT: live io reads a real process and a reaped pid", async () => {
  const io = createLiveIo();
  const a = await io.procIdentity(process.pid);
  assert.match(String(a), /^[1-9][0-9]*$/, "own process has a numeric start tick");
  assert.equal(await io.procIdentity(process.pid), a, "stable across reads");
  const dead = spawnSync("true").pid;
  assert.equal(await io.procIdentity(dead), null, "reaped pid is absent");
});

test("RMDIR: fixture-removed run dir (ENOENT) is not a cleanup failure; other errors still are", async () => {
  const f = makeFake({ rmdirErr: "ENOENT" });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.category, "PASS", J({ div: res.divergence }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
  assert.deepEqual(res.cleanupFailed, []);
  assert.ok(res.events.includes("verify-rundir-removed"));
  // ENOENT is tolerated but absence is still verified: dir reported present -> failure.
  const g = makeFake({ rmdirErr: "ENOENT", leave: { rundir: true } });
  const r2 = await runScenario(Object.assign(baseOpts(), { io: g.io }));
  assert.equal(r2.rc, 1);
  assert.ok(r2.cleanupFailed.some((m) => m.indexOf("run dir still present") >= 0), J(r2.cleanupFailed));
  const h = makeFake({ rmdirErr: "EBUSY" });
  const r3 = await runScenario(Object.assign(baseOpts(), { io: h.io }));
  assert.equal(r3.rc, 1);
  assert.ok(r3.cleanupFailed.some((m) => m.indexOf("EBUSY") >= 0), J(r3.cleanupFailed));
});

test("RMDIR: real io removes an empty dir, tolerates absence via code, refuses a non-empty dir", async () => {
  const io = createLiveIo();
  const base = fs.mkdtempSync(path.join(os.tmpdir(), "kde-scn-rmdir-"));
  try {
    const d = path.join(base, "run");
    fs.mkdirSync(d);
    await io.rmdir(d);
    assert.equal(fs.existsSync(d), false);
    await assert.rejects(io.rmdir(d), (e) => e.code === "ENOENT");
    fs.mkdirSync(d);
    fs.writeFileSync(path.join(d, "f"), "x");
    await assert.rejects(io.rmdir(d), (e) => e.code === "ENOTEMPTY");
    assert.equal(fs.existsSync(d), true);
  } finally { fs.rmSync(base, { recursive: true, force: true }); }
});

test("LIVEIO: real exec status/stdout and real background spawn (waitLine, kill, exit, identity)", async () => {
  const io = createLiveIo();
  const ok = await io.exec([process.execPath, "-e", "process.stdout.write('hi')"], { timeoutMs: 20000 });
  assert.deepEqual([ok.status, ok.stdout], [0, "hi"]);
  const bad = await io.exec([process.execPath, "-e", "process.exit(3)"], { timeoutMs: 20000 });
  assert.equal(bad.status, 3);
  const child = await io.spawnBackground([process.execPath, "-e",
    "console.log('ready-line');setInterval(()=>{},1000)"], {});
  assert.ok(Number.isInteger(child.pid) && child.pid > 0);
  assert.equal(await child.waitLine((l) => l === "ready-line", 20000), "ready-line");
  const tick = await io.procIdentity(child.pid);
  assert.match(String(tick), /^[1-9][0-9]*$/);
  await child.kill("SIGTERM");
  const ex = await child.exited(20000);
  assert.ok(ex && ex.signal === "SIGTERM", J(ex));
  assert.equal(await io.procIdentity(child.pid), null);
});

test("LIVEIO: a background child writing 1 MB to stderr before its line still announces", async () => {
  const io = createLiveIo();
  const child = await io.spawnBackground([process.execPath, "-e",
    "process.stderr.write('x'.repeat(1<<20), () => { console.log('late-line'); setInterval(()=>{},1000); })"], {});
  assert.equal(await child.waitLine((l) => l === "late-line", 20000), "late-line");
  await child.kill("SIGTERM");
  assert.ok(await child.exited(20000));
});

test("DESKS: fixture moved off the appended desktop after the join fails admission", async () => {
  const f = makeFake({ fail: { moveOffAppend: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "FAIL");
  assert.match(res.divergence || "", /^admission: fixture '[ab]' desktops mismatch after join/);
});

// ---------------------------------------------------------------------------
// E. Cleanup verification
// ---------------------------------------------------------------------------

test("E: each leftover verification fails the run; PASS with cleanup failure exits 1", async () => {
  const cases = [
    { key: "fixtureProc", leave: { fixtureProc: true } },
    { key: "fixtureSock", leave: { fixtureSock: true } },
    { key: "rundir", leave: { rundir: true } },
    { key: "planner", leave: { planner: true } },
    { key: "current", fail: { restoreStuck: true } },
    { key: "appended", fail: { removeStuck: true } },
    { key: "devDown", fail: { devUpStuck: true } },
    { key: "ydotoold", leave: { ydotoold: true }, input: true },
  ];
  for (const c of cases) {
    const f = makeFake({ leave: c.leave || {}, fail: c.fail || {} });
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io }, c.input ? { input: true } : {}));
    assert.equal(res.rc, 1, "leave " + c.key);
    assert.ok(res.cleanupFailed.length > 0, "leave " + c.key);
  }
});

test("E: helper source file left behind is reported as a cleanup failure", async () => {
  const f = makeFake({ leave: { helperSrc: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, cf: res.cleanupFailed }));
  assert.equal(res.rc, 1);
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("helper source") >= 0), J(res.cleanupFailed));
});

test("E: restore-stuck leaves the appended desktop current, so removal is refused", async () => {
  const f = makeFake({ fail: { restoreStuck: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("refusing removal") >= 0), J(res.cleanupFailed));
});

test("E: one cleanup failure does not stop later cleanup steps", async () => {
  const f = makeFake({ fail: { devOff: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.ok(res.events.includes("cleanup-workspace-remove"), "later step ran");
  assert.ok(res.events.includes("cleanup-fixture-shutdown"), "later step ran");
  assert.ok(res.events.includes("verify-appended-absent"), "verifications ran");
});

// ---------------------------------------------------------------------------
// F. Helper source tests (execute the real template in vm)
// ---------------------------------------------------------------------------

function runTemplate(op, ws) {
  let src = fs.readFileSync(TEMPLATE, "utf8");
  const nonce = "cd".repeat(16);
  src = src.split("__KDE_SCENARIOS_NONCE__").join(J(nonce))
    .split("__KDE_SCENARIOS_OP__").join(J(op));
  const logs = [];
  const sandbox = { workspace: ws, console: { log: (...a) => logs.push(a.join(" ")) } };
  vm.createContext(sandbox);
  vm.runInContext(src, sandbox, { timeout: 2000 });
  const prefix = "omnitiler:native:" + nonce + ":";
  return { nonce, logs, rows: logs.filter((l) => l.includes(prefix)).map((l) => JSON.parse(l.slice(l.indexOf(prefix) + prefix.length))) };
}

function modelWs() {
  const desks = [{ id: "d1", name: "D1" }];
  let current = desks[0];
  const wins = [
    {
      internalId: UUID_A, resourceClass: SCOPE_CLASS, pid: 70231,
      caption: "OmniTilerTestWindow:a:t (Suffix)", captionNormal: "OmniTilerTestWindow:a:t",
      frameGeometry: { x: 0, y: 0, width: 10, height: 10 },
      output: { name: "HDMI-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } },
      desktops: [{ id: "d1" }],
    },
    {
      internalId: UUID_F, resourceClass: "evil", pid: 666, caption: "FOREIGN-SECRET-CAPTION",
      frameGeometry: { x: 0, y: 0, width: 10, height: 10 },
      output: { name: "HDMI-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } },
      desktops: [{ id: "d1" }],
    },
  ];
  return {
    wins,
    workspace: {
      get desktops() { return desks; },
      get currentDesktop() { return current; },
      set currentDesktop(v) {
        if (typeof v !== "object" || v === null) { throw new Error("need object"); }
        current = v;
      },
      get activeWindow() { return null; },
      set activeWindow(v) {
        if (typeof v !== "object" || v === null) { throw new Error("need object"); }
      },
      windowList() { return wins; },
      createDesktop(pos, name) { desks.push({ id: "desk-new", name }); },
      removeDesktop(o) { desks.splice(desks.findIndex((d) => d.id === o.id), 1); },
    },
  };
}

test("F: foreign windows never emitted; mutate ops use exact id lookup; unknown op rejected", () => {
  const m = modelWs();
  const r = runTemplate({ op: "read", scope: SCOPE_CLASS }, m.workspace);
  assert.equal(r.rows.length, 1);
  assert.equal(r.rows[0].owned.length, 1);
  assert.equal(r.rows[0].foreignCount, 1);
  for (const l of r.logs) { assert.ok(!l.includes("FOREIGN-SECRET-CAPTION")); }
  const bad = runTemplate({ op: "format-drive", scope: SCOPE_CLASS }, m.workspace);
  assert.match(bad.rows[0].error, /unsupported op/);
  const app = runTemplate({ op: "append", name: "omnitiler-test-12345678", scope: SCOPE_CLASS }, m.workspace);
  assert.equal(app.rows[0].id, "desk-new");
  const set = runTemplate({ op: "setCurrent", id: "desk-new", scope: SCOPE_CLASS }, m.workspace);
  assert.equal(set.rows[0].id, "desk-new");
  const act = runTemplate({ op: "activate", uuid: UUID_A, scope: SCOPE_CLASS }, m.workspace);
  assert.equal(act.rows[0].ok, true);
  const rm = runTemplate({ op: "remove", id: "desk-new", scope: SCOPE_CLASS }, m.workspace);
  assert.equal(rm.rows[0].removed, true);
});

test("F: helper reports captionNormal, not the suffixed caption, with caption fallback", () => {
  const m = modelWs();
  const r = runTemplate({ op: "read", scope: SCOPE_CLASS }, m.workspace);
  assert.equal(r.rows.length, 1);
  assert.equal(r.rows[0].owned.length, 1);
  assert.equal(r.rows[0].owned[0].title, "OmniTilerTestWindow:a:t");
  // A window with no captionNormal falls back to caption.
  const m2 = modelWs();
  delete m2.wins[0].captionNormal;
  const r2 = runTemplate({ op: "read", scope: SCOPE_CLASS }, m2.workspace);
  assert.equal(r2.rows[0].owned[0].title, "OmniTilerTestWindow:a:t (Suffix)");
});

test("F: helper source is non-module ES2017 IIFE with no Qt/import/optional syntax", () => {
  const src = fs.readFileSync(TEMPLATE, "utf8");
  assert.ok(!/^\s*import\s/m.test(src));
  assert.ok(!/export\s/m.test(src));
  assert.ok(src.indexOf("Qt.") < 0);
  assert.ok(src.indexOf("?.") < 0);
  assert.ok(src.indexOf("??") < 0);
  assert.ok(!/catch\s*\{/.test(src));
  assert.ok(!/replaceAll/.test(src));
  assert.match(src, /\(function \(\) \{/);
});

// ---------------------------------------------------------------------------
// G. CLI / hygiene
// ---------------------------------------------------------------------------

test("G: no --live exits 2 with zero io calls", async () => {
  let calls = 0;
  const io = new Proxy({}, { get: () => { calls++; return async () => { throw new Error("must not be called"); }; } });
  const rc = await liveMain({ argv: [], ioFactory: () => io, root: ROOT });
  assert.equal(rc, 2);
  assert.equal(calls, 0);
});

test("G: evidence dir validation (relative, in-repo, existing, non-canonical)", async () => {
  for (const bad of ["rel/path", "/repo/ev", "/tmp//ev", "/tmp/../ev", "/tmp/ev/"]) {
    const rc = await liveMain({
      argv: ["--live", "--evidence-dir", bad],
      ioFactory: () => { throw new Error("must not build io"); },
      root: "/repo",
    });
    assert.equal(rc, 2, bad);
  }
});

test("G: --input starts ydotoold in background and cleans it; default does not", async () => {
  const f = makeFake();
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true }));
  assert.equal(res.category, "PASS", J({ div: res.divergence }));
  assert.ok(f.spawnCalls.some((c) => c.indexOf("ydotoold") === 0), "ydotoold spawned in background");
  assert.deepEqual(f.kills.filter((k) => k[0] === f.ydPid).map((k) => k[1]), ["SIGTERM"]);
  assert.equal(res.receipts.ydotoold.pid, f.ydPid);
  assert.ok(res.events.includes("cleanup-ydotoold-stop"), "input daemon stopped in cleanup");
  assert.ok(res.events.includes("verify-ydotoold-gone"), "input daemon absence verified");

  const f2 = makeFake();
  await runScenario(Object.assign(baseOpts(), { io: f2.io }));
  assert.ok(!f2.spawnCalls.some((c) => c.indexOf("ydotoold") === 0), "no ydotoold by default");
});

test("G: existing evidence dir is refused without mutation", async () => {
  const f = makeFake();
  let mkdirCalls = 0;
  const io = Object.assign({}, f.io, { mkdir: async () => { mkdirCalls++; } });
  const rc = await liveMain({
    argv: ["--live", "--evidence-dir", EVID],
    ioFactory: () => io, root: ROOT, templatePath: TEMPLATE,
  });
  assert.equal(rc, 2);
  assert.equal(mkdirCalls, 0);
  assert.equal(f.execCalls.length, 0, "no process boundary call before refusal");
});

// ---------------------------------------------------------------------------
// Order / mutation guards: broken variants must fail
// ---------------------------------------------------------------------------

test("ORDER: skipping readiness still fails (gate cannot be bypassed)", async () => {
  const f = makeFake({ fail: { readinessDrop: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.equal(res.scenario, "init");
  assert.ok(!res.events.includes("scenario-done"));
});

test("READINESS: each missing line kind fails before fixture spawn with cleanup", async () => {
  for (const part of ["no-ready", "no-discovered", "no-dispatch", "no-applied"]) {
    const f = makeFake({ fail: { readinessPart: part } });
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
    assert.equal(res.rc, 1, part);
    assert.match(res.divergence || "", /readiness gate/, part);
    assert.equal(res.scenario, "init", part);
    assert.ok(!res.events.includes("fixture-spawn"), part + ": stopped before fixture spawn");
    assert.ok(res.events.includes("cleanup"), part + ": cleanup ran");
    assert.ok(res.events.includes("verify-dev-down"), part + ": final verify ran");
  }
});

test("ORDER: serve via blocking exec is a violation, not a hang", async () => {
  const f = makeFake();
  await assert.rejects(
    f.io.exec(["cargo", "run", "--locked", "-p", "tiler-live-fixture", "--", "serve", "--dir", "/x"], {}),
    /spawnBackground/);
});

test("ORDER: helper run without load, and kill of unreceipted pid, are violations", async () => {
  const f = makeFake();
  await assert.rejects(
    f.io.exec(["busctl", "--user", "call", ":1.77", "/Scripting/Script999", "org.kde.kwin.Script", "run"], {}),
    /without load/);
  assert.throws(() => f.checkKill(424242), /unreceipted/);
});

test("ORDER: planner read before dev-on is a violation", async () => {
  const f = makeFake();
  await assert.rejects(f.io.exec(plannerReadCmd("kde-scen-x"), {}), /before dev-on/);
});

// Mutation checks: the RUNNER under a deliberately misordering io wrapper.
// Each wrapper performs one out-of-order mutation; the run must fail naming
// the violated guard (never silently absorb it).
test("MUTONCE: runner guard refuses a repeated mutating key", () => {
  const once = createMutOnce();
  once("workspace-append");
  assert.throws(() => once("workspace-append"), /refusing second mutating call/);
  once("other-key");
  assert.throws(() => once("other-key"), /refusing second mutating call/);
});

test("MUT: wrapper-issued second dev-on is refused (no relaunch)", async () => {
  const f = makeFake();
  const orig = f.io.exec;
  f.io.exec = async (argv, o) => {
    const s = argv.map(String).join(" ");
    if (s.indexOf("just dev-on") === 0) { await orig(argv, o); }
    return orig(argv, o);
  };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /relaunch/);
  assert.ok(!res.events.includes("scenario-done"));
  assert.equal(f.devOffCount, 1, "half-up start still gets exactly one dev-off");
});

test("MUT: wrapper-issued second fixture spawn is refused (no repeat)", async () => {
  const f = makeFake();
  const orig = f.io.exec;
  f.io.exec = async (argv, o) => {
    const s = argv.map(String).join(" ");
    if (s.indexOf("spawn-ab") >= 0) { await orig(argv, o); }
    return orig(argv, o);
  };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /repeated/);
  assert.ok(!res.events.includes("scenario-done"));
});

test("MUT: wrapper-issued close before activation is refused (ordering live)", async () => {
  const f = makeFake();
  const orig = f.io.exec;
  f.io.exec = async (argv, o) => {
    const s = argv.map(String).join(" ");
    if (s.indexOf("spawn-ab") >= 0) {
      const r = await orig(argv, o);
      const si = argv.indexOf("--socket");
      const closeArgv = fixtureSendCmd(String(argv[si + 1]), FIX_NONCE,
        ['{"v":1,"request_id":"close-b","op":"close","ids":["b"]}']);
      await orig(closeArgv, o); // throws: close before activation
      return r;
    }
    return orig(argv, o);
  };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /before activation/);
  assert.ok(!res.events.includes("scenario-done"));
});

test("MUT: wrapper-issued spawn before the appended workspace is current is refused", async () => {
  const f = makeFake();
  const orig = f.io.exec;
  let fired = false;
  f.io.exec = async (argv, o) => {
    const s = argv.map(String).join(" ");
    if (!fired && s.indexOf("isScriptLoaded") >= 0) {
      fired = true;
      const sock = f.fixture ? f.fixture.socket : EVID + "/fixture-run/fixture.sock";
      await orig(fixtureSendCmd(sock, FIX_NONCE,
        ['{"v":1,"request_id":"spawn-ab","op":"spawn","windows":[{"id":"a"},{"id":"b"}]}']), o);
    }
    return orig(argv, o);
  };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /before appended workspace/);
  assert.ok(!res.events.includes("scenario-done"));
});

test("PURE: validators, readiness correlation, presentation and window-info parsing", () => {
  assert.equal(parseArgs([]).live, false);
  assert.equal(parseArgs(["--live", "--evidence-dir", "/tmp/x"]).evidenceDir, "/tmp/x");
  assert.throws(() => parseArgs(["--bogus"]), /unknown argument/);
  assert.throws(() => checkEvidenceDir("rel", ROOT), /absolute/);
  assert.throws(() => checkEvidenceDir("/repo/x", "/repo"), /in-repo/);
  assert.equal(isOpaqueId("abc-1.2_3"), true);
  assert.equal(isOpaqueId("bad!"), false);
  assert.equal(isCanonicalU64("0"), true);
  assert.equal(isCanonicalU64("007"), false);
  assert.equal(isCanonicalU64("18446744073709551616"), false);
  assert.equal(normalizeNativeId(UUID_A), BARE(UUID_A));
  assert.equal(normalizeNativeId(BARE(UUID_A)), BARE(UUID_A));
  assert.equal(normalizeNativeId("!!!"), null);
  assert.equal(isCarriedRect([0, 0, 960, 1080]), true);
  assert.equal(isCarriedRect([0, 0, 0, 5]), false);
  assert.equal(parseDevStatusMode("dev mode: DOWN (x)"), "DOWN");
  const good = "omnitiler:world:ready gen=g9 source=local-dev\nomnitiler:world:discovered gen=g9\n" +
    "omnitiler:world:dispatch src=src-1-abc nwinv=1 cmd=observe\nomnitiler:world:applied rev=1 geom=2 member=2 views=0 mk=1 rm=0 fx=0 held=0";
  assert.equal(readinessPass(parseReadiness(good)), true);
  // Namespaces never intersect (ready source vs dispatch src), so a matching
  // production pair passes without any equality; each missing kind fails.
  const hexSrc = "omnitiler:world:ready gen=g9 source=" + "ab".repeat(20);
  assert.equal(readinessPass(parseReadiness(hexSrc + "\n" + good.split("\n").slice(1).join("\n"))), true);
  assert.equal(readinessPass(parseReadiness(good.split("\n").slice(1).join("\n"))), false);
  assert.equal(readinessPass(parseReadiness(good.split("\n").filter((l) => l.indexOf("discovered") < 0).join("\n"))), false);
  assert.equal(readinessPass(parseReadiness(good.split("\n").filter((l) => l.indexOf("dispatch") < 0).join("\n"))), false);
  assert.equal(readinessPass(parseReadiness(good.split("\n").slice(0, 3).join("\n"))), false);
  const req = presentationReq();
  const body = { v: 4, outcome: "presentation", gen: "g", req, rev: "1", obs: "2", windows: [{ win: "abc", out: "o", ws: "w", alloc: [0, 0, 10, 10] }] };
  const parsed = parsePresentation(J({ type: "s", data: [J(body)] }), req);
  assert.equal(parsed.rev, "1");
  assert.throws(() => parsePresentation(J(body), "other-req"), /correlation/);
  const wrappedInfo = (body) => J({ type: "a{sv}", data: [body] });
  const goodBody = {
    resourceClass: wrap("s", SCOPE_CLASS), caption: wrap("s", "c"), uuid: wrap("s", UUID_A),
    pid: wrap("i", 5), desktops: wrap("as", ["d"]),
    x: wrap("d", 1), y: wrap("d", 2), width: wrap("d", 3), height: wrap("d", 4),
  };
  const info = parseWindowInfo(wrappedInfo(goodBody), UUID_A);
  assert.deepEqual(info.frame, { x: 1, y: 2, w: 3, h: 4 });
  assert.deepEqual(info.desks, ["d"]);
  assert.equal(info.pid, 5);
  // Integer geometry is accepted too (signature i or d, finite number).
  const intBody = Object.assign({}, goodBody, { x: wrap("i", 1), width: wrap("i", 3) });
  assert.deepEqual(parseWindowInfo(wrappedInfo(intBody), UUID_A).frame, { x: 1, y: 2, w: 3, h: 4 });
  assert.throws(() => parseWindowInfo(J({ type: "a{sv}", data: [{}] }), UUID_A), /unknown window/);
  // The old bare-value shape is rejected fail-closed, never parsed.
  const bareBody = { resourceClass: SCOPE_CLASS, caption: "c", uuid: UUID_A, pid: 5, desktops: ["d"], x: 1, y: 2, width: 3, height: 4 };
  assert.throws(() => parseWindowInfo(J({ type: "a{sv}", data: [bareBody] }), UUID_A), /bare value/);
  assert.throws(() => parseWindowInfo(wrappedInfo(Object.assign({}, goodBody, { pid: wrap("s", "5") })), UUID_A), /bad signature/);
  assert.throws(() => parseWindowInfo(wrappedInfo(Object.assign({}, goodBody, { desktops: wrap("as", [7]) })), UUID_A), /bad desktops/);
  assert.deepEqual(frameOf([1, 2, 3, 4]), { x: 1, y: 2, w: 3, h: 4 });
  assert.equal(frameEqual({ x: 1, y: 2, w: 3, h: 4 }, { x: 1, y: 2, w: 3, h: 4 }), true);
  assert.equal(frameArea({ x: 0, y: 0, w: 4, h: 5 }), 20);
  assert.equal(frameInside({ x: 0, y: 0, w: 5, h: 5 }, OUT_GEOM), true);
  assert.deepEqual(devStatusCmd(), ["just", "dev-status"]);
  assert.deepEqual(devOffCmd(), ["just", "dev-off"]);
  assert.deepEqual(devOnCmd(), ["just", "dev-on", "--scope-class", SCOPE_CLASS]);
  assert.ok(fixtureServeCmd("/tmp/x/fixture-run").indexOf("serve") >= 0);
  assert.ok(fixtureSendCmd("s", FIX_NONCE, ["{}"]).indexOf("--compact") >= 0);
  assert.ok(fixtureSendCmd("s", FIX_NONCE, ["{}"]).indexOf("--quiet") < 0);
  const pr = plannerReadCmd("kde-scen-abc");
  assert.ok(pr.includes(PLANNER_BUS) && pr.includes(PLANNER_OBJECT) && pr.includes(PLANNER_IFACE) && pr.includes(PLANNER_METHOD));
  assert.equal(WORLD_CONTRACT_VERSION, 4);
  assert.equal(PLUGIN_ID, "omnitiler-kwin");
});
