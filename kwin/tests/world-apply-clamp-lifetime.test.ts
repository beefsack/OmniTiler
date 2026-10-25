// Core-owned frame suppression through the production route: the real
// `WorldEngine` (via the test-only `world_eval` example) behind the
// production `startWorldAdapterEntry`. A clamped native window (setter
// reports accepted but the live frame never lands) keeps a Controlled
// tiled desire with a mismatch, so the adapter writes every round with
// no local retry budget; the core pins the clamp from the recorded
// attempt plus staged facts and publishes a `constraint-clamp` hold with
// the desire intact, and the adapter stops. A genuine change (an external
// move) lifts suppression core-side: the desire returns to Controlled,
// the adapter writes again, and the hold re-pins. Held scopes never take
// writes. Included in `npm test`.
//
// REAL parts: production `startWorldAdapterEntry` on a scripted fake KWin
// surface; retained `WorldEngine` evaluation per DescribeWorld round-trip.
// The per-test `world_eval` process is the replay boundary.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";

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
    readonly screen: FakeScreen;
    readonly wins: FakeWindow[];
    readonly victim: FakeWindow;
    readonly calls: CapturedCall[];
    readonly answered: Set<number>;
    readonly replies: Map<number, string>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean; fired: boolean }>;
    readonly logs: string[];
    readonly geometryWrites: string[];
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    readonly signals: Record<string, FakeSignal>;
    fire: (name: string, payload?: unknown) => void;
    runTimers: (maxDelay?: number) => void;
    flush: () => Promise<number>;
    flushAll: () => Promise<void>;
    stop: () => Promise<void>;
}

