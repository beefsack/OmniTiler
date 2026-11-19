// Connected offline coordinate-closure harness (Unit G, AC #2):
// KWin TS observation -> real checked broker (`broker_world_eval`) ->
// TS `parseWorldReply` + applied frame -> next TS capture -> broker
// convergence + causal effect observation -> real `h.renders`
// `SetPresentation` payload -> native C++ consumers (`gesturelifecycle_test
// --probe`, `presentationconsumer_test --probe`).

import assert from "node:assert/strict";
import { execFileSync } from "node:child_process";
import { describe, it } from "node:test";

import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import {
    answerEffectFacts,
    appliedMatchesReply,
    awaitDescribeFrom,
    describeEnvelopeAt,
    discover,
    flushReads,
    lastDescribeReply,
    makeHarness,
    winById,
} from "./world-producer-harness";

const gestureBin = process.env["OMNITILER_GESTURE_TEST_BIN"];
const presBin = process.env["OMNITILER_PRES_TEST_BIN"];
if ((gestureBin !== undefined && gestureBin.length > 0) !== (presBin !== undefined && presBin.length > 0)) {
    throw new Error("OMNITILER_GESTURE_TEST_BIN and OMNITILER_PRES_TEST_BIN must both be set or both unset");
}
const hasNativeBins = gestureBin !== undefined && gestureBin.length > 0 && presBin !== undefined && presBin.length > 0;

interface ClosureArtifacts {
    readonly firstFrame: readonly [number, number, number, number];
    readonly setPresentationPayload: string;
}

async function runConnectedClosure(): Promise<ClosureArtifacts> {
    const h = await makeHarness("win-a", ["win-a"], {
        withEffectFacts: true,
        outputs: [{ name: "DP-1", geometry: { x: 16384, y: 0, width: 800, height: 600 } }],
    });
    try {
        await discover(h);
        await h.settle();

        const firstIdx = h.calls.findIndex((c) => c.method === "DescribeWorld");
        assert.ok(firstIdx >= 0, "initial DescribeWorld captured");
        const firstEnv = describeEnvelopeAt(h, firstIdx);
        const firstReport = firstEnv["report"] as Record<string, unknown>;
        const firstScreens = firstReport["screens"] as Array<Record<string, unknown>>;
        assert.deepEqual(firstScreens[0]?.["full"], [16384, 0, 800, 600]);

        const firstRawReply = lastDescribeReply(h);
        const firstReply = parseWorldReply(JSON.stringify(firstRawReply));
        assert.ok(isCoreWorldReply(firstReply), "TS parseWorldReply accepts broker reply");
        assert.equal(firstReply.ready, true);
        const firstFrame = firstReply.windows[0]?.frame;
        assert.ok(firstFrame !== null && firstFrame !== undefined, "broker reply carries window frame");
        assert.equal(firstFrame[0], 16392, "x=16384 with outer inset 8 publishes x=16392");
        appliedMatchesReply(h, firstRawReply);

        // Answer the held effect-facts waiter so the next capture carries
        // causal effect observation and emits a real visible SetPresentation.
        const winA = winById(h, "win-a");
        assert.equal(winA.frameGeometry.x, firstFrame[0]);
        const callsBefore = h.calls.length;
        answerEffectFacts(h, "5", true);
        const nextIdx = await awaitDescribeFrom(h, callsBefore);
        await h.flushAt(nextIdx);
        h.runTimers();

        const nextEnv = describeEnvelopeAt(h, nextIdx);
        const nextReport = nextEnv["report"] as Record<string, unknown>;
        const nextWins = nextReport["windows"] as Array<Record<string, unknown>>;
        assert.deepEqual(nextWins[0]?.["rect"], firstFrame, "next TS capture feeds applied broker frame back");

        const nextReply = parseWorldReply(JSON.stringify(lastDescribeReply(h)));
        assert.ok(isCoreWorldReply(nextReply), "TS parseWorldReply accepts second broker reply");
        assert.deepEqual(nextReply.windows[0]?.frame, firstFrame, "checked broker converges on next capture");

        await flushReads(h);
        const realSets = h.renders.filter((r) => r.method === "SetPresentation");
        const realSet = realSets[realSets.length - 1];
        assert.ok(realSet !== undefined, "entry emits real SetPresentation via h.renders");
        const setObj = JSON.parse(realSet.payload) as Record<string, unknown>;
        const borderObj = setObj["border"] as Record<string, unknown>;
        assert.equal(borderObj["visible"], true, "real SetPresentation border is visible");
        const witnessObj = borderObj["witness"] as Record<string, unknown> | null;
        assert.ok(witnessObj !== null, "real SetPresentation carries non-null witness");
        assert.deepEqual(witnessObj["frame"], firstFrame, "real SetPresentation witness frame equals broker frame");

        return { firstFrame, setPresentationPayload: realSet.payload };
    } finally {
        await h.stop();
    }
}

describe("connected offline coordinate closure (x=16384 inset 8 -> x=16392)", () => {
    it("carries real observation bytes through checked broker, TS reply, next capture, and real SetPresentation", async () => {
        await runConnectedClosure();
    });

    it(
        "native gesture and presentation consumers accept the real broker frame and untouched SetPresentation payload",
        { skip: !hasNativeBins },
        async () => {
            const { firstFrame, setPresentationPayload } = await runConnectedClosure();
            const gestureOut = execFileSync(
                gestureBin as string,
                ["--probe", String(firstFrame[0]), String(firstFrame[1]), String(firstFrame[2]), String(firstFrame[3])],
                { encoding: "utf8" },
            );
            const gestureJson = JSON.parse(gestureOut.trim()) as Record<string, unknown>;
            assert.deepEqual(gestureJson["startRect"], firstFrame, "native gesture consumer accepts broker frame");

            const presOut = execFileSync(presBin as string, ["--probe", setPresentationPayload], { encoding: "utf8" });
            const witnessOut = JSON.parse(presOut.trim()) as unknown;
            assert.deepEqual(witnessOut, firstFrame, "native presentation consumer accepts untouched SetPresentation");
        },
    );
});
