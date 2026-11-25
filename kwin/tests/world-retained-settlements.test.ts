// Retained latest settlement outcomes: strict wire validation, KDE
// current-head FIFO discharge independent of issuing flight, idle-retry
// preservation, diagnostic-only operation/switch lanes, and genuine
// engine->codec->broker->entry latest-only & 256-eviction journeys.

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { parseConnectionError } from "../src/world-connection";
import { startWorldAdapterEntry } from "../src/world-entry";
import { parseNativeGesture } from "../src/world-gesture";
import { isCoreWorldReply, isWorldRejection, parseWorldReply } from "../src/world-report";
import { compareU64 } from "../src/world-seq";
import { WorldBridge } from "./world-producer-harness";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

function makeBaseReply(over: Record<string, unknown> = {}): Record<string, unknown> {
    return {
        v: 4,
        adapter: "test-adapter",
        gen: "gen-1",
        req: "src-1",
        obs: "1",
        rev: "1",
        ready: true,
        consumed: false,
        scopes: [],
        workspaces: [],
        outputs: [],
        windows: [],
        transfers: [],
        focusRequest: null,
        dragPreview: null,
        gestureSettled: null,
        operationSettled: [],
        nativeCreations: [],
        warpRequest: null,
        presentation: DISABLED_PRESENTATION,
        intentDesired: [],
        switchSettled: [],
        diagnostics: [],
        source: "src-1",
        nativeInvalid: "1",
        clock: "1",
        gestureAssoc: null,
        gestureSettledAssoc: null,
        ...over,
    };
}

function makeBaseRejection(over: Record<string, unknown> = {}): Record<string, unknown> {
    return {
        v: 4,
        outcome: "rejected",
        kind: "busy",
        message: "busy",
        gen: "gen-1",
        source: "",
        nativeInvalid: "0",
        clock: "1",
        gestureAssoc: null,
        gestureSettledAssoc: null,
        ...over,
    };
}

function gestureJson(over: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 1,
        instance: "gesture-1",
        serial: "1",
        revision: "1",
        phase: "started",
        win: "win-a",
        kind: "move",
        startRect: [0, 0, 200, 200],
        startCursor: [10, 10],
        cursor: [20, 20],
        modalMove: 1,
        changed: 1,
        superseded: 0,
        available: 1,
        ...over,
    });
}

function idleGestureJson(revision = "99"): string {
    return JSON.stringify({
        v: 1,
        instance: "gesture-1",
        serial: "0",
        revision,
        phase: "idle",
        win: "",
        kind: "",
        startRect: null,
        startCursor: null,
        cursor: null,
        modalMove: 0,
        changed: 1,
        superseded: 0,
        available: 1,
    });
}

