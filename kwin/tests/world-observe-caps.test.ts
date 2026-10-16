// Production observation bounds (D4/D5): inventories at and beyond the
// legacy 1024/32 caps must capture with explicit holds, never a silent
// null report. Core bounds mirrored here: 16 outputs, 256 natives per
// output, 10000 windows, 64 domains.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { captureWorldObservation } from "../src/world-observe";

function makeInventory(desktopCount: number, windowCount: number, screenCount: number) {
    const desktops = Array.from({ length: desktopCount }, (_, i) => ({ id: `ws-${i}` }));
    const screens = Array.from({ length: screenCount }, (_, i) => ({
        name: `out-${i}`,
        geometry: { x: i * 1920, y: 0, width: 1920, height: 1080 },
    }));
    const wins = Array.from({ length: windowCount }, (_, i) => ({
        internalId: `win-${i}`,
        normalWindow: true,
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
        output: { name: `out-${i % screenCount}` },
        desktops: [desktops[i % desktopCount] as { id: string }],
        minimized: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        minSize: null,
        maxSize: null,
    }));
    const currentByScreen = new Map(screens.map((s, i) => [s.name, desktops[i % desktopCount] as { id: string }]));
    const surface = {
        windowList: () => wins,
        screens,
        desktops,
        activeScreen: { name: "out-0" },
        activeWindow: wins[0] ?? null,
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }) => currentByScreen.get(screen.name) ?? null,
    };
    return { surface, wins, desktops, screens };
}

function observe(surface: object) {
    return captureWorldObservation({
        workspace: surface,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 8,
        gestureActive: () => false,
        knownClosed: () => [],
        logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-0", pin: false, disp: false, managed: true }),
        isFloating: () => false,
    });
}

describe("world observation inventory bounds", () => {
    it("captures 1025 windows completely (past the legacy 1024 cap)", () => {
        const { surface } = makeInventory(2, 1025, 1);
        const obs = observe(surface);
        assert.ok(obs !== null);
        const total = obs.domains.reduce((n, d) => n + d.windows.length, 0);
        assert.equal(total, 1025);
        assert.equal(obs.complete, true);
    });

    it("captures 2000 windows completely with all domains readable", () => {
        const { surface } = makeInventory(4, 2000, 1);
        const obs = observe(surface);
        assert.ok(obs !== null);
        const total = obs.domains.reduce((n, d) => n + d.windows.length, 0);
        assert.equal(total, 2000);
        assert.ok(obs.domains.every((d) => d.readable && d.hold === ""));
        assert.equal(obs.complete, true);
    });

    it("captures a 40-workspace topology completely (past the legacy 32 cap)", () => {
        const { surface } = makeInventory(40, 40, 1);
        const obs = observe(surface);
        assert.ok(obs !== null);
        assert.equal(obs.topology[0]?.order.length, 40);
        assert.equal(obs.topology[0]?.complete, true);
        assert.equal(obs.bindings.length, 40);
        assert.equal(obs.complete, true);
    });

    it("holds instead of nulling on 17 outputs (core: 16)", () => {
        const { surface } = makeInventory(17, 17, 17);
        const obs = observe(surface);
        assert.ok(obs !== null, "overflow truncates with holds, never a null report");
        assert.equal(obs.topology.length, 16);
        assert.equal(obs.complete, false);
        assert.ok(obs.topology.every((t) => t.complete === false), "truncated screen inventory holds all enumerated outputs");
    });

    it("holds instead of nulling on 10001 windows (core: 10000)", () => {
        const { surface } = makeInventory(2, 10001, 1);
        const obs = observe(surface);
        assert.ok(obs !== null, "overflow truncates with holds, never a null report");
        // The cut tail is unprovable, so no partial head membership is
        // published as complete: held scopes carry no windows.
        const total = obs.domains.reduce((n, d) => n + d.windows.length, 0);
        assert.equal(total, 0);
        assert.ok(obs.domains.every((d) => d.readable === false && d.hold !== ""), "truncated window inventory holds enumerated scopes, never partial-complete");
        assert.equal(obs.complete, false);
    });

    it("truncates per-output native order at 256 with an incomplete topology", () => {
        const { surface } = makeInventory(300, 300, 1);
        const obs = observe(surface);
        assert.ok(obs !== null);
        assert.equal(obs.topology[0]?.order.length, 256);
        assert.equal(obs.topology[0]?.complete, false);
        assert.equal(obs.complete, false);
    });

    it("truncates hidden domains at 64 keeping visible first", () => {
        const { surface } = makeInventory(70, 70, 1);
        const obs = observe(surface);
        assert.ok(obs !== null);
        assert.equal(obs.domains.length, 64);
        assert.ok(obs.domains.some((d) => d.ws === "ws-0"), "visible domain retained");
        assert.equal(obs.complete, false);
    });

    it("homes real sticky empty-desktops windows on the observed output current", () => {
        // Real KWin shape: onAllDesktops true with an empty desktops list is
        // a legitimate sticky lifetime (legacy adopts it homed), never a
        // whole-report unreadable. Homing comes from the observed output
        // current, never a fabricated id, and the sticky occupies exactly
        // that homing (never all backings).
        const desktops = [{ id: "ws-a" }, { id: "ws-b" }];
        const screens = [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
        const stickyWin = {
            internalId: "win-sticky",
            normalWindow: true,
            frameGeometry: { x: 10, y: 10, width: 200, height: 200 },
            output: { name: "out-1" },
            desktops: [],
            onAllDesktops: true,
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            minSize: null,
            maxSize: null,
        };
        const tiledWin = {
            internalId: "win-tiled",
            normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" },
            desktops: [desktops[0]],
            onAllDesktops: false,
            minimized: false,
            fullScreen: false,
            maximizeMode: 0,
            minSize: null,
            maxSize: null,
        };
        const surface = {
            windowList: () => [stickyWin, tiledWin],
            screens,
            desktops,
            activeScreen: { name: "out-1" },
            activeWindow: tiledWin,
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: () => desktops[0],
        };
        const obs = observe(surface);
        assert.ok(obs !== null, "sticky empty desktops never nulls the report");
        assert.equal(obs.complete, true);
        const sticky = obs.domains.flatMap((d) => d.windows).find((w) => w.win === "win-sticky");
        assert.ok(sticky !== undefined, "sticky lifetime observed");
        assert.equal(sticky?.out, "out-1");
        assert.equal(sticky?.ws, "ws-a", "homed from the observed output current");
        assert.equal(sticky?.sticky, true);
        // Exactly one homing: the sticky never occupies all backings.
        const homings = obs.domains.filter((d) => d.windows.some((w) => w.win === "win-sticky"));
        assert.equal(homings.length, 1);
    });
});
