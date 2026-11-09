// Genuine-producer output disconnect and return journeys through the real
// production entry (`startWorldAdapterEntry`) against the retained
// `broker_world_eval` broker (real core via WorldBridge). The shared
// harness (`world-producer-harness`) grows a tier 3B additive surface:
// an `outputs` seed (default stays the single `out` screen,
// observably identical on exercised paths), per-window `homes`, per-output `currents`, a
// `sendClientToScreen` echo mirroring the migration-producer fixture, a
// per-output work area, and `removeOutput`/`returnOutput` helpers modelling
// pinned KWin exactly (sibling `src/workspace.cpp` rearrange plus
// geometry-driven `setOutput(outputAt(center))`: membership never moves on
// removal, only the output name, with per-window `outputChanged` and one
// workspace `screensChanged`; removal drops the output current and a
// returned output re-inits to the first desktop per sibling
// `src/virtualdesktops.cpp`). Tier 3B covers two outputs each with tiled
// windows, removal of one output, and return under a renamed live handle
// carrying the original physical identity (offline physical-identity
// return). Every expectation comes from the real core reply through
// `appliedMatchesReply`, never assumed geometry. Focus placement on
// displacement is selected (user decision 2026-11-09): rehomed members are
// not fresh admissions, so no focus effect and no native activation.
// Included in `npm test`.

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    appliedMatchesReply,
    deskById,
    discover,
    frameOf,
    lastDescribeReply,
    makeHarness,
    replyFrames,
    takeReadys,
    winById,
    type Harness,
} from "./world-producer-harness";

const OUTPUTS = [
    { name: "out-left", geometry: { x: 0, y: 0, width: 1000, height: 800 }, manufacturer: "Acme", model: "Panel-L", serialNumber: "SN-LEFT-1" },
    { name: "out-right", geometry: { x: 1000, y: 50, width: 900, height: 700 }, manufacturer: "Acme", model: "Panel-R", serialNumber: "SN-RIGHT-1" },
];

async function makeTwoOutputHarness(focusId: string): Promise<Harness> {
    const h = await makeHarness(focusId, ["win-a", "win-b", "win-c", "win-d"], {
        outputs: OUTPUTS,
        desktops: ["ws1", "ws2"],
        currents: { "out-left": "ws1", "out-right": "ws2" },
        homes: { "win-a": "out-left", "win-b": "out-left", "win-c": "out-right", "win-d": "out-right" },
    });
    // Right members home to the right current before discovery, mirroring
    // the workspace-selection journeys' pre-discovery re-home.
    winById(h, "win-c").desktops = [deskById(h, "ws2")];
    winById(h, "win-d").desktops = [deskById(h, "ws2")];
    return h;
}

function pagerWins(reply: Record<string, unknown>): string[] {
    const presentation = reply["presentation"] as Record<string, unknown>;
    const pager = presentation["pager"] as Array<Record<string, unknown>>;
    const ids: string[] = [];
    for (const out of pager) {
        for (const cell of (out["workspaces"] as Array<Record<string, unknown>>)) {
            for (const row of (cell["windows"] as Array<Record<string, unknown>>)) {
                ids.push(row["win"] as string);
            }
        }
    }
    return ids.sort();
}

function windowRows(reply: Record<string, unknown>): Array<Record<string, unknown>> {
    return reply["windows"] as Array<Record<string, unknown>>;
}

function rowByWin(reply: Record<string, unknown>, win: string): Record<string, unknown> {
    const found = windowRows(reply).find((w) => w["win"] === win);
    assert.ok(found !== undefined, `reply carries a window row for ${win}`);
    return found;
}

