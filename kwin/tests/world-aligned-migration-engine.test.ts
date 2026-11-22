// Whole-workspace migration end to end through the production
// entry against the real `WorldEngine` (via the test-only `world_eval`
// example: same decode, retained engine,
// codec and generation binding the shipped KDE world service uses; only
// D-Bus and signals are stubbed). No hand-built wire replies on the
// asserted paths: every reply is a fresh Engine report.
//
// Two outputs share one native grid. Each starts on slot one with
// another occupied logical behind it, so migration tests both reassignment
// of existing slots and two observable selector changes. The genuine
// plan authorizes each write; later readbacks settle it without replay.

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";
import { describe, it } from "node:test";

import { WORLD_DEBOUNCE_MS, startWorldAdapterEntry } from "../src/world-entry";
import { parseWorldReply, type WorldReply, type WorldRosterPlan } from "../src/world-report";
import { createMiniBroker, parseRawEvalWithSettlements, type MiniBroker } from "./world-wire-harness";

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
            throw new Error("world_eval stdout unavailable");
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
            this.failAll(new Error(`world_eval exited with code ${String(code)} signal=${String(signal)} stderr=${this.stderrTail.slice(-2000)}`));
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

type SignalHandler = (...args: unknown[]) => void;

interface FakeSignal {
    readonly handlers: SignalHandler[];
    readonly signal: {
        connect: (handler: SignalHandler) => void;
        disconnect: (handler: SignalHandler) => void;
    };
}

