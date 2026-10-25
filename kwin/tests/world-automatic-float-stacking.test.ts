// Automatic fixed-size float stacking on the shipped world route:
// admission keeps project-owned keepAbove above tiles with zero geometry
// or focus writes, preexisting below/above means zero stacking writes,
// later app/user below wins without writes, closed/replaced refs never
// take cleanup writes, explicit unfloat releases once, focused fullscreen
// games are never covered and take no writes, and a focused automatic
// float keeps native focus across automatic (af-*) reconciles with
// correlated diagnostics. Explicit command-issued (fx-*) focus always
// actuates, even under a floating focus. Fakes use the exact KWin
// property names (keepAbove/keepBelow/fullScreen/maximizeMode/
// onAllDesktops/internalId) and mirror native above/below exclusivity;
// no legacy plan-adapter coverage here (that path is not production).

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { createWorldNativeToggles } from "../src/world-native";
import { applyWorldReply } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { startWorldAdapterEntry } from "../src/world-entry";
import type { WorldObservation } from "../src/world-observe";

// ---------------------------------------------------------------------------
// Shared QML-shaped float fakes with native above/below exclusivity.
// ---------------------------------------------------------------------------

interface FloatWrites {
    above: string[];
    below: string[];
}

function makeFloatWin(
    id: string,
    writes: FloatWrites,
    opts: {
        out?: string; ws?: string; fixed?: boolean;
        above?: boolean; below?: boolean;
        fullscreen?: boolean; maximized?: boolean; sticky?: boolean;
    } = {},
): Record<string, unknown> {
    let above = opts.above === true;
    let below = opts.below === true;
    const size = opts.fixed === false
        ? { width: 200, height: 200 }
        : { width: 640, height: 480 };
    const ref: Record<string, unknown> = {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x: 0, y: 0, width: size.width, height: size.height },
        output: { name: opts.out ?? "out" },
        desktops: [{ id: opts.ws ?? "ws1" }],
        minimized: false,
        fullScreen: opts.fullscreen === true,
        maximizeMode: opts.maximized === true ? 3 : 0,
        onAllDesktops: opts.sticky === true,
        minSize: opts.fixed === false ? null : { ...size },
        maxSize: opts.fixed === false ? null : { ...size },
    };
    Object.defineProperty(ref, "keepAbove", {
        configurable: true, enumerable: true,
        get: () => above,
        set: (v: unknown) => {
            writes.above.push(`${id}=${String(v)}`);
            above = v === true;
            if (v === true) {
                below = false;
            }
        },
    });
    Object.defineProperty(ref, "keepBelow", {
        configurable: true, enumerable: true,
        get: () => below,
        set: (v: unknown) => {
            writes.below.push(`${id}=${String(v)}`);
            below = v === true;
            if (v === true) {
                above = false;
            }
        },
    });
    return ref;
}

interface NativeHarness {
    toggles: ReturnType<typeof createWorldNativeToggles>;
    logs: string[];
    writes: FloatWrites;
    live: Map<string, Record<string, unknown>>;
    surface: object;
    setActive: (id: string | null) => void;
}

// Array-like inventory exactly like the scripting host: indexed
// properties plus length on a plain object, never a JS Array.
const qmlLike = (items: ReadonlyArray<unknown>): object => {
    const o: Record<string, unknown> = { length: items.length };
    items.forEach((item, index) => {
        o[String(index)] = item;
    });
    return o;
};

function makeNativeHarness(qmlLists = false): NativeHarness {
    const logs: string[] = [];
    const writes: FloatWrites = { above: [], below: [] };
    const live = new Map<string, Record<string, unknown>>();
    let active: string | null = null;
    const surface = { windowList: () => qmlLists ? qmlLike([...live.values()]) : [...live.values()] };
    const readProp = (value: object, property: string): unknown => {
        try {
            return Reflect.get(value as object, property);
        } catch (error) {
            void error;
            return undefined;
        }
    };
    const toggles = createWorldNativeToggles({
        surface: surface as object,
        log: (m) => { logs.push(m); },
        token: (v) => String(v),
        readProp,
        shortcutFailed: (action) => { logs.push(`failed:${action}`); },
        submitCommand: () => true,
        submitSettledCommand: () => true,
        invalidate: () => undefined,
        scheduleDispatch: () => undefined,
        persistFloatIntent: () => undefined,
        readLiveRect: () => ({ x: 0, y: 0, w: 640, h: 480 }),
        isWorldBusy: () => false,
        isFloating: () => false,
        noteFloating: () => undefined,
        clearFloating: () => undefined,
        activeWindowId: () => active,
        activeWindowRef: () => (active === null ? null : (live.get(active) ?? null) as object | null),
        liveRefById: (id) => (live.get(id) ?? null) as object | null,
    });
    return {
        toggles, logs, writes, live, surface,
        setActive: (id: string | null) => { active = id; },
    };
}

