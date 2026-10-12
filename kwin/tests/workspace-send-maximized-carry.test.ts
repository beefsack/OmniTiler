import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { describe, it } from "node:test";
import { dirname, join, resolve } from "node:path";

import {
    WORKSPACE_SEND_CONTRACT_VERSION,
    WORKSPACE_SEND_DBUS_SERVICE,
    WORKSPACE_SEND_GET_OWNER_METHOD,
    WORKSPACE_SEND_HAS_OWNER_METHOD,
    WORKSPACE_SEND_METHOD,
    WORKSPACE_SEND_START_METHOD,
    WorkspaceSendAdapter,
    type WorkspaceSendAdapterEnv,
    type WorkspaceSendObserved,
} from "../src/workspace-send-adapter";
import { startWorkspaceSendAdapterEntry } from "../src/workspace-send-adapter-entry";

// G-D2 maximized workspace-send carry (REQ-MAX-09 maximize leg,
// User 2026-10-10) plus D28 fullscreen carry (User 2026-10-12): a
// maximized or fullscreen window sent to another workspace keeps its
// overlay on arrival with no gratuitous unmaximize/remaximize and no
// geometry writes to overlaid subjects. Offline only: fake KWin surface
// plus the real Planner via planner_eval for the follow path, canned
// Engine replies for the stay path.

// ---- Real-Engine bridge (mirrors workspace-send-engine-fixture). ----

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

