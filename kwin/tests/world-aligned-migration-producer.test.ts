// Producer-shaped aligned whole-workspace migration end to end through
// the production entry against the real `WorldEngine` (via the test-only
// `raw_world_eval` example with `WORLD_EVAL_ALIGNED=1`) with a faithful native
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
// retargets the command-authoritative migration source), migrates left
// into fresh backing (closing member win-m mid-flight), verifies refill
// plus logical ride/mode carry, migrates back right (closing member
// win-c mid-flight, exercising the expected echo), then migrates an
// empty workspace (both views must still move and settle), with
// unreadable arrival plus stale view rounds. Every reply on the asserted
// paths is a fresh Engine report. Included in `npm test`.

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
                env: { ...process.env, WORLD_EVAL_ALIGNED: "1" },
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
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    hidden: boolean;
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

interface Faults {
    unreadableMembership: Map<string, number>;
    staleView: Map<string, { show: string; rounds: number }>;
}

function lastTakeResult(h: Harness): Record<string, unknown> {
    const readys = h.takeReplies
        .map((raw) => JSON.parse(raw) as Record<string, unknown>)
        .filter((r) => r["outcome"] === "ready");
    assert.ok(readys.length > 0, "at least one ready take result");
    return (readys[readys.length - 1] as Record<string, unknown>)["result"] as Record<string, unknown>;
}

