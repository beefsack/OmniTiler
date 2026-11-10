// Dynamic workspace lifecycle: producer -> real Engine -> publish -> apply.
// Covers the 2026-11-10 decision through the production entry/apply path
// with a fake host and fresh serial replies: adapter bootstrap pads an
// unoccupied single native desktop to the two-desktop floor with no core
// desire on any reply and no creation receipt (proving the floor, not a
// desire, created the native), unreadable inventory holds the pad with no
// creation until readable, every trailing spare requests eager native
// backing immediately (the old
// occupancy-only `world-aligned-spare-occupancy` spare therefore grows
// natives), filling a spare mints the next one, leading/middle empties
// prune only after leaving while the viewed empty is retained, no
// inverse-fill/retire oscillation, globally occupied or visible natives
// never delete, and retired reselect readopts fresh at the native order
// position. Every step counts actual native creates/removes plus window
// membership writes (not mere desires); spare/prune steps take zero
// membership writes. Included in `npm test`.

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { createInterface } from "node:readline";
import { describe, it } from "node:test";

import { WORLD_DEBOUNCE_MS, startWorldAdapterEntry } from "../src/world-entry";
import { createMiniBroker, type MiniBroker } from "./world-wire-harness";

class WorldBridge {
    private readonly proc: ChildProcess;
    private readonly waiters: Array<{
        resolve: (reply: string) => void;
        reject: (error: Error) => void;
    }> = [];
    private dead: string | null = null;
    private stderrTail = "";