function setupEntryHarness(opts: {
    wins?: Array<{ id: string; rect: { x: number; y: number; width: number; height: number } }>;
    onLog?: (line: string) => void;
} = {}) {
    const timers: Array<{ delay: number; cb: () => void; cancelled: boolean }> = [];
    const logs: string[] = [];
    const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
    const gestureCalls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
    const geometryWrites: string[] = [];
    const desktop = { id: "ws1", name: "ws1" };
    const out = { name: "out-a", geometry: { x: 0, y: 0, width: 1920, height: 1080 } };
    const wins = (opts.wins ?? [{ id: "win-a", rect: { x: 0, y: 0, width: 400, height: 400 } }]).map((w) => {
        let liveRect = { ...w.rect };
        const win = {
            internalId: w.id,
            managed: true,
            deleted: false,
            transient: false,
            transientFor: null,
            dialog: false,
            modal: false,
            wantsInput: true,
            normalWindow: true,
            output: out,
            desktops: [desktop],
            minimized: false,
            hidden: false,
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            minSize: { width: 100, height: 100 },
            maxSize: { width: 2147483647, height: 2147483647 },
            pid: 1234,
            resourceClass: "Example",
            caption: w.id,
            windowRole: "normal",
            desktopFileName: "example.app",
            resizeable: true,
            keepAbove: false,
            keepBelow: false,
            outputChanged: { connect: () => undefined, disconnect: () => undefined },
            desktopsChanged: { connect: () => undefined, disconnect: () => undefined },
            hiddenChanged: { connect: () => undefined, disconnect: () => undefined },
            frameGeometryChanged: { connect: () => undefined, disconnect: () => undefined },
        } as Record<string, unknown>;
        Object.defineProperty(win, "frameGeometry", {
            configurable: true,
            get: () => liveRect,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                liveRect = { ...rect };
                geometryWrites.push(`${w.id}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
        Object.defineProperty(win, "clientGeometry", {
            configurable: true,
            get: () => liveRect,
        });
        return win;
    });
    const activatedListeners: Array<() => void> = [];
    const workspace = {
        windowList: () => wins,
        screens: [out],
        desktops: [desktop],
        currentDesktop: desktop,
        activeScreen: out,
        cursorPos: { x: 20, y: 20 },
        currentDesktopForScreen: () => desktop,
        activeWindow: wins[0] ?? null,
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1080 }),
        windowAdded: { connect: () => undefined, disconnect: () => undefined },
        windowRemoved: { connect: () => undefined, disconnect: () => undefined },
        currentDesktopChanged: { connect: () => undefined, disconnect: () => undefined },
        desktopsChanged: { connect: () => undefined, disconnect: () => undefined },
        screensChanged: { connect: () => undefined, disconnect: () => undefined },
        windowActivated: {
            connect: (fn: () => void) => {
                activatedListeners.push(fn);
            },
            disconnect: (fn: () => void) => {
                const idx = activatedListeners.indexOf(fn);
                if (idx >= 0) {
                    activatedListeners.splice(idx, 1);
                }
            },
        },
    };
    const fireWindowActivated = (): void => {
        for (const fn of [...activatedListeners]) {
            fn();
        }
    };
    const handle = startWorldAdapterEntry({
        options: { perOutputVirtualDesktops: true },
        workspace: workspace as never,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback });
        },
        gestureCallDbus: (_s, _p, _i, method, payload, callback) => {
            gestureCalls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            const entry = { delay, cb, cancelled: false };
            timers.push(entry);
            return () => {
                entry.cancelled = true;
            };
        },
        log: (line) => {
            logs.push(line);
            opts.onLog?.(line);
        },
    });
    assert.ok(handle !== null);

    const runTimers = (filterDelay?: number): void => {
        const len = timers.length;
        for (let i = 0; i < len; i += 1) {
            const t = timers[i]!;
            if (t.cancelled) {
                continue;
            }
            if (filterDelay === undefined ? t.delay <= 120 : t.delay === filterDelay) {
                t.cancelled = true;
                t.cb();
            }
        }
    };

    const connectScripted = (adapter = "test-adapter", gen = "gen-1"): void => {
        const hs = calls.find((c) => c.method === "DiscoverWorld");
        assert.ok(hs !== undefined, "handshake DiscoverWorld pending");
        calls.splice(calls.indexOf(hs), 1);
        const hsBody = JSON.parse(hs.payload) as { req: string };
        hs.callback(JSON.stringify({
            type: "handshake-reply", v: 4, adapter, req: hsBody.req, gen, ready: false, kind: "",
        }));
        const st = calls.find((c) => c.method === "DiscoverWorld");
        assert.ok(st !== undefined, "settings DiscoverWorld pending");
        calls.splice(calls.indexOf(st), 1);
        const stBody = JSON.parse(st.payload) as { req: string; rev: string };
        st.callback(JSON.stringify({
            type: "settings-reply", v: 4, adapter, gen, req: stBody.req, rev: stBody.rev, status: "accepted", diagnostics: [],
        }));
        runTimers();
    };

    const popDescribe = () => {
        const idx = calls.findIndex((c) => c.method === "DescribeWorld");
        assert.ok(idx >= 0, "DescribeWorld call pending");
        const call = calls.splice(idx, 1)[0]!;
        const body = JSON.parse(call.payload) as { adapter: string; gen: string; source: string; nativeInvalid: string; report: Record<string, unknown> };
        return { call, body };
    };

    const popWaitGesture = () => {
        const idx = gestureCalls.findIndex((c) => c.method === "WaitGesture");
        assert.ok(idx >= 0, "WaitGesture call pending");
        return gestureCalls.splice(idx, 1)[0]!;
    };

    const ackCalls = () => gestureCalls.filter((c) => c.method === "AckGesture");

    return { handle, timers, logs, calls, gestureCalls, geometryWrites, runTimers, fireWindowActivated, connectScripted, popDescribe, popWaitGesture, ackCalls };
}

describe("retained settlements strict codec & key validation", () => {
    it("requires exact gestureSettledAssoc on bound replies and rejects malformed/missing", () => {
        const goodNull = parseWorldReply(JSON.stringify(makeBaseReply()));
        assert.ok(isCoreWorldReply(goodNull));
        assert.equal(goodNull.gestureSettledAssoc, null);

        const assoc = { instance: "g-1", serial: "4", coreId: "2" };
        const goodPresent = parseWorldReply(JSON.stringify(makeBaseReply({
            gestureSettled: { id: "2", committed: true },
            gestureSettledAssoc: assoc,
        })));
        assert.ok(isCoreWorldReply(goodPresent));
        assert.deepEqual(goodPresent.gestureSettledAssoc, assoc);

        // Missing gestureSettledAssoc on bound reply refuses.
        const missing = makeBaseReply();
        delete missing["gestureSettledAssoc"];
        assert.equal(parseWorldReply(JSON.stringify(missing)), null);

        // Extra key on gestureSettledAssoc refuses.
        assert.equal(parseWorldReply(JSON.stringify(makeBaseReply({
            gestureSettledAssoc: { instance: "g-1", serial: "4", coreId: "2", extra: "x" },
        }))), null);

        // Zero coreId or empty instance or non-canonical serial refuses.
        assert.equal(parseWorldReply(JSON.stringify(makeBaseReply({
            gestureSettledAssoc: { instance: "g-1", serial: "4", coreId: "0" },
        }))), null);
        assert.equal(parseWorldReply(JSON.stringify(makeBaseReply({
            gestureSettledAssoc: { instance: "", serial: "4", coreId: "1" },
        }))), null);
        assert.equal(parseWorldReply(JSON.stringify(makeBaseReply({
            gestureSettledAssoc: { instance: "g-1", serial: "01", coreId: "1" },
        }))), null);
    });

    it("requires null gestureSettledAssoc on rejection and connection-error envelopes", () => {
        const goodRej = parseWorldReply(JSON.stringify(makeBaseRejection()));
        assert.ok(goodRej !== null && isWorldRejection(goodRej));
        const goodConnErr = parseConnectionError(JSON.stringify(makeBaseRejection({ kind: "generation-mismatch" })));
        assert.ok(goodConnErr !== null);

        // Missing gestureSettledAssoc refuses on both.
        const missing = makeBaseRejection();
        delete missing["gestureSettledAssoc"];
        assert.equal(parseWorldReply(JSON.stringify(missing)), null);
        assert.equal(parseConnectionError(JSON.stringify(missing)), null);

        // Non-null gestureSettledAssoc refuses on both.
        const nonNull = makeBaseRejection({
            gestureSettledAssoc: { instance: "g-1", serial: "1", coreId: "1" },
        });
        assert.equal(parseWorldReply(JSON.stringify(nonNull)), null);
        assert.equal(parseConnectionError(JSON.stringify(nonNull)), null);
    });

    it("validates operationSettled exact keys, kind/outcome agreement, and (win, kind) uniqueness", () => {
        // Same win + same op with BOTH clear and float phases is valid.
        const bothPhases = parseWorldReply(JSON.stringify(makeBaseReply({
            operationSettled: [
                { win: "win-a", kind: "clear", op: "op-1", outcome: "continued-float", cause: "" },
                { win: "win-a", kind: "float", op: "op-1", outcome: "float-frame-proven", cause: "" },
            ],
        })));
        assert.ok(isCoreWorldReply(bothPhases));
        assert.equal(bothPhases.operationSettled.length, 2);

        // Kind/outcome disagreement refuses.
        assert.equal(parseWorldReply(JSON.stringify(makeBaseReply({
            operationSettled: [{ win: "win-a", kind: "float", op: "op-1", outcome: "continued-tiled", cause: "" }],
        }))), null);
        assert.equal(parseWorldReply(JSON.stringify(makeBaseReply({
            operationSettled: [{ win: "win-a", kind: "clear", op: "op-1", outcome: "float-frame-proven", cause: "" }],
        }))), null);

        // Duplicate (win, kind) refuses even with different op.
        assert.equal(parseWorldReply(JSON.stringify(makeBaseReply({
            operationSettled: [
                { win: "win-a", kind: "clear", op: "op-1", outcome: "continued-tiled", cause: "" },
                { win: "win-a", kind: "clear", op: "op-2", outcome: "refused-proven", cause: "constraints" },
            ],
        }))), null);

        // Missing kind refuses.
        assert.equal(parseWorldReply(JSON.stringify(makeBaseReply({
            operationSettled: [{ win: "win-a", op: "op-1", outcome: "continued-tiled", cause: "" }],
        }))), null);
    });

    it("validates switchSettled exact keys and out uniqueness", () => {
        // Distinct out with same op is valid.
        const multiOut = parseWorldReply(JSON.stringify(makeBaseReply({
            switchSettled: [
                { out: "out-a", op: "op-1", outcome: "complete", failed: [], focus: "proven", fallback: false },
                { out: "out-b", op: "op-1", outcome: "completed-with-failures", failed: ["win-x"], focus: "unproven", fallback: false },
            ],
        })));
        assert.ok(isCoreWorldReply(multiOut));
        assert.equal(multiOut.switchSettled.length, 2);

        // Duplicate out refuses.
        assert.equal(parseWorldReply(JSON.stringify(makeBaseReply({
            switchSettled: [
                { out: "out-a", op: "op-1", outcome: "complete", failed: [], focus: "proven", fallback: false },
                { out: "out-a", op: "op-2", outcome: "complete", failed: [], focus: "proven", fallback: false },
            ],
        }))), null);

        // Missing out refuses.
        assert.equal(parseWorldReply(JSON.stringify(makeBaseReply({
            switchSettled: [{ op: "op-1", outcome: "complete", failed: [], focus: "proven", fallback: false }],
        }))), null);
    });
});

describe("KDE retained settlement discharge & retry isolation", () => {
    it("matches current pending FIFO head on stale-snapshot ack-only path and ignores redelivery", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted();
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            // Terminal arrives on WaitGesture.
            h.popWaitGesture().callback(gestureJson({ phase: "released", serial: "7", win: "win-a" }));
            h.runTimers();
            const issuing = h.popDescribe();
            // Issuing reply returns WITHOUT gestureSettled (withheld/not yet settled).
            issuing.call.callback(JSON.stringify(makeBaseReply({
                adapter: issuing.body.adapter, req: issuing.body.source, obs: "2", rev: "2",
                source: issuing.body.source, nativeInvalid: issuing.body.nativeInvalid,
                gestureSettled: null, gestureAssoc: { instance: "gesture-1", serial: "7", coreId: "3" },
                gestureSettledAssoc: null,
            })));
            assert.equal(h.ackCalls().length, 0, "no ack before settlement");

            // Fire the 2000ms retry timer and deliver a stale rev ("1" <= "2") reply carrying the retained terminal.
            h.runTimers(2000);
            const retry = h.popDescribe();
            // Deliver a STALE rev ("1" <= "2") reply carrying the retained terminal!
            retry.call.callback(JSON.stringify(makeBaseReply({
                adapter: retry.body.adapter, req: retry.body.source, obs: "3", rev: "1",
                source: retry.body.source, nativeInvalid: retry.body.nativeInvalid,
                gestureSettled: { id: "3", committed: true },
                gestureAssoc: null,
                gestureSettledAssoc: { instance: "gesture-1", serial: "7", coreId: "3" },
                operationSettled: [
                    { win: "win-a", kind: "clear", op: "op-1", outcome: "continued-tiled", cause: "" },
                    { win: "win-a", kind: "float", op: "op-1", outcome: "float-frame-proven", cause: "" },
                ],
                switchSettled: [
                    { out: "out-a", op: "op-2", outcome: "complete", failed: [], focus: "proven", fallback: false },
                ],
            })));
            assert.equal(h.ackCalls().length, 1, "stale-snapshot ack-only path discharges pending FIFO head");
            assert.equal(h.geometryWrites.length, 0, "operationSettled/switchSettled and stale reply cause zero writes");

            // Complete AckGesture with idle -> retires gestureTerminalPending.
            const ack = h.ackCalls()[0]!;
            assert.equal(JSON.parse(ack.payload).serial, "7");
            ack.callback(idleGestureJson("10"));
            h.runTimers();

            // Redelivery of the same gestureSettled + gestureSettledAssoc after retirement is inert.
            const after = h.popDescribe();
            after.call.callback(JSON.stringify(makeBaseReply({
                adapter: after.body.adapter, req: after.body.source, obs: "4", rev: "4",
                source: after.body.source, nativeInvalid: after.body.nativeInvalid,
                gestureSettled: { id: "3", committed: true },
                gestureAssoc: null,
                gestureSettledAssoc: { instance: "gesture-1", serial: "7", coreId: "3" },
            })));
            assert.equal(h.ackCalls().length, 1, "redelivery after retirement is inert");
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=settlement-receipt gen=gen-1 req=${init.body.source} rev=1 obs=1 ops=0 switches=0 gesture=none`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=settlement-applied gen=gen-1 req=${init.body.source} rev=1 obs=1 ops=0 switches=0 gesture=none`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=${issuing.body.source} rev=2 obs=2 decision=no-settled reason=no-settled settledId=- assocInstance=- assocSerial=- assocCoreId=- pendingInstance=gesture-1 pendingSerial=7 pendingWin=win-a flightGesture=gesture-1:7:win-a`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=settlement-receipt gen=gen-1 req=${retry.body.source} rev=1 obs=3 ops=2 switches=1 gesture=present`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=settlement-gesture phase=receipt gen=gen-1 req=${retry.body.source} rev=1 obs=3 id=3 committed=true assocInstance=gesture-1 assocSerial=7 assocCoreId=3`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=settlement-operation phase=receipt gen=gen-1 req=${retry.body.source} rev=1 obs=3 win=win-a kind=clear op=op-1 outcome=continued-tiled cause=-`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=settlement-operation phase=receipt gen=gen-1 req=${retry.body.source} rev=1 obs=3 win=win-a kind=float op=op-1 outcome=float-frame-proven cause=-`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=settlement-switch phase=receipt gen=gen-1 req=${retry.body.source} rev=1 obs=3 out=out-a op=op-2 outcome=complete focus=proven fallback=false failed=-`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=${retry.body.source} rev=1 obs=3 decision=ack-armed reason=matched settledId=3 assocInstance=gesture-1 assocSerial=7 assocCoreId=3 pendingInstance=gesture-1 pendingSerial=7 pendingWin=win-a flightGesture=gesture-1:7:win-a`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=reply-skipped stage=accept gen=gen-1 req=${retry.body.source} rev=1 obs=3 reason=stale-rev`,
            ));
            assert.ok(!h.logs.includes(
                `omnitiler:world:trace ev=settlement-applied gen=gen-1 req=${retry.body.source} rev=1 obs=3 ops=2 switches=1 gesture=present`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-ack-response gen=gen-1 req=${retry.body.source} rev=1 obs=3 instance=gesture-1 serial=7 coreId=3 status=retired reason=idle nextPhase=idle nextInstance=gesture-1 nextSerial=0`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=${after.body.source} rev=4 obs=4 decision=inert-already-retired reason=already-retired settledId=3 assocInstance=gesture-1 assocSerial=7 assocCoreId=3 pendingInstance=- pendingSerial=- pendingWin=- flightGesture=null`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=settlement-applied gen=gen-1 req=${after.body.source} rev=4 obs=4 ops=0 switches=0 gesture=present`,
            ));
        } finally {
            h.handle.stop();
        }
    });

    it("unrelated non-discharging capture does NOT clear earlier unresolved terminalCaptureRetry", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted();
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            // Terminal capture T1 (serial 1) is sent and times out -> arms terminalCaptureRetry.
            h.popWaitGesture().callback(gestureJson({ phase: "released", serial: "1", win: "win-a" }));
            h.runTimers();
            const t1 = h.popDescribe();
            const t1Source = t1.body.source;
            // Time out T1 (do not invoke t1.call.callback) -> arms terminalCaptureRetry.
            h.runTimers(2000);
            assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, 0, "retry timer armed, not fired yet");

            // Fire an unrelated non-discharging capture while terminalCaptureRetry is armed,
            // and answer it with gestureSettled: null. terminalCaptureRetry must NOT be cleared.
            h.fireWindowActivated();
            h.runTimers();
            const intervening = h.popDescribe();
            assert.notEqual(intervening.body.source, t1Source, "fresh capture has distinct source");
            intervening.call.callback(JSON.stringify(makeBaseReply({
                adapter: intervening.body.adapter, req: intervening.body.source, obs: "2", rev: "2",
                source: intervening.body.source, nativeInvalid: intervening.body.nativeInvalid,
                gestureSettled: null, gestureSettledAssoc: null,
            })));
            assert.equal(h.ackCalls().length, 0, "no ack from non-discharging capture");

            // Advance 2000ms -> T1's exact envelope retries!
            h.runTimers(2000);
            const retry = h.popDescribe();
            assert.equal(retry.body.source, t1Source, "terminalCaptureRetry fires with T1 exact source");
        } finally {
            h.handle.stop();
        }
    });

    it("refuses ack on wrong serial, null gestureSettledAssoc, or mismatched coreId", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted();
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            h.popWaitGesture().callback(gestureJson({ phase: "released", serial: "5", win: "win-a" }));
            h.runTimers();
            const cap = h.popDescribe();

            // Wrong serial in gestureSettledAssoc -> no ack.
            cap.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap.body.adapter, req: cap.body.source, obs: "2", rev: "2",
                source: cap.body.source, nativeInvalid: cap.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "4", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 0);

            // Missing gestureSettledAssoc (null) -> no ack.
            h.runTimers(2000);
            const cap2 = h.popDescribe();
            cap2.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap2.body.adapter, req: cap2.body.source, obs: "3", rev: "3",
                source: cap2.body.source, nativeInvalid: cap2.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: null,
            })));
            assert.equal(h.ackCalls().length, 0);

            // Mismatched coreId (settled.id !== gestureSettledAssoc.coreId) -> no ack.
            h.runTimers(2000);
            const cap3 = h.popDescribe();
            cap3.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap3.body.adapter, req: cap3.body.source, obs: "4", rev: "4",
                source: cap3.body.source, nativeInvalid: cap3.body.nativeInvalid,
                gestureSettled: { id: "2", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "5", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 0);
        } finally {
            h.handle.stop();
        }
    });
});