interface FloatEntryOpts {
    out?: string; ws?: string; fixedAuto?: boolean; fixedSuppress?: boolean;
    floating?: boolean; sticky?: boolean; fullscreen?: boolean;
    maximized?: boolean; minimized?: boolean;
}

function floatObservation(
    live: Map<string, Record<string, unknown>>,
    entries: Record<string, FloatEntryOpts>,
    focusedGlobal: string,
): WorldObservation {
    const byDomain = new Map<string, WorldObservation["domains"][number]["windows"]>();
    const minimized: WorldObservation["minimized"] = [];
    for (const [win, opts] of Object.entries(entries)) {
        const ref = live.get(win);
        assert.ok(ref !== undefined, `live ref for ${win}`);
        const floating = opts.floating !== false;
        const entry = {
            win, ref: ref as object,
            out: opts.out ?? "out", ws: opts.ws ?? "ws1",
            rect: { x: 0, y: 0, w: 640, h: 480 },
            floating,
            fitExcluded: floating,
            fullscreen: opts.fullscreen === true,
            maximized: opts.maximized === true,
            sticky: opts.sticky === true,
            fixedAuto: opts.fixedAuto !== false,
            fixedSuppress: opts.fixedSuppress === true, nativeWs: null,
            hints: { minW: 640, minH: 480, maxW: 640, maxH: 480 },
            minimized: opts.minimized === true,
        };
        if (opts.minimized === true) {
            minimized.push(entry);
            continue;
        }
        const key = `${entry.out} ${entry.ws}`;
        const arr = byDomain.get(key) ?? [];
        arr.push(entry);
        byDomain.set(key, arr);
    }
    const domains: WorldObservation["domains"] = [...byDomain].map(([key, windows]) => {
        const [out, ws] = key.split(" ");
        return {
            id: out as string, ws: ws as string,
            bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 4, focused: "", readable: true, hold: "",
            adjacent: {}, windows, closed: [], complete: true,
        };
    });
    return {
        mode: "per-output-local",
        topology: [{ out: "out", order: ["ws1"], current: "ws1", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
        bindings: [],
        domains,
        minimized,
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal,
        screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

describe("world native automatic fixed-float stacking", () => {
    it("admits a fresh automatic float above exactly once with no below write", () => {
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        const fresh = floatObservation(h.live, { "win-b": {} }, "");
        h.toggles.syncAutomaticFloatStacking(fresh as never);
        assert.deepEqual(h.writes.above, ["win-b=true"], "exactly one keepAbove write");
        assert.deepEqual(h.writes.below, [], "zero keepBelow writes");
        assert.equal(Reflect.get(ref, "keepAbove"), true);
        // A retained observation adds no stacking writes.
        h.toggles.syncAutomaticFloatStacking(fresh as never);
        assert.deepEqual(h.writes.above, ["win-b=true"], "retained adds nothing");
        assert.ok(h.logs.some((l) => l.includes("automatic-keep-above") && l.includes("outcome=applied")), "applied diagnostic");
    });

    it("preexisting keepBelow means zero stacking writes through admission, tile and teardown", () => {
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes, { below: true });
        h.live.set("win-b", ref);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, [], "admission writes nothing above");
        assert.deepEqual(h.writes.below, [], "admission writes nothing below");
        // Explicit tile of a never-owned float writes nothing.
        h.toggles.releaseAutomaticFloat("win-b", "explicit-unfloat");
        assert.deepEqual(h.writes.above, [], "tile writes nothing");
        assert.deepEqual(h.writes.below, [], "tile never writes below");
        // Teardown of a never-owned float writes nothing.
        h.live.delete("win-b");
        h.toggles.syncAutomaticFloatStacking(floatObservation(new Map(), {}, "") as never);
        assert.deepEqual(h.writes.above, [], "teardown writes nothing");
        assert.deepEqual(h.writes.below, [], "teardown never writes below");
    });

    it("preexisting keepAbove stays untouched with zero writes and no restoration", () => {
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes, { above: true });
        h.live.set("win-b", ref);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, [], "zero writes over preowned above");
        assert.deepEqual(h.writes.below, [], "zero below writes");
        assert.equal(Reflect.get(ref, "keepAbove"), true, "preowned above intact");
        h.toggles.releaseAutomaticFloat("win-b", "explicit-unfloat");
        assert.deepEqual(h.writes.above, [], "no restoration write for unowned above");
        assert.equal(Reflect.get(ref, "keepAbove"), true, "still intact");
    });

    it("later app keepBelow wins: owned above releases with zero writes", () => {
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true"]);
        // The app sets keepBelow natively; exclusivity already cleared above.
        Reflect.set(ref, "keepBelow", true);
        assert.equal(Reflect.get(ref, "keepAbove"), false, "exclusivity cleared above");
        const writesBefore = h.writes.above.length + h.writes.below.length;
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.equal(h.writes.above.length + h.writes.below.length, writesBefore, "release writes nothing");
        assert.equal(Reflect.get(ref, "keepBelow"), true, "app below choice stands");
        // No re-acquire fight on the same lifetime.
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.equal(h.writes.above.length + h.writes.below.length, writesBefore, "same ref never re-acquired");
    });

    it("closed refs take no cleanup writes; a replaced ref under a reused id stays silent then re-admits fresh", () => {
        const h = makeNativeHarness();
        const refA = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", refA);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true"]);
        // Close gap: the id leaves the live inventory, then a fresh native
        // reference arrives under the reused id.
        h.live.delete("win-b");
        h.toggles.syncAutomaticFloatStacking(floatObservation(new Map(), {}, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true"], "closed ref takes no cleanup write");
        const refB = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", refB);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true", "win-b=true"], "only the fresh ref acquires");
        assert.equal(Reflect.get(refB, "keepAbove"), true);
    });

    it("explicit unfloat releases owned above exactly once and never writes below", () => {
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        h.toggles.releaseAutomaticFloat("win-b", "explicit-unfloat");
        assert.deepEqual(h.writes.above, ["win-b=true", "win-b=false"], "release writes false once");
        assert.deepEqual(h.writes.below, [], "never writes below");
        h.toggles.releaseAutomaticFloat("win-b", "explicit-unfloat");
        assert.deepEqual(h.writes.above, ["win-b=true", "win-b=false"], "second release is a no-op");
    });

    it("a focused fullscreen sibling suspends owned above with no writes to the game, then re-acquires on exit", () => {
        const h = makeNativeHarness();
        const gameWrites: FloatWrites = { above: [], below: [] };
        const game = makeFloatWin("win-game", gameWrites, { fixed: false, fullscreen: true });
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        h.live.set("win-game", game);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true"]);
        // The game focuses fullscreen in the same scope: the owned float
        // must drop below it without touching the game client.
        const withGame = floatObservation(
            h.live,
            { "win-b": {}, "win-game": { fixedAuto: false, floating: false, fullscreen: true } },
            "win-game",
        );
        h.toggles.syncAutomaticFloatStacking(withGame as never);
        assert.deepEqual(h.writes.above, ["win-b=true", "win-b=false"], "suspend restores exactly once");
        assert.deepEqual(gameWrites.above, [], "zero writes to the fullscreen client");
        assert.deepEqual(gameWrites.below, [], "zero below writes to the fullscreen client");
        assert.deepEqual(h.writes.below, [], "suspend never writes below");
        // Admission under a focused fullscreen game writes nothing.
        const before = h.writes.above.length;
        h.toggles.syncAutomaticFloatStacking(withGame as never);
        assert.equal(h.writes.above.length, before, "no admission while the game holds the scope");
        // Game exit re-acquires the same lifetime.
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true", "win-b=false", "win-b=true"], "exit re-acquires");
    });

    it("a user-cleared above drops ownership with zero writes and never fights the same ref", () => {
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        // The user clears above directly (no below): the project must not
        // re-assert stacking over the same lifetime.
        Reflect.set(ref, "keepAbove", false);
        const before = h.writes.above.length;
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.equal(h.writes.above.length, before, "no re-assert write");
        assert.equal(h.writes.below.length, 0);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.equal(h.writes.above.length, before, "still quiet on retention");
    });

    it("a live-fullscreen applicant takes no stacking write despite a stale plain observation", () => {
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes);
        ref["fullScreen"] = true;
        h.live.set("win-b", ref);
        // The capture predates the native flip, so observation still reads
        // plain: every write path must re-read live overlay state first.
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, [], "zero writes to the fullscreen applicant");
        assert.deepEqual(h.writes.below, [], "zero below writes");
    });

    it("stale observation never writes a live-fullscreen owned float, even on release", () => {
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true"]);
        ref["fullScreen"] = true;
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true"], "suspend writes nothing to the fullscreen client");
        h.toggles.releaseAutomaticFloat("win-b", "explicit-unfloat");
        assert.deepEqual(h.writes.above, ["win-b=true"], "release writes nothing to the fullscreen client");
        assert.deepEqual(h.writes.below, [], "never writes below");
        assert.equal(Reflect.get(ref, "keepAbove"), true, "ownership retained without writes");
    });

    it("fixedAuto classification loss restores owned above once", () => {
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true"]);
        // The core stops classifying the window automatic (hint drift
        // without retained origin): the owned above restores once, then
        // the ordinary classification holds it.
        const lost = floatObservation(h.live, { "win-b": { fixedAuto: false } }, "");
        h.toggles.syncAutomaticFloatStacking(lost as never);
        assert.deepEqual(h.writes.above, ["win-b=true", "win-b=false"], "restores exactly once");
        h.toggles.syncAutomaticFloatStacking(lost as never);
        assert.deepEqual(h.writes.above, ["win-b=true", "win-b=false"], "no repeat writes");
    });

    it("classification loss under live fullscreen writes nothing and retains ownership", () => {
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        ref["fullScreen"] = true;
        const lost = floatObservation(h.live, { "win-b": { fixedAuto: false } }, "");
        h.toggles.syncAutomaticFloatStacking(lost as never);
        assert.deepEqual(h.writes.above, ["win-b=true"], "zero writes to the fullscreen client");
        assert.equal(Reflect.get(ref, "keepAbove"), true, "retained for a later retry");
    });

    it("a live-fullscreen focus suspends through live verification, not the stale capture", () => {
        const h = makeNativeHarness();
        const gameWrites: FloatWrites = { above: [], below: [] };
        const game = makeFloatWin("win-game", gameWrites, { fixed: false });
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        h.live.set("win-game", game);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true"]);
        // Native focus sits fullscreen on the game while every capture
        // still reads plain and unfocused: live verification suspends.
        game["fullScreen"] = true;
        h.setActive("win-game");
        const stale = floatObservation(
            h.live,
            { "win-b": {}, "win-game": { fixedAuto: false, floating: false } },
            "",
        );
        h.toggles.syncAutomaticFloatStacking(stale as never);
        assert.deepEqual(h.writes.above, ["win-b=true", "win-b=false"], "live-verified suspend restores once");
        assert.deepEqual(gameWrites.above, [], "zero writes to the game");
        assert.deepEqual(gameWrites.below, [], "zero below writes to the game");
    });

    it("resolves liveness through QML-shaped array-like inventories", () => {
        const h = makeNativeHarness(true);
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true"], "array-like windowList still converges");
        assert.deepEqual(h.writes.below, [], "zero below writes");
    });
});

