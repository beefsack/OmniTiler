// Whole-workspace migration actuation through the production apply
// path: the Rust-engine reply (reply-migrate.json: `result` members plus
// the retained migration transfer) moves every member exactly once under
// the stable operation id, follows the target view, refills the source
// when a remaining row exists, settles applied only on proof, holds
// partial progress truthfully, and never repeats an ambiguous attempt.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { applyWorldReply } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { parseWorldReply, isWorldRejection, type WorldReply } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";

const FIXTURE = path.resolve("..", "test-fixtures", "world-wire", "reply-migrate.json");

function loadReply(): WorldReply {
    const parsed = parseWorldReply(fs.readFileSync(FIXTURE, "utf8"));
    assert.ok(parsed !== null && !isWorldRejection(parsed));
    return parsed as WorldReply;
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
            maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false,
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
                focused: "win-a", readable: true, hold: "", adjacent: {},
                windows: [win("win-a", { x: 0, y: 0, w: 100, h: 100 })], closed: [], complete: true,
            },
            {
                id: "out-b", ws: "wb1",
                bounds: { x: 800, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
                focused: "win-b", readable: true, hold: "", adjacent: {},
                windows: [win("win-b", { x: 800, y: 0, w: 100, h: 100 })], closed: [], complete: true,
            },
        ],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
    };
}

function makeEnv(state: WorldAdapterState, world: FakeWorld) {
    return {
        state,
        log: () => undefined,
        isStillListed: () => true,
        isActiveWindow: () => false,
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
    const state = new WorldAdapterState("adapter-1", "owner-a");
    state.onDiscovery("gen-1");
    state.invalidate();
    state.invalidate();
    reply.obs = state.currentObs;
    return state;
}

describe("world migration apply", () => {
    it("moves every member once and follows the target view on proof", () => {
        const reply = loadReply();
        assert.equal(reply.result?.op, "migrate-workspace");
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
        const applied = applyWorldReply({
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
        const settled = applyWorldReply({
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
        // Bind the trailing token as a remaining source workspace: the
        // source output refills there once the migrated workspace leaves.
        for (const ws of reply.workspaces) {
            if (ws.id === "wl-1" && ws.out === "out-a") {
                ws.bind = "wl-1";
            }
        }
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
        applyWorldReply({
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(world.switchViewCalls.some((c) => c === "out-b->wa1"), "target view follows");
        assert.ok(world.switchViewCalls.some((c) => c === "out-a->wl-1"), "source refills");
    });

    it("holds partial progress truthfully without repeating ambiguity", () => {
        const reply = loadReply();
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
        const first = applyWorldReply({
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(first.held >= 1, "unproven member holds");
        assert.equal(world.sendToOutputCalls.length, 1);
        // The ambiguous attempt never repeats without fresh proof.
        const second = applyWorldReply({
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.equal(world.sendToOutputCalls.length, 1, "no repeated ambiguous move");
        assert.ok(second.held >= 1);
        void first;
    });

    it("wrong-output visibility never proves the target view", () => {
        const reply = loadReply();
        // Target is out-b->wa1. Show wa1 on the WRONG output (out-a
        // already shows it) while out-b still shows wb1: any-output
        // visibility must not settle; only the exact target output proves.
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
        const out = applyWorldReply({
            ...makeEnv(state, world),
            observation: makeObservation(world),
            reply,
        });
        assert.ok(world.switchViewCalls.some((c) => c === "out-b->wa1"), "target output still switches");
        assert.ok(out.viewWrites >= 1);
        assert.deepEqual(world.currents.get("out-b"), "wa1");
    });

    it("a view switch that never lands on the target output holds", () => {
        const reply = loadReply();
        // Rewrite the retained transfer as a bare view target on out-b so
        // only the per-output view proof decides: the native switch lies
        // (records but never shows), while the target id stays visible on
        // the WRONG output. Any-output visibility must not settle applied.
        reply.transfers = [
            { op: "op-view-1", kind: "view", win: "", out: "out-b", ws: "wa1", follow: false, held: false },
        ];
        reply.windows = [];
        const world: FakeWorld = {
            homing: new Map(),
            currents: new Map([["out-a", "wa1"], ["out-b", "wb1"]]),
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
        const out = applyWorldReply({ ...lying, observation: makeObservation(world), reply });
        assert.ok(world.switchViewCalls.some((c) => c === "out-b->wa1"), "switch attempted on the exact target output");
        assert.deepEqual(world.currents.get("out-b"), "wb1", "target output never shows the target");
        assert.ok(out.held >= 1, "unproven view holds");
        assert.ok(!state.appliedViewTarget.has("out-b") || state.appliedViewTarget.get("out-b")?.ws !== "wa1");
    });

    it("incomplete observation authorizes no blind removal", () => {
        const reply = loadReply();
        reply.transfers = [];
        reply.windows = [];
        reply.workspaces = [
            { id: "ws-gone", out: "out-a", bind: "ws-gone", logical: "", scope: "out-a", order: 9, mode: "tiled", exist: false, hold: "" },
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
        const out = applyWorldReply({
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
