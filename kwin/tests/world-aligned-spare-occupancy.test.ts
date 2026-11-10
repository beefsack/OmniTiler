// Eager-spare lifecycle for per-output workspaces.
// Producer-shaped regression through the real `WorldEngine` (via the
// test-only `raw_world_eval` example) plus the production entry/apply path
// with fresh serial replies: showing an empty end creates no spare and no
// native (the rings stay minimal with locally-retired middles fenced by
// native-only absence intents), relative-next wraps from the live end on
// both outputs with the opposite view unchanged, a reshow of a
// locally-retired native readopts fresh with positive output-local evidence
// and mints one backed spare (a single shared native trails both rings),
// the departed fresh middle retires locally while its native survives, and
// a real occupancy arrival on an output's live end extends the ring with a
// backed trailing spare per output (each output's spare carries an
// independent core desire; the adapter executes each exactly once and the
// report receipts correlate op to created native). The superseded
// intermediate backing retires locally on both outputs while its native
// survives (the production sticky anchor vetoes every removal). The filled
// end keeps its logical and binding, the newcomer homes on the filled
// logical, no existing window moves, no view switches, and no transfers
// are retained. Included in `npm test`.

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
    name: string;
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
    minSize: { width: number; height: number } | null;
    maxSize: { width: number; height: number } | null;
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
    readonly __signals?: { readonly outputChanged: FakeSignal; readonly desktopsChanged: FakeSignal; readonly hiddenChanged: FakeSignal; readonly frameGeometryChanged: FakeSignal };
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
    // Actual native setter counts (not desires): creations append the fresh
    // id, removals append the deleted id, every window `desktops`
    // reassignment that changes the id set appends to memberWrites.
    readonly creations: string[];
    readonly removals: string[];
    readonly memberWrites: string[];
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

const LEFT = "out-left";
const RIGHT = "out-right";

