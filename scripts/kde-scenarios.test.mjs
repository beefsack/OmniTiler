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
  WORLD_CONTRACT_VERSION, HELPER_PREFIX, T,
  ScenarioError, createMutOnce, parseArgs, helpText, checkEvidenceDir,
  isOpaqueId, isCanonicalU64, normalizeNativeId, isCarriedRect, parseDevStatusMode,
  frameOf, frameEqual, frameDiff, frameInside, frameArea, parseReadiness, readinessPass,
  parsePresentation, parseWindowInfo, devStatusCmd, devOffCmd, devOnCmd,
  fixtureServeCmd, fixtureSendCmd, ydotooldCmd, ydotoolKeyCmd, ydotoolBurstCmd, rapidBurstSequence,
  RAPID_BURST_LEN, RAPID_BURST_FAILURE_RE, STATE_CHORDS, presentationReq, plannerReadCmd,
  windowInfoCmd, runScenario, liveMain, parseProcStat, createLiveIo, RESIZE_STEP_PX, parseResizeRefusalTokens,
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
    burstStarted: false, burstFinalFrames: null,
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
    // Native state-target model (source-shaped defaults; the chord
    // interpreter below mutates these on injected ydotool key argv).
    for (const w of F.wins) {
      w.maxH = 0; w.maxV = 0; w.fs = false; w.above = false;
      w.floatSt = false; w.stickySt = false;
    }
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
      if (F.fail.restartDevOn && F.devOnCount === 2) { return { status: 1, stdout: "", stderr: "boom" }; }
      if (F.devOnCount > 1 && !F.allowRestart) { busErr("dev-on relaunch (second start)"); }
      if (F.devOnCount > 2) { busErr("dev-on third start"); }
      F.devUp = true;
      F.plannerPid = F.devOnCount === 1 ? 70317 : 70389;
      F.plannerTick = String(100000 + F.plannerPid);
      F.procs.set(F.plannerPid, { tick: F.plannerTick, alive: true, name: "planner" });
      const skipReady = F.fail.restartReadiness && F.devOnCount === 2;
      if (!skipReady) {
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
      }
      if (F.fail.stateFault === "restartStickyLost" && F.devOnCount === 2) {
        const A = F.wins.find((w) => w.internalId === UUID_A);
        if (A) { A.stickySt = false; A.floatSt = false; A.above = false; }
        F.applyStateModel();
      }
      if (F.fail.stateFault === "extraRestart" && F.devOnCount === 2 && !F.extraRestartAdded) {
        // A stray managed row appearing only after the restart: the shared
        // readAll exact-set ({A,B}) check must refuse it at restore.
        F.extraRestartAdded = true;
        F.wins.push(F.mkWin(UUID_X, SCOPE_CLASS, F.fixture ? F.fixture.pid : 70231,
          "OmniTilerTestWindow:x:tag9", { x: 0, y: 0, w: 100, h: 100 }, ["desk-new"]));
      }
      return { status: 0, stdout: "dev-on: planner pid " + F.plannerPid + ", script id 7\n", stderr: "" };
    }
    if (argv[0] === "just" && argv[1] === "dev-off") {
      F.devOffCount++;
      count("dev-off");
      if (F.devOffCount > 1 && !F.allowRestart) { busErr("dev-off repeated"); }
      if (F.devOffCount > 2) { busErr("dev-off third"); }
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
    if (argv[0] === "ydotool") {
      // Injected chord client: argv carries evdev key events
      // (CODE:1 press ... CODE:0 release); the socket rides YDOTOOL_SOCKET.
      if (argv[1] !== "key") { return { status: 1, stdout: "", stderr: "unknown ydotool subcommand" }; }
      count("ydotool-key");
      const seq = argv.slice(2).join(" ");
      if (!o || !o.env || o.env.YDOTOOL_SOCKET !== F.ydSock) {
        return { status: 1, stdout: "", stderr: "ydotool without the owned socket" };
      }
      if (F.activeUuid !== UUID_A && F.activeUuid !== UUID_B) {
        return { status: 1, stdout: "", stderr: "chord without an active fixture" };
      }
      // Rapid-burst: one ydotool invocation carrying all chords back to back.
      const burstChords = F.parseBurst(argv.slice(2));
      if (burstChords) {
        F.burstStarted = true;
        const err = F.applyBurst(burstChords);
        if (err) { return { status: 1, stdout: "", stderr: err }; }
        return { status: 0, stdout: "", stderr: "" };
      }
      const kinds = {
        "125:1 50:1 50:0 125:0": "maximize",
        "125:1 87:1 87:0 125:0": "fullscreen",
        "125:1 34:1 34:0 125:0": "float",
        "125:1 42:1 34:1 34:0 42:0 125:0": "sticky",
        "125:1 56:1 105:1 105:0 56:0 125:0": "resizeLeft",
        "125:1 56:1 106:1 106:0 56:0 125:0": "resizeRight",
      };
      const kind = kinds[seq];
      if (!kind) { return { status: 1, stdout: "", stderr: "unknown chord argv" }; }
      F.stateArmed = true;
      const err = F.toggleState(kind);
      if (err) { return { status: 1, stdout: "", stderr: err }; }
      return { status: 0, stdout: "", stderr: "" };
    }
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
        // Rapid-burst unknown fact: once the burst starts, B stops answering
        // getWindowInfo, so the final state can never PASS.
        if ((F.fail && F.fail.burstFault) === "unknownInfo" && F.burstStarted && uuid === UUID_B) {
          return { status: 0, stdout: J({ type: "a{sv}", data: [{}] }) + "\n", stderr: "" };
        }
        // Resize step-1 unknown fact: once the journey starts, B stops
        // answering getWindowInfo, so the step can never PASS.
        if (F.fail.resizeFault === "step1UnknownInfo" && F.resizeStarted && uuid === UUID_B) {
          return { status: 0, stdout: J({ type: "a{sv}", data: [{}] }) + "\n", stderr: "" };
        }
        const w = F.wins.find((x) => x.internalId === uuid);
        if (!w) { return { status: 0, stdout: J({ type: "a{sv}", data: [{}] }) + "\n", stderr: "" }; }
        F.winfoCount = (F.winfoCount || 0) + 1;
        const cls = F.fail.classMismatch ? "SomethingElse" : w.resourceClass;
        // Production getWindowInfo reports captionNormal, never the suffixed
        // caption: mirror that so the suffixed fake caption stays live.
        const cap = (typeof w.captionNormal === "string" && w.captionNormal) || w.caption;
        const fg = w.frameGeometry || { x: 0, y: 0, width: 0, height: 0 };
        // Real-shaped a{sv}: every value wrapped with its KWin-source
        // signature (strings s, desktops as, pid i, geometry d, state
        // targets: fullscreen/keepAbove b, maximize axes i).
        const desks = (F.fail.moveOffAppend && F.winfoCount > 2) ? ["d1"] : w.desktops.map((d) => d.id);
        const fsWrapped = (F.fail.stateFault === "unknownState" && F.stateArmed)
          ? wrap("s", "maybe")
          : wrap("b", !!w.fs);
        const infoData = {
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
          fullscreen: fsWrapped,
          keepAbove: wrap("b", !!w.above),
          maximizeHorizontal: wrap("i", w.maxH | 0),
          maximizeVertical: wrap("i", w.maxV | 0),
        };
        // D1 probe: a missing state key must read null (unknown, never PASS).
        if (F.fail.stateFault === "missingKeepAbove") { delete infoData.keepAbove; }
        return {
          status: 0,
          stdout: J({ type: "a{sv}", data: [infoData] }) + "\n",
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
        const aFloating = !!a && !!(a.floatSt || a.stickySt);
        const win = (w, alloc) => ({
          win: BARE(w.internalId), out: "out-1", ws: F.fail.splitWs ? (w.internalId === UUID_A ? "ws-1" : "ws-2") : "ws-1",
          alloc, frame: null, frame_auth: "controlled", frame_hold: "",
          member_auth: "controlled", member_hold: "",
          floating: w.internalId === UUID_A
            ? (F.fail.stateFault === "floatFlag" ? false : aFloating)
            : false,
          float_origin: "",
          cmdSeq: null, visibility: null, visibility_auth: "controlled", visibility_hold: "", state: [],
        });
        if (F.closedB) {
          const g = F.framePatch && F.framePatch.grown === false ? [0, 0, 960, 1080] : [0, 0, 1920, 1080];
          rows.push(win(a, F.allocPatch ? F.allocPatch("a1", g) : g));
        } else {
          let ga = [0, 0, 960, 1080];
          let gb = [960, 0, 960, 1080];
          if (aFloating && F.fail.stateFault !== "bNotGrown") { gb = [0, 0, 1920, 1080]; }
          if (!aFloating && F.afterFloatOff) {
            // Fresh-admission slot variants after float-off, in lockstep with
            // the native model above so native==alloc still passes.
            if (F.fail.stateFault === "swapFloatOff") { const t = ga; ga = gb; gb = t; }
            else if (F.fail.stateFault === "ordinaryDriftA") { ga = [0, 0, 100, 100]; }
            else if (F.fail.stateFault === "ordinarySameSlot") { gb = [0, 0, 960, 1080]; }
          }
          // Resize step 1 moves the shared boundary by one keyboard step in
          // both the native model and the published allocations (native faults
          // move only the native side, so the oracle names the divergence).
          if (F.resized12 && !F.closedB && F.fail.resizeFault !== "step1AllocMismatch") {
            ga = [0, 0, 948, 1080];
            gb = [948, 0, 972, 1080];
          }
          F.presentCount = (F.presentCount || 0) + 1;
          // First-divergence probes: a one-time bad allocation resolves on the
          // next poll (transient), while admitDrift keeps disagreeing with a
          // different allocation every later read (persistent).
          if (F.fail.flakyAdmitOnce && F.presentCount === 1) { gb = [0, 0, 10, 10]; }
          else if (F.fail.admitDrift) { gb = F.presentCount === 1 ? [0, 0, 10, 10] : [0, 0, 20, 20]; }
          const bf = (F.fail && F.fail.burstFault) || null;
          // Rapid-burst lockstep: the fake planner publishes the post-burst
          // snapshot, so settled native frames equal their allocations and
          // the model's slot prediction is observable. Faults target one
          // oracle each (unknown alloc, missing row).
          if (F.burstStarted && !F.closedB && F.burstFinalFrames) {
            const fa = F.burstFinalFrames.a, fb = F.burstFinalFrames.b;
            ga = [fa.x, fa.y, fa.width, fa.height];
            gb = [fb.x, fb.y, fb.width, fb.height];
            if (bf === "burstLag") {
              // Slow native convergence behind a synchronous journal: the 60
              // lines land at inject time, but the published effects settle
              // over several polls. Reads 1-2 already match the final model,
              // read 3 drifts (native != alloc), reads 4+ match. A
              // single-match pass would stop at read 1; the quiescence
              // streak must ride through the reset and settle only at the end.
              F.burstReads = (F.burstReads || 0) + 1;
              if (F.burstReads === 3) { gb = [fb.x, fb.y, fb.width + 1, fb.height]; }
            }
            if (bf === "lateDrift") {
              // Matches for fewer than the streak, then swaps the two tiles
              // permanently (native frames and published allocations move
              // together, so the slot oracle names the divergence): must
              // FAIL at the budget, never PASS early.
              F.burstReads = (F.burstReads || 0) + 1;
              if (F.burstReads > 2) {
                if (!F.lateSwapped) {
                  const wa = F.wins.find((w) => w.internalId === UUID_A);
                  const wb = F.wins.find((w) => w.internalId === UUID_B);
                  const g = wa.frameGeometry;
                  wa.frameGeometry = wb.frameGeometry;
                  wb.frameGeometry = g;
                  F.lateSwapped = true;
                }
                const wa = F.wins.find((w) => w.internalId === UUID_A);
                const wb = F.wins.find((w) => w.internalId === UUID_B);
                ga = [wa.frameGeometry.x, wa.frameGeometry.y, wa.frameGeometry.width, wa.frameGeometry.height];
                gb = [wb.frameGeometry.x, wb.frameGeometry.y, wb.frameGeometry.width, wb.frameGeometry.height];
              }
            }
          }
          if (bf === "unknownAlloc" && F.burstStarted && !F.closedB) { gb = null; }
          if (F.allocPatch) {
            rows.push(win(a, F.allocPatch("a0", ga)));
            if (!(bf === "missingRow" && F.burstStarted)) { rows.push(win(b, F.allocPatch("b0", gb))); }
          } else {
            rows.push(win(a, ga));
            if (!(bf === "missingRow" && F.burstStarted)) { rows.push(win(b, gb)); }
          }
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
        // A stray presentation row appearing only once the resize journey
        // starts (admission still sees exactly {A,B}): the exact-set oracle
        // must refuse it at the step.
        if (F.fail.resizeFault === "extraRow" && F.resizeStarted && !F.closedB) {
          rows.push({
            win: "zz9extra01", out: "out-1", ws: "ws-1",
            alloc: [0, 0, 100, 100], frame: null, frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "",
            floating: false, float_origin: "",
            cmdSeq: null, visibility: null, visibility_auth: "controlled", visibility_hold: "", state: [],
          });
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
    // Planner log boundary (read-only): readlink resolves the merged log,
    // stat sizes it, tail reads new bytes. Faults model unreadable logs.
    if (argv[0] === "readlink") {
      const rf = (F.fail && F.fail.resizeFault) || null;
      if (rf === "logReadlinkFail") { return { status: 1, stdout: "", stderr: "no such file" }; }
      if (rf === "logPathMismatch") { return { status: 0, stdout: "/tmp/wrong-path.log\n", stderr: "" }; }
      return { status: 0, stdout: F.plannerLogPath + "\n", stderr: "" };
    }
    if (argv[0] === "stat") {
      if ((F.fail && F.fail.resizeFault) === "logStatFail") { return { status: 1, stdout: "", stderr: "boom" }; }
      if (argv[argv.length - 1] !== F.plannerLogPath) { return { status: 1, stdout: "", stderr: "no such file" }; }
      return { status: 0, stdout: String(F.plannerLog.length) + "\n", stderr: "" };
    }
    if (argv[0] === "tail") {
      if (argv[argv.length - 1] !== F.plannerLogPath) { return { status: 1, stdout: "", stderr: "no such file" }; }
      const cArg = argv.filter((a) => a.indexOf("+") === 0)[0] || "+1";
      const off = Number(cArg.slice(1)) - 1;
      return { status: 0, stdout: F.plannerLog.slice(off), stderr: "" };
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
      F.ydSock = sock;
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
      // the runner's post-rm existence check stays live. ydRmFail models a
      // ydotool socket the runner cannot unlink, so the fallback reports.
      if (F.leave.helperSrc && String(p).indexOf("/helper-") >= 0) { return; }
      if (F.fail.ydRmFail && String(p).endsWith("ydotool.sock")) {
        throw Object.assign(new Error("EACCES: rm " + p), { code: "EACCES" });
      }
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
  // State-target model: interpret injected ydotool chord argv and mutate the
  // fake window model (maximize, fullscreen, keepAbove/float, sticky, with B
  // tile reflow). Shapes mirror KWin v6.8.1 clientToVariantMap (maximize axes
  // int 0/nonzero with the real MaximizeVertical=1/MaximizeHorizontal=2
  // values, fullscreen/keepAbove bool). Faults via F.fail.stateFault target
  // one oracle each (see the ST tests).
  F.maxToggles = 0;
  F.stateArmed = false;
  F.applyStateModel = () => {
    const A = F.wins.find((w) => w.internalId === UUID_A);
    const B = F.wins.find((w) => w.internalId === UUID_B);
    if (!A) { return; }
    if (A.fs || (A.maxH && A.maxV)) {
      A.frameGeometry = { x: 0, y: 0, width: OUT_GEOM.w, height: OUT_GEOM.h };
    } else {
      A.frameGeometry = { x: 0, y: 0, width: 960, height: 1080 };
    }
    if (F.fail.stateFault === "fsFrame" && A.fs) {
      A.frameGeometry = { x: 0, y: 0, width: OUT_GEOM.w - 1, height: OUT_GEOM.h };
    }
    if (F.fail.stateFault === "maxoffFrame" && !A.maxH && !A.maxV && F.maxToggles >= 2) {
      A.frameGeometry = { x: 0, y: 0, width: 961, height: 1080 };
    }
    if (F.fail.stateFault === "fsOffFrame" && F.fsExited && !A.fs) {
      // Frame stuck outside its output after the fullscreen exit: x is
      // negative, so frameInside fails while fullscreen/maximize read back.
      A.frameGeometry = { x: -40, y: 0, width: OUT_GEOM.w, height: OUT_GEOM.h };
    }
    if (F.afterFloatOff && !A.fs && !(A.maxH && A.maxV) && !(A.floatSt || A.stickySt)) {
      // Fresh-admission slot variants after float-off: the planner hook below
      // moves the published allocations in lockstep, so the native==alloc
      // checks still pass and the oracle's set-equality branch is exercised.
      if (F.fail.stateFault === "swapFloatOff") {
        A.frameGeometry = { x: 960, y: 0, width: 960, height: 1080 };
      } else if (F.fail.stateFault === "ordinaryDriftA") {
        A.frameGeometry = { x: 0, y: 0, width: 100, height: 100 };
      }
    }
    if (F.fail.stateFault === "stickyDesks") { /* leave desks alone */ }
    else if (F.fail.stateFault === "stickyTwoDesks" && A.stickySt) {
      // Two-homed while sticky: a sticky window must report an empty
      // desktops list, so two entries (even including the appended
      // workspace) must fail the sticky oracle.
      A.desktops = [{ id: "desk-new" }, { id: "d1" }];
    }
    else {
      A.desktops = A.stickySt ? [] : [{ id: "desk-new" }];
    }
    if (B) {
      const grown = (A.floatSt || A.stickySt) && F.fail.stateFault !== "bNotGrown";
      B.frameGeometry = grown
        ? { x: 0, y: 0, width: OUT_GEOM.w, height: OUT_GEOM.h }
        : { x: 960, y: 0, width: 960, height: 1080 };
      if (!grown && F.afterFloatOff &&
        (F.fail.stateFault === "swapFloatOff" || F.fail.stateFault === "ordinarySameSlot")) {
        B.frameGeometry = { x: 0, y: 0, width: 960, height: 1080 };
      }
    }
  };
  F.toggleState = (kind) => {
    const A = F.wins.find((w) => w.internalId === UUID_A);
    if (!A) { return "no fixture A to drive"; }
    if (kind === "maximize") {
      F.maxToggles++;
      if (F.maxToggles % 2 === 1) {
        if (F.fail.stateFault === "maxAxis") { A.maxH = 2; A.maxV = 0; }
        else { A.maxH = 2; A.maxV = 1; }
      } else { A.maxH = 0; A.maxV = 0; }
    } else if (kind === "fullscreen") {
      A.fs = !A.fs;
      if (F.fail.stateFault === "fsOffFrame" && !A.fs) { F.fsExited = true; }
    } else if (kind === "float") {
      A.floatSt = !A.floatSt;
      if (!A.floatSt && (F.fail.stateFault === "swapFloatOff" ||
        F.fail.stateFault === "ordinaryDriftA" || F.fail.stateFault === "ordinarySameSlot")) {
        F.afterFloatOff = true;
      }
    } else if (kind === "sticky") {
      A.stickySt = !A.stickySt;
    } else if (kind === "resizeLeft" || kind === "resizeRight") {
      F.resizeStarted = true;
      const rf = (F.fail && F.fail.resizeFault) || null;
      const B = F.wins.find((w) => w.internalId === UUID_B);
      if (kind === "resizeLeft" && !F.closedB) {
        // Step 1: B grows toward the shared boundary by one keyboard step.
        const dx = rf === "step1Delta11" ? 11 : 12;
        if (A) { A.frameGeometry = { x: 0, y: 0, width: 960 - dx, height: 1080 }; }
        if (B) { B.frameGeometry = { x: 960 - dx, y: 0, width: 960 + dx, height: 1080 }; }
        F.resized12 = true;
        if (rf === "step1SpuriousRefusal") {
          F.plannerLog += F.makeRefusalLine({ dir: "left", mode: "outwards", internal: "unchanged", wire: "operation-refused", rowsTotal: "2", rowsShown: "2" }) + "\n";
        }
      } else if (kind === "resizeRight" && !F.closedB) {
        // Step 2: outer edge, no neighbour: refused, frames stay at step 1.
        if (rf !== "step2NoRefusal") {
          const n = rf === "step2TwoLines" ? 2 : 1;
          const internal = rf === "step2WrongKind" ? "geometry" : "unchanged";
          let subject, press = "0", rowsTotal = "2", rowsShown = "2";
          if (rf === "step2SubjectA") { subject = BARE(UUID_A); }
          else if (rf === "step2SubjectForeign") { subject = BARE(UUID_X); }
          else if (rf === "step2Press1") { press = "1"; }
          else if (rf === "step2Rows1") { rowsTotal = "1"; rowsShown = "1"; }
          for (let i = 0; i < n; i++) {
            F.plannerLog += F.makeRefusalLine({ subject, dir: "right", mode: "outwards", internal, wire: "operation-refused", press, rowsTotal, rowsShown }) + "\n";
          }
        }
        if (rf === "step2FramesMoved" && B && B.frameGeometry) {
          B.frameGeometry = { x: B.frameGeometry.x + 1, y: 0, width: B.frameGeometry.width, height: 1080 };
        }
      } else if (kind === "resizeLeft" && F.closedB) {
        // Step 3: singleton A grows toward its outer edge: refused.
        if (rf !== "step3RefusalMissing") {
          let subject = BARE(UUID_A), rowsTotal = "1", rowsShown = "1";
          if (rf === "step3SubjectB") { subject = BARE(UUID_B); }
          else if (rf === "step3Rows2") { rowsTotal = "2"; rowsShown = "2"; }
          F.plannerLog += F.makeRefusalLine({ subject, dir: "left", mode: "outwards", internal: "unchanged", wire: "operation-refused", press: "0", rowsTotal, rowsShown }) + "\n";
        }
        if (rf === "step3FrameChanged" && A && A.frameGeometry) {
          A.frameGeometry = { x: 0, y: 0, width: 1919, height: 1080 };
        }
      } else {
        return "unknown resize context";
      }
      // The state-target model below would reset the resized geometry, and
      // resize never touches keepAbove: return with the resize geometry kept.
      return null;
    } else {
      return "unknown state kind";
    }
    A.above = F.fail.stateFault === "keepAbove" ? false : !!(A.floatSt || A.stickySt);
    F.applyStateModel();
    return null;
  };
  // Rapid-burst model: segment a concatenated ydotool key argv into chords
  // (press in order, release in reverse per chord) and apply the two-tile
  // row model to the fake native focus/frames, emitting one
  // `omnitiler:world:command ... label=... depth=...` journal line per chord
  // into the fake KWin journal (read back through the same journalctl
  // argv). Only multi-chord bursts route here; single chords keep the
  // single-chord path above (unknown there stays an error).
  const BURST_CHORDS = { "125,105": 1, "125,106": 1, "125,42,105": 1, "125,42,106": 1 };
  F.parseBurst = (tokens) => {
    const chords = [];
    let stack = [], presses = [];
    for (const t of tokens) {
      const m = /^(\d+):([01])$/.exec(t);
      if (!m) { return null; }
      const code = Number(m[1]);
      if (m[2] === "1") { stack.push(code); presses.push(code); }
      else {
        if (!stack.length || stack.pop() !== code) { return null; }
        if (!stack.length) {
          if (!BURST_CHORDS[presses.join(",")]) { return null; }
          chords.push(presses);
          presses = [];
        }
      }
    };
    if (stack.length || presses.length || chords.length < 2) { return null; }
    return chords;
  };
  F.applyBurst = (chords) => {
    const kindOf = (codes) => {
      const k = codes.join(",");
      if (k === "125,105") { return { kind: "focus", dir: "left" }; }
      if (k === "125,106") { return { kind: "focus", dir: "right" }; }
      if (k === "125,42,105") { return { kind: "move", dir: "left" }; }
      if (k === "125,42,106") { return { kind: "move", dir: "right" }; }
      return null;
    };
    const byId = (id) => F.wins.find((w) => w.internalId === id);
    const leftId = () => {
      const A = byId(UUID_A), B = byId(UUID_B);
      return A.frameGeometry.x <= B.frameGeometry.x ? UUID_A : UUID_B;
    };
    const fault = (F.fail && F.fail.burstFault) || null;
    const lines = [];
    let moveSeen = 0;
    chords.forEach((codes, ix) => {
      const kd = kindOf(codes);
      const op = kd.kind === "focus" ? "focus-directional" : "move-directional";
      const label = kd.kind + "-" + kd.dir;
      if (!(fault === "dropOne" && ix === 5)) {
        lines.push("js: omnitiler:world:command op=" + op + " label=" + label + " nwinv=0 depth=" + ix);
      }
      // Model effect mirrors the runner: focus flips the focused position
      // (the named slot gains focus); move swaps the two fixtures' slots and
      // the moved (focused) fixture keeps focus (active id unchanged).
      if (kd.kind === "focus") {
        F.activeUuid = kd.dir === "left" ? leftId() : (leftId() === UUID_A ? UUID_B : UUID_A);
      } else {
        const A = byId(UUID_A), B = byId(UUID_B);
        if (!(fault === "lostMove" && moveSeen === 0)) {
          const g = A.frameGeometry;
          A.frameGeometry = B.frameGeometry;
          B.frameGeometry = g;
        }
        moveSeen++;
      }
    });
    if (fault === "reorder" && lines.length > 12) {
      const t = lines[10];
      lines[10] = lines[11];
      lines[11] = t;
    }
    for (const l of lines) { F.journalPush(F.kwinPid, l); }
    if (fault === "queueTimeout") {
      F.journalPush(F.kwinPid, "js: omnitiler:world:queue-timeout op=focus-directional nwinv=0");
    }
    if (fault === "refused") {
      F.journalPush(F.kwinPid, "js: omnitiler:world:command-refused op=focus-directional reason=cmd-exhausted");
    }
    if (fault === "focusWrong") {
      F.activeUuid = F.activeUuid === UUID_A ? UUID_B : UUID_A;
    }
    // Snapshot the post-burst geometry first: the fake planner derives the
    // published allocations from this snapshot (lockstep), so a later native
    // drift is observable as native != alloc.
    const A = byId(UUID_A), B = byId(UUID_B);
    F.burstFinalFrames = {
      a: { x: A.frameGeometry.x, y: A.frameGeometry.y, width: A.frameGeometry.width, height: A.frameGeometry.height },
      b: { x: B.frameGeometry.x, y: B.frameGeometry.y, width: B.frameGeometry.width, height: B.frameGeometry.height },
    };
    if (fault === "frameDrift" && B && B.frameGeometry) {
      B.frameGeometry = { x: B.frameGeometry.x, y: 0, width: B.frameGeometry.width + 1, height: 1080 };
    }
    if (fault === "extraRow") {
      F.wins.push(F.mkWin(UUID_X, SCOPE_CLASS, F.fixture.pid, "OmniTilerTestWindow:x:tag9",
        { x: 0, y: 0, w: 100, h: 100 }, ["desk-new"]));
    }
    return null;
  };
  // boundary (readlink/stat/tail), refusal lines, and the 12px boundary
  // move. Faults via F.fail.resizeFault target one oracle each.
  F.plannerLogPath = "/tmp/omnitiler-planner-dev.a1B2c3D4.log";
  F.plannerLog = "";
  F.resized12 = false;
  F.resizeStarted = false;
  F.makeRefusalLine = (o) => (
    "omnitiler:resize-refused req=r1 reqseq=1 obsseq=2 cmdseq=3 op=resize subject=" +
    (o.subject || BARE(UUID_B)) + " dir=" + o.dir + " mode=" + o.mode + " press=" + (o.press || "0") +
    " internal=" + o.internal + " wire=" + o.wire + " focused=f1 homing=out-1/ws-1 focus=f1 tree=root" +
    " subjgeom=948,0,972x1080 subjhints=- rowsTotal=" + o.rowsTotal + " rowsShown=" + o.rowsShown +
    " rows=[a@0,0,948x1080/- b@948,0,972x1080/-]"
  );
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

test("YDSOCK: ydotoold socket left by the daemon is unlinked by the runner and counted", async () => {
  const f = makeFake({ leave: { ydSock: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, cf: res.cleanupFailed }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
  assert.deepEqual(f.kills.filter((k) => k[0] === f.ydPid).map((k) => k[1]), ["SIGTERM"]);
  assert.equal(res.counts.ydotooldSocketUnlinkedByRunner, 1);
  assert.ok(!(await f.io.exists(EVID + "/ydotool.sock")));
});

test("YDSOCK: daemon-unlinked socket passes cleanup with no runner count", async () => {
  const f = makeFake();
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, cf: res.cleanupFailed }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
  assert.equal(res.counts.ydotooldSocketUnlinkedByRunner, undefined);
});

test("YDSOCK: runner rm failure on the leftover socket is a cleanup failure", async () => {
  const f = makeFake({ leave: { ydSock: true }, fail: { ydRmFail: true } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true }));
  assert.equal(res.rc, 1);
  assert.equal(res.counts.ydotooldSocketUnlinkedByRunner, undefined);
  assert.ok(res.cleanupFailed.some((m) => m.indexOf("ydotoold-stop") >= 0 &&
    (m.indexOf("rm failed") >= 0 || m.indexOf("still present") >= 0)), J(res.cleanupFailed));
});

// ---------------------------------------------------------------------------
// ST. state-targets scenario (chord journey with an orderly restart)
// ---------------------------------------------------------------------------

test("ST-CLI: scenario flag defaults, parses, and fails closed before io", async () => {
  assert.equal(parseArgs([]).scenario, "tiling-reflow");
  assert.equal(parseArgs(["--scenario", "state-targets"]).scenario, "state-targets");
  assert.equal(parseArgs(["--scenario=state-targets"]).scenario, "state-targets");
  assert.ok(helpText().indexOf("--scenario") >= 0, "help documents --scenario");
  assert.throws(() => parseArgs(["--scenario", "bogus"]), /unknown --scenario/);
  assert.throws(() => parseArgs(["--scenario=state-targets", "--scenario=tiling-reflow"]), /duplicate --scenario/);
  const noIo = () => { throw new Error("io must not be built"); };
  // Unknown scenario exits 2 before io.
  assert.equal(await liveMain({
    argv: ["--live", "--evidence-dir", "/tmp/kde-st-bad-1", "--scenario", "bogus"],
    ioFactory: noIo, root: ROOT, templatePath: TEMPLATE,
  }), 2);
  // state-targets without --input exits 2 before ioFactory.
  assert.equal(await liveMain({
    argv: ["--live", "--evidence-dir", "/tmp/kde-st-noinput-1", "--scenario", "state-targets"],
    ioFactory: noIo, root: ROOT, templatePath: TEMPLATE,
  }), 2);
  // Direct runScenario without input rejects before any process boundary call.
  const f = makeFake();
  await assert.rejects(
    runScenario(Object.assign(baseOpts(), { io: f.io, scenario: "state-targets" })),
    /state-targets requires --input/);
  assert.equal(f.execCalls.length, 0, "no exec before the input guard");
});

test("ST-BUILDER: exact chord argv, keycodes, and closed construction", () => {
  assert.deepEqual(STATE_CHORDS, {
    maximize: [125, 50], fullscreen: [125, 87], float: [125, 34], sticky: [125, 42, 34],
    resizeLeft: [125, 56, 105], resizeRight: [125, 56, 106],
    focusLeft: [125, 105], focusRight: [125, 106], moveLeft: [125, 42, 105], moveRight: [125, 42, 106],
  });
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "maximize"),
    ["ydotool", "key", "125:1", "50:1", "50:0", "125:0"]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "fullscreen"),
    ["ydotool", "key", "125:1", "87:1", "87:0", "125:0"]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "float"),
    ["ydotool", "key", "125:1", "34:1", "34:0", "125:0"]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "sticky"),
    ["ydotool", "key", "125:1", "42:1", "34:1", "34:0", "42:0", "125:0"]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "resizeLeft"),
    ["ydotool", "key", "125:1", "56:1", "105:1", "105:0", "56:0", "125:0"]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "resizeRight"),
    ["ydotool", "key", "125:1", "56:1", "106:1", "106:0", "56:0", "125:0"]);
  assert.throws(() => ydotoolKeyCmd("/tmp/x.sock", "bogus"), /unknown chord/);
  assert.throws(() => ydotoolKeyCmd("rel.sock", "maximize"), /absolute/);
});

