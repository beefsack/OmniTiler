// Producer-shaped whole-workspace migration end to end through
// the production entry against the real broker (via the test-only
// `broker_world_eval` example: the true `WorldService` route serving
// discovery, queue, capture-batch drain, take, and presentation reads
// in one retained process) with a faithful native
// surface shaped like the pinned KWin semantics: two offset unequal
// outputs (out-left 1000x800 at 0,0 plus out-right 900x700 at 1000,50),
// the pinned `sendClientToScreen` echo (`window.cpp`
// `finishInteractiveOutputChange`: a single-desktop window on the old
// output's current synchronously re-homes onto the new output's current),
// and workspace signals only where the pinned `WorkspaceWrapper` emits
// them (view switches via `currentDesktopChanged`; membership and output
// setters emit the subscribed per-window `outputChanged` /
// `desktopsChanged`, which the entry observes; own same-flight echoes
// defer to one post-apply observation instead of invalidating mid-apply).
//
// Baseline: out-left/ws-1 holds win-left, out-right/ws-2 holds win-b,
// and win-a plus fixed-size win-c plus win-m sit on out-right/ws-1. The
// run selects the out-right/ws-1 cell through the production numbered
// selection route (a core view command moves the retained view; poking
// the native current directly would only stage an echo, which never
// retargets the command-authoritative migration source). It migrates
// left, returns through the pinned native echo and partial readback,
// and migrates an empty workspace. Native closes before and after
// adoption distinguish unproved plans from settled mappings.

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";
import { describe, it } from "node:test";