function trackWin(h: Harness, win: FakeWindow): void {
    let desks = win.desktops;
    Object.defineProperty(win, "desktops", {
        configurable: true,
        get: () => desks,
        set: (refs: FakeDesktop[]) => {
            // Only genuine membership changes count: the fake host's own
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

function counters(h: Harness): { natives: number; creations: number; removals: number; members: number; views: number } {
    return {
        natives: h.desktops.length,
        creations: h.creations.length,
        removals: h.removals.length,
        members: h.memberWrites.length,
        views: h.viewWrites.length,
    };
}

// Every core creation desire observed in replies answered at/after a call
// index. Outstanding desires repeat across replies until executed, so
// callers dedupe by op: one entry per independent desire.
function desiresSince(h: Harness, fromCall: number): Array<Record<string, unknown>> {
    const out: Array<Record<string, unknown>> = [];
    for (const [at, raw] of h.replies) {
        if (at < fromCall || (h.calls[at] as CapturedCall).method !== "DescribeWorld") {
            continue;
        }
        const parsed = JSON.parse(raw) as Record<string, unknown>;
        for (const desire of (parsed["nativeCreations"] as Array<Record<string, unknown>>) ?? []) {
            out.push(desire);
        }
    }
    return out;
}

// Creation receipts the adapter rode on dispatched reports: the causal
// op-to-native proof (op/ws/out plus the created id) for every executed
// desire. The floor pad emits no receipt, so any creation here must have
// exactly one receipt.
function creationReceipts(h: Harness): Array<Record<string, unknown>> {
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

async function makeSpareHarness(): Promise<Harness> {
    const desktops: FakeDesktop[] = ["ws-1", "ws-2", "ws-3", "ws-4"].map((id) => ({ id, name: id }));
    const byId = new Map(desktops.map((d) => [d.id, d]));
    // Monotonic native ids: removals shrink the list, so `length + 1` would
    // collide with a survivor. Production ids are opaque uniques; the fake
    // mints `ws-N` from a counter that never rewinds.
    let nextDesk = desktops.length + 1;
    const screens = [
        { name: LEFT, geometry: { x: 0, y: 0, width: 1000, height: 800 } },
        { name: RIGHT, geometry: { x: 1000, y: 50, width: 900, height: 700 } },
    ];
    const currents = new Map([[LEFT, "ws-4"], [RIGHT, "ws-2"]]);
    const mkWin = (
        id: string, out: string, ws: string,
        sticky: boolean,
    ): FakeWindow => {
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
            // Production sticky shape: an empty desktop list. The observer
            // homes it at its output's current (world-observe.ts), so the
            // anchor follows the view instead of pinning the old cell.
            desktops: sticky ? [] : [byId.get(ws) as FakeDesktop],
            minimized: false,
            hidden: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: sticky,
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
    const wins: FakeWindow[] = [
        mkWin("win-a", LEFT, "ws-1", false),
        mkWin("win-b", RIGHT, "ws-2", false),
        // Production-shaped sticky anchor: empty desktop list, homed by the
        // observer at its output's current. It stages the shown empty end
        // without occupying any backing cell, and rehomes on departure so
        // the old end retires normally instead of looping removal.
        mkWin("win-sticky", LEFT, "ws-4", true),
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
        viewWrites: [],
        creations: [],
        removals: [],
        memberWrites: [],
        activeScreen: { name: LEFT },
        activeRef: wins[2] as FakeWindow,
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
        set activeScreen(_ref: unknown) {
            void _ref;
        },
        get cursorPos(): { x: number; y: number } {
            // Production-shaped cursor inventory: the cursor rides the
            // active output, which the core's cursor-first selection
            // source resolution requires for output-relative commands.
            return harness.activeScreen.name === RIGHT ? { x: 1002, y: 100 } : { x: 2, y: 100 };
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
        sendClientToScreen: (): void => undefined,
        // Production void creation: append one native carrying the logical
        // token as its name for exact-name attribution. Proof comes from
        // the fresh named enumeration, never a return value. Monotonic
        // ids: the eager lifecycle removes and creates across legs, and
        // `length + 1` would collide with a survivor after any removal.
        createDesktop: (position: unknown, name: unknown): void => {
            assert.equal(typeof position, "number", "creation carries a numeric position");
            assert.equal(typeof name, "string", "creation carries a logical token name");
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
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
    });
    assert.notEqual(handle, null, "world production entry starts on the fake surface");
    (harness as { handle: unknown }).handle = handle;
    for (const win of wins) {
        trackWin(harness, win);
    }
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

function transfersOf(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return (reply["transfers"] as Array<Record<string, unknown>>) ?? [];
}

function liveRows(reply: Record<string, unknown>, out: string): Array<Record<string, unknown>> {
    return workspacesOf(reply)
        .filter((row) => row["out"] === out && row["exist"] !== false)
        .sort((a, b) => (a["order"] as number) - (b["order"] as number));
}

function deadRows(reply: Record<string, unknown>, out: string): Array<Record<string, unknown>> {
    return workspacesOf(reply).filter((row) => row["out"] === out && row["exist"] === false);
}

describe("world spare eager-backing", () => {
    it("showing empty ends creates no spare on either output, forward next wraps, occupancy still extends", { timeout: 240000 }, async () => {
        const h = await makeSpareHarness();
        try {
            // Authentic v4 connection pump: the handshake binds the producer
            // adapter lifetime and mints the serving generation; the
            // revisioned settings snapshot (rev 1) precedes the first report.
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
            // Showing empty ends creates no spare and no native: the rings
            // stay minimal (two live rows each) with locally-retired
            // middles fenced by native-only absence intents (every row,
            // live or retired, carries its native bind; no unbound rows).
            assert.equal(h.desktops.length, 4, "no native appended while showing empty ends");
            assert.deepEqual(h.creations, [], "showing ends creates no native");
            assert.deepEqual(h.removals, [], "showing ends removes no native");
            let reply = lastReply(h);
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "no unbound spare rows while showing empty ends",
            );

            // empty-list sticky anchor, homed by the observer at the shown
            // current. The anchor follows the view: after this departure it
            // rehomes onto ws-1 and the old end stays live-bound on the
            // left (native-bound empties survive; locally-retired middles
            // carry the absence intent instead). The wrap moves no window.
            const wrapBase = counters(h);
            h.activeScreen = { name: LEFT };
            h.activeRef = h.wins[0] as FakeWindow;
            h.handle.requestWorkspaceRelative(1);
            h.runTimers();
            await h.settle();
            reply = lastReply(h);
            assert.equal(h.currents.get(LEFT), "ws-1", "relative next wraps from the shown empty end");
            assert.equal(h.currents.get(RIGHT), "ws-2", "opposite view unchanged by left wrap");
            assert.equal(counters(h).members - wrapBase.members, 0, "wrap moves no windows");
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "wrap mints no spare (bound rows only)",
            );
            const departedRow = liveRows(reply, LEFT).find((row) => (row["bind"] as string) === "ws-4");
            assert.ok(
                departedRow !== undefined,
                `departed end stays as a native-bound empty (desktops=${h.desktops.map((d) => d.id).join(",")})`,
            );

            // Reshow the locally-retired ws-3 on the left: selecting a
            // retired native is positive output-local evidence, so the core
            // clears the intent and adopts fresh, then eagerly backs the new
            // trailing spare with one shared native (a single creation
            // trails both rings). The opposite view is unchanged.
            const leftStoneBefore = deadRows(reply, LEFT).find((row) => (row["bind"] as string) === "ws-3")?.["id"] as string;
            assert.ok(leftStoneBefore !== undefined, "ws-3 starts locally retired on the left");
            const reshowBase = counters(h);
            h.currents.set(LEFT, "ws-3");
            h.fire("currentDesktopChanged");
            await h.settle();
            reply = lastReply(h);
            assert.equal(counters(h).creations - reshowBase.creations, 1, "reshow backs the fresh trailing spare exactly once");
            assert.equal(counters(h).removals - reshowBase.removals, 0, "reshow removes no native");
            const freshLeft = liveRows(reply, LEFT).find((row) => (row["bind"] as string) === "ws-3");
            assert.ok(freshLeft !== undefined, "reshow adopts the retired native fresh on the left");
            assert.notEqual(freshLeft?.["id"], leftStoneBefore, "fresh adoption mints a new logical, never the retired id");
            assert.ok(
                !deadRows(reply, LEFT).some((row) => (row["bind"] as string) === "ws-3"),
                "local selection clears the left absence intent",
            );
            const sharedSpare = h.creations[h.creations.length - 1] as unknown as string;
            assert.equal(liveRows(reply, LEFT).pop()?.["bind"], sharedSpare, "new native trails the left ring");
            assert.equal(liveRows(reply, RIGHT).pop()?.["bind"], sharedSpare, "one shared native trails both rings");
            assert.equal(h.currents.get(RIGHT), "ws-2", "opposite view unchanged by left reshow");
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "reshow keeps native-only rows (no unbound logicals)",
            );
            assert.ok(transfersOf(reply).length === 0, "reshow retains no transfers");
            h.activeScreen = { name: LEFT };
            h.handle.requestWorkspaceSelect(1);
            h.runTimers();
            await h.settle();
            reply = lastReply(h);
            assert.equal(h.currents.get(LEFT), "ws-1", "numbered 1 departs the shown empty end");
            assert.equal(h.currents.get(RIGHT), "ws-2", "opposite view unchanged by left departure");
            // The departed fresh middle retires locally (the tombstone
            // reuses the removed logical id) while its native survives; the
            // trailing stays and no new native is minted by the departure.
            const retiredLeft = deadRows(reply, LEFT).find((row) => (row["bind"] as string) === "ws-3");
            assert.ok(retiredLeft !== undefined, "departed middle retires locally on the left");
            assert.equal(retiredLeft?.["id"], freshLeft?.["id"], "tombstone reuses the removed logical id");
            assert.ok(
                !liveRows(reply, LEFT).some((row) => row["id"] === retiredLeft?.["id"]),
                "absence intent is never a live ring row",
            );
            assert.ok(h.desktops.some((d) => d.id === "ws-3"), "retired native survives the local prune");
            assert.equal(counters(h).creations - reshowBase.creations, 1, "departure mints no further native");
            assert.equal(liveRows(reply, LEFT).pop()?.["bind"], sharedSpare, "backed trailing survives the departure");
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "departure keeps native-only rows",
            );
            assert.ok(transfersOf(reply).length === 0, "departure retains no transfers");

            // Same on the right output: its locally-retired ws-3 readopts
            // fresh on selection and mints one shared backed spare trailing
            // both rings. Left view unchanged.
            const rightStoneBefore = deadRows(reply, RIGHT).find((row) => (row["bind"] as string) === "ws-3")?.["id"] as string;
            assert.ok(rightStoneBefore !== undefined, "ws-3 starts locally retired on the right");
            const rightReshowBase = counters(h);
            h.activeScreen = { name: RIGHT };
            h.activeRef = h.wins[1] as FakeWindow;
            h.currents.set(RIGHT, "ws-3");
            h.fire("currentDesktopChanged");
            await h.settle();
            reply = lastReply(h);
            assert.equal(counters(h).creations - rightReshowBase.creations, 1, "right reshow backs its trailing spare exactly once");
            const freshRight = liveRows(reply, RIGHT).find((row) => (row["bind"] as string) === "ws-3");
            assert.ok(freshRight !== undefined, "right reshow adopts the retired native fresh");
            assert.notEqual(freshRight?.["id"], rightStoneBefore, "fresh adoption mints a new logical, never the retired id");
            assert.ok(
                !deadRows(reply, RIGHT).some((row) => (row["bind"] as string) === "ws-3"),
                "local selection clears the right absence intent",
            );
            const sharedRight = h.creations[h.creations.length - 1] as unknown as string;
            assert.equal(liveRows(reply, RIGHT).pop()?.["bind"], sharedRight, "new native trails the right ring");
            assert.equal(liveRows(reply, LEFT).pop()?.["bind"], sharedRight, "one shared native trails both rings");
            assert.equal(h.currents.get(LEFT), "ws-1", "left view unchanged by right reshow");
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "right reshow keeps native-only rows",
            );
            // Forward next follows the live ring: from the fresh ws-3 cell
            // the next cell is the backed trailing spare, so the view lands
            // on the shared native (ws-4 retired once genuinely empty and
            // unviewed, so the old retained end no longer exists). The
            // departed fresh middle retires locally while its native
            // survives, the opposite view is unchanged, and the outcome is
            // quiescent (a further settle creates, removes, and moves
            // nothing).
            const ringBefore = liveRows(reply, RIGHT);
            const atBefore = ringBefore.findIndex((row) => (row["bind"] as string) === h.currents.get(RIGHT));
            assert.ok(atBefore >= 0, "current sits on the live right ring");
            const expectedBind = ringBefore[(atBefore + 1) % ringBefore.length]?.["bind"] as string;
            assert.equal(expectedBind, sharedRight, "live ring ends on the backed spare");
            const relBase = counters(h);
            h.handle.requestWorkspaceRelative(1);
            h.runTimers();
            await h.settle();
            reply = lastReply(h);
            assert.equal(h.currents.get(RIGHT), expectedBind, "relative next lands on the backed trailing spare");
            assert.equal(h.currents.get(LEFT), "ws-1", "left view unchanged by right step");
            const retiredRight = deadRows(reply, RIGHT).find((row) => (row["bind"] as string) === "ws-3");
            assert.ok(retiredRight !== undefined, "departed middle retires locally on the right");
            assert.equal(retiredRight?.["id"], freshRight?.["id"], "tombstone reuses the removed logical id");
            assert.ok(h.desktops.some((d) => d.id === "ws-3"), "retired native survives the local prune");
            assert.ok(
                !liveRows(reply, RIGHT).some((row) => row["id"] === retiredRight?.["id"]),
                "absence intent is never a live ring row",
            );
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "right step keeps native-only rows",
            );
            assert.ok(transfersOf(reply).length === 0, "right step retains no transfers");
            const relAfter = counters(h);
            assert.equal(relAfter.creations - relBase.creations, 0, "step settles with no further creation");
            await h.settle();
            assert.deepEqual(counters(h), relAfter, "stepped lifecycle does not oscillate");

            // Occupancy arrival extends the ring with eagerly-backed spares:
            // a real window on an output's live end mints one independent
            // trailing-spare desire per output (each output's spare is its
            // own logical; the desires are independent, never migration or
            // outside-native creation), the adapter executes each exactly
            // once (one report receipt per op, created id equals the fresh
            // native), and both rings converge trailing on the latest
            // backing while the superseded intermediate retires locally on
            // both outputs with its native surviving (the sticky anchor
            // vetoes every removal). The filled end keeps its logical and
            // binding, the newcomer homes on the filled logical, no existing
            // window moves, no view switches, and no transfers are retained.
            const fillEnd = (tag: string, out: string, ws: string): void => {
                const desk = h.desktops.find((d) => d.id === ws) as FakeDesktop;
                assert.ok(desk !== undefined, `fill target native ${ws} exists`);
                const frame = { x: 8, y: 52, width: 200, height: 200 };
                const newcomer: FakeWindow = {
                    internalId: tag, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                    normalWindow: true,
                    frameGeometry: { ...frame },
                    clientGeometry: { x: frame.x + 2, y: frame.y + 21, width: Math.max(1, frame.width - 4), height: Math.max(1, frame.height - 22) },
                    output: { name: out },
                    desktops: [desk],
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
                    outputChanged: makeSignal().signal,
                    desktopsChanged: makeSignal().signal,
                    hiddenChanged: makeSignal().signal,
                    frameGeometryChanged: makeSignal().signal,
                };
                h.wins.push(newcomer);
                trackWin(h, newcomer);
            };
            // Fill the left live end (the backed spare both rings trail on):
            // the shared trailing native is occupied, so each output mints
            // its own next spare desire.
            const leftEnd = liveRows(reply, LEFT).pop() as Record<string, unknown>;
            const leftFilledLogical = leftEnd["id"] as string;
            const leftFilledNative = leftEnd["bind"] as string;
            const leftCallsBefore = h.calls.length;
            const leftReceiptsBefore = creationReceipts(h).length;
            const leftBase = counters(h);
            fillEnd("win-c", LEFT, leftFilledNative);
            h.fire("windowAdded");
            await h.settle();
            reply = lastReply(h);
            {
                const newNatives = h.desktops.slice(leftBase.natives).map((d) => d.id);
                assert.equal(newNatives.length, 2, `left fill mints one backing per output spare, got ${newNatives.join(",")}`);
                const desires = desiresSince(h, leftCallsBefore);
                const byOp = new Map<string, Record<string, unknown>>();
                for (const desire of desires) {
                    byOp.set(desire["op"] as string, desire);
                }
                assert.equal(byOp.size, 2, `exactly two independent spare desires, got ${JSON.stringify(desires)}`);
                const outs = [...byOp.values()].map((desire) => desire["out"] as string).sort();
                assert.deepEqual(outs, [LEFT, RIGHT], "each output desires its own spare, never migration");
                const leftDesire = [...byOp.values()].find((desire) => desire["out"] === LEFT) as Record<string, unknown>;
                const rightDesire = [...byOp.values()].find((desire) => desire["out"] === RIGHT) as Record<string, unknown>;
                const freshReceipts = creationReceipts(h).slice(leftReceiptsBefore);
                assert.equal(freshReceipts.length, 2, `each desire executes exactly once, got ${JSON.stringify(freshReceipts)}`);
                for (const receipt of freshReceipts) {
                    assert.ok(byOp.has(receipt["op"] as string), `receipt answers an observed desire, got ${JSON.stringify(receipt)}`);
                    assert.equal(receipt["setter"], "accepted", "backing executes cleanly");
                }
                const created = freshReceipts.map((receipt) => receipt["created"] as string).sort();
                assert.deepEqual(created, [...newNatives].sort(), "receipts correlate ops to the fresh natives");
                // The filled end keeps its logical and binding while the
                // newcomer homes on it; both rings converge trailing on the
                // latest backing and the superseded intermediate retires
                // locally on both outputs with its native surviving.
                const filledRow = liveRows(reply, LEFT).find((row) => row["id"] === leftFilledLogical);
                assert.equal(filledRow?.["bind"], leftFilledNative, "filled end keeps its logical and binding");
                const winRows = reply["windows"] as Array<Record<string, unknown>>;
                assert.ok(winRows.some((w) => w["win"] === "win-c" && w["out"] === LEFT && w["ws"] === leftFilledLogical), "newcomer homed on the filled end");
                const latest = newNatives[newNatives.length - 1] as string;
                const intermediate = newNatives[0] as string;
                assert.equal(liveRows(reply, LEFT).pop()?.["bind"], latest, "left ring trails the latest backing");
                assert.equal(liveRows(reply, RIGHT).pop()?.["bind"], latest, "right ring trails the latest backing");
                assert.equal(rightDesire["ws"], liveRows(reply, RIGHT).pop()?.["id"], "right desire names its own trailing spare");
                assert.ok(
                    deadRows(reply, LEFT).some((row) => (row["bind"] as string) === intermediate),
                    "superseded backing retires locally on the left",
                );
                assert.ok(
                    deadRows(reply, RIGHT).some((row) => (row["bind"] as string) === intermediate),
                    "superseded backing retires locally on the right",
                );
                assert.ok(h.desktops.some((d) => d.id === intermediate), "superseded native survives (sticky removal veto)");
                assert.ok(leftDesire["ws"] !== undefined && (leftDesire["ws"] as string) !== "", "left desire names its spare logical");
                void leftDesire;
            }
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "filled rings keep native-only rows",
            );
            assert.deepEqual(reply["nativeCreations"], [], "desires settle once executed");
            assert.equal(counters(h).removals - leftBase.removals, 0, "fill removes no native");
            assert.equal(counters(h).members - leftBase.members, 0, "fill moves no existing window");
            assert.equal(counters(h).views - leftBase.views, 0, "fill switches no view");
            assert.ok(
                transfersOf(reply).length === 0,
                `settled with no retained transfers, got ${JSON.stringify(transfersOf(reply))}`,
            );
            const leftAfter = counters(h);
            await h.settle();
            assert.deepEqual(counters(h), leftAfter, "filled lifecycle does not oscillate");
            // Fill the right live end (the converged trailing both rings
            // share): mirror image, one independent desire per output.
            const rightEnd = liveRows(reply, RIGHT).pop() as Record<string, unknown>;
            const rightFilledLogical = rightEnd["id"] as string;
            const rightFilledNative = rightEnd["bind"] as string;
            const rightCallsBefore = h.calls.length;
            const rightReceiptsBefore = creationReceipts(h).length;
            const rightBase = counters(h);
            fillEnd("win-d", RIGHT, rightFilledNative);
            h.fire("windowAdded");
            await h.settle();
            reply = lastReply(h);
            {
                const newNatives = h.desktops.slice(rightBase.natives).map((d) => d.id);
                assert.equal(newNatives.length, 2, `right fill mints one backing per output spare, got ${newNatives.join(",")}`);
                const desires = desiresSince(h, rightCallsBefore);
                const byOp = new Map<string, Record<string, unknown>>();
                for (const desire of desires) {
                    byOp.set(desire["op"] as string, desire);
                }
                assert.equal(byOp.size, 2, `exactly two independent spare desires, got ${JSON.stringify(desires)}`);
                const outs = [...byOp.values()].map((desire) => desire["out"] as string).sort();
                assert.deepEqual(outs, [LEFT, RIGHT], "each output desires its own spare, never migration");
                const freshReceipts = creationReceipts(h).slice(rightReceiptsBefore);
                assert.equal(freshReceipts.length, 2, `each desire executes exactly once, got ${JSON.stringify(freshReceipts)}`);
                const created = freshReceipts.map((receipt) => receipt["created"] as string).sort();
                assert.deepEqual(created, [...newNatives].sort(), "receipts correlate ops to the fresh natives");
                const filledRow = liveRows(reply, RIGHT).find((row) => row["id"] === rightFilledLogical);
                assert.equal(filledRow?.["bind"], rightFilledNative, "filled end keeps its logical and binding");
                const winRows = reply["windows"] as Array<Record<string, unknown>>;
                assert.ok(winRows.some((w) => w["win"] === "win-d" && w["out"] === RIGHT && w["ws"] === rightFilledLogical), "newcomer homed on the filled end");
                const latest = newNatives[newNatives.length - 1] as string;
                const intermediate = newNatives[0] as string;
                assert.equal(liveRows(reply, RIGHT).pop()?.["bind"], latest, "right ring trails the latest backing");
                assert.equal(liveRows(reply, LEFT).pop()?.["bind"], latest, "left ring trails the latest backing");
                assert.ok(
                    deadRows(reply, RIGHT).some((row) => (row["bind"] as string) === intermediate),
                    "superseded backing retires locally on the right",
                );
                assert.ok(
                    deadRows(reply, LEFT).some((row) => (row["bind"] as string) === intermediate),
                    "superseded backing retires locally on the left",
                );
                assert.ok(h.desktops.some((d) => d.id === intermediate), "superseded native survives (sticky removal veto)");
            }
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "filled rings keep native-only rows",
            );
            assert.deepEqual(reply["nativeCreations"], [], "desires settle once executed");
            assert.equal(counters(h).removals - rightBase.removals, 0, "fill removes no native");
            assert.equal(counters(h).members - rightBase.members, 0, "fill moves no existing window");
            assert.equal(counters(h).views - rightBase.views, 0, "fill switches no view");
            assert.ok(
                transfersOf(reply).length === 0,
                `settled with no retained transfers, got ${JSON.stringify(transfersOf(reply))}`,
            );
            const rightAfter = counters(h);
            await h.settle();
            assert.deepEqual(counters(h), rightAfter, "filled lifecycle does not oscillate");
        } finally {
            await h.stop();
        }
    });
});
