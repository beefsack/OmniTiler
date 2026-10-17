// Regression: directional focus/move must scope cross-output only with a
// live adjacent output. The adapter sent crossOutput=true unconditionally
// for left/right (and all moves), and the core refuses cross-output
// requests without adjacency, so every directional focus/move refused on
// a single-output host (and at any edgeless side). Single output (and
// edgeless directions) now route local single-domain commands; outputs
// with real adjacency keep the cross-output scope.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

interface FakeDesktop {
    id: string;
}
interface FakeWindow {
    internalId: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minSize: null;
    maxSize: null;
}
interface CapturedCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

const desk = (id: string): FakeDesktop => ({ id });

// Scripting-host list shape: indexed properties plus length on a plain
// object, never a JS array. The entry decodes this form on every read.
const qmlLike = (items: ReadonlyArray<unknown>): object => {
    const o: Record<string, unknown> = { length: items.length };
    items.forEach((item, index) => {
        o[String(index)] = item;
    });
    return o;
};

function tiledWin(id: string, out: string, ws: FakeDesktop, x: number): FakeWindow {
    return {
        internalId: id,
        normalWindow: true,
        frameGeometry: { x, y: 0, width: 200, height: 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
    };
}

interface Harness {
    calls: CapturedCall[];
    timers: Array<{ delay: number; cb: () => void }>;
    shortcuts: Map<string, () => void>;
    handle: ReturnType<typeof startWorldAdapterEntry>;
    runTimers: () => void;
}

function makeHarness(wins: FakeWindow[], screens: Array<{ name: string; geometry: { x: number; y: number; width: number; height: number } }>, desktops: FakeDesktop[], currentByScreen: Map<string, FakeDesktop>): Harness {
    const world = {
        screens,
        desktops,
        wins,
        activeScreen: { name: screens[0]?.name ?? "out-a" },
        activeWindow: wins[0] ?? null,
        currentByScreen,
    };
    const calls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const shortcuts = new Map<string, () => void>();
    const surface = {
        windowList: () => world.wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        get activeWindow() {
            return world.activeWindow;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }) => world.currentByScreen.get(screen.name) ?? null,
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
            const found = world.desktops.find((d) => (d as unknown) === deskRef);
            if (found) {
                world.currentByScreen.set(screen.name, found);
            }
        },
        sendClientToScreen: () => undefined,
        createDesktop: () => null,
        removeDesktop: () => false,
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: () => undefined,
        gestureActive: () => false,
        registerShortcutFn: (name, _text, _seq, cb) => {
            shortcuts.set(name, cb);
            return true;
        },
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    const runTimers = (): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            timers.splice(i, 1);
            i -= 1;
            t.cb();
        }
    };
    return { calls, timers, shortcuts, handle, runTimers };
}

const BOOTSTRAP = (gen: string, req: string): string => JSON.stringify({
    v: 1, adapter: "test-adapter", gen, req, obs: "0", rev: "0",
    ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
    windows: [], transfers: [], effects: [],
});

function discover(h: Harness, gen = "gen-1"): void {
    const disc = [...h.calls].reverse().find((c) => c.method === "DiscoverWorld");
    assert.ok(disc !== undefined);
    const body = JSON.parse(disc.payload);
    disc.callback(BOOTSTRAP(gen, body.req));
    const auto = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(auto !== undefined);
    const autoBody = JSON.parse(auto.payload);
    auto.callback(JSON.stringify({
        v: 1, adapter: "test-adapter", gen, req: autoBody.req, obs: autoBody.obs, rev: "1",
        ready: true, consumed: false,
        scopes: [], workspaces: [], outputs: [], windows: [], transfers: [], effects: [],
    }));
}

function lastOperation(h: Harness, op: string): Record<string, unknown> | null {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DescribeWorld") {
            return false;
        }
        const body = JSON.parse(c.payload) as { command?: { op?: unknown; command?: { op?: unknown } } | null };
        return body.command?.op === "operation" && (body.command?.command as { op?: unknown } | undefined)?.op === op;
    });
    if (found === undefined) {
        return null;
    }
    return (JSON.parse(found.payload) as { command: { command: Record<string, unknown> } }).command.command;
}

