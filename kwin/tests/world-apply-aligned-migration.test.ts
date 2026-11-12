// Whole-workspace migration through the production apply path
// (strict v4: the base envelope is the genuine real-core
// MIGRATE_DRAIN.reply.json capture; the replicated-native scenario below reshapes its
// scenario rows in-test with v4-shaped rows, never a v3 conversion):
// the shared native grid repeats one native id per output with a distinct
// logical each. The reply (migration transfer op-61 to the destination
// trailing plus the explicit core refill op-61-refill, D5 reading B) moves
// every member exactly once, follows the target view, actuates the paired
// core refill (never a rows-lowest fallback), settles applied only with all
// members homed and both views proven (D6), holds partial/unreadable
// progress truthfully, never replays members or views, and performs no
// focus writes of its own.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { applyWorldReply } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { parseWorldReply, isWorldRejection, isCoreWorldReply, type WorldReply, type WorldReplyResult, type WorldAuthorityScope, type WorldHoldReason } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";

const FIXTURE = path.resolve("..", "test-fixtures", "kde-v4", "migration", "MIGRATE_DRAIN.reply.json");
const NATIVES = ["ws-1", "ws-2", "ws-3", "ws-4"];

// Correlated terminal take result for the migration scenario (take lane,
// requester-only): both members leave out-a/ws-1 for the out-b trailing
// under the stable core LOGICAL target id. The actuator-lane reply carries
// result null; this rides takeResult at each call site only.
const MIGRATION_TAKE: WorldReplyResult = {
    op: "migrate-workspace", direction: "right",
    sourceOut: "out-a", sourceWs: "ws-1", targetOut: "out-b", targetWs: "al-1",
    members: ["win-a", "win-c"], floats: 0,
};

function loadReply(): WorldReply {
    // The recorded drain capture predates the required nullable
    // `focusRequest` lane (no close intent in this migration scenario):
    // inject the truthful null without touching the frozen bytes.
    const raw = JSON.parse(fs.readFileSync(FIXTURE, "utf8")) as Record<string, unknown>;
    if (!("focusRequest" in raw)) {
        raw["focusRequest"] = null;
    }
    const parsed = parseWorldReply(JSON.stringify(raw));
    assert.ok(parsed !== null && !isWorldRejection(parsed));
    return parsed as WorldReply;
}

// Migration reshape of the genuine v4 drain capture: out-a/ws-1 (win-a,
// win-c) migrates to the out-b trailing ws-3; the source refills to the
// core-chosen ws-4 (moved ws-1 and its fresh replacement excluded).
function loadMigrationReply(): WorldReply {
    const reply = loadReply();
    // Wire rows omit `trailing` when false (present-but-false refuses
    // strict parse); the parser fills false and apply never reads the key.
    // The widened annotation keeps the single downcast at assignment legal.
    const wsRow = (logical: string, out: string, bind: string, order: number): {
        id: string; out: string; bind: string; scope: WorldAuthorityScope;
        order: number; mode: "tiled" | "floating"; exist: boolean; hold: WorldHoldReason;
    } => ({
        id: logical, out, bind, scope: { kind: "output", out }, order, mode: "tiled", exist: true, hold: "",
    });
    reply.workspaces = [
        wsRow("al-1", "out-a", "ws-1", 0),
        wsRow("al-2", "out-a", "ws-2", 1),
        wsRow("al-3", "out-a", "ws-3", 2),
        wsRow("al-4", "out-a", "ws-4", 3),
        wsRow("al-1", "out-b", "ws-3", 2),
        wsRow("al-6", "out-b", "ws-2", 1),
        wsRow("al-7", "out-b", "ws-1", 0),
        wsRow("al-8", "out-b", "ws-4", 3),
    ] as WorldReply["workspaces"];
    reply.outputs = [
        { out: "out-a", view: "ws-1", mode: "tiled", hold: "" },
        { out: "out-b", view: "ws-2", mode: "tiled", hold: "" },
    ];
    const winRow = (win: string, out: string, ws: string): WorldReply["windows"][number] => ({
        win, out, ws, alloc: null, frame: null,
        frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
        frame_op: "", clear_op: "", floating: false, float_origin: "",
        cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
    });
    // Transfer and member rows name the stable core LOGICAL workspace id
    // (resolved adapter-side through the accepted rows above, never by
    // native string equality): the out-b trailing is al-1 (bound to
    // native ws-3), the out-a refill is al-4 (bound to native ws-4).
    reply.windows = [
        winRow("win-a", "out-b", "al-1"),
        winRow("win-c", "out-b", "al-1"),
        winRow("win-b", "out-b", "ws-2"),
    ];
    // Core-published actuation scope for the migration target cell: the
    // adapter actuates members and views only under a controlled scope.
    reply.scopes = [
        { scope: { kind: "workspace", out: "out-b", ws: "al-1" }, auth: "controlled", reason: "" },
    ];
    reply.transfers = [
        { op: "op-61", kind: "migration", win: "", out: "out-b", ws: "al-1", follow: false, held: false, cmdSeq: null },
        { op: "op-61-refill", kind: "view", win: "", out: "out-a", ws: "al-4", follow: false, held: false, cmdSeq: null },
    ];
    // Actuator-lane result stays null (terminal results ride the take lane
    // only); the correlated outcome rides takeResult at each call site.
    reply.result = null;
    reply.effects = [];
    // The reshaped scenario must strict-parse as v4, not just the base
    // envelope: validates the authored bindings, transfers, and scopes
    // above through the real parser. The projection strips ONLY the
    // parser-inserted defaults the real parser fills on read
    // (`suspendedOutput: null`, pager `trailing: false`): the wire omits
    // them and present-but-false/null refuses.
    const { suspendedOutput, ...presentationRest } = reply.presentation;
    const wire = {
        ...reply,
        v: 4,
        presentation: {
            ...presentationRest,
            ...(suspendedOutput === null ? {} : { suspendedOutput }),
            pager: reply.presentation.pager.map((p) => ({
                ...p,
                workspaces: p.workspaces.map((w) => {
                    if (w.trailing) return { ...w };
                    const { id, ordinal, label, current, occupied, containsFocused, windows } = w;
                    return { id, ordinal, label, current, occupied, containsFocused, windows };
                }),
            })),
        },
    };
    const reparsed = parseWorldReply(JSON.stringify(wire));
    assert.ok(reparsed !== null && isCoreWorldReply(reparsed), "reshaped reply stays strict v4");
    return reply;
}