test("ST-PASS: chord journey with restart passes all 16 state checks", async () => {
  const f = makeFake({});
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, checks: res.checks }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
  assert.equal(res.scenario, "done");
  const seq = ["preflight", "kwin-identity", "fixture-serve", "ydotoold-start", "journal-cursor",
    "workspace-read", "workspace-append", "workspace-reread", "workspace-setcurrent",
    "dev-on", "readiness", "fixture-spawn", "native-read", "join", "activate",
    "scenario-state-targets",
    "state-a-maximize-on", "state-b-fullscreen-on", "state-c-fullscreen-off", "state-d-maximize-off",
    "state-e-float-on", "state-f-float-off", "state-g-sticky-on", "state-h-sticky-off",
    "state-i-sticky-on", "state-restart", "readiness-restart", "state-i-restart-restored",
    "state-i-sticky-off", "scenario-done", "cleanup",
    "cleanup-fixture-shutdown", "cleanup-workspace-restore", "cleanup-dev-off",
    "cleanup-workspace-remove", "cleanup-ydotoold-stop",
    "verify-fixture-pid-gone", "verify-cargo-pid-gone",
    "verify-fixture-socket-absent", "verify-rundir-removed", "verify-planner-pid-gone",
    "verify-ydotoold-gone", "verify-current-restored", "verify-appended-absent", "verify-dev-down"];
  assert.deepEqual(res.events, seq);
  const stateChecks = res.checks.filter((c) => c.name.indexOf("state-") === 0);
  assert.equal(stateChecks.length, 16, J(stateChecks.map((c) => c.name)));
  assert.ok(stateChecks.every((c) => c.pass), J(stateChecks));
  // Chord multiset: maximize 2, fullscreen 2, float 2, sticky 4 (10 injections).
  const keys = f.execCalls.filter((c) => c.indexOf("ydotool key") === 0);
  assert.equal(keys.length, 10, J(keys));
  const count = (frag) => keys.filter((c) => c.indexOf(frag) >= 0).length;
  assert.equal(count("125:1 50:1 50:0 125:0"), 2);
  assert.equal(count("125:1 87:1 87:0 125:0"), 2);
  assert.equal(count("125:1 34:1 34:0 125:0"), 2);
  assert.equal(count("125:1 42:1 34:1 34:0 42:0 125:0"), 4);
  assert.equal(f.devOnCount, 2);
  assert.equal(f.devOffCount, 2);
  assert.equal(res.counts.restart, 1);
  assert.equal(res.receipts.planner.pid, 70317);
  assert.equal(res.receipts.plannerRestart.pid, 70389);
  assert.deepEqual(res.receipts.stateBase, {
    a: { x: 0, y: 0, w: 960, h: 1080 }, b: { x: 960, y: 0, w: 960, h: 1080 },
  });
  const blob = J(res) +
    [...f.files.values()].map((x) => String(x.data)).join("\n");
  assert.ok(!blob.includes(FIX_NONCE), "fixture nonce leaked");
  assert.ok(!/helper-[0-9a-f]{32}/.test(J(res)), "helper nonce leaked into evidence");
  assert.ok(!blob.includes("Foreign Files"), "foreign caption leaked");
  assert.ok(!blob.includes("org.kde.dolphin"), "foreign class leaked");
  assert.equal(res.counts.foreign, 1);
});

