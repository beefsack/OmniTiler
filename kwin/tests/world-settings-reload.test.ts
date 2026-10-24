// Production settings-reload coverage for the world adapter (legacy
// parity): gaps re-resolve per capture (a deliberate Options reload applies
// to subsequent layouts, never frozen at startup), same-axis move carries
// the live validated token on every move command, and the resize
// key-repeat index follows the legacy key + structural-signature rules
// (0-based initial press per the core step schedule, reset on
// window/direction/mode or confirmed membership/focus change, saturating
// at the protocol u32 bound). Scripted in-memory transport; no D-Bus, no
// KWin.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import {
    registerWorldShortcuts,
    createResizePressCounter,
    RESIZE_PRESS_MAX,
} from "../src/world-shortcuts";

import {
    normalizeShortcutProfile,
    readShortcutProfileValue,
} from "../src/shortcut-profile";

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
    fullscreen: boolean;
    maximizeMode: number;
    sticky: boolean;
    keepAbove: boolean;
    keepBelow: boolean;
    minSize: null;
    maxSize: null;
    fullScreen: boolean;
    onAllDesktops: boolean;
    setMaximize: (vertically: boolean, horizontally: boolean) => void;
}

const desk = (id: string): FakeDesktop => ({ id });
function win(id: string, out: string, ws: FakeDesktop, x = 0): FakeWindow {
    const fake = {
        internalId: id,
        normalWindow: true,
        frameGeometry: { x, y: 0, width: 200, height: 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false,
        fullscreen: false,
        maximizeMode: 0,
        sticky: false,
        keepAbove: false,
        keepBelow: false,
        minSize: null,
        maxSize: null,
        fullScreen: false,
        onAllDesktops: false,
        setMaximize: (vertically: boolean, horizontally: boolean): void => {
            (fake as FakeWindow).maximizeMode = vertically && horizontally ? 3 : 0;
        },
    } as unknown as FakeWindow;
    return fake;
}

interface FakeSignal {
    readonly handlers: Array<() => void>;
    readonly signal: { connect: (handler: () => void) => void; disconnect: (handler: () => void) => void };
}

function fakeSignal(): FakeSignal {
    const handlers: Array<() => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: () => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: () => void): void => {
                const index = handlers.indexOf(handler);
                if (index >= 0) {
                    handlers.splice(index, 1);
                }
            },
        },
    };
}

interface Settings {
    innerGap: number;
    outerGap: number;
    sameAxis: unknown;
}

interface Harness {
    settings: Settings;
    world: { wins: FakeWindow[]; activeWindow: FakeWindow | null };
    calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>;
    timers: Array<{ delay: number; cb: () => void }>;
    signals: Record<string, FakeSignal>;
    handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    runTimers: () => void;
    lastPayload: (method: string) => string;
    lastCall: (method: string) => { method: string; payload: string; callback: (reply: unknown) => void };
}

function makeHarness(): Harness {
    const settings: Settings = { innerGap: 8, outerGap: 8, sameAxis: "group-with-neighbor" };
    const ws1 = desk("ws1");
    const ws2 = desk("ws2");
    const world = {
        wins: [win("win-a", "out", ws1, 0), win("win-b", "out", ws1, 400)],
        activeWindow: null as FakeWindow | null,
    };
    world.activeWindow = world.wins[0] as FakeWindow;
    const calls: Harness["calls"] = [];
    const timers: Harness["timers"] = [];
    const signals: Record<string, FakeSignal> = {
        windowAdded: fakeSignal(),
        windowRemoved: fakeSignal(),
        windowActivated: fakeSignal(),
    };
    const surface = {
        windowList: () => world.wins,
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1, ws2],
        activeScreen: { name: "out" },
        cursorPos: { x: 2, y: 100 },
        windowAdded: (signals.windowAdded as FakeSignal).signal,
        windowRemoved: (signals.windowRemoved as FakeSignal).signal,
        windowActivated: (signals.windowActivated as FakeSignal).signal,
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(_ref: unknown) {},
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => ws1,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
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
        log: () => {},
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readInnerGapFn: () => settings.innerGap,
        readOuterGapFn: () => settings.outerGap,
        readSameAxisMoveFn: () => settings.sameAxis,
        oracleCallDbus: () => undefined,
    });
    assert.ok(handle !== null);
    const runTimers = (): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= 120) {
                timers.splice(i, 1);
                i -= 1;
                t.cb();
            }
        }
    };
    const lastCall = (method: string) => {
        const found = [...calls].reverse().find((c) => c.method === method);
        assert.ok(found !== undefined, `expected ${method}`);
        return found as { method: string; payload: string; callback: (reply: unknown) => void };
    };
    return {
        settings,
        world,
        calls,
        timers,
        signals,
        handle: handle as NonNullable<typeof handle>,
        runTimers,
        lastCall,
        lastPayload: (method: string) => lastCall(method).payload,
    };
}

