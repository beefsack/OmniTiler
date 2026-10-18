// Split-edge selection through the WORLD production route: the real
// `WorldEngine` (via the test-only `world_eval` example: same decode,
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
// `world_eval` process is the fixture's replay boundary: every command leg
// flushes the entry's dispatches through it in order.

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";
import { describe, it } from "node:test";

import { WORLD_DEBOUNCE_MS, startWorldAdapterEntry } from "../src/world-entry";

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
                env: { ...process.env, CARGO_BUILD_JOBS: "3" },
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
    workspace: Record<string, unknown>;
    readonly screens: { top: FakeScreen; bottom: FakeScreen; bottomRight: FakeScreen };
    readonly desktops: { a: FakeDesktop; b: FakeDesktop; c: FakeDesktop };
    readonly wins: FakeWindow[];
    readonly mover: FakeWindow;
    readonly currentByOutput: Map<string, FakeDesktop>;
    readonly calls: CapturedCall[];
    readonly answered: Set<number>;
    readonly replies: Map<number, string>;
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
    const created: string[] = [];
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
        workspace: {},
        screens: { top, bottom, bottomRight },
        desktops: { a: wsA, b: wsB, c: wsC },
        wins,
        mover,
        currentByOutput,
        calls: [],
        answered: new Set<number>(),
        replies: new Map<number, string>(),
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
            const reply = await harness.bridge.send(harness.calls[index]?.method as string, harness.calls[index]?.payload as string);
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
            const out: Array<{ index: number; body: Record<string, unknown> }> = [];
            harness.calls.forEach((call, index) => {
                if (call.method !== "DescribeWorld") {
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
        createDesktop: (...args: ReadonlyArray<unknown>): void => {
            if (args.length !== 2 || typeof args[0] !== "number" || typeof args[1] !== "string" || (args[1] as string) === "") {
                throw new Error("Insufficient arguments");
            }
            const position = args[0] as number;
            const name = args[1] as string;
            const id = `native-${liveDesktops.length + 1}`;
            liveDesktops.splice(Math.max(0, Math.min(position, liveDesktops.length)), 0, { id, name });
            created.push(id);
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
            if (method !== "DiscoverWorld" && method !== "DescribeWorld") {
                return;
            }
            harness.calls.push({ method, payload, callback });
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
    const reply = await h.bridge.send(disc.method, disc.payload);
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

function frameOf(win: FakeWindow): { x: number; y: number; w: number; h: number } {
    const frame = win.frameGeometry as { x: number; y: number; width: number; height: number };
    return { x: frame.x, y: frame.y, w: frame.width, h: frame.height };
}

function winById(h: Harness, id: string): FakeWindow {
    const found = h.wins.find((win) => win.internalId === id);
    assert.ok(found !== undefined, `window ${id} exists on the fake surface`);
    return found as FakeWindow;
}

// Desired window rows carry `frame` as [x,y,w,h] plus homing: every row
// the real Engine names must be natively present.
function appliedMatchesReply(h: Harness, reply: Record<string, unknown>): void {
    const geometry = reply["windows"] as Array<Record<string, unknown>>;
    assert.ok(Array.isArray(geometry) && geometry.length > 0, "real reply carries desired window rows");
    for (const entry of geometry) {
        const frame = entry["frame"] as [number, number, number, number] | null;
        const win = winById(h, entry["win"] as string);
        if (frame !== null) {
            assert.deepEqual(frameOf(win), { x: frame[0], y: frame[1], w: frame[2], h: frame[3] }, `native frame of ${entry["win"]} matches the Engine plan`);
        }
        assert.equal(win.output.name, entry["out"], `native output of ${entry["win"]} matches the Engine plan`);
        assert.deepEqual(
            win.desktops.map((desk) => desk.id),
            [entry["ws"]],
            `native membership of ${entry["win"]} matches the Engine plan`,
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
            const answered = moves.filter(({ index }) => h.answered.has(index));
            assert.equal(answered.length, 1, "the core answered the move");
            const reply = JSON.parse(h.replies.get(answered[0]?.index as number) as string) as Record<string, unknown>;
            assert.notEqual(reply["outcome"], "rejected", `real Engine plans the cross, got ${JSON.stringify(reply).slice(0, 160)}`);
            // The real Engine selects the mover-ranked target in its desired
            // rows: the mover homes on out-bottom-right/ws-c, the non-left/top
            // candidate. Native relocation is NOT actuated: a
            // move-directional cross retains no window transfer, and the
            // adapter fences membership/output writes without transfer
            // authority (reported to the Lead as a core/adapter contract
            // gap). The fenced steady state below is the honest assertion.
            const rows = reply["windows"] as Array<Record<string, unknown>>;
            const planned = rows.find((entry) => entry["win"] === "w-move");
            assert.ok(planned !== undefined, "Engine plans the crossing mover");
            assert.equal(planned["out"], "out-bottom-right", "selection resolves the non-left/top target");
            assert.equal(planned["ws"], "ws-c", "selection lands in the target view");
            assert.equal(h.outputWrites.length, 0, "no output transfer without transfer authority");
            assert.equal(h.membershipWrites.length, 0, "no membership write without transfer authority");
            assert.equal(h.mover.output.name, "out-top", "mover stays fenced on the source output");
            const left = winById(h, "w-bl");
            assert.equal(left.output.name, "out-bottom", "unchosen left candidate keeps its output");
            assert.deepEqual(
                left.desktops.map((desk) => desk.id),
                ["ws-b"],
                "unchosen left candidate keeps its membership",
            );
            assert.deepEqual({ ...left.frameGeometry }, leftBefore, "unchosen left candidate keeps its geometry");
            assert.equal(h.workspace["activeWindow"], h.mover, "fenced mover keeps focus");
            assert.equal(h.commandPayloads("move-directional").length, 1, "no command replay after settlement");
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
            const answered = sends.filter(({ index }) => h.answered.has(index));
            assert.equal(answered.length, 1, "the core answered the send");
            const reply = JSON.parse(h.replies.get(answered[0]?.index as number) as string) as Record<string, unknown>;
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
        const h = await makeHarness(100);
        try {
            await discover(h);
            h.handle.requestWorkspaceMigrate("down");
            // Bounded rounds, not settle-to-quiet: the retained migration
            // transfer is never dropped through this observer (the global
            // native order can never confirm the move), so observation-only
            // feedback rounds keep flowing; that steady-state ping-pong is a
            // reported core/adapter contract gap, not fixture flakiness. The
            // migration facts below are stable across those rounds: exactly
            // one command, exact target, exact native relocation, exact
            // refill views.
            await h.settleRounds(6);
            const migrates = h.commandPayloads("migrate-directional");
            assert.equal(migrates.length, 1, "exactly one direction-only migrate dispatch, never replayed");
            const cmd = migrates[0]?.body["command"] as Record<string, unknown>;
            assert.deepEqual(Object.keys(cmd).sort(), ["direction", "from", "op", "ws"], "migrate names source only, never the target");
            const answered = migrates.filter(({ index }) => h.answered.has(index));
            assert.ok(answered.length >= 1, "the core answered the migrate");
            const reply = JSON.parse(h.replies.get(answered[0]?.index as number) as string) as Record<string, unknown>;
            const result = reply["result"] as Record<string, unknown>;
            assert.equal(result["op"], "migrate-workspace", "real Engine resolves the migration");
            assert.equal(result["targetOut"], "out-bottom-right", "largest edge wins independent of the mover");
            assert.equal(result["sourceOut"], "out-top", "source names the active output");
            assert.deepEqual(
                (result["members"] as string[]).sort(),
                ["w-move", "w-stay"],
                "the whole workspace migrates together",
            );
            const target = h.screens.bottomRight;
            assert.equal(h.mover.output.name, target.name, "migrating workspace lands on the largest-edge output");
            const companion = winById(h, "w-stay");
            assert.equal(companion.output.name, target.name, "the whole workspace migrates together");
            assert.equal(h.currentByOutput.get(target.name)?.id, "ws-a", "target view shows the migrated workspace");
            const topView = h.currentByOutput.get(h.screens.top.name);
            assert.ok(topView !== undefined && topView.id.startsWith("native-"), "source refills to a created trailing spare");
            assert.equal(topView?.name, "wl-1", "the spare carries the core logical token");
            assert.deepEqual(
                h.viewSwitches.slice(0, 2),
                [`out-bottom-right->ws-a`, `out-top->${topView?.id as string}`],
                "exactly the target and refill view switches, no self-refill of the target",
            );
            assert.equal(h.viewSwitches.length, 2, "steady rounds switch no further views");
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
            const answeredFocus = focuses.filter(({ index }) => h.answered.has(index));
            assert.equal(answeredFocus.length, 1, "the core answered the focus");
            const focusReply = JSON.parse(h.replies.get(answeredFocus[0]?.index as number) as string) as Record<string, unknown>;
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
            const answered = moves.filter(({ index }) => h.answered.has(index));
            assert.equal(answered.length, 1, "the core answered the refused move");
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
