// K1b2b: exact v4 producer-lifetime binding plus ordinary native
// frame/membership/view apply. Real Rust v4 fixtures only (mechanically
// extended from the world-reply-v4 bound helper): no v3 shapes, no
// state-target/arrival/focus-feedback migration, no renderer coverage.
// Persistent intent-write gating stays entry-owned and is covered by
// world-entry-intent.test.ts; this file covers the state/apply slice.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import { applyWorldReply } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { isCoreWorldReply, parseWorldReply, type WorldReply } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";

const DIR = join("..", "test-fixtures", "world-wire", "v4-active", "messages");
const ADAPTER = "adapter-a";
const GEN = "generation-a";

const doc = (name: string): Record<string, unknown> =>
    JSON.parse(readFileSync(join(DIR, name), "utf8")) as Record<string, unknown>;

// Broker bind_response semantics from world-reply-v4.test.ts, extended
// with request correlation: the test flight source names the reply req,
// so source-correlation binds exactly.
const boundText = (body: Record<string, unknown>, source: string): string =>
    JSON.stringify({ ...body, req: source, source, nativeInvalid: "1", clock: "2", gestureAssoc: null });

function parseBound(name: string, source: string): WorldReply {
    const parsed = parseWorldReply(boundText(doc(name), source));
    assert.ok(isCoreWorldReply(parsed));
    return parsed;
}

function parseBoundDoc(body: Record<string, unknown>, source: string): WorldReply {
    const parsed = parseWorldReply(boundText(body, source));
    assert.ok(isCoreWorldReply(parsed));
    return parsed;
}

interface FakeCalls {
    geometry: string[];
    membership: string[];
    view: string[];
    focus: string[];
}

function makeObservation(opts: {
    rect: { x: number; y: number; w: number; h: number };
    floating: boolean;
    homeWs: string;
    viewNative: string;
    topologyCurrent: string;
    fitExcluded?: boolean;
    fullscreen?: boolean;
    maximized?: boolean;
    sticky?: boolean;
}): WorldObservation {
    const win = {
        win: "win-a", ref: { id: "win-a" }, out: "out-a", ws: opts.homeWs,
        rect: { ...opts.rect },
        floating: opts.floating, fitExcluded: opts.fitExcluded ?? false,
        fullscreen: opts.fullscreen ?? false,
        maximized: opts.maximized ?? false, sticky: opts.sticky ?? false,
        fixedAuto: false, fixedSuppress: false,
        hints: { minW: null, minH: null, maxW: null, maxH: null },
        minimized: false, nativeWs: null,
    };
    return {
        mode: "per-output-local",
        topology: [{
            out: "out-a", order: [opts.topologyCurrent], current: opts.topologyCurrent,
            complete: true, rect: { x: 0, y: 0, w: 816, h: 600 }, primary: true,
        }],
        bindings: [],
        domains: [{
            id: "out-a", ws: opts.homeWs,
            bounds: { x: 0, y: 0, w: 816, h: 600 },
            gap: 8, outer: 4, focused: "win-a", readable: true, hold: "",
            windows: [win], closed: [], complete: true,
        }],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
        foregroundKnown: false,
        screens: [], windowFacts: [],
        floatIntent: [], tileIntent: [], views: [], closed: [],
        hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: {
            foreground: "", minimized: false, maximized: false, fullscreen: false,
            cloaked: false, hiddenWorkspace: false, shell: false, frame: null,
            borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0,
            underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false,
            pagerOrder: [], pagerFrames: [],
        },
    };
}

function makeEnv(
    state: WorldAdapterState,
    live: { rect: { x: number; y: number; w: number; h: number }; homeWs: string; viewNative: string },
    calls: FakeCalls,
) {
    return {
        state,
        log: () => undefined,
        isStillListed: () => true,
        readLiveRect: () => ({ ...live.rect }),
        readLiveMembership: () => ({ out: "out-a", ws: live.homeWs }),
        readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
        setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => {
            calls.geometry.push(`${(ref as { id: string }).id}@${rect.x},${rect.y},${rect.w},${rect.h}`);
            live.rect = { ...rect };
            return true;
        },
        setMembership: (ref: object) => {
            calls.membership.push((ref as { id: string }).id);
            return true;
        },
        sendToOutput: (ref: object) => {
            calls.membership.push((ref as { id: string }).id);
            return true;
        },
        resolveOutput: (out: string) => ({ name: out }),
        resolveDesktop: (ws: string) => ({ id: ws }),
        switchView: () => {
            calls.view.push("switch");
            return true;
        },
        createDesktop: () => undefined,
        removeDesktop: () => false,
        focusWindow: (ref: object) => {
            calls.focus.push((ref as { id: string }).id);
            return true;
        },
        desktopOccupants: () => ({ total: 0, readable: true }),
        desktopVisible: () => false,
        readLiveView: (out: string) => (out === "out-a" ? live.viewNative : null),
        desktopById: (nativeId: string) => ({ id: nativeId }),
        minDesktops: 2,
    };
}

