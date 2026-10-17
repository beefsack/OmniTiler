// Production entry consumption of correlated Rust results: real
// engine reply bytes (test-fixtures/world-wire, shared with the
// tiler-protocol byte gate) drive the actual entry request path to native
// renderer/actuation effects. A drag-preview result with no gesture
// renders nothing; an active-group result highlights through the native
// effect with the fullscreen fail-closed gate; a migration result moves
// every member and follows the views without repeats.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import * as fs from "node:fs";
import * as path from "node:path";
import { startWorldAdapterEntry } from "../src/world-entry";

const FIXTURE_DIR = path.resolve("..", "test-fixtures", "world-wire");

function loadFixture(name: string): Record<string, unknown> {
    return JSON.parse(fs.readFileSync(path.join(FIXTURE_DIR, name), "utf8")) as Record<string, unknown>;
}

interface FakeDesktop {
    id: string;
}
interface FakeWindow {
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    minimized: boolean;
}

function makeWindow(id: string, out: { name: string }, ws: FakeDesktop): FakeWindow {
    let frame = { x: 0, y: 0, width: 100, height: 100 };
    const win = {
        internalId: id,
        normalWindow: true,
        output: out,
        desktops: [ws],
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minimized: false,
        keepAbove: false,
        keepBelow: false,
        minSize: null,
        maxSize: null,
    } as unknown as FakeWindow;
    Object.defineProperty(win, "frameGeometry", {
        configurable: true,
        get: () => frame,
        set: (rect: { x: number; y: number; width: number; height: number }) => {
            frame = rect;
            return true;
        },
    });
    return win;
}

interface Harness {
    handle: ReturnType<typeof startWorldAdapterEntry>;
    calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>;
    timers: Array<{ delay: number; cb: () => void }>;
    renders: Array<{ method: string; args: ReadonlyArray<unknown> }>;
    sentToOutput: string[];
    currents: Map<string, string>;
    runTimers: () => void;
    replyNext: (mutate?: (body: Record<string, unknown>) => void) => void;
}

function setup(wins: FakeWindow[], currents: Map<string, string>, floatIntent = false): Harness {
    const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const renders: Array<{ method: string; args: ReadonlyArray<unknown> }> = [];
    const sentToOutput: string[] = [];
    const screens = [...new Set(wins.map((w) => w.output.name))].map((name) => ({ name }));
    const allDesktops: FakeDesktop[] = [];
    for (const w of wins) {
        for (const d of w.desktops) {
            if (!allDesktops.some((e) => e.id === d.id)) {
                allDesktops.push(d);
            }
        }
    }
    const surface = {
        windowList: () => wins,
        screens,
        desktops: allDesktops,
        activeScreen: { name: wins[0]?.output.name ?? "out-a" },
        get activeWindow() {
            return wins[0] ?? null;
        },
        cursorPos: { x: 2, y: 100 },
        get currentDesktop() {
            return allDesktops[0];
        },
        set currentDesktop(_ref: unknown) {
            void _ref;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (out: unknown) => {
            const name = (out as { name: string }).name;
            const id = currents.get(name) ?? "";
            return allDesktops.find((d) => d.id === id) ?? null;
        },
        setCurrentDesktopForScreen: (deskRef: unknown, outRef: unknown) => {
            currents.set((outRef as { name: string }).name, (deskRef as { id: string }).id);
        },
        sendClientToScreen: (client: unknown, output: unknown) => {
            sentToOutput.push(`${(client as FakeWindow).internalId}->${(output as { name: string }).name}`);
            (client as FakeWindow).output = output as { name: string };
        },
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            if (method === "ReadFloatIntent") {
                callback(JSON.stringify({ members: [], tile: [] }));
                return;
            }
            calls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (m) => {
            const arr = (globalThis as Record<string, unknown>)["__logs"] as string[] | undefined;
            if (arr !== undefined) {
                arr.push(m);
            }
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        ...(floatIntent ? { floatIntent: true as const } : {}),
        highlightCallDbus: (_service, _path, _iface, method, ...args) => {
            renders.push({ method, args });
        },
    });
    assert.ok(handle !== null);
    const runTimers = (): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= 120) {
                timers.splice(i, 1);
                i -= 1;
                t.cb();
            }
        }
    };
    const replyNext = (mutate?: (body: Record<string, unknown>) => void): void => {
        const last = [...calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(last !== undefined);
        const body = JSON.parse((last as { payload: string }).payload) as Record<string, unknown>;
        const pending = (globalThis as Record<string, unknown>)["__pendingReply"] as Record<string, unknown>;
        pending["obs"] = body["obs"];
        pending["req"] = body["req"];
        pending["adapter"] = "test-adapter";
        if (mutate !== undefined) {
            mutate(pending);
        }
        (last as { callback: (reply: unknown) => void }).callback(JSON.stringify(pending));
    };
    return { handle, calls, timers, renders, sentToOutput, currents, runTimers, replyNext };
}

