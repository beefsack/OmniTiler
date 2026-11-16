// scripts/kde-live-session.test.mjs: hermetic fake-io tests for createLiveInput.
// No live calls: all effects go through an in-memory fake io.
import { test } from "node:test";
import assert from "node:assert/strict";
import { createLiveInput, createLiveFixture, createDevSession } from "./kde-live-session.mjs";

const SOCK = "/fixture/input.sock";
const RELEASE = [105, 106, 42, 54, 29, 97, 56, 100, 125, 126, 1];

function makeFake(opts = {}) {
  const F = {
    execLog: [], spawnCount: 0, sleepCount: 0, killLog: [],
    socketExists: opts.socketExists ?? false,
    alive: true, tick: "t1", tickOverride: null,
    locked: opts.locked ?? "no",
    failNext: null, spawnPid: 4242,
    records: [],
  };
  const io = {
    exec: async (argv, o = {}) => {
      F.execLog.push({ argv: argv.slice(), env: o.env || {} });
      if (F.failNext) {
        const m = F.failNext;
        if (Array.isArray(m) ? m.some((s) => argv.join(" ").includes(s)) : argv.join(" ").includes(m)) {
          F.failNext = null;
          return { status: 1, stdout: "", stderr: "injected failure" };
        }
      }
      if (argv[0] === "loginctl") {
        const out = F.locked === "unknown" ? "LockedHint=maybe\n" : "LockedHint=" + F.locked + "\n";
        return { status: F.locked === "error" ? 1 : 0, stdout: out, stderr: "" };
      }
      return { status: 0, stdout: "", stderr: "" };
    },
    spawnBackground: async (argv) => {
      F.spawnCount += 1;
      F.spawnArgv = argv.slice();
      F.alive = true;
      if (!F.socketTimeout) { F.socketExists = true; }
      return {
        pid: F.spawnPid,
        kill: async (sig) => { F.killLog.push(sig); F.alive = false; },
        exited: async () => (F.alive ? null : { code: 0, signal: null }),
      };
    },
    exists: async () => F.socketExists,
    procIdentity: async () => (F.alive ? (F.tickOverride ?? F.tick) : null),
    sleep: async () => { F.sleepCount += 1; },
    now: () => 0,
  };
  return { F, io };
}

function makeSession(F, io, extra = {}) {
  return createLiveInput({
    io, root: "/repo", env: { XDG_SESSION_ID: "2", PATH: "/bin" },
    socket: SOCK, record: (e, d) => F.records.push([e, d]), ...extra,
  });
}

async function assertRejects(p, re) {
  await assert.rejects(p, re ? (e) => re.test(e.message) : undefined);
}

test("import and factory perform zero effects", async () => {
  const { F, io } = makeFake();
  const s = makeSession(F, io);
  assert.equal(F.spawnCount, 0);
  assert.equal(F.execLog.length, 0);
  assert.equal(F.records.length, 0);
  assert.equal(typeof s.start, "function");
  assert.equal(typeof s.keys, "function");
  assert.equal(typeof s.moveRelative, "function");
  assert.equal(typeof s.release, "function");
  assert.equal(typeof s.checkLock, "function");
  assert.equal(typeof s.close, "function");
});

test("occupied socket: zero spawn, start rejects", async () => {
  const { F, io } = makeFake({ socketExists: true });
  F.socketExists = true;
  const s = makeSession(F, io);
  await assertRejects(s.start(), /occupied/);
  assert.equal(F.spawnCount, 0);
  assert.equal(F.execLog.length, 0);
});

test("successful start spawns exactly one daemon and unlocked receipt", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  const r = await s.start();
  assert.equal(F.spawnCount, 1);
  assert.equal(F.spawnArgv[0], "ydotoold");
  assert.deepEqual(r, { pid: 4242, socket: SOCK });
  const lockCalls = F.execLog.filter((c) => c.argv[0] === "loginctl");
  assert.equal(lockCalls.length, 1);
  assert.deepEqual(lockCalls[0].argv, ["loginctl", "show-session", "2", "-p", "LockedHint"]);
  assert.deepEqual(F.records.filter(([e]) => e === "lock-check").map(([, d]) => d), [{ locked: false }]);
  assert.ok(F.records.some(([e]) => e === "start"));
  assert.ok(F.records.some(([e]) => e === "socket"));
});

