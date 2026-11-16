// scripts/kde-live-session.mjs: fixture, input and dev lifecycles for PX legs.
// Offline by default: importing this module performs no live calls.
import { readdir as fsReaddir } from "node:fs/promises";
import { createLiveIo, repoRoot, ydotooldCmd, ydotoolKeyCmd, T } from "./kde-scenarios.mjs";

export class LiveSessionError extends Error {}
function fail(m) { throw new LiveSessionError(m); }

// Focus-only scope for the current PX legs; never a burst.
const FOCUS = ["focusLeft", "focusRight"];
const POLLS = 50;
const POLL_MS = 100;
const KILL_MS = (T && T.killWait) || 5000;
const EXEC_MS = (T && T.exec) || 30000;

export function createLiveInput(opts) {
  opts = opts || {};
  const io = opts.io || createLiveIo();
  const root = opts.root === undefined ? repoRoot() : opts.root;
  const env = opts.env === undefined ? process.env : opts.env;
  const socket = opts.socket;
  const record = typeof opts.record === "function" ? opts.record : null;

  if (!io || typeof io.exec !== "function" || typeof io.spawnBackground !== "function" ||
    typeof io.procIdentity !== "function" || typeof io.exists !== "function" ||
    typeof io.sleep !== "function" || typeof io.now !== "function") {
    fail("io.exec/spawnBackground/procIdentity/exists/sleep/now required");
  }
  if (typeof root !== "string" || !root || root[0] !== "/") fail("root must be absolute");
  if (!env || typeof env !== "object") fail("env required");
  if (typeof socket !== "string" || !socket || socket[0] !== "/" || socket.includes("\0")) {
    fail("socket must be a fresh absolute short path");
  }

  let child = null, pid = null, tick = null, started = false, closed = false, inflight = null;

  const emit = (event, data) => {
    if (record) { try { record(event, data === undefined ? null : data); } catch (e) { /* advisory */ } }
  };
  const exec = async (argv, extra) => {
    const r = await io.exec(argv, {
      timeoutMs: EXEC_MS, env: Object.assign({}, env, extra || {}), cwd: root,
    });
    if (!r || typeof r.status !== "number") fail("bad exec result");
    return r;
  };
  const owned = async () => {
    if (!child || !pid) fail("input daemon not started");
    const cur = await io.procIdentity(pid);
    if (cur === null || cur !== tick) fail("owned daemon pid/start identity drifted; refusing signal/input");
    return cur;
  };
  const keyup = async (codes) => {
    const r = await exec(["ydotool", "key"].concat(codes.map((c) => c + ":0")), { YDOTOOL_SOCKET: socket });
    if (r.status !== 0) fail("key release failed (status " + r.status + ")");
    emit("release", { status: r.status });
  };

  const checkLock = async () => {
    const sid = env && env.XDG_SESSION_ID;
    if (typeof sid !== "string" || !sid) fail("lock check: XDG_SESSION_ID unknown");
    emit("lock-check-start", { session: sid });
    const r = await exec(["loginctl", "show-session", sid, "-p", "LockedHint"]);
    const m = /^LockedHint=(yes|no)\s*$/m.exec(typeof r.stdout === "string" ? r.stdout : "");
    if (r.status !== 0 || !m) {
      emit("lock-check", { locked: "unknown" });
      fail("lock check: unknown LockedHint (refusing; never unlock)");
    }
    const locked = m[1] === "yes";
    emit("lock-check", { locked });
    if (locked) fail("session locked (LockedHint=yes; refusing; never unlock)");
    return false;
  };

  const release = async () => {
    await owned();
    await keyup([105, 106, 42, 54, 29, 97, 56, 100, 125, 126, 1]);
  };

  const start = async () => {
    if (started) fail("input session already started");
    if (closed) fail("input session closed");
    if (await io.exists(socket)) fail("input socket path occupied: " + socket);
    child = await io.spawnBackground(ydotooldCmd(socket), { env, cwd: root });
    if (!child || !Number.isInteger(child.pid) || child.pid <= 0) fail("ydotoold: no spawned pid receipt");
    pid = child.pid;
    tick = await io.procIdentity(pid);
    if (tick === null) fail("ydotoold pid has no process identity");
    started = true;
    emit("start", { pid });
    let seen = false;
    for (let i = 0; i < POLLS; i += 1) {
      if (await io.exists(socket)) { seen = true; break; }
      await io.sleep(POLL_MS);
    }
    if (!seen) fail("ydotoold socket never appeared within the bounded wait");
    emit("socket", { socket });
    await checkLock();
    emit("lock-prefetch", { receipt: "pre-setup" });
    return { pid, socket };
  };

  const guarded = async (fn) => {
    let primary = null, releaseErr = null, lockErr = null;
    try {
      await owned();
      await fn();
    } catch (e) { primary = e; }
    try { await release(); } catch (e) { releaseErr = e; }
    try { await checkLock(); } catch (e) { lockErr = e; }
    const parts = [primary, releaseErr, lockErr].filter(Boolean);
    if (parts.length) {
      throw new LiveSessionError(parts.map((e) => (e && e.message ? e.message : String(e))).join("; "));
    }
  };

  const keys = async (name) => {
    if (!name || FOCUS.indexOf(name) < 0) fail("keys scope is focusLeft/focusRight only");
    await guarded(async () => {
      const argv = ydotoolKeyCmd(socket, name);
      const r = await exec(argv, { YDOTOOL_SOCKET: socket });
      emit("keys", { chord: name, status: r.status });
      if (r.status !== 0) fail("key injection failed (status " + r.status + ")");
    });
  };

  const moveRelative = async (dx, dy) => {
    if (!Number.isInteger(dx) || !Number.isInteger(dy)) fail("relative move needs integer dx/dy");
    await guarded(async () => {
      const argv = ["ydotool", "mousemove", "-x", String(dx), "-y", String(dy)];
      if (argv.includes("--absolute")) fail("relative move must not use --absolute");
      const r = await exec(argv, { YDOTOOL_SOCKET: socket });
      emit("mousemove", { dx, dy, status: r.status });
      if (r.status !== 0) fail("relative move failed (status " + r.status + ")");
    });
  };

  const stop = async () => {
    if (!child || !pid || !tick) return;
    const cur = await io.procIdentity(pid);
    if (cur !== tick) fail("owned daemon pid/start identity drifted; refusing SIGTERM");
    if (typeof child.kill === "function") await child.kill("SIGTERM");
    else if (typeof child.signal === "function") await child.signal("SIGTERM");
    else fail("daemon handle has no kill/signal");
    if (typeof child.exited === "function") {
      const ex = await child.exited(KILL_MS);
      if (ex === null) fail("daemon did not exit after SIGTERM");
    }
    if ((await io.procIdentity(pid)) !== null) fail("daemon pid still present after SIGTERM");
    emit("stop", { pid });
  };

  const close = async () => {
    if (inflight) return inflight;
    if (closed) return;
    inflight = (async () => {
      let relErr = null, lockErr = null, stopErr = null;
      if (started && child && pid) {
        try { await release(); } catch (e) { relErr = e; }
      }
      try { await checkLock(); } catch (e) { lockErr = e; }
      // Daemon stop runs even when release/lock failed.
      try { await stop(); } catch (e) { stopErr = e; }
      closed = true;
      emit("close", { released: !relErr, lockedCheck: !lockErr, stopped: !stopErr });
      const primary = relErr || lockErr || stopErr;
      if (primary) throw primary;
    })();
    return inflight;
  };

  return { start, keys, moveRelative, release, checkLock, close };
}

