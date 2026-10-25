// Explicit retained float placement through the production apply path:
// the Rust-engine reply (reply-float.json: the window row carries the
// stable core property token `frame_op`, plus the correlated `result`
// provenance) writes the exact native frame once per operation, spends
// before the setter so duplicates never re-place, settles applied only on
// read-back proof, and never lets unrelated later replies steal the native
// float frame. A lost result reply still recovers through the retained row
// token on the next unrelated snapshot.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { applyWorldReply } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { parseWorldReply, isWorldRejection, type WorldReply } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";

const FIXTURE = path.resolve("..", "test-fixtures", "world-wire", "reply-float.json");
// Stable core property operation identity for the float command that
// produced the shared fixture (the broker command sequence of the
// fixture-generation session).
const OP = "op-1";
// Centered 60% float placement in the outer-inset work area (800x600 inset
// by outer 4 gives 792x592 at (4,4); 60% is 475x355 centered at
// (162,122)): respects the screen-edge gap like tiled frames.
const TARGET = { x: 162, y: 122, w: 475, h: 355 };

function loadReply(): WorldReply {
    const parsed = parseWorldReply(fs.readFileSync(FIXTURE, "utf8"));
    assert.ok(parsed !== null && !isWorldRejection(parsed));
    const reply = parsed as WorldReply;
    // v2 capture lane always carries result null; the terminal toggle-float
    // result rides TakeCommandResult (tested in command-result). Float
    // placement here uses the retained frame_op token.
    assert.equal(reply.result, null);
    assert.equal(reply.windows.find((w) => w.win === "win-a")?.frame_op, OP);
    return reply;
}

interface FakeWorld {
    frames: Map<string, { x: number; y: number; w: number; h: number }>;
    sets: string[];
    gesture: boolean;
}

