// D2/D3 capacity settings offline coverage: tolerant TS readers with
// defaults, live per-dispatch re-read timing, and revisioned settings-lane
// carriage (never the report lane). Included in `npm test`.

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    ANCHOR_FALLBACK_DEFAULT,
    createWorldSettingsLive,
    normalizeAnchorFallback,
    normalizeNoSpaceResult,
    NO_SPACE_RESULT_DEFAULT,
} from "../src/world-settings-live";
import { buildSettingsRequest, parseSettingsReply } from "../src/world-connection";
import { buildWorldReportV4InnerJson } from "../src/world-report";
import { captureWorldObservation } from "../src/world-observe";

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

function v4Observation() {
    const win = {
        internalId: "win-a",
        normalWindow: true,
        dialog: false,
        managed: true,
        deleted: false,
        transient: false,
        transientFor: null,
        modal: false,
        wantsInput: true,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minimized: false,
        hidden: false,
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
        clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
        output: { name: "out-a" },
        desktops: [{ id: "native-a" }],
        pid: 1234,
        resourceClass: "Example",
        caption: "Window",
        windowRole: "normal",
        desktopFileName: "example.app",
        resizeable: true,
        keepAbove: false,
        keepBelow: false,
    };
    const surface = {
        windowList: () => [win],
        screens: [{ name: "out-a", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [{ id: "native-a" }],
        activeScreen: { name: "out-a" },
        activeWindow: win,
        cursorPos: { x: 20, y: 30 },
        windowAt: () => [win],
        stackingOrder: [win],
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1040 }),
        currentDesktopForScreen: () => ({ id: "native-a" }),
    };
    const obs = captureWorldObservation({
        workspace: surface,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 8,
        gestureActive: () => false,
        knownClosed: () => [],
        logicalBindingOf: () => ({ logical: "desk-native-a", out: "out-a", pin: false, disp: false, managed: true }),
        isFloating: () => false,
    });
    assert.ok(obs !== null);
    return obs;
}

describe("capacity settings connection carriage", () => {
    it("rides the separate revisioned settings snapshot, never the report lane", () => {
        const settings = createWorldSettingsLive({
            readAnchorFallbackFn: () => "closest-splittable-relative",
            readNoSpaceResultFn: () => "allow-overlap",
        });
        const values = settings.snapshotConnectionValues();
        const byKey = new Map(values);
        assert.equal(byKey.get("anchorFallback"), "closest-splittable-relative");
        assert.equal(byKey.get("noSpaceResult"), "allow-overlap");
        // Production settings encoding with a synthetic acceptance.
        const req = buildSettingsRequest("test-adapter", "gen-1", "set-1", "1", values);
        assert.ok(req !== null);
        const sentValues = (JSON.parse(req) as Record<string, unknown>)["values"] as Record<string, unknown>;
        assert.equal(sentValues["anchorFallback"], "closest-splittable-relative");
        assert.equal(sentValues["noSpaceResult"], "allow-overlap");
        const reply = JSON.stringify({
            type: "settings-reply", v: 4, adapter: "test-adapter", gen: "gen-1",
            req: "set-1", rev: "1", status: "accepted", diagnostics: [],
        });
        assert.deepEqual(parseSettingsReply(reply)?.status, "accepted");
        // The v4 production encoder carries no capacity settings: the
        // inner report omits every retired settings/policy key.
        const json = buildWorldReportV4InnerJson({
            observation: v4Observation(),
            creationAttempts: [],
            frameAttempts: [],
            memberAttempts: [],
            viewAttempts: [],
            clearAttempts: [],
            intentRecords: [{ win: "win-a", data: "schema2-opaque" }],
        });
        assert.ok(json !== null, "genuine v4 producer bytes encode");
        const body = JSON.parse(json as string) as Record<string, unknown>;
        assert.equal(body["v"], 4);
        for (const key of ["anchorFallback", "noSpaceResult", "gap", "outer", "mode", "sameAxis", "refill", "defaultTiled", "predicate", "bornMaximized"]) {
            assert.ok(!(key in body), `v4 inner report carries no ${key}`);
        }
        for (const key of ["owner", "adapter", "gen", "obs", "reqSeq", "req", "cmdSeq", "command", "monotonicMs"]) {
            assert.ok(!(key in body), `inner report carries no ${key}`);
        }
        // Opaque schema2 intent records ride verbatim (never member/tile verdicts).
        assert.ok(json.includes('"intentRecords":[{"win":"win-a","data":"schema2-opaque"}'));
    });

    it("round-trips the validated fallback alphabet through the settings lane", () => {
        const settings = createWorldSettingsLive({
            readAnchorFallbackFn: () => "bogus",
            readNoSpaceResultFn: () => "bogus",
        });
        const byKey = new Map(settings.snapshotConnectionValues());
        assert.equal(byKey.get("anchorFallback"), ANCHOR_FALLBACK_DEFAULT);
        assert.equal(byKey.get("noSpaceResult"), NO_SPACE_RESULT_DEFAULT);
        const text = buildSettingsRequest("test-adapter", "gen-1", "set-2", "2", settings.snapshotConnectionValues());
        assert.ok(text !== null);
        assert.ok((text as string).includes('"anchorFallback":"largest-splittable-window"'));
        assert.ok((text as string).includes('"noSpaceResult":"float"'));
    });
});