// ---------------------------------------------------------------------------
// World apply focus preservation: a focused automatic float survives an
// automatic reconcile carrying tiled-survivor focus; nothing activates over
// it, with correlated attempt/result diagnostics on the actuated path.
// ---------------------------------------------------------------------------

function applyObservation(focused: string, focusIsFloat: boolean) {
    const floatRef = { id: "win-b" };
    const tileRef = { id: "win-a" };
    const win = (id: string, floating: boolean, fixedAuto: boolean) => ({
        win: id, ref: id === "win-b" ? floatRef : tileRef,
        out: "out", ws: "ws",
        rect: { x: 0, y: 0, w: 100, h: 100 },
        floating, fitExcluded: floating,
        fullscreen: false, maximized: false, sticky: false,
        fixedAuto, fixedSuppress: false, nativeWs: null,
        hints: { minW: null, minH: null, maxW: null, maxH: null },
        minimized: false,
    });
    return {
        observation: {
            mode: "per-output-local",
            topology: [{ out: "out", order: ["ws"], current: "ws", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
            bindings: [],
            domains: [{
                id: "out", ws: "ws",
                bounds: { x: 0, y: 0, w: 800, h: 600 },
                gap: 8, outer: 4, focused: focused, readable: true, hold: "",
                adjacent: {}, closed: [], complete: true,
                windows: [
                    win("win-a", false, false),
                    win("win-b", focusIsFloat, focusIsFloat),
                ],
            }],
            minimized: [],
            predicate: "both-axes-fixed",
            complete: true,
            gesture: false,
            focusedGlobal: focused,
            screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
            cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
            focusFollowsCursor: false, cursorWarp: false,
            switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
            presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
        } as WorldObservation,
        refs: { floatRef: floatRef as object, tileRef: tileRef as object },
    };
}

function applyReply(obs: string, target: string, id = "fx-1") {
    return {
        adapter: "adapter-1", gen: "gen-1", req: "req-1", obs, rev: "7",
        ready: true, consumed: false,
        source: "req-1",
        nativeInvalid: "1",
        clock: "0",
        gestureSettled: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [], workspaces: [], outputs: [],
        windows: [], transfers: [],
        effects: [{ id, kind: "focus", target, rev: "7" }],
        result: null,
    };
}

function applyState(obs: string): WorldAdapterState {
    const state = new WorldAdapterState("adapter-1", "owner-a");
    state.onDiscovery("gen-1", "broker-1");
    while (state.currentInvalid !== obs) {
        state.invalidate();
    }
    return state;
}

describe("world apply automatic-float focus preservation", () => {
    it("a focused automatic float keeps native focus across an automatic af-* reconcile", () => {
        const { observation, refs } = applyObservation("win-b", true);
        // Producer-accurate automatic admission intent (world_engine.rs
        // mints af-<window>); explicit fx-<seq> must never take this path.
        const reply = applyReply("3", "win-a", "af-win-a") as never;
        const state = applyState("3");
        const logs: string[] = [];
        const focusWrites: unknown[] = [];
        const result = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            state,
            observation,
            reply,
            log: (m) => { logs.push(m); },
            isStillListed: () => true,
            isActiveWindow: (ref) => ref === refs.floatRef,
            readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
            readLiveMembership: () => ({ out: "out", ws: "ws" }),
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: () => true,
            setMembership: () => true,
            sendToOutput: () => true,
            resolveOutput: (out: string) => ({ name: out }),
            resolveDesktop: (ws: string) => ({ id: ws }),
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: (ref) => { focusWrites.push(ref); return true; },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => null,
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.deepEqual(focusWrites, [], "zero activation writes to any window");
        assert.equal(result.effects, 0, "no focus effect counted");
        assert.ok(logs.some((l) => l.includes("focus-skipped") && l.includes("reason=automatic-active") && l.includes("target=win-a")), "automatic-active skip logged");
        assert.ok(!logs.some((l) => l.includes("automatic-focus-attempt")), "protected skip attempts no setter");
    });

    it("an explicit fx-* follow focus actuates under a floating focus", () => {
        // Producer-accurate explicit command outcome (world_report.rs
        // mints fx-<seq> for executed focus commands, e.g. send-follow):
        // user intent must never be swallowed by float preservation.
        const { observation, refs } = applyObservation("win-b", true);
        const reply = applyReply("3", "win-a", "fx-3") as never;
        const state = applyState("3");
        const logs: string[] = [];
        const focusWrites: unknown[] = [];
        const result = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            state,
            observation,
            reply,
            log: (m) => { logs.push(m); },
            isStillListed: () => true,
            isActiveWindow: (ref) => ref === refs.tileRef,
            readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
            readLiveMembership: () => ({ out: "out", ws: "ws" }),
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: () => true,
            setMembership: () => true,
            sendToOutput: () => true,
            resolveOutput: (out: string) => ({ name: out }),
            resolveDesktop: (ws: string) => ({ id: ws }),
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: (ref) => { focusWrites.push(ref); return true; },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => "ws",
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.deepEqual(focusWrites, [refs.tileRef], "explicit follow actuates despite the floating focus");
        assert.equal(result.effects, 1);
        assert.ok(!logs.some((l) => l.includes("focus-skipped")), "no preservation skip for explicit focus");
    });

    it("a tiled focus target still actuates with correlated attempt/result diagnostics", () => {
        const { observation, refs } = applyObservation("win-a", false);
        const reply = applyReply("3", "win-b", "fx-7") as never;
        const state = applyState("3");
        const logs: string[] = [];
        const focusWrites: unknown[] = [];
        const result = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            state,
            observation,
            reply,
            log: (m) => { logs.push(m); },
            isStillListed: () => true,
            isActiveWindow: (ref) => ref === refs.floatRef,
            readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
            readLiveMembership: () => ({ out: "out", ws: "ws" }),
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: () => true,
            setMembership: () => true,
            sendToOutput: () => true,
            resolveOutput: (out: string) => ({ name: out }),
            resolveDesktop: (ws: string) => ({ id: ws }),
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: (ref) => { focusWrites.push(ref); return true; },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => "ws",
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.deepEqual(focusWrites, [refs.floatRef], "actuates the planned target");
        assert.equal(result.effects, 1);
        assert.ok(logs.some((l) => l.includes("automatic-focus-attempt") && l.includes("target=win-b")), "attempt logged");
        assert.ok(logs.some((l) => l.includes("automatic-focus-result") && l.includes("target=win-b") && l.includes("outcome=setter-accepted active=true")), "acceptance logged");
    });

    it("an accepted setter without native activation reports active=false, not refused", () => {
        const { observation, refs } = applyObservation("win-a", false);
        const reply = applyReply("3", "win-b", "fx-7") as never;
        const state = applyState("3");
        const logs: string[] = [];
        const focusWrites: unknown[] = [];
        const result = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            state,
            observation,
            reply,
            log: (m) => { logs.push(m); },
            isStillListed: () => true,
            // Setter accepts, but native activation never proves: the
            // outcome must stay setter-accepted with a separate false
            // active token instead of degrading to setter-refused.
            isActiveWindow: () => false,
            readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
            readLiveMembership: () => ({ out: "out", ws: "ws" }),
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: () => true,
            setMembership: () => true,
            sendToOutput: () => true,
            resolveOutput: (out: string) => ({ name: out }),
            resolveDesktop: (ws: string) => ({ id: ws }),
            switchView: () => true,
            createDesktop: () => undefined,
            removeDesktop: () => false,
            focusWindow: (ref) => { focusWrites.push(ref); return true; },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => "ws",
            desktopById: (id: string) => ({ id }),
            minDesktops: 2,
        });
        assert.deepEqual(focusWrites, [refs.floatRef], "setter ran once");
        assert.equal(result.effects, 0, "unproven activation counts nothing");
        assert.ok(logs.some((l) => l.includes("automatic-focus-result") && l.includes("outcome=setter-accepted active=false")), "accepted without activation");
        assert.ok(!logs.some((l) => l.includes("outcome=setter-refused")), "never misreported as refused");
    });
});

