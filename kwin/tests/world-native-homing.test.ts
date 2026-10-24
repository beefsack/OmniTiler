// External native homing adoption (approach2/5): per-window
// `outputChanged` / `desktopsChanged` must autonomously invalidate and
// converge with NO workspace signal. Fail-before: without per-window
// subscriptions the external legs below queue no DescribeWorld (the
// production 7-minute stall); with the fix each leg dispatches on the
// production debounce only, applies through the real Engine, and settles
// with no retained transfers. Own-flight echoes (migration + window
// send) must settle exactly once with no replay/loop/stall, and
// attach/remove/stop must leave no stale callbacks. Included in
// `npm test`.

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";
import { describe, it } from "node:test";

import { WORLD_DEBOUNCE_MS, startWorldAdapterEntry } from "../src/world-entry";
import { createMiniBroker, type MiniBroker } from "./world-wire-harness";

function repoRoot(): string {
    let dir = resolve(process.cwd());
    for (let depth = 0; depth < 4; depth += 1) {
        if (existsSync(join(dir, "Cargo.toml"))) {
            return dir;
        }
        dir = dirname(dir);
    }
    throw new Error("fixture root not found");
}

class WorldBridge {
    private readonly proc: ChildProcess;
    private readonly waiters: Array<{
        resolve: (reply: string) => void;
        reject: (error: Error) => void;
    }> = [];
    private dead: string | null = null;

    private constructor(proc: ChildProcess) {
        this.proc = proc;
        const stdout = proc.stdout;
        if (stdout === null || stdout === undefined) {
            throw new Error("world_eval stdout unavailable");
        }
        createInterface({ input: stdout }).on("line", (line: string) => {
            this.waiters.shift()?.resolve(line);
        });
        proc.stderr?.resume();
        proc.on("error", (error) => {
            this.failAll(error instanceof Error ? error : new Error(String(error)));
        });
        proc.on("exit", (code) => {
            this.failAll(new Error(`world_eval exited with code ${String(code)}`));
        });
    }

    private failAll(error: Error): void {
        if (this.dead === null) {
            this.dead = error.message;
        }
        while (this.waiters.length > 0) {
            this.waiters.shift()?.reject(error);
        }
    }

    static start(): WorldBridge {
        return new WorldBridge(
            spawn("cargo", ["run", "--offline", "-q", "-p", "tiler-protocol", "--example", "world_eval"], {
                cwd: repoRoot(),
                stdio: ["pipe", "pipe", "pipe"],
                env: { ...process.env, WORLD_EVAL_ALIGNED: "1" },
            }),
        );
    }

    send(method: string, payload: string): Promise<string> {
        if (this.dead !== null) {
            return Promise.reject(new Error(this.dead));
        }
        if (this.proc.exitCode !== null || this.proc.signalCode !== null) {
            return Promise.reject(new Error("world_eval process already exited"));
        }
        const line = JSON.stringify({ method, body: payload });
        return new Promise<string>((resolve, reject) => {
            this.waiters.push({ resolve, reject });
            try {
                this.proc.stdin?.write(line + "\n");
            } catch (error) {
                reject(error instanceof Error ? error : new Error(String(error)));
            }
        });
    }

    async close(): Promise<void> {
        try {
            this.proc.stdin?.end();
        } catch (error) {
            void error;
        }
        await new Promise<void>((resolve) => {
            const timer = setTimeout(() => {
                try {
                    this.proc.kill("SIGKILL");
                } catch (error) {
                    void error;
                }
                resolve();
            }, 3000);
            this.proc.on("exit", () => {
                clearTimeout(timer);
                resolve();
            });
        });
    }
}

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

interface FakeDesktop {
    readonly id: string;
    name: string;
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
    minSize: { width: number; height: number } | null;
    maxSize: { width: number; height: number } | null;
    readonly outputChanged: FakeSignal["signal"];
    readonly desktopsChanged: FakeSignal["signal"];
    readonly __signals?: { readonly outputChanged: FakeSignal; readonly desktopsChanged: FakeSignal };
}

interface CapturedCall {
    readonly method: string;
    readonly payload: string;
    readonly callback: (reply: unknown) => void;
}

interface Timer {
    delayMs: number;
    callback: () => void;
    cancelled: boolean;
    fired: boolean;
}