interface Harness {
    readonly bridge: WorldBridge;
    readonly broker: MiniBroker;
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
    readonly faults: Faults;
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
        return {
            internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: frame,
            output: { name: out },
            desktops: [byId.get(ws) as FakeDesktop],
            minimized: false,
            hidden: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: fixed === null ? null : { ...fixed },
            maxSize: fixed === null ? null : { ...fixed },
            outputChanged: outputChanged.signal,
            desktopsChanged: winDesktopsChanged.signal,
            __signals: { outputChanged, desktopsChanged: winDesktopsChanged },
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
        broker: createMiniBroker(),
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
        faults: { unreadableMembership: new Map(), staleView: new Map() },
        activeScreen: { name: RIGHT },
        activeRef: wins[2] as FakeWindow,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        fire: (name: string, payload?: unknown): void => {
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
                harness.fire("windowActivated");
            }
        },
        clientArea: (_kind: unknown, outRef: unknown, _deskRef: unknown): Record<string, number> => {
            const name = (outRef as { name: string }).name;
            const geom = screenByName(name).geometry;
            return { x: geom["x"] as number, y: geom["y"] as number, width: geom["width"] as number, height: geom["height"] as number };
        },
        currentDesktopForScreen: (screen: { name: string }): FakeDesktop => {
            const stale = harness.faults.staleView.get(screen.name);
            if (stale !== undefined && stale.rounds > 0) {
                stale.rounds -= 1;
                return desktopById(stale.show);
            }
            return desktopById(harness.currents.get(screen.name) ?? "");
        },
        // Production per-output switch signature exactly
        // (`setCurrentDesktopForScreen(desktop, output)`, void at pinned
        // KWin 6.7): synchronous current change plus a synchronous
        // workspace signal.
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }): void => {
            const id = (deskRef as FakeDesktop).id;
            assert.ok(byId.has(id), `desktop ${id} exists`);
            harness.currents.set(screen.name, id);
            harness.viewWrites.push(`${screen.name}->${id}`);
            harness.fire("currentDesktopChanged");
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
            get: (): FakeDesktop[] => {
                const remaining = harness.faults.unreadableMembership.get(win.internalId) ?? 0;
                if (remaining > 0) {
                    harness.faults.unreadableMembership.set(win.internalId, remaining - 1);
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
    const parsed = JSON.parse(reply) as Record<string, unknown>;
    assert.equal(parsed["ready"], false, "bootstrap is not ready");
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

function payloadOf(h: Harness, index: number): Record<string, unknown> {
    return JSON.parse((h.calls[index] as CapturedCall).payload) as Record<string, unknown>;
}

function transfersOf(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return (reply["transfers"] as Array<Record<string, unknown>>) ?? [];
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
    const last = lastReply(h);
    return `${tag} calls=${h.calls.length} transfers=${JSON.stringify(transfersOf(last))} ` +
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

describe("world aligned migration producer", () => {
    it("migrates back and forth through the real Engine with echo, refill, empty move and settlement", { timeout: 240000 }, async () => {
        const h = await makeProducerHarness();
        try {
            await discover(h);
            h.fire("desktopsChanged");
            await h.settle();
            assert.deepEqual(transfersOf(lastReply(h)), [], "no transfers before commands");

            // Select the out-right/ws-1 cell through the production route:
            // numbered selection names the active output explicitly and the
            // core moves the retained view by command. The ring position 1
            // on the right output is the ws-1-native cell (ring order follows
            // native order). A bare native current poke would leave the
            // retained migration source behind, so it is never used here.
            h.activeScreen = { name: RIGHT };
            h.activeRef = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            h.handle.requestWorkspaceSelect(1);
            h.runTimers();
            await h.settle();
            {
                const rows = (lastReply(h)["workspaces"] as Array<Record<string, string>>);
                const shown = rows.find((b) => b["out"] === RIGHT && b["bind"] === "ws-1")?.["id"] ?? "";
                assert.ok(shown !== "", "selection resolves the ws-1 cell to a logical");
                assert.equal(h.currents.get(RIGHT), "ws-1", "native view already on the selected cell");
                const selects = h.calls
                    .map((call, index) => ({ call, index }))
                    .filter(({ call }) => {
                        if (call.method !== "QueueWorldCommand") {
                            return false;
                        }
                        try {
                            const body = JSON.parse(call.payload) as Record<string, unknown>;
                            return (body["command"] as Record<string, unknown> | null)?.["op"] === "select-numbered";
                        } catch {
                            return false;
                        }
                    });
                assert.equal(selects.length, 1, "selection travels the production command route exactly once");
            }

            // The float intent lands after the seed: fixed min/max pins
            // win-c floating (automatic fixed admission), converting the
            // seeded member into a float the migration must carry
            // membership-only.
            const winCSeed = h.wins.find((w) => w.internalId === "win-c") as FakeWindow;
            winCSeed.minSize = { width: 100, height: 100 };
            winCSeed.maxSize = { width: 100, height: 100 };
            h.fire("desktopsChanged");
            await h.settle();

            const settledBefore = lastReply(h);
            const sourceLogical = ((settledBefore["workspaces"] as Array<Record<string, string>>).find((b) => b["out"] === RIGHT && b["bind"] === "ws-1") as Record<string, string> | undefined)?.["id"];
            assert.ok(typeof sourceLogical === "string" && sourceLogical !== "", "source cell has a logical identity");

            // Leg 1: migrate out-right/ws-1 (win-a plus floating win-c plus
            // win-m) left.
            const nativeOrderBeforeLeg1 = h.desktops.map((d) => d.id);
            const winCBefore = JSON.stringify((h.wins.find((w) => w.internalId === "win-c") as FakeWindow).frameGeometry);
            h.handle.requestWorkspaceMigrate("left");
            h.runTimers();
            // Close migrating member win-m mid-flight while the transfer
            // is still retained: the survivors must still migrate, the
            // close must converge without wedging the flight.
            const winM = h.wins.findIndex((w) => w.internalId === "win-m");
            assert.ok(winM >= 0, "win-m present before close");
            h.wins.splice(winM, 1);
            h.fire("windowRemoved");
            await h.flushAll();
            try {
                await h.settle();
            } catch (e) {
                console.log(debugState(h, "LEG1-SETTLE"));
                throw e;
            }
            const leg1cmds = migrateCommands(h);
            assert.equal(leg1cmds.length, 1, "one directional migration command, never replayed");
            // v2: the filed command drains through the next capture; the
            // terminal result rides the take lane (never the capture reply).
            const drainIndex = h.calls.findIndex(
                (call, at) => at > (leg1cmds[0]?.index as number) && call.method === "DescribeWorld",
            );
            assert.ok(drainIndex >= 0, "queued command drains on a fresh capture");
            const leg1reply = JSON.parse(h.replies.get(drainIndex) as string) as Record<string, unknown>;
            assert.equal(leg1reply["result"], null, "actuator lane never carries the terminal result");
            const leg1result = lastTakeResult(h);
            assert.equal(leg1result["op"], "migrate-workspace");
            assert.equal(leg1result["sourceOut"], RIGHT);
            assert.equal(leg1result["targetOut"], LEFT);
            // The stable core logical id rides (selected cell's logical,
            // never a destination-trailing rename).
            const leg1targetWs = leg1result["targetWs"] as string;
            assert.equal(leg1targetWs, sourceLogical, "migrated logical id rides the move");
            assert.deepEqual((leg1result["members"] as string[]).sort(), ["win-a", "win-c", "win-m"]);
            assert.equal(leg1result["floats"], 1, "float member accounted in the carry");
            const leg1transfers = transfersOf(leg1reply);
            const leg1migration = leg1transfers.find((t) => t["kind"] === "migration");
            assert.ok(leg1migration !== undefined, "migration transfer retained");
            assert.equal(leg1migration?.["ws"], sourceLogical, "migration transfer names the riding logical");
            const leg1refill = leg1transfers.find((t) => t["kind"] === "view" && (t["op"] as string).endsWith("-refill"));
            assert.ok(leg1refill !== undefined, "explicit core refill retained");
            assert.equal(leg1refill?.["out"], RIGHT);
            const leg1refillWs = leg1refill?.["ws"] as string;
            assert.ok(leg1refillWs !== "" && leg1refillWs !== sourceLogical, "refill excludes the migrated logical");
            // The migrated logical publishes fresh backing on the target
            // (correlated creation; never a cannibalized trailing cell).
            const settled1early = lastReply(h);
            const leg1backing = ((settled1early["workspaces"] as Array<Record<string, string>>).find((b) => b["out"] === LEFT && b["id"] === sourceLogical)?.["bind"] ?? "");
            assert.ok(leg1backing !== "", "migrated logical publishes backing once created");
            assert.ok(!nativeOrderBeforeLeg1.includes(leg1backing), `backing is a fresh desktop, never a reused pre-existing id (got ${leg1backing})`);

            // Native actuation: both members left the source, both views
            // followed, the close converged without wedging the flight.
            const winA = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            const winC = h.wins.find((w) => w.internalId === "win-c") as FakeWindow;
            assert.equal(winA.output.name, LEFT, "member output moved");
            assert.equal(winC.output.name, LEFT, "float member output moved");
            assert.deepEqual(winA.desktops.map((d) => d.id), [leg1backing]);
            assert.deepEqual(winC.desktops.map((d) => d.id), [leg1backing]);
            assert.equal(h.currents.get(LEFT), leg1backing, "target view follows the created backing");
            const leg1refillNative = ((settled1early["workspaces"] as Array<Record<string, string>>).find((b) => b["out"] === RIGHT && b["id"] === leg1refillWs)?.["bind"] ?? "");
            assert.ok(leg1refillNative !== "", "refill logical stays bound");
            assert.equal(h.currents.get(RIGHT), leg1refillNative, "source refills to the core choice");
            assert.equal(JSON.stringify(winC.frameGeometry), winCBefore, "float member carried membership-only, no frame write");
            assert.ok(!h.wins.some((w) => w.internalId === "win-m"), "closed member stays closed");
            const settledDescribe1 = h.calls.map((call, index) => ({ call, index })).filter(({ call }) => call.method === "DescribeWorld").pop();
            assert.ok(settledDescribe1 !== undefined, "settled capture exists");
            const settledReport1 = (payloadOf(h, settledDescribe1.index)["report"] as Record<string, unknown>);
            const settledWins1 = ((settledReport1["windows"] as Array<Record<string, unknown>>).map((w) => w["win"]));
            assert.ok(!settledWins1.includes("win-m"), "closed member acknowledged out of the converged roster");
            assert.deepEqual(h.echoWrites, [], "leg 1 needs no echo: the window already sits on the destination current");
            const settled1 = lastReply(h);
            assert.deepEqual(transfersOf(settled1), [], "leg 1 settles with no retained transfers");
            const workspaces1 = settled1["workspaces"] as Array<Record<string, string>>;
            // The core-published binding is the adapter's live snapshot
            // source: the riding logical binds the created backing on the
            // target output (never a guessed ordinal).
            const moved1 = workspaces1.find((b) => b["out"] === LEFT && b["id"] === sourceLogical);
            assert.equal(moved1?.["bind"], leg1backing, "logical identity rides the move on fresh backing");
            // The emptied source native cell carries no fresh logical (the
            // mapping stays dormant; no rename, no invented binding): the
            // migrated logical lives exactly once, on the target.
            assert.deepEqual(
                workspaces1.filter((w) => w["id"] === sourceLogical).map((w) => w["out"]),
                [LEFT],
                "migrated logical lives exactly once, on the target",
            );
            for (const row of workspaces1.filter((w) => w["out"] === LEFT && w["bind"] === leg1backing)) {
                assert.equal(row["mode"], "tiled", "moved unit keeps its tiling mode");
            }

            // Leg 2: migrate back right. The pinned echo fires here when the
            // source current differs from the destination current: the
            // output step re-homes onto the destination current first.
            h.activeScreen = { name: LEFT };
            h.activeRef = winA;
            const nativeOrderBeforeLeg2 = h.desktops.map((d) => d.id);
            h.handle.requestWorkspaceMigrate("right");
            h.runTimers();
            // One unreadable arrival round holds truthfully, then converges.
            h.faults.unreadableMembership.set("win-a", 4);
            await h.flush();
            assert.ok(transfersOf(lastReply(h)).length > 0, "unreadable arrival retains the transfer");
            h.faults.unreadableMembership.delete("win-a");
            // One stale destination-view round holds, then converges.
            const staleTarget = h.currents.get(RIGHT) ?? "";
            h.faults.staleView.set(RIGHT, { show: staleTarget, rounds: 4 });
            h.runTimers();
            await h.flushAll();
            h.faults.staleView.delete(RIGHT);
            // Close migrating member win-c mid-flight while the transfer
            // is still retained (target view unproven): survivor win-a must
            // still settle with both views, the close must converge.
            const winCClose = h.wins.findIndex((w) => w.internalId === "win-c");
            assert.ok(winCClose >= 0, "win-c present before close");
            h.wins.splice(winCClose, 1);
            h.fire("windowRemoved");
            try {
                await h.settle();
            } catch (e) {
                console.log(debugState(h, "LEG2-SETTLE"));
                throw e;
            }
            const leg2cmds = migrateCommands(h).slice(1);
            assert.equal(leg2cmds.length, 1, "return migration dispatches exactly once (never refused-looped)");
            const leg2drain = h.calls.findIndex(
                (call, at) => at > (leg2cmds[0]?.index as number) && call.method === "DescribeWorld",
            );
            assert.ok(leg2drain >= 0, "return command drains on a fresh capture");
            const leg2reply = JSON.parse(h.replies.get(leg2drain) as string) as Record<string, unknown>;
            assert.equal(leg2reply["result"], null, "actuator lane never carries the terminal result");
            const leg2result = lastTakeResult(h);
            assert.equal(leg2result["op"], "migrate-workspace");
            assert.equal(leg2result["sourceOut"], LEFT);
            assert.equal(leg2result["targetOut"], RIGHT);
            // The same stable logical rides back (never renamed across legs).
            const leg2targetWs = leg2result["targetWs"] as string;
            assert.equal(leg2targetWs, sourceLogical, "migrated logical id rides back");
            const leg2refill = transfersOf(leg2reply).find((t) => t["kind"] === "view" && (t["op"] as string).endsWith("-refill"));
            assert.ok(leg2refill !== undefined, "return refill retained");
            const leg2refillWs = leg2refill?.["ws"] as string;
            assert.ok(leg2refillWs !== "" && leg2refillWs !== sourceLogical, "return refill excludes the riding logical");
            const settled2early = lastReply(h);
            // Released-source return (option 1): leg 1 released the
            // (RIGHT, ws-1) pair and adopted it as a distinct fresh
            // source row, so the returning logical cannot rebind its old
            // cell - another logical workspace's empty desktop is never
            // eligible reuse. The return publishes fresh backing through
            // correlated creation alongside the refill, which keeps its
            // own bound row on the source.
            const leg2backing = ((settled2early["workspaces"] as Array<Record<string, string>>).find((b) => b["out"] === RIGHT && b["id"] === sourceLogical)?.["bind"] ?? "");
            assert.ok(leg2backing !== "", "returned logical publishes fresh backing once created");
            assert.ok(!nativeOrderBeforeLeg2.includes(leg2backing), `return backing is a fresh desktop, never a reused pre-existing id (got ${leg2backing})`);
            const releasedCell = ((settled2early["workspaces"] as Array<Record<string, string>>).find((b) => b["out"] === RIGHT && b["bind"] === "ws-1"));
            assert.ok(
                releasedCell !== undefined && (releasedCell["id"] as string) !== sourceLogical,
                `released ws-1 cell still belongs to its distinct adopted source row, never retaken: ${JSON.stringify(settled2early["workspaces"])}`,
            );
            const leg2refillNative = ((settled2early["workspaces"] as Array<Record<string, string>>).find((b) => b["out"] === LEFT && b["id"] === leg2refillWs)?.["bind"] ?? "");
            assert.ok(leg2refillNative !== "", "return refill logical stays bound");
            assert.equal(winA.output.name, RIGHT, "returned member output moved");
            assert.deepEqual(winA.desktops.map((d) => d.id), [leg2backing]);
            assert.ok(
                h.echoWrites.some((e: string) => e.startsWith("win-a->") || e.startsWith("win-c->")),
                `pinned output-send echo fired mid-flight and was tolerated: ${JSON.stringify(h.echoWrites)}`,
            );
            assert.equal(h.currents.get(RIGHT), leg2backing, "return target view follows the created backing");
            assert.equal(h.currents.get(LEFT), leg2refillNative, "return source refills to the core choice");
            assert.ok(!h.wins.some((w) => w.internalId === "win-c"), "closed member stays closed");
            const settledDescribe2 = h.calls.map((call, index) => ({ call, index })).filter(({ call }) => call.method === "DescribeWorld").pop();
            assert.ok(settledDescribe2 !== undefined, "settled capture exists");
            const settledReport2 = (payloadOf(h, settledDescribe2.index)["report"] as Record<string, unknown>);
            const settledWins2 = ((settledReport2["windows"] as Array<Record<string, unknown>>).map((w) => w["win"]));
            assert.ok(!settledWins2.includes("win-c"), "closed member acknowledged out of the converged roster");
            const settled2 = lastReply(h);
            assert.deepEqual(transfersOf(settled2), [], "leg 2 settles with no retained transfers");
            const workspaces2 = settled2["workspaces"] as Array<Record<string, string>>;
            const moved2 = workspaces2.find((b) => b["out"] === RIGHT && b["id"] === sourceLogical);
            assert.equal(moved2?.["bind"], leg2backing, "logical identity rides back on fresh backing");

            // Leg 3: migrate an empty workspace. No members move, but both
            // views must still follow the core refill and settle. The
            // engine carries a named focus only inside the source cell, so
            // an empty source migrates with empty focus (no active window,
            // as after the last window on the output closed).
            h.activeScreen = { name: LEFT };
            h.activeRef = null;
            h.fire("windowActivated");
            await h.settle();
            const emptySrc = h.currents.get(LEFT) ?? "";
            const occupants = h.wins.filter((w) => {
                try {
                    return w.output.name === LEFT && w.desktops.some((d) => d.id === emptySrc);
                } catch {
                    return false;
                }
            });
            assert.deepEqual(occupants.map((w) => w.internalId), [], `left current ${emptySrc} is empty before the empty migration`);
            // The empty source cell's logical rides too (resolved from the
            // pre-migration settled rows, never a trailing constant).
            const emptyLogicalBefore = ((lastReply(h)["workspaces"] as Array<Record<string, string>>).find((b) => b["out"] === LEFT && b["bind"] === emptySrc)?.["id"] ?? "");
            assert.ok(emptyLogicalBefore !== "", "empty source cell has a logical identity");
            const nativeOrderBeforeLeg3 = h.desktops.map((d) => d.id);
            h.handle.requestWorkspaceMigrate("right");
            try {
                await h.settle();
            } catch (e) {
                console.log(debugState(h, "LEG3-SETTLE"));
                throw e;
            }
            const leg3cmds = migrateCommands(h).slice(2);
            assert.equal(leg3cmds.length, 1, "empty migration dispatches exactly once");
            const leg3drain = h.calls.findIndex(
                (call, at) => at > (leg3cmds[0]?.index as number) && call.method === "DescribeWorld",
            );
            assert.ok(leg3drain >= 0, "empty command drains on a fresh capture");
            const leg3reply = JSON.parse(h.replies.get(leg3drain) as string) as Record<string, unknown>;
            assert.equal(leg3reply["result"], null, "actuator lane never carries the terminal result");
            const leg3result = lastTakeResult(h);
            assert.equal(leg3result["op"] ?? null, "migrate-workspace", "empty migration commits");
            assert.deepEqual(leg3result["members"] as string[], [], "empty migration carries no members");
            assert.equal(leg3result["sourceOut"], LEFT);
            assert.equal(leg3result["targetOut"], RIGHT);
            const leg3targetWs = leg3result["targetWs"] as string;
            assert.equal(leg3targetWs, emptyLogicalBefore, "empty source logical rides the move");
            const leg3refill = transfersOf(leg3reply).find((t) => t["kind"] === "view" && (t["op"] as string).endsWith("-refill"));
            assert.ok(leg3refill !== undefined, "empty migration retains the explicit refill");
            const leg3refillWs = leg3refill?.["ws"] as string;
            assert.ok(leg3refillWs !== "" && leg3refillWs !== leg3targetWs, "empty refill excludes the riding logical");
            const settled3 = lastReply(h);
            const leg3backing = ((settled3["workspaces"] as Array<Record<string, string>>).find((b) => b["out"] === RIGHT && b["id"] === leg3targetWs)?.["bind"] ?? "");
            assert.ok(leg3backing !== "", "empty migrated logical publishes backing once created");
            assert.ok(!nativeOrderBeforeLeg3.includes(leg3backing), `empty backing is a fresh desktop (got ${leg3backing})`);
            const leg3refillNative = ((settled3["workspaces"] as Array<Record<string, string>>).find((b) => b["out"] === LEFT && b["id"] === leg3refillWs)?.["bind"] ?? "");
            assert.ok(leg3refillNative !== "", "empty refill logical stays bound");
            assert.equal(h.currents.get(RIGHT), leg3backing, "empty target view follows the created backing");
            assert.equal(h.currents.get(LEFT), leg3refillNative, "empty source refills to the core choice");
            assert.deepEqual(transfersOf(settled3), [], "empty migration settles both views with no retained transfers");

            assert.equal(migrateCommands(h).length, 3, "three migration commands total, none replayed");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")), "no busy refills");
        } finally {
            await h.stop();
        }
    });
});
