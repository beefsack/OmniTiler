// Rapid directional FIFO through the production entry against the real
// production broker (via the test-only `broker_world_eval` example: the true
// `WorldService` route serving discovery, queue, capture-batch drain, take,
// and presentation reads in one retained process; only D-Bus and signals are
// stubbed, with queue/take bound under a constant test sender). Proves
// integrated native effects under genuine batch semantics: every press files
// FIFO, one fresh capture incorporates native evidence once and drains ALL
// parked commands in order against post-prior core state, returning one final
// publication; per-command settlements ride the take lane. Intermediate
// native focus/geometry never occur (core collapses superseded unissued
// focus; the adapter applies only the latest publication), and a sent command
// never replays. Sequential settled presses still drain one per capture;
// rapid bursts drain together. Window-targeting commands file the explicit
// execution-time marker win:{currentFocus:true} (docs/decisions/commands.md
// #kde-rapid-input); reply placements are read from actual Engine replies
// before choosing directions, never assumed from native rect order. A queued
// native-signal invalidation still withholds the stale batch's real setters
// (filings awaited first), and a timed-out capture still drains its batch on
// the later on-next-event catchup (the next native signal, never an inline
// retry). Included in `npm test`.

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { isWorldPresentationReply, parseWorldReply } from "../src/world-report";
import {
    answerEffectFacts,
    appliedMatchesReply,
    awaitDescribeFrom,
    awaitDispatch,
    awaitQueueCount,
    describeCount,
    discover,
    flushReads,
    frameOf,
    lastDescribeReply,
    makeHarness,
    replyFrames,
    takeReadys,
    winById,
    type FakeWindow,
} from "./world-producer-harness";
describe("world entry rapid directional FIFO against the real Engine", () => {
    it("applies rapid left then right focus with exact native effects in order", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            // Read actual Engine placements from the settled reply before
            // choosing directions (never assume native rect order).
            let frames = replyFrames(lastDescribeReply(h));
            assert.ok(
                (frames.get("win-b")?.x ?? 0) < (frames.get("win-c")?.x ?? 0) &&
                (frames.get("win-c")?.x ?? 0) < (frames.get("win-a")?.x ?? 0),
                "engine settles positional row B,C,A",
            );
            // Setup consumes command sequence 1: single press left from
            // win-a lands middle win-c.
            h.handle.requestFocus("left");
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.deepEqual(h.focused, ["win-c"], "setup press focuses middle win-c");
            assert.equal(takeReadys(h).length, 1, "setup settles its own take");
            assert.equal(takeReadys(h)[0]?.["globalCmd"], "1", "setup holds sequence 1, never reused");
            frames = replyFrames(lastDescribeReply(h));
            assert.ok(
                (frames.get("win-b")?.x ?? 0) < (frames.get("win-c")?.x ?? 0) &&
                (frames.get("win-c")?.x ?? 0) < (frames.get("win-a")?.x ?? 0),
                "settled reply still places win-c between win-b and win-a",
            );
            // Both rapid presses file before either filing settles; the
            // explicit marker rides unchanged (core resolves at execution
            // with same-batch focus advance).
            h.handle.requestFocus("left");
            h.handle.requestFocus("right");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")));
            assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
            // One capture drains the whole parked batch: the dispatch gate
            // fires only after real filings settle. n0 is read BEFORE the
            // filing wait: the batch dispatch itself lands during those
            // yields, so reading after would swallow it (actual==n0).
            const n0 = describeCount(h);
            await awaitQueueCount(h, "focus-directional", 3);
            let moves = h.commandPayloads("focus-directional");
            assert.equal(moves.length, 3, "setup plus both rapid presses file");
            for (const q of moves) {
                assert.deepEqual(
                    (q.body["command"] as Record<string, unknown>)["win"],
                    { currentFocus: true },
                    "rapid press keeps the explicit execution-time marker",
                );
            }
            // One capture drains the whole parked batch: the dispatch gate
            // fires only after real filings settle.
            const batchIndex = await awaitDispatch(h);
            assert.equal(describeCount(h), n0 + 1, "one capture drains the whole parked batch");
            assert.deepEqual(h.focused, ["win-c"], "no intermediate native focus before the batch lands");
            const batchReply = JSON.parse(h.replies.get(await h.flushAt(batchIndex)) as string) as Record<string, unknown>;
            assert.notEqual(batchReply["outcome"], "rejected", "real Engine drains the batch");
            assert.equal(batchReply["result"], null, "actuator lane never carries terminal results");
            // Final-only publication: core collapses the superseded head
            // focus unissued, so only the return to origin publishes.
            // Focus left the effects lane: the grant rides the retained
            // latest-only focusRequest (v4), never a focus effect row.
            assert.deepEqual(batchReply["effects"], [], "no focus effect is published on the v4 wire");
            const batchFocus = batchReply["focusRequest"] as Record<string, unknown> | null;
            assert.ok(batchFocus !== null, `final publication carries the collapsed focus grant, got ${JSON.stringify(batchReply["focusRequest"])}`);
            assert.equal(batchFocus["target"], "win-c", "final publication returns focus to the origin");
            assert.equal(batchFocus["cause"], "explicit-command", "collapsed grant is command-owned");
            assert.equal(batchFocus["cmdSeq"], "3", "collapsed grant rides the latest sequence 3");
            assert.equal(batchFocus["afterShown"], null, "directional focus carries no arrival dependencies");
            assert.ok(!h.focused.includes("win-b"), "intermediate neighbor never actuates natively");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-c");
            appliedMatchesReply(h, batchReply);
            // Every action settles its own take FIFO under fresh sequences.
            const batchTakes = takeReadys(h);
            assert.equal(batchTakes.length, 3, "every batch action settles its own take");
            assert.deepEqual(
                batchTakes.slice(1).map((t) => t["globalCmd"]),
                ["2", "3"],
                "batch takes ride fresh sequences 2/3, never reused 1",
            );
            // No replay, no coalescing: late duplicate input changes nothing.
            h.fire("windowActivated");
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.commandPayloads("focus-directional").length, 3, "no command replay after settlement");
            assert.ok(!h.focused.includes("win-b"), "no duplicate native focus writes");
        } finally {
            await h.stop();
        }
    });

    it("applies rapid left then right pressed during a native burst in order, with zero debounce wait", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            let frames = replyFrames(lastDescribeReply(h));
            assert.ok(
                (frames.get("win-b")?.x ?? 0) < (frames.get("win-c")?.x ?? 0) &&
                (frames.get("win-c")?.x ?? 0) < (frames.get("win-a")?.x ?? 0),
                "engine settles positional row B,C,A",
            );
            h.handle.requestFocus("left");
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.deepEqual(h.focused, ["win-c"], "setup press focuses middle win-c");
            // A native burst arms the observation debounce with no command
            // in flight; both rapid presses queue behind it in order. The
            // filed batch preempts the armed debounce on the zero-delay
            // path (never a 120 ms wait, never a commandless capture while
            // filings are outstanding): one capture still drains the whole
            // batch.
            h.fire("windowActivated");
            h.handle.requestFocus("left");
            h.handle.requestFocus("right");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")));
            assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
            const n0 = describeCount(h);
            await awaitQueueCount(h, "focus-directional", 3);
            const moves = h.commandPayloads("focus-directional");
            assert.equal(moves.length, 3, "setup plus both rapid presses file");
            for (const q of moves) {
                assert.deepEqual(
                    (q.body["command"] as Record<string, unknown>)["win"],
                    { currentFocus: true },
                    "rapid press keeps the explicit execution-time marker",
                );
            }
            const batchIndex = await awaitDispatch(h);
            assert.equal(describeCount(h), n0 + 1, "preempted pair still drains in one capture, never a commandless trailing capture");
            assert.deepEqual(h.focused, ["win-c"], "no intermediate native focus before the batch lands");
            const batchReply = JSON.parse(h.replies.get(await h.flushAt(batchIndex)) as string) as Record<string, unknown>;
            assert.notEqual(batchReply["outcome"], "rejected", "real Engine drains the debounced batch");
            // Focus left the effects lane: the grant rides the retained
            // latest-only focusRequest (v4), never a focus effect row.
            assert.deepEqual(batchReply["effects"], [], "no focus effect is published on the v4 wire");
            const batchFocus = batchReply["focusRequest"] as Record<string, unknown> | null;
            assert.ok(batchFocus !== null, `debounced pair carries the collapsed focus grant, got ${JSON.stringify(batchReply["focusRequest"])}`);
            assert.equal(batchFocus["target"], "win-c", "debounced pair returns focus to the origin");
            assert.equal(batchFocus["cause"], "explicit-command", "collapsed grant is command-owned");
            assert.equal(batchFocus["cmdSeq"], "3", "collapsed grant rides the latest sequence 3");
            assert.equal(batchFocus["afterShown"], null, "directional focus carries no arrival dependencies");
            assert.ok(!h.focused.includes("win-b"), "intermediate neighbor never actuates natively");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-c");
            appliedMatchesReply(h, batchReply);
            frames = replyFrames(batchReply);
            assert.ok(
                (frames.get("win-b")?.x ?? 0) < (frames.get("win-c")?.x ?? 0),
                "batch reply keeps the settled placement",
            );
            const batchTakes = takeReadys(h);
            assert.equal(batchTakes.length, 3, "every batch action settles its own take");
            assert.deepEqual(
                batchTakes.slice(1).map((t) => t["globalCmd"]),
                ["2", "3"],
                "batch takes ride fresh sequences 2/3, never reused 1",
            );
            assert.deepEqual(h.focused.filter((f) => f === "win-b"), [], "debounced pair lands neighbor never, origin finally");
        } finally {
            await h.stop();
        }
    });

    it("applies each repeated directional move with its own planned geometry", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            // Read the actual Engine placement before choosing directions:
            // the Engine settles B left of focused win-a.
            let frames = replyFrames(lastDescribeReply(h));
            assert.ok(
                (frames.get("win-b")?.x ?? 0) < (frames.get("win-a")?.x ?? 0),
                "engine settles B left of focused win-a",
            );
            const nativeBefore = h.wins.map((w) => ({ ...frameOf(w) }));
            // Rapid out-and-back pair: one capture drains both moves in
            // order against post-prior core state, returning one net-zero
            // publication. Both presses file the explicit marker.
            h.handle.requestMove("left");
            h.handle.requestMove("right");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")));
            const n0 = describeCount(h);
            await awaitQueueCount(h, "move-directional", 2);
            let moves = h.commandPayloads("move-directional");
            assert.equal(moves.length, 2, "both rapid moves file");
            for (const q of moves) {
                assert.deepEqual(
                    (q.body["command"] as Record<string, unknown>)["win"],
                    { currentFocus: true },
                    "rapid move keeps the explicit execution-time marker",
                );
                assert.ok(
                    (q.body["command"] as Record<string, unknown>)["direction"] === "left" ||
                    (q.body["command"] as Record<string, unknown>)["direction"] === "right",
                );
            }
            const batchIndex = await awaitDispatch(h);
            assert.equal(describeCount(h), n0 + 1, "one capture drains the whole move batch");
            const batchReply = JSON.parse(h.replies.get(await h.flushAt(batchIndex)) as string) as Record<string, unknown>;
            assert.notEqual(batchReply["outcome"], "rejected", "real Engine drains the move batch");
            assert.equal(batchReply["result"], null, "actuator lane never carries terminal results");
            // Net-zero round trip: the pair swaps out and back, so the
            // final publication matches the pre-batch frames exactly, and
            // the native setters match the final publication.
            appliedMatchesReply(h, batchReply);
            assert.deepEqual(
                h.wins.map((w) => ({ ...frameOf(w) })),
                nativeBefore,
                "out-and-back move pair converges net-zero",
            );
            frames = replyFrames(batchReply);
            assert.deepEqual(frames.get("win-a"), {
                x: nativeBefore[0]?.x, y: nativeBefore[0]?.y, w: nativeBefore[0]?.w, h: nativeBefore[0]?.h,
            });
            // Every action settles its own take FIFO under fresh sequences.
            const batchTakes = takeReadys(h);
            assert.equal(batchTakes.length, 2, "every batch move settles its own take");
            assert.deepEqual(
                batchTakes.map((t) => t["globalCmd"]),
                ["1", "2"],
                "move takes ride fresh sequences 1/2",
            );
            // Follow-focus on the mover is genuine production behavior for
            // tiled moves (tiler-core world_directional.rs
            // `tiled_move_keeps_focus_on_mover_through_one_effect`): the
            // out-and-back batch re-affirms focus on the mover through the
            // retained latest-only focusRequest, never touching the
            // neighbor. Focus left the effects lane (v4), so the grant
            // rides focusRequest; the mover is already focused, so the
            // already-target native fallback satisfies with zero setters.
            assert.deepEqual(batchReply["effects"], [], "no focus effect is published on the v4 wire");
            const moveFocus = batchReply["focusRequest"] as Record<string, unknown> | null;
            assert.ok(moveFocus !== null, `move batch carries the mover follow-focus grant, got ${JSON.stringify(batchReply["focusRequest"])}`);
            assert.equal(moveFocus["target"], "win-a", "move batch re-affirms focus on the mover");
            assert.equal(moveFocus["cause"], "explicit-command", "follow-focus grant is command-owned");
            assert.equal(moveFocus["cmdSeq"], "2", "follow-focus grant rides the latest sequence 2");
            assert.equal(moveFocus["afterShown"], null, "directional follow-focus carries no arrival dependencies");
            assert.deepEqual(h.focused, [], "mover already focused: no redundant native focus write");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-a", "mover stays focused");
            // Late duplicate input replays nothing.
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.commandPayloads("move-directional").length, 2, "no command replay after settlement");
            assert.deepEqual(
                h.wins.map((w) => ({ ...frameOf(w) })),
                nativeBefore,
                "settlement keeps the converged frames",
            );
        } finally {
            await h.stop();
        }
    });

    it("resolves a queued move to the settled focus after a mixed focus then move", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            // Read the actual Engine placement: B settles left of focused
            // win-a, so focus-left lands on win-b and the paired move-right
            // swaps from there. Both file the explicit marker; core
            // resolves subjects at execution with same-batch focus advance
            // (never a press-time rebind).
            const frames = replyFrames(lastDescribeReply(h));
            assert.ok(
                (frames.get("win-b")?.x ?? 0) < (frames.get("win-a")?.x ?? 0),
                "engine settles B left of focused win-a",
            );
            h.handle.requestFocus("left");
            h.handle.requestMove("right");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")));
            assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")));
            const n0 = describeCount(h);
            await awaitQueueCount(h, "move-directional", 1);
            const focuses = h.commandPayloads("focus-directional");
            const moves = h.commandPayloads("move-directional");
            assert.equal(focuses.length, 1, "focus press files");
            assert.equal(moves.length, 1, "move press files behind it");
            assert.deepEqual(
                (focuses[0]?.body["command"] as Record<string, unknown>)["win"],
                { currentFocus: true },
                "focus keeps the explicit execution-time marker",
            );
            assert.deepEqual(
                (moves[0]?.body["command"] as Record<string, unknown>)["win"],
                { currentFocus: true },
                "queued move keeps the explicit execution-time marker (no press-time rebind)",
            );
            assert.deepEqual(
                (moves[0]?.body["command"] as Record<string, unknown>)["direction"],
                "right",
                "queued move keeps its pressed direction",
            );
            // One capture drains the mixed batch in order.
            const batchIndex = await awaitDispatch(h);
            assert.equal(describeCount(h), n0 + 1, "one capture drains the mixed batch");
            assert.deepEqual(h.focused, [], "no intermediate native focus before the batch lands");
            const batchReply = JSON.parse(h.replies.get(await h.flushAt(batchIndex)) as string) as Record<string, unknown>;
            assert.notEqual(batchReply["outcome"], "rejected", "real Engine drains the mixed batch");
            // From stale win-a (right edge) right would be a boundary noop
            // while from settled win-b it swaps: the applied swap proves
            // same-batch resolution to the settled focus.
            // Focus left the effects lane: the grant rides the retained
            // latest-only focusRequest (v4), never a focus effect row.
            assert.deepEqual(batchReply["effects"], [], "no focus effect is published on the v4 wire");
            const mixedFocus = batchReply["focusRequest"] as Record<string, unknown> | null;
            assert.ok(mixedFocus !== null, `batch carries the settled focus grant, got ${JSON.stringify(batchReply["focusRequest"])}`);
            assert.equal(mixedFocus["target"], "win-b", "batch focuses the settled win-b");
            assert.equal(mixedFocus["cause"], "explicit-command", "settled grant is command-owned");
            assert.equal(mixedFocus["cmdSeq"], "2", "settled grant rides the latest sequence 2");
            assert.equal(mixedFocus["afterShown"], null, "directional focus carries no arrival dependencies");
            appliedMatchesReply(h, batchReply);
            assert.deepEqual(h.focused, ["win-b"], "mixed batch lands exactly one native focus on win-b");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-b", "mover stays focused");
            assert.ok(!h.logs.some((l) => l.includes("focus-mismatch")));
            assert.ok(!h.logs.some((l) => l.includes("command-dropped reason=move-")));
            const batchTakes = takeReadys(h);
            assert.equal(batchTakes.length, 2, "every batch action settles its own take");
            assert.deepEqual(
                batchTakes.map((t) => t["globalCmd"]),
                ["1", "2"],
                "mixed takes ride fresh sequences 1/2",
            );
            // No replay after settlement.
            h.fire("windowActivated");
            await h.settle();
            assert.equal(h.commandPayloads("focus-directional").length, 1, "no focus replay after settlement");
            assert.equal(h.commandPayloads("move-directional").length, 1, "no move replay after settlement");
        } finally {
            await h.stop();
        }
    });

    it("withholds a stale head's real setters after queued native invalidation, then drains", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            // Read actual placements: positional row B,C,A; bring middle
            // win-c into focus first so the rapid out-and-back pair starts
            // there.
            let frames = replyFrames(lastDescribeReply(h));
            assert.ok(
                (frames.get("win-b")?.x ?? 0) < (frames.get("win-c")?.x ?? 0) &&
                (frames.get("win-c")?.x ?? 0) < (frames.get("win-a")?.x ?? 0),
                "engine settles positional row B,C,A",
            );
            h.handle.requestFocus("left");
            await h.flush();
            h.runTimers();
            await h.settle();
            assert.deepEqual(h.focused, ["win-c"], "setup press focuses middle win-c");
            // Rapid out-and-back from win-c. The filings are awaited first:
            // only after the batch capture is genuinely dispatched (which
            // the dispatch gate allows solely once every parked Queue filing
            // settles) does genuine native input land under the flight. A
            // synchronously-fired invalidation would merely join the fresh
            // observation instead of superseding it.
            h.handle.requestFocus("left");
            h.handle.requestFocus("right");
            const n0 = describeCount(h);
            const batchIndex = await awaitDispatch(h);
            assert.equal(describeCount(h), n0 + 1, "one capture carries the whole batch");
            assert.ok(!h.logs.some((l) => l.includes("queue-timeout")), "real filings beat the queue timeouts");
            h.fire("windowActivated");
            const writesBefore = h.focused.length;
            await h.flushAt(batchIndex);
            // The batch reply still accepts at the transport, but the
            // observation is superseded, so no setters run on it.
            assert.ok(h.logs.some((l) => l.includes("stale reason=superseded-during-flight")));
            assert.equal(h.focused.length, writesBefore, "superseded batch applies no native focus");
            assert.ok(!h.focused.includes("win-b"), "superseded intermediate never actuates");
            // The drained batch already executed server-side: the take lane
            // collects both terminals, and the follow-up catchup observes
            // the newer watermark into final native/core agreement.
            h.runTimers();
            await h.settle();
            const batchTakes = takeReadys(h);
            assert.equal(batchTakes.length, 3, "every batch action settles its own take despite supersession");
            assert.deepEqual(
                batchTakes.slice(1).map((t) => t["globalCmd"]),
                ["2", "3"],
                "superseded takes ride fresh sequences 2/3, never reused 1",
            );
            appliedMatchesReply(h, lastDescribeReply(h));
            assert.equal(h.commandPayloads("focus-directional").length, 3, "superseded batch never replays its commands");
            assert.equal((h.activeRef as FakeWindow).internalId, "win-c", "native focus converges with the final agreement");
        } finally {
            await h.stop();
        }
    });

    it("drains the tail after a timed-out head with on-next-event recovery", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            const mark = h.timers.length;
            // Read the actual Engine placement: B settles left of focused
            // win-a, so the first move-left swaps and the repeat meets the
            // settled boundary.
            const placed = replyFrames(lastDescribeReply(h));
            assert.ok(
                (placed.get("win-b")?.x ?? 0) < (placed.get("win-a")?.x ?? 0),
                "engine settles B left of focused win-a",
            );
            const nativeBefore = h.wins.map((w) => ({ ...frameOf(w) }));
            // Same applicable direction twice: both file before either
            // filing settles, and one capture carries the batch.
            h.handle.requestMove("left");
            h.handle.requestMove("left");
            await awaitQueueCount(h, "move-directional", 2);
            const moves = h.commandPayloads("move-directional");
            assert.equal(moves.length, 2, "timed-out head plus queued tail file, nothing else");
            // Await real filing and the batch dispatch first: the dispatch
            // gate fires only after every parked Queue filing settles, and
            // each settled filing cancels its own queue timeout. Expiring
            // ONLY the capture flight afterwards never touches a queue.
            const headIndex = await awaitDispatch(h);
            assert.ok(!h.logs.some((l) => l.includes("queue-timeout")), "real filings beat the queue timeouts");
            assert.ok(!h.answered.has(headIndex), "batch capture still in flight, no synthetic reply");
            for (let round = 0; round < 6; round += 1) {
                if (h.logs.some((l) => l.includes("omnitiler:world:timeout recovery=on-next-event"))) {
                    break;
                }
                h.fireNextTimer(2000, mark);
            }
            assert.ok(h.logs.some((l) => l.includes("omnitiler:world:timeout recovery=on-next-event")), "capture flight expires");
            assert.ok(!h.logs.some((l) => l.includes("queue-timeout")), "only the capture expires, never a queue");
            // Recovery is on-next-event (never an inline retry): the next
            // native signal dispatches the catchup, which drains the filed
            // batch server-side exactly once (world-entry.ts `timeout
            // recovery=on-next-event`; the rapid batch decision files the
            // whole parked batch before one fresh capture drains it). The
            // timed-out capture itself is never forwarded, so it never
            // replays.
            h.fire("windowActivated");
            h.runTimers();
            let catchIndex = -1;
            for (let round = 0; round < 200; round += 1) {
                catchIndex = h.calls.findIndex((call, at) => at > headIndex && call.method === "DescribeWorld" && !h.answered.has(at));
                if (catchIndex >= 0) {
                    break;
                }
                h.runTimers();
                await new Promise<void>((resolve) => setTimeout(resolve, 0));
            }
            assert.ok(catchIndex >= 0, "catchup dispatches after the timeout");
            const catchReply = JSON.parse(h.replies.get(await h.flushAt(catchIndex)) as string) as Record<string, unknown>;
            assert.equal(catchReply["outcome"] ?? "accepted", "accepted", "catchup drains the filed batch");
            assert.equal(catchReply["result"], null, "actuator lane never carries the terminal result");
            appliedMatchesReply(h, catchReply);
            assert.ok(
                h.wins.some((w, i) => JSON.stringify(frameOf(w)) !== JSON.stringify(nativeBefore[i])),
                "drained batch applies the Engine-planned swap",
            );
            // The filed batch settles on the take lane per command: the head
            // move swaps (move ops carry no separate result - the snapshot
            // IS the result - so its ready take resolves null), while the
            // repeat meets the settled boundary. After the swap win-a sits
            // at the left edge with no left neighbor, so the planner
            // reports a Boundary noop (directional.rs NoopReason::Boundary),
            // the session refuses PlannerNoop (session/ops/move.rs), and
            // the batch settlement normalizes that unknown wire token to
            // the expected `operation-refused` terminal
            // (world_run_v4.rs settle path via CommandRejectKind::parse
            // fallback; world_report.rs). A refused tail is a legitimate
            // edge terminal by the existing contract, not a dropped action:
            // both commands drain exactly once, in FIFO order.
            const batchTakes = h.takeReplies.map((raw) => JSON.parse(raw) as Record<string, unknown>);
            const firstByCmd = new Map<string, Record<string, unknown>>();
            for (const take of batchTakes) {
                const cmd = take["globalCmd"] as string;
                if (!firstByCmd.has(cmd)) {
                    firstByCmd.set(cmd, take);
                }
            }
            assert.deepEqual([...firstByCmd.keys()], ["1", "2"], "both filed commands settle FIFO exactly once");
            const headTake = firstByCmd.get("1") as Record<string, unknown>;
            const tailTake = firstByCmd.get("2") as Record<string, unknown>;
            assert.equal(headTake["outcome"], "ready", "head move swaps and settles ready");
            assert.equal(headTake["result"], null, "drained move resolves a null take result");
            assert.equal(tailTake["outcome"], "failed", "boundary repeat settles failed, never dropped");
            assert.equal(tailTake["kind"], "operation-refused", "boundary noop normalizes to the expected refusal kind");
            // No-replay proof through production identities: the broker mints
            // one globalCmd per filed command and each settles exactly once
            // on the per-command take lane. The timed-out capture stays
            // unanswered through the catchup: it never replays there.
            assert.ok(!h.answered.has(headIndex), "timed-out capture never replays during catchup");
            const globalCmds = batchTakes.map((t) => t["globalCmd"] as string);
            assert.equal(new Set(globalCmds).size, globalCmds.length, "sent commands never replay");
            // Transport-drop model for the expired capture (explicit, not a
            // silent late forward): the entry transmitted headIndex before
            // its flight expired (transmitCapture sends, then the capture
            // timeout settles the flight with recovery=on-next-event while
            // the settled guard at world-entry.ts:5102 drops any late
            // reply). The test transport never delivered that envelope to
            // the broker, so it must never be evaluated late: mark it
            // answered-without-send (no reply recorded) so settle() cannot
            // forward the expired envelope after the catchup sourceSeq,
            // which the broker would only refuse as stale-source
            // (world_broker.rs:662-669). Either branch of the ambiguity
            // (drop-before-execution here; or evaluate-once-and-hold-the-
            // callback) keeps exactly-once execution; this test pins the
            // drop branch: no later send, no replay.
            h.answered.add(headIndex);
            // A later observation-only round changes nothing further.
            h.fire("windowActivated");
            h.runTimers();
            const framesAfterCatchup = h.wins.map((w) => ({ ...frameOf(w) }));
            await h.settle();
            assert.deepEqual(
                h.wins.map((w) => ({ ...frameOf(w) })),
                framesAfterCatchup,
                "settlement keeps the drained frames",
            );
            assert.equal(h.commandPayloads("move-directional").length, 2, "no command replay after settlement");
            // A remembered take may legitimately reread an already-settled
            // ready (take-lane memory, not a re-execution), so settlement
            // proof is per-identity, not a global uniqueness count: every
            // take names a drained command, first settlements stay FIFO,
            // and no identity ever settles a conflicting outcome.
            const settledTakes = h.takeReplies.map((raw) => JSON.parse(raw) as Record<string, unknown>);
            const settledFirst = new Map<string, Record<string, unknown>>();
            for (const take of settledTakes) {
                const cmd = take["globalCmd"] as string;
                assert.ok(cmd === "1" || cmd === "2", `take names a drained command, got ${cmd}`);
                if (!settledFirst.has(cmd)) {
                    settledFirst.set(cmd, take);
                } else {
                    assert.equal(take["outcome"], (settledFirst.get(cmd) as Record<string, unknown>)["outcome"], `reread agrees with the first settlement for ${cmd}`);
                }
            }
            assert.deepEqual([...settledFirst.keys()], ["1", "2"], "both terminal identities settle FIFO exactly once");
            assert.ok(!h.replies.has(headIndex), "expired capture never evaluated by the broker");
        } finally {
            await h.stop();
        }
    });

    it("plans direct resize presses and orientation with exact native-shaped geometry", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            // The Engine settles B left of focused win-a: growing win-a
            // leftwards outwards moves the shared boundary by the core
            // step (12px on press 0, 14px on press 1); growing rightwards
            // would refuse unchanged at the work-area edge.
            const xs = (id: string): number => frameOf(winById(h, id)).x;
            assert.ok(xs("win-b") < xs("win-a"), "engine settles B left of focused win-a");
            const widths = (): Record<string, number> => Object.fromEntries(
                h.wins.map((win) => [win.internalId, frameOf(win).w]),
            );
            // Press 0 through the production callback: the direct v4
            // resize files on the queue lane and the real Engine plans it.
            let before = widths();
            h.handle.requestResize("left", "outwards");
            h.runTimers();
            assert.equal(h.commandPayloads("resize").length, 1, "first press files the direct resize");
            const headIndex = await h.flush();
            const headReply = JSON.parse(h.replies.get(headIndex) as string) as Record<string, unknown>;
            assert.notEqual(headReply["outcome"], "rejected", "real Engine decodes the direct resize");
            appliedMatchesReply(h, headReply);
            let after = widths();
            assert.equal(after["win-a"] as number - (before["win-a"] as number), 12, "press 0 grows the subject by the initial step");
            assert.equal((before["win-b"] as number) - (after["win-b"] as number), 12, "press 0 takes exactly the step from the neighbor");
            h.runTimers();
            await h.settle();
            // Press 1 steps further from the settled layout: the press
            // counter survives the round trip, the subject keeps press-time
            // identity, and the Engine plans the steeper step.
            before = widths();
            h.handle.requestResize("left", "outwards");
            h.runTimers();
            assert.equal(h.commandPayloads("resize").length, 2, "second press files the direct resize again");
            const tailIndex = await h.flush();
            const tailReply = JSON.parse(h.replies.get(tailIndex) as string) as Record<string, unknown>;
            assert.notEqual(tailReply["outcome"], "rejected", "real Engine decodes the repeated resize");
            appliedMatchesReply(h, tailReply);
            after = widths();
            assert.equal(after["win-a"] as number - (before["win-a"] as number), 14, "press 1 grows the subject by the repeat step");
            assert.equal((before["win-b"] as number) - (after["win-b"] as number), 14, "press 1 takes exactly the repeat step from the neighbor");
            h.runTimers();
            await h.settle();
            // Orientation through the production callback: the direct
            // toggle flips the pair to a vertical stack with exact native
            // frames.
            before = widths();
            h.handle.requestToggleOrientation();
            h.runTimers();
            assert.equal(h.commandPayloads("toggle-orientation").length, 1, "orientation files the direct toggle");
            const orientIndex = await h.flush();
            const orientReply = JSON.parse(h.replies.get(orientIndex) as string) as Record<string, unknown>;
            assert.notEqual(orientReply["outcome"], "rejected", "real Engine decodes the direct toggle");
            appliedMatchesReply(h, orientReply);
            assert.equal(frameOf(winById(h, "win-a")).x, frameOf(winById(h, "win-b")).x, "flipped pair shares the full width");
            assert.notEqual(frameOf(winById(h, "win-a")).y, frameOf(winById(h, "win-b")).y, "flipped pair stacks vertically");
            assert.ok(
                h.wins.some((win) => frameOf(win).w !== (before[win.internalId] as number)),
                "orientation rewrites geometry",
            );
        } finally {
            await h.stop();
        }
    });

    it("presentation overlap through the real broker: unproven capture hides fail-closed, proven overlap paints", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], { withEffectFacts: true });
        try {
            const gen = await discover(h);
            // The bind seeds presentation reads; the facts waiter is still
            // held unanswered, so every capture so far is unproven.
            assert.ok(h.reads.length >= 1, "bind seeds a presentation read");
            assert.equal(h.effectCalls.length, 1, "one held effect-facts waiter");
            // Rapid batch: two presses drain in one genuine capture, and
            // both commands settle on the take lane.
            h.handle.requestFocus("left");
            h.handle.requestFocus("right");
            await awaitQueueCount(h, "focus-directional", 2);
            const batchIndex = await awaitDispatch(h);
            const batchReply = JSON.parse(h.replies.get(await h.flushAt(batchIndex)) as string) as Record<string, unknown>;
            assert.notEqual(batchReply["outcome"], "rejected", "real Engine drains the batch");
            assert.deepEqual(takeReadys(h).map((t) => t["globalCmd"]), ["1", "2"], "both batch commands settle FIFO");
            // The overlapping publication dirtied the bind read. Serve the
            // pending reads from the retained broker: the cache holds the
            // batch projection with no covering witness.
            const malformedBefore = h.logs.filter((l) => l.includes("presentation-malformed")).length;
            const setsBefore = h.renders.filter((r) => r.method === "SetPresentation").length;
            const hidesBefore = h.renders.filter((r) => r.method === "HidePresentation").length;
            assert.equal(setsBefore, 0, "nothing painted before the unproven read");
            const rawNull = await flushReads(h);
            assert.ok(rawNull.length >= 1, "pending reads served");
            const nullBody = JSON.parse(rawNull[rawNull.length - 1] as string) as Record<string, unknown>;
            assert.equal(nullBody["outcome"], "presentation", "genuine broker serves the presentation lane");
            assert.equal(nullBody["gen"], gen, "reply binds the serving generation");
            assert.equal(nullBody["coveredInstance"], null, "unproven capture serves paired-null coverage");
            assert.equal(nullBody["coveredInvalid"], null);
            const parsedNull = parseWorldReply(rawNull[rawNull.length - 1] as string);
            assert.ok(parsedNull !== null && isWorldPresentationReply(parsedNull), "null-coverage bytes pass the lane parser");
            const continuityAt = h.logs.findIndex((l) => l.includes("presentation-continuity-during-flight"));
            const malformedAt = h.logs.findIndex((l) => l.includes("presentation-malformed"));
            assert.ok(continuityAt >= 0 && malformedAt > continuityAt, "formatter refusal follows dirty-flight continuity");
            const malformed = h.logs.filter((l) => l.includes("presentation-malformed"));
            assert.equal(malformed.length, malformedBefore + 1, "exactly one formatter refusal");
            assert.ok((malformed[malformed.length - 1] as string).includes("recovery=hide"), "refusal hides");
            assert.ok((malformed[malformed.length - 1] as string).includes("reason=missing-coverage"), "refusal names missing coverage");
            assert.equal(h.renders.filter((r) => r.method === "SetPresentation").length, setsBefore, "unproven overlap paints nothing");
            assert.equal(h.renders.filter((r) => r.method === "HidePresentation").length, hidesBefore + 1, "unproven overlap hides display-only");
            // Settle the quiescence follow-up observation capture left
            // unflushed by the batch flush: while any capture flight is
            // open the entry correctly fences new dispatches, so the
            // covering capture below needs the wire free first.
            await h.settle();
            // Proven leg: facts arrive, the covering capture carries the
            // witness, and a second overlapping capture dirties the
            // refetch read. The proven overlap must paint, never hide.
            const callsBefore = h.calls.length;
            answerEffectFacts(h, "5", true);
            const coverIndex = await awaitDescribeFrom(h, callsBefore);
            const coverReply = JSON.parse(h.replies.get(await h.flushAt(coverIndex)) as string) as Record<string, unknown>;
            assert.notEqual(coverReply["outcome"], "rejected", "covering capture accepted");
            assert.equal(h.reads.length, 1, "covering acceptance re-drives one read");
            const overFrom = h.calls.length;
            h.fire("windowActivated");
            const overIndex = await awaitDescribeFrom(h, overFrom);
            assert.notEqual(
                (JSON.parse(h.replies.get(await h.flushAt(overIndex)) as string) as Record<string, unknown>)["outcome"],
                "rejected",
                "overlapping capture accepted",
            );
            const setsBeforeProven = h.renders.filter((r) => r.method === "SetPresentation").length;
            const hidesBeforeProven = h.renders.filter((r) => r.method === "HidePresentation").length;
            const malformedBeforeProven = h.logs.filter((l) => l.includes("presentation-malformed")).length;
            const rawProven = await flushReads(h);
            assert.ok(rawProven.length >= 1, "overlapping read served");
            const provenBody = JSON.parse(rawProven[rawProven.length - 1] as string) as Record<string, unknown>;
            assert.equal(provenBody["coveredInstance"], "effect-1", "covering capture proves its witness");
            assert.equal(provenBody["coveredInvalid"], "5");
            assert.equal(
                h.renders.filter((r) => r.method === "SetPresentation").length,
                setsBeforeProven + 1,
                "proven overlap paints through continuity",
            );
            assert.equal(
                h.logs.filter((l) => l.includes("presentation-malformed")).length,
                malformedBeforeProven,
                "proven overlap never hides",
            );
            assert.equal(
                h.renders.filter((r) => r.method === "HidePresentation").length,
                hidesBeforeProven,
                "proven overlap never hides display",
            );
            // The continuity refetch converges without resending.
            const converged = await flushReads(h);
            assert.equal(converged.length, 1, "one bounded refetch");
            assert.equal(
                h.renders.filter((r) => r.method === "SetPresentation").length,
                setsBeforeProven + 1,
                "converged refetch never resends",
            );
        } finally {
            await h.stop();
        }
    });
});
