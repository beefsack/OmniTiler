// Genuine-producer retained positive-close focus (v4): the issuing
// publication plus three unchanged re-emissions are minted by actual core
// report ingestion through the real `broker_world_eval` broker (never
// synthesized here); all four carry one identical closeFocus row while the
// reply revision advances. The issuing and both intermediate publications
// are SKIPPED (never passed to applyWorldReply); only the latest applies,
// with exactly one native setter and feedback carrying the immutable
// issuing rev (different from the latest publication rev). Two redeliveries
// of the latest stay silent. No live KWin; broker binaries run through the
// suite's execution-only launcher, never `cargo run`.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply, type ApplyEnv } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { parseWorldReply, isCoreWorldReply } from "../src/world-report";
import { makeHarness, discover, winById } from "./world-producer-harness";

type Obs = ApplyEnv["observation"];

function observedWin(win: string, ref: object, out: string, ws: string): Obs["domains"][number]["windows"][number] {
    return {
        win, ref, out, ws,
        rect: { x: 0, y: 0, w: 100, h: 100 },
        floating: false, fitExcluded: false,
        fullscreen: false, maximized: false, sticky: false,
        fixedAuto: false, fixedSuppress: false, nativeWs: null,
        hints: { minW: null, minH: null, maxW: null, maxH: null },
        minimized: false,
    };
}