test("ST-NEG: maximize axis fault fails closed at a-maximize-on", async () => {
  const f = makeFake({ fail: { stateFault: "maxAxis" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /state-a-maximize-on/);
  assert.match(res.divergence || "", /maximize axes not both true/);
  assert.ok(!res.events.includes("scenario-done"));
});

test("ST-NEG: fullscreen frame fault fails at b-fullscreen-on with one chord", async () => {
  const f = makeFake({ fail: { stateFault: "fsFrame" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /state-b-fullscreen-on/);
  assert.match(res.divergence || "", /output rectangle/);
  const keys = f.execCalls.filter((c) => c.indexOf("ydotool key 125:1 87:1") === 0);
  assert.equal(keys.length, 1, "failing chord is never retried: " + J(keys));
});

test("ST-NEG: maximize-off frame fault fails at d-maximize-off", async () => {
  const f = makeFake({ fail: { stateFault: "maxoffFrame" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /state-d-maximize-off/);
  assert.match(res.divergence || "", /!= base/);
});

test("ST-NEG: float faults fail closed at e-float-on", async () => {
  for (const [fault, oracle] of [
    ["keepAbove", /keepAbove not true/],
    ["floatFlag", /floating not true/],
    ["bNotGrown", /not larger than base/],
  ]) {
    const f = makeFake({ fail: { stateFault: fault } });
    f.allowRestart = true;
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
    assert.equal(res.rc, 1, fault);
    assert.match(res.divergence || "", /state-e-float-on/, fault);
    assert.match(res.divergence || "", oracle, fault);
  }
});

test("ST-SWAP: float-off fresh admission in either order passes all 16 state checks", async () => {
  const f = makeFake({ fail: { stateFault: "swapFloatOff" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, checks: res.checks }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
  assert.equal(res.scenario, "done");
  const stateChecks = res.checks.filter((c) => c.name.indexOf("state-") === 0);
  assert.equal(stateChecks.length, 16, J(stateChecks.map((c) => c.name)));
  assert.ok(stateChecks.every((c) => c.pass), J(stateChecks));
  const slots = res.receipts.stateSlotOrder || [];
  const fOff = slots.filter((e) => e.step === "f-float-off");
  assert.equal(fOff.length, 1, J(slots));
  assert.equal(fOff[0].aSlot, "b-base", J(slots));
});

test("ST-NEG: float-off drift fails at f-float-off naming a", async () => {
  const f = makeFake({ fail: { stateFault: "ordinaryDriftA" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /state-f-float-off/);
  assert.match(res.divergence || "", /a native frame equals neither base allocation/);
});

test("ST-NEG: float-off same slot fails at f-float-off", async () => {
  const f = makeFake({ fail: { stateFault: "ordinarySameSlot" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /state-f-float-off/);
  assert.match(res.divergence || "", /same base allocation/);
});

test("ST-NEG: sticky desks fault fails at g-sticky-on", async () => {
  const f = makeFake({ fail: { stateFault: "stickyDesks" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /state-g-sticky-on/);
  assert.match(res.divergence || "", /not all-desktop/);
});

test("ST-NEG: two-homed A after sticky-on fails at g-sticky-on (sticky means empty)", async () => {
  const f = makeFake({ fail: { stateFault: "stickyTwoDesks" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /state-g-sticky-on/);
  assert.match(res.divergence || "", /not all-desktop/);
});

test("ST-NEG: fullscreen-off frame fault fails at c-fullscreen-off", async () => {
  const f = makeFake({ fail: { stateFault: "fsOffFrame" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /state-c-fullscreen-off/);
  assert.match(res.divergence || "", /not inside its output/);
});

test("ST-NEG: restart losing sticky fails at i-restart-restored", async () => {
  const f = makeFake({ fail: { stateFault: "restartStickyLost" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /state-i-restart-restored/);
  const down = res.checks.filter((c) => c.name === "cleanup-dev-down")[0];
  assert.ok(down && down.pass, "cleanup still ends DOWN: " + J(res.checks));
});

test("ST-NEG: extra presentation row after restart fails at i-restart-restored", async () => {
  const f = makeFake({ fail: { stateFault: "extraRestart" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /state-i-restart-restored/);
  assert.match(res.divergence || "", /not exactly \{A,B\}/);
});

test("ST-NEG: restart readiness gap fails with dev-down cleanup", async () => {
  const f = makeFake({ fail: { restartReadiness: true } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /restart readiness gate missing/);
  assert.equal(f.devOnCount, 2, "both starts ran before the gate failed");
  const down = res.checks.filter((c) => c.name === "cleanup-dev-down")[0];
  assert.ok(down && down.pass, "cleanup still ends DOWN: " + J(res.checks));
});

test("ST-NEG: restart dev-on failure fails with dev-down cleanup", async () => {
  const f = makeFake({ fail: { restartDevOn: true } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.match(res.divergence || "", /restart dev-on failed/);
  const down = res.checks.filter((c) => c.name === "cleanup-dev-down")[0];
  assert.ok(down && down.pass, "cleanup still ends DOWN: " + J(res.checks));
});

test("ST-NEG: unknown native state is PENDING at a-maximize-on", async () => {
  const f = makeFake({ fail: { stateFault: "unknownState" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "PENDING");
  assert.match(res.divergence || "", /state-a-maximize-on/);
});

test("ST-NEG: missing keepAbove key is PENDING at e-float-on (D1)", async () => {
  const f = makeFake({ fail: { stateFault: "missingKeepAbove" } });
  f.allowRestart = true;
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "state-targets" }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "PENDING", J({ div: res.divergence }));
  assert.match(res.divergence || "", /state-e-float-on/);
});

test("ST-ONCE: runner guard refuses a repeated chord key", () => {
  const once = createMutOnce();
  once("chord-a-maximize-on");
  assert.throws(() => once("chord-a-maximize-on"), /refusing second mutating call/);
});

test("ST-SHAPES: state-key signatures match the captured KWin-derived shapes", async () => {
  assert.deepEqual(SHAPES.getWindowInfoStateKeys.signatures, {
    fullscreen: "b", keepAbove: "b", maximizeHorizontal: "i", maximizeVertical: "i", desktops: "as",
  });
  const f = makeFake();
  const knownR = JSON.parse((await f.io.exec(windowInfoCmd(UUID_A), {})).stdout);
  const body = knownR.data[0];
  for (const k of Object.keys(SHAPES.getWindowInfoStateKeys.signatures)) {
    assert.equal(body[k].type, SHAPES.getWindowInfoStateKeys.signatures[k], "signature: " + k);
  }
  assert.equal(body.maximizeHorizontal.data, 0);
  assert.equal(body.maximizeVertical.data, 0);
});

// ---------------------------------------------------------------------------
// RR. resize-refusal scenario (keyboard grow-towards-direction with planner
// refusal-line oracles)
// ---------------------------------------------------------------------------

test("RR-CLI: resize-refusal parses, is documented, and requires --input before io", async () => {
  assert.equal(parseArgs(["--scenario", "resize-refusal"]).scenario, "resize-refusal");
  assert.equal(parseArgs(["--scenario=resize-refusal"]).scenario, "resize-refusal");
  assert.ok(helpText().indexOf("resize-refusal") >= 0, "help documents resize-refusal");
  assert.throws(() => parseArgs(["--scenario=resize-refusal", "--scenario=tiling-reflow"]), /duplicate --scenario/);
  const noIo = () => { throw new Error("io must not be built"); };
  assert.equal(await liveMain({
    argv: ["--live", "--evidence-dir", "/tmp/kde-rr-noinput-1", "--scenario", "resize-refusal"],
    ioFactory: noIo, root: ROOT, templatePath: TEMPLATE,
  }), 2);
  const f = makeFake();
  await assert.rejects(
    runScenario(Object.assign(baseOpts(), { io: f.io, scenario: "resize-refusal" })),
    /resize-refusal requires --input/);
  assert.equal(f.execCalls.length, 0, "no exec before the input guard");
});

test("RR-BUILDER: resize chord keycodes and exact argv", () => {
  assert.equal(RESIZE_STEP_PX, 12);
  assert.deepEqual(STATE_CHORDS.resizeLeft, [125, 56, 105]);
  assert.deepEqual(STATE_CHORDS.resizeRight, [125, 56, 106]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "resizeLeft"),
    ["ydotool", "key", "125:1", "56:1", "105:1", "105:0", "56:0", "125:0"]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "resizeRight"),
    ["ydotool", "key", "125:1", "56:1", "106:1", "106:0", "56:0", "125:0"]);
});

test("RR-TOKENS: refusal token parser keeps only the bounded vocabulary", () => {
  const line = "omnitiler:resize-refused req=r1 reqseq=1 obsseq=2 cmdseq=3 op=resize subject=deadbeef " +
    "dir=right mode=outwards press=0 internal=unchanged wire=operation-refused focused=f1 homing=o/w " +
    "focus=f1 tree=root subjgeom=1,2,3x4 subjhints=- rowsTotal=2 rowsShown=2 rows=[a b]";
  const tok = parseResizeRefusalTokens(line);
  assert.deepEqual(tok, {
    internal: "unchanged", wire: "operation-refused", dir: "right", mode: "outwards",
    press: "0", rowsTotal: "2", rowsShown: "2", tree: "root", subject: "deadbeef",
  });
  assert.equal(parseResizeRefusalTokens("no refusal here").internal, undefined);
});

test("RR-PASS: grow/refuse/close/refuse journey passes with exact events and checks", async () => {
  const f = makeFake({});
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "resize-refusal" }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, checks: res.checks }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
  assert.equal(res.scenario, "done");
  assert.equal(res.scenarioRan, "resize-refusal");
  const seq = ["preflight", "kwin-identity", "fixture-serve", "ydotoold-start", "journal-cursor",
    "workspace-read", "workspace-append", "workspace-reread", "workspace-setcurrent",
    "dev-on", "readiness", "fixture-spawn", "native-read", "join", "activate",
    "scenario-resize-refusal",
    "resize-step1", "resize-step2", "resize-close-b", "resize-step3",
    "scenario-done", "cleanup",
    "cleanup-fixture-shutdown", "cleanup-workspace-restore", "cleanup-dev-off",
    "cleanup-workspace-remove", "cleanup-ydotoold-stop",
    "verify-fixture-pid-gone", "verify-cargo-pid-gone",
    "verify-fixture-socket-absent", "verify-rundir-removed", "verify-planner-pid-gone",
    "verify-ydotoold-gone", "verify-current-restored", "verify-appended-absent", "verify-dev-down"];
  assert.deepEqual(res.events, seq);
  const names = res.checks.map((c) => c.name);
  for (const n of ["resize-admit-a-frame-equals-alloc", "resize-admit-b-frame-equals-alloc",
    "resize-admit-disjoint", "resize-admit-inside-output", "resize-admit-same-ws",
    "resize-step1", "resize-step2",
    "reflow-b-gone-from-presentation", "reflow-b-absent-native",
    "reflow-a-alloc-changed-larger", "reflow-a-frame-equals-alloc", "reflow-a-inside-output",
    "resize-step3"]) {
    assert.ok(names.includes(n), "missing check " + n + ": " + J(names));
  }
  assert.ok(res.checks.every((c) => c.pass), J(res.checks));
  // Chord multiset: resizeLeft 2 (step1, step3), resizeRight 1 (step2).
  const keys = f.execCalls.filter((c) => c.indexOf("ydotool key") === 0);
  assert.equal(keys.length, 3, J(keys));
  assert.equal(keys.filter((c) => c.indexOf("125:1 56:1 105:1 105:0 56:0 125:0") >= 0).length, 2);
  assert.equal(keys.filter((c) => c.indexOf("125:1 56:1 106:1 106:0 56:0 125:0") >= 0).length, 1);
  // Exact one-press geometry.
  assert.deepEqual(res.receipts.resizeBase, {
    a: { x: 0, y: 0, w: 960, h: 1080 }, b: { x: 960, y: 0, w: 960, h: 1080 },
  });
  assert.deepEqual(res.receipts.resizeStep1, {
    a: { x: 0, y: 0, w: 948, h: 1080 }, b: { x: 948, y: 0, w: 972, h: 1080 },
  });
  // Bounded refusal receipts only: two lines (step2 right, step3 left).
  assert.equal(res.receipts["planner-log-resolved"], true);
  assert.deepEqual(res.receipts.resizeRefusals, [
    { step: "step2", internal: "unchanged", wire: "operation-refused", dir: "right", mode: "outwards", press: "0", rowsTotal: "2", rowsShown: "2", subjectBound: true },
    { step: "step3", internal: "unchanged", wire: "operation-refused", dir: "left", mode: "outwards", press: "0", rowsTotal: "1", rowsShown: "1", subjectBound: true },
  ]);
  // The log path and whole refusal lines are never recorded; no leakage.
  const blob = J(res) +
    [...f.files.values()].map((x) => String(x.data)).join("\n");
  assert.ok(!blob.includes("omnitiler-planner-dev."), "planner log path recorded");
  assert.ok(!blob.includes("omnitiler:resize-refused"), "refusal line recorded");
  assert.ok(!blob.includes(FIX_NONCE), "fixture nonce leaked");
  assert.ok(!/helper-[0-9a-f]{32}/.test(J(res)), "helper nonce leaked into evidence");
  assert.ok(!blob.includes("Foreign Files"), "foreign caption leaked");
  assert.ok(!blob.includes("org.kde.dolphin"), "foreign class leaked");
  assert.equal(res.counts.foreign, 1);
});

test("RR-NEG: step-1 faults fail closed at resize-step1 with no retry", async () => {
  for (const [fault, oracle] of [
    ["step1Delta11", /stepped allocation/],
    ["step1AllocMismatch", /published allocation/],
    ["step1SpuriousRefusal", /unexpected planner refusal line/],
    ["extraRow", /not exactly \{A,B\}/],
  ]) {
    const f = makeFake({ fail: { resizeFault: fault } });
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "resize-refusal" }));
    assert.equal(res.rc, 1, fault);
    assert.equal(res.category, "FAIL", fault);
    assert.match(res.divergence || "", /resize-step1/, fault);
    assert.match(res.divergence || "", oracle, fault);
    assert.ok(!res.events.includes("scenario-done"), fault);
    assert.equal(f.execCalls.filter((c) => c.indexOf("ydotool key") === 0).length, 1,
      fault + ": failing chord is never retried");
  }
});

test("RR-NEG: step-2 faults fail closed at resize-step2", async () => {
  for (const [fault, oracle] of [
    ["step2NoRefusal", /expected exactly one resize refusal line since resize-step2, saw 0/],
    ["step2TwoLines", /expected exactly one resize refusal line since resize-step2, saw 2/],
    ["step2WrongKind", /refusal token mismatch at resize-step2: internal/],
    ["step2FramesMoved", /b frame moved on refused resize/],
  ]) {
    const f = makeFake({ fail: { resizeFault: fault } });
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "resize-refusal" }));
    assert.equal(res.rc, 1, fault);
    assert.equal(res.category, "FAIL", fault);
    assert.match(res.divergence || "", /resize-step2/, fault);
    assert.match(res.divergence || "", oracle, fault);
    assert.ok(!res.events.includes("scenario-done"), fault);
    assert.equal(f.execCalls.filter((c) => c.indexOf("ydotool key") === 0).length, 2,
      fault + ": failing chord is never retried");
  }
});

test("RR-NEG: step-2 refusal subject/press/rows faults fail closed at resize-step2", async () => {
  for (const [fault, oracle] of [
    ["step2SubjectA", /subject mismatch/],
    ["step2SubjectForeign", /subject mismatch/],
    ["step2Press1", /press='1' \(need '0'\)/],
    ["step2Rows1", /rowsTotal='1' \(need '2'\)/],
  ]) {
    const f = makeFake({ fail: { resizeFault: fault } });
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "resize-refusal" }));
    assert.equal(res.rc, 1, fault);
    assert.equal(res.category, "FAIL", fault);
    assert.match(res.divergence || "", /resize-step2/, fault);
    assert.match(res.divergence || "", oracle, fault);
    assert.ok(!res.events.includes("scenario-done"), fault);
    assert.equal(f.execCalls.filter((c) => c.indexOf("ydotool key") === 0).length, 2,
      fault + ": failing chord is never retried");
  }
});

test("RR-NEG: step-3 refusal subject/rows faults fail closed at resize-step3", async () => {
  for (const [fault, oracle] of [
    ["step3SubjectB", /subject mismatch/],
    ["step3Rows2", /rowsTotal='2' \(need '1'\)/],
  ]) {
    const f = makeFake({ fail: { resizeFault: fault } });
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "resize-refusal" }));
    assert.equal(res.rc, 1, fault);
    assert.equal(res.category, "FAIL", fault);
    assert.match(res.divergence || "", /resize-step3/, fault);
    assert.match(res.divergence || "", oracle, fault);
    assert.ok(!res.events.includes("scenario-done"), fault);
    assert.equal(f.execCalls.filter((c) => c.indexOf("ydotool key") === 0).length, 3,
      fault + ": failing chord is never retried");
  }
});

test("RR-NEG: step-3 and planner-log faults fail closed", async () => {
  for (const [fault, step, oracle] of [
    ["step3RefusalMissing", /resize-step3/, /expected exactly one resize refusal line since resize-step3, saw 0/],
    ["step3FrameChanged", /resize-step3/, /a frame changed on refused singleton resize/],
    ["logReadlinkFail", null, /planner log resolve failed/],
    ["logPathMismatch", null, /planner log unresolvable/],
  ]) {
    const f = makeFake({ fail: { resizeFault: fault } });
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "resize-refusal" }));
    assert.equal(res.rc, 1, fault);
    assert.equal(res.category, "FAIL", fault);
    if (step) {
      assert.match(res.divergence || "", step, fault);
    } else {
      assert.ok(!res.events.includes("resize-step1"), fault + ": log failure precedes the journey");
    }
    assert.match(res.divergence || "", oracle, fault);
    assert.ok(!res.events.includes("scenario-done"), fault);
  }
});

test("RR-PENDING: missing/unknown facts never PASS", async () => {
  const f = makeFake({ fail: { resizeFault: "step1UnknownInfo" } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "resize-refusal" }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "PENDING", J(res.divergence));
  assert.match(res.divergence || "", /resize-step1/);
  // Unknown published allocation at step 1 is PENDING too.
  const g = makeFake({});
  g.allocPatch = (which, v) => (which === "b0" && g.resizeStarted ? null : v);
  const res2 = await runScenario(Object.assign(baseOpts(), { io: g.io, input: true, scenario: "resize-refusal" }));
  assert.equal(res2.rc, 1);
  assert.equal(res2.category, "PENDING", J(res2.divergence));
  assert.match(res2.divergence || "", /resize-step1/);
});

test("RR-CLI: liveMain runs resize-refusal end to end and the mismatch tripwire holds", async () => {
  const rr = await liveMainFake(["--live", "--evidence-dir", EVID, "--input", "--scenario", "resize-refusal"]);
  assert.equal(rr.rc, 0, rr.out);
  assert.equal(rr.result.requestedScenario, "resize-refusal");
  assert.equal(rr.result.scenarioRan, "resize-refusal");
  assert.equal(rr.result.scenarioMismatch, false);
  assert.ok(rr.result.events.includes("scenario-resize-refusal"), "resize-refusal events ran");
  assert.ok(rr.result.events.includes("resize-step1"));
  assert.ok(rr.result.events.includes("resize-step2"));
  assert.ok(rr.result.events.includes("resize-step3"));
  assert.ok(!rr.result.events.includes("scenario-admission"), "tiling-reflow did not run");
  // A ran-vs-requested scenario mismatch fails the run and is recorded.
  const f = makeFake({});
  f.dirs.delete(EVID);
  const wrong = async (opts) => runScenario(Object.assign({}, opts, { scenario: "tiling-reflow" }));
  const origLog = console.log; const origErr = console.error;
  const sink = [];
  console.log = (...a) => { sink.push(a.join(" ")); };
  console.error = (...a) => { sink.push(a.join(" ")); };
  let rc;
  try {
    rc = await liveMain({
      argv: ["--live", "--evidence-dir", EVID, "--input", "--scenario", "resize-refusal"],
      ioFactory: () => f.io, root: ROOT, execEnv: ENV, templatePath: TEMPLATE, runScenarioFn: wrong,
    });
  } finally { console.log = origLog; console.error = origErr; }
  assert.equal(rc, 1);
  assert.match(sink.join("\n"), /requested scenario 'resize-refusal' but ran 'tiling-reflow'/);
  const result = JSON.parse(f.files.get(EVID + "/result.json").data);
  assert.equal(result.scenarioMismatch, true);
  assert.equal(result.requestedScenario, "resize-refusal");
  assert.equal(result.scenarioRan, "tiling-reflow");
});

// ---------------------------------------------------------------------------
// RB. rapid-burst scenario (60-chord FIFO focus/move burst with
// journal-order and settled-geometry oracles)
// ---------------------------------------------------------------------------

// Golden vectors: the exact 60 labels pinned literally from the current
// implementation, so a PRNG or constant change cannot pass silently. A-left
// and B-left streams coincide (the kind stream is fixture-independent;
// only the final model differs), but both orders are pinned.
const GOLDEN_RAPID_LABELS_A = [
  "focus-right", "move-left", "focus-right", "focus-left", "move-right",
  "move-left", "move-right", "focus-left", "focus-right", "move-left",
  "focus-right", "focus-left", "move-right", "move-left", "focus-right",
  "focus-left", "move-right", "move-left", "move-right", "focus-left",
  "move-right", "move-left", "focus-right", "focus-left", "move-right",
  "focus-left", "move-right", "move-left", "focus-right", "focus-left",
  "move-right", "move-left", "focus-right", "move-left", "focus-right",
  "move-left", "focus-right", "focus-left", "move-right", "move-left",
  "focus-right", "focus-left", "focus-right", "focus-left", "move-right",
  "move-left", "move-right", "focus-left", "move-right", "move-left",
  "move-right", "focus-left", "move-right", "move-left", "focus-right",
  "move-left", "move-right", "focus-left", "move-right", "move-left",
];
const GOLDEN_RAPID_LABELS_B = [
  "focus-right", "move-left", "focus-right", "focus-left", "move-right",
  "move-left", "move-right", "focus-left", "focus-right", "move-left",
  "focus-right", "focus-left", "move-right", "move-left", "focus-right",
  "focus-left", "move-right", "move-left", "move-right", "focus-left",
  "move-right", "move-left", "focus-right", "focus-left", "move-right",
  "focus-left", "move-right", "move-left", "focus-right", "focus-left",
  "move-right", "move-left", "focus-right", "move-left", "focus-right",
  "move-left", "focus-right", "focus-left", "move-right", "move-left",
  "focus-right", "focus-left", "focus-right", "focus-left", "move-right",
  "move-left", "move-right", "focus-left", "move-right", "move-left",
  "move-right", "focus-left", "move-right", "move-left", "focus-right",
  "move-left", "move-right", "focus-left", "move-right", "move-left",
];

test("RB-GENERATOR: length 60, determinism, mix, forced in-row direction, model", () => {
  assert.equal(RAPID_BURST_LEN, 60);
  const seqA = rapidBurstSequence("a");
  const seqB = rapidBurstSequence("b");
  assert.deepEqual(rapidBurstSequence("a"), seqA, "deterministic for A-left");
  assert.deepEqual(rapidBurstSequence("b"), seqB, "deterministic for B-left");
  assert.deepEqual(seqA.labels, GOLDEN_RAPID_LABELS_A, "A-left golden labels");
  assert.deepEqual(seqB.labels, GOLDEN_RAPID_LABELS_B, "B-left golden labels");
  for (const [seq, leftL, golden] of [[seqA, "a", GOLDEN_RAPID_LABELS_A], [seqB, "b", GOLDEN_RAPID_LABELS_B]]) {
    assert.equal(seq.chords.length, 60, leftL);
    assert.equal(seq.labels.length, 60, leftL);
    const focus = seq.chords.filter((c) => c.indexOf("focus") === 0).length;
    const move = seq.chords.length - focus;
    assert.ok(focus >= 20 && move >= 20, leftL + ": focus=" + focus + " move=" + move);
    // Independent re-simulation of the two-tile row model against the same
    // observed left fixture. The kind comes from the GOLDEN vector, never
    // from the step under test; only the forced direction, the slot swap,
    // and the focus flip are re-derived. Direction is forced by the focused
    // position, focus flips the position, move swaps the slots and keeps
    // focus.
    const other = leftL === "a" ? "b" : "a";
    let order = [leftL, other], pos = 0;
    const expChords = [], expLabels = [];
    for (let i = 0; i < 60; i++) {
      const kind = golden[i].indexOf("focus-") === 0 ? "focus" : "move";
      const dir = pos === 0 ? "right" : "left";
      expChords.push((kind === "focus" ? "focus" : "move") + (dir === "right" ? "Right" : "Left"));
      expLabels.push(kind + "-" + dir);
      if (kind === "move") { order = [order[1], order[0]]; }
      pos = 1 - pos;
    }
    assert.deepEqual(seq.chords, expChords, leftL + ": forced in-row direction");
    assert.deepEqual(seq.labels, expLabels, leftL + ": labels");
    assert.deepEqual(seq.expected,
      { leftFixture: order[0], rightFixture: order[1], focusedFixture: order[pos] },
      leftL + ": final expectation");
    for (let i = 0; i < 60; i++) {
      assert.ok(STATE_CHORDS[seq.chords[i]], leftL + ": chord in the closed table: " + seq.chords[i]);
      assert.ok(/^(focus|move)-(left|right)$/.test(seq.labels[i]), leftL + ": label shape");
      assert.equal(seq.chords[i].toLowerCase(), seq.labels[i].replace("-", ""), leftL + ": chord/label agree");
    }
  }
  assert.throws(() => rapidBurstSequence("c"), /observed left fixture/);
});

test("RB-BUILDER: four chord keycodes and the burst argv", () => {
  assert.deepEqual(STATE_CHORDS.focusLeft, [125, 105]);
  assert.deepEqual(STATE_CHORDS.focusRight, [125, 106]);
  assert.deepEqual(STATE_CHORDS.moveLeft, [125, 42, 105]);
  assert.deepEqual(STATE_CHORDS.moveRight, [125, 42, 106]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "focusLeft"),
    ["ydotool", "key", "125:1", "105:1", "105:0", "125:0"]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "focusRight"),
    ["ydotool", "key", "125:1", "106:1", "106:0", "125:0"]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "moveLeft"),
    ["ydotool", "key", "125:1", "42:1", "105:1", "105:0", "42:0", "125:0"]);
  assert.deepEqual(ydotoolKeyCmd("/tmp/x.sock", "moveRight"),
    ["ydotool", "key", "125:1", "42:1", "106:1", "106:0", "42:0", "125:0"]);
  // Burst argv: per-chord press/release events concatenated in order.
  assert.deepEqual(ydotoolBurstCmd("/tmp/x.sock", ["focusRight", "moveRight", "focusLeft"]),
    ["ydotool", "key",
      "125:1", "106:1", "106:0", "125:0",
      "125:1", "42:1", "106:1", "106:0", "42:0", "125:0",
      "125:1", "105:1", "105:0", "125:0"]);
  assert.throws(() => ydotoolBurstCmd("/tmp/x.sock", []), /at least one chord/);
  assert.throws(() => ydotoolBurstCmd("/tmp/x.sock", ["bogus"]), /unknown chord/);
  assert.throws(() => ydotoolBurstCmd("rel.sock", ["focusLeft"]), /absolute/);
});

test("RB-PASS: burst passes with exact events, one ydotool call, and receipts", async () => {
  const f = makeFake({});
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "rapid-burst" }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, checks: res.checks }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
  assert.equal(res.scenario, "done");
  assert.equal(res.scenarioRan, "rapid-burst");
  const seq = ["preflight", "kwin-identity", "fixture-serve", "ydotoold-start", "journal-cursor",
    "workspace-read", "workspace-append", "workspace-reread", "workspace-setcurrent",
    "dev-on", "readiness", "fixture-spawn", "native-read", "join", "activate",
    "scenario-rapid-burst",
    "rapid-burst",
    "scenario-done", "cleanup",
    "cleanup-fixture-shutdown", "cleanup-workspace-restore", "cleanup-dev-off",
    "cleanup-workspace-remove", "cleanup-ydotoold-stop",
    "verify-fixture-pid-gone", "verify-cargo-pid-gone",
    "verify-fixture-socket-absent", "verify-rundir-removed", "verify-planner-pid-gone",
    "verify-ydotoold-gone", "verify-current-restored", "verify-appended-absent", "verify-dev-down"];
  assert.deepEqual(res.events, seq);
  for (const n of ["rapid-admit-a-frame-equals-alloc", "rapid-admit-b-frame-equals-alloc",
    "rapid-admit-disjoint", "rapid-admit-inside-output", "rapid-admit-same-ws",
    "rapid-journal-fifo", "rapid-final-a-frame-equals-alloc", "rapid-final-b-frame-equals-alloc",
    "rapid-final-disjoint", "rapid-final-inside-output", "rapid-final-order", "rapid-final-focus",
    "rapid-burst"]) {
    assert.ok(res.checks.some((c) => c.name === n), "missing check " + n + ": " + J(res.checks.map((c) => c.name)));
  }
  assert.ok(res.checks.every((c) => c.pass), J(res.checks));
  // Exactly ONE ydotool call carries the whole burst: never retried.
  const keys = f.execCalls.filter((c) => c.indexOf("ydotool key") === 0);
  assert.equal(keys.length, 1, J(keys));
  // The fake admits A left, so the model prediction is the A-left sequence.
  const want = rapidBurstSequence("a");
  const fifo = res.checks.filter((c) => c.name === "rapid-journal-fifo")[0];
  assert.equal(fifo.expected, want.labels.join(","));
  assert.equal(fifo.actual, want.labels.join(","));
  // Final checks carry OBSERVED values as actual (from the last matching
  // read), never the prediction.
  const order = res.checks.filter((c) => c.name === "rapid-final-order")[0];
  assert.equal(order.expected, want.expected.leftFixture + "/" + want.expected.rightFixture);
  assert.equal(order.actual, res.receipts.rapidBurst.left + "/" + res.receipts.rapidBurst.right);
  const focus = res.checks.filter((c) => c.name === "rapid-final-focus")[0];
  assert.equal(focus.expected, want.expected.focusedFixture);
  assert.equal(focus.actual, res.receipts.rapidBurst.focused);
  for (const n of ["rapid-final-a-frame-equals-alloc", "rapid-final-b-frame-equals-alloc"]) {
    const c = res.checks.filter((x) => x.name === n)[0];
    assert.match(String(c.expected), /^alloc \[\d+,\d+,\d+,\d+\]$/, n + ": expected is the observed alloc frame");
    assert.match(String(c.actual), /^native \[\d+,\d+,\d+,\d+\]$/, n + ": actual is the observed native frame");
  }
  const disjoint = res.checks.filter((c) => c.name === "rapid-final-disjoint")[0];
  assert.match(String(disjoint.actual), /^disjoint a=\[.*\] b=\[.*\]$/, "disjoint actual carries observed frames");
  assert.deepEqual(res.receipts.rapidBurst, {
    length: 60,
    focus: want.chords.filter((c) => c.indexOf("focus") === 0).length,
    move: want.chords.filter((c) => c.indexOf("move") === 0).length,
    left: want.expected.leftFixture, right: want.expected.rightFixture,
    focused: want.expected.focusedFixture,
    journalCommands: 60, failureLinesFound: 0,
  });
  // No leakage: letters and counts only, no ids, no raw journal text.
  const blob = J(res) +
    [...f.files.values()].map((x) => String(x.data)).join("\n");
  assert.ok(!blob.includes("omnitiler:world:command"), "raw journal line recorded");
  assert.ok(!blob.includes(FIX_NONCE), "fixture nonce leaked");
  assert.ok(!/helper-[0-9a-f]{32}/.test(J(res)), "helper nonce leaked into evidence");
  assert.ok(!blob.includes("Foreign Files"), "foreign caption leaked");
  assert.ok(!blob.includes("org.kde.dolphin"), "foreign class leaked");
  assert.equal(res.counts.foreign, 1);
});

test("RB-LAG: slow native convergence passes only after the quiescence streak", async () => {
  const f = makeFake({ fail: { burstFault: "burstLag" } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "rapid-burst" }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, checks: res.checks }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
  // Reads 1-2 already matched the final model, so a single-match pass would
  // have stopped there. The streak reset on read 3 and settled only at the
  // end: several post-burst presentation polls must have run, and the burst
  // was still injected exactly once.
  assert.ok((f.burstReads || 0) > T.burstSettleReads + 1,
    "quiescence rode through the reset: burstReads=" + f.burstReads);
  assert.equal(f.execCalls.filter((c) => c.indexOf("ydotool key") === 0).length, 1,
    "the burst is never retried");
  assert.ok(res.checks.every((c) => c.pass), J(res.checks));
});

test("RB-LATE-DRIFT: brief match then permanent drift fails with one call", async () => {
  const f = makeFake({ fail: { burstFault: "lateDrift" } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "rapid-burst" }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "FAIL");
  assert.match(res.divergence || "", /rapid-burst/);
  assert.match(res.divergence || "", /final (left|right) slot holds/);
  assert.ok(!res.events.includes("scenario-done"));
  assert.equal(f.execCalls.filter((c) => c.indexOf("ydotool key") === 0).length, 1,
    "the burst is never retried");
});

test("RB-FAILURE-RE: shortcut-layer failures are attributed, success never matches", () => {
  assert.ok(RAPID_BURST_FAILURE_RE.test("js: omnitiler:world:shortcut-failed op=focus-directional nwinv=0"),
    "shortcut-failed attributed");
  assert.ok(RAPID_BURST_FAILURE_RE.test("js: omnitiler:world:scope-invalid op=move-directional nwinv=0"),
    "scope-invalid attributed");
  assert.ok(RAPID_BURST_FAILURE_RE.test("js: omnitiler:world:queue-timeout op=focus-directional nwinv=0"),
    "queue-timeout still attributed");
  assert.ok(!RAPID_BURST_FAILURE_RE.test("js: omnitiler:world:command op=focus-directional label=focus-right nwinv=0 depth=3"),
    "success line never matches");
});

test("RB-NEG: journal loss faults fail at the burst step with one call and no retry", async () => {
  for (const [fault, oracle] of [
    ["dropOne", /shows 59 command lines \(need 60\)/],
    ["reorder", /journal label mismatch at index 10/],
    ["queueTimeout", /1 failure\/loss line\(s\) \(need zero\)/],
    ["refused", /1 failure\/loss line\(s\) \(need zero\)/],
  ]) {
    const f = makeFake({ fail: { burstFault: fault } });
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "rapid-burst" }));
    assert.equal(res.rc, 1, fault);
    assert.equal(res.category, "FAIL", fault);
    assert.match(res.divergence || "", /rapid-burst/, fault);
    assert.match(res.divergence || "", oracle, fault);
    assert.ok(!res.events.includes("scenario-done"), fault);
    assert.equal(f.execCalls.filter((c) => c.indexOf("ydotool key") === 0).length, 1,
      fault + ": the burst is never retried");
  }
});

