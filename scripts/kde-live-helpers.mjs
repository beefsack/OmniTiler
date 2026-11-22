// scripts/kde-live-helpers.mjs: reusable KWin helper-script lifecycle for live checks.
// Small importable library extracted from scripts/kde-scenarios.mjs helperOp.
// One caller-supplied ES2017 body per run(), plus a workspace cursor read and
// exact-owned cleanup. Fixture scope and procedure bodies stay caller
// responsibility; no fixture, input, or teardown policy lives here.
// Offline by default: importing this module performs no live calls. Live use
// requires an explicit createKwinScriptChannel call with an existing private
// caller-supplied evidenceDir. In-repo evidence is permitted only under the
// caller-supplied directory (for example a gitignored results path); this
// module never hardcodes /tmp or local paths and never checks evidenceDir
// against the repository root.
import { randomBytes } from "node:crypto";
import {
  createLiveIo, repoRoot,
  KWIN_BUS, KWIN_SCRIPTING, KWIN_SCRIPTING_IFACE, KWIN_SCRIPT_IFACE,
  DBUS_BUS, DBUS_OBJECT, DBUS_IFACE, INTRO_IFACE, UNIQUE_RE, T,
} from "./kde-scenarios.mjs";
import { childNames } from "../kwin/tests/baseline-check/baseline.mjs";

export const KWIN_CHANNEL_PLUGIN_PREFIX = "omnitiler-live-";
export const KWIN_CHANNEL_MARK = "omnitiler:live:";
export const RECEIPT_WAIT_MS = 5000;
export const RECEIPT_POLL_MS = 50;
export const RECEIPT_MAX_ATTEMPTS = 100;

export class KwinChannelError extends Error {}

function fail(message) { throw new KwinChannelError(message); }
function errMsg(e) { return (e && e.message) ? e.message : String(e); }

// Generated KWin script: non-module ES2017 IIFE. The caller body must call
// emit(value) exactly once; it runs inside the try block with emit() in scope.
// A throw becomes the single {error} result line. KWin supplies the global
// workspace; Node supplies nothing. No imports, no optional catch binding.
export function generateHelperSource(body, nonce) {
  if (typeof body !== "string" || !body.trim()) { fail("helper body required"); }
  if (typeof nonce !== "string" || !nonce) { fail("nonce required"); }
  return [
    "(function () {",
    "  var NONCE = " + JSON.stringify(nonce) + ";",
    "  var PREFIX = " + JSON.stringify(KWIN_CHANNEL_MARK) + " + NONCE + \":\";",
    "  function emit(value) { console.log(PREFIX + JSON.stringify(value)); }",
    "  try {",
    body,
    "  } catch (e) {",
    "    try { emit({ error: String((e && e.message) || e) }); } catch (x) { /* never throw past the boundary */ }",
    "  }",
    "})();",
    "",
  ].join("\n");
}

export const KWIN_CHANNEL_CURSOR_BODY = [
  "var cp;",
  "try { cp = workspace.cursorPos; } catch (e) { throw new Error('cursorPos unreadable'); }",
  "if (cp === null || typeof cp !== 'object') { throw new Error('cursorPos missing'); }",
  "var cx = cp.x;",
  "var cy = cp.y;",
  "if (typeof cx !== 'number' || !isFinite(cx) || typeof cy !== 'number' || !isFinite(cy)) { throw new Error('cursorPos invalid'); }",
  "emit({ x: Math.round(cx), y: Math.round(cy) });",
].join("\n");