test("keys focusLeft emits chord then complete release list then lock receipt", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  await s.start();
  F.execLog.length = 0;
  await s.keys("focusLeft");
  const keyCalls = F.execLog.filter((c) => c.argv[0] === "ydotool" && c.argv[1] === "key");
  assert.equal(keyCalls.length, 2);
  // focusLeft chord [125,105]: downs 125:1 105:1 then ups 105:0 125:0
  assert.deepEqual(keyCalls[0].argv, ["ydotool", "key", "125:1", "105:1", "105:0", "125:0"]);
  assert.deepEqual(keyCalls[1].argv, ["ydotool", "key"].concat(RELEASE.map((c) => c + ":0")));
  assert.equal(keyCalls[0].env.YDOTOOL_SOCKET, SOCK);
  assert.equal(keyCalls[1].env.YDOTOOL_SOCKET, SOCK);
  const locks = F.execLog.filter((c) => c.argv[0] === "loginctl");
  assert.equal(locks.length, 1);
  // Order: chord, release, then lock check.
  const order = F.execLog.map((c) => c.argv[0] + ":" + (c.argv[1] || ""));
  assert.deepEqual(order, ["ydotool:key", "ydotool:key", "loginctl:show-session"]);
});

test("keys focusRight uses focusRight chord and guarded release", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  await s.start();
  F.execLog.length = 0;
  await s.keys("focusRight");
  const keyCalls = F.execLog.filter((c) => c.argv[0] === "ydotool");
  assert.deepEqual(keyCalls[0].argv, ["ydotool", "key", "125:1", "106:1", "106:0", "125:0"]);
  assert.deepEqual(keyCalls[1].argv, ["ydotool", "key"].concat(RELEASE.map((c) => c + ":0")));
});

test("moveRelative has no --absolute and integer dx/dy only", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  await s.start();
  F.execLog.length = 0;
  await s.moveRelative(7, -3);
  const move = F.execLog.find((c) => c.argv[1] === "mousemove");
  assert.ok(move);
  assert.ok(!move.argv.includes("--absolute"));
  assert.deepEqual(move.argv, ["ydotool", "mousemove", "-x", "7", "-y", "-3"]);
  const rel = F.execLog.filter((c) => c.argv[1] === "key");
  assert.deepEqual(rel[0].argv, ["ydotool", "key"].concat(RELEASE.map((c) => c + ":0")));
  assert.equal(F.execLog.filter((c) => c.argv[0] === "loginctl").length, 1);
  await assertRejects(s.moveRelative(1.5, 0), /integer/);
  await assertRejects(s.moveRelative(0, "2"), /integer/);
});

test("injection failure still performs full release and lock check", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  await s.start();
  F.execLog.length = 0;
  F.failNext = "125:1"; // fail the chord injection only
  await assertRejects(s.keys("focusLeft"), /key injection failed/);
  const rel = F.execLog.filter((c) => c.argv[1] === "key");
  assert.equal(rel.length, 2);
  assert.deepEqual(rel[1].argv, ["ydotool", "key"].concat(RELEASE.map((c) => c + ":0")));
  assert.equal(F.execLog.filter((c) => c.argv[0] === "loginctl").length, 1);
});

test("release failure still performs lock check and surfaces error", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  await s.start();
  F.execLog.length = 0;
  F.failNext = "54:0"; // unique to the complete release list, absent from the chord
  await assertRejects(s.keys("focusLeft"), /key release failed/);
  assert.equal(F.execLog.filter((c) => c.argv[0] === "loginctl").length, 1);
});

test("guarded release failure is combined, not swallowed", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  await s.start();
  F.execLog.length = 0;
  F.failNext = "54:0";
  await assert.rejects(s.keys("focusLeft"), (e) => /key release failed/.test(e.message));
  assert.equal(F.execLog.filter((c) => c.argv[0] === "loginctl").length, 1);
});