test("RB-NEG: settled-state faults fail at the burst step with one call and no retry", async () => {
  for (const [fault, oracle] of [
    ["lostMove", /final (left|right) slot holds/],
    ["focusWrong", /final focused fixture/],
    ["frameDrift", /native frame != published allocation/],
    ["extraRow", /not exactly \{A,B\}/],
    ["missingRow", /not exactly \{A,B\}/],
  ]) {
    const f = makeFake({ fail: { burstFault: fault } });
    const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "rapid-burst" }));
    assert.equal(res.rc, 1, fault);
    assert.equal(res.category, "FAIL", fault);
    assert.match(res.divergence || "", /rapid-burst/, fault);
    assert.match(res.divergence || "", oracle, fault);
    assert.ok(!res.events.includes("scenario-done"), fault);
    assert.equal(f.execCalls.filter((c) => c.indexOf("ydotool key") === 0).length, 1,
      fault + ": the burst is never retried");
  }
});

test("RB-PENDING: missing/unknown facts never PASS", async () => {
  const f = makeFake({ fail: { burstFault: "unknownInfo" } });
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io, input: true, scenario: "rapid-burst" }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "PENDING", J(res.divergence));
  assert.match(res.divergence || "", /rapid-burst/);
  const g = makeFake({ fail: { burstFault: "unknownAlloc" } });
  const res2 = await runScenario(Object.assign(baseOpts(), { io: g.io, input: true, scenario: "rapid-burst" }));
  assert.equal(res2.rc, 1);
  assert.equal(res2.category, "PENDING", J(res2.divergence));
  assert.match(res2.divergence || "", /rapid-burst/);
});