function makeObservation(world: FakeWorld): WorldObservation {
    const win = (id: string) => {
        const rect = world.frames.get(id) ?? { x: 0, y: 0, w: 100, h: 100 };
        return {
            win: id, ref: { id }, out: "out", ws: "ws",
            rect: { ...rect },
            floating: false, fitExcluded: false, fullscreen: false,
            maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false, nativeWs: null,
            hints: { minW: null, minH: null, maxW: null, maxH: null },
            minimized: false,
        };
    };
    return {
        mode: "per-output-local",
        topology: [{ out: "out", order: ["ws"], current: "ws", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
        bindings: [],
        domains: [{
            id: "out", ws: "ws",
            bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 4, focused: "win-a", readable: true, hold: "",
            adjacent: {},
            windows: [win("win-a"), win("win-b")], closed: [], complete: true,
        }],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: world.gesture,
        focusedGlobal: "",
        screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

interface EnvOpts {
    throwOnSet?: boolean;
    refuseSet?: boolean;
    setFilter?: (id: string) => boolean;
}

function makeEnv(state: WorldAdapterState, world: FakeWorld, opts: EnvOpts = {}) {
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
            if (opts.throwOnSet) {
                throw new Error("native setter threw");
            }
            if (opts.refuseSet) {
                return false;
            }
            if (opts.setFilter !== undefined && !opts.setFilter(id)) {
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

function freshState(reply: WorldReply): WorldAdapterState {
    const state = new WorldAdapterState("adapter-1", "owner-a");
    state.onDiscovery("gen-1", "broker-1");
    while (state.currentInvalid !== reply.obs) {
        state.invalidate();
    }
    return state;
}

// Local dispatch-record oracle (verdict transport deleted): a dispatched
// operation records once and never re-dispatches this generation; the raw
// attempt rows plus fresh facts are the settlement evidence the core owns.
function heldAmbiguous(state: WorldAdapterState, op: string): boolean {
    return state.isDispatched(op);
}

describe("world float placement", () => {
    it("places the exact core rect once under the explicit token, never re-places", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            // win-b already converges so only the float placement writes.
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 404, y: 0, w: 396, h: 600 }],
            ]),
            sets: [],
            gesture: false,
        };
        // The tiled win-b row converges exactly, so only the float placement writes.
        reply.windows.find((w) => w.win === "win-b")!.frame = [0, 0, 800, 600];
        reply.windows.find((w) => w.win === "win-b")!.alloc = [0, 0, 800, 600];
        world.frames.set("win-b", { x: 0, y: 0, w: 800, h: 600 });
        const state = freshState(reply);
        // beginCapture clears dirty at flight start; the dispatch below
        // must re-dirty for its follow-up.
        state.dirty = false;
        const first = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.deepEqual(world.sets.filter((c) => c.startsWith("win-a@")), ["win-a@162,122,475,355"]);
        assert.ok(first.geometryWrites >= 1);
        assert.deepEqual(world.frames.get("win-a"), TARGET);
        assert.equal(heldAmbiguous(state, OP), true, "dispatch recorded once; the raw attempt rides, core owns settlement");
        assert.deepEqual(state.framesForReport().filter((a) => a.op === OP).length, 1, "one raw frame attempt recorded");
        assert.equal(state.dirty, true, "dispatch schedules a follow-up capture");
        // A duplicate reply (same operation) never double-places.
        const second = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(second.geometryWrites, 0);
        assert.deepEqual(world.sets.filter((c) => c.startsWith("win-a@")), ["win-a@162,122,475,355"]);
    });

    it("recovers a lost result reply through the retained row token on the next snapshot", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 0, y: 0, w: 800, h: 600 }],
            ]),
            sets: [],
            gesture: false,
        };
        reply.windows.find((w) => w.win === "win-b")!.frame = [0, 0, 800, 600];
        reply.windows.find((w) => w.win === "win-b")!.alloc = [0, 0, 800, 600];
        const state = freshState(reply);
        // The first reply (with the one-shot result) is dropped before any
        // adapter setter: nothing applies and nothing is spent.
        assert.equal(state.floatSpent.size, 0);
        // The next unrelated snapshot carries no result but retains the
        // same explicit operation token with the same target.
        const later: WorldReply = { ...reply, rev: "3", result: null };
        const applied = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply: later,
        });
        assert.ok(applied.geometryWrites >= 1);
        assert.deepEqual(world.sets.filter((c) => c.startsWith("win-a@")), ["win-a@162,122,475,355"]);
        assert.deepEqual(world.frames.get("win-a"), TARGET);
        assert.equal(heldAmbiguous(state, OP), true, "dispatch recorded once; the raw attempt rides, core owns settlement");
    });

    it("spends nothing on a stale final gate, then places once on a fresh capture", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 0, y: 0, w: 800, h: 600 }],
            ]),
            sets: [],
            gesture: false,
        };
        reply.windows.find((w) => w.win === "win-b")!.frame = [0, 0, 800, 600];
        reply.windows.find((w) => w.win === "win-b")!.alloc = [0, 0, 800, 600];
        const state = freshState(reply);
        // Live homing drifted since the observation was captured: the
        // lane-level freshness still passes, but the final pre-setter
        // gate fails closed before anything is spent.
        let calls = 0;
        const staleEnv = {
            ...makeEnv(state, world),
            readLiveMembership: () => ({ out: "out", ws: "drifted" }),
            setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => {
                calls += 1;
                return makeEnv(state, world).setGeometry(ref, rect);
            },
        };
        const stale = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...staleEnv,
            observation: makeObservation(world),
            reply,
        });
        assert.equal(calls, 0, "stale final gate runs no native setter");
        assert.ok(!state.floatSpent.has(OP), "stale veto spends no one-shot token");
        assert.deepEqual(state.framesForReport().filter((a) => a.op === OP), [], "stale veto records no attempt");
        assert.equal(stale.stoppedMidApply, true, "stale homing stops the apply");
        // A fresh capture (homing converged) retries the still-undispatched
        // desire exactly once.
        const fresh = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(fresh.geometryWrites >= 1, "fresh capture places");
        assert.deepEqual(world.sets.filter((c) => c.startsWith("win-a@")), ["win-a@162,122,475,355"]);
        assert.equal(heldAmbiguous(state, OP), true, "dispatch recorded once after the fresh gate");
    });

    it("lets unrelated later replies keep the retained frame without stealing", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 0, y: 0, w: 800, h: 600 }],
            ]),
            sets: [],
            gesture: false,
        };
        reply.windows.find((w) => w.win === "win-b")!.frame = [0, 0, 800, 600];
        reply.windows.find((w) => w.win === "win-b")!.alloc = [0, 0, 800, 600];
        const state = freshState(reply);
        applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...makeEnv(state, world), observation: makeObservation(world), reply });
        assert.deepEqual(world.frames.get("win-a"), TARGET);
        // An unrelated later reply without the token and without a result:
        // the native frame stands untouched.
        const later: WorldReply = {
            ...reply,
            rev: "3",
            result: null,
            windows: reply.windows.map((w) =>
                w.win === "win-a" ? { ...w, frame_op: "" } : w,
            ),
        };
        const again = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply: later,
        });
        assert.equal(again.geometryWrites, 0);
        assert.deepEqual(world.sets.filter((c) => c.startsWith("win-a@")), ["win-a@162,122,475,355"]);
        assert.deepEqual(world.frames.get("win-a"), TARGET);
    });

    it("never writes ordinary float geometry without an explicit token", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 0, y: 0, w: 800, h: 600 }],
            ]),
            sets: [],
            gesture: false,
        };
        reply.windows.find((w) => w.win === "win-b")!.frame = [0, 0, 800, 600];
        reply.windows.find((w) => w.win === "win-b")!.alloc = [0, 0, 800, 600];
        // Strip the token but keep a differing retained frame: no write.
        const tokenless: WorldReply = {
            ...reply,
            result: null,
            windows: reply.windows.map((w) =>
                w.win === "win-a" ? { ...w, frame_op: "" } : w,
            ),
        };
        const state = freshState(tokenless);
        const out = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply: tokenless,
        });
        assert.equal(out.geometryWrites, 0);
        assert.deepEqual(world.sets.filter((c) => c.startsWith("win-a@")), []);
        assert.deepEqual(world.frames.get("win-a"), { x: 10, y: 10, w: 100, h: 100 });
    });

    it("a stale reply applies nothing", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 0, y: 0, w: 800, h: 600 }],
            ]),
            sets: [],
            gesture: false,
        };
        reply.windows.find((w) => w.win === "win-b")!.frame = [0, 0, 800, 600];
        reply.windows.find((w) => w.win === "win-b")!.alloc = [0, 0, 800, 600];
        const state = freshState(reply);
        const mark = state.currentInvalid;
        state.invalidate();
        const stale: WorldReply = { ...reply };
        const out = applyWorldReply({ dispatchWatermark: mark, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply: stale,
        });
        assert.equal(out.geometryWrites, 0);
        assert.deepEqual(world.sets, []);
        assert.equal(heldAmbiguous(state, OP), false, "stale reply dispatches nothing");
    });

    it("a throw after the setter holds ambiguous without repeating", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 0, y: 0, w: 800, h: 600 }],
            ]),
            sets: [],
            gesture: false,
        };
        reply.windows.find((w) => w.win === "win-b")!.frame = [0, 0, 800, 600];
        reply.windows.find((w) => w.win === "win-b")!.alloc = [0, 0, 800, 600];
        const state = freshState(reply);
        let calls = 0;
        const env = {
            ...makeEnv(state, world, { throwOnSet: true }),
            observation: makeObservation(world),
            reply,
        };
        const throwing = {
            ...env,
            dispatchWatermark: state.currentInvalid,
            takeResult: null,
            setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => {
                calls += 1;
                void rect;
                void ref;
                throw new Error("native setter threw");
            },
        };
        const out = applyWorldReply(throwing);
        assert.ok(out.held >= 1);
        assert.equal(calls, 1);
        assert.equal(heldAmbiguous(state, OP), true, "dispatch record stays; the raw creation-style attempt rides");
        // The ambiguous operation never repeats without fresh proof.
        const again = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(calls, 1);
        assert.equal(again.geometryWrites, 0);
    });

    it("an already-ambiguous operation holds without a setter", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 0, y: 0, w: 800, h: 600 }],
            ]),
            sets: [],
            gesture: false,
        };
        reply.windows.find((w) => w.win === "win-b")!.frame = [0, 0, 800, 600];
        reply.windows.find((w) => w.win === "win-b")!.alloc = [0, 0, 800, 600];
        const state = freshState(reply);
        state.markDispatched(OP);
        let calls = 0;
        const env = makeEnv(state, world);
        const out = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...env,
            setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => {
                calls += 1;
                return env.setGeometry(ref, rect);
            },
            observation: makeObservation(world),
            reply,
        });
        assert.equal(calls, 0);
        assert.ok(out.held >= 1);
    });

    it("a post-apply manual move supersedes instead of fighting", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 0, y: 0, w: 800, h: 600 }],
            ]),
            sets: [],
            gesture: false,
        };
        reply.windows.find((w) => w.win === "win-b")!.frame = [0, 0, 800, 600];
        reply.windows.find((w) => w.win === "win-b")!.alloc = [0, 0, 800, 600];
        const state = freshState(reply);
        applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...makeEnv(state, world), observation: makeObservation(world), reply });
        assert.equal(heldAmbiguous(state, OP), true, "dispatch record stays; no second native write");
        // The user moves the float elsewhere; the retained token still rides
        // a later reply (the core has not settled yet).
        world.frames.set("win-a", { x: 300, y: 300, w: 200, h: 150 });
        const moved = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply: { ...reply, rev: "3", result: null },
        });
        assert.equal(moved.geometryWrites, 0);
        assert.deepEqual(world.sets.filter((c) => c.startsWith("win-a@")), ["win-a@162,122,475,355"]);
        assert.deepEqual(world.frames.get("win-a"), { x: 300, y: 300, w: 200, h: 150 });
        assert.equal(heldAmbiguous(state, OP), true, "external move holds without re-actuation; core settles from the attempt");
    });

    it("an explicit refusal records the refused row without repeating", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 0, y: 0, w: 800, h: 600 }],
            ]),
            sets: [],
            gesture: false,
        };
        reply.windows.find((w) => w.win === "win-b")!.frame = [0, 0, 800, 600];
        reply.windows.find((w) => w.win === "win-b")!.alloc = [0, 0, 800, 600];
        const state = freshState(reply);
        let calls = 0;
        const env = makeEnv(state, world, { refuseSet: true });
        const out = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...env,
            setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => {
                calls += 1;
                return env.setGeometry(ref, rect);
            },
            observation: makeObservation(world),
            reply,
        });
        assert.equal(calls, 1);
        assert.equal(heldAmbiguous(state, OP), true, "dispatch record stays; the refused row rides, core owns refusal");
        assert.deepEqual(
            state.framesForReport().filter((a) => a.op === OP),
            [{ win: "win-a", target: [162, 122, 475, 355], setter: "refused", readback: [10, 10, 100, 100], dpiBefore: null, dpiAfter: null, attemptMs: state.lastClock, op: OP, pre: [10, 10, 100, 100] }],
            "refused disposition plus unchanged prewrite rides the next report",
        );
        const again = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(calls, 1);
        assert.equal(again.geometryWrites, 0);
        void out;
    });

    it("suspends placement behind the gesture barrier without spending", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([["win-a", { x: 10, y: 10, w: 100, h: 100 }]]),
            sets: [] as string[],
            gesture: true,
        };
        const state = freshState(reply);
        // beginCapture clears dirty at flight start; the apply below starts
        // from a clean capture.
        state.dirty = false;
        const held = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(held.held >= 1);
        assert.equal(world.sets.length, 0);
        assert.ok(!state.floatSpent.has(OP), "gesture hold stays unspent so the target retries");
        assert.equal(state.dirty, false, "pure hold with no dispatch schedules no follow-up capture");
        assert.deepEqual(state.framesForReport(), [], "pure hold records no attempt");
        // After the barrier lifts the same retained token still places.
        world.gesture = false;
        const placed = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.deepEqual(world.sets.filter((c) => c.startsWith("win-a@")), ["win-a@162,122,475,355"]);
        assert.equal(heldAmbiguous(state, OP), true, "dispatch recorded once after the barrier lifts; core owns settlement");
        void placed;
    });

    it("a never-converging tiled row never suppresses the explicit float target", () => {
        const reply = loadReply();
        const world: FakeWorld = {
            frames: new Map([
                ["win-a", { x: 10, y: 10, w: 100, h: 100 }],
                ["win-b", { x: 1, y: 1, w: 10, h: 10 }],
            ]),
            sets: [],
            gesture: false,
        };
        // win-b never converges (its setter is filtered out), so every
        // Controlled mismatch writes with no local budget; the core owns
        // stop/resume through later desires. The explicit float target
        // for win-a still places exactly once.
        const state = freshState(reply);
        const env = makeEnv(state, world, { setFilter: (id) => id === "win-a" });
        for (let i = 0; i < 5; i += 1) {
            applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null, ...env, observation: makeObservation(world), reply });
        }
        assert.deepEqual(world.sets.filter((c) => c.startsWith("win-a@")), ["win-a@162,122,475,355"]);
        assert.equal(state.framesForReport().filter((a) => a.win === "win-b").length, 1, "latest-wins keeps one row per window");
        assert.equal(heldAmbiguous(state, OP), true, "explicit float dispatch recorded once; tiled retries never touch it");
    });
});