// Fixture-only native read: QML helpers plus a read-only emit. No procedure
// or activation/staging action lives here; caller mutation bodies compose
// KWIN_FIXTURE_PREAMBLE + their own body and reuse joined() per call.
export const KWIN_FIXTURE_PREAMBLE = [
  "var ws=workspace;",
  "function arr(x){var a=[];if(!x||typeof x.length!==\"number\"||!isFinite(x.length)||Math.floor(x.length)!==x.length||x.length<0||x.length>4096)throw Error(\"bad list\");for(var i=0;i<x.length;i++)a.push(x[i]);return a;}",
  "function rect(r){return {x:Math.round(r.x),y:Math.round(r.y),w:Math.round(r.width),h:Math.round(r.height)};}",
  "function did(d){return String(d.id);}",
  "function title(w){return String(w.captionNormal||w.caption||\"\");}",
  "function current(o){return typeof ws.currentDesktopForScreen===\"function\"?did(ws.currentDesktopForScreen(o)):did(ws.currentDesktop);}",
  "function joined(j){var a=arr(ws.windowList()).filter(function(w){return String(w.internalId)===j.uuid&&w.pid===j.pid&&String(w.resourceClass)===\"OmniTilerTestWindow\"&&title(w)===j.title;});if(a.length!==1)throw Error(\"fixture join lost\");return a[0];}",
  "",
].join("\n");

const KWIN_FIXTURE_READ_BODY = [
  "var p=ws.cursorPos;var hits=arr(ws.windowAt({x:p.x,y:p.y}));var active=ws.activeWindow;",
  "emit({desktops:arr(ws.desktops).map(did),active:active?String(active.internalId):null,activeClass:active?String(active.resourceClass):null,activeDesktop:active?active.desktopWindow===true:false,activeDock:active?active.dock===true:false,activeOutput:ws.activeScreen?String(ws.activeScreen.name):null,cursor:{x:p.x,y:p.y},hit:hits.length?String(hits[0].internalId):null,",
  "outputs:arr(ws.screens).map(function(o){return {name:String(o.name),rect:rect(o.geometry),current:current(o)};}),",
  "owned:arr(ws.windowList()).filter(function(w){return String(w.resourceClass)===\"OmniTilerTestWindow\";}).map(function(w){return {uuid:String(w.internalId),pid:w.pid,title:title(w),frame:rect(w.frameGeometry),out:String(w.output.name),desks:arr(w.desktops).map(did),minimized:w.minimized,hidden:w.hidden};})});",
  "",
].join("\n");

export async function readFixtureState(channel, joins = [], label = "fixture-state") {
  if (!channel || typeof channel.run !== "function") { fail("readFixtureState: channel.run required"); }
  if (!Array.isArray(joins)) { fail("readFixtureState: joins must be an array"); }
  if (typeof label !== "string" || !label) { fail("readFixtureState: label required"); }
  const result = await channel.run(KWIN_FIXTURE_PREAMBLE + KWIN_FIXTURE_READ_BODY, label);
  if (!result || typeof result !== "object" || Array.isArray(result) || !Array.isArray(result.owned)) {
    fail("readFixtureState: need {owned:[...]} result");
  }
  for (const j of joins) {
    const n = result.owned.filter((w) => w && w.uuid === j.uuid && w.pid === j.pid && w.title === j.title).length;
    if (n !== 1) { fail("readFixtureState: fixture join lost for " + String(j && j.uuid)); }
  }
  return result;
}

export function joinFixtureWindows(snapshots, native) {
  if (!Array.isArray(snapshots)) { fail("joinFixtureWindows: snapshots must be an array"); }
  if (!native || typeof native !== "object" || !Array.isArray(native.owned)) { fail("joinFixtureWindows: native.owned must be an array"); }
  const ids = new Set();
  const uuids = new Set();
  const out = [];
  for (const snap of snapshots) {
    if (!snap || typeof snap.id !== "string" || !snap.id) { fail("joinFixtureWindows: snapshot id required"); }
    if (ids.has(snap.id)) { fail("joinFixtureWindows: duplicate snapshot id " + snap.id); }
    ids.add(snap.id);
    const matches = native.owned.filter((w) => w && w.pid === snap.pid && w.title === snap.title);
    if (matches.length !== 1) { fail("joinFixtureWindows: fixture join lost for " + snap.id); }
    const uuid = matches[0].uuid;
    if (typeof uuid !== "string" || !uuid) { fail("joinFixtureWindows: bad native uuid for " + snap.id); }
    if (uuids.has(uuid)) { fail("joinFixtureWindows: duplicate native uuid " + uuid); }
    uuids.add(uuid);
    out.push({ id: snap.id, pid: snap.pid, title: snap.title, uuid });
  }
  return out;
}

