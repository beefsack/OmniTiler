// Stage 3 presentation ingress (D5): truthful native foreground, frame,
// stacking, movement, pager and effect-facts snapshots. Every fact reads
// live host state or the latest native-effect snapshot; unknown sources
// hide instead of guessing. No default style numbers, no enumeration-order
// pager fallback, no live-as-retained float/tile slots.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { captureWorldObservation, type PresentationSnapshot } from "../src/world-observe";

interface FakeWin {
    internalId: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    clientGeometry?: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: Array<{ id: string }>;
    minimized: boolean;
    fullScreen?: boolean;
    fullscreen?: boolean;
    maximizeMode?: number;
    maximized?: boolean;
    onAllDesktops?: boolean;
    sticky?: boolean;
    minSize: { width: number; height: number } | null;
    maxSize: { width: number; height: number } | null;
    move?: boolean;
    resize?: boolean;
    resourceClass?: string;
}

const STYLE: PresentationSnapshot = {
    instance: "effect-1",
    rev: "4",
    chord: false,
    borderGap: 0,
    borderWidth: 3,
    underlayExtension: -1,
};

function makeSurface(opts: {
    wins: FakeWin[];
    active?: FakeWin | null;
    stacking?: FakeWin[];
    logicalBindingOf?: (nativeId: string, out?: string) => { logical: string; out: string; pin: boolean; disp: boolean; managed: boolean } | null;
}) {
    const desktops = [{ id: "ws-1" }];
    const screens = [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const surface: Record<string, unknown> = {
        windowList: () => opts.wins,
        screens,
        desktops,
        activeScreen: { name: "out-1" },
        activeWindow: opts.active ?? null,
        cursorPos: { x: 100, y: 100 },
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1040 }),
        currentDesktopForScreen: () => desktops[0],
    };
    if (opts.stacking !== undefined) {
        surface["stackingOrder"] = opts.stacking;
    }
    return { surface, desktops };
}

function observe(surface: Record<string, unknown>, facts: PresentationSnapshot | null | undefined) {
    return captureWorldObservation({
        workspace: surface,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 8,
        gestureActive: () => false,
        knownClosed: () => [],
        logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-1", pin: false, disp: false, managed: true }),
        isFloating: () => false,
        ...(facts === undefined ? {} : { presentationFacts: facts }),
    });
}

function win(id: string, overrides: Partial<FakeWin> = {}): FakeWin {
    return {
        internalId: id,
        normalWindow: true,
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
        output: { name: "out-1" },
        desktops: [{ id: "ws-1" }],
        minimized: false,
        minSize: null,
        maxSize: null,
        ...overrides,
    };
}

