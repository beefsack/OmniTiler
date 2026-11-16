// scripts/kde-live-helpers.test.mjs: hermetic tests for the KWin helper channel.
// Run with exactly: node --test scripts/kde-live-helpers.test.mjs
// OFFLINE ONLY: every test runs against a stateful FAKE io boundary. The fake
// KWin executes the generated helper source in node:vm with the fake
// workspace; journal stubs replay VM-produced rows. No live calls.
import { test } from "node:test";
import assert from "node:assert/strict";
import vm from "node:vm";
import {
  createKwinScriptChannel, KwinChannelError, generateHelperSource,
  KWIN_CHANNEL_PLUGIN_PREFIX, KWIN_CHANNEL_MARK, KWIN_CHANNEL_CURSOR_BODY,
  KWIN_FIXTURE_PREAMBLE, readFixtureState, joinFixtureWindows,
} from "./kde-live-helpers.mjs";

const EVID_DIR = "/evidence/results";
const J = (v) => JSON.stringify(v);

// ---------------------------------------------------------------------------
// Fake boundary: minimal exact-object KWin (only what the channel touches).
// ---------------------------------------------------------------------------

function makeFake(opts) {
  opts = opts || {};
  const F = {
    opts,
    files: new Map(),
    mrdirs: new Set([EVID_DIR]),
    kwinPid: 70011, kwinTick: "424242", kwinOwner: ":1.77",
    loaded: new Map(),
    journal: [], cursorSeq: 0,
    runLog: [], execLog: [], sleepCount: 0,
    scriptSeq: 0,
    cursorPos: { x: 100, y: 100 },
    occupied: new Set(opts.occupied || []),
    fail: opts.fail || {},
    written: [], // {path, data, mode}
    nowT: 1000,
  };
  const J3 = (t, d) => J({ type: t, data: d });
  F.newCursor = () => "CUR" + (F.cursorSeq++);
  F.pushJournal = (pid, msg, pidField) => {
    F.journal.push({ cursor: F.newCursor(), pid, msg, pidField });
  };
  F.introFor = (extra) => {
    const names = [...F.loaded.values()].map((id) => `<node name="Script${id}"/>`).join("");
    return J3("s", ["<node>" + names + "</node>"]);
  };
  F.scriptIntro = (sid) => {
    // Realistic KWin Script0 object: FOUR interfaces (verified shape, inline
    // not read from disk). Only the first carries run; the others must not
    // satisfy the check. Modes via F.fail.introMode override for regressions.
    const mode = F.fail.introMode || "real";
    let xml;
    if (mode === "real") {
      xml = "<node>"
        + '<interface name="org.kde.kwin.Script"><method name="stop"/><method name="run"/></interface>'
        + '<interface name="org.freedesktop.DBus.Properties"><method name="Get"><arg name="interface_name" type="s" direction="in"/><arg name="property_name" type="s" direction="in"/><arg name="value" type="v" direction="out"/></method></interface>'
        + '<interface name="org.freedesktop.DBus.Introspectable"><method name="Introspect"><arg name="xml_data" type="s" direction="out"/></method></interface>'
        + '<interface name="org.freedesktop.DBus.Peer"><method name="Ping"/><method name="GetMachineId"><arg name="machine_uuid" type="s" direction="out"/></method></interface>'
        + "</node>";
    } else if (mode === "dupTarget") {
      xml = "<node>"
        + '<interface name="org.kde.kwin.Script"><method name="run"/></interface>'
        + '<interface name="org.kde.kwin.Script"><method name="run"/></interface>'
        + "</node>";
    } else if (mode === "misplacedRun") {
      // run lives only on an unrelated interface: must NOT pass.
      xml = "<node>"
        + '<interface name="org.kde.kwin.Script"><method name="stop"/></interface>'
        + '<interface name="org.freedesktop.DBus.Properties"><method name="run"/></interface>'
        + "</node>";
    } else if (mode === "noTarget") {
      xml = "<node>"
        + '<interface name="org.freedesktop.DBus.Properties"><method name="run"/></interface>'
        + "</node>";
    } else {
      xml = "<node></node>";
    }
    return J3("s", [xml]);
  };
  F.execHelper = (src) => {
    const m = /var NONCE = "([0-9a-f]{32})"/.exec(src);
    if (!m) { throw new Error("helper source lost nonce"); }
    const nonce = m[1];
    const ws = {
      get cursorPos() {
        if (F.fail.cursorUnknown) { throw new Error("cursorPos unreadable"); }
        return { x: F.cursorPos.x, y: F.cursorPos.y };
      },
    };
    const logs = [];
    const sandbox = { workspace: ws, console: { log: (...a) => logs.push(a.join(" ")) } };
    vm.createContext(sandbox);
    vm.runInContext(src, sandbox, { timeout: 2000 });
    for (const l of logs) { F.pushJournal(F.kwinPid, "js: " + l, F.journalPid !== undefined ? F.journalPid : F.kwinPid); }
    return { nonce, logs };
  };
  const F_io = {
    exec: async (argv, o) => {
      F.execLog.push(argv.map(String).join(" "));
      const s = argv.map(String).join(" ");
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
          const lines = F.journal.filter((e, ix) => {
            if (ix < n) { return false; }
            const ep = e.pidField !== undefined ? e.pidField : e.pid;
            return (pid === null || ep === pid);
          }).map((e) => J({ MESSAGE: e.msg, _PID: e.pidField !== undefined ? e.pidField : e.pid }));
          return { status: 0, stdout: lines.join("\n") + (lines.length ? "\n" : ""), stderr: "" };
        }
      }
      if (argv[0] === "busctl") {
        if (s.indexOf("GetNameOwner") >= 0 && s.indexOf("org.kde.KWin") >= 0) {
          let owner = F.kwinOwner;
          if (F.fail.ownerChange && F.state.phase === "during") { owner = ":1.88"; }
          else if (F.fail.identityChangeAt && F.state.attempts++ >= F.fail.identityChangeAt && F.state.phase === "during") { owner = ":1.88"; }
          return { status: 0, stdout: J3("s", [owner]) + "\n", stderr: "" };
        }
        if (s.indexOf("GetConnectionUnixProcessID") >= 0) {
          const pid = (F.fail.pidChange && F.state.phase === "during") ? 70099 : F.kwinPid;
          return { status: 0, stdout: J3("u", [pid]) + "\n", stderr: "" };
        }
        if (s.indexOf("Introspect") >= 0 && s.indexOf("/Scripting/Script") >= 0) {
          return { status: 0, stdout: (F.fail.badIntro ? J3("s", ["<node></node>"]) : F.scriptIntro()) + "\n", stderr: "" };
        }
        if (s.indexOf("Introspect") >= 0) {
          return { status: 0, stdout: F.introFor() + "\n", stderr: "" };
        }
        if (s.indexOf("isScriptLoaded") >= 0) {
          const plugin = argv[argv.length - 1];
          const v = F.fail.stillLoaded ? true : F.loaded.has(plugin);
          return { status: 0, stdout: J3("b", [v]) + "\n", stderr: "" };
        }
        if (s.indexOf("unloadScript") >= 0) {
          const plugin = argv[argv.length - 1];
          F.unloadCalls = (F.unloadCalls || 0) + 1;
          if (F.fail.unloadFalse) { return { status: 0, stdout: J3("b", [false]) + "\n", stderr: "" }; }
          F.loaded.delete(plugin);
          return { status: 0, stdout: J3("b", [true]) + "\n", stderr: "" };
        }
        if (s.indexOf("loadScript") >= 0) {
          const plugin = argv[argv.length - 1];
          if (F.fail.loadId !== undefined) {
            return { status: 0, stdout: J3("i", [F.fail.loadId]) + "\n", stderr: "" };
          }
          const id = F.scriptSeq++;
          F.loaded.set(plugin, id);
          F.pendingPlugin = plugin;
          return { status: 0, stdout: J3("i", [id]) + "\n", stderr: "" };
        }
        if (argv.indexOf("run") >= 0 && /Script[0-9]+/.test(s)) {
          F.runLog.push(s);
          if (F.fail.runStatus) { return { status: 1, stdout: "", stderr: "boom" }; }
          const src = F.pendingSrc;
          if (src) {
            F.execHelper(src);
            F.pendingSrc = null;
          }
          return { status: 0, stdout: "", stderr: "" };
        }
      }
      throw new Error("fake: unstubbed exec: " + s.slice(0, 140));
    },
    writeFile: async (p, d, m) => {
      F.files.set(p, { data: String(d), mode: m });
      F.written.push({ path: p, data: String(d), mode: m });
      if (String(d).indexOf(KWIN_CHANNEL_MARK) >= 0) { F.pendingSrc = String(d); }
    },
    readFile: async (p) => {
      if (F.files.has(p)) { return F.files.get(p).data; }
      throw new Error("fake: unstubbed read " + p);
    },
    mkdir: async (d) => { F.mrdirs.add(d); },
    rmdir: async (d) => { F.mrdirs.delete(d); },
    rm: async (p) => { F.files.delete(p); },
    exists: async (p) => {
      if (F.occupied.has(p)) { return true; }
      return F.files.has(p) || F.mrdirs.has(p);
    },
    procIdentity: async (pid) => {
      if (pid !== F.kwinPid && !(F.fail.pidChange && F.state.phase === "during" && pid === 70099)) { return null; }
      return (F.fail.tickChange && F.state.phase === "during") ? "999999" : F.kwinTick;
    },
    sleep: async (ms) => { F.sleepCount = (F.sleepCount || 0) + 1; },
    now: () => (F.nowT += 100),
  };
  F.state = { attempts: 0, phase: "init" };
  F.io = F_io;
  return F;
}