describe("world genuine-producer output disconnect and return journeys", () => {
    it("relocates a disconnected output's members onto the survivor with no command traffic", async () => {
        const h = await makeTwoOutputHarness("win-c");
        try {
            await discover(h);
            await h.settle();
            const beforeA = { ...frameOf(winById(h, "win-a")) };
            const beforeB = { ...frameOf(winById(h, "win-b")) };
            const focusedBefore = h.focused.length;
            const callsBefore = h.calls.length;
            h.removeOutput("out-right");
            h.runTimers();
            await h.settle();
            const reply = lastDescribeReply(h);
            assert.equal(reply["outcome"] ?? "accepted", "accepted", "disconnect capture accepted");
            // Published roster exact: the live roster, no more and no fewer.
            const ids = windowRows(reply).map((w) => w["win"] as string).sort();
            assert.deepEqual(ids, ["win-a", "win-b", "win-c", "win-d"], "published window set exact");
            // Every row re-homes to the survivor's same-native unit: a/b
            // stay on ws-1, the displaced c/d join the empty ws-2.
            for (const win of ["win-a", "win-b"]) {
                assert.equal(rowByWin(reply, win)["out"], "out-left", `${win} re-homes to the survivor`);
                assert.equal(rowByWin(reply, win)["ws"], "ws-1", `${win} keeps its workspace`);
            }
            for (const win of ["win-c", "win-d"]) {
                assert.equal(rowByWin(reply, win)["out"], "out-left", `${win} re-homes to the survivor`);
                assert.equal(rowByWin(reply, win)["ws"], "ws-2", `${win} joins the survivor's same-native unit`);
            }
            // Native output reassignment is KWin-complete before the core
            // reply lands: every live window names the survivor.
            assert.deepEqual(
                h.wins.map((w) => w.output.name).sort(),
                ["out-left", "out-left", "out-left", "out-left"],
                "all natives name the survivor output",
            );
            appliedMatchesReply(h, reply);
            // Survivor frames pixel-identical: the row is never merged.
            assert.deepEqual(frameOf(winById(h, "win-a")), beforeA, "survivor win-a keeps its exact frame");
            assert.deepEqual(frameOf(winById(h, "win-b")), beforeB, "survivor win-b keeps its exact frame");
            // Presentation set exact: the pager carries the full roster.
            assert.deepEqual(pagerWins(reply), ["win-a", "win-b", "win-c", "win-d"], "pager carries the full roster");
            // Commandless displacement: no queue filing, no take, null result.
            assert.equal(h.calls.filter((call) => call.method === "QueueWorldCommand").length, 0, "displacement files no command");
            assert.equal(takeReadys(h).length, 0, "displacement settles no takes");
            assert.equal(reply["result"], null, "actuator lane carries a null result");
            // Rehomed members are not fresh admissions: no automatic-admission
            // focus effect, and native focus stays untouched.
            // Scan every capture the disconnect produced: the effect would
            // ride the first one, which a quiescent follow-up capture hides.
            const disconnectEffects: unknown[] = [];
            for (const [at, raw] of h.replies) {
                if (at >= callsBefore && (h.calls[at] as { method: string }).method === "DescribeWorld") {
                    disconnectEffects.push(...(((JSON.parse(raw) as Record<string, unknown>)["effects"] as unknown[] | undefined) ?? []));
                }
            }
            assert.deepEqual(disconnectEffects, [], "displacement mints no focus effect");
            assert.equal(h.focused.length, focusedBefore, "displacement activates no native window");
            assert.equal(h.activeRef?.internalId, "win-c", "native focus stays on the focused rehomed window");
            // No native writes: the adapter moves nothing itself.
            assert.deepEqual(h.viewWrites, [], "displacement writes no native view");
            assert.deepEqual(h.outputWrites, [], "displacement drives no output move");
            assert.deepEqual(h.echoWrites, [], "displacement re-homes no echo desktop");
            // Sensitivity control: a stale output name must fail loudly,
            // proving the output assertions above are not vacuous.
            const tampered = JSON.parse(JSON.stringify(reply)) as Record<string, unknown>;
            (windowRows(tampered).find((w) => w["win"] === "win-c") as Record<string, unknown>)["out"] = "out-right";
            assert.throws(() => appliedMatchesReply(h, tampered), /native output of/, "stale output fails the oracle");
        } finally {
            await h.stop();
        }
    });

    it("returns the removed output under a renamed handle and reclaims only its then-current empty units", async () => {
        const h = await makeTwoOutputHarness("win-c");
        try {
            await discover(h);
            await h.settle();
            h.removeOutput("out-right");
            h.runTimers();
            await h.settle();
            const removedReply = lastDescribeReply(h);
            const removedFrames = replyFrames(removedReply);
            h.returnOutput({ name: "out-right2", geometry: { x: 1000, y: 50, width: 900, height: 700 }, manufacturer: "Acme", model: "Panel-R", serialNumber: "SN-RIGHT-1" });
            h.runTimers();
            await h.settle();
            const reply = lastDescribeReply(h);
            assert.equal(reply["outcome"] ?? "accepted", "accepted", "return capture accepted");
            // Roster exact, and nobody pulls back: every row stays on the
            // survivor with its post-removal workspace and frame.
            const ids = windowRows(reply).map((w) => w["win"] as string).sort();
            assert.deepEqual(ids, ["win-a", "win-b", "win-c", "win-d"], "published window set exact");
            for (const win of ["win-a", "win-b", "win-c", "win-d"]) {
                assert.equal(rowByWin(reply, win)["out"], "out-left", `${win} stays on the survivor`);
                assert.equal(
                    rowByWin(reply, win)["ws"],
                    rowByWin(removedReply, win)["ws"],
                    `${win} keeps its post-removal workspace`,
                );
            }
            assert.deepEqual(replyFrames(reply), removedFrames, "return keeps every post-removal frame");
            for (const win of ["win-a", "win-b", "win-c", "win-d"]) {
                assert.deepEqual(
                    frameOf(winById(h, win)),
                    removedFrames.get(win),
                    `native frame of ${win} matches its post-removal frame`,
                );
            }
            assert.deepEqual(
                h.wins.map((w) => w.output.name).sort(),
                ["out-left", "out-left", "out-left", "out-left"],
                "no native moves back to the returned output",
            );
            appliedMatchesReply(h, reply);
            // The reclaimed trio rides empty on the returned output: ws-3,
            // ws-4 and ws-5 appear exactly once across outputs, all unbound.
            const units = reply["workspaces"] as Array<Record<string, unknown>>;
            for (const trio of ["ws-3", "ws-4", "ws-5"]) {
                const rows = units.filter((u) => u["id"] === trio);
                assert.equal(rows.length, 1, `${trio} appears exactly once across outputs`);
                assert.equal(rows[0]?.["out"], "out-right2", `${trio} reclaims onto the returned output`);
                assert.equal(rows[0]?.["bind"], "", `${trio} reclaims unbound`);
            }
            // Left view unchanged: the survivor still shows ws-1.
            const outputs = reply["outputs"] as Array<Record<string, unknown>>;
            assert.equal(outputs.find((o) => o["out"] === "out-left")?.["view"], "ws-1", "survivor still shows ws-1");
            assert.ok(outputs.some((o) => o["out"] === "out-right2"), "returned output publishes a row");
            // Presentation set exact: the pager carries the full roster.
            assert.deepEqual(pagerWins(reply), ["win-a", "win-b", "win-c", "win-d"], "pager carries the full roster");
            // Commandless return: no queue filing, no take, null result.
            assert.equal(h.calls.filter((call) => call.method === "QueueWorldCommand").length, 0, "return files no command");
            assert.equal(takeReadys(h).length, 0, "return settles no takes");
            assert.equal(reply["result"], null, "actuator lane carries a null result");
            // No native writes: the adapter moves nothing itself.
            assert.deepEqual(h.viewWrites, [], "return writes no native view");
            assert.deepEqual(h.outputWrites, [], "return drives no output move");
            assert.deepEqual(h.echoWrites, [], "return re-homes no echo desktop");
            // Sensitivity control: a stale pulled-back output must fail
            // loudly, proving the no-pullback assertions are not vacuous.
            const tampered = JSON.parse(JSON.stringify(reply)) as Record<string, unknown>;
            (windowRows(tampered).find((w) => w["win"] === "win-a") as Record<string, unknown>)["out"] = "out-right2";
            assert.throws(() => appliedMatchesReply(h, tampered), /native output of/, "stale output fails the oracle");
        } finally {
            await h.stop();
        }
    });
});