interface Harness {
    readonly bridge: WorldBridge;
    readonly broker: MiniBroker;
    readonly wins: FakeWindow[];
    readonly desktops: FakeDesktop[];
    readonly screens: Array<{ name: string; geometry: Record<string, number> }>;
    readonly currents: Map<string, string>;
    readonly calls: CapturedCall[];
    readonly answered: Set<number>;
    readonly replies: Map<number, string>;
    readonly takeReplies: string[];
    readonly timers: Timer[];
    readonly logs: string[];
    readonly workspaceFires: string[];
    activeScreen: { name: string };
    activeRef: FakeWindow | null;
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    fire: (name: string, payload?: unknown) => void;
    fireWindow: (win: FakeWindow, name: "outputChanged" | "desktopsChanged") => void;
    runTimers: (maxDelay?: number) => void;
    flush: () => Promise<number>;
    flushAll: () => Promise<void>;
    settle: () => Promise<void>;
    stop: () => Promise<void>;
}

const LEFT = "out-left";
const RIGHT = "out-right";

async function makeHarness(): Promise<Harness> {
    const desktops: FakeDesktop[] = ["ws-1", "ws-2", "ws-3", "ws-4"].map((id) => ({ id, name: id }));
    const byId = new Map(desktops.map((d) => [d.id, d]));
    const screens = [
        { name: LEFT, geometry: { x: 0, y: 0, width: 1000, height: 800 } },
        { name: RIGHT, geometry: { x: 1000, y: 50, width: 900, height: 700 } },
    ];
    const currents = new Map([[LEFT, "ws-1"], [RIGHT, "ws-2"]]);
    const mkWin = (
        id: string, out: string, ws: string,
        frame: { x: number; y: number; width: number; height: number },
    ): FakeWindow => {
        const outputChanged = makeSignal();
        const winDesktopsChanged = makeSignal();
        return {
            internalId: id,
            normalWindow: true,
            frameGeometry: frame,
            output: { name: out },
            desktops: [byId.get(ws) as FakeDesktop],
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: null,
            maxSize: null,
            outputChanged: outputChanged.signal,
            desktopsChanged: winDesktopsChanged.signal,
            __signals: { outputChanged, desktopsChanged: winDesktopsChanged },
        };
    };
    const wins: FakeWindow[] = [
        mkWin("win-a", RIGHT, "ws-1", { x: 1420, y: 102, width: 400, height: 300 }),
        mkWin("win-b", RIGHT, "ws-2", { x: 1008, y: 102, width: 884, height: 600 }),
        mkWin("win-left", LEFT, "ws-1", { x: 8, y: 52, width: 600, height: 700 }),
    ];
    const signals: Record<string, FakeSignal> = {};
    for (const name of ["windowAdded", "windowRemoved", "windowActivated", "desktopsChanged", "screensChanged", "currentDesktopChanged"] as const) {
        signals[name] = makeSignal();
    }
    const harness: Harness = {
        bridge: WorldBridge.start(),
        broker: createMiniBroker(),
        wins,
        desktops,
        screens,
        currents,
        calls: [],
        answered: new Set<number>(),
        replies: new Map<number, string>(),
        takeReplies: [],
        timers: [],
        logs: [],
        workspaceFires: [],
        activeScreen: { name: RIGHT },
        activeRef: wins[0] as FakeWindow,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        fire: (name: string, payload?: unknown): void => {
            harness.workspaceFires.push(name);
            for (const handler of [...(signals[name] as FakeSignal).handlers]) {
                handler(payload);
            }
        },
        fireWindow: (win: FakeWindow, name: "outputChanged" | "desktopsChanged"): void => {
            const sig = win.__signals?.[name];
            assert.ok(sig !== undefined, `window ${win.internalId} exposes ${name}`);
            for (const handler of [...sig.handlers]) {
                handler();
            }
        },
        runTimers: (maxDelay = WORLD_DEBOUNCE_MS): void => {
            for (let i = 0; i < harness.timers.length; i += 1) {
                const timer = harness.timers[i] as Timer;
                if (timer.delayMs <= maxDelay && !timer.cancelled && !timer.fired) {
                    timer.fired = true;
                    harness.timers.splice(i, 1);
                    i -= 1;
                    timer.callback();
                }
            }
        },
        flush: async () => {
            const index = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
            assert.ok(index >= 0, "expected a queued DescribeWorld dispatch to flush to the real Engine");
            // Mini-broker stamping: the envelope becomes the exact core
            // report the sole broker would dispatch (own namespace plus at
            // most one queued command), and the core reply returns bound
            // with a nulled terminal result.
            const envelope = (harness.calls[index] as CapturedCall).payload;
            const stamped = harness.broker.stampCapture(envelope);
            const coreReply = await harness.bridge.send("DescribeWorld", stamped);
            const reply = harness.broker.bindReply(coreReply, envelope);
            harness.answered.add(index);
            harness.replies.set(index, reply);
            (harness.calls[index] as CapturedCall).callback(reply);
            return index;
        },
        flushAll: async (): Promise<void> => {
            for (let round = 0; round < 32; round += 1) {
                const next = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
                if (next < 0) {
                    return;
                }
                await harness.flush();
            }
            throw new Error("flushAll did not converge: planner keeps dispatching");
        },
        settle: async (): Promise<void> => {
            for (let round = 0; round < 30; round += 1) {
                harness.runTimers();
                const before = harness.calls.length;
                await harness.flushAll();
                harness.runTimers();
                await harness.flushAll();
                if (harness.calls.length === before) {
                    return;
                }
            }
            throw new Error("settle did not converge");
        },
        stop: async (): Promise<void> => {
            try {
                harness.handle?.stop();
            } catch (error) {
                void error;
            }
            await harness.bridge.close();
        },
    };
    const desktopById = (id: string): FakeDesktop => {
        const found = byId.get(id);
        assert.ok(found !== undefined, `desktop ${id} exists`);
        return found as FakeDesktop;
    };
    const screenByName = (name: string): { name: string; geometry: Record<string, number> } => {
        const found = harness.screens.find((s) => s.name === name);
        assert.ok(found !== undefined, `screen ${name} exists`);
        return found as { name: string; geometry: Record<string, number> };
    };
    const surface: Record<string, unknown> = {
        windowList: (): FakeWindow[] => [...harness.wins],
        screens: harness.screens,
        desktops: harness.desktops,
        get activeScreen(): { name: string } {
            return harness.activeScreen;
        },
        set activeScreen(ref: unknown) {
            void ref;
        },
        cursorPos: { x: 2, y: 100 },
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        get activeWindow(): FakeWindow | null {
            return harness.activeRef;
        },
        set activeWindow(ref: unknown) {
            const found = harness.wins.find((win) => (win as unknown) === ref);
            if (found !== undefined && harness.activeRef !== found) {
                harness.activeRef = found;
                harness.fire("windowActivated");
            }
        },
        clientArea: (_kind: unknown, outRef: unknown, _deskRef: unknown): Record<string, number> => {
            const name = (outRef as { name: string }).name;
            const geom = screenByName(name).geometry;
            return { x: geom["x"] as number, y: geom["y"] as number, width: geom["width"] as number, height: geom["height"] as number };
        },
        currentDesktopForScreen: (screen: { name: string }): FakeDesktop => {
            return desktopById(harness.currents.get(screen.name) ?? "");
        },
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }): void => {
            const id = (deskRef as FakeDesktop).id;
            assert.ok(byId.has(id), `desktop ${id} exists`);
            harness.currents.set(screen.name, id);
            harness.fire("currentDesktopChanged");
        },
        // Faithful pinned `sendClientToScreen` echo with window-level
        // signals only (no workspace signal).
        sendClientToScreen: (mover: unknown, out: unknown): void => {
            const win = mover as FakeWindow;
            const name = (out as { name: string }).name;
            const from = win.output.name;
            win.output = { name };
            harness.fireWindow(win, "outputChanged");
            if (win.desktops.length === 1) {
                const home = (win.desktops[0] as FakeDesktop).id;
                const oldCurrent = harness.currents.get(from) ?? "";
                const newCurrent = harness.currents.get(name) ?? "";
                if (home === oldCurrent && newCurrent !== oldCurrent && home !== newCurrent) {
                    win.desktops = [desktopById(newCurrent)];
                }
            }
        },
        // Production void creation: synchronously append one native
        // desktop carrying the logical token as its name. Same-turn
        // before/after enumeration proves the created id causally.
        createDesktop: (position: unknown, name: unknown): void => {
            assert.equal(typeof position, "number", "creation carries a numeric position");
            assert.equal(typeof name, "string", "creation carries a logical token name");
            const id = `ws-${harness.desktops.length + 1}`;
            const desk: FakeDesktop = { id, name: name as string };
            const at = Math.max(0, Math.min(position as number, harness.desktops.length));
            harness.desktops.splice(at, 0, desk);
            byId.set(id, desk);
            harness.fire("desktopsChanged");
        },
        removeDesktop: (): boolean => false,
    };
    for (const win of wins) {
        let desks = win.desktops;
        Object.defineProperty(win, "desktops", {
            configurable: true,
            get: (): FakeDesktop[] => desks,
            set: (refs: FakeDesktop[]) => {
                desks = refs;
                for (const handler of [...(win.__signals?.desktopsChanged.handlers ?? [])]) {
                    handler();
                }
            },
        });
    }
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback): void => {
            if (method !== "DiscoverWorld" && method !== "DescribeWorld" && method !== "QueueWorldCommand" && method !== "TakeCommandResult") {
                return;
            }
            harness.calls.push({ method, payload, callback });
            if (method === "QueueWorldCommand") {
                callback(harness.broker.answerQueue(payload));
            } else if (method === "TakeCommandResult") {
                const takeReply = harness.broker.answerTake(payload);
                harness.takeReplies.push(takeReply);
                callback(takeReply);
            }
        },
        scheduleOnce: (delayMs, callback): (() => void) => {
            const timer: Timer = { delayMs, callback, cancelled: false, fired: false };
            harness.timers.push(timer);
            return (): void => {
                timer.cancelled = true;
            };
        },
        log: (message: string): void => {
            harness.logs.push(message);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
    });
    assert.notEqual(handle, null, "world production entry starts on the fake surface");
    (harness as { handle: unknown }).handle = handle;
    return harness;
}

