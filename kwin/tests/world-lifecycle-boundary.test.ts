// Native lifecycle boundary carriage (deleted closes, unmanaged absent):
// the producer carries raw `managed`/`deleted` facts mechanically and
// converts enumerated-but-gone objects to the explicit close lane on
// identity plus known lifecycle facts alone, never holding the scope on
// unreadable geometry or membership.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { captureWorldObservation, type WorldObservation } from "../src/world-observe";
import { buildWorldInnerReportJson } from "../src/world-report";

interface FakeWin {
    id: string;
    managed?: unknown;
    deleted?: unknown;
    frameGeometry?: unknown;
    desktops?: unknown;
}

function fakeRef(win: FakeWin): object {
    const ref: Record<string, unknown> = {
        internalId: win.id,
        transient: false,
        transientFor: null,
        dialog: false,
        modal: false,
        wantsInput: true,
        normalWindow: true,
        output: { name: "out-1" },
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
    };
    // Explicit undefined means absent (never a default): unknown lifecycle
    // facts must hold, never fabricate. Other lanes default present.
    if (win.managed !== undefined) {
        ref["managed"] = win.managed;
    }
    if (win.deleted !== undefined) {
        ref["deleted"] = win.deleted;
    }
    if (win.frameGeometry !== undefined) {
        ref["frameGeometry"] = win.frameGeometry;
    } else {
        ref["frameGeometry"] = { x: 0, y: 0, width: 200, height: 200 };
    }
    if (win.desktops !== undefined) {
        ref["desktops"] = win.desktops;
    } else {
        ref["desktops"] = [{ id: "ws-1" }];
    }
    return ref;
}

function observe(wins: FakeWin[], knownClosed: string[] = []): WorldObservation | null {
    const desktops = [{ id: "ws-1" }];
    const screens = [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const refs = wins.map(fakeRef);
    const surface = {
        windowList: () => refs,
        screens,
        desktops,
        activeScreen: { name: "out-1" },
        activeWindow: null,
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => desktops[0],
    };
    return captureWorldObservation({
        workspace: surface,
        readMode: () => "per-output-local",
        readGap: () => 8,
        readOuterGap: () => 8,
        gestureActive: () => false,
        knownClosed: () => knownClosed,
        logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-1", pin: false, disp: false, managed: true }),
        isFloating: () => false,
    });
}

function minimalObservation(): WorldObservation {
    const obs = observe([]);
    assert.ok(obs !== null);
    return obs;
}

describe("lifecycle boundary observation carriage", () => {
    it("closes enumerated deleted windows on identity plus lifecycle alone", () => {
        const obs = observe([{ id: "win-gone", managed: true, deleted: true }]);
        assert.ok(obs !== null);
        assert.deepEqual(obs.closed, ["win-gone"]);
        assert.ok(obs.windowFacts.every((w) => w.win !== "win-gone"), "deleted never a candidate");
        assert.equal(obs.complete, true, "gone-object close never holds the scope");
    });

    it("closes deleted windows with unreadable geometry without holding", () => {
        const obs = observe([{ id: "win-gone", managed: true, deleted: true, frameGeometry: null }]);
        assert.ok(obs !== null);
        assert.deepEqual(obs.closed, ["win-gone"]);
        assert.equal(obs.windowFacts.length, 0);
        assert.equal(obs.complete, true, "meaningless gone geometry never holds");
    });

    it("closes deleted windows with unreadable membership without holding", () => {
        const obs = observe([{ id: "win-gone", managed: true, deleted: true, desktops: [{ id: "has space" }] }]);
        assert.ok(obs !== null);
        assert.deepEqual(obs.closed, ["win-gone"]);
        assert.equal(obs.complete, true, "unprovable gone membership never holds");
    });

    it("carries unmanaged rows with flags intact and no close", () => {
        const obs = observe([{ id: "win-popup", managed: false, deleted: false }]);
        assert.ok(obs !== null);
        const row = obs.windowFacts.find((w) => w.win === "win-popup");
        assert.ok(row !== undefined, "unmanaged rides the raw lane for broker normalization");
        assert.equal(row.managed, false);
        assert.equal(row.deleted, false);
        assert.deepEqual(obs.closed, [], "unmanaged absence reports no close");
        assert.equal(obs.complete, true);
    });

    it("gives deleted precedence over unmanaged", () => {
        const obs = observe([{ id: "win-gone", managed: false, deleted: true }]);
        assert.ok(obs !== null);
        assert.deepEqual(obs.closed, ["win-gone"]);
        assert.ok(obs.windowFacts.every((w) => w.win !== "win-gone"));
    });

    it("dedups explicit and native closes", () => {
        const obs = observe([{ id: "win-gone", managed: true, deleted: true }], ["win-gone"]);
        assert.ok(obs !== null);
        assert.deepEqual(obs.closed, ["win-gone"]);
    });

    it("holds on unknown lifecycle booleans instead of fabricating", () => {
        const missing = observe([{ id: "win-x", managed: undefined }]);
        assert.ok(missing !== null);
        assert.equal(missing.complete, false, "unknown managed holds");
        assert.deepEqual(missing.closed, [], "unknown lifecycle closes nothing");
        // A missing-managed row is dropped by the hold (no rawFacts, no close).
        assert.ok(missing.windowFacts.every((w) => w.win !== "win-x"));
    });
});

describe("lifecycle boundary report carriage", () => {
    it("keeps raw managed and deleted flags in the capture envelope", () => {
        const obs = minimalObservation();
        const row = {
            win: "win-popup",
            out: "out-1",
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: null,
            minSize: null,
            maxSize: null,
            desktops: ["ws-1"],
            normalWindow: true,
            managed: false,
            deleted: false,
            transient: false,
            transientFor: null,
            dialog: false,
            modal: false,
            wantsInput: true,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minimized: false,
        };
        const json = buildWorldInnerReportJson({
            observation: { ...obs, windowFacts: [row] },
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
        const body = JSON.parse(json as string) as { windows: Array<Record<string, unknown>> };
        assert.equal(body.windows.length, 1);
        assert.equal(body.windows[0]?.["managed"], false);
        assert.equal(body.windows[0]?.["deleted"], false);
    });
});
