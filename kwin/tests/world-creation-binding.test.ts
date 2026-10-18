import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

interface FakeDesktop { id: string; name: string }
const desk = (id: string, name?: string): FakeDesktop => ({ id, name: name ?? id });

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
    const surface: Record<string, unknown> = {
        windowList: () => world.wins,
        screens: world.screens,
        get desktops() { return world.desktops; },
        activeScreen: world.activeScreen,
        activeWindow: null,
        windowAdded: { connect: () => undefined, disconnect: () => undefined },
        windowRemoved: { connect: () => undefined, disconnect: () => undefined },
        windowActivated: { connect: () => undefined, disconnect: () => undefined },
        desktopsChanged: { connect: () => undefined, disconnect: () => undefined },
        screensChanged: { connect: () => undefined, disconnect: () => undefined },
        currentDesktopChanged: { connect: () => undefined, disconnect: () => undefined },
        activeScreenChanged: { connect: () => undefined, disconnect: () => undefined },
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
    return { world, calls, handle, runTimers, lastCall };
}

const BOOT = (gen: string, req: string): string => JSON.stringify({ v: 1, adapter: "test-adapter", gen, req, obs: "0", rev: "0", ready: false, consumed: false, scopes: [], workspaces: [], outputs: [], windows: [], transfers: [], effects: [] });
const ACC = (gen: string, req: string, obs: string, rev: string, extra: Record<string, unknown> = {}): string => JSON.stringify({ v: 1, adapter: "test-adapter", gen, req, obs, rev, ready: true, consumed: false, scopes: [], workspaces: [], outputs: [], windows: [], transfers: [], effects: [], ...extra });

function discover(h: ReturnType<typeof makeTwoOutput>, gen = "gen-1"): void {
    const c = h.lastCall("DiscoverWorld");
    c.callback(BOOT(gen, (JSON.parse(c.payload) as { req: string }).req));
    const auto = h.lastCall("DescribeWorld");
    const b = JSON.parse(auto.payload) as { req: string; obs: string };
    auto.callback(ACC(gen, b.req, b.obs, "0"));
}

function bindingsOf(h: ReturnType<typeof makeTwoOutput>): Array<{ logical: string; native: string; out: string }> {
    return (JSON.parse(h.lastCall("DescribeWorld").payload) as { bindings: Array<{ logical: string; native: string; out: string }> }).bindings;
}

function dispatchObserve(h: ReturnType<typeof makeTwoOutput>): { req: string; obs: string } {
    h.handle?.requestWorkspaceRelative(1);
    h.runTimers();
    return JSON.parse(h.lastCall("DescribeWorld").payload) as { req: string; obs: string };
}

