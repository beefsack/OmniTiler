// Authoritative native gesture lifecycle discriminators (effect-owned
// only): script signals never mint, native START/terminal maps to the core
// ingress lane, unavailable omits without reset, and terminal acks only on
// the exact core settlement.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { startWorldAdapterEntry } from "../src/world-entry";
import { parseNativeGesture, rawNativeGestureJson } from "../src/world-gesture";

function gestureSnapshot(over: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 1, instance: "gesture-1", serial: "1", revision: "1",
        phase: "started", win: "win-a", kind: "move",
        startRect: [0, 0, 200, 200], startCursor: [10, 10], cursor: [20, 20],
        modalMove: 1, changed: 1, superseded: 0, available: 1, ...over,
    });
}

function last<T>(items: T[]): T | undefined {
    return items.length > 0 ? items[items.length - 1] : undefined;
}

describe("native gesture lifecycle transport", () => {
    it("parses strict snapshots and rejects idle with facts", () => {
        const good = parseNativeGesture(gestureSnapshot());
        assert.ok(good !== null && good.phase === "started" && good.win === "win-a");
        const badIdle = parseNativeGesture(gestureSnapshot({ phase: "idle", serial: "0", win: "", kind: "", startRect: [0, 0, 1, 1] }));
        assert.equal(badIdle, null);
        const badKind = parseNativeGesture(gestureSnapshot({ kind: "drag" }));
        assert.equal(badKind, null);
    });

    it("serializes the raw snapshot canonically for the broker lane", () => {
        const snap = parseNativeGesture(gestureSnapshot({ serial: "5", revision: "9", phase: "started" }));
        assert.ok(snap !== null);
        const raw = JSON.parse(rawNativeGestureJson(snap)) as Record<string, unknown>;
        assert.equal(raw["v"], 1);
        assert.equal(raw["instance"], "gesture-1");
        assert.equal(raw["serial"], "5");
        assert.equal(raw["revision"], "9");
        assert.equal(raw["phase"], "started");
        assert.equal(raw["win"], "win-a");
        assert.equal(raw["kind"], "move");
        assert.deepEqual(raw["startRect"], [0, 0, 200, 200]);
        assert.equal(raw["modalMove"], 1);
        assert.equal(raw["changed"], 1);
        assert.equal(raw["superseded"], 0);
        assert.equal(raw["available"], 1);
        // The script carries facts only: no core id is minted here (the
        // broker owns the mapping per generation).
        assert.ok(!("id" in raw));
    });

    it("raw carry preserves terminal phases verbatim (no Released rewrite)", () => {
        // Fast START-Esc-finish resolves upstream to Cancelled and the
        // transport preserves the phase verbatim (never a Released rewrite).
        const cancelled = parseNativeGesture(gestureSnapshot({ phase: "cancelled", cursor: [30, 30], revision: "3" }));
        assert.ok(cancelled !== null);
        const cancelledRaw = JSON.parse(rawNativeGestureJson(cancelled)) as Record<string, unknown>;
        assert.equal(cancelledRaw["phase"], "cancelled");
        assert.deepEqual(cancelledRaw["cursor"], [30, 30]);
        // A consumed Esc finishes as Released upstream and also passes
        // through verbatim.
        const released = parseNativeGesture(gestureSnapshot({ phase: "released", cursor: null, revision: "4" }));
        assert.ok(released !== null);
        const releasedRaw = JSON.parse(rawNativeGestureJson(released)) as Record<string, unknown>;
        assert.equal(releasedRaw["phase"], "released");
        assert.equal(releasedRaw["cursor"], null);
        // Open snapshots ride with their native cursor (possibly null): the
        // broker folds the same-capture script cursor only when native is
        // null, never the reverse.
        const open = parseNativeGesture(gestureSnapshot({ phase: "started", cursor: null, revision: "5" }));
        assert.ok(open !== null);
        assert.equal((JSON.parse(rawNativeGestureJson(open)) as Record<string, unknown>)["cursor"], null);
    });

    it("script START alone never mints an ingress; native START does; terminal acks exactly once", () => {
        const ws1 = { id: "ws1", name: "Workspace 1" };
        const fakeWin = {
            internalId: "win-a", normalWindow: true, managed: true, deleted: false,
            transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, fullScreen: false, maximized: false, maximizeMode: 0,
            sticky: false, onAllDesktops: false, minSize: null, maxSize: null,
            interactiveMoveResizeStarted: { connect: () => undefined, disconnect: () => undefined },
            interactiveMoveResizeFinished: { connect: () => undefined, disconnect: () => undefined },
        };
        const startedSignal = { connect: (h: () => void) => { (fakeWin as Record<string, unknown>)["__started"] = h; }, disconnect: () => undefined };
        (fakeWin as Record<string, unknown>)["interactiveMoveResizeStarted"] = startedSignal;
        const surface = {
            windowList: () => [fakeWin],
            screens: [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
            desktops: [ws1],
            activeScreen: { name: "out-1" },
            cursorPos: { x: 20, y: 20 },
            get activeWindow() { return fakeWin; },
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: () => ws1,
            windowAdded: { connect: () => undefined, disconnect: () => undefined },
            windowRemoved: { connect: () => undefined, disconnect: () => undefined },
        };
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const gestureCalls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        let describeRev = 0;
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_s, _p, _i, method, payload, callback) => {
                calls.push({ method, payload, callback });
                if (method === "DiscoverWorld") {
                    const body = JSON.parse(payload) as { req: string };
                    callback(JSON.stringify({
                        v: 2, adapter: "broker-1", gen: "gen-1", req: body.req, obs: "0", rev: "0",
                        ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
                        windows: [], transfers: [], effects: [], result: null, gestureSettled: null,
                        operationSettled: [], nativeCreations: [], warpCursor: null,
                        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
                    }));
                } else if (method === "DescribeWorld") {
                    const body = JSON.parse(payload) as { source: string; nativeInvalid: string };
                    describeRev += 1;
                    const report = JSON.parse(payload).report as Record<string, unknown>;
                    const raw = report["nativeGesture"] as Record<string, unknown> | null;
                    const assoc = raw !== null && raw !== undefined &&
                        typeof raw["instance"] === "string" && typeof raw["serial"] === "string" &&
                        (raw["serial"] as string) !== "0"
                        ? {
                            instance: raw["instance"] as string, serial: raw["serial"] as string,
                            coreId: (globalThis as Record<string, unknown>)["__gestureCoreId"] as string ?? "1",
                        }
                        : null;
                    callback(JSON.stringify({
                        v: 2, adapter: "broker-1", gen: "gen-1", req: body.source, obs: String(describeRev), rev: String(describeRev),
                        ready: true, consumed: false, scopes: [], workspaces: [], outputs: [],
                        windows: [], transfers: [], effects: [], result: null,
                        gestureSettled: (globalThis as Record<string, unknown>)["__gestureSettled"] ?? null,
                        gestureAssoc: assoc,
                        operationSettled: [], nativeCreations: [], warpCursor: null,
                        source: body.source, nativeInvalid: body.nativeInvalid, clock: "1",
                        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
                    }));
                }
            },
            gestureCallDbus: (_s, _p, _i, method, payload, callback) => {
                gestureCalls.push({ method, payload, callback });
                if (method === "AckGesture") {
                    callback(JSON.stringify({
                        v: 1, instance: "gesture-1", serial: "0", revision: "3",
                        phase: "idle", win: "", kind: "",
                        startRect: null, startCursor: null, cursor: null,
                        modalMove: 0, changed: 1, superseded: 0, available: 1,
                    }));
                }
            },
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: () => undefined,
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
        // Discovery + auto observation (idle: no ingress lane content).
        calls.find((c) => c.method === "DiscoverWorld")?.callback;
        const discover = calls.find((c) => c.method === "DiscoverWorld");
        assert.ok(discover !== undefined);
        discover.callback(JSON.stringify({
            v: 2, adapter: "broker-1", gen: "gen-1", req: JSON.parse(discover.payload).req, obs: "0", rev: "0",
            ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
            windows: [], transfers: [], effects: [], result: null, gestureSettled: null,
            operationSettled: [], nativeCreations: [], warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        }));
        runTimers();
        const auto = last(calls.filter((c) => c.method === "DescribeWorld"));
        assert.ok(auto !== undefined);
        const autoReport = JSON.parse(auto.payload).report as Record<string, unknown>;
        assert.equal(autoReport["gestureIngress"], null);
        assert.equal(autoReport["nativeGesture"], null);
        assert.equal(autoReport["gesture"], false);
        (globalThis as Record<string, unknown>)["__gestureSettled"] = undefined;

        // A script START signal alone (no native snapshot) never mints.
        ((fakeWin as Record<string, unknown>)["__started"] as (() => void) | undefined)?.();
        runTimers();
        const afterScript = last(calls.filter((c) => c.method === "DescribeWorld"));
        assert.ok(afterScript !== undefined);
        // No new dispatch is forced by script signals alone, and any report
        // still carries no ingress.
        const scriptReport = JSON.parse(afterScript.payload).report as Record<string, unknown>;
        assert.equal(scriptReport["gestureIngress"], null);
        assert.equal(scriptReport["nativeGesture"], null);

        // Native START arrives through the held waiter: next capture carries it raw.
        const wait = gestureCalls.find((c) => c.method === "WaitGesture");
        assert.ok(wait !== undefined);
        wait.callback(gestureSnapshot({ serial: "1", revision: "1", phase: "started" }));
        runTimers();
        const started = last(calls.filter((c) => c.method === "DescribeWorld"));
        assert.ok(started !== undefined);
        const startedReport = JSON.parse(started.payload).report as Record<string, unknown>;
        const raw = startedReport["nativeGesture"] as Record<string, unknown>;
        assert.equal(raw["instance"], "gesture-1");
        assert.equal(raw["serial"], "1");
        assert.equal(raw["win"], "win-a");
        assert.equal(raw["kind"], "move");
        assert.equal(raw["phase"], "started");
        assert.deepEqual(raw["startRect"], [0, 0, 200, 200]);
        assert.equal(startedReport["gestureIngress"], null);
        assert.equal(startedReport["gesture"], true);

        // Native terminal: released ingress; ack only after exact settlement.
        (globalThis as Record<string, unknown>)["__gestureSettled"] = { id: "1", committed: false };
        const wait2 = last(gestureCalls.filter((c) => c.method === "WaitGesture"));
        assert.ok(wait2 !== undefined);
        wait2.callback(gestureSnapshot({ serial: "1", revision: "2", phase: "released", cursor: [30, 30] }));
        runTimers();
        const releasedCalls = calls.filter((c) => c.method === "DescribeWorld");
        const released = [...releasedCalls].reverse().find((c) => {
            try {
                const r = (JSON.parse(c.payload).report as Record<string, unknown>)["nativeGesture"] as Record<string, unknown> | null;
                return r !== null && r["phase"] === "released";
            } catch (error) {
                void error;
                return false;
            }
        });
        assert.ok(released !== undefined, "expected a released capture");
        const releasedReport = JSON.parse(released.payload).report as Record<string, unknown>;
        assert.equal((releasedReport["nativeGesture"] as Record<string, unknown>)["phase"], "released");
        assert.deepEqual((releasedReport["nativeGesture"] as Record<string, unknown>)["cursor"], [30, 30]);
        assert.equal(releasedReport["gestureIngress"], null);
        const acks = gestureCalls.filter((c) => c.method === "AckGesture");
        assert.equal(acks.length, 1);
        assert.deepEqual(JSON.parse(acks[0]?.payload as string), { instance: "gesture-1", serial: "1" });
        delete (globalThis as Record<string, unknown>)["__gestureSettled"];
        handle?.stop();
    });
});

