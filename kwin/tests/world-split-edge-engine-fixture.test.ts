// Split-edge selection through the WORLD production route: the real
// `WorldEngine` (via the test-only `raw_world_eval` example: same decode,
// retained engine, codec, and generation binding the shipped world service
// uses; only D-Bus and signals are stubbed) behind the production
// `startWorldAdapterEntry` (direction-only commands, full DiscoverWorld /
// DescribeWorld transport, native actuation). No adapter-side target
// selection anywhere: move/send resolve the mover-ranked FULL candidate,
// migration resolves the largest shared FULL edge independent of the mover,
// focus keeps the strict unique-reciprocal rule and stays local on the
// split edge. Deferred native relocation proves pinned arrival with exactly
// one follow and no command replay. Included in `npm test`.
//
// REAL parts: production `startWorldAdapterEntry` on a scripted three-output
// fake KWin surface (top full-width over an unequal bottom split, so the
// non-left/top candidate also owns the largest shared edge); retained
// `WorldEngine` evaluation per DescribeWorld round-trip. The per-test
// `raw_world_eval` process is the fixture's replay boundary: every command leg
// flushes the entry's dispatches through it in order.

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

interface FakeScreen {
    readonly name: string;
    geometry: { x: number; y: number; width: number; height: number };
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
    output: FakeScreen;
    desktops: FakeDesktop[];
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minSize: null;
    maxSize: null;
}

interface CapturedCall {
    readonly method: string;
    readonly payload: string;
    readonly callback: (reply: unknown) => void;
}

interface Harness {
    readonly bridge: WorldBridge;
    readonly broker: MiniBroker;
    workspace: Record<string, unknown>;
    readonly screens: { top: FakeScreen; bottom: FakeScreen; bottomRight: FakeScreen };
    readonly desktops: { a: FakeDesktop; b: FakeDesktop; c: FakeDesktop };
    readonly wins: FakeWindow[];
    readonly mover: FakeWindow;
    readonly currentByOutput: Map<string, FakeDesktop>;
    readonly calls: CapturedCall[];
    readonly answered: Set<number>;
    readonly replies: Map<number, string>;
    readonly takeReplies: string[];
    readonly drainCapture: Map<unknown, number>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean; fired: boolean }>;
    readonly logs: string[];
    readonly geometryWrites: string[];
    readonly membershipWrites: string[];
    readonly outputWrites: string[];
    readonly viewSwitches: string[];
    readonly focused: string[];
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    readonly signals: Record<string, FakeSignal>;
    deferredTransfer: boolean;
    pendingTransfers: Array<{ client: FakeWindow; output: FakeScreen }>;
    fire: (name: string, payload?: unknown) => void;
    runTimers: (maxDelay?: number) => void;
    flush: () => Promise<number>;
    flushAll: () => Promise<void>;
    settle: () => Promise<void>;
    settleRounds: (rounds: number) => Promise<void>;
    commandPayloads: (op: string) => Array<{ index: number; body: Record<string, unknown> }>;
    stop: () => Promise<void>;
}

