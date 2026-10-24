import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

interface FakeDesktop { id: string; name: string }
const desk = (id: string, name?: string): FakeDesktop => ({ id, name: name ?? id });

interface FakeSignal {
    readonly handlers: Array<(payload?: unknown) => void>;
    readonly signal: {
        connect: (handler: (payload?: unknown) => void) => void;
        disconnect: (handler: (payload?: unknown) => void) => void;
    };
}

function makeSignal(): FakeSignal {
    const handlers: Array<(payload?: unknown) => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: (payload?: unknown) => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: (payload?: unknown) => void): void => {
                const index = handlers.indexOf(handler);
                if (index >= 0) {
                    handlers.splice(index, 1);
                }
            },
        },
    };
}

function makeTwoOutput(opts: { throwOnce?: boolean } = {}) {
    const wa1 = desk("wa-1", "wa-1");
    const wb1 = desk("wb-1", "wb-1");
    const world = {
        screens: [
            { name: "out-primary", geometry: { x: 0, y: 0, width: 1920, height: 1080 } },
            { name: "out-secondary", geometry: { x: 1920, y: 0, width: 1920, height: 1080 } },
        ],
        desktops: [wa1, wb1] as FakeDesktop[],
        wins: [] as Array<Record<string, unknown>>,
        activeScreen: { name: "out-secondary" },
        currentByOut: new Map<string, FakeDesktop>([["out-primary", wa1], ["out-secondary", wb1]]),
        created: [] as string[],
        throwArmed: opts.throwOnce === true,
    };
    const calls: Array<{ method: string; payload: string; callback: (r: unknown) => void }> = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const signals: Record<string, FakeSignal> = {};
    for (const name of [
        "windowAdded",
        "windowRemoved",
        "windowActivated",
        "desktopsChanged",
        "screensChanged",
        "currentDesktopChanged",
        "activeScreenChanged",
    ] as const) {
        signals[name] = makeSignal();
    }
    const surface: Record<string, unknown> = {
        windowList: () => world.wins,
        screens: world.screens,
        get desktops() { return world.desktops; },
        activeScreen: world.activeScreen,
        activeWindow: null,
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        activeScreenChanged: (signals["activeScreenChanged"] as FakeSignal).signal,
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (out: { name: string }) => world.currentByOut.get(out.name),
        setCurrentDesktopForScreen: (d: FakeDesktop, out: { name: string }) => { world.currentByOut.set(out.name, d); },
        sendClientToScreen: () => undefined,
        createDesktop: (...args: ReadonlyArray<unknown>) => {
            if (world.throwArmed) {
                world.throwArmed = false;
                throw new Error("Insufficient arguments");
            }
            const name = args[1] as string;
            const id = `native-${world.desktops.length + 1}`;
            world.desktops.push(desk(id, name));
            world.created.push(id);
        },
        removeDesktop: () => undefined,
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, cb) => { calls.push({ method, payload, callback: cb }); },
        scheduleOnce: (delay, cb) => { timers.push({ delay, cb }); return () => undefined; },
        log: () => undefined,
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
    });
    assert.ok(handle !== null);
    const runTimers = () => { for (let i = 0; i < timers.length; i += 1) { const t = timers[i] as { delay: number; cb: () => void }; if (t.delay <= 120) { timers.splice(i, 1); i -= 1; t.cb(); } } };
    const lastCall = (m: string) => { const f = [...calls].reverse().find((c) => c.method === m); assert.ok(f !== undefined); return f as { method: string; payload: string; callback: (r: unknown) => void }; };
    const fire = (name: string, payload?: unknown): void => {
        for (const handler of [...(signals[name] as FakeSignal).handlers]) {
            handler(payload);
        }
    };
    return { world, calls, handle, runTimers, lastCall, fire };
}

