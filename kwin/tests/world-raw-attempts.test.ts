// Raw mechanical attempt production (Stage 3 strict v2 cutover): the
// production entry records write/re-read facts for every native dispatch
// (tiled sweep frames, explicit float placements, window membership, exact
// output views, maximize clears) and carries them on the next report with
// original write stamps, actual setter evidence, immediate readbacks, and
// nullable DPI. Host-owned logical receipts stay empty: native homing/views
// never ride as logical pre/post/base evidence (translation through the
// adapter binding table would forge logical evidence and poison core
// third-location proof). The fresh same-turn native desktop/current ids
// ride the native keys for core-side translation. Maximize clears actuate
// through the native nonactivating setter with spend-before-dispatch;
// already-unmaximized desires spend with no invented row. Included in
// `npm test`.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { WorldAdapterState } from "../src/world-state";
import { applyWorldReply } from "../src/world-apply";
import type { WorldObservation } from "../src/world-observe";

interface FakeDesktop {
    id: string;
    name: string;
}

interface FakeWindow {
    managed: boolean;
    deleted: boolean;
    transient: boolean;
    transientFor: unknown;
    dialog: boolean;
    modal: boolean;
    wantsInput: boolean;
    internalId: string;
    normalWindow: boolean;
    frameGeometry: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    hidden: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    setMaximize?: (vertical: boolean, horizontal: boolean) => void;
}

function makeSurface() {
    const d1: FakeDesktop = { id: "d1", name: "d1" };
    const d2: FakeDesktop = { id: "d2", name: "d2" };
    const byId = new Map([["d1", d1], ["d2", d2]]);
    const wins: FakeWindow[] = [
        {
            internalId: "win-a", managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [d1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
        },
        {
            internalId: "win-b", managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { x: 800, y: 0, width: 200, height: 200 },
            output: { name: "out-2" }, desktops: [d2],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
        },
        {
            internalId: "win-c", managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            normalWindow: true,
            frameGeometry: { x: 10, y: 10, width: 100, height: 100 },
            output: { name: "out-1" }, desktops: [d1],
            minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
        },
    ];
    // A maximized tiled window proving the real-engine clear path: the
    // native nonactivating setter flips maximizeMode synchronously, and
    // the immediate readback proves the clear in the same flight.
    const winD: FakeWindow = {
        internalId: "win-d", managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x: 100, y: 100, width: 50, height: 50 },
        output: { name: "out-1" }, desktops: [d1],
        minimized: false, hidden: false, fullScreen: false, maximizeMode: 3, onAllDesktops: false,
        setMaximize: (vertical, horizontal): void => {
            winD.maximizeMode = (vertical || horizontal) ? 3 : 0;
        },
    };
    wins.push(winD);
    // Both views start unproven so follow and standalone actuation run.
    const currents = new Map([["out-1", "d2"], ["out-2", "d1"]]);
    const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    const fire = (name: string): void => {
        for (const handler of [...(signals[name] as Set<(payload?: unknown) => void>)]) {
            handler();
        }
    };
    const signals: Record<string, Set<(payload?: unknown) => void>> = {
        windowAdded: new Set(), windowRemoved: new Set(),
        desktopsChanged: new Set(), currentDesktopChanged: new Set(),
    };
    const surface = {
        windowList: (): FakeWindow[] => [...wins],
        screens: [
            { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 600 } },
            { name: "out-2", geometry: { x: 800, y: 0, width: 800, height: 600 } },
        ],
        desktops: [d1, d2],
        activeScreen: { name: "out-1" },
        activeWindow: wins[0] as FakeWindow,
        cursorPos: { x: 2, y: 100 },
        clientArea: (): Record<string, number> => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: (screen: { name: string }): FakeDesktop =>
            byId.get(currents.get(screen.name) ?? "") as FakeDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }): void => {
            currents.set(screen.name, (deskRef as FakeDesktop).id);
        },
        sendClientToScreen: (mover: unknown, out: unknown): void => {
            (mover as FakeWindow).output = { name: (out as { name: string }).name };
        },
        createDesktop: (): void => undefined,
        removeDesktop: (): boolean => false,
        windowAdded: { connect: (h: (p?: unknown) => void): void => { signals["windowAdded"]?.add(h); }, disconnect: (h: (p?: unknown) => void): void => { signals["windowAdded"]?.delete(h); } },
        windowRemoved: { connect: (h: (p?: unknown) => void): void => { signals["windowRemoved"]?.add(h); }, disconnect: (h: (p?: unknown) => void): void => { signals["windowRemoved"]?.delete(h); } },
        desktopsChanged: { connect: (h: (p?: unknown) => void): void => { signals["desktopsChanged"]?.add(h); }, disconnect: (h: (p?: unknown) => void): void => { signals["desktopsChanged"]?.delete(h); } },
        currentDesktopChanged: { connect: (h: (p?: unknown) => void): void => { signals["currentDesktopChanged"]?.add(h); }, disconnect: (h: (p?: unknown) => void): void => { signals["currentDesktopChanged"]?.delete(h); } },
    };
    return { surface, wins, currents, calls, timers, logs, fire, byId };
}

type Flow = ReturnType<typeof makeSurface> & {
    handle: NonNullable<ReturnType<typeof startWorldAdapterEntry>>;
};

function startFlow(): Flow {
    const base = makeSurface();
    const handle = startWorldAdapterEntry({
        workspace: base.surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            base.calls.push({ method, payload, callback });
            if (method === "TakeCommandResult") {
                callback(JSON.stringify({ v: 2, outcome: "pending", gen: "gen-1" }));
            }
        },
        scheduleOnce: (delay, cb) => {
            base.timers.push({ delay, cb });
            return () => undefined;
        },
        log: (m) => { base.logs.push(m); },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    return { ...base, handle: handle as NonNullable<typeof handle> };
}

function lastPayload(flow: Flow, method: string): string {
    const found = [...flow.calls].reverse().find((c) => c.method === method);
    assert.ok(found !== undefined, `expected ${method}`);
    return (found as { payload: string }).payload;
}

function answerDiscover(flow: Flow, body: Record<string, unknown>): void {
    const found = [...flow.calls].reverse().find((c) => c.method === "DiscoverWorld");
    assert.ok(found !== undefined,
        `expected DiscoverWorld, have [${flow.calls.map((c) => c.method).join(",")}]; logs=${JSON.stringify(flow.logs)}`);
    (found as { callback: (reply: unknown) => void }).callback(JSON.stringify(body));
}

function answerDescribe(flow: Flow, body: Record<string, unknown>): void {
    const found = [...flow.calls].reverse().find((c) => c.method === "DescribeWorld");
    assert.ok(found !== undefined,
        `expected DescribeWorld, have [${flow.calls.map((c) => c.method).join(",")}]; logs=${JSON.stringify(flow.logs)}`);
    (found as { callback: (reply: unknown) => void }).callback(JSON.stringify(body));
}

function runTimers(flow: Flow): void {
    for (let i = 0; i < flow.timers.length; i += 1) {
        const t = flow.timers[i] as { delay: number; cb: () => void };
        if (t.delay <= 120) {
            flow.timers.splice(i, 1);
            i -= 1;
            t.cb();
        }
    }
}

const PRESENTATION = {
    foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null },
    underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [],
};

