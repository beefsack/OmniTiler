// Clamp lifetime for tiled geometry: budget resets follow authoritative
// target convergence only (fresh live read-back or exact post-set
// read-back), never a setter boolean or unrelated success. Alternating
// converged targets (sync or delayed) never starve; an unachieved target
// keeps its own exact target/hints budget across other targets'
// successes; a continuously clamped target still spends its bound.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import type { WorldReply } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";

interface FakeWorld {
    frames: Map<string, { x: number; y: number; w: number; h: number }>;
    sets: string[];
}

function makeWorld(initial: Array<[string, { x: number; y: number; w: number; h: number }]>): FakeWorld {
    return { frames: new Map(initial), sets: [] };
}

function makeObservation(world: FakeWorld): WorldObservation {
    const rect = world.frames.get("win-a") ?? { x: 0, y: 0, w: 100, h: 100 };
    return {
        mode: "per-output-local",
        topology: [{ out: "out", order: ["ws"], current: "ws", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
        bindings: [],
        domains: [{
            id: "out", ws: "ws",
            bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 4, focused: "win-a", readable: true, hold: "",
            adjacent: {},
            windows: [{
                win: "win-a", ref: { id: "win-a" }, out: "out", ws: "ws",
                rect: { ...rect },
                floating: false, fitExcluded: false, fullscreen: false,
                maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false, nativeWs: null,
                hints: { minW: null, minH: null, maxW: null, maxH: null },
                minimized: false,
            }],
            closed: [], complete: true,
        }],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
        screens: [], windowFacts: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

function makeEnv(state: WorldAdapterState, world: FakeWorld, setFilter?: (id: string) => boolean) {
    return {
        state,
        log: () => undefined,
        isStillListed: () => true,
        isActiveWindow: () => false,
        readLiveRect: (ref: object) => {
            const frame = world.frames.get((ref as { id: string }).id);
            return frame === undefined ? null : { ...frame };
        },
        readLiveMembership: () => ({ out: "out", ws: "ws" }),
        readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
        setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => {
            const id = (ref as { id: string }).id;
            if (setFilter !== undefined && !setFilter(id)) {
                return true;
            }
            world.sets.push(`${id}@${rect.x},${rect.y},${rect.w},${rect.h}`);
            world.frames.set(id, { ...rect });
            return true;
        },
        setMembership: () => true,
        sendToOutput: () => true,
        resolveOutput: (out: string) => ({ name: out }),
        resolveDesktop: (ws: string) => ({ id: ws }),
        switchView: () => true,
        createDesktop: () => null,
        removeDesktop: () => false,
        focusWindow: () => true,
        desktopOccupants: () => ({ total: 0, readable: true }),
        desktopVisible: () => false,
        readLiveView: () => null,
        desktopById: (nativeId: string) => ({ id: nativeId }),
        minDesktops: 2,
    };
}

function replyFor(frame: [number, number, number, number]): WorldReply {
    return {
        adapter: "adapter-1", gen: "gen-1", req: "req-1", obs: "1", rev: "1",
        ready: true, consumed: false,
        source: "req-1",
        nativeInvalid: "1",
        clock: "0",
        gestureSettled: null,
        operationSettled: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [], workspaces: [], outputs: [],
        windows: [{
            win: "win-a", out: "out", ws: "ws",
            alloc: frame, frame,
            frame_auth: "controlled", frame_hold: "",
            member_auth: "untouched", member_hold: "",
            frame_op: "", clear_op: "", floating: false, float_origin: "",
        }],
        transfers: [], effects: [], result: null,
    } as unknown as WorldReply;
}

function freshState(): WorldAdapterState {
    const state = new WorldAdapterState("adapter-1", "owner-a");
    state.onDiscovery("gen-1", "broker-1");
    while (state.currentInvalid !== "1") {
        state.invalidate();
    }
    return state;
}

const A: [number, number, number, number] = [10, 10, 300, 300];
const B: [number, number, number, number] = [20, 20, 310, 310];
const sameTarget = (rect: { x: number; y: number; w: number; h: number }, t: readonly [number, number, number, number]): boolean =>
    rect.x === t[0] && rect.y === t[1] && rect.w === t[2] && rect.h === t[3];

describe("world clamp lifetime", () => {
    it("alternating converged targets keep writing past 3 each (sync)", () => {
        const world = makeWorld([["win-a", { x: 0, y: 0, w: 100, h: 100 }]]);
        const state = freshState();
        for (let i = 0; i < 4; i += 1) {
            const outA = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...makeEnv(state, world), observation: makeObservation(world), reply: replyFor(A) });
            assert.equal(outA.geometryWrites, 1, `A leg ${i} still writes`);
            const outB = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...makeEnv(state, world), observation: makeObservation(world), reply: replyFor(B) });
            assert.equal(outB.geometryWrites, 1, `B leg ${i} still writes`);
        }
        assert.equal(world.sets.length, 8, "four alternations write every leg");
    });

    it("alternating converged targets keep writing past 3 each (delayed convergence)", () => {
        const world = makeWorld([["win-a", { x: 0, y: 0, w: 100, h: 100 }]]);
        const state = freshState();
        let pending: { x: number; y: number; w: number; h: number } | null = null;
        const env = {
            ...makeEnv(state, world),
            // Producer-shaped async native: the setter reports true but the
            // live frame only lands before the next observation.
            setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => {
                const id = (ref as { id: string }).id;
                world.sets.push(`${id}@${rect.x},${rect.y},${rect.w},${rect.h}`);
                pending = { ...rect };
                return true;
            },
        };
        const converge = (): void => {
            if (pending !== null) {
                world.frames.set("win-a", { ...pending });
                pending = null;
            }
        };
        for (let i = 0; i < 4; i += 1) {
            const outA = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...env, observation: makeObservation(world), reply: replyFor(A) });
            assert.equal(outA.geometryWrites, 1, `A leg ${i} still writes`);
            converge();
            const outB = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...env, observation: makeObservation(world), reply: replyFor(B) });
            assert.equal(outB.geometryWrites, 1, `B leg ${i} still writes`);
            converge();
        }
        assert.equal(world.sets.length, 8, "delayed alternations write every leg");
    });

    it("a clamped B keeps its 3-attempt budget across A successes", () => {
        const world = makeWorld([["win-a", { x: 0, y: 0, w: 100, h: 100 }]]);
        const state = freshState();
        // Producer-shaped partial clamp: A lands natively, B reports
        // success but never moves the live frame.
        const env = {
            ...makeEnv(state, world),
            setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => {
                const id = (ref as { id: string }).id;
                world.sets.push(`${id}@${rect.x},${rect.y},${rect.w},${rect.h}`);
                if (sameTarget(rect, A)) {
                    world.frames.set(id, { ...rect });
                }
                return true;
            },
        };
        let bWrites = 0;
        for (let i = 0; i < 4; i += 1) {
            const outA = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...env, observation: makeObservation(world), reply: replyFor(A) });
            if (i === 0) {
                assert.equal(outA.geometryWrites, 1, "first A leg moves to A");
            } else {
                assert.equal(outA.geometryWrites, 0, `A leg ${i} already converged`);
            }
            const outB = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...env, observation: makeObservation(world), reply: replyFor(B) });
            bWrites += outB.geometryWrites;
            if (i < 3) {
                assert.equal(outB.geometryWrites, 1, `B attempt ${i} still writes`);
            } else {
                assert.equal(outB.geometryWrites, 0, "B budget spent after 3 attempts despite A successes");
                assert.ok(outB.held >= 1, "spent B holds narrowly");
            }
        }
        assert.equal(bWrites, 3, "B wrote exactly 3 times total");
    });

    it("a never-converging target still spends its bounded budget", () => {
        const world = makeWorld([["win-a", { x: 0, y: 0, w: 100, h: 100 }]]);
        const state = freshState();
        const env = makeEnv(state, world, () => false);
        for (let i = 0; i < 3; i += 1) {
            const out = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...env, observation: makeObservation(world), reply: replyFor(A) });
            assert.equal(out.geometryWrites, 1, `clamped attempt ${i} still writes`);
        }
        const held = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...env, observation: makeObservation(world), reply: replyFor(A) });
        assert.equal(held.geometryWrites, 0, "spent budget writes nothing more");
        assert.ok(held.held >= 1, "spent budget holds narrowly");
    });
});