class EngineBridge {
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
            throw new Error("planner_eval stdout unavailable");
        }
        createInterface({ input: stdout }).on("line", (line: string) => {
            this.waiters.shift()?.resolve(line);
        });
        proc.stderr?.resume();
        proc.on("error", (error) => {
            this.failAll(error instanceof Error ? error : new Error(String(error)));
        });
        proc.on("exit", (code) => {
            this.failAll(new Error(`planner_eval exited with code ${String(code)}`));
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

    private deathReason(): string | null {
        if (this.dead !== null) {
            return this.dead;
        }
        if (this.proc.exitCode !== null || this.proc.signalCode !== null) {
            return "planner_eval process already exited";
        }
        return null;
    }

    static start(): EngineBridge {
        return new EngineBridge(
            spawn("cargo", ["run", "--offline", "-q", "-p", "tiler-protocol", "--example", "planner_eval"], {
                cwd: repoRoot(),
                stdio: ["pipe", "pipe", "pipe"],
            }),
        );
    }

    send(request: string): Promise<string> {
        const dead = this.deathReason();
        if (dead !== null) {
            return Promise.reject(new Error(dead));
        }
        return new Promise<string>((resolve, reject) => {
            this.waiters.push({ resolve, reject });
            try {
                this.proc.stdin?.write(request + "\n");
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

async function waitFor(condition: () => boolean, label: string): Promise<void> {
    const start = Date.now();
    while (!condition()) {
        if (Date.now() - start > 15000) {
            throw new Error(`timeout: ${label}`);
        }
        await new Promise((resolve) => setTimeout(resolve, 5));
    }
}

async function settle(ms: number): Promise<void> {
    await new Promise((resolve) => setTimeout(resolve, ms));
}

// ---- Fake KWin surface for the entry observer path. ----

interface FakeSignal {
    connect: (handler: () => void) => void;
    disconnect: (handler: () => void) => void;
}

function makeSignal(): FakeSignal {
    const handlers = new Set<() => void>();
    return {
        connect: (handler) => {
            handlers.add(handler);
        },
        disconnect: (handler) => {
            handlers.delete(handler);
        },
    };
}

interface FakeWindow {
    normalWindow: boolean;
    managed: boolean;
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    output: { name: string };
    desktops: Array<object>;
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number };
    desktopsChanged: FakeSignal;
}

function makeWindow(id: string, output: { name: string }, desktop: object): FakeWindow {
    return {
        normalWindow: true,
        managed: true,
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        output,
        desktops: [desktop],
        internalId: id,
        frameGeometry: { x: 10, y: 10, width: 500, height: 300 },
        desktopsChanged: makeSignal(),
    };
}

interface SurfaceHarness {
    readonly win: { wa: FakeWindow; wb: FakeWindow; wt: FakeWindow };
    readonly desk: { d1: object; d2: object };
    readonly surface: Record<string, unknown>;
    readonly calls: Array<{ payload: string; reply: string }>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }>;
    readonly logs: string[];
    readonly switches: object[];
    readonly bridge: EngineBridge;
    readonly requestSend: (target: unknown) => boolean;
    readonly stop: () => void;
    flush: () => Promise<void>;
    queued: () => number;
    isInFlight: () => boolean;
}

async function makeSurfaceHarness(): Promise<SurfaceHarness> {
    const out = { name: "out-1" };
    const d1 = { id: "ws-1" };
    const d2 = { id: "ws-2" };
    const wa = makeWindow("n-win-a", out, d1);
    const wb = makeWindow("n-win-b", out, d1);
    const wt = makeWindow("n-win-t", out, d2);
    let current: object = d1;
    const surface: Record<string, unknown> = {
        activeWindow: wa,
        currentDesktop: d1,
        screens: [out],
        desktops: [d1, d2],
        currentDesktopForScreen: (): object => current,
        setCurrentDesktopForScreen: (desktop: object): void => {
            switches.push(desktop);
            current = desktop;
        },
        clientArea: (): object => ({ x: 0, y: 0, width: 1200, height: 800 }),
        windowList: (): Array<object> => [wa, wb, wt],
    };
    const bridge = EngineBridge.start();
    const calls: SurfaceHarness["calls"] = [];
    const timers: SurfaceHarness["timers"] = [];
    const logs: string[] = [];
    const switches: object[] = [];
    const pending: Array<{ payload: string; callback: (reply: unknown) => void }> = [];
    const entry = startWorkspaceSendAdapterEntry({
        workspace: surface,
        callDbus: (service, _path, _iface, method, payload, callback) => {
            if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                callback(true);
                return;
            }
            if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_GET_OWNER_METHOD) {
                callback(":1.7");
                return;
            }
            if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_START_METHOD) {
                callback(1);
                return;
            }
            if (method === WORKSPACE_SEND_METHOD) {
                pending.push({ payload, callback });
                return;
            }
            callback(undefined);
        },
        scheduleOnce: (delayMs, callback) => {
            const timer = { delayMs, callback, cancelled: false };
            timers.push(timer);
            return () => {
                timer.cancelled = true;
            };
        },
        log: (message) => {
            logs.push(message);
        },
        owner: "owner-1",
        generation: "gen-1",
        readInnerGapFn: () => 8,
        readOuterGapFn: () => 8,
    });
    if (entry === null) {
        await bridge.close();
        throw new Error("entry failed to start");
    }
    return {
        win: { wa, wb, wt },
        desk: { d1, d2 },
        surface,
        calls,
        timers,
        logs,
        switches,
        bridge,
        requestSend: (target) => entry.requestSend(target),
        stop: () => entry.stop(),
        flush: async () => {
            const next = pending.shift();
            if (next === undefined) {
                return;
            }
            const call = { payload: next.payload, reply: "" };
            calls.push(call);
            call.reply = await bridge.send(next.payload);
            next.callback(call.reply);
        },
        queued: () => pending.length,
        isInFlight: () => entry.isInFlight(),
    };
}

describe("G-D2 maximized send carry (real Planner)", () => {
    it("sends a maximized mover with follow and keeps maximize on arrival", async () => {
        const h = await makeSurfaceHarness();
        try {
            h.win.wa.maximizeMode = 3;
            const moverBefore = { ...h.win.wa.frameGeometry };
            assert.equal(h.requestSend("ws-2"), true, "maximized send accepted");
            await waitFor(() => h.queued() > 0, "observer request");
            await h.flush();
            const planned = JSON.parse(h.calls[0]?.reply ?? "{}") as Record<string, unknown>;
            assert.equal(planned["outcome"], "planned", "real Engine plans the maximized send");
            await settle(100);
            assert.deepEqual(h.win.wa.desktops, [h.desk.d2], "mover natively on exact target, absent source");
            assert.equal(h.win.wa.maximizeMode, 3, "no gratuitous unmaximize/remaximize: maximize untouched");
            assert.deepEqual({ ...h.win.wa.frameGeometry }, moverBefore, "no geometry write to the overlaid mover");
            assert.deepEqual(h.switches, [h.desk.d2], "follow switches exactly once");
            assert.equal(h.surface["activeWindow"], h.win.wa, "mover focused after proof");
            assert.equal(h.isInFlight(), false, "flight released after arrival");
        } finally {
            h.stop();
            await h.bridge.close();
        }
    });

    it("sends a fullscreen mover with follow and keeps fullscreen on arrival", async () => {
        const h = await makeSurfaceHarness();
        try {
            h.win.wa.fullScreen = true;
            const moverBefore = { ...h.win.wa.frameGeometry };
            assert.equal(h.requestSend("ws-2"), true, "fullscreen send accepted as explicit carry");
            await waitFor(() => h.queued() > 0, "observer request");
            await h.flush();
            const planned = JSON.parse(h.calls[0]?.reply ?? "{}") as Record<string, unknown>;
            assert.equal(planned["outcome"], "planned", "real Engine plans the fullscreen send");
            await settle(100);
            assert.deepEqual(h.win.wa.desktops, [h.desk.d2], "mover natively on exact target, absent source");
            assert.equal(h.win.wa.fullScreen, true, "no gratuitous fullscreen drop: flag untouched");
            assert.deepEqual({ ...h.win.wa.frameGeometry }, moverBefore, "no geometry write to the fullscreen mover");
            assert.deepEqual(h.switches, [h.desk.d2], "follow switches exactly once");
            assert.equal(h.surface["activeWindow"], h.win.wa, "mover focused after proof");
            assert.equal(h.isInFlight(), false, "flight released after arrival");
        } finally {
            h.stop();
            await h.bridge.close();
        }
    });

    it("refuses a sticky mover while the maximized path stays open", async () => {
        const h = await makeSurfaceHarness();
        try {
            h.win.wa.onAllDesktops = true;
            assert.equal(h.requestSend("ws-2"), false, "sticky send still refused as non-tiled-focus");
            assert.equal(h.isInFlight(), false, "no flight starts");
            assert.deepEqual(h.win.wa.desktops, [h.desk.d1], "nothing moves");
        } finally {
            h.stop();
            await h.bridge.close();
        }
    });
});

// ---- Adapter-level harness for the stay path (canned Engine reply, mock
// native). The wire omits maximized, so the reply is identical to an
// ordinary send; the adapter proves overlay-aware actuation. ----

function rect(x: number, y: number, w: number, h: number): { x: number; y: number; w: number; h: number } {
    return { x, y, w, h };
}

interface StayWorld {
    readonly windows: Array<{ id: string; ref: object; rect: { x: number; y: number; w: number; h: number }; workspace: string; maximized: boolean }>;
}

function stayWorld(refs: { a: object; b: object; t: object; desktop: object }): StayWorld {
    return {
        windows: [
            { id: "win-a", ref: refs.a, rect: rect(0, 0, 100, 100), workspace: "ws-1", maximized: true },
            { id: "win-b", ref: refs.b, rect: rect(100, 0, 100, 100), workspace: "ws-1", maximized: false },
            { id: "win-t", ref: refs.t, rect: rect(0, 0, 100, 100), workspace: "ws-2", maximized: false },
        ],
    };
}

function stayObserved(
    world: StayWorld,
    refs: { a: object; b: object; t: object; desktop: object },
    activeRef: object | null,
): WorkspaceSendObserved {
    const flagged = (entry: StayWorld["windows"][number]): Record<string, unknown> =>
        Object.freeze({
            id: entry.id,
            ref: entry.ref,
            rect: Object.freeze({ ...entry.rect }),
            fullscreen: false,
            maximized: entry.maximized,
            floating: false,
            sticky: false,
            fitExcluded: entry.maximized,
            fit_excluded: entry.maximized,
        });
    const source = world.windows.filter((entry) => entry.workspace === "ws-1");
    const activeId = world.windows.find((entry) => entry.ref === activeRef)?.id ?? "";
    const focused = source.some((entry) => entry.id === activeId) ? activeId : "";
    const moverEntry = source.find((entry) => entry.id === activeId);
    return {
        sourceOutput: "out-1",
        sourceWorkspace: "ws-1",
        sourceBounds: Object.freeze(rect(0, 0, 1200, 800)),
        targetOutput: "out-1",
        targetWorkspace: "ws-2",
        targetBounds: Object.freeze(rect(0, 0, 1200, 800)),
        focusedId: focused,
        sourceWindows: Object.freeze(source.map(flagged)),
        targetWindows: Object.freeze(world.windows.filter((entry) => entry.workspace === "ws-2").map(flagged)),
        activeRef,
        moverRef: moverEntry === undefined ? null : moverEntry.ref,
        targetDesktopRef: refs.desktop,
        targetExists: true,
        desktopCount: 2,
        sourceFingerprint: "sfp-1",
        targetFingerprint: "tfp-1",
        currentWorkspace: "ws-1",
    } as unknown as WorkspaceSendObserved;
}

function stayReply(correlation: string): string {
    return JSON.stringify({
        v: WORKSPACE_SEND_CONTRACT_VERSION,
        correlation_id: correlation,
        outcome: "planned",
        kind: "send-to-workspace",
        base_revision: 0,
        detail: { kind: "send-to-workspace", policy_version: 1, capability: "move-tiled" },
        desired_geometry: [
            { window: "win-a", leaf: "leaf-win-a", output: "out-1", workspace: "ws-2", rect: { x: 0, y: 0, w: 600, h: 800 } },
            { window: "win-b", leaf: "leaf-win-b", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 1200, h: 800 } },
            { window: "win-t", leaf: "leaf-win-t", output: "out-1", workspace: "ws-2", rect: { x: 600, y: 0, w: 600, h: 800 } },
        ],
        desired_focus: { domain_output: "out-1", domain_workspace: "ws-1", leaf: "leaf-win-b" },
        preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
        operation: {
            op: "move-tiled",
            window: "win-a",
            leaf: "leaf-win-a",
            source_output: "out-1",
            source_workspace: "ws-1",
            target_output: "out-1",
            target_workspace: "ws-2",
            follow: false,
        },
    });
}