// ---------------------------------------------------------------------------
// Production entry: automatic keepAbove exactly once with zero frame, focus
// or keepBelow writes; owned lifecycle across tile/unfloat, replacement,
// restart and fullscreen legs.
// ---------------------------------------------------------------------------

interface EntryDesk { id: string; name: string }
interface EntryWin {
    internalId: string;
    normalWindow: boolean;
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: EntryDesk[];
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minSize: { width: number; height: number } | null;
    maxSize: { width: number; height: number } | null;
    setMaximize: (v: boolean, h: boolean) => void;
}

interface EntryHarness {
    wins: EntryWin[];
    world: {
        screens: Array<{ name: string; geometry: { x: number; y: number; width: number; height: number } }>;
        desktops: EntryDesk[];
        activeScreen: { name: string };
        activeWindow: EntryWin | null;
        currentDesktop: EntryDesk;
        focused: string[];
        geometryWrites: string[];
    };
    calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>;
    timers: Array<{ delay: number; cb: () => void }>;
    logs: string[];
    aboveWrites: string[];
    belowWrites: string[];
    handle: ReturnType<typeof startWorldAdapterEntry>;
    runTimers: (maxDelay?: number) => void;
    rev: { n: number };
}

function entryWin(id: string, out: string, ws: EntryDesk, fixed: boolean, above: string[], below: string[]): EntryWin {
    let keepAbove = false;
    let keepBelow = false;
    const w = {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x: 0, y: 0, width: fixed ? 640 : 200, height: fixed ? 480 : 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: fixed ? { width: 640, height: 480 } : null,
        maxSize: fixed ? { width: 640, height: 480 } : null,
        setMaximize: (v: boolean, h: boolean): void => { w.maximizeMode = v && h ? 3 : 0; },
    } as EntryWin;
    Object.defineProperty(w, "keepAbove", {
        configurable: true, enumerable: true,
        get: () => keepAbove,
        set: (v: unknown) => {
            above.push(`${id}=${String(v)}`);
            keepAbove = v === true;
            if (v === true) {
                keepBelow = false;
            }
        },
    });
    Object.defineProperty(w, "keepBelow", {
        configurable: true, enumerable: true,
        get: () => keepBelow,
        set: (v: unknown) => {
            below.push(`${id}=${String(v)}`);
            keepBelow = v === true;
            if (v === true) {
                keepAbove = false;
            }
        },
    });
    return w;
}

