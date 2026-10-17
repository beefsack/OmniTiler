// World native toggles through the exact KWin seams: `setMaximize` with
// `maximizeMode` readback, `fullScreen`, `onAllDesktops`. Fakes prefer
// real-property names; the live-safety alias tests below pin the fallback
// precedence explicitly so no alias can mask a regression elsewhere.
// Busy fences and overlay rechecks run before any setter; synchronous
// invalidation runs before the first setter; the setter boolean alone is
// never proof (truthful readback decides).

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { createWorldApplyHost, createWorldNativeToggles, type WorldNativeEnv } from "../src/world-native";
import type { WorldCommand } from "../src/world-report";

interface FakeWin {
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number };
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    keepAbove: boolean;
    keepBelow: boolean;
    setMaximize: ((v: boolean, h: boolean) => void) | undefined;
}

function makeWin(over: Partial<FakeWin> = {}): { ref: FakeWin; order: string[] } {
    const order: string[] = [];
    const ref = {
        internalId: "win-a",
        frameGeometry: { x: 0, y: 0, width: 100, height: 100 },
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        keepAbove: false,
        keepBelow: false,
        setMaximize: undefined as ((v: boolean, h: boolean) => void) | undefined,
        ...over,
    } as FakeWin;
    if (ref.setMaximize === undefined) {
        ref.setMaximize = (v: boolean, h: boolean): void => {
            order.push(`setMaximize:${v},${h}`);
            ref.maximizeMode = v && h ? 3 : 0;
        };
    }
    return { ref, order };
}

interface Harness {
    toggles: ReturnType<typeof createWorldNativeToggles>;
    calls: WorldCommand[];
    logs: string[];
    invalidations: number;
    order: string[];
    env: {
        setBusy: (busy: boolean) => void;
        setFloating: (floating: boolean) => void;
    };
}

function setup(win: FakeWin, order: string[], readLiveRect?: () => { x: number; y: number; w: number; h: number } | null): Harness {
    const calls: WorldCommand[] = [];
    const logs: string[] = [];
    let invalidations = 0;
    let busy = false;
    let floating = false;
    const surface = { windowList: () => [win] };
    const toggles = createWorldNativeToggles({
        surface: surface as object,
        log: (m) => { logs.push(m); },
        token: (v) => String(v),
        readProp: (value, property) => {
            try {
                return Reflect.get(value as object, property);
            } catch (error) {
                void error;
                return undefined;
            }
        },
        shortcutFailed: (action) => {
            logs.push(`failed:${action}`);
        },
        submitCommand: (command) => {
            calls.push(command);
            return true;
        },
        invalidate: () => {
            invalidations += 1;
            order.push("invalidate");
        },
        scheduleDispatch: () => undefined,
        persistFloatIntent: () => undefined,
        isWorldBusy: () => busy,
        isFloating: () => floating,
        noteFloating: () => undefined,
        clearFloating: () => undefined,
        activeWindowId: () => "win-a",
        activeWindowRef: () => win as object,
        readLiveRect: (ref) => {
            if (readLiveRect !== undefined) {
                return readLiveRect();
            }
            const g = (ref as FakeWin).frameGeometry;
            return { x: g.x, y: g.y, w: g.width, h: g.height };
        },
    } satisfies WorldNativeEnv);
    return {
        toggles, calls, logs, order,
        get invalidations() {
            return invalidations;
        },
        env: {
            setBusy: (b) => { busy = b; },
            setFloating: (f) => { floating = f; },
        },
    };
}

