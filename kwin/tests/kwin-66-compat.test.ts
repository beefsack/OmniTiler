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
import { describe, it } from "node:test";

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