function accept(gen: string, source: string, nativeInvalid: string, rev: string): string {
    return JSON.stringify({
        v: 2, adapter: "broker-1", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source,
        nativeInvalid,
        clock: "0",
        gestureSettled: null,
        operationSettled: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
        workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [], result: null,
    });
}

function discover(h: Harness, gen = "gen-1"): void {
    const call = h.lastCall("DiscoverWorld");
    const body = JSON.parse(call.payload);
    call.callback(JSON.stringify({
        v: 2, adapter: "broker-1", gen, req: body.req, obs: "0", rev: "0",
        ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
        gestureSettled: null,
        operationSettled: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        windows: [], transfers: [], effects: [], result: null,
    }));
    const auto = h.lastCall("DescribeWorld");
    const autoBody = JSON.parse(auto.payload);
    assert.equal((autoBody["report"] as Record<string, unknown>)["command"], undefined, "inner reports never carry a command");
    auto.callback(accept(gen, autoBody.source, autoBody.nativeInvalid, "0"));
}

function moveSameAxis(h: Harness): string {
    const req = JSON.parse(h.lastPayload("QueueWorldCommand")) as Record<string, unknown>;
    const cmd = req["command"] as Record<string, unknown>;
    assert.equal(cmd["op"], "move-directional");
    return cmd["sameAxis"] as string;
}

function resizePress(h: Harness): number {
    const req = JSON.parse(h.lastPayload("QueueWorldCommand")) as Record<string, unknown>;
    const cmd = req["command"] as Record<string, unknown>;
    assert.equal(cmd["op"], "operation");
    const inner = cmd["command"] as Record<string, unknown>;
    assert.equal(inner["op"], "resize");
    return inner["press"] as number;
}

function settle(h: Harness, rev: string): void {
    const req = JSON.parse(h.lastPayload("DescribeWorld")) as Record<string, unknown>;
    h.lastCall("DescribeWorld").callback(
        accept("gen-1", req["source"] as string, req["nativeInvalid"] as string, rev),
    );
}

describe("world settings reload", () => {
    it("re-resolves gaps per capture so a reload applies to later layouts", () => {
        const h = makeHarness();
        discover(h);
        h.handle.requestMove("right");
        h.runTimers();
        let domains = ((JSON.parse(h.lastPayload("DescribeWorld")) as Record<string, unknown>)["report"] as Record<string, unknown>)["screens"] as Array<Record<string, unknown>>;
        assert.equal(domains[0]?.["gap"], 8);
        assert.equal(domains[0]?.["outer"], 8);
        settle(h, "1");
        // Deliberate reload: validated values apply to the next capture
        // with no restart and no entry rebuild.
        h.settings.innerGap = 16;
        h.settings.outerGap = 20;
        for (const handler of [...(h.signals.windowActivated as FakeSignal).handlers]) {
            handler();
        }
        h.runTimers();
        domains = ((JSON.parse(h.lastPayload("DescribeWorld")) as Record<string, unknown>)["report"] as Record<string, unknown>)["screens"] as Array<Record<string, unknown>>;
        assert.equal(domains[0]?.["gap"], 16);
        assert.equal(domains[0]?.["outer"], 20);
        h.handle.stop();
    });

    it("carries the live same-axis token on every move with group default", () => {
        const h = makeHarness();
        discover(h);
        h.handle.requestMove("right");
        h.runTimers();
        assert.equal(moveSameAxis(h), "group-with-neighbor");
        settle(h, "1");
        // Live switch applies to subsequent moves only; the settled move
        // keeps the token it dispatched with.
        h.settings.sameAxis = "swap-with-neighbor";
        h.handle.requestMove("right");
        h.runTimers();
        assert.equal(moveSameAxis(h), "swap-with-neighbor");
        settle(h, "2");
        // Unknown values fall back to the validated default, never refuse.
        h.settings.sameAxis = "diagonal-wrap";
        h.handle.requestMove("right");
        h.runTimers();
        assert.equal(moveSameAxis(h), "group-with-neighbor");
        h.handle.stop();
    });

    it("steps the resize index 0,1,2 and resets on key or layout change", () => {
        const h = makeHarness();
        discover(h);
        h.handle.requestResize("right", "outwards");
        h.runTimers();
        assert.equal(resizePress(h), 0);
        settle(h, "1");
        // Same key with a settled layout steps further.
        h.handle.requestResize("right", "outwards");
        h.runTimers();
        assert.equal(resizePress(h), 1);
        settle(h, "2");
        h.handle.requestResize("right", "outwards");
        h.runTimers();
        assert.equal(resizePress(h), 2);
        settle(h, "3");
        // Direction change resets.
        h.handle.requestResize("left", "outwards");
        h.runTimers();
        assert.equal(resizePress(h), 0);
        settle(h, "4");
        // Mode change resets.
        h.handle.requestResize("left", "inwards");
        h.runTimers();
        assert.equal(resizePress(h), 0);
        settle(h, "5");
        // Window change resets.
        h.world.activeWindow = h.world.wins[1] as FakeWindow;
        h.handle.requestResize("left", "inwards");
        h.runTimers();
        assert.equal(resizePress(h), 0);
        settle(h, "6");
        // Confirmed membership change resets even with the same key: a new
        // live window alters the structural signature.
        h.handle.requestResize("left", "inwards");
        h.runTimers();
        assert.equal(resizePress(h), 1);
        settle(h, "7");
        const ws = { id: "ws1" };
        h.world.wins.push(win("win-c", "out", ws, 600));
        for (const handler of [...(h.signals.windowAdded as FakeSignal).handlers]) {
            handler();
        }
        h.handle.requestResize("left", "inwards");
        h.runTimers();
        assert.equal(resizePress(h), 0);
        h.handle.stop();
    });
});