test("close still SIGTERMs and verifies absence if release fails", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  await s.start();
  F.execLog.length = 0;
  F.failNext = "105:0";
  await assertRejects(s.close(), /key release failed/);
  assert.deepEqual(F.killLog, ["SIGTERM"]);
  assert.equal(F.alive, false);
  assert.equal(F.execLog.filter((c) => c.argv[0] === "loginctl").length, 1);
});

test("close still SIGTERMs and verifies absence if lock fails", async () => {
  const { F, io } = makeFake({ locked: "yes" });
  const s = makeSession(F, io);
  // Start emits a lock receipt check; force locked only for close by flipping after start.
  F.locked = "no";
  await s.start();
  F.execLog.length = 0;
  F.locked = "yes";
  await assertRejects(s.close(), /session locked/);
  assert.deepEqual(F.killLog, ["SIGTERM"]);
  assert.equal(F.alive, false);
});

test("close second call is no-op with no duplicate stop", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  await s.start();
  await s.close();
  assert.deepEqual(F.killLog, ["SIGTERM"]);
  const execLen = F.execLog.length;
  await s.close();
  assert.deepEqual(F.killLog, ["SIGTERM"]);
  assert.equal(F.execLog.length, execLen);
});

test("identity drift refuses input and signal", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  await s.start();
  F.tickOverride = "drifted";
  await assertRejects(s.keys("focusLeft"), /drifted|refusing/);
  // guarded() primary fails on owned() so no injection happens; but release also drifts, checkLock still runs.
  assert.ok(!F.execLog.some((c) => c.argv[1] === "key" && c.argv.includes("125:1")));
  F.killLog.length = 0;
  await assertRejects(s.close(), /drifted|refusing/);
  assert.equal(F.killLog.length, 0);
  // Drifted daemon stays alive (identity unverifiable: never signalled).
  assert.equal(F.alive, true);
});

test("socket wait bounded failure then close stops daemon", async () => {
  const { F, io } = makeFake({ locked: "no", socketTimeout: true });
  F.socketTimeout = true;
  F.socketExists = false;
  const s = makeSession(F, io);
  await assertRejects(s.start(), /bounded wait/);
  assert.equal(F.spawnCount, 1);
  assert.ok(F.sleepCount >= 49);
  // start failed during poll but started flag and child handle remain: close stops it.
  F.socketExists = true;
  await s.close();
  assert.deepEqual(F.killLog, ["SIGTERM"]);
  assert.equal(F.alive, false);
});

test("unsafe key names are rejected before any injection", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = makeSession(F, io);
  await s.start();
  F.execLog.length = 0;
  for (const bad of ["focusUp", "moveLeft", "workspace1", "", null, undefined]) {
    await assertRejects(s.keys(bad), /focusLeft\/focusRight/);
  }
  assert.equal(F.execLog.filter((c) => c.argv[0] === "ydotool").length, 0);
});

test("LockedHint=yes refuses and never unlocks", async () => {
  const { F, io } = makeFake({ locked: "yes" });
  const s = makeSession(F, io);
  await assertRejects(s.start(), /session locked/);
  assert.ok(!F.execLog.some((c) => /unlock|set.*[Ll]ock/i.test(c.argv.join(" "))));
  assert.deepEqual(F.execLog.filter((c) => c.argv[0] === "loginctl").map((c) => c.argv),
    [["loginctl", "show-session", "2", "-p", "LockedHint"]]);
});

test("LockedHint unknown refuses and never unlocks", async () => {
  const { F, io } = makeFake({ locked: "unknown" });
  const s = makeSession(F, io);
  await assertRejects(s.start(), /unknown LockedHint/);
  assert.ok(!F.execLog.some((c) => /unlock|set.*[Ll]ock/i.test(c.argv.join(" "))));
});

test("missing XDG_SESSION_ID refuses and never unlocks", async () => {
  const { F, io } = makeFake({ locked: "no" });
  const s = createLiveInput({ io, root: "/repo", env: { PATH: "/bin" }, socket: SOCK, record: null });
  await assertRejects(s.start(), /XDG_SESSION_ID/);
});