// Channel factory over the fake. Tests never touch the live session.
const channel = (F, dir) => createKwinScriptChannel({ evidenceDir: dir || EVID_DIR, io: F.io, root: "/repo", env: { PATH: "/usr/bin" } });
const helperFiles = (F) => F.written.filter((w) => /helper-[0-9a-f]{32}\.js$/.test(w.path) && w.path.endsWith(".js") && !w.path.includes("commands") && !w.path.includes("journal") && !w.path.includes("receipt"));
const nonceOf = (p) => /helper-([0-9a-f]{32})\.js$/.exec(p)[1];

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

test("import has no live side effects (fake not consulted)", async () => {
  const F = makeFake();
  // Re-import is cached; assert the module surface exists and no calls happened
  // before any factory invocation.
  const mod = await import("./kde-live-helpers.mjs");
  assert.equal(typeof mod.createKwinScriptChannel, "function");
  assert.equal(F.execLog.length, 0);
});

test("valid ID 0 loads, runs exact Script0, unloads, receipt persisted", async () => {
  const F = makeFake();
  F.state.phase = "init";
  const ch = await channel(F);
  const res = await ch.run("emit({ok:1,x:1});", "valid-0");
  assert.deepEqual(res, { ok: 1, x: 1 });
  assert.ok(Object.isFrozen(ch.identity), "identity frozen copy");
  assert.deepEqual(JSON.parse(JSON.stringify(ch.identity)), { owner: F.kwinOwner, pid: F.kwinPid, tick: F.kwinTick });
  assert.ok(F.runLog.length === 1 && F.runLog[0].indexOf("/Scripting/Script0") >= 0, "ran exact Script0: " + F.runLog[0]);
  assert.equal(F.loaded.size, 0, "unloaded");
  assert.ok(F.unloadCalls === 1, "unloaded exactly once");
  const written = F.written.map((w) => w.path);
  assert.ok(written.some((p) => p.endsWith(".commands.jsonl")));
  assert.ok(written.some((p) => p.endsWith(".journal.txt")));
  assert.ok(written.some((p) => p.endsWith(".receipt.json")));
  const receipt = JSON.parse(F.written.find((w) => w.path.endsWith(".receipt.json")).data);
  assert.equal(receipt.scriptId, 0);
  assert.equal(receipt.runOk, true);
  assert.equal(receipt.kwinPid, F.kwinPid);
  assert.equal(receipt.result.x, 1);
  // Source preserved (never removed): fresh file still in evidence dir.
  const src = F.written.find((w) => w.path.endsWith(".js") && !w.path.includes("receipt"));
  assert.ok(src);
  assert.ok(F.files.has(src.path));
  await ch.close();
});

