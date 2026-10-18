// Created-desktop friendly rename (KDE): the unique logical token rides the
// void createDesktop call for exact-name attribution; only the proven bound
// native is then renamed to the friendly KDE-style `Desktop N` (native
// one-based position). Adopted/unproven/existing desktops are never renamed,
// and the id-based pending proof survives the rename for feedback.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

interface FakeDesktop { id: string; name: string }
const desk = (id: string, name?: string): FakeDesktop => ({ id, name: name ?? id });

function makeTwoOutput(opts: { throwOnce?: boolean; freezeCreated?: boolean } = {}) {
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
            const row = desk(id, name);
            if (opts.freezeCreated === true) {
                Object.freeze(row);
            }
            world.desktops.push(row);
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

const UNBOUND = (logical: string, out: string, order: number): Record<string, unknown> => ({
    id: logical, out, bind: "", logical, scope: "out", order, mode: "tiled", exist: true, hold: "",
});

describe("created-desktop friendly rename", () => {
    it("renames the proven desktop to Desktop N and keeps the binding publishable", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.req, req.obs, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                UNBOUND("wl-1", "out-secondary", 2),
            ],
        }));
        assert.equal(h.world.created.length, 1);
        const native = h.world.desktops.find((d) => d.id === h.world.created[0]);
        assert.ok(native !== undefined);
        assert.equal(native.name, "Desktop 3", "only the proven desktop is renamed to its native one-based position");
        assert.equal(h.world.desktops.filter((d) => d.name === "wl-1").length, 0, "token name leaves the inventory after binding");
        // The id-based pending proof survives the rename: the next report
        // still publishes the logical binding with no second creation.
        dispatchObserve(h);
        const row = bindingsOf(h).find((b) => b.logical === "wl-1");
        assert.ok(row !== undefined, `expected wl-1 binding, got ${JSON.stringify(bindingsOf(h))}`);
        assert.equal(row.native, native.id);
        assert.equal(row.out, "out-secondary");
        assert.equal(h.world.created.length, 1, "rename never re-creates");
        h.handle?.stop();
    });

    it("renames delayed proof without another native call", () => {
        const h = makeTwoOutput({ throwOnce: true });
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.req, req.obs, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                UNBOUND("wl-1", "out-secondary", 2),
            ],
        }));
        assert.equal(h.world.created.length, 0);
        h.world.desktops.push(desk("native-9", "wl-1"));
        const r2 = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", r2.req, r2.obs, "2", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                UNBOUND("wl-1", "out-secondary", 2),
            ],
        }));
        assert.equal(h.world.created.length, 0, "delayed recovery must not append");
        const recovered = h.world.desktops.find((d) => d.id === "native-9");
        assert.ok(recovered !== undefined);
        assert.equal(recovered.name, "Desktop 3", "delayed proof renames to its enumerated position");
        dispatchObserve(h);
        const row = bindingsOf(h).find((b) => b.logical === "wl-1");
        assert.ok(row !== undefined);
        assert.equal(row.native, "native-9");
        h.handle?.stop();
    });

    it("never renames adopted, unproven, or existing desktops", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        // Ambiguous attempt: the native call is a no-op for this leg, so the
        // token stays unproven while the user appends an unrelated desktop.
        h.world.desktops.push(desk("user-9", "User 9"));
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.req, req.obs, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                UNBOUND("wl-1", "out-secondary", 2),
            ],
        }));
        // wl-1 either bound (renamed) or stayed ambiguous; in neither case
        // may the stranger or the pre-existing desktops be renamed.
        assert.equal(h.world.desktops.find((d) => d.id === "user-9")?.name, "User 9", "unrelated desktop never renamed");
        assert.equal(h.world.desktops.find((d) => d.id === "wa-1")?.name, "wa-1", "existing desktop never renamed");
        assert.equal(h.world.desktops.find((d) => d.id === "wb-1")?.name, "wb-1", "existing desktop never renamed");
        h.handle?.stop();
    });

    it("gives two same-reply tokens distinct friendly names with no shared native", () => {
        const h = makeTwoOutput();
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.req, req.obs, "1", {
            workspaces: [
                UNBOUND("wl-1", "out-secondary", 2),
                UNBOUND("wl-2", "out-primary", 3),
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
        assert.equal(rows.find((b) => b.logical === "wl-1")?.native, n1.id);
        assert.equal(rows.find((b) => b.logical === "wl-2")?.native, n2.id);
        assert.equal(h.world.created.length, 2, "renames never re-create");
        h.handle?.stop();
    });

    it("keeps the binding when the rename write fails", () => {
        const h = makeTwoOutput({ freezeCreated: true });
        discover(h);
        const req = dispatchObserve(h);
        h.lastCall("DescribeWorld").callback(ACC("gen-1", req.req, req.obs, "1", {
            workspaces: [
                { id: "wa-1", out: "out-primary", bind: "wa-1", logical: "", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" },
                UNBOUND("wl-1", "out-secondary", 2),
            ],
        }));
        assert.equal(h.world.created.length, 1);
        dispatchObserve(h);
        const row = bindingsOf(h).find((b) => b.logical === "wl-1");
        assert.ok(row !== undefined, "failed rename keeps the proven binding for feedback");
        assert.equal(row.native, h.world.created[0]);
        assert.equal(h.world.created.length, 1, "failed rename never re-creates");
        h.handle?.stop();
    });
});
