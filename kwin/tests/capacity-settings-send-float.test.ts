// Capacity settings offline coverage: revisioned settings-lane carriage
// (never the report lane) and absence of retired anchorFallback and
// noSpaceResult keys. Included in `npm test`.

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { createWorldSettingsLive } from "../src/world-settings-live";
import { buildSettingsRequest, parseSettingsReply } from "../src/world-connection";
import { buildWorldReportV4InnerJson } from "../src/world-report";
import { captureWorldObservation } from "../src/world-observe";

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
        const settings = createWorldSettingsLive();
        const values = settings.snapshotConnectionValues();
        const byKey = new Map(values);
        assert.ok(!byKey.has("anchorFallback"), "removed anchorFallback must not be emitted");
        assert.ok(!byKey.has("noSpaceResult"), "removed noSpaceResult must not be emitted");
        // Production settings encoding with a synthetic acceptance.
        const req = buildSettingsRequest("test-adapter", "gen-1", "set-1", "1", values);
        assert.ok(req !== null);
        const sentValues = (JSON.parse(req) as Record<string, unknown>)["values"] as Record<string, unknown>;
        assert.ok(!("anchorFallback" in sentValues), "removed anchorFallback must not be sent");
        assert.ok(!("noSpaceResult" in sentValues), "removed noSpaceResult must not be sent");
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
            stateAttempts: [],
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

    it("ignores stored legacy anchorFallback and noSpaceResult in readConfig while retaining valid siblings", () => {
        const readCalls: string[] = [];
        const g = globalThis as Record<string, unknown>;
        const prevReadConfig = g["readConfig"];
        g["readConfig"] = (key: string, fallback: unknown) => {
            readCalls.push(key);
            if (key === "anchorFallback") {
                return "no-search";
            }
            if (key === "noSpaceResult") {
                return "allow-overlap";
            }
            if (key === "defaultTiled") {
                return false;
            }
            return fallback;
        };
        try {
            const settings = createWorldSettingsLive();
            const byKey = new Map(settings.snapshotConnectionValues());
            assert.ok(!readCalls.includes("anchorFallback"), "removed stored anchorFallback must not be read");
            assert.ok(!readCalls.includes("noSpaceResult"), "removed stored noSpaceResult must not be read");
            assert.ok(!byKey.has("anchorFallback"), "removed anchorFallback must not be emitted");
            assert.ok(!byKey.has("noSpaceResult"), "removed noSpaceResult must not be emitted");
            assert.equal(byKey.get("defaultTiled"), false, "valid sibling retained");
        } finally {
            if (prevReadConfig === undefined) {
                delete g["readConfig"];
            } else {
                g["readConfig"] = prevReadConfig;
            }
        }
    });
});
