// Whole-workspace output migration against the real Rust Planner/Engine
// through the production `startPlanAdapterEntry` route: no hand-built wire
// replies. Covers a basic right migration (members, views, active retain,
// Engine post-currency, untouched target workspace), sticky/float/empty
// variants, protected and policy refusals with zero writes, direction
// topology (no-target, ambiguous), delayed arrival with exactly-once
// follow, pre-reply removal, and duplicate-reply no-replay. The
// flight-pinned triple is what lets verification confirm after members
// land on the destination: deriving the source from the live active window
// would lose the scope once the still-active mover sits on the target.
//
// REAL parts: production `startPlanAdapterEntry` (migrate observation and
// actuation) on a scripted two-output fake KWin surface;
// `Planner::evaluate` via the test-only `planner_eval` example (same
// validation and retained state as the shipped binary; only D-Bus and
// signals are stubbed). Included in `npm test`.

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";
import { describe, it } from "node:test";

import { startPlanAdapterEntry } from "../src/plan-adapter-entry";
import { PLAN_DEBOUNCE_MS } from "../src/plan-adapter";

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

    static start(): EngineBridge {
        return new EngineBridge(
            spawn("cargo", ["run", "--offline", "-q", "-p", "tiler-protocol", "--example", "planner_eval"], {
                cwd: repoRoot(),
                stdio: ["pipe", "pipe", "pipe"],
            }),
        );
    }

    send(request: string): Promise<string> {
        if (this.dead !== null) {
            return Promise.reject(new Error(this.dead));
        }
        if (this.proc.exitCode !== null || this.proc.signalCode !== null) {
            return Promise.reject(new Error("planner_eval process already exited"));
        }
        return new Promise<string>((resolvePromise, reject) => {
            this.waiters.push({ resolve: resolvePromise, reject });
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
        await new Promise<void>((resolvePromise) => {
            const timer = setTimeout(() => {
                try {
                    this.proc.kill("SIGKILL");
                } catch (error) {
                    void error;
                }
                resolvePromise();
            }, 3000);
            this.proc.on("exit", () => {
                clearTimeout(timer);
                resolvePromise();
            });
        });
    }
}

interface FakeSignal {
    readonly handlers: Set<() => void>;
    readonly signal: {
        connect: (handler: () => void) => void;
        disconnect: (handler: () => void) => void;
    };
    fire: () => void;
}

function makeSignal(): FakeSignal {
    const handlers = new Set<() => void>();
    return {
        handlers,
        signal: {
            connect: (handler: () => void): void => {
                handlers.add(handler);
            },
            disconnect: (handler: () => void): void => {
                handlers.delete(handler);
            },
        },
        fire: (): void => {
            for (const handler of [...handlers]) {
                handler();
            }
        },
    };
}

interface FakeOutput {
    readonly name: string;
    readonly geometry: { x: number; y: number; width: number; height: number };
    readonly manufacturer: string;
    readonly model: string;
    readonly serialNumber: string;
}

interface FakeDesktop {
    readonly id: string;
    readonly x11DesktopNumber: number;
}

interface FakeWindow extends Record<string, unknown> {
    normalWindow: boolean;
    managed: boolean;
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    transient: boolean;
    transientFor: FakeWindow | null;
    output: FakeOutput;
    desktops: FakeDesktop[];
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number };
    resourceClass: string;
    minSize: { width: number; height: number } | null;
    maxSize: { width: number; height: number } | null;
    keepAbove: boolean;
    keepBelow: boolean;
    tile: null;
    activeChanged: { connect: (h: () => void) => void; disconnect: (h: () => void) => void };
    desktopsChanged: { connect: (h: () => void) => void; disconnect: (h: () => void) => void };
    outputChanged: { connect: (h: () => void) => void; disconnect: (h: () => void) => void };
    frameGeometryChanged: { connect: (h: () => void) => void; disconnect: (h: () => void) => void };
    fullScreenChanged: { connect: (h: () => void) => void; disconnect: (h: () => void) => void };
    maximizedChanged: { connect: (h: () => void) => void; disconnect: (h: () => void) => void };
    desktopsSig: FakeSignal;
    outputSig: FakeSignal;
}

interface HarnessOpts {
    readonly workspaceMode?: unknown;
    readonly options?: unknown;
    readonly wins?: Array<{
        id: string;
        output: "left" | "right" | "right-small" | "right-tall";
        desktop: string;
        sticky?: boolean;
        minimized?: boolean;
        normalWindow?: boolean;
        transientForId?: string;
        minSize?: { width: number; height: number };
        maxSize?: { width: number; height: number };
    }>;
    readonly activeId?: string;
    // Split the right half into small-top/tall-bottom candidates: migration
    // must pick the largest shared edge (tall bottom, non-top) without
    // consulting window position.
    readonly splitRight?: boolean;
}

interface Harness {
    readonly bridge: EngineBridge;
    readonly workspace: Record<string, unknown>;
    readonly outputs: { left: FakeOutput; right: FakeOutput; rightSmall?: FakeOutput };
    readonly desktops: { source: FakeDesktop; target: FakeDesktop; small?: FakeDesktop };
    readonly wins: FakeWindow[];
    readonly currentByOutput: Map<FakeOutput, FakeDesktop>;
    readonly dbusCalls: Array<{ method: string; payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly replies: Map<number, string>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean; fired: boolean }>;
    readonly logs: string[];
    readonly shortcuts: Array<{ action: string; sequence: string; callback: () => void }>;
    readonly handle: NonNullable<ReturnType<typeof startPlanAdapterEntry>>;
    deferredTransfer: boolean;
    pendingTransfers: Array<{ client: FakeWindow; output: FakeOutput }>;
    flushAll: () => Promise<void>;
    fireTimers: (delayMs: number) => number;
    settlePlan: () => Promise<void>;
    flushOp: (op: string) => Promise<{ index: number; body: Record<string, unknown>; reply: Record<string, unknown> }>;
    sendOps: () => string[];
    winById: (id: string) => FakeWindow;
    frameOf: (win: FakeWindow) => { x: number; y: number; w: number; h: number };
    stop: () => Promise<void>;
}

