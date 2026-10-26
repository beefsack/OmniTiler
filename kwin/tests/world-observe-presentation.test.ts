// Stage 3 presentation ingress (D5): truthful native foreground, frame,
// stacking, movement, pager and effect-facts snapshots. Every fact reads
// live host state or the latest native-effect snapshot; unknown sources
// hide instead of guessing. No default style numbers, no enumeration-order
// pager fallback, no live-as-retained float/tile slots.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { captureWorldObservation, type PresentationSnapshot } from "../src/world-observe";

interface FakeWin {
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
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
    invalidation: "9",
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
        internalId: id, normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        fullScreen: false, maximizeMode: 0, onAllDesktops: false,
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

    it("does not hold ordinary candidates for non-normal windows without managed geometry", () => {
        const popup = win("win-popup", { normalWindow: false });
        Object.assign(popup, { frameGeometry: null, output: null, desktops: undefined });
        const member = win("win-member");
        const { surface } = makeSurface({ wins: [popup, member], active: member });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        assert.equal(obs.complete, true);
        assert.deepEqual(obs.windowFacts.map((row) => row.win), ["win-member"]);
    });

    it("captures client hints and frame deltas truthfully for broker conversion", () => {
        const w = win("win-hints", {
            frameGeometry: { x: 10.6, y: 20.4, width: 204.4, height: 104.5 },
            clientGeometry: { x: 12.6, y: 48.4, width: 200.4, height: 70.5 },
            minSize: { width: 176, height: 150 },
            maxSize: { width: 800, height: 600 },
        });
        const { surface } = makeSurface({ wins: [w], active: w });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        // Raw fractional facts ride verbatim (no adapter rounding or
        // verdicts): the Rust broker rounds (frame {11,20,204,105}, client
        // {13,48,200,71}, delta +4/+34) and converts to decorated hints
        // {180,184,804,634} via the shared predicates. Conversion and
        // classification are owned there and pinned by the Rust raw tests.
        const fact = obs.windowFacts.find((f) => f.win === "win-hints");
        assert.deepEqual(fact?.frameGeometry, { x: 10.6, y: 20.4, width: 204.4, height: 104.5 });
        assert.deepEqual(fact?.clientGeometry, { x: 12.6, y: 48.4, width: 200.4, height: 70.5 });
        assert.deepEqual(fact?.minSize, { width: 176, height: 150 });
        assert.deepEqual(fact?.maxSize, { width: 800, height: 600 });
        assert.deepEqual(fact?.desktops, ["ws-1"]);
        assert.equal(fact?.normalWindow, true);
    });

    it("captures fixed-size pins truthfully for broker classification", () => {
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
        assert.deepEqual(fact?.minSize, { width: 400, height: 300 });
        assert.deepEqual(fact?.maxSize, { width: 400, height: 300 });
        assert.deepEqual(fact?.clientGeometry, { x: 4, y: 4, width: 400, height: 300 });
    });

    it("captures over-bound client hints exact, never saturated", () => {
        const w = win("win-huge", {
            frameGeometry: { x: 0, y: 0, width: 16384, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 16364, height: 200 },
            minSize: { width: 16380, height: 100 },
            maxSize: null,
        });
        const { surface } = makeSurface({ wins: [w], active: w });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        // 16380 + 20 = 16400 converts broker-side and rides exact: KDE
        // must not saturate genuine constraints to accommodate the core
        // sanitizer (core caps valid carried rects at 16384 and sanitizes
        // larger values to absent today; that constraint-loss is a parked
        // shared neutrality concern, not solved here).
        const fact = obs.windowFacts.find((f) => f.win === "win-huge");
        assert.deepEqual(fact?.minSize, { width: 16380, height: 100 });
        assert.equal(fact?.maxSize, null);
        assert.deepEqual(fact?.clientGeometry, { x: 0, y: 0, width: 16364, height: 200 });
    });

    it("captures an unreadable client rect as null for broker-side nulling", () => {
        const w = win("win-noclient", {
            minSize: { width: 176, height: 150 },
            maxSize: { width: 800, height: 600 },
        });
        const { surface } = makeSurface({ wins: [w], active: w });
        const obs = observe(surface, STYLE);
        assert.ok(obs !== null);
        // No clientGeometry: no delta exists broker-side, so converted
        // bounds null instead of mixing client and frame spaces.
        const fact = obs.windowFacts.find((f) => f.win === "win-noclient");
        assert.equal(fact?.clientGeometry, null);
        assert.deepEqual(fact?.minSize, { width: 176, height: 150 });
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
        assert.deepEqual(fact?.minSize, { width: 0, height: 0 });
        assert.deepEqual(fact?.maxSize, { width: 16777215, height: 16777215 });
    });

    it("holds the fact set with zero facts on missing native booleans", () => {
        // A missing required native boolean holds the whole fact set
        // (never eligible defaults): the report stays non-null with
        // complete=false and zero facts so the core holds.
        for (const prop of ["managed", "normalWindow", "minimized", "fullScreen", "maximizeMode", "onAllDesktops"] as const) {
            const w = win("win-hold");
            delete (w as unknown as Record<string, unknown>)[prop];
            const { surface } = makeSurface({ wins: [w], active: w });
            const obs = observe(surface, STYLE);
            assert.ok(obs !== null, prop);
            assert.deepEqual(obs.windowFacts, [], `${prop} holds to zero facts`);
            assert.equal(obs.complete, false, `${prop} marks incomplete`);
        }
    });

    it("holds the fact set on unreadable transient owners and desktops", () => {
        // An unreadable transient owner never forges independence, and an
        // invalid or duplicated desktop never fabricates a single home.
        const ownerUnreadable = win("win-hold", {
            transient: true,
            transientFor: { internalId: { nested: true } },
        });
        const ownerMissing = win("win-hold", { transient: true });
        delete (ownerMissing as unknown as Record<string, unknown>)["transientFor"];
        const cases: Array<[string, FakeWin]> = [
            ["owner-unreadable", ownerUnreadable],
            ["owner-missing", ownerMissing],
            ["desktop-invalid", win("win-hold", { desktops: [{ id: "has space" }] })],
            ["desktop-duplicate", win("win-hold", { desktops: [{ id: "ws-1" }, { id: "ws-1" }] })],
        ];
        for (const [name, w] of cases) {
            const { surface } = makeSurface({ wins: [w], active: w });
            const obs = observe(surface, STYLE);
            assert.ok(obs !== null, name);
            assert.deepEqual(obs.windowFacts, [], `${name} holds to zero facts`);
            assert.equal(obs.complete, false, `${name} marks incomplete`);
        }
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
            invalidation: "3",
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
        // The causal witness rides alongside: this capture covers the
        // snapshot's counter in its instance namespace.
        assert.deepEqual(obs.nativeInvalidWitness, { instance: "effect-2", counter: "3" });
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
            // Unknown facts carry no coverage proof: missing proof yields
            // no drawable coverage, never guessed truth.
            assert.equal(obs.nativeInvalidWitness, null);
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
