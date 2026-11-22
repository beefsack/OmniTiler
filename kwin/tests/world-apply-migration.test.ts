// Whole-workspace migration actuation through the production apply
// path: the recorded real-core reply (MIGRATE_DRAIN.reply.json: the retained
// migration transfer plus its paired source-refill child, terminal result
// strictly take-lane) moves every member exactly once under the stable
// operation id once the creation-flow binding proof is staged, follows the
// target view, refills the source when a remaining row exists, settles
// applied only on proof, holds partial progress truthfully, and never
// repeats an ambiguous attempt. The reply is genuine v4 core bytes recorded
// from the retained real WorldEngine against a synthetic offline producer
// through the test-only MiniBroker transport shim (see
// test-fixtures/kde-v4-active/migration/README.md): not
// live-production-broker or native proof. The core leaves the migrated unit
// unbound, so each actuation test stages that one binding proof; the unbound
// hold is covered by the strict-shape parser gate.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { applyWorldReply } from "../src/world-apply";
import { incrementU64 } from "../src/world-seq";
import { WorldAdapterState } from "../src/world-state";
import { isCoreWorldReply, parseWorldReply, isWorldRejection, type WorldReply, type WorldRosterPlan } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";
import type { WorldReplyResult } from "../src/world-report";
import { applyWorldRoster, createWorldRosterSpendState, type WorldRosterHost } from "../src/world-roster";
import {
    awaitDescribeFrom,
    awaitQueueCount,
    deskById,
    discover,
    makeHarness,
    winById,
} from "./world-producer-harness";

const FIXTURE = path.resolve("..", "test-fixtures", "kde-v4-active", "migration", "MIGRATE_DRAIN.reply.json");

// Correlated terminal result for the filed migration command (take lane,
// requester-only): member/source provenance the snapshot alone cannot
// express. Names the stable core LOGICAL workspace id (ws-1), never the
// native desktop: the recorded output binds no native desktop to the
// migrated unit, so the adapter must hold rather than guess.
const MIGRATE_TAKE: WorldReplyResult = {
    op: "migrate-workspace", direction: "right",
    sourceOut: "out-a", sourceWs: "ws-1", targetOut: "out-b", targetWs: "ws-1",
    members: ["win-a"], floats: 0,
};

// The recorded take result stays exact against the const above: the real
// core output, not a hand-written oracle.
assert.deepEqual(
    JSON.parse(fs.readFileSync(path.resolve("..", "test-fixtures", "kde-v4-active", "migration", "MIGRATE_TAKE.result.json"), "utf8")),
    MIGRATE_TAKE,
);

const REFILL_NATIVE = "native-refill-a";

function loadReply(): WorldReply {
    // The active drain capture carries the required nullable
    // `focusRequest` lane natively (null: no close intent in this
    // migration scenario); the frozen historical bytes are untouched.
    const raw = JSON.parse(fs.readFileSync(FIXTURE, "utf8")) as Record<string, unknown>;
    assert.ok("focusRequest" in raw, "active drain reply carries the focusRequest lane");
    const parsed = parseWorldReply(JSON.stringify(raw));
    assert.ok(parsed !== null && !isWorldRejection(parsed));
    const reply = parsed as WorldReply;
    const refillOp = reply.transfers.find((t) => t.op === "op-1-refill");
    assert.ok(
        refillOp !== undefined && !refillOp.held && refillOp.out === "out-a" && refillOp.ws === "ws-2",
        "recorded op-1-refill child is unheld and names out-a/ws-2",
    );
    const refillRow = reply.workspaces.find((ws) => ws.id === "ws-2" && ws.out === "out-a");
    assert.ok(refillRow !== undefined && refillRow.bind === "wb1", "recorded ws-2 row on out-a is bound wb1");
    return reply;
}

// Stage the creation-flow binding proof for the migrated unit: the
// recorded core leaves ws-1 unbound until the adapter proves the native
// desktop, and these actuation oracles test the post-proof flight (member
// moves, view follows, refills) with real core bytes otherwise untouched.
function bindTarget(reply: WorldReply): void {
    for (const ws of reply.workspaces) {
        if (ws.id === "ws-1" && ws.out === "out-b") {
            ws.bind = "wa1";
        }
    }
}

// Stage a distinct synthetic native token on the recorded out-a/ws-2 refill
// row (and optionally adjust its order when testing a strictly lower decoy).
function bindRefill(reply: WorldReply, order?: number): void {
    for (const ws of reply.workspaces) {
        if (ws.id === "ws-2" && ws.out === "out-a") {
            ws.bind = REFILL_NATIVE;
            if (order !== undefined) {
                ws.order = order;
            }
        }
    }
}

