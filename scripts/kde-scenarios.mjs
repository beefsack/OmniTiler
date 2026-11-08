// scripts/kde-scenarios.mjs: fixture-driven KDE native scenario runner.
// Offline by default. Live execution requires explicit `--live`:
//   devenv shell -- node scripts/kde-scenarios.mjs --live --evidence-dir ABS_PATH [--input]
// One scenario: two-fixture tiling and close/reflow on a workspace appended
// beyond the whole roster. All external effects go through one injectable
// async `io` object; only production `io` below touches node APIs.
import { execFileSync, spawn } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, rmdirSync, rmSync, writeFileSync } from "node:fs";
import { randomBytes } from "node:crypto";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { childNames } from "../kwin/tests/baseline-check/baseline.mjs";

// Production identities (just/linux.just, planner_service.rs,
// world_broker.rs, KWin v6.7.4 src/dbusinterface.cpp:179).
export const SCOPE_CLASS = "OmniTilerTestWindow";
export const PLUGIN_ID = "omnitiler-kwin";
export const PLANNER_BUS = "com.omnitiler.Planner";
export const PLANNER_OBJECT = "/com/omnitiler/Planner";
export const PLANNER_IFACE = "com.omnitiler.Planner1";
export const PLANNER_METHOD = "ReadWorldPresentation";
export const KWIN_BUS = "org.kde.KWin";
export const KWIN_SCRIPTING = "/Scripting";
export const KWIN_SCRIPTING_IFACE = "org.kde.kwin.Scripting";
export const KWIN_SCRIPT_IFACE = "org.kde.kwin.Script";
export const KWIN_INFO_OBJECT = "/KWin";
export const KWIN_INFO_IFACE = "org.kde.KWin";
export const DBUS_BUS = "org.freedesktop.DBus";
export const DBUS_OBJECT = "/org/freedesktop/DBus";
export const DBUS_IFACE = "org.freedesktop.DBus";
export const INTRO_IFACE = "org.freedesktop.DBus.Introspectable";
export const WORLD_CONTRACT_VERSION = 4;
export const HELPER_PREFIX = "omnitiler-native-";
export const HELPER_NONCE_TOKEN = "__KDE_SCENARIOS_NONCE__";
export const HELPER_OP_TOKEN = "__KDE_SCENARIOS_OP__";

// Bounded waits (named constants; no unbounded loop, no sleep-as-barrier).
export const T = {
  exec: 30000,
  devOn: 300000,
  announce: 30000,
  readiness: 20000,
  readinessPoll: 1000,
  admit: 15000,
  reflow: 15000,
  poll: 500,
  inputSock: 15000,
  shutdown: 10000,
  killWait: 5000,
};

export const UUID_RE = /^\{[0-9a-fA-F]{8}(-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\}$/;
export const NONCE64_RE = /^[0-9a-f]{64}$/;
export const UNIQUE_RE = /^:[0-9]+\.[0-9]+$/;
const GEO_BOUND = 16384;

export class ScenarioError extends Error {}

// Mutating calls are never repeated: one semantic instance, one attempt.
export function createMutOnce() {
  const seen = new Set();
  return (k) => {
    if (seen.has(k)) { throw new ScenarioError("refusing second mutating call '" + k + "'"); }
    seen.add(k);
  };
}

// ---------------------------------------------------------------------------
// CLI / hygiene (pure)
// ---------------------------------------------------------------------------

export function helpText() {
  return [
    "usage: node scripts/kde-scenarios.mjs [--help] [--live --evidence-dir PATH [--input]]",
    "",
    "Fixture-driven KDE native scenario runner. Offline by default.",
    "Exactly one scenario: admission A/B tiling then close-B/A-reflow,",
    "on a workspace appended beyond the whole roster.",
    "",
    "  --help            show this help and exit (never mutates)",
    "  --live            explicit live mode against the real session.",
    "                    Without --live nothing executes; exit 2.",
    "  --evidence-dir P  fresh private dir for evidence. Must be absolute,",
    "                    canonical, outside the repository, and not pre-exist.",
    "  --input           also start ydotoold (lifecycle only, no injection).",
    "                    Default off.",
  ].join("\n");
}

export function parseArgs(argv) {
  const out = { help: false, live: false, evidenceDir: null, input: false };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (a === "--help" || a === "-h") { out.help = true; }
    else if (a === "--live") {
      if (out.live) { throw new ScenarioError("duplicate --live"); }
      out.live = true;
    } else if (a === "--input") {
      if (out.input) { throw new ScenarioError("duplicate --input"); }
      out.input = true;
    } else if (a === "--evidence-dir") {
      const v = argv[++i];
      if (!v) { throw new ScenarioError("--evidence-dir requires a value"); }
      if (out.evidenceDir) { throw new ScenarioError("duplicate --evidence-dir"); }
      out.evidenceDir = v;
    } else if (a.startsWith("--evidence-dir=")) {
      const v = a.slice("--evidence-dir=".length);
      if (!v) { throw new ScenarioError("--evidence-dir requires a value"); }
      if (out.evidenceDir) { throw new ScenarioError("duplicate --evidence-dir"); }
      out.evidenceDir = v;
    } else { throw new ScenarioError("unknown argument '" + a + "'"); }
  }
  return out;
}

export function repoRoot() {
  return path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
}

// String-only canonical/absolute/outside checks (no fs: usable pre-io).
export function checkEvidenceDir(requested, repo) {
  if (typeof requested !== "string" || !requested) { throw new ScenarioError("--evidence-dir is required with --live"); }
  if (!path.isAbsolute(requested)) { throw new ScenarioError("evidence dir must be absolute"); }
  if (requested.includes("//") || requested.includes("/../") || requested.includes("/./") ||
    requested.endsWith("/..") || requested.endsWith("/.") || requested.endsWith("/")) {
    throw new ScenarioError("evidence dir must be canonical");
  }
  const r = path.resolve(repo);
  const p = path.resolve(requested);
  if (p !== r && !p.startsWith(r + path.sep)) { return p; }
  throw new ScenarioError("refusing in-repo evidence dir");
}

// ---------------------------------------------------------------------------
// Validators (pure; mirror production charsets, never invent values)
// ---------------------------------------------------------------------------

export function isOpaqueId(v) {
  if (typeof v !== "string" || v.length === 0 || v.length > 128) { return false; }
  for (let i = 0; i < v.length; i++) {
    const c = v.charCodeAt(i);
    const alnum = (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122);
    if (!(alnum || c === 45 || c === 95 || c === 46)) { return false; }
  }
  return true;
}

// Canonical u64 (world-seq.ts): "0" or non-zero without leading zeros, <= 2^64-1.
export function isCanonicalU64(v) {
  if (typeof v !== "string" || !v) { return false; }
  if (v === "0") { return true; }
  if (v.length > 20 || v.charCodeAt(0) < 49 || v.charCodeAt(0) > 57) { return false; }
  for (let i = 1; i < v.length; i++) {
    const c = v.charCodeAt(i);
    if (c < 48 || c > 57) { return false; }
  }
  if (v.length === 20 && v > "18446744073709551615") { return false; }
  return true;
}

// Bare opaque id after unwrapping one braced UUID layer (native-id.ts).
export function normalizeNativeId(value) {
  let text;
  try { text = String(value); } catch (e) { return null; }
  if (isOpaqueId(text)) { return text; }
  if (text.length === 38 && text[0] === "{" && text[37] === "}") {
    const inner = text.slice(1, 37);
    if (isOpaqueId(inner)) { return inner; }
  }
  return null;
}

export function isCarriedRect(v) {
  if (!Array.isArray(v) || v.length !== 4) { return false; }
  for (const n of v) {
    if (typeof n !== "number" || !Number.isInteger(n)) { return false; }
  }
  const x = v[0], y = v[1], w = v[2], h = v[3];
  return w > 0 && h > 0 && x >= -GEO_BOUND && x <= GEO_BOUND &&
    y >= -GEO_BOUND && y <= GEO_BOUND && w <= GEO_BOUND && h <= GEO_BOUND &&
    x + w <= 2147483647 && y + h <= 2147483647;
}

export function parseDevStatusMode(output) {
  const m = /dev mode:\s*(UP|SPLIT|DOWN|UNKNOWN)/.exec(String(output));
  if (!m) { throw new ScenarioError("dev-status output has no usable mode"); }
  return m[1];
}

export function frameOf(alloc) {
  if (!isCarriedRect(alloc)) { throw new ScenarioError("allocation is not a carried rect"); }
  return { x: alloc[0], y: alloc[1], w: alloc[2], h: alloc[3] };
}

export function frameEqual(a, e) {
  if (!a || !e) { return false; }
  for (const k of ["x", "y", "w", "h"]) {
    if (!Number.isInteger(a[k]) || !Number.isInteger(e[k])) { return false; }
  }
  return a.x === e.x && a.y === e.y && a.w === e.w && a.h === e.h;
}

// Differing fields between two frames (for divergence messages).
export function frameDiff(a, e) {
  const d = [];
  if (!a || !e) { return d; }
  for (const k of ["x", "y", "w", "h"]) { if (a[k] !== e[k]) { d.push(k); } }
  return d;
}

