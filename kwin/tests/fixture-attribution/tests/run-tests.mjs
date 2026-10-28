// Hermetic offline tests: every subject external goes through injected io.
// Static repo files (helper template) are test inputs only. Run:
// node --test tests/run-tests.mjs (from fixture-attribution/).
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import path from "node:path";
import vm from "node:vm";
import { fileURLToPath } from "node:url";
import { AuditError, attribute, orchestrate, parseNativeJournal, auditPlugin } from "../attribution.mjs";
const THIS = path.dirname(fileURLToPath(import.meta.url));
const NONCE = "ab".repeat(16);
const ANN = { socket: "/tmp/fixture.sock", pid: 111, nonce: "cd".repeat(32) };
const BASE = { pid: 7, tick: "12345" };
const U1 = "{11111111-1111-1111-1111-111111111111}", U2 = "{22222222-2222-2222-2222-222222222222}", U3 = "{33333333-3333-3333-3333-333333333333}";
const title = (id) => `OmniTilerTestWindow:${id}:rt99`;
const HPATH = "/out/helper.source.js";
const J = (v) => JSON.stringify(v);
const enoent = () => Object.assign(new Error("ENOENT"), { code: "ENOENT" });
const PROC = (t = "12345") => `7 (kwin_wayland) S ${[...Array(18).fill("1"), t, "0"].join(" ")}`;
const nat = (o = {}) => ({ internalId: U1, resourceClass: "OmniTilerTestWindow", caption: title("a"), pid: 111, frame: { x: 0, y: 0, w: 100, h: 100 }, ...o });
const natJl = (n, b) => b.map((x) => J({ MESSAGE: `omnitiler:attribution:${n}:${x}` })).join("\n") + "\n";
const natOk = (n, w) => natJl(n, [`ready:count=${w.length}`, ...w.map((x) => `window:${J(x)}`), `done:count=${w.length}`]);
const kids = (n) => `<node>${n.map((x) => `<node name="${x}"/>`).join("")}</node>`;
const W2 = () => [nat({}), nat({ internalId: U2, caption: title("b"), frame: { x: 200, y: 0, w: 100, h: 100 } })];
function fake(o = {}) {
  const calls = [], st = { li: 0, ii: 0, own: 0, proc: 0, unloads: 0, runs: 0, jn: 0 };
  const loads = o.loads ?? [false, true, false], intros = o.intros ?? [kids(["Script0"]), kids(["Script0", "Script3"]), kids(["Script0"])];
  const ok = (v) => ({ status: 0, stdout: typeof v === "string" ? v : J(v), stderr: "" });
  const exec = (argv) => {
    calls.push(argv);
    const s = argv.join(" ");
    if (argv[0] === "cargo") {
      const p = JSON.parse(argv[argv.length - 1]);
      if (o.sendStatus) return { status: o.sendStatus, stdout: "", stderr: "boom" };
      if (p.op === "list") return ok(o.listRaw ?? { v: 1, request_id: p.request_id, ok: true, result: { ids: o.listIds ?? ["a", "b"] } });
      if (p.op === "get") return ok(o.getRaw ?? { v: 1, request_id: p.request_id, ok: true, result: { windows: (o.getIds ?? p.ids ?? ["a", "b"]).map((id) => ({ id, ok: true, snapshot: { id, pid: o.snapPid ?? ANN.pid, title: (o.snapTitle ?? {})[id] || title(id) } })) } });
      throw new Error(`unstubbed fixture op: ${p.op}`);
    }
    if (s.includes("GetNameOwner")) return ok({ type: "s", data: [st.own++ >= (o.driftAt ?? 1) && o.ownerAfter ? o.ownerAfter : ":1.7"] });
    if (s.includes("GetConnectionUnixProcessID")) return ok({ type: "u", data: [7] });
    if (s.includes("Introspect")) return ok({ type: "s", data: [intros[Math.min(st.ii++, intros.length - 1)]] });
    if (s.includes("isScriptLoaded")) return ok({ type: "b", data: [loads[Math.min(st.li++, loads.length - 1)]] });
    if (s.includes("unloadScript")) { st.unloads++; if (o.unloadStatus) return { status: o.unloadStatus, stdout: "", stderr: "boom" }; return ok(o.unload ?? { type: "b", data: [true] }); }
    if (s.includes("loadScript")) { if (o.loadStatus !== undefined) return { status: o.loadStatus, stdout: "", stderr: "boom" }; return ok({ type: "i", data: [o.scriptId ?? 3] }); }
    if (s.includes("org.kde.kwin.Script") && argv.at(-1) === "run") { st.runs++; if (o.runStatus) return { status: o.runStatus, stdout: "", stderr: "boom" }; return { status: 0, stdout: "", stderr: "" }; }
    if (argv[0] === "journalctl" && s.includes("--show-cursor")) return ok("x\n-- cursor: cur1\n");
    if (s.includes("--after-cursor")) { st.jn++; return ok(o.nativeJournal ?? natOk(NONCE, o.nativeWins ?? W2())); }
    throw new Error(`unstubbed exec: ${s}`);
  };
  const io = {
    exec, calls, st,
    readFile: (p) => {
      if (p.startsWith("/proc/")) { const n = st.proc++; return Buffer.from(PROC(n >= (o.tickDriftAt ?? 99) && o.tickAfter ? o.tickAfter : "12345")); }
      if (p === HPATH) { if (o.helperQt) return Buffer.from(`Qt.foo ${NONCE}`); if (o.helperNoNonce) return Buffer.from("no nonce"); return Buffer.from(`helper ${NONCE}`); }
      throw new Error(`unstubbed read: ${p}`);
    },
  };
  return { io, calls, st };
}
const baseOpts = () => ({ announcement: ANN, baselineKwin: BASE, nonce: NONCE, helperPath: HPATH });
test("happy join with exact receipts", () => {
  const { io, st, calls } = fake();
  const sink = {}, res = attribute(io, { ...baseOpts(), sink });
  assert.equal(res.joins.length, 2);
  assert.equal(res.joins.find((j) => j.id === "a").uuid, U1);
  assert.deepEqual(res.joins.find((j) => j.id === "a").frame, { x: 0, y: 0, w: 100, h: 100 });
  assert.equal(res.helper.scriptId, 3);
  assert.equal(res.helper.cursor, "cur1");
  assert.match(res.helper.sourceSha256, /^[0-9a-f]{64}$/);
  assert.equal(st.unloads, 1);
  assert.equal(calls.filter((c) => c.join(" ").includes("unloadScript")).length, 1);
  assert.ok(sink.raw.length > 5);
  assert.ok(!calls.some((c) => c.join(" ").includes("Scripting.start")));
  assert.equal(res.plugin, `omnitiler-audit-${NONCE}`);
});
test("missing/duplicate/ambiguous joins fail closed", () => {
  assert.throws(() => attribute(fake({ nativeWins: [W2()[1]] }).io, baseOpts()), AuditError);
  assert.throws(() => attribute(fake({ snapPid: 999 }).io, baseOpts()), AuditError);
  assert.throws(() => attribute(fake({ nativeWins: [W2()[0], nat({ internalId: U1, caption: title("b"), frame: { x: 200, y: 0, w: 100, h: 100 } })] }).io, baseOpts()), AuditError);
  assert.throws(() => attribute(fake({ nativeWins: [W2()[0], nat({ internalId: U3, caption: title("a"), frame: { x: 1, y: 1, w: 9, h: 9 } })] }).io, baseOpts()), AuditError);
  assert.throws(() => attribute(fake({ nativeWins: [...W2(), nat({ internalId: U3, caption: title("zzz"), frame: { x: 9, y: 9, w: 9, h: 9 } })] }).io, baseOpts()), AuditError);
});
test("full-roster duplicate UUID fails even for foreign rows", () => {
  const w = [...W2(), nat({ internalId: U1, resourceClass: "dolphin", caption: null, frame: { x: 9, y: 9, w: 9, h: 9 } })];
  assert.throws(() => attribute(fake({ nativeWins: w }).io, baseOpts()), /duplicate native UUID/);
  const v = [...W2(), nat({ internalId: U3, resourceClass: "dolphin", caption: null, frame: { x: 9, y: 9, w: 9, h: 9 } })];
  attribute(fake({ nativeWins: v }).io, { ...baseOpts(), sink: {} });
});
test("malformed native rows and frames fail closed", () => {
  for (const bad of [
    nat({ internalId: "nope" }),
    nat({ resourceClass: "dolphin", caption: "secret" }),
    nat({ pid: 0 }),
    nat({ frame: { x: 0, y: 0, w: 0, h: 100 } }),
    nat({ frame: { x: 0, y: 0, w: NaN, h: 100 } }),
    nat({ caption: null }),
  ]) assert.throws(() => attribute(fake({ nativeWins: [bad, W2()[1]] }).io, baseOpts()), AuditError);
  assert.throws(() => parseNativeJournal("not json\n", NONCE), AuditError);
  assert.throws(() => parseNativeJournal(J({ MESSAGE: "x" }) + "\n", NONCE), AuditError);
  assert.throws(() => parseNativeJournal(natJl(NONCE, ["ready:count=1", "done:count=0"]), NONCE), AuditError);
  const helperErr = natJl(NONCE, ["ready:count=1", "error:boom", "done:count=1"]);
  assert.throws(() => parseNativeJournal(helperErr, NONCE), /helper error/);
});
test("nonce mismatch and helper binding fail closed", () => {
  assert.throws(() => attribute(fake({ helperNoNonce: true }).io, baseOpts()), /nonce/);
  assert.throws(() => attribute(fake({ helperQt: true }).io, baseOpts()), /Qt/);
  const wrong = natOk("00".repeat(16), W2());
  assert.throws(() => attribute(fake({ nativeJournal: wrong }).io, baseOpts()), AuditError);
  const foreign = natOk(NONCE, W2()) + J({ MESSAGE: `omnitiler:attribution:${"00".repeat(16)}:ready:count=1` }) + "\n";
  assert.throws(() => parseNativeJournal(foreign, NONCE), /foreign-nonce/);
});
test("baseline/announcement/fixture envelope invalid fails before mutation", () => {
  assert.throws(() => attribute(fake().io, { ...baseOpts(), baselineKwin: { pid: 0, tick: "12345" } }), AuditError);
  assert.throws(() => attribute(fake().io, { ...baseOpts(), baselineKwin: { pid: 7, tick: "bad" } }), AuditError);
  assert.throws(() => attribute(fake().io, { ...baseOpts(), nonce: "short" }), AuditError);
  assert.throws(() => attribute(fake().io, { ...baseOpts(), announcement: { ...ANN, pid: 0 } }), AuditError);
  assert.throws(() => attribute(fake().io, { ...baseOpts(), announcement: { ...ANN, nonce: "bad" } }), AuditError);
  assert.throws(() => attribute(fake({ listRaw: { v: 1, request_id: "attr-list", ok: true, result: { ids: [] } } }).io, baseOpts()), AuditError);
  assert.throws(() => attribute(fake({ listRaw: { v: 1, request_id: "attr-list", ok: true, result: { ids: ["a", "a"] } } }).io, baseOpts()), /duplicate id/);
  assert.throws(() => attribute(fake({ snapTitle: { a: "WRONG" } }).io, baseOpts()), /sentinel/);
  const t = fake({ nativeWins: W2() });
  assert.throws(() => attribute(t.io, { ...baseOpts(), announcement: { ...ANN, socket: "" } }), AuditError);
  assert.ok(!t.calls.some((c) => c.join(" ").includes("loadScript")), "announcement invalid must precede load");
});
test("external failure and teardown fail closed exactly once", () => {
  const r = fake({ runStatus: 1 });
  assert.throws(() => attribute(r.io, baseOpts()), AuditError);
  assert.equal(r.st.unloads, 1);
  const u = fake({ unloadStatus: 1 });
  assert.throws(() => attribute(u.io, baseOpts()), /teardown failed/);
  assert.equal(u.st.unloads, 1);
  const c = fake({ intros: [kids(["Script0", "Script3"]), kids(["Script0", "Script3"]), kids(["Script0"])] });
  assert.throws(() => attribute(c.io, baseOpts()), AuditError);
  assert.equal(c.st.runs, 0);
  assert.equal(c.st.unloads, 1);
  const p = fake({ loads: [true, true, true] });
  assert.throws(() => attribute(p.io, baseOpts()), /already loaded/);
  assert.equal(p.st.runs, 0);
  const d = fake({ ownerAfter: ":1.99" });
  assert.throws(() => attribute(d.io, baseOpts()), /drift|identity/);
  const k = fake({ tickAfter: "99999", tickDriftAt: 4 });
  assert.throws(() => attribute(k.io, baseOpts()), /drift|identity/);
});
test("helper vm: fixture caption logged, foreign never; no Qt; nonce bound", () => {
  const src = readFileSync(path.join(THIS, "..", "helper", "native-roster-helper.js"), "utf8");
  assert.ok(src.includes("__ATTRIBUTION_NONCE__"));
  assert.ok(!/Qt\./.test(src));
  const logs = [];
  const mk = (cls, cap, uuid) => ({ internalId: { toString: () => uuid }, resourceClass: cls, caption: cap, pid: 111, frameGeometry: { x: 0, y: 0, width: 10, height: 10 } });
  const ctx = { workspace: { windowList: () => [mk("OmniTilerTestWindow", "OmniTilerTestWindow:a:rt99 suffix", U1), mk("dolphin", "SECRET-FOREIGN", U2)] }, console: { log: (m) => logs.push(String(m)) }, JSON, String, Math, Error, RegExp, Object, Array, Number, isFinite };
  vm.createContext(ctx);
  vm.runInContext(src.replaceAll("__ATTRIBUTION_NONCE__", NONCE), ctx);
  const bodies = logs.filter((l) => l.includes(`omnitiler:attribution:${NONCE}:`));
  assert.equal(bodies.length, 4);
  assert.equal(JSON.parse(bodies[1].slice(bodies[1].indexOf("window:") + 7)).caption, "OmniTilerTestWindow:a:rt99 suffix");
  assert.equal(JSON.parse(bodies[2].slice(bodies[2].indexOf("window:") + 7)).caption, null);
  for (const l of logs) assert.ok(!l.includes("SECRET-FOREIGN"));
  assert.equal(auditPlugin(NONCE), `omnitiler-audit-${NONCE}`);
});
test("subject uses only injected io; repo templates are test inputs", () => {
  const { io } = fake();
  const seen = new Set();
  const strict = { ...io, readFile: (p) => { seen.add(p); return io.readFile(p); } };
  attribute(strict, { ...baseOpts(), sink: {} });
  for (const p of [HPATH, `/proc/7/stat`]) assert.ok(seen.has(p), `subject must read ${p} via io`);
  assert.ok(![...seen].some((p) => String(p).includes("native-roster-helper")), "subject must not read the repo template");
});
test("orchestrate writes helper before load; failure keeps failure.json, no marker", () => {
  const order = [], o = () => ({ announcement: ANN, baselineKwin: BASE, nonce: NONCE, outDir: "/out" });
  const f = fake(), ex = f.io.exec;
  f.io.exec = (a) => { if (a.includes("loadScript") && !a.join(" ").includes("unloadScript")) order.push("load"); return ex(a); };
  const w = [], files = { template: "tpl __ATTRIBUTION_NONCE__ end", writeFile: (p, b) => { w.push(p); if (p.endsWith("helper.source.js")) { order.push("helper"); assert.ok(String(b).includes(NONCE)); } } };
  orchestrate(f.io, files, o());
  assert.deepEqual(order, ["helper", "load"]);
  assert.ok(w.some((p) => p.endsWith("attribution.complete")));
  const g = fake({ snapPid: 999 }), w2 = [];
  assert.throws(() => orchestrate(g.io, { template: files.template, writeFile: (p) => w2.push(p) }, o()), AuditError);
  assert.ok(w2.some((p) => p.endsWith("failure.json")));
  assert.ok(!w2.some((p) => p.endsWith("attribution.complete")));
});
test("no forbidden effect/probe/link/staging/compositor interfaces remain", () => {
  const src = readFileSync(path.join(THIS, "..", "attribution.mjs"), "utf8");
  for (const s of ["Effects", "load" + "Effect", "unload" + "Effect", "visibility", "probePlugin", "staged", "canon", "compositor", "environ", "XDG_DATA", "symlink", "readlink", "lstat", "unlink", "KPlugin", "kwin-wayland/effects"]) assert.ok(!src.includes(s), `forbidden interface remains: ${s}`);
  const { io, calls } = fake();
  assert.deepEqual(Object.keys(io).sort(), ["calls", "exec", "readFile", "st"]);
  attribute(io, { ...baseOpts(), sink: {} });
  for (const c of calls) {
    const s = c.join(" ");
    assert.ok(!s.includes("Effects") && !s.includes("Effect") || s.includes("isScriptLoaded") || s.includes("loadScript") || s.includes("unloadScript"), `forbidden bus call: ${s}`);
  }
});
