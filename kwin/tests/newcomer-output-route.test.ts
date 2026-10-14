import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    applyNewcomerOutputRoute,
    decideNewcomerOutputRoute,
    type NewcomerRouteDecisionInput,
    type NewcomerRouteDomain,
    type NewcomerRouteEnv,
    type NewcomerRouteFlags,
} from "../src/newcomer-output-route";

// R-OUT-05 (User 2026-10-10, 0.1 triage D06) regression coverage:
// focused-output routing vs pointer placement, stale/ineligible/fullscreen/
// game/transient safeguards, and activation race/order. Pure decision plus
// ordered-write applier over a mock env; no live KWin.

const FOCUSED: NewcomerRouteDomain = { output: "out-focused", workspace: "ws-focused" };
const POINTER: NewcomerRouteDomain = { output: "out-pointer", workspace: "ws-pointer" };

const ORDINARY: NewcomerRouteFlags = {
    fullscreen: false,
    maximized: false,
    floating: false,
    sticky: false,
};

function input(over: Partial<NewcomerRouteDecisionInput>): NewcomerRouteDecisionInput {
    return {
        newcomerOutput: POINTER.output,
        flags: ORDINARY,
        unknownCount: 1,
        addedSinceRefresh: true,
        lastManaged: FOCUSED,
        flightBusy: false,
        ...over,
    };
}

describe("decideNewcomerOutputRoute (R-OUT-05 focused output vs pointer)", () => {
    it("routes an eligible newcomer from the pointer output to the focused output", () => {
        const decision = decideNewcomerOutputRoute(input({}));
        assert.equal(decision.ok, true);
        if (decision.ok) {
            assert.deepEqual(decision.target, FOCUSED);
        }
    });

    it("refuses without a managed domain (startup adoption is never routed)", () => {
        const decision = decideNewcomerOutputRoute(input({ lastManaged: null }));
        assert.deepEqual(decision, { ok: false, reason: "no-managed-domain" });
    });

    it("refuses without a recent birth (view switch and user movement never route)", () => {
        const decision = decideNewcomerOutputRoute(input({ addedSinceRefresh: false }));
        assert.deepEqual(decision, { ok: false, reason: "no-recent-add" });
    });

    it("refuses when no single newcomer is identifiable (zero or mass unknowns)", () => {
        assert.deepEqual(decideNewcomerOutputRoute(input({ unknownCount: 0 })), {
            ok: false,
            reason: "multi-unknown",
        });
        assert.deepEqual(decideNewcomerOutputRoute(input({ unknownCount: 3 })), {
            ok: false,
            reason: "multi-unknown",
        });
    });

    it("refuses same-output births (workspace placement is never fought)", () => {
        const decision = decideNewcomerOutputRoute(input({ newcomerOutput: FOCUSED.output }));
        assert.deepEqual(decision, { ok: false, reason: "same-output" });
    });

    it("refuses while a flight holds the single-flight (no fight with in-flight work)", () => {
        const decision = decideNewcomerOutputRoute(input({ flightBusy: true }));
        assert.deepEqual(decision, { ok: false, reason: "flight-busy" });
    });

    it("never routes born fullscreen or maximized overlays", () => {
        assert.deepEqual(decideNewcomerOutputRoute(input({ flags: { ...ORDINARY, fullscreen: true } })), {
            ok: false,
            reason: "fullscreen",
        });
        assert.deepEqual(decideNewcomerOutputRoute(input({ flags: { ...ORDINARY, maximized: true } })), {
            ok: false,
            reason: "maximized",
        });
    });

    it("never routes floating, sticky, or fixed-size (game-classified) windows", () => {
        assert.deepEqual(decideNewcomerOutputRoute(input({ flags: { ...ORDINARY, floating: true } })), {
            ok: false,
            reason: "floating",
        });
        assert.deepEqual(decideNewcomerOutputRoute(input({ flags: { ...ORDINARY, sticky: true } })), {
            ok: false,
            reason: "sticky",
        });
        assert.deepEqual(
            decideNewcomerOutputRoute(input({ flags: { ...ORDINARY, fixedAuto: true } })),
            { ok: false, reason: "fixed-size" },
        );
        assert.deepEqual(
            decideNewcomerOutputRoute(input({ flags: { ...ORDINARY, fixedSuppress: true } })),
            { ok: false, reason: "fixed-size" },
        );
    });
});

// ---------- applier harness ----------

interface ProofState {
    output: string;
    workspace: string;
    fullscreen: boolean;
    maximized: boolean;
}

function mockEnv(over: Partial<NewcomerRouteEnv & { calls: string[] }> & { proof?: ProofState | null; liveOutput?: string | null } = {}): { env: NewcomerRouteEnv; calls: string[] } {
    const calls: string[] = over.calls ?? [];
    const proof: ProofState | null = over.proof === undefined
        ? { output: FOCUSED.output, workspace: FOCUSED.workspace, fullscreen: false, maximized: false }
        : over.proof;
    const liveOutput: string | null = over.liveOutput === undefined ? POINTER.output : over.liveOutput;
    const outputObj = {};
    const desktopObj = {};
    const env: NewcomerRouteEnv = {
        resolveOutput: (name) => {
            calls.push(`resolveOutput:${name}`);
            return name === FOCUSED.output ? outputObj : null;
        },
        resolveDesktop: (workspace) => {
            calls.push(`resolveDesktop:${workspace}`);
            return workspace === FOCUSED.workspace ? desktopObj : null;
        },
        readOutputName: () => {
            calls.push("readOutputName");
            return liveOutput;
        },
        readDesktopIds: () => {
            calls.push("readDesktopIds");
            return [FOCUSED.workspace];
        },
        sendClientToScreen: () => {
            calls.push("sendClientToScreen");
            return true;
        },
        setDesktops: () => {
            calls.push("setDesktops");
            return true;
        },
        reobserve: () => {
            calls.push("reobserve");
            return proof;
        },
        setActive: () => {
            calls.push("setActive");
            return true;
        },
        ...stripHarnessKeys(over),
    };
    return { env, calls };
}