describe("creation binding handoff", () => {
    it("publishes locally-created native as logical token on next report", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.req, req.obs, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", logical: "wl-1", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 1);
        const native = h.world.desktops.find((d) => d.id === h.world.created[0]);
        assert.ok(native !== undefined);
        assert.equal(native.name, "Desktop 3", "proven desktop renamed to its native one-based position");
        dispatchObserve(h);
        const row = bindingsOf(h).find((b) => b.logical === "wl-1");
        assert.ok(row !== undefined, `expected wl-1 binding, got ${JSON.stringify(bindingsOf(h))}`);
        assert.equal(row.native, native.id);
        assert.equal(row.out, "out-secondary");
        h.handle?.stop();
    });

    it("accepted echo prunes pending: no repeats and later migration stays core-authoritative", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.req, req.obs, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", logical: "wl-1", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        const native = h.world.desktops.find((d) => d.id === h.world.created[0]);
        assert.ok(native !== undefined);
        assert.equal(native.name, "Desktop 3", "proven desktop renamed to its native one-based position");
        // Generic fallback echo must still be corrected by pending.
        const r2 = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.req, r2.obs, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: native.id, logical: "", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 1, "fallback echo must not re-create");
        dispatchObserve(h);
        const corrected = bindingsOf(h).find((b) => b.native === native.id);
        assert.ok(corrected !== undefined);
        assert.equal(corrected.logical, "wl-1", "pending corrects generic fallback");
        assert.equal(corrected.out, "out-secondary");
        // Real ack consumes pending: no second creation.
        const r3 = JSON.parse(h.lastCall("DescribeWorld").payload) as { req: string; obs: string };
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r3.req, r3.obs, "3", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: native.id, logical: "wl-1", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 1);
        dispatchObserve(h);
        const acked = bindingsOf(h).find((b) => b.native === native.id);
        assert.ok(acked !== undefined);
        assert.equal(acked.logical, "wl-1");
        assert.equal(acked.out, "out-secondary");
        // Later legitimate migration of the same native to a new output wins.
        const r4 = JSON.parse(h.lastCall("DescribeWorld").payload) as { req: string; obs: string };
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r4.req, r4.obs, "4", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-primary", bind: native.id, logical: "wl-1", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        dispatchObserve(h);
        const migrated = bindingsOf(h).find((b) => b.native === native.id);
        assert.ok(migrated !== undefined);
        assert.equal(migrated.logical, "wl-1");
        assert.equal(migrated.out, "out-primary", "core migration must override stale pending out");
        assert.equal(h.world.created.length, 1, "migration must not re-create");
        h.handle?.stop();
    });

    it("delayed named recovery publishes without duplicate creation", () => {
        const h = makeTwoOutput({ throwOnce: true });
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.req, req.obs, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", logical: "wl-1", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        // Throw left the native uncreated; nothing bound yet.
        assert.equal(h.world.created.length, 0);
        // Delayed native appearance with the exact requested name.
        h.world.desktops.push(desk("native-9", "wl-1"));
        const r2 = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.req, r2.obs, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "wl-1", out: "out-secondary", bind: "", logical: "wl-1", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 0, "delayed recovery must not append");
        assert.equal(
            h.world.desktops.find((d) => d.id === "native-9")?.name,
            "Desktop 3",
            "delayed proof renames to its enumerated position",
        );
        dispatchObserve(h);
        const row = bindingsOf(h).find((b) => b.logical === "wl-1");
        assert.ok(row !== undefined, `delayed recovery should publish, got ${JSON.stringify(bindingsOf(h))}`);
        assert.equal(row.native, "native-9");
        assert.equal(row.out, "out-secondary");
        h.handle?.stop();
    });

    it("two tokens on two outputs publish independently with no shared native", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.req, req.obs, "1", {
            workspaces: [
                { id: "wl-1", out: "out-secondary", bind: "", logical: "wl-1", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
                { id: "wl-2", out: "out-primary", bind: "", logical: "wl-2", scope: "out", order: 3, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        assert.equal(h.world.created.length, 2);
        const n1 = h.world.desktops.find((d) => d.id === h.world.created[0]);
        const n2 = h.world.desktops.find((d) => d.id === h.world.created[1]);
        assert.ok(n1 !== undefined && n2 !== undefined);
        assert.notEqual(n1.id, n2.id, "no duplicate native binding");
        assert.equal(n1.name, "Desktop 3");
        assert.equal(n2.name, "Desktop 4", "second token uses its own enumerated position");
        dispatchObserve(h);
        const rows = bindingsOf(h);
        const r1 = rows.find((b) => b.logical === "wl-1");
        const r2 = rows.find((b) => b.logical === "wl-2");
        assert.ok(r1 !== undefined && r2 !== undefined);
        assert.equal(r1.native, n1.id);
        assert.equal(r1.out, "out-secondary");
        assert.equal(r2.native, n2.id);
        assert.equal(r2.out, "out-primary");
        assert.notEqual(r1.native, r2.native);
        // Ack only wl-1: wl-2 pending must survive (no map-wide clear).
        const ack = JSON.parse(h.lastCall("DescribeWorld").payload) as { req: string; obs: string };
        h.lastCall("DescribeWorld").callback(ACC("gen-1", ack.req, ack.obs, "2", {
            workspaces: [
                { id: "wl-1", out: "out-secondary", bind: n1.id, logical: "wl-1", scope: "out", order: 2, mode: "tiled", exist: true, hold: "" },
                { id: "wl-2", out: "out-primary", bind: n2.id, logical: "", scope: "out", order: 3, mode: "tiled", exist: true, hold: "" },
            ],
        }));
        dispatchObserve(h);
        const after = bindingsOf(h);
        assert.equal(after.find((b) => b.logical === "wl-1")?.native, n1.id);
        assert.equal(after.find((b) => b.logical === "wl-2")?.native, n2.id);
        assert.equal(h.world.created.length, 2);
        h.handle?.stop();
    });
});