describe("world shortcut settings", () => {
    function makeShortcutEnv(sameAxis: { value: unknown }, signature: { value: string | null }) {
        const commands: unknown[] = [];
        const registered = new Map<string, () => void>();
        const env = {
            readProp: (value: object, property: string): unknown => {
                try {
                    return Reflect.get(value as Record<string, unknown>, property);
                } catch (error) {
                    void error;
                    return undefined;
                }
            },
            isDirection: (value: unknown): value is "left" | "right" | "up" | "down" => {
                return value === "left" || value === "right" || value === "up" || value === "down";
            },
            shortcutFailed: (_action: string): void => {},
            submitCommand: (command: unknown, _label: string): boolean => {
                commands.push(command);
                return true;
            },
            handleFloatFocus: (_d: string): "not-float" | "handled" | "failed" => "not-float",
            handleFloatMove: (_d: string): "not-float" | "handled" | "failed" => "not-float",
            submitCatalogMove: (_a: string, _i: number, _f: boolean): void => {},
            submitOutputSend: (_d: string, _f: boolean): boolean => false,
            submitMigration: (_d: string): boolean => false,
            submitFloat: (_label: string): boolean => false,
            setStickyNative: (): boolean => false,
            setMaximizedNative: (): boolean => false,
            setFullscreenNative: (): boolean => false,
            activeOutputName: (): string | null => "out",
            activeWindowId: (): string | null => "win-a",
            activeWindowRef: (): object | null => null,
            currentScopeId: (): string => "ws1",
            getWorkspaceTilingSnapshot: () => ({ scope: "ws1", tiled: true, defaultTiled: true }),
            registerShortcutFn: (name: string, _text: string, _seq: string, cb: () => void): boolean => {
                registered.set(name, cb);
                return true;
            },
            readSameAxisMove: (): string => {
                const value = sameAxis.value;
                return value === "swap-with-neighbor" ? "swap-with-neighbor" : "group-with-neighbor";
            },
            readResizeSignature: (): string | null => signature.value,
        };
        registerWorldShortcuts(env);
        return { commands, registered };
    }

    it("routes catalog moves with the live same-axis token", () => {
        const sameAxis = { value: "group-with-neighbor" as unknown };
        const signature = { value: "sig-1" as string | null };
        const { commands, registered } = makeShortcutEnv(sameAxis, signature);
        const move = registered.get("omnitiler-move-right");
        assert.ok(move !== undefined);
        (move as () => void)();
        const first = commands[0] as Record<string, unknown>;
        assert.equal(first["sameAxis"], "group-with-neighbor");
        sameAxis.value = "swap-with-neighbor";
        (move as () => void)();
        const second = commands[1] as Record<string, unknown>;
        assert.equal(second["sameAxis"], "swap-with-neighbor");
    });

    it("steps catalog resizes 0,1,2 and resets on signature change", () => {
        const sameAxis = { value: "group-with-neighbor" as unknown };
        const signature = { value: "sig-1" as string | null };
        const { commands, registered } = makeShortcutEnv(sameAxis, signature);
        const resize = registered.get("omnitiler-resize-outwards-right");
        assert.ok(resize !== undefined);
        (resize as () => void)();
        (resize as () => void)();
        (resize as () => void)();
        const presses = commands.map(
            (c) => ((c as Record<string, unknown>)["command"] as Record<string, unknown>)["press"],
        );
        assert.deepEqual(presses, [0, 1, 2]);
        signature.value = "sig-2";
        (resize as () => void)();
        const reset = ((commands[3] as Record<string, unknown>)["command"] as Record<string, unknown>)["press"];
        assert.equal(reset, 0);
    });
});