describe("native gesture broker cutover", () => {
    function rawReport(payload: string): Record<string, unknown> {
        return JSON.parse(payload).report as Record<string, unknown>;
    }

    function startCutoverHarness(opts: {
        useGlobalCallDbus?: boolean;
        settledHook?: (report: Record<string, unknown>, assoc: { instance: string; serial: string; coreId: string } | null) => { id: string; committed: boolean } | null;
        manualDescribe?: boolean;
    } = {}): {
        calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>;
        gestureCalls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }>;
        fireDelay: (delay: number) => void;
        fireScriptStart: () => void;
        globalCalls: Array<{ method: string; payload: string }>;
        heldDescribes: Array<{ payload: string; callback: (reply: unknown) => void }>;
        answerHeld: (index?: number) => void;
        stop: () => void;
    } {
        const ws1 = { id: "ws1", name: "Workspace 1" };
        const fakeWin = {
            internalId: "win-a", normalWindow: true, managed: true, deleted: false,
            transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out-1" }, desktops: [ws1],
            minimized: false, fullscreen: false, fullScreen: false, maximized: false, maximizeMode: 0,
            sticky: false, onAllDesktops: false, minSize: null, maxSize: null,
            interactiveMoveResizeStarted: { connect: () => undefined, disconnect: () => undefined },
            interactiveMoveResizeFinished: { connect: () => undefined, disconnect: () => undefined },
        };
        const surface = {
            windowList: () => [fakeWin],
            screens: [{ name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
            desktops: [ws1],
            activeScreen: { name: "out-1" },
            cursorPos: { x: 20, y: 20 },
            get activeWindow() { return fakeWin; },
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktopForScreen: () => ws1,
            windowAdded: { connect: () => undefined, disconnect: () => undefined },
            windowRemoved: { connect: () => undefined, disconnect: () => undefined },
        };
        const calls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const gestureCalls: Array<{ method: string; payload: string; callback: (reply: unknown) => void }> = [];
        const heldDescribes: Array<{ payload: string; callback: (reply: unknown) => void }> = [];
        const answerDescribePayload = (payload: string, callback: (reply: unknown) => void): void => {
            const body = JSON.parse(payload) as { source: string; nativeInvalid: string };
            describeRev += 1;
            const report = JSON.parse(payload).report as Record<string, unknown>;
            const assoc = harnessAssocFor(report);
            const settled = opts.settledHook?.(report, assoc) ?? null;
            callback(JSON.stringify({
                v: 2, adapter: "broker-1", gen: "gen-1", req: body.source, obs: String(describeRev), rev: String(describeRev),
                ready: true, consumed: false, scopes: [], workspaces: [], outputs: [],
                windows: [], transfers: [], effects: [], result: null,
                gestureSettled: settled, gestureAssoc: assoc, operationSettled: [], nativeCreations: [], warpCursor: null,
                source: body.source, nativeInvalid: body.nativeInvalid, clock: "1",
                presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            }));
        };
        const answerHeld = (index?: number): void => {
            const at = index ?? heldDescribes.length - 1;
            const held = heldDescribes[at];
            assert.ok(held !== undefined, "held DescribeWorld exists");
            heldDescribes.splice(at, 1);
            answerDescribePayload(held.payload, held.callback);
        };
        const globalCalls: Array<{ method: string; payload: string }> = [];
        const timers: Array<{ delay: number; cb: () => void }> = [];
        let describeRev = 0;
        let scriptStartHandler: (() => void) | null = null;
        // Harness broker identity map: first-seen native identities
        // allocate fresh core ids in order, mirroring the production
        // broker (stale/retired discrimination lives in the Rust tests;
        // the harness only proves the binding the script must vet).
        const harnessCoreIds = new Map<string, string>();
        let harnessNextCore = 1;
        const harnessAssocFor = (report: Record<string, unknown>): { instance: string; serial: string; coreId: string } | null => {
            const raw = report["nativeGesture"] as Record<string, unknown> | null;
            if (raw === null || raw === undefined) {
                return null;
            }
            const instance = raw["instance"];
            const serial = raw["serial"];
            if (typeof instance !== "string" || typeof serial !== "string" || serial === "0") {
                return null;
            }
            const key = `${instance}\u0000${serial}`;
            let id = harnessCoreIds.get(key);
            if (id === undefined) {
                id = String(harnessNextCore);
                harnessNextCore += 1;
                harnessCoreIds.set(key, id);
            }
            return { instance, serial, coreId: id };
        };
        (fakeWin as unknown as Record<string, unknown>)["interactiveMoveResizeStarted"] = {
            connect: (h: () => void) => { scriptStartHandler = h; },
            disconnect: () => undefined,
        };
        if (opts.useGlobalCallDbus === true) {
            (globalThis as Record<string, unknown>)["callDBus"] = (
                _s: string, _p: string, _i: string, method: string,
                payload: string, callback: (reply: unknown) => void,
            ) => {
                globalCalls.push({ method, payload });
                gestureCalls.push({ method, payload, callback });
            };
        }
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_s, _p, _i, method, payload, callback) => {
                calls.push({ method, payload, callback });
                if (method === "DiscoverWorld") {
                    const body = JSON.parse(payload) as { req: string };
                    callback(JSON.stringify({
                        v: 2, adapter: "broker-1", gen: "gen-1", req: body.req, obs: "0", rev: "0",
                        ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
                        windows: [], transfers: [], effects: [], result: null, gestureSettled: null,
                        operationSettled: [], nativeCreations: [], warpCursor: null,
                        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
                    }));
                } else if (method === "DescribeWorld") {
                    if (opts.manualDescribe === true) {
                        heldDescribes.push({ payload, callback });
                        return;
                    }
                    answerDescribePayload(payload, callback);
                }
            },
            ...(opts.useGlobalCallDbus === true ? {} : {
                gestureCallDbus: (_s, _p, _i, method, payload, callback) => {
                    gestureCalls.push({ method, payload, callback });
                },
            }),
            scheduleOnce: (delay, cb) => {
                timers.push({ delay, cb });
                return () => undefined;
            },
            log: () => undefined,
        });
        assert.ok(handle !== null);
        const fireDelay = (delay: number): void => {
            for (let i = timers.length - 1; i >= 0; i -= 1) {
                const t = timers[i] as { delay: number; cb: () => void };
                if (t.delay === delay) {
                    timers.splice(i, 1);
                    t.cb();
                }
            }
        };
        // Bind the generation synchronously like the production discovery.
        const discover = calls.find((c) => c.method === "DiscoverWorld");
        assert.ok(discover !== undefined);
        discover.callback(JSON.stringify({
            v: 2, adapter: "broker-1", gen: "gen-1", req: JSON.parse(discover.payload).req, obs: "0", rev: "0",
            ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
            windows: [], transfers: [], effects: [], result: null, gestureSettled: null,
            operationSettled: [], nativeCreations: [], warpCursor: null,
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        }));
        fireDelay(120);
        return {
            calls, gestureCalls, fireDelay, globalCalls, heldDescribes, answerHeld,
        fireScriptStart: () => {
                try {
                    scriptStartHandler?.();
                } catch (error) {
                    void error;
                }
            },
            stop: () => {
                handle?.stop();
                if (opts.useGlobalCallDbus === true) {
                    delete (globalThis as Record<string, unknown>)["callDBus"];
                }
            },
        };
    }

    it("uses the default global callDBus when no gesture override is wired", () => {
        const h = startCutoverHarness({ useGlobalCallDbus: true });
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined, "waiter arms through the default global path");
            const globalWait = h.globalCalls.find((c) => c.method === "WaitGesture");
            assert.deepEqual(globalWait, { method: "WaitGesture", payload: wait.payload });
        } finally {
            h.stop();
        }
    });

    it("carries raw native snapshots and never a script-minted ingress", () => {
        const h = startCutoverHarness({});
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "1", revision: "1", phase: "started" }));
            h.fireDelay(120);
            const started = last(h.calls.filter((c) => c.method === "DescribeWorld"));
            assert.ok(started !== undefined);
            const report = rawReport(started.payload);
            assert.equal(report["gestureIngress"], null);
            const raw = report["nativeGesture"] as Record<string, unknown>;
            assert.equal(raw["instance"], "gesture-1");
            assert.equal(raw["serial"], "1");
            assert.equal(raw["phase"], "started");
            assert.equal(raw["win"], "win-a");
            assert.equal(report["gesture"], true);
        } finally {
            h.stop();
        }
    });

    it("retries a lost ack on a fast cadence, then retains it at the host deadline", () => {
        const h = startCutoverHarness({
            settledHook: (report, assoc) => {
                const raw = report["nativeGesture"] as Record<string, unknown> | null;
                if (raw !== null && (raw["phase"] === "released" || raw["phase"] === "cancelled")) {
                    assert.ok(assoc !== null);
                    return { id: (assoc as { coreId: string }).coreId, committed: true };
                }
                return null;
            },
        });
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "1", revision: "1", phase: "started" }));
            h.fireDelay(120);
            const wait2 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait2 !== undefined);
            wait2.callback(gestureSnapshot({ serial: "1", revision: "2", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            const firstAck = h.gestureCalls.find((c) => c.method === "AckGesture");
            assert.ok(firstAck !== undefined, "terminal acks exactly once after settlement");
            assert.deepEqual(JSON.parse(firstAck.payload as string), { instance: "gesture-1", serial: "1" });
            const waitsAfterTerminal = h.gestureCalls.filter((c) => c.method === "WaitGesture").length;
            // The ack callback never fires (host kills the call): bounded fast retries follow.
            h.fireDelay(2000);
            const retries1 = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(retries1.length, 2, "first bounded ack retry");
            h.fireDelay(2000);
            const retries2 = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(retries2.length, 3, "second bounded ack retry");
            // Fast exhaustion never abandons the core-settled ack: no new
            // waiter arms while the terminal is unacked, and the exact
            // pending ack persists.
            h.fireDelay(2000);
            assert.equal(
                h.gestureCalls.filter((c) => c.method === "AckGesture").length,
                3,
                "fast retries stay bounded (no hot loop, no abandon)",
            );
            const waitsHeld = h.gestureCalls.filter((c) => c.method === "WaitGesture");
            assert.equal(waitsHeld.length, waitsAfterTerminal, "waiter stays down while the terminal is unacked");
            // The 25 s host-deadline cadence retries the exact pending ack;
            // transport recovery then succeeds without reset: the native
            // answer adopts and the waiter re-arms for the next gesture.
            h.fireDelay(25000);
            const hostRetries = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(hostRetries.length, 4, "host-deadline ack retry");
            assert.deepEqual(JSON.parse(hostRetries[3]?.payload as string), { instance: "gesture-1", serial: "1" });
            hostRetries[3]?.callback(gestureSnapshot({
                serial: "0", revision: "3", phase: "idle", win: "", kind: "",
                startRect: null, startCursor: null, cursor: null,
            }));
            h.fireDelay(120);
            const waitsAfter = h.gestureCalls.filter((c) => c.method === "WaitGesture");
            assert.ok(waitsAfter.length >= 2, "waiter re-arms after the native ack lands");
            // A later native gesture binds on the same generation (no reset).
            const waitNext = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(waitNext !== undefined);
            waitNext.callback(gestureSnapshot({ serial: "2", revision: "4", phase: "started" }));
            h.fireDelay(120);
            const latest = last(h.calls.filter((c) => c.method === "DescribeWorld"));
            assert.ok(latest !== undefined);
            assert.equal((rawReport(latest.payload)["nativeGesture"] as Record<string, unknown>)["serial"], "2");
        } finally {
            h.stop();
        }
    });

    it("adopts the ack-next snapshot even when changed is 0", () => {
        const h = startCutoverHarness({
            settledHook: (report, assoc) => {
                const raw = report["nativeGesture"] as Record<string, unknown> | null;
                if (raw !== null && (raw["phase"] === "released" || raw["phase"] === "cancelled")) {
                    assert.ok(assoc !== null);
                    return { id: (assoc as { coreId: string }).coreId, committed: true };
                }
                return null;
            },
        });
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "1", revision: "1", phase: "started" }));
            h.fireDelay(120);
            const wait2 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait2 !== undefined);
            wait2.callback(gestureSnapshot({ serial: "1", revision: "2", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            const ack = h.gestureCalls.find((c) => c.method === "AckGesture");
            assert.ok(ack !== undefined);
            const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
            // Native exposes the next terminal with changed:0 (the ack return
            // shape): the entry still adopts it and dispatches.
            ack.callback(gestureSnapshot({
                serial: "2", revision: "3", phase: "released", cursor: [40, 40],
                changed: 0, startRect: [0, 0, 200, 200],
            }));
            h.fireDelay(120);
            const describesAfter = h.calls.filter((c) => c.method === "DescribeWorld").length;
            assert.ok(describesAfter > describesBefore, "ack-next terminal dispatches despite changed:0");
            const latest = last(h.calls.filter((c) => c.method === "DescribeWorld"));
            assert.ok(latest !== undefined);
            const raw = rawReport(latest.payload)["nativeGesture"] as Record<string, unknown>;
            assert.equal(raw["serial"], "2");
            assert.equal(raw["phase"], "released");
            const secondAck = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(secondAck.length, 2, "next terminal acks after its own settlement");
            assert.deepEqual(JSON.parse(secondAck[1]?.payload as string), { instance: "gesture-1", serial: "2" });
        } finally {
            h.stop();
        }
    });

    it("stale A never becomes script B; native-only B maps fresh", () => {
        const h = startCutoverHarness({
            settledHook: (report, assoc) => {
                const raw = report["nativeGesture"] as Record<string, unknown> | null;
                if (raw !== null && raw["serial"] === "9" && (raw["phase"] === "released" || raw["phase"] === "cancelled")) {
                    assert.ok(assoc !== null);
                    return { id: (assoc as { coreId: string }).coreId, committed: false };
                }
                return null;
            },
        });
        try {
            // Stale old witness A arrives first (old serial, terminal).
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "9", revision: "41", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            const acksA = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(acksA.length, 1, "stale terminal still acks by exact identity");
            assert.deepEqual(JSON.parse(acksA[0]?.payload as string), { instance: "gesture-1", serial: "9" });
            // The ack exposes idle: B must come from native only, never from
            // script signals. No new waiter, no B identity exists yet.
            const ackA = last(acksA);
            assert.ok(ackA !== undefined);
            ackA.callback(gestureSnapshot({
                serial: "0", revision: "42", phase: "idle", win: "", kind: "",
                startRect: null, startCursor: null, cursor: null,
            }));
            h.fireDelay(120);
            const waitsBefore = h.gestureCalls.filter((c) => c.method === "WaitGesture").length;
            // A script START signal alone (no native snapshot) mints nothing:
            // no new waiter fires, no ack, no B identity.
            h.fireScriptStart();
            h.fireDelay(120);
            assert.equal(
                h.gestureCalls.filter((c) => c.method === "WaitGesture").length,
                waitsBefore,
                "script START mints no gesture identity",
            );
            assert.equal(h.gestureCalls.filter((c) => c.method === "AckGesture").length, 1);
            // Native-only B arrives fresh with a distinct identity.
            const waitB = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(waitB !== undefined);
            waitB.callback(gestureSnapshot({ serial: "10", revision: "43", phase: "started" }));
            h.fireDelay(120);
            const latest = last(h.calls.filter((c) => c.method === "DescribeWorld"));
            assert.ok(latest !== undefined);
            const raw = rawReport(latest.payload)["nativeGesture"] as Record<string, unknown>;
            assert.equal(raw["serial"], "10");
            assert.equal(raw["phase"], "started");
            assert.equal(rawReport(latest.payload)["gestureIngress"], null);
        } finally {
            h.stop();
        }
    });

    it("unavailable interval recovers through the entry without map reset", () => {
        const h = startCutoverHarness({
            settledHook: (report, assoc) => {
                const raw = report["nativeGesture"] as Record<string, unknown> | null;
                if (raw !== null && raw["serial"] === "6" && (raw["phase"] === "released" || raw["phase"] === "cancelled")) {
                    assert.ok(assoc !== null);
                    return { id: (assoc as { coreId: string }).coreId, committed: false };
                }
                return null;
            },
        });
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "5", revision: "9", phase: "started" }));
            h.fireDelay(120);
            let latest = last(h.calls.filter((c) => c.method === "DescribeWorld"));
            assert.ok(latest !== undefined);
            assert.equal((rawReport(latest.payload)["nativeGesture"] as Record<string, unknown>)["serial"], "5");
            // Explicit effect teardown: the lane omits truthfully.
            const wait2 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait2 !== undefined);
            wait2.callback(gestureSnapshot({ available: 0 }));
            h.fireDelay(120);
            latest = last(h.calls.filter((c) => c.method === "DescribeWorld"));
            assert.ok(latest !== undefined);
            assert.equal(rawReport(latest.payload)["nativeGesture"], null);
            assert.equal(rawReport(latest.payload)["gestureIngress"], null);
            // The lane recovers without reset: the next serial maps and its
            // terminal settles by exact identity. The recovery chain owns
            // the re-arm (host-deadline cadence, never a hot loop).
            h.fireDelay(25000);
            const wait3 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait3 !== undefined);
            wait3.callback(gestureSnapshot({ serial: "6", revision: "10", phase: "started" }));
            h.fireDelay(120);
            const wait4 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait4 !== undefined);
            wait4.callback(gestureSnapshot({ serial: "6", revision: "11", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            const acks = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(acks.length, 1);
            assert.deepEqual(JSON.parse(acks[0]?.payload as string), { instance: "gesture-1", serial: "6" });
        } finally {
            h.stop();
        }
    });

    it("ignores a late ack callback after stop and after supersession", () => {
        const h = startCutoverHarness({
            settledHook: (_report, assoc) => {
                assert.ok(assoc !== null);
                return { id: (assoc as { coreId: string }).coreId, committed: false };
            },
        });
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "1", revision: "1", phase: "started" }));
            h.fireDelay(120);
            const wait2 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait2 !== undefined);
            wait2.callback(gestureSnapshot({ serial: "1", revision: "2", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            const ack = h.gestureCalls.find((c) => c.method === "AckGesture");
            assert.ok(ack !== undefined);
            const waitsBefore = h.gestureCalls.filter((c) => c.method === "WaitGesture").length;
            const describesBefore = h.calls.filter((c) => c.method === "DescribeWorld").length;
            // A superseding newer ack supersedes the older callback: force a
            // timeout retry, then deliver the stale first-attempt callback.
            h.fireDelay(2000);
            const retries = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(retries.length, 2, "bounded retry arms the superseding nonce");
            ack.callback(gestureSnapshot({
                serial: "0", revision: "3", phase: "idle", win: "", kind: "",
                startRect: null, startCursor: null, cursor: null,
            }));
            assert.equal(
                h.gestureCalls.filter((c) => c.method === "WaitGesture").length,
                waitsBefore,
                "stale ack callback changes nothing",
            );
            assert.equal(
                h.calls.filter((c) => c.method === "DescribeWorld").length,
                describesBefore,
                "stale ack callback dispatches nothing",
            );
            // And after stop the newest callback is equally inert.
            const current = last(h.gestureCalls.filter((c) => c.method === "AckGesture"));
            assert.ok(current !== undefined);
            h.stop();
            current.callback(gestureSnapshot({
                serial: "0", revision: "4", phase: "idle", win: "", kind: "",
                startRect: null, startCursor: null, cursor: null,
            }));
            assert.equal(
                h.calls.filter((c) => c.method === "DescribeWorld").length,
                describesBefore,
                "post-stop ack callback dispatches nothing",
            );
        } finally {
            h.stop();
        }
    });

    it("recovers a lost terminal capture with the identical envelope, then acks once", () => {
        // A: the commandless terminal capture reply is lost (timeout, then
        // transport loss on the retry). The entry redelivers the
        // byte-identical envelope on a bounded cadence with the same
        // immutable terminal/source identity (never a new source, never a
        // new terminal, no unrelated window events), the settlement lands,
        // and the native ack fires exactly once.
        const h = startCutoverHarness({
            manualDescribe: true,
            settledHook: (report, assoc) => {
                const raw = report["nativeGesture"] as Record<string, unknown> | null;
                if (raw !== null && (raw["phase"] === "released" || raw["phase"] === "cancelled")) {
                    assert.ok(assoc !== null);
                    return { id: (assoc as { coreId: string }).coreId, committed: true };
                }
                return null;
            },
        });
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "1", revision: "1", phase: "started" }));
            h.fireDelay(120);
            assert.equal(h.heldDescribes.length, 1, "started capture held");
            h.answerHeld(0);
            h.fireDelay(120);
            assert.equal(h.heldDescribes.length, 1, "started redispatch held");
            h.answerHeld(0);
            h.fireDelay(120);
            assert.equal(h.heldDescribes.length, 0, "wire free before the terminal");
            const wait2 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait2 !== undefined);
            wait2.callback(gestureSnapshot({ serial: "1", revision: "2", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            assert.equal(h.heldDescribes.length, 1, "terminal capture held");
            const firstBytes = h.heldDescribes[0]?.payload as string;
            const firstSource = (JSON.parse(firstBytes) as Record<string, unknown>)["source"];
            // The reply is lost: withhold the callback, then let the
            // timeout fire. No unrelated window events land meanwhile.
            // Each bounded retry costs two sweeps (send timeout, then the
            // identical-envelope send).
            h.fireDelay(2000);
            assert.equal(h.heldDescribes.length, 1, "timeout arms the retry without a new source");
            h.fireDelay(2000);
            assert.equal(h.heldDescribes.length, 2, "identical envelope redelivered");
            const retryBytes = h.heldDescribes[1]?.payload as string;
            assert.equal(retryBytes, firstBytes, "retry reuses the immutable terminal/source identity");
            assert.equal(h.gestureCalls.filter((c) => c.method === "AckGesture").length, 0, "no ack before settlement");
            // Lose the retry reply too: one more bounded identical send.
            h.fireDelay(2000);
            h.fireDelay(2000);
            assert.equal(h.heldDescribes.length, 3, "second bounded retry");
            assert.equal(h.heldDescribes[2]?.payload as string, firstBytes, "still the same envelope");
            // The settlement lands on the retry: the native ack fires once.
            h.answerHeld(2);
            const acks = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(acks.length, 1, "terminal acks exactly once after recovery");
            assert.deepEqual(JSON.parse(acks[0]?.payload as string), { instance: "gesture-1", serial: "1" });
            // The two stranded callbacks land late: both are inert (no
            // second ack, no double commit, no new capture storm).
            h.answerHeld(0);
            h.answerHeld(0);
            assert.equal(h.gestureCalls.filter((c) => c.method === "AckGesture").length, 1, "late replies never re-ack");
            // The pending native ack still completes the lifecycle.
            const pending = last(h.gestureCalls.filter((c) => c.method === "AckGesture"));
            assert.ok(pending !== undefined);
            pending.callback(gestureSnapshot({
                serial: "0", revision: "3", phase: "idle", win: "", kind: "",
                startRect: null, startCursor: null, cursor: null,
            }));
            h.fireDelay(120);
            assert.ok(h.gestureCalls.filter((c) => c.method === "WaitGesture").length >= 2, "waiter re-arms");
            void firstSource;
        } finally {
            h.stop();
        }
    });

    it("never acks an unrelated settled id for the current ingress", () => {
        // F: the broker retains a close settlement for another identity
        // while the current ingress omits or settles differently. The
        // script requires GestureSettled.id === association.coreId AND the
        // flight identity === association native pair, so the unrelated id
        // never acks the current terminal.
        const h = startCutoverHarness({
            settledHook: () => ({ id: "99", committed: false }),
        });
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "1", revision: "1", phase: "started" }));
            h.fireDelay(120);
            const wait2 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait2 !== undefined);
            wait2.callback(gestureSnapshot({ serial: "1", revision: "2", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            assert.equal(
                h.gestureCalls.filter((c) => c.method === "AckGesture").length,
                0,
                "unrelated settled id never acks the current terminal",
            );
        } finally {
            h.stop();
        }
    });

    it("exposes the proven open identity with its broker core id", () => {
        // F: after an accepted open capture the proven bound association is
        // available (not terminal-only). The harness proves the reply path;
        // the entry proves the binding by acking the later terminal with
        // the same core id the open carried.
        const seen: Array<{ instance: string; serial: string; coreId: string } | null> = [];
        const h = startCutoverHarness({
            settledHook: (report, assoc) => {
                const raw = report["nativeGesture"] as Record<string, unknown> | null;
                if (assoc !== null) {
                    seen.push({ ...(assoc as { instance: string; serial: string; coreId: string }) });
                }
                if (raw !== null && (raw["phase"] === "released" || raw["phase"] === "cancelled")) {
                    assert.ok(assoc !== null);
                    return { id: (assoc as { coreId: string }).coreId, committed: true };
                }
                return null;
            },
        });
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "7", revision: "1", phase: "updated" }));
            h.fireDelay(120);
            assert.ok(seen.length >= 1, "open accepted with a proven association");
            const openAssoc = seen[0] as { instance: string; serial: string; coreId: string };
            assert.equal(openAssoc.instance, "gesture-1");
            assert.equal(openAssoc.serial, "7");
            const wait2 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait2 !== undefined);
            wait2.callback(gestureSnapshot({ serial: "7", revision: "2", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            const ack = h.gestureCalls.find((c) => c.method === "AckGesture");
            assert.ok(ack !== undefined, "terminal acks on the proven binding");
            assert.deepEqual(JSON.parse(ack.payload as string), { instance: "gesture-1", serial: "7" });
        } finally {
            h.stop();
        }
    });

    it("REGRESSION: four straight terminal capture losses stay bounded then use the host cadence", () => {
        // Desired: initial + at most 2 fast same-envelope retries, then no
        // more 2 s sends; a 25 s host-cadence timer redelivers the identical
        // envelope; settlement then acks exactly once with no double
        // commit. See kwin/src/world-entry.ts:2982-3044 (maybeRetry arming)
        // and :3049-3071 (transmit timeout). Current defect: the retry
        // timer callback clears terminalCaptureRetry (:3018) before the
        // redelivery, so the next timeout sees prev=null (:3000) and the
        // attempt count resets to 1 forever (cap 2 never reached).
        const h = startCutoverHarness({
            manualDescribe: true,
            settledHook: (report, assoc) => {
                const raw = report["nativeGesture"] as Record<string, unknown> | null;
                if (raw !== null && (raw["phase"] === "released" || raw["phase"] === "cancelled")) {
                    assert.ok(assoc !== null);
                    return { id: (assoc as { coreId: string }).coreId, committed: true };
                }
                return null;
            },
        });
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "1", revision: "1", phase: "started" }));
            h.fireDelay(120);
            h.answerHeld(0);
            h.fireDelay(120);
            h.answerHeld(0);
            h.fireDelay(120);
            assert.equal(h.heldDescribes.length, 0, "wire free before the terminal");
            const wait2 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait2 !== undefined);
            wait2.callback(gestureSnapshot({ serial: "1", revision: "2", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            assert.equal(h.heldDescribes.length, 1, "terminal capture held");
            const firstBytes = h.heldDescribes[0]?.payload as string;
            const sameCount = (): number =>
                h.heldDescribes.filter((d) => d.payload === firstBytes).length;
            // Lose the initial + first retry: two bounded identical sends.
            h.fireDelay(2000);
            assert.equal(h.heldDescribes.length, 1, "send timeout arms without a new source");
            h.fireDelay(2000);
            assert.equal(h.heldDescribes.length, 2, "first bounded retry");
            assert.equal(sameCount(), 2, "retry reuses the immutable envelope");
            h.fireDelay(2000);
            h.fireDelay(2000);
            assert.equal(h.heldDescribes.length, 3, "second bounded retry");
            assert.equal(sameCount(), 3, "still the same envelope");
            // Beyond the bound the fast cadence must stop: four more 2 s
            // sweeps (two full timeout+send cycles) add nothing.
            const heldBefore = h.heldDescribes.length;
            h.fireDelay(2000);
            h.fireDelay(2000);
            h.fireDelay(2000);
            h.fireDelay(2000);
            console.log(`[retry-proof] after-4-losses held=${h.heldDescribes.length} expected=${heldBefore} describes=${h.calls.filter((c) => c.method === "DescribeWorld").length}`);
            assert.equal(h.heldDescribes.length, heldBefore, "fast retries stay bounded past 2 (no unlimited 2 s loop)");
            // The host cadence redelivers the identical envelope once.
            h.fireDelay(25000);
            console.log(`[retry-proof] after-host-cadence held=${h.heldDescribes.length} describes=${h.calls.filter((c) => c.method === "DescribeWorld").length}`);
            assert.equal(h.heldDescribes.length, heldBefore + 1, "host cadence redelivers once");
            assert.equal(h.heldDescribes[heldBefore]?.payload as string, firstBytes, "host retry keeps the exact envelope");
            // Settlement lands on the host retry: one exact ack, no double commit.
            h.answerHeld(heldBefore);
            const acks = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(acks.length, 1, "terminal acks exactly once after recovery");
            assert.deepEqual(JSON.parse(acks[0]?.payload as string), { instance: "gesture-1", serial: "1" });
            while (h.heldDescribes.length > 0) {
                h.answerHeld(0);
            }
            assert.equal(h.gestureCalls.filter((c) => c.method === "AckGesture").length, 1, "late stranded replies never re-ack");
            const pending = last(h.gestureCalls.filter((c) => c.method === "AckGesture"));
            assert.ok(pending !== undefined);
            pending.callback(gestureSnapshot({
                serial: "0", revision: "3", phase: "idle", win: "", kind: "",
                startRect: null, startCursor: null, cursor: null,
            }));
            h.fireDelay(120);
            assert.ok(h.gestureCalls.filter((c) => c.method === "WaitGesture").length >= 2, "waiter re-arms");
        } finally {
            h.stop();
        }
    });

    it("REGRESSION: malformed terminal reply still retries the same envelope", () => {
        // Desired: a non-string/malformed DescribeWorld reply for the
        // terminal flight still arms the bounded same-envelope retry (the
        // terminal stays pending, no wedge). See
        // kwin/src/world-entry.ts:3082-3111: lastResolvedCaptureSource is
        // stamped before the typeof/parse checks, so the maybeRetry guard
        // at :2986 returns immediately and no retry is ever armed.
        const h = startCutoverHarness({
            manualDescribe: true,
            settledHook: (report, assoc) => {
                const raw = report["nativeGesture"] as Record<string, unknown> | null;
                if (raw !== null && (raw["phase"] === "released" || raw["phase"] === "cancelled")) {
                    assert.ok(assoc !== null);
                    return { id: (assoc as { coreId: string }).coreId, committed: true };
                }
                return null;
            },
        });
        try {
            const wait = h.gestureCalls.find((c) => c.method === "WaitGesture");
            assert.ok(wait !== undefined);
            wait.callback(gestureSnapshot({ serial: "1", revision: "1", phase: "started" }));
            h.fireDelay(120);
            h.answerHeld(0);
            h.fireDelay(120);
            h.answerHeld(0);
            h.fireDelay(120);
            assert.equal(h.heldDescribes.length, 0, "wire free before the terminal");
            const wait2 = last(h.gestureCalls.filter((c) => c.method === "WaitGesture"));
            assert.ok(wait2 !== undefined);
            wait2.callback(gestureSnapshot({ serial: "1", revision: "2", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            assert.equal(h.heldDescribes.length, 1, "terminal capture held");
            const firstBytes = h.heldDescribes[0]?.payload as string;
            const held = h.heldDescribes.splice(0, 1)[0];
            assert.ok(held !== undefined);
            held.callback(42);
            assert.equal(h.gestureCalls.filter((c) => c.method === "AckGesture").length, 0, "no ack on malformed reply");
            h.fireDelay(2000);
            h.fireDelay(2000);
            console.log(`[retry-proof] malformed held=${h.heldDescribes.length} describes=${h.calls.filter((c) => c.method === "DescribeWorld").length}`);
            assert.equal(h.heldDescribes.length, 1, "malformed reply arms one same-envelope retry");
            assert.equal(h.heldDescribes[0]?.payload as string, firstBytes, "malformed retry keeps the exact envelope");
            h.answerHeld(0);
            const acks = h.gestureCalls.filter((c) => c.method === "AckGesture");
            assert.equal(acks.length, 1, "terminal acks exactly once after malformed recovery");
            assert.deepEqual(JSON.parse(acks[0]?.payload as string), { instance: "gesture-1", serial: "1" });
        } finally {
            h.stop();
        }
    });
});
