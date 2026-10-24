// Cross-output drag-drop arrival through the production apply path:
// the core pair transaction retains a window transfer while the snapshot
// projects the mover at the pointer target. The adapter moves membership
// first with no native frame until membership proof arrives (the target
// frame belongs to the destination output), then converges geometry on
// the reports after arrival. No repeats, no foreign frames pre-move.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import type { WorldReply } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";

function arrivalReply(): WorldReply {
    return {
        adapter: "adapter-1", gen: "gen-1", req: "req-9", obs: "9", rev: "9",
        ready: true, consumed: false,
        source: "req-1",
        nativeInvalid: "1",
        clock: "0",
        gestureSettled: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [
            { scope: "out-a/wa1", auth: "controlled", reason: "" },
            { scope: "out-b/wb1", auth: "controlled", reason: "" },
        ],
        // Bound rows the broker always ships alongside transfers (fused
        // here, matching the fake natives): adapter-side resolution never
        // falls back to native string equality.
        workspaces: [
            { id: "wa1", out: "out-a", bind: "wa1", scope: "out-a", order: 0, mode: "tiled", exist: true, hold: "" },
            { id: "wb1", out: "out-b", bind: "wb1", scope: "out-b", order: 0, mode: "tiled", exist: true, hold: "" },
        ],
        outputs: [],
        windows: [
            {
                win: "win-a", out: "out-b", ws: "wb1",
                alloc: [810, 10, 140, 80], frame: [810, 10, 140, 80],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
                clear_op: "",
                floating: false, float_origin: "",
            },
            {
                win: "win-b", out: "out-b", ws: "wb1",
                alloc: [950, 10, 140, 80], frame: [950, 10, 140, 80],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
                clear_op: "",
                floating: false, float_origin: "",
            },
        ],
        transfers: [
            { op: "op-87", kind: "window", win: "win-a", out: "out-b", ws: "wb1", follow: false, held: false },
        ],
        effects: [],
        result: null,
    };
}

interface FakeWorld {
    homing: Map<string, { out: string; ws: string }>;
    frames: Map<string, { x: number; y: number; w: number; h: number }>;
    currents: Map<string, string>;
    sets: string[];
    membershipCalls: string[];
    outputCalls: string[];
}

function makeObservation(world: FakeWorld): WorldObservation {
    const win = (id: string) => {
        const home = world.homing.get(id) ?? { out: "out-a", ws: "wa1" };
        const rect = world.frames.get(id) ?? { x: 0, y: 0, w: 100, h: 100 };
        return {
            win: id, ref: { id }, out: home.out, ws: home.ws,
            rect: { ...rect },
            floating: false, fitExcluded: false, fullscreen: false,
            maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false, nativeWs: null,
            hints: { minW: null, minH: null, maxW: null, maxH: null },
            minimized: false,
        };
    };
    return {
        mode: "per-output-local",
        topology: [
            { out: "out-a", order: ["wa1"], current: world.currents.get("out-a") ?? "wa1", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true },
            { out: "out-b", order: ["wb1"], current: world.currents.get("out-b") ?? "wb1", complete: true, rect: { x: 800, y: 0, w: 800, h: 600 }, primary: false },
        ],
        bindings: [],
        domains: [
            {
                id: "out-a", ws: "wa1",
                bounds: { x: 0, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
                focused: "win-a", readable: true, hold: "", adjacent: { right: "out-b" },
                windows: [win("win-a")], closed: [], complete: true,
            },
            {
                id: "out-b", ws: "wb1",
                bounds: { x: 800, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
                focused: "win-b", readable: true, hold: "", adjacent: { left: "out-a" },
                windows: [win("win-b")], closed: [], complete: true,
            },
        ],
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

function makeEnv(state: WorldAdapterState, world: FakeWorld) {
    return {
        state,
        log: () => undefined,
        isStillListed: () => true,
        isActiveWindow: () => false,
        readLiveRect: (ref: object) => {
            const frame = world.frames.get((ref as { id: string }).id);
            return frame === undefined ? null : { ...frame };
        },
        readLiveMembership: (ref: object) => world.homing.get((ref as { id: string }).id) ?? null,
        readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
        setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => {
            world.sets.push(`${(ref as { id: string }).id}@${rect.x},${rect.y},${rect.w},${rect.h}`);
            world.frames.set((ref as { id: string }).id, { ...rect });
            return true;
        },
        setMembership: (ref: object, wsNativeId: string) => {
            const id = (ref as { id: string }).id;
            world.membershipCalls.push(`${id}->${wsNativeId}`);
            world.homing.set(id, { out: world.homing.get(id)?.out ?? "", ws: wsNativeId });
            return true;
        },
        sendToOutput: (ref: object, outRef: object | null) => {
            if (outRef === null) {
                return false;
            }
            const id = (ref as { id: string }).id;
            world.outputCalls.push(`${id}->${(outRef as { name: string }).name}`);
            world.homing.set(id, {
                out: (outRef as { name: string }).name,
                ws: world.homing.get(id)?.ws ?? "",
            });
            return true;
        },
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

function freshState(reply: WorldReply): WorldAdapterState {
    const state = new WorldAdapterState("adapter-1", "owner-a");
    state.onDiscovery("gen-1", "broker-1");
    while (state.currentInvalid !== reply.obs) {
        state.invalidate();
    }
    return state;
}

describe("world drag arrival", () => {
    it("moves membership first with no foreign frame, then converges geometry", () => {
        const reply = arrivalReply();
        const world: FakeWorld = {
            homing: new Map([
                ["win-a", { out: "out-a", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            frames: new Map([
                ["win-a", { x: 0, y: 0, w: 100, h: 100 }],
                ["win-b", { x: 950, y: 10, w: 140, h: 80 }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sets: [], membershipCalls: [], outputCalls: [],
        };
        const state = freshState(reply);
        const arrived = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        // Membership actuates; the destination frame does not land on the
        // source output.
        assert.ok(world.outputCalls.some((c) => c === "win-a->out-b"), "output moves");
        assert.ok(world.membershipCalls.some((c) => c === "win-a->wb1"), "desktop sets");
        assert.ok(!world.sets.some((c) => c.startsWith("win-a@")), "no frame until membership proof");
        assert.deepEqual(world.homing.get("win-a"), { out: "out-b", ws: "wb1" });
        assert.ok(arrived.membershipWrites >= 1);
        // After arrival proof, geometry converges with no new membership.
        const outCalls = world.outputCalls.length;
        const memberCalls = world.membershipCalls.length;
        const converged = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(world.sets.some((c) => c === "win-a@810,10,140,80"), "target frame converges post-arrival");
        assert.ok(converged.geometryWrites >= 1);
        assert.equal(world.outputCalls.length, outCalls, "no duplicate output move");
        assert.equal(world.membershipCalls.length, memberCalls, "no duplicate desktop set");
    });
});
