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

import {
    readDesktopForOutput,
    readPerOutputVirtualDesktopsFlag,
    resolveEffectiveWorkspaceMode,
    supportsPerOutputDesktops,
    writeDesktopForOutput,
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
        internalId: "win-a", managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true, minimized: false, hidden: false,
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
        internalId: "win-b", managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true, minimized: false, hidden: false,
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
        internalId: "win-c", managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true, minimized: false, hidden: false,
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

describe("kwin 6.6 legacy-shared capability seam", () => {
    it("requires both the read method and a strictly enabled option for per-output support", () => {
        const enabled = { perOutputVirtualDesktops: true };
        const legacy = world66();
        assert.equal(supportsPerOutputDesktops(legacy.workspace, enabled), false);
        const modern = world67();
        assert.equal(supportsPerOutputDesktops(modern.workspace, enabled), true);
        assert.equal(supportsPerOutputDesktops(modern.workspace, { perOutputVirtualDesktops: false }), false);
        assert.equal(supportsPerOutputDesktops(modern.workspace, {}), false);
        assert.equal(supportsPerOutputDesktops(null, enabled), false);
    });

    it("degrades requested per-output modes to shared on legacy hosts", () => {
        const legacySupport = supportsPerOutputDesktops(world66().workspace, {});
        assert.equal(resolveEffectiveWorkspaceMode("per-output-local", legacySupport), "shared");
        assert.equal(resolveEffectiveWorkspaceMode("global-unique", legacySupport), "shared");
        assert.equal(resolveEffectiveWorkspaceMode("shared", legacySupport), "shared");
        const modernSupport = supportsPerOutputDesktops(world67().workspace, { perOutputVirtualDesktops: true });
        assert.equal(resolveEffectiveWorkspaceMode("per-output-local", modernSupport), "per-output-local");
        assert.equal(resolveEffectiveWorkspaceMode("global-unique", modernSupport), "global-unique");
    });

    it("reports the absent options property as null", () => {
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
        const options = { perOutputVirtualDesktops: true };
        assert.equal(supportsPerOutputDesktops(partial.workspace, options), true);
        assert.equal(writeDesktopForOutput(partial.workspace, partial.desktops[1] as object, partial.outputs[0] as object, true, options), false);
        assert.equal(partial.workspace["currentDesktop"], partial.desktops[0]);
    });

    it("does not use the shared setter on a modern host whose coupling is unreadable", () => {
        const modern = world67();
        assert.equal(writeDesktopForOutput(modern.workspace, modern.desktops[1] as object, modern.outputs[0] as object, false, {}), false);
        assert.equal(modern.workspace["currentDesktop"], modern.desktops[0]);
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
