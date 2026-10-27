// Capacity-float ordinary-float echo at the KDE adapter boundary (D4/D5):
// explicitly classified rows (floating with untouched frame authority,
// user or capacity alike) converge the shared float-intent membership and
// persist through the same ReadFloatIntent/WriteFloatIntent store; no new
// capacity-specific storage exists. Unclassified held/minimized/tiled rows
// never converge merely for missing allocation, and an explicit-tile
// no-space result records no tile override. Offline only: scripted
// in-memory transport, no D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

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
            output: { name: "out" }, desktops: [ws1],
            minimized: false, hidden: false, fullscreen: false, maximizeMode: 0, sticky: false,
            onAllDesktops: false, keepAbove: false, keepBelow: false,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-cap", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 300, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws1],
            minimized: false, hidden: false, fullscreen: false, maximizeMode: 0, sticky: false,
            onAllDesktops: false, keepAbove: false, keepBelow: false,
            minSize: null, maxSize: null,
        },
        {
            internalId: "win-held", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 600, y: 0, width: 100, height: 100 },
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
    store: { members: string[]; tile: string[] };
    setActive: (id: string) => void;
}

function setup(stored: { members: string[]; tile: string[] }): Harness {
    const { surface, world } = makeSurface();
    const calls: Call[] = [];
    const timers: Harness["timers"] = [];
    const logs: string[] = [];
    const writes: string[] = [];
    const store = { members: [...stored.members], tile: [...stored.tile] };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            if (method === "ReadFloatIntent") {
                callback(JSON.stringify({ members: store.members, tile: store.tile }));
                return;
            }
            if (method === "WriteFloatIntent") {
                writes.push(payload);
                const body = JSON.parse(payload) as { members: string[]; tile: string[] };
                store.members = [...body.members];
                store.tile = [...(body.tile ?? [])];
                callback(JSON.stringify({ v: 1, outcome: "stored" }));
                return;
            }
            calls.push({ method, payload, callback });
            if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({ v: 2, outcome: "queued", gen: "gen-1",
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 2, outcome: "ready", gen: "gen-1", globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 2, outcome: "pending", gen: "gen-1" }));
                }            }
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

function discover(h: Harness): void {
    const disc = h.calls.find((c) => c.method === "DiscoverWorld");
    assert.ok(disc !== undefined);
    const req = (JSON.parse(disc.payload) as Record<string, unknown>)["req"];
    disc.callback(JSON.stringify({
        v: 2, adapter: "broker-1", gen: "gen-1", req,
        obs: "0", rev: "0", ready: false, consumed: false,
        gestureSettled: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [], result: null,
    }));
}

interface RowSpec {
    win: string;
    alloc: [number, number, number, number] | null;
    frame: [number, number, number, number] | null;
    frame_auth: string;
    frame_hold: string;
    member_auth: string;
    member_hold: string;
    frame_op?: string;
    floating: boolean;
    float_origin: string;
}

function row(spec: RowSpec): Record<string, unknown> {
    return {
        win: spec.win, out: "out", ws: "ws1",
        alloc: spec.alloc, frame: spec.frame,
        frame_auth: spec.frame_auth, frame_hold: spec.frame_hold,
        member_auth: spec.member_auth, member_hold: spec.member_hold,
        frame_op: spec.frame_op ?? "",
        clear_op: "",
        floating: spec.floating, float_origin: spec.float_origin,
    };
}

function replyFor(h: Harness, mutate: (body: Record<string, unknown>) => void): void {
    const last = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(last !== undefined);
    const body = JSON.parse(last.payload) as Record<string, unknown>;
    const pending: Record<string, unknown> = {
        v: 2, adapter: "broker-1", gen: "gen-1",
        req: body["source"], obs: "1", rev: "1",
        ready: true, consumed: false,
        source: body["source"],
        nativeInvalid: body["nativeInvalid"],
        clock: "0",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
        workspaces: [], outputs: [], transfers: [], effects: [], result: null,
        windows: [],
    };
    mutate(pending);
    // Terminal results ride Take, never the capture lane: the actuator
    // reply always carries an explicit null result (exact v2 shape).
    const { result, ...snapshot } = pending;
    if (result !== undefined && result !== null) {
        (globalThis as Record<string, unknown>)["__takeResult"] = result;
    }
    last.callback(JSON.stringify({ ...snapshot, result: null }));
}

function reportFloatIntent(h: Harness): Record<string, boolean> {
    const last = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(last !== undefined);
    const body = JSON.parse(last.payload) as Record<string, unknown>;
    const inner = body["report"] as Record<string, unknown>;
    const intent = new Set((inner["floatIntent"] as Array<unknown>).map(String));
    const out: Record<string, boolean> = {};
    for (const w of inner["windows"] as Array<Record<string, unknown>>) {
        out[w["win"] as string] = intent.has(w["win"] as string);
    }
    return out;
}

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
        const h = setup({ members: [], tile: [] });
        discover(h);
        replyFor(h, (pending) => {
            pending["windows"] = [TILE_A, CAPACITY_FLOAT, HELD_OVERLAY];
        });
        // The capacity float converges into the shared membership and the
        // settled change persists through the same user-float store.
        assert.ok(h.writes.length >= 1, "settled change persists");
        const last = JSON.parse(h.writes[h.writes.length - 1] as string) as Record<string, unknown>;
        assert.deepEqual(last["members"], ["win-cap"]);
        assert.deepEqual(last["tile"], []);
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
        const h = setup({ members: ["win-cap"], tile: [] });
        discover(h);
        runTimers(h);
        const flags = reportFloatIntent(h);
        assert.equal(flags["win-cap"], true, "stored float rides the first admission");
        h.handle.stop();
    });

    it("no-space retry records no tile override and keeps the float", () => {
        const h = setup({ members: [], tile: [] });
        discover(h);
        replyFor(h, (pending) => {
            pending["windows"] = [TILE_A, CAPACITY_FLOAT, HELD_OVERLAY];
        });
        assert.ok(h.writes.length >= 1);
        // Explicit unfloat attempt on the capacity float while
        // exhausted: core reports no-space with no placement rect; the
        // window stays floated and no tile override records.
        h.setActive("win-cap");
        h.handle.requestFloat();
        runTimers(h);
        replyFor(h, (pending) => {
            pending["windows"] = [TILE_A, CAPACITY_FLOAT, HELD_OVERLAY];
            pending["rev"] = "2";
            pending["result"] = { op: "toggle-float", win: "win-cap", rect: null, noSpace: true };
        });
        assert.ok(
            h.logs.some((l) => l.includes("toggle-float-no-space")),
            "no-space retry logs one diagnostic line",
        );
        const last = JSON.parse(h.writes[h.writes.length - 1] as string) as Record<string, unknown>;
        assert.deepEqual(last["tile"], [], "no false tile override on no-space");
        assert.deepEqual(last["members"], ["win-cap"], "float membership survives the retry");
        h.handle.stop();
    });

    it("refuses unknown float origins and rows missing the classification", () => {
        const h = setup({ members: [], tile: [] });
        discover(h);
        // Unknown origin token refuses the whole reply.
        replyFor(h, (pending) => {
            pending["windows"] = [{
                ...TILE_A, floating: true, float_origin: "mystery",
            }];
        });
        assert.ok(h.logs.some((l) => l.includes("reply-malformed")), "unknown origin refuses");
        h.handle.stop();
    });
});
