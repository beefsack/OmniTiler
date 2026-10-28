// KDE D7 stable fixed-client explicit tile after workspace enable: zero vs
// TWO intervening observes through the REAL retained WorldEngine (NOT mocked
// Planner replies). One output + ordinary peer + stable fixed client; native
// size hints equal finite min/max for fixed, Qt zero/unbounded for ordinary;
// fresh wrappers per enumerate, QML array-likes, braced live ids with bare
// ids on the wire, real KWin properties. Starts workspace floating, enables
// through the production command in exactly ONE report, then explicitly tiles
// (toggle-float) the fixed client with 0/2 independent observes. No silent
// extra report rounds between enable and the immediate command: exact
// intervening command/report counts are asserted, and the enable/tile replies
// pin the exact window set plus logical/native bindings (never type-only).
// Every test emits its complete unsliced calls/reports/replies/takes/store
// trace via test-context diagnostics (success included; no repo artifact).
// If a complete-report case refuses, the test stops with the exact trace (no
// production fix here). The incomplete case is an observe-only held report,
// never an explicit-tile or partial refusal; exact refusal shapes stay with
// separate core tests.

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";
import { describe, it, type TestContext } from "node:test";

import { startWorldAdapterEntry } from "../src/world-entry";
import { createMiniBroker, type MiniBroker } from "./world-wire-harness";

const WIN_O = "11111111-1111-1111-1111-111111111111";
const WIN_F = "44444444-4444-4444-4444-444444444444";
const OUT = "out-1";
const WS_NATIVE = "ws-a";

const QT_ZERO = { width: 0, height: 0 };
const QT_UNBOUNDED = { width: 16777215, height: 16777215 };
const FIXED_HINT = { width: 480, height: 320 };

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
            this.failAll(new Error(`raw_world_eval exited code=${String(code)} signal=${String(signal)} tail=${this.stderrTail.slice(-2000)}`));
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

const qmlLike = (items: ReadonlyArray<unknown>): object => {
    const o: Record<string, unknown> = { length: items.length };
    items.forEach((item, index) => {
        o[String(index)] = item;
    });
    return o;
};

interface LiveSpec {
    braced: string;
    ws: string;
    frame: { x: number; y: number; width: number; height: number };
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minSize: { width: number; height: number };
    maxSize: { width: number; height: number };
}

interface CapturedCall {
    readonly method: string;
    readonly payload: string;
    readonly callback: (reply: unknown) => void;
}

interface FlushedRound {
    readonly index: number;
    readonly envelope: string;
    readonly stamped: string;
    readonly raw: string;
    readonly bound: string;
    readonly parsed: Record<string, unknown>;
}

interface Harness {
    readonly bridge: WorldBridge;
    readonly broker: MiniBroker;
    workspace: Record<string, unknown>;
    readonly calls: CapturedCall[];
    readonly answered: Set<number>;
    readonly replies: Map<number, string>;
    readonly rounds: FlushedRound[];
    readonly takeReplies: string[];
    readonly storeWrites: string[];
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean; fired: boolean }>;
    readonly logs: string[];
    readonly geometryWrites: string[];
    readonly membershipWrites: string[];
    readonly viewSwitches: string[];
    readonly handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
    readonly signals: Record<string, FakeSignal>;
    readonly live: Map<string, LiveSpec>;
    setActive: (braced: string) => void;
    fire: (name: string, payload?: unknown) => void;
    runTimers: (maxDelay?: number) => void;
    flushOne: () => Promise<FlushedRound>;
    flushAll: () => Promise<void>;
    queueBodies: () => Array<{ index: number; body: Record<string, unknown> }>;
    stop: () => Promise<void>;
}

function termSpec(
    bare: string,
    frame: { x: number; y: number; width: number; height: number },
    extra: Partial<LiveSpec> = {},
): LiveSpec {
    return {
        braced: `{${bare}}`,
        ws: WS_NATIVE,
        frame,
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: { ...QT_ZERO },
        maxSize: { ...QT_UNBOUNDED },
        ...extra,
    };
}