const FIX_DIR = "/fixture/run";
const FIX_SOCK = "/fixture/run/fixture.sock";
const FIX_BIN = "/mock/tiler";
const FIX_NONCE = "a".repeat(64);
const FIX_ANN = JSON.stringify({ socket: "/fixture/run/fixture.sock", pid: 77, nonce: "a".repeat(64) });

function makeFixtureFake(opts = {}) {
  const F = {
    execLog: [], spawnLog: [], killLog: [], records: [],
    alive: false, socketExists: false, dirExists: false,
    tick: "starttick", tickOverride: null,
    announceLine: opts.announceLine ?? FIX_ANN,
    waitLineThrows: opts.waitLineThrows ?? false,
    shutdownMode: opts.shutdownMode ?? "graceful",
    nestedBad: false,
  };
  const io = {
    exec: async (argv, o = {}) => {
      F.execLog.push({ argv: argv.slice(), env: o.env || {} });
      let req = {};
      try { req = JSON.parse(argv[argv.length - 1] || "{}"); } catch (e) { req = {}; }
      const id = req.request_id;
      if (req.op === "shutdown") {
        if (F.shutdownMode === "fail") return { status: 1, stdout: "", stderr: "shutdown failed" };
        if (F.shutdownMode === "stillAlive") return { status: 0, stdout: JSON.stringify({ request_id: id, ok: true, result: {} }) + "\n", stderr: "" };
        F.alive = false; F.socketExists = false;
        return { status: 0, stdout: JSON.stringify({ request_id: id, ok: true, result: {} }) + "\n", stderr: "" };
      }
      if (F.nestedBad) {
        return { status: 0, stdout: JSON.stringify({ request_id: id, ok: true, result: { windows: [{ ok: false }] } }) + "\n", stderr: "" };
      }
      return { status: 0, stdout: JSON.stringify({ request_id: id, ok: true, result: { windows: [{ ok: true }] } }) + "\n", stderr: "" };
    },
    spawnBackground: async (argv, o = {}) => {
      F.spawnLog.push({ argv: argv.slice(), env: o.env || {} });
      F.alive = true; F.socketExists = true; F.dirExists = true;
      return {
        pid: 77,
        waitLine: async (pred) => {
          if (F.waitLineThrows) throw new Error("timeout");
          const line = F.announceLine;
          if (pred && !pred(line)) throw new Error("no match");
          return line;
        },
        kill: async (sig) => { F.killLog.push(sig); F.alive = false; F.socketExists = false; },
        exited: async () => (F.alive ? null : { code: 0, signal: null }),
      };
    },
    exists: async (p) => (p === FIX_SOCK ? F.socketExists : p === FIX_DIR ? F.dirExists : false),
    procIdentity: async (pid) => (pid === 77 ? (F.alive ? (F.tickOverride ?? F.tick) : null) : null),
    sleep: async () => {},
    now: () => 0,
  };
  return { F, io };
}

function makeFixtureSession(F, io, extra = {}) {
  return createLiveFixture({
    io, root: "/repo", env: { PATH: "/bin", WAYLAND_DISPLAY: "wayland-0", WAYLAND_SOCKET: "/run/wayland" },
    dir: FIX_DIR, binary: FIX_BIN, record: (e, d) => F.records.push([e, d]), ...extra,
  });
}

test("fixture factory performs zero effects", async () => {
  const { F, io } = makeFixtureFake();
  const s = makeFixtureSession(F, io);
  assert.equal(F.spawnLog.length, 0);
  assert.equal(F.execLog.length, 0);
  assert.equal(F.records.length, 0);
  assert.equal(F.killLog.length, 0);
  assert.equal(F.alive, false);
  assert.equal(typeof s.start, "function");
  assert.equal(typeof s.send, "function");
  assert.equal(typeof s.close, "function");
});