test("ID -1 / malformed / overbound / non-integer refused with zero run", async () => {
  for (const bad of [-1, 2147483648, 1.5, "3"]) {
    const F = makeFake({ fail: { loadId: bad } });
    const ch = await channel(F);
    await assert.rejects(ch.run("emit({});", "bad-id-" + String(bad)), KwinChannelError);
    assert.equal(F.runLog.length, 0, "zero run for bad id " + bad);
    assert.equal(F.loaded.size, 0);
    await ch.close();
  }
});

test("reused ID (present before load) refuses with zero run", async () => {
  const F = makeFake({ fail: { loadId: 9 } });
  // Pre-occupy Script9 in the scripting inventory before load. The fake must
  // still load it (real KWin would, reusing the numeric path), so the
  // channel's preKids guard must refuse before run.
  F.loaded.set("some-foreign", 9);
  const origLoad = F.io.exec;
  F.io.exec = async (argv, o) => {
    const s = argv.map(String).join(" ");
    if (s.indexOf("loadScript") >= 0 && s.indexOf("isScriptLoaded") < 0 && s.indexOf("unloadScript") < 0) {
      const plugin = argv[argv.length - 1];
      F.loaded.set(plugin, 9);
      F.pendingPlugin = plugin;
      return { status: 0, stdout: JSON.stringify({ type: "i", data: [9] }) + "\n", stderr: "" };
    }
    return origLoad(argv, o);
  };
  const ch = await channel(F);
  await assert.rejects(ch.run("emit({});", "reuse"), KwinChannelError);
  assert.equal(F.runLog.length, 0);
  // The foreign occupant must remain untouched (never unloaded by us). The
  // refused run still unloads the exact owned plugin name once.
  assert.ok(F.loaded.has("some-foreign"));
  await assert.rejects(ch.close(), KwinChannelError);
});

