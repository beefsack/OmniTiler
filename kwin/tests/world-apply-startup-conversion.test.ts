// Per-output-local startup conversion actuation (KDE): a native current
// shown on several outputs is owned by the primary; every other output
// receives its own fresh backing plus membership-only conversion of its
// non-sticky windows from the shared native without changing output,
// preserving float/fullscreen/maximize.
//
// All scenarios drive the PRODUCTION apply loop (`applyWorldReply`) through
// the PRODUCTION native host (`createWorldApplyHost`) with a fake KWin
// surface shaped like the dual-output enable observation (both outputs
// showing ws-1). No live KWin, no D-Bus.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply } from "../src/world-apply";
import { createWorldApplyHost } from "../src/world-native";
import { WorldAdapterState } from "../src/world-state";
import type { WorldReply } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";

const OUT_1 = "out-1";
const OUT_2 = "out-2";
const WS_1 = "ws-1";
const WS_2 = "ws-2";
const WS_3 = "ws-3";
const WS_NEW = "ws-new";
const TOKEN = "wl-1";

interface FakeDesk {
    id: string;
    name: string;
}
interface FakeWin {
    internalId: string;
    output: { name: string };
    desktops: FakeDesk[];
    frameGeometry: { x: number; y: number; width: number; height: number };
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
}

const desk = (id: string, name?: string): FakeDesk => ({ id, name: name ?? id });

function makeSurface(): {
    surface: Record<string, unknown>;
    currents: Map<string, FakeDesk>;
    wins: FakeWin[];
    sendCalls: string[];
    viewCalls: string[];
    setCalls: string[];
    createCalls: Array<unknown[]>;
} {
    const d1 = desk(WS_1);
    const d2 = desk(WS_2);
    const d3 = desk(WS_3);
    const byId = new Map([[WS_1, d1], [WS_2, d2], [WS_3, d3]]);
    const screens = [{ name: OUT_1 }, { name: OUT_2 }];
    const currents = new Map<string, FakeDesk>([[OUT_1, d1], [OUT_2, d1]]);
    const mkWin = (
        internalId: string, out: string, ws: string,
        extra: Partial<Pick<FakeWin, "fullScreen" | "maximizeMode" | "onAllDesktops">> = {},
    ): FakeWin => ({
        internalId,
        output: { name: out },
        desktops: [byId.get(ws) as FakeDesk],
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        ...extra,
    });
    const wins = [
        mkWin("win-a", OUT_1, WS_1),
        mkWin("win-b", OUT_2, WS_1),
        mkWin("win-s", OUT_2, WS_1, { onAllDesktops: true }),
        mkWin("win-f", OUT_2, WS_1),
        mkWin("win-m", OUT_2, WS_1, { maximizeMode: 3 }),
        mkWin("win-FS", OUT_2, WS_1, { fullScreen: true }),
        { ...mkWin("win-z", OUT_2, WS_1), minimized: true },
        { ...mkWin("win-ys", OUT_2, WS_1, { onAllDesktops: true }), minimized: true },
    ];
    const sendCalls: string[] = [];
    const viewCalls: string[] = [];
    const setCalls: string[] = [];
    const createCalls: Array<unknown[]> = [];
    const surface: Record<string, unknown> = {
        screens,
        desktops: [...byId.values()],
        windowList: () => wins,
        currentDesktopForScreen: (out: { name: string }) => currents.get(out.name) ?? null,
        setCurrentDesktopForScreen: (d: FakeDesk, out: { name: string }) => {
            const found = (surface["desktops"] as FakeDesk[]).find((cand) => cand.id === d.id);
            if (found) {
                currents.set(out.name, found);
                viewCalls.push(`${out.name}->${found.id}`);
            }
        },
        sendClientToScreen: (ref: FakeWin, outRef: { name: string }) => {
            sendCalls.push(`${ref.internalId}=>${outRef.name}`);
            ref.output = { name: outRef.name };
        },
        createDesktop: (position: number, name: string) => {
            createCalls.push([position, name]);
            const id = name === TOKEN ? WS_NEW : `native-${((surface["desktops"] as FakeDesk[]).length) + 1}`;
            (surface["desktops"] as FakeDesk[]).push(desk(id, name));
        },
    };
    return { surface, currents, wins, sendCalls, viewCalls, setCalls, createCalls };
}

function readProp(value: object, property: string): unknown {
    try {
        return Reflect.get(value as Record<string, unknown>, property);
    } catch (error) {
        void error;
        return undefined;
    }
}