test("fixture valid start spawns direct binary serve and drops WAYLAND env", async () => {
  const { F, io } = makeFixtureFake();
  const s = makeFixtureSession(F, io);
  const r = await s.start();
  assert.deepEqual(r, { pid: 77, socket: FIX_SOCK });
  assert.equal(F.spawnLog.length, 1);
  assert.deepEqual(F.spawnLog[0].argv, [FIX_BIN, "serve", "--dir", FIX_DIR]);
  assert.ok(!("WAYLAND_DISPLAY" in F.spawnLog[0].env));
  assert.ok(!("WAYLAND_SOCKET" in F.spawnLog[0].env));
  assert.equal(F.spawnLog[0].env.PATH, "/bin");
  assert.ok(F.records.some(([e]) => e === "start"));
  assert.ok(F.records.some(([e]) => e === "announce"));
  await s.close();
});

test("fixture wrong announcement PID/path/nonce rejects then close stops only child", async () => {
  const bad = [
    JSON.stringify({ socket: FIX_SOCK, pid: 999, nonce: FIX_NONCE }),
    JSON.stringify({ socket: "/fixture/run/other.sock", pid: 77, nonce: FIX_NONCE }),
    JSON.stringify({ socket: FIX_SOCK, pid: 77, nonce: "a".repeat(63) }),
  ];
  for (const announceLine of bad) {
    const { F, io } = makeFixtureFake({ announceLine });
    const s = makeFixtureSession(F, io);
    await assertRejects(s.start(), /bad socket\/pid\/nonce/);
    assert.equal(F.spawnLog.length, 1);
    assert.equal(F.killLog.length, 0);
    await s.close();
    assert.deepEqual(F.killLog, ["SIGTERM"]);
    assert.equal(F.alive, false);
    assert.equal(F.execLog.length, 0);
  }
});

test("fixture announcement timeout then close cleans up child", async () => {
  const { F, io } = makeFixtureFake({ waitLineThrows: true });
  const s = makeFixtureSession(F, io);
  await assertRejects(s.start(), /no JSON line/);
  assert.equal(F.spawnLog.length, 1);
  assert.equal(F.killLog.length, 0);
  await s.close();
  assert.deepEqual(F.killLog, ["SIGTERM"]);
  assert.equal(F.alive, false);
  assert.equal(F.socketExists, false);
});

test("fixture valid send and nested ok:false rejection", async () => {
  const { F, io } = makeFixtureFake();
  const s = makeFixtureSession(F, io);
  await s.start();
  const reply = await s.send({ v: 1, request_id: "r1", op: "list" });
  assert.equal(reply.request_id, "r1");
  assert.equal(reply.ok, true);
  assert.ok(F.execLog.length >= 1);
  const sent = F.execLog[F.execLog.length - 1];
  assert.equal(sent.argv[0], FIX_BIN);
  assert.equal(sent.argv[1], "send");
  assert.ok(sent.argv.includes(FIX_SOCK));
  assert.ok(sent.argv.includes(FIX_NONCE));
  F.nestedBad = true;
  await assertRejects(s.send({ v: 1, request_id: "r2", op: "list" }), /nested window ok:false/);
  F.nestedBad = false;
  await s.close();
});

test("fixture graceful close is idempotent with no SIGTERM", async () => {
  const { F, io } = makeFixtureFake();
  const s = makeFixtureSession(F, io);
  await s.start();
  await s.close();
  assert.deepEqual(F.killLog, []);
  assert.equal(F.alive, false);
  assert.equal(F.socketExists, false);
  const execN = F.execLog.length;
  const shutdowns = F.execLog.filter((c) => c.argv.join(" ").includes("shutdown"));
  assert.equal(shutdowns.length, 1);
  await s.close();
  assert.equal(F.execLog.length, execN);
  assert.deepEqual(F.killLog, []);
});

test("fixture shutdown failure still SIGTERMs owned child and propagates", async () => {
  const { F, io } = makeFixtureFake();
  const s = makeFixtureSession(F, io);
  await s.start();
  F.shutdownMode = "fail";
  await assertRejects(s.close(), /send failed/);
  assert.deepEqual(F.killLog, ["SIGTERM"]);
  assert.equal(F.alive, false);
  assert.equal(F.socketExists, false);
});

