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
        gestureSettled: null, gestureAssoc: null,
        focusRequest: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: { foreground: "", rev: "9", suspendedOutput: null, border: { visible: false, reason: "disabled", subject: null, outsets: null, width: null, radius: null, witness: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [
            { scope: { kind: "workspace", out: "out-a", ws: "wa1" }, auth: "controlled", reason: "" },
            { scope: { kind: "workspace", out: "out-b", ws: "wb1" }, auth: "controlled", reason: "" },
        ],
        // Bound rows the broker always ships alongside transfers (fused
        // here, matching the fake natives): adapter-side resolution never
        // falls back to native string equality.
        workspaces: [
            { id: "wa1", out: "out-a", bind: "wa1", scope: { kind: "output", out: "out-a" }, order: 0, mode: "tiled", exist: true, hold: "", trailing: false },
            { id: "wb1", out: "out-b", bind: "wb1", scope: { kind: "output", out: "out-b" }, order: 0, mode: "tiled", exist: true, hold: "", trailing: false },
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
                cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            },
            {
                win: "win-b", out: "out-b", ws: "wb1",
                alloc: [950, 10, 140, 80], frame: [950, 10, 140, 80],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
                clear_op: "",
                floating: false, float_origin: "",
                cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            },
        ],
        transfers: [
            { op: "op-87", kind: "window", win: "win-a", out: "out-b", ws: "wb1", follow: false, held: false, cmdSeq: null },
        ],
        effects: [],
        result: null,
        intentDesired: [],
        switchSettled: [],
        diagnostics: [],
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
                focused: "win-a", readable: true, hold: "",
                windows: [win("win-a")], closed: [], complete: true,
            },
            {
                id: "out-b", ws: "wb1",
                bounds: { x: 800, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
                focused: "win-b", readable: true, hold: "",
                windows: [win("win-b")], closed: [], complete: true,
            },
        ],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
        foregroundKnown: false,
        screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
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
    state.onDiscovery("gen-1", "adapter-1");
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

// Same-output send with distinct logical workspace ids vs native desktop
// bindings (the production hostOwned shape): logical `ws-a`/`ws-b` bind
// `native-1`/`native-9`. The arrival gate must translate the transfer
// target through the reply bindings before comparing against the native
// homing; the raw logical-vs-native comparison never converges while the
// transfer is retained. Setter acceptance alone never proves arrival.
const NATIVE_SOURCE = "native-1";
const NATIVE_TARGET = "native-9";
const LOGICAL_SOURCE = "ws-a";
const LOGICAL_TARGET = "ws-b";
const SEND_OP = "op-87";

function distinctSendReply(): WorldReply {
    // One-output send derived from the cross-output fixture reply: same
    // envelope, presentation, and correlation shape, only the homing rows
    // change to logical `ws-a`/`ws-b` bound to native `native-1`/`native-9`.
    return {
        ...arrivalReply(),
        scopes: [
            { scope: { kind: "workspace", out: "out-a", ws: LOGICAL_SOURCE }, auth: "controlled", reason: "" },
            { scope: { kind: "workspace", out: "out-a", ws: LOGICAL_TARGET }, auth: "controlled", reason: "" },
        ],
        workspaces: [
            { id: LOGICAL_SOURCE, out: "out-a", bind: NATIVE_SOURCE, scope: { kind: "output", out: "out-a" }, order: 0, mode: "tiled", exist: true, hold: "", trailing: false },
            { id: LOGICAL_TARGET, out: "out-a", bind: NATIVE_TARGET, scope: { kind: "output", out: "out-a" }, order: 1, mode: "tiled", exist: true, hold: "", trailing: false },
        ],
        windows: [
            {
                win: "win-a", out: "out-a", ws: LOGICAL_TARGET,
                alloc: [10, 10, 400, 300], frame: [10, 10, 400, 300],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
                clear_op: "",
                floating: false, float_origin: "",
                cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            },
        ],
        transfers: [
            { op: SEND_OP, kind: "window", win: "win-a", out: "out-a", ws: LOGICAL_TARGET, follow: false, held: false, cmdSeq: null },
        ],
    };
}

