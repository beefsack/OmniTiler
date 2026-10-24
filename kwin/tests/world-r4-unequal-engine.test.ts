// Unequal-height occupied-target R4 through the WORLD production route:
// the real `WorldEngine` (via the test-only `world_eval` example: same
// decode, retained engine, codec and generation binding the shipped world
// service uses; only D-Bus and signals are stubbed) behind the production
// `startWorldAdapterEntry` (direction-only commands, full DiscoverWorld /
// DescribeWorld transport, native actuation). Side-by-side horizontal
// outputs carry genuinely unequal FULL and work-area heights (synthetic
// fixture: left FULL 2048x1152, right FULL 1920x1080 at y72; work areas
// 2048x1100 vs 1920x1000). Both outputs stay occupied across an 8-leg left/right
// shuttle: the left starts with two survivors plus the east-edge mover,
// the right with two occupied survivors. Every R4 wraps the whole
// target, preserving the survivor subtree. Every leg lands full height nearest the
// source with no far-side overlap, retains exactly one unheld window
// transfer with a fresh stable op, and actuates natively (no clamp
// exhaustion: every leg writes geometry). Included in `npm test`.
//
// REAL parts: production `startWorldAdapterEntry` on a scripted two-output
// fake KWin surface; retained `WorldEngine` evaluation per DescribeWorld
// round-trip. The per-test `world_eval` process is the fixture's replay
// boundary: every command leg flushes the entry's dispatches through it in
// order. Does NOT use the legacy output-send-engine fixture.

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

interface FakeScreen {
    readonly name: string;
    geometry: { x: number; y: number; width: number; height: number };
}
interface FakeDesktop {
    readonly id: string;
    readonly name: string;
}
interface FakeWindow {
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
    readonly screens: { left: FakeScreen; right: FakeScreen };
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
    readonly focused: string[];
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    readonly signals: Record<string, FakeSignal>;
    fire: (name: string, payload?: unknown) => void;
    runTimers: (maxDelay?: number) => void;
    flush: () => Promise<number>;
    flushAll: () => Promise<void>;
    settle: () => Promise<void>;
    commandPayloads: (op: string) => Array<{ index: number; body: Record<string, unknown> }>;
    stop: () => Promise<void>;
}