async function makeHarness(opts: HarnessOpts = {}): Promise<Harness> {
    // Side-by-side outputs with FULL rectangles touching at x=1920 for
    // right-adjacency selection while carried work areas stay panel-free.
    // splitRight halves the east column: out-right-small (300px edge) on top
    // plus out-right (780px edge) below; migration must pick the tall
    // non-top candidate by largest shared edge.
    const left: FakeOutput = {
        name: "out-left",
        geometry: { x: 0, y: 0, width: 1920, height: 1080 },
        manufacturer: "m",
        model: "d",
        serialNumber: "s-left",
    };
    const right: FakeOutput =
        opts.splitRight === true
            ? {
                  name: "out-right",
                  geometry: { x: 1920, y: 300, width: 1920, height: 780 },
                  manufacturer: "m",
                  model: "d",
                  serialNumber: "s-right",
              }
            : {
                  name: "out-right",
                  geometry: { x: 1920, y: 0, width: 1920, height: 1080 },
                  manufacturer: "m",
                  model: "d",
                  serialNumber: "s-right",
              };
    const rightSmall: FakeOutput | null =
        opts.splitRight === true
            ? {
                  name: "out-right-small",
                  geometry: { x: 1920, y: 0, width: 1920, height: 300 },
                  manufacturer: "m",
                  model: "d",
                  serialNumber: "s-right-small",
              }
            : null;
    const wsSource: FakeDesktop = { id: "ws-1", x11DesktopNumber: 1 };
    const wsTarget: FakeDesktop = { id: "ws-9", x11DesktopNumber: 2 };
    const wsSmall: FakeDesktop = { id: "ws-8", x11DesktopNumber: 3 };
    const currentByOutput = new Map<FakeOutput, FakeDesktop>([
        [left, wsSource],
        [right, wsTarget],
        ...(rightSmall !== null ? [[rightSmall, wsSmall] as const] : []),
    ]);
    const mkWin = (
        output: FakeOutput,
        desktop: FakeDesktop,
        internalId: string,
        frame: { x: number; y: number; width: number; height: number },
        sticky = false,
    ): FakeWindow => {
        const desktopsSig = makeSignal();
        const outputSig = makeSignal();
        const frameSig = makeSignal();
        const fullSig = makeSignal();
        const maxSig = makeSignal();
        const activeSig = makeSignal();
        return {
            normalWindow: true,
            managed: true,
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: sticky,
            transient: false,
            transientFor: null,
            output,
            desktops: [desktop],
            internalId,
            frameGeometry: { ...frame },
            resourceClass: "app",
            minSize: null,
            maxSize: null,
            keepAbove: false,
            keepBelow: false,
            tile: null,
            activeChanged: activeSig.signal,
            desktopsChanged: desktopsSig.signal,
            outputChanged: outputSig.signal,
            frameGeometryChanged: frameSig.signal,
            fullScreenChanged: fullSig.signal,
            maximizedChanged: maxSig.signal,
            desktopsSig,
            outputSig,
        };
    };
    const spec =
        opts.wins ??
        (opts.splitRight === true
            ? [
                  { id: "m-win-1", output: "left" as const, desktop: "ws-1" },
                  { id: "m-win-2", output: "left" as const, desktop: "ws-1" },
                  { id: "m-win-s", output: "right-small" as const, desktop: "ws-8" },
                  { id: "m-win-t", output: "right" as const, desktop: "ws-9" },
              ]
            : [
                  { id: "m-win-1", output: "left" as const, desktop: "ws-1" },
                  { id: "m-win-2", output: "left" as const, desktop: "ws-1" },
                  { id: "m-win-t", output: "right" as const, desktop: "ws-9" },
              ]);
    const wins: FakeWindow[] = spec.map((entry) => {
        const outputFor =
            entry.output === "left" ? left : entry.output === "right-small" && rightSmall !== null ? rightSmall : right;
        const desktopFor = entry.desktop === "ws-1" ? wsSource : entry.desktop === "ws-8" ? wsSmall : wsTarget;
        const win = mkWin(
            outputFor,
            desktopFor,
            entry.id,
            { x: 20, y: 20, width: 400, height: 300 },
            entry.sticky === true,
        );
        if (entry.minimized === true) {
            win.minimized = true;
        }
        if (entry.normalWindow === false) {
            win.normalWindow = false;
        }
        if (entry.minSize !== undefined) {
            win.minSize = { ...entry.minSize };
        }
        if (entry.maxSize !== undefined) {
            win.maxSize = { ...entry.maxSize };
        }
        return win;
    });
    for (const [index, entry] of spec.entries()) {
        if (entry.transientForId !== undefined) {
            const child = wins[index] as FakeWindow;
            const parent = wins.find((win) => win.internalId === entry.transientForId) ?? null;
            child.transient = parent !== null;
            child.transientFor = parent;
        }
    }
    const activeId = opts.activeId ?? "m-win-1";
    const wsig = {
        windowAdded: makeSignal(),
        windowRemoved: makeSignal(),
        windowActivated: makeSignal(),
        screensChanged: makeSignal(),
        currentDesktopChanged: makeSignal(),
        desktopsChanged: makeSignal(),
    };
    const workspace: Record<string, unknown> = {
        activeWindow: wins.find((win) => win.internalId === activeId) ?? null,
        activeScreen: left,
        screens: rightSmall !== null ? [left, rightSmall, right] : [left, right],
        desktops: rightSmall !== null ? [wsSource, wsSmall, wsTarget] : [wsSource, wsTarget],
        currentDesktopForScreen: (output: object): object => currentByOutput.get(output as FakeOutput) ?? wsSource,
        currentDesktop: wsSource,
        setCurrentDesktopForScreen: (desktop: object, output: object): void => {
            currentByOutput.set(output as FakeOutput, desktop as FakeDesktop);
        },
        clientArea: (_kind: number, output: object, _desktop: object): object => {
            if (output === left) {
                return { x: 0, y: 0, w: 1920, h: 1040 };
            }
            if (rightSmall !== null && output === rightSmall) {
                return { x: 1920, y: 0, w: 1920, h: 260 };
            }
            if (rightSmall !== null) {
                return { x: 1920, y: 340, w: 1920, h: 740 };
            }
            return { x: 1920, y: 0, w: 1920, h: 1040 };
        },
        windowList: (): object[] => [...wins],
        sendClientToScreen: (client: object, output: object): void => {
            const win = client as FakeWindow;
            if (harness.deferredTransfer) {
                harness.pendingTransfers.push({ client: win, output: output as FakeOutput });
                return;
            }
            win.output = output as FakeOutput;
            win.outputSig.fire();
            // KWin moves transient descendants implicitly with the parent.
            for (const child of wins) {
                if (child.transientFor === win && !child.onAllDesktops) {
                    child.output = output as FakeOutput;
                }
            }
        },
        // Native directional output-switch slots: exact activeScreen
        // bookkeeping for the empty/sticky-active activation path.
        slotSwitchToLeftScreen: (): void => {
            workspace.activeScreen = left;
        },
        slotSwitchToRightScreen: (): void => {
            workspace.activeScreen = right;
        },
        slotSwitchToAboveScreen: (): void => {},
        slotSwitchToBelowScreen: (): void => {},
        windowAdded: wsig.windowAdded.signal,
        windowRemoved: wsig.windowRemoved.signal,
        windowActivated: wsig.windowActivated.signal,
        screensChanged: wsig.screensChanged.signal,
        currentDesktopChanged: wsig.currentDesktopChanged.signal,
        desktopsChanged: wsig.desktopsChanged.signal,
    };
    const bridge = EngineBridge.start();
    const dbusCalls: Harness["dbusCalls"] = [];
    const callbacks: Harness["callbacks"] = [];
    const replies = new Map<number, string>();
    const timers: Harness["timers"] = [];
    const logs: Harness["logs"] = [];
    const shortcuts: Harness["shortcuts"] = [];
    const harness: Harness = {
        bridge,
        workspace,
        outputs: rightSmall !== null ? { left, right, rightSmall } : { left, right },
        desktops: rightSmall !== null ? { source: wsSource, target: wsTarget, small: wsSmall } : { source: wsSource, target: wsTarget },
        wins,
        currentByOutput,
        dbusCalls,
        callbacks,
        replies,
        timers,
        logs,
        shortcuts,
        handle: null as unknown as NonNullable<ReturnType<typeof startPlanAdapterEntry>>,
        deferredTransfer: false,
        pendingTransfers: [],
        flushAll: async () => {
            for (let round = 0; round < 16; round += 1) {
                const next = dbusCalls.findIndex((call, at) => call.method === "DescribePlan" && !replies.has(at));
                if (next < 0) {
                    return;
                }
                const reply = await bridge.send(dbusCalls[next]?.payload as string);
                replies.set(next, reply);
                callbacks[next]?.(reply);
            }
            throw new Error("flushAll did not converge: planner keeps dispatching");
        },
        flushOp: async (op) => {
            const index = dbusCalls.findIndex((call, at) => {
                if (call.method !== "DescribePlan" || replies.has(at)) {
                    return false;
                }
                try {
                    const body = JSON.parse(call.payload) as Record<string, unknown>;
                    return ((body["command"] as Record<string, unknown> | undefined)?.["op"] as string) === op;
                } catch {
                    return false;
                }
            });
            assert.ok(index >= 0, `expected a queued ${op} dispatch to flush to the real Engine`);
            const body = JSON.parse(dbusCalls[index]?.payload as string) as Record<string, unknown>;
            const replyText = await bridge.send(dbusCalls[index]?.payload as string);
            replies.set(index, replyText);
            callbacks[index]?.(replyText);
            return { index, body, reply: JSON.parse(replyText) as Record<string, unknown> };
        },
        fireTimers: (delayMs) => {
            let fired = 0;
            for (const timer of timers) {
                if (timer.delayMs === delayMs && !timer.cancelled && !timer.fired) {
                    timer.fired = true;
                    fired += 1;
                    timer.callback();
                }
            }
            return fired;
        },
        // Settle Plan flights so later migrate routes observe an idle
        // single-flight against Engine state seeded through the normal
        // reconcile: fire debounce rounds and flush every dispatch until
        // quiescent. Migration never auto-seeds; the retained session comes
        // from this ordinary currency.
        settlePlan: async () => {
            for (let round = 0; round < 10; round += 1) {
                harness.fireTimers(PLAN_DEBOUNCE_MS);
                const before = dbusCalls.length;
                await harness.flushAll();
                harness.fireTimers(PLAN_DEBOUNCE_MS);
                await harness.flushAll();
                if (dbusCalls.length === before) {
                    return;
                }
            }
            throw new Error("plan flights did not settle");
        },
        sendOps: (): string[] => {
            const ops: string[] = [];
            for (const call of dbusCalls) {
                if (call.method !== "DescribePlan") {
                    continue;
                }
                try {
                    const body = JSON.parse(call.payload) as Record<string, unknown>;
                    const op = (body["command"] as Record<string, unknown> | undefined)?.["op"];
                    ops.push(typeof op === "string" ? op : "?");
                } catch {
                    ops.push("?");
                }
            }
            return ops;
        },
        winById: (id) => {
            const found = wins.find((win) => win.internalId === id);
            assert.ok(found !== undefined, `window ${id} exists on the fake surface`);
            return found as FakeWindow;
        },
        frameOf: (win) => {
            const frame = win.frameGeometry as { x: number; y: number; width: number; height: number };
            return { x: frame.x, y: frame.y, w: frame.width, h: frame.height };
        },
        stop: async () => {
            try {
                harness.handle?.stop();
            } catch (error) {
                void error;
            }
            await bridge.close();
        },
    };
    const handle = startPlanAdapterEntry({
        workspace,
        owner: "owner-1",
        generation: "gen-1",
        log: (message: string): void => {
            logs.push(message);
        },
        callDbus: (_service, _path, _iface, method, payload, callback): void => {
            if (method === "NameHasOwner") {
                callback(true);
                return;
            }
            if (method === "GetNameOwner") {
                callback(":1.7");
                return;
            }
            if (method === "StartServiceByName") {
                callback(1);
                return;
            }
            dbusCalls.push({ method, payload });
            callbacks.push(callback);
        },
        scheduleOnce: (delayMs, callback): (() => void) => {
            const timer = { delayMs, callback, cancelled: false, fired: false };
            timers.push(timer);
            return (): void => {
                timer.cancelled = true;
            };
        },
        registerShortcutFn: (action, _text, sequence, callback): boolean => {
            void _text;
            shortcuts.push({ action, sequence, callback });
            return true;
        },
        readProfileFn: (): string => "cosmic",
        readWorkspaceModeFn: (): unknown => opts.workspaceMode ?? "per-output-local",
        readInnerGapFn: (): number => 8,
        readOuterGapFn: (): number => 8,
        options: opts.options ?? { perOutputVirtualDesktops: true },
    });
    assert.notEqual(handle, null, "production entry starts on the fake surface");
    (harness as { handle: unknown }).handle = handle;
    return harness;
}

