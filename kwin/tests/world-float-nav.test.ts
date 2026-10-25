// Float-origin directional focus/move through the production world entry:
// legacy parity (native local float focus/move) plus the Left/Right miss
// Planner fallback (floatSubject true), with native game safety. Real
// production commands and native setters; no Planner core import.

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

function stickyWin(id: string, out: string, ws: FakeDesktop, x: number, extra: Partial<FakeWindow> = {}): FakeWindow {
    return {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x, y: 100, width: 200, height: 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: true,
        minSize: null,
        maxSize: null,
        ...extra,
    };
}

function tiledWin(id: string, out: string, ws: FakeDesktop, x: number): FakeWindow {
    const w: FakeWindow = {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
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
    return w;
}

interface Harness {
    world: {
        screens: Array<{ name: string; geometry: { x: number; y: number; width: number; height: number } }>;
        desktops: FakeDesktop[];
        wins: FakeWindow[];
        activeScreen: { name: string };
        activeWindow: FakeWindow | null;
        currentByScreen: Map<string, FakeDesktop>;
        focused: string[];
        geometryWrites: string[];
    };
    calls: CapturedCall[];
    timers: Array<{ delay: number; cb: () => void }>;
    logs: string[];
    shortcuts: Map<string, () => void>;
    handle: ReturnType<typeof startWorldAdapterEntry>;
    runTimers: () => void;
}

function makeHarness(wins: FakeWindow[], screens: Harness["world"]["screens"], desktops: FakeDesktop[], currentByScreen: Map<string, FakeDesktop>): Harness {
    const world = {
        screens,
        desktops,
        wins,
        activeScreen: { name: screens[0]?.name ?? "out-a" },
        activeWindow: wins[0] ?? null,
        currentByScreen,
        focused: [] as string[],
        geometryWrites: [] as string[],
    };
    const calls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    const shortcuts = new Map<string, () => void>();
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
            const found = world.wins.find((w) => (w as unknown) === ref);
            if (found) {
                world.focused.push(found.internalId);
            }
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
    for (const w of world.wins) {
        let frame = w.frameGeometry;
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                world.geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
    }
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
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
        log: (message) => {
            logs.push(message);
        },
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
    return { world, calls, timers, logs, shortcuts, handle, runTimers };
}

const BOOTSTRAP = (gen: string, req: string): string => JSON.stringify({
    v: 2, adapter: "broker-1", gen, req, obs: "0", rev: "0",
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
    assert.equal((autoBody["report"] as Record<string, unknown>)["command"], undefined, "inner reports never carry a command");
    auto.callback(JSON.stringify({
        v: 2, adapter: "broker-1", gen, req: autoBody.source, obs: "1", rev: "1",
        ready: true, consumed: false,
        source: autoBody.source,
        nativeInvalid: autoBody.nativeInvalid,
        clock: "0",
        gestureSettled: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [], workspaces: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
    }));
}

function queuePayloads(h: Harness): Array<Record<string, unknown>> {
    return h.calls.filter((c) => c.method === "QueueWorldCommand").map((c) => JSON.parse(c.payload) as Record<string, unknown>);
}

describe("world float-origin navigation", () => {
    it("focuses the local float natively with no Planner focus dispatch", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500), stickyWin("win-g", "out-a", ws, 700)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        h.world.focused.length = 0;
        h.handle?.requestFocus("right");
        assert.deepEqual(h.world.focused, ["win-g"], "exactly one native focus to the nearest right float");
        assert.ok(h.logs.some((l) => l.includes("focus-float-applied direction=right")));
        h.runTimers();
        // Local float focus is native-only: no Queue filed.
        assert.equal(queuePayloads(h).length, 0, "no Planner focus intent for a local float target");
    });

    it("drives the shortcut through the same native local focus", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500), stickyWin("win-g", "out-a", ws, 700)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        const cb = h.shortcuts.get("omnitiler-focus-right");
        assert.ok(cb !== undefined, "directional focus shortcut registered");
        h.world.focused.length = 0;
        cb();
        assert.deepEqual(h.world.focused, ["win-g"]);
    });

    it("falls back direction-only with floatSubject true on a Left/Right miss", () => {
        const wsa = desk("ws-a");
        const wsb = desk("ws-b");
        const wins = [stickyWin("win-f", "out-a", wsa, 500)];
        const screens = [
            { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
            { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
        ];
        const h = makeHarness(wins, screens, [wsa, wsb], new Map([["out-a", wsa], ["out-b", wsb]]));
        discover(h);
        h.world.focused.length = 0;
        h.handle?.requestFocus("right");
        assert.deepEqual(h.world.focused, [], "miss performs no native focus");
        h.runTimers();
        const qpayloads = queuePayloads(h);
        const focusOps = qpayloads
            .map((p) => p["command"] as Record<string, unknown> | null)
            .filter((c) => c !== null && c["op"] === "focus-directional");
        assert.ok(focusOps.length >= 1, "fallback dispatches a focus-directional intent");
        const inner = focusOps[focusOps.length - 1] as Record<string, unknown>;
        assert.ok(!("floatSubject" in inner), "no float routing key on the wire");
        assert.equal(inner["win"], "win-f");
        assert.equal(inner["direction"], "right");
    });

    it("retains on an Up miss even with a second output", () => {
        const wsa = desk("ws-a");
        const wsb = desk("ws-b");
        const wins = [stickyWin("win-f", "out-a", wsa, 500)];
        const screens = [
            { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
            { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
        ];
        const h = makeHarness(wins, screens, [wsa, wsb], new Map([["out-a", wsa], ["out-b", wsb]]));
        discover(h);
        h.handle?.requestFocus("up");
        assert.deepEqual(h.world.focused, [], "up miss never focuses");
        assert.ok(h.logs.some((l) => l.includes("focus-float-retained direction=up")));
        h.runTimers();
        assert.equal(queuePayloads(h).length, 0, "up miss dispatches no focus-directional intent");
    });

    it("half-snaps a float move natively with no Planner move dispatch", () => {
        const ws = desk("ws-a");
        const wins = [stickyWin("win-f", "out-a", ws, 500)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        h.handle?.requestMove("right");
        assert.deepEqual(h.world.geometryWrites, ["win-f@404,8,388,584"], "exactly one native half-snap");
        assert.ok(h.logs.some((l) => l.includes("move-float-applied direction=right")));
        h.runTimers();
        assert.equal(queuePayloads(h).length, 0, "float moves never dispatch a Planner move");
    });

    it("routes tiled focus direction-only without floatSubject", () => {
        const ws = desk("ws-a");
        const wins = [tiledWin("win-a", "out-a", ws, 0)];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        h.handle?.requestFocus("right");
        assert.deepEqual(h.world.focused, [], "tiled focus never writes natively");
        h.runTimers();
        const qps = queuePayloads(h);
        const focusOps = qps
            .map((p) => p["command"] as Record<string, unknown> | null)
            .filter((c) => c !== null && c["op"] === "focus-directional");
        assert.ok(focusOps.length >= 1);
        const inner = focusOps[focusOps.length - 1] as Record<string, unknown>;
        assert.ok(!("floatSubject" in inner), "no float routing key on the wire");
    });

    it("never auto-focuses a fullscreen float candidate", () => {
        const ws = desk("ws-a");
        const wins = [
            stickyWin("win-f", "out-a", ws, 500),
            stickyWin("win-g", "out-a", ws, 700, { fullScreen: true, fullscreen: true } as Partial<FakeWindow>),
        ];
        const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } }];
        const h = makeHarness(wins, screens, [ws], new Map([["out-a", ws]]));
        discover(h);
        h.world.focused.length = 0;
        h.handle?.requestFocus("right");
        assert.deepEqual(h.world.focused, [], "game/overlay candidates never take automatic focus");
        assert.ok(h.logs.some((l) => l.includes("focus-float-refused-target")));
    });
});
