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
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number };
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minimized: boolean;
    hidden: boolean;
    keepAbove: boolean;
    keepBelow: boolean;
    output: { name: string } | undefined;
    desktops: Array<{ id: string }>;
    setMaximize: ((v: boolean, h: boolean) => void) | undefined;
}

function makeWin(over: Partial<FakeWin> = {}): { ref: FakeWin; order: string[] } {
    const order: string[] = [];
    const ref = {
        internalId: "win-a", managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true, minimized: false, hidden: false,
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

// Minimal fresh capture for the single-observation boundary: the native
// settlement poll reads complete/topology plus the domain homing
// (readable/hold/complete) for the scope fence; B9 ignores the extras.
const freshWith = (win: string, out: string, ws: string): {
    complete: boolean;
    topology: Array<{ out: string; complete: boolean }>;
    domains: Array<{ id: string; readable: boolean; hold: string; complete: boolean; windows: Array<{ win: string; ws: string }> }>;
} => ({
    complete: true,
    topology: [{ out, complete: true }],
    domains: [{ id: out, readable: true, hold: "", complete: true, windows: [{ win, ws }] }],
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
        setActiveRef: (ref: object | null) => void;
    };
}

function setup(win: FakeWin, order: string[], readLiveRect?: () => { x: number; y: number; w: number; h: number } | null): Harness {
    const calls: WorldCommand[] = [];
    const logs: string[] = [];
    let invalidations = 0;
    let busy = false;
    let floating = false;
    let activeRef: object | null = win as object;
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
        activeWindowRef: () => activeRef,
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
            setActiveRef: (r) => { activeRef = r; },
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

    it("routes a tiled fullscreen float through core with no rect and no writes", () => {
        const { ref, order } = makeWin({ fullScreen: true, maximizeMode: 0 });
        (ref as unknown as Record<string, unknown>)["frameGeometry"] = { x: 0, y: 0, width: 1920, height: 1080 };
        const h = setup(ref, order);
        h.env.setFloating(false);
        assert.equal(h.toggles.submitFloat("float"), true);
        assert.deepEqual((h.calls[0] as { command: Record<string, unknown> }).command, {
            op: "toggle-float", win: "win-a", rect: null,
        });
        assert.equal(ref.maximizeMode, 0, "no B9 clear runs under fullscreen");
        assert.ok(!order.includes("invalidate"), "no native setter runs under fullscreen");
    });

    it("routes a floating fullscreen unfloat with no rect, never the fullscreen frame", () => {
        const { ref, order } = makeWin({ fullScreen: true, maximizeMode: 0 });
        (ref as unknown as Record<string, unknown>)["frameGeometry"] = { x: 0, y: 0, width: 1920, height: 1080 };
        const h = setup(ref, order);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), true);
        assert.deepEqual((h.calls[0] as { command: Record<string, unknown> }).command, {
            op: "toggle-float", win: "win-a", rect: null,
        }, "fullscreen frame never becomes the retained float frame");
        assert.equal(ref.maximizeMode, 0);
        assert.ok(!order.includes("invalidate"), "no native setter runs under fullscreen");
    });

    it("takes no B9 clear under fullscreen maximized floating, submitting for core refusal", () => {
        const { ref, order } = makeWin({ fullScreen: true, maximizeMode: 3 });
        const h = setup(ref, order);
        h.env.setFloating(true);
        assert.equal(h.toggles.submitFloat("float"), true, "adapter submits; maximized core refusal owns the verdict");
        assert.deepEqual((h.calls[0] as { command: Record<string, unknown> }).command, {
            op: "toggle-float", win: "win-a", rect: null,
        });
        assert.equal(ref.maximizeMode, 3, "D5: no native state write to the fullscreen subject");
        assert.ok(!order.includes("invalidate"));
        assert.ok(!h.logs.some((l) => l.includes("float-unfloat-maximize-clear") && l.includes("outcome=issued")));
        h.toggles.pollPendingUnfloat(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.calls.length, 1, "no deferred B9 admission arms under fullscreen");
    });

    it("takes no B9 clear for tiled fullscreen maximized, submitting for core refusal", () => {
        const { ref, order } = makeWin({ fullScreen: true, maximizeMode: 3 });
        const h = setup(ref, order);
        h.env.setFloating(false);
        assert.equal(h.toggles.submitFloat("float"), true, "adapter submits; maximized core refusal owns the verdict");
        assert.deepEqual((h.calls[0] as { command: Record<string, unknown> }).command, {
            op: "toggle-float", win: "win-a", rect: null,
        });
        assert.equal(ref.maximizeMode, 3, "D5: no native state write to the fullscreen subject");
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

    it("maximizes through setMaximize and confirms at the next accepted boundary", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true), true, "issuance pends, never same-stack confirms");
        assert.ok(h.logs.some((l) => l.includes("maximize-pending") && l.includes("outcome=issued")));
        assert.equal(h.calls.length, 0);
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((l) => l.includes("maximize-applied") && l.includes("target=maximized")));
        assert.ok(!h.logs.some((l) => l.startsWith("failed:")));
        assert.equal(h.toggles.setMaximizedNative(ref as object, false), true);
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((l) => l.includes("maximize-applied") && l.includes("target=restored")));
    });

    it("fails closed without setMaximize; a silent no-op refuses once at the boundary", () => {
        const { ref } = makeWin();
        ref.setMaximize = undefined;
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true), false);
        const lying = makeWin();
        lying.ref.setMaximize = () => undefined;
        const hl = setup(lying.ref, []);
        assert.equal(hl.toggles.setMaximizedNative(lying.ref as object, true), true, "issued, pends for the boundary");
        assert.equal(hl.calls.length, 0);
        hl.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(hl.logs.some((l) => l.includes("maximize-refused") && l.includes("silent-refusal")));
        assert.ok(hl.logs.some((l) => l.includes("failed:maximize")));
        // Consumed exactly once: a later settled report admits nothing new.
        hl.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(hl.logs.filter((l) => l.includes("failed:maximize")).length, 1);
    });

    it("toggles fullscreen through fullScreen with boundary confirmation", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setFullscreenNative(ref as object, true), true);
        assert.ok(h.logs.some((l) => l.includes("fullscreen-pending") && l.includes("outcome=issued")));
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((l) => l.includes("fullscreen-applied") && l.includes("target=fullscreen")));
        assert.equal(h.toggles.setFullscreenNative(ref as object, false), true);
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((l) => l.includes("fullscreen-applied") && l.includes("target=restored")));
    });

    it("refuses a silent fullscreen no-op once at the boundary with the caller label", () => {
        const { ref } = makeWin();
        Object.defineProperty(ref, "fullScreen", { configurable: true, get: () => false, set: () => true });
        const h = setup(ref, []);
        assert.equal(h.toggles.setFullscreenNative(ref as object, true, "toggle-fullscreen"), true, "issued, pends");
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((l) => l.includes("fullscreen-refused") && l.includes("silent-refusal")));
        assert.ok(h.logs.some((l) => l.includes("failed:toggle-fullscreen")));
    });

    it("supersedes a pending maximize with newer fullscreen input silently", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true, "maximize"), true);
        assert.equal(h.toggles.setFullscreenNative(ref as object, true, "toggle-fullscreen"), true);
        assert.ok(h.logs.some((l) => l.includes("native-pending-superseded")));
        assert.ok(!h.logs.some((l) => l.includes("failed:maximize")), "superseded press never fails the old label");
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((l) => l.includes("fullscreen-applied")));
    });

    it("fences a pending maximize on scope, focus, busy, and gesture without confirming", () => {
        const scoped = makeWin();
        const hs = setup(scoped.ref, []);
        assert.equal(hs.toggles.setMaximizedNative(scoped.ref as object, true, "maximize"), true);
        hs.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-2") as never, false);
        assert.ok(hs.logs.some((l) => l.includes("scope-raced")), "scope race logged");
        assert.ok(hs.logs.some((l) => l.includes("failed:maximize")), "scope race fails the label");

        const busy = makeWin();
        const hb = setup(busy.ref, []);
        assert.equal(hb.toggles.setMaximizedNative(busy.ref as object, true, "maximize"), true);
        hb.env.setBusy(true);
        hb.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(hb.logs.some((l) => l.includes("reason=busy")), "busy logged");
        assert.ok(hb.logs.some((l) => l.includes("failed:maximize")), "busy fails the label");

        const gest = makeWin();
        const hg = setup(gest.ref, []);
        assert.equal(hg.toggles.setMaximizedNative(gest.ref as object, true, "maximize"), true);
        hg.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, true);
        assert.ok(hg.logs.some((l) => l.includes("gesture-raced")), "gesture logged");
        assert.ok(hg.logs.some((l) => l.includes("failed:maximize")), "gesture fails the label");
    });

    it("refuses a held domain once at the first boundary with no retry", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true, "maximize"), true);
        assert.ok(h.logs.some((l) => l.includes("maximize-pending")), "pending at issuance");
        assert.ok(!h.logs.some((l) => l.startsWith("failed:")), "no failure before the boundary");
        const fresh = freshWith("win-a", "out-1", "ws-1") as unknown as Record<string, unknown>;
        ((fresh["domains"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>)["hold"] = "unreadable";
        h.toggles.pollPendingNativeState(fresh as never, false);
        assert.ok(h.logs.some((l) => l.includes("maximize-refused") && l.includes("observation-held")), "held refused");
        assert.equal(h.logs.filter((l) => l.includes("failed:maximize")).length, 1, "exactly one terminal failure");
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.logs.filter((l) => l.includes("failed:maximize")).length, 1, "consumed once, no replay");
        assert.ok(!h.logs.some((l) => l.includes("maximize-applied")), "never confirms");
    });

    it("refuses unreadable and incomplete domains once at the first boundary", () => {
        const unread = makeWin();
        const hu = setup(unread.ref, []);
        assert.equal(hu.toggles.setMaximizedNative(unread.ref as object, true, "maximize"), true);
        assert.ok(!hu.logs.some((l) => l.startsWith("failed:")), "no failure before the boundary");
        const fu = freshWith("win-a", "out-1", "ws-1") as unknown as Record<string, unknown>;
        ((fu["domains"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>)["readable"] = false;
        hu.toggles.pollPendingNativeState(fu as never, false);
        assert.ok(hu.logs.some((l) => l.includes("maximize-refused") && l.includes("observation-held")), "unreadable refused");
        assert.equal(hu.logs.filter((l) => l.includes("failed:maximize")).length, 1);
        hu.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(hu.logs.filter((l) => l.includes("failed:maximize")).length, 1, "no replay");

        const incompl = makeWin();
        const hi = setup(incompl.ref, []);
        assert.equal(hi.toggles.setMaximizedNative(incompl.ref as object, true, "maximize"), true);
        const fi = freshWith("win-a", "out-1", "ws-1") as unknown as Record<string, unknown>;
        ((fi["domains"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>)["complete"] = false;
        hi.toggles.pollPendingNativeState(fi as never, false);
        assert.ok(hi.logs.some((l) => l.includes("maximize-refused") && l.includes("observation-held")), "incomplete refused");
        assert.equal(hi.logs.filter((l) => l.includes("failed:maximize")).length, 1);
    });

    it("refuses an incomplete capture once at the first boundary", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true, "maximize"), true);
        assert.ok(!h.logs.some((l) => l.startsWith("failed:")), "no failure before the boundary");
        const fresh = freshWith("win-a", "out-1", "ws-1") as unknown as Record<string, unknown>;
        fresh["complete"] = false;
        h.toggles.pollPendingNativeState(fresh as never, false);
        assert.ok(h.logs.some((l) => l.includes("maximize-refused") && l.includes("observation-held")), "incomplete capture refused");
        assert.equal(h.logs.filter((l) => l.includes("failed:maximize")).length, 1);
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.logs.filter((l) => l.includes("failed:maximize")).length, 1, "no replay");
    });

    it("refuses missing and incomplete topology once at the first boundary", () => {
        const missing = makeWin();
        const hm = setup(missing.ref, []);
        assert.equal(hm.toggles.setMaximizedNative(missing.ref as object, true, "maximize"), true);
        assert.ok(!hm.logs.some((l) => l.startsWith("failed:")), "no failure before the boundary");
        const fm = freshWith("win-a", "out-1", "ws-1") as unknown as Record<string, unknown>;
        fm["topology"] = [];
        hm.toggles.pollPendingNativeState(fm as never, false);
        assert.ok(hm.logs.some((l) => l.includes("maximize-refused") && l.includes("observation-held")), "missing topology refused");
        assert.equal(hm.logs.filter((l) => l.includes("failed:maximize")).length, 1);
        hm.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(hm.logs.filter((l) => l.includes("failed:maximize")).length, 1, "no replay");

        const incompl = makeWin();
        const hi = setup(incompl.ref, []);
        assert.equal(hi.toggles.setMaximizedNative(incompl.ref as object, true, "maximize"), true);
        const fi = freshWith("win-a", "out-1", "ws-1") as unknown as Record<string, unknown>;
        ((fi["topology"] as Array<Record<string, unknown>>)[0] as Record<string, unknown>)["complete"] = false;
        hi.toggles.pollPendingNativeState(fi as never, false);
        assert.ok(hi.logs.some((l) => l.includes("maximize-refused") && l.includes("observation-held")), "incomplete topology refused");
        assert.equal(hi.logs.filter((l) => l.includes("failed:maximize")).length, 1);
    });

    it("rejects a moved live scope at settlement even when the capture matches", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true, "maximize"), true);
        assert.ok(!h.logs.some((l) => l.startsWith("failed:")), "no failure before the boundary");
        ref.desktops = [{ id: "ws-2" }];
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((l) => l.includes("maximize-refused") && l.includes("scope-raced")), "moved live scope rejected");
        assert.equal(h.logs.filter((l) => l.includes("failed:maximize")).length, 1);
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.logs.filter((l) => l.includes("failed:maximize")).length, 1, "no replay");
    });

    it("rejects an unreadable live scope at settlement", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true, "maximize"), true);
        assert.ok(!h.logs.some((l) => l.startsWith("failed:")), "no failure before the boundary");
        ref.output = undefined;
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((l) => l.includes("maximize-refused") && l.includes("observe-missing")), "unreadable live scope refused");
        assert.equal(h.logs.filter((l) => l.includes("failed:maximize")).length, 1);
    });

    it("rejects a focus-ref mismatch at settlement like the authorized fence", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true, "maximize"), true);
        assert.ok(!h.logs.some((l) => l.startsWith("failed:")), "no failure before the boundary");
        const other = makeWin({ internalId: "win-b" });
        h.env.setActiveRef(other.ref as object);
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((l) => l.includes("maximize-refused") && l.includes("focus-raced")), "focus-ref mismatch refused");
        assert.equal(h.logs.filter((l) => l.includes("failed:maximize")).length, 1);
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.logs.filter((l) => l.includes("failed:maximize")).length, 1, "no replay");
    });

    it("cancels a pending maximize on stop and generation-style terminal causes", () => {
        const { ref } = makeWin();
        const h = setup(ref, []);
        assert.equal(h.toggles.setMaximizedNative(ref as object, true, "maximize"), true);
        h.toggles.cancelPendingNativeState();
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(!h.logs.some((l) => l.includes("maximize-applied") || l.startsWith("failed:")), "silent stop drop settles nothing");

        const { ref: ref2 } = makeWin();
        const h2 = setup(ref2, []);
        assert.equal(h2.toggles.setMaximizedNative(ref2 as object, true, "maximize"), true);
        h2.toggles.cancelPendingNativeState("generation-mismatch");
        assert.ok(h2.logs.some((l) => l.includes("generation-mismatch")), "terminal cause logged");
        assert.ok(h2.logs.some((l) => l.includes("failed:maximize")), "terminal cause fails the label");
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
            hidden: false,
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
            hidden: false,
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
            hidden: false,
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

    it("aliases native hidden into live minimized and fails closed without hidden", () => {
        assert.deepEqual(safetyOf({ minimized: false, hidden: true } as object), {
            minimized: true,
            fullscreen: null,
            maximized: null,
            sticky: null,
        });
        assert.deepEqual(safetyOf({ minimized: true, hidden: false } as object), {
            minimized: true,
            fullscreen: null,
            maximized: null,
            sticky: null,
        });
        assert.equal(safetyOf({ minimized: false } as object), null);
        assert.equal(safetyOf({ minimized: false, hidden: "yes" } as object), null);
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

    it("renames by id with read-back proof and refuses unknown ids", () => {
        const surface = {
            desktops: [{ id: "ws1", name: "wl-1" }, { id: "ws2", name: "WS2" }],
        };
        const host = createWorldApplyHost(surface as object, (v, p) => Reflect.get(v as object, p));
        assert.equal(host.renameDesktop?.("ws1", "Desktop 1"), true);
        assert.deepEqual(host.listDesktops?.(), [{ id: "ws1", name: "Desktop 1" }, { id: "ws2", name: "WS2" }]);
        assert.equal(host.renameDesktop?.("missing", "Desktop 9"), false);
    });

    it("fails rename closed when the write cannot stick", () => {
        const frozen = Object.freeze({ id: "ws1", name: "wl-1" });
        const surface = { desktops: [frozen, { id: "ws2", name: "WS2" }] };
        const host = createWorldApplyHost(surface as object, (v, p) => Reflect.get(v as object, p));
        assert.equal(host.renameDesktop?.("ws1", "Desktop 1"), false);
        assert.equal(host.listDesktops?.()?.[0]?.name, "wl-1");
    });
});

describe("world authorized maximize", () => {
    const maximizeOf = (win: string, maximized: boolean, clearFullscreen: boolean, wasMaximized: boolean) => ({
        op: "maximize" as const, win, maximized, clearFullscreen, wasMaximized,
    });
    const withRecordingFullscreen = (ref: FakeWin, order: string[], sticky: boolean): void => {
        let full = true;
        Object.defineProperty(ref, "fullScreen", {
            configurable: true,
            get: () => (sticky ? true : full),
            set: (v: boolean) => {
                order.push(`fullScreen:${v}`);
                full = v;
            },
        });
    };

    it("applies an ordinary maximize through setMaximize only, confirmed at the boundary", () => {
        const { ref, order } = makeWin();
        const h = setup(ref, order);
        assert.equal(h.toggles.applyAuthorizedMaximize(maximizeOf("win-a", true, false, false)), true, "issuance pends");
        assert.deepEqual(order, ["invalidate", "setMaximize:true,true"]);
        assert.equal(ref.maximizeMode, 3);
        assert.equal(h.calls.length, 0);
        assert.ok(h.logs.some((m) => m.includes("maximize-pending") && m.includes("outcome=issued")));
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((m) => m.includes("maximize-applied") && m.includes("target=maximized")));
        assert.ok(!h.logs.some((m) => m.startsWith("failed:")));
    });

    it("applies an ordinary restore through setMaximize only, confirmed at the boundary", () => {
        const { ref, order } = makeWin({ maximizeMode: 3 });
        const h = setup(ref, order);
        assert.equal(h.toggles.applyAuthorizedMaximize(maximizeOf("win-a", false, false, true)), true);
        assert.deepEqual(order, ["invalidate", "setMaximize:false,false"]);
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((m) => m.includes("maximize-applied") && m.includes("target=restored")));
    });

    it("clears fullscreen first then maximizes back-to-back, confirmed at the boundary", () => {
        const { ref, order } = makeWin({ fullScreen: true, maximizeMode: 0 });
        withRecordingFullscreen(ref, order, false);
        const h = setup(ref, order);
        assert.equal(h.toggles.applyAuthorizedMaximize(maximizeOf("win-a", true, true, false)), true, "issuance pends");
        assert.deepEqual(order, ["invalidate", "fullScreen:false", "setMaximize:true,true"]);
        assert.equal(ref.maximizeMode, 3);
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((m) => m.includes("maximize-applied") && m.includes("clear=fullscreen")));
        assert.ok(!h.logs.some((m) => m.startsWith("failed:")));
    });

    it("clears fullscreen and retains max on the both-flags result, confirmed at the boundary", () => {
        const { ref, order } = makeWin({ fullScreen: true, maximizeMode: 3 });
        withRecordingFullscreen(ref, order, false);
        const h = setup(ref, order);
        assert.equal(h.toggles.applyAuthorizedMaximize(maximizeOf("win-a", true, true, true)), true);
        assert.deepEqual(order, ["invalidate", "fullScreen:false", "setMaximize:true,true"]);
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((m) => m.includes("maximize-applied") && m.includes("clear=fullscreen")));
    });

    it("reports silent refusal once at the boundary when applied flags stay pending after the setters", () => {
        const { ref, order } = makeWin({ fullScreen: true, maximizeMode: 0 });
        withRecordingFullscreen(ref, order, true);
        ref.setMaximize = (v: boolean, hv: boolean): void => {
            order.push(`setMaximize:${v},${hv}`);
        };
        const h = setup(ref, order);
        assert.equal(h.toggles.applyAuthorizedMaximize(maximizeOf("win-a", true, true, false)), true, "issuance pends");
        // The clear is still requested FIRST: no applied readback is
        // required between the setters (Wayland keeps applied-full true
        // until the client acks), and nothing is retried.
        assert.deepEqual(order, ["invalidate", "fullScreen:false", "setMaximize:true,true"]);
        assert.ok(h.logs.some((m) => m.includes("maximize-pending")));
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.ok(h.logs.some((m) => m.includes("maximize-refused") && m.includes("silent-refusal")));
        assert.ok(h.logs.some((m) => m.includes("failed:maximize")), "silent no-op fails the maximize label once");
        h.toggles.pollPendingNativeState(freshWith("win-a", "out-1", "ws-1") as never, false);
        assert.equal(h.logs.filter((m) => m.includes("failed:maximize")).length, 1, "exactly once");
    });

    it("refuses a refused clear write without invoking maximize", () => {
        const { ref, order } = makeWin({ maximizeMode: 0 });
        Object.defineProperty(ref, "fullScreen", {
            configurable: true,
            get: () => true,
        });
        const h = setup(ref, order);
        assert.equal(h.toggles.applyAuthorizedMaximize(maximizeOf("win-a", true, true, false)), false);
        assert.ok(!order.some((e) => e.startsWith("setMaximize")), "no maximize call after a refused clear");
        assert.ok(h.logs.some((m) => m.includes("reason=clear-refused")));
    });

    it("refuses a stale source preimage with no writes", () => {
        const { ref, order } = makeWin();
        const h = setup(ref, order);
        assert.equal(h.toggles.applyAuthorizedMaximize(maximizeOf("win-a", true, true, false)), false);
        assert.deepEqual(order, []);
        assert.ok(h.logs.some((m) => m.includes("reason=source-raced")));
    });

    it("refuses a result for a window that lost focus with no writes", () => {
        const { ref, order } = makeWin();
        const h = setup(ref, order);
        assert.equal(h.toggles.applyAuthorizedMaximize(maximizeOf("win-b", true, false, false)), false);
        assert.deepEqual(order, []);
        assert.ok(h.logs.some((m) => m.includes("reason=focus-raced")));
    });

    it("refuses a replaced live ref with no writes", () => {
        const { ref, order } = makeWin({ internalId: "win-b" });
        const h = setup(ref, order);
        assert.equal(h.toggles.applyAuthorizedMaximize(maximizeOf("win-a", true, false, false)), false);
        assert.deepEqual(order, []);
        assert.ok(h.logs.some((m) => m.includes("observe-missing")));
    });

    it("fails closed while a world apply is in flight", () => {
        const { ref, order } = makeWin();
        const h = setup(ref, order);
        h.env.setBusy(true);
        assert.equal(h.toggles.applyAuthorizedMaximize(maximizeOf("win-a", true, false, false)), false);
        assert.deepEqual(order, []);
    });
});