describe("world observation presentation ingress", () => {
    it("proves the foreground lifetime with its live frame and flags", () => {
        const fg = win("win-fg", {
            frameGeometry: { x: 10, y: 20, width: 300, height: 200 },
            fullScreen: false,
            maximizeMode: 0,
        });
        const { surface } = makeSurface({ wins: [fg], active: fg });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        const p = obs.presentation;
        assert.equal(p.foreground, "win-fg");
        assert.equal(p.minimized, false);
        assert.equal(p.maximized, false);
        assert.equal(p.fullscreen, false);
        assert.equal(p.hiddenWorkspace, false);
        assert.deepEqual(p.frame, { x: 10, y: 20, w: 300, h: 200 });
        assert.equal(p.paused, false);
    });

    it("reads a minimized foreground with no visible frame", () => {
        const fg = win("win-min", { minimized: true });
        const { surface } = makeSurface({ wins: [fg], active: fg });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        assert.equal(obs.presentation.foreground, "win-min");
        assert.equal(obs.presentation.minimized, true);
        assert.equal(obs.presentation.frame, null);
    });

    it("names no foreground when the active window is unmanaged", () => {
        const foreign = win("win-foreign", { normalWindow: false });
        const member = win("win-member");
        const { surface } = makeSurface({ wins: [foreign, member], active: foreign as unknown as FakeWin });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        // activeWindow is non-normal: liveById has no record, so the
        // foreground stays empty rather than naming unmanaged pixels.
        assert.equal(obs.presentation.foreground, "");
        assert.equal(obs.presentation.frame, null);
    });

    it("converts client hints into decorated frames via the live delta", () => {
        const w = win("win-hints", {
            frameGeometry: { x: 10.6, y: 20.4, width: 204.4, height: 104.5 },
            clientGeometry: { x: 12.6, y: 48.4, width: 200.4, height: 70.5 },
            minSize: { width: 176, height: 150 },
            maxSize: { width: 800, height: 600 },
        });
        const { surface } = makeSurface({ wins: [w], active: w });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        // Frame rounds to {11,20,204,105}, client to {13,48,200,71}:
        // delta is +4/+34 per axis on both bounds.
        const fact = obs.windowFacts.find((f) => f.win === "win-hints");
        assert.deepEqual(fact?.hints, { minW: 180, minH: 184, maxW: 804, maxH: 634 });
        assert.deepEqual(fact?.rect, { x: 11, y: 20, w: 204, h: 105 });
    });

    it("keeps fixed-size classification delta-invariant while converting hints", () => {
        const w = win("win-fixed", {
            frameGeometry: { x: 0, y: 0, width: 408, height: 308 },
            clientGeometry: { x: 4, y: 4, width: 400, height: 300 },
            minSize: { width: 400, height: 300 },
            maxSize: { width: 400, height: 300 },
        });
        const { surface } = makeSurface({ wins: [w], active: w });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        const fact = obs.windowFacts.find((f) => f.win === "win-fixed");
        assert.equal(fact?.fixedAuto, true);
        assert.deepEqual(fact?.hints, { minW: 408, minH: 308, maxW: 408, maxH: 308 });
    });

    it("reports converted hints over the carried bound exact, never saturated", () => {
        const w = win("win-huge", {
            frameGeometry: { x: 0, y: 0, width: 16384, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 16364, height: 200 },
            minSize: { width: 16380, height: 100 },
            maxSize: null,
        });
        const { surface } = makeSurface({ wins: [w], active: w });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        // 16380 + 20 = 16400 rides exact: KDE must not saturate genuine
        // constraints to accommodate the core sanitizer (core caps valid
        // carried rects at 16384 and sanitizes larger values to absent
        // today; that constraint-loss is a parked shared neutrality
        // concern, not solved here).
        const fact = obs.windowFacts.find((f) => f.win === "win-huge");
        assert.deepEqual(fact?.hints, { minW: 16400, minH: 100, maxW: null, maxH: null });
    });

    it("nulls converted bounds when the client rect is unreadable", () => {
        const w = win("win-noclient", {
            minSize: { width: 176, height: 150 },
            maxSize: { width: 800, height: 600 },
        });
        const { surface } = makeSurface({ wins: [w], active: w });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        // No clientGeometry: no delta exists, so converted bounds fail
        // closed instead of mixing client and frame spaces.
        const fact = obs.windowFacts.find((f) => f.win === "win-noclient");
        assert.deepEqual(fact?.hints, { minW: null, minH: null, maxW: null, maxH: null });
    });

    it("rides zero and sentinel hints verbatim for core-side sanitize", () => {
        const w = win("win-sentinel", {
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            minSize: { width: 0, height: 0 },
            maxSize: { width: 16777215, height: 16777215 },
        });
        const { surface } = makeSurface({ wins: [w], active: w });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        const fact = obs.windowFacts.find((f) => f.win === "win-sentinel");
        assert.deepEqual(fact?.hints, { minW: 0, minH: 0, maxW: 16777215, maxH: 16777215 });
    });

    it("orders the pager top-to-bottom from the native stacking snapshot", () => {
        const a = win("win-a");
        const b = win("win-b");
        const c = win("win-c");
        // Bottom-to-top per the pin: C bottom, B top.
        const { surface } = makeSurface({ wins: [a, b, c], active: a, stacking: [c, a, b] });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        assert.deepEqual(obs.presentation.pagerOrder, ["win-b", "win-a", "win-c"]);
    });

    it("drops unknown stacking ids instead of inventing pager members", () => {
        const a = win("win-a");
        const { surface } = makeSurface({ wins: [a], active: a });
        (surface as Record<string, unknown>)["stackingOrder"] = [
            { internalId: "win-gone" },
            a,
        ];
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        assert.deepEqual(obs.presentation.pagerOrder, ["win-a"]);
    });

    it("rides an empty pager order when stacking is unreadable", () => {
        const a = win("win-a");
        const b = win("win-b");
        const { surface } = makeSurface({ wins: [a, b], active: a });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        // windowList order is enumeration, not stacking: never claimed as
        // the native order. The core renders no pager members rather than
        // a fabricated order.
        assert.deepEqual(obs.presentation.pagerOrder, []);
    });

    it("names the focused interactive-move subject and never pauses without a game baseline", () => {
        const mover = win("win-mover", { move: true });
        const { surface } = makeSurface({ wins: [mover], active: mover });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        assert.equal(obs.presentation.moveWindow, "win-mover");
        // No focused-fullscreen-game identification baseline exists on
        // KWin: the lane parks at false instead of inventing an
        // any-fullscreen product behavior.
        const game = win("win-game", { fullScreen: true });
        const { surface: gameSurface } = makeSurface({ wins: [game], active: game });
        const gameObs = observe(gameSurface, STYLE);
        assert.ok(gameObs !== null);
        assert.equal(gameObs.presentation.fullscreen, true);
        assert.equal(gameObs.presentation.paused, false);
        assert.equal(gameObs.presentation.moveWindow, "");
    });

    it("leaves moveWindow empty for resize-only and unfocused movers", () => {
        const resizer = win("win-resizer", { resize: true });
        const bg = win("win-bg", { move: true });
        const { surface } = makeSurface({ wins: [resizer, bg], active: resizer });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        assert.equal(obs.presentation.moveWindow, "");
    });

    it("sources pager frames live only, never retained slots", () => {
        const tiled = win("win-tiled");
        const floated = win("win-floated", {
            frameGeometry: { x: 5, y: 5, width: 100, height: 100 },
        });
        const min = win("win-min", { minimized: true });
        const { surface } = makeSurface({ wins: [tiled, floated, min], active: tiled });
        const obs = captureWorldObservation({
            workspace: surface,
            readMode: () => "per-output-local",
            readGap: () => 8,
            readOuterGap: () => 8,
            gestureActive: () => false,
            knownClosed: () => [],
            logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-1", pin: false, disp: false, managed: true }),
            isFloating: (id) => id === "win-floated",
            presentationFacts: STYLE,
        });
        assert.ok(obs !== null);
        // The adapter keeps no retained frame store: float and tile ride
        // null even for floating windows. The live frame is not a retained
        // source.
        const rows = new Map(obs.presentation.pagerFrames.map((r) => [r.win, r]));
        assert.deepEqual(rows.get("win-tiled"), {
            win: "win-tiled",
            live: { x: 0, y: 0, w: 200, h: 200 },
            float: null,
            tile: null,
        });
        assert.deepEqual(rows.get("win-floated"), {
            win: "win-floated",
            live: { x: 5, y: 5, w: 100, h: 100 },
            float: null,
            tile: null,
        });
        assert.deepEqual(rows.get("win-min"), {
            win: "win-min",
            live: null,
            float: null,
            tile: null,
        });
    });

    it("carries the effect snapshot style with sentinel resolution and clamping", () => {
        const w = win("win-a");
        const { surface } = makeSurface({ wins: [w], active: w });
        const obs = observe(surface, {
            instance: "effect-2",
            rev: "9",
            chord: true,
            borderGap: 0.4,
            borderWidth: 2000,
            underlayExtension: -1,
        });
        assert.ok(obs !== null);
        // Both overlays share the one effect: a snapshot gates both flags.
        assert.equal(obs.presentation.borderEnabled, true);
        assert.equal(obs.presentation.underlayEnabled, true);
        assert.equal(obs.presentation.borderGap, 0);
        // Absurd configured values clamp so style can never refuse the
        // report.
        assert.equal(obs.presentation.borderWidth, 1024);
        // Sentinel -1 resolves to the border width before rounding.
        assert.equal(obs.presentation.underlayExtension, 1024);
        assert.equal(obs.presentation.chordHeld, true);
    });

    it("rides disabled zeros without an effect snapshot", () => {
        const w = win("win-a");
        const { surface } = makeSurface({ wins: [w], active: w });
        for (const facts of [null, undefined]) {
            const obs = observe(surface, facts);
            assert.ok(obs !== null);
            assert.equal(obs.presentation.borderEnabled, false);
            assert.equal(obs.presentation.underlayEnabled, false);
            assert.equal(obs.presentation.borderGap, 0);
            assert.equal(obs.presentation.borderWidth, 0);
            assert.equal(obs.presentation.underlayExtension, 0);
            assert.equal(obs.presentation.chordHeld, false);
        }
    });

    it("parks cloaked and shell facts without a host source", () => {
        const w = win("win-a");
        const { surface } = makeSurface({ wins: [w], active: w, stacking: [w] });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        // No KWin script cloaked or shell-surface source exists.
        assert.equal(obs.presentation.cloaked, false);
        assert.equal(obs.presentation.shell, false);
    });

    it("omits logical view receipts when host-owned", () => {
        const w = win("win-a");
        const { surface } = makeSurface({ wins: [w], active: w });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        assert.equal(obs.hostOwned, true);
        // Host-owned adapters report native current in native[], never
        // logical view receipts: the core derives views from its bindings.
        assert.deepEqual(
            obs.views,
            obs.topology.filter((t) => t.current !== "").map((t) => ({ out: t.out, view: "" })),
        );
        assert.ok(obs.native.length > 0);
    });

    it("never consumes windowAt: surfaces without it observe fully", () => {
        const w = win("win-a");
        const { surface } = makeSurface({ wins: [w], active: w });
        assert.equal("windowAt" in surface, false);
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        assert.equal(obs.presentation.foreground, "win-a");
    });
});