// Isolated live mode (docs/live-kwin-testing.md): one desktop appended past the
// roster per run and made current so fixture windows open on it, removed at
// teardown. Removal refuses while any window is a member, so it never
// relocates windows; an already-absent desktop is a no-op.
export async function appendTestDesktop(channel, name = "omnitiler-test") {
  if (!channel || typeof channel.run !== "function") { fail("appendTestDesktop: channel.run required"); }
  if (typeof name !== "string" || !name) { fail("appendTestDesktop: name required"); }
  const body = [
    "var before=arr(ws.desktops).map(did);",
    "ws.createDesktop(before.length," + JSON.stringify(name) + ");",
    "var fresh=arr(ws.desktops).filter(function(d){return before.indexOf(did(d))<0;});",
    "if(fresh.length!==1)throw Error(\"append uncorrelated\");",
    "ws.currentDesktop=fresh[0];",
    "if(did(ws.currentDesktop)!==did(fresh[0]))throw Error(\"appended desktop not current\");",
    "emit({id:did(fresh[0])});",
  ].join("\n");
  const result = await channel.run(KWIN_FIXTURE_PREAMBLE + body, "desktop-append");
  if (!result || typeof result.id !== "string" || !result.id) { fail("appendTestDesktop: need {id} result"); }
  return result.id;
}

export async function removeTestDesktop(channel, id) {
  if (!channel || typeof channel.run !== "function") { fail("removeTestDesktop: channel.run required"); }
  if (typeof id !== "string" || !id) { fail("removeTestDesktop: id required"); }
  const body = [
    "var id=" + JSON.stringify(id) + ";",
    "var d=arr(ws.desktops).filter(function(x){return did(x)===id;});",
    "if(d.length===0){emit({removed:false});return;}",
    "var occ=arr(ws.windowList()).filter(function(w){return arr(w.desktops).some(function(x){return did(x)===id;});});",
    "if(occ.length>0)throw Error(\"desktop occupied by \"+occ.length+\" window(s)\");",
    "ws.removeDesktop(d[0]);",
    "emit({removed:true});",
  ].join("\n");
  const result = await channel.run(KWIN_FIXTURE_PREAMBLE + body, "desktop-remove");
  if (!result || typeof result.removed !== "boolean") { fail("removeTestDesktop: need {removed} result"); }
  return result.removed;
}

