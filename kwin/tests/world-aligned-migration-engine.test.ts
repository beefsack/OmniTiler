// Whole-workspace migration end to end through the production
// entry against the real `WorldEngine` (via the test-only `raw_world_eval`
// example: same decode, retained engine,
// codec and generation binding the shipped KDE world service uses; only
// D-Bus and signals are stubbed). No hand-built wire replies on the
// asserted paths: every reply is a fresh Engine report.
//
// Two outputs share one native grid (ws-1..ws-4); out-a shows ws-1
// (win-a, win-c), out-b shows ws-2 (win-b). Migrating out-a right moves
// the stable core logical unit to out-b with FRESH backing (Stage 1b
// correlated creation: the destination trailing native still backs its
// own logical, so core never cannibalizes it by reuse); the explicit
// core refill op to ws-4 stays, and the adapter actuates both views plus
// all members before focus: native member moves, target follow onto the
// created desktop, source refill, settlement with no replay, and stable
// logical rebinding across the move. Included in `npm test`.

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
            this.failAll(new Error(`raw_world_eval exited with code ${String(code)} signal=${String(signal)} stderr=${this.stderrTail.slice(-2000)}`));
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
                cwd: repoRoot(),
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
    readonly name: string;
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
    readonly logs: string[];
    readonly membershipWrites: string[];
    readonly outputWrites: string[];
    readonly viewWrites: string[];
    activeScreen: { name: string };
    activeRef: FakeWindow | null;
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    fire: (name: string, payload?: unknown) => void;
    runTimers: (maxDelay?: number) => void;
    flush: () => Promise<number>;
    flushAll: () => Promise<void>;
    settle: () => Promise<void>;
    stop: () => Promise<void>;
}

const NATIVES = ["ws-1", "ws-2", "ws-3", "ws-4"];