function distinctWorld(): FakeWorld {
    return {
        homing: new Map([["win-a", { out: "out-a", ws: NATIVE_SOURCE }]]),
        frames: new Map([["win-a", { x: 0, y: 0, w: 100, h: 100 }]]),
        currents: new Map([["out-a", NATIVE_SOURCE]]),
        sets: [], membershipCalls: [], outputCalls: [],
    };
}

function distinctObservation(world: FakeWorld): WorldObservation {
    // HostOwned one-output shape derived from the shared observation:
    // keep the captured window carrier, re-home it to the live native
    // homing, and project the single-output native topology.
    const base = makeObservation(world);
    const home = world.homing.get("win-a") ?? { out: "out-a", ws: NATIVE_SOURCE };
    const template = base.domains[0]?.windows[0];
    assert.ok(template !== undefined);
    const current = world.currents.get("out-a") ?? NATIVE_SOURCE;
    const counts = new Map<string, number>();
    for (const h of world.homing.values()) {
        counts.set(h.ws, (counts.get(h.ws) ?? 0) + 1);
    }
    return {
        ...base,
        topology: [
            { out: home.out, order: [NATIVE_SOURCE, NATIVE_TARGET], current, complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true },
        ],
        domains: [
            {
                id: home.out, ws: home.ws,
                bounds: { x: 0, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
                focused: "win-a", readable: true, hold: "",
                windows: [{ ...template, out: home.out, ws: home.ws }],
                closed: [], complete: true,
            },
        ],
        hostOwned: true,
        native: [{
            out: home.out, order: [NATIVE_SOURCE, NATIVE_TARGET], current,
            occupancy: [NATIVE_SOURCE, NATIVE_TARGET].map((n) => ({ native: n, count: counts.get(n) ?? 0 })),
        }],
    };
}

describe("world drag arrival distinct namespaces", () => {
    it("moves membership with no source frame, then converges the destination frame on translated arrival", () => {
        const reply = distinctSendReply();
        const world = distinctWorld();
        const state = freshState(reply);
        const first = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: distinctObservation(world),
            reply,
        });
        // Membership actuates to the translated native target; the
        // destination frame does not land on the source homing.
        assert.ok(world.membershipCalls.some((c) => c === `win-a->${NATIVE_TARGET}`), "desktop sets to translated native target");
        assert.ok(!world.sets.some((c) => c.startsWith("win-a@")), "no frame until membership proof");
        assert.deepEqual(world.homing.get("win-a"), { out: "out-a", ws: NATIVE_TARGET });
        assert.ok(first.membershipWrites >= 1);
        assert.deepEqual(world.outputCalls, [], "no output move on a same-output send");
        // The retained transfer parent correlates the attempt: logical
        // target on the row, exact native pre/post evidence beside it.
        const attempts = state.memberAttemptsForReport().filter((a) => a.win === "win-a");
        assert.equal(attempts.length, 1);
        assert.equal(attempts[0]?.op, SEND_OP);
        assert.equal(attempts[0]?.out, "out-a");
        assert.equal(attempts[0]?.ws, LOGICAL_TARGET);
        assert.equal(attempts[0]?.setter, "accepted");
        assert.deepEqual(attempts[0]?.pre, { kind: "native", out: "out-a", ws: NATIVE_SOURCE });
        assert.deepEqual(attempts[0]?.post, { kind: "native", out: "out-a", ws: NATIVE_TARGET });
        // Fresh observation at the translated home with the transfer still
        // retained: the destination frame converges with no second
        // membership write.
        const memberCalls = world.membershipCalls.length;
        const second = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: distinctObservation(world),
            reply,
        });
        assert.ok(world.sets.some((c) => c === "win-a@10,10,400,300"), "target frame converges post-arrival");
        assert.ok(second.geometryWrites >= 1);
        assert.equal(world.membershipCalls.length, memberCalls, "no duplicate desktop set");
        assert.deepEqual(world.outputCalls, [], "still no output move");
    });

    it("an accepted setter without native homing change proves no arrival", () => {
        const reply = distinctSendReply();
        const world = distinctWorld();
        const state = freshState(reply);
        const env = {
            ...makeEnv(state, world),
            setMembership: (ref: object) => {
                world.membershipCalls.push(`${(ref as { id: string }).id}->${NATIVE_TARGET}`);
                return true;
            },
        };
        const result = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...env,
            observation: distinctObservation(world),
            reply,
        });
        // The setter claims success but the live homing never moved: the
        // attempt records accepted-with-source-post, never arrival.
        const attempts = state.memberAttemptsForReport().filter((a) => a.win === "win-a");
        assert.equal(attempts.length, 1);
        assert.equal(attempts[0]?.setter, "accepted");
        assert.deepEqual(attempts[0]?.pre, { kind: "native", out: "out-a", ws: NATIVE_SOURCE });
        assert.deepEqual(attempts[0]?.post, { kind: "native", out: "out-a", ws: NATIVE_SOURCE });
        assert.deepEqual(world.homing.get("win-a"), { out: "out-a", ws: NATIVE_SOURCE });
        assert.ok(!world.sets.some((c) => c.startsWith("win-a@")), "no frame without homing proof");
        assert.ok(result.held >= 1);
        assert.equal(state.appliedWindowTarget.has("win-a"), false, "no proven target recorded");
    });

    it("an unbound logical target holds with no write and no attempt", () => {
        const reply = distinctSendReply();
        reply.workspaces = reply.workspaces.map((w) => w.id === LOGICAL_TARGET ? { ...w, bind: "" } : w);
        const world = distinctWorld();
        const state = freshState(reply);
        const result = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: distinctObservation(world),
            reply,
        });
        assert.deepEqual(world.membershipCalls, [], "no membership write to an unbound target");
        assert.ok(!world.sets.some((c) => c.startsWith("win-a@")), "no frame for an unbound target");
        assert.ok(result.held >= 1);
        assert.deepEqual(state.memberAttemptsForReport(), [], "no attempt without a translated target");
        assert.equal(state.isDispatched(SEND_OP), false, "untranslated op never arms");
    });

    it("a wrong native home or output does not release the frame gate", () => {
        const reply = distinctSendReply();
        for (const [name, home] of [
            ["wrong desktop", { out: "out-a", ws: "native-wrong" }],
            ["wrong output", { out: "out-bogus", ws: NATIVE_TARGET }],
        ] as const) {
            const world = distinctWorld();
            world.homing.set("win-a", { ...home });
            const state = freshState(reply);
            state.markDispatched(SEND_OP);
            const result = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
                ...makeEnv(state, world),
                observation: distinctObservation(world),
                reply,
            });
            assert.ok(!world.sets.some((c) => c.startsWith("win-a@")), `${name} releases no frame`);
            assert.deepEqual(world.membershipCalls, [], `${name}: dispatched op never re-actuates`);
            assert.ok(result.held >= 1, `${name} holds`);
        }
    });

    it("an unreadable live homing holds with no write", () => {
        const reply = distinctSendReply();
        const world = distinctWorld();
        const state = freshState(reply);
        const env = {
            ...makeEnv(state, world),
            readLiveMembership: () => null,
        };
        const result = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...env,
            observation: distinctObservation(world),
            reply,
        });
        assert.deepEqual(world.membershipCalls, [], "no membership write on unreadable homing");
        assert.ok(!world.sets.some((c) => c.startsWith("win-a@")), "no frame on unreadable homing");
        assert.ok(result.held >= 1);
    });
});
