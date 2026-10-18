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
    output: { name: string } | undefined;
    desktops: Array<{ id: string }>;
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
        output: { name: "out-1" },
        desktops: [{ id: "ws-1" }],
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

// Minimal fresh capture for the single-observation boundary: only the
// domains the B9 poll reads (win homing for the scope fence).
const freshWith = (win: string, out: string, ws: string): { domains: Array<{ id: string; windows: Array<{ win: string; ws: string }> }> } => ({
    domains: [{ id: out, windows: [{ win, ws }] }],
});

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
        submitSettledCommand: (command) => {
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
        liveRefById: (id) => (id === "win-a" ? (win as object) : null),
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

    it("defers the maximized unfloat command until the first-report boundary", () => {
        const { ref, order } = makeWin({ maximizeMode: 3 });
        const h = setup(ref, order);
        h.env.setFloating(true);
        // Sync fake settles in the same stack, but the command must not
        // ride the same stack (no same-stack admission).
        assert.equal(h.toggles.submitFloat("float"), true);
        assert.deepEqual(order, ["invalidate", "setMaximize:false,false"]);
        assert.equal(ref.maximizeMode, 0);
        assert.equal(h.calls.length, 0, "deferred: no same-stack command");
        assert.ok(h.logs.some((l) => l.includes("float-unfloat-maximize-clear") && l.includes("outcome=issued")));
        h.toggles.pollPendingUnfloat(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.calls.length, 1, "exactly one command after observed clear");
        assert.deepEqual((h.calls[0] as { command: Record<string, unknown> }).command, {
            op: "toggle-float", win: "win-a", rect: [0, 0, 100, 100],
        });
        assert.ok(h.logs.some((l) => l.includes("outcome=observed-cleared")));
        // The boundary consumed the attempt: a repeat poll is a no-op.
        h.toggles.pollPendingUnfloat(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.calls.length, 1, "no second command");
    });

    it("terminates on the first report when the mode stays maximized", () => {
        const { ref, order } = makeWin({ maximizeMode: 3 });
        ref.setMaximize = () => {
            order.push("setMaximize:false,false");
            // Native refuses: maximizeMode stays put.
        };
        const h = setup(ref, order);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), true, "issued clear pends for the boundary report");
        assert.equal(h.calls.length, 0, "no command before the boundary");
        // First fresh report still maximized: the attempt terminates,
        // never waiting for a later restore.
        h.toggles.pollPendingUnfloat(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.calls.length, 0, "raced clear never admits");
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=still-maximized")));
        // Terminated means terminated: a later settled report admits nothing.
        ref.maximizeMode = 0;
        h.toggles.pollPendingUnfloat(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.calls.length, 0, "no late admission after termination");
    });

    it("refuses the arm when the live scope is unreadable", () => {
        const { ref } = makeWin({ maximizeMode: 3, output: undefined });
        const h = setup(ref, []);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), false);
        assert.equal(h.calls.length, 0, "unobserved arm admits nothing");
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=observe-missing")));
    });

    it("cancels the pending unfloat on a scope race", () => {
        const { ref } = makeWin({ maximizeMode: 3 });
        const h = setup(ref, []);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), true);
        ref.maximizeMode = 0;
        h.toggles.pollPendingUnfloat(freshWith("win-a", "out-1", "ws-2") as never, false);
        assert.equal(h.calls.length, 0, "moved scope never admits");
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=scope-raced")));
    });

    it("cancels the pending unfloat mid-gesture", () => {
        const { ref } = makeWin({ maximizeMode: 3 });
        const h = setup(ref, []);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), true);
        ref.maximizeMode = 0;
        h.toggles.pollPendingUnfloat(freshWith("win-a", "out-1", "ws-1") as never, true);
        assert.equal(h.calls.length, 0, "mid-gesture boundary never admits");
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=gesture-raced")));
    });

    it("cancels the pending unfloat when the world flight is busy at the boundary", () => {
        const { ref } = makeWin({ maximizeMode: 3 });
        const h = setup(ref, []);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), true);
        assert.equal(h.calls.length, 0);
        // Sync fake already settled; a newer flight racing the boundary wins.
        h.env.setBusy(true);
        h.toggles.pollPendingUnfloat(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.calls.length, 0, "busy boundary never admits");
        assert.ok(h.logs.some((l) => l.includes("float-refused-maximize-clear") && l.includes("cause=busy")));
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

describe("world apply host desktop creation", () => {
    it("forwards zero-based position/name through the void KWin API", () => {
        const seen: unknown[][] = [];
        const surface = {
            desktops: [{ id: "ws1", name: "WS1" }],
            createDesktop: (...args: ReadonlyArray<unknown>): void => {
                seen.push([...args]);
            },
        };
        const host = createWorldApplyHost(surface as object, (v, p) => Reflect.get(v as object, p));
        host.createDesktop(1, "wl-1");
        assert.deepEqual(seen, [[1, "wl-1"]]);
        assert.deepEqual(host.listDesktops?.(), [{ id: "ws1", name: "WS1" }]);
    });

    it("throws when the native method is missing so apply holds ambiguous", () => {
        const host = createWorldApplyHost({} as object, (v, p) => Reflect.get(v as object, p));
        assert.throws(() => host.createDesktop(0, "wl-1"));
        assert.equal(host.listDesktops?.(), null);
    });

    it("reports null when any desktop name is unreadable", () => {
        const surface = {
            desktops: [{ id: "ws1", name: "WS1" }, { id: "ws2", name: 42 }],
        };
        const host = createWorldApplyHost(surface as object, (v, p) => Reflect.get(v as object, p));
        assert.equal(host.listDesktops?.(), null);
    });
});