test("incorrect exact introspection (no run interface) refuses with zero run", async () => {
  const F = makeFake({ fail: { badIntro: true } });
  const ch = await channel(F);
  await assert.rejects(ch.run("emit({});", "bad-intro"), KwinChannelError);
  assert.equal(F.runLog.length, 0);
  assert.equal(F.loaded.size, 0, "unloaded after refusal");  await ch.close();
});

test("missing / duplicate / error nonce receipt refuses", async () => {
  // Missing: helper body emits nothing.
  {
    const F = makeFake();
    const ch = await channel(F);
    await assert.rejects(ch.run("var x = 1;", "no-receipt"), KwinChannelError);
    assert.equal(F.loaded.size, 0);
    await ch.close();
  }
  // Duplicate: body emits twice.
  {
    const F = makeFake();
    const ch = await channel(F);
    await assert.rejects(ch.run("emit({a:1}); emit({a:2});", "dup"), KwinChannelError);
    assert.equal(F.loaded.size, 0);
    await ch.close();
  }
  // Error object present: body throws.
  {
    const F = makeFake();
    const ch = await channel(F);
    await assert.rejects(ch.run("throw new Error('nope');", "helper-error"), /helper error: nope/);
    assert.equal(F.loaded.size, 0);
    await ch.close();
  }
  // Foreign nonce line present in the same journal window. The pre-run
  // cursor is taken before the run journal query, so the foreign line must
  // be pushed into the post-cursor window, not at fake construction time.
  {
    const F = makeFake();
    const origJournal = F.io.exec;
    F.io.exec = async (argv, o) => {
      const s = argv.map(String).join(" ");
      if (s.indexOf("journalctl") >= 0 && s.indexOf("--after-cursor") >= 0 && !F._foreignInjected) {
        F._foreignInjected = true;
        F.pushJournal(F.kwinPid, "js: omnitiler:live:" + "ff".repeat(16) + ":{\"evil\":1}");
      }
      return origJournal(argv, o);
    };
    const ch = await channel(F);
    await assert.rejects(ch.run("emit({ok:1});", "foreign"), /foreign-nonce/);
    assert.equal(F.loaded.size, 0);
    await ch.close();
  }
});

test("KWin identity change before run refuses with zero run", async () => {
  const F = makeFake();
  const origExec = F.io.exec;
  F.io.exec = async (argv, o) => {
    const s = argv.map(String).join(" ");
    if (s.indexOf("loadScript") >= 0 && s.indexOf("isScriptLoaded") < 0 && s.indexOf("unloadScript") < 0) {
      F.fail.tickChange = true;
      F.state.phase = "during";
    }
    return origExec(argv, o);
  };
  const ch = await channel(F);
  const before = F.execLog.length;
  await assert.rejects(ch.run("emit({});", "ident-drift"), /KWin identity changed/);
  assert.equal(F.runLog.length, 0);
  // Cleanup refuses after identity drift.
  assert.equal(F.unloadCalls || 0, 0);
  // Post-load drift preserves the unresolved owned entry.
  await assert.rejects(ch.close(), /KWin identity changed/);
  assert.equal(F.unloadCalls || 0, 0, "no blind retry after drifted cleanup");
});

test("run failure still unloads exactly once and propagates", async () => {
  const F = makeFake({ fail: { runStatus: 1 } });
  const ch = await channel(F);
  await assert.rejects(ch.run("emit({});", "run-fail"), KwinChannelError);
  assert.equal(F.unloadCalls, 1);
  assert.equal(F.loaded.size, 0);
  await ch.close();
});