describe("world native toggles", () => {
    it("floats a tiled window through a toggle-float command", () => {
        const { ref, order } = makeWin();
        const h = setup(ref, order);
        assert.equal(h.toggles.submitFloat("float"), true);
        assert.equal(h.calls.length, 1);
        assert.deepEqual((h.calls[0] as { command: Record<string, unknown> }).command, {
            op: "toggle-float", win: "win-a", rect: null,
        });
    });

    it("refuses float while the world flight is busy, clearing nothing", () => {
        const { ref, order } = makeWin({ maximizeMode: 3 });
        const h = setup(ref, order);
        h.env.setBusy(true);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), false);
        assert.equal(ref.maximizeMode, 3, "B9 clear never runs before the busy fence");
        assert.equal(h.calls.length, 0);
        assert.ok(h.logs.some((l) => l.includes("native-fenced")));
    });

    it("clears maximize through setMaximize with readback before floating", () => {
        const { ref, order } = makeWin({ maximizeMode: 3 });
        const h = setup(ref, order);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), true);
        assert.deepEqual(order, ["invalidate", "setMaximize:false,false"]);
        assert.equal(ref.maximizeMode, 0);
        assert.equal(h.calls.length, 1);
    });

    it("refuses float when the maximize clear does not read back", () => {
        const { ref, order } = makeWin({ maximizeMode: 3 });
        ref.setMaximize = () => {
            order.push("setMaximize:false,false");
            // Native refuses: maximizeMode stays put.
        };
        const h = setup(ref, order);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), false);
        assert.equal(h.calls.length, 0, "no command after a refused clear");
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear")));
    });

    it("carries the live frame on unfloat so the next float restores it", () => {
        const { ref } = makeWin({ maximizeMode: 0 });
        (ref as unknown as Record<string, unknown>)["frameGeometry"] = { x: 11, y: 22, width: 333, height: 444 };
        const h = setup(ref, []);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), true);
        assert.deepEqual((h.calls[0] as { command: Record<string, unknown> }).command, {
            op: "toggle-float", win: "win-a", rect: [11, 22, 333, 444],
        });
    });

    it("refuses an unfloat whose live frame is unreadable", () => {
        const { ref, order } = makeWin({ maximizeMode: 0 });
        const h = setup(ref, order, () => null);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), false);
        assert.equal(h.calls.length, 0, "no invented rect, no command");
        assert.ok(h.logs.some((l) => l.includes("float-refused-unreadable")));
    });

    it("refuses float overlays before any setter", () => {
        const { ref, order } = makeWin({ fullScreen: true, maximizeMode: 3 });
        const h = setup(ref, order);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), false);
        assert.equal(ref.maximizeMode, 3);
        assert.ok(!order.includes("invalidate"));
    });

    it("sets sticky through onAllDesktops with readback and choreography", () => {
        const { ref, order } = makeWin();
        const h = setup(ref, order);
        assert.equal(h.toggles.setStickyNative(ref as object, "win-a", true), true);
        assert.equal(ref.onAllDesktops, true);
        assert.deepEqual(order, ["invalidate"]);
        assert.equal(h.toggles.setStickyNative(ref as object, "win-a", false), true);
        assert.equal(ref.onAllDesktops, false);
    });

    it("refuses sticky overlays read through the native names", () => {
        const full = makeWin({ fullScreen: true });
        const hf = setup(full.ref, full.order);
        assert.equal(hf.toggles.setStickyNative(full.ref as object, "win-a", true), false);
        assert.equal(full.ref.onAllDesktops, false);
        const max = makeWin({ maximizeMode: 3 });
        const hm = setup(max.ref, max.order);
        assert.equal(hm.toggles.setStickyNative(max.ref as object, "win-a", true), false);
        assert.equal(max.ref.onAllDesktops, false);
    });

    it("maximizes through setMaximize and proves through maximizeMode", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true), true);
        assert.equal(ref.maximizeMode, 3);
        assert.equal(h.toggles.setMaximizedNative(ref as object, false), true);
        assert.equal(ref.maximizeMode, 0);
    });

    it("fails closed without setMaximize and on readback mismatch", () => {
        const { ref } = makeWin();
        ref.setMaximize = undefined;
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true), false);
        const lying = makeWin();
        lying.ref.setMaximize = () => undefined;
        const hl = setup(lying.ref, []);
        assert.equal(hl.toggles.setMaximizedNative(lying.ref as object, true), false);
    });

    it("toggles fullscreen through fullScreen with readback", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setFullscreenNative(ref as object, true), true);
        assert.equal(ref.fullScreen, true);
        assert.equal(h.toggles.setFullscreenNative(ref as object, false), true);
        assert.equal(ref.fullScreen, false);
    });
});

describe("world apply host live safety", () => {
    const readProp = (value: object, property: string): unknown =>
        Reflect.get(value as object, property);
    const surface = { windowList: () => [] as Array<unknown> };
    const safetyOf = (
        ref: object,
        reader: (value: object, property: string) => unknown = readProp,
    ): unknown =>
        createWorldApplyHost(surface as object, reader).readLiveSafety(ref);

    it("prefers canonical native properties over contradictory aliases", () => {
        const ref = {
            minimized: false,
            fullScreen: true,
            maximizeMode: 3,
            onAllDesktops: false,
            fullscreen: false,
            maximized: false,
            sticky: true,
        };
        assert.deepEqual(safetyOf(ref as object), {
            minimized: false,
            fullscreen: true,
            maximized: true,
            sticky: false,
        });
    });

    it("falls back to aliases when native properties are absent", () => {
        const ref = {
            minimized: false,
            fullscreen: true,
            maximized: true,
            sticky: true,
        };
        assert.deepEqual(safetyOf(ref as object), {
            minimized: false,
            fullscreen: true,
            maximized: true,
            sticky: true,
        });
    });

    it("falls back to aliases when native reads are type-invalid", () => {
        const ref = {
            minimized: 0,
            fullScreen: "yes",
            maximizeMode: "3",
            onAllDesktops: 1,
            fullscreen: true,
            maximized: false,
            sticky: true,
        };
        assert.deepEqual(safetyOf(ref as object), {
            minimized: null,
            fullscreen: true,
            maximized: false,
            sticky: true,
        });
    });

    it("fails the whole safety result when a read throws", () => {
        const throwing = (): unknown => {
            throw new Error("boom");
        };
        assert.equal(safetyOf({} as object, throwing), null);
    });
});