export function frameInside(f, o) {
  if (!f || !o) { return false; }
  return f.x >= o.x && f.y >= o.y && f.x + f.w <= o.x + o.w && f.y + f.h <= o.y + o.h;
}

function rectsOverlap(a, b) {
  return a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h;
}

export function frameArea(f) { return f.w * f.h; }

// Readiness gate over KWin-owner PID-filtered journal text: passes iff ready,
// discovered, dispatch, and applied lines are all present after the pre-start
// cursor. Ready `source=` and dispatch `src=` live in disjoint namespaces
// and are never compared. Callers persist booleans/counts only.
export function parseReadiness(journalText) {
  const text = String(journalText);
  const ready = /omnitiler:world:ready[^\n]*source=([^\s"\n]+)/.test(text);
  const discovered = /omnitiler:world:discovered[^\n]*gen=([^\s"\n]+)/.test(text);
  const dispatch = /omnitiler:world:dispatch[^\n]*src=([^\s"\n]+)/.test(text);
  const applied = /omnitiler:world:applied[^\n]*rev=([^\s"\n]+)/.test(text);
  return { ready, discovered, dispatch, applied };
}

export function readinessPass(g) {
  return !!(g && g.ready && g.discovered && g.dispatch && g.applied);
}

// Presentation reply validation (world-report.ts WorldPresentationReply).
// Extra keys allowed, required keys strict.
export function parsePresentation(text, req) {
  let body;
  try { body = JSON.parse(String(text)); } catch (e) { throw new ScenarioError("presentation: unparsable envelope"); }
  if (body && typeof body === "object" && body.type === "s" && Array.isArray(body.data)) {
    if (body.data.length !== 1 || typeof body.data[0] !== "string") {
      throw new ScenarioError("presentation: bad busctl string envelope");
    }
    try { body = JSON.parse(body.data[0]); } catch (e) { throw new ScenarioError("presentation: unparsable body"); }
  }
  if (!body || typeof body !== "object") { throw new ScenarioError("presentation: not an object"); }
  if (body.v !== WORLD_CONTRACT_VERSION) { throw new ScenarioError("presentation: wrong contract version"); }
  if (body.outcome !== "presentation") { throw new ScenarioError("presentation: not the presentation lane"); }
  if (body.req !== req) { throw new ScenarioError("presentation: correlation mismatch"); }
  if (!isOpaqueId(body.gen)) { throw new ScenarioError("presentation: bad gen"); }
  if (!isCanonicalU64(body.rev) || !isCanonicalU64(body.obs)) { throw new ScenarioError("presentation: bad rev/obs"); }
  if (!Array.isArray(body.windows)) { throw new ScenarioError("presentation: bad windows"); }
  const windows = body.windows.map((w, i) => {
    if (!w || typeof w !== "object") { throw new ScenarioError("presentation window " + i + ": not an object"); }
    if (!isOpaqueId(w.win) || !isOpaqueId(w.out) || !isOpaqueId(w.ws)) {
      throw new ScenarioError("presentation window " + i + ": bad ids");
    }
    if (!(w.alloc === null || isCarriedRect(w.alloc))) {
      throw new ScenarioError("presentation window " + i + ": bad alloc");
    }
    return { win: w.win, out: w.out, ws: w.ws, alloc: w.alloc };
  });
  return { gen: body.gen, rev: body.rev, obs: body.obs, windows };
}

// getWindowInfo reply validation. Keys and value types verified from KWin
// v6.7.4 src/dbusinterface.cpp clientToVariantMap; the busctl --json=short
// envelope is {"type":"a{sv}","data":[{...}]} with every value wrapped as
// {type,data} (scalar data is the bare value, list data is an array).
export function parseWindowInfo(text, uuid) {
  let reply;
  try { reply = JSON.parse(String(text)); } catch (e) { throw new ScenarioError("getWindowInfo " + uuid + ": unparsable JSON"); }
  if (!reply || typeof reply !== "object" || reply.type !== "a{sv}" || !Array.isArray(reply.data) || reply.data.length !== 1) {
    throw new ScenarioError("getWindowInfo " + uuid + ": bad envelope");
  }
  const info = reply.data[0];
  if (!info || typeof info !== "object" || Array.isArray(info)) {
    throw new ScenarioError("getWindowInfo " + uuid + ": bad body");
  }
  if (Object.keys(info).length === 0) { throw new ScenarioError("getWindowInfo " + uuid + ": unknown window (empty map)"); }
  const wrapped = (k, types) => {
    const v = info[k];
    if (!v || typeof v !== "object" || Array.isArray(v)) {
      throw new ScenarioError("getWindowInfo " + uuid + ": bare value for '" + k + "' rejected (need wrapped {type,data})");
    }
    if (types.indexOf(v.type) < 0) {
      throw new ScenarioError("getWindowInfo " + uuid + ": bad signature for '" + k + "' (need " + types.join("/") + ")");
    }
    return v.data;
  };
  const str = (k) => {
    const d = wrapped(k, ["s"]);
    if (typeof d !== "string") { throw new ScenarioError("getWindowInfo " + uuid + ": bad string data for '" + k + "'"); }
    return d;
  };
  const resourceClass = str("resourceClass");
  const caption = str("caption");
  const id = str("uuid");
  const pid = wrapped("pid", ["i"]);
  if (!Number.isInteger(pid) || pid <= 0) { throw new ScenarioError("getWindowInfo " + uuid + ": bad pid"); }
  const desks = wrapped("desktops", ["as"]);
  if (!Array.isArray(desks) || !desks.every((d) => typeof d === "string")) {
    throw new ScenarioError("getWindowInfo " + uuid + ": bad desktops");
  }
  const num = (k) => {
    const d = wrapped(k, ["i", "d"]);
    if (typeof d !== "number" || !Number.isFinite(d)) {
      throw new ScenarioError("getWindowInfo " + uuid + ": bad " + k);
    }
    return d;
  };
  const x = num("x"), y = num("y"), w = num("width"), h = num("height");
  return {
    resourceClass,
    caption,
    uuid: id,
    pid,
    desks: desks.slice(),
    frame: { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) },
  };
}

// ---------------------------------------------------------------------------
// Command builders (argv only)
// ---------------------------------------------------------------------------

export const devStatusCmd = () => ["just", "dev-status"];
export const devOffCmd = () => ["just", "dev-off"];
export const devOnCmd = () => ["just", "dev-on", "--scope-class", SCOPE_CLASS];
export function fixtureServeCmd(dir) {
  if (typeof dir !== "string" || !dir || dir[0] !== "/") { throw new ScenarioError("fixture dir must be absolute"); }
  return ["cargo", "run", "--locked", "-p", "tiler-live-fixture", "--", "serve", "--dir", dir];
}
export function fixtureSendCmd(sock, nonce, payloads) {
  if (!sock) { throw new ScenarioError("fixture send needs --socket"); }
  if (!NONCE64_RE.test(nonce)) { throw new ScenarioError("fixture nonce must be 64 hex"); }
  if (!Array.isArray(payloads) || !payloads.length) { throw new ScenarioError("fixture send needs payloads"); }
  return ["cargo", "run", "--locked", "-p", "tiler-live-fixture", "--", "send",
    "--socket", sock, "--nonce", nonce, "--compact"].concat(payloads);
}
// ydotoold runs lifecycle-only (never any injection): socket path + perm.
export function ydotooldCmd(sockPath) {
  if (typeof sockPath !== "string" || !sockPath || sockPath[0] !== "/") {
    throw new ScenarioError("input socket must be absolute");
  }
  return ["ydotoold", "--socket-path=" + sockPath, "--socket-perm=0600"];
}
export function presentationReq() {
  return "kde-scen-" + randomBytes(8).toString("hex");
}
export function plannerReadCmd(req) {
  if (typeof req !== "string" || !req) { throw new ScenarioError("presentation req required"); }
  return ["busctl", "--user", "--json=short", "call", PLANNER_BUS, PLANNER_OBJECT,
    PLANNER_IFACE, PLANNER_METHOD, "s", JSON.stringify({ v: WORLD_CONTRACT_VERSION, req })];
}
export function windowInfoCmd(uuid) {
  if (typeof uuid !== "string" || !UUID_RE.test(uuid)) { throw new ScenarioError("getWindowInfo needs a braced UUID"); }
  return ["busctl", "--user", "--json=short", "call", KWIN_BUS, KWIN_INFO_OBJECT, KWIN_INFO_IFACE, "getWindowInfo", "s", uuid];
}

// ---------------------------------------------------------------------------
// Scenario
// ---------------------------------------------------------------------------

export async function runScenario(o) {
  const io = o.io, root = o.root, env = o.env, evidenceDir = o.evidenceDir;
  const withInput = !!o.input;
  const templatePath = o.templatePath;
  if (!io || typeof io.exec !== "function" || typeof io.spawnBackground !== "function") {
    throw new ScenarioError("io.exec/spawnBackground required");
  }
  if (typeof root !== "string" || !root || root[0] !== "/") { throw new ScenarioError("root must be absolute"); }
  if (!env || typeof env !== "object") { throw new ScenarioError("env required"); }
  if (typeof evidenceDir !== "string" || !evidenceDir || evidenceDir[0] !== "/") {
    throw new ScenarioError("evidenceDir must be absolute");
  }
  if (typeof templatePath !== "string" || !templatePath) { throw new ScenarioError("templatePath required"); }

  const S = {
    events: [], calls: [], checks: [], cleanups: [],
    receipts: {}, counts: {}, secrets: [],
    category: "NOT-RUN", divergence: null, scenario: "init",
    kwin: null, roster: null, original: null, appended: null,
    fixture: null, cargo: null, ydoc: null, devUp: false,
    joins: null, planner: null, cleanupFailed: [], teardownIssues: [],
  };
  const redact = (s) => {
    let t = String(s);
    for (const n of S.secrets) { if (n) { t = t.split(n).join("[redacted]"); } }
    return t;
  };
  const fail = (m) => { throw new ScenarioError(m); };
  const event = (n) => { S.events.push(n); };
  const check = (name, pass, expected, actual) => {
    S.checks.push({ name, pass: !!pass, expected: expected === undefined ? null : expected, actual: actual === undefined ? null : actual });
    return !!pass;
  };
  // Mutating calls are never repeated: one semantic instance, one attempt.
  const mutOnce = createMutOnce();
  const exec = async (argv, timeoutMs) => {
    S.calls.push(redact(argv.map(String).join(" ")));
    const r = await io.exec(argv, { timeoutMs: timeoutMs || T.exec, env, cwd: root });
    if (!r || typeof r.status !== "number") { fail("bad exec result"); }
    return r;
  };
  const mustExec = async (argv, what, timeoutMs) => {
    const r = await exec(argv, timeoutMs);
    if (r.status !== 0) { fail(what + " failed (status " + r.status + ")"); }
    return r;
  };
  const busJson = async (argv, what, timeoutMs) => {
    const r = await mustExec(argv, what, timeoutMs);
    try { return JSON.parse(r.stdout); } catch (e) { fail(what + ": unparsable busctl JSON"); }
    return null;
  };
  const strictId = (v, what) => {
    if (!v || v.type !== "i" || !Array.isArray(v.data) || v.data.length !== 1) { fail(what + ": need {type:i,data:[id]}"); }
    const id = v.data[0];
    if (!Number.isInteger(id) || id < 0 || id > 2147483647) { fail(what + ": bad script id (-1 is load failure)"); }
    return id;
  };
  const strictBool = (v, what) => {
    if (!v || v.type !== "b" || !Array.isArray(v.data) || v.data.length !== 1 || typeof v.data[0] !== "boolean") {
      fail(what + ": need {type:b,data:[bool]}");
    }
    return v.data[0];
  };
  const showCursor = async (what) => {
    const r = await mustExec(["journalctl", "--user", "--show-cursor", "-n", "0", "--no-pager"], what || "journal cursor");
    const line = r.stdout.split("\n").filter((l) => l.indexOf("-- cursor: ") === 0).pop();
    const cursor = line ? line.slice("-- cursor: ".length).trim() : "";
    if (!cursor) { fail("empty journal cursor"); }
    return cursor;
  };

  const kwinIdentity = async () => {
    const ownerR = await busJson(["busctl", "--user", "--json=short", "call",
      DBUS_BUS, DBUS_OBJECT, DBUS_IFACE, "GetNameOwner", "s", KWIN_BUS], "kwin GetNameOwner");
    if (!ownerR || ownerR.type !== "s" || !Array.isArray(ownerR.data)) { fail("kwin GetNameOwner: need {type:s,data:[owner]}"); }
    const owner = ownerR.data[0];
    if (typeof owner !== "string" || !UNIQUE_RE.test(owner)) { fail("KWin owner is not a unique name"); }
    const pidR = await busJson(["busctl", "--user", "--json=short", "call",
      DBUS_BUS, DBUS_OBJECT, DBUS_IFACE, "GetConnectionUnixProcessID", "s", owner], "kwin owner pid");
    if (!pidR || pidR.type !== "u" || !Array.isArray(pidR.data)) { fail("kwin owner pid: need {type:u,data:[pid]}"); }
    const pid = pidR.data[0];
    if (!Number.isInteger(pid) || pid <= 0) { fail("bad KWin PID reply"); }
    const tick = await io.procIdentity(pid);
    if (tick === null) { fail("KWin process identity unavailable"); }
    return { owner, pid, tick };
  };
  const kwinSame = async () => {
    const cur = await kwinIdentity();
    if (cur.owner !== S.kwin.owner || cur.pid !== S.kwin.pid || cur.tick !== S.kwin.tick) {
      fail("KWin identity changed");
    }
  };

  // Helper lifecycle per op (mirrors attribution load/introspect/run/journal/
  // unload): nonce-named plugin fresh per call, strict id, nonce-prefixed
  // journal result line (a run reply is NOT proof), unload exactly once even
  // on failure. Mutations require unchanged KWin identity first.
  const helperOp = async (op, opts) => {
    opts = opts || {};
    if (opts.mutate) { await kwinSame(); }
    if (!op || typeof op !== "object" || typeof op.op !== "string") { fail("helper op required"); }
    if (["read", "append", "setCurrent", "activate", "remove"].indexOf(op.op) < 0) { fail("unsupported helper op"); }
    const nonce = randomBytes(16).toString("hex");
    const plugin = HELPER_PREFIX + nonce;
    S.secrets.push(nonce);
    let template;
    try { template = String(await io.readFile(templatePath)); } catch (e) { fail("cannot read helper template"); }
    if (template.indexOf(HELPER_NONCE_TOKEN) < 0 || template.indexOf(HELPER_OP_TOKEN) < 0) {
      fail("helper template lost its placeholders");
    }
    const fullOp = Object.assign({}, op, { scope: SCOPE_CLASS });
    const src = template.split(HELPER_NONCE_TOKEN).join(JSON.stringify(nonce))
      .split(HELPER_OP_TOKEN).join(JSON.stringify(fullOp));
    const helperPath = evidenceDir + "/helper-" + nonce + ".js";
    await io.writeFile(helperPath, src, 0o600);
    const owner = S.kwin.owner;
    const introArgv = ["busctl", "--user", "--json=short", "call", owner, KWIN_SCRIPTING, INTRO_IFACE, "Introspect"];
    const loadedArgv = ["busctl", "--user", "--json=short", "call", owner, KWIN_SCRIPTING, KWIN_SCRIPTING_IFACE, "isScriptLoaded", "s", plugin];
    const unloadArgv = ["busctl", "--user", "--json=short", "call", owner, KWIN_SCRIPTING, KWIN_SCRIPTING_IFACE, "unloadScript", "s", plugin];
    const unloadOnce = async () => {
      const u = await busJson(unloadArgv, "helper unloadScript");
      if (strictBool(u, "helper unloadScript") !== true) { fail("helper unloadScript did not return TRUE"); }
      const fin = await busJson(loadedArgv, "helper final isScriptLoaded");
      if (strictBool(fin, "helper final isScriptLoaded") !== false) { fail("helper still loaded after unload"); }
      const kids = childNames(await busJson(introArgv, "helper final introspect"), "helper final introspect");
      if (kids.has("Script" + sid)) { fail("Script" + sid + " still present after unload"); }
    };
    let sid = null;
    let rows = null;
    try {
      if (strictBool(await busJson(loadedArgv, "helper pre isScriptLoaded"), "helper pre isScriptLoaded") !== false) {
        fail("refusing: helper " + plugin + " already loaded");
      }
      const preKids = childNames(await busJson(introArgv, "helper pre introspect"), "helper pre introspect");
      const cursor = await showCursor("helper cursor");
      const loadRaw = await busJson(["busctl", "--user", "--json=short", "call",
        KWIN_BUS, KWIN_SCRIPTING, KWIN_SCRIPTING_IFACE, "loadScript", "ss", helperPath, plugin], "helper loadScript");
      sid = strictId(loadRaw, "helper loadScript");
      if (preKids.has("Script" + sid)) { fail("Script" + sid + " present before load; refusing to run"); }
      const postKids = childNames(await busJson(introArgv, "helper post introspect"), "helper post introspect");
      if (!postKids.has("Script" + sid)) { fail("Script" + sid + " absent after load; refusing to run"); }
      if (strictBool(await busJson(loadedArgv, "helper post isScriptLoaded"), "helper post isScriptLoaded") !== true) {
        fail("helper not loaded before run; refusing to run");
      }
      await kwinSame();
      const run = await exec(["busctl", "--user", "call", owner, KWIN_SCRIPTING + "/Script" + sid, KWIN_SCRIPT_IFACE, "run"], T.exec);
      if (run.status !== 0) { fail("helper run failed for Script" + sid); }
      await kwinSame();
      const jl = await mustExec(["journalctl", "--user", "--after-cursor", cursor, "-o", "json", "_PID=" + S.kwin.pid], "helper journal");
      const mark = "omnitiler:native:" + nonce + ":";
      rows = [];
      for (const line of jl.stdout.split("\n")) {
        if (!line.trim()) { continue; }
        let entry;
        try { entry = JSON.parse(line); } catch (e) { fail("helper journal: unparsable -o json line"); }
        if (typeof entry.MESSAGE !== "string") { fail("helper journal: MESSAGE missing"); }
        let msg = entry.MESSAGE;
        if (msg.indexOf("js:") === 0) { msg = msg.slice(3).replace(/^ /, ""); }
        if (msg.indexOf("omnitiler:native:") >= 0 && msg.indexOf(mark) < 0) {
          fail("helper journal: foreign-nonce helper line present");
        }
        const ix = msg.indexOf(mark);
        if (ix >= 0) {
          const rest = msg.slice(ix + mark.length);
          if (!rest) { fail("helper journal: empty body"); }
          let row;
          try { row = JSON.parse(rest); } catch (e) { fail("helper journal: bad result line"); }
          rows.push(row);
        }
      }
      if (!rows.length) { fail("helper produced no nonce result line (run reply is not proof)"); }
      for (const r of rows) {
        if (r && typeof r === "object" && r.error) { fail("helper error: " + String(r.error).slice(0, 200)); }
      }
    } finally {
      try {
        await unloadOnce();
      } catch (e) {
        S.teardownIssues.push("helper unload (" + op.op + "): " + (e && e.message ? e.message : String(e)));
      }
      try { await io.rm(helperPath); } catch (e) { /* best effort; rm of own file */ }
      try {
        if (await io.exists(helperPath)) {
          S.teardownIssues.push("helper source rm (" + op.op + "): still present after rm");
        }
      } catch (e) { /* existence unverifiable; nothing to report */ }
    }
    return rows;
  };

  const fixtureSend = async (payloads, what) => {
    const argv = fixtureSendCmd(S.fixture.socket, S.fixture.nonce, payloads);
    const r = await exec(argv, T.exec);
    if (r.status !== 0) { fail(what + " failed (status " + r.status + ")"); }
    const lines = r.stdout.split("\n").map((l) => l.trim()).filter(Boolean);
    if (!lines.length) { fail(what + ": empty fixture reply"); }
    return lines.map((l) => {
      try { return JSON.parse(l); } catch (e) { fail(what + ": unparsable fixture reply"); }
      return null;
    });
  };
  const fixtureSnapshots = async (ids) => {
    const replies = await fixtureSend(
      ['{"v":1,"request_id":"get-ab","op":"get","ids":' + JSON.stringify(ids) + "}"], "fixture get");
    const snaps = new Map();
    for (const reply of replies) {
      const wins = (reply && reply.result && reply.result.windows) || [];
      for (const w of wins) {
        if (!w || w.ok !== true || typeof w.id !== "string" || !w.snapshot) { fail("fixture get: bad entry"); }
        const s = w.snapshot;
        if (s.id !== w.id) { fail("fixture get: id mismatch for " + w.id); }
        if (s.pid !== S.fixture.pid) { fail("fixture get: pid mismatch for " + w.id); }
        if (typeof s.title !== "string" || !s.title) { fail("fixture get: bad title for " + w.id); }
        snaps.set(w.id, { callerId: w.id, pid: s.pid, title: s.title });
      }
    }
    return snaps;
  };

  const readPresentation = async () => {
    const req = presentationReq();
    const r = await mustExec(plannerReadCmd(req), "presentation read");
    return parsePresentation(r.stdout, req);
  };
  const windowInfo = async (uuid) => {
    const r = await mustExec(windowInfoCmd(uuid), "getWindowInfo");
    return parseWindowInfo(r.stdout, uuid);
  };

  let rc = 1;
  try {
    // 1. Preflight: dev mode must be DOWN. Anything else: stop, nothing mutated.
    event("preflight");
    const st = await mustExec(devStatusCmd(), "dev-status");
    if (parseDevStatusMode(st.stdout) !== "DOWN") {
      fail("refusing: dev mode is not DOWN; never touches foreign state");
    }
    S.counts.preflight = 1;

    // 2. KWin identity capture.
    event("kwin-identity");
    S.kwin = await kwinIdentity();
    S.receipts.kwin = { pid: S.kwin.pid, tick: S.kwin.tick };

    // 3. Fixture server in BACKGROUND (never a blocking exec of serve).
    mutOnce("fixture-serve");
    event("fixture-serve");
    const runDir = evidenceDir + "/fixture-run";
    const serveChild = await io.spawnBackground(fixtureServeCmd(runDir), { env, cwd: root });
    if (!serveChild || !Number.isInteger(serveChild.pid) || serveChild.pid <= 0) {
      fail("fixture serve: no spawned pid receipt");
    }
    S.cargo = { pid: serveChild.pid, child: serveChild, dir: runDir };
    const cargoTick = await io.procIdentity(serveChild.pid);
    if (cargoTick === null) { fail("fixture cargo child has no process identity"); }
    S.receipts.cargo = { pid: serveChild.pid, tick: cargoTick };
    // NOTE: announced pid and cargo child pid are independent receipts; cargo
    // run forks, so equality is never required.
    // Orphan guard armed BEFORE waiting for the announcement: when no
    // announcement arrives the cargo child is terminated here (tick-verified,
    // SIGTERM to the receipted pid only), never left running.
    S.cleanups.push({ name: "fixture-shutdown", order: 10, fn: async () => {
      if (!S.fixture) {
        const cur = await io.procIdentity(serveChild.pid);
        if (cur !== cargoTick) { fail("fixture cargo identity changed; refusing to signal"); }
        await serveChild.kill("SIGTERM");
        const orph = await serveChild.exited(T.shutdown);
        if (orph === null) { fail("fixture cargo did not exit after SIGTERM"); }
        if ((await io.procIdentity(serveChild.pid)) !== null) { fail("fixture cargo pid still present after SIGTERM"); }
        fail("fixture process pid is unknown when no announcement arrived and so absence of a grandchild fixture cannot be verified");
      }
      await fixtureSend(['{"v":1,"request_id":"bye","op":"shutdown"}'], "fixture shutdown");
      const ex = await serveChild.exited(T.shutdown);
      if (ex === null) { fail("fixture server did not exit within the bounded wait"); }
      if ((await io.procIdentity(S.fixture.pid)) !== null) { fail("fixture pid still present after shutdown"); }
      if ((await io.procIdentity(serveChild.pid)) !== null) { fail("fixture cargo pid still present after shutdown"); }
      if (await io.exists(S.fixture.socket)) { fail("fixture socket still present after shutdown"); }
      // The fixture removes its own run dir on exit; absence is verified below.
      try { await io.rmdir(runDir); } catch (e) { if (!e || e.code !== "ENOENT") { throw e; } }
      if (await io.exists(runDir)) { fail("fixture run dir still present after shutdown"); }
    } });
    let annLine;
    try {
      annLine = await serveChild.waitLine((l) => l.trim().indexOf("{") === 0, T.announce);
    } catch (e) { fail("fixture announcement: no JSON line within the bounded wait"); }
    let ann;
    try { ann = JSON.parse(annLine.trim()); } catch (e) { fail("fixture announcement: unparsable JSON"); }
    if (!ann || typeof ann.socket !== "string" || !Number.isInteger(ann.pid) || ann.pid <= 0 ||
      typeof ann.nonce !== "string" || !NONCE64_RE.test(ann.nonce)) {
      fail("fixture announcement: bad socket/pid/nonce");
    }
    if (ann.socket !== runDir + "/fixture.sock" && ann.socket.indexOf(runDir + "/") !== 0) {
      fail("fixture socket outside the owned run dir");
    }
    S.secrets.push(ann.nonce);
    const annTick = await io.procIdentity(ann.pid);
    if (annTick === null) { fail("fixture announced pid has no process identity"); }
    S.fixture = { socket: ann.socket, pid: ann.pid, tick: annTick, nonce: ann.nonce };
    S.receipts.fixture = { pid: ann.pid, tick: annTick };

    // 4. Optional input daemon in BACKGROUND (default off; lifecycle only).
    if (withInput) {
      mutOnce("ydotoold-start");
      event("ydotoold-start");
      const ysock = evidenceDir + "/ydotool.sock";
      const yd = await io.spawnBackground(ydotooldCmd(ysock), { env, cwd: root });
      if (!yd || !Number.isInteger(yd.pid) || yd.pid <= 0) { fail("ydotoold: no spawned pid receipt"); }
      const ydTick = await io.procIdentity(yd.pid);
      if (ydTick === null) { fail("ydotoold pid has no process identity"); }
      // Orphan guard armed BEFORE waiting for the socket: a daemon whose
      // socket never appears is still terminated here (tick-verified).
      S.cleanups.push({ name: "ydotoold-stop", order: 50, fn: async () => {
        const cur = await io.procIdentity(yd.pid);
        if (cur !== ydTick) { fail("ydotoold identity changed; refusing to signal"); }
        await yd.kill("SIGTERM");
        const ex = await yd.exited(T.killWait);
        if (ex === null) { fail("ydotoold did not exit after SIGTERM"); }
        if ((await io.procIdentity(yd.pid)) !== null) { fail("ydotoold pid still present"); }
        if (!S.ydoc) { fail("ydotoold socket never appeared within the bounded wait (daemon terminated)"); }
        if (await io.exists(ysock)) { fail("ydotoold socket still present"); }
      } });
      const end = io.now() + T.inputSock;
      let seen = false;
      while (io.now() < end) {
        if (await io.exists(ysock)) { seen = true; break; }
        await io.sleep(100);
      }
      if (!seen) { fail("ydotoold socket never appeared within the bounded wait"); }
      S.ydoc = { pid: yd.pid, tick: ydTick, child: yd, sock: ysock };
      S.receipts.ydotoold = { pid: yd.pid, tick: ydTick };
    }

    // 5. Journal cursor BEFORE the dev stack starts.
    event("journal-cursor");
    const cursor0 = await showCursor("pre-start journal cursor");

    // 6. Appended workspace: read, append beyond the roster, verify, host.
    event("workspace-read");
    const r0 = await helperOp({ op: "read" });
    const w0 = r0.filter((r) => Array.isArray(r.desktops))[0];
    if (!w0) { fail("workspace read produced no row"); }
    if (!w0.desktops.length) { fail("empty desktop roster"); }
    const ids0 = w0.desktops.slice();
    if (new Set(ids0).size !== ids0.length) { fail("desktop roster has duplicate ids"); }
    if (ids0.indexOf(w0.current) < 0) { fail("current desktop not in roster"); }
    S.roster = ids0;
    S.original = w0.current;
    S.counts.desktopsBefore = ids0.length;

    // The append cleanup is armed BEFORE the append call. When the append never
    // reported an id, the victim derives from a roster diff (never a guess);
    // a diff that is not exactly one new desktop removes nothing.
    const preAppendRoster = ids0.slice();
    S.cleanups.push({ name: "workspace-remove", order: 40, fn: async () => {
      const rr = await helperOp({ op: "read" });
      const row = rr.filter((r) => Array.isArray(r.desktops))[0];
      if (!row) { fail("remove verification read produced no row"); }
      let victim = S.appended;
      if (!victim) {
        const freshIds = row.desktops.filter((id) => preAppendRoster.indexOf(id) < 0);
        const missingIds = preAppendRoster.filter((id) => row.desktops.indexOf(id) < 0);
        if (freshIds.length === 0 && missingIds.length === 0) {
          S.counts.appendedAlreadyAbsent = 1;
          return;
        }
        if (freshIds.length !== 1 || missingIds.length !== 0 || row.desktops.length !== preAppendRoster.length + 1) {
          fail("append cleanup ambiguous: roster diff is not exactly one new desktop (remove nothing)");
        }
        victim = freshIds[0];
        S.appended = victim;
      } else if (row.desktops.indexOf(victim) < 0) {
        S.counts.appendedAlreadyAbsent = 1;
        return;
      }
      if (row.current === victim) {
        // Still current: restore the original exactly once before removal.
        // When the restore is not possible, do not remove; report instead.
        try {
          mutOnce("workspace-setcurrent-original-remove-path");
          await helperOp({ op: "setCurrent", id: S.original }, { mutate: true });
          const rcRead = await helperOp({ op: "read" });
          const rcRow = rcRead.filter((r) => Array.isArray(r.desktops))[0];
          if (!rcRow || rcRow.current !== S.original) { fail("original current desktop not restored"); }
        } catch (e) {
          fail("appended desktop is current and restore is not possible; refusing removal (" +
            (e && e.message ? e.message : String(e)).slice(0, 120) + ")");
        }
      }
      mutOnce("workspace-remove");
      await helperOp({ op: "remove", id: victim }, { mutate: true });
      const rr2 = await helperOp({ op: "read" });
      const row2 = rr2.filter((r) => Array.isArray(r.desktops))[0];
      if (!row2) { fail("remove proof read produced no row"); }
      if (row2.desktops.indexOf(victim) >= 0) { fail("appended desktop still present after remove"); }
      S.counts.appendedRemovedByRun = 1;
    } });

    mutOnce("workspace-append");
    event("workspace-append");
    const appendName = "omnitiler-test-" + randomBytes(4).toString("hex");
    const a1 = await helperOp({ op: "append", name: appendName }, { mutate: true });
    const aRow = a1.filter((r) => typeof r.id === "string")[0];
    if (!aRow) { fail("append produced no id"); }
    if (ids0.indexOf(aRow.id) >= 0) { fail("appended id collides with the roster"); }
    event("workspace-reread");
    const r1 = await helperOp({ op: "read" });
    const w1 = r1.filter((r) => Array.isArray(r.desktops))[0];
    if (!w1) { fail("append verification read produced no row"); }
    const ids1 = w1.desktops;
    const fresh = ids1.filter((id) => ids0.indexOf(id) < 0);
    const effectPresent = fresh.length === 1 && ids1.length === ids0.length + 1 &&
      ids1[ids1.length - 1] === fresh[0] && fresh[0] === aRow.id;
    if (!effectPresent) { fail("append did not take effect exactly once at the last position (ambiguous: stop)"); }
    S.appended = aRow.id;

    mutOnce("workspace-setcurrent-appended");
    event("workspace-setcurrent");
    await helperOp({ op: "setCurrent", id: S.appended }, { mutate: true });
    const r2 = await helperOp({ op: "read" });
    const w2 = r2.filter((r) => Array.isArray(r.desktops))[0];
    if (!w2) { fail("setCurrent verification read produced no row"); }
    if (w2.current !== S.appended) { fail("current is not the appended workspace (ambiguous: stop)"); }
    S.cleanups.push({ name: "workspace-restore", order: 20, fn: async () => {
      mutOnce("workspace-setcurrent-original");
      await helperOp({ op: "setCurrent", id: S.original }, { mutate: true });
      const rr = await helperOp({ op: "read" });
      const row = rr.filter((r) => Array.isArray(r.desktops))[0];
      if (!row || row.current !== S.original) { fail("original current desktop not restored"); }
    } });

    // 7. Dev stack via the production detached lifecycle (one start).
    mutOnce("dev-on");
    event("dev-on");
    // dev-off is armed BEFORE the start: a half-up dev-on still gets one.
    S.cleanups.push({ name: "dev-off", order: 30, fn: async () => {
      mutOnce("dev-off");
      const off = await exec(devOffCmd(), T.exec);
      if (off.status !== 0) { fail("dev-off failed"); }
    } });
    const devR = await exec(devOnCmd(), T.devOn);
    if (devR.status !== 0) { fail("dev-on failed; no retry, no relaunch"); }
    S.devUp = true;
    const rt = env.XDG_RUNTIME_DIR || "/tmp";
    const pidFile = String(await io.readFile(rt + "/omnitiler-dev/planner-pid")).trim();
    const receiptPtr = String(await io.readFile(rt + "/omnitiler-dev/controller-receipt-path")).trim().split("\n")[0];
    if (!/^[1-9][0-9]*$/.test(pidFile)) { fail("planner-pid file is not a pid"); }
    let receipt;
    try { receipt = JSON.parse(String(await io.readFile(receiptPtr))); } catch (e) { fail("controller receipt unparsable"); }
    if (!receipt || !Number.isInteger(receipt.script_id) || receipt.script_id < 0 || receipt.script_id > 2147483647) {
      fail("controller receipt has no valid script_id");
    }
    const pOwnerR = await busJson(["busctl", "--user", "--json=short", "call",
      DBUS_BUS, DBUS_OBJECT, DBUS_IFACE, "GetNameOwner", "s", PLANNER_BUS], "planner GetNameOwner");
    if (!pOwnerR || pOwnerR.type !== "s" || !Array.isArray(pOwnerR.data)) { fail("planner GetNameOwner: need {type:s,data:[owner]}"); }
    const pOwner = pOwnerR.data[0];
    if (typeof pOwner !== "string" || !UNIQUE_RE.test(pOwner)) { fail("planner owner is not a unique name"); }
    const pPidR = await busJson(["busctl", "--user", "--json=short", "call",
      DBUS_BUS, DBUS_OBJECT, DBUS_IFACE, "GetConnectionUnixProcessID", "s", pOwner], "planner owner pid");
    if (!pPidR || pPidR.type !== "u" || !Array.isArray(pPidR.data)) { fail("planner owner pid: need {type:u,data:[pid]}"); }
    const pPid = pPidR.data[0];
    if (!Number.isInteger(pPid) || pPid <= 0) { fail("bad planner PID reply"); }
    if (String(pPid) !== pidFile) { fail("planner-pid file does not equal the D-Bus owner pid"); }
    const pTick = await io.procIdentity(pPid);
    if (pTick === null) { fail("planner process identity unavailable"); }
    S.planner = { pid: pPid, tick: pTick, scriptId: receipt.script_id };
    S.receipts.planner = { pid: pPid, tick: pTick, scriptId: receipt.script_id };
    const st2 = await mustExec(devStatusCmd(), "dev-status after dev-on");
    if (parseDevStatusMode(st2.stdout) !== "UP") { fail("dev mode is not UP after dev-on"); }

    // 8. Readiness gate after the only start, against the pre-start cursor.
    // No restart on missing lines. Only booleans/counts are persisted.
    event("readiness");
    let gate = null;
    const gateEnd = io.now() + T.readiness;
    for (;;) {
      const j = await mustExec(["journalctl", "--user", "--after-cursor", cursor0, "-o", "json", "_PID=" + S.kwin.pid], "readiness journal");
      gate = parseReadiness(redact(j.stdout));
      if (readinessPass(gate)) { break; }
      if (io.now() >= gateEnd) { break; }
      await io.sleep(T.readinessPoll);
    }
    S.counts.readiness = gate ? {
      ready: gate.ready ? 1 : 0, discovered: gate.discovered ? 1 : 0,
      dispatch: gate.dispatch ? 1 : 0, applied: gate.applied ? 1 : 0,
    } : null;
    if (!gate || !readinessPass(gate)) { fail("readiness gate missing within the bounded window: stop, no restart"); }

    // 9. Spawn fixtures a and b in ONE request, then wait until get returns both.
    mutOnce("fixture-spawn");
    event("fixture-spawn");
    {
      const r = await exec(fixtureSendCmd(S.fixture.socket, S.fixture.nonce,
        ['{"v":1,"request_id":"spawn-ab","op":"spawn","windows":[{"id":"a"},{"id":"b"}]}']), T.exec);
      if (r.status !== 0) {
        await fixtureSend(['{"v":1,"request_id":"state-check","op":"list"}'], "fixture list");
        fail("fixture spawn failed; state re-read once, no retry");
      }
      // A status-0 transport reply is not enough: require a parsable ok
      // acknowledgement, so a corrupt reply fails here.
      let srep;
      try {
        srep = r.stdout.split("\n").map((l) => l.trim()).filter(Boolean).map((l) => JSON.parse(l));
      } catch (e) { fail("fixture spawn: unparsable fixture reply"); }
      if (!srep.length || srep.some((x) => !x || x.ok !== true)) { fail("fixture spawn: no ok acknowledgement"); }
    }
    let snaps = null;
    {
      const end = io.now() + T.admit;
      for (;;) {
        const got = await fixtureSnapshots(["a", "b"]);
        if (got.has("a") && got.has("b")) { snaps = got; break; }
        if (io.now() >= end) { fail("fixture get never returned both windows within the bounded wait"); }
        await io.sleep(T.poll);
      }
    }

    // 10. Join each caller id to a native UUID exactly, confirmed by
    // getWindowInfo per UUID. Fail closed on missing/duplicate/ambiguous.
    event("native-read");
    const nr = await helperOp({ op: "read" });
    const nrow = nr.filter((r) => Array.isArray(r.owned))[0];
    if (!nrow) { fail("native read produced no roster"); }
    S.counts.foreign = nrow.foreignCount || 0;
    const owned = nrow.owned;
    {
      const seenU = new Set();
      for (const w of owned) {
        if (typeof w.uuid !== "string" || !UUID_RE.test(w.uuid)) { fail("owned row has a bad UUID"); }
        if (seenU.has(w.uuid)) { fail("duplicate native UUID " + w.uuid); }
        seenU.add(w.uuid);
      }
    }
    const joins = [];
    const used = new Set();
    for (const id of ["a", "b"]) {
      const snap = snaps.get(id);
      const cands = owned.filter((w) => w.class === SCOPE_CLASS && w.pid === snap.pid && w.title === snap.title);
      if (cands.length === 0) { fail("join for '" + id + "' is absent: need exactly one (stop, no retry)"); }
      if (cands.length !== 1) { fail("join for '" + id + "' is ambiguous: need exactly one (stop, no retry)"); }
      if (used.has(cands[0].uuid)) { fail("native " + cands[0].uuid + " joined twice"); }
      used.add(cands[0].uuid);
      const info = await windowInfo(cands[0].uuid);
      if (info.resourceClass !== SCOPE_CLASS) { fail("join for '" + id + "': getWindowInfo class mismatch"); }
      if (info.caption !== snap.title) { fail("join for '" + id + "': getWindowInfo caption mismatch"); }
      if (info.pid !== snap.pid) { fail("join for '" + id + "': getWindowInfo pid mismatch"); }
      if (info.desks.length !== 1 || info.desks[0] !== S.appended) {
        fail("join for '" + id + "': getWindowInfo desktops mismatch (need exactly the appended workspace)");
      }
      joins.push({ callerId: id, uuid: cands[0].uuid, title: snap.title, infoCaption: info.caption });
    }
    S.joins = joins;
    event("join");
    check("join-a", joins[0].infoCaption === snaps.get("a").title, snaps.get("a").title, joins[0].infoCaption);
    check("join-b", joins[1].infoCaption === snaps.get("b").title, snaps.get("b").title, joins[1].infoCaption);

    // 11. Exact native activation of a (no pointer), verified read-only.
    mutOnce("activate-a");
    event("activate");
    await helperOp({ op: "activate", uuid: joins[0].uuid }, { mutate: true });
    {
      const end = io.now() + T.admit;
      let activeOk = false;
      let activeObserved = null;
      for (;;) {
        const ar = await helperOp({ op: "read" });
        const arow = ar.filter((r) => Array.isArray(r.owned))[0];
        if (arow) { activeObserved = normalizeNativeId(arow.active); }
        if (arow && activeObserved === normalizeNativeId(joins[0].uuid)) { activeOk = true; break; }
        if (io.now() >= end) { break; }
        await io.sleep(T.poll);
      }
      if (!activeOk) { fail("activation of a unverified within the bounded wait"); }
      S.activeObserved = activeObserved;
    }
    check("activate-a", S.activeObserved === normalizeNativeId(joins[0].uuid),
      normalizeNativeId(joins[0].uuid), S.activeObserved);

    // 12. The ONLY scenario: ordinary two-fixture tiling and close/reflow.
    S.scenario = "admission";
    event("scenario-admission");
    const bare = (u) => normalizeNativeId(u);
    const admit = await (async () => {
      const end = io.now() + T.admit;
      let first = "no read yet";
      let last = "no read yet";
      const note = (m) => { if (first === "no read yet") { first = m; } last = m; };
      for (;;) {
        const proj = await readPresentation();
        const nat = await helperOp({ op: "read" });
        const row = nat.filter((r) => Array.isArray(r.owned))[0];
        if (!row) { note("native read produced no roster"); }
        else {
          const wA = proj.windows.filter((w) => w.win === bare(joins[0].uuid))[0];
          const wB = proj.windows.filter((w) => w.win === bare(joins[1].uuid))[0];
          if (!wA || !wB) { note("presentation does not list both windows"); }
          else if (proj.windows.length !== 2 ||
            proj.windows.some((w) => w.win !== bare(joins[0].uuid) && w.win !== bare(joins[1].uuid))) {
            note("presentation window set is not exactly {A,B}: [" +
              proj.windows.map((w) => w.win).sort().join(",") + "]");
          }
          else if (wA.alloc === null || wB.alloc === null) { note("presentation alloc unknown (null never matches)"); }
          else {
            const rA = row.owned.filter((w) => w.uuid === joins[0].uuid)[0];
            const rB = row.owned.filter((w) => w.uuid === joins[1].uuid)[0];
            const scopeOwned = row.owned.filter((w) => w.class === SCOPE_CLASS);
            if (!rA || !rB) { note("native rows for A/B absent"); }
            else if (scopeOwned.length !== 2 ||
              !scopeOwned.some((w) => w.uuid === joins[0].uuid) ||
              !scopeOwned.some((w) => w.uuid === joins[1].uuid)) {
              note("native scope roster is not exactly {A,B}");
            }
            else if (rA.frame === null || rB.frame === null) { note("native frame unknown (PENDING, never PASS)"); }
            else if (!rA.outRect || !rB.outRect) { note("native output geometry unknown (PENDING, never PASS)"); }
            else {
              const expA = frameOf(wA.alloc), expB = frameOf(wB.alloc);
              if (!frameEqual(rA.frame, expA)) { note("tile A frame mismatch (" + frameDiff(rA.frame, expA).join(",") + "): native " + JSON.stringify(rA.frame) + " != alloc " + JSON.stringify(expA) + " raw " + JSON.stringify(rA.frameRaw)); }
              else if (!frameEqual(rB.frame, expB)) { note("tile B frame mismatch (" + frameDiff(rB.frame, expB).join(",") + "): native " + JSON.stringify(rB.frame) + " != alloc " + JSON.stringify(expB) + " raw " + JSON.stringify(rB.frameRaw)); }
              else if (!frameInside(rA.frame, rA.outRect) || !frameInside(rB.frame, rB.outRect)) { note("tile frame outside its output"); }
              else if (rectsOverlap(rA.frame, rB.frame)) { note("tiles A/B overlap: not an ordinary split"); }
              else if (wA.ws !== wB.ws) { note("A/B in different presentation workspaces"); }
              else {
                let deskBad = null;
                for (const j of joins) {
                  const info = await windowInfo(j.uuid);
                  if (info.desks.length !== 1 || info.desks[0] !== S.appended) {
                    deskBad = "fixture '" + j.callerId + "' desktops mismatch after join (need exactly the appended workspace)";
                    break;
                  }
                }
                if (deskBad) { note(deskBad); }
                else {
                  return {
                    proj, expA, expB, fA: rA.frame, fB: rB.frame, out: rA.outRect,
                    outName: rA.out, rawA: rA.frameRaw, rawB: rB.frameRaw,
                    outScale: (typeof rA.outScale === "number" && isFinite(rA.outScale) && rA.outScale > 0) ? rA.outScale : null,
                    sameOut: rA.out === rB.out, wsA: wA.ws, wsB: wB.ws,
                  };
                }
              }
            }
          }
        }
        if (io.now() >= end) {
          if (last.indexOf("unknown") >= 0 || last.indexOf("PENDING") >= 0) { S.category = "PENDING"; }
          fail("admission: " + first + (last !== first ? " (last: " + last + ")" : ""));
        }
        await io.sleep(T.poll);
      }
    })();
    if (admit.sameOut !== true) { fail("admission: A/B on different outputs here"); }
    S.receipts.output = {
      name: admit.outName,
      scale: (typeof admit.outScale === "number" && isFinite(admit.outScale) && admit.outScale > 0) ? admit.outScale : null,
      rect: admit.out,
      raw: { a: admit.rawA, b: admit.rawB },
    };
    check("admit-a-frame-equals-alloc", frameEqual(admit.fA, admit.expA), admit.expA, admit.fA);
    check("admit-b-frame-equals-alloc", frameEqual(admit.fB, admit.expB), admit.expB, admit.fB);
    check("admit-disjoint", !rectsOverlap(admit.fA, admit.fB), "disjoint", "disjoint");
    check("admit-inside-output", frameInside(admit.fA, admit.out) && frameInside(admit.fB, admit.out), admit.out, admit.out);
    check("admit-same-ws", admit.wsA === admit.wsB, admit.wsA, admit.wsB);
    S.receipts.presentationRev = admit.proj.rev;
    event("scenario-close-b");

    S.scenario = "reflow";
    mutOnce("fixture-close-b");
    {
      const r = await exec(fixtureSendCmd(S.fixture.socket, S.fixture.nonce,
        ['{"v":1,"request_id":"close-b","op":"close","ids":["b"]}']), T.exec);
      if (r.status !== 0) {
        await fixtureSend(['{"v":1,"request_id":"state-check","op":"list"}'], "fixture list");
        fail("fixture close failed; state re-read once, no retry");
      }
      let crep;
      try {
        crep = r.stdout.split("\n").map((l) => l.trim()).filter(Boolean).map((l) => JSON.parse(l));
      } catch (e) { fail("fixture close: unparsable fixture reply"); }
      if (!crep.length || crep.some((x) => !x || x.ok !== true)) { fail("fixture close: no ok acknowledgement"); }
    }
    const before = admit.fA;
    const reflow = await (async () => {
      const end = io.now() + T.reflow;
      let first = "no read yet";
      let last = "no read yet";
      const note = (m) => { if (first === "no read yet") { first = m; } last = m; };
      for (;;) {
        const proj = await readPresentation();
        const nat = await helperOp({ op: "read" });
        const row = nat.filter((r) => Array.isArray(r.owned))[0];
        if (!row) { note("native read produced no roster"); }
        else if (proj.windows.some((w) => w.win === bare(joins[1].uuid))) { note("presentation still lists closed b"); }
        else if (proj.windows.length !== 1 || proj.windows[0].win !== bare(joins[0].uuid)) {
          note("presentation window set is not exactly {A} after close: [" +
            proj.windows.map((w) => w.win).sort().join(",") + "]");
        }
        else {
          const wA = proj.windows.filter((w) => w.win === bare(joins[0].uuid))[0];
          if (!wA) { note("presentation lost a after close"); }
          else if (wA.alloc === null) { note("reflow alloc unknown (null never matches)"); }
          else {
            const rA = row.owned.filter((w) => w.uuid === joins[0].uuid)[0];
            if (!rA) { note("native row for A absent after close"); }
            else if (rA.frame === null) { note("native frame for A unknown after close"); }
            else if (!rA.outRect) { note("native output geometry unknown after close"); }
            else {
              const expA = frameOf(wA.alloc);
              const scopeOwned = row.owned.filter((w) => w.class === SCOPE_CLASS);
              if (row.owned.some((w) => w.uuid === joins[1].uuid)) { note("b still in the native roster after close"); }
              else if (scopeOwned.length !== 1 || scopeOwned[0].uuid !== joins[0].uuid) {
                note("native scope roster is not exactly {A} after close");
              }
              else if (!frameEqual(rA.frame, expA)) { note("reflow A frame mismatch (" + frameDiff(rA.frame, expA).join(",") + "): native " + JSON.stringify(rA.frame) + " != alloc " + JSON.stringify(expA) + " raw " + JSON.stringify(rA.frameRaw)); }
              else if (!frameInside(expA, rA.outRect)) { note("reflow allocation outside output bounds"); }
              else if (frameEqual(expA, before)) { note("reflowed A alloc unchanged after closing B"); }
              else if (!(frameArea(expA) > frameArea(before))) { note("reflow did not grow A after closing B"); }
              else {
                const info = await windowInfo(joins[0].uuid);
                if (info.desks.length !== 1 || info.desks[0] !== S.appended) {
                  note("fixture 'a' desktops mismatch after join (need exactly the appended workspace)");
                } else {
                  return {
                    proj, expA, fA: rA.frame, out: rA.outRect,
                    bProj: proj.windows.some((w) => w.win === bare(joins[1].uuid)) ? "present" : "absent",
                    bNat: row.owned.some((w) => w.uuid === joins[1].uuid) ? "present" : "absent",
                  };
                }
              }
            }
          }
        }
        if (io.now() >= end) {
          if (last.indexOf("unknown") >= 0) { S.category = "PENDING"; }
          fail("reflow: " + first + (last !== first ? " (last: " + last + ")" : ""));
        }
        await io.sleep(T.poll);
      }
    })();
    check("reflow-b-gone-from-presentation", reflow.bProj === "absent", "absent", reflow.bProj);
    check("reflow-b-absent-native", reflow.bNat === "absent", "absent", reflow.bNat);
    check("reflow-a-alloc-changed-larger", frameArea(reflow.expA) > frameArea(before), frameArea(before), frameArea(reflow.expA));
    check("reflow-a-frame-equals-alloc", frameEqual(reflow.fA, reflow.expA), reflow.expA, reflow.fA);
    check("reflow-a-inside-output", frameInside(reflow.expA, reflow.out), reflow.out, reflow.out);
    S.scenario = "done";
    S.category = "PASS";
    event("scenario-done");
  } catch (e) {
    if (S.category === "NOT-RUN") {
      const msg = e && e.message ? e.message : String(e);
      S.divergence = redact(msg).slice(0, 300);
      S.category = "FAIL";
    } else if (S.divergence === null) {
      S.divergence = redact(e && e.message ? e.message : String(e)).slice(0, 300);
    }
  } finally {
    // Cleanup ALWAYS runs in declared order, each step in try/catch;
    // failures are collected, reported, and force a nonzero exit.
    event("cleanup");
    const queue = S.cleanups.slice().sort((a, b) => a.order - b.order);
    for (let i = 0; i < queue.length; i++) {
      const c = queue[i];
      event("cleanup-" + c.name);
      try {
        await c.fn();
        S.counts["cleanup-" + c.name] = "ok";
      } catch (e) {
        S.counts["cleanup-" + c.name] = "failed";
        S.cleanupFailed.push(c.name + ": " + redact(e && e.message ? e.message : String(e)).slice(0, 200));
      }
    }
    for (const t of S.teardownIssues.splice(0)) { S.cleanupFailed.push(t); }
    // Final verifications: never silently assumed.
    const verify = async (name, cond) => {
      event("verify-" + name);
      let pass = false;
      try { pass = !!(await cond()); } catch (e) { pass = false; }
      check("cleanup-" + name, pass, true, pass);
      if (!pass) { S.cleanupFailed.push("verify-" + name); }
    };
    if (S.fixture) {
      await verify("fixture-pid-gone", async () => (await io.procIdentity(S.fixture.pid)) === null);
      await verify("cargo-pid-gone", async () => (await io.procIdentity(S.cargo.pid)) === null);
      await verify("fixture-socket-absent", async () => !(await io.exists(S.fixture.socket)));
      await verify("rundir-removed", async () => !(await io.exists(S.cargo.dir)));
    }
    if (S.planner) {
      await verify("planner-pid-gone", async () => (await io.procIdentity(S.planner.pid)) === null);
    }
    if (S.ydoc) {
      await verify("ydotoold-gone", async () => (await io.procIdentity(S.ydoc.pid)) === null);
    }
    if (S.original) {
      await verify("current-restored", async () => {
        const rr = await helperOp({ op: "read" });
        const row = rr.filter((r) => Array.isArray(r.desktops))[0];
        return !!row && row.current === S.original;
      });
    }
    if (S.appended) {
      await verify("appended-absent", async () => {
        const rr = await helperOp({ op: "read" });
        const row = rr.filter((r) => Array.isArray(r.desktops))[0];
        return !!row && row.desktops.indexOf(S.appended) < 0;
      });
    }
    {
      let down = false;
      try {
        const f = await exec(devStatusCmd(), T.exec);
        down = f.status === 0 && parseDevStatusMode(f.stdout) === "DOWN";
      } catch (e) { down = false; }
      check("cleanup-dev-down", down, "DOWN", down ? "DOWN" : "not-DOWN");
      if (!down) { S.cleanupFailed.push("verify-dev-down"); }
      event("verify-dev-down");
    }
    // The verifications above call helperOp, whose unload/source-rm failures
    // land in teardownIssues after the first drain: drain again so none are
    // dropped, then decide the exit code.
    for (const t of S.teardownIssues.splice(0)) { S.cleanupFailed.push(t); }
    if (S.cleanupFailed.length) { rc = 1; }
    else if (S.category === "PASS") { rc = 0; }
    else { rc = 1; }
  }
  return {
    rc, category: S.category, scenario: S.scenario, divergence: S.divergence,
    events: S.events, calls: S.calls, checks: S.checks, receipts: S.receipts,
    counts: S.counts, cleanupFailed: S.cleanupFailed,
    joins: S.joins ? S.joins.map((j) => ({ callerId: j.callerId, uuid: j.uuid })) : null,
  };
}

// ---------------------------------------------------------------------------
// Production io (the only node-API boundary) and CLI
// ---------------------------------------------------------------------------

function prodExec(argv, opts) {
  opts = opts || {};
  try {
    const out = execFileSync(argv[0], argv.slice(1), {
      encoding: "utf8", stdio: ["ignore", "pipe", "pipe"],
      env: opts.env, cwd: opts.cwd, timeout: opts.timeoutMs, maxBuffer: 16 * 1024 * 1024,
    });
    return { status: 0, stdout: out, stderr: "" };
  } catch (e) {
    return {
      status: typeof e.status === "number" ? e.status : 1,
      stdout: typeof e.stdout === "string" ? e.stdout : "",
      stderr: typeof e.stderr === "string" ? e.stderr : String((e && e.message) || e),
    };
  }
}

function prodSpawn(argv, opts) {
  opts = opts || {};
  const child = spawn(argv[0], argv.slice(1), { env: opts.env, cwd: opts.cwd, stdio: ["ignore", "pipe", "pipe"] });
  const pid = child.pid;
  let buf = "";
  const waiters = [];
  let exited = null;
  const exitWaiters = [];
  child.stdout.on("data", (d) => {
    buf += d.toString("utf8");
    let ix;
    while ((ix = buf.indexOf("\n")) >= 0) {
      const line = buf.slice(0, ix);
      buf = buf.slice(ix + 1);
      for (let i = waiters.length - 1; i >= 0; i--) {
        let ok = false;
        try { ok = waiters[i].pred(line); } catch (e) { ok = false; }
        if (ok) { clearTimeout(waiters[i].timer); waiters.splice(i, 1)[0].resolve(line); }
      }
    }
  });
  // stderr is drained and dropped: an unread pipe fills (cargo build output)
  // and blocks the child before it announces.
  child.stderr.resume();
  child.on("exit", (code, signal) => {
    exited = { code, signal };
    for (const w of exitWaiters.splice(0)) { clearTimeout(w.timer); w.resolve(exited); }
    for (const w of waiters.splice(0)) { clearTimeout(w.timer); w.reject(new Error("eof before matching line")); }
  });
  child.on("error", () => { /* exit follows */ });
  return {
    pid,
    exited: (timeoutMs) => new Promise((resolve) => {
      if (exited) { resolve(exited); return; }
      const timer = setTimeout(() => {
        const ix = exitWaiters.findIndex((w) => w.resolve === resolve);
        if (ix >= 0) { exitWaiters.splice(ix, 1); }
        resolve(null);
      }, timeoutMs);
      exitWaiters.push({ resolve, timer });
    }),
    waitLine: (pred, timeoutMs) => new Promise((resolve, reject) => {
      const timer = setTimeout(() => {
        const ix = waiters.findIndex((w) => w.resolve === resolve);
        if (ix >= 0) { waiters.splice(ix, 1); }
        reject(new Error("waitLine timed out"));
      }, timeoutMs);
      waiters.push({ pred, resolve, reject, timer });
    }),
    kill: async (sig) => { try { child.kill(sig || "SIGTERM"); } catch (e) { /* already gone */ } },
  };
}

// A real /proc/<pid>/stat is one line ending in exactly one newline.
export function parseProcStat(text, pid) {
  const stat = text.endsWith("\n") ? text.slice(0, -1) : text;
  if (stat.indexOf("\n") >= 0) { return null; }
  const sp = stat.lastIndexOf(") ");
  if (sp < 0) { return null; }
  if (stat.slice(0, stat.indexOf(" ")) !== String(pid)) { return null; }
  const f = stat.slice(sp + 2).split(" ");
  if (f.length < 20 || !/^[A-Za-z]$/.test(f[0] || "") || !/^[1-9][0-9]*$/.test(f[19] || "")) { return null; }
  return f[19];
}

function procIdentitySync(pid) {
  let stat;
  try { stat = readFileSync("/proc/" + pid + "/stat", "utf8"); } catch (e) { return null; }
  return parseProcStat(stat, pid);
}

export function createLiveIo() {
  return {
    exec: async (argv, o) => prodExec(argv, o),
    spawnBackground: async (argv, o) => prodSpawn(argv, o),
    readFile: async (p) => readFileSync(p),
    writeFile: async (p, d, m) => writeFileSync(p, d, { mode: m }),
    mkdir: async (d, m) => mkdirSync(d, { mode: m, recursive: false }),
    rmdir: async (d) => rmdirSync(d),
    rm: async (p) => rmSync(p, { force: true }),
    exists: async (p) => { try { return existsSync(p); } catch (e) { return false; } },
    procIdentity: async (pid) => procIdentitySync(pid),
    sleep: (ms) => new Promise((r) => setTimeout(r, ms)),
    now: () => Date.now(),
  };
}

export async function liveMain(o) {
  o = o || {};
  const argv = o.argv || process.argv.slice(2);
  const execEnv = o.execEnv || process.env;
  const ioFactory = o.ioFactory || createLiveIo;
  let args;
  try { args = parseArgs(argv); } catch (e) {
    console.error("kde-scenarios: " + (e && e.message ? e.message : String(e)));
    return 2;
  }
  if (args.help) { console.log(helpText()); return 0; }
  if (!args.live) {
    console.error("refusing: live mode not enabled (pass --live to run against the real session). Nothing executed.");
    return 2;
  }
  const root = o.root || repoRoot();
  let evidenceDir;
  try { evidenceDir = checkEvidenceDir(args.evidenceDir, root); } catch (e) {
    console.error("kde-scenarios: " + (e && e.message ? e.message : String(e)));
    return 2;
  }
  // The ONLY mutation before the scenario: create the evidence dir ONCE.
  const io = ioFactory();
  if (await io.exists(evidenceDir)) {
    console.error("kde-scenarios: refusing existing evidence dir");
    return 2;
  }
  await io.mkdir(evidenceDir, 0o700);
  const templatePath = o.templatePath || path.join(path.dirname(fileURLToPath(import.meta.url)), "kde-scenarios-helper.js");
  const res = await runScenario({ io, root, env: execEnv, evidenceDir, input: args.input, templatePath });
  // Evidence: step results, named checks, receipts, counts. No nonces, no
  // foreign window metadata, no home/host names (relative names only).
  const evidence = {
    v: 1, category: res.category, scenario: res.scenario, divergence: res.divergence,
    events: res.events, checks: res.checks, receipts: res.receipts, counts: res.counts,
    cleanupFailed: res.cleanupFailed,
    joins: res.joins,
  };
  await io.writeFile(evidenceDir + "/result.json", JSON.stringify(evidence, null, 2) + "\n", 0o600);
  console.log("category: " + res.category + " scenario: " + res.scenario);
  for (const c of res.checks) {
    console.log((c.pass ? "PASS" : "FAIL") + " " + c.name +
      " expected=" + JSON.stringify(c.expected) + " actual=" + JSON.stringify(c.actual));
  }
  console.log("receipts: " + JSON.stringify(res.receipts));
  console.log("cleanupFailed: " + JSON.stringify(res.cleanupFailed));
  return res.rc;
}

const isMain = (() => {
  try { return path.resolve(process.argv[1]) === fileURLToPath(import.meta.url); } catch (e) { return false; }
})();
if (isMain) {
  liveMain().then((c) => process.exit(c)).catch((e) => {
    console.error("kde-scenarios: " + (e && e.message ? e.message : String(e)));
    process.exit(1);
  });
}
