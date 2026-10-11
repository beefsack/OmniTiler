// KWin 6.6 (legacy-shared) compatibility: the workspace exposes no
// currentDesktopForScreen/setCurrentDesktopForScreen methods and options
// carries no perOutputVirtualDesktops property. The shared currentDesktop is
// then current on every output; saved/requested per-output modes degrade
// predictably to shared with one honest diagnostic.
//
// Fixtures below are 6.6-shaped by ABSENCE (the keys do not exist), so every
// row fails if a code path touches the 6.7-only API without the
// workspace-native capability seam. 6.7 behavior is covered by the existing
// suite; this file only pins the fallback.

import assert from "node:assert/strict";
import { spawn, type ChildProcess } from "node:child_process";
import { existsSync } from "node:fs";
import { createInterface } from "node:readline";
import { dirname, join, resolve } from "node:path";
import { describe, it } from "node:test";

import { PLAN_DEBOUNCE_MS } from "../src/plan-adapter";
import { WORKSPACE_SEND_TIMEOUT_MS } from "../src/workspace-send-adapter";
import { readDomainGaps } from "../src/domain-gap";
import {
    observeDirectionalDomain,
    observeHiddenDomains,
    observeOutputSendTarget,
    observeSendTarget,
    startPlanAdapterEntry,
} from "../src/plan-adapter-entry";
import {
    hasPerOutputSwitch,
    readDesktopForOutput,
    readPerOutputVirtualDesktopsFlag,
    resolveEffectiveWorkspaceMode,
    supportsPerOutputDesktops,
    writeDesktopForOutput,
    WorkspaceNativeAdapter,
} from "../src/workspace-native";

function inertSignal(): { connect: () => void; disconnect: () => void } {
    return { connect: (): void => {}, disconnect: (): void => {} };
}

interface Fixture66 {
    readonly workspace: Record<string, unknown>;
    readonly outputs: Array<Record<string, unknown>>;
    readonly desktops: Array<Record<string, unknown>>;
    readonly wins: Array<Record<string, unknown>>;
}

// 6.6-shaped world: two outputs, two desktops, one shared current desktop,
// no per-output methods, no options property. Windows: win-a active on
// (out-1, ws-1), win-b on (out-2, ws-1), win-c background on (out-1, ws-2).
function world66(): Fixture66 {
    const out1: Record<string, unknown> = {
        name: "out-1",
        manufacturer: "m",
        model: "d",
        serialNumber: "s",
        geometry: { x: 0, y: 0, width: 1200, height: 800 },
    };
    const out2: Record<string, unknown> = {
        name: "out-2",
        manufacturer: "m",
        model: "d",
        serialNumber: "s",
        geometry: { x: 1200, y: 0, width: 1200, height: 800 },
    };
    const ws1: Record<string, unknown> = { id: "ws-1", x11DesktopNumber: 1 };
    const ws2: Record<string, unknown> = { id: "ws-2", x11DesktopNumber: 2 };
    const winA: Record<string, unknown> = {
        normalWindow: true,
        internalId: "win-a",
        resourceClass: "test-app",
        output: out1,
        desktops: [ws1],
        frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
        moveResizedChanged: inertSignal(),
        fullScreenChanged: inertSignal(),
        fullScreen: false,
        maximizedChanged: inertSignal(),
        maximizeMode: 0,
        desktopsChanged: inertSignal(),
        onAllDesktops: false,
    };
    const winB: Record<string, unknown> = {
        normalWindow: true,
        internalId: "win-b",
        resourceClass: "test-app",
        output: out2,
        desktops: [ws1],
        frameGeometry: { x: 1200, y: 0, width: 600, height: 800 },
        moveResizedChanged: inertSignal(),
        fullScreenChanged: inertSignal(),
        fullScreen: false,
        maximizedChanged: inertSignal(),
        maximizeMode: 0,
        desktopsChanged: inertSignal(),
        onAllDesktops: false,
    };
    const winC: Record<string, unknown> = {
        normalWindow: true,
        internalId: "win-c",
        resourceClass: "test-app",
        output: out1,
        desktops: [ws2],
        frameGeometry: { x: 0, y: 0, width: 600, height: 800 },
        moveResizedChanged: inertSignal(),
        fullScreenChanged: inertSignal(),
        fullScreen: false,
        maximizedChanged: inertSignal(),
        maximizeMode: 0,
        desktopsChanged: inertSignal(),
        onAllDesktops: false,
    };
    const desktops = [ws1, ws2];
    const workspace: Record<string, unknown> = {
        activeWindow: winA,
        activeScreen: out1,
        windowList: (): unknown[] => [winA, winB, winC],
        screens: [out1, out2],
        desktops,
        // Shared current desktop: current on EVERY output on 6.6.
        currentDesktop: ws1,
        // NOTE: no currentDesktopForScreen / setCurrentDesktopForScreen.
        createDesktop: (position: unknown, _name: unknown): void => {
            void position;
            const fresh: Record<string, unknown> = {
                id: `ws-new-${String(desktops.length + 1)}`,
                x11DesktopNumber: desktops.length + 1,
            };
            desktops.push(fresh);
        },
        removeDesktop: (desktop: unknown): void => {
            const at = desktops.indexOf(desktop as Record<string, unknown>);
            if (at >= 0) {
                desktops.splice(at, 1);
            }
        },
        clientArea: (): unknown => ({ x: 0, y: 0, width: 1200, height: 800 }),
        windowAdded: inertSignal(),
        windowRemoved: inertSignal(),
        windowActivated: inertSignal(),
        screensChanged: inertSignal(),
        desktopsChanged: inertSignal(),
        currentDesktopChanged: inertSignal(),
    };
    return { workspace, outputs: [out1, out2], desktops, wins: [winA, winB, winC] };
}