async function makeHarness(): Promise<Harness> {
    // Defect geometry: genuinely unequal FULL heights (1152 vs 1080 at y72)
    // and genuinely unequal work areas (1100 vs 1000). The Engine ranks the
    // windowed R4 candidate by mover-centre projection over these FULL
    // rects; the adapter never selects a target.
    const left: FakeScreen = { name: "out-left", geometry: { x: 0, y: 0, width: 2048, height: 1152 } };
    const right: FakeScreen = { name: "out-right", geometry: { x: 2048, y: 72, width: 1920, height: 1080 } };
    const wsA: FakeDesktop = { id: "ws-a", name: "WS A" };
    const wsB: FakeDesktop = { id: "ws-b", name: "WS B" };
    const liveDesktops: FakeDesktop[] = [wsA, wsB];
    const currentByOutput = new Map<string, FakeDesktop>([
        ["out-left", wsA],
        ["out-right", wsB],
    ]);
    const mkWin = (
        output: FakeScreen,
        desktop: FakeDesktop,
        internalId: string,
        frame: { x: number; y: number; width: number; height: number },
    ): FakeWindow => ({
        internalId,
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
    // Left seeds admit as a horizontal row (full work height); the right
    // starts with two occupied tiles. The left holds two survivors plus
    // the east-edge mover (nearest the right output); the right holds two
    // occupied survivors. Every shuttle leg keeps both outputs occupied
    // (2 and 3 alternating, never empty), including return from a nested
    // whole-target wrap, with four visits per target.
    const wins: FakeWindow[] = [
        mkWin(left, wsA, "w-l1", { x: 10, y: 10, width: 650, height: 1000 }),
        mkWin(left, wsA, "w-l2", { x: 680, y: 10, width: 650, height: 1000 }),
        mkWin(left, wsA, "w-move", { x: 1350, y: 10, width: 650, height: 1000 }),
        mkWin(right, wsB, "w-r1", { x: 2058, y: 82, width: 880, height: 900 }),
        mkWin(right, wsB, "w-r2", { x: 2958, y: 82, width: 880, height: 900 }),
    ];
    const mover = wins.find((win) => win.internalId === "w-move") as FakeWindow;
    const geometryWrites: string[] = [];
    const membershipWrites: string[] = [];
    const outputWrites: string[] = [];
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
        screens: { left, right },
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
        focused,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        signals,
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
    let activeRef: FakeWindow | null = mover;
    const surface: Record<string, unknown> = {
        windowList: (): FakeWindow[] => [...harness.wins],
        screens: [left, right],
        desktops: liveDesktops,
        // Active screen follows the focused window's output (no stale
        // primary proof): production KWin moves the active screen with the
        // focused client, and the world topology primary mirrors it.
        get activeScreen(): FakeScreen {
            return (activeRef?.output as FakeScreen | undefined) ?? left;
        },
        cursorPos: { x: 1300, y: 100 },
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
            const out = (activeRef?.output as FakeScreen | undefined)?.name ?? "out-left";
            return harness.currentByOutput.get(out) ?? wsA;
        },
        set currentDesktop(ref: unknown) {
            void ref;
        },
        clientArea: (_kind: number, output: unknown, _desktop: unknown): Record<string, number> => {
            const name = (output as FakeScreen).name;
            if (name === "out-left") {
                return { x: 0, y: 0, w: 2048, h: 1100 };
            }
            return { x: 2048, y: 72, w: 1920, h: 1000 };
        },
        currentDesktopForScreen: (output: unknown): FakeDesktop => {
            const name = (output as FakeScreen).name;
            return harness.currentByOutput.get(name) ?? wsA;
        },
        setCurrentDesktopForScreen: (): void => undefined,
        sendClientToScreen: (client: unknown, output: unknown): void => {
            const win = client as FakeWindow;
            win.output = output as FakeScreen;
            harness.outputWrites.push(`${win.internalId}=>${(output as FakeScreen).name}`);
        },
        createDesktop: (): void => undefined,
        removeDesktop: (): void => undefined,
    };
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
    disc.callback(reply);
    await h.settle();
    assert.equal(h.commandPayloads("move-directional").length, 0, "no move before legs");
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
        // v2: reply `ws` is the core logical id; producer natives resolve
        // through the core-owned binding, never by string equality.
        const planned = rows.find((row) => row["out"] === entry["out"] && row["id"] === entry["ws"]);
        assert.ok(planned !== undefined, `Engine plans a workspace row for ${entry["win"]} at ${entry["out"]}/${entry["ws"]}`);
        assert.deepEqual(
            win.desktops.map((desk) => desk.id),
            [planned["bind"]],
            `native membership of ${entry["win"]} matches the Engine plan through the core binding`,
        );
    }
}