test("fixture still-alive shutdown timeout SIGTERMs once and propagates", async () => {
  const { F, io } = makeFixtureFake();
  const s = makeFixtureSession(F, io);
  await s.start();
  F.shutdownMode = "stillAlive";
  await assertRejects(s.close(), /did not exit/);
  assert.deepEqual(F.killLog, ["SIGTERM"]);
  assert.equal(F.alive, false);
  assert.equal(F.socketExists, false);
});

test("fixture identity drift refuses send and signal", async () => {
  const { F, io } = makeFixtureFake();
  const s = makeFixtureSession(F, io);
  await s.start();
  F.tickOverride = "drifted";
  const execN = F.execLog.length;
  await assertRejects(s.send({ v: 1, request_id: "r1", op: "list" }), /drifted|refusing/);
  assert.equal(F.execLog.length, execN);
  await assertRejects(s.close(), /drifted|refusing/);
  assert.deepEqual(F.killLog, []);
  assert.equal(F.alive, true);
});

// --- createDevSession: in-memory files/dirs Map, all io fake (incl. readdir) ---

const DEV_E = "/ev";
const DEV_RT = "/rt";
const DEV_PLAN_PTR = DEV_RT + "/omnitiler-dev/planner-log";

function makeDevFake(opts = {}) {
  const files = new Map(); // path -> { data, mode }
  const dirs = new Map(); // path -> mode
  const F = {
    execLog: [], writeLog: [], mkdirLog: [], records: [],
    runtimeNames: opts.runtimeNames ?? [],
    runtimeContents: opts.runtimeContents ?? {},
    plannerPointer: opts.plannerPointer ?? null, // string path or null => no pointer file
    plannerContents: opts.plannerContents ?? "planner-line\n",
    failDevOn: opts.failDevOn ?? false,
    failDevOff: opts.failDevOff ?? false,
    devStatus: opts.devStatus ?? "dev mode: DOWN",
    copyReadFail: opts.copyReadFail ?? false, // readdir throws
    writeFail: opts.writeFail ?? null, // substring of write path
  };
  const io = {
    exec: async (argv, o = {}) => {
      F.execLog.push({ argv: argv.slice(), o });
      if (argv[1] === "dev-on" && F.failDevOn) return { status: 1, stdout: "", stderr: "dev-on broke" };
      if (argv[1] === "dev-off" && F.failDevOff) return { status: 1, stdout: "", stderr: "dev-off broke" };
      if (argv[1] === "dev-status") return { status: 0, stdout: F.devStatus + "\n", stderr: "" };
      return { status: 0, stdout: "", stderr: "" };
    },
    exists: async (p) => files.has(p) || dirs.has(p) ||
      (p === DEV_PLAN_PTR ? F.plannerPointer !== null : p === F.plannerPointer),
    mkdir: async (d, mode) => { dirs.set(d, mode); F.mkdirLog.push({ d, mode }); },
    writeFile: async (p, data, mode) => {
      F.writeLog.push({ p, mode });
      if (F.writeFail && p.includes(F.writeFail)) throw new Error("write broke for " + p);
      files.set(p, { data: String(data), mode });
    },
    readFile: async (p) => {
      if (p === DEV_PLAN_PTR) {
        if (F.plannerPointer === null) throw new Error("ENOENT pointer");
        return F.plannerPointer + "\n";
      }
      if (files.has(p)) return files.get(p).data;
      if (p === F.plannerPointer) return F.plannerContents;
      if (p.startsWith(DEV_RT + "/") && Object.hasOwn(F.runtimeContents, p.slice(DEV_RT.length + 1))) {
        return F.runtimeContents[p.slice(DEV_RT.length + 1)];
      }
      throw new Error("ENOENT " + p);
    },
    readdir: async (d) => {
      if (F.copyReadFail) throw new Error("readdir broke");
      if (d === DEV_RT) return F.runtimeNames.slice();
      return [];
    },
  };
  return { F, io, files, dirs };
}