// Fixture helper for the current PX legs: direct built binary only, forced
// X11 for the serve child. Offline by default: creating the helper performs
// no effects.
export function createLiveFixture(opts) {
  opts = opts || {};
  const io = opts.io || createLiveIo();
  const root = opts.root === undefined ? repoRoot() : opts.root;
  const env = opts.env === undefined ? process.env : opts.env;
  const dir = opts.dir;
  const binary = opts.binary;
  const record = typeof opts.record === "function" ? opts.record : null;

  if (!io || typeof io.exec !== "function" || typeof io.spawnBackground !== "function" ||
    typeof io.procIdentity !== "function" || typeof io.exists !== "function") {
    fail("io.exec/spawnBackground/procIdentity/exists required");
  }
  if (typeof root !== "string" || !root || root[0] !== "/") fail("root must be absolute");
  if (!env || typeof env !== "object") fail("env required");

  let child = null, childPid = null, childTick = null, announced = null, annTick = null;
  let started = false, closed = false, inflight = null;

  const emit = (event, data) => {
    if (record) { try { record(event, data === undefined ? null : data); } catch (e) { /* advisory */ } }
  };
  const socketFor = () => dir + "/fixture.sock";
  const serveEnv = () => {
    const e = Object.assign({}, env);
    delete e.WAYLAND_DISPLAY;
    delete e.WAYLAND_SOCKET;
    return e;
  };
  const checkOwned = async () => {
    if (!childPid || childTick === null || childTick === undefined) fail("fixture child identity unknown");
    if ((await io.procIdentity(childPid)) !== childTick) {
      fail("fixture child pid/start identity drifted; refusing");
    }
    if (announced && announced.pid !== childPid) {
      if (annTick === null || annTick === undefined ||
        (await io.procIdentity(announced.pid)) !== annTick) {
        fail("fixture pid/start identity drifted; refusing");
      }
    }
  };

  const start = async () => {
    if (started) fail("fixture already started");
    if (closed) fail("fixture closed");
    if (typeof dir !== "string" || !dir || dir[0] !== "/" || dir.includes("\0")) {
      fail("dir must be a fresh absolute path");
    }
    if (typeof binary !== "string" || !binary || binary[0] !== "/" || binary.includes("\0")) {
      fail("binary must be absolute");
    }
    if (await io.exists(dir)) fail("fixture dir occupied: " + dir);
    child = await io.spawnBackground([binary, "serve", "--dir", dir], { env: serveEnv(), cwd: root });
    if (!child || !Number.isInteger(child.pid) || child.pid <= 0) fail("fixture serve: no spawned pid receipt");
    childPid = child.pid;
    childTick = await io.procIdentity(childPid);
    started = true;
    if (childTick === null || childTick === undefined) fail("fixture serve pid has no process identity");
    emit("start", { pid: childPid });
    let annLine;
    try {
      annLine = await child.waitLine((l) => l.trim().indexOf("{") === 0, 5000);
    } catch (e) { fail("fixture announcement: no JSON line within the bounded wait"); }
    let ann;
    try { ann = JSON.parse(String(annLine).trim()); } catch (e) { fail("fixture announcement: unparsable JSON"); }
    if (!ann || typeof ann !== "object" || ann.socket !== socketFor() || ann.pid !== childPid ||
      typeof ann.nonce !== "string" || !/^[0-9a-f]{64}$/.test(ann.nonce)) {
      fail("fixture announcement: bad socket/pid/nonce");
    }
    annTick = await io.procIdentity(ann.pid);
    if (annTick === null || annTick === undefined) fail("fixture announced pid has no process identity");
    announced = { socket: ann.socket, pid: ann.pid, nonce: ann.nonce };
    emit("announce", { pid: announced.pid, socket: announced.socket });
    return { pid: announced.pid, socket: announced.socket };
  };

  const send = async (request) => {
    if (!announced) fail("fixture not announced");
    if (!request || typeof request !== "object" || request.v !== 1 ||
      typeof request.request_id !== "string" || !request.request_id) {
      fail("request must carry v:1 and explicit request_id");
    }
    await checkOwned();
    const r = await io.exec(
      [binary, "send", "--socket", announced.socket, "--nonce", announced.nonce, JSON.stringify(request)],
      { timeoutMs: EXEC_MS, env: Object.assign({}, env), cwd: root });
    if (!r || typeof r.status !== "number") fail("bad exec result");
    if (r.status !== 0) fail("fixture send failed (status " + r.status + ")");
    const text = typeof r.stdout === "string" ? r.stdout.trim() : "";
    const lines = text ? text.split("\n").map((s) => s.trim()).filter(Boolean) : [];
    if (lines.length !== 1) fail("fixture send: expected one reply line");
    let reply;
    try { reply = JSON.parse(lines[0]); } catch (e) { fail("fixture send: unparsable reply"); }
    if (!reply || typeof reply !== "object" || reply.request_id !== request.request_id || reply.ok !== true) {
      fail("fixture send: reply mismatch or ok:false");
    }
    const wins = reply.result && Array.isArray(reply.result.windows) ? reply.result.windows : null;
    if (wins) {
      for (const w of wins) { if (w && w.ok === false) fail("fixture send: nested window ok:false"); }
    }
    emit("send", { request_id: request.request_id });
    return reply;
  };

  const termChild = async () => {
    if (!childPid || childTick === null || childTick === undefined) {
      fail("fixture child identity unknown; refusing SIGTERM");
    }
    if ((await io.procIdentity(childPid)) !== childTick) {
      fail("fixture child pid/start identity drifted; refusing SIGTERM");
    }
    if (typeof child.kill === "function") await child.kill("SIGTERM");
    else if (typeof child.signal === "function") await child.signal("SIGTERM");
    else fail("fixture child handle has no kill/signal");
  };
  const verifyGone = async () => {
    if (typeof child.exited === "function") {
      const ex = await child.exited(KILL_MS);
      emit("exit", { pid: childPid, code: ex && typeof ex.code === "number" ? ex.code : null, signal: ex && ex.signal ? ex.signal : null, timedOut: ex === null });
      if (ex === null) fail("fixture did not exit within the bounded wait");
    }
    if ((await io.procIdentity(childPid)) !== null) fail("fixture pid still present after shutdown");
    if (announced && announced.pid !== childPid &&
      (await io.procIdentity(announced.pid)) !== null) {
      fail("fixture announced pid still present after shutdown");
    }
    const sock = announced ? announced.socket : socketFor();
    if (sock && (await io.exists(sock))) fail("fixture socket still present after shutdown");
  };

  const close = async () => {
    if (inflight) return inflight;
    if (closed) return;
    inflight = (async () => {
      const errs = [];
      let needTerm = false, gracefulOk = false;
      if (announced) {
        try { await send({ v: 1, request_id: "bye", op: "shutdown" }); gracefulOk = true; }
        catch (e) { errs.push(e); needTerm = true; }
      } else if (started && child) {
        needTerm = true;
      }
      if (started && child && childPid) {
        try {
          if (needTerm) await termChild();
          await verifyGone();
        } catch (e) {
          errs.push(e);
          if (gracefulOk && announced && childPid && childTick !== null && childTick !== undefined &&
            (await io.procIdentity(childPid)) === childTick) {
            try {
              await termChild();
              await verifyGone();
            } catch (e2) { errs.push(e2); }
          }
        }
      }
      closed = true;
      emit("close", { ok: errs.length === 0 });
      if (errs.length === 1) throw errs[0];
      if (errs.length > 1) {
        throw new LiveSessionError(errs.map((e) => (e && e.message ? e.message : String(e))).join("; "));
      }
    })();
    return inflight;
  };

  return { start, send, close };
}