export async function createKwinScriptChannel(opts) {
  opts = opts || {};
  const evidenceDir = opts.evidenceDir;
  if (typeof evidenceDir !== "string" || !evidenceDir || evidenceDir[0] !== "/") {
    fail("evidenceDir must be absolute");
  }
  const io = opts.io || createLiveIo();
  if (!io || typeof io.exec !== "function" || typeof io.writeFile !== "function" ||
    typeof io.exists !== "function" || typeof io.procIdentity !== "function" ||
    typeof io.sleep !== "function" || typeof io.now !== "function") {
    fail("io.exec/writeFile/exists/procIdentity/sleep/now required");
  }
  const root = opts.root || repoRoot();
  if (typeof root !== "string" || !root || root[0] !== "/") { fail("root must be absolute"); }
  const env = opts.env || process.env;
  if (!env || typeof env !== "object") { fail("env required"); }

  const rawCalls = [];
  const pending = [];
  let kwin = null;

  const exec = async (argv, timeoutMs) => {
    let r;
    try {
      r = await io.exec(argv, { timeoutMs: timeoutMs || T.exec, env, cwd: root });
    } catch (e) {
      rawCalls.push({ argv: argv.map(String), status: (e && typeof e.status === "number") ? e.status : 1,
        stdout: (e && typeof e.stdout === "string") ? e.stdout : "", stderr: errMsg(e) });
      throw e;
    }
    if (!r || typeof r.status !== "number") { fail("bad exec result"); }
    rawCalls.push({ argv: argv.map(String), status: r.status,
      stdout: typeof r.stdout === "string" ? r.stdout : "",
      stderr: typeof r.stderr === "string" ? r.stderr : "" });
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
    if (cur.owner !== kwin.owner || cur.pid !== kwin.pid || cur.tick !== kwin.tick) {
      fail("KWin identity changed");
    }
  };
  // Exact-object introspection proves the returned Script object exposes
  // org.kde.kwin.Script run; the /Scripting child inventory alone cannot.
  // The strict childNames() parser is insufficient here: a script object has
  // no child <node> entries, so the interface check is a narrow containment
  // check on the envelope-validated XML string, never a full parse rewrite.
  // A real KWin Script object has FOUR interfaces (org.kde.kwin.Script plus
  // org.freedesktop.DBus Properties/Introspectable/Peer): exactly one target
  // block is required, its run method is required inside it, and run methods
  // on unrelated interfaces never count.
  const checkScriptInterface = (v, what) => {
    if (!v || v.type !== "s" || !Array.isArray(v.data) || v.data.length !== 1 || typeof v.data[0] !== "string") {
      fail(what + ": need {type:s,data:[xml]}");
    }
    const xml = v.data[0];
    const targetRe = /<interface\s[^>]*\bname="org\.kde\.kwin\.Script"[^>]*>([\s\S]*?)<\/interface>/g;
    const blocks = [];
    let m;
    while ((m = targetRe.exec(xml)) !== null) { blocks.push(m[1]); }
    if (blocks.length !== 1) { fail(what + ": need exactly one org.kde.kwin.Script interface"); }
    if (!/<method\s[^>]*\bname="run"(?:\s|\/|>)/.test(blocks[0])) {
      fail(what + ": missing run method on org.kde.kwin.Script");
    }
  };
  // Result lines only: exact nonce mark, entries from the exact KWin PID
  // (scoped by the --after-cursor _PID query; a present _PID field mismatch
  // is a foreign line, never silently dropped).
  const parseReceiptRows = (stdout, mark) => {
    const rows = [];
    for (const line of stdout.split("\n")) {
      if (!line.trim()) { continue; }
      let entry;
      try { entry = JSON.parse(line); } catch (e) { fail("helper journal: unparsable -o json line"); }
      if (!entry || typeof entry !== "object" || typeof entry.MESSAGE !== "string") { fail("helper journal: MESSAGE missing"); }
      if (Object.prototype.hasOwnProperty.call(entry, "_PID") && String(entry._PID) !== String(kwin.pid)) {
        fail("helper journal: entry from wrong PID");
      }
      let msg = entry.MESSAGE;
      if (msg.indexOf("js:") === 0) { msg = msg.slice(3).replace(/^ /, ""); }
      if (msg.indexOf(KWIN_CHANNEL_MARK) >= 0 && msg.indexOf(mark) < 0) {
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
    return rows;
  };
  // Bounded receipt wait: KWin journal flush is asynchronous, so poll with
  // a deadline and an attempt cap. Both bind; neither loop is unbounded.
  const awaitReceipt = async (cursor, mark, holder) => {
    const argv = ["journalctl", "--user", "--after-cursor", cursor, "-o", "json", "_PID=" + kwin.pid];
    const start = await io.now();
    for (let attempt = 0; attempt < RECEIPT_MAX_ATTEMPTS; attempt++) {
      const r = await mustExec(argv, "helper journal");
      holder.stdout = r.stdout;
      const rows = parseReceiptRows(r.stdout, mark);
      if (rows.length > 1) { fail("helper produced duplicate nonce result lines"); }
      if (rows.length === 1) {
        const row = rows[0];
        if (row && typeof row === "object" && row.error) { fail("helper error: " + String(row.error).slice(0, 200)); }
        return row;
      }
      if (attempt + 1 >= RECEIPT_MAX_ATTEMPTS) { break; }
      const nowVal = await io.now();
      if (typeof nowVal === "number" && nowVal - start >= RECEIPT_WAIT_MS) { break; }
      await io.sleep(RECEIPT_POLL_MS);
    }
    fail("helper produced no nonce result line (run reply is not proof)");
    return null;
  };
  // Cleanup requires unchanged KWin identity before any unload: an identity
  // drift (owner/PID/process-start change) refuses the unload, preserves the
  // unresolved owned state/error, and close() never blindly retries it.
  const unloadOnce = async (entry) => {
    const plugin = entry.plugin, sid = entry.sid;
    const introArgv = ["busctl", "--user", "--json=short", "call", kwin.owner, KWIN_SCRIPTING, INTRO_IFACE, "Introspect"];
    const loadedArgv = ["busctl", "--user", "--json=short", "call", kwin.owner, KWIN_SCRIPTING, KWIN_SCRIPTING_IFACE, "isScriptLoaded", "s", plugin];
    const unloadArgv = ["busctl", "--user", "--json=short", "call", kwin.owner, KWIN_SCRIPTING, KWIN_SCRIPTING_IFACE, "unloadScript", "s", plugin];
    await kwinSame();
    const u = await busJson(unloadArgv, "helper unloadScript");
    if (strictBool(u, "helper unloadScript") !== true) { fail("helper unloadScript did not return TRUE"); }
    entry.unloadReply = u;
    const fin = await busJson(loadedArgv, "helper final isScriptLoaded");
    if (strictBool(fin, "helper final isScriptLoaded") !== false) { fail("helper still loaded after unload"); }
    const kids = childNames(await busJson(introArgv, "helper final introspect"), "helper final introspect");
    if (kids.has("Script" + sid)) { fail("Script" + sid + " still present after unload"); }
  };

  kwin = await kwinIdentity();
  const identity = Object.freeze({ owner: kwin.owner, pid: kwin.pid, tick: kwin.tick });

  async function run(body, label) {
    if (typeof body !== "string" || !body.trim()) { fail("helper body required"); }
    if (typeof label !== "string" || !label) { fail("run label required"); }
    const runCallStart = rawCalls.length;
    const nonce = randomBytes(16).toString("hex");
    const plugin = KWIN_CHANNEL_PLUGIN_PREFIX + nonce;
    const mark = KWIN_CHANNEL_MARK + nonce + ":";
    const src = generateHelperSource(body, nonce);
    const helperPath = evidenceDir + "/helper-" + nonce + ".js";
    const entry = { plugin, nonce, sid: null, owned: false, state: "pending", error: null, unloadReply: null };
    pending.push(entry);
    // Fresh source file only: an occupied path is refused before any bus
    // call and never overwritten; the caller directory owns the evidence.
    try {
      if (await io.exists(helperPath)) { fail("refusing: helper source path occupied " + helperPath); }
      await io.writeFile(helperPath, src, 0o600);
    } catch (e) {
      entry.state = "unloaded";
      throw e;
    }
    let cursor = null, sid = null, loadRaw = null, result = null, runOk = false;
    let primaryError = null, cleanupError = null, persistError = null;
    const journalHolder = { stdout: "" };
    try {
      const loadedArgv = ["busctl", "--user", "--json=short", "call", kwin.owner, KWIN_SCRIPTING, KWIN_SCRIPTING_IFACE, "isScriptLoaded", "s", plugin];
      const introArgv = ["busctl", "--user", "--json=short", "call", kwin.owner, KWIN_SCRIPTING, INTRO_IFACE, "Introspect"];
      if (strictBool(await busJson(loadedArgv, "helper pre isScriptLoaded"), "helper pre isScriptLoaded") !== false) {
        fail("refusing: helper " + plugin + " already loaded");
      }
      const preKids = childNames(await busJson(introArgv, "helper pre introspect"), "helper pre introspect");
      cursor = await showCursor("helper cursor");
      await kwinSame();
      loadRaw = await busJson(["busctl", "--user", "--json=short", "call",
        kwin.owner, KWIN_SCRIPTING, KWIN_SCRIPTING_IFACE, "loadScript", "ss", helperPath, plugin], "helper loadScript");
      sid = strictId(loadRaw, "helper loadScript");
      entry.sid = sid;
      entry.owned = true;
      entry.state = "loaded";
      if (preKids.has("Script" + sid)) { fail("Script" + sid + " present before load; refusing to run"); }
      const postKids = childNames(await busJson(introArgv, "helper post introspect"), "helper post introspect");
      if (!postKids.has("Script" + sid)) { fail("Script" + sid + " absent after load; refusing to run"); }
      checkScriptInterface(await busJson(["busctl", "--user", "--json=short", "call",
        kwin.owner, KWIN_SCRIPTING + "/Script" + sid, INTRO_IFACE, "Introspect"], "helper exact introspect"), "helper exact introspect");
      if (strictBool(await busJson(loadedArgv, "helper post isScriptLoaded"), "helper post isScriptLoaded") !== true) {
        fail("helper not loaded before run; refusing to run");
      }
      await kwinSame();
      const runRes = await exec(["busctl", "--user", "call", kwin.owner, KWIN_SCRIPTING + "/Script" + sid, KWIN_SCRIPT_IFACE, "run"], T.exec);
      if (runRes.status !== 0) { fail("helper run failed for Script" + sid); }
      await kwinSame();
      result = await awaitReceipt(cursor, mark, journalHolder);
      runOk = true;
    } catch (e) {
      primaryError = e;
    }
    if (entry.owned && entry.sid !== null) {
      try {
        await unloadOnce(entry);
        entry.state = "unloaded";
      } catch (e) {
        entry.state = "unload-failed";
        entry.error = e;
        cleanupError = e;
      }
    } else {
      entry.state = "unloaded";
    }
    // Evidence: generated source (written above) plus this run's raw
    // commands, journal excerpt, and lifecycle receipt. Fresh nonce files
    // only; nothing is removed (source preservation is intentional).
    try {
      const cmds = rawCalls.slice(runCallStart).map((c) => JSON.stringify(c)).join("\n") + (rawCalls.length > runCallStart ? "\n" : "");
      await io.writeFile(evidenceDir + "/helper-" + nonce + ".commands.jsonl", cmds, 0o600);
      await io.writeFile(evidenceDir + "/helper-" + nonce + ".journal.txt", journalHolder.stdout || "", 0o600);
      const receipt = {
        v: 1, plugin, scriptId: sid, label, nonce,
        kwinPid: kwin.pid, kwinTick: kwin.tick, kwinOwner: kwin.owner, cursor,
        loadReply: loadRaw, unloadReply: entry.unloadReply, runOk,
        result: runOk ? result : null,
        error: (primaryError || cleanupError) ? errMsg(primaryError || cleanupError) : null,
      };
      await io.writeFile(evidenceDir + "/helper-" + nonce + ".receipt.json", JSON.stringify(receipt, null, 2) + "\n", 0o600);
    } catch (e) {
      persistError = e;
    }
    if (cleanupError && primaryError) {
      const combined = new KwinChannelError(errMsg(primaryError) + "; cleanup: " + errMsg(cleanupError));
      combined.cause = cleanupError;
      throw combined;
    }
    if (cleanupError) { throw cleanupError; }
    if (primaryError) { throw primaryError; }
    if (persistError) { throw persistError; }
    return result;
  }

  async function cursor() {
    const res = await run(KWIN_CHANNEL_CURSOR_BODY, "cursor");
    if (!res || typeof res !== "object" || Array.isArray(res)) { fail("cursor: need {x,y} result"); }
    if (!Number.isInteger(res.x) || !Number.isInteger(res.y)) { fail("cursor: need integer x,y"); }
    return { x: res.x, y: res.y };
  }

  // Idempotent for truly completed/unloaded scripts. A failed ambiguous
  // unload is never blindly retried: close() surfaces the recorded failure
  // so the caller can write it down. run() already unloads an owned helper
  // exactly once in its finally; close() only covers outstanding entries.
  async function close() {
    for (const entry of pending) {
      if (entry.state === "unloaded") { continue; }
      if (entry.state === "unload-failed") { throw entry.error; }
      if (entry.state === "loaded") {
        try {
          await unloadOnce(entry);
          entry.state = "unloaded";
        } catch (e) {
          entry.state = "unload-failed";
          entry.error = e;
          throw e;
        }
      } else {
        entry.state = "unloaded";
      }
    }
  }

  return { run, cursor, close, identity: Object.freeze({ owner: kwin.owner, pid: kwin.pid, tick: kwin.tick }) };
}
