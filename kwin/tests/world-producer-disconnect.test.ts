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
    takeReadys,
    winById,
    type Harness,
} from "./world-producer-harness";

const OUTPUTS = [
    { name: "out-left", geometry: { x: 0, y: 0, width: 1000, height: 800 }, manufacturer: "Acme", model: "Panel-L", serialNumber: "SN-LEFT-1" },
    { name: "out-right", geometry: { x: 1000, y: 50, width: 900, height: 700 }, manufacturer: "Acme", model: "Panel-R", serialNumber: "SN-RIGHT-1" },
];

async function makeTwoOutputHarness(focusId: string, shared = true): Promise<Harness> {
    const h = await makeHarness(focusId, ["win-a", "win-b", "win-c", "win-d"], {
        outputs: OUTPUTS,
        desktops: ["ws1", "ws2"],
        currents: { "out-left": "ws1", "out-right": "ws2" },
        homes: { "win-a": "out-left", "win-b": "out-left", "win-c": "out-right", "win-d": "out-right" },
        // Displacement mints separate logical units through the real
        // native-creation mechanism; the faithful opt-in lets the
        // adapter create the backing so quiescence converges.
        withNativeCreation: true,
        ...(shared ? {} : { sharedNativeRoster: false }),
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

// Test-only handshake-capability seam: runs the genuine entry discovery
// pump but downgrades the producer handshake's sharedNativeRoster bit in
// the encoded bytes before the broker binds the engine. The observation/
// capture route stays genuine; only the capability bit is rewritten, and
// the production value (always true) is asserted before the rewrite so a
// production change fails loudly instead of silently testing the wrong
// configuration. Never a claim of genuinely native KDE behavior: this
// exercises the explicit independent-roster path only.
async function discoverAs(h: Harness, shared: boolean): Promise<void> {
    const hsIndex = h.calls.findIndex((call) => call.method === "DiscoverWorld");
    assert.ok(hsIndex >= 0, "entry discovers on start");
    const sent = JSON.parse((h.calls[hsIndex] as { payload: string }).payload) as Record<string, unknown>;
    assert.equal(sent["type"], "handshake");
    const caps = (sent["capabilities"] as Record<string, unknown>);
    assert.equal(caps["sharedNativeRoster"], shared);
    if (!shared) {
        caps["sharedNativeRoster"] = false;
    }
    const payload = JSON.stringify(sent);
    const hsReplyText = await h.bridge.send("DiscoverWorld", payload);
    const hsReply = JSON.parse(hsReplyText) as Record<string, unknown>;
    assert.equal(hsReply["type"], "handshake-reply");
    assert.equal(hsReply["ready"], false);
    (h.calls[hsIndex] as { callback: (r: string) => void }).callback(hsReplyText);
    const gen = hsReply["gen"] as string;
    const setIndex = h.calls.findIndex((call, at) => at > hsIndex && call.method === "DiscoverWorld");
    assert.ok(setIndex >= 0, "revisioned settings follow the handshake");
    const setReplyText = await h.bridge.send("DiscoverWorld", (h.calls[setIndex] as { payload: string }).payload);
    const setReply = JSON.parse(setReplyText) as Record<string, unknown>;
    assert.equal(setReply["type"], "settings-reply");
    assert.equal(setReply["status"], "accepted");
    assert.equal(setReply["gen"], gen);
    (h.calls[setIndex] as { callback: (r: string) => void }).callback(setReplyText);
    await h.settle();
}

describe("world genuine-producer output disconnect and return journeys", () => {
    it("relocates a disconnected output's members onto the survivor with no command traffic", async () => {
        const h = await makeTwoOutputHarness("win-c");
        try {
            await discover(h);
            await h.settle();
            const before = lastDescribeReply(h);
            assert.equal(before["rosterPlan"], undefined, "bootstrap has settled before unplug");
            const survivor = rowByWin(before, "win-a")["ws"];
            const displaced = rowByWin(before, "win-c")["ws"];
            assert.ok(typeof survivor === "string" && typeof displaced === "string" && survivor !== displaced);
            const nativeBefore = (h.workspace["desktops"] as Array<{ id: string }>).map((d) => d.id);
            const viewsBefore = h.viewWrites.length;
            const current = h.workspace["currentDesktopForScreen"] as (screen: { name: string }) => { id: string };
            const survivorCurrent = current({ name: "out-left" }).id;
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
            // Preserve both logical homes, independently of native slot
            // assignment after the right-hand ring joins the survivor.
            for (const win of ["win-a", "win-b"]) {
                assert.equal(rowByWin(reply, win)["out"], "out-left", `${win} re-homes to the survivor`);
                assert.equal(rowByWin(reply, win)["ws"], survivor, `${win} keeps its workspace`);
            }
            for (const win of ["win-c", "win-d"]) {
                assert.equal(rowByWin(reply, win)["out"], "out-left", `${win} re-homes to the survivor`);
                assert.equal(rowByWin(reply, win)["ws"], displaced, `${win} keeps its distinct displaced workspace`);
            }
            const order = (h.workspace["desktops"] as Array<{ id: string }>).map((d) => d.id);
            const ring = (reply["workspaces"] as Array<Record<string, unknown>>)
                .sort((a, b) => (a["order"] as number) - (b["order"] as number));
            assert.ok(ring.every((row) => row["out"] === "out-left" && row["exist"] !== false),
                "only the survivor's real rows remain");
            assert.deepEqual(ring.map((row) => row["bind"]), order, "every real row binds its ordered native slot");
            assert.deepEqual(order.slice(0, nativeBefore.length), nativeBefore, "growth does not replace existing slots");
            assert.equal(new Set(order).size, order.length, "the grown roster has distinct native ids");
            const survivorSlot = ring.findIndex((row) => row["id"] === survivor);
            const displacedSlot = ring.findIndex((row) => row["id"] === displaced);
            assert.equal(survivorSlot, 0, "the survivor's occupied row stays first");
            assert.ok(displacedSlot > survivorSlot && displacedSlot < nativeBefore.length,
                "the distinct displaced row uses an existing slot after the survivor's occupied row");
            const displacedBacking = order[displacedSlot];
            const trailing = ring[ring.length - 1];
            assert.equal(trailing?.["trailing"], true, "the new tail is a real empty spare");
            assert.ok(trailing?.["id"] !== displaced && trailing?.["id"] !== survivor);
            assert.ok(!h.wins.some((win) => win.desktops.some((desk) => desk.id === trailing?.["bind"])),
                "the backed trailing spare is natively empty");
            for (const win of ["win-c", "win-d"]) {
                assert.deepEqual(
                    winById(h, win).desktops.map((d) => d.id),
                    [displacedBacking],
                    `native membership of ${win} uses the assigned slot`,
                );
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
            // Displacement uses the native apply lane, never the command lane.
            assert.equal(h.calls.filter((call) => call.method === "QueueWorldCommand").length, 0, "displacement files no command");
            assert.equal(takeReadys(h).length, 0, "displacement settles no takes");
            assert.equal("result" in reply, false, "actuator lane omits result");
            // Settled quiescence leaks nothing: no in-flight transfer and no
            // outstanding creation desire ride the publication.
            assert.deepEqual(reply["transfers"] ?? [], [], "displacement leaves no in-flight transfer");
            assert.deepEqual(reply["nativeCreations"] ?? [], [], "displacement leaves no outstanding creation");
            assert.equal(reply["rosterPlan"], undefined, "the native slot plan settles");
            // Rehomed members are not fresh admissions: no automatic-admission
            // focus effect, and native focus stays untouched.
            // Scan every capture the disconnect produced: the effect would
            // ride the first one, which a quiescent follow-up capture hides.
            const disconnectReplies = [...h.replies]
                .filter(([at]) => at >= callsBefore && h.calls[at]?.method === "DescribeWorld")
                .map(([, raw]) => JSON.parse(raw) as Record<string, unknown>);
            const plans = disconnectReplies.flatMap((entry) => entry["rosterPlan"] ? [entry["rosterPlan"]] : []);
            assert.ok(plans.length > 0, "topology changes publish actual native slot authority");
            const transfers = disconnectReplies.flatMap((entry) => (entry["transfers"] as Array<Record<string, unknown>>) ?? []);
            assert.ok(transfers.length > 0, "displacement keeps commandless provenance until settled");
            assert.ok(transfers.every((transfer) => transfer["cmdSeq"] === null), "no policy step invents a command sequence");
            assert.ok(disconnectReplies.every((entry) => !("effects" in entry) && entry["focusRequest"] === null),
                "displacement mints no focus request and omits retired effects");
            assert.equal(h.focused.length, focusedBefore, "displacement activates no native window");
            assert.equal(h.activeRef?.internalId, "win-c", "native focus stays on the focused rehomed window");
            assert.equal(current({ name: "out-left" }).id, survivorCurrent, "the survivor's native current is unchanged");
            assert.deepEqual(h.viewWrites.slice(viewsBefore), [], "displacement writes no native view");
            assert.deepEqual(h.outputWrites, [], "displacement drives no output move");
            assert.deepEqual(h.echoWrites, [], "displacement re-homes no echo desktop");
            const memberships = h.wins.map((win) => win.desktops);
            const frameWrites = h.geometryWrites.length;
            const viewWrites = h.viewWrites.length;
            h.fire("screensChanged");
            await h.settle();
            for (const [i, win] of h.wins.entries()) {
                assert.equal(win.desktops, memberships[i], `${win.internalId} receives no replayed membership write`);
            }
            assert.equal(h.geometryWrites.length, frameWrites, "no frame replay on settled observation");
            assert.equal(h.viewWrites.length, viewWrites, "no view replay on settled observation");
            assert.equal(h.focused.length, focusedBefore, "no focus replay on settled observation");
            assert.deepEqual((h.workspace["desktops"] as Array<{ id: string }>).map((d) => d.id), order,
                "no roster mutation on settled observation");
            assert.equal(h.calls.filter((call) => call.method === "QueueWorldCommand").length, 0);
            assert.equal(takeReadys(h).length, 0);
        } finally {
            await h.stop();
        }
    });

    it("returns the removed output under a renamed handle and reclaims its then-current units", async () => {
        const h = await makeTwoOutputHarness("win-c");
        try {
            await discover(h);
            await h.settle();
            const startup = lastDescribeReply(h);
            assert.equal(startup["rosterPlan"], undefined, "startup compaction settles");
            const startupSlots = [...h.replies]
                .filter(([at]) => h.calls[at]?.method === "DescribeWorld")
                .map(([, raw]) => (JSON.parse(raw) as Record<string, unknown>)["rosterPlan"])
                .filter((plan) => plan !== undefined) as Array<{
                    members: Array<{ win: string; out: string }>;
                    views: Array<{ out: string }>;
                }>;
            assert.deepEqual([...new Set(startupSlots.flatMap((plan) =>
                plan.members.map((step) => `${step.win}->${step.out}`)))].sort(),
            ["win-c->out-right", "win-d->out-right"], "startup compacts exactly the two right occupants in place");
            assert.deepEqual([...new Set(startupSlots.flatMap((plan) => plan.views.map((step) => step.out)))],
                ["out-right"], "startup changes exactly the right selector");
            assert.deepEqual(h.viewWrites, ["out-right->ws1"], "right current compacts to the first live slot");
            appliedMatchesReply(h, startup);
            const startupMembers = h.wins.map((win) => win.desktops);
            const startupWrites = [h.viewWrites.length, h.outputWrites.length, h.geometryWrites.length, h.focused.length];
            const startupOrder = (h.workspace["desktops"] as Array<{ id: string }>).map((desk) => desk.id);
            h.fire("screensChanged");
            await h.settle();
            for (const [i, win] of h.wins.entries()) {
                assert.equal(win.desktops, startupMembers[i], "settled startup does not replay membership");
            }
            assert.deepEqual([h.viewWrites.length, h.outputWrites.length, h.geometryWrites.length, h.focused.length],
                startupWrites, "settled startup does not replay native setters");
            assert.deepEqual((h.workspace["desktops"] as Array<{ id: string }>).map((desk) => desk.id), startupOrder);
            const viewWritesBeforeHotplug = h.viewWrites.length;
            h.removeOutput("out-right");
            h.runTimers();
            await h.settle();
            const removedReply = lastDescribeReply(h);
            const callsBefore = h.calls.length;
            const focusedBefore = h.focused.length;
            h.returnOutput({ name: "out-right2", geometry: { x: 1000, y: 50, width: 900, height: 700 }, manufacturer: "Acme", model: "Panel-R", serialNumber: "SN-RIGHT-1" });
            h.runTimers();
            await h.settle();
            const reply = lastDescribeReply(h);
            assert.equal(reply["outcome"] ?? "accepted", "accepted", "return capture accepted");
            // Roster exact, with then-current contents: a/b never left and
            // stay on the survivor, while the displaced c/d ride their own
            // unit home to the returned output (the same logical unit as
            // after removal, now hosted on out-right2). Frames re-tile
            // across the output move, so the oracle below (exact native
            // frame match against the plan) replaces any frame-identity
            // assertion against the post-removal capture.
            const ids = windowRows(reply).map((w) => w["win"] as string).sort();
            assert.deepEqual(ids, ["win-a", "win-b", "win-c", "win-d"], "published window set exact");
            for (const win of ["win-a", "win-b"]) {
                assert.equal(rowByWin(reply, win)["out"], "out-left", `${win} stays on the survivor`);
                assert.equal(
                    rowByWin(reply, win)["ws"],
                    rowByWin(removedReply, win)["ws"],
                    `${win} keeps its post-removal workspace`,
                );
            }
            for (const win of ["win-c", "win-d"]) {
                assert.equal(rowByWin(reply, win)["out"], "out-right2", `${win} rides home to the returned output`);
                assert.equal(
                    rowByWin(reply, win)["ws"],
                    rowByWin(removedReply, win)["ws"],
                    `${win} keeps its displaced unit across the return`,
                );
            }
            assert.deepEqual(
                h.wins.map((w) => w.output.name).sort(),
                ["out-left", "out-left", "out-right2", "out-right2"],
                "displaced natives move back to the returned output",
            );
            appliedMatchesReply(h, reply);
            // The displaced unit reclaims bound, not empty: c/d's
            // then-current unit publishes on the returned output bound to a
            // live native, and that native is exactly the adapter-written
            // membership (direct bound write, never an echo re-home).
            const units = reply["workspaces"] as Array<Record<string, unknown>>;
            for (const win of ["win-c", "win-d"]) {
                const home = rowByWin(reply, win)["ws"] as string;
                const rows = units.filter((u) => u["id"] === home && u["out"] === "out-right2");
                assert.equal(rows.length, 1, `${win}'s unit publishes exactly once on the returned output`);
                const bind = rows[0]?.["bind"] as string;
                assert.ok(bind !== "", `${win}'s unit reclaims bound`);
                assert.deepEqual(
                    winById(h, win).desktops.map((d) => d.id),
                    [bind],
                    `native membership of ${win} matches its reclaimed bind`,
                );
            }
            // The survivor retains its own selected logical workspace.
            const outputs = reply["outputs"] as Array<Record<string, unknown>>;
            assert.equal(outputs.find((o) => o["out"] === "out-left")?.["view"], rowByWin(startup, "win-a")["ws"],
                "survivor still shows its original logical workspace");
            assert.ok(outputs.some((o) => o["out"] === "out-right2"), "returned output publishes a row");
            // Presentation set exact: the pager carries the full roster.
            assert.deepEqual(pagerWins(reply), ["win-a", "win-b", "win-c", "win-d"], "pager carries the full roster");
            // Commandless return: no queue filing, no take, null result.
            assert.equal(h.calls.filter((call) => call.method === "QueueWorldCommand").length, 0, "return files no command");
            assert.equal(takeReadys(h).length, 0, "return settles no takes");
            assert.equal("result" in reply, false, "actuator lane omits result");
            // The adapter drives exactly the home move and nothing else:
            // one cross-output send per displaced window, no echo re-home,
            // no view switch. Settled quiescence leaks nothing.
            assert.deepEqual(h.viewWrites.slice(viewWritesBeforeHotplug), [], "hotplug and return write no native view");
            assert.deepEqual(h.outputWrites, ["win-c->out-right2", "win-d->out-right2"], "return drives exactly the home move");
            assert.deepEqual(h.echoWrites, [], "return re-homes no echo desktop");
            assert.deepEqual(reply["transfers"] ?? [], [], "return leaves no in-flight transfer");
            assert.deepEqual(reply["nativeCreations"] ?? [], [], "return leaves no outstanding creation");
            // Reclaimed members are not fresh admissions: no focus effect
            // rides the return captures, and no native activates.
            for (const [at, raw] of h.replies) {
                if (at >= callsBefore && (h.calls[at] as { method: string }).method === "DescribeWorld") {
                    const parsed = JSON.parse(raw) as Record<string, unknown>;
                    assert.equal("effects" in parsed, false, "return reply omits retired effects");
                    assert.equal(parsed["focusRequest"], null, "return mints no focus request");
                }
            }
            assert.equal(h.focused.length, focusedBefore, "return activates no native window");
            // Sensitivity control: a stale pulled-back output must fail
            // loudly, proving the no-pullback assertions are not vacuous.
            const tampered = JSON.parse(JSON.stringify(reply)) as Record<string, unknown>;
            (windowRows(tampered).find((w) => w["win"] === "win-a") as Record<string, unknown>)["out"] = "out-right2";
            assert.throws(() => appliedMatchesReply(h, tampered), /native output of/, "stale output fails the oracle");
        } finally {
            await h.stop();
        }
    });

    it("disconnects under the independent roster with no duplicate spare (independent variant)", async () => {
        // Same journey as the first test but through the test-only
        // sharedNativeRoster:false seam: the independent roster never
        // adopted the memberless orphan (its per-output prune dropped it
        // pre-disconnect), so this pins the independent converged shape
        // the shared path now matches: three live rows, three creations,
        // no outstanding desire.
        const h = await makeTwoOutputHarness("win-c", false);
        try {
            winById(h, "win-c").desktops = [deskById(h, "ws2")];
            winById(h, "win-d").desktops = [deskById(h, "ws2")];
            await discoverAs(h, false);
            h.removeOutput("out-right");
            h.runTimers();
            await h.settle();
            const reply = lastDescribeReply(h);
            assert.equal(reply["outcome"] ?? "accepted", "accepted", "disconnect capture accepted");
            const ids = windowRows(reply).map((w) => w["win"] as string).sort();
            assert.deepEqual(ids, ["win-a", "win-b", "win-c", "win-d"], "published window set exact");
            for (const win of ["win-a", "win-b"]) {
                assert.equal(rowByWin(reply, win)["out"], "out-left", `${win} re-homes to the survivor`);
                assert.equal(rowByWin(reply, win)["ws"], "ws-1", `${win} keeps its workspace`);
            }
            for (const win of ["win-c", "win-d"]) {
                assert.equal(rowByWin(reply, win)["out"], "out-left", `${win} re-homes to the survivor`);
                assert.equal(rowByWin(reply, win)["ws"], "ws-4", `${win} rides the separate displaced unit, never the survivor's own`);
            }
            assert.deepEqual(
                (h.workspace["desktops"] as Array<{ id: string }>).map((d) => d.id),
                ["ws1", "ws2", "ws-3", "ws-4", "ws-5"],
                "independent disconnect creates the displaced backing plus the eager trailing spare",
            );
            const live = (reply["workspaces"] as Array<Record<string, unknown>>)
                .filter((u) => u["out"] === "out-left" && u["exist"] !== false)
                .sort((a, b) => (a["order"] as number) - (b["order"] as number));
            assert.deepEqual(
                live.map((u) => `${u["id"]}@${u["bind"]}`),
                ["ws-1@ws1", "ws-4@ws-4", "ws-5@ws-5"],
                "independent converged ring holds three live rows",
            );
            appliedMatchesReply(h, reply);
            assert.deepEqual(reply["transfers"] ?? [], [], "displacement leaves no in-flight transfer");
            assert.deepEqual(reply["nativeCreations"] ?? [], [], "displacement leaves no outstanding creation");
            assert.equal("result" in reply, false, "actuator lane omits result");
        } finally {
            await h.stop();
        }
    });

    it("reconnects the leg-2 survivor-focus journey with a settled explicit send (P1 reproduction)", async () => {        // Exact 2026-11-09 leg-2 topology, no merge decision taken: A holds
        // T1 on ws1 with focus; B holds T2 on ws1 and F/T3 on ws2. Removal
        // re-homes every B window onto separate hosted units on A; T3 is
        // then sent to ws1 with settled membership before B returns under
        // a renamed handle carrying the same physical identity. Expected
        // return: F rides home to B, T2 rides home to B, T3 stays where
        // sent. Before correction F lost its return-eligible home through
        // the survivor's same-native receipt. T3 staying put distinguishes
        // the offline core path from live native
        // restoration (H3, unmodelled: returnOutput moves nothing back).
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c", "win-d"], {
            outputs: OUTPUTS,
            desktops: ["ws1", "ws2"],
            currents: { "out-left": "ws1", "out-right": "ws2" },
            homes: { "win-a": "out-left", "win-b": "out-right", "win-c": "out-right", "win-d": "out-right" },
            // Displacement mints separate logical units through the real
            // native-creation mechanism; the faithful opt-in lets the
            // adapter create the backing so quiescence converges.
            withNativeCreation: true,
        });
        try {
            winById(h, "win-b").desktops = [deskById(h, "ws1")];
            winById(h, "win-c").desktops = [deskById(h, "ws2")];
            winById(h, "win-d").desktops = [deskById(h, "ws2")];
            await discover(h);
            await h.settle();
            // Setup proof through live natives: T1 A/ws1, T2 B/ws1, F/T3
            // B/ws2, with the plan matching every native.
            assert.equal(winById(h, "win-a").output.name, "out-left", "T1 starts on A");
            assert.deepEqual(winById(h, "win-a").desktops.map((d) => d.id), ["ws1"], "T1 starts on ws1");
            assert.equal(winById(h, "win-b").output.name, "out-right", "T2 starts on B");
            assert.deepEqual(winById(h, "win-b").desktops.map((d) => d.id), ["ws1"], "T2 starts on ws1");
            for (const win of ["win-c", "win-d"]) {
                assert.equal(winById(h, win).output.name, "out-right", `${win} starts on B`);
                assert.deepEqual(winById(h, win).desktops.map((d) => d.id), ["ws2"], `${win} starts on ws2`);
            }
            appliedMatchesReply(h, lastDescribeReply(h));
            // Survivor-focus removal: T1 keeps native focus, no admission
            // focus effect for the rehomed members.
            const focusedBefore = h.focused.length;
            const callsBefore = h.calls.length;
            h.removeOutput("out-right");
            h.runTimers();
            await h.settle();
            const removedReply = lastDescribeReply(h);
            assert.equal(removedReply["outcome"] ?? "accepted", "accepted", "disconnect capture accepted");
            assert.deepEqual(
                windowRows(removedReply).map((w) => w["win"] as string).sort(),
                ["win-a", "win-b", "win-c", "win-d"],
                "published window set exact after removal",
            );
            assert.deepEqual(
                h.wins.map((w) => w.output.name).sort(),
                ["out-left", "out-left", "out-left", "out-left"],
                "all natives name the survivor output",
            );
            appliedMatchesReply(h, removedReply);
            for (const [at, raw] of h.replies) {
                if (at >= callsBefore && (h.calls[at] as { method: string }).method === "DescribeWorld") {
                    const parsed = JSON.parse(raw) as Record<string, unknown>;
                    assert.equal("effects" in parsed, false, "disconnect reply omits retired effects");
                    assert.equal(parsed["focusRequest"], null, "displacement mints no focus request");
                }
            }
            assert.equal(h.focused.length, focusedBefore, "displacement activates no native window");
            assert.equal(h.activeRef?.internalId, "win-a", "native focus stays on the survivor T1");
            assert.notEqual(rowByWin(removedReply, "win-b")["ws"], rowByWin(removedReply, "win-a")["ws"], "T2 keeps a separate displaced unit on the occupied native index");
            assert.equal(rowByWin(removedReply, "win-c")["ws"], rowByWin(removedReply, "win-d")["ws"], "F/T3 keep their shared displaced unit");
            const hosted = (removedReply["workspaces"] as Array<Record<string, unknown>>)
                .filter((row) => row["out"] === "out-left" && row["exist"] !== false)
                .sort((a, b) => (a["order"] as number) - (b["order"] as number));
            const floatHome = hosted.find((row) => row["id"] === rowByWin(removedReply, "win-c")["ws"]);
            const displacedTileHome = hosted.find((row) => row["id"] === rowByWin(removedReply, "win-b")["ws"]);
            assert.ok(floatHome?.["bind"] && displacedTileHome?.["bind"], "both displaced units have settled native slots");
            assert.notEqual(floatHome["bind"], displacedTileHome["bind"], "F/T3 and T2 retain distinct native backing");
            assert.deepEqual(winById(h, "win-c").desktops.map((d) => d.id), [floatHome["bind"]]);
            // L2 select position 2 through the production numbered route.
            // Freeze its logical identity before the command so a later
            // projection cannot make selection of the wrong row pass.
            const second = hosted[1]?.["id"];
            assert.equal(typeof second, "string", "reshaped ring holds a second real cell");
            h.activeScreen = { name: "out-left" };
            h.handle.requestWorkspaceSelect(2);
            // The drain dispatch gates on the async Queue filing ack, which
            // quiescent settle() never waits for: flush() bounds that wait,
            // then settle() quiesces the follow-up round-trips.
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.equal(h.commandPayloads("select-numbered").length, 1, "selection files exactly once");
            const currentFor = h.workspace["currentDesktopForScreen"] as (screen: { name: string }) => { id: string };
            const selected = lastDescribeReply(h);
            const selectedOutput = (selected["outputs"] as Array<Record<string, unknown>>)
                .find((row) => row["out"] === "out-left");
            assert.equal(selectedOutput?.["view"], second, `numbered selection shows the original second logical cell: ${JSON.stringify({
                plan: selected["rosterPlan"], transfers: selected["transfers"], take: takeReadys(h), writes: h.viewWrites,
                activity: h.logs.slice(-20),
            })}`);
            const selectedSlot = (selected["workspaces"] as Array<Record<string, unknown>>)
                .find((row) => row["out"] === "out-left" && row["id"] === second)?.["bind"];
            assert.equal(typeof selectedSlot, "string");
            assert.equal(
                currentFor({ name: "out-left" }).id,
                selectedSlot,
                "numbered select lands on the live ring's second cell",
            );
            const selectTakes = takeReadys(h);
            assert.equal(selectTakes.length, 1, "selection settles its own take");
            assert.equal(selectTakes[0]?.["globalCmd"], "1", "selection take rides sequence 1");
            appliedMatchesReply(h, lastDescribeReply(h));
            // L2 focus: the user selects T3. Native activation through the
            // supported workspace signal, read back below.
            h.activeRef = winById(h, "win-d");
            h.fire("windowActivated");
            h.runTimers();
            await h.settle();
            assert.equal(h.activeRef?.internalId, "win-d", "T3 holds native focus before the send");
            // L2 send: T3 to T1's logical workspace with follow, then prove
            // the resulting native membership settled before return.
            h.handle.requestWorkspaceMove(1, true);
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.equal(h.commandPayloads("send-numbered").length, 1, "send files exactly once");
            const sendTakes = takeReadys(h);
            assert.equal(sendTakes.length, 2, "send settles its own take after the selection take");
            assert.equal(sendTakes[1]?.["globalCmd"], "2", "send take rides sequence 2");
            assert.equal(sendTakes[1]?.["result"], null, "send take resolves null");
            const destination = rowByWin(removedReply, "win-a")["ws"];
            const sentReply = lastDescribeReply(h);
            const destinationSlot = (sentReply["workspaces"] as Array<Record<string, unknown>>)
                .find((row) => row["out"] === "out-left" && row["id"] === destination)?.["bind"];
            assert.equal(typeof destinationSlot, "string");
            assert.equal(currentFor({ name: "out-left" }).id, destinationSlot, "follow selects T1's assigned native slot");
            assert.deepEqual(winById(h, "win-d").desktops.map((d) => d.id), [destinationSlot],
                "T3 native membership settles on T1's assigned slot");
            appliedMatchesReply(h, sentReply);
            const sentWs = rowByWin(sentReply, "win-d")["ws"] as string;
            assert.equal(rowByWin(sentReply, "win-a")["ws"], sentWs, "T3 settles onto T1's workspace");
            assert.equal(sentWs, destination, "the original destination logical identity survives the send");
            // Same-identity return under a renamed handle.
            h.returnOutput({ name: "out-right2", geometry: { x: 1000, y: 50, width: 900, height: 700 }, manufacturer: "Acme", model: "Panel-R", serialNumber: "SN-RIGHT-1" });
            h.runTimers();
            await h.settle();
            const reply = lastDescribeReply(h);
            assert.equal(reply["outcome"] ?? "accepted", "accepted", "return capture accepted");
            assert.deepEqual(
                windowRows(reply).map((w) => w["win"] as string).sort(),
                ["win-a", "win-b", "win-c", "win-d"],
                "published window set exact after return",
            );
            assert.ok((reply["outputs"] as Array<Record<string, unknown>>).some((o) => o["out"] === "out-right2"), "returned output publishes a row");
            // T3 is not pulled back: it stays where explicitly sent.
            assert.equal(rowByWin(reply, "win-d")["out"], "out-left", "sent T3 stays on the survivor");
            assert.equal(rowByWin(reply, "win-d")["ws"], sentWs, "sent T3 keeps its settled workspace");
            // The plan matches every live native at quiescence, so the
            // failure below is homing, not an unapplied native write.
            appliedMatchesReply(h, reply);
            // F rides home to B. Fails before the H1 correction: F rehomed
            // onto the survivor's own unit, so reclaiming B's emptied unit
            // cannot return it.
            assert.equal(rowByWin(reply, "win-c")["out"], "out-right2", "F returns to B");
            // T2 rides home to B (same H1 mechanism on the occupied unit).
            assert.equal(rowByWin(reply, "win-b")["out"], "out-right2", "T2 returns to B");
            // Settled quiescence leaks nothing: the select/send takes
            // resolved on the command lane, and no policy flight or
            // creation desire rides the return publication.
            assert.deepEqual(reply["transfers"] ?? [], [], "settled return leaves no in-flight transfer");
            assert.deepEqual(reply["nativeCreations"] ?? [], [], "settled return leaves no outstanding creation");
        } finally {
            await h.stop();
        }
    });
});
