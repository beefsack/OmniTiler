// Native occupancy v3: `native[].occupancy` counts every raw enumerated
// window per ordered desktop (unmanaged, deleted, minimized, non-normal,
// multi-desktop, and sticky windows all count; sticky counts toward every
// ordered desktop). Unreadable membership yields explicit null counts, never
// zero. The encoder carries the exact `occupancy` wire shape; v2 replies
// refuse.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { captureWorldObservation, type WorldObservation } from "../src/world-observe";
import {
    buildPresentationReadRequest,
    buildWorldInnerReportJson,
    parseWorldReply,
} from "../src/world-report";
import { WORLD_CONTRACT_VERSION } from "../src/world-seq";

interface FakeWin {
    id: string;
    desktops?: unknown;
    onAllDesktops?: unknown;
    managed?: unknown;
    deleted?: unknown;
    minimized?: unknown;
    normalWindow?: unknown;
    dialog?: unknown;
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
        hidden: false,
        fullScreen: false,
        maximizeMode: 0,
        minSize: null,
        maxSize: null,
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
    };
    if (win.desktops !== undefined) {
        ref["desktops"] = win.desktops;
    } else {
        ref["desktops"] = [{ id: "ws-1" }];
    }
    if (win.onAllDesktops !== undefined) {
        ref["onAllDesktops"] = win.onAllDesktops;
    } else {
        ref["onAllDesktops"] = false;
    }
    if (win.managed !== undefined) {
        ref["managed"] = win.managed;
    } else {
        ref["managed"] = true;
    }
    if (win.deleted !== undefined) {
        ref["deleted"] = win.deleted;
    } else {
        ref["deleted"] = false;
    }
    if (win.minimized !== undefined) {
        ref["minimized"] = win.minimized;
    }
    if (win.normalWindow !== undefined) {
        ref["normalWindow"] = win.normalWindow;
    }
    if (win.dialog !== undefined) {
        ref["dialog"] = win.dialog;
    }
    return ref;
}

function observe(wins: FakeWin[]): WorldObservation | null {
    const desktops = [{ id: "ws-1" }, { id: "ws-2" }];
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
        knownClosed: () => [],
        logicalBindingOf: (nativeId) => ({ logical: `desk-${nativeId}`, out: "out-1", pin: false, disp: false, managed: true }),
        isFloating: () => false,
    });
}

function occupancyOf(obs: WorldObservation): Array<{ native: string; count: number | null }> {
    assert.equal(obs.native.length, 1);
    assert.deepEqual(obs.native[0]?.order, ["ws-1", "ws-2"]);
    return obs.native[0]?.occupancy ?? [];
}

describe("world v3 contract", () => {
    it("rides contract version 3", () => {
        assert.equal(WORLD_CONTRACT_VERSION, 3);
        const req = buildPresentationReadRequest("read-1");
        assert.ok(req !== null);
        assert.equal((JSON.parse(req) as { v: number }).v, 3);
    });

    it("refuses v2 replies", () => {
        assert.equal(
            parseWorldReply(
                `{"v":2,"outcome":"rejected","kind":"k","message":"m","gen":"abc","source":"","nativeInvalid":"0","clock":"0"}`,
            ),
            null,
        );
        assert.notEqual(
            parseWorldReply(
                `{"v":3,"outcome":"rejected","kind":"k","message":"m","gen":"abc","source":"","nativeInvalid":"0","clock":"0"}`,
            ),
            null,
        );
    });
});

describe("native occupancy observation", () => {
    it("counts every raw window including unmanaged, deleted, minimized, non-normal, multi-desktop, and sticky", () => {
        const obs = observe([
            { id: "w1", desktops: [{ id: "ws-1" }] },
            { id: "w2", desktops: [{ id: "ws-2" }] },
            { id: "w3-unmanaged", managed: false, desktops: [{ id: "ws-1" }] },
            { id: "w4-min", minimized: true, desktops: [{ id: "ws-2" }] },
            { id: "w5-multi", desktops: [{ id: "ws-1" }, { id: "ws-2" }] },
            { id: "w6-sticky", onAllDesktops: true, desktops: [] },
            { id: "w7-sticky-members", onAllDesktops: true, desktops: [{ id: "ws-2" }] },
            { id: "w8-nonnormal", normalWindow: false, dialog: false, desktops: [{ id: "ws-1" }] },
            { id: "w9-gone", managed: true, deleted: true, desktops: [{ id: "ws-1" }] },
        ]);
        assert.ok(obs !== null);
        assert.deepEqual(obs.closed, ["w9-gone"]);
        assert.equal(obs.complete, true);
        assert.deepEqual(occupancyOf(obs), [
            { native: "ws-1", count: 7 },
            { native: "ws-2", count: 5 },
        ]);
    });

    it("rides exact zero for empty desktops", () => {
        const obs = observe([]);
        assert.ok(obs !== null);
        assert.equal(obs.complete, true);
        assert.deepEqual(occupancyOf(obs), [
            { native: "ws-1", count: 0 },
            { native: "ws-2", count: 0 },
        ]);
    });

    it("rides null on unreadable membership instead of zero", () => {
        const obs = observe([
            { id: "w1", desktops: [{ id: "ws-1" }] },
            { id: "w-bad", desktops: [{ id: "has space" }] },
        ]);
        assert.ok(obs !== null);
        assert.deepEqual(occupancyOf(obs), [
            { native: "ws-1", count: null },
            { native: "ws-2", count: null },
        ]);
    });

    it("rides null on non-boolean onAllDesktops", () => {
        const obs = observe([
            { id: "w1", desktops: [{ id: "ws-1" }] },
            { id: "w-bad", onAllDesktops: "yes", desktops: [{ id: "ws-1" }] },
        ]);
        assert.ok(obs !== null);
        assert.deepEqual(occupancyOf(obs), [
            { native: "ws-1", count: null },
            { native: "ws-2", count: null },
        ]);
    });

    it("rides null on duplicate membership ids", () => {
        const obs = observe([
            { id: "w-dup", desktops: [{ id: "ws-1" }, { id: "ws-1" }] },
        ]);
        assert.ok(obs !== null);
        assert.deepEqual(occupancyOf(obs), [
            { native: "ws-1", count: null },
            { native: "ws-2", count: null },
        ]);
    });
});

describe("native occupancy encoding", () => {
    it("emits the exact v3 occupancy wire shape", () => {
        const obs = observe([
            { id: "w1", desktops: [{ id: "ws-1" }] },
            { id: "w-sticky", onAllDesktops: true, desktops: [] },
        ]);
        assert.ok(obs !== null);
        const json = buildWorldInnerReportJson({
            observation: obs,
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
        const body = JSON.parse(json as string) as {
            native: Array<{ out: string; order: string[]; current: string; occupancy: Array<{ native: string; count: number | null }> }>;
        };
        assert.equal(body.native.length, 1);
        assert.deepEqual(body.native[0]?.occupancy, [
            { native: "ws-1", count: 2 },
            { native: "ws-2", count: 1 },
        ]);
    });
});