describe("world unequal-height occupied-target R4 through the production entry (real Engine)", () => {
    // Return-leg refusal fixed core-side: after an R4 departure with
    // follow, the vacated source session is focusless while the global
    // active window lives on the other output; the retained Reconcile
    // converges that focusless source with focus none instead of holding
    // it on `focus-mismatch` (which fenced the return pair as
    // `operation-refused`). The clamp lifetime
    // fix itself is proven by world-apply-clamp-lifetime (alternating A/B
    // keeps writing).
    it("replays left/right R4 across unequal-height outputs with near-side full-height arrivals", async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            // Fixture contract: genuinely unequal FULL heights (1152 vs
            // 1080 at y72) and genuinely unequal work areas (1100 vs
            // 1000). Both outputs stay occupied (2 and 3 alternating, never
            // empty): the left starts with two survivors plus the east-edge
            // mover, the right with two occupied survivors. The mover
            // starts east (near the right output).
            const leftGeo = h.screens.left.geometry;
            const rightGeo = h.screens.right.geometry;
            assert.notEqual(leftGeo.height, rightGeo.height, "FULL heights are genuinely unequal");
            assert.equal(leftGeo.height, 1152, "left FULL fixture height");
            assert.equal(rightGeo.height, 1080, "right FULL fixture height");
            assert.notEqual(rightGeo.y, leftGeo.y, "outputs carry a vertical offset");
            assert.equal((h.workspace["activeWindow"] as FakeWindow).internalId, "w-move", "mover starts focused");
            assert.equal(h.mover.output.name, "out-left", "mover starts on the left");
            const moverStart = frameOf(h.mover);
            const l1Start = frameOf(winById(h, "w-l1"));
            assert.ok(moverStart.x > l1Start.x, "mover starts near the east edge");

            const moveOps: string[] = [];
            const visits = new Map<string, number>([["out-left", 0], ["out-right", 0]]);
            // Eight legs shuttle outward crossings: right from the left
            // east edge, left from the right west edge, and so on. Each
            // target is visited four times (>3). Every leg must plan R4
            // (never a local move, never a capacity float), land full
            // height nearest the source with no far-side overlap, retain
            // exactly one unheld window transfer with a fresh stable op,
            // and actuate natively (no clamp exhaustion: every leg writes
            // geometry).
            for (let leg = 0; leg < 8; leg += 1) {
                const direction = leg % 2 === 0 ? "right" : "left";
                const expectedOut = direction === "right" ? "out-right" : "out-left";
                visits.set(expectedOut, (visits.get(expectedOut) as number) + 1);
                const geometryBefore = h.geometryWrites.length;
                h.handle.requestMove(direction);
                await h.settle();
                const moves = h.commandPayloads("move-directional");
                assert.equal(moves.length, leg + 1, `exactly one new move dispatch on leg ${leg}`);
                // v2: filed moves drain FIFO through captures; the draining
                // capture's bound reply carries the plan (never the queue
                // call, which carries no reply).
                const captureIndex = h.drainCapture.get((moves[leg]?.body as Record<string, unknown>)["clientCmd"]);
                assert.ok(captureIndex !== undefined, `the core drained every move through leg ${leg}`);
                const reply = JSON.parse(h.replies.get(captureIndex as number) as string) as Record<string, unknown>;
                assert.notEqual(reply["outcome"], "rejected", `real Engine plans R4 leg ${leg}, got ${JSON.stringify(reply).slice(0, 200)}`);
                const transfers = reply["transfers"] as Array<Record<string, unknown>>;
                assert.equal(transfers.length, 1, `leg ${leg} retains exactly one transfer`);
                const transfer = transfers[0] as Record<string, unknown>;
                assert.equal(transfer["kind"], "window", `leg ${leg} transfer is a window, never a capacity float`);
                assert.equal(transfer["win"], "w-move", `leg ${leg} transfer carries the mover`);
                assert.equal(transfer["out"], expectedOut, `leg ${leg} lands on the commanded output`);
                assert.equal(transfer["follow"], true, `leg ${leg} follows the mover`);
                assert.equal(transfer["held"], false, `leg ${leg} never holds/exhausts`);
                assert.match(transfer["op"] as string, /^op-\d+$/, `leg ${leg} carries a stable command identity`);
                moveOps.push(transfer["op"] as string);
                const rows = reply["windows"] as Array<Record<string, unknown>>;
                const planned = rows.find((entry) => entry["win"] === "w-move") as Record<string, unknown>;
                assert.ok(planned !== undefined, `leg ${leg} plans the crossing mover`);
                const frame = planned["frame"] as [number, number, number, number];
                appliedMatchesReply(h, reply);
                assert.equal(h.mover.output.name, expectedOut, `leg ${leg} mover natively on the target`);
                // Full target vertical allocation: the horizontal R4 wrap
                // keeps the full work height (width halves). The native
                // mover height must equal the planned full height and the
                // survivors' full height on the same output.
                const moverFrame = frameOf(h.mover);
                assert.equal(moverFrame.h, frame[3], `leg ${leg} native height matches the plan`);
                assert.equal(moverFrame.y, frame[1], `leg ${leg} native y matches the plan`);
                // Whole-target wrap gives the mover half the root width,
                // not one third as a flat three-child append would.
                assert.equal(moverFrame.w, direction === "right" ? 948 : 1012, `leg ${leg} wraps the whole occupied target`);
                assert.equal(moverFrame.h, direction === "right" ? 984 : 1084, `leg ${leg} uses the full inset work-area height`);
                assert.equal(moverFrame.y, direction === "right" ? 80 : 8, `leg ${leg} uses the target work-area origin`);
                const survivors = h.wins.filter((win) => win.internalId !== "w-move" && win.output.name === expectedOut);
                assert.ok(survivors.length >= 1, `leg ${leg} target stays occupied`);
                for (const survivor of survivors) {
                    const survivorFrame = frameOf(survivor);
                    assert.equal(moverFrame.h, survivorFrame.h, `leg ${leg} mover takes the full target height like ${survivor.internalId}`);
                    assert.equal(moverFrame.y, survivorFrame.y, `leg ${leg} mover shares the target row like ${survivor.internalId}`);
                }
                // Near-side x with no far-side overlap: right lands west
                // of the eastern survivors within the right work area;
                // left lands east of the western survivors within the left
                // work area.
                if (direction === "right") {
                    for (const survivor of survivors) {
                        const survivorFrame = frameOf(survivor);
                        assert.ok(moverFrame.x + moverFrame.w <= survivorFrame.x, `leg ${leg} right lands nearest the source with no overlap, mover x+w=${moverFrame.x + moverFrame.w} vs ${survivor.internalId} x=${survivorFrame.x}`);
                    }
                    assert.ok(moverFrame.x >= 2048, `leg ${leg} mover stays inside the right work area, got x=${moverFrame.x}`);
                    assert.ok(moverFrame.x + moverFrame.w <= 2048 + 1920, `leg ${leg} mover never crosses the far side, got x+w=${moverFrame.x + moverFrame.w}`);
                } else {
                    for (const survivor of survivors) {
                        const survivorFrame = frameOf(survivor);
                        assert.ok(survivorFrame.x + survivorFrame.w <= moverFrame.x, `leg ${leg} left lands nearest the source with no overlap, ${survivor.internalId} x+w=${survivorFrame.x + survivorFrame.w} vs mover x=${moverFrame.x}`);
                    }
                    assert.ok(moverFrame.x >= 0, `leg ${leg} mover stays inside the left work area, got x=${moverFrame.x}`);
                    assert.ok(moverFrame.x + moverFrame.w <= 2048, `leg ${leg} mover never crosses the far side, got x+w=${moverFrame.x + moverFrame.w}`);
                }
                assert.equal(h.workspace["activeWindow"], h.mover, `leg ${leg} mover keeps focus`);
                assert.ok(h.geometryWrites.length > geometryBefore, `leg ${leg} actuates geometry (no clamp exhaustion)`);
                const floats = (reply["windows"] as Array<Record<string, unknown>>).filter((entry) => entry["floating"] === true);
                assert.equal(floats.length, 0, `leg ${leg} never floats/exhausts`);
            }
            // Each target visited four times (>3); eight legs ride eight
            // distinct ops; late duplicate signals replay nothing.
            assert.equal(visits.get("out-left"), 4, "left target visited four times");
            assert.equal(visits.get("out-right"), 4, "right target visited four times");
            assert.equal(new Set(moveOps).size, 8, "eight legs carry eight distinct transfer identities");
            const movesBefore = h.commandPayloads("move-directional").length;
            const outputsBefore = h.outputWrites.length;
            const membershipsBefore = h.membershipWrites.length;
            h.fire("windowActivated");
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.commandPayloads("move-directional").length, movesBefore, "no replay on duplicate signals");
            assert.equal(h.outputWrites.length, outputsBefore, "no second relocation on duplicate signals");
            assert.equal(h.membershipWrites.length, membershipsBefore, "no second membership write on duplicate signals");
            assert.ok(!h.logs.some((line) => line.includes("command-dropped reason=move-")), "no move drops");
        } finally {
            await h.stop();
        }
    });
});
