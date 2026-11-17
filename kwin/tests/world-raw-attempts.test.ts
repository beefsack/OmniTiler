// Raw mechanical attempt production (Stage 3 strict v2 cutover): the
// production entry records write/re-read facts for every native dispatch
// (tiled sweep frames, explicit float placements, window membership, exact
// output views, maximize clears) and carries them on the next report with
// original write stamps, actual setter evidence, immediate readbacks, and
// nullable DPI. Member/view evidence rides approved v4 WorkspaceRefs
// (`{kind,out,ws}` on the single namespace the host capability allows;
// native-only for views), never retired native* sidecars and never
// translated or rewritten preimages: the exact live output rides each
// ref with the native id. Maximize clears actuate through the native
// nonactivating setter with spend-before-dispatch; already-unmaximized
// desires spend with no invented row. Included in `npm test`.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { WorldAdapterState } from "../src/world-state";
import { applyWorldReply } from "../src/world-apply";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import type { WorldReply, WorldReplyScope } from "../src/world-report";
import type { WorldObservation } from "../src/world-observe";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

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
                callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
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

function answerHandshake(flow: Flow, gen = "gen-1"): void {
    const found = [...flow.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined,
        `expected DiscoverWorld/handshake, have [${flow.calls.map((c) => c.method).join(",")}]; logs=${JSON.stringify(flow.logs)}`);
    const req = (JSON.parse((found as { payload: string }).payload) as Record<string, unknown>)["req"] as string;
    (found as { callback: (reply: unknown) => void }).callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(flow: Flow, gen = "gen-1"): void {
    const found = [...flow.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined,
        `expected DiscoverWorld/settings, have [${flow.calls.map((c) => c.method).join(",")}]; logs=${JSON.stringify(flow.logs)}`);
    const body = JSON.parse((found as { payload: string }).payload) as Record<string, unknown>;
    (found as { callback: (reply: unknown) => void }).callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
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
    foreground: "",
    rev: "1",
    border: { visible: false, reason: "disabled", subject: null, outsets: null, width: null, radius: null, witness: null },
    underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [],
};

// Full bound acceptance shape: every key the strict core-reply parser
// requires, with the live source/watermark echoed back.
function accept(
    flow: Flow, rev: string, mutate: (reply: Record<string, unknown>) => void,
): void {
    const payload = JSON.parse(lastPayload(flow, "DescribeWorld")) as Record<string, unknown>;
    const reply: Record<string, unknown> = {
        v: 4, adapter: "test-adapter", gen: "gen-1", req: payload["source"],
        obs: rev, rev, ready: true, consumed: false,
        source: payload["source"], nativeInvalid: payload["nativeInvalid"], clock: "1",
        scopes: [], outputs: [], windows: [], transfers: [], effects: [], focusRequest: null, dragPreview: null,
        gestureSettled: null, gestureAssoc: null, gestureSettledAssoc: null, operationSettled: [], nativeCreations: [], warpCursor: null,
        intentDesired: [], switchSettled: [], diagnostics: [],
        presentation: PRESENTATION,
        workspaces: [
            { id: "w1", out: "out-1", bind: "d1", scope: { kind: "output", out: "out-1" }, order: 0, mode: "tiled", exist: true, hold: "" },
            { id: "w2", out: "out-2", bind: "d2", scope: { kind: "output", out: "out-2" }, order: 0, mode: "tiled", exist: true, hold: "" },
        ],
    };
    mutate(reply);
    const text = JSON.stringify(reply);
    const parsed = parseWorldReply(text);
    assert.ok(isCoreWorldReply(parsed), "authored accept binds strict v4");
    answerDescribe(flow, reply);
}

describe("world raw mechanical attempts", () => {
    it("carries populated frame/member/view arrays with exact wire shapes", () => {
        const flow = startFlow();
        try {
            answerHandshake(flow, "gen-1");
            answerSettings(flow, "gen-1");
            runTimers(flow);
            // Round 1: converge bindings plus win-c float intent (floating
            // with untouched authority, no write token yet).
            accept(flow, "1", (reply) => {
                reply["outputs"] = [
                    { out: "out-1", view: "w1", mode: "tiled", hold: "" },
                    { out: "out-2", view: "w2", mode: "tiled", hold: "" },
                ];
                reply["windows"] = [
                    { win: "win-a", out: "out-1", ws: "w1", alloc: [0, 0, 200, 200], frame: [0, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-b", out: "out-2", ws: "w2", alloc: [800, 0, 200, 200], frame: [800, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-c", out: "out-1", ws: "w1", alloc: [10, 10, 100, 100], frame: [10, 10, 100, 100], frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: true, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
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
                    { win: "win-a", out: "out-2", ws: "w2", alloc: [800, 0, 200, 200], frame: [800, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-b", out: "out-2", ws: "w2", alloc: [800, 0, 400, 300], frame: [800, 0, 400, 300], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "op-clear-1", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-c", out: "out-1", ws: "w1", alloc: [10, 10, 100, 100], frame: [300, 200, 250, 180], frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "op-float-1", clear_op: "", floating: true, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-d", out: "out-1", ws: "w1", alloc: [100, 100, 50, 50], frame: [100, 100, 50, 50], frame_auth: "untouched", frame_hold: "", member_auth: "untouched", member_hold: "", frame_op: "", clear_op: "op-clear-2", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                ];
                reply["transfers"] = [
                    { op: "op-9", kind: "window", win: "win-a", out: "out-2", ws: "w2", follow: true, held: false, cmdSeq: null },
                    { op: "op-10", kind: "view", win: "", out: "out-1", ws: "w1", follow: false, held: false, cmdSeq: null },
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
            // setter; the exact live WorkspaceRefs ride pre/post (the
            // cross-output preimage keeps its source output, never the
            // target) for core-side translation.
            assert.deepEqual(
                members[0],
                {
                    op: "op-9", win: "win-a", out: "out-2", ws: "w2", setter: "accepted",
                    pre: { kind: "native", out: "out-1", ws: "d1" },
                    post: { kind: "native", out: "out-2", ws: "d2" },
                    attemptMs: "1",
                },
            );
            // Views: follow parent plus standalone, same native-only ref
            // rule with fresh live currents plus the observation-time
            // baseline; the adapter never invents logical proof.
            const follow = views.find((v) => v["op"] === "op-9");
            assert.deepEqual(
                follow,
                {
                    op: "op-9", out: "out-2", ws: "w2", setter: "accepted",
                    pre: { kind: "native", out: "out-2", ws: "d1" },
                    post: { kind: "native", out: "out-2", ws: "d2" },
                    base: { kind: "native", out: "out-2", ws: "d1" },
                    attemptMs: "1",
                },
            );
            const alone = views.find((v) => v["op"] === "op-10");
            assert.deepEqual(
                alone,
                {
                    op: "op-10", out: "out-1", ws: "w1", setter: "accepted",
                    pre: { kind: "native", out: "out-1", ws: "d2" },
                    post: { kind: "native", out: "out-1", ws: "d1" },
                    base: { kind: "native", out: "out-1", ws: "d2" },
                    attemptMs: "1",
                },
            );
        } finally {
            flow.handle.stop();
        }
    });
    it("serializes a genuine no-call already-at-target member row and redelivers it across a lost ACK", () => {
        // Genuine production entry path: startWorldAdapterEntry drives the
        // real observer/apply/codec/ACK seams. The native home of win-a is
        // already out-1/d1, which is exactly the binding of logical w1, so
        // a fresh unheld transfer to out-1/w1 (follow false, frame untouched)
        // must spend with zero native setters and emit one no-call evidence
        // row carrying the original broker stamp. We then lose the bound
        // reply at the transport seam and prove the row redelivers byte-for-
        // byte unchanged, before a valid bound reply consumes it and a
        // spent reapply cannot recreate it. No encoder/recordMemberAttempt/
        // ackDelivered mocking anywhere on this path.
        const flow = startFlow();
        try {
            // Count every method-style native setter at the surface seam;
            // the setters are looked up dynamically at call time, so
            // wrapping them here (after entry start) still observes them.
            let outputSends = 0;
            let viewSwitches = 0;
            const surface = flow.surface as {
                sendClientToScreen: (mover: unknown, out: unknown) => void;
                setCurrentDesktopForScreen: (deskRef: unknown, screen: { name: string }) => void;
            };
            const origSend = surface.sendClientToScreen;
            const origSwitch = surface.setCurrentDesktopForScreen;
            surface.sendClientToScreen = (mover: unknown, out: unknown): void => {
                outputSends += 1;
                origSend(mover, out);
            };
            surface.setCurrentDesktopForScreen = (deskRef: unknown, screen: { name: string }): void => {
                viewSwitches += 1;
                origSwitch(deskRef, screen);
            };
            // win-a is the subject: the property lanes are written with
            // Reflect.set (no method to wrap), so intercept the actual
            // assignments with accessors that keep the real values. This
            // counts native property setter CALLS, not just final equality.
            const winA = flow.wins[0] as FakeWindow;
            let frameWrites = 0;
            let deskWrites = 0;
            let frameValue: FakeWindow["frameGeometry"] = winA.frameGeometry;
            let deskValue: FakeWindow["desktops"] = winA.desktops;
            Object.defineProperty(winA, "frameGeometry", {
                configurable: true,
                get: (): FakeWindow["frameGeometry"] => frameValue,
                set: (v: FakeWindow["frameGeometry"]): void => { frameWrites += 1; frameValue = v; },
            });
            Object.defineProperty(winA, "desktops", {
                configurable: true,
                get: (): FakeWindow["desktops"] => deskValue,
                set: (v: FakeWindow["desktops"]): void => { deskWrites += 1; deskValue = v; },
            });
            const frameBefore = { ...frameValue };
            const homesBefore = deskValue.map((d) => d.id);

            // The v4 codec omits empty attempt lanes, so normalize the
            // absent field to an empty array. The capture source proves each
            // read is a genuinely fresh DescribeWorld, not a stale re-read.
            const lastCapture = (): { source: string; members: Array<Record<string, unknown>> } => {
                const payload = JSON.parse(lastPayload(flow, "DescribeWorld")) as Record<string, unknown>;
                const inner = payload["report"] as Record<string, unknown>;
                const members = inner["memberAttempts"];
                return {
                    source: payload["source"] as string,
                    members: (Array.isArray(members) ? members : []) as Array<Record<string, unknown>>,
                };
            };
            const malformedDescribe = (): void => {
                const found = [...flow.calls].reverse().find((c) => c.method === "DescribeWorld");
                assert.ok(found !== undefined, "expected a DescribeWorld to answer malformed");
                (found as { callback: (reply: unknown) => void }).callback("not-json");
            };
            const assertNoNativeWrites = (label: string): void => {
                assert.equal(outputSends, 0, `${label}: no sendClientToScreen`);
                assert.equal(viewSwitches, 0, `${label}: no setCurrentDesktopForScreen`);
                assert.equal(frameWrites, 0, `${label}: zero native frameGeometry assignments`);
                assert.equal(deskWrites, 0, `${label}: zero native desktops assignments`);
                assert.deepEqual(winA.frameGeometry, frameBefore, `${label}: frame untouched natively`);
                assert.deepEqual(winA.desktops.map((d) => d.id), homesBefore, `${label}: membership untouched natively`);
            };

            answerHandshake(flow, "gen-1");
            answerSettings(flow, "gen-1");
            runTimers(flow);
            // Round 1: converge the bindings (w1->d1 on out-1, w2->d2 on
            // out-2) so the transfer target resolves natively.
            accept(flow, "1", (reply) => {
                reply["outputs"] = [
                    { out: "out-1", view: "w1", mode: "tiled", hold: "" },
                    { out: "out-2", view: "w2", mode: "tiled", hold: "" },
                ];
                reply["windows"] = [
                    { win: "win-a", out: "out-1", ws: "w1", alloc: [0, 0, 200, 200], frame: [0, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-b", out: "out-2", ws: "w2", alloc: [800, 0, 200, 200], frame: [800, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                ];
            });
            flow.fire("desktopsChanged");
            runTimers(flow);
            // Round 2: one fresh, unheld window transfer of win-a to the
            // very place it already lives natively (out-1/w1 -> d1), follow
            // false, frame untouched. The apply must record a no-call
            // already-at-target member attempt with the original stamp.
            accept(flow, "2", (reply) => {
                reply["outputs"] = [
                    { out: "out-1", view: "w1", mode: "tiled", hold: "" },
                    { out: "out-2", view: "w2", mode: "tiled", hold: "" },
                ];
                reply["windows"] = [
                    { win: "win-a", out: "out-1", ws: "w1", alloc: [0, 0, 200, 200], frame: [0, 0, 200, 200], frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                ];
                reply["transfers"] = [
                    { op: "op-at-target", kind: "window", win: "win-a", out: "out-1", ws: "w1", follow: false, held: false, cmdSeq: null },
                ];
            });
            const expected = {
                op: "op-at-target", win: "win-a", out: "out-1", ws: "w1",
                setter: "already-at-target",
                pre: { kind: "native", out: "out-1", ws: "d1" },
                post: { kind: "native", out: "out-1", ws: "d1" },
                attemptMs: "1",
            };
            const diag = (): string => `logs=${JSON.stringify(flow.logs)}`;
            // Next capture via native event: the actual serialized report
            // must carry the exact already-at-target row.
            flow.fire("desktopsChanged");
            runTimers(flow);
            const carrying = lastCapture();
            assert.deepEqual(carrying.members, [expected], `actual captured report carries the exact no-call row ${diag()}`);
            const firstBytes = JSON.stringify(carrying.members);
            assertNoNativeWrites("fresh already-at-target");

            // Lose the bound reply at the transport seam: the malformed
            // data cannot ACK, so the flight retires without consuming the
            // delivered row and its original stamp. A genuinely new capture
            // must then reserialize the same bytes.
            malformedDescribe();
            flow.fire("desktopsChanged");
            runTimers(flow);
            const redelivered = lastCapture();
            assert.notEqual(redelivered.source, carrying.source, "lost ACK is followed by a genuinely fresh capture");
            assert.deepEqual(redelivered.members, [expected], `lost ACK keeps the exact row and stamp ${diag()}`);
            assert.equal(JSON.stringify(redelivered.members), firstBytes, "lost ACK redelivers the serialized row byte-for-byte unchanged");
            assertNoNativeWrites("lost-ACK redelivery");

            // A genuine bound valid reply consumes exactly the delivered row.
            accept(flow, "3", () => undefined);
            flow.fire("desktopsChanged");
            runTimers(flow);
            const consumed = lastCapture();
            assert.notEqual(consumed.source, redelivered.source, "post-ACK capture is genuinely fresh");
            assert.deepEqual(consumed.members, [], `bound valid reply consumes the delivered row ${diag()}`);
            assertNoNativeWrites("post-ack");

            // Reapplying the unchanged, already-spent transfer cannot
            // recreate the row: the dispatch fence still owns the op.
            accept(flow, "4", (reply) => {
                reply["windows"] = [
                    { win: "win-a", out: "out-1", ws: "w1", alloc: [0, 0, 200, 200], frame: [0, 0, 200, 200], frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                ];
                reply["transfers"] = [
                    { op: "op-at-target", kind: "window", win: "win-a", out: "out-1", ws: "w1", follow: false, held: false, cmdSeq: null },
                ];
            });
            flow.fire("desktopsChanged");
            runTimers(flow);
            const spent = lastCapture();
            assert.notEqual(spent.source, consumed.source, "spent-reapply capture is genuinely fresh");
            assert.deepEqual(spent.members, [], `spent transfer reapply never recreates the row ${diag()}`);
            assertNoNativeWrites("spent reapply");
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
            answerHandshake(flow, "gen-1");
            answerSettings(flow, "gen-1");
            runTimers(flow);
            accept(flow, "1", (reply) => {
                reply["outputs"] = [
                    { out: "out-1", view: "w1", mode: "tiled", hold: "" },
                    { out: "out-2", view: "w2", mode: "tiled", hold: "" },
                ];
                reply["windows"] = [
                    { win: "win-a", out: "out-1", ws: "w1", alloc: [0, 0, 200, 200], frame: [0, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-b", out: "out-2", ws: "w2", alloc: [800, 0, 200, 200], frame: [800, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-c", out: "out-1", ws: "w1", alloc: [10, 10, 100, 100], frame: [10, 10, 100, 100], frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: true, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
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
                    { win: "win-a", out: "out-2", ws: "w2", alloc: [800, 0, 200, 200], frame: [800, 0, 200, 200], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-b", out: "out-2", ws: "w2", alloc: [800, 0, 400, 300], frame: [800, 0, 400, 300], frame_auth: "controlled", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "", clear_op: "op-clear-1", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-c", out: "out-1", ws: "w1", alloc: [10, 10, 100, 100], frame: [300, 200, 250, 180], frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "", frame_op: "op-float-1", clear_op: "", floating: true, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                    { win: "win-d", out: "out-1", ws: "w1", alloc: null, frame: null, frame_auth: "hold", frame_hold: "unreadable", member_auth: "hold", member_hold: "unreadable", frame_op: "", clear_op: "op-clear-2", floating: false, float_origin: "", cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [] },
                ];
                reply["transfers"] = [
                    { op: "op-9", kind: "window", win: "win-a", out: "out-2", ws: "w2", follow: true, held: false, cmdSeq: null },
                    { op: "op-10", kind: "view", win: "", out: "out-1", ws: "w1", follow: false, held: false, cmdSeq: null },
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
        state.onDiscovery("gen-2", "adapter-a");
        assert.equal(state.framesForReport().length, 0);
    });
    it("tags the member namespace the capability allows and keeps views native-only", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.recordClock("7");
        // A native member preimage (host-owned route) retains verbatim.
        assert.equal(state.recordMemberAttempt({
            op: "op-1", win: "win-a", out: "out-2", ws: "w2",
            setter: "accepted",
            pre: { kind: "native", out: "out-1", ws: "d1" },
            post: { kind: "native", out: "out-2", ws: "d2" },
            attemptMs: "7",
        }), true);
        // A logical member preimage stays tagged logical, never rewritten
        // to the target.
        assert.equal(state.recordMemberAttempt({
            op: "op-2", win: "win-b", out: "out-1", ws: "w1",
            setter: "accepted",
            pre: { kind: "logical", out: "out-1", ws: "w9" },
            post: { kind: "logical", out: "out-1", ws: "w1" },
            attemptMs: "7",
        }), true);
        assert.deepEqual(state.memberAttemptsForReport(), [
            {
                op: "op-1", win: "win-a", out: "out-2", ws: "w2",
                setter: "accepted",
                pre: { kind: "native", out: "out-1", ws: "d1" },
                post: { kind: "native", out: "out-2", ws: "d2" },
                attemptMs: "7",
            },
            {
                op: "op-2", win: "win-b", out: "out-1", ws: "w1",
                setter: "accepted",
                pre: { kind: "logical", out: "out-1", ws: "w9" },
                post: { kind: "logical", out: "out-1", ws: "w1" },
                attemptMs: "7",
            },
        ]);
        // Views are native-only: a logical view ref has no wire spelling
        // and refuses at retention, never reaching the report lane.
        assert.equal(state.recordViewAttempt({
            op: "op-3", out: "out-1", ws: "w1",
            setter: "accepted",
            pre: { kind: "logical", out: "out-1", ws: "w1" } as never,
            post: null, base: null, attemptMs: "7",
        }), false);
        assert.equal(state.recordViewAttempt({
            op: "op-3", out: "out-1", ws: "w1",
            setter: "accepted",
            pre: { kind: "native", out: "out-1", ws: "d1" },
            post: { kind: "native", out: "out-1", ws: "d2" },
            base: { kind: "native", out: "out-1", ws: "d1" },
            attemptMs: "7",
        }), true);
        // Malformed ref ids fail closed on both lanes.
        assert.equal(state.recordMemberAttempt({
            op: "op-4", win: "win-a", out: "out-1", ws: "w1",
            setter: "accepted",
            pre: { kind: "native", out: "out-1", ws: "bad id" },
            post: null, attemptMs: "7",
        }), false);
        assert.equal(state.recordViewAttempt({
            op: "op-4", out: "out-1", ws: "w1",
            setter: "accepted", pre: null, post: null, base: null, attemptMs: "7",
        }), true);
        assert.equal(state.recordViewAttempt({
            op: "", out: "out-1", ws: "w1",
            setter: "accepted", pre: null, post: null, base: null, attemptMs: "7",
        }), false);
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
        foregroundKnown: false,
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
        scopes: WorldReplyScope[],
        outputs: Array<{ out: string; view: string; mode: string; hold: string }> = [
            { out: "out-b", view: "w9", mode: "tiled", hold: "" },
        ],
    ): Record<string, unknown> => ({
        adapter: "adapter-a", gen: "gen-1", req: "src-1", obs: "1", rev: "1",
        ready: true, consumed: false, scopes,
        workspaces: [
            { id: "w1", out: "out-b", bind: "wb1", scope: { kind: "output", out: "out-b" }, order: 0, mode: "tiled", exist: true, hold: "" },
            { id: "wr", out: "out-a", bind: "wra", scope: { kind: "output", out: "out-a" }, order: 0, mode: "tiled", exist: true, hold: "" },
        ],
        outputs,
        windows: [], transfers: [
            { op: "op-1", kind: "migration", win: "", out: "out-b", ws: "w1", follow: false, held: false, cmdSeq: null },
            { op: "op-1-refill", kind: "view", win: "", out: "out-a", ws: "wr", follow: false, held: false, cmdSeq: null },
        ],
        effects: [], focusRequest: null, dragPreview: null, gestureSettled: null, gestureAssoc: null, gestureSettledAssoc: null, operationSettled: [],
        nativeCreations: [], warpCursor: null,
        intentDesired: [], switchSettled: [], diagnostics: [],
        presentation: { foreground: "", rev: "1", suspendedOutput: null, border: { visible: false, reason: "disabled", subject: null, outsets: null, width: null, radius: null, witness: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
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
            emptyReply([{ scope: { kind: "workspace", out: "out-b", ws: "w1" }, auth: "controlled", reason: "" }]),
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
            emptyReply([{ scope: { kind: "workspace", out: "out-b", ws: "w1" }, auth: "untouched", reason: "" }]),
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
        const reply = emptyReply([{ scope: { kind: "workspace", out: "out-b", ws: "w1" }, auth: "untouched", reason: "" }]);
        (reply.windows as Array<Record<string, unknown>>).push({
            win: "win-m", out: "out-b", ws: "w1", alloc: null, frame: null,
            frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
            frame_op: "", clear_op: "", floating: false, float_origin: "",
            cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
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
                emptyReply([{ scope: { kind: "workspace", out: "out-b", ws: "w1" }, auth: "untouched", reason: "" }], c.outputs),
                null, switched, currentViews, c.gesture,
            );
            const out = applyWorldReply(env);
            assert.equal(out.viewWrites, 0, `no view writes for untouched ${c.name}`);
            assert.deepEqual(switched, [], `no view switch for untouched ${c.name}`);
            assert.ok(out.held >= 1, `untouched ${c.name} holds`);
        }
    });
    it("holds with no view for held and missing scopes, ignoring unrelated terminals", () => {
        const cases: WorldReplyScope[][] = [
            [{ scope: { kind: "workspace", out: "out-b", ws: "w1" }, auth: "hold", reason: "refused-target" }],
            [],
        ];
        for (const scopes of cases) {
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
            adapter: "adapter-a", gen: "gen-1", req: src, source: src,
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
            focused: "", readable: true, hold: "",
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
        adapter: "adapter-a", gen: "gen-1", req: "src-1", obs: "1", rev: "1",
        ready: true, consumed: false, scopes, workspaces,
        outputs: [], windows, transfers,
        effects: [], focusRequest: null, dragPreview: null, gestureSettled: null, gestureAssoc: null, gestureSettledAssoc: null, operationSettled: [],
        nativeCreations: [], warpCursor: null,
        intentDesired: [], switchSettled: [], diagnostics: [],
        presentation: { foreground: "", rev: "1", suspendedOutput: null, border: { visible: false, reason: "disabled", subject: null, outsets: null, width: null, radius: null, witness: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
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
                    frame_op: "", clear_op: "", floating: false, float_origin: "",
                    cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
                }],
                [{ op: "op-9", kind: "window", win: "win-a", out: "out-2", ws: "w2", follow: false, held: false, cmdSeq: null }],
                [{ id: "w2", out: "out-2", bind: "d2", scope: { kind: "output", out: "out-2" }, order: 0, mode: "tiled", exist: true, hold: "" }],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
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
            setter: "accepted",
            pre: { kind: "native", out: "out-1", ws: "d1" },
            post: { kind: "native", out: "out-2", ws: "d1" },
            attemptMs: "9",
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
                        frame_op: "", clear_op: "", floating: false, float_origin: "",
                        cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
                    },
                    {
                        win: "win-n", out: "out-b", ws: "w9", alloc: null, frame: null,
                        frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
                        frame_op: "", clear_op: "", floating: false, float_origin: "",
                        cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
                    },
                ],
                [
                    { op: "op-7", kind: "migration", win: "", out: "out-b", ws: "w1", follow: false, held: false, cmdSeq: null },
                    { op: "op-8", kind: "migration", win: "", out: "out-b", ws: "w9", follow: false, held: false, cmdSeq: null },
                ],
                [
                    { id: "w1", out: "out-b", bind: "wb1", scope: { kind: "output", out: "out-b" }, order: 0, mode: "tiled", exist: true, hold: "" },
                    { id: "w9", out: "out-b", bind: "wbX", scope: { kind: "output", out: "out-b" }, order: 1, mode: "tiled", exist: true, hold: "" },
                ],
                [
                    { scope: { kind: "workspace", out: "out-b", ws: "w1" }, auth: "controlled", reason: "" },
                    { scope: { kind: "workspace", out: "out-b", ws: "w9" }, auth: "controlled", reason: "" },
                ],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
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
                    frame_op: "", clear_op: "", floating: false, float_origin: "",
                    cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
                }],
                [{ op: "op-9", kind: "window", win: "win-a", out: "out-2", ws: "w2", follow: false, held: false, cmdSeq: null }],
                [{ id: "w2", out: "out-2", bind: "d2", scope: { kind: "output", out: "out-2" }, order: 0, mode: "tiled", exist: true, hold: "" }],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
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
        const callRow = {
            op: "op-9", win: "win-a", out: "out-2", ws: "w2",
            setter: "threw",
            pre: { kind: "native", out: "out-1", ws: "d1" },
            post: { kind: "native", out: "out-2", ws: "d2" },
            attemptMs: "9",
        };
        const result = applyWorldReply(env);
        assert.equal(setters, 1, "output setter ran once before throwing");
        assert.deepEqual(state.memberAttemptsForReport(), [callRow], "throw after moving keeps the threw disposition, never accepted");
        assert.equal(result.membershipWrites, 0, "ambiguous throw earns no write credit");

        // The throw already moved the window to the native target. Reapply
        // the same spent op at a later clock: the already-at-target no-call
        // path must NOT replace the prior call proof or restamp it.
        state.recordClock("10");
        const replay = applyWorldReply(env);
        assert.equal(setters, 1, "no second output call on the already-at-target replay");
        assert.equal(replay.membershipWrites, 0, "no-call replay earns no write credit");
        assert.deepEqual(state.memberAttemptsForReport(), [callRow], "prior threw call proof is never replaced or restamped by a no-call");

        // A bound ACK consumes the delivered call row; a later unchanged
        // spent apply must not recreate evidence or run any setter.
        const flight = state.beginCapture("src-1", null);
        assert.ok(flight !== null);
        assert.equal(flight.delivered.members.length, 1);
        assert.equal(state.ackDelivered("src-1", true), 1, "bound ACK retires the delivered threw row");
        assert.deepEqual(state.memberAttemptsForReport(), [], "bound ACK consumes the row");
        const afterAck = applyWorldReply(env);
        assert.equal(setters, 1, "no setter on the spent replay after ACK");
        assert.equal(afterAck.membershipWrites, 0, "no write credit after ACK");
        assert.deepEqual(state.memberAttemptsForReport(), [], "spent replay after ACK never recreates evidence");
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
                    frame_op: "", clear_op: "", floating: false, float_origin: "",
                    cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
                }],
                [{ op: "op-10", kind: "migration", win: "", out: "out-b", ws: "w1", follow: false, held: false, cmdSeq: null }],
                [{ id: "w1", out: "out-b", bind: "wb1", scope: { kind: "output", out: "out-b" }, order: 0, mode: "tiled", exist: true, hold: "" }],
                [{ scope: { kind: "workspace", out: "out-b", ws: "w1" }, auth: "controlled", reason: "" }],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
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
            setter: "accepted",
            pre: { kind: "native", out: "out-a", ws: "da" },
            post: null,
            attemptMs: "9",
        }], "superseded flight keeps the real output disposition");
        assert.equal(result.membershipWrites, 0, "superseded flight earns no write credit");
    });

    it("fresh already-at-target window transfer spends and emits one no-call member attempt with retained evidence", () => {
        const state = new WorldAdapterState("adapter-a", "owner-a");
        state.boundGen = "gen-1";
        state.recordClock("9");
        const ref = {};
        // Live homing already at the native target; observation lags behind.
        const homing = new Map<object, { out: string; ws: string }>([[ref, { out: "out-2", ws: "d2" }]]);
        let setters = 0;
        const noSetter = (label: string): boolean => {
            setters += 1;
            throw new Error(`no ${label} setter expected`);
        };
        const env = {
            state,
            observation: setterObservation([{ win: "win-a", ref, out: "out-1", ws: "d1" }]),
            reply: setterReply(
                [{
                    win: "win-a", out: "out-1", ws: "w1", alloc: null, frame: null,
                    frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
                    frame_op: "", clear_op: "", floating: false, float_origin: "",
                    cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
                }],
                [{ op: "op-new", kind: "window", win: "win-a", out: "out-2", ws: "w2", follow: false, held: false, cmdSeq: null }],
                [{ id: "w2", out: "out-2", bind: "d2", scope: { kind: "output", out: "out-2" }, order: 0, mode: "tiled", exist: true, hold: "" }],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => true,
            readLiveRect: (): null => null,
            readLiveMembership: (r: object): { out: string; ws: string } | null => homing.get(r) ?? null,
            readLiveSafety: setterSafety,
            setGeometry: (): boolean => noSetter("frame"),
            setMembership: (): boolean => noSetter("desktop"),
            sendToOutput: (): boolean => noSetter("output"),
            resolveOutput: (out: string): object => ({ name: out }),
            resolveDesktop: (ws: string): object => ({ id: ws }),
            switchView: (): boolean => noSetter("view"),
            createDesktop: (): void => undefined,
            removeDesktop: (): boolean => false,
            focusWindow: (): boolean => false,
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: (): boolean => false,
            readLiveView: (): string | null => null,
            desktopById: (id: string): object => ({ id }),
            minDesktops: 1,
        };
        const expected = {
            op: "op-new", win: "win-a", out: "out-2", ws: "w2",
            setter: "already-at-target",
            pre: { kind: "native", out: "out-2", ws: "d2" },
            post: { kind: "native", out: "out-2", ws: "d2" },
            attemptMs: "9",
        };
        // Gate negatives BEFORE the first successful spend: each must leave
        // no row, leave the op undispatched, and run zero setters.
        const assertNoEvidence = (label: string): void => {
            assert.deepEqual(state.memberAttemptsForReport(), [], `${label}: no row`);
            assert.equal(state.isDispatched("op-new"), false, `${label}: op stays undispatched`);
            assert.equal(setters, 0, `${label}: zero setters`);
        };
        const heldReply = setterReply(
            [{
                win: "win-a", out: "out-1", ws: "w1", alloc: null, frame: null,
                frame_auth: "untouched", frame_hold: "", member_auth: "controlled", member_hold: "",
                frame_op: "", clear_op: "", floating: false, float_origin: "",
                cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
            }],
            [{ op: "op-new", kind: "window", win: "win-a", out: "out-2", ws: "w2", follow: false, held: true, cmdSeq: null }],
            [{ id: "w2", out: "out-2", bind: "d2", scope: { kind: "output", out: "out-2" }, order: 0, mode: "tiled", exist: true, hold: "" }],
        );
        applyWorldReply({ ...env, reply: heldReply });
        assertNoEvidence("held transfer");
        applyWorldReply({ ...env, isStillListed: () => false });
        assertNoEvidence("unlisted member");
        applyWorldReply({ ...env, readLiveSafety: () => null });
        assertNoEvidence("unreadable safety");
        let membershipReads = 0;
        applyWorldReply({
            ...env,
            readLiveMembership: (): { out: string; ws: string } | null => {
                membershipReads += 1;
                return membershipReads === 1 ? { out: "out-2", ws: "d2" } : null;
            },
        });
        assertNoEvidence("membership unreadable at preSetterOk");
        assert.equal(membershipReads, 2, "target read then null at the preSetterOk recheck");

        // 1. Fresh unheld transfer with live homing already at target: no
        // setter runs, one no-call evidence row rides with the original
        // write stamp, and the op spends its dispatch fence.
        const first = applyWorldReply(env);
        assert.equal(setters, 0, "no setter runs when live homing is already at target");
        assert.equal(first.membershipWrites, 0, "zero membership writes");
        assert.deepEqual(state.memberAttemptsForReport(), [expected], "one exact already-at-target row");
        assert.equal(state.isDispatched("op-new"), true, "fresh transfer spends its dispatch fence");

        // 2. Replay the unchanged envelope at a later broker clock: the row
        // and its original stamp survive untouched, with no new setters.
        state.recordClock("10");
        const replay = applyWorldReply(env);
        assert.equal(setters, 0, "replay runs no setters");
        assert.equal(replay.membershipWrites, 0, "replay earns no write credit");
        assert.deepEqual(state.memberAttemptsForReport(), [expected], "replay keeps the original row and stamp");

        // 3. Raw pending evidence survives a lost acknowledgement.
        const lost = state.beginCapture("src-1", null);
        assert.ok(lost !== null);
        assert.equal(lost.delivered.members.length, 1, "flight snapshots the member row");
        assert.equal(state.ackDelivered("src-1", false), 0, "lost ack retires nothing");
        assert.deepEqual(state.memberAttemptsForReport(), [expected], "row survives lost ack");
        assert.equal(state.releaseFlight("src-1"), true, "release the unacknowledged flight");

        // 4. A later capture plus a bound ACK retires the exact delivered row.
        const acked = state.beginCapture("src-2", null);
        assert.ok(acked !== null);
        assert.equal(acked.delivered.members.length, 1);
        assert.equal(state.ackDelivered("src-2", true), 1, "bound ack retires the delivered row");
        assert.deepEqual(state.memberAttemptsForReport(), [], "exact delivered equality removes the row");

        // 5. Reapplying the same spent op never recreates the row or setters.
        const spent = applyWorldReply(env);
        assert.equal(setters, 0, "spent op runs no setters");
        assert.equal(spent.membershipWrites, 0, "spent op earns no write credit");
        assert.deepEqual(state.memberAttemptsForReport(), [], "spent op never recreates the row");

        // 6. Fresh bound state at clock 9: membership readable at both the
        // target check and preSetterOk recheck, but unreadable at the post
        // read. The no-call row must store post:null, never a fabricated
        // equal proof, while still spending its dispatch fence with no
        // native setter.
        const state2 = new WorldAdapterState("adapter-a", "owner-a");
        state2.boundGen = "gen-1";
        state2.recordClock("9");
        let lateReads = 0;
        const env2 = {
            ...env,
            state: state2,
            readLiveMembership: (): { out: string; ws: string } | null => {
                lateReads += 1;
                return lateReads <= 2 ? { out: "out-2", ws: "d2" } : null;
            },
        };
        applyWorldReply(env2);
        assert.equal(lateReads, 3, "membership read at target, at preSetterOk, then at post");
        assert.deepEqual(state2.memberAttemptsForReport(), [{
            op: "op-new", win: "win-a", out: "out-2", ws: "w2",
            setter: "already-at-target",
            pre: { kind: "native", out: "out-2", ws: "d2" },
            post: null,
            attemptMs: "9",
        }], "unreadable post stores null, never a fabricated equal proof");
        assert.equal(state2.isDispatched("op-new"), true, "fresh no-call still spends its dispatch fence");
        assert.equal(setters, 0, "zero setters with an unreadable post read");
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
                    frame_op: "", clear_op: "op-clear-1", floating: false, float_origin: "",
                    cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
                }],
                [],
                [],
            ),
            dispatchWatermark: "0",
            takeResult: null,
            log: (): void => undefined,
            isStillListed: (): boolean => opts.listed ?? true,
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
        state.onDiscovery("gen-2", "adapter-a");
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

    describe("retained operation/switch outcomes have no adapter authority", () => {
        const mustWorldReply = (raw: Record<string, unknown>): WorldReply => {
            const parsed = parseWorldReply(JSON.stringify({
                v: 4,
                ...raw,
                presentation: DISABLED_PRESENTATION,
            }));
            assert.ok(parsed !== null && isCoreWorldReply(parsed), "expected valid bound WorldReply");
            return parsed;
        };

        it("fresh accepted operationSettled and switchSettled grant no extra clear writes and preserve spend fences", () => {
            const state = boundClear();
            const { env, calls } = clearEnv(state, { maximized: true, clear: "refuse" });
            let switchCalls = 0;
            const countedEnv = {
                ...env,
                switchView: (): boolean => {
                    switchCalls += 1;
                    return false;
                },
            };
            const f1 = state.beginCapture("src-1", null);
            assert.ok(f1 !== null);
            const reply1 = mustWorldReply({
                ...(countedEnv.reply as unknown as Record<string, unknown>),
                req: "src-1",
                source: "src-1",
                obs: "1",
                rev: "1",
                clock: "9",
            });
            assert.deepEqual(state.acceptReply(reply1, "src-1", f1.watermark), { applied: true, reason: "accepted" });
            const first = applyWorldReply({ ...countedEnv, reply: reply1, dispatchWatermark: f1.watermark });
            assert.equal(calls(), 1, "baseline clear setter ran once");
            assert.equal(switchCalls, 0, "no switchView call before fresh accepted outcome application");
            assert.equal(first.clears, 0, "refused clear leaves window maximized (still needing setter)");
            assert.ok(state.clearSpent.has("op-clear-1"), "clear token spent before fresh reply");
            assert.ok(state.isDispatched("op-clear-1"), "dispatch fence recorded before fresh reply");
            assert.equal(state.clearAttemptsForReport().length, 1, "nonempty clear attempt recorded");

            // Fresh bound ready reply carrying scripted valid operationSettled
            // (clear + float) and switchSettled payloads with the same desire
            // and native maximized=true state still needing a setter.
            const f2 = state.beginCapture("src-2", null);
            assert.ok(f2 !== null);
            assert.equal(f2.delivered.clears.length, 1);
            const reply2 = mustWorldReply({
                ...(countedEnv.reply as unknown as Record<string, unknown>),
                req: "src-2",
                source: "src-2",
                obs: "2",
                rev: "2",
                clock: "10",
                operationSettled: [
                    { win: "win-c", kind: "clear", op: "op-clear-1", outcome: "refused-proven", cause: "refused-with-unmaximized" },
                    { win: "win-c", kind: "float", op: "op-float-1", outcome: "float-frame-proven", cause: "" },
                ],
                switchSettled: [
                    { out: "out-1", op: "op-switch-1", outcome: "complete", failed: [], focus: "proven", fallback: false },
                ],
            });
            assert.deepEqual(state.acceptReply(reply2, "src-2", f2.watermark), { applied: true, reason: "accepted" });
            const second = applyWorldReply({ ...countedEnv, reply: reply2, dispatchWatermark: f2.watermark });
            assert.equal(calls(), 1, "fresh accepted outcomes grant zero extra clear calls");
            assert.equal(switchCalls, 0, "fresh accepted outcomes grant zero switchView calls");
            assert.equal(second.clears, 0);
            assert.ok(state.clearSpent.has("op-clear-1"), "clear spend fence persists");
            assert.ok(state.isDispatched("op-clear-1"), "dispatch fence persists");
            assert.deepEqual(state.clearAttemptsForReport(), [], "exact delivered equality retires old unchanged clear receipt");
        });

        it("fresh accepted outcomes grant no extra float writes and never replace or retire newer same-key frame receipts", () => {
            const state = new WorldAdapterState("adapter-a", "owner-a");
            state.boundGen = "gen-1";
            state.recordClock("9");
            const ref = {};
            let geoCalls = 0;
            let switchCalls = 0;
            const makeFloatReply = (over: Record<string, unknown>, winOver: Record<string, unknown> = {}): WorldReply => mustWorldReply({
                ...(setterReply(
                    [{
                        win: "win-a", out: "out-1", ws: "w1", alloc: null, frame: [10, 20, 300, 200],
                        frame_auth: "untouched", frame_hold: "", member_auth: "untouched", member_hold: "",
                        frame_op: "op-float-1", clear_op: "", floating: true, float_origin: "",
                        cmdSeq: null, visibility: null, visibility_auth: "untouched", visibility_hold: "", state: [],
                        ...winOver,
                    }],
                    [],
                    [],
                ) as unknown as Record<string, unknown>),
                ...over,
            });
            const floatEnv = (reply: WorldReply, watermark: string) => ({
                state,
                observation: setterObservation([{ win: "win-a", ref, out: "out-1", ws: "d1" }]),
                reply,
                dispatchWatermark: watermark,
                takeResult: null,
                log: (): void => undefined,
                isStillListed: (): boolean => true,
                readLiveRect: (): { x: number; y: number; w: number; h: number } => ({ x: 0, y: 0, w: 100, h: 100 }),
                readLiveMembership: (): { out: string; ws: string } => ({ out: "out-1", ws: "d1" }),
                readLiveSafety: setterSafety,
                setGeometry: (): boolean => {
                    geoCalls += 1;
                    return false;
                },
                setMembership: (): boolean => { throw new Error("no desktop setter expected"); },
                sendToOutput: (): boolean => { throw new Error("no output setter expected"); },
                resolveOutput: (out: string): object => ({ name: out }),
                resolveDesktop: (ws: string): object => ({ id: ws }),
                switchView: (): boolean => {
                    switchCalls += 1;
                    return false;
                },
                createDesktop: (): void => undefined,
                removeDesktop: (): boolean => false,
                focusWindow: (): boolean => false,
                desktopOccupants: () => ({ total: 0, readable: true }),
                desktopVisible: (): boolean => false,
                readLiveView: (): string | null => null,
                desktopById: (id: string): object => ({ id }),
                minDesktops: 1,
            });

            // Deferred-apply ordering on the public state/apply seam:
            // 1) Accept src-1 (obs=1, rev=1) desiring op-float-1, deferring its apply
            //    until after src-2 capture so src-2 snapshots [] while op-float-1
            //    becomes live mid-flight.
            const f1 = state.beginCapture("src-1", null);
            assert.ok(f1 !== null);
            const reply1 = makeFloatReply({ req: "src-1", source: "src-1", obs: "1", rev: "1", clock: "9" });
            assert.deepEqual(state.acceptReply(reply1, "src-1", f1.watermark), { applied: true, reason: "accepted" });

            // 2) Begin src-2 capture (empty delivered snapshot), then run deferred reply1
            //    apply to produce raw receipt op-float-1 while src-2 is in flight.
            const f2 = state.beginCapture("src-2", null);
            assert.ok(f2 !== null);
            assert.equal(f2.delivered.frames.length, 0);
            applyWorldReply(floatEnv(reply1, f1.watermark));
            assert.equal(geoCalls, 1, "baseline float setGeometry ran once");
            assert.equal(switchCalls, 0, "no switchView call on baseline float apply");
            assert.ok(state.isDispatched("op-float-1"), "float dispatch fence recorded before fresh reply");
            assert.equal(state.framesForReport().length, 1);
            assert.equal(state.framesForReport()[0]?.op, "op-float-1");

            // 3) Accept genuinely fresh src-2 (obs=2, rev=2) desiring op-float-2,
            //    deferring its apply; empty f2 delivered snapshot leaves op-float-1 intact.
            const reply2 = makeFloatReply(
                { req: "src-2", source: "src-2", obs: "2", rev: "2", clock: "10" },
                { frame_op: "op-float-2", frame: [20, 30, 400, 300] },
            );
            assert.deepEqual(state.acceptReply(reply2, "src-2", f2.watermark), { applied: true, reason: "accepted" });
            assert.equal(state.framesForReport()[0]?.op, "op-float-1");

            // 4) Begin src-3 capture (snapshots op-float-1), then run deferred accepted
            //    reply2 apply to generate newer same-key op-float-2 receipt while src-3
            //    is in flight.
            const f3 = state.beginCapture("src-3", null);
            assert.ok(f3 !== null);
            assert.equal(f3.delivered.frames[0]?.op, "op-float-1");
            applyWorldReply(floatEnv(reply2, f2.watermark));
            assert.equal(geoCalls, 2, "accepted reply2 op-float-2 ran setGeometry once");
            assert.equal(switchCalls, 0, "no switchView call before fresh accepted outcome application");
            assert.equal(state.framesForReport()[0]?.op, "op-float-2");
            assert.notEqual(f3.delivered.frames[0]?.op, state.framesForReport()[0]?.op, "delivered snapshot op-float-1 and newer stored op-float-2 differ");

            // 5) Accept fresh src-3 (obs=3, rev=3) carrying scripted valid clear/float/switch
            //    settlements and the same spent op-float-1 desire: grants zero extra writes,
            //    preserves op-float-1 dispatch fence, and leaves newer op-float-2 intact.
            const reply3 = makeFloatReply({
                req: "src-3",
                source: "src-3",
                obs: "3",
                rev: "3",
                clock: "11",
                operationSettled: [
                    { win: "win-a", kind: "clear", op: "op-clear-1", outcome: "continued-tiled", cause: "" },
                    { win: "win-a", kind: "float", op: "op-float-1", outcome: "float-refused-proven", cause: "fixed" },
                ],
                switchSettled: [
                    { out: "out-1", op: "op-switch-1", outcome: "completed-with-failures", failed: ["win-a"], focus: "unproven", fallback: true },
                ],
            });
            assert.deepEqual(state.acceptReply(reply3, "src-3", f3.watermark), { applied: true, reason: "accepted" });
            applyWorldReply(floatEnv(reply3, f3.watermark));
            assert.equal(geoCalls, 2, "fresh accepted outcomes grant zero extra float writes despite unmoved rect");
            assert.equal(switchCalls, 0, "fresh accepted outcomes grant zero switchView calls");
            assert.ok(state.isDispatched("op-float-1"), "float dispatch fence persists");
            assert.equal(state.framesForReport().length, 1, "newer same-key frame receipt survives older acceptance");
            assert.equal(state.framesForReport()[0]?.op, "op-float-2");
            assert.deepEqual(state.framesForReport()[0]?.target, [20, 30, 400, 300]);

            // Settlement rows in an unbound/mismatched reply never substitute for attempt receipt.
            const mismatchReply = makeFloatReply({
                req: "src-wrong",
                source: "src-wrong",
                obs: "4",
                rev: "4",
                clock: "12",
                operationSettled: [
                    { win: "win-a", kind: "float", op: "op-float-2", outcome: "float-frame-proven", cause: "" },
                ],
                switchSettled: [
                    { out: "out-1", op: "op-switch-2", outcome: "complete", failed: [], focus: "proven", fallback: false },
                ],
            });
            assert.equal(state.acceptReply(mismatchReply, "src-4", state.currentInvalid).applied, false);
            assert.equal(state.framesForReport().length, 1, "settlement rows alone do not retire attempt receipt");

            // Exact delivered equality on a subsequent bound accept retires op-float-2.
            const f4 = state.beginCapture("src-4", null);
            assert.ok(f4 !== null);
            const reply4 = makeFloatReply({
                req: "src-4",
                source: "src-4",
                obs: "5",
                rev: "5",
                clock: "13",
                operationSettled: [
                    { win: "win-a", kind: "float", op: "op-float-2", outcome: "float-frame-proven", cause: "" },
                ],
            });
            assert.deepEqual(state.acceptReply(reply4, "src-4", f4.watermark), { applied: true, reason: "accepted" });
            assert.deepEqual(state.framesForReport(), [], "exact delivered equality retires unchanged frame receipt on bound accept");
        });
    });
});