// Dev lifecycle: isolated settings/intent root + `just dev-on`, and a
// `just dev-off` that copies runtime + planner logs first. Offline by
// default: creating the helper performs no effects. The caller unloads
// channel helpers before off(); no controller lifecycle here.
export function createDevSession(opts) {
  opts = opts || {};
  const io = opts.io || createLiveIo();
  const root = opts.root === undefined ? repoRoot() : opts.root;
  const env = opts.env === undefined ? process.env : opts.env;
  const evidenceDir = opts.evidenceDir;
  const record = typeof opts.record === "function" ? opts.record : null;
  if (typeof io.readdir !== "function") io.readdir = (p) => fsReaddir(p);
  if (!io || typeof io.exec !== "function" || typeof io.exists !== "function" ||
    typeof io.mkdir !== "function" || typeof io.writeFile !== "function" ||
    typeof io.readFile !== "function" || typeof io.readdir !== "function") {
    fail("io.exec/exists/mkdir/readFile/writeFile/readdir required");
  }
  if (typeof root !== "string" || !root || root[0] !== "/") fail("root must be absolute");
  if (typeof evidenceDir !== "string" || !evidenceDir || evidenceDir[0] !== "/") fail("evidenceDir must be absolute");
  if (!env || typeof env !== "object") fail("env required");
  if (typeof env.XDG_RUNTIME_DIR !== "string" || !env.XDG_RUNTIME_DIR || env.XDG_RUNTIME_DIR[0] !== "/") {
    fail("env.XDG_RUNTIME_DIR must be absolute");
  }
  let active = false, attempted = false, sequence = 0;
  const emit = (event, data) => {
    if (record) { try { record(event, data === undefined ? null : data); } catch (e) { /* advisory */ } }
  };
  const exec = async (argv, extra, timeoutMs) => {
    const r = await io.exec(argv, {
      timeoutMs: timeoutMs || EXEC_MS, env: Object.assign({}, env, extra || {}), cwd: root,
    });
    if (!r || typeof r.status !== "number") fail("bad exec result");
    return r;
  };
  const on = async (label, settings) => {
    if (attempted) fail("dev session already attempted");
    if (typeof label !== "string" || !/^[a-z][a-z0-9-]*$/.test(label)) fail("label must match /^[a-z][a-z0-9-]*$/");
    settings = settings === undefined ? { focusFollowsCursor: false } : settings;
    if (!settings || typeof settings !== "object" || typeof settings.focusFollowsCursor !== "boolean") {
      fail("settings.focusFollowsCursor must be boolean");
    }
    const settingsRoot = evidenceDir + "/" + label + "-settings";
    const intentRoot = evidenceDir + "/" + label + "-intent";
    if ((await io.exists(settingsRoot)) || (await io.exists(intentRoot))) fail("dev roots occupied");
    await io.mkdir(settingsRoot, 0o700);
    await io.mkdir(intentRoot, 0o700);
    await io.writeFile(settingsRoot + "/settings.json",
      JSON.stringify({ focusFollowsCursor: settings.focusFollowsCursor }) + "\n", 0o600);
    attempted = true;
    const r = await exec(["just", "dev-on", "--scope-class", "OmniTilerTestWindow",
      "--settings-root", settingsRoot, "--intent-root", intentRoot], { OMNITILER_TRACE: "1" }, 300000);
    if (r.status !== 0) fail("dev-on failed (status " + r.status + ")");
    active = true;
    emit("dev-on", { label });
    return { settingsRoot, intentRoot };
  };
  const off = async () => {
    if (!attempted) return;
    const errs = [];
    const runtimeDir = env.XDG_RUNTIME_DIR;
    try {
      const names = await io.readdir(runtimeDir);
      const logs = (names || []).filter((n) => /^omnitiler-dev\..*\.log$/.test(n));
      const dest = evidenceDir + "/runtime-logs-" + (++sequence);
      await io.mkdir(dest, 0o700);
      for (const n of logs) {
        await io.writeFile(dest + "/" + n, await io.readFile(runtimeDir + "/" + n), 0o600);
      }
      emit("dev-off-copy", { dir: dest, count: logs.length });
      const ptr = runtimeDir + "/omnitiler-dev/planner-log";
      if (await io.exists(ptr)) {
        const target = String(await io.readFile(ptr)).trim().split("\n")[0];
        if (target && (await io.exists(target))) {
          await io.writeFile(dest + "/planner.log", await io.readFile(target), 0o600);
        }
      }
    } catch (e) { errs.push(e); }
    try {
      const r = await exec(["just", "dev-off"]);
      if (r.status !== 0) fail("dev-off failed (status " + r.status + ")");
    } catch (e) { errs.push(e); }
    try {
      const s = await exec(["just", "dev-status"]);
      const out = typeof s.stdout === "string" ? s.stdout : "";
      if (s.status !== 0 || out.indexOf("dev mode: DOWN") < 0) fail("dev mode not DOWN after dev-off");
    } catch (e) { errs.push(e); }
    if (errs.length) {
      throw new LiveSessionError(errs.map((e) => (e && e.message ? e.message : String(e))).join("; "));
    }
    attempted = false;
    active = false;
    emit("dev-off", { ok: true });
  };
  return { on, off };
}