test("unload=false or still-loaded/object-present propagates; close idempotent, no blind retry", async () => {
  // unloadScript returns false.
  {
    const F = makeFake({ fail: { unloadFalse: true } });
    const ch = await channel(F);
    await assert.rejects(ch.run("emit({ok:1});", "unload-false"), /unloadScript did not return TRUE/);
    assert.ok(F.unloadCalls === 1);
    // close() must not blindly retry the ambiguous unload.
    await assert.rejects(ch.close(), /unloadScript did not return TRUE/);
    assert.ok(F.unloadCalls === 1, "no second unload on close");
  }
  // isScriptLoaded still true after unload (fake keeps reporting loaded).
  // stillLoaded must only affect the post-unload read, so the flag is armed
  // after the run reaches unload, not for the pre-check.
  {
    const F = makeFake({ fail: { stillLoaded: true } });
    F.fail.stillLoaded = false;
    const origUnload = F.io.exec;
    F.io.exec = async (argv, o) => {
      const s = argv.map(String).join(" ");
      if (s.indexOf("unloadScript") >= 0 && s.indexOf("isScriptLoaded") < 0) {
        F.fail.stillLoaded = true;
      }
      return origUnload(argv, o);
    };
    const ch = await channel(F);
    await assert.rejects(ch.run("emit({ok:1});", "still-loaded"), /still loaded after unload/);
    await assert.rejects(ch.close(), /still loaded after unload/);
    assert.ok(F.unloadCalls === 1);
  }
  // Object still in inventory after unload: model by leaving a phantom Script.
  {
    const F = makeFake();
    const origExec = F.io.exec;
    F.io.exec = async (argv, o) => {
      const r = await origExec(argv, o);
      if (argv.includes("unloadScript")) { F.loaded.set("phantom", 0); }
      return r;
    };
    const ch = await channel(F);
    await assert.rejects(ch.run("emit({ok:1});", "phantom"), /still present after unload/);
    await assert.rejects(ch.close(), /still present after unload/);
    assert.ok(F.unloadCalls === 1);
  }
});

test("occupied source path rejected before any bus call", async () => {
  const F = makeFake();
  // Precompute: the factory writes helper-<rand>.js; occupy a wildcard check
  // by seeding one nonce path the factory might pick is impractical, so we
  // instead occupy via a failing write? Simpler: occupy ALL via exists hook:
  // we hook exists to return true for any helper-*.js once.
  const origExists = F.io.exists;
  let occupied = false;
  F.io.exists = async (p) => {
    if (/helper-[0-9a-f]{32}\.js$/.test(p) && !occupied) { occupied = true; return true; }
    return origExists(p);
  };
  const ch = await channel(F);
  const before = F.execLog.length;
  await assert.rejects(ch.run("emit({});", "occupied"), /occupied/);
  assert.equal(F.runLog.length, 0);
  assert.ok(F.execLog.length === before, "no bus call after occupied-path refusal");
});

test("exact-PID journal + after-cursor receipt; foreign PID refused", async () => {
  // A journal entry whose _PID does not match the captured KWin PID must
  // fail rather than be silently filtered to a missing receipt. Without the
  // fake's _PID pre-filter in the query, the channel's _PID field check is
  // needed; inject the hostile entry only after cursor capture is too late
  // (query already scoped), so instead push foreign PID via the fake journal
  // with pidField and let the query surface it. Fake query filters by PID,
  // so this models the unfiltered case the channel guards against: push a
  // wrong-PID line into the window and surface it by querying unfiltered.
  {
    const F = makeFake();
    // Override the fake query to return all journal entries unfiltered, as a
    // real journalctl -o json would when _PID has no match. The channel's
    // query string still carries _PID=<exact kwin pid>; the fake returns a
    // foreign-PID entry anyway to prove the channel never guesses.
    const origExec = F.io.exec;
    F.io.exec = async (argv, o) => {
      const s = argv.map(String).join(" ");
      if (s.indexOf("journalctl") >= 0 && s.indexOf("--after-cursor") >= 0) {
        const ci = argv.indexOf("--after-cursor");
        const cur = argv[ci + 1];
        const n = Number(String(cur).replace("CUR", ""));
        const lines = F.journal.filter((e, ix) => ix >= n)
          .map((e) => JSON.stringify({ MESSAGE: e.msg, _PID: e.pidField !== undefined ? e.pidField : e.pid }));
        return { status: 0, stdout: lines.join("\n") + (lines.length ? "\n" : ""), stderr: "" };
      }
      return origExec(argv, o);
    };
    F.journalPid = 99999;
    const ch = await channel(F);
    await assert.rejects(ch.run("emit({ok:1});", "wrong-pid"), /wrong PID/);
    assert.equal(F.loaded.size, 0);
    await ch.close();
  }
});

test("cursor() independent of adapter production code", async () => {
  const F = makeFake();
  F.cursorPos = { x: 33, y: 44 };
  const ch = await channel(F);
  const c = await ch.cursor();
  assert.deepEqual(c, { x: 33, y: 44 });
  // Cursor used the channel run path (helper emitted {x,y} from workspace).
  const receipt = JSON.parse(F.written.find((w) => w.path.endsWith(".receipt.json")).data);
  assert.deepEqual(receipt.result, { x: 33, y: 44 });
  await ch.close();
});