    private constructor(proc: ChildProcess) {
        this.proc = proc;
        const stdout = proc.stdout;
        if (stdout === null || stdout === undefined) {
            throw new Error("raw_world_eval stdout unavailable");
        }
        createInterface({ input: stdout }).on("line", (line: string) => {
            this.waiters.shift()?.resolve(line);
        });
        proc.stderr?.on("data", (chunk: unknown) => {
            this.stderrTail = (this.stderrTail + String(chunk)).slice(-8192);
        });
        proc.on("error", (error) => {
            this.failAll(error instanceof Error ? error : new Error(String(error)));
        });
        proc.on("close", (code, signal) => {
            this.failAll(new Error(`raw_world_eval exited with code ${String(code)} signal=${signal} stderr=${this.stderrTail.slice(-2000)}`));
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
            spawn("cargo", ["run", "--offline", "-q", "-p", "omnitiler", "--example", "raw_world_eval"], {
                stdio: ["pipe", "pipe", "pipe"],
            }),
        );
    }

    send(method: string, payload: string): Promise<string> {
        if (this.dead !== null) {
            return Promise.reject(new Error(this.dead));
        }
        if (this.proc.exitCode !== null || this.proc.signalCode !== null) {
            return Promise.reject(new Error("raw_world_eval process already exited"));
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
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    clientGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    hidden: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minSize: { width: number; height: number };
    maxSize: { width: number; height: number };
    pid: number;
    resourceClass: string;
    caption: string;
    windowRole: string;
    desktopFileName: string;
    resizeable: boolean;
    keepAbove: boolean;
    keepBelow: boolean;
    readonly outputChanged: FakeSignal["signal"];
    readonly desktopsChanged: FakeSignal["signal"];
    readonly hiddenChanged: FakeSignal["signal"];
    readonly frameGeometryChanged: FakeSignal["signal"];
    readonly __signals: { readonly outputChanged: FakeSignal; readonly desktopsChanged: FakeSignal; readonly hiddenChanged: FakeSignal; readonly frameGeometryChanged: FakeSignal };
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
    readonly viewWrites: string[];
    // Actual native setter counts (not desires): creations append here with
    // the fresh id, removals append here, every window `desktops` assignment
    // and cross-output send appends to memberWrites.
    readonly creations: string[];
    readonly removals: string[];
    readonly memberWrites: string[];
    activeScreen: { name: string };
    activeRef: FakeWindow | null;
    // Test-only inventory fault: when true the surface reports no native
    // desktops (production unreadable-inventory shape). Host truth
    // (desktops/byId/currents) is untouched; only adapter reads go blind.
    hideNatives: boolean;
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    readonly mode: string;
    fire: (name: string, payload?: unknown) => void;
    runTimers: (maxDelay?: number) => void;
    flush: () => Promise<number>;
    flushAll: () => Promise<void>;
    settle: () => Promise<void>;
    stop: () => Promise<void>;
}

interface MakeOpts {
    readonly mode?: string;
    readonly desktops?: string[];
    readonly outputs?: string[];
    readonly currents?: Record<string, string>;
    readonly homes?: Record<string, string>;
    readonly wins?: Array<{ id: string; out: string; ws: string }>;
}

function trackWin(h: Harness, win: FakeWindow): void {
    let desks = win.desktops;
    Object.defineProperty(win, "desktops", {
        configurable: true,
        get: () => desks,
        set: (refs: FakeDesktop[]) => {
            // Count only genuine membership changes: the fake host's own
            // removal bookkeeping reassigns the same id set (a no-op
            // filter), which is never an adapter move.
            const before = desks.map((d) => d.id).join(",");
            desks = refs;
            if (refs.map((d) => d.id).join(",") !== before) {
                h.memberWrites.push(`${win.internalId}->${refs.map((d) => d.id).join(",")}`);
            }
            return true;
        },
    });
}

async function makeLifecycleHarness(opts: MakeOpts = {}): Promise<Harness> {
    const mode = opts.mode ?? "per-output-local";
    const desktopIds = opts.desktops ?? ["ws-1", "ws-2"];
    assert.ok(desktopIds.length > 0, "at least one native desktop");
    const desktops: FakeDesktop[] = desktopIds.map((id) => ({ id, name: id }));
    const byId = new Map(desktops.map((d) => [d.id, d]));
    const screens = (opts.outputs ?? ["out"]).map((name, index) => ({
        name,
        geometry: { x: index * 1000, y: 0, width: 1000, height: 800 },
    }));
    const currents = new Map<string, string>();
    for (const s of screens) {
        const want = opts.currents?.[s.name] ?? desktops[0]?.id ?? "";
        assert.ok(byId.has(want), `current desktop ${want} exists`);
        currents.set(s.name, want);
    }
    // Monotonic native ids: removals shrink the list, so `length + 1` would
    // collide with a survivor. Production ids are opaque uniques; the fake
    // mints `ws-N` from a counter that never rewinds.
    let nextDesk = desktops.length + 1;
    const homeOf = (id: string): string => opts.homes?.[id] ?? screens[0]?.name ?? "";
    const mkWin = (id: string, out: string, ws: string): FakeWindow => {
        const outputChanged = makeSignal();
        const winDesktopsChanged = makeSignal();
        const winHiddenChanged = makeSignal();
        const winFrameChanged = makeSignal();
        const frame = { x: 8, y: 52, width: 200, height: 200 };
        return {
            internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { ...frame },
            clientGeometry: { x: frame.x + 2, y: frame.y + 21, width: Math.max(1, frame.width - 4), height: Math.max(1, frame.height - 22) },
            output: { name: out },
            desktops: [byId.get(ws) as FakeDesktop],
            minimized: false,
            hidden: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: { width: 100, height: 100 },
            maxSize: { width: 2147483647, height: 2147483647 },
            pid: 1234,
            resourceClass: "Example",
            caption: "Window",
            windowRole: "normal",
            desktopFileName: "example.app",
            resizeable: true,
            keepAbove: false,
            keepBelow: false,
            outputChanged: outputChanged.signal,
            desktopsChanged: winDesktopsChanged.signal,
            hiddenChanged: winHiddenChanged.signal,
            frameGeometryChanged: winFrameChanged.signal,
            __signals: { outputChanged, desktopsChanged: winDesktopsChanged, hiddenChanged: winHiddenChanged, frameGeometryChanged: winFrameChanged },
        };
    };
    const seedWins = opts.wins ?? [];
    const wins: FakeWindow[] = seedWins.map((w) => mkWin(w.id, w.out, w.ws));
    const sigNames = [
        "windowAdded",
        "windowRemoved",
        "windowActivated",
        "desktopsChanged",
        "screensChanged",
        "currentDesktopChanged",
    ] as const;
    const signals: Record<string, FakeSignal> = {};
    for (const name of sigNames) {
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
        viewWrites: [],
        creations: [],
        removals: [],
        memberWrites: [],
        activeScreen: { name: screens[0]?.name ?? "" },
        activeRef: (wins[0] as FakeWindow | undefined) ?? null,
        hideNatives: false,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        mode,
        fire: (name: string, payload?: unknown): void => {
            for (const handler of [...(signals[name] as FakeSignal).handlers]) {
                handler(payload);
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
    for (const win of wins) {
        trackWin(harness, win);
    }
    void homeOf;
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
        get desktops(): FakeDesktop[] {
            return harness.hideNatives ? [] : harness.desktops;
        },
        get activeScreen(): { name: string } {
            return harness.activeScreen;
        },
        set activeScreen(_ref: unknown) {
            void _ref;
        },
        get cursorPos(): { x: number; y: number } {
            return { x: 2, y: 100 };
        },
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
            if (found !== undefined) {
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
            assert.ok(byId.has(id) || harness.desktops.some((d) => d.id === id), `desktop ${id} exists`);
            harness.currents.set(screen.name, id);
            harness.viewWrites.push(`${screen.name}->${id}`);
            harness.fire("currentDesktopChanged");
        },
        sendClientToScreen: (mover: unknown, out: unknown): void => {
            const win = mover as FakeWindow;
            const name = (out as { name: string }).name;
            win.output = { name };
            harness.memberWrites.push(`${win.internalId}=>${name}`);
        },
        createDesktop: (position: unknown, name: unknown): void => {
            assert.equal(typeof position, "number", "creation carries a numeric position");
            assert.equal(typeof name, "string", "creation carries a name");
            const id = `ws-${nextDesk}`;
            nextDesk += 1;
            const desk: FakeDesktop = { id, name: name as string };
            const at = Math.max(0, Math.min(position as number, harness.desktops.length));
            harness.desktops.splice(at, 0, desk);
            byId.set(id, desk);
            harness.creations.push(id);
            harness.fire("desktopsChanged");
        },
        removeDesktop: (ref: unknown): boolean => {
            const id = (ref as FakeDesktop).id;
            const at = harness.desktops.findIndex((d) => d.id === id);
            if (at < 0) {
                return false;
            }
            harness.desktops.splice(at, 1);
            byId.delete(id);
            for (const win of harness.wins) {
                win.desktops = win.desktops.filter((d) => d.id !== id);
            }
            harness.removals.push(id);
            harness.fire("desktopsChanged");
            return true;
        },
    };
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
        log: (): void => undefined,
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => mode,
        readDefaultTiledFn: () => true,
    });
    assert.notEqual(handle, null, "world production entry starts on the fake surface");
    (harness as { handle: unknown }).handle = handle;
    return harness;
}

function lastReply(h: Harness): Record<string, unknown> {
    const indices = [...h.replies.keys()].filter((at) => (h.calls[at] as CapturedCall).method === "DescribeWorld");
    assert.ok(indices.length > 0, "at least one answered DescribeWorld");
    const raw = h.replies.get(Math.max(...indices) as number) as string;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    assert.equal(parsed["outcome"] ?? "accepted", "accepted", `fresh reply accepted, got ${raw.slice(0, 300)}`);
    return parsed;
}

function workspacesOf(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return (reply["workspaces"] as Array<Record<string, unknown>>) ?? [];
}

function tombstonesOf(reply: Record<string, unknown>, out?: string): Array<Record<string, unknown>> {
    return workspacesOf(reply).filter((row) => row["exist"] === false && (out === undefined || row["out"] === out));
}

async function discoverOnly(h: Harness): Promise<void> {
    const hsIndex = h.calls.findIndex((call) => call.method === "DiscoverWorld");
    assert.ok(hsIndex >= 0, "entry discovers on start");
    const hsBody = JSON.parse((h.calls[hsIndex] as CapturedCall).payload) as Record<string, unknown>;
    assert.equal(hsBody["type"], "handshake", "first discovery is the handshake");
    const hsReplyText = await h.bridge.send("DiscoverWorld", (h.calls[hsIndex] as CapturedCall).payload);
    const hsReply = JSON.parse(hsReplyText) as Record<string, unknown>;
    assert.equal(hsReply["type"], "handshake-reply", "engine answers the handshake");
    const gen = hsReply["gen"] as string;
    const boundHs = h.broker.bindDiscovery(hsReplyText);
    (h.calls[hsIndex] as CapturedCall).callback(boundHs);
    const setIndex = h.calls.findIndex((call, at) => at > hsIndex && call.method === "DiscoverWorld");
    assert.ok(setIndex >= 0, "revisioned settings follow the handshake");
    const setReplyText = await h.bridge.send("DiscoverWorld", (h.calls[setIndex] as CapturedCall).payload);
    (h.calls[setIndex] as CapturedCall).callback(setReplyText);
    void gen;
}

async function discoverAndSettle(h: Harness): Promise<void> {
    const hsIndex = h.calls.findIndex((call) => call.method === "DiscoverWorld");
    assert.ok(hsIndex >= 0, "entry discovers on start");
    const hsBody = JSON.parse((h.calls[hsIndex] as CapturedCall).payload) as Record<string, unknown>;
    assert.equal(hsBody["type"], "handshake", "first discovery is the handshake");
    const hsReplyText = await h.bridge.send("DiscoverWorld", (h.calls[hsIndex] as CapturedCall).payload);
    const hsReply = JSON.parse(hsReplyText) as Record<string, unknown>;
    assert.equal(hsReply["type"], "handshake-reply", "engine answers the handshake");
    const gen = hsReply["gen"] as string;
    const boundHs = h.broker.bindDiscovery(hsReplyText);
    (h.calls[hsIndex] as CapturedCall).callback(boundHs);
    const setIndex = h.calls.findIndex((call, at) => at > hsIndex && call.method === "DiscoverWorld");
    assert.ok(setIndex >= 0, "revisioned settings follow the handshake");
    const setReplyText = await h.bridge.send("DiscoverWorld", (h.calls[setIndex] as CapturedCall).payload);
    (h.calls[setIndex] as CapturedCall).callback(setReplyText);
    await h.settle();
    void gen;
}

// Bounded report pump without requiring convergence: while the native
// inventory is unreadable the planner keeps polling on empty
// observations, which is fine; what matters is that nothing is created.
async function pumpReports(h: Harness, rounds: number): Promise<void> {
    for (let round = 0; round < rounds; round += 1) {
        h.runTimers();
        await h.flushAll();
        h.runTimers();
        await h.flushAll();
    }
}

function counters(h: Harness): { natives: number; creations: number; removals: number; members: number; views: number } {
    return {
        natives: h.desktops.length,
        creations: h.creations.length,
        removals: h.removals.length,
        members: h.memberWrites.length,
        views: h.viewWrites.length,
    };
}

// Creation receipts the adapter rode on dispatched reports: the causal
// op-to-native proof for every executed desire. The floor pad emits no
// receipt, so any creation here must carry exactly one.
function reportCreationAttempts(h: Harness): Array<Record<string, unknown>> {
    const out: Array<Record<string, unknown>> = [];
    for (const call of h.calls) {
        if (call.method !== "DescribeWorld") {
            continue;
        }
        try {
            const body = JSON.parse(call.payload) as Record<string, unknown>;
            const report = (body["report"] as Record<string, unknown>) ?? {};
            for (const attempt of (report["creationAttempts"] as Array<Record<string, unknown>>) ?? []) {
                out.push(attempt);
            }
        } catch (error) {
            void error;
        }
    }
    return out;
}

function addWindow(h: Harness, tag: string, out: string, ws: string): void {
    const desk = h.desktops.find((d) => d.id === ws);
    assert.ok(desk !== undefined, `desktop ${ws} exists for newcomer ${tag}`);
    const frame = { x: 8, y: 52, width: 200, height: 200 };
    const outputChanged = makeSignal();
    const winDesktopsChanged = makeSignal();
    const winHiddenChanged = makeSignal();
    const winFrameChanged = makeSignal();
    const win: FakeWindow = {
        internalId: tag, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { ...frame },
        clientGeometry: { x: frame.x + 2, y: frame.y + 21, width: Math.max(1, frame.width - 4), height: Math.max(1, frame.height - 22) },
        output: { name: out },
        desktops: [desk as FakeDesktop],
        minimized: false,
        hidden: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: { width: 100, height: 100 },
        maxSize: { width: 2147483647, height: 2147483647 },
        pid: 1234,
        resourceClass: "Example",
        caption: "Window",
        windowRole: "normal",
        desktopFileName: "example.app",
        resizeable: true,
        keepAbove: false,
        keepBelow: false,
        outputChanged: outputChanged.signal,
        desktopsChanged: winDesktopsChanged.signal,
        hiddenChanged: winHiddenChanged.signal,
        frameGeometryChanged: winFrameChanged.signal,
        __signals: { outputChanged, desktopsChanged: winDesktopsChanged, hiddenChanged: winHiddenChanged, frameGeometryChanged: winFrameChanged },
    };
    trackWin(h, win);
    h.wins.push(win);
}

describe("world dynamic lifecycle", () => {
    it("single native bootstraps to two with no core desire and no membership writes", { timeout: 240000 }, async () => {
        // The floor pad isolates from core desire: the single native is
        // unoccupied, so the core publishes one bound row with no spare
        // and no creation desire on the FIRST reply (an occupied single
        // native would desire its trailing spare, which the desire path
        // would execute instead). The adapter floor pads to the
        // two-desktop minimum with no receipt (the floor owns no core op);
        // the core adopts the padding through ordinary observation.
        const h = await makeLifecycleHarness({
            desktops: ["ws-1"],
            outputs: ["out"],
            currents: { out: "ws-1" },
            wins: [],
        });
        try {
            await discoverAndSettle(h);
            // EVERY reply carries no desire: the first apply (and all later
            // ones) received no core creation desire, so the pad is purely
            // adapter-owned. A desire-path execution would ride a
            // creationAttempts receipt on the next report; no report carries
            // one, proving the floor (not a desire) created the native.
            const answered = [...h.replies.keys()]
                .filter((at) => (h.calls[at] as CapturedCall).method === "DescribeWorld")
                .sort((a, b) => a - b);
            assert.ok(answered.length > 0, "at least one answered DescribeWorld");
            for (const at of answered) {
                const parsed = JSON.parse(h.replies.get(at) as string) as Record<string, unknown>;
                assert.deepEqual(parsed["nativeCreations"], [], `reply ${String(at)} carries no core desire`);
            }
            assert.deepEqual(reportCreationAttempts(h), [], "no creation receipt rides any report (floor owns no core op)");
            // Adapter floor pads one native with no core desire; the core
            // adopts the padding through ordinary observation.
            assert.equal(h.desktops.length, 2, `bootstrap pads to two natives, got ${h.desktops.map((d) => d.id).join(",")}`);
            assert.equal(h.creations.length, 1, `exactly one bootstrap creation, got ${JSON.stringify(h.creations)}`);
            assert.equal(h.removals.length, 0, "bootstrap removes nothing");
            const reply = lastReply(h);
            assert.deepEqual(reply["nativeCreations"], [], "bootstrap needs no core desire");
            assert.equal(h.memberWrites.length, 0, `bootstrap moves no windows, got ${JSON.stringify(h.memberWrites)}`);
            assert.equal(h.viewWrites.length, 0, `bootstrap switches no view, got ${JSON.stringify(h.viewWrites)}`);
            assert.equal(tombstonesOf(reply).length, 0, "bootstrap publishes no absence intent");
            // Stability: repeated settles create and remove nothing (the
            // synchronous native signal inside creation re-enters apply,
            // and the fresh per-step reads never double-pad).
            const before = counters(h);
            await h.settle();
            assert.deepEqual(counters(h), before, "bootstrap outcome is stable");
        } finally {
            await h.stop();
        }
    });

    it("unreadable native inventory holds the floor pad with no creation", { timeout: 240000 }, async () => {
        // Partial inventory never bootstraps: while the surface reports no
        // native desktops the floor holds (no creation, no removal, settle
        // still converges); once readable, the same settle pads to two
        // with no core desire and no receipt.
        const h = await makeLifecycleHarness({
            desktops: ["ws-1"],
            outputs: ["out"],
            currents: { out: "ws-1" },
            wins: [],
        });
        try {
            h.hideNatives = true;
            await discoverOnly(h);
            // Bounded pump while blind: empty observations keep the planner
            // polling (no convergence to await), but nothing is created.
            await pumpReports(h, 5);
            assert.equal(h.desktops.length, 1, `hidden inventory creates nothing, got ${h.desktops.map((d) => d.id).join(",")}`);
            assert.equal(h.creations.length, 0, "hidden inventory performs no creation");
            assert.equal(h.removals.length, 0, "hidden inventory performs no removal");
            h.hideNatives = false;
            await h.settle();
            const answered = [...h.replies.keys()]
                .filter((at) => (h.calls[at] as CapturedCall).method === "DescribeWorld")
                .sort((a, b) => a - b);
            assert.ok(answered.length > 0, "at least one answered DescribeWorld");
            for (const at of answered) {
                const parsed = JSON.parse(h.replies.get(at) as string) as Record<string, unknown>;
                assert.deepEqual(parsed["nativeCreations"], [], `reply ${String(at)} carries no core desire`);
            }
            assert.deepEqual(reportCreationAttempts(h), [], "no creation receipt rides any report (floor owns no core op)");
            assert.equal(h.desktops.length, 2, `readable inventory pads to two, got ${h.desktops.map((d) => d.id).join(",")}`);
            assert.equal(h.creations.length, 1, `exactly one bootstrap creation, got ${JSON.stringify(h.creations)}`);
            assert.equal(tombstonesOf(lastReply(h)).length, 0, "bootstrap publishes no absence intent");
            const before = counters(h);
            await h.settle();
            assert.deepEqual(counters(h), before, "bootstrap outcome is stable");
        } finally {
            await h.stop();
        }
    });

    it("eager spare backs immediately, fill grows the next, zero membership writes", { timeout: 240000 }, async () => {
        const h = await makeLifecycleHarness({
            desktops: ["ws-1", "ws-2"],
            outputs: ["out"],
            currents: { out: "ws-1" },
            wins: [{ id: "win-a", out: "out", ws: "ws-1" }],
        });
        try {
            await discoverAndSettle(h);
            const startNatives = h.desktops.length;
            assert.ok(startNatives >= 2, `floor holds at settle, got ${startNatives}`);
            const base = counters(h);
            // Fill the bound leading end: the trailing spare is already
            // eagerly backed, so this arrival extends the ring with a fresh
            // spare that is backed immediately (one creation, no moves).
            addWindow(h, "win-b", "out", h.desktops[h.desktops.length - 1]?.id ?? "");
            h.fire("windowAdded");
            await h.settle();
            const afterFill = counters(h);
            assert.equal(afterFill.creations - base.creations, 1, `fill backs the next spare exactly once, got creations=${JSON.stringify(h.creations)}`);
            assert.equal(afterFill.removals - base.removals, 0, "fill removes nothing");
            assert.equal(afterFill.members - base.members, 0, `fill moves no windows, got ${JSON.stringify(h.memberWrites)}`);
            const reply = lastReply(h);
            const rows = workspacesOf(reply).filter((row) => row["out"] === "out");
            const live = rows.filter((row) => row["exist"] !== false);
            assert.ok(live.length >= 2, `ring grows, got ${JSON.stringify(live.map((r) => r["id"]))}`);
            // Stability: no inverse-fill/retire oscillation while settled.
            await h.settle();
            assert.deepEqual(counters(h), afterFill, "settled lifecycle does not oscillate");
        } finally {
            await h.stop();
        }
    });

    it("leading empty prunes after leave while viewed empty is retained, then prunes", { timeout: 240000 }, async () => {
        // Viewed middle empty is retained: wins pin both ends while the
        // current sits on the genuinely empty middle.
        const h = await makeLifecycleHarness({
            desktops: ["ws-1", "ws-2", "ws-3"],
            outputs: ["out"],
            currents: { out: "ws-2" },
            wins: [
                { id: "win-a", out: "out", ws: "ws-1" },
                { id: "win-b", out: "out", ws: "ws-3" },
            ],
        });
        try {
            await discoverAndSettle(h);
            // Viewed empty middle is retained while shown: its native
            // survives this settle and no absence intent names it.
            const shown = h.currents.get("out") ?? "";
            assert.equal(shown, "ws-2", "setup holds the viewed empty middle");
            const nativesBefore = h.desktops.map((d) => d.id);
            assert.ok(nativesBefore.includes(shown), `shown native survives, natives=${nativesBefore.join(",")}`);
            const base = counters(h);
            const replyBefore = lastReply(h);
            assert.ok(
                !tombstonesOf(replyBefore, "out").some((row) => row["bind"] === shown),
                `viewed empty publishes no absence intent, got ${JSON.stringify(tombstonesOf(replyBefore, "out"))}`,
            );
            // Leave the shown middle for the occupied start: the departed
            // middle prunes (tombstone, then native removal above the floor)
            // with zero membership writes.
            h.activeScreen = { name: "out" };
            h.activeRef = h.wins[0] as FakeWindow;
            h.handle.requestWorkspaceSelect(1);
            h.runTimers();
            // Settle past the select: the departed middle prunes (local
            // tombstone, then native removal above the floor) and the next
            // captures observe the deleted native and clear the marker.
            await h.settle();
            const afterLeave = counters(h);
            assert.equal(h.currents.get("out"), "ws-1", "numbered select leaves for the occupied start");
            const reply = lastReply(h);
            assert.ok(!h.desktops.some((d) => d.id === "ws-2"), `departed native is removed, got ${h.desktops.map((d) => d.id).join(",")}`);
            assert.ok(
                !workspacesOf(reply).some((row) => row["out"] === "out" && row["exist"] !== false && row["bind"] === "ws-2"),
                `no live row rebinds the deleted native, got ${JSON.stringify(workspacesOf(reply))}`,
            );
            assert.equal(afterLeave.members - base.members, 0, `prune moves no windows, got ${JSON.stringify(h.memberWrites)}`);
            assert.ok(afterLeave.removals - base.removals >= 1, `departed native is removed, got ${JSON.stringify(h.removals)}`);
            // Stability after prune: no oscillation.
            await h.settle();
            assert.deepEqual(counters(h), afterLeave, "pruned lifecycle does not oscillate");
        } finally {
            await h.stop();
        }
    });

    it("globally occupied or visible native never deletes and the floor holds", { timeout: 240000 }, async () => {
        const h = await makeLifecycleHarness({
            desktops: ["ws-1", "ws-2", "ws-3", "ws-4"],
            outputs: ["out-left", "out-right"],
            currents: { "out-left": "ws-1", "out-right": "ws-2" },
            wins: [
                { id: "win-a", out: "out-left", ws: "ws-1" },
                { id: "win-b", out: "out-right", ws: "ws-2" },
            ],
        });
        try {
            await discoverAndSettle(h);
            const reply = lastReply(h);
            const stones = tombstonesOf(reply);
            const base = counters(h);
            // Absence intents are output-local: a native occupied or viewed
            // on another output may still retire where it is locally empty
            // and unviewed (its deletion is then vetoed by the foreign
            // occupant). What never happens: a stone naming a native that
            // is occupied or viewed on the stone's own output, or any
            // deletion of a globally occupied/visible native.
            const occupiedByOut = new Map<string, Set<string>>();
            for (const win of h.wins) {
                for (const desk of win.desktops) {
                    if (!occupiedByOut.has(win.output.name)) {
                        occupiedByOut.set(win.output.name, new Set());
                    }
                    (occupiedByOut.get(win.output.name) as Set<string>).add(desk.id);
                }
            }
            for (const stone of stones) {
                const out = stone["out"] as string;
                const bind = stone["bind"] as string;
                assert.ok(!(occupiedByOut.get(out)?.has(bind) ?? false), `locally occupied native never retires, got ${JSON.stringify(stone)}`);
                assert.notEqual(h.currents.get(out), bind, `locally viewed native never retires, got ${JSON.stringify(stone)}`);
            }
            // Repeated settles delete nothing occupied or visible: any
            // removal only names globally empty, unviewed natives above the
            // two-desktop floor.
            await h.settle();
            const after = counters(h);
            const globallyHeld = new Set<string>();
            for (const ids of occupiedByOut.values()) {
                for (const id of ids) {
                    globallyHeld.add(id);
                }
            }
            for (const cur of h.currents.values()) {
                globallyHeld.add(cur);
            }
            for (const removed of h.removals.slice(base.removals)) {
                assert.ok(!globallyHeld.has(removed), `occupied/visible native never deleted, got ${removed}`);
            }
            assert.ok(h.desktops.length >= 2, `two-desktop floor holds, got ${h.desktops.map((d) => d.id).join(",")}`);
            assert.equal(after.members - base.members, 0, `safety holds move no windows, got ${JSON.stringify(h.memberWrites)}`);
            void after;
        } finally {
            await h.stop();
        }
    });

    it("retired reselect readopts fresh at the native position and admits normally", { timeout: 240000 }, async () => {
        // The retired native survives on a foreign occupant while the ring
        // trailing stays occupied, so reselect converges with no spare churn:
        // adoption lands before an occupied trailing and no new spare is
        // needed until the next fill.
        const h = await makeLifecycleHarness({
            desktops: ["ws-1", "ws-2", "ws-3", "ws-4"],
            outputs: ["out-left", "out-right"],
            currents: { "out-left": "ws-1", "out-right": "ws-2" },
            wins: [
                { id: "win-a", out: "out-left", ws: "ws-1" },
                { id: "win-b", out: "out-right", ws: "ws-2" },
                { id: "win-d", out: "out-right", ws: "ws-3" },
                { id: "win-c", out: "out-left", ws: "ws-4" },
            ],
        });
        try {
            await discoverAndSettle(h);
            // Settle until the left middle (ws-3, empty on the left) retires
            // locally while its native survives on the foreign occupant. The
            // tombstone reuses the removed logical id plus its native
            // binding and is never a live ring row.
            const reply = lastReply(h);
            const leftStones = tombstonesOf(reply, "out-left");
            assert.ok(leftStones.length >= 1, `left prune publishes an absence intent, got ${JSON.stringify(workspacesOf(reply))}`);
            const stone = leftStones.find((row) => row["bind"] === "ws-3") as Record<string, unknown>;
            assert.ok(stone !== undefined, `ws-3 retires on the left, got ${JSON.stringify(leftStones)}`);
            const retiredNative = stone["bind"] as string;
            const retiredLogical = stone["id"] as string;
            assert.ok(h.desktops.some((d) => d.id === retiredNative), `retired native ${retiredNative} survives (floor or foreign occupant)`);
            const liveLeft = workspacesOf(reply).filter((row) => row["out"] === "out-left" && row["exist"] !== false);
            assert.ok(!liveLeft.some((row) => row["id"] === retiredLogical), "absence intent is not a live ring row");
            // Repeated global occupancy alone (the producer repeats totals on
            // every output) never clears the local tombstone: without a
            // local current selection or a local window fact on the retired
            // native, the intent persists.
            await h.settle();
            const again = lastReply(h);
            assert.ok(
                tombstonesOf(again, "out-left").some((row) => row["bind"] === retiredNative),
                `global occupancy alone never readopts, got ${JSON.stringify(workspacesOf(again))}`,
            );
            // The user selects the surviving native on the left: positive
            // output-local evidence clears the tombstone and ordinary
            // adoption mints a fresh logical at the native order position.
            // The core guarantees actual quiescence after reselect: settle
            // fully, then a further settle must create, remove, and move
            // nothing (no spare create/delete rotation).
            const nativeOrder = h.desktops.map((d) => d.id);
            const targetIndex = nativeOrder.indexOf(retiredNative);
            assert.ok(targetIndex >= 0, "retired native keeps its order position");
            const targetRef = h.desktops[targetIndex] as FakeDesktop;
            (h as { currents: Map<string, string> }).currents.set("out-left", targetRef.id);
            h.fire("currentDesktopChanged");
            // Quiescence, not bounded churn: the retired-native reselect
            // fix holds the sibling-bound tombstone, so settling converges
            // and a further settle changes nothing.
            await h.settle();
            const reselectBase = counters(h);
            await h.settle();
            assert.deepEqual(counters(h), reselectBase, `reselect is quiescent, got creations=${JSON.stringify(h.creations)} removals=${JSON.stringify(h.removals)}`);
            const reselected = lastReply(h);
            assert.ok(h.creations.length - reselectBase.creations === 0, `no backup churn, got ${h.creations.length - reselectBase.creations}`);
            assert.ok(
                !tombstonesOf(reselected, "out-left").some((row) => row["bind"] === retiredNative),
                `local selection ends the absence intent, got ${JSON.stringify(workspacesOf(reselected))}`,
            );
            const freshRows = workspacesOf(reselected).filter((row) => row["out"] === "out-left" && row["exist"] !== false && row["bind"] === retiredNative);
            assert.equal(freshRows.length, 1, `fresh logical binds the native, got ${JSON.stringify(workspacesOf(reselected))}`);
            assert.notEqual(freshRows[0]?.["id"], retiredLogical, "reselect mints a fresh logical, never the retired id");
            // Ordinal stability (no moves for compaction): the left ring
            // stays compact and the fresh row keeps its slot across rounds.
            // Compact live-binding order is the rule: the fresh row sits at
            // its native-order position among live ring bindings, ignoring
            // locally retired projections (no raw global-index padding).
            const leftSorted = workspacesOf(reselected)
                .filter((row) => row["out"] === "out-left" && row["exist"] !== false)
                .sort((a, b) => (a["order"] as number) - (b["order"] as number));
            leftSorted.forEach((row, index) => assert.equal(row["order"], index, "compact ordinals"));
            // A new window admits on the fresh logical normally and the next
            // fill grows a backed spare with no moves of existing windows.
            const membersBefore = h.memberWrites.length;
            addWindow(h, "win-e", "out-left", retiredNative);
            h.fire("windowAdded");
            for (let round = 0; round < 2; round += 1) {
                h.runTimers();
                await h.flushAll();
                h.runTimers();
                await h.flushAll();
            }
            const admitted = lastReply(h);
            const winRows = (admitted["windows"] as Array<Record<string, unknown>>) ?? [];
            assert.ok(
                winRows.some((w) => w["win"] === "win-e" && w["out"] === "out-left"),
                `newcomer admits on the fresh logical, got ${JSON.stringify(winRows)}`,
            );
            assert.equal(h.memberWrites.length - membersBefore, 0, `admission moves no existing windows, got ${JSON.stringify(h.memberWrites)}`);
            assert.ok(
                workspacesOf(admitted).filter((row) => row["out"] === "out-left" && row["exist"] !== false && row["bind"] === retiredNative).length === 1,
                "fresh logical persists through admission",
            );
            assert.ok(
                !tombstonesOf(admitted, "out-left").some((row) => row["bind"] === retiredNative),
                "admission never re-retires the reselected native",
            );
        } finally {
            await h.stop();
        }
    });

    it("shared mode backs the unified spare and retires empties without moves", { timeout: 240000 }, async () => {
        const h = await makeLifecycleHarness({
            mode: "shared",
            desktops: ["ws-1", "ws-2"],
            outputs: ["out-left", "out-right"],
            currents: { "out-left": "ws-1", "out-right": "ws-1" },
            wins: [{ id: "win-a", out: "out-left", ws: "ws-1" }],
        });
        try {
            await discoverAndSettle(h);
            const base = counters(h);
            // Fill the unified end: the shared trailing spare requests one
            // backing creation and no window moves.
            addWindow(h, "win-b", "out-left", h.desktops[h.desktops.length - 1]?.id ?? "");
            h.fire("windowAdded");
            await h.settle();
            const afterFill = counters(h);
            assert.equal(afterFill.creations - base.creations, 1, `shared fill backs the unified spare once, got ${JSON.stringify(h.creations)}`);
            assert.equal(afterFill.members - base.members, 0, `shared fill moves nothing, got ${JSON.stringify(h.memberWrites)}`);
            const reply = lastReply(h);
            assert.ok(
                (reply["nativeCreations"] as unknown[]).length <= 1,
                `at most one outstanding backing desire, got ${JSON.stringify(reply["nativeCreations"])}`,
            );
            await h.settle();
            assert.deepEqual(counters(h), afterFill, "shared lifecycle does not oscillate");
        } finally {
            await h.stop();
        }
    });
});