function discover(h: Harness): void {
    const disc = h.calls.find((c) => c.method === "DiscoverWorld");
    assert.ok(disc !== undefined);
    const req = (JSON.parse((disc as { payload: string }).payload) as Record<string, unknown>)["req"];
    (disc as { callback: (reply: unknown) => void }).callback(JSON.stringify({
        v: 1, adapter: "test-adapter", gen: "gen-1", req,
        obs: "0", rev: "0", ready: false, consumed: false,
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [], result: null,
    }));
}

function stageFixture(name: string): void {
    (globalThis as Record<string, unknown>)["__pendingReply"] = loadFixture(name);
}

describe("world command result entry", () => {
    it("renders nothing for a preview result with no gesture in flight", () => {
        const out = { name: "out" };
        const ws = { id: "ws" };
        const h = setup([makeWindow("win-a", out, ws), makeWindow("win-b", out, ws)], new Map([["out", "ws"]]));
        discover(h);
        stageFixture("reply-preview.json");
        h.replyNext();
        assert.ok(!h.renders.some((r) => r.method === "SetDragTargetPreview"), "no gesture means no overlay");
        h.handle?.stop();
        delete (globalThis as Record<string, unknown>)["__pendingReply"];
    });

    it("highlights the core-derived group bounds with the fullscreen gate", () => {
        (globalThis as Record<string, unknown>)["__logs"] = [];
        const out = { name: "out" };
        const ws = { id: "ws" };
        const h = setup([makeWindow("win-a", out, ws), makeWindow("win-b", out, ws)], new Map([["out", "ws"]]));
        discover(h);
        stageFixture("reply-group.json");
        h.replyNext();
        const sets = h.renders.filter((r) => r.method === "SetGroupHighlight");
        assert.equal(sets.length, 1);
        const first = sets[0] as { args: ReadonlyArray<unknown> };
        const payload = JSON.parse(first.args[0] as string) as Record<string, unknown>;
        // Group union respects the outer gap: 800x600 inset by outer 4
        // gives 792x592 at (4,4), never the raw client area. Members are
        // the same pair (order follows the inset fallback seed).
        assert.deepEqual(payload["bounds"], { x: 4, y: 4, w: 792, h: 592 });
        assert.deepEqual([...(payload["members"] as Array<string>)].sort(), ["win-a", "win-b"]);
        // Fallback seed under the inset (synthetic edge windows decline
        // fit, which checks containment in the inset work area): same
        // members, inset bounds, stable fallback group identity. The
        // atomic serial seed admits both members at base revision 0, so
        // the group carries r0 (per-leg seeding used r1).
        assert.equal(payload["group"], "grp-win-a-r0");
        h.handle?.stop();
        delete (globalThis as Record<string, unknown>)["__pendingReply"];
    });

    it("migrates every member and follows the views without repeats", () => {
        const outA = { name: "out-a" };
        const outB = { name: "out-b" };
        const wa1 = { id: "wa1" };
        const wb1 = { id: "wb1" };
        const currents = new Map([["out-a", "wa1"], ["out-b", "wb1"]]);
        const h = setup([makeWindow("win-a", outA, wa1), makeWindow("win-b", outB, wb1)], currents);
        discover(h);
        stageFixture("reply-migrate.json");
        h.replyNext();
        assert.ok(h.sentToOutput.some((c) => c === "win-a->out-b"), "member arrives natively");
        assert.equal(currents.get("out-b"), "wa1", "target view follows");
        const sends = h.sentToOutput.length;
        // A repeat dispatch with converged observation issues no new moves.
        h.runTimers();
        const last = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
        if (last !== undefined) {
            const body = JSON.parse((last as { payload: string }).payload) as Record<string, unknown>;
            const pending = loadFixture("reply-migrate.json");
            pending["obs"] = body["obs"];
            pending["req"] = body["req"];
            pending["adapter"] = "test-adapter";
            pending["rev"] = "3";
            (last as { callback: (reply: unknown) => void }).callback(JSON.stringify(pending));
        }
        assert.equal(h.sentToOutput.length, sends, "no repeated ambiguous moves");
        h.handle?.stop();
        delete (globalThis as Record<string, unknown>)["__pendingReply"];
    });

    it("clears the highlight on a no-group result", () => {
        const out = { name: "out" };
        const ws = { id: "ws" };
        const h = setup([makeWindow("win-a", out, ws), makeWindow("win-b", out, ws)], new Map([["out", "ws"]]));
        discover(h);
        stageFixture("reply-group.json");
        h.replyNext((pending) => {
            pending["result"] = { op: "no-group", reason: "no-parent-group", rev: null };
        });
        const clears = h.renders.filter((r) => r.method === "ClearGroupHighlight");
        assert.ok(clears.length >= 1, "no-group clears the renderer");
        assert.ok(!h.renders.some((r) => r.method === "SetGroupHighlight"));
        h.handle?.stop();
        delete (globalThis as Record<string, unknown>)["__pendingReply"];
    });

    it("persists the tile override on unfloat and keeps it settled", () => {
        const out = { name: "out" };
        const ws = { id: "ws" };
        const wins = [makeWindow("win-a", out, ws), makeWindow("win-b", out, ws)];
        const h = setup(wins, new Map([["out", "ws"]]), true);
        discover(h);
        // Float first: the accepted untouched row converges membership.
        stageFixture("reply-float.json");
        h.replyNext();
        // Unfloat through the production shortcut: the command carries the
        // live frame, and the accepted result (no placement rect) records
        // the explicit tile override.
        h.handle?.requestFloat();
        h.runTimers();
        const sent = [...h.calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(sent !== undefined);
        const cmd = (JSON.parse((sent as { payload: string }).payload) as Record<string, unknown>)["command"] as Record<string, unknown>;
        assert.equal(cmd["op"], "operation");
        const inner = cmd["command"] as Record<string, unknown>;
        assert.equal(inner["op"], "toggle-float");
        assert.ok(Array.isArray(inner["rect"]), "unfloat carries the live frame");
        const body = JSON.parse((sent as { payload: string }).payload) as Record<string, unknown>;
        const unfloat = loadFixture("reply-float.json");
        unfloat["obs"] = body["obs"];
        unfloat["req"] = body["req"];
        unfloat["adapter"] = "test-adapter";
        unfloat["rev"] = "3";
        (unfloat["result"] as Record<string, unknown>) = { op: "toggle-float", win: "win-a", rect: null };
        // A real unfloat reply re-admits the window tiled: the untouched
        // exception row is gone, so membership leaves the float set while
        // the tile override records.
        // Re-admitted tile respects the outer gap (outer 4 fixture).
        unfloat["windows"] = [
            {
                win: "win-a", out: "out", ws: "ws",
                alloc: [4, 4, 792, 592], frame: [4, 4, 792, 592],
                frame_auth: "controlled", frame_hold: "",
                member_auth: "controlled", member_hold: "",
                frame_op: "",
                floating: false, float_origin: "",
            },
            ...((unfloat["windows"] as Array<Record<string, unknown>>).filter((w) => w["win"] !== "win-a")),
        ];
        (sent as { callback: (reply: unknown) => void }).callback(JSON.stringify(unfloat));

        const intentCalls = h.calls.filter((c) => c.method === "WriteFloatIntent");
        assert.ok(intentCalls.length >= 1, "settled change persists");
        const lastIntent = JSON.parse(
            (intentCalls[intentCalls.length - 1] as { payload: string }).payload,
        ) as Record<string, unknown>;
        assert.deepEqual(lastIntent["tile"], ["win-a"], "explicit tile override persists");
        assert.ok(!(lastIntent["members"] as string[]).includes("win-a"), "unfloated leaves float membership");
        h.handle?.stop();
        delete (globalThis as Record<string, unknown>)["__pendingReply"];
    });
});
