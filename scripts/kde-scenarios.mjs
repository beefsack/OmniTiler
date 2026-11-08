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
  stateStep: 15000,
  // Rapid-burst quiescence: the settled-state predicate must hold on
  // burstSettleReads consecutive polls spanning at least burstSettleMin of
  // runner clock (io.now()) inside the overall stateStep budget.
  burstSettleReads: 3,
  burstSettleMin: 2000,
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
    "usage: node scripts/kde-scenarios.mjs [--help] [--live --evidence-dir PATH [--input] [--scenario NAME]]",
    "",
    "Fixture-driven KDE native scenario runner. Offline by default.",
    "Three scenarios on a workspace appended beyond the whole roster:",
    "  tiling-reflow: admission A/B tiling then close-B/A-reflow (default).",
    "  state-targets:  maximize/fullscreen/float/sticky chord journey with an",
    "                  orderly production restart (requires --input).",
    "  resize-refusal: keyboard grow-towards-direction journey with planner",
    "                  refusal-line oracles (requires --input).",
    "  rapid-burst:    60-chord FIFO focus/move burst with journal-order and",
    "                  settled-geometry oracles (requires --input).",
    "",
    "  --help            show this help and exit (never mutates)",
    "  --live            explicit live mode against the real session.",
    "                    Without --live nothing executes; exit 2.",
    "  --evidence-dir P  fresh private dir for evidence. Must be absolute,",
    "                    canonical, outside the repository, and not pre-exist.",
    "  --input           also start ydotoold (lifecycle only for tiling-reflow,",
    "                    chord injection for state-targets). Default off.",
    "  --scenario NAME   tiling-reflow (default), state-targets,",
    "                    resize-refusal, or rapid-burst. state-targets,",
    "                    resize-refusal, and rapid-burst REQUIRE --input.",
    "                    Unknown values exit 2 before any io.",
  ].join("\n");
}