function world67(): Fixture66 {
    const world = world66();
    const byOutput = new Map<unknown, unknown>();
    for (const output of world.outputs) {
        byOutput.set(output, world.desktops[0]);
    }
    world.workspace["currentDesktopForScreen"] = (output: unknown): unknown => byOutput.get(output) ?? null;
    world.workspace["setCurrentDesktopForScreen"] = (desktop: unknown, output: unknown): void => {
        byOutput.set(output, desktop);
        world.workspace["currentDesktop"] = desktop;
    };
    return world;
}

function startEntry66(
    world: Fixture66,
    mode: unknown,
): { handle: NonNullable<ReturnType<typeof startPlanAdapterEntry>>; logs: string[]; dbusCalls: Array<{ method: string; payload: string }> } {
    const logs: string[] = [];
    const dbusCalls: Array<{ method: string; payload: string }> = [];
    const handle = startPlanAdapterEntry({
        workspace: world.workspace,
        options: {},
        callDbus: (_s, _p, _i, method, payload, callback): void => {
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
        },
        scheduleOnce: () => () => {},
        log: (message): void => {
            logs.push(message);
        },
        owner: "owner-1",
        generation: "gen-1",
        registerShortcutFn: () => true,
        readProfileFn: () => "cosmic",
        readWorkspaceModeFn: () => mode,
    });
    assert.ok(handle !== null);
    return { handle, logs, dbusCalls };
}

