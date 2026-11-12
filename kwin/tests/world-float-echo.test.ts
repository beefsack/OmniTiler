// Capacity-float ordinary-float echo at the KDE adapter boundary (D4/D5):
// the shared opaque intent store carries the converged float membership as
// byte-exact schema-2 records; only accepted core `intentWrites` update
// persistence, never inferred floating state. Unclassified held/minimized/
// tiled rows never converge, and an explicit-tile no-space result records
// no delete. Offline only: scripted in-memory transport, no D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface FakeDesktop {
    id: string;
}

const desk = (id: string): FakeDesktop => ({ id });

function makeSurface() {
    const ws1 = desk("ws1");
    const wins = [
        {
            internalId: "win-tiled", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            // Truthful undecorated client rect: the fake carries no
            // decoration, so the client mirrors the frame exactly.
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws1],
            minimized: false, hidden: false, fullscreen: false, maximizeMode: 0, sticky: false,
            onAllDesktops: false, keepAbove: false, keepBelow: false,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-cap", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 300, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 300, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws1],
            minimized: false, hidden: false, fullscreen: false, maximizeMode: 0, sticky: false,
            onAllDesktops: false, keepAbove: false, keepBelow: false,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-held", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 600, y: 0, width: 100, height: 100 },
            clientGeometry: { x: 600, y: 0, width: 100, height: 100 },
            output: { name: "out" }, desktops: [ws1],
            minimized: false, hidden: false, fullscreen: true, maximizeMode: 0, sticky: false,
            onAllDesktops: false, keepAbove: false, keepBelow: false,
            minSize: null, maxSize: null,
        },
    ];
    const world = {
        wins,
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1],
        activeScreen: { name: "out" },
        activeWindow: wins[0],
    };
    const surface = {
        windowList: () => world.wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            world.activeWindow = ref as typeof world.activeWindow;
        },
        get currentDesktop() {
            return ws1;
        },
        set currentDesktop(_ref: unknown) {
            void _ref;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => ws1,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface, world };
}

interface Call {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

interface Harness {
    handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    calls: Call[];
    timers: Array<{ delay: number; cb: () => void }>;
    logs: string[];
    writes: string[];
    store: { records: Array<{ win: string; data: string }> };
    setActive: (id: string) => void;
}

function setup(stored: Array<{ win: string; data: string }>): Harness {
    const { surface, world } = makeSurface();
    const calls: Call[] = [];
    const timers: Harness["timers"] = [];
    const logs: string[] = [];
    const writes: string[] = [];
    const store = { records: stored.map((r) => ({ ...r })) };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            if (method === "ReadFloatIntent" || method === "WriteFloatIntent") {
                const body = JSON.parse(payload) as Record<string, unknown>;
                const corr = body["correlation_id"] as string;
                if (method === "ReadFloatIntent") {
                    // Opaque schema-2 seed: exact counts, byte-exact records.
                    callback(JSON.stringify({
                        v: 2, correlation_id: corr, outcome: "ok",
                        records: store.records,
                        stored: store.records.length, returned: store.records.length,
                    }));
                    return;
                }
                writes.push(payload);
                store.records = (body["records"] as Array<{ win: string; data: string }>).map((r) => ({ ...r }));
                callback(JSON.stringify({ v: 2, correlation_id: corr, outcome: "stored", stored: store.records.length }));
                return;
            }
            calls.push({ method, payload, callback });
            if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                const queueGen = typeof queueBody["gen"] === "string" ? (queueBody["gen"] as string) : "gen-1";
                callback(JSON.stringify({ v: 4, outcome: "queued", gen: queueGen,
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"],
                    globalCmd: String(calls.filter((q) => q.method === "QueueWorldCommand").length) }));
            } else if (method === "TakeCommandResult") {
                const takeBody = JSON.parse(payload) as Record<string, unknown>;
                const takeGen = typeof takeBody["gen"] === "string" ? (takeBody["gen"] as string) : "gen-1";
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 4, outcome: "ready", gen: takeGen, globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 4, outcome: "pending", gen: takeGen }));
                }
            }
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (m) => { logs.push(m); },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        floatIntent: true as const,
    });
    assert.ok(handle !== null);
    const setActive = (id: string): void => {
        const found = world.wins.find((w) => w.internalId === id);
        assert.ok(found !== undefined);
        world.activeWindow = found;
    };
    return { handle: handle as NonNullable<typeof handle>, calls, timers, logs, writes, store, setActive };
}