function consumerObs(
    refs: Map<string, object>,
    wins: Array<{ win: string; out: string; ws: string }>,
    out: string,
    ws: string,
): Obs {
    return {
        mode: "per-output-local",
        topology: [{ out, order: [ws], current: ws, complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
        bindings: [],
        domains: [{
            id: out, ws, bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 0, focused: "", readable: true, hold: "",
            complete: true,
            windows: wins.map((w) => observedWin(w.win, refs.get(w.win) as object, w.out, w.ws)),
            closed: [],
        }],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        // The close capture proved this foreground: the host fallback had
        // already picked win-a while the frozen MRU survivor is win-c.
        focusedGlobal: "win-a",
        foregroundKnown: true,
        screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

async function awaitDispatch(h: Awaited<ReturnType<typeof makeHarness>>, from: number): Promise<number> {
    for (let round = 0; round < 200; round += 1) {
        const index = h.calls.findIndex((call, at) => at >= from && call.method === "DescribeWorld" && !h.answered.has(at));
        if (index >= 0) {
            return index;
        }
        h.runTimers();
        await new Promise<void>((resolve) => setTimeout(resolve, 0));
    }
    assert.ok(false, "expected the close capture to dispatch through the production broker");
    throw new Error("unreachable");
}

describe("world retained close focus genuine producer", () => {
    it("skips the issuing and intermediate publications; latest applies once with the issuing rev", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            await h.settle();
            // Focus history win-a -> win-c -> win-b, so the MRU survivor
            // excluding a closed win-b is win-c.
            h.activeRef = winById(h, "win-c");
            h.fire("windowActivated");
            h.runTimers();
            await h.settle();
            h.activeRef = winById(h, "win-b");
            h.fire("windowActivated");
            h.runTimers();
            await h.settle();
            // Positive close of the focused lifetime with the host fallback
            // already on win-a (never the frozen winner win-c).
            const at = h.wins.findIndex((w) => w.internalId === "win-b");
            assert.ok(at >= 0, "win-b present before close");
            const [removed] = h.wins.splice(at, 1);
            h.activeRef = winById(h, "win-a");
            const from = h.calls.length;
            h.fire("windowRemoved", removed);
            h.fire("windowActivated");
            h.runTimers();
            const dispatch = await awaitDispatch(h, from);
            assert.equal(h.activeRef?.internalId, "win-a", "close capture carries the win-a fallback");
            // The entry's own dispatch stays unanswered: the adapter never
            // observes these publications through its own apply path. All
            // four broker replies below come from actual report ingestion
            // of the entry's genuine bytes (the re-emissions only advance
            // the transport correlation, never the report facts).
            const envelope = JSON.parse(h.calls[dispatch]?.payload as string) as Record<string, unknown>;
            const seqBase = Number.parseInt(envelope["sourceSeq"] as string, 10);
            assert.ok(Number.isInteger(seqBase) && seqBase >= 1, "genuine capture carries a canonical sourceSeq");
            const requestPayloads: string[] = [h.calls[dispatch]?.payload as string];
            const rawReplies: string[] = [];
            rawReplies.push(await h.bridge.send("DescribeWorld", h.calls[dispatch]?.payload as string));
            for (let i = 1; i <= 3; i += 1) {
                const clone = {
                    ...(envelope as object),
                    source: `${envelope["source"] as string}-r${String(i + 1)}`,
                    sourceSeq: String(seqBase + i),
                };
                const payload = JSON.stringify(clone);
                requestPayloads.push(payload);
                rawReplies.push(await h.bridge.send("DescribeWorld", payload));
            }
            // All four requests are identical except the correlation-only
            // source/sourceSeq increments; the raw report facts never change.
            const stripped = requestPayloads.map((payload) => {
                const body = JSON.parse(payload) as Record<string, unknown>;
                const { source, sourceSeq, ...rest } = body;
                void source;
                void sourceSeq;
                return JSON.stringify(rest);
            });
            assert.equal(stripped[1], stripped[0], "first re-emission request identical");
            assert.equal(stripped[2], stripped[0], "second re-emission request identical");
            assert.equal(stripped[3], stripped[0], "third re-emission request identical");
            const replies = rawReplies.map((raw) => {
                const parsed = parseWorldReply(raw);
                assert.ok(parsed !== null && isCoreWorldReply(parsed), "genuine broker reply parses as a core reply");
                if (!isCoreWorldReply(parsed)) {
                    throw new Error("unreachable");
                }
                return parsed;
            });
            const closes = replies.map((r) => r.closeFocus);
            assert.ok(closes[0] !== null && closes[1] !== null && closes[2] !== null && closes[3] !== null, "all four genuine publications mint closeFocus");
            assert.deepEqual(closes[1], closes[0], "first re-emission is identical");
            assert.deepEqual(closes[2], closes[0], "second re-emission is identical");
            assert.deepEqual(closes[3], closes[0], "third re-emission is identical");
            const issued = closes[0] as { id: string; target: string; rev: string };
            assert.equal(issued.target, "win-c", "frozen MRU survivor excludes the closed focused win-b");
            const revs = replies.map((r) => r.rev);
            assert.ok(revs[0] !== undefined && revs[1] !== undefined && revs[2] !== undefined && revs[3] !== undefined);
            assert.ok(BigInt(revs[1] as string) > BigInt(revs[0] as string), "reply rev advances across re-emissions");
            assert.ok(BigInt(revs[2] as string) > BigInt(revs[1] as string), "reply rev advances again");
            assert.ok(BigInt(revs[3] as string) > BigInt(revs[2] as string), "reply rev advances a third time");
            assert.equal(issued.rev, revs[0], "close rev is the immutable issuing rev");
            assert.notEqual(issued.rev, revs[3], "issuing rev differs from the latest publication rev");
            // Consumer: a fresh adapter that never observed the issuing or
            // intermediate publications applies ONLY the latest.
            const latest = replies[3] as ApplyEnv["reply"];
            const state = new WorldAdapterState(latest.adapter, "owner-a");
            assert.ok(state.onDiscovery(latest.gen, latest.adapter), "consumer binds the genuine generation");
            state.invalidate();
            const flight = state.beginCapture(latest.source, null);
            assert.ok(flight !== null, "consumer opens the genuine source flight");
            assert.deepEqual(
                state.acceptReply(latest, latest.source, state.currentInvalid),
                { applied: true, reason: "accepted" },
            );
            const refs = new Map<string, object>([
                ["win-a", {}],
                ["win-c", {}],
            ]);
            const wins = (latest.windows as Array<{ win: string; out: string; ws: string }>).map((w) => ({
                win: w.win, out: w.out, ws: w.ws,
            }));
            assert.deepEqual(wins.map((w) => w.win).sort(), ["win-a", "win-c"], "genuine window set exact after close");
            const closeRow = latest.closeFocus;
            assert.ok(closeRow !== null, "genuine close row present on the latest publication");
            const observation = consumerObs(refs, wins, closeRow.out, closeRow.ws);
            const fg: { current: string | null } = { current: "win-a" };
            const focusCalls: string[] = [];
            const appliedRevs: string[] = [];
            const countingApply = (reply: ApplyEnv["reply"]): ReturnType<typeof applyWorldReply> => {
                appliedRevs.push(reply.rev);
                return applyWorldReply({
                    state,
                    observation,
                    reply,
                    dispatchWatermark: state.currentInvalid,
                    takeResult: null,
                    log: () => undefined,
                    isStillListed: () => true,
                    readActiveWindowId: () => fg.current,
                    readWindowId: (ref) => {
                        for (const [id, r] of refs) {
                            if (r === ref) {
                                return id;
                            }
                        }
                        return null;
                    },
                    readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
                    readLiveMembership: (ref) => {
                        for (const d of observation.domains) {
                            for (const w of d.windows) {
                                if (w.ref === ref) {
                                    return { out: w.out, ws: w.ws };
                                }
                            }
                        }
                        return null;
                    },
                    readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
                    readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: null }),
                    setGeometry: () => { throw new Error("no geometry here"); },
                    setMembership: () => { throw new Error("no membership here"); },
                    sendToOutput: () => { throw new Error("no send here"); },
                    resolveOutput: () => null,
                    resolveDesktop: () => null,
                    switchView: () => { throw new Error("no view here"); },
                    createDesktop: () => null,
                    removeDesktop: () => false,
                    focusWindow: (ref) => {
                        for (const [id, r] of refs) {
                            if (r === ref) {
                                focusCalls.push(id);
                                fg.current = id;
                                return true;
                            }
                        }
                        return false;
                    },
                    desktopOccupants: () => ({ total: 0, readable: true }),
                    desktopVisible: () => false,
                    readLiveView: () => null,
                    desktopById: () => null,
                    minDesktops: 2,
                });
            };
            // SKIP by construction: the issuing and both intermediate replies
            // (indices 0, 1, 2) are never passed to applyWorldReply; only
            // the latest (index 3) applies.
            const first = countingApply(latest);
            assert.equal(first.effects, 1, "latest publication dispatches exactly once");
            assert.deepEqual(focusCalls, [issued.target], "one native setter names the frozen target");
            assert.deepEqual(appliedRevs, [revs[3]], "only the latest publication reached apply");
            const rows = state.focusFeedbacksForReport();
            assert.equal(rows.length, 1, "one raw feedback row");
            assert.equal(rows[0]?.id, issued.id, "feedback id preserved");
            assert.equal(rows[0]?.target, issued.target, "feedback target preserved");
            assert.equal(rows[0]?.rev, issued.rev, "feedback carries the immutable issuing rev, not the publication rev");
            assert.equal(rows[0]?.setter, "accepted", "truthful raw setter");
            assert.equal(rows[0]?.reason, "", "ordinary call carries no reason");
            assert.equal(rows[0]?.pre, "win-a", "truthful raw pre");
            assert.equal(rows[0]?.post, issued.target, "truthful raw post");
            assert.equal(rows[0]?.foregroundProven, true, "fresh post proves the target");
            assert.deepEqual(rows[0]?.arrival, [], "close focus rides empty arrival");
            // Two unchanged redeliveries of the latest stay silent.
            const second = countingApply(latest);
            assert.equal(second.effects, 0, "first redelivery is duplicate-spent");
            const third = countingApply(latest);
            assert.equal(third.effects, 0, "second redelivery is duplicate-spent");
            assert.deepEqual(focusCalls, [issued.target], "still exactly one native setter overall");
            assert.equal(state.focusFeedbacksForReport().length, 1, "still one feedback row");
            // Null envelope retires through the actual accounting path.
            assert.equal(state.observeCloseEnvelope(null, latest.gen), "absent", "null retires");
            assert.ok(!state.isCurrentCloseRequest(issued.id, issued.target, issued.rev, closeRow.out, closeRow.ws, latest.gen), "retired record no longer current");
        } finally {
            await h.stop();
        }
    });
});