function bootstrap(gen: string, req: string): Record<string, unknown> {
    return {
        v: 2, adapter: "broker-1", gen, req, obs: "0", rev: "0",
        ready: false, consumed: false,
        scopes: [], workspaces: [], outputs: [], windows: [],
        transfers: [], effects: [], result: null,
        gestureSettled: null,  operationSettled: [], nativeCreations: [], warpCursor: null,
        presentation: PRESENTATION,
    };
}

// Full bound acceptance shape: every key the strict core-reply parser
// requires, with the live source/watermark echoed back.
function accept(
    flow: Flow, rev: string, mutate: (reply: Record<string, unknown>) => void,
): void {
    const payload = JSON.parse(lastPayload(flow, "DescribeWorld")) as Record<string, unknown>;
    const reply: Record<string, unknown> = {
        v: 2, adapter: "broker-1", gen: "gen-1", req: payload["source"],
        obs: rev, rev, ready: true, consumed: false,
        source: payload["source"], nativeInvalid: payload["nativeInvalid"], clock: "1",
        scopes: [], outputs: [], windows: [], transfers: [], effects: [], result: null,
        gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [], warpCursor: null,
        presentation: PRESENTATION,
        workspaces: [
            { id: "w1", out: "out-1", bind: "d1", scope: "out-1", order: 0, mode: "tiled", exist: true, hold: "" },
            { id: "w2", out: "out-2", bind: "d2", scope: "out-2", order: 0, mode: "tiled", exist: true, hold: "" },
        ],
    };
    mutate(reply);
    answerDescribe(flow, reply);
}