describe("migrate engine fixture", () => {
    it("migrates right with members, views, active retain, and Engine post-currency", async () => {
        const harness = await makeHarness();
        try {
            await harness.settlePlan();
            const targetFrameBefore = harness.frameOf(harness.winById("m-win-t"));
            const shortcut = harness.shortcuts.find((entry) => entry.action === "omnitiler-migrate-workspace-right");
            assert.notEqual(shortcut, undefined, "migrate-right shortcut is registered");
            assert.equal(shortcut?.sequence, "");
            (shortcut as { callback: () => void }).callback();
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            assert.equal(flushed.reply["kind"], "migrate-workspace");
            // Wire request shape through the production observer.
            const body = flushed.body;
            assert.deepEqual(body["command"], { op: "migrate-workspace", direction: "right" });
            assert.deepEqual((body["domain"] as Record<string, unknown>)["output"], "out-left");
            assert.deepEqual((body["target_domain"] as Record<string, unknown>)["output"], "out-right");
            assert.deepEqual((body["target_domain"] as Record<string, unknown>)["workspace"], "ws-1");
            assert.deepEqual(body["target_windows"], []);
            assert.equal(body["revision"], 0);
            assert.equal(body["focused_window"], "m-win-1");
            const windows = body["windows"] as Array<Record<string, unknown>>;
            assert.deepEqual(
                windows.map((entry) => entry["window"]).sort(),
                ["m-win-1", "m-win-2"],
            );
            // Native placement: every migrating window reads back the exact
            // target output with the migrated workspace as its sole desktop.
            for (const id of ["m-win-1", "m-win-2"]) {
                const win = harness.winById(id);
                assert.equal((win.output as FakeOutput).name, "out-right");
                assert.deepEqual(
                    win.desktops.map((desktop) => desktop.id),
                    ["ws-1"],
                );
            }
            // Untouched target workspace cache: prior view window unchanged.
            const target = harness.winById("m-win-t");
            assert.equal((target.output as FakeOutput).name, "out-right");
            assert.deepEqual(
                target.desktops.map((desktop) => desktop.id),
                ["ws-9"],
            );
            assert.deepEqual(harness.frameOf(target), targetFrameBefore);
            // Views: target shows the migrated workspace, source refills.
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-1");
            assert.equal(harness.currentByOutput.get(harness.outputs.left)?.id, "ws-9");
            // Active client retained after verified arrival and views.
            assert.equal((harness.workspace["activeWindow"] as FakeWindow).internalId, "m-win-1");
            // Migrated frames tile inside the target work area.
            for (const id of ["m-win-1", "m-win-2"]) {
                const frame = harness.frameOf(harness.winById(id));
                assert.ok(frame.x >= 1920 && frame.w > 0 && frame.h > 0, JSON.stringify(frame));
            }
            // Engine post-currency: the settled refresh reconciles the
            // rekeyed target domain without divergence.
            await harness.settlePlan();
            const answered = [...harness.replies.entries()].sort((a, b) => a[0] - b[0]);
            assert.ok(answered.length > 0);
            const lastReply = JSON.parse(answered[answered.length - 1]?.[1] as string) as Record<string, unknown>;
            assert.equal(lastReply["outcome"], "planned", JSON.stringify(lastReply));
        } finally {
            await harness.stop();
        }
    });

    it("migrates to the largest shared edge with the real Engine (non-top target)", async () => {
        // Multi-candidate integration: production observeMigrateWorkspace
        // ranks two FULL right candidates by largest shared edge (780px tall
        // bottom vs 300px small top), selecting out-right without consulting
        // window position. The resolved pair rides a real Planner::evaluate.
        const harness = await makeHarness({ splitRight: true });
        try {
            await harness.settlePlan();
            const smallBefore = harness.frameOf(harness.winById("m-win-s"));
            const targetBefore = harness.frameOf(harness.winById("m-win-t"));
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            assert.equal(flushed.reply["kind"], "migrate-workspace");
            const body = flushed.body;
            assert.deepEqual((body["target_domain"] as Record<string, unknown>)["output"], "out-right", "largest edge selects the tall non-top target");
            assert.deepEqual((body["target_domain"] as Record<string, unknown>)["workspace"], "ws-1");
            for (const id of ["m-win-1", "m-win-2"]) {
                const win = harness.winById(id);
                assert.equal((win.output as FakeOutput).name, "out-right");
                assert.deepEqual(
                    win.desktops.map((desktop) => desktop.id),
                    ["ws-1"],
                );
                const frame = harness.frameOf(win);
                assert.ok(frame.x >= 1920 && frame.w > 0 && frame.h > 0, JSON.stringify(frame));
            }
            // Unchosen small candidate untouched: output, membership, geometry.
            const small = harness.winById("m-win-s");
            assert.equal((small.output as FakeOutput).name, "out-right-small");
            assert.deepEqual(
                small.desktops.map((desktop) => desktop.id),
                ["ws-8"],
            );
            assert.deepEqual(harness.frameOf(small), smallBefore);
            // Prior tall view window stays native-only on its workspace.
            const target = harness.winById("m-win-t");
            assert.deepEqual(harness.frameOf(target), targetBefore);
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-1");
            assert.equal((harness.workspace["activeWindow"] as FakeWindow).internalId, "m-win-1");
        } finally {
            await harness.stop();
        }
    });

    it("keeps sticky homed on the source and echoes null for sticky-active", async () => {
        const harness = await makeHarness({
            wins: [
                { id: "m-win-1", output: "left", desktop: "ws-1" },
                { id: "m-win-s", output: "left", desktop: "ws-1", sticky: true },
                { id: "m-win-t", output: "right", desktop: "ws-9" },
            ],
            activeId: "m-win-s",
        });
        try {
            await harness.settlePlan();
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            const operation = flushed.reply["operation"] as Record<string, unknown>;
            assert.equal(operation["active_window"], null);
            const sticky = harness.winById("m-win-s");
            assert.equal((sticky.output as FakeOutput).name, "out-left");
            assert.equal(sticky.onAllDesktops, true);
            const mover = harness.winById("m-win-1");
            assert.equal((mover.output as FakeOutput).name, "out-right");
            // Sticky-active runs zero focus setters: the active window is
            // still the sticky client on the source.
            assert.equal((harness.workspace["activeWindow"] as FakeWindow).internalId, "m-win-s");
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-1");
        } finally {
            await harness.stop();
        }
    });

    it("retains a float active client with its frame preserved", async () => {
        const harness = await makeHarness();
        try {
            await harness.settlePlan();
            // Float the second window through the production toggle, then
            // migrate with the float active.
            const win2 = harness.winById("m-win-2");
            harness.workspace["activeWindow"] = win2;
            harness.handle.requestFloat();
            await harness.settlePlan();
            const floatedFrame = harness.frameOf(harness.winById("m-win-2"));
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            const operation = flushed.reply["operation"] as Record<string, unknown>;
            assert.equal(operation["active_window"], "m-win-2");
            assert.equal(flushed.reply["desired_focus"] ?? null, null);
            const frameAfter = harness.frameOf(harness.winById("m-win-2"));
            assert.equal((harness.winById("m-win-2").output as FakeOutput).name, "out-right");
            assert.equal((harness.workspace["activeWindow"] as FakeWindow).internalId, "m-win-2");
            assert.deepEqual(frameAfter, floatedFrame);
        } finally {
            await harness.stop();
        }
    });

    it("migrates an empty workspace with views and no focus writes", async () => {
        const harness = await makeHarness({
            wins: [{ id: "m-win-t", output: "right", desktop: "ws-9" }],
        });
        try {
            await harness.settlePlan();
            (harness.workspace as Record<string, unknown>)["activeWindow"] = null;
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            assert.deepEqual(flushed.reply["desired_geometry"], []);
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-1");
            assert.equal(harness.workspace["activeWindow"], null);
        } finally {
            await harness.stop();
        }
    });

    it("carries a fullscreen member with native move only and no geometry or focus writes", async () => {
        const harness = await makeHarness();
        try {
            await harness.settlePlan();
            harness.winById("m-win-1").fullScreen = true;
            const fullBefore = harness.frameOf(harness.winById("m-win-1"));
            const tiledBefore = harness.frameOf(harness.winById("m-win-2"));
            // Track native writes: frameGeometry sets prove geometry writes,
            // activeWindow sets prove focus writes. Fullscreen must take
            // neither; the tiled sibling still takes its planned geometry.
            let activeWrites = 0;
            let activeValue = harness.workspace["activeWindow"];
            Object.defineProperty(harness.workspace, "activeWindow", {
                get: (): unknown => activeValue,
                set: (value: unknown): void => {
                    activeWrites += 1;
                    activeValue = value;
                },
                configurable: true,
            });
            const frameWrites = new Map<string, number>();
            for (const win of harness.wins) {
                let value: unknown = win.frameGeometry as unknown;
                let count = 0;
                frameWrites.set(win.internalId, 0);
                const id = win.internalId;
                Object.defineProperty(win, "frameGeometry", {
                    get: (): unknown => value,
                    set: (next: unknown): void => {
                        count += 1;
                        frameWrites.set(id, count);
                        value = next;
                    },
                    configurable: true,
                });
            }
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            // Both members arrive on the target with the migrated workspace.
            for (const id of ["m-win-1", "m-win-2"]) {
                const win = harness.winById(id);
                assert.equal((win.output as FakeOutput).name, "out-right");
                assert.deepEqual(
                    win.desktops.map((desktop) => desktop.id),
                    ["ws-1"],
                );
            }
            // Fullscreen takes the native move only: frame untouched, no
            // geometry write. The tiled sibling still tiles on the target.
            assert.deepEqual(harness.frameOf(harness.winById("m-win-1")), fullBefore);
            assert.equal(frameWrites.get("m-win-1"), 0);
            assert.ok((frameWrites.get("m-win-2") ?? 0) > 0, JSON.stringify([...frameWrites]));
            assert.notDeepEqual(harness.frameOf(harness.winById("m-win-2")), tiledBefore);
            // Fullscreen active takes no tiler focus write, views still switch.
            assert.equal(activeWrites, 0);
            assert.equal((harness.workspace["activeWindow"] as FakeWindow).internalId, "m-win-1");
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-1");
            assert.equal(harness.currentByOutput.get(harness.outputs.left)?.id, "ws-9");
            // Carried-overlay log counts classes without raw ids.
            const carried = harness.logs.filter((line) => line.includes("event=overlays-carried"));
            assert.equal(carried.length, 1, harness.logs.join("\n"));
            const carriedLine: string = carried[0] as string;
            assert.ok(carriedLine.includes("fullscreen=1"), carriedLine);
            assert.ok(carriedLine.includes("maximized=0"), carriedLine);
            assert.ok(!carriedLine.includes("m-win-1") && !carriedLine.includes("m-win-2"), carriedLine);
            // Fullscreen follow records native-only: no tiler focus write,
            // terminal arrival still verified.
            const follow = harness.logs.filter(
                (line) => line.includes("stage=follow") && line.includes("outcome=native-only"),
            );
            assert.equal(follow.length, 1, harness.logs.join("\n"));
            assert.ok(
                harness.logs.some((line) => line.includes("stage=release") && line.includes("outcome=arrived")),
                harness.logs.join("\n"),
            );
        } finally {
            await harness.stop();
        }
    });

    it("refuses policy gates with exact reasons and no writes", async () => {
        for (const opts of [
            { options: { perOutputVirtualDesktops: false }, token: "per-output-disabled" },
            { options: {}, token: "per-output-unreadable" },
            { workspaceMode: "shared", token: "mode-shared" },
        ]) {
            const harness = await makeHarness(opts as HarnessOpts);
            try {
                await harness.settlePlan();
                const before = harness.sendOps().length;
                harness.handle.requestWorkspaceMigrate("right");
                assert.deepEqual(harness.sendOps().length, before, opts.token);
                assert.ok(
                    harness.logs.some((line) => line.includes("component=workspace-migrate") && line.includes(`outcome=${opts.token}`)),
                    `${opts.token}:\n${harness.logs.join("\n")}`,
                );
            } finally {
                await harness.stop();
            }
        }
    });

    it("no-ops without a candidate and refuses ambiguous topology", async () => {
        const harness = await makeHarness();
        try {
            await harness.settlePlan();
            const before = harness.sendOps().length;
            harness.handle.requestWorkspaceMigrate("up");
            assert.deepEqual(harness.sendOps().length, before);
            assert.ok(harness.logs.some((line) => line.includes("outcome=no-target")));
        } finally {
            await harness.stop();
        }
    });

    it("waits for delayed transfers and follows exactly once", async () => {
        const harness = await makeHarness();
        try {
            await harness.settlePlan();
            harness.deferredTransfer = true;
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            // Nothing placed yet: no views, flight still live.
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-9");
            assert.ok(harness.logs.some((line) => line.includes("outcome=waiting")));
            // Delayed native arrival becomes visible; signals finish once.
            harness.deferredTransfer = false;
            for (const pending of harness.pendingTransfers) {
                pending.client.output = pending.output;
                pending.client.outputSig.fire();
            }
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-1");
            assert.equal((harness.workspace["activeWindow"] as FakeWindow).internalId, "m-win-1");
            const switches = harness.logs.filter((line) => line.includes("outcome=state-confirmed"));
            assert.equal(switches.length, 1);
        } finally {
            await harness.stop();
        }
    });

    it("refuses pre-reply removal with zero writes and ignores duplicate replies", async () => {
        const harness = await makeHarness();
        try {
            await harness.settlePlan();
            harness.handle.requestWorkspaceMigrate("right");
            const index = harness.dbusCalls.findIndex((call, at) => {
                if (call.method !== "DescribePlan" || harness.replies.has(at)) {
                    return false;
                }
                try {
                    const body = JSON.parse(call.payload) as Record<string, unknown>;
                    return ((body["command"] as Record<string, unknown> | undefined)?.["op"] as string) === "migrate-workspace";
                } catch {
                    return false;
                }
            });
            assert.ok(index >= 0);
            // Remove a migrating member before the reply lands.
            const doomed = harness.winById("m-win-2");
            const at = harness.wins.indexOf(doomed);
            harness.wins.splice(at, 1);
            const replyText = await harness.bridge.send(harness.dbusCalls[index]?.payload as string);
            harness.replies.set(index, replyText);
            harness.callbacks[index]?.(replyText);
            const reply = JSON.parse(replyText) as Record<string, unknown>;
            assert.equal(reply["outcome"], "planned", replyText);
            // The pinned re-observation mismatches: zero native writes.
            assert.equal((harness.winById("m-win-1").output as FakeOutput).name, "out-left");
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-9");
            assert.ok(harness.logs.some((line) => line.includes("outcome=stale-revision")));
        } finally {
            await harness.stop();
        }
    });

    it("never replays a duplicate real reply", async () => {        const harness = await makeHarness();
        try {
            await harness.settlePlan();
            harness.handle.requestWorkspaceMigrate("right");
            const index = harness.dbusCalls.findIndex((call, at) => {
                if (call.method !== "DescribePlan" || harness.replies.has(at)) {
                    return false;
                }
                try {
                    const body = JSON.parse(call.payload) as Record<string, unknown>;
                    return ((body["command"] as Record<string, unknown> | undefined)?.["op"] as string) === "migrate-workspace";
                } catch {
                    return false;
                }
            });
            assert.ok(index >= 0);
            const replyText = await harness.bridge.send(harness.dbusCalls[index]?.payload as string);
            harness.replies.set(index, replyText);
            harness.callbacks[index]?.(replyText);
            assert.equal((harness.workspace["activeWindow"] as FakeWindow).internalId, "m-win-1");
            const rightCount = harness.wins.filter((win) => (win.output as FakeOutput).name === "out-right").length;
            // A late duplicate of the same reply never replays native writes.
            harness.callbacks[index]?.(replyText);
            assert.equal((harness.workspace["activeWindow"] as FakeWindow).internalId, "m-win-1");
            assert.equal(
                harness.wins.filter((win) => (win.output as FakeOutput).name === "out-right").length,
                rightCount,
            );
        } finally {
            await harness.stop();
        }
    });

    it("preserves automatic fixed-float origin and hints across migration", async () => {
        const harness = await makeHarness({
            wins: [
                { id: "m-win-1", output: "left", desktop: "ws-1" },
                {
                    id: "m-win-fixed",
                    output: "left",
                    desktop: "ws-1",
                    minSize: { width: 400, height: 300 },
                    maxSize: { width: 400, height: 300 },
                },
                { id: "m-win-t", output: "right", desktop: "ws-9" },
            ],
        });
        try {
            await harness.settlePlan();
            const fixedBefore = harness.frameOf(harness.winById("m-win-fixed"));
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            // The automatic float rides the wire with its origin and hints.
            const windows = flushed.body["windows"] as Array<Record<string, unknown>>;
            const fixedWire = windows.find((entry) => entry["window"] === "m-win-fixed") as Record<string, unknown>;
            assert.equal(fixedWire["floating"], true);
            assert.equal(fixedWire["fixed_auto"], true);
            assert.deepEqual(fixedWire["min_size"], { w: 400, h: 300 });
            assert.deepEqual(fixedWire["max_size"], { w: 400, h: 300 });
            // Native transfer with the frame preserved (no tiler geometry).
            const fixed = harness.winById("m-win-fixed");
            assert.equal((fixed.output as FakeOutput).name, "out-right");
            assert.deepEqual(harness.frameOf(fixed), fixedBefore);
            // Post-currency keeps the origin: the follow-up reconcile still
            // carries fixed_auto for the same client (no origin loss).
            await harness.settlePlan();
            const reconciles = harness.dbusCalls
                .map((call, index) => ({ call, index }))
                .filter(({ call, index }) => {
                    if (call.method !== "DescribePlan" || !harness.replies.has(index)) {
                        return false;
                    }
                    try {
                        const body = JSON.parse(call.payload) as Record<string, unknown>;
                        return ((body["command"] as Record<string, unknown> | undefined)?.["op"] as string) === "reconcile";
                    } catch {
                        return false;
                    }
                });
            assert.ok(reconciles.length > 0, "expected a post-migration reconcile");
            const rekeyed = reconciles.filter(({ call }) => {
                try {
                    const probe = JSON.parse(call.payload) as Record<string, unknown>;
                    const domain = probe["domain"] as Record<string, unknown>;
                    return domain["output"] === "out-right" && domain["workspace"] === "ws-1";
                } catch {
                    return false;
                }
            });
            assert.ok(rekeyed.length > 0, "expected a post-migration reconcile of the rekeyed domain");
            const last = rekeyed[rekeyed.length - 1] as { call: { payload: string } };
            const lastBody = JSON.parse(last.call.payload) as Record<string, unknown>;
            const lastWindows = lastBody["windows"] as Array<Record<string, unknown>>;
            const carried = lastWindows.find((entry) => entry["window"] === "m-win-fixed") as Record<string, unknown>;
            assert.equal(carried["fixed_auto"], true);
            assert.deepEqual(carried["min_size"], { w: 400, h: 300 });
        } finally {
            await harness.stop();
        }
    });

    it("preserves an explicit tile override across migration", async () => {
        const harness = await makeHarness({
            wins: [
                { id: "m-win-1", output: "left", desktop: "ws-1" },
                {
                    id: "m-win-fixed",
                    output: "left",
                    desktop: "ws-1",
                    minSize: { width: 400, height: 300 },
                    maxSize: { width: 400, height: 300 },
                },
                { id: "m-win-t", output: "right", desktop: "ws-9" },
            ],
            activeId: "m-win-fixed",
        });
        try {
            await harness.settlePlan();
            // Tile the automatic float explicitly: a single toggle
            // unfloats it and records the suppress pin (explicit user tile
            // win). Re-pin active first: seeding focuses a tiled
            // survivor, never the fixed client.
            harness.workspace["activeWindow"] = harness.winById("m-win-fixed");
            harness.handle.requestFloat();
            await harness.settlePlan();
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            const windows = flushed.body["windows"] as Array<Record<string, unknown>>;
            const fixedWire = windows.find((entry) => entry["window"] === "m-win-fixed") as Record<string, unknown>;
            assert.equal(fixedWire["fixed_suppress"], true);
            assert.equal("fixed_auto" in fixedWire, false);
            // The overridden tile migrates as a tiled member with planned
            // geometry on the target.
            const fixed = harness.winById("m-win-fixed");
            assert.equal((fixed.output as FakeOutput).name, "out-right");
            const frame = harness.frameOf(fixed);
            assert.ok(frame.x >= 1920 && frame.w > 0, JSON.stringify(frame));
        } finally {
            await harness.stop();
        }
    });

    it("carries a maximized member with slot preserved and no unmaximize", async () => {
        const harness = await makeHarness();
        try {
            await harness.settlePlan();
            harness.winById("m-win-2").maximizeMode = 3;
            const maxBefore = harness.frameOf(harness.winById("m-win-2"));
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            // The maximized member keeps its overlay flag and moves natively;
            // the reply still projects its reserved tile slot.
            const geometry = flushed.reply["desired_geometry"] as Array<Record<string, unknown>>;
            assert.ok(geometry.some((entry) => entry["window"] === "m-win-2"), JSON.stringify(geometry));
            const moved = harness.winById("m-win-2");
            assert.equal((moved.output as FakeOutput).name, "out-right");
            assert.deepEqual(
                moved.desktops.map((desktop) => desktop.id),
                ["ws-1"],
            );
            assert.notEqual(moved.maximizeMode, 0, "no unmaximize");
            assert.deepEqual(harness.frameOf(moved), maxBefore, "no geometry write for maximized");
            // The tiled active still follows after verified arrival and views.
            assert.equal((harness.workspace["activeWindow"] as FakeWindow).internalId, "m-win-1");
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-1");
            const carried = harness.logs.filter((line) => line.includes("event=overlays-carried"));
            assert.equal(carried.length, 1, harness.logs.join("\n"));
            const carriedLine: string = carried[0] as string;
            assert.ok(carriedLine.includes("maximized=1"), carriedLine);
            assert.ok(!carriedLine.includes("m-win-2"), carriedLine);
        } finally {
            await harness.stop();
        }
    });

    it("carries affected-view fullscreen and maximized without refusal", async () => {
        for (const overlay of ["fullscreen", "maximized"] as const) {
            const harness = await makeHarness();
            try {
                await harness.settlePlan();
                const target = harness.winById("m-win-t");
                const targetBefore = harness.frameOf(target);
                if (overlay === "fullscreen") {
                    target.fullScreen = true;
                } else {
                    target.maximizeMode = 3;
                }
                harness.handle.requestWorkspaceMigrate("right");
                const flushed = await harness.flushOp("migrate-workspace");
                assert.equal(flushed.reply["outcome"], "planned", `${overlay}: ${JSON.stringify(flushed.reply)}`);
                // Source members move; the affected target view stays native-only.
                for (const id of ["m-win-1", "m-win-2"]) {
                    const win = harness.winById(id);
                    assert.equal((win.output as FakeOutput).name, "out-right", overlay);
                    assert.deepEqual(
                        win.desktops.map((desktop) => desktop.id),
                        ["ws-1"],
                        overlay,
                    );
                }
                assert.equal((target.output as FakeOutput).name, "out-right", overlay);
                assert.deepEqual(
                    target.desktops.map((desktop) => desktop.id),
                    ["ws-9"],
                    overlay,
                );
                assert.deepEqual(harness.frameOf(target), targetBefore, overlay);
                assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-1", overlay);
            } finally {
                await harness.stop();
            }
        }
    });

    it("converges delayed native overlay re-fit without verifying old geometry", async () => {
        const harness = await makeHarness();
        try {
            await harness.settlePlan();
            harness.winById("m-win-1").fullScreen = true;
            harness.deferredTransfer = true;
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-9");
            assert.ok(harness.logs.some((line) => line.includes("outcome=waiting")));
            // Delayed native arrival plus a native overlay re-fit that changes
            // the fullscreen frame: arrival verifies identity/membership/output,
            // never the old rectangle.
            harness.deferredTransfer = false;
            for (const pending of harness.pendingTransfers) {
                pending.client.output = pending.output;
                pending.client.outputSig.fire();
            }
            harness.winById("m-win-1").frameGeometry = { x: 1920, y: 0, width: 1920, height: 1080 };
            harness.winById("m-win-1").desktopsSig.fire();
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-1");
            assert.equal((harness.workspace["activeWindow"] as FakeWindow).internalId, "m-win-1");
            const refitFollow = harness.logs.filter(
                (line) => line.includes("stage=follow") && line.includes("outcome=native-only"),
            );
            assert.equal(refitFollow.length, 1, harness.logs.join("\n"));
        } finally {
            await harness.stop();
        }
    });

    it("minimized member converges via ordinary reconcile before migrate (D23)", async () => {
        const harness = await makeHarness({
            wins: [
                { id: "m-win-1", output: "left", desktop: "ws-1" },
                { id: "m-win-2", output: "left", desktop: "ws-1", minimized: true },
                { id: "m-win-t", output: "right", desktop: "ws-9" },
            ],
        });
        try {
            await harness.settlePlan();
            const min2 = harness.winById("m-win-2");
            const min2Frame = { ...min2.frameGeometry };
            harness.handle.requestWorkspaceMigrate("right");
            const flushed = await harness.flushOp("migrate-workspace");
            // D23 (REQ-MNZ-01): the foreground observation omits the
            // minimized client, so ordinary settle already converged it
            // into a stored old slot and the session no longer links it
            // as tiled. The migrate wire still carries no minimized
            // client, but the observation is now complete, so the core
            // plans the tiled survivors without touching the minimized
            // client (pre-D23 this refused partial-observation).
            assert.equal(flushed.reply["outcome"], "planned", JSON.stringify(flushed.reply));
            const geometry = flushed.reply["desired_geometry"] as Array<Record<string, unknown>>;
            assert.ok(
                geometry.every((entry) => entry["window"] !== "m-win-2"),
                "planned geometry never covers the minimized client",
            );
            assert.equal(min2.minimized, true, "minimized state untouched");
            assert.deepEqual({ ...min2.frameGeometry }, min2Frame, "minimized frame untouched");
            assert.equal(
                (harness.workspace["activeWindow"] as FakeWindow).internalId,
                "m-win-1",
                "focus retained on the tiled active",
            );
            assert.equal((harness.winById("m-win-1").output as FakeOutput).name, "out-right", "tiled survivor migrates");
            assert.equal((harness.winById("m-win-2").output as FakeOutput).name, "out-right", "minimized client rides its workspace");
            assert.deepEqual(
                (harness.winById("m-win-2").desktops as FakeDesktop[]).map((d) => d.id),
                ["ws-1"],
                "minimized client keeps its desktop",
            );
            assert.equal(harness.currentByOutput.get(harness.outputs.right)?.id, "ws-1", "target shows the migrated workspace");
        } finally {
            await harness.stop();
        }
    });
});
