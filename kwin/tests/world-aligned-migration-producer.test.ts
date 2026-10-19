// Producer-shaped aligned whole-workspace migration end to end through
// the production entry against the real `WorldEngine` (via the test-only
// `world_eval` example with `WORLD_EVAL_ALIGNED=1`) with a faithful native
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
// run selects out-right/ws-1, migrates left into the trailing (closing
// member win-m mid-flight), verifies refill plus logical ride/mode carry,
// migrates back right (closing member win-c mid-flight, exercising the
// expected echo), then migrates an empty workspace (both views must still
// move and settle), with unreadable arrival plus stale view rounds. Every
// reply on the asserted paths is a fresh Engine report. Included in
// `npm test`.

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
    name: string;
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

interface Harness {
    readonly bridge: WorldBridge;
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
            internalId: id,
            normalWindow: true,
            frameGeometry: frame,
            output: { name: out },
            desktops: [byId.get(ws) as FakeDesktop],
            minimized: false,
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
            const reply = await harness.bridge.send(
                (harness.calls[index] as CapturedCall).method,
                (harness.calls[index] as CapturedCall).payload,
            );
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
        createDesktop: (): null => null,
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
    return h.calls
        .map((call, index) => ({ call, index }))
        .filter(({ call }) => {
            if (call.method !== "DescribeWorld") {
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

            // Select the out-right/ws-1 cell before migrating it.
            h.activeScreen = { name: RIGHT };
            h.activeRef = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            h.currents.set(RIGHT, "ws-1");
            h.fire("currentDesktopChanged");
            await h.settle();

            // The float intent lands after the seed: fixed min/max pins
            // win-c floating (automatic fixed admission), converting the
            // seeded member into a float the migration must carry
            // membership-only.
            const winCSeed = h.wins.find((w) => w.internalId === "win-c") as FakeWindow;
            winCSeed.minSize = { width: 100, height: 100 };
            winCSeed.maxSize = { width: 100, height: 100 };
            h.fire("desktopsChanged");
            await h.settle();

            const bindingsBefore = (payloadOf(h, h.calls.length - 1)["bindings"] as Array<Record<string, string>>);
            const sourceLogical = (bindingsBefore.find((b) => b["out"] === RIGHT && b["native"] === "ws-1") as Record<string, string>)["logical"];
            assert.ok(typeof sourceLogical === "string" && sourceLogical !== "", "source cell has a logical identity");

            // Leg 1: migrate out-right/ws-1 (win-a plus floating win-c plus
            // win-m) left.
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
            const leg1reply = JSON.parse(h.replies.get(leg1cmds[0]?.index as number) as string) as Record<string, unknown>;
            const leg1result = leg1reply["result"] as Record<string, unknown>;
            assert.equal(leg1result["op"], "migrate-workspace");
            assert.equal(leg1result["sourceOut"], RIGHT);
            assert.equal(leg1result["targetOut"], LEFT);
            const leg1targetWs = leg1result["targetWs"] as string;
            assert.deepEqual((leg1result["members"] as string[]).sort(), ["win-a", "win-c", "win-m"]);
            assert.equal(leg1result["floats"], 1, "float member accounted in the carry");
            const leg1transfers = transfersOf(leg1reply);
            const leg1migration = leg1transfers.find((t) => t["kind"] === "migration");
            assert.ok(leg1migration !== undefined, "migration transfer retained");
            const leg1refill = leg1transfers.find((t) => t["kind"] === "view" && (t["op"] as string).endsWith("-refill"));
            assert.ok(leg1refill !== undefined, "explicit core refill retained");
            assert.equal(leg1refill?.["out"], RIGHT);
            const leg1refillWs = leg1refill?.["ws"] as string;
            assert.ok(leg1refillWs !== "ws-1", "refill excludes the migrated native");

            // Native actuation: both members left the source, both views
            // followed, the close converged without wedging the flight.
            const winA = h.wins.find((w) => w.internalId === "win-a") as FakeWindow;
            const winC = h.wins.find((w) => w.internalId === "win-c") as FakeWindow;
            assert.equal(winA.output.name, LEFT, "member output moved");
            assert.equal(winC.output.name, LEFT, "float member output moved");
            assert.deepEqual(winA.desktops.map((d) => d.id), [leg1targetWs]);
            assert.deepEqual(winC.desktops.map((d) => d.id), [leg1targetWs]);
            assert.equal(h.currents.get(LEFT), leg1targetWs, "target view follows");
            assert.equal(h.currents.get(RIGHT), leg1refillWs, "source refills to the core choice");
            assert.equal(JSON.stringify(winC.frameGeometry), winCBefore, "float member carried membership-only, no frame write");
            assert.ok(!h.wins.some((w) => w.internalId === "win-m"), "closed member stays closed");
            const settledDomains1 = ((payloadOf(h, h.calls.length - 1)["domains"] as Array<Record<string, unknown>>)
                .flatMap((d) => d["windows"] as Array<Record<string, unknown>>)
                .map((w) => w["win"]));
            assert.ok(!settledDomains1.includes("win-m"), "closed member acknowledged out of the converged roster");
            assert.deepEqual(h.echoWrites, [], "leg 1 needs no echo: the window already sits on the destination current");
            const settled1 = lastReply(h);
            assert.deepEqual(transfersOf(settled1), [], "leg 1 settles with no retained transfers");
            const payload1 = payloadOf(h, h.calls.length - 1);
            const bindings1 = payload1["bindings"] as Array<Record<string, string>>;
            const moved1 = bindings1.find((b) => b["out"] === LEFT && b["native"] === leg1targetWs);
            assert.equal(moved1?.["logical"], sourceLogical, "logical identity rides the move");
            const fresh1 = bindings1.find((b) => b["out"] === RIGHT && b["native"] === "ws-1");
            assert.ok(fresh1 !== undefined && fresh1["logical"] !== sourceLogical, "emptied source cell mints fresh");
            const workspaces1 = settled1["workspaces"] as Array<Record<string, string>>;
            for (const row of workspaces1.filter((w) => w["out"] === LEFT && w["bind"] === leg1targetWs)) {
                assert.equal(row["mode"], "tiled", "moved unit keeps its tiling mode");
            }

            // Leg 2: migrate back right. The pinned echo fires here when the
            // source current differs from the destination current: the
            // output step re-homes onto the destination current first.
            h.activeScreen = { name: LEFT };
            h.activeRef = winA;
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
            const leg2reply = JSON.parse(h.replies.get(leg2cmds[0]?.index as number) as string) as Record<string, unknown>;
            const leg2result = leg2reply["result"] as Record<string, unknown>;
            assert.equal(leg2result["op"], "migrate-workspace");
            assert.equal(leg2result["sourceOut"], LEFT);
            assert.equal(leg2result["targetOut"], RIGHT);
            const leg2targetWs = leg2result["targetWs"] as string;
            const leg2refill = transfersOf(leg2reply).find((t) => t["kind"] === "view" && (t["op"] as string).endsWith("-refill"));
            assert.ok(leg2refill !== undefined, "return refill retained");
            const leg2refillWs = leg2refill?.["ws"] as string;
            assert.equal(winA.output.name, RIGHT, "returned member output moved");
            assert.deepEqual(winA.desktops.map((d) => d.id), [leg2targetWs]);
            assert.ok(
                h.echoWrites.some((e: string) => e.startsWith("win-a->") || e.startsWith("win-c->")),
                `pinned output-send echo fired mid-flight and was tolerated: ${JSON.stringify(h.echoWrites)}`,
            );
            assert.equal(h.currents.get(RIGHT), leg2targetWs, "return target view follows");
            assert.equal(h.currents.get(LEFT), leg2refillWs, "return source refills");
            assert.ok(!h.wins.some((w) => w.internalId === "win-c"), "closed member stays closed");
            const settledDomains2 = ((payloadOf(h, h.calls.length - 1)["domains"] as Array<Record<string, unknown>>)
                .flatMap((d) => d["windows"] as Array<Record<string, unknown>>)
                .map((w) => w["win"]));
            assert.ok(!settledDomains2.includes("win-c"), "closed member acknowledged out of the converged roster");
            const settled2 = lastReply(h);
            assert.deepEqual(transfersOf(settled2), [], "leg 2 settles with no retained transfers");
            const bindings2 = (payloadOf(h, h.calls.length - 1)["bindings"] as Array<Record<string, string>>);
            const moved2 = bindings2.find((b) => b["out"] === RIGHT && b["native"] === leg2targetWs);
            assert.equal(moved2?.["logical"], sourceLogical, "logical identity rides back");

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
            h.handle.requestWorkspaceMigrate("right");
            try {
                await h.settle();
            } catch (e) {
                console.log(debugState(h, "LEG3-SETTLE"));
                throw e;
            }
            const leg3cmds = migrateCommands(h).slice(2);
            assert.equal(leg3cmds.length, 1, "empty migration dispatches exactly once");
            const leg3reply = JSON.parse(h.replies.get(leg3cmds[0]?.index as number) as string) as Record<string, unknown>;
            const leg3result = leg3reply["result"] as Record<string, unknown>;
            assert.equal(leg3result["op"] ?? null, "migrate-workspace", "empty migration commits");
            assert.deepEqual(leg3result["members"] as string[], [], "empty migration carries no members");
            assert.equal(leg3result["sourceOut"], LEFT);
            assert.equal(leg3result["targetOut"], RIGHT);
            const leg3targetWs = leg3result["targetWs"] as string;
            const leg3refill = transfersOf(leg3reply).find((t) => t["kind"] === "view" && (t["op"] as string).endsWith("-refill"));
            assert.ok(leg3refill !== undefined, "empty migration retains the explicit refill");
            const leg3refillWs = leg3refill?.["ws"] as string;
            assert.equal(h.currents.get(RIGHT), leg3targetWs, "empty target view follows");
            assert.equal(h.currents.get(LEFT), leg3refillWs, "empty source refills");
            assert.deepEqual(transfersOf(lastReply(h)), [], "empty migration settles both views with no retained transfers");

            assert.equal(migrateCommands(h).length, 3, "three migration commands total, none replayed");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")), "no busy refills");
        } finally {
            await h.stop();
        }
    });
});
