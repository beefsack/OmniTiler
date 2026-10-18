// KWin pinned native-echo coverage (finishInteractiveOutputChange):
// `sendClientToScreen` synchronously re-homes a single-desktop window whose
// source workspace was the source-output current onto the target-output
// current (window.cpp `finishInteractiveOutputChange`, called at the tail of
// `sendToOutput`). The adapter must accept ONLY that exact expected echo
// immediately after its own output setter within the same armed operation,
// then set the requested target workspace on the same flight (never a retry
// or a pre-switched destination view), and prove every arrival before any
// follow/refill view. Unrelated third workspaces, refreshed target currents,
// callback view races, overlay flips, and missing views still fail closed
// with no second-member or repeated native call after ambiguity.
//
// All scenarios below drive the PRODUCTION apply loop (`applyWorldReply`)
// through the PRODUCTION native host (`createWorldApplyHost`) with a fake
// KWin surface whose `sendClientToScreen` implements the pinned C++ formula
// verbatim. No live KWin, no D-Bus.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply } from "../src/world-apply";
import { createWorldApplyHost } from "../src/world-native";
import { WorldAdapterState } from "../src/world-state";
import type { WorldReply } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";

const DP = "out-left";
const HDMI = "out-right";
const D6 = "ws-source";
const DE = "ws-target-current";
const D3 = "ws-refill";
const DY = "ws-send-target";
const DX = "ws-unrelated";

interface FakeDesk {
    id: string;
    name: string;
}
interface FakeWin {
    internalId: string;
    output: { name: string };
    desktops: FakeDesk[];
    frameGeometry: { x: number; y: number; width: number; height: number };
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
}
interface Hooks {
    onSend?: (id: string) => void;
    breakTargetView?: boolean;
}

const desk = (id: string): FakeDesk => ({ id, name: `ws-${id.slice(0, 4)}` });

function makeSurface(hooks: Hooks = {}): {
    surface: Record<string, unknown>;
    currents: Map<string, FakeDesk>;
    wins: FakeWin[];
    sendCalls: string[];
    viewCalls: string[];
    setCalls: string[];
} {
    const d6 = desk(D6);
    const de = desk(DE);
    const d3 = desk(D3);
    const dy = desk(DY);
    const dx = desk(DX);
    const byId = new Map([[D6, d6], [DE, de], [D3, d3], [DY, dy], [DX, dx]]);
    const screens = [{ name: DP }, { name: HDMI }];
    const currents = new Map<string, FakeDesk>([[DP, d6], [HDMI, de]]);
    const mkWin = (internalId: string, out: string, ws: string): FakeWin => ({
        internalId,
        output: { name: out },
        desktops: [byId.get(ws) as FakeDesk],
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
    });
    const wins = [mkWin("win-a", DP, D6), mkWin("win-b", DP, D6)];
    const sendCalls: string[] = [];
    const viewCalls: string[] = [];
    const setCalls: string[] = [];
    const surface: Record<string, unknown> = {
        screens,
        desktops: [...byId.values()],
        windowList: () => wins,
        currentDesktopForScreen: (out: { name: string }) => {
            if (hooks.breakTargetView === true && out.name === HDMI) {
                return null;
            }
            return currents.get(out.name) ?? null;
        },
        setCurrentDesktopForScreen: (d: FakeDesk, out: { name: string }) => {
            currents.set(out.name, d);
            viewCalls.push(`${out.name}->${d.id}`);
        },
        // Pinned KWin formula, verbatim (window.cpp
        // `finishInteractiveOutputChange` at the tail of `sendToOutput`):
        // keep the window on the current desktop across the interactive
        // output change by replacing the old-output current with the
        // new-output current for a single-desktop window.
        sendClientToScreen: (ref: FakeWin, outRef: { name: string }) => {
            sendCalls.push(`${ref.internalId}=>${outRef.name}`);
            hooks.onSend?.(ref.internalId);
            const oldOut = ref.output;
            ref.output = { name: outRef.name };
            const curNew = currents.get(outRef.name) ?? null;
            if (curNew !== null && ref.desktops.some((d) => d.id === curNew.id)) {
                return;
            }
            const curOld = currents.get(oldOut.name) ?? null;
            if (curNew !== null && curOld !== null && curNew.id === curOld.id) {
                return;
            }
            if (curNew !== null && curOld !== null && ref.desktops.some((d) => d.id === curOld.id)) {
                setCalls.push(`${ref.internalId}~>${curNew.id}`);
                ref.desktops = [curNew];
            }
        },
    };
    return { surface, currents, wins, sendCalls, viewCalls, setCalls };
}