describe("genuine broker_world_eval -> production entry retained gesture journeys", () => {
    async function makeBrokerEntry() {
        const bridge = WorldBridge.start();
        const timers: Array<{ delay: number; cb: () => void; cancelled: boolean }> = [];
        const gestureCalls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const logs: string[] = [];
        const desktop1 = { id: "ws1", name: "ws1" };
        const desktop2 = { id: "ws2", name: "ws2" };
        const out = { name: "out-a", geometry: { x: 0, y: 0, width: 1920, height: 1080 } };
        const makeWin = (id: string, rect: { x: number; y: number; width: number; height: number }) => {
            let liveRect = { ...rect };
            const w = {
                internalId: id,
                managed: true,
                deleted: false,
                transient: false,
                transientFor: null,
                dialog: false,
                modal: false,
                wantsInput: true,
                normalWindow: true,
                output: out,
                desktops: [desktop1],
                minimized: false,
                hidden: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
                minSize: { width: 100, height: 100 },
                maxSize: { width: 2147483647, height: 2147483647 },
                pid: 1234,
                resourceClass: "Example",
                caption: id,
                windowRole: "normal",
                desktopFileName: "example.app",
                resizeable: true,
                keepAbove: false,
                keepBelow: false,
                outputChanged: { connect: () => undefined, disconnect: () => undefined },
                desktopsChanged: { connect: () => undefined, disconnect: () => undefined },
                hiddenChanged: { connect: () => undefined, disconnect: () => undefined },
                frameGeometryChanged: { connect: () => undefined, disconnect: () => undefined },
            } as Record<string, unknown>;
            Object.defineProperty(w, "frameGeometry", {
                configurable: true,
                get: () => liveRect,
                set: (r: { x: number; y: number; width: number; height: number }) => {
                    liveRect = { ...r };
                    return true;
                },
            });
            Object.defineProperty(w, "clientGeometry", {
                configurable: true,
                get: () => liveRect,
            });
            return w;
        };
        const wins = [
            makeWin("win-a", { x: 0, y: 0, width: 960, height: 1080 }),
            makeWin("win-b", { x: 960, y: 0, width: 960, height: 1080 }),
        ];
        const removedListeners: Array<(w: unknown) => void> = [];
        const activatedListeners: Array<(w: unknown) => void> = [];
        const workspace = {
            windowList: () => wins,
            screens: [out],
            desktops: [desktop1, desktop2],
            currentDesktop: desktop1,
            activeScreen: out,
            cursorPos: { x: 20, y: 20 },
            currentDesktopForScreen: () => desktop1,
            activeWindow: wins[0],
            clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1080 }),
            windowAdded: { connect: () => undefined, disconnect: () => undefined },
            windowRemoved: {
                connect: (fn: (w: unknown) => void) => {
                    removedListeners.push(fn);
                },
                disconnect: (fn: (w: unknown) => void) => {
                    const idx = removedListeners.indexOf(fn);
                    if (idx >= 0) {
                        removedListeners.splice(idx, 1);
                    }
                },
            },
            currentDesktopChanged: { connect: () => undefined, disconnect: () => undefined },
            desktopsChanged: { connect: () => undefined, disconnect: () => undefined },
            screensChanged: { connect: () => undefined, disconnect: () => undefined },
            windowActivated: {
                connect: (fn: (w: unknown) => void) => {
                    activatedListeners.push(fn);
                },
                disconnect: (fn: (w: unknown) => void) => {
                    const idx = activatedListeners.indexOf(fn);
                    if (idx >= 0) {
                        activatedListeners.splice(idx, 1);
                    }
                },
            },
        };
        const removeWin = (id: string): void => {
            const idx = wins.findIndex((w) => w["internalId"] === id);
            assert.ok(idx >= 0, `window ${id} exists`);
            const removed = wins.splice(idx, 1)[0]!;
            for (const fn of [...removedListeners]) {
                fn(removed);
            }
        };
        const activateWin = (id: string): void => {
            const target = wins.find((w) => w["internalId"] === id);
            assert.ok(target !== undefined, `window ${id} exists`);
            workspace.activeWindow = target;
            for (const fn of [...activatedListeners]) {
                fn(target);
            }
        };
        let holdNextDescribe = false;
        let omitNextNativeGesture = false;
        let queueCommandCount = 0;
        let lastOriginalDescribeNativeGesture: unknown = null;
        let lastNativeInvalid = "0";
        let lastDescribePayload = "";
        let lastDescribeReply = "";
        const heldReplies: Array<{ replyText: string; callback: (reply: unknown) => void }> = [];
        const inflight: Promise<void>[] = [];
        const handle = startWorldAdapterEntry({
            options: { perOutputVirtualDesktops: true },
            workspace: workspace as never,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_s, _p, _i, method, payload, callback) => {
                if (method === "QueueWorldCommand") {
                    queueCommandCount += 1;
                }
                let outgoing = payload;
                if (method === "DescribeWorld") {
                    const origEnv = JSON.parse(payload) as { report: Record<string, unknown> };
                    lastOriginalDescribeNativeGesture = origEnv.report["nativeGesture"] ?? null;
                    if (omitNextNativeGesture) {
                        omitNextNativeGesture = false;
                        origEnv.report["nativeGesture"] = null;
                        origEnv.report["gesture"] = false;
                        outgoing = JSON.stringify(origEnv);
                    }
                    lastDescribePayload = outgoing;
                    const env = JSON.parse(outgoing) as { nativeInvalid?: string };
                    if (typeof env.nativeInvalid === "string") {
                        lastNativeInvalid = env.nativeInvalid;
                    }
                }
                const p = bridge.send(method, outgoing).then((replyText) => {
                    if (method === "DescribeWorld") {
                        lastDescribeReply = replyText;
                    }
                    if (method === "DescribeWorld" && holdNextDescribe) {
                        heldReplies.push({ replyText, callback });
                    } else {
                        callback(replyText);
                    }
                });
                inflight.push(p);
            },
            gestureCallDbus: (_s, _p, _i, method, payload, callback) => {
                gestureCalls.push({ method, payload, callback });
            },
            scheduleOnce: (delay, cb) => {
                const entry = { delay, cb, cancelled: false };
                timers.push(entry);
                return () => {
                    entry.cancelled = true;
                };
            },
            log: (line) => {
                logs.push(line);
            },
        });
        if (handle === null) {
            await bridge.close();
            assert.fail("startWorldAdapterEntry returned null");
        }

        const flush = async (): Promise<void> => {
            while (inflight.length > 0) {
                const batch = inflight.splice(0, inflight.length);
                await Promise.all(batch);
            }
        };
        const runTimers = (filterDelay?: number): void => {
            const len = timers.length;
            for (let i = 0; i < len; i += 1) {
                const t = timers[i]!;
                if (t.cancelled) {
                    continue;
                }
                if (filterDelay === undefined ? t.delay <= 120 : t.delay === filterDelay) {
                    t.cancelled = true;
                    t.cb();
                }
            }
        };
        const runNextLongestTimer = (): void => {
            const active = timers.filter((t) => !t.cancelled);
            assert.ok(active.length > 0, "expected a pending timer");
            const chosen = active.reduce((a, b) => (b.delay >= a.delay ? b : a));
            chosen.cancelled = true;
            chosen.cb();
        };
        const settle = async (): Promise<void> => {
            for (let i = 0; i < 6; i += 1) {
                runTimers();
                await flush();
            }
        };
        const runNewestTimer = (filterDelay: number): void => {
            for (let i = timers.length - 1; i >= 0; i -= 1) {
                const t = timers[i]!;
                if (!t.cancelled && t.delay === filterDelay) {
                    t.cancelled = true;
                    t.cb();
                    return;
                }
            }
            assert.fail(`expected active timer with delay ${filterDelay}`);
        };
        const setHoldDescribe = (hold: boolean): void => {
            holdNextDescribe = hold;
        };
        const setOmitNextNativeGesture = (omit: boolean): void => {
            omitNextNativeGesture = omit;
        };
        const popWaitGesture = () => {
            const idx = gestureCalls.findIndex((c) => c.method === "WaitGesture");
            assert.ok(idx >= 0, "WaitGesture pending");
            return gestureCalls.splice(idx, 1)[0]!;
        };
        const ackCalls = () => gestureCalls.filter((c) => c.method === "AckGesture");
        const stop = async (): Promise<void> => {
            handle.stop();
            await bridge.close();
        };

        return { bridge, wins, removeWin, activateWin, workspace, handle, timers, gestureCalls, logs, heldReplies, queueCommandCount: () => queueCommandCount, lastOriginalDescribeNativeGesture: () => lastOriginalDescribeNativeGesture, lastNativeInvalid: () => lastNativeInvalid, lastDescribePayload: () => lastDescribePayload, lastDescribeReply: () => lastDescribeReply, flush, runTimers, runNextLongestTimer, runNewestTimer, settle, setHoldDescribe, setOmitNextNativeGesture, popWaitGesture, ackCalls, stop };
    }

    it("withholds issuing + 2 intermediates, first delivers 4th retry reply, retires once, and ignores redelivery", { timeout: 60000 }, async () => {
        const h = await makeBrokerEntry();
        try {
            await h.settle();
            assert.equal(h.ackCalls().length, 0);

            // Start gesture on win-a (tiled at [964, 8, 948, 1064]), move toward win-b's center ([400, 500]), then release.
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "1", phase: "started", win: "win-a",
                startRect: [964, 8, 948, 1064], startCursor: [1400, 500], cursor: [1400, 500],
            }));
            await h.settle();

            h.wins[0]!["frameGeometry"] = { x: 8, y: 8, width: 948, height: 1064 };
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "2", phase: "updated", win: "win-a",
                startRect: [964, 8, 948, 1064], startCursor: [1400, 500], cursor: [50, 500],
            }));
            await h.settle();

            // Hold DescribeWorld replies for the issuing terminal and 2 intermediates!
            h.setHoldDescribe(true);
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "3", phase: "released", win: "win-a",
                startRect: [964, 8, 948, 1064], startCursor: [1400, 500], cursor: [50, 500],
            }));
            h.runTimers();
            await h.flush();
            assert.equal(h.heldReplies.length, 1, "issuing terminal reply withheld from entry");
            const issuingParsed = parseWorldReply(h.heldReplies[0]!.replyText);
            assert.ok(isCoreWorldReply(issuingParsed) && issuingParsed.gestureSettled !== null);
            assert.equal(issuingParsed.gestureSettled.committed, true, `issuing reply committed: ${JSON.stringify(issuingParsed.gestureSettled)} logs=${JSON.stringify(h.logs.slice(-15))}`);

            // Time out the issuing capture and withhold 2 intermediate retries from entry;
            // verify the broker retains gestureSettled + gestureSettledAssoc across all of them.
            for (let i = 1; i <= 2; i += 1) {
                h.runTimers(2000); // timeout current flight -> arm retry
                h.runTimers(2000); // fire retry -> held in heldReplies
                await h.flush();
                assert.equal(h.heldReplies.length, 1 + i, `intermediate ${i} withheld from entry`);
                const interReply = parseWorldReply(h.heldReplies[i]!.replyText);
                assert.ok(isCoreWorldReply(interReply));
                assert.deepEqual(interReply.gestureSettled, issuingParsed.gestureSettled, `intermediate ${i} retains gestureSettled`);
                assert.deepEqual(interReply.gestureSettledAssoc, issuingParsed.gestureSettledAssoc, `intermediate ${i} retains gestureSettledAssoc`);
            }
            assert.equal(h.ackCalls().length, 0, "entry has not acked while issuing + 2 intermediates withheld");

            // Deliver ONLY the 4th reply (never delivering issuing or intermediates 1..2).
            h.setHoldDescribe(false);
            h.runTimers(2000); // timeout intermediate 2 -> escalates to host-deadline timer
            h.runNextLongestTimer(); // fire 4th capture -> delivered to entry!
            await h.flush();
            assert.equal(h.ackCalls().length, 1, "one exact AckGesture retirement on first delivered reply");
            h.ackCalls()[0]!.callback(idleGestureJson("10"));
            await h.settle();
            assert.equal(h.ackCalls().length, 1, "redelivery after retirement is inert");
            // Redelivering a withheld earlier reply after retirement is also inert.
            h.heldReplies[0]!.callback(h.heldReplies[0]!.replyText);
            assert.equal(h.ackCalls().length, 1, "late redelivery of withheld issuing reply is inert");
        } finally {
            await h.stop();
        }
    });

    it("withholds issuing + 2 genuinely fresh intermediates (distinct source/obs/rev), first delivers fresh commandless omitted-ingress reply, retires once, and ignores late redelivery", { timeout: 60000 }, async () => {
        const h = await makeBrokerEntry();
        try {
            await h.settle();
            assert.equal(h.ackCalls().length, 0, "precondition: no ack before gesture");

            // Start and update gesture on win-a; verify pre-release state.
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "1", phase: "started", win: "win-a",
                startRect: [964, 8, 948, 1064], startCursor: [1400, 500], cursor: [1400, 500],
            }));
            await h.settle();

            h.wins[0]!["frameGeometry"] = { x: 8, y: 8, width: 948, height: 1064 };
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "2", phase: "updated", win: "win-a",
                startRect: [964, 8, 948, 1064], startCursor: [1400, 500], cursor: [50, 500],
            }));
            await h.settle();

            const preReleaseReply = parseWorldReply(h.lastDescribeReply());
            assert.ok(isCoreWorldReply(preReleaseReply), "precondition: pre-release reply is core reply");
            assert.equal(preReleaseReply.gestureSettled, null, "precondition: no gestureSettled before release");
            assert.equal(preReleaseReply.gestureSettledAssoc, null, "precondition: no gestureSettledAssoc before release");
            assert.ok(
                preReleaseReply.gestureAssoc !== null &&
                preReleaseReply.gestureAssoc.instance === "g-1" &&
                preReleaseReply.gestureAssoc.serial === "1",
                "precondition: active gestureAssoc bound before release",
            );
            assert.equal(h.ackCalls().length, 0, "precondition: no ack while gesture active");

            // Hold DescribeWorld replies for issuing terminal and 2 genuinely fresh intermediates.
            h.setHoldDescribe(true);
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "3", phase: "released", win: "win-a",
                startRect: [964, 8, 948, 1064], startCursor: [1400, 500], cursor: [50, 500],
            }));
            h.runTimers();
            await h.flush();
            assert.equal(h.heldReplies.length, 1, "issuing terminal reply withheld from entry");
            const issuingParsed = parseWorldReply(h.heldReplies[0]!.replyText);
            assert.ok(isCoreWorldReply(issuingParsed) && issuingParsed.gestureSettled !== null);
            assert.equal(issuingParsed.gestureSettled.committed, true, "issuing reply committed=true");
            assert.deepEqual(
                issuingParsed.gestureSettledAssoc,
                { instance: "g-1", serial: "1", coreId: issuingParsed.gestureSettled.id },
                "issuing reply binds gestureSettledAssoc",
            );
            assert.deepEqual(issuingParsed.gestureAssoc, issuingParsed.gestureSettledAssoc, "issuing reply carries gestureAssoc");
            assert.equal(compareU64(issuingParsed.obs, preReleaseReply.obs), 1, "issuing obs advances");
            assert.equal(compareU64(issuingParsed.rev, preReleaseReply.rev), 1, "issuing rev advances on core mutation");
            const allocProjection = (windows: ReadonlyArray<{ win: string; out: string; ws: string; alloc: unknown }>) =>
                windows.map((w) => ({ win: w.win, out: w.out, ws: w.ws, alloc: w.alloc })).sort((a, b) => a.win.localeCompare(b.win));
            const issuingAlloc = allocProjection(issuingParsed.windows);
            assert.equal(issuingAlloc.length, 2, "precondition: both windows in issuing allocation projection");

            // Time out the issuing capture flight, then trigger 2 genuinely fresh intermediate
            // captures via distinct native windowActivated events (win-b, then win-a),
            // withholding both from entry.
            h.runNewestTimer(2000);
            const seenSources = new Set<string>([issuingParsed.source]);
            let prevObs = issuingParsed.obs;
            let prevRev = issuingParsed.rev;
            for (const [idx, targetWin] of (["win-b", "win-a"] as const).entries()) {
                h.activateWin(targetWin);
                h.runTimers();
                await h.flush();
                assert.equal(h.heldReplies.length, idx + 2, `fresh intermediate ${idx + 1} withheld from entry`);
                const interParsed = parseWorldReply(h.heldReplies[idx + 1]!.replyText);
                assert.ok(isCoreWorldReply(interParsed));
                assert.ok(!seenSources.has(interParsed.source), `intermediate ${idx + 1} has distinct source`);
                seenSources.add(interParsed.source);
                assert.equal(compareU64(interParsed.obs, prevObs), 1, `intermediate ${idx + 1} obs strictly increases`);
                assert.equal(compareU64(interParsed.rev, prevRev), 1, `intermediate ${idx + 1} rev strictly increases`);
                prevObs = interParsed.obs;
                prevRev = interParsed.rev;
                assert.deepEqual(interParsed.gestureSettled, issuingParsed.gestureSettled, `intermediate ${idx + 1} retains gestureSettled`);
                assert.deepEqual(interParsed.gestureSettledAssoc, issuingParsed.gestureSettledAssoc, `intermediate ${idx + 1} retains gestureSettledAssoc`);
                assert.deepEqual(allocProjection(interParsed.windows), issuingAlloc, `intermediate ${idx + 1} preserves committed core window allocation projection`);
                // Time out only this intermediate's transmitCapture timer so inFlight clears.
                h.runNewestTimer(2000);
            }
            assert.equal(h.ackCalls().length, 0, "entry has not acked while issuing + 2 fresh intermediates withheld");

            // First delivered terminal reply: fresh commandless omitted-ingress transport seam
            // (not a true null-flight in world-entry: transport omits nativeGesture on the outgoing
            // DescribeWorld so the genuine broker produces gestureAssoc: null while entry still
            // holds gestureTerminalPending and a non-null terminal flight).
            h.setHoldDescribe(false);
            h.setOmitNextNativeGesture(true);
            h.activateWin("win-b");
            h.runTimers();
            await h.flush();

            const origFinalNativeGesture = h.lastOriginalDescribeNativeGesture() as { phase?: unknown; instance?: unknown; serial?: unknown } | null;
            assert.ok(origFinalNativeGesture !== null, "entry-generated capture still carries non-null terminal nativeGesture before transport omission");
            assert.equal(origFinalNativeGesture.phase, "released", "entry-generated nativeGesture phase is released");
            assert.equal(origFinalNativeGesture.instance, "g-1", "entry-generated nativeGesture instance is g-1");
            assert.equal(origFinalNativeGesture.serial, "1", "entry-generated nativeGesture serial is 1");

            const finalSent = JSON.parse(h.lastDescribePayload()) as { report: Record<string, unknown> };
            assert.equal(finalSent.report["nativeGesture"], null, "final outgoing capture has transport-omitted nativeGesture");
            assert.equal(finalSent.report["gesture"], false, "final outgoing capture has gesture=false");
            assert.equal(h.queueCommandCount(), 0, "no QueueWorldCommand issued during commandless journey");

            const finalParsed = parseWorldReply(h.lastDescribeReply());
            assert.ok(isCoreWorldReply(finalParsed));
            assert.ok(!seenSources.has(finalParsed.source), "final delivered reply has distinct source");
            assert.equal(compareU64(finalParsed.obs, prevObs), 1, "final delivered reply obs strictly increases");
            assert.equal(compareU64(finalParsed.rev, prevRev), 1, "final delivered reply rev strictly increases");
            assert.equal(finalParsed.gestureAssoc, null, "final delivered reply has gestureAssoc=null (omitted ingress)");
            assert.deepEqual(finalParsed.gestureSettled, issuingParsed.gestureSettled, "final delivered reply retains exact gestureSettled");
            assert.deepEqual(finalParsed.gestureSettledAssoc, issuingParsed.gestureSettledAssoc, "final delivered reply retains exact gestureSettledAssoc");
            assert.deepEqual(allocProjection(finalParsed.windows), issuingAlloc, "final delivered reply preserves committed core window allocation projection (no gesture mutation rerun)");

            // Actual pending native FIFO head retires once.
            assert.equal(h.ackCalls().length, 1, "one exact AckGesture retirement on first delivered omitted-ingress reply");
            const ackPayload = JSON.parse(h.ackCalls()[0]!.payload) as { instance: string; serial: string };
            assert.equal(ackPayload.instance, "g-1");
            assert.equal(ackPayload.serial, "1");

            h.ackCalls()[0]!.callback(idleGestureJson("10"));
            await h.settle();
            assert.equal(h.ackCalls().length, 1, "subsequent idle settlement is inert after retirement");

            // Late redelivery of withheld issuing and intermediate replies is inert.
            h.heldReplies[0]!.callback(h.heldReplies[0]!.replyText);
            h.heldReplies[1]!.callback(h.heldReplies[1]!.replyText);
            assert.equal(h.ackCalls().length, 1, "late redelivery of withheld replies is inert");
        } finally {
            await h.stop();
        }
    });

    it("256 eviction close end-to-end: evicts 256 normalization mappings while G1 active, then closes win-a with G1 cancelled and retires G1", { timeout: 60000 }, async () => {
        const h = await makeBrokerEntry();
        try {
            await h.settle();
            // Open gesture G1 on win-a.
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "1", phase: "started", win: "win-a",
                startRect: [0, 0, 960, 1080], startCursor: [100, 100], cursor: [120, 120],
            }));
            await h.settle();
            assert.equal(h.ackCalls().length, 0, "open gesture does not ack");

            // Capture G1's initial gestureAssoc from an updated snapshot while active.
            h.setHoldDescribe(true);
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "2", phase: "updated", win: "win-a",
                startRect: [0, 0, 960, 1080], startCursor: [100, 100], cursor: [130, 130],
            }));
            h.runTimers();
            await h.flush();
            const held = h.heldReplies.shift()!;
            const probeParsed = parseWorldReply(held.replyText);
            assert.ok(isCoreWorldReply(probeParsed));
            const g1Assoc = probeParsed.gestureAssoc;
            assert.ok(g1Assoc !== null && g1Assoc.instance === "g-1" && g1Assoc.serial === "1");
            h.setHoldDescribe(false);
            held.callback(held.replyText);
            await h.settle();

            // Send 256 unbindable newcomers through the production entry -> genuine broker
            // (distinct serials "2".."257" on a different instance "g-evict"),
            // evicting G1 from the 256-entry normalization map while G1 stays active.
            for (let i = 2; i <= 257; i += 1) {
                h.popWaitGesture().callback(gestureJson({
                    instance: "g-evict", serial: String(i), revision: String(i), phase: "started", win: "win-missing",
                    startRect: [8, 8, 948, 1064], startCursor: [400, 500], cursor: [410, 500],
                }));
                await h.settle();
            }
            assert.equal(h.ackCalls().length, 0, "G1 still active during eviction");

            // Remove win-a and deliver cancelled for G1 on WaitGesture: the broker's
            // pinned active_assoc resolves G1's gestureSettledAssoc and entry retires G1 once.
            h.removeWin("win-a"); // win-a departs with explicit windowRemoved signal
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "3", phase: "cancelled", win: "win-a",
                startRect: [0, 0, 960, 1080], startCursor: [100, 100], cursor: [130, 130],
            }));
            await h.settle();
            assert.equal(h.ackCalls().length, 1, `G1 retires after 256-entry eviction; lastReply=${h.lastDescribeReply()}`);
            const ackPayload = JSON.parse(h.ackCalls()[0]!.payload) as { instance: string; serial: string };
            assert.equal(ackPayload.instance, "g-1");
            assert.equal(ackPayload.serial, "1");
            h.ackCalls()[0]!.callback(idleGestureJson("20"));
            await h.settle();
            assert.equal(h.ackCalls().length, 1, "no duplicate ack after retirement");
        } finally {
            await h.stop();
        }
    });

    it("settles cancelled gesture as committed=false through genuine broker and retires once", { timeout: 60000 }, async () => {
        const h = await makeBrokerEntry();
        try {
            await h.settle();
            // Start gesture on win-a, then cancel without moving to another cell.
            h.popWaitGesture().callback(gestureJson({
                instance: "g-cancel", serial: "1", revision: "1", phase: "started", win: "win-a",
                startRect: [0, 0, 960, 1080], startCursor: [100, 100], cursor: [100, 100],
            }));
            await h.settle();

            h.setHoldDescribe(true);
            h.popWaitGesture().callback(gestureJson({
                instance: "g-cancel", serial: "1", revision: "2", phase: "cancelled", win: "win-a",
                startRect: [0, 0, 960, 1080], startCursor: [100, 100], cursor: [100, 100],
            }));
            h.runTimers();
            await h.flush();
            assert.equal(h.heldReplies.length, 1);
            const held = h.heldReplies.shift()!;
            const parsed = parseWorldReply(held.replyText);
            assert.ok(isCoreWorldReply(parsed));
            assert.ok(parsed.gestureSettled !== null);
            assert.equal(parsed.gestureSettled.committed, false, "cancelled gesture settles committed=false");
            assert.deepEqual(parsed.gestureSettledAssoc, { instance: "g-cancel", serial: "1", coreId: parsed.gestureSettled.id });

            h.setHoldDescribe(false);
            held.callback(held.replyText);
            assert.equal(h.ackCalls().length, 1, "cancelled terminal retires once");
            h.ackCalls()[0]!.callback(idleGestureJson("3"));
            await h.settle();
            assert.equal(h.ackCalls().length, 1, "inert after retirement");
        } finally {
            await h.stop();
        }
    });

    it("256 eviction close with omitted broker ingress: settles old active G1 from closed=win-a, resolves pinned assoc, and retires entry pending head once", { timeout: 60000 }, async () => {
        const h = await makeBrokerEntry();
        try {
            await h.settle();
            assert.equal(h.ackCalls().length, 0, "precondition: no ack before gesture");

            // Open gesture G1 on win-a through production entry -> genuine broker.
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "1", phase: "started", win: "win-a",
                startRect: [0, 0, 960, 1080], startCursor: [100, 100], cursor: [120, 120],
            }));
            await h.settle();
            assert.equal(h.ackCalls().length, 0, "open gesture does not ack");

            // Save initial G1 association from genuine broker while G1 is active.
            const g1OpenParsed = parseWorldReply(h.lastDescribeReply());
            assert.ok(isCoreWorldReply(g1OpenParsed), "G1 open reply is core reply");
            assert.equal(g1OpenParsed.gestureSettled, null, "no settlement on open");
            assert.equal(g1OpenParsed.gestureSettledAssoc, null, "no settled assoc on open");
            const g1Assoc = g1OpenParsed.gestureAssoc;
            assert.ok(
                g1Assoc !== null && g1Assoc.instance === "g-1" && g1Assoc.serial === "1" && typeof g1Assoc.coreId === "string" && g1Assoc.coreId.length > 0,
                "initial G1 assoc bound by genuine broker",
            );

            // Send 256 unbindable newcomers ("g-evict", serials "2".."257", started on "win-missing")
            // through production entry -> genuine broker, evicting G1 from the 256-entry
            // normalization map while old core G1 stays active.
            const seenSources = new Set<string>([g1OpenParsed.source]);
            let prevObs = g1OpenParsed.obs;
            for (let i = 2; i <= 257; i += 1) {
                h.popWaitGesture().callback(gestureJson({
                    instance: "g-evict", serial: String(i), revision: String(i), phase: "started", win: "win-missing",
                    startRect: [8, 8, 948, 1064], startCursor: [400, 500], cursor: [410, 500],
                }));
                await h.settle();
                const newcomerParsed = parseWorldReply(h.lastDescribeReply());
                assert.ok(isCoreWorldReply(newcomerParsed), `newcomer ${i} accepted by genuine broker`);
                assert.ok(!seenSources.has(newcomerParsed.source), `newcomer ${i} has distinct fresh source`);
                seenSources.add(newcomerParsed.source);
                assert.equal(compareU64(newcomerParsed.obs, prevObs), 1, `newcomer ${i} obs strictly increases`);
                prevObs = newcomerParsed.obs;
                assert.equal(newcomerParsed.gestureSettled, null, `newcomer ${i} causes no terminal replacement`);
                assert.equal(newcomerParsed.gestureSettledAssoc, null, `newcomer ${i} produces no gestureSettledAssoc`);
            }
            assert.equal(h.ackCalls().length, 0, "G1 still active and unacked after 256 eviction newcomers");

            // Hold the genuine broker close reply to inspect all preconditions before delivering once.
            // Native pending head is supplied via the WaitGesture released seam ONLY to establish
            // entry's pending FIFO head (gestureTerminalPending), NOT as a core terminal cancellation.
            // Outgoing transport omission (setOmitNextNativeGesture(true)) is arranged BEFORE any
            // capture so the broker only sees omitted-ingress close (nativeGesture: null, gesture: false,
            // closed: ["win-a"]). Note: setOmitNextNativeGesture(true) is an outgoing transport omission
            // seam; it does not make entry's flight.gesture null (source invariants prevent an owed
            // pending terminal with a null flight via public seams).
            h.setHoldDescribe(true);
            h.setOmitNextNativeGesture(true);
            h.popWaitGesture().callback(gestureJson({
                instance: "g-1", serial: "1", revision: "3", phase: "released", win: "win-a",
                startRect: [0, 0, 960, 1080], startCursor: [100, 100], cursor: [130, 130],
            }));
            h.removeWin("win-a");
            h.runTimers();
            await h.flush();

            // Preconditions while genuine broker close reply is withheld from entry.
            const origNative = h.lastOriginalDescribeNativeGesture() as { instance?: unknown; serial?: unknown; phase?: unknown } | null;
            assert.ok(origNative !== null, "entry generated pending terminal nativeGesture before transport omission");
            assert.equal(origNative.instance, "g-1");
            assert.equal(origNative.serial, "1");
            assert.equal(origNative.phase, "released");

            const closeSent = JSON.parse(h.lastDescribePayload()) as { report: Record<string, unknown> };
            assert.equal(closeSent.report["nativeGesture"], null, "outgoing close payload has nativeGesture === null");
            assert.equal(closeSent.report["gesture"], false, "outgoing close payload has gesture === false");
            const closedList = closeSent.report["closed"];
            assert.ok(Array.isArray(closedList) && closedList.includes("win-a"), "outgoing close payload closed includes win-a");

            assert.equal(h.ackCalls().length, 0, "precondition: no AckGesture before broker close reply delivered");
            assert.equal(h.heldReplies.length, 1, "genuine broker close reply held before delivery to entry");
            const held = h.heldReplies.shift()!;
            const closeParsed = parseWorldReply(held.replyText);
            assert.ok(isCoreWorldReply(closeParsed), "held close reply is core reply");
            assert.equal(closeParsed.gestureAssoc, null, "close reply has gestureAssoc === null (no G1 normalization at close)");
            assert.ok(closeParsed.gestureSettled !== null, "close reply settles active G1 from closed win-a");
            assert.equal(closeParsed.gestureSettled.id, g1Assoc.coreId, "terminal coreId equals ORIGINAL old G1 coreId");
            assert.equal(closeParsed.gestureSettled.committed, false, "close settlement has committed === false");
            assert.deepEqual(closeParsed.gestureSettledAssoc, g1Assoc, "old active pin resolves final gestureSettledAssoc");

            // Deliver genuine broker close reply once and verify exact pending head AckGesture.
            h.setHoldDescribe(false);
            held.callback(held.replyText);
            assert.equal(h.ackCalls().length, 1, "exactly one AckGesture matching entry pending head");
            const ackPayload = JSON.parse(h.ackCalls()[0]!.payload) as { instance: string; serial: string };
            assert.equal(ackPayload.instance, "g-1");
            assert.equal(ackPayload.serial, "1");

            // Complete AckGesture and verify later retained publication and redelivery are inert.
            h.ackCalls()[0]!.callback(idleGestureJson("20"));
            await h.settle();
            assert.equal(h.ackCalls().length, 1, "idle settlement after AckGesture is inert");

            h.activateWin("win-b");
            await h.settle();
            const postRetireParsed = parseWorldReply(h.lastDescribeReply());
            assert.ok(isCoreWorldReply(postRetireParsed), "post-retirement capture accepted");
            assert.deepEqual(postRetireParsed.gestureSettled, closeParsed.gestureSettled, "later publication retains settled G1");
            assert.deepEqual(postRetireParsed.gestureSettledAssoc, g1Assoc, "later publication retains pinned G1 assoc");
            assert.equal(h.ackCalls().length, 1, "later retained publication is inert");

            held.callback(held.replyText);
            assert.equal(h.ackCalls().length, 1, "late redelivery of held close reply is inert");
        } finally {
            await h.stop();
        }
    });
});