async function makeHarness(): Promise<Harness> {
    const desktops: FakeDesktop[] = NATIVES.map((id) => ({ id, name: id }));
    const byId = new Map(desktops.map((d) => [d.id, d]));
    const screens = [
        { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
        { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
    ];
    const currents = new Map([["out-a", "ws-1"], ["out-b", "ws-2"]]);
    const mkWin = (id: string, out: string, ws: string, x: number): FakeWindow => {
        const outputChanged = makeSignal();
        const winDesktopsChanged = makeSignal();
        const winHiddenChanged = makeSignal();
        const winFrameChanged = makeSignal();
        const frame = { x, y: 0, width: 200, height: 200 };
        const win: FakeWindow = {
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
        // Truthful native decoration: every native write (entry frame
        // actuation via Reflect.set, never a bypass) keeps the client rect
        // tracking the frame with the same insets the constructor proves,
        // so no capture ever pairs a fresh frame with a stale client.
        let liveFrame = { ...frame };
        let liveClient = { ...win.clientGeometry };
        Object.defineProperty(win, "frameGeometry", {
            configurable: true,
            get: () => liveFrame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                liveFrame = rect;
                liveClient = {
                    x: rect.x + 2,
                    y: rect.y + 21,
                    width: Math.max(1, rect.width - 4),
                    height: Math.max(1, rect.height - 22),
                };
            },
        });
        Object.defineProperty(win, "clientGeometry", {
            configurable: true,
            get: () => liveClient,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                liveClient = rect;
            },
        });
        return win;
    };
    const wins: FakeWindow[] = [
        mkWin("win-a", "out-a", "ws-1", 0),
        mkWin("win-c", "out-a", "ws-1", 210),
        mkWin("win-b", "out-b", "ws-2", 0),
    ];
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
        logs: [],
        membershipWrites: [],
        outputWrites: [],
        viewWrites: [],
        activeScreen: { name: "out-a" },
        activeRef: wins[0] as FakeWindow,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
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
            assert.ok(
                (harness.calls[index] as CapturedCall | undefined)?.method === "DescribeWorld",
                "expected the targeted DescribeWorld dispatch to be unanswered",
            );
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
            for (let round = 0; round < 16; round += 1) {
                const next = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
                if (next < 0) {
                    return;
                }
                await harness.flush();
            }
            throw new Error("flushAll did not converge: planner keeps dispatching");
        },
        settle: async (): Promise<void> => {
            for (let round = 0; round < 10; round += 1) {
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
        get cursorPos(): { x: number; y: number } {
            // Production-shaped cursor inventory: the cursor rides the
            // active output, which the core's cursor-first selection
            // source resolution requires for output-relative commands.
            return harness.activeScreen.name === "out-b" ? { x: 802, y: 100 } : { x: 2, y: 100 };
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
            }
        },
        clientArea: (_kind: unknown, outRef: unknown, _deskRef: unknown): Record<string, number> => {
            const name = (outRef as { name: string }).name;
            const geom = screenByName(name).geometry;
            return { x: geom["x"] as number, y: geom["y"] as number, width: geom["width"] as number, height: geom["height"] as number };
        },
        currentDesktopForScreen: (screen: { name: string }): FakeDesktop =>
            desktopById(harness.currents.get(screen.name) ?? ""),
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }): void => {
            const id = (deskRef as FakeDesktop).id;
            assert.ok(byId.has(id), `desktop ${id} exists`);
            harness.currents.set(screen.name, id);
            harness.viewWrites.push(`${screen.name}->${id}`);
            harness.fire("currentDesktopChanged");
        },
        sendClientToScreen: (mover: unknown, out: unknown): void => {
            const win = mover as FakeWindow;
            const name = (out as { name: string }).name;
            win.output = { name };
            harness.outputWrites.push(`${win.internalId}->${name}`);
        },
        // Production void creation: synchronously append one native
        // desktop carrying the logical token as its name. Same-turn
        // before/after enumeration proves the created id causally; the
        // synchronous desktop signal is the pinned reentrant path the
        // apply loop spends before.
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
            get: () => desks,
            set: (refs: FakeDesktop[]) => {
                desks = refs;
                harness.membershipWrites.push(`${win.internalId}->${refs.map((d) => d.id).join("+")}`);
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

async function discover(h: Harness): Promise<string> {
    // Authentic v4 connection pump: the handshake binds the producer
    // adapter lifetime and mints the serving generation; the revisioned
    // settings snapshot (rev 1) precedes the first report on the wire.
    const hsIndex = h.calls.findIndex((call) => call.method === "DiscoverWorld");
    assert.ok(hsIndex >= 0, "entry discovers on start");
    const hsBody = JSON.parse((h.calls[hsIndex] as CapturedCall).payload) as Record<string, unknown>;
    assert.equal(hsBody["type"], "handshake", "first discovery is the handshake");
    assert.equal(hsBody["adapter"], "test-adapter", "handshake carries the producer adapter");
    const hsReplyText = await h.bridge.send("DiscoverWorld", (h.calls[hsIndex] as CapturedCall).payload);
    const hsReply = JSON.parse(hsReplyText) as Record<string, unknown>;
    assert.equal(hsReply["type"], "handshake-reply", "engine answers the handshake");
    assert.equal(hsReply["ready"], false, "accepted handshake carries ready:false");
    assert.equal(hsReply["kind"], "", "accepted handshake carries an empty kind");
    assert.equal(hsReply["adapter"], "test-adapter", "handshake retains the producer adapter, never a broker incarnation");
    assert.equal(hsReply["req"], hsBody["req"], "handshake echoes the request correlation");
    assert.ok(typeof hsReply["gen"] === "string" && (hsReply["gen"] as string) !== "", "handshake mints the serving generation");
    const gen = hsReply["gen"] as string;
    const boundHs = h.broker.bindDiscovery(hsReplyText);
    assert.equal((JSON.parse(boundHs) as Record<string, unknown>)["adapter"], "test-adapter", "harness passes the producer lifetime through");
    assert.equal(h.broker.gen, gen, "harness records the actual serving generation");
    (h.calls[hsIndex] as CapturedCall).callback(boundHs);
    const setIndex = h.calls.findIndex((call, at) => at > hsIndex && call.method === "DiscoverWorld");
    assert.ok(setIndex >= 0, "revisioned settings follow the handshake");
    const setBody = JSON.parse((h.calls[setIndex] as CapturedCall).payload) as Record<string, unknown>;
    assert.equal(setBody["type"], "settings", "second discovery is the revisioned settings snapshot");
    assert.equal(setBody["adapter"], "test-adapter", "settings carry the producer adapter");
    assert.equal(setBody["gen"], gen, "settings bind the serving generation");
    assert.equal(setBody["rev"], "1", "first settings snapshot carries rev 1");
    const setReplyText = await h.bridge.send("DiscoverWorld", (h.calls[setIndex] as CapturedCall).payload);
    const setReply = JSON.parse(setReplyText) as Record<string, unknown>;
    assert.equal(setReply["type"], "settings-reply", "engine answers settings");
    assert.equal(setReply["status"], "accepted", `settings accepted, got ${setReplyText.slice(0, 300)}`);
    assert.equal(setReply["adapter"], "test-adapter", "settings reply retains the producer adapter");
    assert.equal(setReply["gen"], gen, "settings reply binds the serving generation");
    assert.equal(setReply["req"], setBody["req"], "settings reply echoes the request correlation");
    assert.equal(setReply["rev"], "1", "settings reply binds the sent revision");
    (h.calls[setIndex] as CapturedCall).callback(setReplyText);
    assert.deepEqual(
        h.calls.slice(0, setIndex + 1).map((call) => call.method),
        ["DiscoverWorld", "DiscoverWorld"],
        "no report precedes the accepted settings",
    );
    await h.settle();
    return gen;
}

function lastTakeResult(h: Harness): Record<string, unknown> {
    const readys = h.takeReplies
        .map((raw) => JSON.parse(raw) as Record<string, unknown>)
        .filter((r) => r["outcome"] === "ready");
    assert.ok(readys.length > 0, "at least one ready take result");
    return (readys[readys.length - 1] as Record<string, unknown>)["result"] as Record<string, unknown>;
}

function lastReply(h: Harness): Record<string, unknown> {
    const indices = [...h.replies.keys()].filter((at) => (h.calls[at] as CapturedCall).method === "DescribeWorld");
    assert.ok(indices.length > 0, "at least one answered DescribeWorld");
    const raw = h.replies.get(Math.max(...indices) as number) as string;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    assert.equal(parsed["outcome"] ?? "accepted", "accepted", `fresh reply accepted, got ${raw.slice(0, 200)}`);
    return parsed;
}

function transfersOf(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return (reply["transfers"] as Array<Record<string, unknown>>) ?? [];
}

// One lag direction through the real Engine and production apply:
// migrate, let the native actuation (fresh backing creation plus both
// view follows) land, then rewind the migration-follow view and report
// members-arrived-but-target-old. The fresh Engine reply must retain
// (never prematurely settle) the migration; re-actuation then restores
// the created-backing view and a final settle converges.
async function lagDirection(options: {
    activeOut: string;
    activeWin: string;
    direction: string;
    targetOut: string;
    refillOut: string;
    oldTargetView: string;
}): Promise<void> {
    const h = await makeHarness();
    try {
        await discover(h);
        h.fire("desktopsChanged");
        await h.settle();
        // The migrating logical is the cell shown on the active output
        // (resolved from the settled rows, never a hardcoded trailing).
        const beforeRows = (lastReply(h)["workspaces"] as Array<Record<string, string>>);
        const activeNative = h.currents.get(options.activeOut) ?? "";
        const migratedLogical = beforeRows.find((b) => b["out"] === options.activeOut && b["bind"] === activeNative)?.["id"] ?? "";
        assert.ok(migratedLogical !== "", "active cell has a logical identity to migrate");
        const nativeOrderBefore = h.desktops.map((d) => d.id);
        const active = h.wins.find((w) => w.internalId === options.activeWin) as FakeWindow;
        h.activeScreen = { name: options.activeOut };
        h.activeRef = active;
        h.handle.requestWorkspaceMigrate(options.direction);
        // Converge only to the bound reply: the drain carries the fresh
        // backing desire (unbound target, members hold); the creation then
        // binds it and one bound apply moves members plus both views. Stop
        // before the proof capture settles the flight, so the rewind below
        // reports members-arrived-but-views-old mid-flight.
        let boundReply: Record<string, unknown> | null = null;
        for (let round = 0; round < 16 && boundReply === null; round += 1) {
            h.runTimers();
            await h.flushAll();
            const rows = (lastReply(h)["workspaces"] as Array<Record<string, string>>);
            if ((rows.find((b) => b["out"] === options.targetOut && b["id"] === migratedLogical)?.["bind"] ?? "") !== "") {
                boundReply = lastReply(h);
            }
        }
        assert.ok(boundReply !== null, "backing binds mid-flight");
        // The drain reply names the riding logical plus the core-chosen
        // refill; the bound reply publishes the fresh backing binding.
        let refillLogical = "";
        for (const raw of h.replies.values()) {
            const reply = JSON.parse(raw) as Record<string, unknown>;
            const migration = transfersOf(reply).find((t) => t["kind"] === "migration");
            if (migration !== undefined && migration["out"] === options.targetOut && migration["ws"] === migratedLogical) {
                const refill = transfersOf(reply).find((t) => t["kind"] === "view" && (t["op"] as string) === `${migration["op"] as string}-refill`);
                refillLogical = (refill?.["ws"] as string) ?? "";
                break;
            }
        }
        assert.ok(refillLogical !== "", "drain reply retains the explicit refill");
        assert.ok(refillLogical !== migratedLogical, "refill excludes the migrated logical");
        const boundRows = (boundReply["workspaces"] as Array<Record<string, string>>);
        const createdNative = boundRows.find((b) => b["out"] === options.targetOut && b["id"] === migratedLogical)?.["bind"] ?? "";
        assert.ok(createdNative !== "", "migrated logical publishes fresh backing once created");
        assert.ok(!nativeOrderBefore.includes(createdNative), `backing is a fresh desktop (got ${createdNative})`);
        const refillNative = boundRows.find((b) => b["out"] === options.refillOut && b["id"] === refillLogical)?.["bind"] ?? "";
        assert.ok(refillNative !== "", "refill logical stays bound");
        // Native actuation landed: members moved, both views followed.
        assert.equal(h.currents.get(options.targetOut), createdNative, "target view follows the created backing");
        assert.equal(h.currents.get(options.refillOut), refillNative, "source refills to the core choice");
        // Rewind only the migration-follow view: members stay arrived on
        // the created backing, the refill stays proven, the target view
        // reads old. Rewinding the refill source view instead would be
        // genuine newer external input (proven switch abandoned): the
        // adapter drops the refill as superseded while core holds its
        // child on the non-matching echo with no raw attempt to prove the
        // external move, so that journey cannot settle under the current
        // contract (reported separately). The target lag below is the
        // converging partial-arrival shape: the fresh Engine reply must
        // retain (never prematurely settle) the migration.
        h.currents.set(options.targetOut, options.oldTargetView);
        h.fire("currentDesktopChanged");
        h.fire("desktopsChanged");
        h.runTimers();
        const lagIndex = await h.flush();
        const lagged = JSON.parse(h.replies.get(lagIndex) as string) as Record<string, unknown>;
        assert.equal(lagged["outcome"] ?? "accepted", "accepted");
        const held = transfersOf(lagged);
        assert.ok(
            held.some((t) => t["kind"] === "migration"),
            `members arrived but target view old retains the migration: ${JSON.stringify(held)}`,
        );
        assert.ok(
            !held.some((t) => t["kind"] === "view"),
            `proven refill stays settled while only the target lags: ${JSON.stringify(held)}`,
        );
        // The held reply re-actuates the target view; the next round settles.
        try {
            await h.settle();
        } catch (e) {
            const last = lastReply(h);
            console.log("SETTLE-DEBUG calls=" + String(h.calls.length) + " transfers=" + JSON.stringify(transfersOf(last)) + " currents=" + JSON.stringify(Object.fromEntries(h.currents)) + " wins=" + JSON.stringify(h.wins.map((w) => [w.internalId, w.output.name, w.desktops.map((d) => d.id)])) + " viewWrites=" + JSON.stringify(h.viewWrites.slice(-10)));
            throw e;
        }
        assert.deepEqual(transfersOf(lastReply(h)), [], "members plus both views settle");
    } finally {
        await h.stop();
    }
}

describe("world migration engine", () => {
    it("migrates through fresh Engine replies with replicated bindings, refill and settlement", { timeout: 120000 }, async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            // One more observation round so the adapter snapshot converges
            // the core-owned mapping from the first accepted reply.
            h.fire("desktopsChanged");
            await h.settle();
            // The core-owned workspace rows bind each native once per
            // output with a distinct logical identity per cell (id IS the
            // logical id in v2, no `logical` key).
            const settled = lastReply(h);
            const settledRows = settled["workspaces"] as Array<Record<string, string>>;
            const ws1 = settledRows.filter((b) => b["bind"] === "ws-1");
            assert.equal(ws1.length, 2, "ws-1 binds once per output");
            assert.notEqual(ws1[0]?.["id"], ws1[1]?.["id"], "distinct logical identity per cell");
            const sourceLogicalBefore = (ws1.find((b) => b["out"] === "out-a") as Record<string, string>)["id"];
            assert.deepEqual(settled["transfers"], [], "no transfers before the command");
            // Pre-migration native inventory: the migration backing must be
            // a genuinely fresh desktop (never an ordinal/count reuse of a
            // pre-existing id, never a stolen trailing cell).
            const nativeOrderBefore = h.desktops.map((d) => d.id);

            // Migrate the active workspace (out-a/ws-1) right.
            h.handle.requestWorkspaceMigrate("right");
            await h.settle();
            const migrateCalls = h.calls
                .map((call, index) => ({ call, index }))
                .filter(({ call }) => {
                    if (call.method !== "QueueWorldCommand") {
                        return false;
                    }
                    try {
                        const body = JSON.parse(call.payload) as Record<string, unknown>;
                        return (body["command"] as Record<string, unknown> | null)?.["op"] === "migrate-directional";
                    } catch {
                        return false;
                    }
                });
            assert.equal(migrateCalls.length, 1, "one directional migration command, never replayed");
            // The drained capture is the first DescribeWorld after the queue.
            const drainIndex = h.calls.findIndex(
                (call, at) => at > (migrateCalls[0]?.index as number) && call.method === "DescribeWorld",
            );
            assert.ok(drainIndex >= 0, "queued command drains on a fresh capture");
            const migrateReply = JSON.parse(h.replies.get(drainIndex) as string) as Record<string, unknown>;
            assert.equal(migrateReply["result"], null, "actuator lane never carries the terminal result");
            const result = lastTakeResult(h);
            assert.equal(result["op"], "migrate-workspace");
            assert.equal(result["sourceOut"], "out-a");
            assert.equal(result["targetOut"], "out-b");
            // The stable core logical id rides the move (core-owned
            // identity; never a destination-trailing rename).
            assert.equal(result["targetWs"], sourceLogicalBefore, "migrated logical id rides to the target");
            assert.deepEqual((result["members"] as string[]).sort(), ["win-a", "win-c"]);
            const transfers = migrateReply["transfers"] as Array<Record<string, unknown>>;
            const migration = transfers.find((t) => t["kind"] === "migration");
            assert.ok(migration !== undefined, "migration transfer retained");
            assert.equal(migration?.["out"], "out-b");
            assert.equal(migration?.["ws"], sourceLogicalBefore, "migration transfer names the riding logical");
            const refill = transfers.find((t) => t["kind"] === "view");
            assert.ok(refill !== undefined, "explicit core refill retained (reading B)");
            assert.match(refill?.["op"] as string, /-refill$/);
            assert.equal(refill?.["out"], "out-a");
            assert.equal(refill?.["ws"], "ws-4", "moved ws-1 and its fresh replacement excluded");

            // Native actuation: both members remap output and rebind to the
            // freshly created backing (resolved from the settled reply,
            // never a hardcoded trailing id); both views follow exactly
            // once each.
            const finalReply = lastReply(h);
            assert.deepEqual(finalReply["transfers"], [], "all transfers settle");
            const finalRows = finalReply["workspaces"] as Array<Record<string, string>>;
            const moved = finalRows.find((b) => b["out"] === "out-b" && b["id"] === sourceLogicalBefore);
            const createdNative = moved?.["bind"] ?? "";
            assert.ok(createdNative !== "", "migrated logical publishes fresh backing once created");
            assert.ok(
                !nativeOrderBefore.includes(createdNative),
                `backing is a fresh desktop, never a reused pre-existing id (got ${createdNative})`,
            );
            const winA = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            const winC = h.wins.find((w) => w.internalId === "win-c") as FakeWindow;
            assert.equal(winA.output.name, "out-b");
            assert.equal(winC.output.name, "out-b");
            assert.deepEqual(winA.desktops.map((d) => d.id), [createdNative]);
            assert.deepEqual(winC.desktops.map((d) => d.id), [createdNative]);
            assert.equal(h.currents.get("out-b"), createdNative, "target view follows the created backing");
            assert.equal(h.currents.get("out-a"), "ws-4", "source refills to the core choice");
            assert.deepEqual(
                h.viewWrites.filter((c) => c === `out-b->${createdNative}`),
                [`out-b->${createdNative}`],
                "target view switches exactly once",
            );
            assert.deepEqual(
                h.viewWrites.filter((c) => c === "out-a->ws-4"),
                ["out-a->ws-4"],
                "refill view switches exactly once",
            );
            assert.equal(h.outputWrites.filter((c) => c === "win-a->out-b").length, 1);
            assert.equal(h.outputWrites.filter((c) => c === "win-c->out-b").length, 1);

            // Settlement: the logical unit rode to the destination on fresh
            // backing; the destination trailing native still backs its own
            // logical (no steal by ordinal reuse); the migrated logical
            // appears exactly once (on the target); the released source
            // native adopts as a fresh empty logical row at native-order
            // position (no dormant return binding, never the migrated id).
            assert.equal(moved?.["id"], sourceLogicalBefore, "logical identity rides the move");
            const trailingCell = finalRows.find((b) => b["out"] === "out-b" && b["bind"] === "ws-3");
            assert.ok(
                trailingCell !== undefined && trailingCell["id"] !== sourceLogicalBefore,
                "destination trailing keeps its own logical (never cannibalized)",
            );
            assert.deepEqual(
                finalRows.filter((b) => b["id"] === sourceLogicalBefore).map((b) => b["out"]),
                ["out-b"],
                "migrated logical lives exactly once, on the target",
            );
            assert.ok(
                finalRows.some((b) => b["out"] === "out-a" && b["bind"] === "ws-1" && b["id"] !== sourceLogicalBefore),
                `released source native adopts a fresh logical: ${JSON.stringify(finalRows)}`,
            );
            const freshSource = finalRows.find((b) => b["out"] === "out-a" && b["bind"] === "ws-1") as Record<string, string>;
            assert.notEqual(freshSource["id"], sourceLogicalBefore, "fresh row is never the migrated id");
            // No replay anywhere: one command, bounded native calls.
            assert.equal(migrateCalls.length, 1);
            assert.ok(!h.logs.some((l) => l.includes("command-busy")), "no busy refills");
        } finally {
            await h.stop();
        }
    });

    it("holds settlement while members arrived but views lag, both directions", { timeout: 180000 }, async () => {
        // Right: out-a/ws-1 cell migrates right; the target view lands on
        // the fresh backing (never the trailing reuse), the source refills
        // to the core-chosen cell. The target view rewinds to ws-2 for the
        // lagged round.
        await lagDirection({
            activeOut: "out-a",
            activeWin: "win-a",
            direction: "right",
            targetOut: "out-b",
            refillOut: "out-a",
            oldTargetView: "ws-2",
        });
        // Left: out-b/ws-2 cell migrates left; same fresh-backing shape.
        // The target view rewinds to ws-1.
        await lagDirection({
            activeOut: "out-b",
            activeWin: "win-b",
            direction: "left",
            targetOut: "out-a",
            refillOut: "out-b",
            oldTargetView: "ws-1",
        });
    });

    it("retains the migration while members arrived but BOTH views lag, then converges on fresh adoption", { timeout: 180000 }, async () => {
        // Both-view rewind discriminator (released-source contract): after
        // both currents rewind to the pre-move desktops - including the
        // migration-vacated source desktop - the first reply retains the
        // migration (never settled from a gap) while the vacated source
        // already adopts as a fresh logical row. The refill retires on the
        // next round and never re-actuates; both user switches win with
        // zero view writes, and the flight converges to empty.
        // Parked contract: zero unchosen source view writes.
        const h = await makeHarness();
        try {
            await discover(h);
            h.fire("desktopsChanged");
            await h.settle();
            const beforeRows = (lastReply(h)["workspaces"] as Array<Record<string, string>>);
            const migratedLogical = beforeRows.find((b) => b["out"] === "out-a" && b["bind"] === "ws-1")?.["id"] ?? "";
            assert.ok(migratedLogical !== "", "active cell has a logical identity to migrate");
            const active = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            h.activeScreen = { name: "out-a" };
            h.activeRef = active;
            h.handle.requestWorkspaceMigrate("right");
            h.runTimers();
            await h.flushAll();
            // Converge only to the bound reply: the drain carries the fresh
            // backing desire (unbound target, members hold); the creation
            // then binds it and one bound apply moves members plus both
            // views. Stop before the proof capture settles the flight, so
            // the rewind below reports members-arrived-but-views-old
            // mid-flight.
            let boundReply: Record<string, unknown> | null = null;
            for (let round = 0; round < 16 && boundReply === null; round += 1) {
                h.runTimers();
                await h.flushAll();
                const rows = (lastReply(h)["workspaces"] as Array<Record<string, string>>);
                if ((rows.find((b) => b["out"] === "out-b" && b["id"] === migratedLogical)?.["bind"] ?? "") !== "") {
                    boundReply = lastReply(h);
                }
            }
            assert.ok(boundReply !== null, "backing binds mid-flight");
            const boundRows = (boundReply["workspaces"] as Array<Record<string, string>>);
            const createdNative = boundRows.find((b) => b["out"] === "out-b" && b["id"] === migratedLogical)?.["bind"] ?? "";
            assert.ok(createdNative !== "", "migrated logical publishes fresh backing once created");
            // Native actuation landed: members moved, both views followed.
            assert.equal(h.currents.get("out-b"), createdNative, "target view follows the created backing");
            // Rewind both views to the pre-move currents.
            const writesBefore = h.viewWrites.length;
            h.currents.set("out-b", "ws-2");
            h.currents.set("out-a", "ws-1");
            h.fire("currentDesktopChanged");
            h.fire("desktopsChanged");
            h.runTimers();
            const lagIndex = await h.flush();
            const lagged = JSON.parse(h.replies.get(lagIndex) as string) as Record<string, unknown>;
            assert.equal(lagged["outcome"] ?? "accepted", "accepted");
            const held = transfersOf(lagged);
            assert.ok(
                held.some((t) => t["kind"] === "migration"),
                `members arrived but both views old retains the migration: ${JSON.stringify(held)}`,
            );
            const laggedRows = (lagged["workspaces"] as Array<Record<string, string>>);
            // The vacated source desktop adopts as a FRESH logical row on
            // the rewind reply itself (never the migrated id, never the
            // refill): released-source adoption at native-order position.
            const laggedFresh = laggedRows.find((b) => b["out"] === "out-a" && b["bind"] === "ws-1");
            assert.ok(
                laggedFresh !== undefined && laggedFresh["id"] !== migratedLogical,
                `rewound source adopts a fresh row for the vacated desktop: ${JSON.stringify(laggedRows)}`,
            );
            const laggedRefillWs = (held.find((t) => t["kind"] === "view")?.["ws"] as string) ?? "";
            assert.ok(
                (laggedFresh as Record<string, string>)["id"] !== laggedRefillWs,
                "fresh row is never the retired refill",
            );
            assert.equal(
                Number((laggedFresh as Record<string, string>)["order"]),
                0,
                "fresh row sits at its native-order position",
            );
            // The migrated logical rides on the destination on its fresh
            // backing, with members homed there.
            const laggedMoved = laggedRows.find((b) => b["out"] === "out-b" && b["id"] === migratedLogical);
            assert.equal(
                laggedMoved?.["bind"],
                createdNative,
                "migrated logical stays on the destination on fresh backing",
            );
            for (const id of ["win-a", "win-c"]) {
                const win = h.wins.find((w) => w.internalId === id) as FakeWindow;
                assert.equal(win.output.name, "out-b", `member output moved: ${id}`);
                assert.deepEqual(
                    win.desktops.map((d) => d.id),
                    [createdNative],
                    `member homed on the created backing: ${id}`,
                );
            }
            // Zero unchosen source writes after the rewind: the refill
            // retires on the next round (asserted below), so no later
            // source-output switch may name it.
            const laggedRefill = held.find((t) => t["kind"] === "view");
            const refillBacking = laggedRefill !== undefined
                ? (laggedRows.find((b) => b["out"] === (laggedRefill["out"] as string) && b["id"] === (laggedRefill["ws"] as string))?.["bind"] ?? "")
                : "";
            assert.ok(refillBacking !== "", "lagged reply binds the core-chosen refill");
            // The refill retires on the next round and never re-actuates:
            // a proven-then-abandoned view is newer external input, not
            // lag, on either output. Bounded rounds (not the converging
            // settle helper) so the failure names the stuck transfers
            // instead of timing out.
            let firstRound: Array<Record<string, unknown>> | null = null;
            for (let round = 0; round < 6; round += 1) {
                h.runTimers();
                await h.flushAll();
                if (round === 0) {
                    firstRound = transfersOf(lastReply(h));
                }
            }
            assert.ok(
                firstRound !== null && !firstRound.some((t) => t["kind"] === "view"),
                `refill retires the round after the rewind, never reapplied: ${JSON.stringify(firstRound)}`,
            );
            const fresh = h.viewWrites.slice(writesBefore);
            assert.deepEqual(
                fresh,
                [],
                `no view re-actuation after the rewind: both switches win, got ${JSON.stringify(fresh)}`,
            );
            assert.deepEqual(transfersOf(lastReply(h)), [], "members plus both views settle");
            assert.deepEqual(
                Object.fromEntries(h.currents),
                { "out-a": "ws-1", "out-b": "ws-2" },
                "both rewound currents stick: the user's switch wins both outputs",
            );
            const finalRows = (lastReply(h)["workspaces"] as Array<Record<string, string>>);
            const finalMoved = finalRows.find((b) => b["out"] === "out-b" && b["id"] === migratedLogical);
            assert.equal(
                finalMoved?.["bind"],
                createdNative,
                "destination logical identity and backing survive convergence",
            );
            const finalFresh = finalRows.find((b) => b["out"] === "out-a" && b["bind"] === "ws-1");
            assert.ok(
                finalFresh !== undefined && finalFresh["id"] !== migratedLogical,
                `fresh source row survives convergence: ${JSON.stringify(finalRows)}`,
            );
        } finally {
            await h.stop();
        }
    });
});
