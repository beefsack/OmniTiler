// Logical workspace resolution in actuation (D2): reply transfer, view,
// and migration rows name the stable core LOGICAL workspace id while
// native setters, live membership, and live views speak NATIVE desktop
// ids. The adapter resolves through the accepted reply's own workspace
// rows for the exact output, never by ordinal or native string equality.
// Unbound or unknown targets hold with no writes. Native ids here are
// UUID-shaped and never equal the logical tokens by construction. Strict
// v4 synthetic replies throughout (hand-authored inline, validated through
// the real parseWorldReply); logical-resolution scenario only, not the
// genuine migration real-core capture.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply, type ApplyEnv } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { parseWorldReply, isCoreWorldReply } from "../src/world-report";

const NATIVE_A = "550e8400-e29b-41d4-a716-446655440001";
const NATIVE_B = "550e8400-e29b-41d4-a716-446655440002";

function directEnv(
    overrides: Partial<ApplyEnv> & { state: WorldAdapterState; observation: ApplyEnv["observation"]; reply: ApplyEnv["reply"] },
): ApplyEnv {
    return {
        log: () => undefined,
        isStillListed: () => true,
        isActiveWindow: () => false,
        readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
        readLiveMembership: () => ({ out: "out-1", ws: NATIVE_B }),
        readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
        dispatchWatermark: overrides.state.currentInvalid,
        takeResult: null,
        setGeometry: () => true,
        setMembership: () => { throw new Error("unexpected membership write"); },
        sendToOutput: () => { throw new Error("unexpected output write"); },
        resolveOutput: () => null,
        resolveDesktop: () => null,
        switchView: () => { throw new Error("unexpected view write"); },
        createDesktop: () => null,
        removeDesktop: () => false,
        focusWindow: () => { throw new Error("unexpected focus write"); },
        desktopOccupants: () => ({ total: 0, readable: true }),
        desktopVisible: () => false,
        readLiveView: () => null,
        desktopById: () => null,
        minDesktops: 2,
        ...overrides,
    };
}

function acceptEnv(
    replyJson: Record<string, unknown>,
    observation: ApplyEnv["observation"],
    envOverrides: Partial<ApplyEnv> = {},
): { result: ReturnType<typeof applyWorldReply>; state: WorldAdapterState } {
    // Parse first, then bind the actual synthetic receipt: producer
    // adapter, serving generation, source correlation, and the capture
    // watermark, mirroring the typed v4 apply harness.
    const parsed = parseWorldReply(JSON.stringify(replyJson));
    assert.ok(parsed !== null && isCoreWorldReply(parsed));
    if (!isCoreWorldReply(parsed)) {
        throw new Error("fixture must parse as a core reply");
    }
    const state = new WorldAdapterState(parsed.adapter, "owner-a");
    assert.equal(state.onDiscovery(parsed.gen, parsed.adapter, "disc-1", "disc-1"), true);
    const flight = state.beginCapture(parsed.source, null);
    assert.ok(flight !== null);
    const verdict = state.acceptReply(parsed, parsed.source, flight.watermark);
    assert.deepEqual(verdict, { applied: true, reason: "accepted" });
    const env = directEnv({ state, observation, reply: parsed, ...envOverrides });
    return { result: applyWorldReply(env), state };
}