async function discover(h: Harness): Promise<void> {
    const discIndex = h.calls.findIndex((call) => call.method === "DiscoverWorld");
    assert.ok(discIndex >= 0, "entry discovers on start");
    const disc = h.calls[discIndex] as CapturedCall;
    const engineBootstrap = await h.bridge.send(disc.method, disc.payload);
    const reply = h.broker.bindDiscovery(engineBootstrap);
    disc.callback(reply);
    await h.settle();
}

function lastReply(h: Harness): Record<string, unknown> {
    const indices = [...h.replies.keys()].filter((at) => (h.calls[at] as CapturedCall).method === "DescribeWorld");
    assert.ok(indices.length > 0, "at least one answered DescribeWorld");
    const raw = h.replies.get(Math.max(...indices) as number) as string;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    assert.equal(parsed["outcome"] ?? "accepted", "accepted", `fresh reply accepted, got ${raw.slice(0, 300)}`);
    return parsed;
}

function transfersOf(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return (reply["transfers"] as Array<Record<string, unknown>>) ?? [];
}

function homingOf(h: Harness, index: number, win: string): { out: string; ws: string } | null {
    // v2: the inner report carries window facts with the observed native
    // workspace (never a domains lane); native truth resolves through the
    // producer-owned id.
    const body = JSON.parse((h.calls[index] as CapturedCall).payload) as Record<string, unknown>;
    const report = body["report"] as Record<string, unknown>;
    for (const w of (report["windows"] as Array<Record<string, unknown>>) ?? []) {
        if (w["win"] === win) {
            return { out: w["out"] as string, ws: w["nativeWs"] as string };
        }
    }
    return null;
}