function makeSignal(): FakeSignal {
    const handlers: SignalHandler[] = [];
    return {
        handlers,
        signal: {
            connect: (handler: SignalHandler): void => {
                handlers.push(handler);
            },
            disconnect: (handler: SignalHandler): void => {
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
    readonly delayedViews: Set<string>;
    readonly pendingViews: Map<string, () => void>;
    activeScreen: { name: string };
    activeRef: FakeWindow | null;
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    fire: (name: string, ...args: unknown[]) => void;
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
    const currents = new Map([["out-a", "ws-1"], ["out-b", "ws-1"]]);
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
        mkWin("win-b", "out-b", "ws-1", 800),
        mkWin("win-a-later", "out-a", "ws-2", 0),
        mkWin("win-b-later", "out-b", "ws-2", 800),
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
        delayedViews: new Set(),
        pendingViews: new Map(),
        activeScreen: { name: "out-a" },
        activeRef: wins[0] as FakeWindow,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        fire: (name: string, ...args: unknown[]): void => {
            for (const handler of [...(signals[name] as FakeSignal).handlers]) {
                handler(...args);
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
            const { reply: coreReply, settlements } = parseRawEvalWithSettlements(await harness.bridge.send("DescribeWorldWithSettlements", stamped));
            const reply = harness.broker.bindReply(coreReply, envelope, settlements);
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
    let nextDesktop = 0;
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
            if (found !== undefined && harness.activeRef !== found) {
                harness.activeRef = found;
                harness.fire("windowActivated", found);
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
            const previous = desktopById(harness.currents.get(screen.name) ?? "");
            const deliver = (): void => {
                harness.currents.set(screen.name, id);
                harness.fire("currentDesktopChanged", previous, deskRef, screen);
            };
            harness.viewWrites.push(`${screen.name}->${id}`);
            if (harness.delayedViews.has(screen.name)) {
                assert.ok(!harness.pendingViews.has(screen.name), `selector ${screen.name} already has an in-flight write`);
                harness.pendingViews.set(screen.name, deliver);
            } else {
                deliver();
            }
        },
        sendClientToScreen: (mover: unknown, out: unknown): void => {
            const win = mover as FakeWindow;
            const name = (out as { name: string }).name;
            win.output = { name };
            harness.outputWrites.push(`${win.internalId}->${name}`);
            for (const handler of [...(win.__signals?.outputChanged.handlers ?? [])]) handler();
        },
        // IDs never alias a desktop retired earlier in the journey.
        // Production void creation: synchronously append one native
        // desktop carrying the logical token as its name. Same-turn
        // before/after enumeration proves the created id causally; the
        // synchronous desktop signal is the pinned reentrant path the
        // apply loop spends before.
        createDesktop: (position: unknown, name: unknown): void => {
            assert.equal(typeof position, "number", "creation carries a numeric position");
            assert.equal(typeof name, "string", "creation carries a logical token name");
            const id = `native-${++nextDesktop}`;
            const desk: FakeDesktop = { id, name: name as string };
            const at = Math.max(0, Math.min(position as number, harness.desktops.length));
            harness.desktops.splice(at, 0, desk);
            byId.set(id, desk);
            harness.fire("desktopsChanged");
        },
        removeDesktop: ({ id }: FakeDesktop): void => {
            const at = harness.desktops.findIndex((d) => d.id === id);
            assert.ok(at >= 0 && ![...harness.currents.values()].includes(id));
            assert.ok(harness.wins.every((win) => win.desktops.every((d) => d.id !== id)));
            harness.desktops.splice(at, 1);
            byId.delete(id);
            harness.fire("desktopsChanged");
        },
    };
    for (const win of wins) {
        let desks = win.desktops;
        Object.defineProperty(win, "desktops", {
            configurable: true,
            get: () => desks,
            set: (refs: FakeDesktop[]) => {
                desks = refs;
                harness.membershipWrites.push(`${win.internalId}->${refs.map((d) => d.id).join("+")}`);
                for (const handler of [...(win.__signals?.desktopsChanged.handlers ?? [])]) handler();
            },
        });
    }
    const handle = startWorldAdapterEntry({
        options: { perOutputVirtualDesktops: true },
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

function publication(h: Harness, index = Math.max(...h.replies.keys())): WorldReply {
    const raw = h.replies.get(index);
    assert.ok(raw !== undefined, "capture has a genuine reply");
    const parsed = parseWorldReply(raw);
    assert.ok(parsed !== null && "transfers" in parsed, "capture has a parsed world publication");
    return parsed;
}

function nativeFor(plan: WorldRosterPlan, out: string, logical: string): string {
    const rows = plan.projections.find((projection) => projection.out === out)?.rows;
    const slot = plan.slots[rows?.indexOf(logical) ?? -1];
    assert.ok(slot?.kind === "existing", `${out}/${logical} is assigned an exact existing slot`);
    assert.ok(plan.baseline.order.includes(slot.native), `slot ${slot.native} was in the baseline roster`);
    return slot.native;
}

function queueCount(h: Harness): number {
    return h.calls.filter((call) => call.method === "QueueWorldCommand").length;
}

async function startMigration(h: Harness, direction: "left" | "right", delayedOutputs: readonly string[] = []) {
    await discover(h);
    h.fire("desktopsChanged");
    await h.settle();
    const startup = publication(h);
    assert.equal(startup.rosterPlan?.op, undefined, "startup settles before migration");
    assert.deepEqual(startup.transfers, [], "no pending logical transfers before migration");
    for (const out of delayedOutputs) h.delayedViews.add(out);
    const source = direction === "right" ? "out-a" : "out-b";
    const target = direction === "right" ? "out-b" : "out-a";
    const sourceBefore = h.currents.get(source);
    const targetBefore = h.currents.get(target);
    const rows = startup.workspaces;
    const logical = rows.find((row) => row.out === source && row.bind === sourceBefore)?.id;
    const anchor = rows.find((row) => row.out === target && row.bind === targetBefore);
    assert.ok(logical && anchor, "both shown cells have real logical identities");
    assert.notEqual(logical, anchor.id, "sharing a native slot does not share the logical identity");
    const nativeOrder = h.desktops.map((d) => d.id);
    const active = h.wins.find((win) => win.internalId === (direction === "right" ? "win-a" : "win-b"));
    assert.ok(active);
    h.activeRef = active;
    h.activeScreen = { name: source };
    const members = h.wins.filter((win) =>
        win.output.name === source && win.desktops.some((desk) => desk.id === sourceBefore));
    const offsets = { output: h.outputWrites.length, membership: h.membershipWrites.length, view: h.viewWrites.length };

    h.handle.requestWorkspaceMigrate(direction);
    h.runTimers();
    const drain = await h.flush();
    const reply = publication(h, drain);
    assert.equal("result" in JSON.parse(h.replies.get(drain) ?? ""), false, "terminal result is on the take lane");
    const plan = reply.rosterPlan;
    assert.ok(plan, "migration publishes typed roster authority");
    assert.deepEqual(plan.baseline.order, nativeOrder);
    const migration = reply.transfers.find((row) => row.kind === "migration");
    assert.ok(migration, "migration transfer retained");
    assert.equal(migration.out, target);
    assert.equal(migration.ws, logical);
    const refill = reply.transfers.find((row) => row.kind === "view" && row.parent === migration.op);
    assert.ok(refill, "explicit paired refill retained");
    assert.equal(refill.out, source);
    assert.notEqual(refill.ws, logical);
    assert.equal(refill.ws, rows.find((row) => row.out === source && row.trailing)?.id,
        "the paired refill selects the source's prior trailing logical");
    const targetNative = nativeFor(plan, target, logical);
    const refillNative = nativeFor(plan, source, refill.ws);
    const targetRing = plan.projections.find((projection) => projection.out === target)?.rows ?? [];
    assert.equal(targetRing.indexOf(logical), targetRing.indexOf(anchor.id) + 1, "insert immediately after destination current");
    assert.notEqual(targetNative, targetBefore, "destination is a genuine selector change");
    assert.notEqual(refillNative, sourceBefore, "source refill is a genuine selector change");
    return { source, target, sourceBefore, targetBefore, logical, refill, targetNative, refillNative, active, members, plan, offsets };
}

type Migration = Awaited<ReturnType<typeof startMigration>>;

function assertMemberArrival(h: Harness, move: Migration): void {
    const { target, targetNative, members, plan, offsets } = move;
    for (const win of members) {
        assert.equal(win.output.name, target);
        assert.deepEqual(win.desktops.map((d) => d.id), [targetNative], `${win.internalId} arrived on the assigned slot`);
        assert.ok(!win.hidden && !win.minimized, `${win.internalId} was not hidden by migration`);
    }
    assert.deepEqual(h.outputWrites.slice(offsets.output).sort(),
        members.map((win) => `${win.internalId}->${target}`).sort(), "each riding member crosses exactly once");
    const membership = plan.members.flatMap((member) => {
        assert.ok(member.target.kind === "existing", "occupied rows in this journey use existing slots");
        return member.before.native !== member.target.native ? [`${member.win}->${member.target.native}`] : [];
    });
    assert.deepEqual(h.membershipWrites.slice(offsets.membership).sort(), membership.sort(),
        "exactly the planned membership changes, including any displaced occupant");
}

async function assertSettlement(h: Harness, move: Migration): Promise<void> {
    await h.settle();
    const settled = publication(h);
    assert.equal(settled.rosterPlan?.op, undefined, `exact native readback adopts the plan: ${JSON.stringify({
        currents: [...h.currents], order: h.desktops.map((desk) => desk.id), views: h.viewWrites,
        output: h.outputWrites, membership: h.membershipWrites,
        capture: h.calls[Math.max(...h.replies.keys())]?.payload, logs: h.logs.slice(-12),
    })}`);
    assert.deepEqual(settled.transfers, [], "all transfers settle");
    assert.deepEqual(settled.workspaces.filter((row) => row.id === move.logical).map((row) => [row.out, row.bind]),
        [[move.target, move.targetNative]], "the riding logical has a single settled home on its assigned slot");
    assert.equal(settled.workspaces.find((row) => row.out === move.source && row.id === move.refill.ws)?.bind,
        move.refillNative, "source refill keeps its own identity and assigned slot");
    assert.equal(h.currents.get(move.target), move.targetNative, "arrived members are visible on the followed view");
    assert.equal(h.currents.get(move.source), move.refillNative, "source shows the paired refill");
    assert.equal(h.activeRef, move.active, "the moved active member retains focus");
    const roster = h.desktops.map((desk) => desk.id);
    for (const [index, slot] of move.plan.slots.entries()) {
        const native = roster[index];
        if (slot.kind === "existing") {
            assert.equal(native, slot.native, "settled roster preserves the authorized surviving slot");
        } else {
            assert.ok(native && !move.plan.baseline.order.includes(native),
                "only the plan's append grows the roster");
        }
    }
    assert.equal(roster.length, move.plan.slots.length);
    assertMemberArrival(h, move);
    assert.deepEqual(h.viewWrites.slice(move.offsets.view).sort(),
        [`${move.source}->${move.refillNative}`, `${move.target}->${move.targetNative}`].sort(),
        "one dispatch per selector, including asynchronous writes");
    const counts = [h.outputWrites.length, h.membershipWrites.length, h.viewWrites.length];
    h.fire("desktopsChanged");
    await h.settle();
    assert.deepEqual(h.desktops.map((desk) => desk.id), roster, "settled replay cannot mutate the roster");
    assert.deepEqual([h.outputWrites.length, h.membershipWrites.length, h.viewWrites.length], counts,
        "a fresh settled publication cannot replay native setters");
    assert.equal(queueCount(h), 1, "one directional migration command, never replayed");
    assert.ok(!h.logs.some((line) => line.includes("command-busy")), "no busy refills");
}

async function lagDirection(direction: "left" | "right", both: boolean): Promise<void> {
    const h = await makeHarness();
    try {
        const target = direction === "right" ? "out-b" : "out-a";
        // Arm delays after startup, before the first migration setter.
        const move = await startMigration(h, direction, both ? ["out-a", "out-b"] : [target]);
        const laggedOutputs = both ? [move.source, move.target] : [move.target];
        for (const out of laggedOutputs) {
            assert.equal(h.currents.get(out), out === move.source ? move.sourceBefore : move.targetBefore,
                `${out} still shows its old view after member arrival`);
        }
        assertMemberArrival(h, move);
        const assertHeld = async (): Promise<void> => {
            h.fire("desktopsChanged");
            h.runTimers();
            const lagged = publication(h, await h.flush());
            assert.equal(lagged.rosterPlan?.op, move.plan.op, "lag retains the same roster operation");
            assert.ok(lagged.transfers.some((row) => row.kind === "migration" && row.ws === move.logical),
                "member arrival alone cannot settle the migration");
            assertMemberArrival(h, move);
        };
        await assertHeld();
        for (let i = 0; i < laggedOutputs.length; i++) {
            const pending = h.pendingViews.entries().next().value;
            assert.ok(pending, "a selector write is actually in flight");
            const [out, deliver] = pending;
            assert.ok(laggedOutputs.includes(out));
            assert.equal(h.viewWrites.slice(move.offsets.view).filter((write) =>
                write === `${out}->${out === move.source ? move.refillNative : move.targetNative}`).length, 1,
            "the repeated unproved publication did not replay the setter");
            h.pendingViews.delete(out);
            deliver();
            if (i + 1 < laggedOutputs.length) {
                await assertHeld();
                assert.equal(h.pendingViews.size, 1, "the next selector issues only after the first arrives");
                await assertHeld();
            }
        }
        await assertSettlement(h, move);
    } finally {
        await h.stop();
    }
}

describe("world migration engine", () => {
    it("migrates through genuine slot authority with stable logical identity, refill and settlement", { timeout: 120000 }, async () => {
        const h = await makeHarness();
        try {
            const move = await startMigration(h, "right");
            await assertSettlement(h, move);
            const result = lastTakeResult(h);
            assert.equal(result["op"], "migrate-workspace");
            assert.equal(result["sourceOut"], move.source);
            assert.equal(result["targetOut"], move.target);
            assert.equal(result["targetWs"], move.logical);
            assert.ok(Array.isArray(result["members"]));
            assert.deepEqual([...result["members"]].sort(), ["win-a", "win-c"]);
        } finally {
            await h.stop();
        }
    });

    it("holds settlement while members arrived but the target view lags, both directions", { timeout: 180000 }, async () => {
        await lagDirection("right", false);
        await lagDirection("left", false);
    });

    it("retains the migration while members arrived but both views lag, then adopts exact selector readbacks", { timeout: 180000 }, async () => {
        await lagDirection("right", true);
    });
});