const DISABLED_PRESENTATION = { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] };
const BOOT = (gen: string, req: string): string => JSON.stringify({ v: 2, adapter: "broker-1", gen, req, obs: "0", rev: "0", ready: false, consumed: false, scopes: [], workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null, gestureSettled: null, operationSettled: [], warpCursor: null, presentation: DISABLED_PRESENTATION });
const ACC = (gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string => JSON.stringify({ v: 2, adapter: "broker-1", gen, req: source, obs: rev, rev, ready: true, consumed: false, source, nativeInvalid, clock: "1", gestureSettled: null, operationSettled: [], warpCursor: null, presentation: DISABLED_PRESENTATION, scopes: [], workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null, ...extra });

function discover(h: ReturnType<typeof makeTwoOutput>, gen = "gen-1"): void {
    const c = h.lastCall("DiscoverWorld");
    c.callback(BOOT(gen, (JSON.parse(c.payload) as { req: string }).req));
    const auto = h.lastCall("DescribeWorld");
    const b = JSON.parse(auto.payload) as { source: string; nativeInvalid: string };
    auto.callback(ACC(gen, b.source, b.nativeInvalid, "0"));
}

// V2 publication oracle: adapter-owned pending bindings publish as the
// logical view per output (the inner report carries no bindings lane).
function viewsOf(h: ReturnType<typeof makeTwoOutput>): Array<{ out: string; view: string }> {
    return (JSON.parse(h.lastCall("DescribeWorld").payload) as { report: { views: Array<{ out: string; view: string }> } }).report.views;
}

function dispatchObserve(h: ReturnType<typeof makeTwoOutput>): { source: string; nativeInvalid: string } {
    // Signal-driven observation round (production path): a native inventory
    // event invalidates and the debounce dispatches a fresh capture. Press
    // intents queue instead of dispatching, so they cannot drive the round.
    h.fire("desktopsChanged");
    h.runTimers();
    return JSON.parse(h.lastCall("DescribeWorld").payload) as { source: string; nativeInvalid: string };
}

// Show a native desktop on an output (ordinary native view input): pending
// bindings publish through the views lane only for the current native, so
// observing publication requires the created desktop to be shown.
function showNative(h: ReturnType<typeof makeTwoOutput>, out: string, nativeId: string): void {
    const ref = h.world.desktops.find((d) => d.id === nativeId);
    assert.ok(ref !== undefined, `native ${nativeId} exists`);
    h.world.currentByOut.set(out, ref);
}

describe("creation binding handoff", () => {
    it("publishes locally-created native as logical token on next report", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 1);
        const native = h.world.desktops.find((d) => d.id === h.world.created[0]);
        assert.ok(native !== undefined);
        assert.equal(native.name, "Desktop 3", "proven desktop renamed to its native one-based position");
        showNative(h, "out-secondary", native.id);
        dispatchObserve(h);
        const row = viewsOf(h).find((b) => b.view === "wl-1");
        assert.ok(row !== undefined, `expected wl-1 view, got ${JSON.stringify(viewsOf(h))}`);
        assert.equal(row.out, "out-secondary");
        h.handle?.stop();
    });

    it("accepted echo prunes pending: no repeats and later migration stays core-authoritative", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        const native = h.world.desktops.find((d) => d.id === h.world.created[0]);
        assert.ok(native !== undefined);
        assert.equal(native.name, "Desktop 3", "proven desktop renamed to its native one-based position");
        // Generic fallback echo must still be corrected by pending.
        const r2 = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.source, r2.nativeInvalid, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: native.id, scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 1, "fallback echo must not re-create");
        showNative(h, "out-secondary", (h.world.desktops.find((d) => d.id === h.world.created[0]) as FakeDesktop).id);
        dispatchObserve(h);
        const corrected = viewsOf(h).find((b) => b.out === "out-secondary");
        assert.ok(corrected !== undefined);
        assert.equal(corrected.view, "wl-1", "pending corrects generic fallback");
        // Real ack consumes pending: no second creation.
        const r3 = JSON.parse(h.lastCall("DescribeWorld").payload) as { source: string; nativeInvalid: string };
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r3.source, r3.nativeInvalid, "3", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: native.id, scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 1);
        dispatchObserve(h);
        const acked = viewsOf(h).find((b) => b.out === "out-secondary");
        assert.ok(acked !== undefined);
        assert.equal(acked.view, "wl-1");
        assert.equal(acked.out, "out-secondary");
        // Later legitimate migration of the same native to a new output wins.
        const r4 = JSON.parse(h.lastCall("DescribeWorld").payload) as { source: string; nativeInvalid: string };
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r4.source, r4.nativeInvalid, "4", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-primary", bind: native.id, scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        showNative(h, "out-primary", native.id);
        dispatchObserve(h);
        const migrated = viewsOf(h).find((b) => b.out === "out-primary");
        assert.ok(migrated !== undefined);
        assert.equal(migrated.view, "wl-1");
        assert.equal(migrated.out, "out-primary", "core migration must override stale pending out");
        assert.equal(h.world.created.length, 1, "migration must not re-create");
        h.handle?.stop();
    });

    it("delayed named recovery publishes without duplicate creation", () => {
        const h = makeTwoOutput({ throwOnce: true });
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        // Throw left the native uncreated; nothing bound yet.
        assert.equal(h.world.created.length, 0);
        // Delayed native appearance with the exact requested name.
        h.world.desktops.push(desk("native-9", "wl-1"));
        const r2 = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.source, r2.nativeInvalid, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 0, "delayed recovery must not append");
        assert.equal(
            h.world.desktops.find((d) => d.id === "native-9")?.name,
            "Desktop 3",
            "delayed proof renames to its enumerated position",
        );
        showNative(h, "out-secondary", "native-9");
        dispatchObserve(h);
        const row = viewsOf(h).find((b) => b.view === "wl-1");
        assert.ok(row !== undefined, `delayed recovery should publish, got ${JSON.stringify(viewsOf(h))}`);
        assert.equal(row.out, "out-secondary");
        h.handle?.stop();
    });

    it("two tokens on two outputs publish independently with no shared native", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.source, req.nativeInvalid, "1", {
            workspaces: [
                { id: "wl-1", out: "out-secondary", bind: "", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
                { id: "wl-2", out: "out-primary", bind: "", scope: "out", order: 3, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 2);
        const n1 = h.world.desktops.find((d) => d.id === h.world.created[0]);
        const n2 = h.world.desktops.find((d) => d.id === h.world.created[1]);
        assert.ok(n1 !== undefined && n2 !== undefined);
        assert.notEqual(n1.id, n2.id, "no duplicate native binding");
        assert.equal(n1.name, "Desktop 3");
        assert.equal(n2.name, "Desktop 4", "second token uses its own enumerated position");
        showNative(h, "out-secondary", n1.id);
        showNative(h, "out-primary", n2.id);
        dispatchObserve(h);
        const rows = viewsOf(h);
        const r1 = rows.find((b) => b.view === "wl-1");
        const r2 = rows.find((b) => b.view === "wl-2");
        assert.ok(r1 !== undefined && r2 !== undefined);
        assert.equal(r1.out, "out-secondary");
        assert.equal(r2.out, "out-primary");
        // Ack only wl-1: wl-2 pending must survive (no map-wide clear).
        const ack = JSON.parse(h.lastCall("DescribeWorld").payload) as { source: string; nativeInvalid: string };
        h.lastCall("DescribeWorld").callback(ACC("gen-1", ack.source, ack.nativeInvalid, "2", {
            workspaces: [
                { id: "wl-1", out: "out-secondary", bind: n1.id, scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
                { id: "wl-2", out: "out-primary", bind: n2.id, scope: "out", order: 3, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        dispatchObserve(h);
        const after = viewsOf(h);
        assert.equal(after.find((b) => b.view === "wl-1")?.out, "out-secondary");
        assert.equal(after.find((b) => b.view === "wl-2")?.out, "out-primary");
        assert.equal(h.world.created.length, 2);
        h.handle?.stop();
    });
});
