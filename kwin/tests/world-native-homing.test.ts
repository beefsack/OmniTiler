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
    readonly handlers: Array<(...args: unknown[]) => void>;
    readonly signal: {
        connect: (handler: (...args: unknown[]) => void) => void;
        disconnect: (handler: (...args: unknown[]) => void) => void;
    };
}

function makeSignal(): FakeSignal {
    const handlers: Array<(...args: unknown[]) => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: (...args: unknown[]) => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: (...args: unknown[]) => void): void => {
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
    readonly logs: string[];
    readonly workspaceFires: string[];
    deferOutput: boolean;
    gesture: boolean;
    readonly outputRequests: FakeWindow[];
    readonly rosterWrites: string[];
    readonly captureWrappers: Map<FakeWindow, FakeWindow>;
    activeScreen: { name: string };
    activeRef: FakeWindow | null;
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    fire: (name: string, ...args: unknown[]) => void;
    fireWindow: (win: FakeWindow, name: "outputChanged" | "desktopsChanged" | "hiddenChanged" | "frameGeometryChanged") => void;
    selectDesktop: (out: string, native: string) => void;
    runTimers: (maxDelay?: number) => void;
    flush: () => Promise<number>;
    flushAll: () => Promise<void>;
    settle: () => Promise<void>;
    stop: () => Promise<void>;
}

const LEFT = "out-left";
const RIGHT = "out-right";

// Truthful native decoration: stable nonnegative insets mirroring the v4
// reference (frame 8.4,8.6,800,600 -> client 10,30,796,578: left 2, top 21,
// right 2, bottom 1; deltaW 4, deltaH 22). The tiler writes frameGeometry
// only; the native client tracks with the same insets so every capture
// carries consistent frame/client facts.
const DECOR = { left: 2, top: 21, right: 2, bottom: 1 };
function truthfulClientFor(f: { x: number; y: number; width: number; height: number }): { x: number; y: number; width: number; height: number } {
    const w = Math.max(1, f.width - (DECOR.left + DECOR.right));
    const h = Math.max(1, f.height - (DECOR.top + DECOR.bottom));
    const x = f.x + Math.min(DECOR.left, Math.max(0, f.width - w));
    const y = f.y + Math.min(DECOR.top, Math.max(0, f.height - h));
    return { x, y, width: w, height: h };
}