async function makeHarness(): Promise<Harness> {
    const screen: FakeScreen = { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 600 } };
    const ws: FakeDesktop = { id: "ws-1", name: "WS 1" };
    const liveDesktops: FakeDesktop[] = [ws];
    const mkWin = (
        internalId: string,
        frame: { x: number; y: number; width: number; height: number },
    ): FakeWindow => ({
        internalId,
        normalWindow: true,
        frameGeometry: { ...frame },
        output: screen,
        desktops: [ws],
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
    });
    // The victim starts far from any tiled slot; the sibling starts near
    // its slot so only the victim exercises the clamp lane.
    const wins: FakeWindow[] = [
        mkWin("victim", { x: 10, y: 10, width: 100, height: 100 }),
        mkWin("sibling", { x: 400, y: 0, width: 392, height: 600 }),
    ];
    const victim = wins[0] as FakeWindow;
    const geometryWrites: string[] = [];
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
        screen,
        wins,
        victim,
        calls: [],
        answered: new Set<number>(),
        replies: new Map<number, string>(),
        timers: [],
        logs: [],
        geometryWrites,
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
        stop: async (): Promise<void> => {
            try {
                harness.handle?.stop();
            } catch (error) {
                void error;
            }
            await harness.bridge.close();
        },
    };
    let activeRef: FakeWindow | null = wins[1] as FakeWindow;
    const surface: Record<string, unknown> = {
        windowList: (): FakeWindow[] => [...harness.wins],
        screens: [screen],
        desktops: liveDesktops,
        get activeScreen(): FakeScreen {
            return (activeRef?.output as FakeScreen | undefined) ?? screen;
        },
        cursorPos: { x: 400, y: 300 },
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
            }
        },
        get currentDesktop(): FakeDesktop {
            return ws;
        },
        set currentDesktop(ref: unknown) {
            void ref;
        },
        clientArea: (_kind: number, _output: unknown, _desktop: unknown): Record<string, number> => ({ x: 0, y: 0, w: 800, h: 600 }),
        currentDesktopForScreen: (): FakeDesktop => ws,
        setCurrentDesktopForScreen: (): void => undefined,
        sendClientToScreen: (): void => undefined,
        createDesktop: (): void => undefined,
        removeDesktop: (): boolean => false,
    };
    // Native setters: the sibling converges; the victim is clamped (the
    // setter reports accepted but the live frame never lands). Every
    // native write is recorded with its target.
    for (const win of wins) {
        let frame = win.frameGeometry;
        const clamped = win.internalId === "victim";
        Object.defineProperty(win, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                harness.geometryWrites.push(`${win.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                if (!clamped) {
                    frame = rect;
                }
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
                callback(harness.broker.answerTake(payload));
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
}

function lastReply(h: Harness): Record<string, unknown> {
    assert.ok(h.replies.size > 0, "at least one reply flushed");
    const last = Math.max(...h.replies.keys());
    return JSON.parse(h.replies.get(last) as string) as Record<string, unknown>;
}

function victimRow(h: Harness): Record<string, unknown> | undefined {
    const reply = lastReply(h);
    const windows = reply["windows"] as Array<Record<string, unknown>>;
    return windows.find((w) => w["win"] === "victim");
}

function victimWrites(h: Harness): number {
    return h.geometryWrites.filter((w) => w.startsWith("victim@")).length;
}

async function round(h: Harness): Promise<void> {
    h.fire("desktopsChanged");
    h.runTimers();
    await h.flushAll();
}

describe("core-owned tiled clamp suppression through the production route (real Engine)", () => {
    it("writes while Controlled, stops on the core clamp hold, and resumes on genuine change", { timeout: 180000 }, async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            // Converge the bootstrap: the sibling lands in its slot; the
            // victim cannot (clamped), so the core keeps a Controlled
            // desire with a mismatch and the adapter keeps writing with
            // no local budget of its own.
            for (let i = 0; i < 6; i += 1) {
                await round(h);
            }
            let row = victimRow(h);
            assert.ok(row !== undefined, "core publishes a tiled desire for the victim");
            const writesBeforeHold = victimWrites(h);
            assert.ok(writesBeforeHold >= 1, `adapter writes the Controlled mismatch (no local suppression), got ${writesBeforeHold}`);
            // Drive until the core pins the clamp from the recorded
            // attempts plus staged facts and publishes the hold with the
            // desire intact.
            let heldRow: Record<string, unknown> | undefined;
            for (let i = 0; i < 12; i += 1) {
                await round(h);
                const current = victimRow(h);
                if (current !== undefined && current["frame_auth"] === "hold" && current["frame_hold"] === "constraint-clamp") {
                    heldRow = current;
                    break;
                }
            }
            assert.ok(heldRow !== undefined, `core publishes the constraint-clamp hold with the desire intact: ${JSON.stringify(victimRow(h))}`);
            assert.ok((heldRow["frame"] as unknown) !== null || (heldRow["alloc"] as unknown) !== null, "held row keeps the desired frame");
            // Held scope takes no writes: a full round changes nothing.
            const writesAtHold = victimWrites(h);
            await round(h);
            assert.equal(victimWrites(h), writesAtHold, "held scope writes nothing");
            const stillHeld = victimRow(h);
            assert.equal(stillHeld?.["frame_auth"], "hold", "hold persists while the pair is unchanged");
            assert.equal(stillHeld?.["frame_hold"], "constraint-clamp");
            // Genuine change lifts suppression core-side: an external move
            // of the victim re-arms the pair, the desire returns to
            // Controlled, and the adapter writes again. The mismatched
            // readback then re-pins the hold.
            const stored = (h.victim.frameGeometry as { x: number; y: number; width: number; height: number });
            const moved = { x: stored.x + 40, y: stored.y + 40, width: stored.width, height: stored.height };
            // Bypass the clamped setter: this is external native input,
            // not an adapter write.
            Object.defineProperty(h.victim, "frameGeometry", {
                configurable: true,
                get: () => ({ ...moved }),
                set: (rect: { x: number; y: number; width: number; height: number }) => {
                    h.geometryWrites.push(`victim@${rect.x},${rect.y},${rect.width},${rect.height}`);
                    return true;
                },
            });
            await round(h);
            assert.ok(victimWrites(h) > writesAtHold, "genuine change resumes adapter writes (core lifted suppression)");
            let repinned = false;
            for (let i = 0; i < 12; i += 1) {
                await round(h);
                const current = victimRow(h);
                if (current !== undefined && current["frame_auth"] === "hold" && current["frame_hold"] === "constraint-clamp") {
                    repinned = true;
                    break;
                }
            }
            assert.ok(repinned, "stable mismatch re-pins the core hold; the adapter never suppresses locally");
            const writesAtRepin = victimWrites(h);
            await round(h);
            assert.equal(victimWrites(h), writesAtRepin, "re-pinned hold writes nothing again");
        } finally {
            await h.stop();
        }
    });
});