describe("kwin 6.6 legacy-shared capability seam", () => {
    it("detects per-output support by the read method only", () => {
        const legacy = world66();
        assert.equal(supportsPerOutputDesktops(legacy.workspace), false);
        assert.equal(hasPerOutputSwitch(legacy.workspace), false);
        const modern = world67();
        assert.equal(supportsPerOutputDesktops(modern.workspace), true);
        assert.equal(hasPerOutputSwitch(modern.workspace), true);
        assert.equal(supportsPerOutputDesktops(null), false);
    });

    it("degrades requested per-output modes to shared on legacy hosts", () => {
        const legacy = world66();
        assert.equal(resolveEffectiveWorkspaceMode("per-output-local", legacy.workspace), "shared");
        assert.equal(resolveEffectiveWorkspaceMode("global-unique", legacy.workspace), "shared");
        assert.equal(resolveEffectiveWorkspaceMode("shared", legacy.workspace), "shared");
        const modern = world67();
        assert.equal(resolveEffectiveWorkspaceMode("per-output-local", modern.workspace), "per-output-local");
        assert.equal(resolveEffectiveWorkspaceMode("global-unique", modern.workspace), "global-unique");
    });

    it("reports the absent migration-only options property as null", () => {
        assert.equal(readPerOutputVirtualDesktopsFlag({}), null);
        assert.equal(readPerOutputVirtualDesktopsFlag({ perOutputVirtualDesktops: true }), true);
        assert.equal(readPerOutputVirtualDesktopsFlag({ perOutputVirtualDesktops: false }), false);
    });

    it("reads the shared current desktop for every output on legacy hosts", () => {
        const world = world66();
        for (const output of world.outputs) {
            const read = readDesktopForOutput(world.workspace, output);
            assert.ok(read !== null);
            assert.equal(read.id, "ws-1");
            assert.equal(read.ref, world.desktops[0]);
        }
        assert.equal(readDesktopForOutput(null, world.outputs[0] as object), null);
    });

    it("writes the shared current desktop on legacy hosts and stays fail-closed on partial shapes", () => {
        const world = world66();
        assert.equal(writeDesktopForOutput(world.workspace, world.desktops[1] as object, world.outputs[0] as object), true);
        assert.equal(world.workspace["currentDesktop"], world.desktops[1]);
        // Partial shape (read method present, setter missing) keeps the
        // historical fail-closed refusal: no shared write is attempted.
        const partial = world67();
        delete partial.workspace["setCurrentDesktopForScreen"];
        assert.equal(supportsPerOutputDesktops(partial.workspace), true);
        assert.equal(writeDesktopForOutput(partial.workspace, world.desktops[1] as object, world.outputs[0] as object), false);
    });

    it("reflects a refused shared write instead of claiming success", () => {
        const world = world66();
        const shared = world.desktops[0];
        Object.defineProperty(world.workspace, "currentDesktop", {
            configurable: true,
            get: (): unknown => shared,
        });
        assert.equal(writeDesktopForOutput(world.workspace, world.desktops[1] as object, world.outputs[0] as object), false);
    });
});

describe("kwin 6.6 workspace navigation degrades to shared", () => {
    function startNative66(mode: unknown): { adapter: WorkspaceNativeAdapter; logs: string[]; world: Fixture66 } {
        const world = world66();
        const logs: string[] = [];
        const adapter = new WorkspaceNativeAdapter({
            getWorkspace: () => world.workspace,
            readWorkspaceMode: () => mode,
            log: (message): void => {
                logs.push(message);
            },
        });
        assert.equal(adapter.enable(), true);
        return { adapter, logs, world };
    }

    it("degrades per-output-local and global-unique with one honest diagnostic", () => {
        for (const mode of ["per-output-local", "global-unique"] as const) {
            const { adapter, logs } = startNative66(mode);
            assert.equal(adapter.getMode(), "shared");
            assert.ok(
                logs.some((line) => line === `omnitiler:workspace:workspace-mode-degraded:${mode}-to-shared`),
                `degrade diagnostic for ${mode}: ${JSON.stringify(logs)}`,
            );
        }
    });

    it("leaves shared and 6.7 modes untouched", () => {
        const { adapter, logs } = startNative66("shared");
        assert.equal(adapter.getMode(), "shared");
        assert.ok(!logs.some((line) => line.includes("workspace-mode-degraded")), "no degrade noise for shared");
        const modern = world67();
        const modernLogs: string[] = [];
        const modernAdapter = new WorkspaceNativeAdapter({
            getWorkspace: () => modern.workspace,
            readWorkspaceMode: () => "per-output-local",
            log: (message): void => {
                modernLogs.push(message);
            },
        });
        assert.equal(modernAdapter.enable(), true);
        assert.equal(modernAdapter.getMode(), "per-output-local");
        assert.ok(!modernLogs.some((line) => line.includes("workspace-mode-degraded")), "no degrade noise on 6.7");
    });

    it("switches the shared desktop on every output", () => {
        const { adapter, world } = startNative66("per-output-local");
        assert.equal(adapter.currentScopeId(), "ws-1");
        assert.equal(adapter.selectLogical(2), true);
        assert.equal(world.workspace["currentDesktop"], world.desktops[1]);
        assert.equal(adapter.currentScopeId(), "ws-2");
        assert.equal(adapter.selectRelative(-1), true);
        assert.equal(world.workspace["currentDesktop"], world.desktops[0]);
    });
});