function makeEntryHarness(qmlLists = false): EntryHarness {
    const ws1: EntryDesk = { id: "ws1", name: "ws1" };
    const ws2: EntryDesk = { id: "ws2", name: "ws2" };
    const aboveWrites: string[] = [];
    const belowWrites: string[] = [];
    const wins: EntryWin[] = [
        entryWin("win-a", "out", ws1, false, aboveWrites, belowWrites),
        entryWin("win-b", "out", ws1, true, aboveWrites, belowWrites),
    ];
    const world = {
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1, ws2],
        activeScreen: { name: "out" },
        activeWindow: wins[0] as EntryWin | null,
        currentDesktop: ws1,
        focused: [] as string[],
        geometryWrites: [] as string[],
    };
    const calls: EntryHarness["calls"] = [];
    const timers: EntryHarness["timers"] = [];
    const logs: string[] = [];
    const surface = {
        windowList: () => qmlLists ? qmlLike(wins) : wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            const found = wins.find((w) => (w as unknown) === ref);
            if (found !== undefined) {
                world.focused.push(found.internalId);
            }
        },
        get currentDesktop() {
            return world.currentDesktop;
        },
        set currentDesktop(ref: unknown) {
            const found = world.desktops.find((d) => (d as unknown) === ref);
            if (found !== undefined) {
                world.currentDesktop = found;
            }
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => world.currentDesktop,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => undefined,
        removeDesktop: () => false,
    };
    for (const w of wins) {
        let frame = w.frameGeometry;
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                world.geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
    }
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "QueueWorldCommand") {
                const queueBody = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({ v: 2, outcome: "queued", gen: "gen-1",
                    client: queueBody["client"], clientCmd: queueBody["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                const stagedTake = (globalThis as Record<string, unknown>)["__takeResult"] as Record<string, unknown> | undefined;
                if (stagedTake !== undefined) {
                    delete (globalThis as Record<string, unknown>)["__takeResult"];
                    callback(JSON.stringify({ v: 2, outcome: "ready", gen: "gen-1", globalCmd: "1", result: stagedTake }));
                } else {
                    callback(JSON.stringify({ v: 2, outcome: "pending", gen: "gen-1" }));
                }            }
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (message) => { logs.push(message); },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        readDefaultTiledFn: () => true,
    });
    assert.ok(handle !== null);
    const runTimers = (maxDelay = 120): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= maxDelay) {
                timers.splice(i, 1);
                i -= 1;
                t.cb();
            }
        }
    };
    return { wins, world, calls, timers, logs, aboveWrites, belowWrites, handle, runTimers, rev: { n: 0 } };
}