describe("G-D2 maximized send carry (stay path)", () => {
    it("stay sends a maximized mover with a numbered ordinal and focuses the source MRU", () => {
        const refs = { a: {}, b: {}, t: {}, desktop: {} };
        const world = stayWorld(refs);
        const geometries: object[] = [];
        const moverWrites: object[] = [];
        const switches: object[] = [];
        const focuses: object[] = [];
        const logs: string[] = [];
        const dbusCalls: Array<{ method: string; payload: string }> = [];
        const callbacks: Array<(reply: unknown) => void> = [];
        let activeRef: object | null = refs.a;
        const env: WorkspaceSendAdapterEnv = {
            callDbus: (_service, _path, _iface, method, payload, callback) => {
                if (method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                    callback(true);
                    return;
                }
                dbusCalls.push({ method, payload });
                callbacks.push(callback);
            },
            scheduleOnce: (): (() => void) => (): void => {},
            log: (message) => {
                logs.push(message);
            },
            observe: () => stayObserved(world, refs, activeRef),
            onSettled: () => {},
            setGeometry: (target) => {
                geometries.push(target);
                return true;
            },
            setDesktops: (target) => {
                moverWrites.push(target);
                for (const entry of world.windows) {
                    if (entry.ref === target) {
                        entry.workspace = "ws-2";
                    }
                }
                return true;
            },
            switchToTarget: (desktopRef) => {
                switches.push(desktopRef);
                return true;
            },
            focusWindow: (windowRef) => {
                focuses.push(windowRef);
                activeRef = windowRef;
                return true;
            },
        };
        const adapter = new WorkspaceSendAdapter(env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        // Numbered ordinal (2) plus explicit stay selection.
        assert.equal(adapter.requestSend("ws-2", 2, false), true);
        assert.equal(dbusCalls[0]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
        callbacks[0]?.(":1.7");
        const body = JSON.parse(dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
        assert.equal((body["command"] as Record<string, unknown>)["follow"], false);
        const correlation = body["correlation_id"] as string;
        callbacks[1]?.(stayReply(correlation));
        assert.equal(moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(moverWrites[0], refs.a);
        assert.ok(!geometries.includes(refs.a), "no geometry write to the overlaid maximized mover");
        assert.deepEqual(switches, [], "stay never switches desktops");
        assert.deepEqual(focuses, [refs.b], "stay focuses the source MRU survivor, never the mover");
        assert.ok(
            logs.some((l) => l.includes("event=follow") && l.includes("outcome=stay-confirmed")),
            logs.join("\n"),
        );
        assert.equal(adapter.isInFlight, false);
    });
});

// ---- D28 fullscreen send carry (numbered/relative follow/stay, same
// output). Adapter-level harness with canned Engine replies: the mover is
// fullscreen with a retained tile slot, so dispatch binds it and geometry
// writes skip every overlay. ----

interface CarryWorld {
    readonly windows: Array<{
        id: string;
        ref: object;
        rect: { x: number; y: number; w: number; h: number };
        workspace: string;
        fullscreen: boolean;
        maximized: boolean;
    }>;
}

function carryWorld(refs: { a: object; b: object; t: object; desktop: object }): CarryWorld {
    return {
        windows: [
            { id: "win-a", ref: refs.a, rect: rect(0, 0, 100, 100), workspace: "ws-1", fullscreen: true, maximized: false },
            { id: "win-b", ref: refs.b, rect: rect(100, 0, 100, 100), workspace: "ws-1", fullscreen: false, maximized: false },
            // Destination overlay: fullscreen-but-ordinary target tile whose
            // retained allocation must never take a geometry write.
            { id: "win-t", ref: refs.t, rect: rect(0, 0, 100, 100), workspace: "ws-2", fullscreen: true, maximized: false },
        ],
    };
}

function carryObserved(
    world: CarryWorld,
    refs: { a: object; b: object; t: object; desktop: object },
    activeRef: object | null,
): WorkspaceSendObserved {
    const flagged = (entry: CarryWorld["windows"][number]): Record<string, unknown> =>
        Object.freeze({
            id: entry.id,
            ref: entry.ref,
            rect: Object.freeze({ ...entry.rect }),
            fullscreen: entry.fullscreen,
            maximized: entry.maximized,
            floating: false,
            sticky: false,
            fitExcluded: entry.fullscreen || entry.maximized,
            fit_excluded: entry.fullscreen || entry.maximized,
        });
    const source = world.windows.filter((entry) => entry.workspace === "ws-1");
    const activeId = world.windows.find((entry) => entry.ref === activeRef)?.id ?? "";
    const focused = source.some((entry) => entry.id === activeId) ? activeId : "";
    const moverEntry = source.find((entry) => entry.id === activeId);
    return {
        sourceOutput: "out-1",
        sourceWorkspace: "ws-1",
        sourceBounds: Object.freeze(rect(0, 0, 1200, 800)),
        targetOutput: "out-1",
        targetWorkspace: "ws-2",
        targetBounds: Object.freeze(rect(0, 0, 1200, 800)),
        focusedId: focused,
        sourceWindows: Object.freeze(source.map(flagged)),
        targetWindows: Object.freeze(world.windows.filter((entry) => entry.workspace === "ws-2").map(flagged)),
        activeRef,
        moverRef: moverEntry === undefined ? null : moverEntry.ref,
        targetDesktopRef: refs.desktop,
        targetExists: true,
        desktopCount: 2,
        sourceFingerprint: "sfp-1",
        targetFingerprint: "tfp-1",
        currentWorkspace: "ws-1",
    } as unknown as WorkspaceSendObserved;
}

function carryReply(correlation: string, follow: boolean): string {
    return JSON.stringify({
        v: WORKSPACE_SEND_CONTRACT_VERSION,
        correlation_id: correlation,
        outcome: "planned",
        kind: "send-to-workspace",
        base_revision: 0,
        detail: { kind: "send-to-workspace", policy_version: 1, capability: "move-tiled" },
        desired_geometry: [
            { window: "win-a", leaf: "leaf-win-a", output: "out-1", workspace: "ws-2", rect: { x: 0, y: 0, w: 600, h: 800 } },
            { window: "win-b", leaf: "leaf-win-b", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 1200, h: 800 } },
            { window: "win-t", leaf: "leaf-win-t", output: "out-1", workspace: "ws-2", rect: { x: 600, y: 0, w: 600, h: 800 } },
        ],
        desired_focus: follow
            ? { domain_output: "out-1", domain_workspace: "ws-2", leaf: "leaf-win-a" }
            : { domain_output: "out-1", domain_workspace: "ws-1", leaf: "leaf-win-b" },
        preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
        operation: follow
            ? {
                  op: "move-tiled",
                  window: "win-a",
                  leaf: "leaf-win-a",
                  source_output: "out-1",
                  source_workspace: "ws-1",
                  target_output: "out-1",
                  target_workspace: "ws-2",
              }
            : {
                  op: "move-tiled",
                  window: "win-a",
                  leaf: "leaf-win-a",
                  source_output: "out-1",
                  source_workspace: "ws-1",
                  target_output: "out-1",
                  target_workspace: "ws-2",
                  follow: false,
              },
    });
}

interface CarryFlight {
    readonly refs: { a: object; b: object; t: object; desktop: object };
    readonly world: CarryWorld;
    readonly geometries: object[];
    readonly moverWrites: object[];
    readonly switches: object[];
    readonly focuses: object[];
    readonly logs: string[];
    readonly dbusCalls: Array<{ method: string; payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly adapter: WorkspaceSendAdapter;
    activeRef: object | null;
}

function startCarryFlight(): CarryFlight {
    const refs = { a: {}, b: {}, t: {}, desktop: {} };
    const world = carryWorld(refs);
    const geometries: object[] = [];
    const moverWrites: object[] = [];
    const switches: object[] = [];
    const focuses: object[] = [];
    const logs: string[] = [];
    const dbusCalls: CarryFlight["dbusCalls"] = [];
    const callbacks: CarryFlight["callbacks"] = [];
    const flight: CarryFlight = {
        refs,
        world,
        geometries,
        moverWrites,
        switches,
        focuses,
        logs,
        dbusCalls,
        callbacks,
        adapter: null as unknown as WorkspaceSendAdapter,
        activeRef: refs.a,
    };
    const env: WorkspaceSendAdapterEnv = {
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            if (method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                callback(true);
                return;
            }
            dbusCalls.push({ method, payload });
            callbacks.push(callback);
        },
        scheduleOnce: (): (() => void) => (): void => {},
        log: (message) => {
            logs.push(message);
        },
        observe: () => carryObserved(world, refs, flight.activeRef),
        onSettled: () => {},
        setGeometry: (target) => {
            geometries.push(target);
            return true;
        },
        setDesktops: (target) => {
            moverWrites.push(target);
            for (const entry of world.windows) {
                if (entry.ref === target) {
                    entry.workspace = "ws-2";
                }
            }
            return true;
        },
        switchToTarget: (desktopRef) => {
            switches.push(desktopRef);
            return true;
        },
        focusWindow: (windowRef) => {
            focuses.push(windowRef);
            flight.activeRef = windowRef;
            return true;
        },
    };
    (flight as { adapter: WorkspaceSendAdapter }).adapter = new WorkspaceSendAdapter(env);
    assert.equal(flight.adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
    return flight;
}

function dispatchCarry(flight: CarryFlight, ordinal: unknown, follow: unknown): string {
    assert.equal(flight.adapter.requestSend("ws-2", ordinal, follow), true, "fullscreen carry must dispatch");
    assert.equal(flight.dbusCalls[0]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
    flight.callbacks[0]?.(":1.7");
    const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
    return body["correlation_id"] as string;
}

describe("D28 fullscreen send carry (adapter follow/stay)", () => {
    it("numbered stay carries fullscreen with source MRU focus and no overlay writes", () => {
        const flight = startCarryFlight();
        const correlation = dispatchCarry(flight, 2, false);
        flight.callbacks[1]?.(carryReply(correlation, false));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.moverWrites[0], flight.refs.a);
        assert.ok(!flight.geometries.includes(flight.refs.a), "no geometry write to the fullscreen mover");
        assert.ok(!flight.geometries.includes(flight.refs.t), "no geometry write to the destination overlay");
        assert.deepEqual(flight.switches, [], "stay never switches desktops");
        assert.deepEqual(flight.focuses, [flight.refs.b], "stay focuses the source MRU survivor");
        assert.ok(
            flight.logs.some((l) => l.includes("event=follow") && l.includes("outcome=stay-confirmed")),
            flight.logs.join("\n"),
        );
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("numbered follow carries fullscreen, switches, and focuses the mover", () => {
        const flight = startCarryFlight();
        const correlation = dispatchCarry(flight, 2, true);
        flight.callbacks[1]?.(carryReply(correlation, true));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.ok(!flight.geometries.includes(flight.refs.a), "no geometry write to the fullscreen mover");
        assert.ok(!flight.geometries.includes(flight.refs.t), "no geometry write to the destination overlay");
        assert.deepEqual(flight.switches, [flight.refs.desktop], "follow switches exactly once");
        assert.deepEqual(flight.focuses, [flight.refs.a], "follow focuses the fullscreen mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("follow with diagnostic ordinal -1 carries fullscreen like the numbered route", () => {
        // Adapter-level ordinal only (-1 is diagnostic-only here, never
        // entry ring resolution): real relative shortcuts are covered at
        // the production entry route.
        const flight = startCarryFlight();
        const correlation = dispatchCarry(flight, -1, true);
        flight.callbacks[1]?.(carryReply(correlation, true));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.ok(!flight.geometries.includes(flight.refs.a), "no geometry write to the fullscreen mover");
        assert.deepEqual(flight.switches, [flight.refs.desktop], "relative follow switches exactly once");
        assert.deepEqual(flight.focuses, [flight.refs.a], "relative follow focuses the mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("stay with diagnostic ordinal -1 carries fullscreen with source MRU focus", () => {
        // Adapter-level ordinal only (see above): real relative shortcuts
        // are covered at the production entry route.
        const flight = startCarryFlight();
        const correlation = dispatchCarry(flight, -1, false);
        flight.callbacks[1]?.(carryReply(correlation, false));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.switches, [], "relative stay never switches desktops");
        assert.deepEqual(flight.focuses, [flight.refs.b], "relative stay focuses the source MRU survivor");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("a mover fullscreen flag race refuses without setters or follow", () => {
        const flight = startCarryFlight();
        const correlation = dispatchCarry(flight, 2, true);
        // The mover exits fullscreen after dispatch: the homed flag fence
        // must refuse the carry instead of moving a changed window.
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.a) {
                entry.fullscreen = false;
            }
        }
        flight.callbacks[1]?.(carryReply(correlation, true));
        assert.deepEqual(flight.moverWrites, [], "flag-changed mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "flag-changed mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "flag-changed mover never switches");
        assert.deepEqual(flight.focuses, [], "flag-changed mover never focuses");
    });
});