describe("kwin 6.6 observation treats shared current as current everywhere", () => {
    it("observes hidden background domains on both outputs", () => {
        const world = world66();
        const gaps = readDomainGaps({ readInnerGapFn: () => 8, readOuterGapFn: () => 8 });
        const hidden = observeHiddenDomains(world.workspace, new Map(), new Set(), gaps);
        const keys = hidden.map((entry) => `${entry.domainOutput}\u0000${entry.domainWorkspace}`).sort();
        assert.deepEqual(keys, ["out-1\u0000ws-2", "out-2\u0000ws-1", "out-2\u0000ws-2"]);
        const shared = hidden.find((entry) => entry.domainOutput === "out-2" && entry.domainWorkspace === "ws-1");
        assert.ok(shared !== undefined);
        assert.deepEqual(shared.windows.map((entry) => entry.id), ["win-b"]);
        const background = hidden.find((entry) => entry.domainOutput === "out-1" && entry.domainWorkspace === "ws-2");
        assert.ok(background !== undefined);
        assert.deepEqual(background.windows.map((entry) => entry.id), ["win-c"]);
    });

    it("observes the send source from the shared current", () => {
        const world = world66();
        const observed = observeSendTarget(world.workspace, new Map(), "ws-2", new Set());
        assert.ok(observed !== null);
        assert.equal(observed.sourceOutput, "out-1");
        assert.equal(observed.sourceWorkspace, "ws-1");
        assert.equal(observed.focusedId, "win-a");
        assert.equal(observed.currentWorkspace, "ws-1");
        assert.deepEqual(observed.sourceWindows.map((entry) => entry.id), ["win-a"]);
        assert.deepEqual(observed.targetWindows.map((entry) => entry.id), ["win-c"]);
    });

    it("observes cross-output send targets against the same shared desktop", () => {
        const world = world66();
        const observed = observeOutputSendTarget(world.workspace, new Map(), "out-2", new Set());
        assert.ok(observed !== null);
        assert.equal(observed.sourceOutput, "out-1");
        assert.equal(observed.targetOutput, "out-2");
        assert.equal(observed.sourceWorkspace, "ws-1");
        assert.equal(observed.targetWorkspace, "ws-1");
        assert.deepEqual(observed.sourceWindows.map((entry) => entry.id), ["win-a"]);
        assert.deepEqual(observed.targetWindows.map((entry) => entry.id), ["win-b"]);
    });

    it("resolves directional domains across outputs on the shared desktop", () => {
        const world = world66();
        const gaps = readDomainGaps({ readInnerGapFn: () => 8, readOuterGapFn: () => 8 });
        const seen = observeDirectionalDomain(world.workspace, new Map(), new Set(), gaps, "right");
        assert.equal(seen.status, "ready");
        assert.ok(seen.observed !== null);
    });
});