function entryAcceptedReply(gen: string, source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 2, adapter: "broker-1", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source,
        nativeInvalid,
        clock: "0",
        gestureSettled: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
        workspaces: [],
        outputs: [],
        windows: [],
        transfers: [],
        effects: [],
        result: null,
        ...extra,
    });
}

function entryDiscover(h: EntryHarness, gen = "gen-1"): void {
    const call = [...h.calls].reverse().find((c) => c.method === "DiscoverWorld");
    assert.ok(call !== undefined);
    const body = JSON.parse(call.payload);
    call.callback(JSON.stringify({
        v: 2, adapter: "broker-1", gen, req: body.req, obs: "0", rev: "0",
        ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
        gestureSettled: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        windows: [], transfers: [], effects: [], result: null,
    }));
    const auto = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(auto !== undefined);
    const autoBody = JSON.parse(auto.payload);
    auto.callback(entryAcceptedReply(gen, autoBody.source, autoBody.nativeInvalid, "0"));
}

function entryRound(h: EntryHarness, extra: Record<string, unknown> = {}): void {
    h.handle?.requestWorkspaceRelative(1);
    h.runTimers();
    const call = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(call !== undefined);
    const body = JSON.parse(call.payload);
    h.rev.n += 1;
    // Terminal results ride the take lane requester-only, never the
    // capture reply.
    const { result, ...snapshot } = extra;
    if (result !== undefined) {
        (globalThis as Record<string, unknown>)["__takeResult"] = result;
    }
    call.callback(entryAcceptedReply("gen-1", body.source, body.nativeInvalid, String(h.rev.n), snapshot));
}