function observationFor(homeWs: string, ref: object): ApplyEnv["observation"] {
    return {
        mode: "per-output-local",
        topology: [{ out: "out-1", order: [NATIVE_A, NATIVE_B], current: homeWs, complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
        bindings: [],
        domains: [{
            id: "out-1", ws: homeWs, bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 0, focused: "", readable: true, hold: "",
            adjacent: {}, complete: true,
            windows: [{
                win: "win-a", ref, out: "out-1", ws: homeWs,
                rect: { x: 0, y: 0, w: 100, h: 100 },
                floating: false, fitExcluded: false, fullscreen: false,
                maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false, nativeWs: homeWs,
                hints: { minW: null, minH: null, maxW: null, maxH: null },
                minimized: false,
            }],
            closed: [],
        }],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
        screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

const PRESENTATION = {
    foreground: "", rev: "1",
    border: { visible: false, reason: "disabled", subject: null, outsets: null, width: null, radius: null, witness: null },
    underlay: { status: "hidden", reason: "disabled", baseRev: null },
    pager: [{
        out: "out-1",
        workspaces: [{ id: "wl-1", ordinal: 1, label: "1", current: true, occupied: true, containsFocused: false, windows: [] }],
    }],
};

function coreReply(workspaces: Array<Record<string, unknown>>, transfers: Array<Record<string, unknown>>): Record<string, unknown> {
    return {
        v: 4, adapter: "broker-1", gen: "gen-1", req: "req-1",
        obs: "1", rev: "1", ready: true, consumed: false,
        source: "req-1", nativeInvalid: "1", clock: "0", gestureAssoc: null,
        gestureSettled: null, operationSettled: [], nativeCreations: [], warpCursor: null,
        presentation: PRESENTATION,
        scopes: [{ scope: { kind: "output", out: "out-1" }, auth: "controlled", reason: "" }],
        workspaces,
        outputs: [{ out: "out-1", view: "wl-1", mode: "tiled", hold: "" }],
        windows: [{
            win: "win-a", out: "out-1", ws: "wl-1",
            alloc: null, frame: null,
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "",
            frame_op: "", floating: false, float_origin: "",
            clear_op: "", cmdSeq: null,
            visibility: null, visibility_auth: "untouched", visibility_hold: "",
            state: [],
        }],
        transfers: transfers.map((t) => ({ ...t, cmdSeq: null })),
        effects: [],
        result: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
    };
}

const WORKSPACES = [
    { id: "wl-1", out: "out-1", bind: NATIVE_A, scope: { kind: "output", out: "out-1" }, order: 0, mode: "tiled", exist: true, hold: "" },
    { id: "wl-2", out: "out-1", bind: NATIVE_B, scope: { kind: "output", out: "out-1" }, order: 1, mode: "tiled", exist: true, hold: "" },
];

describe("world logical workspace resolution in actuation", () => {
    it("moves membership through the bound native for a differing logical target", () => {
        const ref = {};
        const refA = { native: NATIVE_A };
        const refB = { native: NATIVE_B };
        let home = { out: "out-1", ws: NATIVE_B };
        const desktopCalls: string[] = [];
        let memberArgs: Array<unknown> = [];
        const { result } = acceptEnv(
            coreReply(WORKSPACES, [
                { op: "op-1", kind: "window", win: "win-a", out: "out-1", ws: "wl-1", follow: false, held: false },
            ]),
            observationFor(NATIVE_B, ref),
            {
                readLiveMembership: () => ({ ...home }),
                desktopById: (id: string) => {
                    desktopCalls.push(id);
                    if (id === NATIVE_A) {
                        return refA;
                    }
                    if (id === NATIVE_B) {
                        return refB;
                    }
                    return null;
                },
                resolveOutput: () => ({ out: "out-1" }),
                setMembership: (gotRef: object, wsNativeId: string, wsRef: object | null) => {
                    memberArgs = [gotRef, wsNativeId, wsRef];
                    home = { out: "out-1", ws: wsNativeId };
                    return true;
                },
            },
        );
        assert.equal(result.membershipWrites, 1, "logical target actuates membership");
        assert.deepEqual(memberArgs, [ref, NATIVE_A, refA], "setter names the bound native, never the logical token");
        assert.ok(!desktopCalls.includes("wl-1"), "no native-equality fallback lookup for the logical id");
        assert.ok(result.held === 0, `nothing held, got held=${result.held}`);
    });

    it("switches a view through the bound native for a differing logical target", () => {
        const ref = {};
        const refB = { native: NATIVE_B };
        let liveView: string | null = NATIVE_A;
        let switched: Array<unknown> = [];
        const { result } = acceptEnv(
            coreReply(WORKSPACES, [
                { op: "op-view-1", kind: "view", win: "", out: "out-1", ws: "wl-2", follow: false, held: false },
            ]),
            observationFor(NATIVE_B, ref),
            {
                readLiveView: () => liveView,
                desktopById: (id: string) => (id === NATIVE_B ? refB : null),
                resolveOutput: () => ({ out: "out-1" }),
                switchView: (wsRef: object | null, outRef: object | null) => {
                    switched = [wsRef, outRef];
                    liveView = NATIVE_B;
                    return true;
                },
            },
        );
        assert.equal(result.viewWrites, 1, "logical view target actuates");
        assert.deepEqual(switched[0], refB, "view switch names the bound native desktop");
    });

    it("holds an unknown logical target with no writes", () => {
        const ref = {};
        const desktopCalls: string[] = [];
        const { result } = acceptEnv(
            coreReply(WORKSPACES, [
                { op: "op-9", kind: "window", win: "win-a", out: "out-1", ws: "wl-9", follow: false, held: false },
            ]),
            observationFor(NATIVE_B, ref),
            {
                desktopById: (id: string) => {
                    desktopCalls.push(id);
                    return null;
                },
            },
        );
        assert.equal(result.membershipWrites, 0, "unknown logical target writes nothing");
        assert.ok(result.held > 0, "unknown logical target holds");
        assert.ok(!desktopCalls.includes("wl-9"), "unknown logical never reaches the native lookup");
    });

    it("holds an unbound logical target with no writes", () => {
        const ref = {};
        const withUnbound = [
            ...WORKSPACES,
            { id: "wl-3", out: "out-1", bind: "", scope: { kind: "output", out: "out-1" }, order: 2, mode: "tiled", exist: true, hold: "" },
        ];
        const { result } = acceptEnv(
            coreReply(withUnbound, [
                { op: "op-9", kind: "window", win: "win-a", out: "out-1", ws: "wl-3", follow: false, held: false },
            ]),
            observationFor(NATIVE_B, ref),
            {
                desktopById: () => { throw new Error("unbound logical must never reach the native lookup"); },
            },
        );
        assert.equal(result.membershipWrites, 0, "unbound logical target writes nothing");
        assert.ok(result.held > 0, "unbound logical target holds");
    });
});