describe("kwin 6.6 production entry runs shared", () => {
    it("starts degraded, snapshots the shared domain, and switches it", () => {
        const world = world66();
        const { handle, logs, dbusCalls } = startEntry66(world, "per-output-local");
        assert.ok(
            logs.some((line) => line === "omnitiler:workspace:workspace-mode-degraded:per-output-local-to-shared"),
            `honest degrade log: ${JSON.stringify(logs)}`,
        );
        handle.requestFocus("left");
        assert.equal(dbusCalls.length, 1);
        const payload = JSON.parse(dbusCalls[0]?.payload as string) as Record<string, unknown>;
        const domain = payload["domain"] as Record<string, unknown>;
        assert.equal(domain["output"], "out-1");
        assert.equal(domain["workspace"], "ws-1");
        handle.requestWorkspaceSelect(2);
        assert.equal(world.workspace["currentDesktop"], world.desktops[1]);
        handle.stop();
    });

    it("refuses whole-workspace migration honestly without the options property", () => {
        const world = world66();
        const { handle, logs } = startEntry66(world, "per-output-local");
        handle.requestWorkspaceMigrate("right");
        assert.ok(
            logs.some((line) => line.includes("event=workspace-migrate") && line.includes("outcome=mode-shared")),
            `migration refusal log: ${JSON.stringify(logs)}`,
        );
        handle.stop();
    });
});

// ---- Legacy (6.6) production-entry wire coverage against the real Engine ----
// Same route as the 6.7 engine fixtures, but the workspace exposes no
// currentDesktopForScreen/setCurrentDesktopForScreen methods: every
// per-output read resolves to the shared currentDesktop and every switch to
// the shared write. Replies come from the real Planner (planner_eval), never
// hand-built, and timers fire deterministically.
function legacyRepoRoot(): string {
    let dir = resolve(process.cwd());
    for (let depth = 0; depth < 4; depth += 1) {
        if (existsSync(join(dir, "Cargo.toml"))) {
            return dir;
        }
        dir = dirname(dir);
    }
    throw new Error("fixture root not found");
}

class LegacyBridge {
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

