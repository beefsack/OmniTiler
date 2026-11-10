// Sticky occupancy exclusion: on-all-desktops windows (panels, docks, user
// sticky) never occupy any native desktop in either KDE counter:
// `native[].occupancy` (world-observe.ts) and `desktopOccupants`
// (world-native.ts). Ordinary occupants (managed or unmanaged) and
// non-sticky subset memberships still veto with exact counts; unreadable
// membership still rides null/unreadable, never zero. Core sticky-home
// retention is unchanged (see the retirement integration test).

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { captureWorldObservation, type WorldObservation } from "../src/world-observe";
import { createWorldApplyHost } from "../src/world-native";

interface FakeWin {
    id: string;
    desktops?: unknown;
    onAllDesktops?: unknown;
    managed?: unknown;
    minimized?: unknown;
    normalWindow?: unknown;
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
    ref["desktops"] = win.desktops !== undefined ? win.desktops : [{ id: "ws-1" }];
    ref["onAllDesktops"] = win.onAllDesktops !== undefined ? win.onAllDesktops : false;
    ref["managed"] = win.managed !== undefined ? win.managed : true;
    ref["deleted"] = false;
    if (win.minimized !== undefined) {
        ref["minimized"] = win.minimized;
    }
    if (win.normalWindow !== undefined) {
        ref["normalWindow"] = win.normalWindow;
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

function occupantsOf(wins: Array<Record<string, unknown>>, nativeId: string): { total: number; readable: boolean } {
    const host = createWorldApplyHost({ windowList: () => wins }, (v, p) => Reflect.get(v as object, p as string));
    return host.desktopOccupants(nativeId);
}

describe("sticky occupancy exclusion in native[].occupancy", () => {
    it("sticky panels and user-sticky windows occupy nothing while ordinary and subset occupants count", () => {
        const obs = observe([
            { id: "w-plain", desktops: [{ id: "ws-1" }] },
            { id: "panel-sticky", managed: false, onAllDesktops: true, desktops: [] },
            { id: "user-sticky", onAllDesktops: true, desktops: [] },
            { id: "unmanaged-plain", managed: false, desktops: [{ id: "ws-2" }] },
            { id: "w-subset", desktops: [{ id: "ws-1" }, { id: "ws-2" }] },
        ]);
        assert.ok(obs !== null);
        assert.equal(obs.complete, true);
        assert.deepEqual(occupancyOf(obs), [
            { native: "ws-1", count: 2 },
            { native: "ws-2", count: 2 },
        ]);
    });

    it("sticky with a leftover member list still occupies nothing", () => {
        const obs = observe([
            { id: "w-sticky-members", onAllDesktops: true, desktops: [{ id: "ws-2" }] },
        ]);
        assert.ok(obs !== null);
        assert.equal(obs.complete, true);
        assert.deepEqual(occupancyOf(obs), [
            { native: "ws-1", count: 0 },
            { native: "ws-2", count: 0 },
        ]);
    });

    it("empty-member non-sticky fails safe as an incomplete report, never empty counts", () => {
        const obs = observe([
            { id: "w-empty", onAllDesktops: false, desktops: [] },
        ]);
        assert.ok(obs !== null);
        assert.equal(obs.complete, false);
    });

    it("non-boolean onAllDesktops still rides null instead of zero", () => {
        const obs = observe([
            { id: "w-plain", desktops: [{ id: "ws-1" }] },
            { id: "w-bad", onAllDesktops: "yes", desktops: [{ id: "ws-1" }] },
        ]);
        assert.ok(obs !== null);
        assert.deepEqual(occupancyOf(obs), [
            { native: "ws-1", count: null },
            { native: "ws-2", count: null },
        ]);
    });
});

describe("sticky occupancy exclusion in desktopOccupants", () => {
    it("sticky-only windows prove empty on every desktop", () => {
        const wins = [
            { onAllDesktops: true, desktops: [] },
            { managed: false, onAllDesktops: true, desktops: [] },
            { onAllDesktops: true, desktops: [{ id: "ws-1" }] },
        ];
        assert.deepEqual(occupantsOf(wins, "ws-1"), { total: 0, readable: true });
        assert.deepEqual(occupantsOf(wins, "ws-2"), { total: 0, readable: true });
    });

    it("ordinary occupants still veto on their own desktop only", () => {
        const wins = [{ onAllDesktops: false, desktops: [{ id: "ws-1" }] }];
        assert.deepEqual(occupantsOf(wins, "ws-1"), { total: 1, readable: true });
        assert.deepEqual(occupantsOf(wins, "ws-2"), { total: 0, readable: true });
    });

    it("unmanaged ordinary and subset memberships still veto", () => {
        const wins = [
            { managed: false, onAllDesktops: false, desktops: [{ id: "ws-2" }] },
            { onAllDesktops: false, desktops: [{ id: "ws-1" }, { id: "ws-2" }] },
        ];
        assert.deepEqual(occupantsOf(wins, "ws-1"), { total: 1, readable: true });
        assert.deepEqual(occupantsOf(wins, "ws-2"), { total: 2, readable: true });
    });

    it("unreadable and empty-list shapes never prove empty", () => {
        assert.deepEqual(
            occupantsOf([{ onAllDesktops: null, desktops: [{ id: "ws-1" }] }], "ws-1"),
            { total: 0, readable: false },
        );
        assert.deepEqual(
            occupantsOf([{ onAllDesktops: false, desktops: [] }], "ws-1"),
            { total: 1, readable: true },
        );
        assert.deepEqual(
            occupantsOf([{ onAllDesktops: false, desktops: [null] }], "ws-1"),
            { total: 0, readable: false },
        );
    });
});