function readProp(value: object, property: string): unknown {
    try {
        return Reflect.get(value as Record<string, unknown>, property);
    } catch (error) {
        void error;
        return undefined;
    }
}

function observe(surf: ReturnType<typeof makeSurface>): WorldObservation {
    const winObs = (w: FakeWin) => ({
        win: w.internalId,
        ref: w,
        out: w.output.name,
        ws: w.desktops[0]?.id ?? "",
        rect: { x: 0, y: 0, w: 200, h: 200 },
        floating: false,
        fitExcluded: false,
        fullscreen: false,
        maximized: false,
        sticky: false,
        fixedAuto: false,
        fixedSuppress: false,
        hints: { minW: null, minH: null, maxW: null, maxH: null },
        minimized: false,
    });
    const currents = surf.currents;
    return {
        mode: "per-output-local",
        topology: [
            {
                out: DP, order: [D6], current: currents.get(DP)?.id ?? "",
                complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true,
            },
            {
                out: HDMI, order: [DE], current: currents.get(HDMI)?.id ?? "",
                complete: true, rect: { x: 800, y: 0, w: 800, h: 600 }, primary: false,
            },
        ],
        bindings: [],
        domains: [
            {
                id: DP, ws: D6,
                bounds: { x: 0, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
                focused: "win-a", readable: true, hold: "", adjacent: {},
                windows: surf.wins.filter((w) => w.output.name === DP).map(winObs),
                closed: [], complete: true,
            },
            {
                id: HDMI, ws: DE,
                bounds: { x: 800, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
                focused: "", readable: true, hold: "", adjacent: {},
                windows: surf.wins.filter((w) => w.output.name === HDMI).map(winObs),
                closed: [], complete: true,
            },
        ],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
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

function windowRow(win: string, out: string, ws: string): WorldReply["windows"][number] {
    return {
        win, out, ws,
        alloc: null, frame: null,
        frame_auth: "", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", floating: false, float_origin: "",
    };
}

function migrationReply(): WorldReply {
    return {
        adapter: "test-adapter", gen: "gen-1", req: "req-1", obs: "", rev: "1",
        ready: true, consumed: false,
        scopes: [],
        workspaces: [],
        outputs: [],
        windows: [windowRow("win-a", HDMI, D6), windowRow("win-b", HDMI, D6)],
        transfers: [
            { op: "op-22", kind: "migration", win: "", out: HDMI, ws: D6, follow: false, held: false },
            { op: "op-22-refill", kind: "view", win: "", out: DP, ws: D3, follow: false, held: false },
        ],
        effects: [],
        result: {
            op: "migrate-workspace", direction: "right",
            sourceOut: DP, sourceWs: D6, targetOut: HDMI, targetWs: D6,
            members: ["win-a", "win-b"], floats: 0,
        },
    };
}

function applyThroughHost(
    surf: ReturnType<typeof makeSurface>,
    state: WorldAdapterState,
    reply: WorldReply,
    observation: WorldObservation,
) {
    const host = createWorldApplyHost(surf.surface as object, readProp);
    return applyWorldReply({ ...host, state, observation, reply, log: () => undefined });
}

describe("world native output echo", () => {
    it("migrates both members through the pinned echo to the original workspace", () => {
        const surf = makeSurface();
        const reply = migrationReply();
        const state = freshState(reply);
        const first = applyThroughHost(surf, state, reply, observe(surf));
        // Both members ride the same stable operation: exactly one native
        // output send each, then the requested desktop set on the same
        // flight (the synchronous target-current echo is tolerated, never a
        // superseded-external refusal stranding the second member).
        assert.deepEqual(surf.sendCalls, [`win-a=>${HDMI}`, `win-b=>${HDMI}`]);
        assert.deepEqual(
            surf.wins.map((w) => `${w.output.name}/${w.desktops[0]?.id}`),
            [`${HDMI}/${D6}`, `${HDMI}/${D6}`],
        );
        assert.equal(first.held, 0);
        assert.equal(first.membershipWrites, 2);
        // Target AND refill views switch only after every arrival proves.
        assert.deepEqual(surf.viewCalls, [`${HDMI}->${D6}`, `${DP}->${D3}`]);
        assert.equal(first.viewWrites, 2);
        assert.equal(state.isHeldAmbiguous("op-22"), false);
        assert.deepEqual(state.appliedWindowTarget.get("win-a"), { op: "op-22", out: HDMI, ws: D6 });
        assert.deepEqual(state.appliedWindowTarget.get("win-b"), { op: "op-22", out: HDMI, ws: D6 });
        // A converged repeat proves without any new native call.
        const second = applyThroughHost(surf, state, reply, observe(surf));
        assert.deepEqual(surf.sendCalls, [`win-a=>${HDMI}`, `win-b=>${HDMI}`]);
        assert.deepEqual(surf.viewCalls, [`${HDMI}->${D6}`, `${DP}->${D3}`]);
        assert.equal(second.membershipWrites, 0);
        assert.equal(second.held, 0);
    });

    it("moves an ordinary follow and stay pair into a workspace different from the target current", () => {
        const surf = makeSurface();
        const reply: WorldReply = {
            adapter: "test-adapter", gen: "gen-1", req: "req-1", obs: "", rev: "1",
            ready: true, consumed: false,
            scopes: [], workspaces: [], outputs: [],
            windows: [windowRow("win-a", HDMI, DY), windowRow("win-b", HDMI, DY)],
            transfers: [
                { op: "op-1", kind: "window", win: "win-a", out: HDMI, ws: DY, follow: true, held: false },
                { op: "op-2", kind: "window", win: "win-b", out: HDMI, ws: DY, follow: false, held: false },
            ],
            effects: [],
            result: null,
        };
        const state = freshState(reply);
        const out = applyThroughHost(surf, state, reply, observe(surf));
        // The synchronous target-current echo is tolerated for both windows;
        // both arrive at the requested workspace, and the follow view lands
        // exactly once on the target output.
        assert.deepEqual(
            surf.wins.map((w) => `${w.output.name}/${w.desktops[0]?.id}`),
            [`${HDMI}/${DY}`, `${HDMI}/${DY}`],
        );
        assert.deepEqual(surf.viewCalls, [`${HDMI}->${DY}`]);
        assert.equal(out.held, 0);
        assert.equal(state.isHeldAmbiguous("op-1"), false);
        assert.equal(state.isHeldAmbiguous("op-2"), false);
    });

    it("still fails closed when a third workspace lands during the setter", () => {
        const surf = makeSurface({
            onSend: (id) => {
                if (id === "win-a") {
                    // External input moves the subject somewhere unexpected
                    // synchronously inside the native call: never tolerated.
                    const w = surf.wins.find((cand) => cand.internalId === id) as FakeWin;
                    w.desktops = [desk(DX)];
                }
            },
        });
        const reply = migrationReply();
        const state = freshState(reply);
        const obs = observe(surf);
        const out = applyThroughHost(surf, state, reply, obs);
        void out;
        assert.equal(surf.sendCalls.length, 1, "no second-member native call after ambiguity");
        const b = surf.wins.find((w) => w.internalId === "win-b") as FakeWin;
        assert.deepEqual(`${b.output.name}/${b.desktops[0]?.id}`, `${DP}/${D6}`, "second member untouched");
        assert.deepEqual(surf.viewCalls, [], "no follow or refill view without proof");
        assert.deepEqual(
            state.pendingFeedback.find((f) => f.op === "op-22")?.outcome,
            "superseded-external",
            "third-location proof settles the whole migration",
        );
        assert.equal(state.appliedWindowTarget.has("win-a"), false);
        assert.equal(state.appliedWindowTarget.has("win-b"), false);
        // A repeat retries nothing settled.
        applyThroughHost(surf, state, reply, obs);
        assert.equal(surf.sendCalls.length, 1, "settled migration never retried");
        assert.deepEqual(surf.viewCalls, [], "still no view moves");
    });

    it("still fails closed when the target current is refreshed during the setter", () => {
        const hooks: Hooks = {};
        const surf = makeSurface(hooks);
        hooks.onSend = (id) => {
            if (id === "win-a") {
                // A view callback races the native call: the captured
                // target current no longer matches fresh truth.
                surf.currents.set(HDMI, desk(D3));
            }
        };
        const reply = migrationReply();
        const state = freshState(reply);
        const obs = observe(surf);
        const out = applyThroughHost(surf, state, reply, obs);
        void out;
        assert.equal(surf.sendCalls.length, 1, "no second-member native call after ambiguity");
        const b = surf.wins.find((w) => w.internalId === "win-b") as FakeWin;
        assert.deepEqual(`${b.output.name}/${b.desktops[0]?.id}`, `${DP}/${D6}`, "second member untouched");
        assert.deepEqual(surf.viewCalls, [], "no follow or refill view without proof");
        assert.deepEqual(
            state.pendingFeedback.find((f) => f.op === "op-22")?.outcome,
            "superseded-external",
            "refreshed target current is never tolerated as the echo",
        );
        assert.equal(state.appliedWindowTarget.has("win-a"), false);
        // A repeat retries nothing settled.
        applyThroughHost(surf, state, reply, obs);
        assert.equal(surf.sendCalls.length, 1, "settled migration never retried");
        assert.deepEqual(surf.viewCalls, [], "still no view moves");
    });

    it("still fails closed when an overlay flips during the setter, without repeating", () => {
        const surf = makeSurface({
            onSend: (id) => {
                if (id === "win-a") {
                    const w = surf.wins.find((cand) => cand.internalId === id) as FakeWin;
                    w.fullScreen = true;
                }
            },
        });
        const reply = migrationReply();
        const state = freshState(reply);
        const first = applyThroughHost(surf, state, reply, observe(surf));
        assert.equal(surf.sendCalls.length, 1);
        assert.equal(first.membershipWrites, 0, "no membership write on an overlay flip");
        assert.deepEqual(surf.viewCalls, [], "no view moves without member proof");
        assert.ok(first.held >= 1);
        assert.equal(state.isHeldAmbiguous("op-22"), true, "ambiguous attempt stays at-most-once");
        // A repeat under the fresh revision retries nothing ambiguous.
        const second = applyThroughHost(surf, state, reply, observe(surf));
        assert.equal(surf.sendCalls.length, 1, "ambiguous member never retried");
        assert.deepEqual(surf.viewCalls, [], "still no view moves");
        assert.ok(second.held >= 1);
    });

    it("still fails closed when the target view is unreadable during the setter", () => {
        const hooks: Hooks = {};
        const surf = makeSurface(hooks);
        hooks.onSend = (id) => {
            if (id === "win-a") {
                // The target-output view becomes unreadable inside the
                // native call: the captured current can no longer be
                // verified against fresh truth.
                hooks.breakTargetView = true;
            }
        };
        const reply = migrationReply();
        const state = freshState(reply);
        const obs = observe(surf);
        const out = applyThroughHost(surf, state, reply, obs);
        void out;
        assert.equal(surf.sendCalls.length, 1, "no second-member native call after ambiguity");
        const b = surf.wins.find((w) => w.internalId === "win-b") as FakeWin;
        assert.deepEqual(`${b.output.name}/${b.desktops[0]?.id}`, `${DP}/${D6}`, "second member untouched");
        assert.deepEqual(surf.viewCalls, [], "no follow or refill view without proof");
        assert.deepEqual(
            state.pendingFeedback.find((f) => f.op === "op-22")?.outcome,
            "superseded-external",
            "an unreadable target view never verifies the echo",
        );
        assert.equal(state.appliedWindowTarget.has("win-a"), false);
        // A repeat retries nothing settled.
        applyThroughHost(surf, state, reply, obs);
        assert.equal(surf.sendCalls.length, 1, "settled migration never retried");
        assert.deepEqual(surf.viewCalls, [], "still no view moves");
    });
});