interface FakeWorld {
    homing: Map<string, { out: string; ws: string }>;
    currents: Map<string, string>;
    sendToOutputCalls: string[];
    setMembershipCalls: string[];
    switchViewCalls: string[];
    lieMembership: boolean;
}

function makeObservation(world: FakeWorld): WorldObservation {
    const win = (id: string, rect: { x: number; y: number; w: number; h: number }) => {
        const home = world.homing.get(id) ?? { out: "out-a", ws: "wa1" };
        return {
            win: id, ref: { id }, out: home.out, ws: home.ws, rect,
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
                windows: [win("win-a", { x: 0, y: 0, w: 100, h: 100 })], closed: [], complete: true,
            },
            {
                id: "out-b", ws: "wb1",
                bounds: { x: 800, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
                focused: "win-b", readable: true, hold: "",
                windows: [win("win-b", { x: 800, y: 0, w: 100, h: 100 })], closed: [], complete: true,
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
        readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
        readLiveMembership: (ref: object) => {
            const id = (ref as { id: string }).id;
            return world.homing.get(id) ?? null;
        },
        readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
        setGeometry: () => true,
        setMembership: (ref: object, wsNativeId: string) => {
            const id = (ref as { id: string }).id;
            world.setMembershipCalls.push(`${id}->${wsNativeId}`);
            if (!world.lieMembership) {
                world.homing.set(id, { out: world.homing.get(id)?.out ?? "", ws: wsNativeId });
            }
            return true;
        },
        sendToOutput: (ref: object, outRef: object | null) => {
            if (outRef === null) {
                return false;
            }
            const id = (ref as { id: string }).id;
            world.sendToOutputCalls.push(`${id}->${(outRef as { name: string }).name}`);
            if (!world.lieMembership) {
                world.homing.set(id, {
                    out: (outRef as { name: string }).name,
                    ws: world.homing.get(id)?.ws ?? "",
                });
            }
            return true;
        },
        resolveOutput: (out: string) => ({ name: out }),
        resolveDesktop: (ws: string) => ({ id: ws }),
        switchView: (desktopRef: object, outRef: object) => {
            world.switchViewCalls.push(`${(outRef as { name: string }).name}->${(desktopRef as { id: string }).id}`);
            world.currents.set((outRef as { name: string }).name, (desktopRef as { id: string }).id);
            return true;
        },
        createDesktop: () => null,
        removeDesktop: () => false,
        focusWindow: () => true,
        desktopOccupants: () => ({ total: 0, readable: true }),
        desktopVisible: (nativeId: string) => {
            for (const current of world.currents.values()) {
                if (current === nativeId) {
                    return true;
                }
            }
            return false;
        },
        readLiveView: (out: string) => world.currents.get(out) ?? null,
        desktopById: (nativeId: string) => ({ id: nativeId }),
        minDesktops: 2,
    };
}

function freshState(reply: WorldReply): WorldAdapterState {
    // Bind the actual recorded receipt: producer adapter, serving
    // generation, broker-stamped source correlation, and the capture
    // watermark, mirroring the typed v4 apply harness
    // (world-v4-binding-apply.test.ts freshBound). The old hardcoded
    // adapter-1/gen-1 fenced every setter via replyFresh. Never mutates
    // the broker-owned reply.obs sequencing lane.
    const state = new WorldAdapterState(reply.adapter, "owner-a");
    assert.equal(state.onDiscovery(reply.gen, reply.adapter, "disc-1", "disc-1"), true);
    const flight = state.beginCapture(reply.source, null);
    assert.ok(flight !== null);
    assert.deepEqual(state.acceptReply(reply, reply.source, flight.watermark), { applied: true, reason: "accepted" });
    return state;
}

describe("world migration apply", () => {
    it("an unbound migrated target holds without guessing", () => {
        const reply = loadReply();
        // No binding staged: the recorded core leaves ws-1 unbound, so the
        // adapter must hold the whole migration rather than guess a
        // native desktop. Fail-closed under the exact wire bytes.
        const world: FakeWorld = {
            homing: new Map([
                ["win-a", { out: "out-a", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        const applied = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 0, "unbound target moves nothing");
        assert.equal(world.switchViewCalls.length, 0, "unbound target follows nothing");
        assert.ok(applied.held >= 1, "unbound migration holds");
        // A repeat still issues no new calls.
        const settled = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 0);
        assert.equal(world.switchViewCalls.length, 0);
        assert.ok(settled.held >= 1);
    });

    it("moves every member once and follows the target view on proof", () => {
        const reply = loadReply();
        bindTarget(reply);
        assert.equal(MIGRATE_TAKE.op, "migrate-workspace");
        const world: FakeWorld = {
            homing: new Map([
                ["win-a", { out: "out-a", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        const applied = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(world.sendToOutputCalls.some((c) => c === "win-a->out-b"), "member output moves");
        assert.equal(world.setMembershipCalls.length, 0, "same workspace id needs no desktop set");
        assert.ok(world.switchViewCalls.some((c) => c === "out-b->wa1"), "target view follows");
        assert.ok(applied.membershipWrites >= 1);
        assert.ok(applied.viewWrites >= 1);
        assert.deepEqual({ ...world.homing.get("win-a") }, { out: "out-b", ws: "wa1" });
        const homedB = world.homing.get("win-b");
        assert.equal(homedB?.out, "out-b", "non-members untouched");
        assert.equal(homedB?.ws, "wb1", "non-members untouched");
        // A repeat with converged observation settles without new calls.
        const settled = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(settled.membershipWrites, 0, "no duplicate member moves");
        assert.equal(world.sendToOutputCalls.length, 1);
        assert.equal(world.setMembershipCalls.length, 0);
    });

    it("refills the source view to its first remaining row", () => {
        const reply = loadReply();
        bindTarget(reply);
        // Stage a distinct native token on the recorded source refill row:
        // the source output refills there once the migrated workspace leaves.
        bindRefill(reply);
        const world: FakeWorld = {
            homing: new Map([
                ["win-a", { out: "out-a", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(world.switchViewCalls.some((c) => c === "out-b->wa1"), "target view follows");
        assert.ok(world.switchViewCalls.some((c) => c === `out-a->${REFILL_NATIVE}`), "source refills");
    });

    it("prefers the core-retained refill view over the rows-lowest fallback", () => {
        const reply = loadReply();
        bindTarget(reply);
        // Two remaining source rows: wa0 sorts first (order 0 vs 1), but the
        // broker's own paired refill child (op-1-refill, emitted with the
        // migration) names ws-2 and must win. The adapter never selects the refill.
        bindRefill(reply, 1);
        reply.workspaces.push({
            id: "wa0", out: "out-a", bind: "wa0",
            scope: { kind: "output", out: "out-a" }, order: 0, mode: "tiled", exist: true, hold: "", trailing: false,
        });
        const world: FakeWorld = {
            homing: new Map([
                ["win-a", { out: "out-a", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(world.switchViewCalls.some((c) => c === `out-a->${REFILL_NATIVE}`), "core refill wins");
        assert.ok(!world.switchViewCalls.some((c) => c === "out-a->wa0"), "rows-lowest loses");
    });

    it("covering output authority actuates the migration target cell", () => {
        const reply = loadReply();
        bindTarget(reply);
        // v4 structural compression (world_publish compresses uniform
        // per-output workspace statuses to AuthorityScope::Output): no
        // exact workspace row for the target cell, only the covering
        // controlled output authority.
        reply.scopes = [
            { scope: { kind: "output", out: "out-a" }, auth: "controlled", reason: "" },
            { scope: { kind: "output", out: "out-b" }, auth: "controlled", reason: "" },
        ];
        const world: FakeWorld = {
            homing: new Map([
                ["win-a", { out: "out-a", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        const applied = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(world.sendToOutputCalls.some((c) => c === "win-a->out-b"), "member output moves under covering authority");
        assert.ok(world.switchViewCalls.some((c) => c === "out-b->wa1"), "target view follows under covering authority");
        assert.ok(applied.membershipWrites >= 1);
        assert.ok(applied.viewWrites >= 1);
        assert.deepEqual({ ...world.homing.get("win-a") }, { out: "out-b", ws: "wa1" });
    });

    it("a narrower workspace hold still refuses under a controlled covering output", () => {
        const reply = loadReply();
        bindTarget(reply);
        // Exact workspace Hold on the target cell plus a controlled
        // covering output: the narrower Hold wins, never bypassed by the
        // broader Controlled.
        reply.scopes = [
            { scope: { kind: "workspace", out: "out-b", ws: "ws-1" }, auth: "hold", reason: "constraint-clamp" },
            { scope: { kind: "output", out: "out-a" }, auth: "controlled", reason: "" },
            { scope: { kind: "output", out: "out-b" }, auth: "controlled", reason: "" },
        ];
        const world: FakeWorld = {
            homing: new Map([
                ["win-a", { out: "out-a", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        const applied = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 0, "held target moves nothing");
        assert.equal(world.switchViewCalls.length, 0, "held target follows nothing");
        assert.ok(applied.held >= 1, "workspace hold refuses despite covering controlled");
    });

    it("holds partial progress truthfully without repeating ambiguity", () => {
        const reply = loadReply();
        bindTarget(reply);
        const world: FakeWorld = {
            homing: new Map([
                ["win-a", { out: "out-a", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: true,
        };
        const state = freshState(reply);
        const first = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(first.held >= 1, "unproven member holds");
        assert.equal(world.sendToOutputCalls.length, 1);
        // The ambiguous attempt never repeats without fresh proof.
        const second = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 1, "no repeated ambiguous move");
        assert.ok(second.held >= 1);
        // Partial member failure switches NO follow or refill view: the
        // target view stays untouched until all member arrival proof.
        assert.equal(world.switchViewCalls.length, 0, "zero source and target view calls");
        void first;
    });

    it("wrong-output visibility never proves the target view", () => {
        const reply = loadReply();
        // Bare view target on out-b/ws-4 (bound to native wb1). Show wb1
        // on the WRONG output (out-a) while out-b still shows wa1: switching
        // the exact target output still settles, because only the exact
        // target output proves. Any-output visibility alone never settles.
        reply.transfers = [
            { op: "op-view-9", kind: "view", win: "", out: "out-b", ws: "ws-4", follow: false, held: false, cmdSeq: null, parent: null },
        ];
        reply.windows = [];
        const world: FakeWorld = {
            homing: new Map(),
            currents: new Map([["out-a", "wb1"], ["out-b", "wa1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        const out = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(world.switchViewCalls.some((c) => c === "out-b->wb1"), "exact target output switches");
        assert.ok(out.viewWrites >= 1);
        assert.deepEqual(world.currents.get("out-b"), "wb1");
        void out;
    });

    it("a view switch that never lands on the target output holds", () => {
        const reply = loadReply();
        // Bare view target on out-b/ws-4 (bound to native wb1). The native
        // switch lies (records but never shows), and wb1 stays visible
        // nowhere: any-output visibility must not settle applied.
        reply.transfers = [
            { op: "op-view-1", kind: "view", win: "", out: "out-b", ws: "ws-4", follow: false, held: false, cmdSeq: null, parent: null },
        ];
        reply.windows = [];
        const world: FakeWorld = {
            homing: new Map(),
            currents: new Map([["out-a", "wa1"], ["out-b", "wa1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        const env = makeEnv(state, world);
        const lying = {
            ...env,
            switchView: (desktopRef: object, outRef: object) => {
                world.switchViewCalls.push(`${(outRef as { name: string }).name}->${(desktopRef as { id: string }).id}`);
                return true;
            },
        };
        const out = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE, ...lying, observation: makeObservation(world), reply });
        assert.ok(world.switchViewCalls.some((c) => c === "out-b->wb1"), "switch attempted on the exact target output");
        assert.deepEqual(world.currents.get("out-b"), "wa1", "target output never shows the target");
        assert.ok(out.held >= 1, "unproven view holds");
        assert.ok(!state.appliedViewTarget.has("out-b") || state.appliedViewTarget.get("out-b")?.ws !== "ws-4");
    });

    it("incomplete observation authorizes no blind removal", () => {
        const reply = loadReply();
        reply.transfers = [];
        reply.windows = [];
        reply.workspaces = [
            { id: "ws-gone", out: "out-a", bind: "ws-gone", scope: { kind: "output", out: "out-a" }, order: 9, mode: "tiled", exist: false, hold: "", trailing: false },
        ];
        const world: FakeWorld = {
            homing: new Map(),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        const env = makeEnv(state, world);
        let removed = 0;
        const obs = { ...makeObservation(world), complete: false };
        const out = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...env,
            removeDesktop: () => {
                removed += 1;
                return true;
            },
            desktopOccupants: () => ({ total: 0, readable: true }),
            observation: obs,
            reply,
        });
        assert.equal(removed, 0, "no native removal on incomplete inventory");
        assert.ok(out.held >= 1);
    });
});

describe("world migration refill gating", () => {
    // Reply with the broker-retained paired refill child (op-1-refill,
    // emitted with the op-1 migration by the recorded core): the paired view
    // names the core-chosen ws-2 source refill, with wa0 as the
    // rows-lowest decoy (order 0 vs 1).
    function loadPairedReply(): WorldReply {
        const reply = loadReply();
        bindTarget(reply);
        bindRefill(reply, 1);
        reply.workspaces.push({
            id: "wa0", out: "out-a", bind: "wa0",
            scope: { kind: "output", out: "out-a" }, order: 0, mode: "tiled", exist: true, hold: "", trailing: false,
        });
        return reply;
    }

    function pairedWorld(): FakeWorld {
        return {
            homing: new Map([
                ["win-a", { out: "out-a", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
    }

    it("stale held policy views are never chosen as the refill", () => {
        const reply = loadPairedReply();
        // A stale possibly-executed leftover naming an unlisted workspace:
        // exact pairing rejects it, so the rows can never refill there.
        reply.transfers.push({
            op: "op-9", kind: "view", win: "", out: "out-a",
            ws: "ws-gone", follow: false, held: true, cmdSeq: null, parent: null,
        });
        const world = pairedWorld();
        const state = freshState(reply);
        const applied = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(!world.switchViewCalls.some((c) => c === "out-a->ws-gone"), "conflicting view not chosen");
        assert.ok(world.switchViewCalls.some((c) => c === `out-a->${REFILL_NATIVE}`), "paired refill wins");
        assert.ok(world.switchViewCalls.some((c) => c === "out-b->wa1"), "target follows");
        assert.ok(applied.viewWrites >= 2);
    });

    it("delayed member arrival settles the refill once with no member replay", () => {
        const reply = loadPairedReply();
        const world = pairedWorld();
        world.lieMembership = true;
        const state = freshState(reply);
        // First apply: the member move never lands, so no follow or refill
        // view switches at all.
        const first = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 1, "member attempted once");
        assert.equal(world.switchViewCalls.length, 0, "zero view calls without member proof");
        assert.ok(first.held >= 1);
        void first;
        // The move lands externally (no adapter call): the ambiguous parent
        // accepts the late full-membership proof without member replay,
        // then switches both views exactly once.
        world.homing.set("win-a", { out: "out-b", ws: "wa1" });
        world.lieMembership = false;
        const second = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 1, "no member replay on late proof");
        assert.deepEqual(
            world.switchViewCalls,
            ["out-b->wa1", `out-a->${REFILL_NATIVE}`],
            "target and refill switch exactly once each",
        );
        assert.ok(second.viewWrites >= 2);
        void second;
        // Repeated snapshots with converged observation duplicate nothing.
        const third = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 1);
        assert.equal(world.setMembershipCalls.length, 0);
        assert.deepEqual(world.switchViewCalls, ["out-b->wa1", `out-a->${REFILL_NATIVE}`], "no duplicate views");
        void third;
    });

    it("a refill switch that never shows on the exact output proves nothing and never retries", () => {
        const reply = loadPairedReply();
        const world = pairedWorld();
        const state = freshState(reply);
        const env = makeEnv(state, world);
        // The source switch records but never lands; the target lands.
        const lying = {
            ...env,
            switchView: (desktopRef: object, outRef: object) => {
                const out = (outRef as { name: string }).name;
                const ws = (desktopRef as { id: string }).id;
                world.switchViewCalls.push(`${out}->${ws}`);
                if (out === "out-b") {
                    world.currents.set(out, ws);
                }
                return true;
            },
        };
        const first = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE, ...lying, observation: makeObservation(world), reply });
        assert.ok(world.switchViewCalls.some((c) => c === `out-a->${REFILL_NATIVE}`), "refill attempted once");
        assert.ok(first.held >= 1, "unproven refill holds");
        void first;
        // A repeat under the fresh revision retries nothing ambiguous.
        const second = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE, ...lying, observation: makeObservation(world), reply });
        assert.deepEqual(
            world.switchViewCalls.filter((c) => c === `out-a->${REFILL_NATIVE}`),
            [`out-a->${REFILL_NATIVE}`],
            "ambiguous refill never retried",
        );
        assert.ok(second.held >= 1);
        void second;
    });

    it("an already-shown refill proves on the exact output with no new call", () => {
        const reply = loadPairedReply();
        const world = pairedWorld();
        // Members already homed at the target, both views already shown.
        world.homing.set("win-a", { out: "out-b", ws: "wa1" });
        world.currents.set("out-b", "wa1");
        world.currents.set("out-a", REFILL_NATIVE);
        const state = freshState(reply);
        const applied = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 0, "no member replay");
        assert.equal(world.switchViewCalls.length, 0, "exact-output proof needs no call");
        assert.equal(applied.viewWrites, 0);
    });
});

describe("world migration member authority", () => {
    function makeSiblingObs(world: FakeWorld, includeWinC = true): WorldObservation {
        const obs = makeObservation(world);
        if (includeWinC) {
            const homeC = world.homing.get("win-c") ?? { out: "out-a", ws: "wa1" };
            const source = obs.domains[0];
            assert.ok(source !== undefined);
            source.windows.push({
                win: "win-c", ref: { id: "win-c" }, out: homeC.out, ws: homeC.ws,
                rect: { x: 100, y: 0, w: 100, h: 100 },
                floating: false, fitExcluded: false, fullscreen: false,
                maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false, nativeWs: null,
                hints: { minW: null, minH: null, maxW: null, maxH: null },
                minimized: false,
            });
        }
        return obs;
    }

    function loadAuthReply(
        memberAuth: "hold" | "untouched" | "controlled",
        memberHold: WorldReply["windows"][number]["member_hold"] = "",
    ): WorldReply {
        const reply = loadReply();
        bindTarget(reply);
        bindRefill(reply);
        const winA = reply.windows.find((w) => w.win === "win-a");
        assert.ok(winA !== undefined);
        winA.frame_auth = "untouched";
        winA.frame_hold = "";
        winA.member_auth = memberAuth;
        winA.member_hold = memberHold;
        winA.floating = true;
        winA.alloc = null;
        winA.frame = [10, 10, 200, 150];
        reply.windows.push({
            ...winA,
            win: "win-c",
            alloc: [0, 0, 100, 100],
            frame: [0, 0, 100, 100],
            frame_auth: "controlled",
            frame_hold: "",
            member_auth: "controlled",
            member_hold: "",
            floating: false,
        });
        return reply;
    }

    it("hold and untouched unhomed members block setters, view switches, and premature dispatch across controlled siblings", () => {
        for (const [auth, hold] of [
            ["hold", "unreadable"],
            ["untouched", ""],
        ] as const) {
            const blockedReply = loadAuthReply(auth, hold);
            const world: FakeWorld = {
                homing: new Map([
                    ["win-a", { out: "out-a", ws: "ws-0" }],
                    ["win-c", { out: "out-a", ws: "ws-0" }],
                    ["win-b", { out: "out-b", ws: "wb1" }],
                ]),
                currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
                sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
                lieMembership: false,
            };
            const state = freshState(blockedReply);
            const frameWrites: string[] = [];
            const env = {
                ...makeEnv(state, world),
                setGeometry: (ref: object) => {
                    assert.ok("id" in ref && typeof ref.id === "string");
                    frameWrites.push(ref.id);
                    return true;
                },
            };
            const blocked = applyWorldReply({
                dispatchWatermark: state.currentInvalid,
                takeResult: MIGRATE_TAKE,
                ...env,
                observation: makeSiblingObs(world),
                reply: blockedReply,
            });
            assert.equal(world.sendToOutputCalls.length, 0, `${auth}: no output setter`);
            assert.equal(world.setMembershipCalls.length, 0, `${auth}: no membership setter`);
            assert.equal(world.switchViewCalls.length, 0, `${auth}: no premature target/refill switch`);
            assert.ok(!frameWrites.includes("win-a"), `${auth}: no frame write on blocked float`);
            assert.ok(blocked.held >= 1, `${auth}: holds migration`);
            const opId = blockedReply.transfers.find((t) => t.kind === "migration")?.op;
            assert.ok(opId !== undefined);
            assert.equal(state.isDispatched(opId), false, `${auth}: no premature dispatch before authorization`);

            // A later authorized reply carries the same float without a frame write.
            const authReply = loadAuthReply("controlled", "");
            const rev = incrementU64(blockedReply.rev);
            const obs = incrementU64(blockedReply.obs);
            assert.ok(rev !== null && obs !== null);
            authReply.rev = rev;
            authReply.obs = obs;
            authReply.req = authReply.source = "src-authorized-2";
            const flight2 = state.beginCapture(authReply.source, null);
            assert.ok(flight2 !== null);
            assert.deepEqual(state.acceptReply(authReply, authReply.source, flight2.watermark), {
                applied: true,
                reason: "accepted",
            });
            const applied = applyWorldReply({
                dispatchWatermark: state.currentInvalid,
                takeResult: MIGRATE_TAKE,
                ...env,
                observation: makeSiblingObs(world),
                reply: authReply,
            });
            assert.deepEqual(world.sendToOutputCalls, ["win-a->out-b", "win-c->out-b"], `${auth}: later authorized output moves`);
            assert.deepEqual(world.setMembershipCalls, ["win-a->wa1", "win-c->wa1"], `${auth}: later authorized membership moves`);
            assert.deepEqual(world.switchViewCalls, ["out-b->wa1", `out-a->${REFILL_NATIVE}`], `${auth}: target and refill switch on proof`);
            assert.ok(!frameWrites.includes("win-a"), `${auth}: no frame write on authorized float`);
            assert.equal(applied.membershipWrites, 2);
        }
    });

    it("already-homed readback proves arrival without a setter even when unauthorized", () => {
        const reply = loadAuthReply("untouched", "");
        reply.windows = reply.windows.filter((w) => w.win === "win-a");
        const world: FakeWorld = {
            homing: new Map([
                ["win-a", { out: "out-b", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        let setGeometryCalls = 0;
        const applied = applyWorldReply({
            dispatchWatermark: state.currentInvalid,
            takeResult: MIGRATE_TAKE,
            ...makeEnv(state, world),
            setGeometry: () => {
                setGeometryCalls += 1;
                return true;
            },
            observation: makeSiblingObs(world, false),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 0);
        assert.equal(world.setMembershipCalls.length, 0);
        assert.equal(setGeometryCalls, 0);
        assert.deepEqual(world.switchViewCalls, ["out-b->wa1", `out-a->${REFILL_NATIVE}`]);
        assert.equal(applied.viewWrites, 2);
    });

    it("changed readback before the setter pass fences unauthorized output/membership writes", () => {
        const reply = loadAuthReply("untouched", "");
        reply.windows = reply.windows.filter((w) => w.win === "win-a");
        const world: FakeWorld = {
            homing: new Map([
                ["win-a", { out: "out-b", ws: "wa1" }],
                ["win-b", { out: "out-b", ws: "wb1" }],
            ]),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
            sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [],
            lieMembership: false,
        };
        const state = freshState(reply);
        let reads = 0;
        const env = {
            ...makeEnv(state, world),
            readLiveMembership: (ref: object) => {
                assert.ok("id" in ref && typeof ref.id === "string");
                const id = ref.id;
                if (id === "win-a") {
                    reads += 1;
                    // First read (prescan) sees already-homed; second read (setter pass) sees unhomed.
                    return reads === 1 ? { out: "out-b", ws: "wa1" } : { out: "out-a", ws: "wa1" };
                }
                return world.homing.get(id) ?? null;
            },
        };
        const applied = applyWorldReply({
            dispatchWatermark: state.currentInvalid,
            takeResult: MIGRATE_TAKE,
            ...env,
            observation: makeSiblingObs(world, false),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 0, "changed readback fences output setter");
        assert.equal(world.setMembershipCalls.length, 0, "changed readback fences membership setter");
        assert.equal(world.switchViewCalls.length, 0, "unresolved arrival blocks target/refill switches");
        assert.ok(applied.held >= 1);
    });

    it("genuine checked-broker migration pairs unrelated opaque ops and inverse lookalike acts independently", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c", "win-d", "win-e"], {
            outputs: [
                { name: "out-source", geometry: { x: 0, y: 0, width: 800, height: 600 } },
                { name: "out-dest", geometry: { x: 800, y: 0, width: 800, height: 600 } },
            ],
            desktops: ["slot-a", "slot-b", "slot-c", "slot-d"],
            homes: { "win-b": "out-dest" },
            currents: { "out-source": "slot-b", "out-dest": "slot-a" },
            scopeClass: "per-output-local",
            withNativeCreation: true,
        });
        winById(h, "win-a").desktops = [deskById(h, "slot-b")];
        winById(h, "win-d").desktops = [deskById(h, "slot-c")];
        winById(h, "win-e").desktops = [deskById(h, "slot-c")];

        let brokerReplyRaw = "";
        try {
            await discover(h);
            await h.settle();
            h.handle.requestWorkspaceMigrate("right");
            await awaitQueueCount(h, "migrate-directional", 1);
            const cmds = h.commandPayloads("migrate-directional");
            assert.equal(cmds.length, 1, "one genuine migrate-directional command filed");
            const cmdIndex = cmds[0]?.index as number;
            const drainIndex = await awaitDescribeFrom(h, cmdIndex + 1);
            await h.settle();
            brokerReplyRaw = h.replies.get(drainIndex) ?? "";
            assert.ok(brokerReplyRaw !== "", "checked broker returned drain reply");
        } finally {
            h.handle.stop();
            await h.bridge.close();
        }

        const prepareBrokerReply = (independent: boolean) => {
            const raw = JSON.parse(brokerReplyRaw) as Record<string, unknown>;
            const transfers = raw["transfers"] as Array<Record<string, unknown>>;
            const migration = transfers.find((row) => row["kind"] === "migration");
            const refill = transfers.find((row) => row["kind"] === "view" && row["parent"] === migration?.["op"]);
            assert.ok(migration !== undefined && refill !== undefined, "broker retains migration and its paired refill");
            migration["op"] = "mig-parent-alpha";
            refill["op"] = independent ? "mig-parent-alpha-refill" : "view-child-beta";
            refill["parent"] = independent ? null : migration["op"];
            const plan = raw["rosterPlan"] as { members: Array<{ step: string; win: string }>; views: Array<{ out: string; deps: string[] }> };
            assert.ok(plan !== undefined, "real shared KWin route issues slot authority");
            const member = plan.members.find((row) => row.win === "win-a");
            const source = plan.views.find((row) => row.out === "out-source");
            const target = plan.views.find((row) => row.out === "out-dest");
            assert.ok(member !== undefined && source !== undefined && target !== undefined, JSON.stringify(plan));
            assert.ok(source.deps.includes(member.step) && target.deps.includes(member.step), "both real views require exact arrival");
            if (independent) source.deps = source.deps.filter((step) => step !== member.step);
            const parsed = parseWorldReply(JSON.stringify(raw));
            assert.ok(isCoreWorldReply(parsed) && parsed.rosterPlan !== undefined);
            return parsed.rosterPlan;
        };
        const exercise = (plan: WorldRosterPlan, arrives: boolean) => {
            const order = [...plan.baseline.order];
            const currents = new Map(plan.currents.map((row) => [row.out, row.native]));
            const occupants = new Map(plan.occupants.map((row) => [row.native, new Map(row.windows.map((w) => [w.win, w.out]))]));
            const writes: string[] = [];
            const viewWrites: string[] = [];
            const memberHome: WorldRosterHost["memberHome"] = (win) => {
                for (const [native, windows] of occupants) {
                    const out = windows.get(win);
                    if (out !== undefined) return { out, native };
                }
                return null;
            };
            const host: WorldRosterHost = {
                fresh: () => true, read: () => ({ order, currents, occupants }),
                memberHome, canAffect: () => true, canWrite: () => true, canRemove: () => true,
                append: () => { writes.push("append"); order.push("appended-tail"); return true; },
                move: (win, out, native) => {
                    writes.push(`move:${win}`);
                    if (win !== "win-a" || arrives) {
                        const before = memberHome(win);
                        assert.ok(before !== null);
                        occupants.get(before.native)?.delete(win);
                        let windows = occupants.get(native);
                        if (windows === undefined) occupants.set(native, windows = new Map());
                        windows.set(win, out);
                    }
                    return true;
                },
                select: (out, native) => {
                    writes.push(`view:${out}`); viewWrites.push(out); currents.set(out, native); return true;
                },
                remove: (native) => {
                    writes.push(`remove:${native}`); order.splice(order.indexOf(native), 1); occupants.delete(native); return true;
                },
                focus: () => true,
            };
            const spend = createWorldRosterSpendState();
            const applied = applyWorldRoster(plan, host, spend);
            const beforeReplay = [...writes];
            assert.deepEqual(applyWorldRoster(plan, host, spend), applied, "same publication preserves exact proof");
            assert.deepEqual(writes, beforeReplay, "no native setter replays");
            return { applied, viewWrites };
        };

        const pending = exercise(prepareBrokerReply(false), false);
        assert.deepEqual(pending.viewWrites, [], "no early refill or target switch before arrival");
        assert.equal(pending.applied.stopped, "pending");

        const arrived = exercise(prepareBrokerReply(false), true);
        assert.deepEqual(arrived.viewWrites, ["out-dest", "out-source"], "arrival follows target then paired refill without requester result");
        assert.equal(arrived.applied.complete, true);

        const inverse = exercise(prepareBrokerReply(true), false);
        assert.deepEqual(inverse.viewWrites, ["out-source"], "unparented suffix lookalike acts independently while migration holds");
        assert.equal(inverse.applied.stopped, "pending");
    });
});