// Faithful external KWin native output drag: output changes plus the
// pinned single-desktop echo, emitting ONLY window-level signals (never
// a workspace signal). Currents, active window, and the window-list
// roster stay unchanged by construction.
function externalNativeOutputMove(h: Harness, winId: string, targetOut: string): void {
    const win = h.wins.find((w) => w.internalId === winId);
    assert.ok(win !== undefined, `window ${winId} exists`);
    const from = (win as FakeWindow).output.name;
    assert.notEqual(from, targetOut, "external move crosses outputs");
    (win as FakeWindow).output = { name: targetOut };
    h.fireWindow(win as FakeWindow, "outputChanged");
    const desks = (win as FakeWindow).desktops;
    if (desks.length === 1) {
        const home = (desks[0] as FakeDesktop).id;
        const oldCurrent = h.currents.get(from) ?? "";
        const newCurrent = h.currents.get(targetOut) ?? "";
        if (home === oldCurrent && newCurrent !== oldCurrent && home !== newCurrent) {
            const target = h.desktops.find((d) => d.id === newCurrent) as FakeDesktop;
            (win as FakeWindow).desktops = [target];
        }
    }
}

describe("world native homing", () => {
    it("external output moves both directions plus same-output desktop change converge with no workspace signal", { timeout: 240000 }, async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            h.fire("desktopsChanged");
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "no transfers before external input");

            // Leg 1: external drag win-a RIGHT(ws-1) -> LEFT. LEFT current
            // is ws-1 so no echo; homing becomes LEFT/ws-1.
            const firesBefore1 = h.workspaceFires.length;
            const callsBefore1 = h.calls.length;
            externalNativeOutputMove(h, "win-a", LEFT);
            assert.equal(h.workspaceFires.length, firesBefore1, "external move fires no workspace signal");
            // The production debounce alone must queue the report: no
            // explicit workspace signal or timer oracle shortcut.
            h.runTimers();
            assert.ok(h.calls.length > callsBefore1, "per-window homing autonomously dispatches (fail-before: no dispatch without window subscriptions)");
            await h.settle();
            const leg1 = lastReply(h);
            assert.deepEqual(transfersOf(leg1), [], "leg 1 settles with no retained transfers");
            assert.deepEqual(homingOf(h, h.calls.length - 1, "win-a"), { out: LEFT, ws: "ws-1" }, "leg 1 homing converges");

            // Leg 2: external drag win-a back LEFT -> RIGHT. RIGHT
            // current is ws-2 while win-a sits on ws-1 (LEFT current),
            // so the pinned echo re-homes to ws-2.
            const firesBefore2 = h.workspaceFires.length;
            const callsBefore2 = h.calls.length;
            externalNativeOutputMove(h, "win-a", RIGHT);
            assert.equal(h.workspaceFires.length, firesBefore2, "external move fires no workspace signal");
            h.runTimers();
            assert.ok(h.calls.length > callsBefore2, "return move autonomously dispatches");
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "leg 2 settles with no retained transfers");
            assert.deepEqual(homingOf(h, h.calls.length - 1, "win-a"), { out: RIGHT, ws: "ws-2" }, "leg 2 echo homing converges");

            // Leg 3: external same-output desktop change (win-b
            // RIGHT/ws-2 -> RIGHT/ws-1) with only a window signal.
            const winB = h.wins.find((w) => w.internalId === "win-b") as FakeWindow;
            const firesBefore3 = h.workspaceFires.length;
            const callsBefore3 = h.calls.length;
            const ws1 = h.desktops.find((d) => d.id === "ws-1") as FakeDesktop;
            winB.desktops = [ws1];
            assert.equal(h.workspaceFires.length, firesBefore3, "same-output change fires no workspace signal");
            h.runTimers();
            assert.ok(h.calls.length > callsBefore3, "same-output desktop change autonomously dispatches");
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "leg 3 settles with no retained transfers");
            assert.deepEqual(homingOf(h, h.calls.length - 1, "win-b"), { out: RIGHT, ws: "ws-1" }, "leg 3 homing converges");

            // Leg 4: own window send still settles exactly once (no
            // loop/replay/stall from the new subscriptions).
            // v2 command lane: presses file QueueWorldCommand; DescribeWorld
            // envelopes carry no inline command.
            const sendsBefore = h.calls.filter((c) => {
                if (c.method !== "QueueWorldCommand") {
                    return false;
                }
                try {
                    return ((JSON.parse(c.payload) as Record<string, unknown>)["command"] as Record<string, unknown>)["op"] === "send-directional";
                } catch {
                    return false;
                }
            }).length;
            h.activeScreen = { name: RIGHT };
            h.activeRef = h.wins.find((w) => w.internalId === "win-b") as FakeWindow;
            // Focus legibility: win-b sits on RIGHT/ws-1 after leg 3,
            // so show that cell before the own send.
            h.currents.set(RIGHT, "ws-1");
            h.fire("currentDesktopChanged");
            await h.settle();
            h.handle.requestSendToOutput("left", true);
            await h.settle();
            const sendsAfter = h.calls.filter((c) => {
                if (c.method !== "QueueWorldCommand") {
                    return false;
                }
                try {
                    return ((JSON.parse(c.payload) as Record<string, unknown>)["command"] as Record<string, unknown>)["op"] === "send-directional";
                } catch {
                    return false;
                }
            }).length;
            assert.equal(sendsAfter - sendsBefore, 1, "own window send dispatches exactly once");
            assert.deepEqual(transfersOf(lastReply(h)), [], "own send settles with no retained transfers");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")), "no busy loops");
        } finally {
            await h.stop();
        }
    });

    it("window attach/remove/stop leave no stale callbacks and own migration settles once", { timeout: 240000 }, async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            h.fire("desktopsChanged");
            await h.settle();

            // Own migration first on the clean grid (mirrors the producer
            // leg 1 shape): select the win-a cell, then migrate left
            // exactly once with no replay and no stall from the new
            // per-window subscriptions.
            h.currents.set(RIGHT, "ws-1");
            h.activeScreen = { name: RIGHT };
            h.activeRef = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            h.fire("currentDesktopChanged");
            await h.settle();
            const migrateBefore = h.calls.filter((c) => {
                if (c.method !== "QueueWorldCommand") {
                    return false;
                }
                try {
                    return ((JSON.parse(c.payload) as Record<string, unknown>)["command"] as Record<string, unknown> | null)?.["op"] === "migrate-directional";
                } catch {
                    return false;
                }
            }).length;
            h.handle.requestWorkspaceMigrate("left");
            await h.settle();
            const migrateAfter = h.calls.filter((c) => {
                if (c.method !== "QueueWorldCommand") {
                    return false;
                }
                try {
                    return ((JSON.parse(c.payload) as Record<string, unknown>)["command"] as Record<string, unknown> | null)?.["op"] === "migrate-directional";
                } catch {
                    return false;
                }
            }).length;
            assert.equal(migrateAfter - migrateBefore, 1, "own migration dispatches exactly once");
            assert.deepEqual(transfersOf(lastReply(h)), [], "migration settles with no retained transfers");

            // Attach: a new window arrives with its payload; its later
            // homing signal must dispatch.
            const desks = h.desktops;
            const ws1 = desks.find((d) => d.id === "ws-1") as FakeDesktop;
            const freshOutputChanged = makeSignal();
            const freshDesktopsChanged = makeSignal();
            const fresh = {
                internalId: "win-new",
                normalWindow: true,
                frameGeometry: { x: 20, y: 60, width: 300, height: 200 },
                output: { name: LEFT },
                desktops: [ws1],
                minimized: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
                minSize: null,
                maxSize: null,
                outputChanged: freshOutputChanged.signal,
                desktopsChanged: freshDesktopsChanged.signal,
                __signals: { outputChanged: freshOutputChanged, desktopsChanged: freshDesktopsChanged },
            } as unknown as FakeWindow;
            let freshDesks = fresh.desktops;
            Object.defineProperty(fresh, "desktops", {
                configurable: true,
                get: (): FakeDesktop[] => freshDesks,
                set: (refs: FakeDesktop[]) => {
                    freshDesks = refs;
                    for (const handler of [...(fresh.__signals?.desktopsChanged.handlers ?? [])]) {
                        handler();
                    }
                },
            });
            h.wins.push(fresh);
            const callsBeforeAttach = h.calls.length;
            h.fire("windowAdded", fresh);
            h.runTimers();
            await h.flushAll();
            h.runTimers();
            await h.flushAll();
            assert.ok(h.calls.length > callsBeforeAttach, "attached window dispatches");
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "attach settles");

            // The attached window's own homing signal dispatches too.
            const ws2 = desks.find((d) => d.id === "ws-2") as FakeDesktop;
            const callsBeforeHoming = h.calls.length;
            const firesBeforeHoming = h.workspaceFires.length;
            fresh.desktops = [ws2];
            assert.equal(h.workspaceFires.length, firesBeforeHoming, "attached window homing fires no workspace signal");
            h.runTimers();
            assert.ok(h.calls.length > callsBeforeHoming, "attached window homing dispatches");
            await h.settle();

            // Remove: the window leaves; its detached signals must never
            // dispatch again (no stale callbacks).
            const at = h.wins.findIndex((w) => w.internalId === "win-new");
            assert.ok(at >= 0, "attached window present before remove");
            const [removed] = h.wins.splice(at, 1);
            h.fire("windowRemoved", removed);
            await h.settle();
            const callsAfterRemove = h.calls.length;
            for (const handler of [...(fresh.__signals?.desktopsChanged.handlers ?? [])]) {
                handler();
            }
            for (const handler of [...(fresh.__signals?.outputChanged.handlers ?? [])]) {
                handler();
            }
            h.runTimers();
            assert.equal(h.calls.length, callsAfterRemove, "removed window emits no further dispatch (stale callbacks detached)");

            // Stop: no signal dispatches afterwards.
            h.handle.stop();
            const callsAtStop = h.calls.length;
            const winA = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            for (const handler of [...(winA.__signals?.desktopsChanged.handlers ?? [])]) {
                handler();
            }
            h.fire("desktopsChanged");
            h.runTimers();
            assert.equal(h.calls.length, callsAtStop, "no dispatch after stop");
        } finally {
            await h.stop();
        }
    });
});