describe("world directional cross scope", () => {
    it("scopes single-output focus right as local", () => {
        const ws = desk("ws-a");
        const h = makeHarness(
            [tiledWin("win-a", "out-a", ws, 0), tiledWin("win-b", "out-a", ws, 210)],
            [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }],
            [ws],
            new Map([["out-a", ws]]),
        );
        discover(h);
        h.handle?.requestFocus("right");
        h.runTimers();
        const inner = lastOperation(h, "focus");
        assert.ok(inner !== null, "focus operation dispatched");
        assert.equal(inner["direction"], "right");
        assert.equal(inner["crossOutput"], false);
    });

    it("scopes single-output move down as local", () => {
        const ws = desk("ws-a");
        const h = makeHarness(
            [tiledWin("win-a", "out-a", ws, 0), tiledWin("win-b", "out-a", ws, 210)],
            [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }],
            [ws],
            new Map([["out-a", ws]]),
        );
        discover(h);
        h.handle?.requestMove("down");
        h.runTimers();
        const inner = lastOperation(h, "move");
        assert.ok(inner !== null, "move operation dispatched");
        assert.equal(inner["direction"], "down");
        assert.equal(inner["crossOutput"], false);
    });

    it("keeps cross-output focus scope toward a live adjacent output", () => {
        const wsa = desk("ws-a");
        const wsb = desk("ws-b");
        const h = makeHarness(
            [tiledWin("win-a", "out-a", wsa, 0), tiledWin("win-b", "out-b", wsb, 0)],
            [
                { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
                { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
            ],
            [wsa, wsb],
            new Map([["out-a", wsa], ["out-b", wsb]]),
        );
        discover(h);
        h.handle?.requestFocus("right");
        h.runTimers();
        const focus = lastOperation(h, "focus");
        assert.ok(focus !== null, "focus operation dispatched");
        assert.equal(focus["crossOutput"], true);
    });

    it("keeps cross-output move scope toward a live adjacent output", () => {
        const wsa = desk("ws-a");
        const wsb = desk("ws-b");
        const h = makeHarness(
            [tiledWin("win-a", "out-a", wsa, 0), tiledWin("win-b", "out-b", wsb, 0)],
            [
                { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
                { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
            ],
            [wsa, wsb],
            new Map([["out-a", wsa], ["out-b", wsb]]),
        );
        discover(h);
        h.handle?.requestMove("right");
        h.runTimers();
        const move = lastOperation(h, "move");
        assert.ok(move !== null, "move operation dispatched");
        assert.equal(move["crossOutput"], true);
    });

    it("routes the focus-right shortcut with local scope on one output", () => {
        const ws = desk("ws-a");
        const h = makeHarness(
            [tiledWin("win-a", "out-a", ws, 0), tiledWin("win-b", "out-a", ws, 210)],
            [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }],
            [ws],
            new Map([["out-a", ws]]),
        );
        discover(h);
        const cb = h.shortcuts.get("omnitiler-focus-right");
        assert.ok(cb !== undefined, "directional focus shortcut registered");
        cb();
        h.runTimers();
        const inner = lastOperation(h, "focus");
        assert.ok(inner !== null, "shortcut dispatches a focus operation");
        assert.equal(inner["crossOutput"], false);
    });

    it("resolves braced ids over array-like lists with local scope on one output", () => {
        // Production entry shape: fresh array-like lists per read and
        // canonical braced wrapper ids that must normalize to the bare
        // observation ids. A single output has no adjacency, so the
        // directional focus stays local instead of failing.
        const wsA = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        const winA = "11111111-1111-1111-1111-111111111111";
        const winB = "22222222-2222-2222-2222-222222222222";
        const out = "out-1";
        const base = [
            { win: winA, ws: wsA, x: 0 },
            { win: winB, ws: wsA, x: 210 },
        ];
        const wrap = (row: { win: string; ws: string; x: number }): FakeWindow => ({
            internalId: `{${row.win}}`,
            normalWindow: true,
            frameGeometry: { x: row.x, y: 0, width: 200, height: 200 },
            output: { name: out },
            desktops: qmlLike([{ id: row.ws }]) as unknown as FakeDesktop[],
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: null,
            maxSize: null,
        });
        const world = {
            screens: [{ name: out, geometry: { x: 0, y: 0, width: 800, height: 600 } }],
            desktops: [{ id: wsA }],
            activeScreen: { name: out },
            currentByScreen: new Map([[out, { id: wsA }]]),
        };
        const calls: CapturedCall[] = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const shortcuts = new Map<string, () => void>();
        const surface = {
            windowList: () => qmlLike(base.map((row) => wrap(row))),
            get screens() {
                return qmlLike(world.screens.map((s) => ({ ...s })));
            },
            get desktops() {
                return qmlLike(world.desktops.map((d) => ({ ...d })));
            },
            activeScreen: world.activeScreen,
            get activeWindow() {
                return wrap(base[0] as { win: string; ws: string; x: number });
            },
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: (screen: { name: string }) => world.currentByScreen.get(screen.name) ?? null,
            setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => {
                const all = [world.desktops[0]];
                const found = all.find((d) => (d as unknown) === deskRef);
                if (found) {
                    world.currentByScreen.set(screen.name, found as { id: string });
                }
            },
            sendClientToScreen: () => undefined,
            createDesktop: () => null,
            removeDesktop: () => false,
        };
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_service, _path, _iface, method, payload, callback) => {
                calls.push({ method, payload, callback });
            },
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: () => undefined,
            gestureActive: () => false,
            registerShortcutFn: (name, _text, _seq, cb) => {
                shortcuts.set(name, cb);
                return true;
            },
            readWorkspaceModeFn: () => "per-output-local",
        });
        assert.ok(handle !== null);
        const h: Harness = {
            calls, timers, shortcuts, handle,
            runTimers: () => {
                for (let i = 0; i < timers.length; i += 1) {
                    const t = timers[i] as { delay: number; cb: () => void };
                    timers.splice(i, 1);
                    i -= 1;
                    t.cb();
                }
            },
        };
        discover(h);
        handle?.requestFocus("right");
        h.runTimers();
        const inner = lastOperation(h, "focus");
        assert.ok(inner !== null, "focus operation dispatched");
        assert.equal(inner["win"], winA);
        assert.equal(inner["direction"], "right");
        assert.equal(inner["crossOutput"], false);
    });
});