function runTimers(h: Harness): void {
    // Debounce only (120ms): never fire the 2000ms flight timeouts, or
    // the reply below would land after the timeout settled the flight.
    for (let i = 0; i < h.timers.length; i += 1) {
        const t = h.timers[i] as { delay: number; cb: () => void };
        if (t.delay <= 120) {
            h.timers.splice(i, 1);
            i -= 1;
            t.cb();
        }
    }
}

function answerHandshake(h: Harness, gen = "gen-1"): void {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined, "expected DiscoverWorld/handshake");
    const req = (JSON.parse((found as Call).payload) as Record<string, unknown>)["req"] as string;
    (found as Call).callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(h: Harness, gen = "gen-1"): void {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined, "expected DiscoverWorld/settings");
    const body = JSON.parse((found as Call).payload) as Record<string, unknown>;
    (found as Call).callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

function discover(h: Harness): void {
    // Genuine v4 connection bootstrap: the handshake reply echoes the
    // producer adapter lifetime and mints the serving generation; the
    // revisioned settings snapshot must read accepted before the first
    // report flies. The opaque intent seed answers synchronously from the
    // scripted store, so the pump below lands the first capture.
    answerHandshake(h);
    answerSettings(h);
    runTimers(h);
}

interface RowSpec {
    win: string;
    alloc: [number, number, number, number] | null;
    frame: [number, number, number, number] | null;
    frame_auth: string;
    frame_hold: string;
    member_auth: string;
    member_hold: string;
    floating: boolean;
    float_origin: string;
}

// Genuine 18-key v4 window row (retired 13-key shape refuses).
function row(spec: RowSpec): Record<string, unknown> {
    return {
        win: spec.win, out: "out", ws: "ws1",
        alloc: spec.alloc, frame: spec.frame,
        frame_auth: spec.frame_auth, frame_hold: spec.frame_hold,
        member_auth: spec.member_auth, member_hold: spec.member_hold,
        frame_op: "", floating: spec.floating, float_origin: spec.float_origin,
        clear_op: "",
        cmdSeq: null,
        visibility: null, visibility_auth: "untouched", visibility_hold: "",
        state: [],
    };
}

function replyFor(h: Harness, mutate: (body: Record<string, unknown>) => void, rev = "1"): void {
    const last = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(last !== undefined);
    const body = JSON.parse(last.payload) as Record<string, unknown>;
    // STRICT V4 SYNTHETIC accept (hand-authored inline, never real-core
    // output): the producer adapter lifetime, the serving generation, the
    // capture source correlation and native watermark. This scripted
    // harness advances broker obs and revision together; terminal results
    // stay requester-only on the take lane.
    const pending: Record<string, unknown> = {
        v: 4, adapter: "test-adapter", gen: "gen-1",
        req: body["source"], obs: rev, rev,
        ready: true, consumed: false,
        source: body["source"],
        nativeInvalid: body["nativeInvalid"],
        clock: "1",
        gestureSettled: null, gestureAssoc: null,
        focusRequest: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: [], outputs: [], transfers: [], effects: [], result: null,
        windows: [],
    };
    mutate(pending);
    // Terminal results ride Take, never the capture lane: strip the
    // requester-only result first, then validate the actual sent bytes
    // (never the pre-projection pending object).
    const { result, ...snapshot } = pending;
    if (result !== undefined && result !== null) {
        (globalThis as Record<string, unknown>)["__takeResult"] = result;
    }
    const text = JSON.stringify({ ...snapshot, result: null });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    last.callback(text);
}

function reportFloatIntent(h: Harness): Record<string, boolean> {
    const last = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(last !== undefined);
    const body = JSON.parse(last.payload) as Record<string, unknown>;
    const inner = body["report"] as Record<string, unknown>;
    // The converged membership echoes as byte-exact opaque intent records
    // live-joined to the captured native set.
    const records = inner["intentRecords"] as Array<{ win: string; data: string }>;
    const intent = new Set(records.map((r) => String(r.win)));
    const out: Record<string, boolean> = {};
    for (const w of inner["windows"] as Array<Record<string, unknown>>) {
        out[w["win"] as string] = intent.has(w["win"] as string);
    }
    return out;
}

// Explicitly synthetic opaque sentinel (never genuine core-minted
// bytes): stored verbatim and echoed live-joined, never interpreted
// adapter-side.
const DATA_CAP = "cap-float";

const TILE_A = row({
    win: "win-tiled", alloc: [0, 0, 400, 600], frame: [0, 0, 400, 600],
    frame_auth: "controlled", frame_hold: "",
    member_auth: "controlled", member_hold: "",
    floating: false, float_origin: "",
});