function freshBound(): { state: WorldAdapterState; flightWatermark: string } {
    const state = new WorldAdapterState(ADAPTER, "owner-a");
    assert.equal(state.onDiscovery(GEN, ADAPTER, "disc-1", "disc-1"), true);
    const flight = state.beginCapture("src-a", null);
    assert.ok(flight !== null);
    return { state, flightWatermark: flight.watermark };
}

function zeroWrites(
    result: { geometryWrites: number; membershipWrites: number; viewWrites: number; creations: number; removals: number; effects: number; clears: number },
): void {
    assert.equal(result.geometryWrites, 0);
    assert.equal(result.membershipWrites, 0);
    assert.equal(result.viewWrites, 0);
    assert.equal(result.creations, 0);
    assert.equal(result.removals, 0);
    assert.equal(result.effects, 0);
    assert.equal(result.clears, 0);
}

describe("world v4 producer binding and ordinary apply", () => {
    it("accepts the real observe reply and converges tiling with no native writes", () => {
        const { state, flightWatermark } = freshBound();
        const reply = parseBound("reply-observe.json", "src-a");
        assert.deepEqual(state.acceptReply(reply, "src-a", flightWatermark), { applied: true, reason: "accepted" });
        assert.equal(state.lastAcceptedRev, "1");
        const live = { rect: { x: 8, y: 8, w: 800, h: 600 }, homeWs: "ws-a", viewNative: "native-1" };
        const calls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
        const result = applyWorldReply({
            ...makeEnv(state, live, calls),
            observation: makeObservation({ rect: live.rect, floating: false, homeWs: "ws-a", viewNative: "native-1", topologyCurrent: "native-1" }),
            reply,
            dispatchWatermark: flightWatermark,
            takeResult: null,
        });
        zeroWrites(result);
        assert.equal(result.stoppedMidApply, false);
        assert.deepEqual(calls, { geometry: [], membership: [], view: [], focus: [] });
        assert.equal(state.framesForReport().length, 0);
    });

    it("applies one ordinary frame write with proven readback when diverged", () => {
        const { state, flightWatermark } = freshBound();
        const reply = parseBound("reply-observe.json", "src-a");
        assert.deepEqual(state.acceptReply(reply, "src-a", flightWatermark), { applied: true, reason: "accepted" });
        const live = { rect: { x: 0, y: 0, w: 100, h: 100 }, homeWs: "ws-a", viewNative: "native-1" };
        const calls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
        const result = applyWorldReply({
            ...makeEnv(state, live, calls),
            observation: makeObservation({ rect: live.rect, floating: false, homeWs: "ws-a", viewNative: "native-1", topologyCurrent: "native-1" }),
            reply,
            dispatchWatermark: flightWatermark,
            takeResult: null,
        });
        assert.equal(result.geometryWrites, 1);
        assert.deepEqual(calls.geometry, ["win-a@8,8,800,600"]);
        assert.deepEqual(calls.membership, []);
        assert.deepEqual(calls.view, []);
        const attempts = state.framesForReport().filter((a) => a.win === "win-a");
        assert.equal(attempts.length, 1);
        assert.deepEqual(attempts[0]?.target, [8, 8, 800, 600]);
        assert.equal(attempts[0]?.setter, "accepted");
        assert.deepEqual(attempts[0]?.readback, [8, 8, 800, 600]);
    });

    it("local inferred floating/fitExcluded carrier still writes when the accepted row is tiled", () => {
        const { state, flightWatermark } = freshBound();
        const reply = parseBound("reply-observe.json", "src-a");
        const row = reply.windows.find((w) => w.win === "win-a");
        assert.ok(row !== undefined && row.floating === false);
        assert.equal(row.frame_op, "");
        assert.deepEqual(state.acceptReply(reply, "src-a", flightWatermark), { applied: true, reason: "accepted" });
        const live = { rect: { x: 0, y: 0, w: 100, h: 100 }, homeWs: "ws-a", viewNative: "native-1" };
        const calls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
        const result = applyWorldReply({
            ...makeEnv(state, live, calls),
            observation: makeObservation({
                rect: live.rect, floating: true, fitExcluded: true,
                homeWs: "ws-a", viewNative: "native-1", topologyCurrent: "native-1",
            }),
            reply,
            dispatchWatermark: flightWatermark,
            takeResult: null,
        });
        assert.equal(result.geometryWrites, 1);
        assert.deepEqual(calls.geometry, ["win-a@8,8,800,600"]);
        const attempts = state.framesForReport().filter((a) => a.win === "win-a");
        assert.equal(attempts.length, 1);
        assert.deepEqual(attempts[0]?.target, [8, 8, 800, 600]);
        assert.equal(attempts[0]?.setter, "accepted");
        assert.deepEqual(attempts[0]?.readback, [8, 8, 800, 600]);
    });

    it("same local carrier with accepted floating row takes no ordinary frame write", () => {
        const { state, flightWatermark } = freshBound();
        const body = doc("reply-observe.json");
        const windows = (body.windows as Array<Record<string, unknown>>).map((w) => ({ ...w, floating: true }));
        const floatReply = parseBoundDoc({ ...body, windows }, "src-a");
        const row = floatReply.windows.find((w) => w.win === "win-a");
        assert.ok(row !== undefined && row.floating === true);
        assert.equal(row.frame_op, "");
        assert.deepEqual(state.acceptReply(floatReply, "src-a", flightWatermark), { applied: true, reason: "accepted" });
        const live = { rect: { x: 0, y: 0, w: 100, h: 100 }, homeWs: "ws-a", viewNative: "native-1" };
        const calls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
        const result = applyWorldReply({
            ...makeEnv(state, live, calls),
            observation: makeObservation({
                rect: live.rect, floating: true, fitExcluded: true,
                homeWs: "ws-a", viewNative: "native-1", topologyCurrent: "native-1",
            }),
            reply: floatReply,
            dispatchWatermark: flightWatermark,
            takeResult: null,
        });
        assert.equal(result.geometryWrites, 0);
        assert.ok(result.held >= 1);
        assert.deepEqual(calls.geometry, []);
        assert.equal(state.framesForReport().filter((a) => a.win === "win-a").length, 0);
    });

    it("raw fullscreen/maximized/sticky still veto the ordinary frame write", () => {
        for (const flag of ["fullscreen", "maximized", "sticky"] as const) {
            const { state, flightWatermark } = freshBound();
            const reply = parseBound("reply-observe.json", "src-a");
            assert.deepEqual(state.acceptReply(reply, "src-a", flightWatermark), { applied: true, reason: "accepted" });
            const live = { rect: { x: 0, y: 0, w: 100, h: 100 }, homeWs: "ws-a", viewNative: "native-1" };
            const calls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
            const result = applyWorldReply({
                ...makeEnv(state, live, calls),
                observation: makeObservation({
                    rect: live.rect, floating: false, homeWs: "ws-a",
                    viewNative: "native-1", topologyCurrent: "native-1", [flag]: true,
                }),
                reply,
                dispatchWatermark: flightWatermark,
                takeResult: null,
            });
            assert.equal(result.geometryWrites, 0, flag);
            assert.ok(result.held >= 1, flag);
            assert.deepEqual(calls.geometry, [], flag);
            assert.equal(state.framesForReport().filter((a) => a.win === "win-a").length, 0, flag);
        }
    });

    it("wrong generation, stale revision, and unknown window apply no native effects", () => {
        const { state, flightWatermark } = freshBound();
        const good = parseBound("reply-observe.json", "src-a");
        assert.deepEqual(state.acceptReply(good, "src-a", flightWatermark), { applied: true, reason: "accepted" });
        // Wrong generation: binds nothing, applies nothing.
        const foreignBody = { ...doc("reply-observe.json"), gen: "generation-b" };
        const flight2 = state.beginCapture("src-b", null);
        assert.ok(flight2 !== null);
        const foreign = parseBoundDoc(foreignBody, "src-b");
        assert.deepEqual(state.acceptReply(foreign, "src-b", flight2.watermark), { applied: false, reason: "generation-mismatch" });
        const liveWrong = { rect: { x: 0, y: 0, w: 100, h: 100 }, homeWs: "ws-a", viewNative: "native-1" };
        const wrongCalls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
        const wrongResult = applyWorldReply({
            ...makeEnv(state, liveWrong, wrongCalls),
            observation: makeObservation({ rect: liveWrong.rect, floating: false, homeWs: "ws-a", viewNative: "native-1", topologyCurrent: "native-1" }),
            reply: foreign,
            dispatchWatermark: flight2.watermark,
            takeResult: null,
        });
        assert.equal(wrongResult.stoppedMidApply, true);
        zeroWrites(wrongResult);
        assert.deepEqual(wrongCalls.geometry, []);
        // Stale revision against the accepted rev: no setters, no ack.
        const staleBody = { ...doc("reply-observe.json"), rev: "0" };
        const stale = parseBoundDoc(staleBody, "src-b");
        assert.deepEqual(state.acceptReply(stale, "src-b", flight2.watermark), { applied: false, reason: "stale-rev" });
        const liveStale = { rect: { x: 0, y: 0, w: 100, h: 100 }, homeWs: "ws-a", viewNative: "native-1" };
        const staleCalls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
        const staleResult = applyWorldReply({
            ...makeEnv(state, liveStale, staleCalls),
            observation: makeObservation({ rect: liveStale.rect, floating: false, homeWs: "ws-a", viewNative: "native-1", topologyCurrent: "native-1" }),
            reply: stale,
            dispatchWatermark: flight2.watermark,
            takeResult: null,
        });
        assert.equal(staleResult.stoppedMidApply, true);
        zeroWrites(staleResult);
        assert.deepEqual(staleCalls.geometry, []);
        // Unknown window holds with no invented writes.
        const observeDoc = doc("reply-observe.json");
        const observeWindows = observeDoc.windows as Array<Record<string, unknown>>;
        const firstWindow = observeWindows[0];
        assert.ok(firstWindow !== undefined);
        const ghostBody = {
            ...observeDoc,
            rev: "2",
            windows: [{ ...firstWindow, win: "win-ghost" }],
        };
        const ghost = parseBoundDoc(ghostBody, "src-b");
        assert.deepEqual(state.acceptReply(ghost, "src-b", flight2.watermark), { applied: true, reason: "accepted" });
        const liveGhost = { rect: { x: 8, y: 8, w: 800, h: 600 }, homeWs: "ws-a", viewNative: "native-1" };
        const ghostCalls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
        const ghostResult = applyWorldReply({
            ...makeEnv(state, liveGhost, ghostCalls),
            observation: makeObservation({ rect: liveGhost.rect, floating: false, homeWs: "ws-a", viewNative: "native-1", topologyCurrent: "native-1" }),
            reply: ghost,
            dispatchWatermark: flight2.watermark,
            takeResult: null,
        });
        zeroWrites(ghostResult);
        assert.equal(ghostResult.stoppedMidApply, false);
        assert.ok(ghostResult.held >= 1);
        assert.deepEqual(ghostCalls.geometry, []);
        assert.deepEqual(ghostCalls.membership, []);
    });

    it("floating workspace row with empty origin converges with no frame write", () => {
        const { state, flightWatermark } = freshBound();
        const reply = parseBound("reply-workspace-floating.json", "src-a");
        const row = reply.windows.find((w) => w.win === "win-a");
        assert.ok(row !== undefined && row.floating === true && row.float_origin === "");
        assert.equal(row.frame, null);
        assert.equal(row.frame_op, "");
        assert.deepEqual(state.acceptReply(reply, "src-a", flightWatermark), { applied: true, reason: "accepted" });
        const live = { rect: { x: 100, y: 100, w: 300, h: 200 }, homeWs: "ws-a", viewNative: "native-1" };
        const calls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
        const result = applyWorldReply({
            ...makeEnv(state, live, calls),
            observation: makeObservation({ rect: live.rect, floating: true, homeWs: "ws-a", viewNative: "native-1", topologyCurrent: "native-1" }),
            reply,
            dispatchWatermark: flightWatermark,
            takeResult: null,
        });
        // No tiled inference from the missing allocation and no float
        // placement without an explicit token: the float simply stands.
        zeroWrites(result);
        assert.equal(result.stoppedMidApply, false);
        assert.equal(result.held, 0);
        assert.deepEqual(calls.geometry, []);
        assert.equal(state.framesForReport().length, 0);
    });

    it("controlled workspace scope authorizes the converged migration with no writes", () => {
        const { state, flightWatermark } = freshBound();
        const base = doc("reply-transfer-migration.json");
        const workspaces = (base.workspaces as Array<Record<string, unknown>>).map((w) =>
            w["id"] === "ws-a" ? { ...w, bind: "native-1" } : w,
        );
        const scoped = {
            ...base,
            workspaces,
            scopes: [
                ...(base.scopes as Array<unknown>),
                { scope: { kind: "workspace", out: "out-a", ws: "ws-a" }, auth: "controlled", reason: "" },
            ],
        };
        const reply = parseBoundDoc(scoped, "src-a");
        assert.deepEqual(state.acceptReply(reply, "src-a", flightWatermark), { applied: true, reason: "accepted" });
        const live = { rect: { x: 8, y: 8, w: 800, h: 600 }, homeWs: "native-1", viewNative: "native-1" };
        const calls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
        const result = applyWorldReply({
            ...makeEnv(state, live, calls),
            observation: makeObservation({ rect: live.rect, floating: false, homeWs: "native-1", viewNative: "native-1", topologyCurrent: "native-1" }),
            reply,
            dispatchWatermark: flightWatermark,
            takeResult: null,
        });
        zeroWrites(result);
        assert.equal(result.held, 0);
        assert.equal(result.stoppedMidApply, false);
        assert.deepEqual(calls, { geometry: [], membership: [], view: [], focus: [] });
        assert.deepEqual(state.appliedWindowTarget.get("win-a"), { op: "transfer-a", out: "out-a", ws: "native-1" });
        assert.equal(state.isDispatched("transfer-a"), false);
    });

    it("missing workspace scope holds the migration with no write and no retirement", () => {
        const { state, flightWatermark } = freshBound();
        // As shipped: output-kind scope only, no workspace-kind authority
        // for the target cell.
        const reply = parseBound("reply-transfer-migration.json", "src-a");
        assert.ok(!reply.scopes.some((s) => s.scope.kind === "workspace"));
        assert.deepEqual(state.acceptReply(reply, "src-a", flightWatermark), { applied: true, reason: "accepted" });
        const live = { rect: { x: 8, y: 8, w: 800, h: 600 }, homeWs: "native-1", viewNative: "native-1" };
        const calls: FakeCalls = { geometry: [], membership: [], view: [], focus: [] };
        const result = applyWorldReply({
            ...makeEnv(state, live, calls),
            observation: makeObservation({ rect: live.rect, floating: false, homeWs: "native-1", viewNative: "native-1", topologyCurrent: "native-1" }),
            reply,
            dispatchWatermark: flightWatermark,
            takeResult: null,
        });
        zeroWrites(result);
        assert.ok(result.held >= 1);
        assert.deepEqual(calls, { geometry: [], membership: [], view: [], focus: [] });
        assert.equal(state.appliedWindowTarget.has("win-a"), false);
        assert.equal(state.isDispatched("transfer-a"), false);
    });

    it("every actual handshake resets, even on repeat gen; wrong adapter or req binds nothing", () => {
        const state = new WorldAdapterState(ADAPTER, "owner-a");
        assert.equal(state.onDiscovery(GEN, ADAPTER, "disc-1", "disc-1"), true);
        state.markDispatched("op-x");
        assert.equal(state.mintClientCmd(), "1");
        // A fresh handshake on the same generation still rotates: the core
        // resets on every handshake, so the adapter follows.
        assert.equal(state.onDiscovery(GEN, ADAPTER, "disc-2", "disc-2"), true);
        assert.equal(state.isDispatched("op-x"), false);
        assert.equal(state.inFlight, null);
        assert.equal(state.mintClientCmd(), "1");
        // A delayed arbitrary reply for another adapter binds nothing.
        assert.equal(state.onDiscovery("generation-b", "adapter-evil", "disc-3", "disc-3"), false);
        assert.equal(state.boundGen, GEN);
        // ...nor does a wrong req for this adapter.
        assert.equal(state.onDiscovery("generation-b", ADAPTER, "disc-9", "disc-3"), false);
        assert.equal(state.boundGen, GEN);
    });

    it("refuses replies naming another producer adapter", () => {
        const { state, flightWatermark } = freshBound();
        const evilBody = { ...doc("reply-observe.json"), adapter: "adapter-evil" };
        const evil = parseBoundDoc(evilBody, "src-a");
        assert.deepEqual(state.acceptReply(evil, "src-a", flightWatermark), { applied: false, reason: "adapter-mismatch" });
        assert.equal(state.hasAccepted, false);
    });
});
