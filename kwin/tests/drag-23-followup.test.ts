import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    ORACLE_KWIN_THIRDS_MIN_EDGE_PX,
    identifyGrabbedEdges,
    resolveOracleResizeTargets,
} from "../src/drag-oracle-pull";

describe("drag-23 kwin-thirds grab identification", () => {
    it("uses the 64px interior gate as the regime boundary", () => {
        assert.equal(ORACLE_KWIN_THIRDS_MIN_EDGE_PX, 64);
        const start = { x: 0, y: 0, w: 600, h: 800 };
        assert.equal(identifyGrabbedEdges(start, { x: 64, y: 400 })?.source, "kwin-thirds");
        assert.equal(identifyGrabbedEdges(start, { x: 63, y: 400 })?.source, "nearest-pointer");
        assert.deepEqual(identifyGrabbedEdges(start, { x: 63, y: 400 })?.grabbed, { horizontal: "left", vertical: null });
    });

    it("classifies the drag-23 interior press as KWin TopLeft", () => {
        const start = { x: 680, y: 538, w: 848, h: 478 };
        const identified = identifyGrabbedEdges(start, { x: 828, y: 647 });
        assert.deepEqual(identified?.grabbed, { horizontal: "left", vertical: "up" });
        assert.equal(identified?.source, "kwin-thirds");
        const resolved = resolveOracleResizeTargets(start, { x: 397, y: 315, w: 1131, h: 701 }, identified?.grabbed ?? { horizontal: null, vertical: null });
        assert.deepEqual(resolved?.targets, [
            { direction: "left", boundary: 397 },
            { direction: "up", boundary: 315 },
        ]);
        assert.deepEqual(resolved?.ignored, []);
    });

    it("covers all 9 thirds zones including the y-center half branch", () => {
        const start = { x: 0, y: 0, w: 900, h: 600 };
        const at = (x: number, y: number) => identifyGrabbedEdges(start, { x, y });
        assert.deepEqual(at(100, 100)?.grabbed, { horizontal: "left", vertical: "up" });
        assert.deepEqual(at(450, 100)?.grabbed, { horizontal: null, vertical: "up" });
        assert.deepEqual(at(800, 100)?.grabbed, { horizontal: "right", vertical: "up" });
        assert.deepEqual(at(100, 300)?.grabbed, { horizontal: "left", vertical: null });
        assert.deepEqual(at(400, 300)?.grabbed, { horizontal: "left", vertical: null });
        assert.deepEqual(at(500, 300)?.grabbed, { horizontal: "right", vertical: null });
        assert.deepEqual(at(800, 300)?.grabbed, { horizontal: "right", vertical: null });
        assert.deepEqual(at(100, 500)?.grabbed, { horizontal: "left", vertical: "down" });
        assert.deepEqual(at(450, 500)?.grabbed, { horizontal: null, vertical: "down" });
        assert.deepEqual(at(800, 500)?.grabbed, { horizontal: "right", vertical: "down" });
        for (const point of [[100, 100], [450, 100], [800, 500], [500, 300]] as const) {
            assert.equal(at(point[0], point[1])?.source, "kwin-thirds");
        }
    });

    it("honours strict boundary ties (x thirds, y thirds, center half)", () => {
        const start = { x: 0, y: 0, w: 900, h: 600 };
        // Float thresholds: thirdW=300, twoThirdW=600, thirdH=200,
        // twoThirdH=400, halfW=450.
        assert.deepEqual(identifyGrabbedEdges(start, { x: 300, y: 100 })?.grabbed, { horizontal: null, vertical: "up" });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 600, y: 100 })?.grabbed, { horizontal: "right", vertical: "up" });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 100, y: 200 })?.grabbed, { horizontal: "left", vertical: null });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 100, y: 400 })?.grabbed, { horizontal: "left", vertical: "down" });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 450, y: 300 })?.grabbed, { horizontal: "right", vertical: null });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 449, y: 300 })?.grabbed, { horizontal: "left", vertical: null });
    });

    it("compares integer offsets against float thirds for non-divisible sizes", () => {
        // w=848: third=282.67, twoThird=565.33, half=424; h=478: third=159.33,
        // twoThird=318.67. Integer-truncated thirds would misclassify lx=282
        // (282 < 282.67 is left) and lx=565 (565 < 565.33 is center).
        const start = { x: 0, y: 0, w: 848, h: 478 };
        assert.deepEqual(identifyGrabbedEdges(start, { x: 282, y: 100 })?.grabbed, { horizontal: "left", vertical: "up" });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 565, y: 100 })?.grabbed, { horizontal: null, vertical: "up" });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 100, y: 318 })?.grabbed, { horizontal: "left", vertical: null });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 100, y: 319 })?.grabbed, { horizontal: "left", vertical: "down" });
        for (const point of [[282, 100], [565, 100], [100, 318]] as const) {
            assert.equal(identifyGrabbedEdges(start, { x: point[0], y: point[1] })?.source, "kwin-thirds");
        }
    });

    it("never selects thirds for a pointer outside the starting frame", () => {
        const start = { x: 0, y: 0, w: 600, h: 800 };
        // Absolute distances beyond the 64px gate, but outside: nearest edge.
        assert.deepEqual(identifyGrabbedEdges(start, { x: -100, y: 400 })?.grabbed, { horizontal: "left", vertical: null });
        assert.equal(identifyGrabbedEdges(start, { x: -100, y: 400 })?.source, "nearest-pointer");
        assert.deepEqual(identifyGrabbedEdges(start, { x: 700, y: 400 })?.grabbed, { horizontal: "right", vertical: null });
        assert.equal(identifyGrabbedEdges(start, { x: 700, y: 400 })?.source, "nearest-pointer");
        assert.deepEqual(identifyGrabbedEdges(start, { x: 300, y: -100 })?.grabbed, { horizontal: null, vertical: "up" });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 300, y: 900 })?.grabbed, { horizontal: null, vertical: "down" });
    });

    it("keeps ordinary near-frame edge and corner grips on nearest-pointer", () => {
        const start = { x: 0, y: 0, w: 600, h: 800 };
        assert.deepEqual(identifyGrabbedEdges(start, { x: 5, y: 400 })?.grabbed, { horizontal: "left", vertical: null });
        assert.equal(identifyGrabbedEdges(start, { x: 5, y: 400 })?.source, "nearest-pointer");
        assert.deepEqual(identifyGrabbedEdges(start, { x: 595, y: 400 })?.grabbed, { horizontal: "right", vertical: null });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 300, y: 5 })?.grabbed, { horizontal: null, vertical: "up" });
        assert.deepEqual(identifyGrabbedEdges(start, { x: 5, y: 5 })?.grabbed, { horizontal: "left", vertical: "up" });
        assert.equal(identifyGrabbedEdges(start, { x: 5, y: 5 })?.source, "nearest-pointer");
    });
});