const CAPACITY_FLOAT = row({
    win: "win-cap", alloc: null, frame: null,
    frame_auth: "untouched", frame_hold: "",
    member_auth: "untouched", member_hold: "",
    floating: true, float_origin: "capacity",
});

// Unclassified held overlay: no allocation, but never a float.
const HELD_OVERLAY = row({
    win: "win-held", alloc: null, frame: null,
    frame_auth: "hold", frame_hold: "unreadable",
    member_auth: "hold", member_hold: "unreadable",
    floating: false, float_origin: "",
});

describe("world capacity-float echo", () => {
    it("converges explicitly classified rows and persists them in the shared store", () => {
        const h = setup([]);
        discover(h);
        replyFor(h, (pending) => {
            pending["windows"] = [TILE_A, CAPACITY_FLOAT, HELD_OVERLAY];
            // Explicit core decision: the capacity float converges into the
            // shared opaque membership. Nothing is inferred adapter-side
            // from the classified row alone.
            pending["intentWrites"] = [{ win: "win-cap", data: DATA_CAP }];
        });
        // The settled change persists through the same opaque store.
        assert.ok(h.writes.length >= 1, "settled change persists");
        const last = JSON.parse(h.writes[h.writes.length - 1] as string) as Record<string, unknown>;
        assert.deepEqual(last["records"], [{ win: "win-cap", data: DATA_CAP }]);
        // The next report echoes the converged membership back to core.
        h.handle.requestFloat();
        runTimers(h);
        const flags = reportFloatIntent(h);
        assert.equal(flags["win-cap"], true, "capacity float echoes floating");
        assert.equal(flags["win-tiled"], false, "tile echoes tiled");
        assert.equal(flags["win-held"], false, "held overlay never echoes floating");
        h.handle.stop();
    });

    it("seeds restart membership from the store before first dispatch", () => {
        const h = setup([{ win: "win-cap", data: DATA_CAP }]);
        discover(h);
        runTimers(h);
        const flags = reportFloatIntent(h);
        assert.equal(flags["win-cap"], true, "stored float rides the first admission");
        h.handle.stop();
    });

    it("no-space retry records no tile override and keeps the float", () => {
        const h = setup([]);
        discover(h);
        replyFor(h, (pending) => {
            pending["windows"] = [TILE_A, CAPACITY_FLOAT, HELD_OVERLAY];
            pending["intentWrites"] = [{ win: "win-cap", data: DATA_CAP }];
        });
        assert.ok(h.writes.length >= 1);
        // Explicit unfloat attempt on the capacity float while
        // exhausted: core reports no-space with no placement rect; the
        // window stays floated and no delete records.
        h.setActive("win-cap");
        h.handle.requestFloat();
        runTimers(h);
        replyFor(h, (pending) => {
            pending["windows"] = [TILE_A, CAPACITY_FLOAT, HELD_OVERLAY];
            pending["intentWrites"] = [];
            pending["result"] = { op: "toggle-float", win: "win-cap", rect: null, noSpace: true };
        }, "2");
        assert.ok(
            h.logs.some((l) => l.includes("toggle-float-no-space")),
            "no-space retry logs one diagnostic line",
        );
        const last = JSON.parse(h.writes[h.writes.length - 1] as string) as Record<string, unknown>;
        assert.deepEqual(last["records"], [{ win: "win-cap", data: DATA_CAP }], "no false delete on no-space");
        h.handle.stop();
    });

    it("refuses unknown float origins and rows missing the classification", () => {
        const h = setup([]);
        discover(h);
        // Unknown origin token refuses the whole reply.
        const last = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(last !== undefined);
        const body = JSON.parse(last.payload) as Record<string, unknown>;
        const pending: Record<string, unknown> = {
            v: 4, adapter: "test-adapter", gen: "gen-1",
            req: body["source"], obs: "1", rev: "1",
            ready: true, consumed: false,
            source: body["source"],
            nativeInvalid: body["nativeInvalid"],
            clock: "1",
            gestureSettled: null, gestureAssoc: null,
            focusRequest: null,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            intentWrites: [], switchSettled: [], diagnostics: [],
            presentation: DISABLED_PRESENTATION,
            scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
            workspaces: [], outputs: [], transfers: [], effects: [], result: null,
            windows: [{
                ...TILE_A, floating: true, float_origin: "mystery",
            }],
        };
        assert.equal(parseWorldReply(JSON.stringify(pending)), null, "unknown origin refuses at the parser");
        last.callback(JSON.stringify(pending));
        assert.ok(h.logs.some((l) => l.includes("reply-malformed")), "unknown origin refuses");
        assert.equal(h.writes.length, 0, "refused reply persists nothing");
        h.handle.stop();
    });
});