function stripHarnessKeys(over: Record<string, unknown>): Record<string, unknown> {
    const { calls: _calls, proof: _proof, liveOutput: _live, ...rest } = over;
    void _calls;
    void _proof;
    void _live;
    return rest;
}

describe("applyNewcomerOutputRoute (ordered writes with proof before focus)", () => {
    it("routes with transfer, membership, proof, then exactly one newcomer focus", () => {
        const { env, calls } = mockEnv();
        const outcome = applyNewcomerOutputRoute(env, {}, POINTER.output, FOCUSED);
        assert.equal(outcome, "routed");
        assert.deepEqual(calls, [
            `resolveOutput:${FOCUSED.output}`,
            `resolveDesktop:${FOCUSED.workspace}`,
            "readOutputName",
            "readDesktopIds",
            "sendClientToScreen",
            "setDesktops",
            "reobserve",
            "setActive",
        ]);
    });

    it("refuses the activation race with no writes past the pre-check", () => {
        const { env, calls } = mockEnv({ liveOutput: FOCUSED.output });
        const outcome = applyNewcomerOutputRoute(env, {}, POINTER.output, FOCUSED);
        assert.equal(outcome, "refused-race");
        assert.deepEqual(calls, [
            `resolveOutput:${FOCUSED.output}`,
            `resolveDesktop:${FOCUSED.workspace}`,
            "readOutputName",
            "readDesktopIds",
        ]);
    });

    it("refuses proof mismatch without focusing (never focus unproven placement)", () => {
        const { env, calls } = mockEnv({
            proof: { output: POINTER.output, workspace: POINTER.workspace, fullscreen: false, maximized: false },
        });
        const outcome = applyNewcomerOutputRoute(env, {}, POINTER.output, FOCUSED);
        assert.equal(outcome, "refused-proof");
        assert.ok(!calls.includes("setActive"));
    });

    it("refuses focus when the proof shows a fresh overlay (D01 native activation ownership)", () => {
        for (const overlay of [
            { output: FOCUSED.output, workspace: FOCUSED.workspace, fullscreen: true, maximized: false },
            { output: FOCUSED.output, workspace: FOCUSED.workspace, fullscreen: false, maximized: true },
        ]) {
            const { env, calls } = mockEnv({ proof: overlay });
            const outcome = applyNewcomerOutputRoute(env, {}, POINTER.output, FOCUSED);
            assert.equal(outcome, "refused-proof");
            assert.ok(!calls.includes("setActive"));
        }
    });

    it("reports focus failure once with no retry or focus fighting", () => {
        let focuses = 0;
        const { env, calls } = mockEnv({
            setActive: () => {
                calls.push("setActive");
                focuses += 1;
                return false;
            },
        });
        const outcome = applyNewcomerOutputRoute(env, {}, POINTER.output, FOCUSED);
        assert.equal(outcome, "routed-focus-failed");
        assert.equal(focuses, 1);
    });

    it("refuses before any write when the transfer primitive is absent", () => {
        const { env, calls } = mockEnv({ sendClientToScreen: undefined });
        const outcome = applyNewcomerOutputRoute(env, {}, POINTER.output, FOCUSED);
        assert.equal(outcome, "refused-no-transfer");
        assert.deepEqual(calls, [
            `resolveOutput:${FOCUSED.output}`,
            `resolveDesktop:${FOCUSED.workspace}`,
            "readOutputName",
            "readDesktopIds",
        ]);
    });

    it("refuses unreadable targets before any native write", () => {
        const noOutputCalls: string[] = [];
        const noOutput = mockEnv({
            calls: noOutputCalls,
            resolveOutput: (name: string) => {
                noOutputCalls.push(`resolveOutput:${name}`);
                return null;
            },
        });
        assert.equal(applyNewcomerOutputRoute(noOutput.env, {}, POINTER.output, FOCUSED), "refused-no-output");
        assert.deepEqual(noOutputCalls, [`resolveOutput:${FOCUSED.output}`]);

        const noDesktopCalls: string[] = [];
        const noDesktop = mockEnv({
            calls: noDesktopCalls,
            resolveDesktop: (workspace: string) => {
                noDesktopCalls.push(`resolveDesktop:${workspace}`);
                return null;
            },
        });
        assert.equal(applyNewcomerOutputRoute(noDesktop.env, {}, POINTER.output, FOCUSED), "refused-no-desktop");
        assert.ok(!noDesktopCalls.includes("setDesktops"));
        assert.ok(!noDesktopCalls.includes("setActive"));
    });
});
