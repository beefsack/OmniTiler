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
// (filings awaited first), and a timed-out sealed capture drains its batch
// by replaying exact bytes on the next event, never via an inline retry.
// Included in `npm test`.

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
    describeEnvelopeAt,
    deskById,
    discover,
    flushReads,
    frameOf,
    lastDescribeReply,
    makeHarness,
    replyFrames,
    takeReadys,
    winById,
    type FakeDesktop,
    type FakeWindow,
    type Harness,
} from "./world-producer-harness";

function press(h: Harness, key: string): void {
    const callback = h.shortcuts.get(key);
    assert.ok(callback, `registered shortcut ${key}`);
    callback();
}

function activate(h: Harness, win: string): void {
    h.activeRef = winById(h, win);
    h.fire("windowActivated", h.activeRef);
}

function reportAt(h: Harness, index: number): Record<string, unknown> {
    return describeEnvelopeAt(h, index)["report"] as Record<string, unknown>;
}

function assertFloats(reply: Record<string, unknown>, expected: Record<string, boolean>): void {
    const rows = reply["windows"] as Array<Record<string, unknown>>;
    for (const [id, floating] of Object.entries(expected)) {
        assert.equal(rows.find((row) => row["win"] === id)?.["floating"], floating, `float state of ${id}`);
    }
}

