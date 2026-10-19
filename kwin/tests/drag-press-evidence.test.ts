import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    DragOraclePull,
    identifyGrabbedEdges,
    identifyPressGrabbed,
    parseDragOracleVerdict,
} from "../src/drag-oracle-pull";

function verdictBase(finalRect: { x: number; y: number; w: number; h: number }, correlation: string): Record<string, unknown> {
    return {
        v: 1,
        cancelled: false,
        finalRect,
        windowIdentity: "win-a",
        correlation,
        reason: "ok-moved",
    };
}

describe("drag press strict parse", () => {
    it("accepts the legacy six-field verdict with no press", () => {
        const parsed = parseDragOracleVerdict(JSON.stringify(verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-1")));
        assert.ok(parsed !== null);
        assert.equal(parsed.press, undefined);
    });

    it("accepts the seven-field verdict with an exact bounded press", () => {
        for (const binding of ["configured", "default"]) {
            const body = { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-2"), press: { x: 250.5, y: 100.25, binding } };
            const parsed = parseDragOracleVerdict(JSON.stringify(body));
            assert.ok(parsed !== null);
            assert.deepEqual(parsed.press, { x: 250.5, y: 100.25, binding });
        }
    });

    it("rejects unknown top-level keys and malformed press objects", () => {
        const good = { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-3"), press: { x: 1, y: 2, binding: "configured" } };
        const cases: unknown[] = [
            // Unknown seventh key instead of press.
            { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-3"), extra: 1 },
            // Eighth key alongside a valid press.
            { ...good, extra: 1 },
            // Press shape violations.
            { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-3"), press: { x: 1, y: 2 } },
            { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-3"), press: { x: 1, y: 2, binding: "configured", z: 0 } },
            { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-3"), press: { x: "1", y: 2, binding: "configured" } },
            { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-3"), press: { x: 1, y: 2, binding: "live" } },
            { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-3"), press: { x: 16384.5, y: 2, binding: "configured" } },
            { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-3"), press: { x: 1, y: -16385, binding: "default" } },
            { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-3"), press: null },
            { ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-3"), press: [1, 2] },
            // Overflow literal parses to a non-finite double.
            `{"v":1,"cancelled":false,"finalRect":{"x":5,"y":0,"w":100,"h":100},"windowIdentity":"win-a","correlation":"drag-3","reason":"ok-moved","press":{"x":1e999,"y":2,"binding":"configured"}}`,
        ];
        for (const reply of cases) {
            assert.equal(parseDragOracleVerdict(typeof reply === "string" ? reply : JSON.stringify(reply)), null, JSON.stringify(reply).slice(0, 120));
        }
    });

    it("fails a malformed press closed at the pull with no route call", () => {
        const logs: string[] = [];
        let routed = 0;
        const pull = new DragOraclePull({
            callDbus: (_s, _p, _i, _m, callback): void => {
                callback(JSON.stringify({ ...verdictBase({ x: 5, y: 0, w: 100, h: 100 }, "drag-4"), press: { x: 1, y: 2, binding: "live" } }));
            },
            log: (message): void => {
                logs.push(message);
            },
            routePointer: (): void => {
                routed += 1;
            },
        });
        pull.pullVerdict();
        assert.equal(routed, 0);
        assert.ok(logs.some((line) => line === "omnitiler:route-diag:drag-reply-invalid correlation=none"));
    });
});

describe("drag press thirds classification", () => {
    // Recorded trace values (omnitiler-dev log): drag-27 started
    // 558,52,970,315 with pointerStart 727,318, which the 64px Started gate
    // reads as single-axis down while KWin moved left and bottom.
    it("classifies recorded drag-27 by thirds while the Started gate reads single-axis", () => {
        const start = { x: 558, y: 52, w: 970, h: 315 };
        const pointer = { x: 727, y: 318 };
        const started = identifyGrabbedEdges(start, pointer);
        assert.ok(started !== null);
        assert.equal(started.source, "nearest-pointer");
        assert.deepEqual(started.grabbed, { horizontal: null, vertical: "down" });
        const pressed = identifyPressGrabbed(start, { x: 727, y: 318, binding: "configured" });
        assert.ok(pressed !== null);
        assert.equal(pressed.source, "kwin-thirds");
        assert.deepEqual(pressed.grabbed, { horizontal: "left", vertical: "down" });
    });

    // Recorded drag-33 started 558,52,970,716 with pointerStart 604,714,
    // which the gate reads as single-axis left. Outside presses stay null
    // so the caller keeps the Started capture.
    it("classifies recorded drag-33 by thirds and rejects outside presses", () => {
        const start = { x: 558, y: 52, w: 970, h: 716 };
        const started = identifyGrabbedEdges(start, { x: 604, y: 714 });
        assert.ok(started !== null);
        assert.deepEqual(started.grabbed, { horizontal: "left", vertical: null });
        const pressed = identifyPressGrabbed(start, { x: 604, y: 714, binding: "configured" });
        assert.ok(pressed !== null);
        assert.deepEqual(pressed.grabbed, { horizontal: "left", vertical: "down" });
        assert.equal(identifyPressGrabbed(start, { x: 0, y: 0, binding: "configured" }), null);
        assert.equal(identifyPressGrabbed(start, null), null);
    });
});