async function makeHarness(moverX: number, soloTop = false): Promise<Harness> {
    // Unequal bottom split: out-bottom shares a 640 edge with out-top,
    // out-bottom-right shares 1280, so migration (largest edge) and the
    // right-of-centre mover (centre projection) agree on the non-left/top
    // target for independent reasons.
    const top: FakeScreen = { name: "out-top", geometry: { x: 0, y: 0, width: 1920, height: 540 } };
    const bottom: FakeScreen = { name: "out-bottom", geometry: { x: 0, y: 540, width: 640, height: 540 } };
    const bottomRight: FakeScreen = { name: "out-bottom-right", geometry: { x: 640, y: 540, width: 1280, height: 540 } };
    const wsA: FakeDesktop = { id: "ws-a", name: "WS A" };
    const wsB: FakeDesktop = { id: "ws-b", name: "WS B" };
    const wsC: FakeDesktop = { id: "ws-c", name: "WS C" };
    const liveDesktops: FakeDesktop[] = [wsA, wsB, wsC];
    const removed: string[] = [];
    const currentByOutput = new Map<string, FakeDesktop>([
        ["out-top", wsA],
        ["out-bottom", wsB],
        ["out-bottom-right", wsC],
    ]);
    const mkWin = (
        output: FakeScreen,
        desktop: FakeDesktop,
        internalId: string,
        frame: { x: number; y: number; width: number; height: number },
    ): FakeWindow => ({
        internalId,
        managed: true,
        deleted: false,
        transient: false,
        transientFor: null,
        dialog: false,
        modal: false,
        wantsInput: true,
        normalWindow: true,
        frameGeometry: { ...frame },
        output,
        desktops: [desktop],
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
    });
    const wins: FakeWindow[] = [
        mkWin(top, wsA, "w-move", { x: moverX, y: 20, width: 200, height: 100 }),
        ...(soloTop
            ? []
            : [mkWin(top, wsA, "w-stay", { x: 20, y: 300, width: 400, height: 200 })]),
        mkWin(bottom, wsB, "w-bl", { x: 20, y: 600, width: 400, height: 300 }),
        mkWin(bottomRight, wsC, "w-br", { x: 700, y: 600, width: 400, height: 300 }),
    ];
    const mover = wins.find((win) => win.internalId === "w-move") as FakeWindow;
    const geometryWrites: string[] = [];
    const membershipWrites: string[] = [];
    const outputWrites: string[] = [];
    const viewSwitches: string[] = [];
    const focused: string[] = [];
    const sigNames = [
        "windowAdded",
        "windowRemoved",
        "windowActivated",
        "desktopsChanged",
        "screensChanged",
        "currentDesktopChanged",
        "activeScreenChanged",
    ] as const;
    const signals: Record<string, FakeSignal> = {};
    for (const name of sigNames) {
        signals[name] = makeSignal();
    }
    const harness: Harness = {
        bridge: WorldBridge.start(),
        broker: createMiniBroker(),
        workspace: {},
        screens: { top, bottom, bottomRight },
        desktops: { a: wsA, b: wsB, c: wsC },
        wins,
        mover,
        currentByOutput,
        calls: [],
        answered: new Set<number>(),
        replies: new Map<number, string>(),
        takeReplies: [],
        drainCapture: new Map<unknown, number>(),
        timers: [],
        logs: [],
        geometryWrites,
        membershipWrites,
        outputWrites,
        viewSwitches,
        focused,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        signals,
        deferredTransfer: false,
        pendingTransfers: [],
        fire: (name: string, payload?: unknown): void => {
            for (const handler of [...(signals[name] as FakeSignal).handlers]) {
                handler(payload);
            }
        },
        runTimers: (maxDelay = WORLD_DEBOUNCE_MS): void => {
            for (let i = 0; i < harness.timers.length; i += 1) {
                const timer = harness.timers[i] as { delayMs: number; callback: () => void; cancelled: boolean; fired: boolean };
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
            const drainedBefore = harness.broker.drained.length;
            const stamped = harness.broker.stampCapture(envelope);
            if (harness.broker.drained.length > drainedBefore) {
                const drained = harness.broker.drained[harness.broker.drained.length - 1] as { clientCmd: unknown };
                harness.drainCapture.set(drained.clientCmd, index);
            }
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
        settleRounds: async (rounds: number): Promise<void> => {
            for (let round = 0; round < rounds; round += 1) {
                harness.runTimers();
                await harness.flushAll();
            }
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
        commandPayloads: (op: string): Array<{ index: number; body: Record<string, unknown> }> => {
            // v2 command lane: presses file QueueWorldCommand (the broker
            // drains at most one per capture); DescribeWorld envelopes
            // carry no inline command.
            const out: Array<{ index: number; body: Record<string, unknown> }> = [];
            harness.calls.forEach((call, index) => {
                if (call.method !== "QueueWorldCommand") {
                    return;
                }
                try {
                    const body = JSON.parse(call.payload) as Record<string, unknown>;
                    if ((body["command"] as Record<string, unknown> | null)?.["op"] === op) {
                        out.push({ index, body });
                    }
                } catch {
                    return;
                }
            });
            return out;
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
    const surface: Record<string, unknown> = {
        windowList: (): FakeWindow[] => [...harness.wins],
        screens: [top, bottom, bottomRight],
        desktops: liveDesktops,
        activeScreen: top,
        cursorPos: { x: 1200, y: 100 },
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        activeScreenChanged: (signals["activeScreenChanged"] as FakeSignal).signal,
        get activeWindow(): FakeWindow | null {
            return activeRef;
        },
        set activeWindow(ref: unknown) {
            const found = harness.wins.find((win) => (win as unknown) === ref);
            if (found !== undefined) {
                activeRef = found;
                harness.focused.push(found.internalId);
            }
        },
        get currentDesktop(): FakeDesktop {
            return wsA;
        },
        set currentDesktop(ref: unknown) {
            void ref;
        },
        clientArea: (_kind: number, output: unknown, _desktop: unknown): Record<string, number> => {
            const name = (output as FakeScreen).name;
            if (name === "out-top") {
                return { x: 0, y: 0, w: 1920, h: 500 };
            }
            if (name === "out-bottom") {
                return { x: 0, y: 580, w: 640, h: 500 };
            }
            return { x: 640, y: 580, w: 1280, h: 500 };
        },
        currentDesktopForScreen: (output: unknown): FakeDesktop => {
            const name = (output as FakeScreen).name;
            return harness.currentByOutput.get(name) ?? wsA;
        },
        setCurrentDesktopForScreen: (desktop: unknown, output: unknown): void => {
            const name = (output as FakeScreen).name;
            harness.currentByOutput.set(name, desktop as FakeDesktop);
            harness.viewSwitches.push(`${name}->${(desktop as FakeDesktop).id}`);
        },
        sendClientToScreen: (client: unknown, output: unknown): void => {
            const win = client as FakeWindow;
            if (harness.deferredTransfer) {
                harness.pendingTransfers.push({ client: win, output: output as FakeScreen });
                return;
            }
            win.output = output as FakeScreen;
            harness.outputWrites.push(`${win.internalId}=>${(output as FakeScreen).name}`);
        },
        // Production void creation: synchronously append one native
        // desktop carrying the logical token as its name. Explicit core
        // desires alone authorize creation; same-turn enumeration proves
        // the created id causally. Fresh backing versus existing-desktop
        // reuse remains the retained migration oracle's open distinction.
        createDesktop: (...args: ReadonlyArray<unknown>): void => {
            if (args.length !== 2 || typeof args[0] !== "number" || typeof args[1] !== "string" || (args[1] as string) === "") {
                throw new Error("Insufficient arguments");
            }
            const id = `native-${liveDesktops.length + 1}`;
            const at = Math.max(0, Math.min(args[0] as number, liveDesktops.length));
            liveDesktops.splice(at, 0, { id, name: args[1] as string });
            harness.fire("desktopsChanged");
        },
        removeDesktop: (ref: unknown): void => {
            const at = liveDesktops.indexOf(ref as FakeDesktop);
            if (at >= 0) {
                removed.push(liveDesktops[at]?.id as string);
                liveDesktops.splice(at, 1);
            }
        },
    };
    let activeRef: FakeWindow | null = mover;
    // Instrument native setters through live refs.
    for (const win of wins) {
        let frame = win.frameGeometry;
        Object.defineProperty(win, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                harness.geometryWrites.push(`${win.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
        let desks = win.desktops;
        Object.defineProperty(win, "desktops", {
            configurable: true,
            get: () => desks,
            set: (refs: FakeDesktop[]) => {
                desks = refs;
                harness.membershipWrites.push(`${win.internalId}->${refs.map((desk) => desk.id).join("+")}`);
                return true;
            },
        });
    }
    harness.workspace = surface;
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
            const timer = { delayMs, callback, cancelled: false, fired: false };
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
    assert.ok(typeof parsed["gen"] === "string" && (parsed["gen"] as string).length > 0, "bootstrap binds a generation");
    disc.callback(reply);
    // Settle the discovery-triggered observation report so every leg below
    // starts from an idle single-flight with retained FULL topology.
    await h.settle();
    assert.equal(h.commandPayloads("move-directional").length, 0);
    assert.equal(h.commandPayloads("send-directional").length, 0);
}

function lastTakeResult(h: Harness): Record<string, unknown> {
    const readys = h.takeReplies
        .map((raw) => JSON.parse(raw) as Record<string, unknown>)
        .filter((r) => r["outcome"] === "ready");
    assert.ok(readys.length > 0, "at least one ready take result");
    return (readys[readys.length - 1] as Record<string, unknown>)["result"] as Record<string, unknown>;
}

function frameOf(win: FakeWindow): { x: number; y: number; w: number; h: number } {
    const frame = win.frameGeometry as { x: number; y: number; width: number; height: number };
    return { x: frame.x, y: frame.y, w: frame.width, h: frame.height };
}

function winById(h: Harness, id: string): FakeWindow {
    const found = h.wins.find((win) => win.internalId === id);
    assert.ok(found !== undefined, `window ${id} exists on the fake surface`);
    return found as FakeWindow;
}

// v2: reply workspace `id` is the core logical; producer natives resolve
// through the core-owned binding (`bind`), never by string equality.
function boundNativeOf(reply: Record<string, unknown>, out: string, logical: string): string | null {
    const rows = reply["workspaces"] as Array<Record<string, unknown>>;
    const row = rows.find((entry) => entry["out"] === out && entry["id"] === logical);
    assert.ok(row !== undefined, `Engine plans a workspace row for ${out}/${logical}`);
    const bind = row["bind"] as string;
    return bind === "" ? null : bind;
}

// Desired window rows carry `frame` as [x,y,w,h] plus homing: every row
// the real Engine names must be natively present. Membership resolves
// through the core-owned native binding (v2: reply `ws` is the logical
// id, producer natives never equal it by construction).
function appliedMatchesReply(h: Harness, reply: Record<string, unknown>): void {
    const geometry = reply["windows"] as Array<Record<string, unknown>>;
    assert.ok(Array.isArray(geometry) && geometry.length > 0, "real reply carries desired window rows");
    const rows = reply["workspaces"] as Array<Record<string, unknown>>;
    assert.ok(Array.isArray(rows), "real reply carries workspace rows");
    for (const entry of geometry) {
        const frame = entry["frame"] as [number, number, number, number] | null;
        const win = winById(h, entry["win"] as string);
        if (frame !== null) {
            assert.deepEqual(frameOf(win), { x: frame[0], y: frame[1], w: frame[2], h: frame[3] }, `native frame of ${entry["win"]} matches the Engine plan`);
        }
        assert.equal(win.output.name, entry["out"], `native output of ${entry["win"]} matches the Engine plan`);
        const planned = rows.find((row) => row["out"] === entry["out"] && row["id"] === entry["ws"]);
        assert.ok(planned !== undefined, `Engine plans a workspace row for ${entry["win"]} at ${entry["out"]}/${entry["ws"]}`);
        assert.deepEqual(
            win.desktops.map((desk) => desk.id),
            [planned["bind"]],
            `native membership of ${entry["win"]} matches the Engine plan through the core binding`,
        );
    }
}

describe("world split-edge selection through the production entry (real Engine)", () => {
    it("moves down to the mover-selected non-left/top output", async () => {
        // Sole source leaf: a second local tile would let local R1 win, so
        // the cross leg holds only the mover. Mover centre x=1200 falls in
        // out-bottom-right's shared edge [640,1920): the direction-only move
        // resolves it with no adapter endpoint anywhere on the wire.
        const h = await makeHarness(1100, true);
        try {
            await discover(h);
            const leftBefore = { ...winById(h, "w-bl").frameGeometry };
            h.handle.requestMove("down");
            await h.settle();
            const moves = h.commandPayloads("move-directional");
            assert.equal(moves.length, 1, "exactly one direction-only move dispatch, never replayed");
            const cmd = (moves[0] as { body: Record<string, unknown> }).body["command"] as Record<string, unknown>;
            assert.equal(cmd["op"], "move-directional");
            assert.deepEqual(Object.keys(cmd).sort(), ["direction", "op", "sameAxis", "win"], "command carries no endpoint or cross flag");
            // v2: the filed move drains through a capture; the draining
            // capture's bound reply carries the plan.
            const captureIndex = h.drainCapture.get((moves[0] as { body: Record<string, unknown> }).body["clientCmd"]);
            assert.ok(captureIndex !== undefined, "the core drained the move");
            const reply = JSON.parse(h.replies.get(captureIndex as number) as string) as Record<string, unknown>;
            assert.notEqual(reply["outcome"], "rejected", `real Engine plans the cross, got ${JSON.stringify(reply).slice(0, 160)}`);
            // The real Engine selects the mover-ranked target and retains
            // one identity-pinned window transfer (existing send
            // contract): the adapter relocates output + membership natively
            // with the foreign frame deferred until arrival proof, then
            // converges geometry on later rounds. Focus stays on the mover
            // with no redundant setter.
            const rows = reply["windows"] as Array<Record<string, unknown>>;
            const planned = rows.find((entry) => entry["win"] === "w-move");
            assert.ok(planned !== undefined, "Engine plans the crossing mover");
            assert.equal(planned["out"], "out-bottom-right", "selection resolves the non-left/top target");
            assert.equal(boundNativeOf(reply, planned["out"] as string, planned["ws"] as string), "ws-c", "selection lands in the target view");
            const transfers = reply["transfers"] as Array<Record<string, unknown>>;
            assert.equal(transfers.length, 1, "exactly one retained transfer");
            assert.equal(transfers[0]?.["kind"], "window");
            assert.equal(transfers[0]?.["win"], "w-move");
            assert.equal(transfers[0]?.["out"], "out-bottom-right");
            assert.equal(boundNativeOf(reply, transfers[0]?.["out"] as string, transfers[0]?.["ws"] as string), "ws-c", "transfer targets the ws-c cell");
            assert.equal(transfers[0]?.["follow"], true, "move follows the mover to the target view");
            assert.match(transfers[0]?.["op"] as string, /^op-\d+$/, "stable command identity");
            appliedMatchesReply(h, reply);
            assert.equal(h.mover.output.name, "out-bottom-right", "mover natively on the target after arrival");
            assert.deepEqual(
                h.mover.desktops.map((desk) => desk.id),
                ["ws-c"],
                "membership homed on arrival",
            );
            assert.equal(h.outputWrites.length, 1, "exactly one native output relocation");
            assert.equal(h.membershipWrites.length, 1, "exactly one native membership write");
            const left = winById(h, "w-bl");
            assert.equal(left.output.name, "out-bottom", "unchosen left candidate keeps its output");
            assert.deepEqual(
                left.desktops.map((desk) => desk.id),
                ["ws-b"],
                "unchosen left candidate keeps its membership",
            );
            assert.deepEqual({ ...left.frameGeometry }, leftBefore, "unchosen left candidate keeps its geometry");
            assert.equal(h.workspace["activeWindow"], h.mover, "mover keeps focus across the pinned arrival");
            assert.equal(h.focused.length, 0, "no redundant focus setter: the mover never lost focus");
            assert.equal(h.commandPayloads("move-directional").length, 1, "no command replay after settlement");
            // Late duplicate native input never replays the flight.
            h.fire("windowActivated");
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.commandPayloads("move-directional").length, 1, "no replay on duplicate signals");
            assert.equal(h.outputWrites.length, 1, "no second relocation on duplicate signals");
            assert.equal(h.membershipWrites.length, 1, "no second membership write on duplicate signals");
        } finally {
            await h.stop();
        }
    });

    it("sends down to the mover-selected target with follow", async () => {
        const h = await makeHarness(1100);
        try {
            await discover(h);
            h.handle.requestSendToOutput("down", true);
            await h.settle();
            const sends = h.commandPayloads("send-directional");
            assert.equal(sends.length, 1, "exactly one direction-only send dispatch, never replayed");
            const cmd = sends[0]?.body["command"] as Record<string, unknown>;
            assert.equal(cmd["follow"], true);
            assert.deepEqual(Object.keys(cmd).sort(), ["direction", "follow", "op", "win"], "send carries no endpoint");
            const captureIndex = h.drainCapture.get((sends[0] as { body: Record<string, unknown> }).body["clientCmd"]);
            assert.ok(captureIndex !== undefined, "the core drained the send");
            const reply = JSON.parse(h.replies.get(captureIndex as number) as string) as Record<string, unknown>;
            assert.notEqual(reply["outcome"], "rejected", "real Engine plans the send");
            appliedMatchesReply(h, reply);
            assert.equal(h.mover.output.name, "out-bottom-right", "send lands on the mover-selected target");
            assert.deepEqual(
                h.mover.desktops.map((desk) => desk.id),
                ["ws-c"],
                "send lands in the target view",
            );
            assert.equal(h.workspace["activeWindow"], h.mover, "follow focuses the mover");
            assert.equal(h.commandPayloads("send-directional").length, 1, "no send replay after settlement");
        } finally {
            await h.stop();
        }
    });

    it("migrates down to the largest edge independent of the mover", async () => {
        // Mover centre x=150 falls in out-bottom's shared edge [0,640): a
        // windowed rank would pick the left candidate, but migration ranks
        // by largest shared edge (1280 vs 640) and must still pick right.
        // The migrated logical rides with fresh backing on both the target
        // (correlated creation; the existing ws-a desktop keeps backing
        // its own cell, never cannibalized) and the source refill.
        const h = await makeHarness(100);
        try {
            await discover(h);
            const nativeOrderBefore = ([...(h.workspace["desktops"] as Array<{ id: string }>)].map((d) => d.id));
            h.handle.requestWorkspaceMigrate("down");
            await h.settle();
            const migrates = h.commandPayloads("migrate-directional");
            assert.equal(migrates.length, 1, "exactly one direction-only migrate dispatch, never replayed");
            const cmd = migrates[0]?.body["command"] as Record<string, unknown>;
            // v2: direction only; legacy from/ws keys refuse as unknown fields.
            assert.deepEqual(Object.keys(cmd).sort(), ["direction", "op"], "migrate names direction only, never the target");
            const captureIndex = h.drainCapture.get((migrates[0] as { body: Record<string, unknown> }).body["clientCmd"]);
            assert.ok(captureIndex !== undefined, "the core drained the migrate");
            // v2: terminal results ride the take lane (requester-only), never
            // the capture reply.
            const result = lastTakeResult(h);
            assert.equal(result["op"], "migrate-workspace", "real Engine resolves the migration");
            assert.equal(result["targetOut"], "out-bottom-right", "largest edge wins independent of the mover");
            assert.equal(result["sourceOut"], "out-top", "source names the active output");
            // The stable core logical id rides (never a destination rename).
            const migratedLogical = result["targetWs"] as string;
            assert.equal(migratedLogical, result["sourceWs"], "migrated logical id rides the move");
            assert.deepEqual(
                (result["members"] as string[]).sort(),
                ["w-move", "w-stay"],
                "the whole workspace migrates together",
            );
            // Fresh backing for both ends, resolved from the settled
            // core-owned rows (never by ordinal or native reuse): the
            // migrated logical binds a created desktop on the target, the
            // core-chosen refill logical binds a created desktop on the
            // source. Both created ids are genuinely fresh (absent from
            // the live pre-migration native inventory snapshotted above).
            const settledRows = (): Array<Record<string, unknown>> => {
                const indices = [...h.replies.keys()].filter((at) => (h.calls[at] as CapturedCall).method === "DescribeWorld");
                const raw = h.replies.get(Math.max(...indices) as number) as string;
                return (JSON.parse(raw) as Record<string, unknown>)["workspaces"] as Array<Record<string, unknown>>;
            };
            const rows = settledRows();
            const targetBacking = rows.find((b) => b["out"] === "out-bottom-right" && b["id"] === migratedLogical)?.["bind"] as string ?? "";
            assert.ok(targetBacking !== "" && !nativeOrderBefore.includes(targetBacking), `target binds fresh backing (got ${targetBacking})`);
            const target = h.screens.bottomRight;
            assert.equal(h.mover.output.name, target.name, "migrating workspace lands on the largest-edge output");
            const companion = winById(h, "w-stay");
            assert.equal(companion.output.name, target.name, "the whole workspace migrates together");
            assert.deepEqual(h.mover.desktops.map((desk) => desk.id), [targetBacking], "mover homed on the fresh backing");
            assert.deepEqual(companion.desktops.map((desk) => desk.id), [targetBacking], "companion homed on the fresh backing");
            assert.equal(h.currentByOutput.get(target.name)?.id, targetBacking, "target view shows the fresh backing");
            // The refill logical rides the paired refill transfer (never a
            // guessed ordinal); its backing is fresh as well.
            const refillTransfer = ((): Record<string, unknown> | undefined => {
                for (const raw of h.replies.values()) {
                    const reply = JSON.parse(raw) as Record<string, unknown>;
                    const found = (reply["transfers"] as Array<Record<string, unknown>>).find((t) => t["kind"] === "view" && (t["op"] as string).endsWith("-refill"));
                    if (found !== undefined) {
                        return found;
                    }
                }
                return undefined;
            })();
            assert.ok(refillTransfer !== undefined, "explicit core refill retained");
            const refillLogical = refillTransfer["ws"] as string;
            assert.ok(refillLogical !== "" && refillLogical !== migratedLogical, "refill excludes the migrated logical");
            const refillBacking = rows.find((b) => b["out"] === "out-top" && b["id"] === refillLogical)?.["bind"] as string ?? "";
            assert.ok(refillBacking !== "" && !nativeOrderBefore.includes(refillBacking), `refill binds fresh backing (got ${refillBacking})`);
            const topView = h.currentByOutput.get(h.screens.top.name);
            assert.equal(topView?.id, refillBacking, "source refills to the created backing");
            assert.equal(topView?.name, "Desktop 4", "the proven spare carries its friendly position name, not the token");
            // The existing ws-a desktop still backs its own cells (never
            // cannibalized for the migration).
            const wsARows = rows.filter((b) => b["bind"] === "ws-a");
            assert.ok(wsARows.length > 0 && wsARows.every((b) => b["id"] !== migratedLogical), "existing desktop keeps its own logical");
            assert.deepEqual(
                h.viewSwitches.slice(0, 2),
                [`out-bottom-right->${targetBacking}`, `out-top->${refillBacking}`],
                "exactly the target and refill view switches, no self-refill of the target",
            );
            assert.equal(h.viewSwitches.length, 2, "settled rounds switch no further views");
            assert.equal(h.commandPayloads("migrate-directional").length, 1, "no migrate replay after settlement");
        } finally {
            await h.stop();
        }
    });

    it("keeps split-edge focus local while the windowed move selects", async () => {
        // Strict unique-reciprocal focus: two forward touches below refuse
        // the cross as unchanged with zero writes. The entry then recovers
        // through observation-only catchup and the same edge still
        // window-selects the move on the sole-leaf report below.
        const h = await makeHarness(1100);
        try {
            await discover(h);
            const framesBefore = new Map(h.wins.map((win) => [win.internalId, { ...win.frameGeometry }]));
            h.handle.requestFocus("down");
            await h.settle();
            const focuses = h.commandPayloads("focus-directional");
            assert.equal(focuses.length, 1, "exactly one direction-only focus dispatch");
            const focusCapture = h.drainCapture.get((focuses[0] as { body: Record<string, unknown> }).body["clientCmd"]);
            assert.ok(focusCapture !== undefined, "the core drained the focus");
            const focusReply = JSON.parse(h.replies.get(focusCapture as number) as string) as Record<string, unknown>;
            assert.equal(focusReply["outcome"], "rejected", "strict split edge refuses the cross");
            assert.equal(focusReply["kind"], "unchanged", "no local target either: strict unchanged");
            const active = h.workspace["activeWindow"] as FakeWindow;
            assert.equal(active.output.name, "out-top", "focus never leaves the source output");
            assert.equal(h.outputWrites.length, 0, "focus writes no output transfer");
            assert.equal(h.membershipWrites.length, 0, "focus writes no membership");
            for (const win of h.wins) {
                assert.deepEqual({ ...win.frameGeometry }, framesBefore.get(win.internalId), `focus writes no geometry on ${win.internalId}`);
            }
            assert.equal(h.commandPayloads("focus-directional").length, 1, "refusal never replays");
            // Recovery: a native event settles the observation-only catchup
            // (rejections never replay), then the move dispatches once and
            // stays local here: with a second local tile R1 wins, so the
            // cross itself is covered by the sole-leaf move leg above.
            h.fire("windowActivated");
            await h.settle();
            h.handle.requestMove("down");
            await h.settle();
            assert.equal(h.commandPayloads("move-directional").length, 1, "move dispatches once after the catchup");
            assert.equal(h.mover.output.name, "out-top", "two-tile move stays local on the same edge");
            const localCapture = h.drainCapture.get(((h.commandPayloads("move-directional")[0] as { body: Record<string, unknown> }).body)["clientCmd"]);
            assert.ok(localCapture !== undefined, "the core drained the local move");
            const localReply = JSON.parse(h.replies.get(localCapture as number) as string) as Record<string, unknown>;
            assert.equal((localReply["transfers"] as Array<unknown>).length, 0, "local R1-R3 move retains no transfer");
            // v2: the executed local move carries an explicit focus effect
            // for the mover (already focused): it actuates with no redundant
            // native write, so focus is unchanged.
            const localEffects = localReply["effects"] as Array<Record<string, unknown>>;
            assert.equal(localEffects.length, 1, "local move carries exactly the mover focus effect");
            assert.equal(localEffects[0]?.["kind"], "focus", "mover focus effect");
            assert.equal(localEffects[0]?.["target"], "w-move", "focus stays on the mover");
            assert.equal(h.outputWrites.length, 0, "local move writes no output transfer");
            assert.equal(h.membershipWrites.length, 0, "local move writes no membership");
            assert.equal(h.commandPayloads("move-directional").length, 1, "no move replay after settlement");
        } finally {
            await h.stop();
        }
    });

    it("pins the send flight across deferred native arrival with retained focus and no replay", async () => {
        const h = await makeHarness(1100);
        try {
            await discover(h);
            h.deferredTransfer = true;
            h.handle.requestSendToOutput("down", true);
            h.runTimers();
            await h.flushAll();
            assert.equal(h.commandPayloads("send-directional").length, 1, "send dispatched once");
            assert.equal(h.pendingTransfers.length, 1, "native transfer initiated exactly once");
            assert.equal(h.mover.output.name, "out-top", "output still lagging");
            assert.deepEqual(
                h.mover.desktops.map((desk) => desk.id),
                ["ws-c"],
                "membership applies while the output lags",
            );
            assert.equal(h.focused.length, 0, "no follow setter before arrival proof");
            // Delayed native arrival lands through a surface signal: exactly
            // one follow, then settlement.
            const [pending] = h.pendingTransfers;
            assert.ok(pending !== undefined);
            pending.client.output = pending.output;
            h.outputWrites.push(`${pending.client.internalId}=>${pending.output.name}`);
            h.deferredTransfer = false;
            h.pendingTransfers.length = 0;
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.mover.output.name, "out-bottom-right", "mover natively on the target after arrival");
            assert.deepEqual(
                h.mover.desktops.map((desk) => desk.id),
                ["ws-c"],
                "membership homed on arrival",
            );
            assert.equal(h.workspace["activeWindow"], h.mover, "mover keeps focus across the pinned arrival");
            assert.equal(h.focused.length, 0, "no redundant focus setter: the mover never lost focus");
            // Late duplicate native input never replays the flight.
            h.fire("windowActivated");
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.commandPayloads("send-directional").length, 1, "no replay on duplicate signals");
            assert.equal(h.focused.length, 0, "still no redundant focus setter");
        } finally {
            await h.stop();
        }
    });

    it("pins the move flight across deferred native arrival with no replay", async () => {
        // Sole source leaf so the exhausted cross runs: deferred output
        // proves the pinned R4 flight holds membership without replay,
        // then arrival settles exactly once with no second actuation.
        const h = await makeHarness(1100, true);
        try {
            await discover(h);
            h.deferredTransfer = true;
            h.handle.requestMove("down");
            h.runTimers();
            await h.flushAll();
            assert.equal(h.commandPayloads("move-directional").length, 1, "move dispatched once");
            assert.equal(h.pendingTransfers.length, 1, "native transfer initiated exactly once");
            assert.equal(h.mover.output.name, "out-top", "output still lagging");
            assert.deepEqual(
                h.mover.desktops.map((desk) => desk.id),
                ["ws-c"],
                "membership applies while the output lags",
            );
            assert.equal(h.focused.length, 0, "no focus setter before arrival proof");
            const [pending] = h.pendingTransfers;
            assert.ok(pending !== undefined);
            pending.client.output = pending.output;
            h.outputWrites.push(`${pending.client.internalId}=>${pending.output.name}`);
            h.deferredTransfer = false;
            h.pendingTransfers.length = 0;
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.mover.output.name, "out-bottom-right", "mover natively on the target after arrival");
            assert.deepEqual(
                h.mover.desktops.map((desk) => desk.id),
                ["ws-c"],
                "membership homed on arrival",
            );
            assert.equal(h.workspace["activeWindow"], h.mover, "mover keeps focus across the pinned arrival");
            assert.equal(h.focused.length, 0, "no redundant focus setter: the mover never lost focus");
            assert.equal(h.outputWrites.length, 1, "exactly one relocation after deferred arrival");
            assert.equal(h.membershipWrites.length, 1, "exactly one membership write after deferred arrival");
            h.fire("windowActivated");
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.commandPayloads("move-directional").length, 1, "no replay on duplicate signals");
            assert.equal(h.outputWrites.length, 1, "no second relocation on duplicate signals");
            assert.equal(h.focused.length, 0, "still no redundant focus setter");
        } finally {
            await h.stop();
        }
    });

    it("settles the arrived move against fresh topology with no rerank", async () => {
        // After the R4 arrival settles, a refreshed topology report (same
        // rects, new observation) validates without re-ranking the
        // relocated mover: no new transfer, no replay, mover stays homed.
        const h = await makeHarness(1100, true);
        try {
            await discover(h);
            h.handle.requestMove("down");
            await h.settle();
            assert.equal(h.mover.output.name, "out-bottom-right", "precondition: mover arrived");
            assert.equal(h.commandPayloads("move-directional").length, 1, "precondition: one move");
            h.fire("screensChanged");
            await h.settle();
            assert.equal(h.commandPayloads("move-directional").length, 1, "fresh topology never reranks the settled mover");
            assert.equal(h.mover.output.name, "out-bottom-right", "mover stays homed after topology refresh");
            assert.deepEqual(
                h.mover.desktops.map((desk) => desk.id),
                ["ws-c"],
                "membership stays homed after topology refresh",
            );
            assert.equal(h.outputWrites.length, 1, "no second relocation after topology refresh");
            assert.equal(h.membershipWrites.length, 1, "no second membership write after topology refresh");
        } finally {
            await h.stop();
        }
    });

    it("refuses an unreadable split edge with zero writes", async () => {
        const h = await makeHarness(1100);
        try {
            await discover(h);
            const framesBefore = new Map(h.wins.map((win) => [win.internalId, { ...win.frameGeometry }]));
            // Corrupt one FULL rectangle: the core can no longer rank the
            // edge, so the directional command must refuse fail-closed.
            (h.screens.bottomRight.geometry as Record<string, unknown>)["width"] = "bad";
            h.fire("screensChanged");
            h.handle.requestMove("down");
            h.runTimers();
            await h.flushAll();
            const moves = h.commandPayloads("move-directional");
            assert.equal(moves.length, 1, "refused command still dispatches once for the core verdict");
            const refusedCapture = h.drainCapture.get((moves[0] as { body: Record<string, unknown> }).body["clientCmd"]);
            assert.ok(refusedCapture !== undefined, "the core drained the refused move");
            assert.equal(h.mover.output.name, "out-top", "no output transfer on refusal");
            for (const win of h.wins) {
                assert.deepEqual({ ...win.frameGeometry }, framesBefore.get(win.internalId), `no geometry write on ${win.internalId}`);
            }
            assert.equal(h.membershipWrites.length, 0, "no membership write on refusal");
            assert.equal(h.commandPayloads("move-directional").length, 1, "refusal never replays");
        } finally {
            await h.stop();
        }
    });
});