function floatSubjects(h: Harness): unknown[] {
    return takeReadys(h).flatMap((take) => {
        const result = take["result"] as Record<string, unknown> | null;
        return result?.["op"] === "toggle-float" ? [result["win"]] : [];
    });
}
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
            assert.equal("result" in batchReply, false, "actuator lane never carries terminal results");
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
            assert.equal("result" in batchReply, false, "actuator lane never carries terminal results");
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
            const beforeCapture = describeCount(h);
            // Read the actual Engine placement: B settles left of focused
            // win-a, so the first move-left swaps and the repeat meets the
            // settled boundary.
            const placed = replyFrames(lastDescribeReply(h));
            assert.ok(
                (placed.get("win-b")?.x ?? 0) < (placed.get("win-a")?.x ?? 0),
                "engine settles B left of focused win-a",
            );
            const nativeBefore = h.wins.map((w) => ({ ...frameOf(w) }));
            // Same applicable direction twice, before either ACK: one cut.
            press(h, "Meta+Shift+H");
            press(h, "Meta+Shift+H");
            // The first dispatched capture must already own the entire
            // same-cut batch, regardless of how long its Queue ACKs took.
            const headIndex = await awaitDispatch(h);
            assert.equal(h.commandPayloads("move-directional").length, 2,
                "both moves file before the sealed capture dispatches");
            // Expiring only the capture must not touch either filing.
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
            assert.equal(describeCount(h), beforeCapture + 1, "timeout does not retry inline");
            // Model a dropped delivery: do not forward the expired attempt.
            // A native signal wakes the producer but cannot replace its cut.
            h.answered.add(headIndex);
            activate(h, "win-a");
            const catchIndex = await awaitDescribeFrom(h, headIndex + 1);
            assert.equal(h.calls[catchIndex]?.payload, h.calls[headIndex]?.payload, "exact source and sealed bytes");
            const catchReply = JSON.parse(h.replies.get(await h.flushAt(catchIndex)) as string) as Record<string, unknown>;
            assert.equal(catchReply["outcome"] ?? "accepted", "accepted", "catchup drains the filed batch");
            assert.equal("result" in catchReply, false, "actuator lane never carries the terminal result");
            assert.deepEqual(h.wins.map((w) => frameOf(w)), nativeBefore, "stale replay does not apply");
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
            // A repeated Describe transport attempt never repeats commands.
            const globalCmds = batchTakes.map((t) => t["globalCmd"] as string);
            assert.equal(new Set(globalCmds).size, globalCmds.length, "sent commands never replay");
            await h.settle();
            const converged = lastDescribeReply(h);
            appliedMatchesReply(h, converged);
            assert.ok(
                h.wins.some((w, i) => JSON.stringify(frameOf(w)) !== JSON.stringify(nativeBefore[i])),
                "fresh catchup applies the Engine-planned swap",
            );
            // A later observation-only round changes nothing further.
            activate(h, "win-a");
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

    it("seals the admitted D press before T activates under the Queue ACK", async () => {
        const h = await makeHarness("win-b", ["win-a", "win-b"]);
        try {
            await discover(h);
            const initial = lastDescribeReply(h);
            assertFloats(initial, { "win-a": false, "win-b": false });
            const n0 = describeCount(h);
            // Hold only delivery: the broker really admits the command before T.
            const held: Array<{ reply: string; callback: (reply: unknown) => void }> = [];
            h.deliverQueueAck = (reply, callback) => {
                held.push({ reply, callback });
            };
            activate(h, "win-a");
            press(h, "Meta+G");
            await awaitQueueCount(h, "toggle-float", 1);
            assert.equal(h.commandPayloads("toggle-float").length, 1);
            for (let round = 0; held.length === 0 && round < 200; round += 1) {
                await new Promise<void>((resolve) => setTimeout(resolve, 0));
            }
            const ack = held.shift();
            assert.ok(ack, "the real broker acknowledged admission");
            assert.equal(JSON.parse(ack.reply)["outcome"], "queued");
            h.deliverQueueAck = (reply, callback) => callback(reply);
            activate(h, "win-b");
            h.runTimers();
            assert.equal(describeCount(h), n0, "no capture drains before Queue ACK");
            ack.callback(ack.reply);
            const index = await awaitDispatch(h);
            const report = reportAt(h, index);
            assert.equal(report["focused"], "win-a");
            assert.deepEqual(report["focusChanges"], {
                after: initial["rev"], events: [{ kind: "foreground", win: "win-a" }],
            });
            h.geometryWrites.length = 0;
            await h.flushAt(index);
            const drained = lastDescribeReply(h);
            assert.equal(BigInt(drained["rev"] as string), BigInt(initial["rev"] as string) + 1n);
            assertFloats(drained, { "win-a": true, "win-b": false });
            assert.deepEqual(floatSubjects(h), ["win-a"]);
            assert.deepEqual(h.geometryWrites, [], "the old cut settles without native writes");
            await h.settle();
            const settled = lastDescribeReply(h);
            assertFloats(settled, { "win-a": true, "win-b": false });
            assert.deepEqual(floatSubjects(h), ["win-a"], "catchup does not toggle again");
            assert.equal(h.activeRef?.internalId, "win-b");
            assert.deepEqual(h.focused, []);
            appliedMatchesReply(h, settled);
        } finally {
            await h.stop();
        }
    });

    for (const boundary of ["change and return", "selector only", "focus-event overflow"] as const) {
        it(`keeps the earlier float cut intact across ${boundary}`, async () => {
            const selectorOnly = boundary === "selector only";
            const overflow = boundary === "focus-event overflow";
            const h = await makeHarness(selectorOnly ? "win-a" : "win-b", ["win-a", "win-b"],
                selectorOnly ? { desktops: ["ws1", "ws2"] } : undefined);
            try {
                if (selectorOnly) {
                    winById(h, "win-b").desktops = [deskById(h, "ws2")];
                }
                await discover(h);
                const initial = lastDescribeReply(h);
                const foregroundA = { kind: "foreground", win: "win-a" };
                const foregroundB = { kind: "foreground", win: "win-b" };
                if (!selectorOnly) {
                    activate(h, "win-a");
                }
                press(h, "Meta+G");

                let events: unknown;
                if (selectorOnly) {
                    const set = h.workspace["setCurrentDesktopForScreen"] as (desktop: FakeDesktop, out: { name: string }) => void;
                    set(deskById(h, "ws2"), h.activeScreen);
                    set(deskById(h, "ws1"), h.activeScreen);
                    events = ["ws2", "ws1"].map((native) => ({ kind: "view", out: h.activeScreen.name, native }));
                } else if (overflow) {
                    for (let i = 0; i < 65; i += 1) {
                        activate(h, i % 2 === 0 ? "win-b" : "win-a");
                    }
                    events = null;
                } else {
                    activate(h, "win-b");
                    activate(h, "win-a");
                    events = [foregroundB, foregroundA];
                }
                const secondSubject = overflow ? "win-b" : "win-a";
                assert.equal(h.activeRef?.internalId, secondSubject);
                press(h, "Meta+G");

                const first = await awaitDispatch(h);
                assert.equal(h.commandPayloads("toggle-float").length, 1, "boundary splits Queue filing");
                const firstReport = reportAt(h, first);
                assert.equal(firstReport["focused"], "win-a");
                assert.deepEqual(firstReport["focusChanges"], {
                    after: initial["rev"], events: selectorOnly ? [] : [foregroundA],
                });
                h.geometryWrites.length = 0;
                await h.flushAt(first);
                const firstReply = lastDescribeReply(h);
                assertFloats(firstReply, { "win-a": true, "win-b": false });
                assert.deepEqual(floatSubjects(h), ["win-a"]);
                assert.deepEqual(h.geometryWrites, [], "even a return to the original subject fences apply");

                const second = await awaitDescribeFrom(h, first + 1);
                const secondReport = reportAt(h, second);
                assert.equal(secondReport["focused"], secondSubject);
                assert.deepEqual(secondReport["focusChanges"], { after: firstReply["rev"], events });
                await h.flushAt(second);
                await h.settle();
                const settled = lastDescribeReply(h);
                assertFloats(settled, { "win-a": overflow, "win-b": overflow });
                assert.deepEqual(floatSubjects(h), ["win-a", secondSubject]);
                appliedMatchesReply(h, settled);
            } finally {
                await h.stop();
            }
        });
    }

    it("refuses only the shortcut torn by a reentrant subject getter", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            const initial = lastDescribeReply(h);
            const active = Object.getOwnPropertyDescriptor(h.workspace, "activeWindow");
            assert.ok(active?.get);
            let reentered = false;
            Object.defineProperty(h.workspace, "activeWindow", {
                ...active,
                get() {
                    const old = h.activeRef;
                    if (!reentered) {
                        reentered = true;
                        activate(h, "win-b");
                    }
                    return old;
                },
            });
            press(h, "Meta+G");
            Object.defineProperty(h.workspace, "activeWindow", active);
            assert.ok(reentered, "the real registered callback read the torn subject");
            assert.equal(h.commandPayloads("toggle-float").length, 0);
            press(h, "Meta+G");
            const index = await awaitDispatch(h);
            const report = reportAt(h, index);
            assert.equal(report["focused"], "win-b");
            assert.deepEqual(report["focusChanges"], {
                after: initial["rev"], events: [{ kind: "foreground", win: "win-b" }],
            });
            await h.flushAt(index);
            await h.settle();
            const settled = lastDescribeReply(h);
            assert.equal(h.commandPayloads("toggle-float").length, 1);
            assert.deepEqual(floatSubjects(h), ["win-b"]);
            assertFloats(settled, { "win-a": false, "win-b": true });
            appliedMatchesReply(h, settled);
        } finally {
            await h.stop();
        }
    });

    it("separates coherent D and T cuts across an unhooked activation", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            // A fresh entry attaches to the same surface, but activation
            // delivery is unavailable for the whole generation.
            delete h.workspace["windowActivated"];
            const restartAt = h.calls.length;
            h.restart();
            await discover(h, restartAt);
            assert.equal(h.signals["windowActivated"]?.handlers.length, 0);
            h.fire("screensChanged");
            const predecessor = await awaitDispatch(h);
            press(h, "Meta+G");
            activate(h, "win-b"); // No attached handler observes this.
            press(h, "Meta+G");

            await h.flushAt(predecessor);
            const priorReply = lastDescribeReply(h);
            assert.deepEqual(floatSubjects(h), [], "the parked predecessor cannot drain the D or T press");
            const first = await awaitDescribeFrom(h, predecessor + 1);
            assert.equal(h.commandPayloads("toggle-float").length, 1);
            const firstReport = reportAt(h, first);
            assert.equal(firstReport["focused"], "win-a");
            assert.deepEqual(firstReport["focusChanges"], { after: priorReply["rev"], events: null });
            await h.flushAt(first);
            const firstReply = lastDescribeReply(h);
            assert.deepEqual(floatSubjects(h), ["win-a"]);
            assertFloats(firstReply, { "win-a": true, "win-b": false });

            const second = await awaitDescribeFrom(h, first + 1);
            const secondReport = reportAt(h, second);
            assert.equal(secondReport["focused"], "win-b");
            assert.deepEqual(secondReport["focusChanges"], { after: firstReply["rev"], events: null });
            await h.flushAt(second);
            await h.settle();
            const settled = lastDescribeReply(h);
            assert.deepEqual(floatSubjects(h), ["win-a", "win-b"]);
            assertFloats(settled, { "win-a": true, "win-b": true });
            appliedMatchesReply(h, settled);
        } finally {
            await h.stop();
        }
    });

    for (const [variant, title] of [
        "separates D → float → proved-own T → float before the first drain",
        "uses T after D → proved-own T → float without reviving the older request",
        "keeps the nested T press and original activation when own-scope readback fails",
    ].entries()) {
        const pressBeforeOwn = variant === 0;
        const failProof = variant === 2;
        const subjects = pressBeforeOwn ? ["win-b", "win-c"] : failProof ? ["win-c", "win-a"] : ["win-c"];
        it(title, async () => {
            const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
            try {
                await discover(h);
                const initial = lastDescribeReply(h);
                assertFloats(initial, { "win-b": false, "win-c": false });
                const frames = replyFrames(initial);
                assert.ok(frames.get("win-c")!.x < frames.get("win-a")!.x);
                assert.ok(frames.get("win-b")!.x < frames.get("win-c")!.x);

                press(h, "Meta+H");
                const issuingIndex = await awaitDispatch(h);
                // Inject native signals inside the real focus setter call.
                // The original fake setter and the producer's actual Unit A
                // proof/readback run unchanged. D is external, T is its echo.
                const active = Object.getOwnPropertyDescriptor(h.workspace, "activeWindow");
                assert.ok(active?.set);
                let injected = false;
                Object.defineProperty(h.workspace, "activeWindow", {
                    ...active,
                    set(ref: unknown) {
                        if (!injected && ref === winById(h, "win-c")) {
                            injected = true;
                            activate(h, "win-b");
                            if (pressBeforeOwn) {
                                press(h, "Meta+G");
                            }
                            active.set?.call(h.workspace, ref);
                            h.fire("windowActivated", ref);
                            press(h, "Meta+G");
                            if (failProof) {
                                // The echo was initially a valid own candidate,
                                // but A wins before the wrapper's readback.
                                activate(h, "win-a");
                                press(h, "Meta+G");
                            }
                            return;
                        }
                        active.set?.call(h.workspace, ref);
                    },
                });
                await h.flushAt(issuingIndex);
                const issued = lastDescribeReply(h);
                const request = issued["focusRequest"] as Record<string, unknown>;
                assert.equal(request["target"], "win-c");
                assert.ok(injected, "native inputs actually ran in the issuing setter");
                assert.equal(h.activeRef?.internalId, failProof ? "win-a" : "win-c");
                assert.deepEqual(h.focused, ["win-c"]);

                const first = await awaitDescribeFrom(h, issuingIndex + 1);
                const firstReport = reportAt(h, first);
                assert.equal(firstReport["focused"], subjects[0]);
                const firstEvents = [{ kind: "foreground", win: "win-b" }];
                if (failProof) {
                    firstEvents.push({ kind: "foreground", win: "win-c" });
                }
                assert.deepEqual(firstReport["focusChanges"], { after: issued["rev"], events: firstEvents });
                assert.equal(h.commandPayloads("toggle-float").length, 1,
                    "the later shortcut cannot file before the first cut incorporates");
                h.geometryWrites.length = 0;
                await h.flushAt(first);
                const firstReply = lastDescribeReply(h);
                assert.equal(BigInt(firstReply["rev"] as string), BigInt(issued["rev"] as string) + 1n);
                assert.equal(firstReply["focusRequest"] ?? null, null);
                assert.deepEqual(floatSubjects(h), subjects.slice(0, 1));
                assertFloats(firstReply, { "win-a": false, "win-b": pressBeforeOwn, "win-c": !pressBeforeOwn });
                if (subjects.length === 2) {
                    assert.deepEqual(h.geometryWrites, [], "later focus transition fences old apply but not ACK");
                    const second = await awaitDescribeFrom(h, first + 1);
                    const secondReport = reportAt(h, second);
                    assert.equal(secondReport["focused"], subjects[1]);
                    assert.deepEqual(secondReport["focusChanges"], {
                        after: firstReply["rev"],
                        events: failProof ? [{ kind: "foreground", win: "win-a" }] : [],
                    });
                    await h.flushAt(second);
                    const secondReply = lastDescribeReply(h);
                    assert.equal(BigInt(secondReply["rev"] as string), BigInt(firstReply["rev"] as string) + 1n);
                    assert.equal(secondReply["focusRequest"] ?? null, null);
                    assertFloats(secondReply, { "win-a": failProof, "win-b": pressBeforeOwn, "win-c": true });
                    assert.deepEqual(floatSubjects(h), subjects);
                }
                await h.settle();
                const settled = lastDescribeReply(h);
                assertFloats(settled, { "win-a": failProof, "win-b": pressBeforeOwn, "win-c": true });
                assert.deepEqual(floatSubjects(h), subjects);
                assert.equal(settled["focusRequest"] ?? null, null);
                assert.deepEqual(h.focused, ["win-c"], "no compensating focus setter");
                assert.equal(h.activeRef?.internalId, failProof ? "win-a" : "win-c");
                appliedMatchesReply(h, settled);
            } finally {
                await h.stop();
            }
        });
    }

    for (const scenario of ["within one cut", "across a flight", "nested in the focus getter"]) {
        const forcedFlight = scenario === "across a flight";
        const nested = scenario === "nested in the focus getter";
        it(`focus-left feeds float ${scenario} before native arrival`, async () => {
            const h = await makeHarness("win-a", ["win-a", "win-b"]);
            try {
                await discover(h);
                const initial = lastDescribeReply(h);
                const frames = replyFrames(initial);
                assert.ok(frames.get("win-b")!.x < frames.get("win-a")!.x);
                const n0 = describeCount(h);
                // Model a delayed native focus setter. Do not emit an
                // activation or mutate the foreground between the cuts.
                const active = Object.getOwnPropertyDescriptor(h.workspace, "activeWindow");
                assert.ok(active?.set);
                let reentered = false;
                Object.defineProperty(h.workspace, "activeWindow", {
                    ...active,
                    get() {
                        if (nested && !reentered) {
                            reentered = true;
                            press(h, "Meta+G");
                        }
                        return h.activeRef;
                    },
                    set(ref: FakeWindow) {
                        h.focused.push(ref.internalId);
                    },
                });
                press(h, "Meta+H");
                const focusFlight = forcedFlight ? await awaitDispatch(h) : null;
                assert.equal(h.activeRef?.internalId, "win-a");
                if (nested) {
                    assert.ok(reentered, "the nested shortcut ran in the outer focus callback");
                } else {
                    press(h, "Meta+G");
                }
                let predecessor = initial;
                if (focusFlight !== null) {
                    await h.flushAt(focusFlight);
                    predecessor = lastDescribeReply(h);
                    assert.equal((predecessor["focusRequest"] as Record<string, unknown>)["target"], "win-b");
                    assertFloats(predecessor, { "win-a": false, "win-b": false });
                    assert.deepEqual(floatSubjects(h), []);
                    assert.equal(h.activeRef?.internalId, "win-a");
                }
                const floatFlight = await awaitDispatch(h);
                const cut = reportAt(h, floatFlight);
                assert.equal(cut["focused"], "win-a");
                assert.deepEqual(cut["focusChanges"], { after: predecessor["rev"], events: [] });
                assert.equal(describeCount(h), n0 + (forcedFlight ? 2 : 1));
                await h.flushAt(floatFlight);
                const drained = lastDescribeReply(h);
                assert.equal(BigInt(drained["rev"] as string), BigInt(predecessor["rev"] as string) + 1n);
                assert.equal(h.commandPayloads("toggle-float").length, 1, "float belongs to the dispatched cut");
                assert.ok(h.commandPayloads("focus-directional")[0]!.index < h.commandPayloads("toggle-float")[0]!.index,
                    "focus files before float, even when float reenters its getter");
                assert.deepEqual(takeReadys(h).map((take) => take["globalCmd"]), ["1", "2"]);
                assert.deepEqual(floatSubjects(h), ["win-b"]);
                assertFloats(drained, { "win-a": false, "win-b": true });
                Object.defineProperty(h.workspace, "activeWindow", active);
                activate(h, "win-b");
                await h.settle();
                const settled = lastDescribeReply(h);
                assertFloats(settled, { "win-a": false, "win-b": true });
                appliedMatchesReply(h, settled);
            } finally {
                await h.stop();
            }
        });
    }

    it("queues two rapid float presses while pending, executing float then unfloat FIFO with latest-only apply", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            const initialFrames = replyFrames(lastDescribeReply(h));
            const frameABefore = initialFrames.get("win-a");
            const frameBBefore = initialFrames.get("win-b");
            assert.ok(frameABefore !== undefined && frameBBefore !== undefined);
            h.geometryWrites.length = 0;

            const n0 = describeCount(h);
            press(h, "Meta+G");
            press(h, "Meta+G");

            assert.ok(!h.logs.some((l) => l.includes("native-fenced")), "no busy fence on rapid float");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")), "no command-busy refusal");
            assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no shortcut failure");

            const batchIndex = await awaitDispatch(h);
            const payloads = h.commandPayloads("toggle-float");
            assert.equal(payloads.length, 2, "both rapid float presses file FIFO");
            for (const q of payloads) {
                assert.deepEqual(
                    (q.body["command"] as Record<string, unknown>)["win"],
                    { currentFocus: true },
                    "toggle-float files explicit execution-time currentFocus marker",
                );
                assert.equal((q.body["command"] as Record<string, unknown>)["rect"], null);
            }

            assert.equal(describeCount(h), n0 + 1, "one capture drains both parked float presses");
            const batchReply = JSON.parse(h.replies.get(await h.flushAt(batchIndex)) as string) as Record<string, unknown>;
            assert.notEqual(batchReply["outcome"], "rejected", "real Engine drains the float batch");
            appliedMatchesReply(h, batchReply);

            const takes = takeReadys(h);
            assert.equal(takes.length, 2, "both float presses settle FIFO on the take lane");
            assert.deepEqual(takes.map((t) => t["globalCmd"]), ["1", "2"]);
            assert.deepEqual(
                takes.map((t) => (t["result"] as Record<string, unknown>)["op"]),
                ["toggle-float", "toggle-float"],
            );
            assert.deepEqual(
                takes.map((t) => (t["result"] as Record<string, unknown>)["win"]),
                ["win-a", "win-a"],
            );

            // Final publication returns win-a to tiled: no intermediate float frame written.
            const winRows = (batchReply["windows"] as Record<string, unknown>[]) ?? [];
            const rowA = winRows.find((r) => r["win"] === "win-a");
            assert.ok(rowA !== undefined && rowA["floating"] === false, "float then unfloat leaves win-a tiled");
            assert.deepEqual(frameOf(winById(h, "win-a")), frameABefore, "win-a ends at tiled frame without intermediate float writes");
            assert.deepEqual(frameOf(winById(h, "win-b")), frameBBefore, "win-b ends at tiled frame without intermediate reflow writes");
            assert.deepEqual(h.geometryWrites, [], "exact latest-only setter history: zero intermediate geometry writes");
        } finally {
            await h.stop();
        }
    });

    it("queues two rapid float presses while capture is in-flight against execution-time currentFocus", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            const frames = replyFrames(lastDescribeReply(h));
            assert.ok(
                (frames.get("win-b")?.x ?? 0) < (frames.get("win-c")?.x ?? 0) &&
                (frames.get("win-c")?.x ?? 0) < (frames.get("win-a")?.x ?? 0),
                "engine settles positional row B,C,A",
            );

            // Dispatch a focus-left capture so a DescribeWorld flight is in-flight.
            h.handle.requestFocus("left");
            await awaitQueueCount(h, "focus-directional", 1);
            const focusFlightIndex = await awaitDispatch(h);

            // While the first capture is in-flight, press Meta+G twice rapidly.
            h.handle.requestFloat();
            h.handle.requestFloat();

            assert.ok(!h.logs.some((l) => l.includes("native-fenced")), "no busy fence while capture in-flight");
            assert.ok(!h.logs.some((l) => l.includes("command-busy")), "no command-busy refusal");
            assert.ok(!h.logs.some((l) => l.includes("shortcut-failed")), "no shortcut failure");

            // Answer the in-flight focus capture (shifts core focus to win-c);
            // the parked second float press files once the capture settles and
            // both drain together in the follow-up capture.
            const focusReply = JSON.parse(h.replies.get(await h.flushAt(focusFlightIndex)) as string) as Record<string, unknown>;
            assert.notEqual(focusReply["outcome"], "rejected");
            appliedMatchesReply(h, focusReply);
            assert.deepEqual(h.focused, ["win-c"], "focus flight settles on win-c");

            await awaitQueueCount(h, "toggle-float", 2);
            const floatPayloads = h.commandPayloads("toggle-float");
            assert.equal(floatPayloads.length, 2, "both float presses queued while capture was in-flight");
            for (const q of floatPayloads) {
                assert.deepEqual(
                    (q.body["command"] as Record<string, unknown>)["win"],
                    { currentFocus: true },
                    "in-flight float press files explicit currentFocus marker",
                );
            }

            // One follow-up capture drains both parked float commands against execution-time currentFocus (win-c).
            const floatBatchIndex = await awaitDispatch(h);
            const floatBatchReply = JSON.parse(h.replies.get(await h.flushAt(floatBatchIndex)) as string) as Record<string, unknown>;
            assert.notEqual(floatBatchReply["outcome"], "rejected", "real Engine drains the parked float batch");
            appliedMatchesReply(h, floatBatchReply);

            const takes = takeReadys(h);
            assert.equal(takes.length, 3, "focus + both float presses settle FIFO");
            assert.deepEqual(takes.map((t) => t["globalCmd"]), ["1", "2", "3"]);
            assert.deepEqual(
                takes.slice(1).map((t) => (t["result"] as Record<string, unknown>)["op"]),
                ["toggle-float", "toggle-float"],
            );
            assert.deepEqual(
                takes.slice(1).map((t) => (t["result"] as Record<string, unknown>)["win"]),
                ["win-c", "win-c"],
                "both float commands resolve to execution-time currentFocus win-c, not press-time win-a",
            );

            const winRows = (floatBatchReply["windows"] as Record<string, unknown>[]) ?? [];
            const rowC = winRows.find((r) => r["win"] === "win-c");
            assert.ok(rowC !== undefined && rowC["floating"] === false, "float then unfloat leaves win-c tiled");
        } finally {
            await h.stop();
        }
    });

    it("does not attach press-time frame of a floating moved window to a different execution-time subject", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b", "win-c"]);
        try {
            await discover(h);
            // Float win-a (right) and win-b (left) with distinct native frames.
            press(h, "Meta+G");
            await h.flush();
            h.runTimers();
            await h.settle();
            // Commit D's own moved frame first so the final plan/native
            // oracle does not compare an unrelated floating live drag
            // against D's older retained frame. T's move stays native only.
            winById(h, "win-a").frameGeometry = { x: 1400, y: 77, width: 333, height: 222 };
            for (let i = 0; i < 2; i += 1) {
                press(h, "Meta+G");
                await h.flush();
                await h.settle();
            }

            activate(h, "win-b");
            h.runTimers();
            await h.settle();
            press(h, "Meta+G");
            await h.flush();
            h.runTimers();
            await h.settle();

            activate(h, "win-a");
            h.runTimers();
            await h.settle();
            // Only the resolved subject may donate its still-uncommitted frame.
            winById(h, "win-b").frameGeometry = { x: 200, y: 100, width: 300, height: 200 };
            h.geometryWrites.length = 0;
            h.focused.length = 0;

            // Queue focus-left (shifts execution-time focus from win-a to win-b).
            press(h, "Meta+H");
            await awaitQueueCount(h, "focus-directional", 1);
            const focusFlightIndex = await awaitDispatch(h);

            // Press float while focus-left is in-flight (press-time active is win-a at [1400, 77, 333, 222]).
            press(h, "Meta+G");
            const focusReply = JSON.parse(h.replies.get(await h.flushAt(focusFlightIndex)) as string) as Record<string, unknown>;
            assert.notEqual(focusReply["outcome"], "rejected");
            assert.deepEqual(h.focused, ["win-b"], "focus shifts from win-a to win-b");

            const floatFlightIndex = await awaitDispatch(h);
            const floatPayloads = h.commandPayloads("toggle-float");
            assert.equal(floatPayloads.length, 5);
            assert.deepEqual(
                (floatPayloads[4]?.body["command"] as Record<string, unknown>)["win"],
                { currentFocus: true },
            );
            assert.equal(
                (floatPayloads[4]?.body["command"] as Record<string, unknown>)["rect"],
                null,
                "press-time floating frame of win-a must not attach to execution-time subject win-b",
            );

            const cut = reportAt(h, floatFlightIndex);
            assert.equal(cut["focused"], "win-a");
            const rows = cut["windows"] as Array<Record<string, unknown>>;
            assert.deepEqual(rows.find((row) => row["win"] === "win-a")?.["rect"], [1400, 77, 333, 222]);
            assert.deepEqual(rows.find((row) => row["win"] === "win-b")?.["rect"], [200, 100, 300, 200]);
            const floatReply = JSON.parse(h.replies.get(await h.flushAt(floatFlightIndex)) as string) as Record<string, unknown>;
            assert.notEqual(floatReply["outcome"], "rejected");

            const takes = takeReadys(h);
            assert.equal(takes.length, 6);
            const floatResult = takes[5]?.["result"] as Record<string, unknown>;
            assert.equal(floatResult["op"], "toggle-float");
            assert.equal(floatResult["win"], "win-b");
            assertFloats(floatReply, { "win-a": true, "win-b": false });
            await h.settle();
            press(h, "Meta+G");
            await h.flush();
            await h.settle();
            const settled = lastDescribeReply(h);
            assert.deepEqual(floatSubjects(h), ["win-a", "win-a", "win-a", "win-b", "win-b", "win-b"]);
            assertFloats(settled, { "win-a": true, "win-b": true });
            assert.deepEqual(frameOf(winById(h, "win-b")), { x: 200, y: 100, w: 300, h: 200 });
            assert.deepEqual(frameOf(winById(h, "win-a")), { x: 1400, y: 77, w: 333, h: 222 });
            appliedMatchesReply(h, settled);
        } finally {
            await h.stop();
        }
    });

    it("does not eagerly mutate a press-time floating-maximized window while a focus shift is in-flight", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"], {
            withNativePreimage: true,
            desktops: ["ws1", "ws2"],
            outputs: [
                { name: "out-left", geometry: { x: 0, y: 0, width: 1000, height: 800 }, manufacturer: "Acme", model: "Panel-L", serialNumber: "SN-LEFT-1" },
                { name: "out-right", geometry: { x: 1000, y: 0, width: 1000, height: 800 }, manufacturer: "Acme", model: "Panel-R", serialNumber: "SN-RIGHT-1" },
            ],
            currents: { "out-left": "ws1", "out-right": "ws2" },
            homes: { "win-a": "out-right", "win-b": "out-left" },
        });
        try {
            await discover(h);
            // Float and maximize win-a on out-right; win-b sits on out-left so
            // focus("left") genuinely crosses outputs from floating-maximized win-a.
            h.handle.requestFloat();
            await awaitQueueCount(h, "toggle-float", 1);
            await h.flushAt(await awaitDispatch(h));
            h.runTimers();
            await h.settle();

            h.handle.requestMaximize();
            await awaitQueueCount(h, "toggle-maximize", 1);
            const maxFlightIndex = await awaitDispatch(h);
            const maxReply = JSON.parse(h.replies.get(await h.flushAt(maxFlightIndex)) as string) as Record<string, unknown>;
            assert.notEqual(maxReply["outcome"], "rejected");
            h.runTimers();
            await h.settle();
            assert.equal(winById(h, "win-a").maximizeMode, 3, "precondition: win-a is floating-maximized before focus shift");
            h.nativeStateWrites.length = 0;
            h.focused.length = 0;

            h.handle.requestFocus("left");
            await awaitQueueCount(h, "focus-directional", 1);
            const focusFlightIndex = await awaitDispatch(h);

            // Press float while focus-left is in-flight: must queue for win-b, never clear win-a's maximize!
            h.handle.requestFloat();
            assert.equal(winById(h, "win-a").maximizeMode, 3, "must not eagerly clear maximize on press-time win-a");
            assert.ok(
                !h.nativeStateWrites.some((w) => w.startsWith("win-a@setMaximize:")),
                "no eager native maximize-clear write on press-time win-a",
            );

            const focusReply = JSON.parse(h.replies.get(await h.flushAt(focusFlightIndex)) as string) as Record<string, unknown>;
            assert.notEqual(focusReply["outcome"], "rejected");
            assert.deepEqual(h.focused, ["win-b"]);

            await awaitQueueCount(h, "toggle-float", 2);
            const floatFlightIndex = await awaitDispatch(h);
            const floatReply = JSON.parse(h.replies.get(await h.flushAt(floatFlightIndex)) as string) as Record<string, unknown>;
            assert.notEqual(floatReply["outcome"], "rejected");

            const takes = takeReadys(h);
            assert.equal(takes.length, 4);
            assert.equal((takes[3]?.["result"] as Record<string, unknown>)["win"], "win-b");
            assert.equal(winById(h, "win-a").maximizeMode, 3, "win-a maximize state stays untouched");
            assert.ok(
                !h.nativeStateWrites.some((w) => w.startsWith("win-a@setMaximize:")),
                "win-a receives no native maximize mutation across the flight",
            );
        } finally {
            await h.stop();
        }
    });

    it("queues start-floating double toggle retaining moved placement with zero intermediate tile writes", async () => {
        const h = await makeHarness("win-a", ["win-a", "win-b"]);
        try {
            await discover(h);
            h.handle.requestFloat();
            await h.flush();
            h.runTimers();
            await h.settle();

            // Move floating win-a natively, then clear geometryWrites.
            winById(h, "win-a").frameGeometry = { x: 111, y: 77, width: 333, height: 222 };
            h.geometryWrites.length = 0;

            // Rapid double toggle: unfloat (captures moved rect) -> re-float (restores moved rect).
            press(h, "Meta+G");
            press(h, "Meta+G");

            const batchIndex = await awaitDispatch(h);
            const payloads = h.commandPayloads("toggle-float");
            assert.equal(payloads.length, 3);
            for (const q of payloads.slice(1)) {
                assert.equal((q.body["command"] as Record<string, unknown>)["rect"], null,
                    "the moved frame belongs in the native cut, not either implicit Queue command");
            }

            const cutWindows = reportAt(h, batchIndex)["windows"] as Array<Record<string, unknown>>;
            assert.deepEqual(cutWindows.find((row) => row["win"] === "win-a")?.["rect"], [111, 77, 333, 222]);
            const batchReply = JSON.parse(h.replies.get(await h.flushAt(batchIndex)) as string) as Record<string, unknown>;
            assert.notEqual(batchReply["outcome"], "rejected");
            appliedMatchesReply(h, batchReply);

            const takes = takeReadys(h);
            assert.equal(takes.length, 3);
            assert.deepEqual(takes.slice(1).map((t) => t["globalCmd"]), ["2", "3"]);
            assert.deepEqual(floatSubjects(h), ["win-a", "win-a", "win-a"]);
            const winRows = (batchReply["windows"] as Record<string, unknown>[]) ?? [];
            const rowA = winRows.find((r) => r["win"] === "win-a");
            assert.ok(rowA !== undefined && rowA["floating"] === true, "double toggle from floating leaves win-a floating");
            assert.deepEqual(rowA["frame"], [111, 77, 333, 222], "retained moved placement survives double toggle");
            assert.deepEqual(h.geometryWrites, [], "zero intermediate tile/reflow geometry writes");
        } finally {
            await h.stop();
        }
    });
});