test("caller error paths: bad evidenceDir, missing io, empty body", async () => {
  await assert.rejects(() => createKwinScriptChannel({ evidenceDir: "relative", io: makeFake().io }), KwinChannelError);
  await assert.rejects(() => createKwinScriptChannel({ evidenceDir: EVID_DIR, io: {} }), KwinChannelError);
  const F = makeFake();
  const ch = await channel(F);
  await assert.rejects(() => ch.run("", "empty"), KwinChannelError);
  await assert.rejects(() => ch.run("emit({});", ""), KwinChannelError);
});

test("target Script interface is unique and owns run", async () => {
  for (const mode of ["dupTarget", "misplacedRun", "noTarget"]) {
    const F = makeFake({ fail: { introMode: mode } });
    const ch = await channel(F);
    await assert.rejects(ch.run("emit({ok:1});", mode), KwinChannelError);
    assert.equal(F.runLog.length, 0);
    await ch.close();
  }
});

test("KWin identity change after run refuses cleanup with zero unload", async () => {
  for (const flag of ["ownerChange", "pidChange", "tickChange"]) {
    const F = makeFake();
    const origExec = F.io.exec;
    F.io.exec = async (argv, o) => {
      const r = await origExec(argv, o);
      if (argv.includes("run")) {
        F.fail[flag] = true;
        F.state.phase = "during";
      }
      return r;
    };
    const ch = await channel(F);
    await assert.rejects(ch.run("emit({ok:1});", "ident-after-run-" + flag), /KWin identity changed/);
    assert.equal(F.unloadCalls || 0, 0);
    await assert.rejects(ch.close(), /KWin identity changed/);
    assert.equal(F.unloadCalls || 0, 0);
  }
});

test("loadScript targets captured owner; pre-load owner drift causes zero loads", async () => {
  // Owner drift at pre-load: the captured :1.77 must be re-validated before
  // the load so a drifted bus owner yields zero load attempts.
  {
    const F = makeFake();
    const ch = await channel(F);
    F.fail.ownerChange = true;
    F.state.phase = "during";
    const before = F.execLog.length;
    await assert.rejects(ch.run("emit({ok:1});", "pre-load-drift"), /KWin identity changed/);
    const loads = F.execLog.slice(before).filter((l) => l.indexOf("loadScript") >= 0 && l.indexOf("isScriptLoaded") < 0 && l.indexOf("unloadScript") < 0);
    assert.equal(loads.length, 0, "zero loads after pre-load owner drift");
    assert.equal(F.runLog.length, 0);
    assert.equal(F.unloadCalls || 0, 0);
    await ch.close();
  }
  // Successful load targets the exact unique owner captured at channel setup.
  {
    const F = makeFake();
    const ch = await channel(F);
    const before = F.execLog.length;
    const res = await ch.run("emit({ok:1});", "owner-target");
    assert.deepEqual(res, { ok: 1 });
    const loads = F.execLog.slice(before).filter((l) => l.indexOf("loadScript") >= 0 && l.indexOf("isScriptLoaded") < 0 && l.indexOf("unloadScript") < 0);
    assert.equal(loads.length, 1, "exactly one load");
    assert.ok(loads[0].indexOf(F.kwinOwner) >= 0, "load targets captured owner: " + loads[0]);
    assert.ok(loads[0].indexOf("org.kde.KWin") < 0, "load must not target well-known name: " + loads[0]);
    await ch.close();
  }
});

test("generateHelperSource contains emit + workspace-global body, no imports", () => {
  const src = generateHelperSource("emit({a:1});", "ab".repeat(16));
  assert.ok(src.indexOf("console.log(PREFIX + JSON.stringify(value))") >= 0);
  assert.ok(src.indexOf("workspace") < 0, "no workspace reference outside caller body");
  assert.ok(src.indexOf("import ") < 0);
  assert.ok(src.indexOf("console.log(PREFIX") >= 0);
  assert.throws(() => generateHelperSource("", "ab".repeat(16)), KwinChannelError);
  assert.ok(KWIN_CHANNEL_CURSOR_BODY.indexOf("cursorPos") >= 0);
  assert.ok(KWIN_CHANNEL_MARK === "omnitiler:live:");
  assert.ok(KWIN_CHANNEL_PLUGIN_PREFIX === "omnitiler-live-");
});

// ---------------------------------------------------------------------------
// Fixture preamble / readFixtureState / joinFixtureWindows: VM-only, no io.
// The fake channel runs the body against a fake workspace in node:vm and
// captures the single emit() value (cross-realm JSON normalize before assert).
// ---------------------------------------------------------------------------

