// D2/D3 capacity settings offline coverage: tolerant TS readers with
// defaults, live per-dispatch re-read timing, and report JSON carriage.
// Included in `npm test`.

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    ANCHOR_FALLBACK_DEFAULT,
    createWorldSettingsLive,
    normalizeAnchorFallback,
    normalizeNoSpaceResult,
    NO_SPACE_RESULT_DEFAULT,
} from "../src/world-settings-live";
import { buildWorldInnerReportJson } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";

describe("capacity settings tolerant readers", () => {
    it("default to largest-splittable-window and float", () => {
        assert.equal(ANCHOR_FALLBACK_DEFAULT, "largest-splittable-window");
        assert.equal(NO_SPACE_RESULT_DEFAULT, "float");
        assert.equal(normalizeAnchorFallback(undefined), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeNoSpaceResult(undefined), NO_SPACE_RESULT_DEFAULT);
    });

    it("accepts exactly the catalog tokens while rejecting the rest", () => {
        assert.equal(normalizeAnchorFallback("largest-splittable-window"), "largest-splittable-window");
        assert.equal(normalizeAnchorFallback("closest-splittable-relative"), "closest-splittable-relative");
        assert.equal(normalizeAnchorFallback("no-search"), "no-search");
        assert.equal(normalizeAnchorFallback(""), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeAnchorFallback("largest"), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeAnchorFallback("no_search"), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeAnchorFallback(0), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeAnchorFallback(null), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(normalizeNoSpaceResult("float"), "float");
        assert.equal(normalizeNoSpaceResult("allow-overlap"), "allow-overlap");
        assert.equal(normalizeNoSpaceResult(""), NO_SPACE_RESULT_DEFAULT);
        assert.equal(normalizeNoSpaceResult("overlap"), NO_SPACE_RESULT_DEFAULT);
        assert.equal(normalizeNoSpaceResult("allow_overlap"), NO_SPACE_RESULT_DEFAULT);
        assert.equal(normalizeNoSpaceResult(false), NO_SPACE_RESULT_DEFAULT);
    });
});

describe("capacity settings live timing", () => {
    it("re-resolves per dispatch with no restart, tolerating failures", () => {
        let anchor: unknown = "largest-splittable-window";
        let noSpace: unknown = "float";
        const settings = createWorldSettingsLive({
            readAnchorFallbackFn: () => anchor,
            readNoSpaceResultFn: () => noSpace,
        });
        assert.equal(settings.readAnchorFallbackLive(), "largest-splittable-window");
        assert.equal(settings.readNoSpaceResultLive(), "float");
        // A deliberate Options reload applies to the next dispatch.
        anchor = "closest-splittable-relative";
        noSpace = "allow-overlap";
        assert.equal(settings.readAnchorFallbackLive(), "closest-splittable-relative");
        assert.equal(settings.readNoSpaceResultLive(), "allow-overlap");
        // Invalid values backfill instead of sticking.
        anchor = "bogus";
        noSpace = "bogus";
        assert.equal(settings.readAnchorFallbackLive(), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(settings.readNoSpaceResultLive(), NO_SPACE_RESULT_DEFAULT);
    });

    it("falls back when readers throw or config is unreadable", () => {
        const settings = createWorldSettingsLive({
            readAnchorFallbackFn: () => {
                throw new Error("boom");
            },
            readNoSpaceResultFn: () => {
                throw new Error("boom");
            },
        });
        assert.equal(settings.readAnchorFallbackLive(), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(settings.readNoSpaceResultLive(), NO_SPACE_RESULT_DEFAULT);
        // No overrides and no KWin readConfig in node: unreadable config
        // backfills the defaults instead of throwing.
        const bare = createWorldSettingsLive();
        assert.equal(bare.readAnchorFallbackLive(), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(bare.readNoSpaceResultLive(), NO_SPACE_RESULT_DEFAULT);
    });
});

function minimalObservation(): WorldObservation {
    return {
        mode: "per-output-local",
        topology: [],
        bindings: [],
        domains: [],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
        screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

describe("capacity settings report carriage", () => {
    it("encodes validated tokens on every full report", () => {
        const json = buildWorldInnerReportJson({
            observation: minimalObservation(),
            defaultTiled: true,
            refill: "last-remaining-workspace",
            bornMaximized: "tile-on-launch",
            anchorFallback: "closest-splittable-relative",
            noSpaceResult: "allow-overlap",
            creationAttempts: [],
            frameAttempts: [],
            memberAttempts: [],
            viewAttempts: [],
            clearAttempts: [],
        });
        assert.ok(json !== null);
        const body = JSON.parse(json as string) as Record<string, unknown>;
        assert.equal(body["anchorFallback"], "closest-splittable-relative");
        assert.equal(body["noSpaceResult"], "allow-overlap");
        // Strict v2: the broker-owned namespace never rides the inner
        // report; there is exactly one sequencing namespace.
        for (const key of ["owner", "adapter", "gen", "obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs"]) {
            assert.ok(!(key in body), `inner report carries no ${key}`);
        }
        assert.equal(body["v"], 2);
    });

    it("round-trips the documented wire alphabet shape", () => {
        const json = buildWorldInnerReportJson({
            observation: minimalObservation(),
            defaultTiled: true,
            refill: "last-remaining-workspace",
            bornMaximized: "tile-on-launch",
            anchorFallback: "largest-splittable-window",
            noSpaceResult: "float",
            creationAttempts: [],
            frameAttempts: [],
            memberAttempts: [],
            viewAttempts: [],
            clearAttempts: [],
        });
        assert.ok(json !== null);
        const text = json as string;
        assert.ok(text.includes("\"anchorFallback\":\"largest-splittable-window\""));
        assert.ok(text.includes("\"noSpaceResult\":\"float\""));
    });
});