function observe(surf: ReturnType<typeof makeSurface>): WorldObservation {
    const winObs = (w: FakeWin) => ({
        win: w.internalId,
        ref: w,
        out: w.output.name,
        ws: w.desktops[0]?.id ?? "",
        rect: { x: 0, y: 0, w: 200, h: 200 },
        floating: w.internalId === "win-f",
        fitExcluded: false,
        fullscreen: w.fullScreen,
        maximized: w.maximizeMode !== 0,
        sticky: w.onAllDesktops,
        fixedAuto: false,
        fixedSuppress: false,
        hints: { minW: null, minH: null, maxW: null, maxH: null },
        minimized: w.minimized,
    });
    const liveWins = surf.wins.filter((w) => !w.minimized);
    const minimizedWins = surf.wins.filter((w) => w.minimized);
    return {
        mode: "per-output-local",
        topology: [
            {
                out: OUT_1, order: [WS_1, WS_2, WS_3], current: surf.currents.get(OUT_1)?.id ?? "",
                complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true,
            },
            {
                out: OUT_2, order: [WS_1, WS_2, WS_3], current: surf.currents.get(OUT_2)?.id ?? "",
                complete: true, rect: { x: 800, y: 0, w: 800, h: 600 }, primary: false,
            },
        ],
        bindings: [],
        domains: [
            {
                id: OUT_1, ws: WS_1,
                bounds: { x: 0, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
                focused: "win-a", readable: true, hold: "", adjacent: {},
                windows: liveWins.filter((w) => w.output.name === OUT_1).map(winObs),
                closed: [], complete: true,
            },
            {
                id: OUT_2, ws: WS_1,
                bounds: { x: 800, y: 0, w: 800, h: 600 }, gap: 8, outer: 4,
                focused: "", readable: true, hold: "", adjacent: {},
                windows: liveWins.filter((w) => w.output.name === OUT_2).map(winObs),
                closed: [], complete: true,
            },
        ],
        minimized: minimizedWins.map(winObs),
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
    };
}

function freshState(reply: WorldReply): WorldAdapterState {
    const state = new WorldAdapterState("adapter-1", "owner-a");
    state.onDiscovery("gen-1");
    state.invalidate();
    state.invalidate();
    reply.obs = state.currentObs;
    return state;
}

function windowRow(win: string, out: string, ws: string): WorldReply["windows"][number] {
    return {
        win, out, ws,
        alloc: null, frame: null,
        frame_auth: "", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", floating: false, float_origin: "",
    };
}

function conversionCreationReply(): WorldReply {
    return {
        adapter: "test-adapter", gen: "gen-1", req: "req-1", obs: "", rev: "1",
        ready: true, consumed: false,
        scopes: [],
        workspaces: [
            {
                id: TOKEN, out: OUT_2, bind: "", logical: TOKEN,
                scope: OUT_2, order: 0, mode: "tiled", exist: true, hold: "",
            },
        ],
        outputs: [],
        windows: [
            windowRow("win-a", OUT_1, WS_1),
            windowRow("win-b", OUT_2, WS_1),
            windowRow("win-s", OUT_2, WS_1),
            windowRow("win-f", OUT_2, WS_1),
            windowRow("win-m", OUT_2, WS_1),
        ],
        transfers: [],
        effects: [],
        result: null,
    };
}

function conversionMoveReply(): WorldReply {
    // Producer-faithful row shapes: tiled members carry controlled frames
    // (deferred behind pending membership arrival), the float member rides
    // an untouched float row, and the minimized lifetime rides an explicit
    // minimized-hold row.
    const tiledRow = (win: string): WorldReply["windows"][number] => ({
        win, out: OUT_2, ws: WS_NEW,
        alloc: [0, 0, 800, 600], frame: [0, 0, 784, 584],
        frame_auth: "controlled", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", floating: false, float_origin: "",
    });
    return {
        adapter: "test-adapter", gen: "gen-1", req: "req-2", obs: "", rev: "2",
        ready: true, consumed: false,
        scopes: [],
        workspaces: [],
        outputs: [],
        windows: [
            tiledRow("win-b"),
            {
                win: "win-f", out: OUT_2, ws: WS_NEW,
                alloc: null, frame: null,
                frame_auth: "untouched", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "", floating: true, float_origin: "",
            },
            tiledRow("win-m"),
            tiledRow("win-FS"),
            {
                win: "win-z", out: OUT_2, ws: WS_NEW,
                alloc: null, frame: null,
                frame_auth: "hold", frame_hold: "minimized",
                member_auth: "controlled", member_hold: "",
                frame_op: "", floating: false, float_origin: "",
            },
            {
                win: "win-ys", out: OUT_2, ws: WS_1,
                alloc: null, frame: null,
                frame_auth: "hold", frame_hold: "minimized",
                member_auth: "controlled", member_hold: "",
                frame_op: "", floating: false, float_origin: "",
            },
        ],
        transfers: [
            { op: "op-scm-1-0", kind: "window", win: "win-b", out: OUT_2, ws: WS_NEW, follow: false, held: false },
            { op: "op-scm-1-1", kind: "window", win: "win-f", out: OUT_2, ws: WS_NEW, follow: false, held: false },
            { op: "op-scm-1-2", kind: "window", win: "win-m", out: OUT_2, ws: WS_NEW, follow: false, held: false },
            { op: "op-scm-1-3", kind: "window", win: "win-FS", out: OUT_2, ws: WS_NEW, follow: false, held: false },
            { op: "op-scm-1-4", kind: "window", win: "win-z", out: OUT_2, ws: WS_NEW, follow: false, held: false },
            { op: "op-scv-1-0", kind: "view", win: "", out: OUT_2, ws: WS_NEW, follow: false, held: false },
        ],
        effects: [],
        result: null,
    };
}

describe("world startup conversion actuation", () => {
    it("creates the fresh backing without touching shared members", () => {
        const surf = makeSurface();
        const reply = conversionCreationReply();
        const state = freshState(reply);
        const host = createWorldApplyHost(surf.surface, readProp);
        const out = applyWorldReply({ ...host, state, observation: observe(surf), reply, log: () => undefined });
        // Exactly one void creation flight names the logical token; the new
        // desktop binds by exact-name enumeration, then takes its friendly
        // `Desktop N` name at its native one-based position.
        assert.deepEqual(surf.createCalls, [[3, TOKEN]]);
        assert.ok(
            (surf.surface["desktops"] as FakeDesk[]).some((d) => d.id === WS_NEW && d.name === "Desktop 4"),
            "fresh backing binds by exact token name, then renames friendly",
        );
        assert.deepEqual(surf.sendCalls, [], "no output change while converting");
        assert.equal(out.membershipWrites, 0);
        assert.equal(out.viewWrites, 0);
        assert.equal(out.creations, 1);
    });

    it("converts members membership-only and follows the view", () => {
        const surf = makeSurface();
        // The fresh backing already bound (as the creation leg proves).
        (surf.surface["desktops"] as FakeDesk[]).push(desk(WS_NEW, TOKEN));
        const reply = conversionMoveReply();
        const state = freshState(reply);
        const host = createWorldApplyHost(surf.surface, readProp);
        const framesBefore = surf.wins.map((w) => JSON.stringify(w.frameGeometry));
        const out = applyWorldReply({ ...host, state, observation: observe(surf), reply, log: () => undefined });
        // Membership-only: no output send, members stay on their output.
        assert.deepEqual(surf.sendCalls, [], "conversion never changes output");
        assert.deepEqual(
            surf.wins.map((w) => `${w.internalId}:${w.output.name}/${w.desktops[0]?.id}`),
            [
                `win-a:${OUT_1}/${WS_1}`,
                `win-b:${OUT_2}/${WS_NEW}`,
                `win-s:${OUT_2}/${WS_1}`,
                `win-f:${OUT_2}/${WS_NEW}`,
                `win-m:${OUT_2}/${WS_NEW}`,
                `win-FS:${OUT_2}/${WS_NEW}`,
                `win-z:${OUT_2}/${WS_NEW}`,
                `win-ys:${OUT_2}/${WS_1}`,
            ],
            "non-sticky members convert on the same output, sticky stays",
        );
        // Overlay states ride the membership setter untouched: no geometry
        // writes even though tiled rows carry controlled frames (arrival
        // defers frames until membership proves), and native rects/flags
        // are unchanged. The minimized lifetime moves desktops only: still
        // minimized, no geometry, no output send.
        assert.equal(out.geometryWrites, 0, "no frame writes during conversion");
        assert.deepEqual(
            surf.wins.map((w) => JSON.stringify(w.frameGeometry)),
            framesBefore,
            "native frames unchanged",
        );
        const byId = new Map(surf.wins.map((w) => [w.internalId, w]));
        assert.equal(byId.get("win-m")?.maximizeMode, 3, "maximize preserved");
        assert.equal(byId.get("win-FS")?.fullScreen, true, "fullscreen preserved");
        assert.equal(byId.get("win-f")?.fullScreen, false, "float preserved without overlay write");
        assert.equal(byId.get("win-s")?.onAllDesktops, true, "sticky preserved");
        assert.equal(byId.get("win-z")?.minimized, true, "converted lifetime stays minimized");
        assert.equal(byId.get("win-ys")?.minimized, true, "sticky minimized stays minimized");
        // The converting output follows to its fresh backing exactly once.
        assert.deepEqual(surf.viewCalls, [`${OUT_2}->${WS_NEW}`]);
        assert.equal(out.membershipWrites, 5);
        assert.equal(out.viewWrites, 1);
        assert.equal(out.held, 0, "every conversion move actuates, nothing held");
    });

    it("never actuates against the unbound token", () => {
        const surf = makeSurface();
        const reply: WorldReply = {
            adapter: "test-adapter", gen: "gen-1", req: "req-3", obs: "", rev: "3",
            ready: true, consumed: false,
            scopes: [],
            workspaces: [],
            outputs: [],
            windows: [windowRow("win-b", OUT_2, WS_1)],
            transfers: [
                { op: "op-scm-1-0", kind: "window", win: "win-b", out: OUT_2, ws: TOKEN, follow: false, held: false },
            ],
            effects: [],
            result: null,
        };
        const state = freshState(reply);
        const host = createWorldApplyHost(surf.surface, readProp);
        const out = applyWorldReply({ ...host, state, observation: observe(surf), reply, log: () => undefined });
        // The logical token is not a native id: the desktop does not
        // resolve, so the move holds with no native call.
        assert.equal(surf.wins.find((w) => w.internalId === "win-b")?.desktops[0]?.id, WS_1);
        assert.deepEqual(surf.sendCalls, []);
        assert.equal(out.membershipWrites, 0);
        assert.ok(out.held >= 1, "unbound-token transfer holds");
    });
});