function makeDevSession(F, io, extra = {}) {
  return createDevSession({
    io, root: "/repo", env: { XDG_RUNTIME_DIR: DEV_RT },
    evidenceDir: DEV_E, record: (e, d) => F.records.push([e, d]), ...extra,
  });
}

test("dev factory performs zero effects", async () => {
  const { F, io } = makeDevFake();
  const s = makeDevSession(F, io);
  assert.equal(F.execLog.length, 0);
  assert.equal(F.writeLog.length, 0);
  assert.equal(F.mkdirLog.length, 0);
  assert.equal(F.records.length, 0);
  assert.equal(typeof s.on, "function");
  assert.equal(typeof s.off, "function");
});

test("dev on creates both roots 0700 and JSON 0600 with exact argv/env/timeout", async () => {
  const { F, io, files, dirs } = makeDevFake();
  const s = makeDevSession(F, io);
  const r = await s.on("leg-a", { focusFollowsCursor: true });
  assert.deepEqual(r, {
    settingsRoot: "/ev/leg-a-settings", intentRoot: "/ev/leg-a-intent",
  });
  assert.equal(dirs.get("/ev/leg-a-settings"), 0o700);
  assert.equal(dirs.get("/ev/leg-a-intent"), 0o700);
  const sj = files.get("/ev/leg-a-settings/settings.json");
  assert.equal(sj.mode, 0o600);
  assert.deepEqual(JSON.parse(sj.data), { focusFollowsCursor: true });
  assert.equal(F.execLog.length, 1);
  const c = F.execLog[0];
  assert.deepEqual(c.argv, ["just", "dev-on", "--scope-class", "OmniTilerTestWindow",
    "--settings-root", "/ev/leg-a-settings", "--intent-root", "/ev/leg-a-intent"]);
  assert.equal(c.o.timeoutMs, 300000);
  assert.equal(c.o.cwd, "/repo");
  assert.equal(c.o.env.OMNITILER_TRACE, "1");
  assert.equal(c.o.env.XDG_RUNTIME_DIR, DEV_RT);
  assert.ok(F.records.some(([e]) => e === "dev-on"));
});

test("dev occupied root: zero writes and zero exec", async () => {
  const { F, io } = makeDevFake();
  io.mkdir = async (d, mode) => { F.mkdirLog.push({ d, mode }); };
  dirsPreoccupy(io, "/ev/leg-a-settings");
  const s = makeDevSession(F, io);
  await assertRejects(s.on("leg-a"), /occupied/);
  assert.equal(F.writeLog.length, 0);
  assert.equal(F.execLog.length, 0);
});

function dirsPreoccupy(io, path) {
  const origMk = io.mkdir;
  io.exists = async (p) => (p === path ? true : p.startsWith("/rt/") ? true : false);
}

test("dev failed dev-on blocks second on but off still performs teardown", async () => {
  const { F, io } = makeDevFake({ failDevOn: true });
  const s = makeDevSession(F, io);
  await assertRejects(s.on("leg-a"), /dev-on failed/);
  const execN = F.execLog.length;
  const writeN = F.writeLog.length;
  await assertRejects(s.on("leg-b"), /already attempted/);
  assert.equal(F.execLog.length, execN);
  assert.equal(F.writeLog.length, writeN);
  // attempted is true, so off() must still run dev-off/status and clear on verify
  await s.off();
  assert.deepEqual(F.execLog.slice(execN).map((c) => c.argv[1]), ["dev-off", "dev-status"]);
});