interface FakeWorld {
    homing: Map<string, { out: string; ws: string }>;
    currents: Map<string, string>;
    sendToOutputCalls: string[];
    setMembershipCalls: string[];
    switchViewCalls: string[];
    focusCalls: string[];
    lieMembership: boolean;
}

function makeObservation(world: FakeWorld): WorldObservation {
    const win = (id: string) => {
        const home = world.homing.get(id) ?? { out: "out-a", ws: "ws-1" };
        return {
            win: id, ref: { id }, out: home.out, ws: home.ws,
            rect: { x: 0, y: 0, w: 100, h: 100 },
            floating: false, fitExcluded: false, fullscreen: false,
            maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false, nativeWs: null,
            hints: { minW: null, minH: null, maxW: null, maxH: null },
            minimized: false,
        };
    };
    const domain = (id: string, ws: string, ids: string[], focused: string) => ({
        id, ws,
        bounds: { x: 0, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
        focused, readable: true, hold: "",
        windows: ids.map(win), closed: [], complete: true,
    });
    return {
        mode: "per-output-local",
        topology: [
            { out: "out-a", order: [...NATIVES], current: world.currents.get("out-a") ?? "ws-1", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true },
            { out: "out-b", order: [...NATIVES], current: world.currents.get("out-b") ?? "ws-2", complete: true, rect: { x: 800, y: 0, w: 800, h: 600 }, primary: false },
        ],
        // Replicated bindings: one row per (output, native).
        bindings: [
            { logical: "al-1", native: "ws-1", out: "out-a", pin: false, disp: false, managed: true },
            { logical: "al-2", native: "ws-2", out: "out-a", pin: false, disp: false, managed: true },
            { logical: "al-7", native: "ws-1", out: "out-b", pin: false, disp: false, managed: true },
            { logical: "al-6", native: "ws-2", out: "out-b", pin: false, disp: false, managed: true },
        ],
        domains: [
            domain("out-a", "ws-1", ["win-a", "win-c"], "win-a"),
            domain("out-b", "ws-2", ["win-b"], "win-b"),
        ],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "win-a",
        foregroundKnown: true,
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
        focusWindow: (ref: object) => {
            world.focusCalls.push((ref as { id: string }).id);
            return true;
        },
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
    // watermark, mirroring the typed v4 apply harness. Never mutates the
    // broker-owned reply.obs sequencing lane.
    const state = new WorldAdapterState(reply.adapter, "owner-a");
    assert.equal(state.onDiscovery(reply.gen, reply.adapter, "disc-1", "disc-1"), true);
    const flight = state.beginCapture(reply.source, null);
    assert.ok(flight !== null);
    assert.deepEqual(state.acceptReply(reply, reply.source, flight.watermark), { applied: true, reason: "accepted" });
    return state;
}

function baseWorld(): FakeWorld {
    return {
        homing: new Map([
            ["win-a", { out: "out-a", ws: "ws-1" }],
            ["win-c", { out: "out-a", ws: "ws-1" }],
            ["win-b", { out: "out-b", ws: "ws-2" }],
        ]),
        currents: new Map([["out-a", "ws-1"], ["out-b", "ws-2"]]),
        sendToOutputCalls: [], setMembershipCalls: [], switchViewCalls: [], focusCalls: [],
        lieMembership: false,
    };
}

describe("world migration apply", () => {
    it("moves every member once, follows the target and actuates the paired refill", () => {
        const reply = loadMigrationReply();
        const world = baseWorld();
        const state = freshState(reply);
        const applied = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATION_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.deepEqual(
            [...world.sendToOutputCalls].sort(),
            ["win-a->out-b", "win-c->out-b"],
            "each member remaps output exactly once",
        );
        assert.deepEqual(
            [...world.setMembershipCalls].sort(),
            ["win-a->ws-3", "win-c->ws-3"],
            "each member rebinds to the trailing native exactly once",
        );
        assert.ok(world.switchViewCalls.some((c) => c === "out-b->ws-3"), "target view follows");
        assert.ok(world.switchViewCalls.some((c) => c === "out-a->ws-4"), "paired core refill actuates");
        assert.ok(!world.switchViewCalls.some((c) => c === "out-a->ws-2"), "no rows-lowest fallback");
        assert.equal(world.focusCalls.length, 0, "no fabricated focus of the moved client");
        assert.ok(applied.membershipWrites >= 2);
        assert.ok(applied.viewWrites >= 2);
        // Converged repeat settles with no new native calls.
        const settled = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATION_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 2, "no member replay");
        assert.equal(world.setMembershipCalls.length, 2);
        assert.equal(world.switchViewCalls.length, 2, "no view replay");
        assert.equal(settled.membershipWrites, 0);
    });

    it("holds partial arrival with zero view switches, then lands both views once", () => {
        const reply = loadMigrationReply();
        const world = baseWorld();
        world.lieMembership = true;
        const state = freshState(reply);
        const first = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATION_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 2, "both members attempted once");
        assert.equal(world.switchViewCalls.length, 0, "zero views without full member proof");
        assert.ok(first.held >= 1);
        void first;
        // win-a lands externally; win-c still delayed: still no views.
        world.homing.set("win-a", { out: "out-b", ws: "ws-3" });
        const partial = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATION_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 2, "no member replay on partial proof");
        assert.equal(world.switchViewCalls.length, 0, "partial arrival switches nothing");
        assert.ok(partial.held >= 1);
        void partial;
        // Full arrival: both views switch exactly once, then settle.
        world.homing.set("win-c", { out: "out-b", ws: "ws-3" });
        world.lieMembership = false;
        const second = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATION_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.deepEqual(
            world.switchViewCalls,
            ["out-b->ws-3", "out-a->ws-4"],
            "target and refill switch exactly once each",
        );
        assert.ok(second.viewWrites >= 2);
        void second;
        const third = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATION_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 2);
        assert.equal(world.setMembershipCalls.length, 2);
        assert.equal(world.switchViewCalls.length, 2, "no duplicate views");
        assert.equal(third.membershipWrites, 0);
        void third;
    });

    it("a closed member holds truthfully without member replay or view switches", () => {
        const reply = loadMigrationReply();
        const world = baseWorld();
        // win-c closes mid-flight: absent from every domain.
        world.homing.delete("win-c");
        const state = freshState(reply);
        const out = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATION_TAKE,
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(out.held >= 1, "unobservable member holds");
        assert.equal(world.switchViewCalls.length, 0, "no views while a member is unproven");
        assert.ok(
            world.sendToOutputCalls.filter((c) => c.startsWith("win-c")).length <= 1,
            "no repeat for the closed member",
        );
        void out;
    });

    it("unreadable member homing holds without any native call", () => {
        const reply = loadMigrationReply();
        const world = baseWorld();
        const state = freshState(reply);
        const env = makeEnv(state, world);
        const lying = {
            ...env,
            readLiveMembership: (ref: object) => {
                const id = (ref as { id: string }).id;
                if (id === "win-c") {
                    return null;
                }
                return world.homing.get(id) ?? null;
            },
        };
        const out = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: MIGRATION_TAKE, ...lying, observation: makeObservation(world), reply });
        assert.ok(out.held >= 1, "unreadable arrival holds");
        assert.equal(world.switchViewCalls.length, 0, "no views on unreadable proof");
        void out;
    });

    it("duplicate same-native rows never double-remove the native", () => {
        const reply = loadMigrationReply();
        // Both outputs repeat ws-4 with desired absence: one native call max.
        for (const ws of reply.workspaces) {
            if (ws.bind === "ws-4") {
                ws.exist = false;
            }
        }
        reply.transfers = [];
        reply.windows = [];
        reply.result = null;
        const world = baseWorld();
        const state = freshState(reply);
        const env = makeEnv(state, world);
        let removed = 0;
        const out = applyWorldReply({ dispatchWatermark: state.currentInvalid, takeResult: null,
            ...env,
            removeDesktop: () => {
                removed += 1;
                return true;
            },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            observation: makeObservation(world),
            reply,
        });
        assert.ok(removed <= 1, `single native removal, got ${removed}`);
        void out;
    });
});