describe("resize press counter", () => {
    it("starts at 0 per the core step schedule and saturates at u32 max", () => {
        assert.equal(RESIZE_PRESS_MAX, 4294967295);
        const next = createResizePressCounter(() => "sig");
        assert.deepEqual([next("w", "right", "outwards"), next("w", "right", "outwards"), next("w", "right", "outwards")], [0, 1, 2]);
        assert.equal(next("w", "left", "outwards"), 0);
        assert.equal(next("other", "left", "outwards"), 0);
        assert.equal(next("other", "left", "inwards"), 0);
    });

    it("resets on structural signature change, never on geometry alone", () => {
        let sig: string | null = "sig-1";
        const next = createResizePressCounter(() => sig);
        assert.equal(next("w", "right", "outwards"), 0);
        assert.equal(next("w", "right", "outwards"), 1);
        // Same membership/focus (geometry may have moved): keeps stepping.
        assert.equal(next("w", "right", "outwards"), 2);
        sig = "sig-2";
        assert.equal(next("w", "right", "outwards"), 0);
    });
});

describe("world workspace mode startup", () => {
    function modeHarness(mode: { value: unknown }) {
        const ws1 = desk("ws1");
        const world = { wins: [win("win-a", "out", ws1, 0)], activeWindow: null as FakeWindow | null };
        world.activeWindow = world.wins[0] as FakeWindow;
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        const logs: string[] = [];
        const configChanged = fakeSignal();
        const surface = {
            windowList: () => world.wins,
            screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
            desktops: [ws1],
            activeScreen: { name: "out" },
            cursorPos: { x: 2, y: 100 },
            get activeWindow() {
                return world.activeWindow;
            },
            set activeWindow(_ref: unknown) {},
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: () => ws1,
            setCurrentDesktopForScreen: () => undefined,
            sendClientToScreen: () => undefined,
            createDesktop: () => null as unknown as object,
            removeDesktop: () => undefined,
        };
        const handle = startWorldAdapterEntry({
            workspace: surface,
            options: { configChanged: configChanged.signal },
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
                    }                }
            },
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: (message) => {
                logs.push(message);
            },
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => mode.value,
        });
        assert.ok(handle !== null);
        const lastCall = (method: string) => {
            const found = [...calls].reverse().find((c) => c.method === method);
            assert.ok(found !== undefined, `expected ${method}`);
            return found as { method: string; payload: string; callback: (reply: unknown) => void };
        };
        const runTimers = (): void => {
            for (let i = 0; i < timers.length; i += 1) {
                const t = timers[i] as { delay: number; cb: () => void };
                if (t.delay <= 120) {
                    timers.splice(i, 1);
                    i -= 1;
                    t.cb();
                }
            }
        };
        const discoverNow = (gen = "gen-1"): void => {
            const call = lastCall("DiscoverWorld");
            const body = JSON.parse(call.payload);
            call.callback(JSON.stringify({
                v: 2, adapter: "broker-1", gen, req: body.req, obs: "0", rev: "0",
                ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
                gestureSettled: null,
                operationSettled: [],
                warpCursor: null,
                presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
                windows: [], transfers: [], effects: [], result: null,
            }));
            const auto = lastCall("DescribeWorld");
            const autoBody = JSON.parse(auto.payload);
            auto.callback(accept(gen, autoBody.source, autoBody.nativeInvalid, "0"));
        };
        return {
            calls, logs, configChanged, handle: handle as NonNullable<typeof handle>,
            lastCall, runTimers, discoverNow,
            lastMode: (): unknown => ((JSON.parse(lastCall("DescribeWorld").payload) as Record<string, unknown>)["report"] as Record<string, unknown>)["mode"],
        };
    }

    it("preserves the initial mode and logs restart-required drift without adopting it", () => {
        const mode = { value: "per-output-local" as unknown };
        const h = modeHarness(mode);
        h.discoverNow();
        assert.equal(h.lastMode(), "per-output-local");
        // Deliberate reload to a new mode: logged restart-required, never
        // adopted by the running activation.
        mode.value = "shared";
        for (const fire of [...h.configChanged.handlers]) {
            fire();
        }
        h.runTimers();
        assert.ok(
            h.logs.some((line) => line === "omnitiler:world:config-reloaded stage=restart-required keys=workspaceMode"),
            "mode drift logged restart-required",
        );
        assert.equal(h.lastMode(), "per-output-local", "running activation keeps its startup mode");
        h.handle.stop();
        // A new instance adopts the changed value.
        const fresh = modeHarness(mode);
        fresh.discoverNow();
        assert.equal(fresh.lastMode(), "shared", "new activation adopts the saved mode");
        fresh.handle.stop();
    });
});
describe("world shortcut profile startup", () => {
    function profileHarness(profile: { value: unknown }) {
        const ws1 = desk("ws1");
        const world = { wins: [win("win-a", "out", ws1, 0)], activeWindow: null as FakeWindow | null };
        world.activeWindow = world.wins[0] as FakeWindow;
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const logs: string[] = [];
        const shortcuts = new Map<string, { text: string; sequence: string }>();
        const configChanged = fakeSignal();
        const surface = {
            windowList: () => world.wins,
            screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
            desktops: [ws1],
            activeScreen: { name: "out" },
            cursorPos: { x: 2, y: 100 },
            get activeWindow() {
                return world.activeWindow;
            },
            set activeWindow(_ref: unknown) {},
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: () => ws1,
            setCurrentDesktopForScreen: () => undefined,
            sendClientToScreen: () => undefined,
            createDesktop: () => null as unknown as object,
            removeDesktop: () => undefined,
        };
        const options = { configChanged: configChanged.signal };
        const handle = startWorldAdapterEntry({
            workspace: surface,
            options,
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
                    }                }
            },
            scheduleOnce: () => () => undefined,
            log: (message) => {
                logs.push(message);
            },
            gestureActive: () => false,
            registerShortcutFn: (name, text, sequence) => {
                shortcuts.set(name, { text, sequence });
                return true;
            },
            readWorkspaceModeFn: () => "per-output-local",
            readProfileFn: () => profile.value,
        });
        assert.ok(handle !== null);
        return { calls, logs, shortcuts, configChanged, handle: handle as NonNullable<typeof handle> };
    }

    it("validates the three profiles with a cosmic fallback", () => {
        assert.equal(normalizeShortcutProfile("cosmic"), "cosmic");
        assert.equal(normalizeShortcutProfile("hyprland"), "hyprland");
        assert.equal(normalizeShortcutProfile("bspwm"), "bspwm");
        for (const bad of [undefined, null, "", "COSMIC", "i3", 0]) {
            assert.equal(normalizeShortcutProfile(bad), "cosmic", `${String(bad)} falls back`);
        }
        assert.equal(readShortcutProfileValue(() => "hyprland"), "hyprland");
        assert.equal(readShortcutProfileValue(() => "bogus"), "cosmic");
        assert.equal(
            readShortcutProfileValue(() => {
                throw new Error("boom");
            }),
            "cosmic",
        );
    });

    it("shares one directional catalog across profiles and never rewrites bindings", () => {
        const cosmic = profileHarness({ value: "cosmic" });
        const bspwm = profileHarness({ value: "bspwm" });
        assert.ok(cosmic.shortcuts.size > 0 && bspwm.shortcuts.size > 0);
        assert.deepEqual([...cosmic.shortcuts.keys()].sort(), [...bspwm.shortcuts.keys()].sort());
        assert.deepEqual(
            [...cosmic.shortcuts.entries()].sort(),
            [...bspwm.shortcuts.entries()].sort(),
        );
        cosmic.handle.stop();
        bspwm.handle.stop();
    });

    it("logs restart-required drift without adopting it or touching bindings", () => {
        const profile = { value: "cosmic" as unknown };
        const h = profileHarness(profile);
        const before = new Map(h.shortcuts);
        profile.value = "hyprland";
        for (const fire of [...h.configChanged.handlers]) {
            fire();
        }
        assert.ok(
            h.logs.some((line) => line === "omnitiler:world:config-reloaded stage=restart-required keys=shortcutProfile"),
            "drift logged restart-required",
        );
        assert.deepEqual([...h.shortcuts.entries()].sort(), [...before.entries()].sort(), "bindings untouched");
        // A reload back at the startup value stays silent.
        profile.value = "cosmic";
        const count = h.logs.length;
        for (const fire of [...h.configChanged.handlers]) {
            fire();
        }
        assert.equal(h.logs.length, count, "no reload line without drift");
        h.handle.stop();
    });
});