async function makeHarness(): Promise<Harness> {
    const desktops: FakeDesktop[] = ["ws-1", "ws-2", "ws-3", "ws-4"].map((id) => ({ id, name: id }));
    const byId = new Map(desktops.map((d) => [d.id, d]));
    let nextDesktopId = desktops.length + 1;
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
        const winHiddenChanged = makeSignal();
        const winFrameChanged = makeSignal();
        // Truthful native fake: finite minimum, the unbounded INT_MAX
        // maximum the KWin backends report (never a (0,0) cap), a positive
        // pid, resizable, class identity, and a client rect tracking every
        // frame write with the stable decoration insets.
        let liveFrame = { ...frame };
        let liveClient = truthfulClientFor(liveFrame);
        const win: FakeWindow = {
            internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { ...frame },
            clientGeometry: { ...liveClient },
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
            caption: `Window ${id}`,
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
        Object.defineProperty(win, "frameGeometry", {
            configurable: true,
            enumerable: true,
            get: () => liveFrame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                liveFrame = rect;
                liveClient = truthfulClientFor(rect);
            },
        });
        Object.defineProperty(win, "clientGeometry", {
            configurable: true,
            enumerable: true,
            get: () => liveClient,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                liveClient = rect;
            },
        });
        return win;
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
        deferOutput: false,
        gesture: false,
        outputRequests: [],
        rosterWrites: [],
        captureWrappers: new Map(),
        replies: new Map<number, string>(),
        takeReplies: [],
        timers: [],
        logs: [],
        workspaceFires: [],
        activeScreen: { name: RIGHT },
        activeRef: wins[0] as FakeWindow,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        fire: (name: string, ...args: unknown[]): void => {
            harness.workspaceFires.push(name);
            for (const handler of [...(signals[name] as FakeSignal).handlers]) {
                handler(...args);
            }
        },
        fireWindow: (win: FakeWindow, name: "outputChanged" | "desktopsChanged" | "hiddenChanged" | "frameGeometryChanged"): void => {
            const sig = win.__signals?.[name];
            assert.ok(sig !== undefined, `window ${win.internalId} exposes ${name}`);
            for (const handler of [...sig.handlers]) {
                handler();
            }
        },
        selectDesktop: (out, native): void => {
            const screen = screenByName(out);
            const desktop = desktopById(native);
            const previous = desktopById(currents.get(out) ?? "");
            if (previous === desktop) return;
            currents.set(out, native);
            harness.rosterWrites.push(`view:${out}->${native}`);
            harness.fire("currentDesktopChanged", previous, desktop, screen);
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
            const { reply: coreReply, settlements } = parseRawEvalWithSettlements(await harness.bridge.send("DescribeWorldWithSettlements", stamped));
            const reply = harness.broker.bindReply(coreReply, envelope, settlements);
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
        windowList: (): FakeWindow[] => harness.wins.map(win => harness.captureWrappers.get(win) ?? win),
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
            // intended active output, which the core's cursor-first
            // selection source resolution requires for output-relative
            // commands.
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
            const found = harness.wins.find(win => win === ref || harness.captureWrappers.get(win) === ref);
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
        currentDesktopForScreen: (screen: { name: string }): FakeDesktop => {
            return desktopById(harness.currents.get(screen.name) ?? "");
        },
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }): void => {
            const desktop = deskRef as FakeDesktop;
            assert.equal(byId.get(desktop.id), desktop, `desktop ${desktop.id} is live`);
            harness.selectDesktop(screen.name, desktop.id);
        },
        // Faithful pinned `sendClientToScreen` echo with window-level
        // signals only (no workspace signal).
        sendClientToScreen: (mover: unknown, out: unknown): void => {
            const win = mover as FakeWindow;
            const name = (out as { name: string }).name;
            const from = win.output.name;
            harness.outputRequests.push(win);
            if (harness.deferOutput) return;
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
        // KWin's void API inserts the requested name at the supplied
        // zero-based position; list readback sees it before the signal.
        createDesktop: (position: unknown, name: unknown): void => {
            assert.equal(typeof position, "number", "creation carries a numeric position");
            assert.equal(typeof name, "string", "creation carries a logical token name");
            assert.ok(Number.isInteger(position) && (position as number) >= 0 && (position as number) <= desktops.length);
            const id = `ws-${nextDesktopId++}`;
            const desk: FakeDesktop = { id, name: name as string };
            desktops.splice(position as number, 0, desk);
            byId.set(id, desk);
            harness.rosterWrites.push(`create:${id}`);
            harness.fire("desktopsChanged");
        },
        removeDesktop: (desktop: FakeDesktop): void => {
            const index = desktops.indexOf(desktop);
            if (index < 0 || desktops.length === 1) return;
            // These journeys only authorize deleting a globally vacant,
            // unshown desktop. A native fallback would mask a bad plan.
            assert.ok(![...currents.values()].includes(desktop.id), "removed desktop is not shown");
            assert.ok(!wins.some(win => !win.onAllDesktops && win.desktops.includes(desktop)), "removed desktop is vacant");
            desktops.splice(index, 1);
            byId.delete(desktop.id);
            harness.rosterWrites.push(`remove:${desktop.id}`);
            harness.fire("desktopsChanged");
        },
    };
    for (const win of wins) {
        let desks = win.desktops;
        Object.defineProperty(win, "desktops", {
            configurable: true,
            get: (): FakeDesktop[] => desks,
            set: (refs: FakeDesktop[]) => {
                if (refs.length === desks.length && refs.every((ref, index) => ref === desks[index])) return;
                desks = refs;
                harness.rosterWrites.push(`member:${win.internalId}->${refs.map(ref => ref.id).join(",")}`);
                for (const handler of [...(win.__signals?.desktopsChanged.handlers ?? [])]) {
                    handler();
                }
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
        gestureActive: () => harness.gesture,
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
    assert.deepEqual(h.desktops.map(d => d.id), ["ws-1", "ws-2", "ws-3"], "startup retains the two occupied desktops and right spare");
    assert.deepEqual(h.rosterWrites, ["remove:ws-4"], "startup compacts once without rewriting member or view homes");
    assert.deepEqual(h.outputRequests, [], "startup performs no output sends");
    assert.equal(lastReply(h)["rosterPlan"], undefined, "startup removal readback settles the native plan without replay");
    return gen;
}

function lastReply(h: Harness): Record<string, unknown> {
    const indices = [...h.replies.keys()].filter((at) => (h.calls[at] as CapturedCall).method === "DescribeWorld");
    assert.ok(indices.length > 0, "at least one answered DescribeWorld");
    const raw = h.replies.get(Math.max(...indices) as number) as string;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    assert.equal(parsed["outcome"] ?? "accepted", "accepted", `fresh reply accepted, got ${raw.slice(0, 300)}`);
    assert.equal("result" in parsed, false, "terminal result rides the take lane only, never the capture reply");
    return parsed;
}

function transfersOf(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return (reply["transfers"] as Array<Record<string, unknown>>) ?? [];
}

function homingOf(h: Harness, index: number, win: string): { out: string; ws: string } | null {
    // Tagged native fact: the inner report carries the single-homed native
    // desktop as `nativeWs` (never a logical slot); exact out/ws asserted,
    // never loosened.
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
    for (const arrivalSignal of ["outputChanged", "desktopsChanged", "frameGeometryChanged"] as const) {
        it(`expedites an asynchronous tiled crossing on the mover's exact ${arrivalSignal} proof`, async () => {
            const h = await makeHarness();
            try {
                h.currents.set(RIGHT, "ws-1");
                await discover(h);
                const mover = h.wins[0] as FakeWindow;
                const sibling = h.wins[2] as FakeWindow;
                const fromFrame = { ...mover.frameGeometry };
                // QML enumeration and signal subscriptions need not expose
                // the same JS wrapper for a native window lifetime.
                const applyRef = arrivalSignal === "outputChanged" ? new Proxy(mover, {}) : mover;
                h.captureWrappers.set(mover, applyRef);
                h.deferOutput = true;
                const outputBefore = h.outputRequests.length;
                h.handle.requestSendToOutput("left", true);
                h.runTimers(0);
                await h.flushAll();
                assert.deepEqual(h.outputRequests.slice(outputBefore), [applyRef]);
                assert.equal(h.outputRequests[outputBefore], applyRef);
                assert.equal(mover.output.name, RIGHT, "setter returns before output changes");
                assert.deepEqual(mover.frameGeometry, fromFrame, "no final frame on the source");
                assert.ok(h.timers.some(t => t.delayMs === WORLD_DEBOUNCE_MS && !t.cancelled && !t.fired));
                const before = h.calls.length;

                // No exact homing yet, even though the mover's output changed.
                mover.desktops = [h.desktops[1] as FakeDesktop];
                mover.output = { name: LEFT };
                h.fireWindow(mover, "outputChanged");
                h.runTimers(0);
                assert.equal(h.calls.length, before, "target output alone cannot promote capture");
                const unreadable: FakeDesktop[] = [h.desktops[0] as FakeDesktop];
                Object.defineProperty(unreadable, 0, { get: () => { throw new Error("native desktop unreadable"); } });
                Object.defineProperty(mover, "desktops", { value: unreadable, configurable: true });
                h.fireWindow(mover, arrivalSignal);
                h.runTimers(0);
                assert.equal(h.calls.length, before, "unreadable proof does not expedite or throw");
                // Set the exact native desktop independently of the signal;
                // an unrelated window must not use the mover's proof.
                Object.defineProperty(mover, "desktops", { value: [h.desktops[0]], configurable: true });
                h.fireWindow(sibling, "outputChanged");
                h.runTimers(0);
                assert.equal(h.calls.length, before);

                h.fireWindow(mover, arrivalSignal);
                assert.equal(h.timers.filter(t => t.delayMs === 0 && !t.cancelled && !t.fired).length, 1,
                    `the ${arrivalSignal} signal itself schedules the early capture`);
                h.fireWindow(mover, "frameGeometryChanged");
                h.runTimers(0);
                assert.equal(h.calls.length, before + 1, "mover proof preempts the 120 ms timer exactly once");
                assert.deepEqual(homingOf(h, before, mover.internalId), { out: LEFT, ws: "ws-1" });
                assert.deepEqual(mover.frameGeometry, fromFrame, "the signal is not a geometry writer");
                await h.flushAll();
                const target = (lastReply(h)["windows"] as Array<Record<string, unknown>>).find(w => w["win"] === mover.internalId);
                const [x, y, width, height] = (target?.["frame"] ?? target?.["alloc"]) as number[];
                assert.deepEqual(mover.frameGeometry, { x, y, width, height }, "the expedited reply sets the final tile");
                assert.notDeepEqual(mover.frameGeometry, fromFrame);
                assert.deepEqual(h.outputRequests.slice(outputBefore), [applyRef], "no repeat of the dispatched transfer");

                const spent = h.calls.length;
                h.fireWindow(mover, "outputChanged");
                h.runTimers(0);
                assert.equal(h.calls.length, spent, "late duplicate after spend uses ordinary debounce");
                await h.settle();
                assert.deepEqual(transfersOf(lastReply(h)), []);
                assert.deepEqual(h.outputRequests.slice(outputBefore), [applyRef], "ordinary duplicate observation does not replay the send");
            } finally {
                await h.stop();
            }
        });
    }

    for (const intervening of ["none", "gesture", "leaves target"] as const) {
        it(`bounds the fresh catch-up after a stale crossing arrival (${intervening})`, async () => {
            const h = await makeHarness();
            try {
                h.currents.set(RIGHT, "ws-1");
                await discover(h);
                const mover = h.wins[0] as FakeWindow;
                const sibling = h.wins[2] as FakeWindow;
                const fromFrame = { ...mover.frameGeometry };
                h.deferOutput = true;
                const outputBefore = h.outputRequests.length;
                h.handle.requestSendToOutput("left", true);
                h.runTimers(0);
                await h.flushAll();
                assert.deepEqual(h.outputRequests.slice(outputBefore), [mover]);

                const arrival = h.calls.length;
                mover.output = { name: LEFT };
                h.fireWindow(mover, "outputChanged");
                h.runTimers(0);
                assert.equal(h.calls.length, arrival + 1);
                assert.deepEqual(homingOf(h, arrival, mover.internalId), { out: LEFT, ws: "ws-1" });

                h.fireWindow(sibling, "frameGeometryChanged");
                assert.equal(await h.flush(), arrival);
                assert.ok(h.logs.includes("omnitiler:world:stale reason=superseded-during-flight recovery=catchup"));
                assert.deepEqual(mover.frameGeometry, fromFrame, "the stale reply writes no final frame");
                const catchup = h.calls.length;
                if (intervening === "gesture") h.gesture = true;
                if (intervening === "leaves target") {
                    mover.output = { name: RIGHT };
                    h.fireWindow(mover, "outputChanged");
                }
                h.runTimers(0);
                assert.equal(h.calls.length, catchup + (intervening === "none" ? 1 : 0));

                if (intervening === "none") {
                    assert.deepEqual(homingOf(h, catchup, mover.internalId), { out: LEFT, ws: "ws-1" });
                    h.fireWindow(sibling, "frameGeometryChanged");
                    assert.equal(await h.flush(), catchup);
                    assert.deepEqual(mover.frameGeometry, fromFrame, "the catch-up is fenced too");
                    const after = h.calls.length;
                    h.runTimers(0);
                    assert.equal(h.calls.length, after, "a second stale reply does not expedite again");
                }
                h.gesture = false;
                const ordinary = h.calls.length;
                h.runTimers();
                assert.equal(h.calls.length, ordinary + 1, "ordinary recovery is still available");
                await h.flushAll();
                if (intervening !== "leaves target") {
                    assert.notDeepEqual(mover.frameGeometry, fromFrame, "only a fresh reply writes the final tile");
                }
                assert.deepEqual(h.outputRequests.slice(outputBefore), [mover], "catch-up never resends the crossing command");
            } finally {
                await h.stop();
            }
        });
    }

    it("keeps a qualifying arrival on ordinary debounce during a gesture", async () => {
        const h = await makeHarness();
        try {
            h.currents.set(RIGHT, "ws-1");
            await discover(h);
            const mover = h.wins[0] as FakeWindow;
            h.deferOutput = true;
            const outputBefore = h.outputRequests.length;
            h.handle.requestSendToOutput("left", true);
            h.runTimers(0);
            await h.flushAll();
            assert.deepEqual(h.outputRequests.slice(outputBefore), [mover]);
            const before = h.calls.length;
            const fromFrame = { ...mover.frameGeometry };
            h.gesture = true;
            mover.output = { name: LEFT };
            h.fireWindow(mover, "outputChanged");
            h.runTimers(0);
            assert.equal(h.calls.length, before);
            assert.deepEqual(mover.frameGeometry, fromFrame);
            h.runTimers();
            assert.equal(h.calls.length, before + 1, "ordinary observation remains scheduled");
            await h.flushAll();
            assert.deepEqual(mover.frameGeometry, fromFrame, "gesture still blocks the frame");
        } finally {
            await h.stop();
        }
    });

    it("keeps synchronous commanded crossings on the existing same-apply frame path", async () => {
        const h = await makeHarness();
        try {
            h.currents.set(RIGHT, "ws-1");
            await discover(h);
            const mover = h.wins[0] as FakeWindow;
            const fromFrame = { ...mover.frameGeometry };
            const before = h.calls.length;
            const outputBefore = h.outputRequests.length;
            h.handle.requestSendToOutput("left", true);
            h.runTimers(0);
            await h.flushAll();
            assert.deepEqual(h.outputRequests.slice(outputBefore), [mover]);
            assert.equal(mover.output.name, LEFT);
            assert.notDeepEqual(mover.frameGeometry, fromFrame, "final tile writes in the command apply");
            const afterApply = h.calls.length;
            assert.equal(h.calls.slice(before).filter(c => c.method === "DescribeWorld").length, 1);
            h.fireWindow(mover, "outputChanged");
            h.runTimers(0);
            assert.equal(h.calls.length, afterApply, "synchronous echo and duplicate do not expedite");
        } finally {
            await h.stop();
        }
    });

    it("external output moves both directions plus same-output desktop change converge with no workspace signal", { timeout: 240000 }, async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            h.fire("desktopsChanged");
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "no transfers before external input");
            assert.deepEqual(h.rosterWrites, ["remove:ws-4"], "a fresh unchanged observation does not replay startup compaction");

            // Leg 1: external drag win-a RIGHT(ws-1) -> LEFT. LEFT current
            // is ws-1 so no echo; homing becomes LEFT/ws-1.
            const firesBefore1 = h.workspaceFires.length;
            const callsBefore1 = h.calls.length;
            externalNativeOutputMove(h, "win-a", LEFT);
            assert.equal(h.workspaceFires.length, firesBefore1, "external move fires no workspace signal");
            h.runTimers(0);
            assert.equal(h.calls.length, callsBefore1, "commandless output changes do not expedite");
            // The production debounce alone must queue the report: no
            // explicit workspace signal or timer oracle shortcut.
            h.runTimers();
            assert.ok(h.calls.length > callsBefore1, "per-window homing autonomously dispatches (fail-before: no dispatch without window subscriptions)");
            await h.settle();
            const leg1 = lastReply(h);
            assert.deepEqual(transfersOf(leg1), [], "leg 1 settles with no retained transfers");
            assert.deepEqual(homingOf(h, h.calls.length - 1, "win-a"), { out: LEFT, ws: "ws-1" }, "leg 1 homing converges");

            // Leg 1 empties the first right slot: compaction moves win-b
            // and the right current to ws-1. Select the now-empty right
            // spare before arming the return leg so the native echo must
            // really change win-a's desktop, not merely its output.
            const winB = h.wins.find((w) => w.internalId === "win-b") as FakeWindow;
            assert.deepEqual(winB.desktops.map(d => d.id), ["ws-1"]);
            assert.equal(h.currents.get(RIGHT), "ws-1");
            h.selectDesktop(RIGHT, "ws-2");
            await h.settle();
            assert.equal(h.currents.get(LEFT), "ws-1");
            assert.equal(h.currents.get(RIGHT), "ws-2");

            // Leg 2: external drag win-a back LEFT -> RIGHT, including
            // the pinned source-current to destination-current echo.
            const firesBefore2 = h.workspaceFires.length;
            const callsBefore2 = h.calls.length;
            externalNativeOutputMove(h, "win-a", RIGHT);
            assert.equal(h.workspaceFires.length, firesBefore2, "external move fires no workspace signal");
            h.runTimers();
            assert.ok(h.calls.length > callsBefore2, "return move autonomously dispatches");
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "leg 2 settles with no retained transfers");
            assert.deepEqual(homingOf(h, h.calls.length - 1, "win-a"), { out: RIGHT, ws: "ws-2" }, "leg 2 echo homing converges");

            // Leg 3: win-b now moves RIGHT/ws-1 -> RIGHT/ws-2. Keep the
            // vacated local slot shown so that this leg tests adoption of
            // the desktop change, not a second slot compaction.
            h.selectDesktop(RIGHT, "ws-1");
            await h.settle();
            assert.deepEqual(winB.desktops.map(d => d.id), ["ws-1"]);
            const firesBefore3 = h.workspaceFires.length;
            const callsBefore3 = h.calls.length;
            const ws2 = h.desktops.find(d => d.id === "ws-2");
            assert.ok(ws2);
            winB.desktops = [ws2];
            assert.equal(h.workspaceFires.length, firesBefore3, "same-output change fires no workspace signal");
            h.runTimers();
            assert.ok(h.calls.length > callsBefore3, "same-output desktop change autonomously dispatches");
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "leg 3 settles with no retained transfers");
            assert.deepEqual(homingOf(h, h.calls.length - 1, "win-b"), { out: RIGHT, ws: "ws-2" }, "leg 3 homing converges");

            // Leg 4: own window send still settles exactly once (no
            // loop/replay/stall from the new subscriptions).
            // v2 command lane: presses file QueueWorldCommand; DescribeWorld
            // envelopes carry no inline command.
            h.activeScreen = { name: RIGHT };
            h.activeRef = winB;
            h.selectDesktop(RIGHT, "ws-2");
            h.fire("windowActivated", winB);
            await h.settle();
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
            const outputBefore = h.outputRequests.length;
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
            assert.deepEqual(h.outputRequests.slice(outputBefore), [winB], "own send performs one native output write");
            assert.equal(winB.output.name, LEFT, "own send crosses to the destination output");
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
            h.activeScreen = { name: RIGHT };
            h.activeRef = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            h.selectDesktop(RIGHT, "ws-1");
            h.fire("windowActivated", h.activeRef);
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
            const outputBefore = h.outputRequests.length;
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
            assert.deepEqual(h.outputRequests.slice(outputBefore).map(win => win.internalId), ["win-a"], "migration crosses once without replay");
            assert.deepEqual(transfersOf(lastReply(h)), [], "migration settles with no retained transfers");

            // Attach: a new window arrives with its payload; its later
            // homing signal must dispatch.
            const desks = h.desktops;
            const ws1 = desks.find((d) => d.id === "ws-1") as FakeDesktop;
            const freshOutputChanged = makeSignal();
            const freshDesktopsChanged = makeSignal();
            const freshHiddenChanged = makeSignal();
            const freshFrameChanged = makeSignal();
            const fresh = {
                internalId: "win-new", managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                normalWindow: true,
                frameGeometry: { x: 20, y: 60, width: 300, height: 200 },
                clientGeometry: truthfulClientFor({ x: 20, y: 60, width: 300, height: 200 }),
                output: { name: LEFT },
                desktops: [ws1],
                minimized: false,
                hidden: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
                minSize: { width: 100, height: 100 },
                maxSize: { width: 2147483647, height: 2147483647 },
                pid: 1234,
                resourceClass: "Example",
                caption: "Window win-new",
                windowRole: "normal",
                desktopFileName: "example.app",
                resizeable: true,
                keepAbove: false,
                keepBelow: false,
                outputChanged: freshOutputChanged.signal,
                desktopsChanged: freshDesktopsChanged.signal,
                hiddenChanged: freshHiddenChanged.signal,
                frameGeometryChanged: freshFrameChanged.signal,
                __signals: { outputChanged: freshOutputChanged, desktopsChanged: freshDesktopsChanged, hiddenChanged: freshHiddenChanged, frameGeometryChanged: freshFrameChanged },
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
            for (const handler of [...(fresh.__signals?.hiddenChanged.handlers ?? [])]) {
                handler();
            }
            for (const handler of [...(fresh.__signals?.frameGeometryChanged.handlers ?? [])]) {
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
            for (const handler of [...(winA.__signals?.frameGeometryChanged.handlers ?? [])]) {
                handler();
            }
            h.fire("desktopsChanged");
            h.runTimers();
            assert.equal(h.calls.length, callsAtStop, "no dispatch after stop");
        } finally {
            await h.stop();
        }
    });

    it("hiddenChanged autonomously dispatches a recapture with no workspace signal", { timeout: 240000 }, async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            h.fire("desktopsChanged");
            await h.settle();
            const winA = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            const firesBefore = h.workspaceFires.length;
            const callsBefore = h.calls.length;
            winA.hidden = true;
            h.fireWindow(winA, "hiddenChanged");
            assert.equal(h.workspaceFires.length, firesBefore, "hidden change fires no workspace signal");
            h.runTimers();
            assert.ok(h.calls.length > callsBefore, "hiddenChanged autonomously dispatches");
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "hidden change settles with no retained transfers");
        } finally {
            await h.stop();
        }
    });

    it("external frameGeometryChanged autonomously dispatches a recapture with no workspace signal", { timeout: 240000 }, async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            h.fire("desktopsChanged");
            await h.settle();
            const winA = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            assert.ok(
                (winA.__signals?.frameGeometryChanged.handlers.length ?? 0) > 0,
                "frame homing subscribed (fail-before: no frameGeometryChanged subscription)",
            );
            const firesBefore = h.workspaceFires.length;
            const callsBefore = h.calls.length;
            h.fireWindow(winA, "frameGeometryChanged");
            assert.equal(h.workspaceFires.length, firesBefore, "frame change fires no workspace signal");
            h.runTimers();
            assert.ok(h.calls.length > callsBefore, "frameGeometryChanged autonomously dispatches (fail-before: no dispatch without frame subscription)");
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "frame change settles with no retained transfers");
        } finally {
            await h.stop();
        }
    });

    it("safe synchronous own frame echoes defer to one post-apply observation with all writes", { timeout: 240000 }, async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            h.fire("desktopsChanged");
            await h.settle();
            const winLeft = h.wins.find((w) => w.internalId === "win-left") as FakeWindow;
            const winB = h.wins.find((w) => w.internalId === "win-b") as FakeWindow;
            const settledLeft = { ...winLeft.frameGeometry };
            const settledB = { ...winB.frameGeometry };
            // Production-like synchronous echo: the native setter emits
            // frameGeometryChanged before returning.
            const events: string[] = [];
            const armEcho = (win: FakeWindow): void => {
                let backing = win.frameGeometry;
                Object.defineProperty(win, "frameGeometry", {
                    configurable: true,
                    enumerable: true,
                    get: () => backing,
                    set: (rect: { x: number; y: number; width: number; height: number }) => {
                        backing = rect;
                        win.clientGeometry = truthfulClientFor(rect);
                        events.push(`write:${win.internalId}`);
                        for (const handler of [...(win.__signals?.frameGeometryChanged.handlers ?? [])]) {
                            handler();
                        }
                    },
                });
            };
            armEcho(winLeft);
            armEcho(winB);
            const timers = h.timers as Timer[];
            const origPush = timers.push.bind(timers);
            (timers as unknown as { push: (...items: Timer[]) => number }).push = (...items: Timer[]): number => {
                for (const item of items) {
                    events.push(`timer:${item.delayMs}`);
                }
                return origPush(...items);
            };
            try {
                // Perturb both frames: one dispatch carries both mismatches,
                // so one apply writes both back with synchronous safe echoes.
                const callsBefore = h.calls.length;
                winLeft.frameGeometry = { x: 1, y: 1, width: 10, height: 10 };
                winB.frameGeometry = { x: 2, y: 2, width: 11, height: 11 };
                h.runTimers();
                assert.ok(h.calls.length > callsBefore, "perturbed frames dispatch (fail-before: no frame subscription, no dispatch)");
                events.length = 0;
                await h.flush();
                const writes = events.filter((e) => e.startsWith("write:"));
                const debounce = events.filter((e) => e === `timer:${WORLD_DEBOUNCE_MS}`);
                assert.equal(writes.length, 2, `one apply writes both frames, got ${JSON.stringify(events)}`);
                assert.ok(
                    writes.some((e) => e === "write:win-left") && writes.some((e) => e === "write:win-b"),
                    `both windows written, got ${JSON.stringify(events)}`,
                );
                assert.equal(debounce.length, 1, `safe own echoes coalesce to one post-apply observation, got ${JSON.stringify(events)}`);
                const lastTimer = events.lastIndexOf(`timer:${WORLD_DEBOUNCE_MS}`);
                const lastWrite = Math.max(
                    events.lastIndexOf("write:win-left"),
                    events.lastIndexOf("write:win-b"),
                );
                assert.ok(lastTimer > lastWrite, `safe echoes defer past all writes (fail-before: mid-apply invalidation), got ${JSON.stringify(events)}`);
                assert.deepEqual(winLeft.frameGeometry, settledLeft, "win-left frame restored by the single apply");
                assert.deepEqual(winB.frameGeometry, settledB, "win-b frame restored by the single apply");
            } finally {
                (timers as unknown as { push: (...items: Timer[]) => number }).push = origPush;
            }
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "deferred observation settles with no retained transfers");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")), "no busy loops");
        } finally {
            await h.stop();
        }
    });

    it("unsafe own frame echo invalidates immediately and halts the apply", { timeout: 240000 }, async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            h.fire("desktopsChanged");
            await h.settle();
            const winLeft = h.wins.find((w) => w.internalId === "win-left") as FakeWindow;
            const winB = h.wins.find((w) => w.internalId === "win-b") as FakeWindow;
            const settledLeft = { ...winLeft.frameGeometry };
            const settledB = { ...winB.frameGeometry };
            // Unsafe echo: each setter corrupts live safety (unreadable
            // `hidden` fails readLiveSafety) before emitting, so the
            // suppressed own echo must invalidate immediately instead of
            // deferring. Both windows corrupt so the timer lands between
            // the writes regardless of apply row order.
            const events: string[] = [];
            const armUnsafeEcho = (win: FakeWindow): void => {
                let backing = win.frameGeometry;
                Object.defineProperty(win, "frameGeometry", {
                    configurable: true,
                    enumerable: true,
                    get: () => backing,
                    set: (rect: { x: number; y: number; width: number; height: number }) => {
                        backing = rect;
                        win.clientGeometry = truthfulClientFor(rect);
                        events.push(`write:${win.internalId}`);
                        (win as unknown as Record<string, unknown>)["hidden"] = "corrupt";
                        for (const handler of [...(win.__signals?.frameGeometryChanged.handlers ?? [])]) {
                            handler();
                        }
                    },
                });
            };
            armUnsafeEcho(winLeft);
            armUnsafeEcho(winB);
            const timers = h.timers as Timer[];
            const origPush = timers.push.bind(timers);
            (timers as unknown as { push: (...items: Timer[]) => number }).push = (...items: Timer[]): number => {
                for (const item of items) {
                    events.push(`timer:${item.delayMs}`);
                }
                return origPush(...items);
            };
            try {
                const callsBefore = h.calls.length;
                winLeft.frameGeometry = { x: 1, y: 1, width: 10, height: 10 };
                winB.frameGeometry = { x: 2, y: 2, width: 11, height: 11 };
                (winLeft as unknown as Record<string, unknown>)["hidden"] = false;
                (winB as unknown as Record<string, unknown>)["hidden"] = false;
                h.runTimers();
                assert.ok(h.calls.length > callsBefore, "perturbed frames dispatch (fail-before: no frame subscription, no dispatch)");
                events.length = 0;
                await h.flush();
                (winLeft as unknown as Record<string, unknown>)["hidden"] = false;
                (winB as unknown as Record<string, unknown>)["hidden"] = false;
                const writes = events.filter((e) => e.startsWith("write:"));
                const debounce = events.filter((e) => e === `timer:${WORLD_DEBOUNCE_MS}`);
                // Unsafe echo stops the apply: the first write lands, the
                // immediate invalidation freshens the watermark, and the
                // remaining row waits for the follow-up observation.
                assert.equal(writes.length, 1, `unsafe echo halts the apply after one write, got ${JSON.stringify(events)}`);
                assert.equal(debounce.length, 1, `one immediate invalidation, got ${JSON.stringify(events)}`);
                const timerIdx = events.indexOf(`timer:${WORLD_DEBOUNCE_MS}`);
                assert.equal(timerIdx, events.length - 1, `unsafe echo invalidates immediately with no subsequent in-apply write, got ${JSON.stringify(events)}`);
                assert.ok(timerIdx > 0, `invalidation follows the first write, got ${JSON.stringify(events)}`);
                const writtenId = writes[0] as string;
                const writtenWin = writtenId === "write:win-left" ? winLeft : winB;
                const pendingWin = writtenId === "write:win-left" ? winB : winLeft;
                const settledWritten = writtenId === "write:win-left" ? settledLeft : settledB;
                assert.deepEqual(writtenWin.frameGeometry, settledWritten, "first window frame restored in-apply");
                assert.notDeepEqual(pendingWin.frameGeometry, pendingWin === winLeft ? settledLeft : settledB, "second window waits for the follow-up observation");
            } finally {
                (timers as unknown as { push: (...items: Timer[]) => number }).push = origPush;
            }
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "unsafe-echo observation settles with no retained transfers");
            assert.deepEqual(winLeft.frameGeometry, settledLeft, "win-left frame restored after follow-up");
            assert.deepEqual(winB.frameGeometry, settledB, "win-b frame restored after follow-up");
        } finally {
            await h.stop();
        }
    });
});
