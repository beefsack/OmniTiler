// Regression: directional focus/move send direction-only intents; the
// core owns crossing from retained FULL topology. No adapter cross flag
// or target derivation exists, so every directional action dispatches on
// single-output hosts and at edgeless sides (the core runs the local
// fallback), and two-output intents carry direction only.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

interface FakeDesktop {
    id: string;
}
interface FakeWindow {
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
    internalId: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    hidden: boolean;
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
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x, y: 0, width: 200, height: 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false,
        hidden: false,
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
            if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({ v: 3, outcome: "queued", gen: "gen-1",
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 3, outcome: "ready", gen: "gen-1", globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 3, outcome: "pending", gen: "gen-1" }));
                }            }
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
            // Host-deadline timers (25 s verified host call deadline, see
            // WAIT_HOST_DEADLINE_MS) never fire in these offline flows: the
            // production wait/discovery chains re-arm them host-causally,
            // and flushing them would collapse production cadence into a
            // sync loop. Debounce/timeout timers flush normally.
            if (t.delay >= 25000) {
                continue;
            }
            timers.splice(i, 1);
            i -= 1;
            t.cb();
        }
    };
    return { calls, timers, shortcuts, handle, runTimers };
}

const BOOTSTRAP = (gen: string, req: string): string => JSON.stringify({
    v: 3, adapter: "broker-1", gen, req, obs: "0", rev: "0",
    ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
    gestureSettled: null,
    operationSettled: [], nativeCreations: [],
    warpCursor: null,
    presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
    windows: [], transfers: [], effects: [], result: null,
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
        v: 3, adapter: "broker-1", gen, req: autoBody.source, obs: "1", rev: "1",
        ready: true, consumed: false,
        source: autoBody.source,
        nativeInvalid: autoBody.nativeInvalid,
        clock: "0",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [], workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
    }));
}

function lastCommand(h: Harness): Record<string, unknown> | null {
    const found = [...h.calls].reverse().find((c) => c.method === "QueueWorldCommand");
    if (found === undefined) {
        return null;
    }
    return (JSON.parse(found.payload) as { command: Record<string, unknown> }).command;
}

describe("world directional intent (core-owned crossing)", () => {
    it("sends focus-directional on a single output (core runs local fallback)", () => {
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
        const cmd = lastCommand(h);
        assert.ok(cmd !== null, "focus intent dispatched");
        assert.equal(cmd["op"], "focus-directional");
        assert.equal(cmd["direction"], "right");
        assert.ok(!("floatSubject" in cmd), "no float routing key on the wire");
    });

    it("sends move-directional on a single output (core runs local R1-R3)", () => {
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
        const cmd = lastCommand(h);
        assert.ok(cmd !== null, "move intent dispatched");
        assert.equal(cmd["op"], "move-directional");
        assert.equal(cmd["direction"], "down");
    });

    it("sends focus-directional toward a second output (no adapter endpoint)", () => {
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
        const cmd = lastCommand(h);
        assert.ok(cmd !== null, "focus intent dispatched");
        assert.equal(cmd["op"], "focus-directional");
        assert.equal(cmd["direction"], "right");
        assert.ok(!("out" in (cmd as object)), "no adapter endpoint");
        assert.ok(!("crossOutput" in (cmd as object)), "no adapter cross flag");
    });

    it("sends move-directional toward a second output (no adapter endpoint)", () => {
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
        const cmd = lastCommand(h);
        assert.ok(cmd !== null, "move intent dispatched");
        assert.equal(cmd["op"], "move-directional");
        assert.equal(cmd["direction"], "right");
    });

    it("routes the focus-right shortcut as focus-directional", () => {
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
        const cmd = lastCommand(h);
        assert.ok(cmd !== null, "shortcut dispatches a focus intent");
        assert.equal(cmd["op"], "focus-directional");
    });

    it("resolves braced ids over array-like lists as focus-directional", () => {
        // Production entry shape: fresh array-like lists per read and
        // canonical braced wrapper ids that must normalize to the bare
        // observation ids. A single output dispatches direction-only; the
        // core runs the local fallback instead of refusing.
        const wsA = "aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa";
        const winA = "11111111-1111-1111-1111-111111111111";
        const winB = "22222222-2222-2222-2222-222222222222";
        const out = "out-1";
        const base = [
            { win: winA, ws: wsA, x: 0 },
            { win: winB, ws: wsA, x: 210 },
        ];
        const wrap = (row: { win: string; ws: string; x: number }): FakeWindow => ({
            internalId: `{${row.win}}`, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { x: row.x, y: 0, width: 200, height: 200 },
            output: { name: out },
            desktops: qmlLike([{ id: row.ws }]) as unknown as FakeDesktop[],
            minimized: false,
            hidden: false,
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
                if (method === "QueueWorldCommand") {
                    const queueBody = JSON.parse(payload) as Record<string, unknown>;
                    callback(JSON.stringify({ v: 3, outcome: "queued", gen: "gen-1",
                        client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
                } else if (method === "TakeCommandResult") {
                    const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                    if (stagedTake !== undefined) {
                        delete (globalThis as Record<string, unknown>)["__takeResult"];
                        callback(JSON.stringify({ v: 3, outcome: "ready", gen: "gen-1", globalCmd: "1", result: stagedTake }));
                    } else {
                        callback(JSON.stringify({ v: 3, outcome: "pending", gen: "gen-1" }));
                    }                }
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
                    // Host-deadline timers (25 s verified host call
                    // deadline, see WAIT_HOST_DEADLINE_MS) never fire in
                    // these offline flows: the production wait/discovery
                    // chains re-arm them host-causally, and flushing them
                    // would collapse production cadence into a sync loop.
                    // Debounce/timeout timers flush normally.
                    if (t.delay >= 25000) {
                        continue;
                    }
                    timers.splice(i, 1);
                    i -= 1;
                    t.cb();
                }
            },
        };
        discover(h);
        handle?.requestFocus("right");
        h.runTimers();
        const cmd = lastCommand(h);
        assert.ok(cmd !== null, "focus intent dispatched");
        assert.equal(cmd["win"], winA);
        assert.equal(cmd["direction"], "right");
        assert.equal(cmd["op"], "focus-directional");
    });
});