test("RB-CLI: rapid-burst parses, requires --input before io, and runs end to end", async () => {
  assert.equal(parseArgs(["--scenario", "rapid-burst"]).scenario, "rapid-burst");
  assert.equal(parseArgs(["--scenario=rapid-burst"]).scenario, "rapid-burst");
  assert.ok(helpText().indexOf("rapid-burst") >= 0, "help documents rapid-burst");
  assert.throws(() => parseArgs(["--scenario=rapid-burst", "--scenario=tiling-reflow"]), /duplicate --scenario/);
  const noIo = () => { throw new Error("io must not be built"); };
  assert.equal(await liveMain({
    argv: ["--live", "--evidence-dir", "/tmp/kde-rb-noinput-1", "--scenario", "rapid-burst"],
    ioFactory: noIo, root: ROOT, templatePath: TEMPLATE,
  }), 2);
  const f = makeFake();
  await assert.rejects(
    runScenario(Object.assign(baseOpts(), { io: f.io, scenario: "rapid-burst" })),
    /rapid-burst requires --input/);
  assert.equal(f.execCalls.length, 0, "no exec before the input guard");
  const rb = await liveMainFake(["--live", "--evidence-dir", EVID, "--input", "--scenario", "rapid-burst"]);
  assert.equal(rb.rc, 0, rb.out);
  assert.equal(rb.result.requestedScenario, "rapid-burst");
  assert.equal(rb.result.scenarioRan, "rapid-burst");
  assert.equal(rb.result.scenarioMismatch, false);
  assert.ok(rb.result.events.includes("scenario-rapid-burst"), "rapid-burst events ran");
  assert.ok(rb.result.events.includes("rapid-burst"));
  assert.ok(!rb.result.events.includes("scenario-admission"), "tiling-reflow did not run");
  // A ran-vs-requested scenario mismatch fails the run and is recorded.
  const h = makeFake({});
  h.dirs.delete(EVID);
  const wrong = async (opts) => runScenario(Object.assign({}, opts, { scenario: "tiling-reflow" }));
  const origLog = console.log; const origErr = console.error;
  const sink = [];
  console.log = (...a) => { sink.push(a.join(" ")); };
  console.error = (...a) => { sink.push(a.join(" ")); };
  let rc;
  try {
    rc = await liveMain({
      argv: ["--live", "--evidence-dir", EVID, "--input", "--scenario", "rapid-burst"],
      ioFactory: () => h.io, root: ROOT, execEnv: ENV, templatePath: TEMPLATE, runScenarioFn: wrong,
    });
  } finally { console.log = origLog; console.error = origErr; }
  assert.equal(rc, 1);
  assert.match(sink.join("\n"), /requested scenario 'rapid-burst' but ran 'tiling-reflow'/);
  const result = JSON.parse(h.files.get(EVID + "/result.json").data);
  assert.equal(result.scenarioMismatch, true);
  assert.equal(result.requestedScenario, "rapid-burst");
  assert.equal(result.scenarioRan, "tiling-reflow");
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
// H. Fractional-scale native coordinates (adapter toInt mirror, no tolerance)
// ---------------------------------------------------------------------------

test("H: helper rounds fractional QRectF doubles per component (1.25 grid)", () => {
  const m = modelWs();
  m.wins[0].frameGeometry = { x: 0.8, y: 1.6, width: 960.8, height: 1079.2 };
  m.wins[0].output = { name: "HDMI-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 }, devicePixelRatio: 1.25 };
  const r = runTemplate({ op: "read", scope: SCOPE_CLASS }, m.workspace);
  assert.equal(r.rows.length, 1);
  assert.equal(r.rows[0].owned.length, 1);
  assert.deepEqual(r.rows[0].owned[0].frame, { x: 1, y: 2, w: 961, h: 1079 });
  assert.deepEqual(r.rows[0].owned[0].outRect, { x: 0, y: 0, w: 1920, h: 1080 });
});

test("H: helper rounds 1.5-grid and integer values per component", () => {
  const m = modelWs();
  m.wins[0].frameGeometry = { x: 100 + 2 / 3, y: 5, width: 10, height: 10 };
  const r = runTemplate({ op: "read", scope: SCOPE_CLASS }, m.workspace);
  assert.deepEqual(r.rows[0].owned[0].frame, { x: 101, y: 5, w: 10, h: 10 });
});

test("H: helper rounds exact halves toward positive infinity", () => {
  const m = modelWs();
  m.wins[0].frameGeometry = { x: 10.5, y: 11.5, width: 12.5, height: 13.5 };
  const r = runTemplate({ op: "read", scope: SCOPE_CLASS }, m.workspace);
  assert.deepEqual(r.rows[0].owned[0].frame, { x: 11, y: 12, w: 13, h: 14 });
  const m2 = modelWs();
  m2.wins[0].frameGeometry = { x: -0.5, y: -1.5, width: 10, height: 10 };
  const r2 = runTemplate({ op: "read", scope: SCOPE_CLASS }, m2.workspace);
  assert.deepEqual(r2.rows[0].owned[0].frame, { x: 0, y: -1, w: 10, h: 10 });
});

test("H: helper nulls non-finite or non-number components", () => {
  for (const bad of [NaN, Infinity, -Infinity, "10", null, undefined]) {
    const m = modelWs();
    m.wins[0].frameGeometry = { x: bad, y: 0, width: 10, height: 10 };
    const r = runTemplate({ op: "read", scope: SCOPE_CLASS }, m.workspace);
    assert.equal(r.rows[0].owned[0].frame, null, "x=" + String(bad));
  }
  const m = modelWs();
  m.wins[0].frameGeometry = { x: 0, y: NaN, width: 10, height: 10 };
  assert.equal(runTemplate({ op: "read", scope: SCOPE_CLASS }, m.workspace).rows[0].owned[0].frame, null);
  const m2 = modelWs();
  m2.wins[0].output = { name: "HDMI-1", geometry: { x: NaN, y: 0, width: 1920, height: 1080 }, devicePixelRatio: 1 };
  assert.equal(runTemplate({ op: "read", scope: SCOPE_CLASS }, m2.workspace).rows[0].owned[0].outRect, null);
});

test("H: helper nulls sizes whose rounding reaches zero", () => {
  const m = modelWs();
  m.wins[0].frameGeometry = { x: 0, y: 0, width: 0.4, height: 10 };
  assert.equal(runTemplate({ op: "read", scope: SCOPE_CLASS }, m.workspace).rows[0].owned[0].frame, null);
  const m2 = modelWs();
  m2.wins[0].frameGeometry = { x: 0, y: 0, width: 10, height: 0.4 };
  assert.equal(runTemplate({ op: "read", scope: SCOPE_CLASS }, m2.workspace).rows[0].owned[0].frame, null);
  const m3 = modelWs();
  m3.wins[0].output = { name: "HDMI-1", geometry: { x: 0, y: 0, width: 0.4, height: 1080 }, devicePixelRatio: 1 };
  assert.equal(runTemplate({ op: "read", scope: SCOPE_CLASS }, m3.workspace).rows[0].owned[0].outRect, null);
});

test("H: helper still bounds rounded frame and outRect by GEO_BOUND", () => {
  const m = modelWs();
  m.wins[0].frameGeometry = { x: 16384.4, y: 0, width: 10, height: 10 };
  assert.deepEqual(runTemplate({ op: "read", scope: SCOPE_CLASS }, m.workspace).rows[0].owned[0].frame,
    { x: 16384, y: 0, w: 10, h: 10 });
  const m2 = modelWs();
  m2.wins[0].frameGeometry = { x: 16384.6, y: 0, width: 10, height: 10 };
  assert.equal(runTemplate({ op: "read", scope: SCOPE_CLASS }, m2.workspace).rows[0].owned[0].frame, null);
  const m3 = modelWs();
  m3.wins[0].frameGeometry = { x: 0, y: 0, width: 16384.6, height: 10 };
  assert.equal(runTemplate({ op: "read", scope: SCOPE_CLASS }, m3.workspace).rows[0].owned[0].frame, null);
  const m4 = modelWs();
  m4.wins[0].output = { name: "HDMI-1", geometry: { x: 0, y: 0, width: 16384.4, height: 1080 }, devicePixelRatio: 1 };
  assert.deepEqual(runTemplate({ op: "read", scope: SCOPE_CLASS }, m4.workspace).rows[0].owned[0].outRect,
    { x: 0, y: 0, w: 16384, h: 1080 });
  const m5 = modelWs();
  m5.wins[0].output = { name: "HDMI-1", geometry: { x: 0, y: 0, width: 16384.6, height: 1080 }, devicePixelRatio: 1 };
  assert.equal(runTemplate({ op: "read", scope: SCOPE_CLASS }, m5.workspace).rows[0].owned[0].outRect, null);
});

test("H: helper emits frameRaw and outScale; invalid scale is null; foreign stays hidden", () => {
  const m = modelWs();
  m.wins[0].frameGeometry = { x: 0.8, y: 1.6, width: 960.8, height: 1079.2 };
  m.wins[0].output = { name: "HDMI-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 }, devicePixelRatio: 1.25 };
  const r = runTemplate({ op: "read", scope: SCOPE_CLASS }, m.workspace);
  const row = r.rows[0].owned[0];
  assert.deepEqual(row.frameRaw, { x: 0.8, y: 1.6, w: 960.8, h: 1079.2 });
  assert.equal(row.outScale, 1.25);
  assert.equal(row.out, "HDMI-1");
  for (const l of r.logs) { assert.ok(!l.includes("FOREIGN-SECRET-CAPTION")); }
  for (const bad of [0, -1, NaN, Infinity, "1.25", null, undefined]) {
    const mb = modelWs();
    mb.wins[0].output = { name: "HDMI-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 }, devicePixelRatio: bad };
    const rb = runTemplate({ op: "read", scope: SCOPE_CLASS }, mb.workspace);
    assert.equal(rb.rows[0].owned[0].outScale, null, "scale=" + String(bad));
  }
  const mn = modelWs();
  delete mn.wins[0].output;
  const rn = runTemplate({ op: "read", scope: SCOPE_CLASS }, mn.workspace);
  assert.equal(rn.rows[0].owned[0].outScale, null);
  assert.equal(rn.rows[0].owned[0].out, null);
  assert.equal(rn.rows[0].owned[0].outRect, null);
  const mu = modelWs();
  mu.wins[0].frameGeometry = { x: NaN, y: 0, width: 10, height: 10 };
  const ru = runTemplate({ op: "read", scope: SCOPE_CLASS }, mu.workspace);
  assert.equal(ru.rows[0].owned[0].frame, null);
  assert.equal(ru.rows[0].owned[0].frameRaw, null);
});

test("SCALE: runner PASSES at a 1.25 output scale with fractional native frames", async () => {
  const f = makeFake();
  for (const w of f.wins) {
    if (w.internalId === UUID_A || w.internalId === UUID_B) {
      w.output = { name: "HDMI-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 }, devicePixelRatio: 1.25 };
    }
  }
  f.wins.find((w) => w.internalId === UUID_A).frameGeometry = { x: 0, y: 0, width: 960.8, height: 1080 };
  f.wins.find((w) => w.internalId === UUID_B).frameGeometry = { x: 960.8, y: 0, width: 959.2, height: 1080 };
  f.allocPatch = (which, g) => {
    if (which === "a0") { return [0, 0, 961, 1080]; }
    if (which === "b0") { return [961, 0, 959, 1080]; }
    return g;
  };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.category, "PASS", J({ div: res.divergence, checks: res.checks }));
  assert.equal(res.rc, 0, J(res.cleanupFailed));
  assert.deepEqual(res.receipts.output, { name: "HDMI-1", scale: 1.25, rect: { x: 0, y: 0, w: 1920, h: 1080 },
    raw: { a: { x: 0, y: 0, w: 960.8, h: 1080 }, b: { x: 960.8, y: 0, w: 959.2, h: 1080 } } });
  assert.ok(res.checks.some((c) => c.name === "reflow-a-frame-equals-alloc" && c.pass));
});

test("SCALE: runner FAILS naming the exact field when one rounded component differs by 1", async () => {
  const f = makeFake();
  for (const w of f.wins) {
    if (w.internalId === UUID_A || w.internalId === UUID_B) {
      w.output = { name: "HDMI-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 }, devicePixelRatio: 1.25 };
    }
  }
  f.wins.find((w) => w.internalId === UUID_A).frameGeometry = { x: 0, y: 0, width: 960.8, height: 1080 };
  f.wins.find((w) => w.internalId === UUID_B).frameGeometry = { x: 960.8, y: 0, width: 959.2, height: 1080 };
  f.allocPatch = (which, g) => {
    if (which === "a0") { return [0, 0, 961, 1080]; }
    if (which === "b0") { return [961, 0, 960, 1080]; }
    return g;
  };
  const res = await runScenario(Object.assign(baseOpts(), { io: f.io }));
  assert.equal(res.rc, 1);
  assert.equal(res.category, "FAIL");
  assert.match(res.divergence, /tile B frame mismatch \(w\)/);
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

async function liveMainFake(argv, extra) {
  const f = makeFake({});
  f.allowRestart = true;
  f.dirs.delete(EVID);
  const stdoutSink = [];
  const origLog = console.log; const origErr = console.error;
  console.log = (...a) => { stdoutSink.push(a.join(" ")); };
  console.error = (...a) => { stdoutSink.push(a.join(" ")); };
  let rc;
  try {
    rc = await liveMain(Object.assign({ argv, ioFactory: () => f.io, root: ROOT, execEnv: ENV, templatePath: TEMPLATE }, extra || {}));
  } finally { console.log = origLog; console.error = origErr; }
  const raw = f.files.get(EVID + "/result.json");
  return { rc, f, out: stdoutSink.join("\n"), result: raw ? JSON.parse(raw.data) : null };
}

test("G: CLI --scenario reaches runScenario; result.json names the scenario that ran", async () => {
  const st = await liveMainFake(["--live", "--evidence-dir", EVID, "--input", "--scenario", "state-targets"]);
  assert.equal(st.rc, 0, st.out);
  assert.equal(st.result.requestedScenario, "state-targets");
  assert.equal(st.result.scenarioRan, "state-targets");
  assert.equal(st.result.scenarioMismatch, false);
  assert.ok(st.result.events.includes("scenario-state-targets"), "state-targets events ran");
  assert.ok(st.result.events.includes("state-a-maximize-on"));
  assert.ok(!st.result.events.includes("scenario-admission"), "tiling-reflow did not run");
  assert.equal(st.result.checks.filter((c) => c.name.indexOf("state-") === 0).length, 16);

  const tr = await liveMainFake(["--live", "--evidence-dir", EVID]);
  assert.equal(tr.rc, 0, tr.out);
  assert.equal(tr.result.requestedScenario, "tiling-reflow");
  assert.equal(tr.result.scenarioRan, "tiling-reflow");
  assert.ok(tr.result.events.includes("scenario-admission"));
  assert.ok(!tr.result.events.includes("scenario-state-targets"));

  const tr2 = await liveMainFake(["--live", "--evidence-dir", EVID, "--scenario", "tiling-reflow"]);
  assert.equal(tr2.result.scenarioRan, "tiling-reflow");
});

test("G: a ran-vs-requested scenario mismatch fails the run and is recorded", async () => {
  const f = makeFake({});
  f.dirs.delete(EVID);
  const wrong = async (opts) => {
    const res = await runScenario(Object.assign({}, opts, { scenario: "tiling-reflow" }));
    return res;
  };
  const origLog = console.log; const origErr = console.error;
  const sink = [];
  console.log = (...a) => { sink.push(a.join(" ")); };
  console.error = (...a) => { sink.push(a.join(" ")); };
  let rc;
  try {
    rc = await liveMain({
      argv: ["--live", "--evidence-dir", EVID, "--input", "--scenario", "state-targets"],
      ioFactory: () => f.io, root: ROOT, execEnv: ENV, templatePath: TEMPLATE, runScenarioFn: wrong,
    });
  } finally { console.log = origLog; console.error = origErr; }
  assert.equal(rc, 1);
  assert.match(sink.join("\n"), /requested scenario 'state-targets' but ran 'tiling-reflow'/);
  const result = JSON.parse(f.files.get(EVID + "/result.json").data);
  assert.equal(result.scenarioMismatch, true);
  assert.equal(result.requestedScenario, "state-targets");
  assert.equal(result.scenarioRan, "tiling-reflow");
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
