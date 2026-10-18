// Rapid directional FIFO through the production entry against the real
// `WorldEngine` (via the test-only `world_eval` example: same decode,
// retained engine, codec and generation binding the shipped world service
// uses; only D-Bus and signals are stubbed). Proves integrated native
// effects, not just dispatched command sequences: left-then-right focus
// lands exact native focus per the Engine in order (neighbor then origin:
// queued tiled focus resolves its subject at drain time from the
// authoritative current native focus), queued tiled moves rebind the same
// way (mixed focus-then-move proves the mover is the settled new focus),
// and each repeated move applies its own planned geometry round. A queued native-signal invalidation still
// withholds the stale head's real setters, and a timed-out head still
// drains its tail with no synthetic signal. Included in `npm test`.

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

interface FakeDesktop {
    readonly id: string;
    readonly name: string;
}

interface FakeWindow {
    internalId: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
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

interface Timer {
    delayMs: number;
    callback: () => void;
    cancelled: boolean;
    fired: boolean;
}

interface Harness {
    readonly bridge: WorldBridge;
    workspace: Record<string, unknown>;
    readonly wins: FakeWindow[];
    readonly calls: CapturedCall[];
    readonly answered: Set<number>;
    readonly replies: Map<number, string>;
    readonly timers: Timer[];
    readonly logs: string[];
    readonly geometryWrites: string[];
    readonly focused: string[];
    activeRef: FakeWindow | null;
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    readonly signals: Record<string, FakeSignal>;
    fire: (name: string, payload?: unknown) => void;
    runTimers: (maxDelay?: number) => void;
    fireNextTimer: (maxDelay: number, from: number) => void;
    flush: () => Promise<number>;
    flushAt: (index: number) => Promise<number>;
    flushAll: () => Promise<void>;
    settle: () => Promise<void>;
    commandPayloads: (op: string) => Array<{ index: number; body: Record<string, unknown> }>;
    stop: () => Promise<void>;
}

function frameOf(win: FakeWindow): { x: number; y: number; w: number; h: number } {
    const frame = win.frameGeometry as { x: number; y: number; width: number; height: number };
    return { x: frame.x, y: frame.y, w: frame.width, h: frame.height };
}

async function makeHarness(focusId: string, ids: string[]): Promise<Harness> {
    const ws: FakeDesktop = { id: "ws1", name: "WS1" };
    const liveDesktops: FakeDesktop[] = [ws];
    const wins: FakeWindow[] = ids.map((id, i) => ({
        internalId: id,
        normalWindow: true,
        frameGeometry: { x: i * 300, y: 0, width: 200, height: 200 },
        output: { name: "out" },
        desktops: [ws],
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
    }));
    const geometryWrites: string[] = [];
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
        wins,
        calls: [],
        answered: new Set<number>(),
        replies: new Map<number, string>(),
        timers: [],
        logs: [],
        geometryWrites,
        focused,
        activeRef: null,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        signals,
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
        fireNextTimer: (maxDelay: number, from: number): void => {
            let at = -1;
            for (let i = from; i < harness.timers.length; i += 1) {
                const timer = harness.timers[i] as Timer;
                if (!timer.cancelled && !timer.fired && timer.delayMs <= maxDelay &&
                    (at < 0 || timer.delayMs < (harness.timers[at] as Timer).delayMs)) {
                    at = i;
                }
            }
            assert.ok(at >= 0, "expected a due timer");
            const [timer] = harness.timers.splice(at, 1) as Timer[];
            (timer as Timer).callback();
        },
        flush: async () => {
            const index = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
            assert.ok(index >= 0, "expected a queued DescribeWorld dispatch to flush to the real Engine");
            return harness.flushAt(index);
        },
        flushAt: async (index: number): Promise<number> => {
            assert.ok(
                (harness.calls[index] as CapturedCall | undefined)?.method === "DescribeWorld" && !harness.answered.has(index),
                "expected the targeted DescribeWorld dispatch to be unanswered",
            );
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
    const active = wins.find((win) => win.internalId === focusId);
    assert.ok(active !== undefined, `focus window ${focusId} exists`);
    harness.activeRef = active as FakeWindow;
    const surface: Record<string, unknown> = {
        windowList: (): FakeWindow[] => [...harness.wins],
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: liveDesktops,
        activeScreen: { name: "out" },
        cursorPos: { x: 2, y: 100 },
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        activeScreenChanged: (signals["activeScreenChanged"] as FakeSignal).signal,
        get activeWindow(): FakeWindow | null {
            return harness.activeRef;
        },
        set activeWindow(ref: unknown) {
            const found = harness.wins.find((win) => (win as unknown) === ref);
            if (found !== undefined) {
                harness.activeRef = found;
                harness.focused.push(found.internalId);
            }
        },
        get currentDesktop(): FakeDesktop {
            return ws;
        },
        set currentDesktop(ref: unknown) {
            void ref;
        },
        clientArea: (): Record<string, number> => ({ x: 0, y: 0, width: 1920, height: 400 }),
        currentDesktopForScreen: (): FakeDesktop => ws,
        setCurrentDesktopForScreen: (): void => undefined,
        sendClientToScreen: (): void => undefined,
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
    const reply = await h.bridge.send(disc.method, disc.payload);
    const parsed = JSON.parse(reply) as Record<string, unknown>;
    assert.equal(parsed["ready"], false, "bootstrap is not ready");
    disc.callback(reply);
    await h.settle();
}

function winById(h: Harness, id: string): FakeWindow {
    const found = h.wins.find((win) => win.internalId === id);
    assert.ok(found !== undefined, `window ${id} exists on the fake surface`);
    return found as FakeWindow;
}

// Every window row the real Engine plans must match the live native
// frame, output and membership exactly.
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

describe("world entry rapid directional FIFO against the real Engine", () => {
    it("applies rapid left then right focus with exact native effects in order", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            // The Engine settles the focused window last: positional order
            // B,C,A. Bring middle win-c into focus with one accepted press;
            // focus never moves windows, so the tree stays put.
            const xs = (id: string): number => frameOf(winById(h, id)).x;
            assert.ok(xs("win-b") < xs("win-c") && xs("win-c") < xs("win-a"), "engine settles positional row B,C,A");
            h.handle.requestFocus("left");
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.deepEqual(h.focused, ["win-c"], "setup press focuses middle win-c");
            // Both rapid presses start from middle win-c; neither press
            // advances the watermark, so each report stays fresh. The
            // queued tail rebinds to the settled focus at dispatch.
            h.handle.requestFocus("left");
            h.handle.requestFocus("right");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")));
            assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
            let moves = h.commandPayloads("focus-directional");
            assert.equal(moves.length, 2, "setup plus head dispatched, tail queued");
            // Head through the real Engine: focus lands on win-b natively.
            const headIndex = await h.flush();
            const headReply = JSON.parse(h.replies.get(headIndex) as string) as Record<string, unknown>;
            const headEffects = headReply["effects"] as Array<Record<string, unknown>>;
            assert.ok(
                headEffects.some((fx) => fx["kind"] === "focus" && fx["target"] === "win-b"),
                `real Engine plans head focus on win-b, got ${JSON.stringify(headEffects)}`,
            );
            assert.deepEqual(h.focused, ["win-c", "win-b"], "Engine-planned focus-left actuates natively");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-b");
            // Tail drains zero-delay after the acceptance. The head moved
            // focus to win-b, so the tail resolves its subject at drain
            // time from the authoritative current native focus (win-b) and
            // plans focus-right back onto win-c: FIFO order is preserved
            // and the core adjudicates each rebuilt intent in turn.
            h.runTimers();
            moves = h.commandPayloads("focus-directional");
            assert.equal(moves.length, 3, "queued opposite press drains after the acceptance");
            const tailBody = moves[2]?.body as Record<string, unknown>;
            assert.deepEqual(
                (tailBody["command"] as Record<string, unknown>)["win"],
                "win-b",
                "queued tail rebinds to the settled native focus at dispatch",
            );
            const tailIndex = await h.flush();
            const tailReply = JSON.parse(h.replies.get(tailIndex) as string) as Record<string, unknown>;
            assert.notEqual(tailReply["outcome"], "rejected", "real Engine plans the rebuilt tail");
            const tailEffects = tailReply["effects"] as Array<Record<string, unknown>>;
            assert.ok(
                tailEffects.some((fx) => fx["kind"] === "focus" && fx["target"] === "win-c"),
                `real Engine plans tail focus on win-c, got ${JSON.stringify(tailEffects)}`,
            );
            assert.ok(!h.logs.some((l) => l.includes("focus-mismatch")));
            assert.deepEqual(h.focused, ["win-c", "win-b", "win-c"], "tail returns focus to the origin natively");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-c");
            // No replay, no coalescing: late duplicate input changes nothing.
            h.fire("windowActivated");
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.commandPayloads("focus-directional").length, 3, "no command replay after settlement");
            assert.deepEqual(h.focused, ["win-c", "win-b", "win-c"], "no duplicate native focus writes");
        } finally {
            await h.stop();
        }
    });

    it("applies rapid left then right pressed during a coalescing debounce in order", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            const xs = (id: string): number => frameOf(winById(h, id)).x;
            assert.ok(xs("win-b") < xs("win-c") && xs("win-c") < xs("win-a"), "engine settles positional row B,C,A");
            h.handle.requestFocus("left");
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.deepEqual(h.focused, ["win-c"], "setup press focuses middle win-c");
            // A native burst arms the observation debounce with no command
            // in flight; both rapid presses queue behind it in order.
            h.fire("windowActivated");
            h.handle.requestFocus("left");
            h.handle.requestFocus("right");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")));
            assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
            // The debounce flushes the head first with a fresh observation.
            h.runTimers();
            let moves = h.commandPayloads("focus-directional");
            assert.equal(moves.length, 2, "setup plus debounced head dispatched, tail queued");
            const headIndex = await h.flush();
            const headReply = JSON.parse(h.replies.get(headIndex) as string) as Record<string, unknown>;
            const headEffects = headReply["effects"] as Array<Record<string, unknown>>;
            assert.ok(
                headEffects.some((fx) => fx["kind"] === "focus" && fx["target"] === "win-b"),
                `real Engine plans head focus on win-b, got ${JSON.stringify(headEffects)}`,
            );
            assert.equal((h.activeRef as FakeWindow).internalId, "win-b");
            // The accepted head drains the queued opposite press next, which
            // rebinds to win-b at dispatch and returns to win-c.
            h.runTimers();
            moves = h.commandPayloads("focus-directional");
            assert.equal(moves.length, 3, "queued opposite press drains after the acceptance");
            const tailIndex = await h.flush();
            const tailReply = JSON.parse(h.replies.get(tailIndex) as string) as Record<string, unknown>;
            assert.notEqual(tailReply["outcome"], "rejected", "real Engine plans the rebuilt debounced tail");
            const tailEffects = tailReply["effects"] as Array<Record<string, unknown>>;
            assert.ok(
                tailEffects.some((fx) => fx["kind"] === "focus" && fx["target"] === "win-c"),
                `real Engine plans tail focus on win-c, got ${JSON.stringify(tailEffects)}`,
            );
            assert.deepEqual(h.focused, ["win-c", "win-b", "win-c"], "debounced pair lands neighbor then origin");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-c");
        } finally {
            await h.stop();
        }
    });

    it("applies each repeated directional move with its own planned geometry", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            // The Engine settles B left of focused win-a: left swaps, then
            // right swaps back. Both presses name win-a.
            const xs = (id: string): number => frameOf(winById(h, id)).x;
            assert.ok(xs("win-b") < xs("win-a"), "engine settles B left of focused win-a");
            h.handle.requestMove("left");
            h.handle.requestMove("right");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")));
            let moves = h.commandPayloads("move-directional");
            assert.equal(moves.length, 1, "head dispatched immediately, tail queued");
            // Head through the real Engine: the swap applies natively with
            // the Engine's own planned frames.
            const headIndex = await h.flush();
            const headReply = JSON.parse(h.replies.get(headIndex) as string) as Record<string, unknown>;
            assert.notEqual(headReply["outcome"], "rejected", "real Engine plans the head move");
            appliedMatchesReply(h, headReply);
            const framesAfterHead = h.wins.map((w) => ({ ...frameOf(w) }));
            // Tail drains zero-delay and swaps back with fresh planning.
            h.runTimers();
            moves = h.commandPayloads("move-directional");
            assert.equal(moves.length, 2, "queued move drains after the acceptance");
            const tailIndex = await h.flush();
            const tailReply = JSON.parse(h.replies.get(tailIndex) as string) as Record<string, unknown>;
            assert.notEqual(tailReply["outcome"], "rejected", "real Engine plans the tail move");
            appliedMatchesReply(h, tailReply);
            assert.ok(
                h.wins.some((w, i) => JSON.stringify(frameOf(w)) !== JSON.stringify(framesAfterHead[i])),
                "tail move rewrites geometry again",
            );
            // Late duplicate input replays nothing.
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.commandPayloads("move-directional").length, 2, "no command replay after settlement");
        } finally {
            await h.stop();
        }
    });

    it("resolves a queued move to the settled focus after a mixed focus then move", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            // The Engine settles B left of focused win-a. Focus-left lands
            // on win-b; the immediately queued move-right still names
            // win-a at press time (the head has not applied yet), so the
            // tail must rebind to the settled win-b at dispatch. From the
            // stale win-a (right edge) right would be a boundary noop,
            // while from win-b it swaps: the planned tail proves the
            // rebinding.
            const xs = (id: string): number => frameOf(winById(h, id)).x;
            assert.ok(xs("win-b") < xs("win-a"), "engine settles B left of focused win-a");
            h.handle.requestFocus("left");
            h.handle.requestMove("right");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")));
            assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
            let focuses = h.commandPayloads("focus-directional");
            let moves = h.commandPayloads("move-directional");
            assert.equal(focuses.length, 1, "focus head dispatched immediately");
            assert.equal(moves.length, 0, "move tail queued behind the focus flight");
            const headIndex = await h.flush();
            const headReply = JSON.parse(h.replies.get(headIndex) as string) as Record<string, unknown>;
            assert.notEqual(headReply["outcome"], "rejected", "real Engine plans the focus head");
            assert.deepEqual(h.focused, ["win-b"], "focus head lands on win-b natively");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-b");
            // The queued move drains zero-delay and rebinds to win-b.
            h.runTimers();
            moves = h.commandPayloads("move-directional");
            assert.equal(moves.length, 1, "queued move drains after the acceptance");
            const tailBody = moves[0]?.body as Record<string, unknown>;
            assert.deepEqual(
                (tailBody["command"] as Record<string, unknown>)["win"],
                "win-b",
                "queued move rebinds to the settled native focus at dispatch",
            );
            assert.deepEqual(
                (tailBody["command"] as Record<string, unknown>)["direction"],
                "right",
                "queued move keeps its pressed direction",
            );
            const tailIndex = await h.flush();
            const tailReply = JSON.parse(h.replies.get(tailIndex) as string) as Record<string, unknown>;
            assert.notEqual(tailReply["outcome"], "rejected", "real Engine plans the rebound move (stale win-a right would be a noop)");
            appliedMatchesReply(h, tailReply);
            assert.equal((h.activeRef as FakeWindow).internalId, "win-b", "mover stays focused");
            assert.ok(!h.logs.some((l) => l.includes("focus-mismatch")));
            assert.ok(!h.logs.some((l) => l.includes("command-dropped reason=move-")));
            // No replay after settlement.
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.commandPayloads("focus-directional").length, 1, "no focus replay after settlement");
            assert.equal(h.commandPayloads("move-directional").length, 1, "no move replay after settlement");
        } finally {
            await h.stop();
        }
    });

    it("withholds a stale head's real setters after queued native invalidation, then drains", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            // Positional row B,C,A: bring middle win-c into focus first so
            // both rapid presses name it and plan focus-left onto win-b.
            const xs = (id: string): number => frameOf(winById(h, id)).x;
            assert.ok(xs("win-b") < xs("win-c") && xs("win-c") < xs("win-a"), "engine settles positional row B,C,A");
            h.handle.requestFocus("left");
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.deepEqual(h.focused, ["win-c"], "setup press focuses middle win-c");
            // Rapid right twice from win-c: both plan focus-right onto win-a.
            h.handle.requestFocus("right");
            h.handle.requestFocus("right");
            // Genuine native input under the flight moves the watermark, so
            // the head's real reply resolves stale with no actuation.
            h.fire("windowActivated");
            const writesBefore = h.focused.length;
            await h.flush();
            assert.ok(h.logs.some((l) => l.includes("stale reason=stale-obs")));
            assert.equal(h.focused.length, writesBefore, "stale head applies no native focus");
            // Catchup stays observation-only, then the queued tail drains
            // and applies exactly once.
            h.runTimers();
            await h.flush();
            h.runTimers();
            await h.flush();
            assert.deepEqual(h.focused, ["win-c", "win-a"], "queued tail applies exactly one native focus after catchup");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-a");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-a");
        } finally {
            await h.stop();
        }
    });

    it("drains the tail after a timed-out head with no synthetic signal", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            const mark = h.timers.length;
            // Same applicable direction twice: the dropped head changes
            // nothing, so the tail still applies against settled topology
            // (A right of B, focus on A).
            const xs = (id: string): number => frameOf(winById(h, id)).x;
            assert.ok(xs("win-b") < xs("win-a"), "engine settles B left of focused win-a");
            h.handle.requestMove("left");
            h.handle.requestMove("left");
            // Expire the head flight, then the scheduled catchup, without
            // answering either and without any native event.
            h.fireNextTimer(2000, mark);
            h.fireNextTimer(2000, mark);
            h.fireNextTimer(2000, mark);
            assert.ok(h.logs.some((l) => l.includes("timeout")));
            // Catchup answers cleanly (flushed by index: the dead head
            // is never sent, so the Engine never sees it), then the tail
            // drains and applies.
            await h.flushAt(h.calls.length - 1);
            h.runTimers();
            const moves = h.commandPayloads("move-directional");
            assert.equal(moves.length, 2, "timed-out head plus drained tail, nothing else");
            const tailIndex = await h.flushAt(h.calls.length - 1);
            const tailReply = JSON.parse(h.replies.get(tailIndex) as string) as Record<string, unknown>;
            assert.notEqual(tailReply["outcome"], "rejected", "real Engine plans the drained tail move");
            appliedMatchesReply(h, tailReply);
            const seqs = h.calls
                .filter((c) => c.method === "DescribeWorld")
                .map((c) => JSON.parse(c.payload) as Record<string, unknown>)
                .filter((req) => req["command"] !== null)
                .map((req) => req["cmdSeq"] as string);
            assert.equal(new Set(seqs).size, seqs.length, "sent head never replays");
        } finally {
            await h.stop();
        }
    });
});