describe("KDE retained settlement native identity & freshness fences", () => {
    it("refuses ack on old generation, wrong adapter, wrong instance, or stale req/source, then acks on valid matching settlement", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "5", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap1 = h.popDescribe();
            const ng1 = cap1.body.report["nativeGesture"] as { instance: string; serial: string; phase: string } | null;
            assert.ok(ng1 !== null && ng1.instance === "gesture-1" && ng1.serial === "5" && ng1.phase === "released", "precondition: pending native terminal in flight");
            assert.equal(cap1.body.gen, "gen-1", "precondition: bound generation");
            assert.equal(cap1.body.adapter, "test-adapter", "precondition: bound adapter");
            assert.equal(h.ackCalls().length, 0, "precondition: zero ack before reply");

            // Fence 1: old generation.
            cap1.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap1.body.adapter, gen: "gen-0", req: cap1.body.source, obs: "2", rev: "2",
                source: cap1.body.source, nativeInvalid: cap1.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "5", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 0, "old generation refuses ack");
            assert.ok(h.logs.some((l) => l.includes("omnitiler:world:stale reason=generation-mismatch")));

            // Fence 2: wrong adapter.
            h.runTimers();
            const cap2 = h.popDescribe();
            cap2.call.callback(JSON.stringify(makeBaseReply({
                adapter: "wrong-adapter", gen: "gen-1", req: cap2.body.source, obs: "3", rev: "3",
                source: cap2.body.source, nativeInvalid: cap2.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "5", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 0, "wrong adapter refuses ack");
            assert.ok(h.logs.some((l) => l.includes("omnitiler:world:stale reason=adapter-mismatch")));

            // Fence 3: wrong instance in gestureSettledAssoc.
            h.runTimers();
            const cap3 = h.popDescribe();
            cap3.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap3.body.adapter, gen: "gen-1", req: cap3.body.source, obs: "4", rev: "4",
                source: cap3.body.source, nativeInvalid: cap3.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: { instance: "gesture-other", serial: "5", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 0, "wrong gestureSettledAssoc.instance refuses ack");

            // Fence 4: stale req/source.
            h.fireWindowActivated();
            h.runTimers();
            const cap4 = h.popDescribe();
            cap4.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap4.body.adapter, gen: "gen-1", req: "stale-src", obs: "5", rev: "5",
                source: "stale-src", nativeInvalid: cap4.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "5", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 0, "stale req/source refuses ack");
            assert.ok(h.logs.some((l) => l.includes("omnitiler:world:stale reason=stale-req")));
            assert.equal(h.logs.filter((l) => l.startsWith("omnitiler:world:drop-settled ")).length, 0, "no drop-settled before valid match");

            // Non-vacuity: valid matching fresh settlement retires pending head once and logs drop-settled.
            h.runTimers();
            const cap5 = h.popDescribe();
            cap5.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap5.body.adapter, gen: "gen-1", req: cap5.body.source, obs: "6", rev: "6",
                source: cap5.body.source, nativeInvalid: cap5.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "5", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 1, "valid matching settlement acks pending head once");
            assert.deepEqual(JSON.parse(h.ackCalls()[0]!.payload), { instance: "gesture-1", serial: "5" });
            const dropSettled = h.logs.filter((l) => l.startsWith("omnitiler:world:drop-settled "));
            assert.equal(dropSettled.length, 1, "drop-settled logged once on valid fresh apply");
            const dropPayload = JSON.parse(dropSettled[0]!.slice("omnitiler:world:drop-settled ".length)) as Record<string, unknown>;
            assert.equal(dropPayload["serial"], "5");
            assert.equal(dropPayload["instance"], "gesture-1");
            assert.equal(dropPayload["win"], "win-a");
            assert.equal(dropPayload["settled"], "1");
            assert.equal(dropPayload["committed"], true);
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-0 req=${cap1.body.source} rev=2 obs=2 decision=fenced reason=gen settledId=1 assocInstance=gesture-1 assocSerial=5 assocCoreId=1 pendingInstance=gesture-1 pendingSerial=5 pendingWin=win-a flightGesture=gesture-1:5:win-a`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=reply-skipped stage=accept gen=gen-0 req=${cap1.body.source} rev=2 obs=2 reason=generation-mismatch`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=${cap2.body.source} rev=3 obs=3 decision=fenced reason=adapter settledId=1 assocInstance=gesture-1 assocSerial=5 assocCoreId=1 pendingInstance=gesture-1 pendingSerial=5 pendingWin=win-a flightGesture=gesture-1:5:win-a`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=reply-skipped stage=accept gen=gen-1 req=${cap2.body.source} rev=3 obs=3 reason=adapter-mismatch`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=${cap3.body.source} rev=4 obs=4 decision=fenced reason=native-identity settledId=1 assocInstance=gesture-other assocSerial=5 assocCoreId=1 pendingInstance=gesture-1 pendingSerial=5 pendingWin=win-a flightGesture=gesture-1:5:win-a`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=stale-src rev=5 obs=5 decision=fenced reason=req settledId=1 assocInstance=gesture-1 assocSerial=5 assocCoreId=1 pendingInstance=gesture-1 pendingSerial=5 pendingWin=win-a flightGesture=gesture-1:5:win-a`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=reply-skipped stage=accept gen=gen-1 req=stale-src rev=5 obs=5 reason=stale-req`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=${cap5.body.source} rev=6 obs=6 decision=ack-armed reason=matched settledId=1 assocInstance=gesture-1 assocSerial=5 assocCoreId=1 pendingInstance=gesture-1 pendingSerial=5 pendingWin=win-a flightGesture=gesture-1:5:win-a`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=in-apply gen=gen-1 req=${cap5.body.source} rev=6 obs=6 decision=ack-already-underway reason=already-underway settledId=1 assocInstance=gesture-1 assocSerial=5 assocCoreId=1 pendingInstance=gesture-1 pendingSerial=5 pendingWin=win-a flightGesture=gesture-1:5:win-a`,
            ));
        } finally {
            h.handle.stop();
        }
    });

    it("newer pending native head from AckGesture refuses older retained terminal, then acks on its own settlement", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "1", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap1 = h.popDescribe();
            cap1.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap1.body.adapter, gen: "gen-1", req: cap1.body.source, obs: "2", rev: "2",
                source: cap1.body.source, nativeInvalid: cap1.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "1", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 1);
            assert.deepEqual(JSON.parse(h.ackCalls()[0]!.payload), { instance: "gesture-1", serial: "1" });
            assert.equal(h.logs.filter((l) => l.startsWith("omnitiler:world:drop-settled ")).length, 1);

            // AckGesture returns newer pending native terminal (serial 2).
            h.ackCalls()[0]!.callback(gestureJson({
                instance: "gesture-1", serial: "2", revision: "5", phase: "released", win: "win-a", changed: 0,
            }));
            h.runTimers();
            const cap2 = h.popDescribe();
            const ng2 = cap2.body.report["nativeGesture"] as { serial: string; phase: string } | null;
            assert.ok(ng2 !== null && ng2.serial === "2" && ng2.phase === "released", "precondition: newer native head serial 2 pending");

            // Reply still carries older retained terminal (serial 1) -> must NOT ack serial 2.
            cap2.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap2.body.adapter, gen: "gen-1", req: cap2.body.source, obs: "3", rev: "3",
                source: cap2.body.source, nativeInvalid: cap2.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "1", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 1, "older retained terminal does not ack newer pending native head");
            assert.equal(h.logs.filter((l) => l.startsWith("omnitiler:world:drop-settled ")).length, 1);

            // Subsequent capture returns serial 2's own settlement -> acks serial 2 once and logs drop-settled.
            h.fireWindowActivated();
            h.runTimers();
            const cap3 = h.popDescribe();
            cap3.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap3.body.adapter, gen: "gen-1", req: cap3.body.source, obs: "4", rev: "4",
                source: cap3.body.source, nativeInvalid: cap3.body.nativeInvalid,
                gestureSettled: { id: "2", committed: false },
                gestureSettledAssoc: { instance: "gesture-1", serial: "2", coreId: "2" },
            })));
            assert.equal(h.ackCalls().length, 2, "newer pending native head acks on its own settlement");
            assert.deepEqual(JSON.parse(h.ackCalls()[1]!.payload), { instance: "gesture-1", serial: "2" });
            const dropSettled = h.logs.filter((l) => l.startsWith("omnitiler:world:drop-settled "));
            assert.equal(dropSettled.length, 2);
            const drop2 = JSON.parse(dropSettled[1]!.slice("omnitiler:world:drop-settled ".length)) as Record<string, unknown>;
            assert.equal(drop2["serial"], "2");
            assert.equal(drop2["settled"], "2");
            assert.equal(drop2["committed"], false);
        } finally {
            h.handle.stop();
        }
    });

    it("same-generation transport reconnect preserves pending terminal and retires once; fresh handshake clears it and ignores old settlement", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            // Part A: Same-generation transport-loss reconnect (without DiscoverWorld handshake)
            // preserves pending terminal + watermark and retries the same envelope.
            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "3", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap1 = h.popDescribe();
            const preReconnectWatermark = cap1.body.nativeInvalid;
            cap1.call.callback(null); // transport error on DescribeWorld

            const wakeIdx1 = h.calls.findIndex((c) => c.method === "WaitWorldWake");
            assert.ok(wakeIdx1 >= 0, "precondition: WaitWorldWake armed");
            h.calls.splice(wakeIdx1, 1)[0]!.callback(null); // transport error on WaitWorldWake

            h.runTimers(2000);
            const cap2 = h.popDescribe();
            const ng2 = cap2.body.report["nativeGesture"] as { instance: string; serial: string; phase: string } | null;
            assert.ok(ng2 !== null && ng2.instance === "gesture-1" && ng2.serial === "3" && ng2.phase === "released", "same-gen transport reconnect preserves pending native terminal");
            assert.equal(cap2.body.nativeInvalid, preReconnectWatermark, "same-gen transport reconnect preserves nativeInvalid watermark");
            assert.equal(cap2.body.source, cap1.body.source, "same-gen transport reconnect retries exact envelope");
            cap2.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap2.body.adapter, gen: "gen-1", req: cap2.body.source, obs: "2", rev: "2",
                source: cap2.body.source, nativeInvalid: cap2.body.nativeInvalid,
                gestureSettled: { id: "3", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "3", coreId: "3" },
            })));
            assert.equal(h.ackCalls().length, 1, "same-gen transport reconnect retires preserved pending terminal");
            assert.deepEqual(JSON.parse(h.ackCalls()[0]!.payload), { instance: "gesture-1", serial: "3" });
            h.ackCalls()[0]!.callback(idleGestureJson("10"));
            h.runTimers();
            const postIdle = h.popDescribe();
            postIdle.call.callback(JSON.stringify(makeBaseReply({
                adapter: postIdle.body.adapter, gen: "gen-1", req: postIdle.body.source, obs: "3", rev: "3",
                source: postIdle.body.source, nativeInvalid: postIdle.body.nativeInvalid,
            })));

            // Part B: Fresh handshake (DiscoverWorld, even on equal gen or rotated gen-1 -> gen-2)
            // clears pending terminal and ignores old settlement.
            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "4", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap3 = h.popDescribe();
            assert.equal((cap3.body.report["nativeGesture"] as { serial: string }).serial, "4");

            h.runTimers(25000);
            const wakeIdx2 = h.calls.findIndex((c) => c.method === "WaitWorldWake");
            assert.ok(wakeIdx2 >= 0, "precondition: WaitWorldWake armed in gen-1");
            h.calls.splice(wakeIdx2, 1)[0]!.callback(JSON.stringify(makeBaseRejection({
                kind: "generation-mismatch", gen: "gen-1",
            })));
            h.connectScripted("test-adapter", "gen-2");

            const cap4 = h.popDescribe();
            assert.equal(cap4.body.gen, "gen-2", "fresh handshake rotated to gen-2");
            assert.equal(cap4.body.report["nativeGesture"], null, "fresh handshake cleared old pending native terminal");
            cap4.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap4.body.adapter, gen: "gen-2", req: cap4.body.source, obs: "1", rev: "1",
                source: cap4.body.source, nativeInvalid: cap4.body.nativeInvalid,
                gestureSettled: { id: "4", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "4", coreId: "4" },
            })));
            assert.equal(h.ackCalls().length, 1, "fresh handshake ignores old serial-4 settlement");

            // Non-vacuity in gen-2: new gesture binds and retires cleanly.
            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "5", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap5 = h.popDescribe();
            cap5.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap5.body.adapter, gen: "gen-2", req: cap5.body.source, obs: "2", rev: "2",
                source: cap5.body.source, nativeInvalid: cap5.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "5", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 2, "new gesture in gen-2 retires once");
            assert.deepEqual(JSON.parse(h.ackCalls()[1]!.payload), { instance: "gesture-1", serial: "5" });
        } finally {
            h.handle.stop();
        }
    });

    it("late DescribeWorld callback after timeout and after stop is inert", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "8", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap1 = h.popDescribe();
            assert.equal((cap1.body.report["nativeGesture"] as { serial: string }).serial, "8");

            // Time out cap1 and fire retry cap2.
            h.runTimers(2000);
            h.runTimers(2000);
            const cap2 = h.popDescribe();
            assert.equal(cap2.body.source, cap1.body.source);

            // Late callback on timed-out cap1 is inert.
            cap1.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap1.body.adapter, gen: "gen-1", req: cap1.body.source, obs: "2", rev: "2",
                source: cap1.body.source, nativeInvalid: cap1.body.nativeInvalid,
                gestureSettled: { id: "8", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "8", coreId: "8" },
            })));
            assert.equal(h.ackCalls().length, 0, "late timed-out DescribeWorld callback does not ack");

            // Stop entry, then deliver late callback on cap2 -> inert.
            h.handle.stop();
            cap2.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap2.body.adapter, gen: "gen-1", req: cap2.body.source, obs: "3", rev: "3",
                source: cap2.body.source, nativeInvalid: cap2.body.nativeInvalid,
                gestureSettled: { id: "8", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "8", coreId: "8" },
            })));
            assert.equal(h.ackCalls().length, 0, "post-stop DescribeWorld callback does not ack");
        } finally {
            h.handle.stop();
        }
    });

    it("superseded-during-flight reply discharges via tryDischargeTerminalSettlement without running apply or drop-settled", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "9", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap1 = h.popDescribe();
            assert.equal((cap1.body.report["nativeGesture"] as { serial: string }).serial, "9");

            // Bump native watermark during flight so reply is accepted-but-superseded.
            h.fireWindowActivated();
            cap1.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap1.body.adapter, gen: "gen-1", req: cap1.body.source, obs: "2", rev: "2",
                source: cap1.body.source, nativeInvalid: cap1.body.nativeInvalid,
                gestureSettled: { id: "9", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "9", coreId: "9" },
            })));
            assert.ok(h.logs.some((l) => l.includes("omnitiler:world:stale reason=superseded-during-flight")));
            assert.equal(h.ackCalls().length, 1, "superseded-during-flight discharges via tryDischargeTerminalSettlement");
            assert.deepEqual(JSON.parse(h.ackCalls()[0]!.payload), { instance: "gesture-1", serial: "9" });
            assert.equal(h.geometryWrites.length, 0, "superseded-during-flight runs zero geometry writes");
            assert.equal(h.logs.filter((l) => l.startsWith("omnitiler:world:drop-settled ")).length, 0, "superseded-during-flight skips runApply and drop-settled");
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=reply-skipped stage=flight gen=gen-1 req=${cap1.body.source} rev=2 obs=2 reason=superseded-during-flight`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=${cap1.body.source} rev=2 obs=2 decision=ack-armed reason=matched settledId=9 assocInstance=gesture-1 assocSerial=9 assocCoreId=9 pendingInstance=gesture-1 pendingSerial=9 pendingWin=win-a flightGesture=gesture-1:9:win-a`,
            ));
            assert.ok(!h.logs.includes(
                `omnitiler:world:trace ev=settlement-applied gen=gen-1 req=${cap1.body.source} rev=2 obs=2 ops=0 switches=0 gesture=present`,
            ));
        } finally {
            h.handle.stop();
        }
    });

    it("refuses ack and drop-settled on mismatched response nativeInvalid even when gen/adapter/req/source/identity match, then acks on correct nativeInvalid", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "10", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap1 = h.popDescribe();
            const ng1 = cap1.body.report["nativeGesture"] as { instance: string; serial: string; phase: string } | null;
            assert.ok(ng1 !== null && ng1.instance === "gesture-1" && ng1.serial === "10" && ng1.phase === "released", "precondition: pending native terminal in flight");
            assert.equal(cap1.body.gen, "gen-1", "precondition: bound generation");
            assert.equal(cap1.body.adapter, "test-adapter", "precondition: bound adapter");
            assert.equal(h.ackCalls().length, 0, "precondition: zero ack before reply");

            // Deliver fresh obs/rev with valid gen/adapter/req/source/gestureSettled/gestureSettledAssoc,
            // but WRONG canonical nativeInvalid ("999" !== cap1.body.nativeInvalid), with current watermark unchanged.
            const wrongWatermark = cap1.body.nativeInvalid === "999" ? "998" : "999";
            cap1.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap1.body.adapter, gen: "gen-1", req: cap1.body.source, obs: "2", rev: "2",
                source: cap1.body.source, nativeInvalid: wrongWatermark,
                gestureSettled: { id: "10", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "10", coreId: "10" },
            })));
            assert.equal(h.ackCalls().length, 0, "mismatched response nativeInvalid must not ack");
            assert.equal(h.logs.filter((l) => l.startsWith("omnitiler:world:drop-settled ")).length, 0, "mismatched response nativeInvalid must not log drop-settled");
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=${cap1.body.source} rev=2 obs=2 decision=fenced reason=nativeInvalid settledId=10 assocInstance=gesture-1 assocSerial=10 assocCoreId=10 pendingInstance=gesture-1 pendingSerial=10 pendingWin=win-a flightGesture=gesture-1:10:win-a`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=in-apply gen=gen-1 req=${cap1.body.source} rev=2 obs=2 decision=fenced reason=nativeInvalid settledId=10 assocInstance=gesture-1 assocSerial=10 assocCoreId=10 pendingInstance=gesture-1 pendingSerial=10 pendingWin=win-a flightGesture=gesture-1:10:win-a`,
            ));

            // Non-vacuity: subsequent fresh capture with matching nativeInvalid acks once.
            h.fireWindowActivated();
            h.runTimers();
            const cap2 = h.popDescribe();
            cap2.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap2.body.adapter, gen: "gen-1", req: cap2.body.source, obs: "3", rev: "3",
                source: cap2.body.source, nativeInvalid: cap2.body.nativeInvalid,
                gestureSettled: { id: "10", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "10", coreId: "10" },
            })));
            assert.equal(h.ackCalls().length, 1, "correct-bound fresh reply acks once");
            assert.deepEqual(JSON.parse(h.ackCalls()[0]!.payload), { instance: "gesture-1", serial: "10" });
        } finally {
            h.handle.stop();
        }
    });
});

describe("KDE retained settlement trace diagnostics", () => {
    it("bounds switchSettled.failed at 8 with explicit failedOmitted on receipt and apply", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            const failed = ["w1", "w2", "w3", "w4", "w5", "w6", "w7", "w8", "w9", "w10"];
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
                switchSettled: [
                    { out: "out-a", op: "op-9", outcome: "completed-with-failures", failed, focus: "unproven", fallback: true },
                ],
            })));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=settlement-switch phase=receipt gen=gen-1 req=${init.body.source} rev=1 obs=1 out=out-a op=op-9 outcome=completed-with-failures focus=unproven fallback=true failed=w1,w2,w3,w4,w5,w6,w7,w8 failedOmitted=2`,
            ));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=settlement-switch phase=apply gen=gen-1 req=${init.body.source} rev=1 obs=1 out=out-a op=op-9 outcome=completed-with-failures focus=unproven fallback=true failed=w1,w2,w3,w4,w5,w6,w7,w8 failedOmitted=2`,
            ));
        } finally {
            h.handle.stop();
        }
    });

    it("pins missing-association, coreId fence, unproven AckGesture responses (same-head, rotated-instance, malformed, unavailable, superseded), and stale-nonce", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "11", phase: "released", win: "win-a" }));
            h.runTimers();

            // missing-association
            const cap1 = h.popDescribe();
            cap1.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap1.body.adapter, gen: "gen-1", req: cap1.body.source, obs: "2", rev: "2",
                source: cap1.body.source, nativeInvalid: cap1.body.nativeInvalid,
                gestureSettled: { id: "11", committed: true },
                gestureSettledAssoc: null,
            })));
            assert.equal(h.ackCalls().length, 0);
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=${cap1.body.source} rev=2 obs=2 decision=fenced reason=missing-association settledId=11 assocInstance=- assocSerial=- assocCoreId=- pendingInstance=gesture-1 pendingSerial=11 pendingWin=win-a flightGesture=gesture-1:11:win-a`,
            ));

            // coreId mismatch
            h.fireWindowActivated();
            h.runTimers();
            const cap2 = h.popDescribe();
            cap2.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap2.body.adapter, gen: "gen-1", req: cap2.body.source, obs: "3", rev: "3",
                source: cap2.body.source, nativeInvalid: cap2.body.nativeInvalid,
                gestureSettled: { id: "11", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "11", coreId: "99" },
            })));
            assert.equal(h.ackCalls().length, 0);
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-settlement-decision path=discharge gen=gen-1 req=${cap2.body.source} rev=3 obs=3 decision=fenced reason=coreId settledId=11 assocInstance=gesture-1 assocSerial=11 assocCoreId=99 pendingInstance=gesture-1 pendingSerial=11 pendingWin=win-a flightGesture=gesture-1:11:win-a`,
            ));

            // Valid match arms ack
            h.fireWindowActivated();
            h.runTimers();
            const cap3 = h.popDescribe();
            cap3.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap3.body.adapter, gen: "gen-1", req: cap3.body.source, obs: "4", rev: "4",
                source: cap3.body.source, nativeInvalid: cap3.body.nativeInvalid,
                gestureSettled: { id: "11", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "11", coreId: "11" },
            })));
            assert.equal(h.ackCalls().length, 1);

            // Same-head AckGesture response -> unproven, does NOT mark inert-already-retired
            h.ackCalls()[0]!.callback(gestureJson({
                instance: "gesture-1", serial: "11", revision: "5", phase: "released", win: "win-a", changed: 0,
            }));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-ack-response gen=gen-1 req=${cap3.body.source} rev=4 obs=4 instance=gesture-1 serial=11 coreId=11 status=unproven reason=same-head nextPhase=released nextInstance=gesture-1 nextSerial=11`,
            ));

            // Late duplicate callback on same AckGesture -> fenced stale-nonce
            h.ackCalls()[0]!.callback(idleGestureJson("6"));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-ack-response gen=gen-1 req=${cap3.body.source} rev=4 obs=4 instance=gesture-1 serial=11 coreId=11 status=fenced reason=stale-nonce nextPhase=- nextInstance=- nextSerial=-`,
            ));

            // Next settlement after same-head unproven ack: re-arms against newly set pending head or sees no-pending-head (never inert-already-retired)
            h.runTimers();
            const cap4 = h.popDescribe();
            cap4.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap4.body.adapter, gen: "gen-1", req: cap4.body.source, obs: "5", rev: "5",
                source: cap4.body.source, nativeInvalid: cap4.body.nativeInvalid,
                gestureSettled: { id: "11", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "11", coreId: "11" },
            })));
            assert.equal(h.ackCalls().length, 2);
            // Rotated-instance AckGesture response -> unproven
            const rotatedPayload = gestureJson({
                instance: "gesture-2", serial: "0", revision: "7", phase: "idle", win: "", kind: "",
                startRect: null, startCursor: null, cursor: null, modalMove: 0, changed: 0,
            });
            assert.ok(parseNativeGesture(rotatedPayload)?.available === true, "precondition: rotated-instance payload must parse");
            h.ackCalls()[1]!.callback(rotatedPayload);
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-ack-response gen=gen-1 req=${cap4.body.source} rev=5 obs=5 instance=gesture-1 serial=11 coreId=11 status=unproven reason=rotated-instance nextPhase=idle nextInstance=gesture-2 nextSerial=0`,
            ));
        } finally {
            h.handle.stop();
        }
    });

    it("pins malformed, unavailable, and superseded AckGesture responses as unproven", () => {
        const malformedCase = {
            name: "malformed",
            payload: "not-json",
            precondition: () => assert.equal(parseNativeGesture("not-json"), null),
            expectedSuffix: "status=unproven reason=malformed nextPhase=- nextInstance=- nextSerial=-",
        };
        const unavailablePayload = gestureJson({
            instance: "gesture-2", serial: "0", revision: "8", phase: "idle", win: "", kind: "",
            startRect: null, startCursor: null, cursor: null, modalMove: 0,
            changed: 0, superseded: 0, available: 0,
        });
        const unavailableCase = {
            name: "unavailable",
            payload: unavailablePayload,
            precondition: () => assert.equal(parseNativeGesture(unavailablePayload)?.available, false),
            expectedSuffix: "status=unproven reason=unavailable nextPhase=idle nextInstance=gesture-2 nextSerial=0",
        };
        const supersededPayload = gestureJson({
            instance: "gesture-2", serial: "0", revision: "9", phase: "idle", win: "", kind: "",
            startRect: null, startCursor: null, cursor: null, modalMove: 0,
            changed: 0, superseded: 1, available: 1,
        });
        const supersededCase = {
            name: "superseded",
            payload: supersededPayload,
            precondition: () => {
                const parsed = parseNativeGesture(supersededPayload);
                assert.ok(parsed !== null && parsed.available && parsed.superseded);
            },
            expectedSuffix: "status=unproven reason=superseded nextPhase=idle nextInstance=gesture-2 nextSerial=0",
        };
        for (const tc of [malformedCase, unavailableCase, supersededCase]) {
            const h = setupEntryHarness();
            try {
                h.connectScripted("test-adapter", "gen-1");
                const init = h.popDescribe();
                init.call.callback(JSON.stringify(makeBaseReply({
                    adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                    source: init.body.source, nativeInvalid: init.body.nativeInvalid,
                })));

                h.popWaitGesture().callback(gestureJson({ instance: "gesture-2", serial: "12", phase: "released", win: "win-a" }));
                h.runTimers();
                const cap = h.popDescribe();
                cap.call.callback(JSON.stringify(makeBaseReply({
                    adapter: cap.body.adapter, gen: "gen-1", req: cap.body.source, obs: "2", rev: "2",
                    source: cap.body.source, nativeInvalid: cap.body.nativeInvalid,
                    gestureSettled: { id: "12", committed: true },
                    gestureSettledAssoc: { instance: "gesture-2", serial: "12", coreId: "12" },
                })));
                assert.equal(h.ackCalls().length, 1, `${tc.name}: ack armed`);
                tc.precondition();
                h.ackCalls()[0]!.callback(tc.payload);
                assert.ok(h.logs.includes(
                    `omnitiler:world:trace ev=gesture-ack-response gen=gen-1 req=${cap.body.source} rev=2 obs=2 instance=gesture-2 serial=12 coreId=12 ${tc.expectedSuffix}`,
                ), `${tc.name}: expected trace line`);
            } finally {
                h.handle.stop();
            }
        }
    });

    it("pins nonterminal same-serial and older-serial AckGesture responses as unproven, and next-terminal as retired", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            // Same-serial nonterminal (phase=started, serial=11) -> unproven same-head
            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "11", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap1 = h.popDescribe();
            cap1.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap1.body.adapter, gen: "gen-1", req: cap1.body.source, obs: "2", rev: "2",
                source: cap1.body.source, nativeInvalid: cap1.body.nativeInvalid,
                gestureSettled: { id: "11", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "11", coreId: "11" },
            })));
            assert.equal(h.ackCalls().length, 1);
            const sameSerialStarted = gestureJson({
                instance: "gesture-1", serial: "11", revision: "5", phase: "started", win: "win-a", kind: "move", changed: 0,
            });
            assert.equal(parseNativeGesture(sameSerialStarted)?.phase, "started", "precondition: same-serial nonterminal parses");
            h.ackCalls()[0]!.callback(sameSerialStarted);
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-ack-response gen=gen-1 req=${cap1.body.source} rev=2 obs=2 instance=gesture-1 serial=11 coreId=11 status=unproven reason=same-head nextPhase=started nextInstance=gesture-1 nextSerial=11`,
            ));

            // Older-serial non-idle (phase=started, serial=10 < 11) -> unproven older-head
            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "11", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap2 = h.popDescribe();
            cap2.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap2.body.adapter, gen: "gen-1", req: cap2.body.source, obs: "3", rev: "3",
                source: cap2.body.source, nativeInvalid: cap2.body.nativeInvalid,
                gestureSettled: { id: "11", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "11", coreId: "11" },
            })));
            assert.equal(h.ackCalls().length, 2);
            const olderSerialStarted = gestureJson({
                instance: "gesture-1", serial: "10", revision: "6", phase: "started", win: "win-a", kind: "move", changed: 0,
            });
            assert.equal(parseNativeGesture(olderSerialStarted)?.serial, "10", "precondition: older-serial nonterminal parses");
            h.ackCalls()[1]!.callback(olderSerialStarted);
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-ack-response gen=gen-1 req=${cap2.body.source} rev=3 obs=3 instance=gesture-1 serial=11 coreId=11 status=unproven reason=older-head nextPhase=started nextInstance=gesture-1 nextSerial=10`,
            ));

            // Next-terminal (phase=released, serial=12 > 11) -> retired next-terminal
            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "11", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap3 = h.popDescribe();
            cap3.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap3.body.adapter, gen: "gen-1", req: cap3.body.source, obs: "4", rev: "4",
                source: cap3.body.source, nativeInvalid: cap3.body.nativeInvalid,
                gestureSettled: { id: "11", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "11", coreId: "11" },
            })));
            assert.equal(h.ackCalls().length, 3);
            const nextTerminal = gestureJson({
                instance: "gesture-1", serial: "12", revision: "7", phase: "released", win: "win-a", kind: "move", changed: 0,
            });
            assert.equal(parseNativeGesture(nextTerminal)?.serial, "12", "precondition: next-terminal parses");
            h.ackCalls()[2]!.callback(nextTerminal);
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-ack-response gen=gen-1 req=${cap3.body.source} rev=4 obs=4 instance=gesture-1 serial=11 coreId=11 status=retired reason=next-terminal nextPhase=released nextInstance=gesture-1 nextSerial=12`,
            ));
        } finally {
            h.handle.stop();
        }
    });

    it("late G1 AckGesture callback while G2 ack is underway emits G1 tuple with stale-nonce and never mixes G2", () => {
        const h = setupEntryHarness();
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            // G1 settles and arms ack[0]
            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "1", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap1 = h.popDescribe();
            cap1.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap1.body.adapter, gen: "gen-1", req: cap1.body.source, obs: "2", rev: "2",
                source: cap1.body.source, nativeInvalid: cap1.body.nativeInvalid,
                gestureSettled: { id: "1", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "1", coreId: "1" },
            })));
            assert.equal(h.ackCalls().length, 1);

            // G1 ack retires exposing G2 (serial=2)
            h.ackCalls()[0]!.callback(gestureJson({
                instance: "gesture-1", serial: "2", revision: "5", phase: "released", win: "win-a", changed: 0,
            }));
            h.runTimers();
            const cap2 = h.popDescribe();
            cap2.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap2.body.adapter, gen: "gen-1", req: cap2.body.source, obs: "3", rev: "3",
                source: cap2.body.source, nativeInvalid: cap2.body.nativeInvalid,
                gestureSettled: { id: "2", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "2", coreId: "2" },
            })));
            assert.equal(h.ackCalls().length, 2, "G2 ack is underway");

            // Late G1 callback arrives while G2 ack is pending
            const logCountBefore = h.logs.length;
            h.ackCalls()[0]!.callback(idleGestureJson("6"));
            assert.equal(h.ackCalls().length, 2, "late G1 callback triggers zero extra ack");
            const lateLogs = h.logs.slice(logCountBefore);
            assert.ok(lateLogs.includes(
                `omnitiler:world:trace ev=gesture-ack-response gen=gen-1 req=${cap1.body.source} rev=2 obs=2 instance=gesture-1 serial=1 coreId=1 status=fenced reason=stale-nonce nextPhase=- nextInstance=- nextSerial=-`,
            ));
            assert.ok(!lateLogs.some((l) => l.includes("serial=2") || l.includes("coreId=2") || l.includes(`req=${cap2.body.source}`)), "must never mix G2 tuple/coreId");

            // G2 ack completes normally
            h.ackCalls()[1]!.callback(idleGestureJson("7"));
            assert.ok(h.logs.includes(
                `omnitiler:world:trace ev=gesture-ack-response gen=gen-1 req=${cap2.body.source} rev=3 obs=3 instance=gesture-1 serial=2 coreId=2 status=retired reason=idle nextPhase=idle nextInstance=gesture-1 nextSerial=0`,
            ));
        } finally {
            h.handle.stop();
        }
    });

    it("trace logger failures are fail-open and preserve native ack and apply behavior", () => {
        const h = setupEntryHarness({
            onLog: (line) => {
                if (line.startsWith("omnitiler:world:trace")) {
                    throw new Error("trace logger boom");
                }
            },
        });
        try {
            h.connectScripted("test-adapter", "gen-1");
            const init = h.popDescribe();
            init.call.callback(JSON.stringify(makeBaseReply({
                adapter: init.body.adapter, req: init.body.source, obs: "1", rev: "1",
                source: init.body.source, nativeInvalid: init.body.nativeInvalid,
            })));

            h.popWaitGesture().callback(gestureJson({ instance: "gesture-1", serial: "20", phase: "released", win: "win-a" }));
            h.runTimers();
            const cap1 = h.popDescribe();
            cap1.call.callback(JSON.stringify(makeBaseReply({
                adapter: cap1.body.adapter, gen: "gen-1", req: cap1.body.source, obs: "2", rev: "2",
                source: cap1.body.source, nativeInvalid: cap1.body.nativeInvalid,
                gestureSettled: { id: "20", committed: true },
                gestureSettledAssoc: { instance: "gesture-1", serial: "20", coreId: "20" },
            })));
            assert.equal(h.ackCalls().length, 1, "throwing trace logger must not block native ack");
            assert.equal(h.geometryWrites.length, 0, "no extra setters");
            h.ackCalls()[0]!.callback(idleGestureJson("10"));
        } finally {
            h.handle.stop();
        }
    });
});
