// Aligned whole-workspace migration end to end through the production
// entry against the real `WorldEngine` (via the test-only `world_eval`
// example with `WORLD_EVAL_ALIGNED=1`: same decode, retained engine,
// codec and generation binding the shipped KDE world service uses; only
// D-Bus and signals are stubbed). No hand-built wire replies on the
// asserted paths: every reply is a fresh Engine report.
//
// Two outputs share one native grid (ws-1..ws-4); out-a shows ws-1
// (win-a, win-c), out-b shows ws-2 (win-b). Migrating out-a right lands
// the moved logical unit in the out-b trailing ws-3 (D5 reading B keeps
// tree/order/shares/focus/tiling, only the native changes), retains the
// explicit core refill op-*-refill to ws-4, and the adapter actuates both
// views plus all members before focus (D6): native member moves, target
// follow, source refill, settlement with no replay, and stable logical
// rebinding across the move. Included in `npm test`.

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
                // Test-only platform selection mirroring the KDE service
                // construction (never a user setting).
                env: { ...process.env, WORLD_EVAL_ALIGNED: "1" },
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

const NATIVES = ["ws-1", "ws-2", "ws-3", "ws-4"];

async function makeHarness(): Promise<Harness> {
    const desktops: FakeDesktop[] = NATIVES.map((id) => ({ id, name: id }));
    const byId = new Map(desktops.map((d) => [d.id, d]));
    const screens = [
        { name: "out-a", geometry: { x: 0, y: 0, width: 800, height: 600 } },
        { name: "out-b", geometry: { x: 800, y: 0, width: 800, height: 600 } },
    ];
    const currents = new Map([["out-a", "ws-1"], ["out-b", "ws-2"]]);
    const mkWin = (id: string, out: string, ws: string, x: number): FakeWindow => ({
        internalId: id,
        normalWindow: true,
        frameGeometry: { x, y: 0, width: 200, height: 200 },
        output: { name: out },
        desktops: [byId.get(ws) as FakeDesktop],
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
    });
    const wins: FakeWindow[] = [
        mkWin("win-a", "out-a", "ws-1", 0),
        mkWin("win-c", "out-a", "ws-1", 210),
        mkWin("win-b", "out-b", "ws-2", 0),
    ];
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
        activeScreen: { name: "out-a" },
        activeRef: wins[0] as FakeWindow,
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
        activeScreenChanged: (signals["activeScreenChanged"] as FakeSignal).signal,
        get activeWindow(): FakeWindow | null {
            return harness.activeRef;
        },
        set activeWindow(ref: unknown) {
            const found = harness.wins.find((win) => (win as unknown) === ref);
            if (found !== undefined) {
                harness.activeRef = found;
            }
        },
        clientArea: (): Record<string, number> => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }): FakeDesktop =>
            desktopById(harness.currents.get(screen.name) ?? ""),
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }): void => {
            const id = (deskRef as FakeDesktop).id;
            assert.ok(byId.has(id), `desktop ${id} exists`);
            harness.currents.set(screen.name, id);
            harness.viewWrites.push(`${screen.name}->${id}`);
        },
        sendClientToScreen: (mover: unknown, out: unknown): void => {
            const win = mover as FakeWindow;
            const name = (out as { name: string }).name;
            win.output = { name };
            harness.outputWrites.push(`${win.internalId}->${name}`);
        },
        createDesktop: (): null => null,
        removeDesktop: (): boolean => false,
    };
    for (const win of wins) {
        let desks = win.desktops;
        Object.defineProperty(win, "desktops", {
            configurable: true,
            get: () => desks,
            set: (refs: FakeDesktop[]) => {
                desks = refs;
                harness.membershipWrites.push(`${win.internalId}->${refs.map((d) => d.id).join("+")}`);
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

function lastTakeResult(h: Harness): Record<string, unknown> {
    const readys = h.takeReplies
        .map((raw) => JSON.parse(raw) as Record<string, unknown>)
        .filter((r) => r["outcome"] === "ready");
    assert.ok(readys.length > 0, "at least one ready take result");
    return (readys[readys.length - 1] as Record<string, unknown>)["result"] as Record<string, unknown>;
}

function lastReply(h: Harness): Record<string, unknown> {
    const indices = [...h.replies.keys()].filter((at) => (h.calls[at] as CapturedCall).method === "DescribeWorld");
    assert.ok(indices.length > 0, "at least one answered DescribeWorld");
    const raw = h.replies.get(Math.max(...indices) as number) as string;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    assert.equal(parsed["outcome"] ?? "accepted", "accepted", `fresh reply accepted, got ${raw.slice(0, 200)}`);
    return parsed;
}

function transfersOf(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return (reply["transfers"] as Array<Record<string, unknown>>) ?? [];
}

// One lag direction through the real Engine and production apply:
// migrate, let the native actuation land, then rewind both currents to
// the pre-move views and report members-arrived-but-views-old. The fresh
// Engine reply must retain (never prematurely settle) both transfers;
// re-actuation then restores the views and a final settle converges.
async function lagDirection(options: {
    activeOut: string;
    activeWin: string;
    direction: string;
    targetOut: string;
    targetWs: string;
    refillOut: string;
    refillWs: string;
    oldTargetView: string;
    oldSourceView: string;
}): Promise<void> {
    const h = await makeHarness();
    try {
        await discover(h);
        h.fire("desktopsChanged");
        await h.settle();
        const active = h.wins.find((w) => w.internalId === options.activeWin) as FakeWindow;
        h.activeScreen = { name: options.activeOut };
        h.activeRef = active;
        h.handle.requestWorkspaceMigrate(options.direction);
        h.runTimers();
        await h.flushAll();
        // Native actuation landed: members moved, both views followed.
        assert.equal(h.currents.get(options.targetOut), options.targetWs, "target view follows");
        assert.equal(h.currents.get(options.refillOut), options.refillWs, "source refills");
        // Rewind both views: members stay arrived, views read old.
        h.currents.set(options.targetOut, options.oldTargetView);
        h.currents.set(options.refillOut, options.oldSourceView);
        h.fire("currentDesktopChanged");
        h.fire("desktopsChanged");
        h.runTimers();
        const lagIndex = await h.flush();
        const lagged = JSON.parse(h.replies.get(lagIndex) as string) as Record<string, unknown>;
        assert.equal(lagged["outcome"] ?? "accepted", "accepted");
        const held = transfersOf(lagged);
        assert.ok(
            held.some((t) => t["kind"] === "migration"),
            `members arrived but views old retains the migration: ${JSON.stringify(held)}`,
        );
        assert.ok(
            held.some((t) => t["kind"] === "view"),
            `members arrived but views old retains the refill: ${JSON.stringify(held)}`,
        );
        // The held reply re-actuates both views; the next round settles.
        try {
            await h.settle();
        } catch (e) {
            const last = lastReply(h);
            console.log("SETTLE-DEBUG calls=" + String(h.calls.length) + " transfers=" + JSON.stringify(transfersOf(last)) + " currents=" + JSON.stringify(Object.fromEntries(h.currents)) + " wins=" + JSON.stringify(h.wins.map((w) => [w.internalId, w.output.name, w.desktops.map((d) => d.id)])) + " viewWrites=" + JSON.stringify(h.viewWrites.slice(-10)));
            throw e;
        }
        assert.deepEqual(transfersOf(lastReply(h)), [], "members plus both views settle");
    } finally {
        await h.stop();
    }
}

describe("world aligned migration engine", () => {
    it("migrates through fresh Engine replies with replicated bindings, refill and settlement", { timeout: 120000 }, async () => {
        const h = await makeHarness();
        try {
            await discover(h);
            // One more observation round so the adapter snapshot converges
            // the core-owned mapping from the first accepted reply.
            h.fire("desktopsChanged");
            await h.settle();
            // The core-owned workspace rows bind each native once per
            // output with a distinct logical identity per cell (id IS the
            // logical id in v2, no `logical` key).
            const settled = lastReply(h);
            const settledRows = settled["workspaces"] as Array<Record<string, string>>;
            const ws1 = settledRows.filter((b) => b["bind"] === "ws-1");
            assert.equal(ws1.length, 2, "ws-1 binds once per output");
            assert.notEqual(ws1[0]?.["id"], ws1[1]?.["id"], "distinct logical identity per cell");
            const sourceLogicalBefore = (ws1.find((b) => b["out"] === "out-a") as Record<string, string>)["id"];
            assert.deepEqual(settled["transfers"], [], "no transfers before the command");

            // Migrate the active workspace (out-a/ws-1) right.
            h.handle.requestWorkspaceMigrate("right");
            await h.settle();
            const migrateCalls = h.calls
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
                });
            assert.equal(migrateCalls.length, 1, "one directional migration command, never replayed");
            // The drained capture is the first DescribeWorld after the queue.
            const drainIndex = h.calls.findIndex(
                (call, at) => at > (migrateCalls[0]?.index as number) && call.method === "DescribeWorld",
            );
            assert.ok(drainIndex >= 0, "queued command drains on a fresh capture");
            const migrateReply = JSON.parse(h.replies.get(drainIndex) as string) as Record<string, unknown>;
            assert.equal(migrateReply["result"], null, "actuator lane never carries the terminal result");
            const result = lastTakeResult(h);
            assert.equal(result["op"], "migrate-workspace");
            assert.equal(result["sourceOut"], "out-a");
            assert.equal(result["targetOut"], "out-b");
            assert.equal(result["targetWs"], "ws-3", "destination trailing empty");
            assert.deepEqual((result["members"] as string[]).sort(), ["win-a", "win-c"]);
            const transfers = migrateReply["transfers"] as Array<Record<string, unknown>>;
            const migration = transfers.find((t) => t["kind"] === "migration");
            assert.ok(migration !== undefined, "migration transfer retained");
            assert.equal(migration?.["out"], "out-b");
            assert.equal(migration?.["ws"], "ws-3");
            const refill = transfers.find((t) => t["kind"] === "view");
            assert.ok(refill !== undefined, "explicit core refill retained (reading B)");
            assert.match(refill?.["op"] as string, /-refill$/);
            assert.equal(refill?.["out"], "out-a");
            assert.equal(refill?.["ws"], "ws-4", "moved ws-1 and its fresh replacement excluded");

            // Native actuation: both members remap output and rebind to the
            // trailing; both views follow exactly once each.
            const winA = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            const winC = h.wins.find((w) => w.internalId === "win-c") as FakeWindow;
            assert.equal(winA.output.name, "out-b");
            assert.equal(winC.output.name, "out-b");
            assert.deepEqual(winA.desktops.map((d) => d.id), ["ws-3"]);
            assert.deepEqual(winC.desktops.map((d) => d.id), ["ws-3"]);
            assert.equal(h.currents.get("out-b"), "ws-3", "target view follows");
            assert.equal(h.currents.get("out-a"), "ws-4", "source refills to the core choice");
            assert.deepEqual(
                h.viewWrites.filter((c) => c === "out-b->ws-3"),
                ["out-b->ws-3"],
                "target view switches exactly once",
            );
            assert.deepEqual(
                h.viewWrites.filter((c) => c === "out-a->ws-4"),
                ["out-a->ws-4"],
                "refill view switches exactly once",
            );
            assert.equal(h.outputWrites.filter((c) => c === "win-a->out-b").length, 1);
            assert.equal(h.outputWrites.filter((c) => c === "win-c->out-b").length, 1);

            // Settlement: the converged Engine reply carries no transfers,
            // homings match, and the logical unit rode to the destination
            // trailing while the source cell minted fresh.
            const finalReply = lastReply(h);
            assert.deepEqual(finalReply["transfers"], [], "all transfers settle");
            const finalRows = finalReply["workspaces"] as Array<Record<string, string>>;
            const moved = finalRows.find((b) => b["out"] === "out-b" && b["bind"] === "ws-3");
            assert.equal(moved?.["id"], sourceLogicalBefore, "logical identity rides the move");
            const sourceFresh = finalRows.find((b) => b["out"] === "out-a" && b["bind"] === "ws-1");
            assert.ok(
                sourceFresh !== undefined && sourceFresh["id"] !== sourceLogicalBefore,
                "emptied source cell mints fresh",
            );
            // No replay anywhere: one command, bounded native calls.
            assert.equal(migrateCalls.length, 1);
            assert.ok(!h.logs.some((l) => l.includes("command-busy")), "no busy refills");
        } finally {
            await h.stop();
        }
    });

    it("holds settlement while members arrived but views lag, both directions", { timeout: 180000 }, async () => {
        // Right: out-a/ws-1 (win-a, win-c) lands in the out-b trailing ws-3,
        // refill out-a/ws-4. Views rewind to ws-2/ws-1 for the lagged round.
        await lagDirection({
            activeOut: "out-a",
            activeWin: "win-a",
            direction: "right",
            targetOut: "out-b",
            targetWs: "ws-3",
            refillOut: "out-a",
            refillWs: "ws-4",
            oldTargetView: "ws-2",
            oldSourceView: "ws-1",
        });
        // Left: out-b/ws-2 (win-b) lands in the out-a trailing ws-2 across
        // outputs, refill out-b/ws-4. Views rewind to ws-1/ws-2.
        await lagDirection({
            activeOut: "out-b",
            activeWin: "win-b",
            direction: "left",
            targetOut: "out-a",
            targetWs: "ws-2",
            refillOut: "out-b",
            refillWs: "ws-4",
            oldTargetView: "ws-1",
            oldSourceView: "ws-2",
        });
    });
});