describe("world entry automatic-float stacking", () => {
    it("admits the automatic float above exactly once with zero frame, focus or below writes", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true"], "admission writes keepAbove exactly once");
        assert.deepEqual(h.belowWrites, [], "zero keepBelow writes");
        assert.ok(!h.world.geometryWrites.some((w) => w.startsWith("win-b@")), "zero frame writes to the float");
        assert.deepEqual(h.world.focused, [], "zero focus writes");
        entryRound(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true"], "retained observation adds no stacking writes");
        assert.ok(h.logs.some((l) => l.includes("automatic-keep-above") && l.includes("outcome=applied")), "applied diagnostic");
    });

    it("a focused automatic float survives a survivor-focus reconcile with no activation", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        h.world.activeWindow = h.wins.find((w) => w.internalId === "win-b") ?? null;
        entryRound(h, {
            effects: [{ id: "af-win-a", kind: "focus", target: "win-a", rev: "9" }],
        });
        assert.deepEqual(h.world.focused, [], "no activation over the focused float");
        assert.ok(h.logs.some((l) => l.includes("focus-skipped") && l.includes("reason=automatic-active")), "automatic-active skip");
    });

    it("an explicit follow focus actuates under a focused automatic float", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        h.world.activeWindow = h.wins.find((w) => w.internalId === "win-b") ?? null;
        entryRound(h, {
            effects: [{ id: "fx-3", kind: "focus", target: "win-a", rev: "9" }],
        });
        assert.deepEqual(h.world.focused, ["win-a"], "explicit fx-* follow actuates");
        assert.ok(!h.logs.some((l) => l.includes("focus-skipped")), "no preservation skip for explicit focus");
    });

    it("explicit unfloat releases owned above once, then tiles with suppression and no below write", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true"]);
        entryRound(h, {
            result: { op: "toggle-float", win: "win-b", rect: null, noSpace: false },
        });
        assert.deepEqual(h.aboveWrites, ["win-b=true", "win-b=false"], "explicit tile releases once");
        assert.deepEqual(h.belowWrites, [], "tile never writes below");
        entryRound(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true", "win-b=false"], "no repeat writes");
    });

    it("a replaced native reference under a reused id re-admits fresh with no stale writes", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true"]);
        // Close gap: the float leaves the inventory for one accepted round.
        const closed = h.wins.find((w) => w.internalId === "win-b");
        assert.ok(closed !== undefined);
        h.wins.splice(h.wins.indexOf(closed), 1);
        entryRound(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true"], "closed ref takes no cleanup write");
        // A fresh native reference arrives under the reused id.
        const ws1 = h.world.desktops[0] as EntryDesk;
        const fresh = entryWin("win-b", "out", ws1, true, h.aboveWrites, h.belowWrites);
        h.wins.push(fresh);
        entryRound(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true", "win-b=true"], "only the fresh ref acquires");
        assert.equal((fresh as unknown as Record<string, unknown>)["keepAbove"], true);
    });

    it("restart after explicit stop re-admits fresh with exactly one write", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true"]);
        // Explicit stop best-effort releases; a later restart over the
        // same live clients re-admits the automatic float exactly once.
        h.handle?.stop();
        assert.deepEqual(h.aboveWrites, ["win-b=true", "win-b=false"]);
        const h2 = makeEntryHarness();
        h2.wins.splice(0, h2.wins.length, ...h.wins);
        h2.world.activeWindow = h.wins[0] ?? null;
        entryDiscover(h2);
        assert.deepEqual(h.aboveWrites, ["win-b=true", "win-b=false", "win-b=true"], "restart re-admits once");
        assert.deepEqual(h2.aboveWrites, [], "restart instance wrote nothing itself");
        assert.deepEqual(h2.belowWrites, [], "zero below writes");
        entryRound(h2);
        assert.deepEqual(h.aboveWrites, ["win-b=true", "win-b=false", "win-b=true"], "retained adds nothing");
        h2.handle?.stop();
    });

    it("a focused fullscreen game suspends owned floats with zero writes to the game", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true"]);
        const game = h.wins.find((w) => w.internalId === "win-a") as EntryWin;
        game.fullScreen = true;
        h.world.activeWindow = game;
        entryRound(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true", "win-b=false"], "owned float suspends");
        assert.deepEqual(h.belowWrites, [], "no below writes");
        assert.deepEqual(h.world.focused, [], "no focus writes to the game");
        assert.ok(!h.world.geometryWrites.some((w) => w.startsWith("win-a@")), "no frame writes to the game");
        // Game exit re-acquires the same lifetime.
        game.fullScreen = false;
        entryRound(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true", "win-b=false", "win-b=true"], "exit re-acquires");
    });

    it("explicit stop best-effort releases owned above (handle.stop only, not KWin unload)", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true"]);
        // handle.stop() is the only harnessed teardown: the actual KWin
        // script unload path is unharnessed and makes no claim here.
        h.handle?.stop();
        assert.deepEqual(h.aboveWrites, ["win-b=true", "win-b=false"], "stop releases owned above once");
        assert.deepEqual(h.belowWrites, [], "stop never writes below");
    });

    it("explicit stop writes nothing to a live-fullscreen owned float", () => {
        const h = makeEntryHarness();
        entryDiscover(h);
        const owned = h.wins.find((w) => w.internalId === "win-b") as EntryWin;
        owned.fullScreen = true;
        h.handle?.stop();
        assert.deepEqual(h.aboveWrites, ["win-b=true"], "zero writes to the fullscreen client on stop");
        assert.deepEqual(h.belowWrites, [], "zero below writes");
    });

    it("a refused stop retains ownership so a later stop retries the release", () => {
        // A fullscreen-refused stop must retain ownership (comment
        // contract); clearing the overlay then stopping again releases.
        const h = makeNativeHarness();
        const ref = makeFloatWin("win-b", h.writes);
        h.live.set("win-b", ref);
        h.toggles.syncAutomaticFloatStacking(floatObservation(h.live, { "win-b": {} }, "") as never);
        assert.deepEqual(h.writes.above, ["win-b=true"], "setup owns above");
        (ref as Record<string, unknown>)["fullScreen"] = true;
        h.toggles.releaseAllAutomaticFloats("stop");
        assert.deepEqual(h.writes.above, ["win-b=true"], "refused stop writes nothing");
        (ref as Record<string, unknown>)["fullScreen"] = false;
        h.toggles.releaseAllAutomaticFloats("stop");
        assert.deepEqual(h.writes.above, ["win-b=true", "win-b=false"], "retry releases after the overlay clears");
        assert.deepEqual(h.writes.below, [], "never writes below");
        h.toggles.releaseAllAutomaticFloats("stop");
        assert.deepEqual(h.writes.above, ["win-b=true", "win-b=false"], "second success is a no-op");
    });

    it("admits above tiles through QML-shaped array-like inventories", () => {
        const h = makeEntryHarness(true);
        entryDiscover(h);
        assert.deepEqual(h.aboveWrites, ["win-b=true"], "array-like windowList still converges");
        assert.deepEqual(h.belowWrites, [], "zero keepBelow writes");
        assert.ok(!h.world.geometryWrites.some((w) => w.startsWith("win-b@")), "zero frame writes to the float");
        assert.deepEqual(h.world.focused, [], "zero focus writes");
        h.handle?.stop();
    });
});
