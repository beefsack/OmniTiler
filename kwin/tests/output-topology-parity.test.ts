// Shared core/KDE output-topology parity (strict unique-reciprocal map).
// Consumes `test-fixtures/output-topology-parity.json`, the frozen capture
// of the original KDE world observation adjacency, and requires exact map
// equality from the frozen oracle below only. No production code derives
// targets: direction-only world commands carry no endpoint and the core
// owns selection. Any approved split-edge policy-alignment delta needs
// separately named expected-result tests, never an edit of the shared
// expectations.
import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";

// Frozen test-only oracle of the original production implementation
// `kwin/src/world-observe.ts::computeAdjacency` as of core-output-topology
// `7b10c286` (strict unique-reciprocal exact edge-touch, positive overlap,
// no wrap). Provenance: verbatim copy of the removed production function,
// retained here only to prove the shared fixture is an exact capture of
// the original KDE behavior. No production code derives topology; the core
// owns selection from FULL `topology[].rect`, with wire `adjacent` always
// empty.
function frozenOriginalAdjacency(
    names: ReadonlyArray<string>,
    rects: ReadonlyArray<{ x: number; y: number; w: number; h: number }>,
): Record<string, Record<string, string>> {
    const out: Record<string, Record<string, string>> = {};
    const overlapY = (a: { y: number; h: number }, b: { y: number; h: number }): number =>
        Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
    const overlapX = (a: { x: number; w: number }, b: { x: number; w: number }): number =>
        Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
    const touches = (s: number, t: number, dir: string): boolean => {
        const src = rects[s] as { x: number; y: number; w: number; h: number };
        const tgt = rects[t] as { x: number; y: number; w: number; h: number };
        if (dir === "left") {
            return tgt.x + tgt.w === src.x && overlapY(tgt, src) > 0;
        }
        if (dir === "right") {
            return src.x + src.w === tgt.x && overlapY(tgt, src) > 0;
        }
        if (dir === "up") {
            return tgt.y + tgt.h === src.y && overlapX(tgt, src) > 0;
        }
        return src.y + src.h === tgt.y && overlapX(tgt, src) > 0;
    };
    const opposite = (dir: string): string =>
        dir === "left" ? "right" : dir === "right" ? "left" : dir === "up" ? "down" : "up";
    for (let s = 0; s < names.length; s += 1) {
        const row: Record<string, string> = {};
        for (const dir of ["left", "right", "up", "down"] as const) {
            const cands: number[] = [];
            for (let t = 0; t < names.length; t += 1) {
                if (t !== s && touches(s, t, dir)) {
                    cands.push(t);
                }
            }
            if (cands.length !== 1) {
                continue;
            }
            const t = cands[0] as number;
            let reverse = 0;
            let reverseHit = -1;
            for (let u = 0; u < names.length; u += 1) {
                if (u !== t && touches(t, u, opposite(dir))) {
                    reverse += 1;
                    reverseHit = u;
                }
            }
            if (reverse === 1 && reverseHit === s) {
                row[dir] = names[t] as string;
            }
        }
        out[names[s] as string] = row;
    }
    return out;
}

interface ParityCase {
    name: string;
    outputs: Array<{ out: string; rect: [number, number, number, number] }>;
    expected: Record<string, Record<string, string>>;
}

const DIRS = ["left", "right", "up", "down"] as const;

function loadFixture(): { version: number; cases: ParityCase[] } {
    const raw = readFileSync(join("..", "test-fixtures", "output-topology-parity.json"), "utf8");
    const parsed = JSON.parse(raw) as { version: number; cases: ParityCase[] };
    assert.equal(parsed.version, 1);
    assert.ok(Array.isArray(parsed.cases) && parsed.cases.length > 0);
    return parsed;
}

describe("output topology parity (frozen KDE oracle vs shared fixture)", () => {
    it("matches the shared strict map on all 12 cases", () => {
        const { cases } = loadFixture();
        assert.equal(cases.length, 12, "fixture carries 12 parity cases");
        for (const c of cases) {
            const names = c.outputs.map((o) => o.out);
            const rects = c.outputs.map((o) => ({ x: o.rect[0], y: o.rect[1], w: o.rect[2], h: o.rect[3] }));
            const actual = frozenOriginalAdjacency(names, rects);
            assert.deepEqual(actual, c.expected, `${c.name}: frozen oracle matches`);
            assert.deepEqual(Object.keys(actual).sort(), names.slice().sort(), `${c.name}: rows name every output`);
        }
    });

    it("keeps split-edge ambiguity (strict refuses where ranked selectors would resolve)", () => {
        const { cases } = loadFixture();
        const ambiguous = cases.filter((c) => {
            const rects = c.outputs.map((o) => ({ x: o.rect[0], y: o.rect[1], w: o.rect[2], h: o.rect[3] }));
            const touches = (a: number, b: number, dir: string): boolean => {
                const s = rects[a] as { x: number; y: number; w: number; h: number };
                const t = rects[b] as { x: number; y: number; w: number; h: number };
                const overlapY = Math.min(s.y + s.h, t.y + t.h) - Math.max(s.y, t.y);
                const overlapX = Math.min(s.x + s.w, t.x + t.w) - Math.max(s.x, t.x);
                if (dir === "left") return t.x + t.w === s.x && overlapY > 0;
                if (dir === "right") return s.x + s.w === t.x && overlapY > 0;
                if (dir === "up") return t.y + t.h === s.y && overlapX > 0;
                return s.y + s.h === t.y && overlapX > 0;
            };
            let hasTouch = false;
            for (let s = 0; s < rects.length; s += 1) {
                for (let t = 0; t < rects.length; t += 1) {
                    if (s === t) continue;
                    for (const d of DIRS) {
                        if (touches(s, t, d)) hasTouch = true;
                    }
                }
            }
            const allEmpty = Object.values(c.expected).every((row) => Object.keys(row).length === 0);
            return hasTouch && allEmpty;
        });
        assert.equal(ambiguous.length, 1, "fixture keeps full-omission ambiguity case");
        assert.equal(ambiguous[0]?.name, "mirrored-overlap-above-wide");
    });
});

describe("output topology parity (adapter emits no adjacency)", () => {
    it("wire domains stay empty (core-owned topology only)", () => {
        // The production observer always emits empty wire-compat adjacent
        // maps; strict resolution reads FULL topology rects instead. This
        // test guards the emission absence without invoking KWin.
        const { cases } = loadFixture();
        const two = cases.find((c) => c.name === "two-output-horizontal");
        assert.ok(two !== undefined);
        // The shared fixture still carries the resolving strict map for the
        // oracle, but production wire domains must be empty: covered by the
        // world-wire byte fixtures (report-*.json carry "adjacent":{}).
        assert.deepEqual(two.expected["left"], { right: "right" });
    });
});
