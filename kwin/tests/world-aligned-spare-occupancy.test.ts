// Occupancy-only spare lifecycle for aligned per-output workspaces.
// Producer-shaped regression through the real `WorldEngine` (via the
// test-only `raw_world_eval` example with `WORLD_EVAL_ALIGNED=1`) plus the
// production entry/apply path with fresh serial replies: showing an empty
// end (including via a production-shaped empty-list sticky anchor, homed
// by the observer at its output's current) creates no spare, relative-next
// wraps from the live end on both outputs with the opposite view unchanged,
// the departed end stays as a native-bound empty (core retains
// native-bound empties; only unbound trailing spares retire), and a real
// occupancy arrival on each output's end appends exactly one unbound
// trailing logical (the ordinary spare never requests native backing).
// No-creation is asserted directly via bound rows and unchanged native
// inventories, never via constant counts across departures. Included in
// `npm test`.

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

const LEFT = "out-left";
const RIGHT = "out-right";

async function makeSpareHarness(): Promise<Harness> {
    const desktops: FakeDesktop[] = ["ws-1", "ws-2", "ws-3", "ws-4"].map((id) => ({ id, name: id }));
    const byId = new Map(desktops.map((d) => [d.id, d]));
    const screens = [
        { name: LEFT, geometry: { x: 0, y: 0, width: 1000, height: 800 } },
        { name: RIGHT, geometry: { x: 1000, y: 50, width: 900, height: 700 } },
    ];
    const currents = new Map([[LEFT, "ws-4"], [RIGHT, "ws-2"]]);
    const mkWin = (
        id: string, out: string, ws: string,
        sticky: boolean,
    ): FakeWindow => {
        const outputChanged = makeSignal();
        const winDesktopsChanged = makeSignal();
        return {
            internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { x: 8, y: 52, width: 200, height: 200 },
            output: { name: out },
            // Production sticky shape: an empty desktop list. The observer
            // homes it at its output's current (world-observe.ts), so the
            // anchor follows the view instead of pinning the old cell.
            desktops: sticky ? [] : [byId.get(ws) as FakeDesktop],
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: sticky,
            minSize: null,
            maxSize: null,
            outputChanged: outputChanged.signal,
            desktopsChanged: winDesktopsChanged.signal,
            __signals: { outputChanged, desktopsChanged: winDesktopsChanged },
        };
    };
    const wins: FakeWindow[] = [
        mkWin("win-a", LEFT, "ws-1", false),
        mkWin("win-b", RIGHT, "ws-2", false),
        // Production-shaped sticky anchor: empty desktop list, homed by the
        // observer at its output's current. It stages the shown empty end
        // without occupying any backing cell, and rehomes on departure so
        // the old end retires normally instead of looping removal.
        mkWin("win-sticky", LEFT, "ws-4", true),
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
        wins,
        desktops,
        screens,
        currents,
        calls: [],
        answered: new Set<number>(),
        replies: new Map<number, string>(),
        takeReplies: [],
        timers: [],
        viewWrites: [],
        activeScreen: { name: LEFT },
        activeRef: wins[2] as FakeWindow,
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
        set activeScreen(_ref: unknown) {
            void _ref;
        },
        cursorPos: { x: 2, y: 100 },
        windowAdded: (signals["windowAdded"] as FakeSignal).signal,
        windowRemoved: (signals["windowRemoved"] as FakeSignal).signal,
        windowActivated: (signals["windowActivated"] as FakeSignal).signal,
        desktopsChanged: (signals["desktopsChanged"] as FakeSignal).signal,
        screensChanged: (signals["screensChanged"] as FakeSignal).signal,
        currentDesktopChanged: (signals["currentDesktopChanged"] as FakeSignal).signal,
        get activeWindow(): FakeWindow | null {
            return harness.activeRef;
        },
        set activeWindow(ref: unknown) {
            const found = harness.wins.find((win) => (win as unknown) === ref);
            if (found !== undefined) {
                harness.activeRef = found;
                harness.fire("windowActivated");
            }
        },
        clientArea: (_kind: unknown, outRef: unknown, _deskRef: unknown): Record<string, number> => {
            const name = (outRef as { name: string }).name;
            const geom = screenByName(name).geometry;
            return { x: geom["x"] as number, y: geom["y"] as number, width: geom["width"] as number, height: geom["height"] as number };
        },
        currentDesktopForScreen: (screen: { name: string }): FakeDesktop => {
            return desktopById(harness.currents.get(screen.name) ?? "");
        },
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }): void => {
            const id = (deskRef as FakeDesktop).id;
            assert.ok(byId.has(id) || harness.desktops.some((d) => d.id === id), `desktop ${id} exists`);
            harness.currents.set(screen.name, id);
            harness.viewWrites.push(`${screen.name}->${id}`);
            harness.fire("currentDesktopChanged");
        },
        sendClientToScreen: (): void => undefined,
        // Production void creation: append one native carrying the logical
        // token as its name for exact-name attribution. Proof comes from
        // the fresh named enumeration, never a return value.
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
        removeDesktop: (ref: unknown): boolean => {
            const id = (ref as FakeDesktop).id;
            const at = harness.desktops.findIndex((d) => d.id === id);
            if (at < 0) {
                return false;
            }
            harness.desktops.splice(at, 1);
            byId.delete(id);
            for (const win of harness.wins) {
                win.desktops = win.desktops.filter((d) => d.id !== id);
            }
            harness.fire("desktopsChanged");
            return true;
        },
    };
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
        log: (): void => undefined,
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
    });
    assert.notEqual(handle, null, "world production entry starts on the fake surface");
    (harness as { handle: unknown }).handle = handle;
    return harness;
}

