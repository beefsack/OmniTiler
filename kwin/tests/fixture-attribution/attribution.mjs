// Fixture attribution: sentinel+PID native UUID/frame join only.
// Usage: node attribution.mjs --announcement FILE --baseline DIR --out DIR --nonce HEX
// OUT must be fresh outside-repo (mode 0700). Success writes attribution.json +
// commands.jsonl with attribution.complete LAST; failures retain failure.json.
import { createHash } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { childNames, validateDir } from "../baseline-check/baseline.mjs";
const here = path.dirname(fileURLToPath(import.meta.url));
const KWIN = "org.kde.KWin", SP = "/Scripting", SPI = "org.kde.kwin.Scripting", SI = "org.kde.kwin.Script";
const DBUS = "org.freedesktop.DBus", DBUSP = "/org/freedesktop/DBus", DBUSI = "org.freedesktop.DBus";
const NONCE_RE = /^[0-9a-f]{32}$/, FIX_NONCE_RE = /^[0-9a-f]{64}$/;
const UUID_RE = /^\{[0-9a-fA-F]{8}(-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\}$/;
const ID_RE = /^[A-Za-z0-9_.-]+$/, NAME_RE = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const SENTINEL = "OmniTilerTestWindow";
export class AuditError extends Error {}
const fail = (m) => { throw new AuditError(m); };
const msg = (e) => (e && e.message) || String(e);
const sha = (s) => createHash("sha256").update(s).digest("hex");
const isNum = (v) => typeof v === "number" && Number.isFinite(v);
const noUnknown = (o, allowed, what) => { for (const k of Object.keys(o)) if (!allowed.has(k)) fail(`${what}: unknown field "${k}"`); };
export const auditPlugin = (n) => (NONCE_RE.test(n) ? `omnitiler-audit-${n}` : fail("bad nonce"));
export function validateAnnouncement(o) {
  if (!o || typeof o !== "object") fail("announcement: need object");
  noUnknown(o, new Set(["socket", "pid", "nonce"]), "announcement");
  if (typeof o.socket !== "string" || !o.socket) fail("announcement: bad socket");
  if (!Number.isInteger(o.pid) || o.pid <= 0) fail("announcement: bad pid");
  if (typeof o.nonce !== "string" || !FIX_NONCE_RE.test(o.nonce)) fail("announcement: bad fixture nonce");
  return o;
}
const needBool = (v, what) => {
  if (!v || v.type !== "b" || !Array.isArray(v.data) || v.data.length !== 1 || typeof v.data[0] !== "boolean") fail(`${what}: need {type:b,data:[bool]}`);
  return v.data[0];
};
const needId = (v, what) => {
  if (!v || v.type !== "i" || !Array.isArray(v.data) || v.data.length !== 1) fail(`${what}: need {type:i,data:[id]}`);
  const id = v.data[0];
  if (!Number.isInteger(id) || id < 0 || id > 2147483647) fail(`${what}: bad script id`);
  return id;
};
const parseTick = (stat, pid) => {
  if (stat.includes("\n")) fail("bad /proc stat (newline)");
  const sp = stat.lastIndexOf(") ");
  if (sp < 0) fail("bad /proc stat shape");
  if (stat.slice(0, stat.indexOf(" ")) !== String(pid)) fail("proc pid mismatch");
  const f = stat.slice(sp + 2).split(" ");
  if (f.length < 20 || !/^[A-Za-z]$/.test(f[0] || "") || !/^[1-9][0-9]*$/.test(f[19] || "")) fail("bad /proc stat fields");
  return f[19];
};
const NAT_KEYS = new Set(["internalId", "resourceClass", "caption", "pid", "frame"]);
const rect = (r, what) => {
  if (!r || typeof r !== "object") fail(`${what}: need rect`);
  for (const k of ["x", "y", "w", "h"]) if (!isNum(r[k])) fail(`${what}: rect.${k} must be finite`);
  if (r.w <= 0 || r.h <= 0) fail(`${what}: size must be positive`);
};
function nativeRow(w, i) {
  const t = `native[${i}]`;
  if (!w || typeof w !== "object") fail(`${t}: need object`);
  noUnknown(w, NAT_KEYS, t);
  if (typeof w.internalId !== "string" || !UUID_RE.test(w.internalId)) fail(`${t}: bad internalId`);
  if (typeof w.resourceClass !== "string" || !w.resourceClass) fail(`${t}: bad resourceClass`);
  if (w.resourceClass === SENTINEL) { if (typeof w.caption !== "string" || !w.caption) fail(`${t}: fixture caption missing`); }
  else if (w.caption !== null) fail(`${t}: foreign caption must be null`);
  if (!Number.isInteger(w.pid) || w.pid <= 0) fail(`${t}: bad pid`);
  rect(w.frame, `${t}.frame`);
}
export function parseNativeJournal(stdout, nonce) {
  const MARK = `omnitiler:attribution:${nonce}:`, bodies = [];
  for (const line of String(stdout).split("\n")) {
    if (!line.trim()) continue;
    let o;
    try { o = JSON.parse(line); } catch { fail("journal: unparsable -o json line"); }
    if (typeof o.MESSAGE !== "string") fail("journal: MESSAGE missing");
    let m = o.MESSAGE;
    if (m.startsWith("js:")) m = m.slice(3).replace(/^ /, "");
    if (m.includes("omnitiler:attribution:") && !m.includes(MARK)) fail("journal: foreign-nonce attribution line present");
    const ix = m.indexOf(MARK);
    if (ix >= 0) { const rest = m.slice(ix + MARK.length); if (!rest) fail("journal: empty body"); bodies.push(rest); }
  }
  if (!bodies.length) fail("journal: no nonce lines");
  const rm = /^ready:count=(\d+)$/.exec(bodies[0]);
  if (!rm) fail("journal: first nonce line must be ready");
  const ready = +rm[1], wins = [];
  let done = null;
  bodies.slice(1).forEach((b) => {
    if (b.startsWith("window:")) {
      if (done !== null) fail("journal: window out of order");
      try { wins.push(JSON.parse(b.slice(7))); } catch { fail("journal: bad window line"); }
      return;
    }
    if (b.startsWith("error:")) fail(`journal: helper error: ${b.slice(6, 206)}`);
    const dm = /^done:count=(\d+)$/.exec(b);
    if (!dm || done !== null) fail("journal: bad tail (need exactly one done)");
    done = +dm[1];
  });
  if (done === null) fail("journal: missing done");
  if (ready !== done || ready !== wins.length) fail(`journal: count mismatch ready=${ready} done=${done} windows=${wins.length}`);
  wins.forEach(nativeRow);
  const seen = new Set();
  for (const w of wins) { if (seen.has(w.internalId)) fail(`journal: duplicate native UUID ${w.internalId}`); seen.add(w.internalId); }
  return wins;
}
function fixtureSend(io, sock, fnonce, payload, raw) {
  const argv = ["cargo", "run", "--locked", "-p", "tiler-live-fixture", "--", "send", "--socket", sock, "--nonce", fnonce, JSON.stringify(payload)];
  let r;
  try { r = io.exec(argv); } catch (e) { raw.push({ argv, status: 1, stdout: "", stderr: msg(e) }); fail(`fixture send failed: ${msg(e)}`); }
  raw.push({ argv, status: r.status, stdout: r.stdout, stderr: r.stderr });
  if (r.status !== 0) fail(`fixture send failed (status ${r.status}): ${(r.stderr || r.stdout || "").trim().slice(0, 200)}`);
  const line = String(r.stdout).split("\n").find((l) => l.trim());
  if (!line) fail("fixture send: empty reply");
  try { return JSON.parse(line); } catch { fail("fixture send: unparsable reply JSON"); }
}
// Every subject external goes through io (exec/readFile only).
export function attribute(io, opts) {
  const { announcement, baselineKwin, nonce, helperPath } = opts || {};
  validateAnnouncement(announcement);
  if (!baselineKwin || !Number.isInteger(baselineKwin.pid) || baselineKwin.pid <= 0 || typeof baselineKwin.tick !== "string" || !/^[0-9]+$/.test(baselineKwin.tick)) fail("bad baselineKwin");
  if (!NONCE_RE.test(nonce)) fail("bad nonce");
  if (typeof helperPath !== "string" || !helperPath) fail("bad helperPath");
  const raw = [];
  if (opts && typeof opts.sink === "object") opts.sink.raw = raw;
  const exec = (argv) => {
    let r;
    try { r = io.exec(argv); } catch (e) { raw.push({ argv, status: 1, stdout: "", stderr: msg(e) }); throw e; }
    raw.push({ argv, status: r.status, stdout: r.stdout, stderr: r.stderr });
    return r;
  };
  const run = (argv, what) => {
    const r = exec(argv);
    if (r.status !== 0) fail(`${what} failed (status ${r.status}): ${(r.stderr || r.stdout || "").trim().slice(0, 200)}`);
    return r;
  };
  const bus = (argv, what) => { try { return JSON.parse(run(argv, what).stdout); } catch (e) { if (e instanceof AuditError) throw e; fail(`${what}: unparsable busctl JSON`); } };
  const getOwner = bus(["busctl", "--user", "--json=short", "call", DBUS, DBUSP, DBUSI, "GetNameOwner", "s", KWIN], "GetNameOwner");
  if (getOwner.type !== "s" || !/^:[0-9]+\.[0-9]+$/.test(getOwner.data?.[0])) fail("KWin owner is not a unique name");
  const owner = getOwner.data[0];
  const getPid = bus(["busctl", "--user", "--json=short", "call", DBUS, DBUSP, DBUSI, "GetConnectionUnixProcessID", "s", owner], "GetConnectionUnixProcessID");
  if (getPid.type !== "u" || !/^[1-9][0-9]*$/.test(String(getPid.data?.[0]))) fail("bad KWin PID reply");
  const kwinPid = getPid.data[0];
  let tick;
  try { tick = parseTick(io.readFile(`/proc/${kwinPid}/stat`).toString("utf8").trim(), kwinPid); } catch (e) { if (e instanceof AuditError) throw e; fail("cannot read KWin /proc stat"); }
  if (kwinPid !== baselineKwin.pid || tick !== baselineKwin.tick) fail(`KWin identity drift vs baseline (live ${kwinPid}/${tick} vs base ${baselineKwin.pid}/${baselineKwin.tick})`);
  const sameIdent = () => {
    try {
      const r = exec(["busctl", "--user", "--json=short", "call", DBUS, DBUSP, DBUSI, "GetNameOwner", "s", KWIN]);
      if (r.status !== 0) return false;
      return JSON.parse(r.stdout)?.data?.[0] === owner && parseTick(io.readFile(`/proc/${kwinPid}/stat`).toString("utf8").trim(), kwinPid) === tick;
    } catch { return false; }
  };
  const mutate = (argv, what) => {
    if (!sameIdent()) fail(`KWin identity changed before ${what}`);
    const a = [...argv], ci = a.indexOf("call");
    if (ci >= 0 && a[ci + 1] === KWIN) a[ci + 1] = owner;
    return bus(a, what);
  };
  const listR = fixtureSend(io, announcement.socket, announcement.nonce, { v: 1, request_id: "attr-list", nonce: announcement.nonce, op: "list" }, raw);
  if (!listR || listR.request_id !== "attr-list" || listR.ok !== true || !Array.isArray(listR.result?.ids) || !listR.result.ids.length) fail("fixture list: need nonempty ids");
  const ids = [...new Set(listR.result.ids)];
  if (ids.length !== listR.result.ids.length) fail("fixture list: duplicate id");
  for (const id of ids) if (typeof id !== "string" || !ID_RE.test(id) || id.length > 64) fail(`fixture list: bad id ${id}`);
  const getR = fixtureSend(io, announcement.socket, announcement.nonce, { v: 1, request_id: "attr-get", nonce: announcement.nonce, op: "get", ids }, raw);
  if (!getR || getR.request_id !== "attr-get" || getR.ok !== true || !Array.isArray(getR.result?.windows)) fail("fixture get: bad envelope");
  const snaps = new Map();
  for (const w of getR.result.windows) {
    if (!w || w.ok !== true || typeof w.id !== "string" || !w.snapshot) fail("fixture get: bad entry");
    if (!ids.includes(w.id) || snaps.has(w.id)) fail(`fixture get: unknown/duplicate id ${w.id}`);
    const s = w.snapshot;
    if (s.id !== w.id || s.pid !== announcement.pid) fail(`fixture get: id/PID mismatch for ${w.id}`);
    if (typeof s.title !== "string" || !s.title.startsWith(`${SENTINEL}:${w.id}:`)) fail(`fixture get: bad sentinel title for ${w.id}`);
    snaps.set(w.id, s);
  }
  if (snaps.size !== ids.length) fail("fixture get: missing ids");
  const plugin = auditPlugin(nonce);
  const helperSrc = io.readFile(helperPath).toString("utf8");
  if (/Qt\./.test(helperSrc)) fail("helper must not use the Qt global");
  if (!helperSrc.includes(nonce)) fail("helper source lost the run nonce");
  const intro = ["busctl", "--user", "--json=short", "call", owner, SP, "org.freedesktop.DBus.Introspectable", "Introspect"];
  const isLoaded = ["busctl", "--user", "--json=short", "call", owner, SP, SPI, "isScriptLoaded", "s", plugin];
  if (needBool(bus(isLoaded, "pre isScriptLoaded"), "pre isScriptLoaded") !== false) fail(`refusing: plugin ${plugin} already loaded`);
  const preKids = childNames(bus(intro, "pre introspect"), "pre introspect");
  const cur0 = run(["journalctl", "--user", "--show-cursor", "-n", "0", "--no-pager"], "journalctl --show-cursor");
  const cursor0 = (cur0.stdout.split("\n").filter((l) => l.startsWith("-- cursor: ")).at(-1) || "").slice("-- cursor: ".length).trim();
  if (!cursor0) fail("no journal cursor in --show-cursor output");
  let sid = null, armed = false, tried = false, loadRaw = null, hur = null;
  const unloadArgv = ["busctl", "--user", "--json=short", "call", owner, SP, SPI, "unloadScript", "s", plugin];
  const teardown = () => {
    if (tried) fail("teardown retried (refusing second unload)");
    tried = true;
    const okIdent = sameIdent(), u = bus(unloadArgv, "unloadScript");
    if (!okIdent) fail("KWin identity changed; unload targeted original owner only, refusing success");
    if (needBool(u, "unloadScript") !== true) fail("unloadScript did not return TRUE");
    hur = u;
    if (needBool(bus(isLoaded, "final isScriptLoaded"), "final isScriptLoaded") !== false) fail("plugin still loaded after unload");
    const post = childNames(bus(intro, "post introspect"), "post introspect");
    if (sid !== null && post.has(`Script${sid}`)) fail(`Script${sid} still present after unload`);
    if (!sameIdent()) fail("KWin identity changed at completion");
  };
  const failThrough = (e) => {
    let terr = null;
    if (armed && !tried) { try { teardown(); } catch (t) { terr = msg(t); } }
    const m = e instanceof AuditError ? e.message : `capture failed: ${msg(e)}`;
    throw new AuditError(terr ? `${m}; teardown: ${terr}` : m);
  };
  let nativeWins;
  try {
    try {
      loadRaw = mutate(["busctl", "--user", "--json=short", "call", KWIN, SP, SPI, "loadScript", "ss", helperPath, plugin], "loadScript");
      sid = needId(loadRaw, "loadScript");
    } catch (e) {
      let now = false, unknown = false;
      try { now = needBool(bus(isLoaded, "post-failure isScriptLoaded"), "post-failure isScriptLoaded"); } catch { unknown = true; }
      if (!now && !unknown) throw e;
      armed = true; failThrough(e);
    }
    armed = true;
    if (preKids.has(`Script${sid}`)) failThrough(`lifecycle failure: Script${sid} present before load; unloading name only, never touching occupant`);
    if (!childNames(bus(intro, "post introspect"), "post introspect").has(`Script${sid}`)) failThrough(`Script${sid} absent after load; refusing to run`);
    if (needBool(bus(isLoaded, "post isScriptLoaded"), "post isScriptLoaded") !== true) failThrough("helper not loaded before run; refusing to run");
    if (!sameIdent()) failThrough("KWin identity changed before run");
    run(["busctl", "--user", "call", owner, `${SP}/Script${sid}`, SI, "run"], `run Script${sid}`);
    const j = run(["journalctl", "--user", "--after-cursor", cursor0, "-o", "json", `_PID=${kwinPid}`], "journalctl --after-cursor");
    try { nativeWins = parseNativeJournal(j.stdout, nonce); } catch (e) { failThrough(e); }
  } catch (e) { failThrough(e); }
  try { teardown(); } catch (e) { const err = new AuditError(`helper teardown failed: ${msg(e)}`); err.rawLog = raw.map((r) => JSON.stringify(r)).join("\n") + "\n"; throw err; }
  const byTitle = new Map();
  for (const w of nativeWins) {
    if (w.resourceClass !== SENTINEL) continue;
    if (byTitle.has(w.caption)) fail("native: duplicate fixture caption");
    byTitle.set(w.caption, w);
  }
  const joins = [], used = new Set();
  for (const id of ids) {
    const s = snaps.get(id), nat = byTitle.get(s.title);
    if (!nat) fail(`attribution: no native row for fixture "${id}" (sentinel title+PID match required)`);
    if (nat.pid !== s.pid) fail(`attribution: PID mismatch for fixture "${id}"`);
    if (used.has(nat.internalId)) fail(`attribution: duplicate native UUID for fixture "${id}"`);
    used.add(nat.internalId);
    joins.push({ id, title: s.title, pid: s.pid, uuid: nat.internalId, frame: { ...nat.frame } });
    byTitle.delete(s.title);
  }
  if (byTitle.size) fail(`attribution: ${byTitle.size} unmatched fixture-class native row(s) (stale/foreign)`);
  return { joins, kwin: { pid: kwinPid, startTick: tick }, plugin,
    helper: { plugin, scriptId: sid, loadReply: loadRaw, runOk: true, unloadReply: hur, cursor: cursor0, sourceSha256: sha(helperSrc) } };
}
export function orchestrate(runner, files, o) {
  if (!NONCE_RE.test(o.nonce)) fail("bad nonce");
  if (typeof files.template !== "string" || !files.template.includes("__ATTRIBUTION_NONCE__")) fail("helper template missing nonce placeholder");
  const helperPath = path.join(o.outDir, "helper.source.js");
  files.writeFile(helperPath, files.template.replaceAll("__ATTRIBUTION_NONCE__", o.nonce));
  const sink = {};
  try {
    const res = (o.invoke || attribute)(runner, { announcement: o.announcement, baselineKwin: o.baselineKwin, nonce: o.nonce, helperPath, sink });
    const put = (rel, buf) => files.writeFile(path.join(o.outDir, rel), buf);
    put("attribution.json", JSON.stringify({ v: 1, nonce: o.nonce, kwin: res.kwin, helper: res.helper, joins: res.joins }, null, 2) + "\n");
    put("commands.jsonl", sink.raw.map((r) => JSON.stringify(r)).join("\n") + "\n");
    files.writeFile(path.join(o.outDir, "attribution.complete"), `complete ${o.nonce}\n`);
    return res;
  } catch (e) {
    try {
      files.writeFile(path.join(o.outDir, "failure.json"), JSON.stringify({ v: 1, error: e instanceof AuditError ? e.message : `failed: ${msg(e)}` }, null, 2) + "\n");
      const log = (e && e.rawLog) || (sink.raw ? sink.raw.map((r) => JSON.stringify(r)).join("\n") + "\n" : null);
      if (log) files.writeFile(path.join(o.outDir, "commands.jsonl"), log);
    } catch {}
    throw e;
  }
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const args = process.argv.slice(2);
  const get = (k) => { const i = args.indexOf(k); return i < 0 || i + 1 >= args.length ? null : args[i + 1]; };
  const usage = () => { console.error("usage: node attribution.mjs --announcement FILE --baseline DIR --out DIR --nonce HEX"); process.exit(1); };
  try {
    const annFile = get("--announcement"), baseDir = get("--baseline"), outDir = get("--out"), nonceArg = get("--nonce");
    if (!annFile || !baseDir || !outDir || !nonceArg) usage();
    if (!NONCE_RE.test(nonceArg)) fail("bad --nonce (need 32 hex)");
    const announcement = validateAnnouncement(JSON.parse(readFileSync(annFile, "utf8")));
    const validated = validateDir(baseDir);
    const { realpathSync } = await import("node:fs");
    const repoRoot = realpathSync(path.resolve(here, "..", "..", ".."));
    const abs = path.resolve(outDir);
    if (!NAME_RE.test(path.basename(abs))) fail(`bad capture dir name: ${path.basename(abs)}`);
    let parent;
    try { parent = realpathSync(path.dirname(abs)); } catch { fail(`capture parent dir missing: ${path.dirname(abs)}`); }
    const real = path.join(parent, path.basename(abs));
    if (real === repoRoot || real.startsWith(repoRoot + path.sep)) fail(`refusing in-repo capture dir: ${real}`);
    if (existsSync(real)) fail(`refusing existing capture dir: ${real}`);
    mkdirSync(real, { mode: 0o700, recursive: false });
    const runner = {
      exec(argv) {
        try { return { status: 0, stdout: execFileSync(argv[0], argv.slice(1), { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], maxBuffer: 16 * 1024 * 1024 }), stderr: "" }; }
        catch (e) { return { status: typeof e.status === "number" ? e.status : 1, stdout: typeof e.stdout === "string" ? e.stdout : "", stderr: typeof e.stderr === "string" ? e.stderr : "" }; }
      },
      readFile: (p) => readFileSync(p),
    };
    const files = { template: readFileSync(path.join(here, "helper", "native-roster-helper.js"), "utf8"), writeFile: (p, b) => writeFileSync(p, b) };
    const res = orchestrate(runner, files, { announcement, baselineKwin: { pid: validated.manifest.kwin.pid, tick: validated.manifest.kwin.startTick }, nonce: nonceArg, outDir: real });
    console.log(`attribution complete: ${res.joins.length} fixture(s) joined`);
  } catch (e) {
    console.error(`attribution: ${msg(e)}`);
    process.exit(1);
  }
}