function fakeNativeWorkspace() {
  const desktop = { id: "d1" };
  const left = { name: "DP-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } };
  const right = { name: "DP-2", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } };
  const owned = {
    internalId: "u-owned", pid: 4242, resourceClass: "OmniTilerTestWindow",
    captionNormal: "OmniTilerTestWindow a run1", caption: "",
    frameGeometry: { x: 10.2, y: 20.7, width: 640.4, height: 480.6 },
    output: left, desktops: [desktop], minimized: false, hidden: false,
    desktopWindow: false, dock: false,
  };
  const foreign = {
    internalId: "u-foreign", pid: 9999, resourceClass: "SomeOtherApp",
    captionNormal: "Other", caption: "",
    frameGeometry: { x: 0, y: 0, width: 100, height: 100 },
    output: left, desktops: [desktop], minimized: false, hidden: false,
    desktopWindow: false, dock: false,
  };
  const desktopWin = { ...foreign, internalId: "u-desk", desktopWindow: true };
  return {
    cursorPos: { x: 5, y: 7 },
    desktops: [desktop],
    currentDesktop: desktop,
    activeScreen: left,
    activeWindow: owned,
    screens: [left, right],
    windowList: () => [foreign, owned, desktopWin],
    windowAt: () => [owned, foreign],
    currentDesktopForScreen: (o) => desktop,
  };
}

function vmChannel(workspace) {
  const calls = [];
  return {
    calls,
    async run(body, label) {
      calls.push({ body, label });
      let captured;
      vm.runInNewContext(
        "(function(){ " + body + " })();",
        { workspace, emit: (v) => { captured = v; } },
      );
      return JSON.parse(JSON.stringify(captured));
    },
  };
}

test("readFixtureState returns only fixture windows with truthful raw facts", async () => {
  const ch = vmChannel(fakeNativeWorkspace());
  // Desktop-typed active: raw active stays the desktop window (truthful).
  // Swap activeWindow to the desktop window to prove no fixture filtering of
  // the raw active fields.
  const j = { uuid: "u-owned", pid: 4242, title: "OmniTilerTestWindow a run1" };
  const s = await readFixtureState(ch, [j], "fixture-state");
  assert.deepEqual(s.desktops, ["d1"]);
  assert.equal(s.active, "u-owned");
  assert.equal(s.activeClass, "OmniTilerTestWindow");
  assert.equal(s.activeDesktop, false);
  assert.equal(s.activeDock, false);
  assert.equal(s.activeOutput, "DP-1");
  assert.deepEqual(s.cursor, { x: 5, y: 7 });
  assert.equal(s.hit, "u-owned");
  assert.deepEqual(s.outputs, [
    { name: "DP-1", rect: { x: 0, y: 0, w: 1920, h: 1080 }, current: "d1" },
    { name: "DP-2", rect: { x: 1920, y: 0, w: 1920, h: 1080 }, current: "d1" },
  ]);
  assert.equal(s.owned.length, 1, "only the fixture window is owned");
  assert.deepEqual(s.owned[0], {
    uuid: "u-owned", pid: 4242, title: "OmniTilerTestWindow a run1",
    frame: { x: 10, y: 21, w: 640, h: 481 }, out: "DP-1", desks: ["d1"],
    minimized: false, hidden: false,
  });
  assert.equal(ch.calls[0].label, "fixture-state");
  assert.ok(ch.calls[0].body.startsWith(KWIN_FIXTURE_PREAMBLE));

  const ws = fakeNativeWorkspace();
  const desk = ws.windowList().find((w) => w.internalId === "u-desk");
  ws.activeWindow = desk;
  const s2 = await readFixtureState(vmChannel(ws), [], "desk-active");
  assert.equal(s2.active, "u-desk");
  assert.equal(s2.activeClass, "SomeOtherApp");
  assert.equal(s2.activeDesktop, true);
});

test("readFixtureState retains known joins and rejects lost/duplicate joins", async () => {
  const keep = { uuid: "u-owned", pid: 4242, title: "OmniTilerTestWindow a run1" };
  const ok = await readFixtureState(vmChannel(fakeNativeWorkspace()), [keep], "keep");
  assert.equal(ok.owned.length, 1);
  await assert.rejects(
    readFixtureState(vmChannel(fakeNativeWorkspace()), [{ ...keep, pid: 1 }], "wrongpid"),
    /fixture join lost/,
  );
  await assert.rejects(
    readFixtureState(vmChannel(fakeNativeWorkspace()), [{ ...keep, title: "nope" }], "wrongtitle"),
    /fixture join lost/,
  );
  await assert.rejects(
    readFixtureState(vmChannel(fakeNativeWorkspace()), [{ ...keep, uuid: "missing" }], "missing"),
    /fixture join lost/,
  );
});

