import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    DragOraclePull,
    deriveOracleEdge,
    parseDragOracleVerdict,
} from "../src/drag-oracle-pull";

function movedVerdict(): string {
    return JSON.stringify({
        v: 1,
        cancelled: false,
        finalRect: { x: 0, y: 0, w: 1000, h: 800 },
        windowIdentity: "win-a",
        correlation: "drag-9",
        reason: "ok-moved",
    });
}

function cancelledVerdict(): string {
    return JSON.stringify({
        v: 1,
        cancelled: true,
        finalRect: { x: 0, y: 0, w: 600, h: 800 },
        windowIdentity: "win-a",
        correlation: "drag-10",
        reason: "no-change",
    });
}

describe("slice 2 oracle edge derivation", () => {
    it("derives single edges with the opposite fixed and rejects mixed", () => {
        const start = { x: 0, y: 0, w: 600, h: 800 };
        assert.deepEqual(deriveOracleEdge(start, { x: 0, y: 0, w: 1000, h: 800 }), { direction: "right", boundary: 1000 });
        assert.deepEqual(deriveOracleEdge(start, { x: 400, y: 0, w: 200, h: 800 }), { direction: "left", boundary: 400 });
        assert.deepEqual(deriveOracleEdge(start, { x: 0, y: 100, w: 600, h: 700 }), { direction: "up", boundary: 100 });
        assert.deepEqual(deriveOracleEdge(start, { x: 0, y: 0, w: 600, h: 700 }), { direction: "down", boundary: 700 });
        assert.equal(deriveOracleEdge(start, start), null);
        assert.equal(deriveOracleEdge(start, { x: 100, y: 100, w: 500, h: 700 }), "mixed");
    });
});

describe("slice 2 verdict routing contract", () => {
    it("cancelled verdict is a strict no-op with no route call", () => {
        let routed = 0;
        const pull = new DragOraclePull({
            callDbus: (_s, _p, _i, _m, callback): void => {
                callback(cancelledVerdict());
            },
            log: (): void => {},
            routePointer: (): void => {
                routed += 1;
            },
        });
        pull.pullVerdict();
        assert.equal(routed, 0);
        assert.notEqual(parseDragOracleVerdict(cancelledVerdict()), null);
    });

    it("non-cancelled verdict routes exactly once after the D-Bus reply", () => {
        const seen: string[] = [];
        const pull = new DragOraclePull({
            callDbus: (_s, _p, _i, _m, callback): void => {
                callback(movedVerdict());
            },
            log: (): void => {},
            routePointer: (verdict): void => {
                seen.push(verdict.correlation);
            },
        });
        pull.pullVerdict();
        assert.deepEqual(seen, ["drag-9"]);
    });

    it("invalid verdict fails closed with no route call", () => {
        const logs: string[] = [];
        let routed = 0;
        const pull = new DragOraclePull({
            callDbus: (_s, _p, _i, _m, callback): void => {
                callback("garbage");
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
        assert.deepEqual(logs, [
            "omnitiler:route-diag:drag-pull action=dispatch",
            "omnitiler:route-diag:drag-reply-invalid correlation=none",
        ]);
        assert.ok(!logs.some((line) => line === "omnitiler:route-diag:drag-unavailable"));
    });
});

describe("slice 2 oracle completion callback", () => {
    it("notifies after a cancelled verdict without routing", () => {
        const seen: string[] = [];
        let routed = 0;
        const pull = new DragOraclePull({
            callDbus: (_s, _p, _i, _m, callback): void => {
                callback(cancelledVerdict());
            },
            log: (): void => {},
            routePointer: (): void => {
                routed += 1;
            },
            onSettled: (verdict): void => {
                seen.push(verdict === null ? "null" : `${String(verdict.cancelled)}:${verdict.correlation}`);
            },
        });
        pull.pullVerdict();
        assert.equal(routed, 0);
        assert.deepEqual(seen, ["true:drag-10"]);
    });

    it("notifies after a non-cancelled verdict after routing", () => {
        const order: string[] = [];
        const pull = new DragOraclePull({
            callDbus: (_s, _p, _i, _m, callback): void => {
                callback(movedVerdict());
            },
            log: (): void => {},
            routePointer: (): void => {
                order.push("route");
            },
            onSettled: (verdict): void => {
                order.push(`settled:${verdict === null ? "null" : verdict.correlation}`);
            },
        });
        pull.pullVerdict();
        assert.deepEqual(order, ["route", "settled:drag-9"]);
    });

    it("notifies with null for invalid and unavailable verdicts", () => {
        for (const reply of ["garbage", null]) {
            const seen: Array<string | null> = [];
            const pull = new DragOraclePull({
                callDbus: (_s, _p, _i, _m, callback): void => {
                    callback(reply);
                },
                log: (): void => {},
                onSettled: (verdict): void => {
                    seen.push(verdict === null ? null : verdict.correlation);
                },
            });
            pull.pullVerdict();
            assert.deepEqual(seen, [null]);
        }
        const seenThrow: Array<string | null> = [];
        const throwing = new DragOraclePull({
            callDbus: (): void => {
                throw new Error("no-bus");
            },
            log: (): void => {},
            onSettled: (verdict): void => {
                seenThrow.push(verdict === null ? null : verdict.correlation);
            },
        });
        throwing.pullVerdict();
        assert.deepEqual(seenThrow, [null]);
    });
});