describe("world raw mechanical attempts", () => {
    it("carries populated frame/member/view arrays with exact wire shapes", () => {
        const flow = startFlow();
        try {
            answerDiscover(flow, bootstrap("gen-1", "disc-1"));
            // Round 1: converge bindings plus win-c float intent (floating
            // with untouched authority, no write token yet).
            accept(flow, "1", (reply) => {
                reply["outputs"] = [
                    { out: "out-1", view: "w1", mode: "tiled", hold: "" },
                    { out: "out-2", view: "w2", mode: "tiled", hold: "" },
                ];
                reply["windows"] = [
                    { win: "win-a", out: "out-1", ws: "w1", alloc: [0, 0, 200, 200], frame: [0, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "" },
                    { win: "win-b", out: "out-2", ws: "w2", alloc: [800, 0, 200, 200], frame: [800, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "" },
                    { win: "win-c", out: "out-1", ws: "w1", alloc: [10, 10, 100, 100], frame: [10, 10, 100, 100], frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: true, float_origin: "" },
                ];
            });
            // Round 2: one tiled frame correction (win-b), one member move
            // with follow (win-a to out-2, target view unproven), one
            // standalone view (out-1 back to d1), one explicit float
            // placement (win-c), one clear token on an already-clear
            // window (win-b, spends with no row), and one clear token on
            // a maximized window (win-d, actuates through the native
            // nonactivating setter with fresh pre/post proof).
            // The round-1 apply may leave no dirty work, so drive the next
            // flight explicitly through a native invalidation.
            flow.fire("desktopsChanged");
            runTimers(flow);
            accept(flow, "2", (reply) => {
                reply["outputs"] = [
                    { out: "out-1", view: "w1", mode: "tiled", hold: "" },
                    { out: "out-2", view: "w2", mode: "tiled", hold: "" },
                ];
                reply["windows"] = [
                    { win: "win-a", out: "out-2", ws: "w2", alloc: [800, 0, 200, 200], frame: [800, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "" },
                    { win: "win-b", out: "out-2", ws: "w2", alloc: [800, 0, 400, 300], frame: [800, 0, 400, 300], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "op-clear-1", floating: false, float_origin: "" },
                    { win: "win-c", out: "out-1", ws: "w1", alloc: [10, 10, 100, 100], frame: [300, 200, 250, 180], frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "op-float-1", clear_op: "", floating: true, float_origin: "" },
                    { win: "win-d", out: "out-1", ws: "w1", alloc: [100, 100, 50, 50], frame: [100, 100, 50, 50], frame_auth: "untouched", frame_hold: "", member_auth: "untouched", member_hold: "", frame_op: "", clear_op: "op-clear-2", floating: false, float_origin: "" },
                ];
                reply["transfers"] = [
                    { op: "op-9", kind: "window", win: "win-a", out: "out-2", ws: "w2", follow: true, held: false },
                    { op: "op-10", kind: "view", win: "", out: "out-1", ws: "w1", follow: false, held: false },
                ];
            });
            // The member move landed (output + desktop setters ran on the
            // fake), the follow target view was unproven so it actuated,
            // the standalone view actuated back to d1, the tiled frame
            // corrected, the float placed, the already-clear token spent
            // with no row, and the maximized window cleared natively with
            // fresh pre/post proof.
            flow.fire("desktopsChanged");
            runTimers(flow);
            const payload = JSON.parse(lastPayload(flow, "DescribeWorld")) as Record<string, unknown>;
            const inner = payload["report"] as Record<string, unknown>;
            const frames = inner["frameAttempts"] as Array<Record<string, unknown>>;
            const members = inner["memberAttempts"] as Array<Record<string, unknown>>;
            const views = inner["viewAttempts"] as Array<Record<string, unknown>>;
            const clears = inner["clearAttempts"] as Array<Record<string, unknown>>;
            const diag = `logs=${JSON.stringify(flow.logs)} wins=${JSON.stringify(inner["windows"])}`;
            assert.ok(Array.isArray(frames) && frames.length === 2, `two frame attempts ride, got ${JSON.stringify(frames)} ${diag}`);
            assert.ok(Array.isArray(members) && members.length === 1, `one member attempt rides, got ${JSON.stringify(members)}`);
            assert.ok(Array.isArray(views) && views.length === 2, `two view attempts ride, got ${JSON.stringify(views)}`);
            assert.ok(Array.isArray(clears) && clears.length === 1, `one clear attempt rides, got ${JSON.stringify(clears)} ${diag}`);
            assert.deepEqual(
                clears[0],
                { op: "op-clear-2", win: "win-d", setter: "accepted", pre: true, post: false, attemptMs: "1" },
                "native nonactivating clear with fresh pre/post proof; the already-clear token spends silently",
            );
            // Tiled sweep: ordinary write (empty op, no prewrite), real
            // target plus immediate readback, nullable DPI, write stamp.
            const sweep = frames.find((f) => f["op"] === "");
            assert.deepEqual(
                sweep,
                { win: "win-b", target: [800, 0, 400, 300], setter: "accepted", readback: [800, 0, 400, 300], dpiBefore: null, dpiAfter: null, attemptMs: "1", op: "", pre: null },
            );
            // Explicit float placement: stable op correlation plus the live
            // pre-write frame; the setter alone never proves completion.
            const placed = frames.find((f) => f["op"] === "op-float-1");
            assert.deepEqual(
                placed,
                { win: "win-c", target: [300, 200, 250, 180], setter: "accepted", readback: [300, 200, 250, 180], dpiBefore: null, dpiAfter: null, attemptMs: "1", op: "op-float-1", pre: [10, 10, 100, 100] },
            );
            // Membership: exact logical target correlation plus the real
            // setter; logical pre/post stay null while the fresh same-turn
            // native desktop ids ride the native keys for core translation.
            assert.deepEqual(
                members[0],
                { op: "op-9", win: "win-a", out: "out-2", ws: "w2", setter: "accepted", pre: null, post: null, nativePre: "d1", nativePost: "d2", attemptMs: "1" },
            );
            // Views: follow parent plus standalone, same null logical
            // rule with fresh native currents plus the observation-time
            // baseline; the adapter never invents logical proof.
            const follow = views.find((v) => v["op"] === "op-9");
            assert.deepEqual(
                follow,
                { op: "op-9", out: "out-2", ws: "w2", setter: "accepted", pre: null, post: null, base: null, nativePre: "d1", nativePost: "d2", nativeBase: "d1", attemptMs: "1" },
            );
            const alone = views.find((v) => v["op"] === "op-10");
            assert.deepEqual(
                alone,
                { op: "op-10", out: "out-1", ws: "w1", setter: "accepted", pre: null, post: null, base: null, nativePre: "d2", nativePost: "d1", nativeBase: "d2", attemptMs: "1" },
            );
        } finally {
            flow.handle.stop();
        }
    });
    it("consumes born clear tokens on held-scope rows with no geometry writes", () => {
        // N6 property authority: the core rides `clear_operation` on held
        // rows while frame/member authority stays held. The clear lane
        // keys only on the token plus live safety/homing/freshness, never
        // on scope authority, so the maximized birth clears natively with
        // fresh pre/post proof and the held scope authorizes no
        // frame/member/view write.
        const flow = startFlow();
        try {
            answerDiscover(flow, bootstrap("gen-1", "disc-1"));
            accept(flow, "1", (reply) => {
                reply["outputs"] = [
                    { out: "out-1", view: "w1", mode: "tiled", hold: "" },
                    { out: "out-2", view: "w2", mode: "tiled", hold: "" },
                ];
                reply["windows"] = [
                    { win: "win-a", out: "out-1", ws: "w1", alloc: [0, 0, 200, 200], frame: [0, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "" },
                    { win: "win-b", out: "out-2", ws: "w2", alloc: [800, 0, 200, 200], frame: [800, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "" },
                    { win: "win-c", out: "out-1", ws: "w1", alloc: [10, 10, 100, 100], frame: [10, 10, 100, 100], frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: true, float_origin: "" },
                ];
            });
            flow.fire("desktopsChanged");
            runTimers(flow);
            accept(flow, "2", (reply) => {
                reply["outputs"] = [
                    { out: "out-1", view: "w1", mode: "tiled", hold: "" },
                    { out: "out-2", view: "w2", mode: "tiled", hold: "" },
                ];
                reply["windows"] = [
                    { win: "win-a", out: "out-2", ws: "w2", alloc: [800, 0, 200, 200], frame: [800, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "" },
                    { win: "win-b", out: "out-2", ws: "w2", alloc: [800, 0, 400, 300], frame: [800, 0, 400, 300], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "op-clear-1", floating: false, float_origin: "" },
                    { win: "win-c", out: "out-1", ws: "w1", alloc: [10, 10, 100, 100], frame: [300, 200, 250, 180], frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "op-float-1", clear_op: "", floating: true, float_origin: "" },
                    { win: "win-d", out: "out-1", ws: "w1", alloc: null, frame: null, frame_auth: "hold", frame_hold: "unreadable", member_auth: "hold", member_hold: "unreadable", frame_op: "", clear_op: "op-clear-2", floating: false, float_origin: "" },
                ];
                reply["transfers"] = [
                    { op: "op-9", kind: "window", win: "win-a", out: "out-2", ws: "w2", follow: true, held: false },
                    { op: "op-10", kind: "view", win: "", out: "out-1", ws: "w1", follow: false, held: false },
                ];
            });
            flow.fire("desktopsChanged");
            runTimers(flow);
            const payload = JSON.parse(lastPayload(flow, "DescribeWorld")) as Record<string, unknown>;
            const inner = payload["report"] as Record<string, unknown>;
            const clears = inner["clearAttempts"] as Array<Record<string, unknown>>;
            const diag = `logs=${JSON.stringify(flow.logs)} wins=${JSON.stringify(inner["windows"])}`;
            assert.ok(Array.isArray(clears) && clears.length === 1, `one clear attempt rides from the held row, got ${JSON.stringify(clears)} ${diag}`);
            assert.deepEqual(
                clears[0],
                { op: "op-clear-2", win: "win-d", setter: "accepted", pre: true, post: false, attemptMs: "1" },
                "held scope authorizes the property clear with fresh pre/post proof",
            );
        } finally {
            flow.handle.stop();
        }
    });
});

describe("world attempt retention", () => {
    it("keeps latest-wins, prunes on bound accept, and resets on generation", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.recordClock("7");
        assert.equal(state.recordFrameAttempt({
            win: "win-a", target: [0, 0, 1, 1], setter: "accepted", readback: null,
            dpiBefore: null, dpiAfter: null, attemptMs: "7", op: "", pre: null,
        }), true);
        assert.equal(state.recordFrameAttempt({
            win: "win-a", target: [0, 0, 2, 2], setter: "refused", readback: null,
            dpiBefore: null, dpiAfter: null, attemptMs: "7", op: "", pre: null,
        }), true);
        assert.equal(state.recordMemberAttempt({
            op: "op-1", win: "win-a", out: "out-1", ws: "w1",
            setter: "accepted", pre: null, post: null, attemptMs: "7",
        }), true);
        assert.equal(state.recordViewAttempt({
            op: "op-2", out: "out-1", ws: "w1",
            setter: "threw", pre: null, post: null, base: null, attemptMs: "7",
        }), true);
        // Invalid rows never store: empty op on a member attempt.
        assert.equal(state.recordMemberAttempt({
            op: "", win: "win-a", out: "out-1", ws: "w1",
            setter: "accepted", pre: null, post: null, attemptMs: "7",
        }), false);
        assert.equal(state.framesForReport().length, 1);
        assert.deepEqual(state.framesForReport()[0]?.["target"], [0, 0, 2, 2]);
        assert.equal(state.memberAttemptsForReport().length, 1);
        assert.equal(state.viewAttemptsForReport().length, 1);
        // A bound accepted reply carrying the flight source consumes the
        // delivered facts; a newer update for the same key survives.
        const flight = state.beginCapture("src-1", null);
        assert.ok(flight !== null);
        state.recordFrameAttempt({
            win: "win-a", target: [0, 0, 3, 3], setter: "accepted", readback: null,
            dpiBefore: null, dpiAfter: null, attemptMs: "7", op: "", pre: null,
        });
        const pruned = state.ackDelivered("src-1", true);
        assert.equal(pruned, 2);
        assert.equal(state.framesForReport().length, 1, "newer frame update survives its predecessor ack");
        assert.deepEqual(state.framesForReport()[0]?.["target"], [0, 0, 3, 3]);
        assert.equal(state.memberAttemptsForReport().length, 0);
        assert.equal(state.viewAttemptsForReport().length, 0);
        // Generation rotation clears every retained attempt.
        state.onDiscovery("gen-2", "broker-1");
        assert.equal(state.framesForReport().length, 0);
    });
});

function emptyObservation(): WorldObservation {
    return {
        mode: "per-output-local",
        topology: [
            { out: "out-b", order: ["wb1"], current: "wb1", complete: true, rect: { x: 800, y: 0, w: 800, h: 600 }, primary: false },
        ],
        bindings: [],
        domains: [],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: false,
        focusedGlobal: "",
        screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [],
        hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

describe("world empty migration controlled scope", () => {
    // Core-published reply for a truly empty migration target: no desired
    // rows, the bound target backing, and the paired source refill. The
    // scope row varies per case; everything else is identical.
    const emptyReply = (
        scopes: Array<{ scope: string; auth: string; reason: string }>,
        outputs: Array<{ out: string; view: string; mode: string; hold: string }> = [
            { out: "out-b", view: "w9", mode: "tiled", hold: "" },
        ],
    ): Record<string, unknown> => ({
        adapter: "broker-1", gen: "gen-1", req: "src-1", obs: "1", rev: "1",
        ready: true, consumed: false, scopes,
        workspaces: [
            { id: "w1", out: "out-b", bind: "wb1", scope: "out-b", order: 0, mode: "tiled", exist: true, hold: "" },
            { id: "wr", out: "out-a", bind: "wra", scope: "out-a", order: 0, mode: "tiled", exist: true, hold: "" },
        ],
        outputs,
        windows: [], transfers: [
            { op: "op-1", kind: "migration", win: "", out: "out-b", ws: "w1", follow: false, held: false },
            { op: "op-1-refill", kind: "view", win: "", out: "out-a", ws: "wr", follow: false, held: false },
        ],
        effects: [], result: null, gestureSettled: null, gestureAssoc: null, operationSettled: [],
        nativeCreations: [], warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        source: "src-1", nativeInvalid: "0", clock: "1",
    });
    const emptyEnv = (reply: Record<string, unknown>, takeResult: unknown, switched: string[], currentViews: Map<string, string>, gesture = false) => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        return {
            state,
            observation: { ...emptyObservation(), gesture },
            reply: reply as never,
            dispatchWatermark: "0",
            // No terminal lane at all: takeResult null on a fresh state with
            // no lastTakeResult mirrors an external client that cannot
            // expose its terminal to the actuator.
            takeResult: takeResult as never,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
            isActiveWindow: (): boolean => false,
            readLiveRect: (): null => null,
            readLiveMembership: (): null => null,
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: (): boolean => false,
            setMembership: (): boolean => false,
            sendToOutput: (): boolean => false,
            resolveOutput: (out: string): object => ({ name: out }),
            resolveDesktop: (ws: string): object => ({ id: ws }),
            switchView: (desktopRef: object, outRef: object): boolean => {
                const out = (outRef as { name: string }).name;
                switched.push(`${out}->${(desktopRef as { id: string }).id}`);
                currentViews.set(out, (desktopRef as { id: string }).id);
                return true;
            },
            createDesktop: (): void => undefined,
            removeDesktop: (): boolean => false,
            focusWindow: (): boolean => false,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: (): boolean => false,
            readLiveView: (out: string): string | null => currentViews.get(out) ?? null,
            desktopById: (id: string): object => ({ id }),
            minDesktops: 1,
        };
    };
    it("actuates both views for a truly empty controlled target with no terminal", () => {
        const switched: string[] = [];
        const currentViews = new Map([["out-b", "wb9"], ["out-a", "wq"]]);
        const env = emptyEnv(
            emptyReply([{ scope: "out-b/w1", auth: "controlled", reason: "" }]),
            null, switched, currentViews,
        );
        const out = applyWorldReply(env);
        assert.ok(switched.some((c) => c === "out-b->wb1"), `controlled empty actuates the target view, got ${JSON.stringify(switched)}`);
        assert.ok(switched.some((c) => c === "out-a->wra"), `controlled empty actuates the paired refill, got ${JSON.stringify(switched)}`);
        assert.equal(out.viewWrites, 2, "target plus refill write exactly once each");
        assert.equal(out.membershipWrites, 0, "no members means no membership writes");
        assert.equal(out.held, 0, "complete empty set holds nothing");
        assert.equal(env.state.isDispatched("op-1"), false, "undispatched parent records nothing; the refill dispatch is the evidence");
    });
    it("actuates views for an untouched empty target with unheld output and no gesture", () => {
        const switched: string[] = [];
        const currentViews = new Map([["out-b", "wb9"], ["out-a", "wq"]]);
        const env = emptyEnv(
            emptyReply([{ scope: "out-b/w1", auth: "untouched", reason: "" }]),
            null, switched, currentViews,
        );
        const out = applyWorldReply(env);
        assert.ok(switched.some((c) => c === "out-b->wb1"), `untouched empty actuates the target view, got ${JSON.stringify(switched)}`);
        assert.ok(switched.some((c) => c === "out-a->wra"), `untouched empty actuates the paired refill, got ${JSON.stringify(switched)}`);
        assert.equal(out.viewWrites, 2, "target plus refill write exactly once each");
        assert.equal(out.held, 0, "actuatable untouched set holds nothing");
    });
    it("gates a nonempty floating migration on the same untouched scope", () => {
        const ref = {};
        const switched: string[] = [];
        const currentViews = new Map([["out-b", "wb9"], ["out-a", "wq"]]);
        let memberSetters = 0;
        const reply = emptyReply([{ scope: "out-b/w1", auth: "untouched", reason: "" }]);
        (reply.windows as Array<Record<string, unknown>>).push({
            win: "win-m", out: "out-b", ws: "w1", alloc: null, frame: null,
            frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
            frame_op: "", clear_op: "", floating: false, float_origin: "",
        });
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        const env = {
            state,
            observation: setterObservation([{ win: "win-m", ref, out: "out-b", ws: "wb1" }]),
            reply: reply as never,
            dispatchWatermark: "0",
            takeResult: null as never,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
            isActiveWindow: (): boolean => false,
            readLiveRect: (): null => null,
            readLiveMembership: (r: object): { out: string; ws: string } | null =>
                r === ref ? { out: "out-b", ws: "wb1" } : null,
            readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
            setGeometry: (): boolean => false,
            setMembership: (): boolean => { memberSetters += 1; return true; },
            sendToOutput: (): boolean => { memberSetters += 1; return true; },
            resolveOutput: (out: string): object => ({ name: out }),
            resolveDesktop: (ws: string): object => ({ id: ws }),
            switchView: (desktopRef: object, outRef: object): boolean => {
                const outName = (outRef as { name: string }).name;
                switched.push(`${outName}->${(desktopRef as { id: string }).id}`);
                currentViews.set(outName, (desktopRef as { id: string }).id);
                return true;
            },
            createDesktop: (): void => undefined,
            removeDesktop: (): boolean => false,
            focusWindow: (): boolean => false,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: (): boolean => false,
            readLiveView: (out: string): string | null => currentViews.get(out) ?? null,
            desktopById: (id: string): object => ({ id }),
            minDesktops: 1,
        };
        const out = applyWorldReply(env);
        assert.equal(memberSetters, 0, "already-homed member runs no setter");
        assert.ok(switched.some((c) => c === "out-b->wb1"), `untouched nonempty actuates the target view, got ${JSON.stringify(switched)}`);
        assert.ok(switched.some((c) => c === "out-a->wra"), `untouched nonempty actuates the paired refill, got ${JSON.stringify(switched)}`);
        assert.equal(out.held, 0, "actuatable untouched set holds nothing");
    });
    it("holds untouched empty when the output is held, missing, or gesturing", () => {
        const cases: Array<{ name: string; outputs: Array<{ out: string; view: string; mode: string; hold: string }>; gesture: boolean }> = [
            { name: "held-output", outputs: [{ out: "out-b", view: "w9", mode: "tiled", hold: "x" }], gesture: false },
            { name: "missing-output", outputs: [], gesture: false },
            { name: "gesture", outputs: [{ out: "out-b", view: "w9", mode: "tiled", hold: "" }], gesture: true },
        ];
        for (const c of cases) {
            const switched: string[] = [];
            const currentViews = new Map([["out-b", "wb9"], ["out-a", "wq"]]);
            const env = emptyEnv(
                emptyReply([{ scope: "out-b/w1", auth: "untouched", reason: "" }], c.outputs),
                null, switched, currentViews, c.gesture,
            );
            const out = applyWorldReply(env);
            assert.equal(out.viewWrites, 0, `no view writes for untouched ${c.name}`);
            assert.deepEqual(switched, [], `no view switch for untouched ${c.name}`);
            assert.ok(out.held >= 1, `untouched ${c.name} holds`);
        }
    });
    it("holds with no view for held and missing scopes, ignoring unrelated terminals", () => {
        for (const scopes of [
            [{ scope: "out-b/w1", auth: "hold", reason: "x" }],
            [],
        ]) {
            const switched: string[] = [];
            const currentViews = new Map([["out-b", "wb9"], ["out-a", "wq"]]);
            // Unrelated terminal naming a different target with an
            // unobservable member: never member authority, never consulted.
            const unrelated = {
                op: "migrate-workspace", direction: "right",
                sourceOut: "out-c", sourceWs: "w9", targetOut: "out-c", targetWs: "w9",
                members: ["ghost"], floats: 0,
            };
            const env = emptyEnv(emptyReply(scopes), unrelated, switched, currentViews);
            const out = applyWorldReply(env);
            assert.equal(out.viewWrites, 0, `no view writes without a controlled scope, got ${JSON.stringify(scopes)}`);
            assert.deepEqual(switched, [], `no view switch without a controlled scope, got ${JSON.stringify(scopes)}`);
            assert.ok(out.held >= 1, `unscoped empty holds, got ${JSON.stringify(scopes)}`);
        }
    });
});

describe("world attempt acknowledgement", () => {
    it("retains delivered attempts on malformed/stale replies, consumes on valid", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        state.brokerIncarnation = "broker-1";
        state.recordClock("9");
        assert.equal(state.recordFrameAttempt({
            win: "win-a", target: [0, 0, 1, 1], setter: "accepted", readback: null,
            dpiBefore: null, dpiAfter: null, attemptMs: "9", op: "", pre: null,
        }), true);
        assert.equal(state.recordMemberAttempt({
            op: "op-1", win: "win-a", out: "out-1", ws: "w1",
            setter: "accepted", pre: null, post: null, attemptMs: "9",
        }), true);
        const reply = (rev: string, obs: string, src = "src-1"): never => ({
            adapter: "broker-1", gen: "gen-1", req: src, source: src,
            ready: true, consumed: false, rev, obs, clock: "9",
        } as never);
        const flight = state.beginCapture("src-1", null);
        assert.ok(flight !== null);
        // Malformed counters never consume: the flight and its delivered
        // facts survive for redelivery.
        assert.deepEqual(state.acceptReply(reply("x", "1"), "src-1", "0"), { applied: false, reason: "malformed-counter" });
        assert.ok(state.inFlight !== null, "malformed reply keeps the flight");
        assert.equal(state.framesForReport().length, 1, "malformed reply retains the frame");
        assert.equal(state.memberAttemptsForReport().length, 1, "malformed reply retains the member");
        // A valid bound reply consumes exactly the delivered snapshot.
        assert.deepEqual(state.acceptReply(reply("2", "2"), "src-1", "0"), { applied: true, reason: "accepted" });
        assert.equal(state.inFlight, null);
        assert.equal(state.framesForReport().length, 0, "valid reply consumes the frame");
        assert.equal(state.memberAttemptsForReport().length, 0, "valid reply consumes the member");
        // A stale revision consumes nothing: newer facts survive.
        assert.ok(state.beginCapture("src-2", null) !== null);
        assert.equal(state.recordFrameAttempt({
            win: "win-a", target: [0, 0, 2, 2], setter: "refused", readback: null,
            dpiBefore: null, dpiAfter: null, attemptMs: "9", op: "", pre: null,
        }), true);
        assert.deepEqual(state.acceptReply(reply("1", "1", "src-2"), "src-2", "0"), { applied: false, reason: "stale-rev" });
        assert.equal(state.framesForReport().length, 1, "stale reply retains the newer frame");
    });
});

function setterObservation(homings: Array<{ win: string; ref: object; out: string; ws: string }>): WorldObservation {
    const base = emptyObservation();
    return {
        ...base,
        domains: [{
            id: "dom-1", ws: "d1",
            bounds: { x: 0, y: 0, w: 800, h: 600 }, gap: 8, outer: 8,
            focused: "", readable: true, hold: "", adjacent: {},
            windows: homings.map((h) => ({
                win: h.win, ref: h.ref, out: h.out, ws: h.ws,
                rect: { x: 0, y: 0, w: 100, h: 100 },
                floating: false, fitExcluded: false, fullscreen: false, maximized: false,
                sticky: false, fixedAuto: false, fixedSuppress: false, minimized: false,
                hints: { minW: null, minH: null, maxW: null, maxH: null }, nativeWs: h.ws,
            })),
            closed: [], complete: true,
        }],
    };
}

function setterReply(
    windows: Array<Record<string, unknown>>,
    transfers: Array<Record<string, unknown>>,
    workspaces: Array<Record<string, unknown>>,
    scopes: Array<Record<string, unknown>> = [],
): never {
    return {
        adapter: "broker-1", gen: "gen-1", req: "src-1", obs: "1", rev: "1",
        ready: true, consumed: false, scopes, workspaces,
        outputs: [], windows, transfers,
        effects: [], result: null, gestureSettled: null, gestureAssoc: null, operationSettled: [],
        nativeCreations: [], warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        source: "src-1", nativeInvalid: "0", clock: "9",
    } as never;
}

function setterSafety(): { minimized: boolean; fullscreen: boolean; maximized: boolean; sticky: boolean } {
    return { minimized: false, fullscreen: false, maximized: false, sticky: false };
}

describe("world setter attempt correspondence", () => {
    it("keeps one real member attempt when the output send is vetoed between steps", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        state.recordClock("9");
        const ref = {};
        const homing = new Map<object, { out: string; ws: string }>([[ref, { out: "out-1", ws: "d1" }]]);
        let setters = 0;
        const env = {
            state,
            observation: setterObservation([{ win: "win-a", ref, out: "out-1", ws: "d1" }]),
            reply: setterReply(
                [{
                    win: "win-a", out: "out-1", ws: "w1", alloc: null, frame: null,
                    frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
                    frame_op: "", clear_op: "", floating: false,
                }],
                [{ op: "op-9", kind: "window", win: "win-a", out: "out-2", ws: "w2", follow: false, held: false }],
                [{ id: "w2", out: "out-2", bind: "d2", scope: "out-2", order: 0, mode: "tiled", exist: true, hold: "" }],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
            isActiveWindow: (): boolean => false,
            readLiveRect: (): null => null,
            readLiveMembership: (r: object): { out: string; ws: string } | null => homing.get(r) ?? null,
            readLiveSafety: setterSafety,
            setGeometry: (): boolean => { throw new Error("no frame setter expected"); },
            setMembership: (): boolean => { throw new Error("no desktop setter expected"); },
            sendToOutput: (): boolean => {
                setters += 1;
                homing.set(ref, { out: "out-2", ws: "d1" });
                state.invalidate();
                return true;
            },
            resolveOutput: (out: string): object => ({ name: out }),
            resolveDesktop: (ws: string): object => ({ id: ws }),
            switchView: (): boolean => { throw new Error("no view setter expected"); },
            createDesktop: (): void => undefined,
            removeDesktop: (): boolean => false,
            focusWindow: (): boolean => false,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: (): boolean => false,
            readLiveView: (): string | null => null,
            desktopById: (id: string): object => ({ id }),
            minDesktops: 1,
        };
        const result = applyWorldReply(env);
        assert.equal(setters, 1, "exactly the output setter ran before the veto");
        assert.ok(result.held >= 1, "vetoed flight holds");
        const members = state.memberAttemptsForReport();
        assert.deepEqual(members, [{
            op: "op-9", win: "win-a", out: "out-2", ws: "w2",
            setter: "accepted", pre: null, post: null,
            nativePre: "d1", nativePost: "d1", attemptMs: "9",
        }], "veto keeps the one real setter disposition");
        assert.equal(result.membershipWrites, 0, "unproven setter earns no write credit");
    });

    it("records no member attempt when migration members need zero setters", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        state.recordClock("9");
        const homedRef = {};
        const stuckRef = {};
        const homing = new Map<object, { out: string; ws: string }>([
            [homedRef, { out: "out-b", ws: "wb1" }],
            [stuckRef, { out: "out-b", ws: "d9" }],
        ]);
        let setters = 0;
        const noSetter = (): boolean => { setters += 1; return true; };
        const env = {
            state,
            observation: setterObservation([
                { win: "win-m", ref: homedRef, out: "out-b", ws: "wb1" },
                { win: "win-n", ref: stuckRef, out: "out-b", ws: "d9" },
            ]),
            reply: setterReply(
                [
                    {
                        win: "win-m", out: "out-b", ws: "w1", alloc: null, frame: null,
                        frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
                        frame_op: "", clear_op: "", floating: false,
                    },
                    {
                        win: "win-n", out: "out-b", ws: "w9", alloc: null, frame: null,
                        frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
                        frame_op: "", clear_op: "", floating: false,
                    },
                ],
                [
                    { op: "op-7", kind: "migration", win: "", out: "out-b", ws: "w1", follow: false, held: false },
                    { op: "op-8", kind: "migration", win: "", out: "out-b", ws: "w9", follow: false, held: false },
                ],
                [
                    { id: "w1", out: "out-b", bind: "wb1", scope: "out-b", order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "w9", out: "out-b", bind: "wbX", scope: "out-b", order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                [
                    { scope: "out-b/w1", auth: "controlled", reason: "" },
                    { scope: "out-b/w9", auth: "controlled", reason: "" },
                ],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
            isActiveWindow: (): boolean => false,
            readLiveRect: (): null => null,
            readLiveMembership: (r: object): { out: string; ws: string } | null => homing.get(r) ?? null,
            readLiveSafety: setterSafety,
            setGeometry: (): boolean => { throw new Error("no frame setter expected"); },
            setMembership: noSetter,
            sendToOutput: noSetter,
            resolveOutput: (out: string): object => ({ name: out }),
            resolveDesktop: (ws: string): object => ({ id: ws }),
            switchView: (): boolean => { throw new Error("no view setter expected"); },
            createDesktop: (): void => undefined,
            removeDesktop: (): boolean => false,
            focusWindow: (): boolean => false,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: (): boolean => false,
            readLiveView: (out: string): string | null => (out === "out-b" ? "wb1" : null),
            // The w9 backing has no live desktop: resolution fails closed
            // with a hold, never a forged acceptance.
            desktopById: (id: string): object | null => (id === "wbX" ? null : { id }),
            minDesktops: 1,
        };
        const result = applyWorldReply(env);
        assert.equal(setters, 0, "homed and unresolvable members run zero setters");
        assert.deepEqual(state.memberAttemptsForReport(), [], "zero setters means zero member attempts");
        assert.equal(result.membershipWrites, 0, "zero setters means zero write credit");
        assert.ok(result.held >= 1, "unresolvable member holds the flight");
    });

    it("retains threw when the output send throws after homing at target", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        state.recordClock("9");
        const ref = {};
        const homing = new Map<object, { out: string; ws: string }>([[ref, { out: "out-1", ws: "d1" }]]);
        let setters = 0;
        const env = {
            state,
            observation: setterObservation([{ win: "win-a", ref, out: "out-1", ws: "d1" }]),
            reply: setterReply(
                [{
                    win: "win-a", out: "out-1", ws: "w1", alloc: null, frame: null,
                    frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
                    frame_op: "", clear_op: "", floating: false,
                }],
                [{ op: "op-9", kind: "window", win: "win-a", out: "out-2", ws: "w2", follow: false, held: false }],
                [{ id: "w2", out: "out-2", bind: "d2", scope: "out-2", order: 0, mode: "tiled", exist: true, hold: "" }],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
            isActiveWindow: (): boolean => false,
            readLiveRect: (): null => null,
            readLiveMembership: (r: object): { out: string; ws: string } | null => homing.get(r) ?? null,
            readLiveSafety: setterSafety,
            setGeometry: (): boolean => { throw new Error("no frame setter expected"); },
            setMembership: (): boolean => { throw new Error("no desktop setter expected"); },
            sendToOutput: (): boolean => {
                setters += 1;
                homing.set(ref, { out: "out-2", ws: "d2" });
                throw new Error("throw-after-moving");
            },
            resolveOutput: (out: string): object => ({ name: out }),
            resolveDesktop: (ws: string): object => ({ id: ws }),
            switchView: (): boolean => { throw new Error("no view setter expected"); },
            createDesktop: (): void => undefined,
            removeDesktop: (): boolean => false,
            focusWindow: (): boolean => false,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: (): boolean => false,
            readLiveView: (): string | null => null,
            desktopById: (id: string): object => ({ id }),
            minDesktops: 1,
        };
        const result = applyWorldReply(env);
        assert.equal(setters, 1, "output setter ran once before throwing");
        assert.deepEqual(state.memberAttemptsForReport(), [{
            op: "op-9", win: "win-a", out: "out-2", ws: "w2",
            setter: "threw", pre: null, post: null,
            nativePre: "d1", nativePost: "d2", attemptMs: "9",
        }], "throw after moving keeps the threw disposition, never accepted");
        assert.equal(result.membershipWrites, 0, "ambiguous throw earns no write credit");
    });

    it("retains the output disposition when migration homing goes unreadable after the send", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        state.recordClock("9");
        const ref = {};
        let sent = false;
        const homing = new Map<object, { out: string; ws: string }>([[ref, { out: "out-a", ws: "da" }]]);
        let sends = 0;
        const env = {
            state,
            observation: setterObservation([{ win: "win-q", ref, out: "out-a", ws: "da" }]),
            reply: setterReply(
                [{
                    win: "win-q", out: "out-b", ws: "w1", alloc: null, frame: null,
                    frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
                    frame_op: "", clear_op: "", floating: false,
                }],
                [{ op: "op-10", kind: "migration", win: "", out: "out-b", ws: "w1", follow: false, held: false }],
                [{ id: "w1", out: "out-b", bind: "wb1", scope: "out-b", order: 0, mode: "tiled", exist: true, hold: "" }],
                [{ scope: "out-b/w1", auth: "controlled", reason: "" }],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
            isActiveWindow: (): boolean => false,
            readLiveRect: (): null => null,
            readLiveMembership: (r: object): { out: string; ws: string } | null => {
                if (sent) {
                    throw new Error("unreadable after send");
                }
                return homing.get(r) ?? null;
            },
            readLiveSafety: setterSafety,
            setGeometry: (): boolean => { throw new Error("no frame setter expected"); },
            setMembership: (): boolean => { throw new Error("no desktop setter expected"); },
            sendToOutput: (): boolean => {
                sends += 1;
                sent = true;
                return true;
            },
            resolveOutput: (out: string): object => ({ name: out }),
            resolveDesktop: (ws: string): object => ({ id: ws }),
            switchView: (): boolean => { throw new Error("no view setter expected"); },
            createDesktop: (): void => undefined,
            removeDesktop: (): boolean => false,
            focusWindow: (): boolean => false,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: (): boolean => false,
            readLiveView: (): string | null => "dv",
            desktopById: (id: string): object => ({ id }),
            minDesktops: 1,
        };
        const result = applyWorldReply(env);
        assert.equal(sends, 1, "output setter ran once before the homing went unreadable");
        assert.deepEqual(state.memberAttemptsForReport(), [{
            op: "op-10", win: "win-q", out: "out-b", ws: "w1",
            setter: "accepted", pre: null, post: null,
            nativePre: "da", nativePost: null, attemptMs: "9",
        }], "superseded flight keeps the real output disposition");
        assert.equal(result.membershipWrites, 0, "superseded flight earns no write credit");
    });
});

describe("world clear attempt correspondence", () => {
    // Focused Stage 3 clear coverage through the production apply loop:
    // spend-before-dispatch, fresh maximized pre/post, truthful
    // accepted/refused/threw dispositions on the broker clock, veto and
    // gap handling, at-most-once replay protection, and generation reset.
    const clearEnv = (state: WorldAdapterState, opts: {
        maximized: boolean | null;
        fullscreen?: boolean;
        minimized?: boolean;
        sticky?: boolean;
        liveRect?: { x: number; y: number; w: number; h: number } | null;
        clear?: "accept" | "refuse" | "throw" | "missing";
        gesture?: boolean;
        listed?: boolean;
        homing?: { out: string; ws: string } | null;
        // Optional dynamic seams for final-gate regressions: a per-call
        // safety reader (to flip overlay state between the veto read and
        // the final pre-dispatch re-read) and a frame-race side effect
        // (to advance the watermark mid-apply).
        safetyFn?: () => { minimized: boolean; fullscreen: boolean; maximized: boolean | null; sticky: boolean };
        onRect?: () => void;
    }) => {
        let maximized = opts.maximized;
        let calls = 0;
        const ref = {};
        const env = {
            state,
            observation: {
                ...setterObservation([{ win: "win-c", ref, out: "out-1", ws: "d1" }]),
                gesture: opts.gesture ?? false,
            },
            reply: setterReply(
                [{
                    win: "win-c", out: "out-1", ws: "w1", alloc: null, frame: null,
                    frame_auth: "untouched", frame_hold: "", member_auth: "untouched", member_hold: "",
                    frame_op: "", clear_op: "op-clear-1", floating: false,
                }],
                [],
                [],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => opts.listed ?? true,
            isActiveWindow: (): boolean => false,
            readLiveRect: (): { x: number; y: number; w: number; h: number } | null => {
                opts.onRect?.();
                return opts.liveRect === undefined ? { x: 0, y: 0, w: 100, h: 100 } : opts.liveRect;
            },
            readLiveMembership: (): { out: string; ws: string } | null => opts.homing === undefined ? { out: "out-1", ws: "d1" } : opts.homing,
            readLiveSafety: () => opts.safetyFn?.() ?? ({
                minimized: opts.minimized ?? false,
                fullscreen: opts.fullscreen ?? false,
                maximized,
                sticky: opts.sticky ?? false,
            }),
            setGeometry: (): boolean => { throw new Error("no frame setter expected"); },
            setMembership: (): boolean => { throw new Error("no desktop setter expected"); },
            sendToOutput: (): boolean => { throw new Error("no output setter expected"); },
            resolveOutput: (out: string): object => ({ name: out }),
            resolveDesktop: (ws: string): object => ({ id: ws }),
            switchView: (): boolean => { throw new Error("no view setter expected"); },
            createDesktop: (): void => undefined,
            removeDesktop: (): boolean => false,
            focusWindow: (): boolean => false,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: (): boolean => false,
            readLiveView: (): string | null => null,
            desktopById: (id: string): object => ({ id }),
            minDesktops: 1,
        };
        if (opts.clear !== "missing") {
            (env as Record<string, unknown>)["clearMaximized"] = (): boolean => {
                calls += 1;
                if (opts.clear === "throw") {
                    throw new Error("native clear threw");
                }
                if (opts.clear === "refuse" || opts.clear === undefined) {
                    return false;
                }
                maximized = false;
                return true;
            };
        }
        return { env, calls: () => calls };
    };

    function boundClear(): WorldAdapterState {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        state.recordClock("9");
        return state;
    }

    it("dispatches the native clear once with fresh pre/post proof", () => {
        const state = boundClear();
        const { env, calls } = clearEnv(state, { maximized: true, clear: "accept" });
        const result = applyWorldReply(env);
        assert.equal(calls(), 1, "exactly one native clear");
        assert.deepEqual(state.clearAttemptsForReport(), [{
            op: "op-clear-1", win: "win-c", setter: "accepted", pre: true, post: false, attemptMs: "9",
        }], "accepted disposition with fresh pre/post maximized states");
        assert.equal(result.clears, 1, "accepted dispatch counts the clear write");
        assert.equal(result.held, 0, "proven clear holds nothing");
        assert.equal(state.isDispatched("op-clear-1"), true, "dispatch recorded once");
        // A duplicate reply never re-actuates the spent operation.
        const again = applyWorldReply(env);
        assert.equal(calls(), 1, "no second native clear");
        assert.equal(again.clears, 0);
        assert.equal(state.clearAttemptsForReport().length, 1, "no duplicate attempt row");
    });

    it("spends without an attempt when already unmaximized", () => {
        const state = boundClear();
        const { env, calls } = clearEnv(state, { maximized: false, clear: "accept" });
        const result = applyWorldReply(env);
        assert.equal(calls(), 0, "no native call for an already-clear window");
        assert.ok(state.clearSpent.has("op-clear-1"), "token spent so the desire never wedges");
        assert.deepEqual(state.clearAttemptsForReport(), [], "fresh observation is the proof: no invented row");
        assert.equal(result.clears, 0);
        assert.equal(result.held, 0);
    });

    it("records refused with unchanged proof and never repeats", () => {
        const state = boundClear();
        const { env, calls } = clearEnv(state, { maximized: true, clear: "refuse" });
        const result = applyWorldReply(env);
        assert.equal(calls(), 1, "the refused setter still ran once");
        assert.deepEqual(state.clearAttemptsForReport(), [{
            op: "op-clear-1", win: "win-c", setter: "refused", pre: true, post: true, attemptMs: "9",
        }], "refused disposition keeps the unchanged pre/post proof");
        assert.equal(result.clears, 0, "refused dispatch earns no write credit");
        assert.ok(result.held >= 1, "unproven clear holds");
        const again = applyWorldReply(env);
        assert.equal(calls(), 1, "spent refusal never re-actuates");
        assert.equal(again.clears, 0);
    });

    it("records threw with best-known proof and never repeats", () => {
        const state = boundClear();
        const { env, calls } = clearEnv(state, { maximized: true, clear: "throw" });
        const result = applyWorldReply(env);
        assert.equal(calls(), 1, "the throwing setter ran once");
        assert.deepEqual(state.clearAttemptsForReport(), [{
            op: "op-clear-1", win: "win-c", setter: "threw", pre: true, post: true, attemptMs: "9",
        }], "throw keeps the best-known unchanged proof, never fabricated arrival");
        assert.equal(result.clears, 0);
        assert.ok(result.held >= 1);
        const again = applyWorldReply(env);
        assert.equal(calls(), 1, "spent throw never re-actuates");
        assert.equal(again.clears, 0);
    });

    it("vetoes fullscreen with no native call and no invented row", () => {
        const state = boundClear();
        const { env, calls } = clearEnv(state, { maximized: true, fullscreen: true, clear: "accept" });
        const result = applyWorldReply(env);
        assert.equal(calls(), 0, "game/fullscreen lane never takes a clear write");
        assert.deepEqual(state.clearAttemptsForReport(), [], "no setter ran and no post state was read: no row");
        assert.ok(!state.clearSpent.has("op-clear-1"), "veto spends nothing; the core owns abandonment");
        assert.ok(result.held >= 1);
        // A later readable reply retries the still-undispatched desire.
        const readable = clearEnv(state, { maximized: true, clear: "accept" });
        const retry = applyWorldReply(readable.env);
        assert.equal(readable.calls(), 1, "readable reply dispatches the retained desire");
        assert.equal(retry.clears, 1);
    });

    it("holds unspent when safety flips between the veto read and the final gate", () => {
        const state = boundClear();
        let reads = 0;
        const { env, calls } = clearEnv(state, {
            maximized: true,
            clear: "accept",
            safetyFn: () => {
                reads += 1;
                // First read (vetoes) sees a clearable window; the final
                // pre-dispatch re-read sees a newer fullscreen flip.
                return reads <= 1
                    ? { minimized: false, fullscreen: false, maximized: true, sticky: false }
                    : { minimized: false, fullscreen: true, maximized: true, sticky: false };
            },
        });
        const result = applyWorldReply(env);
        assert.equal(calls(), 0, "flipped gate runs no native setter");
        assert.ok(!state.clearSpent.has("op-clear-1"), "flipped gate spends nothing");
        assert.deepEqual(state.clearAttemptsForReport(), [], "flipped gate records no row");
        assert.ok(result.held >= 1);
        assert.ok(!state.isDispatched("op-clear-1"), "no dispatch recorded");
        // A fresh reply with stable safety dispatches exactly once.
        const readable = clearEnv(state, { maximized: true, clear: "accept" });
        const retry = applyWorldReply(readable.env);
        assert.equal(readable.calls(), 1, "stable safety dispatches the retained desire");
        assert.deepEqual(state.clearAttemptsForReport(), [{
            op: "op-clear-1", win: "win-c", setter: "accepted", pre: true, post: false, attemptMs: "9",
        }], "fresh proof rides the next report");
        assert.equal(retry.clears, 1);
    });

    it("holds unspent when the watermark advances before dispatch", () => {
        const state = boundClear();
        let invalidated = false;
        const { env, calls } = clearEnv(state, {
            maximized: true,
            clear: "accept",
            onRect: () => {
                // Native input lands mid-apply (after the frame-race read):
                // the final gate sees the advanced watermark.
                if (!invalidated) {
                    invalidated = true;
                    state.invalidate();
                }
            },
        });
        const result = applyWorldReply(env);
        assert.equal(calls(), 0, "advanced watermark runs no native setter");
        assert.ok(!state.clearSpent.has("op-clear-1"), "advanced watermark spends nothing");
        assert.deepEqual(state.clearAttemptsForReport(), [], "advanced watermark records no row");
        assert.ok(result.held >= 1);
        // A fresh capture (new watermark) dispatches exactly once.
        const readable = clearEnv(state, { maximized: true, clear: "accept" });
        const retry = applyWorldReply({ ...readable.env, dispatchWatermark: state.currentInvalid });
        assert.equal(readable.calls(), 1, "fresh watermark dispatches the retained desire");
        assert.equal(retry.clears, 1);
    });

    it("holds unspent on unreadable safety, gesture, race, and missing capability", () => {
        for (const opts of [
            { maximized: null as boolean | null, clear: "accept" as const },
            { maximized: true as boolean | null, clear: "accept" as const, gesture: true },
            { maximized: true as boolean | null, clear: "accept" as const, liveRect: { x: 5, y: 5, w: 50, h: 50 } },
            { maximized: true as boolean | null, clear: "missing" as const },
            { maximized: true as boolean | null, clear: "accept" as const, homing: null },
        ]) {
            const state = boundClear();
            const { env, calls } = clearEnv(state, opts);
            const result = applyWorldReply(env);
            assert.equal(calls(), 0, `no native call for gap ${JSON.stringify(opts)}`);
            assert.ok(!state.clearSpent.has("op-clear-1"), `gap stays unspent: ${JSON.stringify(opts)}`);
            assert.deepEqual(state.clearAttemptsForReport(), [], `no invented row: ${JSON.stringify(opts)}`);
            assert.ok(result.held >= 1);
            // The undispatched desire retries on a readable reply.
            const readable = clearEnv(state, { maximized: true, clear: "accept" });
            const retry = applyWorldReply(readable.env);
            assert.equal(readable.calls(), 1, `undispatched desire retries: ${JSON.stringify(opts)}`);
            assert.equal(retry.clears, 1);
        }
    });

    it("resets spent clear tokens on generation rotation", () => {
        const state = boundClear();
        const first = clearEnv(state, { maximized: true, clear: "accept" });
        applyWorldReply(first.env);
        assert.equal(first.calls(), 1);
        state.onDiscovery("gen-2", "broker-1");
        assert.ok(!state.clearSpent.has("op-clear-1"), "rotation clears the spent set");
        assert.ok(!state.isDispatched("op-clear-1"), "rotation clears the dispatch record");
        const second = clearEnv(state, { maximized: true, clear: "accept" });
        const out = applyWorldReply({
            ...second.env,
            reply: { ...(second.env.reply as unknown as Record<string, unknown>), gen: "gen-2" } as never,
            dispatchWatermark: state.currentInvalid,
        });
        assert.equal(second.calls(), 1, "a fresh generation may dispatch the retained desire again");
        assert.equal(out.clears, 1);
    });
});