async function makeHarness(): Promise<Harness> {
    const live = new Map<string, LiveSpec>();
    const seedO = termSpec(WIN_O, { x: 0, y: 0, width: 400, height: 600 });
    const seedF = termSpec(WIN_F, { x: 100, y: 100, width: 480, height: 320 }, {
        minSize: { ...FIXED_HINT },
        maxSize: { ...FIXED_HINT },
    });
    live.set(seedO.braced, seedO);
    live.set(seedF.braced, seedF);
    let activeBraced: string | null = seedO.braced;
    const geometryWrites: string[] = [];
    const membershipWrites: string[] = [];
    const viewSwitches: string[] = [];
    const storeWrites: string[] = [];
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
        calls: [],
        answered: new Set<number>(),
        replies: new Map<number, string>(),
        rounds: [],
        takeReplies: [],
        storeWrites,
        timers: [],
        logs: [],
        geometryWrites,
        membershipWrites,
        viewSwitches,
        handle: null as unknown as NonNullable<ReturnType<typeof startWorldAdapterEntry>>,
        signals,
        live,
        setActive: (braced: string): void => {
            activeBraced = braced;
        },
        fire: (name: string, payload?: unknown): void => {
            for (const handler of [...(signals[name] as FakeSignal).handlers]) {
                handler(payload);
            }
        },
        runTimers: (maxDelay = 120): void => {
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
        flushOne: async (): Promise<FlushedRound> => {
            const index = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
            assert.ok(index >= 0, `expected a queued DescribeWorld dispatch (queued=${harness.calls.length} answered=${harness.answered.size})`);
            const envelope = (harness.calls[index] as CapturedCall).payload;
            const stamped = harness.broker.stampCapture(envelope);
            const raw = await harness.bridge.send("DescribeWorld", stamped);
            const bound = harness.broker.bindReply(raw, envelope);
            harness.answered.add(index);
            harness.replies.set(index, bound);
            const parsed = JSON.parse(bound) as Record<string, unknown>;
            const round: FlushedRound = { index, envelope, stamped, raw, bound, parsed };
            harness.rounds.push(round);
            (harness.calls[index] as CapturedCall).callback(bound);
            return round;
        },
        flushAll: async (): Promise<void> => {
            for (let round = 0; round < 16; round += 1) {
                const next = harness.calls.findIndex((call, at) => call.method === "DescribeWorld" && !harness.answered.has(at));
                if (next < 0) {
                    return;
                }
                await harness.flushOne();
            }
            throw new Error("flushAll did not converge: planner keeps dispatching");
        },
        queueBodies: (): Array<{ index: number; body: Record<string, unknown> }> => {
            const out: Array<{ index: number; body: Record<string, unknown> }> = [];
            harness.calls.forEach((call, index) => {
                if (call.method !== "QueueWorldCommand") {
                    return;
                }
                try {
                    out.push({ index, body: JSON.parse(call.payload) as Record<string, unknown> });
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
    const wrap = (spec: LiveSpec): object => {
        const w: Record<string, unknown> = {
            internalId: spec.braced,
            managed: true,
            deleted: false,
            transient: false,
            transientFor: null,
            dialog: false,
            modal: false,
            wantsInput: true,
            normalWindow: true,
            output: { name: OUT },
            desktops: qmlLike([{ id: spec.ws }]),
            minimized: spec.minimized,
            hidden: false,
            fullscreen: spec.fullScreen,
            maximized: spec.maximizeMode !== 0,
            sticky: spec.onAllDesktops,
            keepAbove: false,
            keepBelow: false,
            minSize: { ...spec.minSize },
            maxSize: { ...spec.maxSize },
            fullScreen: spec.fullScreen,
            maximizeMode: spec.maximizeMode,
            onAllDesktops: spec.onAllDesktops,
        };
        let frame = { ...spec.frame };
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => ({ ...frame }),
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = { ...rect };
                const cur = live.get(spec.braced);
                if (cur !== undefined) {
                    cur.frame = { ...rect };
                }
                geometryWrites.push(`${spec.braced}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
        return w;
    };
    const nativeDesktops = [{ id: WS_NATIVE }];
    const surface: Record<string, unknown> = {
        windowList: (): object => qmlLike([...live.values()].map((s) => wrap(s))),
        get screens() {
            return qmlLike([{ name: OUT, geometry: { x: 0, y: 0, width: 800, height: 600 } }]);
        },
        get desktops() {
            return qmlLike(nativeDesktops.map((d) => ({ ...d })));
        },
        activeScreen: { name: OUT },
        get activeWindow() {
            return activeBraced === null ? null : { internalId: activeBraced };
        },
        currentDesktop: { id: WS_NATIVE },
        currentDesktopForScreen: (_screen: unknown) => ({ id: WS_NATIVE }),
        setCurrentDesktopForScreen: (desk: unknown, screen: unknown): void => {
            viewSwitches.push(`${(screen as { name: string }).name}->${(desk as { id: string }).id}`);
        },
        sendClientToScreen: (client: unknown, screen: unknown): void => {
            membershipWrites.push(`send:${String((client as { internalId?: unknown }).internalId)}=>${(screen as { name: string }).name}`);
        },
        createDesktop: (): null => null,
        removeDesktop: (): boolean => false,
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        activeScreenChanged: (signals["activeScreenChanged"] as FakeSignal).signal,
    };
    harness.workspace = surface;
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        floatIntent: true,
        callDbus: (_service, _path, _iface, method, payload, callback): void => {
            if (method === "ReadFloatIntent") {
                callback(JSON.stringify({ v: 1, correlation_id: "float-seed", members: [], stored: 0, returned: 0, tile: [], tile_stored: 0, tile_returned: 0 }));
                return;
            }
            if (method === "WriteFloatIntent") {
                storeWrites.push(payload);
                return;
            }
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
        readDefaultTiledFn: () => false,
    });
    assert.notEqual(handle, null, "world production entry starts on the fake surface");
    (harness as { handle: unknown }).handle = handle;
    return harness;
}

async function discover(h: Harness): Promise<string> {
    const discIndex = h.calls.findIndex((call) => call.method === "DiscoverWorld");
    assert.ok(discIndex >= 0, "entry discovers on start");
    const disc = h.calls[discIndex] as CapturedCall;
    const engineBootstrap = await h.bridge.send(disc.method, disc.payload);
    const reply = h.broker.bindDiscovery(engineBootstrap);
    const parsed = JSON.parse(reply) as Record<string, unknown>;
    assert.equal(parsed["ready"], false, "bootstrap is not ready");
    assert.ok(typeof parsed["gen"] === "string" && (parsed["gen"] as string).length > 0, "bootstrap binds a generation");
    disc.callback(reply);
    const gen = h.broker.gen;
    assert.ok(gen.length > 0, "generation bound");
    return gen;
}

function queueOpOf(body: Record<string, unknown>): string {
    const command = body["command"] as Record<string, unknown>;
    const nested = command["command"] as Record<string, unknown> | undefined;
    if (nested !== undefined && typeof nested["op"] === "string") {
        return `operation:${nested["op"] as string}`;
    }
    return String(command["op"]);
}

function countQueueOp(h: Harness, op: string): number {
    return h.queueBodies().filter((q) => queueOpOf(q.body) === op).length;
}

function describeCount(h: Harness): number {
    return h.calls.filter((c) => c.method === "DescribeWorld").length;
}

function lastReadyTake(h: Harness): Record<string, unknown> | null {
    const readys = h.takeReplies
        .map((raw) => JSON.parse(raw) as Record<string, unknown>)
        .filter((r) => r["outcome"] === "ready");
    if (readys.length === 0) {
        return null;
    }
    return (readys[readys.length - 1] as Record<string, unknown>)["result"] as Record<string, unknown> | null;
}

function fullTrace(h: Harness): Record<string, unknown> {
    return {
        calls: h.calls.map((c) => ({ method: c.method, payload: c.payload })),
        rounds: h.rounds.map((r) => ({ index: r.index, envelope: r.envelope, stamped: r.stamped, raw: r.raw, bound: r.bound })),
        takeReplies: [...h.takeReplies],
        storeWrites: [...h.storeWrites],
        queues: h.queueBodies(),
        geometryWrites: [...h.geometryWrites],
        membershipWrites: [...h.membershipWrites],
        viewSwitches: [...h.viewSwitches],
        logs: [...h.logs],
    };
}

function emitTrace(t: TestContext, h: Harness, label: string): void {
    t.diagnostic(`${label} full trace: ${JSON.stringify(fullTrace(h))}`);
}

function failWithTrace(t: TestContext, h: Harness, label: string, round: FlushedRound, extra: Record<string, unknown> = {}): never {
    emitTrace(t, h, label);
    const parsed = round.parsed;
    assert.fail(
        `${label}: kind=${String(parsed["kind"] ?? "")} outcome=${String(parsed["outcome"] ?? "")} ready=${String(parsed["ready"] ?? "")} `
        + `round=${round.index} extra=${JSON.stringify(extra)} (complete calls/reports/replies/takes in the test diagnostics above)`,
    );
}

function assertBoundTransport(round: FlushedRound, envelope: Record<string, unknown>): void {
    const parsed = round.parsed;
    assert.equal(parsed["source"], envelope["source"], `bound reply carries the capture source (round ${round.index})`);
    assert.ok(typeof parsed["clock"] === "string", `bound reply carries the dispatch clock (round ${round.index})`);
    assert.ok("nativeInvalid" in parsed, `bound reply carries the native watermark (round ${round.index})`);
    assert.equal(parsed["result"], null, `terminal result rides the take lane only, never the capture reply (round ${round.index})`);
}

function assertExactBindings(h: Harness, round: FlushedRound, label: string): string {
    const parsed = round.parsed;
    const wins = parsed["windows"] as Array<Record<string, unknown>>;
    assert.deepEqual(wins.map((w) => w["win"]).sort(), [WIN_F, WIN_O].sort(), `${label}: reply carries exactly the known window set`);
    const rows = parsed["workspaces"] as Array<Record<string, unknown>>;
    const boundRows = rows.filter((r) => r["out"] === OUT && r["bind"] === WS_NATIVE);
    assert.equal(boundRows.length, 1, `${label}: exactly one workspace row binds the native desktop`);
    const logical = (boundRows[0] as Record<string, unknown>)["id"] as string;
    assert.ok(logical.length > 0, `${label}: bound logical id is non-empty`);
    for (const w of wins) {
        assert.equal(w["out"], OUT, `${label}: ${String(w["win"])} homed on the single output`);
        assert.equal(w["ws"], logical, `${label}: ${String(w["win"])} homed on the bound logical through the core binding`);
    }
    const scopes = parsed["scopes"] as Array<Record<string, unknown>>;
    const scope = scopes.find((s) => s["scope"] === `${OUT}/${logical}`);
    assert.ok(scope !== undefined, `${label}: a scope names the bound logical`);
    assert.equal(scope["auth"], "controlled", `${label}: the bound scope is controlled`);
    for (const bare of [WIN_O, WIN_F]) {
        assert.equal(h.live.get(`{${bare}}`)?.ws, WS_NATIVE, `${label}: native membership of ${bare} homed on the workspace native`);
    }
    return logical;
}

function assertEnablePrecondition(h: Harness, round: FlushedRound): string {
    const logical = assertExactBindings(h, round, "enable");
    const wins = round.parsed["windows"] as Array<Record<string, unknown>>;
    const fixedRow = wins.find((w) => w["win"] === WIN_F) as Record<string, unknown>;
    const peerRow = wins.find((w) => w["win"] === WIN_O) as Record<string, unknown>;
    assert.equal(fixedRow["floating"], true, "enable auto-floats the stable fixed client (precondition for explicit tile)");
    assert.equal(peerRow["floating"], false, "enable keeps the ordinary peer tiled");
    const envelope = JSON.parse(round.envelope) as Record<string, unknown>;
    const report = envelope["report"] as Record<string, unknown>;
    const reported = report["windows"] as Array<Record<string, unknown>>;
    assert.deepEqual(reported.map((w) => w["win"]).sort(), [WIN_F, WIN_O].sort(), "enable producer report names exactly the two windows");
    const repFixed = reported.find((w) => w["win"] === WIN_F) as Record<string, unknown>;
    const repPeer = reported.find((w) => w["win"] === WIN_O) as Record<string, unknown>;
    assert.deepEqual(repFixed["minSize"], { ...FIXED_HINT }, "producer carries the finite fixed minimum");
    assert.deepEqual(repFixed["maxSize"], { ...FIXED_HINT }, "producer carries the finite fixed maximum");
    assert.deepEqual(repPeer["minSize"], { ...QT_ZERO }, "producer carries the Qt zero minimum for the ordinary peer");
    assert.deepEqual(repPeer["maxSize"], { ...QT_UNBOUNDED }, "producer carries the Qt unbounded maximum for the ordinary peer");
    assert.deepEqual(repFixed["desktops"], [WS_NATIVE], "fixed producer membership readable on the native desktop");
    assert.deepEqual(repPeer["desktops"], [WS_NATIVE], "peer producer membership readable on the native desktop");
    const fixedFrame = repFixed["frameGeometry"] as Record<string, unknown>;
    for (const key of ["x", "y", "width", "height"]) {
        assert.equal(typeof fixedFrame[key], "number", `fixed producer frame ${key} readable`);
    }
    return logical;
}

async function enableWorkspace(h: Harness, t: TestContext): Promise<{ round: FlushedRound }> {
    const tilesBefore = countQueueOp(h, "toggle-workspace-tiling");
    const describesBefore = describeCount(h);
    assert.equal(h.handle.getWorkspaceTilingSnapshot().tiled, false, "precondition: workspace starts floating");
    h.handle.requestWorkspaceTilingToggle();
    h.runTimers();
    const filed = h.queueBodies().filter((q) => queueOpOf(q.body) === "toggle-workspace-tiling");
    assert.equal(filed.length, tilesBefore + 1, `enable files exactly one toggle-workspace-tiling command: ${JSON.stringify(h.queueBodies().map((q) => queueOpOf(q.body)))}`);
    const cmd = (filed[filed.length - 1] as { body: Record<string, unknown> }).body["command"] as Record<string, unknown>;
    assert.equal(cmd["out"], OUT, "enable names the single output");
    // Exactly ONE enable report dispatched: the press queues a single
    // DescribeWorld and the single flush converges tiling. A scheduled
    // automatic observe may remain pending (timers are not run here); the
    // fixture dispatches nothing further before tile.
    assert.equal(describeCount(h) - describesBefore, 1, "enable queues exactly one DescribeWorld report");
    const round = await h.flushOne();
    if (round.parsed["outcome"] === "rejected") {
        failWithTrace(t, h, "enable rejected on a complete report; STOP (no production fix here)", round, { cmd });
    }
    assertBoundTransport(round, JSON.parse(round.envelope) as Record<string, unknown>);
    assert.equal(h.handle.getWorkspaceTilingSnapshot().tiled, true, "enable converges the scope to tiled in the single report");
    // No runTimers here: the accepted enable leaves a scheduled automatic
    // observe pending (geometry/float intent changed). The zero-gap leg
    // issues tile immediately, coalescing that pending observe with the
    // command into exactly one command-bearing capture (asserted in the
    // leg). This pins none dispatched before tile, not that none can ever
    // be scheduled.
    return { round };
}

async function runFixedTileLeg(intervening: 0 | 2, t: TestContext): Promise<void> {
    const h = await makeHarness();
    const label = intervening === 0 ? "zero-gap" : "two-gap";
    try {
        await discover(h);
        h.runTimers();
        await h.flushAll();
        assert.equal(countQueueOp(h, "toggle-workspace-tiling"), 0, "no enable command before the leg");
        const { round: enableRound } = await enableWorkspace(h, t);
        const enableLogical = assertEnablePrecondition(h, enableRound);
        const fixedBraced = `{${WIN_F}}`;
        // Independent observation-only rounds between enable and the
        // immediate tile command (0 or 2 by leg). Each must file no
        // command and stay accepted on a complete report.
        const queuesBeforeGap = h.queueBodies().length;
        const describesBeforeGap = describeCount(h);
        for (let i = 0; i < intervening; i += 1) {
            h.fire("windowActivated");
            h.runTimers();
            const pending = h.calls.findIndex((call, at) => call.method === "DescribeWorld" && !h.answered.has(at));
            assert.ok(pending >= 0, `gap observe ${i}: expected a queued DescribeWorld`);
            const round = await h.flushOne();
            if (round.parsed["outcome"] === "rejected") {
                failWithTrace(t, h, `gap observe ${i} rejected on a complete report; STOP`, round, { intervening });
            }
            assertBoundTransport(round, JSON.parse(round.envelope) as Record<string, unknown>);
        }
        assert.equal(h.queueBodies().length, queuesBeforeGap, `exactly ${intervening} observation-only gap rounds file no command`);
        assert.equal(describeCount(h) - describesBeforeGap, intervening, `exactly ${intervening} intervening DescribeWorld rounds`);
        // Immediate explicit tile with no silent settle in between: exactly
        // one new queue (toggle-float) and exactly one new DescribeWorld
        // queued by the press before the flush below.
        const queuesBeforeTile = h.queueBodies().length;
        const describesBeforeTile = describeCount(h);
        h.setActive(fixedBraced);
        h.handle.requestFloat();
        h.runTimers();
        const newQueues = h.queueBodies().slice(queuesBeforeTile);
        assert.equal(newQueues.length, 1, `immediate tile files exactly one command, got ${JSON.stringify(newQueues.map((q) => queueOpOf(q.body)))}`);
        assert.equal(queueOpOf((newQueues[0] as { body: Record<string, unknown> }).body), "operation:toggle-float", "immediate command is the explicit toggle-float");
        const nested = (((newQueues[0] as { body: Record<string, unknown> }).body["command"] as Record<string, unknown>)["command"] as Record<string, unknown>);
        assert.equal(nested["win"], WIN_F, "explicit tile names the stable fixed client (bare id on the wire)");
        assert.equal(describeCount(h) - describesBeforeTile, 1, "press queues exactly one DescribeWorld before the tile flush (no silent extra rounds)");
        const tileRound = await h.flushOne();
        if (tileRound.parsed["outcome"] === "rejected") {
            failWithTrace(t, h, "complete-report explicit fixed tile refused; STOP (candidate: float target/mismatch/maximized or lifecycle subject/peer; no production fix here)", tileRound, { intervening, nested });
        }
        assertBoundTransport(tileRound, JSON.parse(tileRound.envelope) as Record<string, unknown>);
        const tileLogical = assertExactBindings(h, tileRound, "tile");
        assert.equal(tileLogical, enableLogical, "tile preserves the bound logical across the leg");
        const replyWins = tileRound.parsed["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual(replyWins.map((w) => w["win"]).sort(), [WIN_F, WIN_O].sort(), "tile reply preserves the exact window set");
        const fixedRow = replyWins.find((w) => w["win"] === WIN_F) as Record<string, unknown>;
        const tilePeerRow = replyWins.find((w) => w["win"] === WIN_O) as Record<string, unknown>;
        assert.equal(tilePeerRow["floating"], false, "peer stays tiled through the explicit tile");
        assert.equal(fixedRow["out"], OUT, "fixed row stays on the single output");
        assert.equal(fixedRow["floating"], false, `explicit tile clears the float exception: ${JSON.stringify(fixedRow)}`);
        assert.equal(fixedRow["frame_auth"], "controlled", `fixed tile under controlled frame authority: ${JSON.stringify(fixedRow)}`);
        assert.equal(fixedRow["member_auth"], "controlled", `fixed tile under controlled membership authority: ${JSON.stringify(fixedRow)}`);
        assert.ok(fixedRow["frame"] !== null, `fixed tile carries desired geometry: ${JSON.stringify(fixedRow)}`);
        const takeResult = lastReadyTake(h);
        assert.ok(takeResult !== null, `take lane carries the terminal result (takes=${h.takeReplies.length})`);
        assert.equal((takeResult as Record<string, unknown>)["op"], "toggle-float", `terminal result is the toggle-float: ${JSON.stringify(takeResult)}`);
        assert.equal((takeResult as Record<string, unknown>)["win"], WIN_F, "terminal result names the fixed client");
        assert.equal((takeResult as Record<string, unknown>)["rect"], null, "explicit tile carries a null rect result");
        // Converge the acceptance: one more observation-only round lets
        // the take-lane terminal settle the tile override, then the fixed
        // tile sticks with zero further commands and the store keeps the
        // tile override disjoint from floats.
        h.fire("windowActivated");
        h.runTimers();
        await h.flushAll();
        assert.equal(countQueueOp(h, "operation:toggle-float"), 1, "explicit tile never replays after settlement");
        const settledWrites = h.storeWrites.map((raw) => JSON.parse(raw) as Record<string, unknown>);
        assert.ok(settledWrites.length >= 1, `expected settled store writes, got ${settledWrites.length}`);
        const lastWrite = settledWrites[settledWrites.length - 1] as Record<string, unknown>;
        const lastMembers = (lastWrite["members"] as Array<string>) ?? [];
        const lastTile = ((lastWrite["tile"] as Array<string>) ?? []);
        assert.ok(lastTile.includes(WIN_F), `store retains the fixed tile override: ${JSON.stringify(lastWrite)}`);
        assert.ok(!lastMembers.includes(WIN_F), `sides stay disjoint: ${JSON.stringify(lastWrite)}`);
        emitTrace(t, h, `${label} leg complete`);
    } catch (error) {
        emitTrace(t, h, `${label} leg failed`);
        throw error;
    } finally {
        await h.stop();
    }
}

describe("world fixed-client explicit tile after workspace enable (real Engine)", () => {
    it("tiles the stable fixed client with zero intervening observes", { timeout: 120000 }, async (t) => {
        await runFixedTileLeg(0, t);
    });

    it("tiles the stable fixed client across two independent observes", { timeout: 120000 }, async (t) => {
        await runFixedTileLeg(2, t);
    });

    it("holds an observation-only incomplete producer report with zero writes", { timeout: 120000 }, async (t) => {
        const h = await makeHarness();
        try {
            await discover(h);
            h.runTimers();
            await h.flushAll();
            await enableWorkspace(h, t);
            const writesBefore = h.geometryWrites.length;
            const membersBefore = h.membershipWrites.length;
            // Observe-only held incomplete case: no command is filed here,
            // so this is NOT an explicit-tile or partial-refusal case (exact
            // refusal shapes stay with separate core tests). An unreadable
            // fixed frame holds the affected scope on the real engine path;
            // the verdict is retained as the discriminating incomplete case,
            // never manufactured.
            const liveFixed = h.live.get(`{${WIN_F}}`) as LiveSpec;
            const savedFrame = { ...liveFixed.frame };
            (liveFixed as { frame: unknown }).frame = { x: 100, y: 100, width: "bad", height: 320 };
            h.fire("windowActivated");
            h.runTimers();
            const round = await h.flushOne();
            const scopes = (round.parsed["scopes"] as Array<Record<string, unknown>> | undefined) ?? [];
            const held = scopes.find((s) => s["auth"] === "hold" && String(s["reason"]).includes("incomplete"));
            assert.ok(held !== undefined, "incomplete producer facts hold the scope naturally (complete evidence in test diagnostics)");
            assert.equal(h.geometryWrites.length, writesBefore, "held incomplete report takes no geometry writes");
            assert.equal(h.membershipWrites.length, membersBefore, "held incomplete report takes no membership writes");
            // Restore the readable condition; the next complete report is
            // accepted again through the same retained engine.
            liveFixed.frame = savedFrame;
            h.fire("windowActivated");
            h.runTimers();
            const recovered = await h.flushOne();
            assert.notEqual(recovered.parsed["outcome"], "rejected", "readable recovery accepts (complete evidence in test diagnostics)");
            emitTrace(t, h, "held-incomplete leg complete");
        } catch (error) {
            emitTrace(t, h, "held-incomplete leg failed");
            throw error;
        } finally {
            await h.stop();
        }
    });
});