export function parseArgs(argv) {
  const out = { help: false, live: false, evidenceDir: null, input: false, scenario: "tiling-reflow" };
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
    } else if (a === "--scenario") {
      const v = argv[++i];
      if (!v) { throw new ScenarioError("--scenario requires a value"); }
      if (out.scenario !== "tiling-reflow") { throw new ScenarioError("duplicate --scenario"); }
      if (v !== "tiling-reflow" && v !== "state-targets" && v !== "resize-refusal" && v !== "rapid-burst") { throw new ScenarioError("unknown --scenario '" + v + "'"); }
      out.scenario = v;
    } else if (a.startsWith("--scenario=")) {
      const v = a.slice("--scenario=".length);
      if (!v) { throw new ScenarioError("--scenario requires a value"); }
      if (out.scenario !== "tiling-reflow") { throw new ScenarioError("duplicate --scenario"); }
      if (v !== "tiling-reflow" && v !== "state-targets" && v !== "resize-refusal" && v !== "rapid-burst") { throw new ScenarioError("unknown --scenario '" + v + "'"); }
      out.scenario = v;
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

// Planner `omnitiler:resize-refused` diagnostic tokens (world_engine.rs):
// extract only the bounded closed-vocabulary tokens plus the opaque subject
// window id (never titles, captions, or focus ids). The subject is compared
// in memory only and never recorded. Missing keys read undefined (never a
// guess).
export function parseResizeRefusalTokens(line) {
  const out = {};
  const re = /(?:^|\s)(internal|wire|dir|mode|press|rowsTotal|rowsShown|tree|subject)=([^\s]+)/g;
  let m;
  const text = String(line);
  while ((m = re.exec(text)) !== null) { out[m[1]] = m[2]; }
  return out;
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
    // Floating flag (WorldReplyWindow.floating, always a boolean on the
    // wire): strict boolean or reject when present; absent reads null
    // (unknown, never matches), never a guess.
    let floating = null;
    if (w.floating !== undefined && w.floating !== null) {
      if (typeof w.floating !== "boolean") {
        throw new ScenarioError("presentation window " + i + ": bad floating");
      }
      floating = w.floating;
    }
    return { win: w.win, out: w.out, ws: w.ws, alloc: w.alloc, floating };
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
  // Native state targets (KWin v6.8.1 src/dbusinterface.cpp
  // clientToVariantMap): maximize axes are the plain-enum MaximizeMode bit
  // test (MaximizeVertical 1 / MaximizeHorizontal 2, src/effect/globals.h),
  // so an int 0/nonzero; fullscreen/keepAbove are bools. All-desktop
  // (sticky) has no key: it is the desktops array being empty
  // (Window::setOnAllDesktops(true) calls setDesktops({})). Missing keys read null (unknown, never matches); present
  // values are strictly wrapped with exact source types.
  const optBool = (k) => {
    if (info[k] === undefined) { return null; }
    const d = wrapped(k, ["b"]);
    if (typeof d !== "boolean") { throw new ScenarioError("getWindowInfo " + uuid + ": bad boolean data for '" + k + "'"); }
    return d;
  };
  const optMax = (k) => {
    if (info[k] === undefined) { return null; }
    const d = wrapped(k, ["i"]);
    if (!Number.isInteger(d)) { throw new ScenarioError("getWindowInfo " + uuid + ": bad maximize data for '" + k + "'"); }
    return d !== 0;
  };
  const maxH = optMax("maximizeHorizontal");
  const maxV = optMax("maximizeVertical");
  const fullscreen = optBool("fullscreen");
  const keepAbove = optBool("keepAbove");
  return {
    resourceClass,
    caption,
    uuid: id,
    pid,
    desks: desks.slice(),
    frame: { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) },
    maxH,
    maxV,
    fullscreen,
    keepAbove,
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
// State-target, resize, and rapid-burst chord keycodes (Linux
// input-event-codes.h UAPI, stable ABI; verified against local
// linux-headers: KEY_LEFTMETA 125, KEY_LEFTSHIFT 42, KEY_LEFTALT 56,
// KEY_G 34, KEY_M 50, KEY_F11 87, KEY_LEFT 105, KEY_RIGHT 106).
// Sequences mirror the registered defaults in
// kwin/src/world-shortcuts.ts: Meta+M maximize, Meta+F11 fullscreen,
// Meta+G float, Meta+Shift+G sticky; grow-towards-direction is
// Meta+Alt+Arrow (world-shortcuts.ts:205); directional focus is
// Meta+Arrow and directional move is Meta+Shift+Arrow. Press in order,
// release in reverse.
export const STATE_CHORDS = {
  maximize: [125, 50],
  fullscreen: [125, 87],
  float: [125, 34],
  sticky: [125, 42, 34],
  resizeLeft: [125, 56, 105],
  resizeRight: [125, 56, 106],
  focusLeft: [125, 105],
  focusRight: [125, 106],
  moveLeft: [125, 42, 105],
  moveRight: [125, 42, 106],
};
// One keyboard resize press in the allocation coordinate space (the first
// press of a series; crates/tiler-core/src/cosmic_v1.rs:233).
export const RESIZE_STEP_PX = 12;
export function ydotoolKeyCmd(sockPath, chord) {
  if (typeof sockPath !== "string" || !sockPath || sockPath[0] !== "/") {
    throw new ScenarioError("input socket must be absolute");
  }
  const codes = STATE_CHORDS[chord];
  if (!codes) { throw new ScenarioError("unknown chord '" + String(chord) + "'"); }
  const down = codes.map((c) => c + ":1");
  const up = codes.slice().reverse().map((c) => c + ":0");
  return ["ydotool", "key"].concat(down, up);
}
// One ydotool invocation carrying a whole burst of chords back to back:
// the per-chord press/release events concatenated in order (same builder
// form as ydotoolKeyCmd). Never empty: an empty burst is a caller bug.
export function ydotoolBurstCmd(sockPath, names) {
  if (typeof sockPath !== "string" || !sockPath || sockPath[0] !== "/") {
    throw new ScenarioError("input socket must be absolute");
  }
  if (!Array.isArray(names) || !names.length) { throw new ScenarioError("burst needs at least one chord"); }
  const events = [];
  for (const name of names) {
    const codes = STATE_CHORDS[name];
    if (!codes) { throw new ScenarioError("unknown chord '" + String(name) + "'"); }
    for (const c of codes) { events.push(c + ":1"); }
    for (let i = codes.length - 1; i >= 0; i--) { events.push(codes[i] + ":0"); }
  }
  return ["ydotool", "key"].concat(events);
}
// Rapid-burst sequence derivation (deterministic, pure): from the OBSERVED
// admission order (leftFixture is the caller id whose admitted frame has the
// smaller x) and a fixed-constant LCG (Numerical Recipes multiplier and
// increment, date-pinned seed; bit 16 selects the kind), generate exactly 60
// chords. Model of a two-tile row: state = (order [left,right], focused
// position). Each step picks kind focus or move from the PRNG and the
// direction FORCED by the focused position, so every chord has an in-row
// target and none can be an edge/no-op or cross to another output (focused
// on left -> right; focused on right -> left). Model effect: focus flips the
// focused position; move swaps the two fixtures' slots and the moved
// (focused) fixture keeps focus, i.e. position flips.
export const RAPID_BURST_LEN = 60;
export function rapidBurstSequence(leftFixture) {
  if (leftFixture !== "a" && leftFixture !== "b") {
    throw new ScenarioError("rapid-burst sequence needs the observed left fixture ('a' or 'b')");
  }
  const other = leftFixture === "a" ? "b" : "a";
  let order = [leftFixture, other];
  let focusedPos = 0; // the left fixture is native-activated first, so focused
  const chords = [];
  const labels = [];
  let st = 20261019 >>> 0;
  for (let i = 0; i < RAPID_BURST_LEN; i++) {
    st = (Math.imul(1664525, st) + 1013904223) >>> 0;
    const kind = ((st >>> 16) & 1) === 0 ? "focus" : "move";
    const dir = focusedPos === 0 ? "right" : "left";
    const name = (kind === "focus" ? "focus" : "move") + (dir === "right" ? "Right" : "Left");
    chords.push(name);
    labels.push(kind + "-" + dir);
    if (kind === "move") { order = [order[1], order[0]]; }
    focusedPos = 1 - focusedPos;
  }
  return {
    chords, labels,
    expected: { leftFixture: order[0], rightFixture: order[1], focusedFixture: order[focusedPos] },
  };
}
// KWin-side failure/loss lines that must never appear during the burst
// (world-entry.ts command/queue/take/request outcomes, plus the
// shortcut-layer and scope-ownership rejections). The success line
// `omnitiler:world:command op=... label=...` never matches: it carries a
// space after `command`, every failure token carries `-...` there instead.
export const RAPID_BURST_FAILURE_RE =
  /omnitiler:world:(command-failed|command-dropped|command-refused|command-busy|queue-rejected|queue-stale|queue-timeout|queue-transport|queue-dropped|queue-malformed|queue-oversize|take-stale|take-duplicate|take-malformed|request-oversize|shortcut-failed|scope-invalid)/;
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

  const scenarioName = o.scenario === undefined ? "tiling-reflow" : o.scenario;
  if (scenarioName !== "tiling-reflow" && scenarioName !== "state-targets" && scenarioName !== "resize-refusal" && scenarioName !== "rapid-burst") {
    throw new ScenarioError("unknown scenario '" + String(scenarioName) + "'");
  }
  if ((scenarioName === "state-targets" || scenarioName === "resize-refusal" || scenarioName === "rapid-burst") && !withInput) {
    throw new ScenarioError(scenarioName + " requires --input");
  }

  const S = {
    events: [], calls: [], checks: [], cleanups: [],
    receipts: {}, counts: {}, secrets: [],
    category: "NOT-RUN", divergence: null, scenario: "init", scenarioRan: null,
    kwin: null, roster: null, original: null, appended: null,
    fixture: null, cargo: null, ydoc: null, devUp: false,
    joins: null, planner: null, cleanupFailed: [], teardownIssues: [],
  };
  // Join results, shared by the scenario closures below (which are defined
  // outside the try block) and filled in by the inline join step.
  const joins = [];
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

  // Shared lifecycle pieces used by both scenarios: re-derive the planner
  // pid/script-id receipts exactly as the initial dev-on does, run the
  // readiness gate against a given pre-start cursor, and run the shared
  // A/B admission check (frames equal published allocations, disjoint,
  // inside the output). label/prefix name the scenario in failures/checks.
  const derivePlannerReceipts = async () => {
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
    // Both planner receipts survive the restart: planner stays the first
    // start, plannerRestart is the second (post-restart) start.
    if (!S.receipts.planner) {
      S.receipts.planner = { pid: pPid, tick: pTick, scriptId: receipt.script_id };
    } else {
      S.receipts.plannerRestart = { pid: pPid, tick: pTick, scriptId: receipt.script_id };
    }
    const st2 = await mustExec(devStatusCmd(), "dev-status after dev-on");
    if (parseDevStatusMode(st2.stdout) !== "UP") { fail("dev mode is not UP after dev-on"); }
  };

  const runReadinessGate = async (cursor, countsKey, missingMsg) => {
    let gate = null;
    const gateEnd = io.now() + T.readiness;
    for (;;) {
      const j = await mustExec(["journalctl", "--user", "--after-cursor", cursor, "-o", "json", "_PID=" + S.kwin.pid], "readiness journal");
      gate = parseReadiness(redact(j.stdout));
      if (readinessPass(gate)) { break; }
      if (io.now() >= gateEnd) { break; }
      await io.sleep(T.readinessPoll);
    }
    S.counts[countsKey] = gate ? {
      ready: gate.ready ? 1 : 0, discovered: gate.discovered ? 1 : 0,
      dispatch: gate.dispatch ? 1 : 0, applied: gate.applied ? 1 : 0,
    } : null;
    if (!gate || !readinessPass(gate)) { fail(missingMsg + " within the bounded window: stop, no restart"); }
  };

  const pollAdmission = async (label, prefix) => {
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
          fail(label + ": " + first + (last !== first ? " (last: " + last + ")" : ""));
        }
        await io.sleep(T.poll);
      }
    })();
    if (admit.sameOut !== true) { fail(label + ": A/B on different outputs here"); }
    S.receipts.output = {
      name: admit.outName,
      scale: (typeof admit.outScale === "number" && isFinite(admit.outScale) && admit.outScale > 0) ? admit.outScale : null,
      rect: admit.out,
      raw: { a: admit.rawA, b: admit.rawB },
    };
    check(prefix + "-a-frame-equals-alloc", frameEqual(admit.fA, admit.expA), admit.expA, admit.fA);
    check(prefix + "-b-frame-equals-alloc", frameEqual(admit.fB, admit.expB), admit.expB, admit.fB);
    check(prefix + "-disjoint", !rectsOverlap(admit.fA, admit.fB), "disjoint", "disjoint");
    check(prefix + "-inside-output", frameInside(admit.fA, admit.out) && frameInside(admit.fB, admit.out), admit.out, admit.out);
    check(prefix + "-same-ws", admit.wsA === admit.wsB, admit.wsA, admit.wsB);
    return admit;
  };

  // Planner log path (read-only, through the existing exec boundary, never a
  // new io primitive). The Planner starts with stdout+stderr merged into one
  // regular file; resolve it once via the /proc fd of the receipted planner
  // pid. The resolved path must be absolute and match the production shape,
  // else fail closed. Only `planner-log-resolved: true` is recorded, never
  // the path itself.
  const PLANNER_LOG_RE = /^\/tmp\/omnitiler-planner-dev\.[A-Za-z0-9]+\.log$/;
  const resolvePlannerLogPath = async () => {
    const r = await mustExec(["readlink", "/proc/" + S.planner.pid + "/fd/2"], "planner log resolve");
    const p = String(r.stdout).split("\n").map((l) => l.trim()).filter(Boolean).pop() || "";
    if (!PLANNER_LOG_RE.test(p)) { fail("planner log unresolvable (need an absolute /tmp/omnitiler-planner-dev.*.log path)"); }
    // No secret, but transcripts stay generic: later stat/tail argv carrying
    // the path are recorded redacted, and receipts keep only the boolean.
    S.secrets.push(p);
    S.receipts["planner-log-resolved"] = true;
    return p;
  };
  const plannerLogOffset = async (logPath) => {
    const r = await mustExec(["stat", "-c", "%s", logPath], "planner log size");
    const n = Number(String(r.stdout).trim());
    if (!Number.isInteger(n) || n < 0) { fail("planner log size unreadable (" + String(r.stdout).slice(0, 40) + ")"); }
    return n;
  };
  const plannerLogRefusalsSince = async (logPath, offset) => {
    const r = await mustExec(["tail", "-c", "+" + (offset + 1), logPath], "planner log tail");
    return String(r.stdout).split("\n").filter((l) => l.indexOf("omnitiler:resize-refused") >= 0);
  };

  // Shared close-B + reflow oracle, used by tiling-reflow and resize-refusal.
  // `before` is the pre-close A frame the reflowed allocation must grow past.
  const closeFixtureB = async () => {
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
  };
  const pollReflowAfterClose = async (before) => {
    const bare = (u) => normalizeNativeId(u);
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
    return reflow;
  };

  // Ordinary two-fixture tiling and close/reflow over the shared context.
  const scenarioTilingReflow = async () => {
    S.scenario = "admission";
    event("scenario-admission");
    const admit = await pollAdmission("admission", "admit");
    S.receipts.presentationRev = admit.proj.rev;
    event("scenario-close-b");

    S.scenario = "reflow";
    await closeFixtureB();
    await pollReflowAfterClose(admit.fA);
    S.scenario = "done";
    S.category = "PASS";
    event("scenario-done");
  };

  // State-targets chord journey over the shared context: maximize /
  // fullscreen / float / sticky toggles driven by injected ydotool chords
  // with exact native activation of A before each chord, plus one orderly
  // production stop/start. Native oracles come from getWindowInfo (wrapped
  // strict values); presentation oracles from ReadWorldPresentation rows.
  // Unknown (null/missing) is PENDING, never PASS; a product-behaviour
  // mismatch is a FAIL naming the first divergence.
  const scenarioStateTargets = async () => {
    S.scenario = "state-targets";
    event("scenario-state-targets");
    const bare = (u) => normalizeNativeId(u);
    const uuidA = joins[0].uuid, uuidB = joins[1].uuid;
    const admit = await pollAdmission("state-admit", "state-admit");
    const outRect = admit.out;
    const baseA = admit.expA, baseB = admit.expB;
    S.receipts.stateBase = { a: baseA, b: baseB };
    const bad = (note) => ({ note });
    const good = (expected, actual) => ({ ok: true, expected, actual });
    const prow = (s, uuid) => s.proj.windows.filter((w) => w.win === bare(uuid))[0] || null;
    const frameStr = (f) => (f ? "[" + f.x + "," + f.y + "," + f.w + "," + f.h + "]" : "null");
    // Join/home re-check after every step: same UUIDs, and A keeps exactly
    // the appended workspace as its home except while sticky (empty desktops
    // list means all-desktop: setDesktops({}), never the roster).
    const joinHome = (s, sticky) => {
      if (s.infoA.uuid !== uuidA || s.infoB.uuid !== uuidB) {
        return "fixture join changed (UUIDs differ)";
      }
      if (sticky) {
        if (!Array.isArray(s.infoA.desks) || s.infoA.desks.length !== 0) {
          return "a not all-desktop while sticky (desks " + JSON.stringify(s.infoA.desks) + ")";
        }
      } else if (s.infoA.desks.length !== 1 || s.infoA.desks[0] !== S.appended) {
        return "a home is not exactly the appended workspace (desks " + JSON.stringify(s.infoA.desks) + ")";
      }
      return null;
    };
    const readAll = async () => {
      const proj = await readPresentation();
      const bareA = normalizeNativeId(uuidA), bareB = normalizeNativeId(uuidB);
      const set = proj.windows.map((w) => w.win).sort().join(",");
      if (proj.windows.length !== 2 || set !== [bareA, bareB].sort().join(",")) {
        return { fail: "presentation window set is not exactly {A,B}: [" + proj.windows.map((w) => w.win).sort().join(",") + "]" };
      }
      const nat = await helperOp({ op: "read" });
      const row = nat.filter((r) => Array.isArray(r.owned))[0];
      if (!row) { return { fail: "native read produced no roster" }; }
      const infoA = await windowInfo(uuidA);
      const infoB = await windowInfo(uuidB);
      return { proj, row, infoA, infoB };
    };
    const activateA = async (stepKey) => {
      mutOnce("activate-" + stepKey);
      await helperOp({ op: "activate", uuid: uuidA }, { mutate: true });
      const end = io.now() + T.admit;
      for (;;) {
        const ar = await helperOp({ op: "read" });
        const arow = ar.filter((r) => Array.isArray(r.owned))[0];
        const observed = arow ? normalizeNativeId(arow.active) : null;
        if (arow && observed === bare(uuidA)) { return; }
        if (io.now() >= end) { break; }
        await io.sleep(T.poll);
      }
      fail("state-" + stepKey + ": activation of a unverified within the bounded wait");
    };
    const injectChord = async (stepKey, chord) => {
      mutOnce("chord-" + stepKey);
      const argv = ydotoolKeyCmd(S.ydoc.sock, chord);
      S.calls.push(redact(argv.map(String).join(" ")));
      const r = await io.exec(argv, { timeoutMs: T.exec, env: Object.assign({}, env, { YDOTOOL_SOCKET: S.ydoc.sock }), cwd: root });
      if (!r || typeof r.status !== "number") { fail("bad exec result"); }
      if (r.status !== 0) { fail("state-" + stepKey + ": chord '" + chord + "' failed (status " + r.status + ")"); }
    };
    const pollStep = async (name, verify) => {
      const end = io.now() + T.stateStep;
      let first = "no read yet";
      let last = "no read yet";
      const note = (m) => { if (first === "no read yet") { first = m; } last = m; };
      for (;;) {
        let v = null;
        try {
          v = await verify(name);
        } catch (e) {
          const msg = e && e.message ? e.message : String(e);
          if (e instanceof ScenarioError && msg.indexOf("getWindowInfo") >= 0) {
            note("native state unknown (" + msg.slice(0, 120) + ")");
          } else { throw e; }
        }
        if (v && v.ok === true) {
          check("state-" + name, true, v.expected, v.actual);
          return;
        }
        if (v && v.note) { note(v.note); }
        if (io.now() >= end) {
          if (last.indexOf("unknown") >= 0 || last.indexOf("PENDING") >= 0) { S.category = "PENDING"; }
          fail("state-" + name + ": " + first + (last !== first ? " (last: " + last + ")" : ""));
        }
        await io.sleep(T.poll);
      }
    };
    const aAllocFrame = (s) => {
      const pA = prow(s, uuidA);
      if (!pA) { return { fail: "presentation lost a" }; }
      if (pA.alloc === null) { return { fail: "a published allocation unknown (null never matches)" }; }
      if (s.infoA.frame === null) { return { fail: "a native frame unknown (null never matches)" }; }
      if (!frameEqual(s.infoA.frame, frameOf(pA.alloc))) {
        return { fail: "a native frame != published allocation (" + frameDiff(s.infoA.frame, frameOf(pA.alloc)).join(",") + "): native " + frameStr(s.infoA.frame) + " != alloc " + JSON.stringify(pA.alloc) };
      }
      return { alloc: pA.alloc };
    };
    const bAllocFrame = (s) => {
      const pB = prow(s, uuidB);
      if (!pB) { return { fail: "presentation lost b" }; }
      if (pB.alloc === null) { return { fail: "b published allocation unknown (null never matches)" }; }
      if (s.infoB.frame === null) { return { fail: "b native frame unknown (null never matches)" }; }
      if (!frameEqual(s.infoB.frame, frameOf(pB.alloc))) {
        return { fail: "b native frame != published allocation (" + frameDiff(s.infoB.frame, frameOf(pB.alloc)).join(",") + "): native " + frameStr(s.infoB.frame) + " != alloc " + JSON.stringify(pB.alloc) };
      }
      return { alloc: pB.alloc };
    };
    const flagStr = (v) => (v === null ? "unknown" : String(v));
    const verifyMaxOn = async () => {
      const s = await readAll();
      if (s.fail) { return bad(s.fail); }
      const jh = joinHome(s, false);
      if (jh) { return bad(jh); }
      if (s.infoA.maxH === null || s.infoA.maxV === null) {
        return bad("a maximize unknown (maxH=" + flagStr(s.infoA.maxH) + " maxV=" + flagStr(s.infoA.maxV) + ")");
      }
      if (s.infoA.maxH !== true || s.infoA.maxV !== true) {
        return bad("a maximize axes not both true (maxH=" + s.infoA.maxH + " maxV=" + s.infoA.maxV + ")");
      }
      if (s.infoA.fullscreen === null) { return bad("a fullscreen unknown (null never matches)"); }
      if (s.infoA.fullscreen !== false) {
        return bad("a fullscreen not false while maximizing (fullscreen=" + s.infoA.fullscreen + ")");
      }
      if (s.infoA.frame === null) { return bad("a native frame unknown (null never matches)"); }
      if (!frameInside(s.infoA.frame, outRect)) {
        return bad("a frame not inside its output (frame " + frameStr(s.infoA.frame) + ")");
      }
      const bf = bAllocFrame(s);
      if (bf.fail) { return bad(bf.fail); }
      return good("maxH=true maxV=true fs=false a-inside b==alloc",
        "maxH=" + s.infoA.maxH + " maxV=" + s.infoA.maxV + " fs=" + s.infoA.fullscreen);
    };
    const verifyFsOn = async () => {
      const s = await readAll();
      if (s.fail) { return bad(s.fail); }
      const jh = joinHome(s, false);
      if (jh) { return bad(jh); }
      if (s.infoA.maxH === null || s.infoA.maxV === null) {
        return bad("a maximize unknown (maxH=" + flagStr(s.infoA.maxH) + " maxV=" + flagStr(s.infoA.maxV) + ")");
      }
      if (s.infoA.maxH !== true || s.infoA.maxV !== true) {
        return bad("a maximize axes not both true under fullscreen (maxH=" + s.infoA.maxH + " maxV=" + s.infoA.maxV + ")");
      }
      if (s.infoA.fullscreen === null) { return bad("a fullscreen unknown (null never matches)"); }
      if (s.infoA.fullscreen !== true) {
        return bad("a fullscreen not true (fullscreen=" + s.infoA.fullscreen + ")");
      }
      if (s.infoA.frame === null) { return bad("a native frame unknown (null never matches)"); }
      if (!frameEqual(s.infoA.frame, outRect)) {
        return bad("a frame != output rectangle (" + frameDiff(s.infoA.frame, outRect).join(",") + "): native " + frameStr(s.infoA.frame) + " != output " + frameStr(outRect));
      }
      return good("maxH=true maxV=true fs=true a==output", "maxH=" + s.infoA.maxH + " fs=" + s.infoA.fullscreen);
    };
    const verifyFsOff = async () => {
      const s = await readAll();
      if (s.fail) { return bad(s.fail); }
      const jh = joinHome(s, false);
      if (jh) { return bad(jh); }
      if (s.infoA.fullscreen === null) { return bad("a fullscreen unknown (null never matches)"); }
      if (s.infoA.fullscreen !== false) {
        return bad("a fullscreen not false after exit (fullscreen=" + s.infoA.fullscreen + ")");
      }
      if (s.infoA.maxH === null || s.infoA.maxV === null) {
        return bad("a maximize unknown (maxH=" + flagStr(s.infoA.maxH) + " maxV=" + flagStr(s.infoA.maxV) + ")");
      }
      if (s.infoA.maxH !== true || s.infoA.maxV !== true) {
        return bad("a maximize overlay not retained after fullscreen exit (maxH=" + s.infoA.maxH + " maxV=" + s.infoA.maxV + ")");
      }
      if (s.infoA.frame === null) { return bad("a native frame unknown (null never matches)"); }
      if (!frameInside(s.infoA.frame, outRect)) {
        return bad("a frame not inside its output after fullscreen exit (frame " + frameStr(s.infoA.frame) + ")");
      }
      const bf = bAllocFrame(s);
      if (bf.fail) { return bad(bf.fail); }
      return good("fs=false maxH=true maxV=true a-inside b==alloc", "fs=" + s.infoA.fullscreen);
    };
    const verifyMaxOff = async () => {
      const s = await readAll();
      if (s.fail) { return bad(s.fail); }
      const jh = joinHome(s, false);
      if (jh) { return bad(jh); }
      if (s.infoA.maxH === null || s.infoA.maxV === null) {
        return bad("a maximize unknown (maxH=" + flagStr(s.infoA.maxH) + " maxV=" + flagStr(s.infoA.maxV) + ")");
      }
      if (s.infoA.maxH !== false || s.infoA.maxV !== false) {
        return bad("a maximize axes not both false (maxH=" + s.infoA.maxH + " maxV=" + s.infoA.maxV + ")");
      }
      if (s.infoA.fullscreen === null) { return bad("a fullscreen unknown (null never matches)"); }
      if (s.infoA.fullscreen !== false) {
        return bad("a fullscreen not false after maximize exit (fullscreen=" + s.infoA.fullscreen + ")");
      }
      if (s.infoA.frame === null) { return bad("a native frame unknown (null never matches)"); }
      if (!frameEqual(s.infoA.frame, baseA)) {
        return bad("a frame != base allocation (" + frameDiff(s.infoA.frame, baseA).join(",") + "): native " + frameStr(s.infoA.frame) + " != base " + frameStr(baseA));
      }
      const bf = bAllocFrame(s);
      if (bf.fail) { return bad(bf.fail); }
      return good("maxH=false maxV=false fs=false a==base b==alloc", "a=" + frameStr(s.infoA.frame));
    };
    const verifyFloatOn = async () => {
      const s = await readAll();
      if (s.fail) { return bad(s.fail); }
      const jh = joinHome(s, false);
      if (jh) { return bad(jh); }
      const pA = prow(s, uuidA);
      if (!pA) { return bad("presentation lost a"); }
      if (pA.floating === null) { return bad("a floating unknown (null never matches)"); }
      if (pA.floating !== true) { return bad("a presentation floating not true (floating=" + pA.floating + ")"); }
      if (s.infoA.keepAbove === null) { return bad("a keepAbove unknown (null never matches)"); }
      if (s.infoA.keepAbove !== true) { return bad("a keepAbove not true while floating (keepAbove=" + s.infoA.keepAbove + ")"); }
      const bf = bAllocFrame(s);
      if (bf.fail) { return bad(bf.fail); }
      if (!(frameArea(frameOf(bf.alloc)) > frameArea(baseB))) {
        return bad("b published allocation not larger than base while floating (area " + frameArea(frameOf(bf.alloc)) + " <= base " + frameArea(baseB) + ")");
      }
      return good("floating=true above=true b>base b==alloc", "floating=" + pA.floating + " above=" + s.infoA.keepAbove);
    };
    const verifyOrdinaryBase = async (step) => {
      const s = await readAll();
      if (s.fail) { return bad(s.fail); }
      const jh = joinHome(s, false);
      if (jh) { return bad(jh); }
      const pA = prow(s, uuidA);
      if (!pA) { return bad("presentation lost a"); }
      if (pA.floating === null) { return bad("a floating unknown (null never matches)"); }
      if (pA.floating !== false) { return bad("a presentation floating not false (floating=" + pA.floating + ")"); }
      if (s.infoA.keepAbove === null) { return bad("a keepAbove unknown (null never matches)"); }
      if (s.infoA.keepAbove !== false) { return bad("a keepAbove not false (keepAbove=" + s.infoA.keepAbove + ")"); }
      const af = aAllocFrame(s);
      if (af.fail) { return bad(af.fail); }
      const bf = bAllocFrame(s);
      if (bf.fail) { return bad(bf.fail); }
      // Leaving floating/sticky is fresh admission: A and B hold the two base
      // allocations in either order, never the same one twice.
      if (s.infoA.frame === null) { return bad("a native frame unknown (null never matches)"); }
      if (s.infoB.frame === null) { return bad("b native frame unknown (null never matches)"); }
      const aIsA = frameEqual(s.infoA.frame, baseA);
      const aIsB = frameEqual(s.infoA.frame, baseB);
      const bIsA = frameEqual(s.infoB.frame, baseA);
      const bIsB = frameEqual(s.infoB.frame, baseB);
      if (!aIsA && !aIsB) {
        return bad("a native frame equals neither base allocation (" + frameDiff(s.infoA.frame, baseA).join(",") + "): native " + frameStr(s.infoA.frame) + " != base a " + frameStr(baseA) + " base b " + frameStr(baseB));
      }
      if (!bIsA && !bIsB) {
        return bad("b native frame equals neither base allocation (" + frameDiff(s.infoB.frame, baseB).join(",") + "): native " + frameStr(s.infoB.frame) + " != base a " + frameStr(baseA) + " base b " + frameStr(baseB));
      }
      if ((aIsA && bIsA) || (aIsB && bIsB)) {
        return bad("a and b occupy the same base allocation (a " + frameStr(s.infoA.frame) + " b " + frameStr(s.infoB.frame) + " base a " + frameStr(baseA) + " base b " + frameStr(baseB) + ")");
      }
      const aSlot = aIsA ? "a-base" : "b-base";
      if (!Array.isArray(S.receipts.stateSlotOrder)) { S.receipts.stateSlotOrder = []; }
      S.receipts.stateSlotOrder.push({ step: step, aSlot: aSlot });
      return good("floating=false above=false {a,b}=={base a, base b} (either order) a/b==alloc", "a=" + frameStr(s.infoA.frame) + " b=" + frameStr(s.infoB.frame));
    };
    const verifyStickyOn = async () => {
      const s = await readAll();
      if (s.fail) { return bad(s.fail); }
      const jh = joinHome(s, true);
      if (jh) { return bad(jh); }
      const pA = prow(s, uuidA);
      if (!pA) { return bad("presentation lost a"); }
      if (pA.floating === null) { return bad("a floating unknown (null never matches)"); }
      if (pA.floating !== true) { return bad("a presentation floating not true while sticky (floating=" + pA.floating + ")"); }
      if (s.infoA.keepAbove === null) { return bad("a keepAbove unknown (null never matches)"); }
      if (s.infoA.keepAbove !== true) { return bad("a keepAbove not true while sticky (keepAbove=" + s.infoA.keepAbove + ")"); }
      const bf = bAllocFrame(s);
      if (bf.fail) { return bad(bf.fail); }
      if (!(frameArea(frameOf(bf.alloc)) > frameArea(baseB))) {
        return bad("b published allocation not larger than base while sticky (area " + frameArea(frameOf(bf.alloc)) + " <= base " + frameArea(baseB) + ")");
      }
      return good("all-desktop above=true floating=true b>base b==alloc", "desks=" + s.infoA.desks.length);
    };
    const chordStep = async (name, chord, verify) => {
      event("state-" + name);
      await activateA(name);
      await injectChord(name, chord);
      await pollStep(name, verify);
    };
    await chordStep("a-maximize-on", "maximize", verifyMaxOn);
    await chordStep("b-fullscreen-on", "fullscreen", verifyFsOn);
    await chordStep("c-fullscreen-off", "fullscreen", verifyFsOff);
    await chordStep("d-maximize-off", "maximize", verifyMaxOff);
    await chordStep("e-float-on", "float", verifyFloatOn);
    await chordStep("f-float-off", "float", verifyOrdinaryBase);
    await chordStep("g-sticky-on", "sticky", verifyStickyOn);
    await chordStep("h-sticky-off", "sticky", verifyOrdinaryBase);
    await chordStep("i-sticky-on", "sticky", verifyStickyOn);
    // One orderly production stop/start. The armed dev-off cleanup still
    // ends DOWN even if this restart faults between its own dev-off/dev-on.
    event("state-restart");
    const cursor1 = await showCursor("pre-restart journal cursor");
    mutOnce("state-restart-dev-off");
    const off = await exec(devOffCmd(), T.exec);
    if (off.status !== 0) { fail("state restart dev-off failed (status " + off.status + ")"); }
    S.devUp = false;
    mutOnce("state-restart-dev-on");
    const on = await exec(devOnCmd(), T.devOn);
    if (on.status !== 0) { fail("state restart dev-on failed; no retry, no relaunch"); }
    S.devUp = true;
    await derivePlannerReceipts();
    S.counts.restart = 1;
    event("readiness-restart");
    await runReadinessGate(cursor1, "readinessRestart", "restart readiness gate missing");
    event("state-i-restart-restored");
    await pollStep("i-restart-restored", verifyStickyOn);
    await chordStep("i-sticky-off", "sticky", verifyOrdinaryBase);
    S.scenario = "done";
    S.category = "PASS";
    event("scenario-done");
  };

  // Resize-refusal journey over the shared context: A left, B right after
  // admission. Step 1 grows B toward the shared boundary (Meta+Alt+Left) and
  // expects the exact one-press delta with no refusal line; step 2 grows B
  // toward the outer edge (Meta+Alt+Right) and expects exactly one
  // `omnitiler:resize-refused` diagnostic with unchanged frames; step 3
  // closes B exactly as tiling-reflow does, then grows the singleton A
  // toward its outer edge and expects exactly one refusal line with the
  // reflowed frame unchanged. Native oracles come from getWindowInfo (exact
  // frames, never tolerance); refusal oracles from the planner log.
  // Missing/unknown facts are PENDING, never PASS.
  const scenarioResizeRefusal = async () => {
    S.scenario = "resize-refusal";
    event("scenario-resize-refusal");
    const bare = (u) => normalizeNativeId(u);
    const uuidA = joins[0].uuid, uuidB = joins[1].uuid;
    const admit = await pollAdmission("resize-admit", "resize-admit");
    const outRect = admit.out;
    const baseA = admit.expA, baseB = admit.expB;
    S.receipts.resizeBase = { a: baseA, b: baseB };
    const logPath = await resolvePlannerLogPath();
    const bad = (note) => ({ note });
    const good = (expected, actual) => ({ ok: true, expected, actual });
    const prow = (s, uuid) => s.proj.windows.filter((w) => w.win === bare(uuid))[0] || null;
    const frameStr = (f) => (f ? "[" + f.x + "," + f.y + "," + f.w + "," + f.h + "]" : "null");
    const readAllPair = async () => {
      const proj = await readPresentation();
      const bareA = normalizeNativeId(uuidA), bareB = normalizeNativeId(uuidB);
      const set = proj.windows.map((w) => w.win).sort().join(",");
      if (proj.windows.length !== 2 || set !== [bareA, bareB].sort().join(",")) {
        return { fail: "presentation window set is not exactly {A,B}: [" + proj.windows.map((w) => w.win).sort().join(",") + "]" };
      }
      const nat = await helperOp({ op: "read" });
      const row = nat.filter((r) => Array.isArray(r.owned))[0];
      if (!row) { return { fail: "native read produced no roster" }; }
      const infoA = await windowInfo(uuidA);
      const infoB = await windowInfo(uuidB);
      return { proj, row, infoA, infoB };
    };
    const readAllSingle = async () => {
      const proj = await readPresentation();
      if (proj.windows.length !== 1 || proj.windows[0].win !== bare(uuidA)) {
        return { fail: "presentation window set is not exactly {A} after close: [" +
          proj.windows.map((w) => w.win).sort().join(",") + "]" };
      }
      const nat = await helperOp({ op: "read" });
      const row = nat.filter((r) => Array.isArray(r.owned))[0];
      if (!row) { return { fail: "native read produced no roster" }; }
      const infoA = await windowInfo(uuidA);
      return { proj, row, infoA };
    };
    const joinHomePair = (s) => {
      if (s.infoA.uuid !== uuidA || s.infoB.uuid !== uuidB) {
        return "fixture join changed (UUIDs differ)";
      }
      for (const pair of [["a", s.infoA], ["b", s.infoB]]) {
        if (pair[1].desks.length !== 1 || pair[1].desks[0] !== S.appended) {
          return "fixture '" + pair[0] + "' home is not exactly the appended workspace (desks " + JSON.stringify(pair[1].desks) + ")";
        }
      }
      return null;
    };
    const nativeAllocFrame = (s, uuid, tag) => {
      const p = prow(s, uuid);
      if (!p) { return { fail: "presentation lost " + tag }; }
      if (p.alloc === null) { return { fail: tag + " published allocation unknown (null never matches)" }; }
      const info = tag === "a" ? s.infoA : s.infoB;
      if (info.frame === null) { return { fail: tag + " native frame unknown (null never matches)" }; }
      if (!frameEqual(info.frame, frameOf(p.alloc))) {
        return { fail: tag + " native frame != published allocation (" + frameDiff(info.frame, frameOf(p.alloc)).join(",") + "): native " + frameStr(info.frame) + " != alloc " + JSON.stringify(p.alloc) };
      }
      return { alloc: p.alloc };
    };
    const activateNative = async (stepKey, uuid) => {
      mutOnce("activate-" + stepKey);
      await helperOp({ op: "activate", uuid }, { mutate: true });
      const end = io.now() + T.admit;
      for (;;) {
        const ar = await helperOp({ op: "read" });
        const arow = ar.filter((r) => Array.isArray(r.owned))[0];
        const observed = arow ? normalizeNativeId(arow.active) : null;
        if (arow && observed === bare(uuid)) { return; }
        if (io.now() >= end) { break; }
        await io.sleep(T.poll);
      }
      fail("resize-" + stepKey + ": activation unverified within the bounded wait");
    };
    const injectResizeChord = async (stepKey, chord) => {
      mutOnce("chord-" + stepKey);
      const argv = ydotoolKeyCmd(S.ydoc.sock, chord);
      S.calls.push(redact(argv.map(String).join(" ")));
      const r = await io.exec(argv, { timeoutMs: T.exec, env: Object.assign({}, env, { YDOTOOL_SOCKET: S.ydoc.sock }), cwd: root });
      if (!r || typeof r.status !== "number") { fail("bad exec result"); }
      if (r.status !== 0) { fail("resize-" + stepKey + ": chord '" + chord + "' failed (status " + r.status + ")"); }
    };
    const pollResizeStep = async (name, verify) => {
      const end = io.now() + T.stateStep;
      let first = "no read yet";
      let last = "no read yet";
      const note = (m) => { if (first === "no read yet") { first = m; } last = m; };
      for (;;) {
        let v = null;
        try {
          v = await verify(name);
        } catch (e) {
          const msg = e && e.message ? e.message : String(e);
          if (e instanceof ScenarioError && msg.indexOf("getWindowInfo") >= 0) {
            note("native state unknown (" + msg.slice(0, 120) + ")");
          } else { throw e; }
        }
        if (v && v.ok === true) {
          check("resize-" + name, true, v.expected, v.actual);
          return;
        }
        if (v && v.note) { note(v.note); }
        if (io.now() >= end) {
          if (last.indexOf("unknown") >= 0 || last.indexOf("PENDING") >= 0) { S.category = "PENDING"; }
          fail("resize-" + name + ": " + first + (last !== first ? " (last: " + last + ")" : ""));
        }
        await io.sleep(T.poll);
      }
    };
    const noteRefusalReceipt = (step, tok) => {
      const kept = {
        step, internal: tok.internal, wire: tok.wire, dir: tok.dir,
        mode: tok.mode, press: tok.press, rowsTotal: tok.rowsTotal, rowsShown: tok.rowsShown,
        subjectBound: true,
      };
      const rest = (S.receipts.resizeRefusals || []).filter((r) => r.step !== step);
      rest.push(kept);
      S.receipts.resizeRefusals = rest;
    };
    const checkRefusalTokens = (step, tok, wantDir, wantSubject, wantRowsTotal) => {
      const want = { internal: "unchanged", wire: "operation-refused", dir: wantDir, mode: "outwards" };
      for (const k of Object.keys(want)) {
        if (tok[k] !== want[k]) {
          return "refusal token mismatch at resize-" + step + ": " + k + "='" +
            (tok[k] === undefined ? "missing" : tok[k]) + "' (need '" + want[k] + "')";
        }
      }
      // The subject window id is asserted in memory only: the message names
      // the field but never prints the id.
      if (tok.subject !== wantSubject) {
        return "refusal subject mismatch at resize-" + step + " (need bound subject)";
      }
      if (tok.press !== "0") {
        return "refusal token mismatch at resize-" + step + ": press='" +
          (tok.press === undefined ? "missing" : tok.press) + "' (need '0')";
      }
      if (tok.rowsTotal !== wantRowsTotal) {
        return "refusal token mismatch at resize-" + step + ": rowsTotal='" +
          (tok.rowsTotal === undefined ? "missing" : tok.rowsTotal) + "' (need '" + wantRowsTotal + "')";
      }
      if (tok.rowsShown !== wantRowsTotal) {
        return "refusal token mismatch at resize-" + step + ": rowsShown='" +
          (tok.rowsShown === undefined ? "missing" : tok.rowsShown) + "' (need '" + wantRowsTotal + "')";
      }
      return null;
    };
    // Step 1: B grows toward the shared boundary by exactly one press.
    const expA1 = { x: baseA.x, y: baseA.y, w: baseA.w - RESIZE_STEP_PX, h: baseA.h };
    const expB1 = { x: baseB.x - RESIZE_STEP_PX, y: baseB.y, w: baseB.w + RESIZE_STEP_PX, h: baseB.h };
    let step1Frames = null;
    const verifyResizeStep1 = (logOff) => async () => {
      const s = await readAllPair();
      if (s.fail) { return bad(s.fail); }
      const jh = joinHomePair(s);
      if (jh) { return bad(jh); }
      const lines = await plannerLogRefusalsSince(logPath, logOff);
      if (lines.length) { return bad("unexpected planner refusal line(s) at resize-step1 (" + lines.length + ")"); }
      if (s.infoA.frame === null) { return bad("a native frame unknown (null never matches)"); }
      if (s.infoB.frame === null) { return bad("b native frame unknown (null never matches)"); }
      if (!frameEqual(s.infoA.frame, expA1)) {
        return bad("a frame != stepped allocation (" + frameDiff(s.infoA.frame, expA1).join(",") + "): native " + frameStr(s.infoA.frame) + " != stepped " + frameStr(expA1));
      }
      if (!frameEqual(s.infoB.frame, expB1)) {
        return bad("b frame != stepped allocation (" + frameDiff(s.infoB.frame, expB1).join(",") + "): native " + frameStr(s.infoB.frame) + " != stepped " + frameStr(expB1));
      }
      const af = nativeAllocFrame(s, uuidA, "a");
      if (af.fail) { return bad(af.fail); }
      const bf = nativeAllocFrame(s, uuidB, "b");
      if (bf.fail) { return bad(bf.fail); }
      if (!frameInside(s.infoA.frame, outRect) || !frameInside(s.infoB.frame, outRect)) {
        return bad("resized tile frame outside its output (a " + frameStr(s.infoA.frame) + " b " + frameStr(s.infoB.frame) + ")");
      }
      if (rectsOverlap(s.infoA.frame, s.infoB.frame)) { return bad("tiles A/B overlap after resize"); }
      step1Frames = { a: s.infoA.frame, b: s.infoB.frame };
      return good("a==base-12 b==base+12 a/b==alloc disjoint inside no-refusal",
        "a=" + frameStr(s.infoA.frame) + " b=" + frameStr(s.infoB.frame));
    };
    // Step 2: B grows toward the outer edge (no neighbour): refused, frames stay.
    const verifyResizeStep2 = (logOff) => async () => {
      const s = await readAllPair();
      if (s.fail) { return bad(s.fail); }
      const jh = joinHomePair(s);
      if (jh) { return bad(jh); }
      if (s.infoA.frame === null) { return bad("a native frame unknown (null never matches)"); }
      if (s.infoB.frame === null) { return bad("b native frame unknown (null never matches)"); }
      if (!frameEqual(s.infoA.frame, step1Frames.a)) {
        return bad("a frame moved on refused resize (" + frameDiff(s.infoA.frame, step1Frames.a).join(",") + "): native " + frameStr(s.infoA.frame) + " != step1 " + frameStr(step1Frames.a));
      }
      if (!frameEqual(s.infoB.frame, step1Frames.b)) {
        return bad("b frame moved on refused resize (" + frameDiff(s.infoB.frame, step1Frames.b).join(",") + "): native " + frameStr(s.infoB.frame) + " != step1 " + frameStr(step1Frames.b));
      }
      const af = nativeAllocFrame(s, uuidA, "a");
      if (af.fail) { return bad(af.fail); }
      const bf = nativeAllocFrame(s, uuidB, "b");
      if (bf.fail) { return bad(bf.fail); }
      const lines = await plannerLogRefusalsSince(logPath, logOff);
      if (lines.length !== 1) {
        return bad("expected exactly one resize refusal line since resize-step2, saw " + lines.length);
      }
      const tok = parseResizeRefusalTokens(lines[0]);
      const mismatch = checkRefusalTokens("step2", tok, "right", bare(uuidB), "2");
      if (mismatch) { return bad(mismatch); }
      noteRefusalReceipt("step2", tok);
      return good("refusal unchanged/operation-refused right/outwards frames==step1 a/b==alloc",
        "dir=" + tok.dir + " mode=" + tok.mode);
    };
    // Step 3: singleton A grows toward its outer edge: refused, frame stays.
    const verifyResizeStep3 = (logOff, reflow) => async () => {
      const s = await readAllSingle();
      if (s.fail) { return bad(s.fail); }
      if (s.infoA.uuid !== uuidA) { return bad("fixture join changed (UUIDs differ)"); }
      if (s.infoA.desks.length !== 1 || s.infoA.desks[0] !== S.appended) {
        return bad("a home is not exactly the appended workspace (desks " + JSON.stringify(s.infoA.desks) + ")");
      }
      const scopeOwned = s.row.owned.filter((w) => w.class === SCOPE_CLASS);
      if (scopeOwned.length !== 1 || scopeOwned[0].uuid !== uuidA) {
        return bad("native scope roster is not exactly {A} after close");
      }
      if (s.infoA.frame === null) { return bad("a native frame unknown (null never matches)"); }
      if (!frameEqual(s.infoA.frame, reflow.fA)) {
        return bad("a frame changed on refused singleton resize (" + frameDiff(s.infoA.frame, reflow.fA).join(",") + "): native " + frameStr(s.infoA.frame) + " != reflowed " + frameStr(reflow.fA));
      }
      const pA = s.proj.windows[0];
      if (pA.alloc === null) { return bad("a published allocation unknown (null never matches)"); }
      if (!frameEqual(s.infoA.frame, frameOf(pA.alloc))) {
        return bad("a native frame != published allocation (" + frameDiff(s.infoA.frame, frameOf(pA.alloc)).join(",") + "): native " + frameStr(s.infoA.frame) + " != alloc " + JSON.stringify(pA.alloc));
      }
      const lines = await plannerLogRefusalsSince(logPath, logOff);
      if (lines.length !== 1) {
        return bad("expected exactly one resize refusal line since resize-step3, saw " + lines.length);
      }
      const tok = parseResizeRefusalTokens(lines[0]);
      const mismatch = checkRefusalTokens("step3", tok, "left", bare(uuidA), "1");
      if (mismatch) { return bad(mismatch); }
      noteRefusalReceipt("step3", tok);
      return good("refusal unchanged/operation-refused left/outwards a==reflowed a==alloc",
        "dir=" + tok.dir + " mode=" + tok.mode);
    };
    event("resize-step1");
    await activateNative("resize-step1", uuidB);
    const off1 = await plannerLogOffset(logPath);
    await injectResizeChord("resize-step1", "resizeLeft");
    await pollResizeStep("step1", verifyResizeStep1(off1));
    S.receipts.resizeStep1 = step1Frames;
    event("resize-step2");
    await activateNative("resize-step2", uuidB);
    const off2 = await plannerLogOffset(logPath);
    await injectResizeChord("resize-step2", "resizeRight");
    await pollResizeStep("step2", verifyResizeStep2(off2));
    event("resize-close-b");
    await closeFixtureB();
    const reflow = await pollReflowAfterClose(step1Frames.a);
    event("resize-step3");
    await activateNative("resize-step3", uuidA);
    const off3 = await plannerLogOffset(logPath);
    await injectResizeChord("resize-step3", "resizeLeft");
    await pollResizeStep("step3", verifyResizeStep3(off3, reflow));
    S.scenario = "done";
    S.category = "PASS";
    event("scenario-done");
  };

  // Rapid-burst journey over the shared context: A and B tile side by side
  // after admission. The observed order (smaller admitted x is left) seeds a
  // deterministic 60-chord focus/move burst; the left fixture is
  // native-activated first so it is focused, then ONE ydotool invocation
  // carries all 60 chords back to back. Oracles: (1) the KWin journal since
  // the pre-burst cursor shows exactly the 60 `omnitiler:world:command`
  // lines with ops in the focus/move family and label= values in the exact
  // generated order (FIFO preserved, no drop/reorder/extra) and ZERO
  // failure/loss lines (absence of every failure outcome plus exact
  // count/order is the observable: no per-command READY line exists);
  // (2) the settled final state matches the model prediction on
  // T.burstSettleReads consecutive polls spanning T.burstSettleMin of
  // runner clock, with identical presentation rev/obs and journal labels
  // across the streak (exact frames, disjoint inside the output,
  // left/right slots, focus). A streak that never quiesces fails
  // "not settled"; terminal journal states fail immediately.
  // Missing/unknown facts are PENDING, never PASS; a mismatch FAILs naming
  // the field with predicted vs observed fixture letters (never ids).
  const scenarioRapidBurst = async () => {
    S.scenario = "rapid-burst";
    event("scenario-rapid-burst");
    const bare = (u) => normalizeNativeId(u);
    const uuidA = joins[0].uuid, uuidB = joins[1].uuid;
    const admit = await pollAdmission("rapid-admit", "rapid-admit");
    const outRect = admit.out;
    let leftLetter;
    if (admit.fA.x < admit.fB.x) { leftLetter = "a"; }
    else if (admit.fB.x < admit.fA.x) { leftLetter = "b"; }
    else { fail("rapid-burst: admitted frames share x (" + admit.fA.x + "); left slot ambiguous"); }
    const leftUuid = leftLetter === "a" ? uuidA : uuidB;
    const seq = rapidBurstSequence(leftLetter);
    const focusCount = seq.chords.filter((c) => c.indexOf("focus") === 0).length;
    const moveCount = seq.chords.length - focusCount;
    const bad = (note) => ({ note });
    const good = (expected, actual, detail) => ({ ok: true, expected, actual, detail: detail || null });
    const frameStr = (f) => (f ? "[" + f.x + "," + f.y + "," + f.w + "," + f.h + "]" : "null");
    const readAllBurst = async () => {
      const proj = await readPresentation();
      const bareA = normalizeNativeId(uuidA), bareB = normalizeNativeId(uuidB);
      const set = proj.windows.map((w) => w.win).sort().join(",");
      if (proj.windows.length !== 2 || set !== [bareA, bareB].sort().join(",")) {
        return { fail: "presentation window set is not exactly {A,B}: [" + proj.windows.map((w) => w.win).sort().join(",") + "]" };
      }
      const nat = await helperOp({ op: "read" });
      const row = nat.filter((r) => Array.isArray(r.owned))[0];
      if (!row) { return { fail: "native read produced no roster" }; }
      const infoA = await windowInfo(uuidA);
      const infoB = await windowInfo(uuidB);
      return { proj, row, infoA, infoB };
    };
    const readBurstJournal = async (cursor) => {
      const j = await mustExec(["journalctl", "--user", "--after-cursor", cursor, "-o", "json", "_PID=" + S.kwin.pid], "burst journal");
      const cmds = [];
      let failures = 0;
      for (const line of j.stdout.split("\n")) {
        if (!line.trim()) { continue; }
        let entry;
        try { entry = JSON.parse(line); } catch (e) { fail("rapid-burst: journal: unparsable -o json line"); }
        if (typeof entry.MESSAGE !== "string") { fail("rapid-burst: journal: MESSAGE missing"); }
        let msg = entry.MESSAGE;
        if (msg.indexOf("js:") === 0) { msg = msg.slice(3).replace(/^ /, ""); }
        if (RAPID_BURST_FAILURE_RE.test(msg)) { failures++; }
        if (msg.indexOf("omnitiler:world:command ") >= 0) {
          const opM = /op=([^\s]+)/.exec(msg);
          const lbM = /label=([^\s]+)/.exec(msg);
          cmds.push({ op: opM ? opM[1] : null, label: lbM ? lbM[1] : null });
        }
      }
      return { cmds, failures };
    };
    const verifyBurst = async (cursor) => {
      const terminal = (note) => ({ note, terminal: true });
      const bj = await readBurstJournal(cursor);
      if (bj.failures !== 0) {
        return terminal("journal shows " + bj.failures + " failure/loss line(s) (need zero)");
      }
      if (bj.cmds.length > seq.labels.length) {
        return terminal("journal shows " + bj.cmds.length + " command lines (need " + seq.labels.length + "; extra lines can never drain)");
      }
      if (bj.cmds.length !== seq.labels.length) {
        return bad("journal shows " + bj.cmds.length + " command lines (need " + seq.labels.length + ")");
      }
      for (let i = 0; i < seq.labels.length; i++) {
        const wantLabel = seq.labels[i];
        const wantOp = wantLabel.indexOf("focus-") === 0 ? "focus-directional" : "move-directional";
        if (bj.cmds[i].label !== wantLabel) {
          return terminal("journal label mismatch at index " + i + ": saw '" + bj.cmds[i].label + "' (need '" + wantLabel + "')");
        }
        if (bj.cmds[i].op !== wantOp) {
          return terminal("journal op mismatch at index " + i + ": saw '" + bj.cmds[i].op + "' (need '" + wantOp + "')");
        }
      }
      const s = await readAllBurst();
      if (s.fail) { return bad(s.fail); }
      if (s.infoA.uuid !== uuidA || s.infoB.uuid !== uuidB) {
        return bad("fixture join changed (UUIDs differ)");
      }
      for (const pair of [["a", s.infoA], ["b", s.infoB]]) {
        if (pair[1].desks.length !== 1 || pair[1].desks[0] !== S.appended) {
          return bad("fixture '" + pair[0] + "' home is not exactly the appended workspace (desks " + JSON.stringify(pair[1].desks) + ")");
        }
      }
      const pA = s.proj.windows.filter((w) => w.win === bare(uuidA))[0];
      const pB = s.proj.windows.filter((w) => w.win === bare(uuidB))[0];
      if (!pA || !pB) { return bad("presentation lost a fixture after the burst"); }
      if (pA.alloc === null) { return bad("a published allocation unknown (null never matches)"); }
      if (pB.alloc === null) { return bad("b published allocation unknown (null never matches)"); }
      if (s.infoA.frame === null) { return bad("a native frame unknown (null never matches)"); }
      if (s.infoB.frame === null) { return bad("b native frame unknown (null never matches)"); }
      if (!frameEqual(s.infoA.frame, frameOf(pA.alloc))) {
        return bad("a native frame != published allocation (" + frameDiff(s.infoA.frame, frameOf(pA.alloc)).join(",") + "): native " + frameStr(s.infoA.frame) + " != alloc " + JSON.stringify(pA.alloc));
      }
      if (!frameEqual(s.infoB.frame, frameOf(pB.alloc))) {
        return bad("b native frame != published allocation (" + frameDiff(s.infoB.frame, frameOf(pB.alloc)).join(",") + "): native " + frameStr(s.infoB.frame) + " != alloc " + JSON.stringify(pB.alloc));
      }
      if (rectsOverlap(s.infoA.frame, s.infoB.frame)) { return bad("tiles A/B overlap after the burst"); }
      if (!frameInside(s.infoA.frame, outRect) || !frameInside(s.infoB.frame, outRect)) {
        return bad("tile frame outside its output after the burst (a " + frameStr(s.infoA.frame) + " b " + frameStr(s.infoB.frame) + ")");
      }
      const allocA = frameOf(pA.alloc), allocB = frameOf(pB.alloc);
      let observedLeft = null;
      if (allocA.x < allocB.x) { observedLeft = "a"; }
      else if (allocB.x < allocA.x) { observedLeft = "b"; }
      if (observedLeft === null) { return bad("published allocations share x after the burst; left slot ambiguous"); }
      if (observedLeft !== seq.expected.leftFixture) {
        return bad("final left slot holds '" + observedLeft + "' (need '" + seq.expected.leftFixture + "')");
      }
      const observedRight = observedLeft === "a" ? "b" : "a";
      if (observedRight !== seq.expected.rightFixture) {
        return bad("final right slot holds '" + observedRight + "' (need '" + seq.expected.rightFixture + "')");
      }
      const activeObserved = s.row ? normalizeNativeId(s.row.active) : null;
      if (activeObserved === null) { return bad("natively active window unknown (null never matches)"); }
      let observedFocused = null;
      if (activeObserved === bare(uuidA)) { observedFocused = "a"; }
      else if (activeObserved === bare(uuidB)) { observedFocused = "b"; }
      if (observedFocused === null) { return bad("natively active window is neither A nor B after the burst"); }
      if (observedFocused !== seq.expected.focusedFixture) {
        return bad("final focused fixture is '" + observedFocused + "' (need '" + seq.expected.focusedFixture + "')");
      }
      return good("fifo-60 focus=" + focusCount + " move=" + moveCount +
        " order=" + seq.expected.leftFixture + seq.expected.rightFixture +
        " focused=" + seq.expected.focusedFixture,
        "journal=60 labels-match failures=0 order=" + observedLeft + observedRight + " focused=" + observedFocused,
        {
          labels: bj.cmds.map((c) => c.label),
          rev: s.proj.rev, obs: s.proj.obs,
          observedLeft, observedRight, observedFocused,
          frameA: frameStr(s.infoA.frame), frameB: frameStr(s.infoB.frame),
          allocA: frameStr(allocA), allocB: frameStr(allocB),
        });
    };
    // Quiescence: the journal writes land synchronously in the shortcut
    // handler, but native effects drain asynchronously through planner
    // round-trips, and intermediate applied states can equal the final
    // model. A single matching read proves nothing, so PASS requires the
    // settled-state predicate to hold on T.burstSettleReads consecutive
    // polls spanning at least T.burstSettleMin of runner clock, with the
    // presentation rev/obs and the journal label vector identical across
    // the streak. Any divergent read resets the streak; terminal journal
    // states (any failure line, more lines than the burst, or a mismatch
    // at the full count) fail immediately since journal lines are
    // append-only inside one cursor window.
    const pollBurst = async (cursor) => {
      const end = io.now() + T.stateStep;
      const NEED = T.burstSettleReads, MINWIN = T.burstSettleMin;
      let first = "no read yet";
      let last = "no read yet";
      let streak = 0, streakStart = 0;
      let streakLabels = null, streakRev = null, streakObs = null;
      const note = (m) => { if (first === "no read yet") { first = m; } last = m; };
      for (;;) {
        let v = null;
        try {
          v = await verifyBurst(cursor);
        } catch (e) {
          const msg = e && e.message ? e.message : String(e);
          if (e instanceof ScenarioError && msg.indexOf("getWindowInfo") >= 0) {
            note("native state unknown (" + msg.slice(0, 120) + ")");
          } else { throw e; }
        }
        const t = io.now();
        if (v && v.ok === true) {
          const d = v.detail;
          const sameJournal = streakLabels !== null && streakLabels.length === d.labels.length &&
            streakLabels.every((l, i) => l === d.labels[i]);
          if (streak === 0 || !sameJournal || d.rev !== streakRev || d.obs !== streakObs) {
            streak = 1;
            streakStart = t;
            streakLabels = d.labels.slice();
            streakRev = d.rev;
            streakObs = d.obs;
          } else {
            streak++;
          }
          if (streak >= NEED && (t - streakStart) >= MINWIN) {
            const got = d.labels;
            check("rapid-journal-fifo", true, seq.labels.join(","), got.join(","));
            check("rapid-final-a-frame-equals-alloc", true, "alloc " + d.allocA, "native " + d.frameA);
            check("rapid-final-b-frame-equals-alloc", true, "alloc " + d.allocB, "native " + d.frameB);
            check("rapid-final-disjoint", true, "disjoint", "disjoint a=" + d.frameA + " b=" + d.frameB);
            check("rapid-final-inside-output", true, outRect, "inside a=" + d.frameA + " b=" + d.frameB);
            check("rapid-final-order", true,
              seq.expected.leftFixture + "/" + seq.expected.rightFixture,
              d.observedLeft + "/" + d.observedRight);
            check("rapid-final-focus", true, seq.expected.focusedFixture, d.observedFocused);
            check("rapid-burst", true, v.expected, v.actual);
            return got.length;
          }
          if (t >= end) {
            fail("rapid-burst: not settled within the bounded wait (matched " + streak + "/" + NEED +
              " consecutive reads over " + (t - streakStart) + "ms of " + MINWIN + "ms" +
              (first === "no read yet" ? "; no divergent read" : "; first: " + first + (last !== first ? " (last: " + last + ")" : "")) + ")");
          }
        } else {
          if (v && v.terminal) {
            fail("rapid-burst: " + v.note);
          }
          if (v && v.note) { note(v.note); }
          streak = 0;
          streakLabels = null;
          streakRev = null;
          streakObs = null;
          if (t >= end) {
            if (last.indexOf("unknown") >= 0 || last.indexOf("PENDING") >= 0) { S.category = "PENDING"; }
            fail("rapid-burst: " + first + (last !== first ? " (last: " + last + ")" : ""));
          }
        }
        await io.sleep(T.poll);
      }
    };
    // Native-activate the observed left fixture first, so it is focused.
    event("rapid-burst");
    mutOnce("activate-rapid-left");
    await helperOp({ op: "activate", uuid: leftUuid }, { mutate: true });
    {
      const end = io.now() + T.admit;
      for (;;) {
        const ar = await helperOp({ op: "read" });
        const arow = ar.filter((r) => Array.isArray(r.owned))[0];
        const observed = arow ? normalizeNativeId(arow.active) : null;
        if (arow && observed === bare(leftUuid)) { break; }
        if (io.now() >= end) { break; }
        await io.sleep(T.poll);
      }
      const fin = await helperOp({ op: "read" });
      const finRow = fin.filter((r) => Array.isArray(r.owned))[0];
      if (!finRow || normalizeNativeId(finRow.active) !== bare(leftUuid)) {
        fail("rapid-burst: activation of the left fixture unverified within the bounded wait");
      }
    }
    // KWin journal cursor immediately before the single burst call.
    const cursor = await showCursor("rapid-burst journal cursor");
    mutOnce("burst-inject");
    {
      const argv = ydotoolBurstCmd(S.ydoc.sock, seq.chords);
      S.calls.push(redact(argv.map(String).join(" ")));
      const r = await io.exec(argv, { timeoutMs: T.exec, env: Object.assign({}, env, { YDOTOOL_SOCKET: S.ydoc.sock }), cwd: root });
      if (!r || typeof r.status !== "number") { fail("bad exec result"); }
      if (r.status !== 0) { fail("rapid-burst: burst inject failed (status " + r.status + ")"); }
    }
    const journalCommands = await pollBurst(cursor);
    S.receipts.rapidBurst = {
      length: seq.chords.length, focus: focusCount, move: moveCount,
      left: seq.expected.leftFixture, right: seq.expected.rightFixture,
      focused: seq.expected.focusedFixture,
      journalCommands, failureLinesFound: 0,
    };
    S.scenario = "done";
    S.category = "PASS";
    event("scenario-done");
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
        if (await io.exists(ysock)) {
          // Unknown real behaviour: whether ydotoold unlinks its socket on
          // SIGTERM. A socket the daemon left behind is removed here (only
          // this exact owned path); afterwards absence is still verified,
          // and only then is the runner count set.
          try { await io.rm(ysock); } catch (e) {
            fail("ydotoold socket rm failed (" + (e && e.message ? e.message : String(e)).slice(0, 120) + ")");
          }
          if (await io.exists(ysock)) { fail("ydotoold socket still present after runner rm"); }
          S.counts.ydotooldSocketUnlinkedByRunner = 1;
        }
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
    await derivePlannerReceipts();

    // 8. Readiness gate after the first start, against the pre-start cursor.
    // (state-targets restarts once more later; see state-restart.) No restart
    // No restart on missing lines. Only booleans/counts are persisted.
    event("readiness");
    await runReadinessGate(cursor0, "readiness", "readiness gate missing");

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

    // 12. Scenario dispatch over the shared context (S, helpers, joins).
    // Ordinary two-fixture tiling and close/reflow (default), the
    // state-targets chord journey, or the resize-refusal journey. Each
    // scenario function is defined above.
    if (scenarioName === "state-targets") {
      S.scenarioRan = "state-targets";
      await scenarioStateTargets();
    } else if (scenarioName === "resize-refusal") {
      S.scenarioRan = "resize-refusal";
      await scenarioResizeRefusal();
    } else if (scenarioName === "rapid-burst") {
      S.scenarioRan = "rapid-burst";
      await scenarioRapidBurst();
    } else {
      S.scenarioRan = "tiling-reflow";
      await scenarioTilingReflow();
    }
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
    rc, category: S.category, scenario: S.scenario, scenarioRan: S.scenarioRan, divergence: S.divergence,
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
  if (args.scenario === "state-targets" && !args.input) {
    console.error("kde-scenarios: --scenario state-targets requires --input");
    return 2;
  }
  if (args.scenario === "resize-refusal" && !args.input) {
    console.error("kde-scenarios: --scenario resize-refusal requires --input");
    return 2;
  }
  if (args.scenario === "rapid-burst" && !args.input) {
    console.error("kde-scenarios: --scenario rapid-burst requires --input");
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
  const res = await (o.runScenarioFn || runScenario)({ io, root, env: execEnv, evidenceDir, input: args.input, scenario: args.scenario, templatePath });
  // The scenario that ran must be the one requested: a mismatch fails the run.
  let rc = res.rc;
  let scenarioMismatch = false;
  if (res.scenarioRan !== null && res.scenarioRan !== args.scenario) {
    scenarioMismatch = true;
    rc = 1;
    console.error("kde-scenarios: requested scenario '" + args.scenario + "' but ran '" + res.scenarioRan + "'");
  }
  // Evidence: step results, named checks, receipts, counts. No nonces, no
  // foreign window metadata, no home/host names (relative names only).
  const evidence = {
    v: 1, category: res.category, requestedScenario: args.scenario, scenarioRan: res.scenarioRan,
    scenarioMismatch, scenario: res.scenario, divergence: res.divergence,
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
  return rc;
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