test("joinFixtureWindows maps caller ids to exact unique pid+title natives", () => {
  const native = {
    owned: [
      { uuid: "u1", pid: 11, title: "T one" },
      { uuid: "u2", pid: 22, title: "T two" },
    ],
  };
  const got = joinFixtureWindows([
    { id: "a", pid: 11, title: "T one", extra: true },
    { id: "b", pid: 22, title: "T two" },
  ], native);
  assert.deepEqual(got, [
    { id: "a", pid: 11, title: "T one", uuid: "u1" },
    { id: "b", pid: 22, title: "T two", uuid: "u2" },
  ]);
  // Duplicate snapshot ids rejected even when both would match.
  assert.throws(() => joinFixtureWindows([
    { id: "a", pid: 11, title: "T one" }, { id: "a", pid: 22, title: "T two" },
  ], native), /duplicate snapshot id/);
  // Duplicate native UUIDs (same window reached by two snapshots) rejected.
  assert.throws(() => joinFixtureWindows([
    { id: "a", pid: 11, title: "T one" }, { id: "b", pid: 11, title: "T one" },
  ], native), /duplicate native uuid/);
  // Missing match rejected.
  assert.throws(() => joinFixtureWindows([
    { id: "x", pid: 33, title: "T one" },
  ], native), /fixture join lost/);
  // Ambiguous match (two natives share pid+title) rejected.
  const amb = { owned: [
    { uuid: "u1", pid: 11, title: "T one" }, { uuid: "u9", pid: 11, title: "T one" },
  ] };
  assert.throws(() => joinFixtureWindows([{ id: "a", pid: 11, title: "T one" }], amb), /fixture join lost/);
});

test("preamble joined(j) rejects foreign class and wrong pid/title before any setter", async () => {
  const ws = fakeNativeWorkspace();
  const ch = vmChannel(ws);
  const good = { uuid: "u-owned", pid: 4242, title: "OmniTilerTestWindow a run1" };
  // A mutating caller body must not run when joined() rejects: the setter
  // flag is set only after joined() returns the exact window.
  const setter = (j) => KWIN_FIXTURE_PREAMBLE
    + "var w=joined(" + JSON.stringify(j) + ");globalThis.__set=true;emit({sent:true});";
  const runSetter = async (j) => {
    const workspace = fakeNativeWorkspace();
    let captured;
    try {
      vm.runInNewContext("(function(){ " + setter(j) + " })();",
        { workspace, emit: (v) => { captured = v; } });
    } catch (e) {
      return { error: String(e.message), captured };
    }
    return { captured };
  };
  const okR = await runSetter(good);
  assert.deepEqual(JSON.parse(JSON.stringify(okR.captured)), { sent: true });
  // Foreign class: same uuid/pid/title lookup against a non-fixture class.
  const foreignWs = fakeNativeWorkspace();
  foreignWs.windowList = () => [{
    internalId: "u-owned", pid: 4242, resourceClass: "Other",
    captionNormal: "OmniTilerTestWindow a run1", caption: "",
  }];
  assert.throws(() => vm.runInNewContext(
    "(function(){ " + setter(good) + " })();",
    { workspace: foreignWs, emit: () => {} },
  ), /fixture join lost/);
  const wrongPid = await runSetter({ ...good, pid: 1111 });
  assert.match(String(wrongPid.error), /fixture join lost/);
  const wrongTitle = await runSetter({ ...good, title: "nope" });
  assert.match(String(wrongTitle.error), /fixture join lost/);
  assert.equal(ch.calls.length, 0, "no channel call: pure VM decode only");
});

test("preamble arr decoder accepts empty/valid and rejects invalid/overbound", () => {
  const run = (expr) => {
    let captured;
    vm.runInNewContext(
      "(function(){ " + KWIN_FIXTURE_PREAMBLE + "emit(arr(" + expr + ")); })();",
      { workspace: fakeNativeWorkspace(), emit: (v) => { captured = v; } },
    );
    return JSON.parse(JSON.stringify(captured));
  };
  assert.deepEqual(run("[]"), []);
  assert.deepEqual(run("[1,2,3]"), [1, 2, 3]);
  assert.deepEqual(run("({length:2,0:'a',1:'b'})"), ["a", "b"]);
  for (const bad of ["null", "undefined", "({})", "({length:-1})", "({length:1.5,0:'a'})",
    "({length:4097})", "({length:'2',0:'a',1:'b'})"]) {
    assert.throws(() => run(bad), /bad list/, bad);
  }
});
