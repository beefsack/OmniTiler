// baseline-check: hermetic KWin baseline capture/compare (node, no deps).
// node baseline.mjs capture DIR | compare BASE FRESH. DIR outside the repo.
// Manifest only after every check + full load/run/unload lifecycle. Raw
// calls/receipts saved in DIR. Compare validates both, reports per-item
// MATCH/MISMATCH, never repairs. KWin identity (pid+startTick) is a compared
// item; nonce/source/receipts are run proof, not inventory. KWin v6.7.4 pins
// (sibling `git show v6.7.4:<path>`): scripting.h/.cpp, .Script.xml,
// window.h (QUuid internalId), workspace_wrapper.h, core/output.h,
// core/rect.h, virtualdesktops.h.
import { createHash, randomBytes } from "node:crypto";
import { execFileSync } from "node:child_process";
import { existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, realpathSync, statSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
const here = path.dirname(fileURLToPath(import.meta.url));
const repoRoot = realpathSync(path.resolve(here, "..", "..", ".."));
const KWIN = "org.kde.KWin", SP = "/Scripting", SPI = "org.kde.kwin.Scripting", SI = "org.kde.kwin.Script";
const VDSK = "/VirtualDesktopManager", VDSKI = "org.kde.KWin.VirtualDesktopManager", EFF = "/Effects", EFFI = "org.kde.kwin.Effects";
const KG = "org.kde.kglobalaccel", KGP = "/kglobalaccel", KGI = "org.kde.KGlobalAccel", KGCI = "org.kde.kglobalaccel.Component";
const DBUS = "org.freedesktop.DBus", DBUSP = "/org/freedesktop/DBus", DBUSI = "org.freedesktop.DBus";
const PROPS = "org.freedesktop.DBus.Properties", INTRO = "org.freedesktop.DBus.Introspectable";
const NCOMP = 20, MAXJ = 16 * 1024 * 1024, MAXCFG = 256 * 1024;
const NONCE_RE = /^[0-9a-f]{32}$/, UUID_RE = /^\{[0-9a-fA-F]{8}(-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}\}$/;
const MODE_RE = /^[0-7]{4}$/, NAME_RE = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;
const SAFE_REL = /^(?:[A-Za-z0-9._][A-Za-z0-9._-]*\/)*[A-Za-z0-9._][A-Za-z0-9._-]*$/;
export class AuditError extends Error {}
const fail = (m) => { throw new AuditError(m); };
const msg = (e) => (e && e.message) || String(e);
const sha = (b) => createHash("sha256").update(b).digest("hex");
const isEnoent = (e) => !!e && e.code === "ENOENT";
const isNum = (v) => typeof v === "number" && Number.isFinite(v);
const fmtMode = (m) => {
  if (!Number.isInteger(m) || m < 0 || m > 0o7777) fail(`bad file mode: ${m}`);
  return m.toString(8).padStart(4, "0");
};
const noUnknown = (o, allowed, what) => { for (const k of Object.keys(o)) if (!allowed.has(k)) fail(`${what}: unknown field "${k}"`); };
const reqStr = (v, w) => { if (typeof v !== "string" || !v) fail(`${w}: bad string`); return v; };
const rect = (r, what) => {
  if (!r || typeof r !== "object") fail(`${what}: need rect`);
  for (const k of ["x", "y", "w", "h"]) if (!isNum(r[k])) fail(`${what}: rect.${k} must be finite`);
};
const WIN_KEYS = new Set(["internalId", "resourceClass", "resourceName", "pid", "active", "minimized", "hidden", "fullScreen", "maximizeMode", "onAllDesktops", "keepAbove", "keepBelow", "managed", "deleted", "normalWindow", "dialog", "splash", "utility", "dock", "frame", "client", "desktops", "activities", "output"]);
const BOOLS = ["active", "minimized", "hidden", "fullScreen", "onAllDesktops", "keepAbove", "keepBelow", "managed", "deleted", "normalWindow", "dialog", "splash", "utility", "dock"];
export function validateWindows(v) {
  if (!Array.isArray(v) || !v.length) fail("windows.json: need nonempty array");
  const seen = new Set();
  v.forEach((w, i) => {
    const t = `windows.json[${i}]`;
    if (!w || typeof w !== "object") fail(`${t}: need object`);
    noUnknown(w, WIN_KEYS, t);
    if (typeof w.internalId !== "string" || !UUID_RE.test(w.internalId)) fail(`${t}: bad internalId`);
    if (seen.has(w.internalId)) fail(`${t}: duplicate`); seen.add(w.internalId);
    reqStr(w.resourceClass, `${t}.resourceClass`); reqStr(w.resourceName, `${t}.resourceName`);
    if (!Number.isInteger(w.pid) || w.pid <= 0) fail(`${t}: bad pid`);
    for (const k of BOOLS) if (typeof w[k] !== "boolean") fail(`${t}.${k}: must be boolean`);
    if (!Number.isInteger(w.maximizeMode)) fail(`${t}.maximizeMode: must be int`);
    rect(w.frame, `${t}.frame`); rect(w.client, `${t}.client`);
    for (const k of ["desktops", "activities"]) if (!Array.isArray(w[k]) || !w[k].every((d) => typeof d === "string")) fail(`${t}: bad ${k}`);
    if (!w.output || typeof w.output.name !== "string" || !w.output.name) fail(`${t}: bad output`);
    rect(w.output.geom, `${t}.output.geom`);
  });
}
const WS_KEYS = new Set(["desktops", "currentDesktop", "perOutput", "activeWindow", "cursorPos", "screens", "activeScreen", "activities", "currentActivity"]);
export function validateWorkspace(v) {
  const t = "workspace.json";
  if (!v || typeof v !== "object" || Array.isArray(v)) fail(`${t}: need object`);
  noUnknown(v, WS_KEYS, t);
  if (!Array.isArray(v.desktops) || !v.desktops.length) fail(`${t}: desktops empty`);
  v.desktops.forEach((d, i) => { if (!d || typeof d.id !== "string" || !d.id || !Number.isInteger(d.num)) fail(`${t} desktops[${i}]: need {id,num}`); });
  reqStr(v.currentDesktop, `${t}.currentDesktop`); reqStr(v.activeScreen, `${t}.activeScreen`);
  if (v.activeWindow !== null && (typeof v.activeWindow !== "string" || !UUID_RE.test(v.activeWindow))) fail(`${t}: bad activeWindow`);
  if (!v.cursorPos || !isNum(v.cursorPos.x) || !isNum(v.cursorPos.y)) fail(`${t}: missing cursorPos`);
  if (!Array.isArray(v.screens) || !v.screens.length) fail(`${t}: screens empty`);
  v.screens.forEach((s, i) => { reqStr(s && s.name, `${t} screens[${i}].name`); rect(s.geom, `${t} screens[${i}].geom`); });
  if (!Array.isArray(v.perOutput) || !v.perOutput.length) fail(`${t}: perOutput empty`);
  v.perOutput.forEach((p, i) => {
    if (!p || typeof p.output !== "string" || !p.output || typeof p.desktop !== "string" || !p.desktop) fail(`${t} perOutput[${i}]: need {output,desktop}`);
    if (!v.screens.some((s) => s.name === p.output)) fail(`${t} perOutput[${i}]: output not a screen`);
  });
  if (!Array.isArray(v.activities) || !v.activities.every((a) => typeof a === "string")) fail(`${t}: bad activities`);
  if (typeof v.currentActivity !== "string") fail(`${t}: missing currentActivity`);
}
const envelope = (v, inner, what) => {
  if (typeof v === "string" && /No such method|Call failed|Unknown method/i.test(v)) fail(`${what}: error-as-data; use Properties.Get`);
  if (!v || v.type !== "v" || !Array.isArray(v.data) || v.data.length !== 1) fail(`${what}: need Properties.Get {type:v,data:[inner]}`);
  const n = v.data[0];
  if (!n || n.type !== inner) fail(`${what}: need inner {type:${inner}} (found ${n && n.type})`);
  return n;
};
const checkEffects = (v, what) => {
  const n = envelope(v, "as", what);
  if (!Array.isArray(n.data) || !n.data.every((s) => typeof s === "string")) fail(`${what}: names must be strings`);
};
export function validateDesktopsProp(v, what) {
  const n = envelope(v, "a(uss)", what);
  if (!Array.isArray(n.data) || !n.data.length) fail(`${what}: need nonempty a(uss)`);
  const pos = new Set(), ids = new Set();
  n.data.forEach((t, i) => {
    if (!Array.isArray(t) || t.length !== 3) fail(`${what}[${i}]: need 3-tuple`);
    if (!Number.isInteger(t[0]) || t[0] < 0) fail(`${what}[${i}]: bad position`);
    reqStr(t[1], `${what}[${i}].id`); reqStr(t[2], `${what}[${i}].name`);
    if (pos.has(t[0]) || ids.has(t[1])) fail(`${what}[${i}]: duplicate position/id`);
    pos.add(t[0]); ids.add(t[1]);
  });
}
const COMP_RE = /^\/component\/[A-Za-z0-9_][A-Za-z0-9_.@-]*$/;
export function validateComponents(v) {
  const t = "kglobalaccel.components.json";
  if (!v || v.type !== "ao" || !Array.isArray(v.data) || v.data.length !== 1 || !Array.isArray(v.data[0])) fail(`${t}: need {type:ao,data:[[..]]}`);
  const roster = v.data[0];
  if (roster.length !== NCOMP) fail(`${t}: expected 20 components, found ${roster.length}`);
  const seen = new Set();
  for (const c of roster) {
    if (typeof c !== "string" || !COMP_RE.test(c)) fail(`${t}: bad entry ${c}`);
    if (seen.has(c)) fail(`${t}: duplicate ${c}`); seen.add(c);
  }
  return roster;
}
export function validateInfoBody(v, what) {
  if (!v || v.type !== "a(ssssssaiai)" || !Array.isArray(v.data) || v.data.length !== 1) fail(`${what}: need {type:a(ssssssaiai),data:[[..]]}`);
  v.data[0].forEach((t, i) => {
    if (!Array.isArray(t) || t.length !== 8) fail(`${what}[${i}]: need 8-tuple`);
    for (let k = 0; k < 6; k++) if (typeof t[k] !== "string") fail(`${what}[${i}]: field ${k} must be string`);
    for (let k = 6; k < 8; k++) if (!Array.isArray(t[k]) || !t[k].every(Number.isInteger)) fail(`${what}[${i}]: field ${k} must be int[]`);
  });
}
const checkLoaded = (v, what) => {
  if (!v || v.type !== "b" || !Array.isArray(v.data) || v.data.length !== 1 || typeof v.data[0] !== "boolean") fail(`${what}: need {type:b,data:[bool]}`);
  return v.data[0];
};
const checkLoad = (v, what) => {
  if (!v || v.type !== "i" || !Array.isArray(v.data) || v.data.length !== 1) fail(`${what}: need {type:i,data:[id]}`);
  const id = v.data[0];
  if (!Number.isInteger(id) || id < 0 || id > 2147483647) fail(`${what}: id must be non-negative int32 (-1 is load failure)`);
  return id;
};
export function childNames(v, what = "introspect") {
  if (!v || v.type !== "s" || !Array.isArray(v.data) || v.data.length !== 1 || typeof v.data[0] !== "string") fail(`${what}: need {type:s,data:[xml]}`);
  const xml = v.data[0];
  const bad = (m) => fail(`${what}: malformed XML (${m})`);
  const isXmlChar = (cp) => cp === 9 || cp === 10 || cp === 13 || (cp >= 0x20 && cp <= 0xD7FF) || (cp >= 0xE000 && cp <= 0xFFFD) || (cp >= 0x10000 && cp <= 0x10FFFF);
  for (let i = 0; i < xml.length;) {
    const cp = xml.codePointAt(i);
    if (!isXmlChar(cp)) bad("bad character");
    i += cp > 0xFFFF ? 2 : 1;
  }
  const ENT = { amp: "&", lt: "<", gt: ">", quot: '"', apos: "'" };
  const dec = (raw, ctx) => {
    if (raw.includes("<")) bad(`${ctx} unescaped <`);
    if (/&(?!(?:amp|lt|gt|quot|apos);|#[0-9]+;|#x[0-9A-Fa-f]+;)/.test(raw)) bad(`${ctx} bad entity`);
    return raw.replace(/&(amp|lt|gt|quot|apos);|&#([0-9]+);|&#x([0-9A-Fa-f]+);/g, (m, n, d, h) => {
      if (n) return ENT[n];
      const cp = d ? parseInt(d, 10) : parseInt(h, 16);
      if (!isXmlChar(cp)) bad(`${ctx} bad codepoint`);
      return String.fromCodePoint(cp);
    });
  };
  const okChild = (s) => /^[A-Za-z0-9_-]+$/.test(s);
  const okMember = (s) => /^[A-Za-z_][A-Za-z0-9_]*$/.test(s);
  const okIface = (s) => /^[A-Za-z_][A-Za-z0-9_]*(\.[A-Za-z_][A-Za-z0-9_]*)+$/.test(s);
  const okType = (s) => s.length > 0 && /^[ybnqiuxtdsogavh(){}]+$/.test(s);
  const okRootName = (s) => s.length > 0 && /^[A-Za-z0-9_.\/-]+$/.test(s);
  let pos = 0;
  const len = xml.length;
  if (len && xml.charCodeAt(0) === 0xFEFF) pos = 1;
  const isWS = (c) => c === " " || c === "\t" || c === "\r" || c === "\n";
  const skipWS = () => { while (pos < len && isWS(xml[pos])) pos++; };
  const eatComment = () => {
    if (!xml.startsWith("<!--", pos)) return false;
    const end = xml.indexOf("-->", pos + 4);
    if (end < 0) bad("unclosed comment");
    const inner = xml.slice(pos + 4, end);
    if (inner.includes("--") || inner.endsWith("-")) bad("bad comment");
    pos = end + 3;
    return true;
  };
  skipWS();
  if (xml.startsWith("<?xml", pos)) {
    const end = xml.indexOf("?>", pos);
    if (end < 0) bad("unclosed xml declaration");
    const decl = xml.slice(pos, end + 2);
    if (!/^<\?xml\s+version\s*=\s*"1\.0"(?:\s+encoding\s*=\s*"[^"]+")?(?:\s+standalone\s*=\s*"(?:yes|no)")?\s*\?>$/.test(decl)) bad("bad xml declaration");
    pos = end + 2;
    skipWS();
  } else if (xml.startsWith("<?", pos)) bad("PI refused");
  let doctypeSeen = false;
  for (;;) {
    skipWS();
    if (eatComment()) continue;
    if (xml.startsWith("<!DOCTYPE", pos)) {
      if (doctypeSeen) bad("duplicate DOCTYPE");
      const end = xml.indexOf(">", pos);
      if (end < 0) bad("unclosed DOCTYPE");
      const decl = xml.slice(pos, end + 1);
      if (decl.includes("[") || decl.includes("]")) bad("DOCTYPE internal subset refused");
      if (!/^<!DOCTYPE\s+node\s+PUBLIC\s+"-\/\/freedesktop\/\/DTD D-BUS Object Introspection 1\.0\/\/EN"\s+"http:\/\/www\.freedesktop\.org\/standards\/dbus\/1\.0\/introspect\.dtd"\s*>$/.test(decl)) bad("bad DOCTYPE");
      doctypeSeen = true;
      pos = end + 1;
      continue;
    }
    break;
  }
  skipWS();
  if (xml.startsWith("<!--", pos)) bad("unreachable");
  if (xml.startsWith("<!", pos)) bad("bad <! markup");
  if (xml.startsWith("<?", pos)) bad("PI refused");
  const tagAt = (p) => {
    if (xml[p] !== "<") return null;
    const re = /<(\/?)([A-Za-z_][\w:.-]*)((?:\s+[A-Za-z_:][\w:.-]*\s*=\s*"[^"]*")*)\s*(\/?)>/y;
    re.lastIndex = p;
    const m = re.exec(xml);
    if (!m || m.index !== p) bad("bad tag");
    return { closing: m[1] === "/", name: m[2], attrsRaw: m[3], self: m[4] === "/", end: re.lastIndex };
  };
  const attrsOf = (raw, ctx) => {
    const pairs = [...raw.matchAll(/([A-Za-z_:][\w:.-]*)\s*=\s*"([^"]*)"/g)];
    let rest = raw;
    for (const mm of pairs) rest = rest.replace(mm[0], " ");
    if (rest.trim() !== "") bad(`${ctx} bad attribute syntax`);
    const map = new Map();
    for (const mm of pairs) {
      if (map.has(mm[1])) bad(`${ctx} dup attr`);
      map.set(mm[1], dec(mm[2], `${ctx} attr ${mm[1]}`));
    }
    return map;
  };
  const checkNoExtra = (map, allowed, ctx) => { for (const k of map.keys()) if (!allowed.has(k)) bad(`${ctx} bad attr "${k}"`); };
  const need = (map, keys, ctx) => { for (const k of keys) if (!map.has(k)) bad(`${ctx} missing ${k}=`); };
  const stack = [];
  const out = new Set();
  let rootSeen = false, rootDone = false;
  const top = () => stack[stack.length - 1];
  for (;;) {
    skipWS();
    while (eatComment()) skipWS();
    if (pos >= len) break;
    if (xml.startsWith("<?", pos)) bad("PI refused");
    if (xml.startsWith("<!", pos)) bad("bad <! markup");
    if (xml[pos] !== "<") {
      const nxt = xml.indexOf("<", pos);
      const slice = nxt < 0 ? xml.slice(pos) : xml.slice(pos, nxt);
      if (/[^\s]/.test(slice)) bad("text not allowed");
      pos = nxt < 0 ? len : nxt;
      continue;
    }
    const t = tagAt(pos);
    pos = t.end;
    if (t.closing && t.self) bad("bad close tag");
    if (t.closing) {
      if (t.attrsRaw.trim() !== "") bad("bad close tag");
      const tp = stack.pop();
      if (t.name === "node") { if (tp !== "R" && tp !== "C") bad(`unbalanced </${t.name}>`); }
      else if (!tp || tp !== t.name) bad(`unbalanced </${t.name}>`);
      if (!stack.length) rootDone = true;
      continue;
    }
    if (!stack.length && rootDone) bad("multiple roots");
    if (!["node", "interface", "method", "signal", "property", "arg", "annotation"].includes(t.name)) bad(`unknown <${t.name}>`);
    if (!stack.length) {
      if (t.name !== "node") bad("root must be <node>");
      if (t.self) bad("self-closed root");
      const am = attrsOf(t.attrsRaw, "root <node>");
      checkNoExtra(am, new Set(["name"]), "root <node>");
      if (am.has("name") && !okRootName(am.get("name"))) bad("bad root name");
      stack.push("R");
      rootSeen = true;
      continue;
    }
    const parent = top();
    if (t.name === "node") {
      if (stack.length !== 1 || parent !== "R") bad("nested <node> refused");
      const am = attrsOf(t.attrsRaw, "<node>");
      checkNoExtra(am, new Set(["name"]), "<node>");
      need(am, ["name"], "<node>");
      const nm = am.get("name");
      if (!okChild(nm)) bad(`malformed child name "${nm}"`);
      if (out.has(nm)) bad(`duplicate child "${nm}"`);
      out.add(nm);
      if (!t.self) stack.push("C");
      continue;
    }
    if (parent === "C") bad(`<${t.name}> inside child <node> refused`);
    if (parent === "arg" || parent === "annotation") bad(`<${t.name}> inside <${parent}> refused`);
    if (t.name === "interface") {
      if (parent !== "R") bad("<interface> misplaced");
      const am = attrsOf(t.attrsRaw, "<interface>");
      checkNoExtra(am, new Set(["name"]), "<interface>");
      need(am, ["name"], "<interface>");
      if (!okIface(am.get("name"))) bad("bad interface name");
      if (!t.self) stack.push("interface");
      continue;
    }
    if (t.name === "method" || t.name === "signal") {
      if (parent !== "interface") bad(`<${t.name}> misplaced`);
      const am = attrsOf(t.attrsRaw, `<${t.name}>`);
      checkNoExtra(am, new Set(["name"]), `<${t.name}>`);
      need(am, ["name"], `<${t.name}>`);
      if (!okMember(am.get("name"))) bad(`bad ${t.name} name`);
      if (!t.self) stack.push(t.name);
      continue;
    }
    if (t.name === "property") {
      if (parent !== "interface") bad("<property> misplaced");
      const am = attrsOf(t.attrsRaw, "<property>");
      checkNoExtra(am, new Set(["name", "type", "access"]), "<property>");
      need(am, ["name", "type", "access"], "<property>");
      if (!okMember(am.get("name"))) bad("bad property name");
      if (!okType(am.get("type"))) bad("bad property type");
      if (!["read", "write", "readwrite"].includes(am.get("access"))) bad("bad property access");
      if (!t.self) stack.push("property");
      continue;
    }
    if (t.name === "arg") {
      if (parent !== "method" && parent !== "signal") bad("<arg> misplaced");
      const am = attrsOf(t.attrsRaw, "<arg>");
      checkNoExtra(am, new Set(["name", "type", "direction"]), "<arg>");
      need(am, ["type"], "<arg>");
      if (am.has("name") && !okMember(am.get("name"))) bad("bad arg name");
      if (!okType(am.get("type"))) bad("bad arg type");
      if (am.has("direction") && !["in", "out"].includes(am.get("direction"))) bad("bad arg direction");
      if (!t.self) stack.push("arg");
      continue;
    }
    if (t.name === "annotation") {
      if (!["interface", "method", "signal", "property", "arg"].includes(parent)) bad("<annotation> misplaced");
      const am = attrsOf(t.attrsRaw, "<annotation>");
      checkNoExtra(am, new Set(["name", "value"]), "<annotation>");
      need(am, ["name", "value"], "<annotation>");
      if (!am.get("name") || /[\s<>]/.test(am.get("name"))) bad("bad annotation name");
      if (!t.self) stack.push("annotation");
      continue;
    }
    bad(`misplaced <${t.name}>`);
  }
  if (!rootSeen || !rootDone || stack.length) bad("unbalanced document");
  return out;
}
const parseTick = (stat, pid) => {
  if (stat.includes("\n")) fail("bad /proc stat (newline)");
  const sp = stat.lastIndexOf(") ");
  if (sp < 0) fail("bad /proc stat shape");
  if (stat.slice(0, stat.indexOf(" ")) !== String(pid)) fail("proc pid mismatch");
  const f = stat.slice(sp + 2).split(" ");
  if (f.length < 20 || !/^[A-Za-z]$/.test(f[0] || "") || !/^[1-9][0-9]*$/.test(f[19] || "")) fail("bad /proc stat fields");
  return f[19];
};
function parseJournal(stdout, nonce) {
  const bodies = [];
  let ready = null, done = null, ws = null;
  const wins = [], MARK = "omnitiler:baseline:";
  const strip = (m) => {
    const i = m.indexOf(MARK);
    if (i < 0) return null;
    const rest = m.slice(i + MARK.length);
    if (!rest.startsWith(`${nonce}:`)) fail("journal: foreign-nonce baseline line present");
    return rest.slice(nonce.length + 1);
  };
  for (const line of stdout.split("\n")) {
    if (!line.trim()) continue;
    let o;
    try { o = JSON.parse(line); } catch { fail("journal: unparsable -o json line"); }
    if (typeof o.MESSAGE !== "string") fail("journal: MESSAGE missing");
    let m = o.MESSAGE;
    if (m.startsWith("js:")) m = m.slice(3).replace(/^ /, "");
    const b = strip(m);
    if (b !== null) bodies.push(b);
  }
  bodies.forEach((b, ix) => {
    if (ix === 0) {
      const m = /^ready:count=(\d+)$/.exec(b);
      if (!m) fail("journal: first nonce line must be ready");
      ready = +m[1]; return;
    }
    if (b.startsWith("window:")) {
      if (ws !== null || done !== null) fail("journal: window out of order");
      try { wins.push(JSON.parse(b.slice(7))); } catch { fail("journal: bad window line"); } return;
    }
    if (b.startsWith("workspace:")) {
      if (ws !== null || done !== null) fail("journal: duplicate/misplaced workspace");
      try { ws = JSON.parse(b.slice(10)); } catch { fail("journal: bad workspace line"); } return;
    }
    if (b.startsWith("error:")) fail(`journal: helper error: ${b.slice(6, 206)}`);
    const m = /^done:count=(\d+)$/.exec(b);
    if (!m || ws === null || done !== null) fail("journal: bad tail (need workspace then exactly one done)");
    done = +m[1];
  });
  if (ready === null || ws === null || done === null) fail("journal: no valid output (need ready/window*/workspace/done)");
  if (ready !== done || ready !== wins.length) fail(`journal: count mismatch ready=${ready} done=${done} windows=${wins.length}`);
  return { wins, ws };
}
export function systemRunner(env = process.env) {
  return {
    env,
    exec(argv) {
      try { return { status: 0, stdout: execFileSync(argv[0], argv.slice(1), { encoding: "utf8", stdio: ["ignore", "pipe", "pipe"], maxBuffer: MAXJ }), stderr: "" }; }
      catch (e) { return { status: typeof e.status === "number" ? e.status : 1, stdout: typeof e.stdout === "string" ? e.stdout : "", stderr: typeof e.stderr === "string" ? e.stderr : "" }; }
    },
    readFile: (p) => readFileSync(p),
    lstat: (p) => { const s = lstatSync(p); return { mode: s.mode & 0o7777, size: s.size, isFile: s.isFile(), isDir: s.isDirectory(), isSymlink: s.isSymbolicLink() }; },
    stat: (p) => { const s = statSync(p); return { mode: s.mode & 0o7777, size: s.size, isFile: s.isFile(), isSymlink: false }; },
    listDir: (p) => readdirSync(p),
  };
}
// All external reads/calls go through io (hermetic; no real-fs/proc fallback).
export function captureData(io, nonce, helperPath, sink) {
  if (!NONCE_RE.test(nonce)) fail("bad nonce");
  const plugin = `omnitiler-audit-${nonce}`;
  const files = new Map(), raw = [];
  if (sink && typeof sink === "object") sink.raw = raw;
  const put = (r, b) => files.set(r, Buffer.isBuffer(b) ? b : Buffer.from(b, "utf8"));
  const exec = (argv) => {
    let r;
    try { r = io.exec(argv); }
    catch (e) {
      raw.push({ argv, status: typeof (e && e.status) === "number" ? e.status : 1, stdout: typeof (e && e.stdout) === "string" ? e.stdout : "", stderr: typeof (e && e.stderr) === "string" ? e.stderr : msg(e) });
      throw e;
    }
    raw.push({ argv, status: r.status, stdout: r.stdout, stderr: r.stderr });
    return r;
  };
  const run = (argv, what) => {
    const r = exec(argv);
    if (r.status !== 0) fail(`${what} failed (status ${r.status}): ${(r.stderr || r.stdout || "").trim().slice(0, 300)}`);
    return r;
  };
  const bus = (argv, what) => {
    const r = run(argv, what);
    try { return JSON.parse(r.stdout); } catch { fail(`${what}: unparsable busctl JSON`); }
  };
  const getOwner = bus(["busctl", "--user", "--json=short", "call", DBUS, DBUSP, DBUSI, "GetNameOwner", "s", KWIN], "GetNameOwner");
  if (getOwner.type !== "s" || !/^:[0-9]+\.[0-9]+$/.test(getOwner.data && getOwner.data[0])) fail("KWin owner is not a unique name");
  const owner = getOwner.data[0];
  const getPid = bus(["busctl", "--user", "--json=short", "call", DBUS, DBUSP, DBUSI, "GetConnectionUnixProcessID", "s", owner], "GetConnectionUnixProcessID");
  if (getPid.type !== "u" || !/^[1-9][0-9]*$/.test(String(getPid.data && getPid.data[0]))) fail("bad KWin PID reply");
  const kwinPid = getPid.data[0];
  let statBytes;
  try { statBytes = io.readFile(`/proc/${kwinPid}/stat`); } catch { fail("cannot read KWin /proc stat"); }
  const tick = parseTick(statBytes.toString("utf8").trim(), kwinPid);
  const sameIdent = () => {
    try {
      const r = exec(["busctl", "--user", "--json=short", "call", DBUS, DBUSP, DBUSI, "GetNameOwner", "s", KWIN]);
      if (!r || r.status !== 0 || typeof r.stdout !== "string") return false;
      let o;
      try { o = JSON.parse(r.stdout); } catch { return false; }
      if (!o || o.type !== "s" || !Array.isArray(o.data) || o.data.length !== 1 || o.data[0] !== owner) return false;
      return parseTick(io.readFile(`/proc/${kwinPid}/stat`).toString("utf8").trim(), kwinPid) === tick;
    } catch { return false; }
  };
  const mutate = (argv, what) => {
    if (!sameIdent()) fail(`KWin identity changed before ${what}`);
    const a = [...argv], ci = a.indexOf("call");
    if (ci >= 0 && a[ci + 1] === KWIN) a[ci + 1] = owner;
    return bus(a, what);
  };
  const propGet = (obj, iface, prop) => ["busctl", "--user", "--json=short", "call", KWIN, obj, PROPS, "Get", "ss", iface, prop];
  const introArgv = ["busctl", "--user", "--json=short", "call", owner, SP, INTRO, "Introspect"];
  const isLoadedArgv = ["busctl", "--user", "--json=short", "call", owner, SP, SPI, "isScriptLoaded", "s", plugin];
  for (const [prop, file] of [["loadedEffects", "effects.loaded.json"], ["activeEffects", "effects.active.json"]]) {
    const v = bus(propGet(EFF, EFFI, prop), `Properties.Get Effects.${prop}`);
    checkEffects(v, file); put(file, JSON.stringify(v));
  }
  const compsRaw = bus(["busctl", "--user", "--json=short", "call", KG, KGP, KGI, "allComponents"], "KGlobalAccel allComponents");
  const roster = validateComponents(compsRaw);
  put("kglobalaccel.components.json", JSON.stringify(compsRaw));
  for (const c of roster) {
    const v = bus(["busctl", "--user", "--json=short", "call", KG, c, KGCI, "allShortcutInfos", "s", "default"], `allShortcutInfos ${c}`);
    validateInfoBody(v, `infos/${c}`); put(`infos/${c.replaceAll("/", "_")}.json`, JSON.stringify(v));
  }
  const vdsk = bus(propGet(VDSK, VDSKI, "desktops"), "Properties.Get VirtualDesktopManager.desktops");
  validateDesktopsProp(vdsk, "desktops.json"); put("desktops.json", JSON.stringify(vdsk));
  const env = io.env || {};
  const configHome = env.XDG_CONFIG_HOME || (env.HOME ? path.join(env.HOME, ".config") : null);
  if (!configHome) fail("no XDG_CONFIG_HOME/HOME for config paths");
  const readBlob = (p, what, absentOk) => {
    let lst;
    try { lst = io.lstat(p); } catch (e) {
      if (e instanceof AuditError) throw e;
      if (absentOk && isEnoent(e)) return null;
      fail(`cannot lstat ${what}: ${msg(e)}`);
    }
    if (lst.isSymlink) fail(`${what} symlink refused: ${p}`);
    if (!lst.isFile) fail(`${what} is not a plain file: ${p}`);
    let st, bytes;
    try { st = io.stat(p); bytes = io.readFile(p); } catch (e) { fail(`cannot read ${what} (not absence): ${msg(e)}`); }
    if (!st.isFile) fail(`${what} target is not a plain file`);
    if (bytes.length !== st.size) fail(`${what} bytes do not match stat size`);
    return { meta: { mode: fmtMode(st.mode), bytes: st.size, sha256: sha(bytes) }, bytes };
  };
  const kw = readBlob(path.join(configHome, "kwinrc"), "kwinrc", false);
  put("kwinrc.bytes", kw.bytes);
  put("kwinrc.json", JSON.stringify({ symlink: false, linkTarget: null, ...kw.meta }));
  if (!env.XDG_RUNTIME_DIR) fail("missing XDG_RUNTIME_DIR for float store");
  const fl = readBlob(path.join(env.XDG_RUNTIME_DIR, "omnitiler", "float-intent.json"), "float store", true);
  put("float-store.json", JSON.stringify(fl ? { present: true, ...fl.meta } : { present: false, reason: "absent", mode: null, bytes: null, sha256: null }));
  if (fl) put("float-store.bytes", fl.bytes);
  const projDir = path.join(configHome, "omnitiler");
  let names = null;
  try {
    const lst = io.lstat(projDir);
    if (lst.isSymlink) fail(`project config dir symlink refused: ${projDir}`);
    if (!lst.isDir) fail(`project config dir is not a directory: ${projDir}`);
    names = io.listDir(projDir);
  } catch (e) {
    if (e instanceof AuditError) throw e;
    if (isEnoent(e)) names = null;
    else fail(`cannot list project config dir (not absence): ${msg(e)}`);
  }
  const projFiles = [];
  if (names === null) put("project-config.json", JSON.stringify({ present: false, files: [] }));
  else {
    const sorted = [...names].sort();
    if (sorted.length > 64) fail("too many project config files");
    const blobs = new Map();
    for (const name of sorted) {
      if (!NAME_RE.test(name)) fail(`bad project config name: ${name}`);
      const r = readBlob(path.join(projDir, name), `project config ${name}`, false);
      if (r.bytes.length > MAXCFG) fail(`project config too large: ${name}`);
      blobs.set(name, r.bytes); projFiles.push({ name, ...r.meta });
    }
    put("project-config.json", JSON.stringify({ present: true, files: projFiles }));
    for (const [n, b] of blobs) put(`project-config/${n}.bytes`, b);
  }
  const cur = run(["journalctl", "--user", "--show-cursor", "-n", "0", "--no-pager"], "journalctl --show-cursor");
  const cursor = cur.stdout.split("\n").filter((l) => l.startsWith("-- cursor: ")).at(-1);
  if (!cursor) fail("no journal cursor in --show-cursor output");
  const cursorVal = cursor.slice("-- cursor: ".length).trim();
  if (!cursorVal) fail("empty journal cursor");
  const source = io.readFile(helperPath).toString("utf8");
  if (/Qt\./.test(source)) fail("helper must not use the Qt global");
  if (!source.includes(nonce)) fail("helper source lost the run nonce");
  put("helper.source.js", source);
  if (checkLoaded(bus(isLoadedArgv, "pre isScriptLoaded"), "pre isScriptLoaded") !== false) fail(`refusing: plugin ${plugin} already loaded`);
  const preKids = childNames(bus(introArgv, "pre introspect /Scripting"), "pre introspect /Scripting");
  let id = null, torn = null, unloadTried = false, loadRaw = null, armed = false;
  const unloadArgv = ["busctl", "--user", "--json=short", "call", owner, SP, SPI, "unloadScript", "s", plugin];
  const teardown = () => {
    if (unloadTried) fail("teardown retried (refusing second unload)");
    unloadTried = true;
    const identBefore = sameIdent();
    const u = bus(unloadArgv, "unloadScript");
    if (!identBefore) fail("KWin identity changed; unload targeted original owner only, refusing success");
    if (checkLoaded(u, "unloadScript") !== true) fail("unloadScript did not return TRUE");
    if (checkLoaded(bus(isLoadedArgv, "final isScriptLoaded"), "final isScriptLoaded") !== false) fail("plugin still reported loaded after unload");
    const post = childNames(bus(introArgv, "post introspect /Scripting"), "post introspect /Scripting");
    if (id !== null && post.has(`Script${id}`)) fail(`Script${id} still present after unload`);
    if (!sameIdent()) fail("KWin identity changed at completion");
    return { unloadReply: u, postChildren: [...post].sort() };
  };
  const dumpRaw = () => raw.map((r) => JSON.stringify(r)).join("\n") + "\n";
  const failAfterLoad = (e) => {
    let terr = null;
    if (armed && !unloadTried) { try { torn = teardown(); } catch (t) { terr = msg(t); } }
    const m = typeof e === "string" ? e : e instanceof AuditError ? e.message : `capture failed: ${msg(e)}`;
    const err = new AuditError(terr ? `${m}; teardown: ${terr}` : m);
    err.rawLog = dumpRaw();
    throw err;
  };
  let wins = null, wsOut = null, j = null;
  try {
    try {
      loadRaw = mutate(["busctl", "--user", "--json=short", "call", KWIN, SP, SPI, "loadScript", "ss", helperPath, plugin], "loadScript");
      id = checkLoad(loadRaw, "loadScript");
    } catch (e) {
      let now = false, unknown = false;
      try { now = checkLoaded(bus(isLoadedArgv, "post-failure isScriptLoaded"), "post-failure isScriptLoaded"); } catch { unknown = true; }
      if (!now && !unknown) throw e;
      armed = true; failAfterLoad(e);
    }
    armed = true;
    if (preKids.has(`Script${id}`)) failAfterLoad(`lifecycle failure: Script${id} present before load; unloading name only, never touching occupant`);
    if (!childNames(bus(introArgv, "post introspect /Scripting"), "post introspect /Scripting").has(`Script${id}`)) failAfterLoad(`Script${id} absent after load; refusing to run`);
    if (checkLoaded(bus(isLoadedArgv, "post isScriptLoaded"), "post isScriptLoaded") !== true) failAfterLoad("helper not reported loaded before run; refusing to run");
    if (!sameIdent()) failAfterLoad("KWin identity changed before run");
    run(["busctl", "--user", "call", owner, `${SP}/Script${id}`, SI, "run"], `run Script${id}`);
    // Every step below runs after a successful load, so any failure here
    // must still unload the named plugin exactly once (outer failAfterLoad).
    j = run(["journalctl", "--user", "--after-cursor", cursorVal, "-o", "json", `_PID=${kwinPid}`], "journalctl --after-cursor");
    const parsed = parseJournal(j.stdout, nonce);
    wins = parsed.wins; wsOut = parsed.ws;
    validateWindows(wins); validateWorkspace(wsOut);
    put("windows.json", JSON.stringify(wins)); put("workspace.json", JSON.stringify(wsOut));
  } catch (e) { failAfterLoad(e); }
  try { torn = teardown(); } catch (e) {
    const err = new AuditError(`helper teardown failed: ${msg(e)}`);
    err.rawLog = dumpRaw();
    throw err;
  }
  put("helper.receipts.json", JSON.stringify({ plugin, scriptId: id, loadReply: loadRaw, runOk: true, unloadReply: torn.unloadReply, postChildren: torn.postChildren, kwinPid, startTick: tick, cursor: cursorVal, nonce, sourceSha256: sha(Buffer.from(source, "utf8")) }));
  put("journal.excerpt.txt", j.stdout);
  put("commands.jsonl", raw.map((r) => JSON.stringify(r)).join("\n") + "\n");
  return { files, raw, receipts: JSON.parse(files.get("helper.receipts.json")), counts: { windows: wins.length, components: roster.length, infos: roster.length }, kwin: { pid: kwinPid, startTick: tick }, plugin, nonce };
}
const resolveOutDir = (dir) => {
  const abs = path.resolve(dir), base = path.basename(abs);
  if (!NAME_RE.test(base)) fail(`bad capture dir name: ${base}`);
  let parent;
  try { parent = realpathSync(path.dirname(abs)); } catch { fail(`capture parent dir missing: ${path.dirname(abs)}`); }
  const real = path.join(parent, base);
  if (real === repoRoot || real.startsWith(repoRoot + path.sep)) fail(`refusing in-repo capture dir: ${real}`);
  if (existsSync(real)) fail(`refusing existing capture dir: ${real}`);
  return real;
};
const checkRel = (r) => { if (!SAFE_REL.test(r) || r.includes("..")) fail(`unsafe rel path: ${r}`); };
function snapshot(abs) {
  const out = new Map();
  const walk = (rel) => {
    let entries;
    try { entries = readdirSync(path.join(abs, rel), { withFileTypes: true }); }
    catch (e) { if (isEnoent(e)) return; fail(`cannot list capture dir: ${msg(e)}`); }
    for (const e of entries) {
      const r = rel ? `${rel}/${e.name}` : e.name;
      if (e.isSymbolicLink()) fail(`symlink refused in capture dir: ${r}`);
      if (e.isDirectory()) walk(r);
      else if (e.isFile()) { checkRel(r); out.set(r, sha(readFileSync(path.join(abs, r)))); }
      else fail(`non-file refused in capture dir: ${r}`);
    }
  };
  walk("");
  return out;
}
export function captureDir(outDir, runner = systemRunner(), opts = {}) {
  const real = resolveOutDir(outDir);
  mkdirSync(real, { mode: 0o700, recursive: false });
  const nonce = opts.nonce && NONCE_RE.test(opts.nonce) ? opts.nonce : randomBytes(16).toString("hex");
  const tpl = readFileSync(path.join(here, "helper", "baseline-helper.js"), "utf8");
  if (!tpl.includes("__BASELINE_NONCE__")) fail("helper template lost its nonce placeholder");
  writeFileSync(path.join(real, "helper.source.js"), tpl.replaceAll("__BASELINE_NONCE__", nonce));
  const put = (rel, buf) => {
    checkRel(rel);
    mkdirSync(path.dirname(path.join(real, rel)), { recursive: true });
    writeFileSync(path.join(real, rel), Buffer.isBuffer(buf) ? buf : Buffer.from(buf, "utf8"));
  };
  let inv;
  const sink = {};
  try { inv = captureData(runner, nonce, path.join(real, "helper.source.js"), sink); }
  catch (e) {
    try {
      put("failure.json", JSON.stringify({ v: 1, error: e instanceof AuditError ? e.message : `capture failed: ${msg(e)}` }, null, 2) + "\n");
      const log = (e && typeof e.rawLog === "string" && e.rawLog) || (sink.raw ? sink.raw.map((r) => JSON.stringify(r)).join("\n") + "\n" : null);
      if (log) put("commands.jsonl", log);
    } catch { /* keep original */ }
    throw e instanceof AuditError ? e : new AuditError(`capture failed: ${msg(e)}`);
  }
  for (const [r, b] of inv.files) put(r, b);
  const files = Object.fromEntries(snapshot(real));
  const manifest = { v: 1, complete: true, created: new Date().toISOString(), kwin: inv.kwin, plugin: inv.plugin, nonce, counts: inv.counts, files };
  validateBodies(real, manifest);
  writeFileSync(path.join(real, "manifest.json"), JSON.stringify(manifest, null, 2) + "\n");
  return { dir: real, manifest };
}
const blobMeta = (abs, rel, meta, what) => {
  if (typeof meta.mode !== "string" || !MODE_RE.test(meta.mode) || !Number.isInteger(meta.bytes) || typeof meta.sha256 !== "string") fail(`${what}: bad mode/bytes/sha256 (strict octal)`);
  const b = readFileSync(path.join(abs, rel));
  if (b.length !== meta.bytes || sha(b) !== meta.sha256) fail(`${rel} does not match metadata`);
};
const FLOAT_KEYS = new Set(["present", "reason", "mode", "bytes", "sha256"]);
const KWINRC_KEYS = new Set(["symlink", "linkTarget", "mode", "bytes", "sha256"]);
const PROJ_KEYS = new Set(["present", "files"]);
const PROJ_ENTRY = new Set(["name", "bytes", "mode", "sha256"]);
const HELP_KEYS = new Set(["plugin", "scriptId", "loadReply", "runOk", "unloadReply", "postChildren", "kwinPid", "startTick", "cursor", "nonce", "sourceSha256"]);
export function validateBodies(abs, m) {
  if (!m || m.v !== 1 || m.complete !== true) fail("no complete manifest: capture never finalized here");
  if (typeof m.created !== "string" || !m.created) fail("manifest: missing created");
  if (!m.kwin || !Number.isInteger(m.kwin.pid) || m.kwin.pid <= 0 || typeof m.kwin.startTick !== "string" || !/^[0-9]+$/.test(m.kwin.startTick)) fail("manifest: bad kwin identity");
  if (typeof m.nonce !== "string" || !NONCE_RE.test(m.nonce)) fail("manifest: bad nonce");
  if (typeof m.plugin !== "string" || m.plugin !== `omnitiler-audit-${m.nonce}`) fail("manifest: plugin must be omnitiler-audit-<nonce>");
  if (!m.counts || !Number.isInteger(m.counts.windows) || m.counts.components !== NCOMP || m.counts.infos !== NCOMP) fail("manifest: bad counts (components/infos must be 20)");
  if (!m.files || typeof m.files !== "object" || Array.isArray(m.files)) fail("manifest: bad files map");
  for (const [r, h] of Object.entries(m.files)) { checkRel(r); if (typeof h !== "string" || !/^[0-9a-f]{64}$/.test(h)) fail(`manifest: bad hash for ${r}`); }
  const disk = snapshot(abs);
  disk.delete("manifest.json"); disk.delete("failure.json");
  const listed = new Set(Object.keys(m.files));
  for (const r of listed) if (!disk.has(r)) fail(`manifest lists missing file: ${r}`);
  for (const r of disk.keys()) if (!listed.has(r)) fail(`unlisted file in capture dir: ${r}`);
  for (const [r, h] of Object.entries(m.files)) if (disk.get(r) !== h) fail(`hash mismatch (altered body): ${r}`);
  const J = (r) => JSON.parse(readFileSync(path.join(abs, r), "utf8"));
  const need = new Set(["windows.json", "workspace.json", "desktops.json", "float-store.json", "kwinrc.json", "kwinrc.bytes", "project-config.json", "kglobalaccel.components.json", "effects.loaded.json", "effects.active.json", "helper.receipts.json", "helper.source.js", "journal.excerpt.txt", "commands.jsonl"]);
  const float = J("float-store.json");
  noUnknown(float, FLOAT_KEYS, "float-store.json");
  if (float.present !== true && float.present !== false) fail("float-store.json: present must be bool");
  if (float.present) { need.add("float-store.bytes"); blobMeta(abs, "float-store.bytes", float, "float-store.json"); }
  else if (float.mode !== null || float.bytes !== null || float.sha256 !== null || (float.reason !== "absent" && float.reason !== "no-runtime-dir")) fail("float-store.json: absence needs reason + null mode/bytes/sha256");
  const wins = J("windows.json");
  validateWindows(wins);
  if (wins.length !== m.counts.windows) fail(`manifest counts.windows (${m.counts.windows}) != windows.json (${wins.length})`);
  validateWorkspace(J("workspace.json"));
  validateDesktopsProp(J("desktops.json"), "desktops.json");
  const kw = J("kwinrc.json");
  noUnknown(kw, KWINRC_KEYS, "kwinrc.json");
  if (kw.symlink !== false || kw.linkTarget !== null) fail("kwinrc.json: symlinks refused");
  blobMeta(abs, "kwinrc.bytes", kw, "kwinrc.json");
  const proj = J("project-config.json");
  noUnknown(proj, PROJ_KEYS, "project-config.json");
  if (proj.present !== true && proj.present !== false) fail("project-config.json: present must be bool");
  if (!Array.isArray(proj.files)) fail("project-config.json: files must be array");
  if (!proj.present && proj.files.length) fail("project-config.json: absent dir must list no files");
  for (const f of proj.files) {
    noUnknown(f, PROJ_ENTRY, "project-config entry");
    if (!NAME_RE.test(f.name)) fail(`project-config: bad name ${f.name}`);
    need.add(`project-config/${f.name}.bytes`);
    blobMeta(abs, `project-config/${f.name}.bytes`, f, `project-config/${f.name}`);
  }
  const roster = validateComponents(J("kglobalaccel.components.json"));
  if (roster.length !== m.counts.components) fail("manifest counts do not match component roster");
  for (const c of roster) { const r = `infos/${c.replaceAll("/", "_")}.json`; need.add(r); validateInfoBody(J(r), r); }
  checkEffects(J("effects.loaded.json"), "effects.loaded.json");
  checkEffects(J("effects.active.json"), "effects.active.json");
  const h = J("helper.receipts.json");
  noUnknown(h, HELP_KEYS, "helper.receipts.json");
  if (h.plugin !== m.plugin) fail("helper.receipts: bad plugin");
  if (checkLoad(h.loadReply, "helper.receipts loadReply") !== h.scriptId) fail("helper.receipts: scriptId/loadReply mismatch");
  if (h.runOk !== true) fail("helper.receipts: runOk must be true");
  checkLoaded(h.unloadReply, "helper.receipts unloadReply");
  if (h.unloadReply.data[0] !== true) fail("helper.receipts: unloadReply must be TRUE");
  if (!Array.isArray(h.postChildren) || h.postChildren.includes(`Script${h.scriptId}`)) fail("helper.receipts: Script<id> not proven absent post-unload");
  if (h.kwinPid !== m.kwin.pid || h.startTick !== m.kwin.startTick) fail("helper.receipts: KWin identity mismatch");
  if (h.nonce !== m.nonce) fail("helper.receipts: nonce mismatch");
  if (typeof h.cursor !== "string" || !h.cursor || typeof h.sourceSha256 !== "string" || !/^[0-9a-f]{64}$/.test(h.sourceSha256)) fail("helper.receipts: bad cursor/sourceSha256");
  const src = readFileSync(path.join(abs, "helper.source.js"), "utf8");
  if (!src.includes(m.nonce)) fail("helper.source.js lost the run nonce");
  if (sha(Buffer.from(src, "utf8")) !== h.sourceSha256) fail("helper.source.js mismatch");
  if (JSON.stringify([...need].sort()) !== JSON.stringify([...listed].sort())) {
    const missing = [...need].filter((r) => !listed.has(r)), extra = [...listed].filter((r) => !need.has(r));
    fail(`manifest inventory mismatch (missing: ${missing.join(",") || "-"}; extra: ${extra.join(",") || "-"})`);
  }
  return { roster, windows: wins.length };
}
export function validateDir(dir) {
  const abs = path.resolve(dir);
  let rreal;
  try { rreal = realpathSync(abs); } catch { fail(`no such capture dir: ${abs}`); }
  if (rreal === repoRoot || rreal.startsWith(repoRoot + path.sep)) fail(`refusing capture dir inside repo: ${rreal}`);
  let m;
  try { m = JSON.parse(readFileSync(path.join(rreal, "manifest.json"), "utf8")); }
  catch { fail("no complete manifest: capture never finalized here"); }
  return { abs: rreal, manifest: m, ...validateBodies(rreal, m) };
}
const RUN_SPECIFIC = new Set(["helper.receipts.json", "helper.source.js", "journal.excerpt.txt", "commands.jsonl"]);
export function compareDirs(base, fresh) {
  const b = validateDir(base), f = validateDir(fresh);
  const lines = [];
  let mismatch = 0;
  const bk = b.manifest.kwin, fk = f.manifest.kwin;
  if (bk.pid === fk.pid && bk.startTick === fk.startTick) lines.push(`MATCH kwin identity pid=${bk.pid} startTick=${bk.startTick}`);
  else { lines.push(`MISMATCH kwin identity base=${bk.pid}/${bk.startTick} fresh=${fk.pid}/${fk.startTick}`); mismatch++; }
  const bw = new Map(JSON.parse(readFileSync(path.join(b.abs, "windows.json"), "utf8")).map((w) => [w.internalId, w]));
  const fw = new Map(JSON.parse(readFileSync(path.join(f.abs, "windows.json"), "utf8")).map((w) => [w.internalId, w]));
  for (const uuid of [...new Set([...bw.keys(), ...fw.keys()])].sort()) {
    const x = bw.get(uuid), y = fw.get(uuid);
    if (!x) { lines.push(`ADDED window ${uuid}`); mismatch++; }
    else if (!y) { lines.push(`REMOVED window ${uuid}`); mismatch++; }
    else {
      const diff = [...new Set([...Object.keys(x), ...Object.keys(y)])].sort().filter((k) => JSON.stringify(x[k]) !== JSON.stringify(y[k]));
      if (!diff.length) lines.push(`MATCH window ${uuid}`);
      else { lines.push(`MISMATCH window ${uuid} fields: ${diff.join(",")}`); mismatch++; }
    }
  }
  const wb = JSON.parse(readFileSync(path.join(b.abs, "workspace.json"), "utf8"));
  const wf = JSON.parse(readFileSync(path.join(f.abs, "workspace.json"), "utf8"));
  for (const k of [...new Set([...Object.keys(wb), ...Object.keys(wf)])].sort()) {
    if (JSON.stringify(wb[k]) === JSON.stringify(wf[k])) lines.push(`MATCH workspace.${k}`);
    else { lines.push(`MISMATCH workspace.${k}`); mismatch++; }
  }
  for (const rel of [...new Set([...Object.keys(b.manifest.files), ...Object.keys(f.manifest.files)])].filter((r) => !RUN_SPECIFIC.has(r)).sort()) {
    const x = b.manifest.files[rel], y = f.manifest.files[rel];
    if (x === undefined) { lines.push(`MISMATCH body ${rel} (added in fresh)`); mismatch++; }
    else if (y === undefined) { lines.push(`MISMATCH body ${rel} (missing in fresh)`); mismatch++; }
    else if (x === y) lines.push(`MATCH body ${rel}`);
    else { lines.push(`MISMATCH body ${rel}`); mismatch++; }
  }
  return { lines, mismatch };
}
if (process.argv[1] && path.resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const [cmd, a, b2] = process.argv.slice(2);
  const usage = () => { console.error("usage: node baseline.mjs capture DIR | compare BASE FRESH"); process.exit(1); };
  try {
    if (cmd === "capture" && a && !b2) {
      const r = captureDir(a);
      console.log(`capture complete: ${r.dir} windows=${r.manifest.counts.windows}`);
    } else if (cmd === "compare" && a && b2) {
      const { lines, mismatch } = compareDirs(a, b2);
      for (const l of lines) console.log(l);
      process.exit(mismatch ? 2 : 0);
    } else usage();
  } catch (e) {
    console.error(`baseline: ${msg(e)}`);
    process.exit(1);
  }
}