function lastReply(h: Harness): Record<string, unknown> {
    const indices = [...h.replies.keys()].filter((at) => (h.calls[at] as CapturedCall).method === "DescribeWorld");
    assert.ok(indices.length > 0, "at least one answered DescribeWorld");
    const raw = h.replies.get(Math.max(...indices) as number) as string;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    assert.equal(parsed["outcome"] ?? "accepted", "accepted", `fresh reply accepted, got ${raw.slice(0, 300)}`);
    return parsed;
}

function workspacesOf(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return (reply["workspaces"] as Array<Record<string, unknown>>) ?? [];
}

function transfersOf(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return (reply["transfers"] as Array<Record<string, unknown>>) ?? [];
}

describe("world aligned spare occupancy-only", () => {
    it("showing empty ends creates no spare on either output, forward next wraps, occupancy still extends", { timeout: 240000 }, async () => {
        const h = await makeSpareHarness();
        try {
            const discIndex = h.calls.findIndex((call) => call.method === "DiscoverWorld");
            assert.ok(discIndex >= 0, "entry discovers on start");
            const disc = h.calls[discIndex] as CapturedCall;
            const engineBootstrap = await h.bridge.send(disc.method, disc.payload);
            const discReply = h.broker.bindDiscovery(engineBootstrap);
            disc.callback(discReply);
            await h.settle();
            assert.equal(h.desktops.length, 4, "no native appended while showing empty ends");
            let reply = lastReply(h);
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "no unbound spare rows while showing empty ends",
            );

            // empty-list sticky anchor, homed by the observer at the shown
            // current. The anchor follows the view: after this departure it
            // rehomes onto ws-1 and the old end stays as a native-bound
            // empty (core retains native-bound empties against trailing
            // lifecycle removal; only unbound trailing spares retire).
            // No-creation is asserted directly via bound rows.
            h.activeScreen = { name: LEFT };
            h.activeRef = h.wins[0] as FakeWindow;
            h.handle.requestWorkspaceRelative(1);
            h.runTimers();
            await h.settle();
            reply = lastReply(h);
            assert.equal(h.currents.get(LEFT), "ws-1", "relative next wraps from the shown empty end");
            assert.equal(h.currents.get(RIGHT), "ws-2", "opposite view unchanged by left wrap");
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "wrap mints no spare (bound rows only)",
            );
            const departedRow = workspacesOf(reply).find((row) => row["out"] === LEFT && (row["bind"] as string) === "ws-4");
            assert.ok(
                departedRow !== undefined,
                `departed end stays as a native-bound empty (desktops=${h.desktops.map((d) => d.id).join(",")})`,
            );

            // Reshow the live end (ws-3) on the left: genuinely empty with
            // the anchor homed at the current view, then numbered-depart it.
            // Showing creates nothing either way.
            h.currents.set(LEFT, "ws-3");
            h.fire("currentDesktopChanged");
            await h.settle();
            assert.ok(
                workspacesOf(lastReply(h)).every((row) => (row["bind"] as string) !== ""),
                "reshowing the live end creates no spare",
            );
            h.activeScreen = { name: LEFT };
            h.handle.requestWorkspaceSelect(1);
            h.runTimers();
            await h.settle();
            reply = lastReply(h);
            assert.equal(h.currents.get(LEFT), "ws-1", "numbered 1 departs the shown empty end");
            assert.equal(h.currents.get(RIGHT), "ws-2", "opposite view unchanged by left departure");
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "numbered departure mints no spare",
            );

            // Same on the right output: its shown end (ws-3, genuinely
            // empty there) steps forward onto the retained ws-4 cell (the
            // departed ws-4 was retained, not retired, so the ring still
            // holds four cells and only the true end wraps). Left view
            // unchanged and no spare minted.
            h.activeScreen = { name: RIGHT };
            h.activeRef = h.wins[1] as FakeWindow;
            h.currents.set(RIGHT, "ws-3");
            h.fire("currentDesktopChanged");
            await h.settle();
            assert.ok(
                workspacesOf(lastReply(h)).every((row) => (row["bind"] as string) !== ""),
                "right empty end creates no spare",
            );
            h.handle.requestWorkspaceRelative(1);
            h.runTimers();
            await h.settle();
            reply = lastReply(h);
            assert.equal(h.currents.get(RIGHT), "ws-4", "relative next steps onto the retained end on the right");
            assert.equal(h.currents.get(LEFT), "ws-1", "left view unchanged by right step");
            assert.ok(
                workspacesOf(reply).every((row) => (row["bind"] as string) !== ""),
                "right wrap mints no spare",
            );

            // Occupancy arrival still extends the spare on each output: a
            // real window on an output's end appends exactly one trailing
            // logical. The ordinary spare never requests native backing
            // (core-owned lifecycle), so no native desktop is created; the
            // filled end keeps its binding and the spare stays unbound. The
            // retained ws-4 cell is the live end of the left ring (the
            // departed end was retained, not retired), so the arrival
            // targets it directly.
            const fillEnd = (tag: string, out: string, ws: string): void => {
                const desk = h.desktops.find((d) => d.id === ws) as FakeDesktop;
                const newcomer: FakeWindow = {
                    internalId: tag, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
                    normalWindow: true,
                    frameGeometry: { x: 8, y: 52, width: 200, height: 200 },
                    output: { name: out },
                    desktops: [desk],
                    minimized: false,
                    fullScreen: false,
                    maximizeMode: 0,
                    onAllDesktops: false,
                    minSize: null,
                    maxSize: null,
                    outputChanged: makeSignal().signal,
                    desktopsChanged: makeSignal().signal,
                };
                h.wins.push(newcomer);
            };
            const spareOf = (rows: Array<Record<string, unknown>>, out: string): Array<Record<string, unknown>> =>
                rows.filter((row) => row["out"] === out && (row["bind"] as string) === "");
            const countBeforeLeft = h.desktops.length;
            fillEnd("win-c", LEFT, "ws-4");
            h.fire("windowAdded");
            await h.settle();
            assert.equal(h.desktops.length, countBeforeLeft, "left fill creates no native (ordinary spare needs no backing)");
            reply = lastReply(h);
            {
                const rows = workspacesOf(reply).filter((row) => row["out"] === LEFT);
                const spares = spareOf(workspacesOf(reply), LEFT);
                assert.equal(spares.length, 1, "left fill appends exactly one trailing spare");
                assert.equal(spares[0]?.["id"], rows[rows.length - 1]?.["id"], "spare is the trailing row, never a mid-ring rename");
                const filledLogical = rows.find((row) => (row["bind"] as string) === "ws-4")?.["id"] as string;
                assert.ok(filledLogical !== undefined && filledLogical !== "", "filled end keeps its logical");
                const winRows = (reply["windows"] as Array<Record<string, unknown>>);
                assert.ok(winRows.some((w) => w["win"] === "win-c" && w["out"] === LEFT && w["ws"] === filledLogical), "newcomer homed on the filled end");
                assert.deepEqual(reply["nativeCreations"], [], "ordinary spare desires no backing");
            }
            assert.ok(
                transfersOf(reply).length === 0,
                `settled with no retained transfers, got ${JSON.stringify(transfersOf(reply))}`,
            );
            const countBeforeRight = h.desktops.length;
            const rightEnd = h.desktops[h.desktops.length - 1] as FakeDesktop;
            fillEnd("win-d", RIGHT, rightEnd.id);
            h.fire("windowAdded");
            await h.settle();
            assert.equal(h.desktops.length, countBeforeRight, "right fill creates no native (ordinary spare needs no backing)");
            reply = lastReply(h);
            {
                const rows = workspacesOf(reply).filter((row) => row["out"] === RIGHT);
                const spares = spareOf(workspacesOf(reply), RIGHT);
                assert.equal(spares.length, 1, "right fill appends exactly one trailing spare");
                assert.equal(spares[0]?.["id"], rows[rows.length - 1]?.["id"], "spare is the trailing row, never a mid-ring rename");
                const filledLogical = rows.find((row) => (row["bind"] as string) === rightEnd.id)?.["id"] as string;
                assert.ok(filledLogical !== undefined && filledLogical !== "", "filled end keeps its logical");
                const winRows = (reply["windows"] as Array<Record<string, unknown>>);
                assert.ok(winRows.some((w) => w["win"] === "win-d" && w["out"] === RIGHT && w["ws"] === filledLogical), "newcomer homed on the filled end");
                assert.deepEqual(reply["nativeCreations"], [], "ordinary spare desires no backing");
            }
            assert.ok(
                transfersOf(reply).length === 0,
                `settled with no retained transfers, got ${JSON.stringify(transfersOf(reply))}`,
            );
        } finally {
            await h.stop();
        }
    });
});