test("dev off copies exact-regex logs and planner pointer contents before dev-off", async () => {
  const names = ["omnitiler-dev.abc.log", "omnitiler-dev.XYZ-123.log", "omnitiler-dev.keep", "other.log"];
  const contents = {
    "omnitiler-dev.abc.log": "ABC",
    "omnitiler-dev.XYZ-123.log": "XYZ",
    "omnitiler-dev.keep": "KEEP",
    "other.log": "OTHER",
  };
  const { F, io, files, dirs } = makeDevFake({
    runtimeNames: names, runtimeContents: contents,
    plannerPointer: DEV_RT + "/omnitiler-planner-dev.p.log",
    plannerContents: "planner-line\n",
  });
  // plant planner pointer target as a runtime file readable by path
  const s = makeDevSession(F, io);
  await s.on("leg-a");
  files.set(DEV_RT + "/omnitiler-planner-dev.p.log", { data: "planner-line\n", mode: 0o644 });
  F.execLog.length = 0;
  await s.off();
  const dest = "/ev/runtime-logs-1";
  assert.equal(dirs.get(dest), 0o700);
  assert.equal(files.get(dest + "/omnitiler-dev.abc.log")?.data, "ABC");
  assert.equal(files.get(dest + "/omnitiler-dev.XYZ-123.log")?.data, "XYZ");
  assert.equal(files.has(dest + "/omnitiler-dev.keep"), false);
  assert.equal(files.has(dest + "/other.log"), false);
  assert.equal(files.get(dest + "/planner.log")?.data, "planner-line\n");
  const copy = F.records.find(([e]) => e === "dev-off-copy");
  assert.deepEqual(copy?.[1], { dir: dest, count: 2 });
  // Order: copy receipts/writes precede the dev-off exec.
  assert.deepEqual(F.execLog.map((c) => c.argv[1]), ["dev-off", "dev-status"]);
  assert.ok(F.records.some(([e]) => e === "dev-off"));
});

test("dev off with empty logs still creates copy dir and count 0 receipt", async () => {
  const { F, io, dirs } = makeDevFake({ runtimeNames: ["no-match.txt"], plannerPointer: null });
  const s = makeDevSession(F, io);
  await s.on("leg-a");
  F.execLog.length = 0;
  await s.off();
  const dest = "/ev/runtime-logs-1";
  assert.equal(dirs.get(dest), 0o700);
  const copy = F.records.find(([e]) => e === "dev-off-copy");
  assert.deepEqual(copy?.[1], { dir: dest, count: 0 });
  assert.deepEqual(F.execLog.map((c) => c.argv[1]), ["dev-off", "dev-status"]);
});

test("dev copy error still runs dev-off/status and error propagates", async () => {
  const { F, io } = makeDevFake({ copyReadFail: true });
  const s = makeDevSession(F, io);
  await s.on("leg-a");
  F.execLog.length = 0;
  await assertRejects(s.off(), /readdir broke/);
  assert.deepEqual(F.execLog.map((c) => c.argv[1]), ["dev-off", "dev-status"]);
  // attempted is NOT cleared on error: off() retries and fails the same way
  F.execLog.length = 0;
  await assertRejects(s.off(), /readdir broke/);
  assert.deepEqual(F.execLog.map((c) => c.argv[1]), ["dev-off", "dev-status"]);
});

test("dev dev-off nonzero and status-not-DOWN failures propagate", async () => {
  {
    const { F, io } = makeDevFake({ failDevOff: true });
    const s = makeDevSession(F, io);
    await s.on("leg-a");
    F.execLog.length = 0;
    await assertRejects(s.off(), /dev-off failed/);
    assert.deepEqual(F.execLog.map((c) => c.argv[1]), ["dev-off", "dev-status"]);
  }
  {
    const { F, io } = makeDevFake({ devStatus: "dev mode: UP" });
    const s = makeDevSession(F, io);
    await s.on("leg-a");
    F.execLog.length = 0;
    await assertRejects(s.off(), /not DOWN/);
    assert.deepEqual(F.execLog.map((c) => c.argv[1]), ["dev-off", "dev-status"]);
  }
});

test("dev verified off is idempotent", async () => {
  const { F, io } = makeDevFake();
  const s = makeDevSession(F, io);
  await s.on("leg-a");
  await s.off();
  const execN = F.execLog.length;
  const writeN = F.writeLog.length;
  const mkdirN = F.mkdirLog.length;
  await s.off();
  await s.off();
  assert.equal(F.execLog.length, execN);
  assert.equal(F.writeLog.length, writeN);
  assert.equal(F.mkdirLog.length, mkdirN);
});