    static start(): LegacyBridge {
        return new LegacyBridge(
            spawn("cargo", ["run", "--offline", "-q", "-p", "tiler-protocol", "--example", "planner_eval"], {
                cwd: legacyRepoRoot(),
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

interface LegacySignal {
    readonly handlers: Set<() => void>;
    readonly signal: {
        connect: (handler: () => void) => void;
        disconnect: (handler: () => void) => void;
    };
    readonly fire: () => void;
}

function legacySignal(): LegacySignal {
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

interface LegacyOutput {
    readonly name: string;
    readonly geometry: { x: number; y: number; width: number; height: number };
}

interface LegacyDesktop {
    readonly id: string;
    readonly x11DesktopNumber: number;
}

interface LegacyWindow extends Record<string, unknown> {
    normalWindow: boolean;
    managed: boolean;
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    output: LegacyOutput;
    desktops: LegacyDesktop[];
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number };
    resourceClass: string;
    outputSig: LegacySignal;
}

interface LegacyHarness {
    readonly bridge: LegacyBridge;
    readonly workspace: Record<string, unknown>;
    readonly outputs: { top: LegacyOutput; bottom: LegacyOutput };
    readonly desktops: { a: LegacyDesktop; b?: LegacyDesktop };
    readonly wins: LegacyWindow[];
    readonly mover: LegacyWindow;
    readonly dbusCalls: Array<{ method: string; payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly replies: Map<number, string>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean; fired: boolean }>;
    readonly logs: string[];
    readonly handle: NonNullable<ReturnType<typeof startPlanAdapterEntry>>;
    readonly fireTimers: (delayMs: number) => number;
    readonly flushAll: () => Promise<void>;
    readonly sendPayloads: () => Array<{ index: number; body: Record<string, unknown> }>;
    readonly stop: () => Promise<void>;
}

async function makeLegacyHarness(secondDesktop = false): Promise<LegacyHarness> {
    // Stacked outputs sharing one current desktop: the 6.6 shape is defined
    // by ABSENCE, so the per-output keys must never appear on the surface.
    const top: LegacyOutput = { name: "out-top", geometry: { x: 0, y: 0, width: 1920, height: 540 } };
    const bottom: LegacyOutput = { name: "out-bottom", geometry: { x: 0, y: 540, width: 1920, height: 540 } };
    const wsA: LegacyDesktop = { id: "ws-a", x11DesktopNumber: 1 };
    const wsB: LegacyDesktop = { id: "ws-b", x11DesktopNumber: 2 };
    const liveDesktops = secondDesktop ? [wsA, wsB] : [wsA];
    const mkWin = (
        output: LegacyOutput,
        desktop: LegacyDesktop,
        internalId: string,
        frame: { x: number; y: number; width: number; height: number },
    ): LegacyWindow => {
        const outputSig = legacySignal();
        return {
            normalWindow: true,
            managed: true,
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            output,
            desktops: [desktop],
            internalId,
            frameGeometry: { ...frame },
            resourceClass: "app",
            desktopsChanged: legacySignal().signal,
            outputChanged: outputSig.signal,
            frameGeometryChanged: legacySignal().signal,
            fullScreenChanged: legacySignal().signal,
            maximizedChanged: legacySignal().signal,
            outputSig,
        };
    };
    const wins: LegacyWindow[] = [
        mkWin(top, wsA, "o-win-1", { x: 20, y: 20, width: 400, height: 300 }),
        mkWin(top, wsA, "o-win-2", { x: 440, y: 20, width: 400, height: 300 }),
        mkWin(bottom, wsA, "o-win-t", { x: 20, y: 600, width: 400, height: 300 }),
    ];
    const mover = wins[1] as LegacyWindow;
    const wsig = {
        windowAdded: legacySignal(),
        windowRemoved: legacySignal(),
        windowActivated: legacySignal(),
        screensChanged: legacySignal(),
        currentDesktopChanged: legacySignal(),
        desktopsChanged: legacySignal(),
    };
    const workspace: Record<string, unknown> = {
        activeWindow: mover,
        activeScreen: top,
        screens: [top, bottom],
        desktops: liveDesktops,
        // Shared current desktop: current on EVERY output on 6.6.
        currentDesktop: wsA,
        // NOTE: no currentDesktopForScreen / setCurrentDesktopForScreen.
        clientArea: (kind: unknown, output: unknown): object => {
            void kind;
            if (output === top) {
                return { x: 0, y: 0, w: 1920, h: 500 };
            }
            return { x: 0, y: 580, w: 1920, h: 500 };
        },
        windowList: (): object[] => [...wins],
        sendClientToScreen: (client: object, output: object): void => {
            const win = client as LegacyWindow;
            win.output = output as LegacyOutput;
            win.outputSig.fire();
        },
        windowAdded: wsig.windowAdded.signal,
        windowRemoved: wsig.windowRemoved.signal,
        windowActivated: wsig.windowActivated.signal,
        screensChanged: wsig.screensChanged.signal,
        currentDesktopChanged: wsig.currentDesktopChanged.signal,
        desktopsChanged: wsig.desktopsChanged.signal,
    };
    const bridge = LegacyBridge.start();
    const dbusCalls: LegacyHarness["dbusCalls"] = [];
    const callbacks: LegacyHarness["callbacks"] = [];
    const replies = new Map<number, string>();
    const timers: LegacyHarness["timers"] = [];
    const logs: LegacyHarness["logs"] = [];
    const harness: LegacyHarness = {
        bridge,
        workspace,
        outputs: { top, bottom },
        desktops: secondDesktop ? { a: wsA, b: wsB } : { a: wsA },
        wins,
        mover,
        dbusCalls,
        callbacks,
        replies,
        timers,
        logs,
        handle: null as unknown as NonNullable<ReturnType<typeof startPlanAdapterEntry>>,
        fireTimers: (delayMs: number): number => {
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
        flushAll: async (): Promise<void> => {
            for (let round = 0; round < 12; round += 1) {
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
        sendPayloads: (): Array<{ index: number; body: Record<string, unknown> }> => {
            const out: Array<{ index: number; body: Record<string, unknown> }> = [];
            dbusCalls.forEach((call, index) => {
                if (call.method !== "DescribePlan") {
                    return;
                }
                try {
                    const body = JSON.parse(call.payload) as Record<string, unknown>;
                    if ((body["command"] as Record<string, unknown> | undefined)?.["op"] === "send-to-output") {
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
        registerShortcutFn: (): boolean => true,
        readProfileFn: (): string => "cosmic",
        readWorkspaceModeFn: (): unknown => "per-output-local",
        readInnerGapFn: (): number => 8,
        readOuterGapFn: (): number => 8,
    });
    assert.notEqual(handle, null, "production entry starts on the 6.6-shaped surface");
    (harness as { handle: unknown }).handle = handle;
    return harness;
}

function legacyPlanCalls(harness: LegacyHarness): Array<Record<string, unknown>> {
    return harness.dbusCalls
        .filter((call) => call.method === "DescribePlan")
        .map((call) => JSON.parse(call.payload) as Record<string, unknown>);
}

function assertLegacyShape(harness: LegacyHarness): void {
    assert.equal("currentDesktopForScreen" in harness.workspace, false, "6.6 surface never gains the read method");
    assert.equal("setCurrentDesktopForScreen" in harness.workspace, false, "6.6 surface never gains the setter");
}

async function settleLegacy(harness: LegacyHarness): Promise<void> {
    for (let round = 0; round < 10; round += 1) {
        harness.fireTimers(PLAN_DEBOUNCE_MS);
        const before = harness.dbusCalls.length;
        await harness.flushAll();
        harness.fireTimers(PLAN_DEBOUNCE_MS);
        await harness.flushAll();
        if (harness.dbusCalls.length === before) {
            return;
        }
    }
    throw new Error("legacy plan flights did not settle");
}

describe("kwin 6.6 production entry wire coverage (real Engine, methods absent)", () => {
    it("runs the automatic startup resync on the shared desktop", async () => {
        const h = await makeLegacyHarness();
        try {
            assert.ok(
                h.logs.some((line) => line === "omnitiler:workspace:workspace-mode-degraded:per-output-local-to-shared"),
                `honest degrade log: ${JSON.stringify(h.logs)}`,
            );
            assertLegacyShape(h);
            h.fireTimers(PLAN_DEBOUNCE_MS);
            await h.flushAll();
            const calls = legacyPlanCalls(h);
            assert.ok(calls.length >= 1, "startup resync dispatches automatically");
            for (const body of calls) {
                assert.equal(
                    (body["domain"] as Record<string, unknown>)["workspace"],
                    "ws-a",
                    "every startup domain resolves the shared current desktop",
                );
            }
            const domains = new Set(
                calls.map((body) => {
                    const domain = body["domain"] as Record<string, unknown>;
                    return `${domain["output"] as string} ${domain["workspace"] as string}`;
                }),
            );
            assert.ok(domains.has("out-top ws-a"), `foreground startup domain: ${JSON.stringify([...domains])}`);
            assert.ok(domains.has("out-bottom ws-a"), `hidden startup domain: ${JSON.stringify([...domains])}`);
            assertLegacyShape(h);
        } finally {
            await h.stop();
        }
    });

    it("sends cross-output with follow and no desktop switch", async () => {
        const h = await makeLegacyHarness();
        try {
            await settleLegacy(h);
            assertLegacyShape(h);
            h.handle.requestSendToOutput("down", true);
            assert.equal(h.sendPayloads().length, 1, "exactly one send-to-output dispatch, no replay at dispatch");
            const sent = h.sendPayloads()[0]?.body as Record<string, unknown>;
            assert.equal((sent["command"] as Record<string, unknown>)["follow"], true);
            assert.equal((sent["target_domain"] as Record<string, unknown>)["output"], "out-bottom");
            assert.equal((sent["target_domain"] as Record<string, unknown>)["workspace"], "ws-a");
            await h.flushAll();
            const sendCall = h.sendPayloads()[0];
            assert.ok(sendCall !== undefined);
            const reply = JSON.parse(h.replies.get(sendCall.index) as string) as Record<string, unknown>;
            assert.equal(reply["outcome"], "planned", "real Engine plans the legacy shared-desktop send");
            assert.equal(reply["kind"], "send-to-output");
            // Cross-output transfer actually actuated on the shared desktop.
            assert.equal(h.mover.output.name, "out-bottom");
            assert.deepEqual(
                h.mover.desktops.map((entry) => entry.id),
                ["ws-a"],
            );
            // Follow focuses the mover with no desktop switch: one desktop is
            // current everywhere on legacy hosts.
            assert.equal(h.workspace["activeWindow"], h.mover);
            assert.equal((h.workspace["currentDesktop"] as { id: string }).id, "ws-a");
            assert.equal(
                h.logs.filter((line) => line.includes("follow") && line.includes("state-confirmed")).length,
                1,
                "exactly one follow, no replay",
            );
            assert.equal(h.sendPayloads().length, 1, "no replay after settlement");
            h.fireTimers(WORKSPACE_SEND_TIMEOUT_MS);
            assert.equal(h.sendPayloads().length, 1, "arrival deadline is a no-op post-settlement");
            assertLegacyShape(h);
        } finally {
            await h.stop();
        }
    });

    it("sends cross-output with stay and keeps source focus", async () => {
        const h = await makeLegacyHarness();
        try {
            // Seed source focus history through the real Engine on the
            // pending startup debounce (direct assignment fires no signal,
            // so this must ride timers already queued at startup): focus
            // o-win-1, then the mover.
            const survivorSeed = h.wins.find((win) => win.internalId === "o-win-1");
            assert.ok(survivorSeed !== undefined);
            h.workspace["activeWindow"] = survivorSeed;
            h.fireTimers(PLAN_DEBOUNCE_MS);
            await h.flushAll();
            h.workspace["activeWindow"] = h.mover;
            h.fireTimers(PLAN_DEBOUNCE_MS);
            await h.flushAll();
            await settleLegacy(h);
            h.handle.requestSendToOutput("down", false);
            assert.equal(h.sendPayloads().length, 1, "exactly one send-to-output dispatch");
            const sent = h.sendPayloads()[0]?.body as Record<string, unknown>;
            assert.equal((sent["command"] as Record<string, unknown>)["follow"], false);
            await h.flushAll();
            assert.equal(h.mover.output.name, "out-bottom", "transfer actuates on stay too");
            assert.deepEqual(
                h.mover.desktops.map((entry) => entry.id),
                ["ws-a"],
            );
            const survivorStay = h.wins.find((win) => win.internalId === "o-win-1");
            assert.ok(survivorStay !== undefined);
            assert.equal(h.workspace["activeWindow"], survivorStay, "stay binds focus to the source survivor");
            assert.equal((h.workspace["currentDesktop"] as { id: string }).id, "ws-a", "stay never switches");
            assert.equal(h.sendPayloads().length, 1, "no replay");
            assertLegacyShape(h);
        } finally {
            await h.stop();
        }
    });

    it("moves workspaces with follow through the shared switch", async () => {
        const h = await makeLegacyHarness(true);
        try {
            await settleLegacy(h);
            assertLegacyShape(h);
            h.handle.requestWorkspaceMove(2, true);
            h.fireTimers(PLAN_DEBOUNCE_MS);
            await h.flushAll();
            assert.deepEqual(
                h.mover.desktops.map((entry) => entry.id),
                ["ws-b"],
                "membership follows the mover to the target workspace",
            );
            assert.equal(
                (h.workspace["currentDesktop"] as { id: string }).id,
                "ws-b",
                "follow switches the shared desktop, current everywhere",
            );
            assert.equal(h.workspace["activeWindow"], h.mover, "follow keeps focus on the mover");
            assertLegacyShape(h);
        } finally {
            await h.stop();
        }
    });
});
