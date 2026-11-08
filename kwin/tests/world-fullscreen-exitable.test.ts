// KDE fullscreen exitability (v4 fullscreenExitable): nullable
// fullScreenable passthrough, no ownership inference. Offline only:
// scripted in-memory workspace fakes with native KWin property
// spellings, no D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { captureWorldObservation } from "../src/world-observe";
import { buildWorldReportV4InnerJson } from "../src/world-report";

function makeDesk(id: string) {
    return { id };
}

function makeWin(over: Record<string, unknown>) {
    return {
        internalId: "win-a",
        normalWindow: true,
        dialog: false,
        managed: true,
        deleted: false,
        transient: false,
        transientFor: null,
        modal: false,
        wantsInput: true,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minimized: false,
        hidden: false,
        frameGeometry: { x: 8.4, y: 8.6, width: 800, height: 600 },
        clientGeometry: { x: 10, y: 30, width: 796, height: 578 },
        minSize: { width: 100, height: 100 },
        maxSize: { width: 2147483647, height: 2147483647 },
        output: { name: "out-a" },
        desktops: [makeDesk("native-a")],
        pid: 1234,
        resourceClass: "Example",
        caption: "Window",
        windowRole: "normal",
        desktopFileName: "example.app",
        resizeable: true,
        keepAbove: false,
        keepBelow: false,
        ...over,
    };
}

function makeSurface(wins: Array<Record<string, unknown>>) {
    const desktops = [makeDesk("native-a")];
    const screens = [{ name: "out-a", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    return {
        windowList: () => wins,
        screens,
        desktops,
        activeScreen: { name: "out-a" },
        activeWindow: wins[0] ?? null,
        cursorPos: { x: 20, y: 30 },
        windowAt: () => (wins.length > 0 ? [wins[0]] : []),
        stackingOrder: [...wins].reverse(),
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1040 }),
        currentDesktopForScreen: () => desktops[0],
    };
}

function windowRow(surface: object) {
    const obs = captureWorldObservation({
        workspace: surface,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 8,
        gestureActive: () => false,
        knownClosed: () => [],
        logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-a", pin: false, disp: false, managed: true }),
        isFloating: () => false,
    });
    assert.ok(obs !== null);
    const json = buildWorldReportV4InnerJson({
        observation: obs,
        creationAttempts: [],
        frameAttempts: [],
        memberAttempts: [],
        viewAttempts: [],
        clearAttempts: [],
        stateAttempts: [],
        intentRecords: [],
    });
    assert.ok(json !== null, "report must stay alive");
    const value = JSON.parse(json) as Record<string, unknown>;
    const windows = value["windows"] as Array<Record<string, unknown>>;
    assert.equal(windows.length, 1);
    return { json, row: windows[0] as Record<string, unknown> };
}

describe("v4 fullscreen exitability", () => {
    it("passes a positive native fullScreenable through to fullscreenExitable", () => {
        const { json, row } = windowRow(makeSurface([makeWin({ fullScreenable: true })]));
        assert.equal(row["fullscreenExitable"], true);
        assert.ok(json.includes('"fullscreenExitable":true'));
    });

    it("carries a positive fact on a fullscreen window so exit can receive it", () => {
        const { row } = windowRow(makeSurface([makeWin({ fullScreen: true, fullScreenable: true })]));
        assert.equal(row["fullscreen"], true);
        assert.equal(row["fullscreenExitable"], true);
    });

    it("carries a positive fact on an unfullscreened window so maximize fullscreen-clear can receive it", () => {
        const { row } = windowRow(makeSurface([makeWin({ fullScreen: false, maximizeMode: 3, fullScreenable: true })]));
        assert.equal(row["fullscreen"], false);
        assert.equal(row["maximized"], true);
        assert.equal(row["fullscreenExitable"], true);
    });

    it("passes an explicit false without holding the report", () => {
        const { json, row } = windowRow(makeSurface([makeWin({ fullScreenable: false })]));
        assert.equal(row["fullscreenExitable"], false);
        assert.ok(json.includes('"fullscreenExitable":false'));
    });

    it("rides null when the native property is missing", () => {
        const { row } = windowRow(makeSurface([makeWin({})]));
        assert.equal(row["fullscreenExitable"], null);
    });

    it("rides null on a malformed native value without holding the report", () => {
        for (const bad of [42, "yes", 1, {}]) {
            const { json, row } = windowRow(makeSurface([makeWin({ fullScreenable: bad })]));
            assert.ok(json !== null, `malformed ${String(bad)} must not hold the report`);
            assert.equal(row["fullscreenExitable"], null, `malformed ${String(bad)} must ride null`);
        }
    });

    it("rides null when a native getter throws, keeping the report alive", () => {
        const win = makeWin({}) as Record<string, unknown>;
        Object.defineProperty(win, "fullScreenable", {
            enumerable: true, configurable: true,
            get() { throw new Error("native boom"); },
        });
        const { json, row } = windowRow(makeSurface([win]));
        assert.ok(json !== null, "throwing getter must not hold the report");
        assert.equal(row["fullscreenExitable"], null);
    });

    it("never infers ownership from a fullscreen window", () => {
        const { row } = windowRow(makeSurface([makeWin({ fullScreen: true, fullScreenable: true })]));
        assert.equal(row["fullscreenOwner"], null);
    });
});