import { WORLD_DEBOUNCE_MS, startWorldAdapterEntry } from "../src/world-entry";
import { parseWorldReply, type WorldReply, type WorldRosterPlan, type WorldRosterSlot } from "../src/world-report";

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
            throw new Error("broker_world_eval stdout unavailable");
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
            this.failAll(new Error(`broker_world_eval exited with code ${String(code)} signal=${String(signal)} stderr=${this.stderrTail.slice(-2000)}`));
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
            spawn("cargo", ["run", "--offline", "-q", "-p", "omnitiler", "--example", "broker_world_eval"], {
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
            return Promise.reject(new Error("broker_world_eval process already exited"));
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


function readyTakeCount(h: Harness): number {
    return h.takeReplies
        .map((raw) => JSON.parse(raw) as Record<string, unknown>)
        .filter((r) => r["outcome"] === "ready").length;
}

async function nextTakeResult(h: Harness, known: number, tag: string): Promise<Record<string, unknown>> {
    // The take lane is async through the real broker (like production
    // D-Bus): after a drain the entry collects the terminal over
    // subsequent yields. Wait boundedly for a NEW ready take beyond
    // `known` and return its result; the op asserts below keep their
    // exact messages.
    for (let round = 0; round < 300; round += 1) {
        h.runTimers();
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
        const readys = h.takeReplies
            .map((raw) => JSON.parse(raw) as Record<string, unknown>)
            .filter((r) => r["outcome"] === "ready");
        if (readys.length > known) {
            return (readys[readys.length - 1] as Record<string, unknown>)["result"] as Record<string, unknown>;
        }
    }
    assert.ok(false, `expected the drained command to settle a new ready take (${tag})`);
    throw new Error("unreachable");
}

interface Harness {
    readonly bridge: WorldBridge;
    readonly takeReplies: string[];
    readonly wins: FakeWindow[];
    readonly desktops: FakeDesktop[];
    readonly screens: Array<{ name: string; geometry: Record<string, number> }>;
    readonly currents: Map<string, string>;
    readonly calls: CapturedCall[];
    readonly answered: Set<number>;
    readonly replies: Map<number, string>;
    readonly timers: Timer[];
    readonly logs: string[];
    readonly membershipWrites: string[];
    readonly echoWrites: string[];
    readonly outputWrites: string[];
    readonly viewWrites: string[];
    readonly unreadableMembership: Set<string>;
    readonly delayedViews: Set<string>;
    readonly pendingViews: Map<string, () => void>;
    readonly frameWrites: string[];
    activeScreen: { name: string };
    activeRef: FakeWindow | null;
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    fire: (name: string, ...args: unknown[]) => void;
    fireWindow: (win: FakeWindow, name: "outputChanged" | "desktopsChanged") => void;
    runTimers: (maxDelay?: number) => void;
    flush: () => Promise<number>;
    flushAll: () => Promise<void>;
    settle: () => Promise<void>;
    stop: () => Promise<void>;
}

const LEFT = "out-left";
const RIGHT = "out-right";
const NATIVES = ["ws-1", "ws-2", "ws-3", "ws-4"];

async function makeProducerHarness(): Promise<Harness> {
    const desktops: FakeDesktop[] = NATIVES.map((id) => ({ id, name: id }));
    const byId = new Map(desktops.map((d) => [d.id, d]));
    const screens = [
        { name: LEFT, geometry: { x: 0, y: 0, width: 1000, height: 800 } },
        { name: RIGHT, geometry: { x: 1000, y: 50, width: 900, height: 700 } },
    ];
    const currents = new Map([[LEFT, "ws-1"], [RIGHT, "ws-2"]]);
    const mkWin = (
        id: string, out: string, ws: string,
        frame: { x: number; y: number; width: number; height: number },
        fixed: { width: number; height: number } | null = null,
    ): FakeWindow => {
        const outputChanged = makeSignal();
        const winDesktopsChanged = makeSignal();
        const winHiddenChanged = makeSignal();
        const winFrameChanged = makeSignal();
        return {
            internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: frame,
            clientGeometry: { x: frame.x + 2, y: frame.y + 21, width: Math.max(1, frame.width - 4), height: Math.max(1, frame.height - 22) },
            output: { name: out },
            desktops: [byId.get(ws) as FakeDesktop],
            minimized: false,
            hidden: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: fixed === null ? { width: 100, height: 100 } : { ...fixed },
            maxSize: fixed === null ? { width: 2147483647, height: 2147483647 } : { ...fixed },
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
        mkWin("win-left", LEFT, "ws-1", { x: 8, y: 52, width: 600, height: 700 }),
        mkWin("win-m", RIGHT, "ws-1", { x: 1008, y: 102, width: 400, height: 300 }),
        mkWin("win-a", RIGHT, "ws-1", { x: 1420, y: 102, width: 400, height: 300 }),
        // win-c seeds tiled with the cell; the fixed-size float intent
        // lands after the seed, converting the seeded member into a float
        // the migration must carry membership-only.
        mkWin("win-c", RIGHT, "ws-1", { x: 1050, y: 150, width: 100, height: 100 }),
        mkWin("win-b", RIGHT, "ws-2", { x: 1008, y: 102, width: 884, height: 600 }),
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
        takeReplies: [],
        wins,
        desktops,
        screens,
        currents,
        calls: [],
        answered: new Set<number>(),
        replies: new Map<number, string>(),
        timers: [],
        logs: [],
        membershipWrites: [],
        echoWrites: [],
        outputWrites: [],
        viewWrites: [],
        unreadableMembership: new Set(),
        delayedViews: new Set(),
        pendingViews: new Map(),
        frameWrites: [],
        activeScreen: { name: RIGHT },
        activeRef: wins[2] as FakeWindow,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        fire: (name: string, ...args: unknown[]): void => {
            for (const handler of [...(signals[name] as FakeSignal).handlers]) {
                handler(...args);
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
            // The broker transport is async (process lines, like D-Bus):
            // a press files its Queue first and the entry dispatches the
            // DescribeWorld only once the filing answer returns. Yield to
            // transport I/O and pump due production timers boundedly so
            // the dispatch lands before giving up.
            let index = -1;
            for (let round = 0; round < 200; round += 1) {
                index = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
                if (index >= 0) {
                    break;
                }
                harness.runTimers();
                await new Promise<void>((resolve) => setTimeout(resolve, 0));
            }
            assert.ok(index >= 0, "expected a queued DescribeWorld dispatch to flush through the production broker");
            // Production broker drain: the envelope runs end to end through
            // the real `WorldService` (one fresh capture incorporates native
            // evidence once and drains ALL parked commands FIFO in core,
            // returning one final full publication with the terminal result
            // nulled for the take lane). No test-side stamping, sequencing,
            // or reply binding: the broker owns all of it.
            const envelope = (harness.calls[index] as CapturedCall).payload;
            const reply = await harness.bridge.send("DescribeWorld", envelope);
            harness.answered.add(index);
            harness.replies.set(index, reply);
            (harness.calls[index] as CapturedCall).callback(reply);
            // Pump to quiescence: post-accept native apply waits for the
            // async per-command take collection (production D-Bus is async
            // the same way), so yield to transport I/O and due timers until
            // no new lane call appears and every take is answered.
            // Follow-up DescribeWorld dispatches stay recorded but unflushed
            // for the caller to drive.
            let calm = 0;
            let lastCalls = -1;
            let lastTakes = -1;
            for (let round = 0; round < 300 && calm < 3; round += 1) {
                harness.runTimers();
                await new Promise<void>((resolve) => setTimeout(resolve, 0));
                const takeCalls = harness.calls.filter((call) => call.method === "TakeCommandResult").length;
                if (
                    harness.calls.length === lastCalls &&
                    harness.takeReplies.length === lastTakes &&
                    takeCalls <= harness.takeReplies.length
                ) {
                    calm += 1;
                } else {
                    calm = 0;
                    lastCalls = harness.calls.length;
                    lastTakes = harness.takeReplies.length;
                }
            }
            return index;
        },
        flushAll: async (): Promise<void> => {
            for (let round = 0; round < 32; round += 1) {
                harness.runTimers();
                await new Promise<void>((resolve) => setTimeout(resolve, 0));
                const next = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
                if (next < 0) {
                    // Quiescence recheck: an async take settling may dispatch
                    // a follow-up observation after one more yield.
                    await new Promise<void>((resolve) => setTimeout(resolve, 0));
                    harness.runTimers();
                    const retry = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
                    if (retry < 0) {
                        return;
                    }
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
            return harness.activeScreen.name === RIGHT ? { x: 1002, y: 100 } : { x: 2, y: 100 };
        },
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        // NOTE: no `activeScreenChanged` (the pinned KWin 6.7
        // `WorkspaceWrapper` exposes none, so the entry never subscribes).
        get activeWindow(): FakeWindow | null {
            return harness.activeRef;
        },
        set activeWindow(ref: unknown) {
            const found = harness.wins.find((win) => (win as unknown) === ref);
            if (found !== undefined && harness.activeRef !== found) {
                harness.activeRef = found;
                // Live `workspace.activeWindow` writes emit activation.
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
        // Void setter; the pinned per-output signal is
        // currentDesktopChanged(previous, current, output).
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
        // Pinned KWin `sendClientToScreen(client, output)` echo
        // (`window.cpp` `finishInteractiveOutputChange` at pin ab7df7cc):
        // moving a single-desktop window on the old output's current onto
        // a new output synchronously re-homes it onto the NEW output's
        // current desktop before any explicit target set. Faithful
        // signal shape: the output step emits the window-level
        // `outputChanged`, the echo re-home emits the window-level
        // `desktopsChanged` (both subscribed per-window by the entry);
        // the pinned `WorkspaceWrapper` exposes no `activeScreenChanged`
        // workspace signal, so workspace signals never fire here. The
        // entry defers its own same-flight echoes to one post-apply
        // observation instead of invalidating mid-apply.
        sendClientToScreen: (mover: unknown, out: unknown): void => {
            const win = mover as FakeWindow;
            const name = (out as { name: string }).name;
            const from = win.output.name;
            win.output = { name };
            harness.outputWrites.push(`${win.internalId}->${name}`);
            harness.fireWindow(win, "outputChanged");
            if (win.desktops.length === 1) {
                const home = (win.desktops[0] as FakeDesktop).id;
                const oldCurrent = harness.currents.get(from) ?? "";
                const newCurrent = harness.currents.get(name) ?? "";
                if (home === oldCurrent && newCurrent !== oldCurrent && home !== newCurrent) {
                    // The desktops setter below emits the window-level
                    // `desktopsChanged` synchronously (faithful).
                    win.desktops = [desktopById(newCurrent)];
                    harness.echoWrites.push(`${win.internalId}->${newCurrent}`);
                }
            }
        },
        // Production void creation: append with an opaque, never-reused ID.
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
        // Truthful native decoration: every native write (entry frame
        // actuation via Reflect.set, never a bypass) keeps the client rect
        // tracking the frame with the same insets the constructor proves,
        // so no capture ever pairs a fresh frame with a stale client.
        let frame = { ...win.frameGeometry };
        let client = { ...win.clientGeometry };
        Object.defineProperty(win, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                harness.frameWrites.push(win.internalId);
                frame = rect;
                client = {
                    x: rect.x + 2,
                    y: rect.y + 21,
                    width: Math.max(1, rect.width - 4),
                    height: Math.max(1, rect.height - 22),
                };
            },
        });
        Object.defineProperty(win, "clientGeometry", {
            configurable: true,
            get: () => client,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                client = rect;
            },
        });
        let desks = win.desktops;
        Object.defineProperty(win, "desktops", {
            configurable: true,
            get: (): FakeDesktop[] => {
                if (harness.unreadableMembership.has(win.internalId)) {
                    throw new Error(`unreadable desktops for ${win.internalId}`);
                }
                return desks;
            },
            set: (refs: FakeDesktop[]) => {
                desks = refs;
                harness.membershipWrites.push(`${win.internalId}->${refs.map((d) => d.id).join("+")}`);
                // Faithful pinned shape: `Window::setDesktops` emits the
                // window-level `desktopsChanged` synchronously. The entry
                // subscribes per-window: external moves invalidate
                // immediately, own same-flight echoes defer to one
                // post-apply observation.
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
            // Queue/take run through the real broker (ordered process
            // lines, so the broker sees Queue before a later DescribeWorld
            // exactly in call order). Replies arrive asynchronously; the
            // entry already treats delayed queue/take answers as honest
            // on-next-event recovery, and a dead bridge leaves pending
            // filings to the entry's own timeouts.
            if (method === "QueueWorldCommand") {
                harness.bridge.send("QueueWorldCommand", payload).then(
                    (queueReply) => callback(queueReply),
                    () => undefined,
                );
            } else if (method === "TakeCommandResult") {
                harness.bridge.send("TakeCommandResult", payload).then(
                    (takeReply) => {
                        harness.takeReplies.push(takeReply);
                        callback(takeReply);
                    },
                    () => undefined,
                );
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
    // The production broker binds the producer adapter lifetime verbatim
    // (never a broker incarnation) and mints the serving generation it
    // stamps on every later queue/capture/take binding: pass through.
    (h.calls[hsIndex] as CapturedCall).callback(hsReplyText);
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

function publication(h: Harness, index = Math.max(...h.replies.keys())): WorldReply {
    const raw = h.replies.get(index);
    assert.ok(raw !== undefined);
    const parsed = parseWorldReply(raw);
    assert.ok(parsed !== null && "transfers" in parsed, `expected genuine world publication: ${raw}`);
    return parsed;
}

function payloadOf(h: Harness, index: number): Record<string, unknown> {
    return JSON.parse((h.calls[index] as CapturedCall).payload) as Record<string, unknown>;
}

function migrateCommands(h: Harness): Array<{ index: number; body: Record<string, unknown> }> {
    // v2 command lane: presses file QueueWorldCommand (the broker drains
    // at most one per capture); DescribeWorld envelopes carry no command.
    return h.calls
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
        })
        .map(({ call, index }) => ({ index, body: JSON.parse(call.payload) as Record<string, unknown> }));
}

function debugState(h: Harness, tag: string): string {
    return `${tag} calls=${h.calls.length} reply=${h.replies.get(Math.max(...h.replies.keys()))} ` +
        `capture=${h.calls[Math.max(...h.replies.keys())]?.payload} ` +
        `currents=${JSON.stringify(Object.fromEntries(h.currents))} ` +
        `wins=${JSON.stringify(h.wins.map((w) => {
            let home = "?";
            try {
                home = `${w.output.name}/${w.desktops.map((d) => d.id).join("+")}`;
            } catch {
                home = "unreadable";
            }
            return [w.internalId, home];
        }))} viewWrites=${JSON.stringify(h.viewWrites.slice(-8))}`;
}

function slotNative(plan: WorldRosterPlan, slot: WorldRosterSlot, out: string, settled: WorldReply): string {
    if (slot.kind === "existing") {
        assert.ok(plan.baseline.order.includes(slot.native));
        return slot.native;
    }
    const index = plan.slots.findIndex((row) => row.kind === "append" && row.step === slot.step);
    assert.ok(index >= 0, "append target references a plan step");
    const logical = plan.projections.find((row) => row.out === out)?.rows[index];
    const native = settled.workspaces.find((row) => row.out === out && row.id === logical)?.bind;
    assert.ok(native && !plan.baseline.order.includes(native), "settlement publishes the appended native ID");
    return native;
}

function shownLogical(h: Harness, out: string): string {
    const logical = publication(h).workspaces.find((row) => row.out === out && row.bind === h.currents.get(out))?.id;
    assert.ok(logical, `shown native on ${out} owns a logical row`);
    return logical;
}

function writeOffsets(h: Harness) {
    return {
        output: h.outputWrites.length, membership: h.membershipWrites.length, view: h.viewWrites.length,
        echo: h.echoWrites.length, frame: h.frameWrites.length,
    };
}

async function startMigration(h: Harness, direction: "left" | "right", logical: string) {
    const [source, target] = direction === "left" ? [RIGHT, LEFT] : [LEFT, RIGHT];
    assert.equal(shownLogical(h, source), logical);
    assert.equal(h.activeScreen.name, source);
    const before = writeOffsets(h);
    const commandCount = migrateCommands(h).length;
    const baseline = h.desktops.map((desktop) => desktop.id);
    const takes = readyTakeCount(h);
    h.handle.requestWorkspaceMigrate(direction);
    const drain = await h.flush();
    const command = migrateCommands(h)[commandCount];
    assert.ok(command && command.index < drain, "command files before the genuine drain");
    assert.equal(migrateCommands(h).length, commandCount + 1);
    assert.equal("result" in JSON.parse(h.replies.get(drain) ?? ""), false, "terminal result stays on the take lane");
    const reply = publication(h, drain);
    const plan = reply.rosterPlan;
    assert.ok(plan, "migration has typed native plan authority");
    assert.deepEqual(plan.baseline.order, baseline);
    const moving = reply.transfers.find((row) => row.kind === "migration");
    assert.ok(moving);
    assert.equal(moving.out, target);
    assert.equal(moving.ws, logical);
    const refill = reply.transfers.find((row) => row.kind === "view" && row.parent === moving.op);
    assert.ok(refill, "explicit paired refill retained");
    assert.equal(refill.out, source);
    assert.notEqual(refill.ws, logical);
    return { before, takes, source, target, logical, refill, plan };
}

async function settleMigration(h: Harness, move: Awaited<ReturnType<typeof startMigration>>, members: readonly string[], floats: number) {
    const { before, source, target, plan, logical, refill } = move;
    await h.settle();
    const settled = publication(h);
    assert.equal(settled.rosterPlan?.op, undefined, debugState(h, "Exact readbacks must adopt the plan"));
    assert.deepEqual(settled.transfers, []);
    const result = await nextTakeResult(h, move.takes, `${source}->${target}`);
    assert.equal(result["op"], "migrate-workspace");
    assert.equal(result["sourceOut"], source);
    assert.equal(result["targetOut"], target);
    assert.equal(result["targetWs"], logical, "the same logical identity rides the move");
    assert.ok(Array.isArray(result["members"]));
    assert.deepEqual([...result["members"]].sort(), [...members].sort());
    assert.equal(result["floats"], floats);
    for (const [out, id] of [[source, refill.ws], [target, logical]] as const) {
        const index = plan.projections.find((row) => row.out === out)?.rows.indexOf(id) ?? -1;
        const slot = plan.slots[index];
        assert.ok(slot, `plan assigns a slot to ${out}/${id}`);
        const native = slotNative(plan, slot, out, settled);
        assert.deepEqual(settled.workspaces.filter((row) => row.id === id).map((row) => [row.out, row.bind]), [[out, native]]);
        assert.equal(h.currents.get(out), native, `${out} shows the assigned native slot`);
        if (out === target) {
            for (const winId of members) {
                const win = h.wins.find((row) => row.internalId === winId);
                assert.ok(win);
                assert.equal(win.output.name, out);
                assert.deepEqual(win.desktops.map((desktop) => desktop.id), [native], `${winId} arrives on the shown slot`);
            }
        }
    }
    const currents = new Map(plan.currents.map((row) => [row.out, row.native]));
    const membership: string[] = [];
    const output: string[] = [];
    const echoes: string[] = [];
    for (const row of plan.members) {
        const native = slotNative(plan, row.target, row.out, settled);
        let from = row.before.native;
        if (row.before.out !== row.out) {
            output.push(`${row.win}->${row.out}`);
            const current = currents.get(row.out);
            if (from === currents.get(row.before.out) && current !== undefined && current !== from) {
                from = current;
                echoes.push(`${row.win}->${from}`);
                membership.push(`${row.win}->${from}`);
            }
        }
        if (from !== native) membership.push(`${row.win}->${native}`);
    }
    assert.deepEqual(h.outputWrites.slice(before.output), output, "one output setter per authorized crossing");
    assert.deepEqual(h.membershipWrites.slice(before.membership), membership, "only authorized membership writes and pinned echoes");
    assert.deepEqual(h.echoWrites.slice(before.echo), echoes, "exact destination-current echoes");
    assert.deepEqual(h.viewWrites.slice(before.view), plan.views.map((row) => `${row.out}->${slotNative(plan, row.target, row.out, settled)}`));
    const order = h.desktops.map((desk) => desk.id);
    assert.equal(order.length, plan.slots.length, "settled shared roster has the planned count");
    for (const [index, slot] of plan.slots.entries()) {
        if (slot.kind === "existing") {
            assert.equal(order[index], slot.native, "settled order preserves the authorized surviving slot");
        } else {
            const projection = plan.projections.find((row) => row.rows[index] !== undefined);
            assert.ok(projection, "append backs a real row");
            assert.equal(order[index], slotNative(plan, slot, projection.out, settled));
        }
    }
    const counts = writeOffsets(h);
    h.fire("desktopsChanged");
    await h.settle();
    assert.deepEqual(h.desktops.map((desk) => desk.id), order, "settled replay cannot mutate the roster");
    assert.deepEqual(writeOffsets(h), counts, "settled observations do not replay native writes");
    return settled;
}

function closeWindow(h: Harness, id: string): void {
    const at = h.wins.findIndex((win) => win.internalId === id);
    assert.ok(at >= 0);
    const [closed] = h.wins.splice(at, 1);
    h.fire("windowRemoved", closed);
}

async function closeAfterAdoption(h: Harness, id: string): Promise<void> {
    closeWindow(h, id);
    await h.settle();
    assert.ok(!publication(h).windows.some((row) => row.win === id), "settled publication acknowledges the close");
    const report = payloadOf(h, Math.max(...h.replies.keys()))["report"] as Record<string, unknown>;
    assert.ok(!(report["windows"] as Array<Record<string, unknown>>).some((row) => row["win"] === id), "capture excludes closed lifetime");
}

async function setupOccupiedSource(h: Harness): Promise<string> {
    await discover(h);
    h.fire("desktopsChanged");
    await h.settle();
    assert.deepEqual(publication(h).transfers, []);
    h.handle.requestWorkspaceSelect(1);
    await h.settle();
    assert.equal(h.calls.filter((call) => call.method === "QueueWorldCommand"
        && JSON.parse(call.payload).command?.op === "select-numbered").length, 1);
    assert.equal(h.currents.get(RIGHT), h.currents.get(LEFT), debugState(h, "Both outputs must show the first slot"));
    return shownLogical(h, RIGHT);
}

describe("world migration producer", () => {
    it("migrates back and forth through the real broker with echo, refill, empty move and settlement", { timeout: 240000 }, async () => {
        const h = await makeProducerHarness();
        try {
            const logical = await setupOccupiedSource(h);
            const winA = h.wins.find((win) => win.internalId === "win-a");
            const winC = h.wins.find((win) => win.internalId === "win-c");
            assert.ok(winA && winC);
            h.activeRef = winC;
            h.handle.requestFloat();
            await h.settle();
            assert.equal(h.calls.filter((call) => call.method === "QueueWorldCommand"
                && JSON.parse(call.payload).command?.op === "toggle-float").length, 1);
            assert.equal(publication(h).windows.find((row) => row.win === "win-c")?.floating, true);
            h.activeRef = winA;
            h.fire("windowActivated", winA);
            await h.settle();
            const floatFrame = { ...winC.frameGeometry };
            const leg1 = await startMigration(h, "left", logical);
            const first = await settleMigration(h, leg1, ["win-a", "win-c", "win-m"], 1);
            assert.deepEqual(h.echoWrites.slice(leg1.before.echo), [], "equal currents cause no echo");
            assert.equal(first.workspaces.find((row) => row.out === LEFT && row.id === logical)?.mode, "tiled");
            assert.equal(first.windows.find((row) => row.win === "win-c")?.floating, true);
            assert.deepEqual(winC.frameGeometry, floatFrame);
            assert.ok(!h.frameWrites.slice(leg1.before.frame).includes("win-c"), "float travels without frame setter");
            assert.equal(h.activeRef?.internalId, winA.internalId, debugState(h, `first migration focus ${JSON.stringify(leg1.plan.focus)}`));
            await closeAfterAdoption(h, "win-m");

            h.activeScreen = { name: RIGHT };
            h.handle.requestWorkspaceSelect(1);
            await h.settle();
            const oldTarget = h.currents.get(RIGHT);
            assert.notEqual(oldTarget, h.currents.get(LEFT), "different currents trigger the pinned echo");
            h.activeScreen = { name: LEFT };
            h.activeRef = winA;
            h.fire("windowActivated", winA);
            await h.settle();
            h.delayedViews.add(RIGHT);
            const leg2 = await startMigration(h, "right", logical);
            await h.flushAll();
            const deliver = h.pendingViews.get(RIGHT);
            assert.ok(deliver, debugState(h, "Return must request a native view change"));
            const step = leg2.plan.members.find((row) => row.win === "win-a");
            assert.ok(step?.target.kind === "existing");
            assert.equal(winA.output.name, RIGHT);
            assert.deepEqual(winA.desktops.map((desk) => desk.id), [step.target.native]);
            assert.equal(h.currents.get(RIGHT), oldTarget, "member arrives before its view follows");
            assert.notEqual(step.target.native, oldTarget);
            assert.ok(h.echoWrites.slice(leg2.before.echo).includes(`win-a->${oldTarget}`), "pinned echo fired");
            const counts = writeOffsets(h);
            h.unreadableMembership.add("win-a");
            h.fire("desktopsChanged");
            const unreadableCapture = await h.flush();
            const unreadable = payloadOf(h, unreadableCapture)["report"] as Record<string, unknown>;
            const unreadableWindows = unreadable["windows"] as Array<Record<string, unknown>>;
            assert.ok(!unreadableWindows.some((row) => row["win"] === "win-a" && typeof row["nativeWs"] === "string"),
                "the delivered observation cannot prove the unreadable membership");
            assert.equal(publication(h).rosterPlan?.op, leg2.plan.op, "unreadable arrival holds");
            assert.ok(publication(h).transfers.some((row) => row.kind === "migration" && row.ws === logical));
            h.unreadableMembership.delete("win-a");
            h.fire("desktopsChanged");
            await h.flushAll();
            assert.equal(publication(h).rosterPlan?.op, leg2.plan.op, "readable arrival alone does not settle a lagging view");
            assert.deepEqual(writeOffsets(h), counts, "partial publications do not replay setters");
            h.pendingViews.delete(RIGHT);
            h.delayedViews.delete(RIGHT);
            deliver();
            const returned = await settleMigration(h, leg2, ["win-a", "win-c"], 1);
            assert.equal(returned.windows.find((row) => row.win === "win-c")?.floating, true);
            assert.deepEqual(winC.frameGeometry, floatFrame);
            assert.ok(!h.frameWrites.slice(leg2.before.frame).includes("win-c"));
            const focusHistory = [...h.replies.keys()].slice(-10).map((index) => {
                const report = payloadOf(h, index)["report"] as Record<string, unknown>;
                const bound = publication(h, index);
                return [bound.rev, report["focused"], report["focusChanges"], report["focusFeedback"],
                    bound.focusRequest?.id, bound.rosterPlan?.focus?.id];
            });
            assert.equal(h.activeRef?.internalId, winA.internalId,
                `return migration focus history ${JSON.stringify(focusHistory)} ` +
                JSON.stringify(h.logs.filter((line) => line.includes("focus-request")).slice(-15)));
            await closeAfterAdoption(h, "win-c");

            h.activeScreen = { name: LEFT };
            // Select the trailing real spare by logical order; the refill
            // policy need not have selected it on the previous leg.
            const spare = publication(h).workspaces.find((row) => row.out === LEFT && row.trailing);
            assert.ok(spare);
            if (h.currents.get(LEFT) !== spare.bind) {
                h.handle.requestWorkspaceSelect(spare.order + 1);
                await h.settle();
            }
            h.activeRef = null;
            h.fire("windowActivated", null);
            await h.settle();
            const empty = h.currents.get(LEFT);
            assert.ok(h.wins.every((win) => win.output.name !== LEFT || win.desktops.every((desk) => desk.id !== empty)));
            const leg3 = await startMigration(h, "right", shownLogical(h, LEFT));
            await settleMigration(h, leg3, [], 0);
            assert.equal(h.outputWrites.length, leg3.before.output, "empty move does not cross a window");
            assert.equal(h.activeRef, null, "empty migration has no focus target");
            assert.equal(migrateCommands(h).length, 3, "three migrations, none refiled");
            assert.ok(!h.logs.some((line) => line.includes("command-busy")));
        } finally {
            await h.stop();
        }
    });

    it("holds the settled source and never replays after a member closes before slot adoption", { timeout: 120000 }, async () => {
        const h = await makeProducerHarness();
        try {
            const logical = await setupOccupiedSource(h);
            const sourceNative = h.currents.get(RIGHT);
            h.delayedViews.add(LEFT);
            const move = await startMigration(h, "left", logical);
            await h.flushAll();
            const deliver = h.pendingViews.get(LEFT);
            assert.ok(deliver, debugState(h, "Follow must be spent but not yet shown"));
            const survivor = h.wins.find((win) => win.internalId === "win-a");
            const step = move.plan.members.find((row) => row.win === "win-a");
            assert.ok(survivor && step?.target.kind === "existing");
            assert.equal(survivor.output.name, LEFT);
            assert.deepEqual(survivor.desktops.map((desk) => desk.id), [step.target.native]);
            const counts = writeOffsets(h);
            const order = h.desktops.map((desk) => desk.id);
            closeWindow(h, "win-m");
            h.pendingViews.delete(LEFT);
            deliver();
            await h.settle();
            const held = publication(h);
            assert.ok(held.rosterPlan);
            assert.equal(held.rosterPlan.op, move.plan.op, "missing lifetime cannot prove the pending epoch");
            assert.deepEqual(held.rosterPlan.baseline, move.plan.baseline, "the retained epoch stays the preflight epoch");
            assert.ok(held.transfers.some((row) => row.kind === "migration" && row.ws === logical));
            const sourceView = held.outputs.find((row) => row.out === RIGHT);
            assert.equal(sourceView?.view, logical, "unproved plan cannot release the original source view");
            assert.notEqual(sourceView?.hold, "", "source view stays held, not a native write grant");
            const index = held.rosterPlan.baseline.order.findIndex((id) => id === sourceNative);
            assert.equal(held.rosterPlan.baseline.projections.find((row) => row.out === RIGHT)?.rows[index], logical,
                "preflight logical identity still belongs to the settled source slot");
            assert.deepEqual(h.desktops.map((desk) => desk.id), order);
            assert.deepEqual(writeOffsets(h), counts, "no replay or dependent writes after close");
            assert.equal(migrateCommands(h).length, 1);
        } finally {
            await h.stop();
        }
    });
});
