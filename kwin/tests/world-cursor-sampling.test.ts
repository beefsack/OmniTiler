// Stage 6 cursor sampling and warp defense (D5): the adapter live-samples
// cursorPos and calls windowAt on every event/command/gesture capture only.
// Pure pointer motion triggers no capture and no enabled behavior (no
// cursorPosChanged subscription, no polling timer), and the moved position
// rides the next relevant capture. A nonnull warpCursor reply is unexpected:
// the adapter logs and ignores it, performs no write, and the capability
// stays false in the next capture.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

interface FakeSignal {
    handlers: Array<() => void>;
    signal: { connect: (handler: () => void) => void; disconnect: (handler: () => void) => void };
}

function fakeSignal(): FakeSignal {
    const handlers: Array<() => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: () => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: () => void): void => {
                const index = handlers.indexOf(handler);
                if (index >= 0) {
                    handlers.splice(index, 1);
                }
            },
        },
    };
}

interface Call {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

const GEN = "gen-1";

function acceptedReply(source: string, nativeInvalid: string, rev: string, extra: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 3, adapter: "broker-1", gen: GEN, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source, nativeInvalid, clock: "1",
        gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [],
        warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        scopes: [{ scope: "out/ws1", auth: "controlled", reason: "" }],
        workspaces: [],
        outputs: [],
        windows: [],
        transfers: [],
        effects: [],
        result: null,
        ...extra,
    });
}

function bootstrap(req: string): string {
    return JSON.stringify({
        v: 3, adapter: "broker-1", gen: GEN, req, obs: "0", rev: "0",
        ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
        windows: [], transfers: [], effects: [], result: null,
        gestureSettled: null, operationSettled: [], nativeCreations: [], warpCursor: null,
        presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
    });
}

function makeHarness() {
    const ws1 = { id: "ws1", name: "ws1" };
    const out = { name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } };
    const w1 = {
        internalId: "win-a", managed: true, deleted: false, transient: false, transientFor: null,
        dialog: false, modal: false, wantsInput: true, normalWindow: true,
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
        output: { name: "out" },
        desktops: [ws1],
        minimized: false, hidden: false, fullScreen: false, maximizeMode: 0, onAllDesktops: false,
        minSize: null, maxSize: null,
    };
    const wins = [w1];
    const cursor = { pos: { x: 2, y: 100 }, at: null as null | ((pos: { x: number; y: number }) => unknown) };
    const signals: Record<string, FakeSignal> = {
        windowAdded: fakeSignal(),
        windowRemoved: fakeSignal(),
        windowActivated: fakeSignal(),
        desktopsChanged: fakeSignal(),
        screensChanged: fakeSignal(),
        currentDesktopChanged: fakeSignal(),
        activeScreenChanged: fakeSignal(),
    };
    const calls: Call[] = [];
    const writes: Array<{ prop: string | symbol; value: unknown }> = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    const logs: string[] = [];
    const reads: string[] = [];
    const target = {
        windowList: () => wins,
        screens: [out],
        desktops: [ws1],
        activeScreen: { name: "out" },
        get cursorPos() {
            return cursor.pos;
        },
        get activeWindow() {
            return w1;
        },
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1040 }),
        currentDesktop: ws1,
        currentDesktopForScreen: () => ws1,
        get windowAt() {
            return cursor.at;
        },
        windowAdded: (signals.windowAdded as FakeSignal).signal,
        windowRemoved: (signals.windowRemoved as FakeSignal).signal,
        windowActivated: (signals.windowActivated as FakeSignal).signal,
        desktopsChanged: (signals.desktopsChanged as FakeSignal).signal,
        screensChanged: (signals.screensChanged as FakeSignal).signal,
        currentDesktopChanged: (signals.currentDesktopChanged as FakeSignal).signal,
        activeScreenChanged: (signals.activeScreenChanged as FakeSignal).signal,
    };
    const surface = new Proxy(target, {
        get(t, p, r) {
            if (typeof p === "string") {
                reads.push(p);
            }
            return Reflect.get(t, p, r);
        },
        set(t, p, v, r) {
            writes.push({ prop: p, value: v });
            return Reflect.set(t, p, v, r);
        },
    });
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "QueueWorldCommand") {
                const body = JSON.parse(payload) as Record<string, unknown>;
                callback(JSON.stringify({ v: 3, outcome: "queued", gen: GEN,
                    client: body["client"], clientCmd: body["clientCmd"], globalCmd: "1" }));
            } else if (method === "TakeCommandResult") {
                callback(JSON.stringify({ v: 3, outcome: "pending", gen: GEN }));
            }
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (message) => {
            logs.push(message);
        },
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    const runTimers = (maxDelay = 120): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as { delay: number; cb: () => void };
            if (t.delay <= maxDelay) {
                timers.splice(i, 1);
                i -= 1;
                t.cb();
            }
        }
    };
    const pendingDescribes = (): Call[] => calls.filter((c) => c.method === "DescribeWorld");
    const answerDescribes = (extra: Record<string, unknown> = {}): void => {
        for (let i = calls.length - 1; i >= 0; i -= 1) {
            const call = calls[i] as Call;
            if (call.method !== "DescribeWorld") {
                continue;
            }
            calls.splice(i, 1);
            const body = JSON.parse(call.payload) as { source: string; nativeInvalid: string };
            rev += 1;
            call.callback(acceptedReply(body.source, body.nativeInvalid, String(rev), extra));
        }
    };
    let rev = 0;
    const fire = (name: string): void => {
        for (const handler of [...(signals[name] as FakeSignal).handlers]) {
            handler();
        }
    };
    // Bind: answer discovery, then settle the auto-dispatched observation.
    const discoverCall = [...calls].reverse().find((c) => c.method === "DiscoverWorld");
    assert.ok(discoverCall !== undefined, "expected DiscoverWorld");
    const discoverBody = JSON.parse(discoverCall.payload) as { req: string };
    calls.splice(calls.indexOf(discoverCall), 1);
    discoverCall.callback(bootstrap(discoverBody.req));
    runTimers();
    answerDescribes();
    runTimers();
    assert.ok(timers.every((t) => t.delay >= 2000), "idle timers are held-call fences only");
    return { handle, calls, writes, timers, logs, reads, signals, cursor, wins, runTimers, pendingDescribes, answerDescribes, fire, revOf: () => rev };
}

function reportOf(call: Call): Record<string, unknown> {
    return (JSON.parse(call.payload) as { report: Record<string, unknown> }).report;
}

describe("world entry cursor sampling and warp defense", () => {
    it("never subscribes to cursorPosChanged and runs no polling timer at idle", () => {
        const h = makeHarness();
        assert.ok(!h.reads.includes("cursorPosChanged"), "no cursorPosChanged subscription read");
        // Idle timers are only transport timeouts (2000) and host-deadline
        // watchdogs (25000) for held calls: no debounce (120), no drain (0),
        // no polling cadence.
        assert.ok(h.timers.length > 0, "held-call timers exist at idle");
        assert.ok(h.timers.every((t) => t.delay >= 2000), `no short-cadence timer at idle: ${h.timers.map((t) => t.delay).join(",")}`);
        const timersBefore = h.timers.map((t) => t.delay);
        const callsBefore = h.calls.length;
        // Pure pointer motion alone schedules nothing and captures nothing.
        h.cursor.pos = { x: 900, y: 700 };
        h.runTimers();
        assert.deepEqual(h.timers.map((t) => t.delay), timersBefore, "motion alone arms no timer");
        assert.equal(h.calls.length, callsBefore, "motion alone emits no transport");
        assert.equal(h.pendingDescribes().length, 0, "motion alone captures nothing");
    });

    it("samples moved cursor and windowAt on the next relevant event capture", () => {
        const h = makeHarness();
        h.cursor.pos = { x: 321, y: 77 };
        h.cursor.at = () => [{ internalId: "win-a" }];
        h.fire("windowActivated");
        h.runTimers();
        const describes = h.pendingDescribes();
        assert.equal(describes.length, 1, "event triggers exactly one capture");
        const report = reportOf(describes[0] as Call);
        assert.deepEqual(report["cursor"], [321, 77]);
        assert.equal(report["cursorWindow"], "win-a");
        assert.equal(report["cursorWarp"], false);
        h.answerDescribes();
        h.runTimers();
    });

    it("samples moved cursor on the next command capture", () => {
        const h = makeHarness();
        h.cursor.pos = { x: 55, y: 66 };
        h.cursor.at = () => ({ length: 1, 0: { internalId: "win-a" } });
        h.handle.requestFocus("left");
        const describes = h.pendingDescribes();
        assert.ok(describes.length >= 1, "command triggers a capture");
        const report = reportOf(describes[describes.length - 1] as Call);
        assert.deepEqual(report["cursor"], [55, 66]);
        assert.equal(report["cursorWindow"], "win-a");
        h.answerDescribes();
        h.runTimers();
    });

    it("logs and ignores an unexpected nonnull warp reply with no write, staying false", () => {
        const h = makeHarness();
        // Baseline: the real core reply carries no warp while the capability
        // reads false.
        h.fire("windowActivated");
        h.runTimers();
        assert.equal(h.pendingDescribes().length, 1);
        h.answerDescribes();
        h.runTimers();
        // Injected nonnull warp on the next reply stays labeled unexpected.
        h.cursor.pos = { x: 321, y: 77 };
        h.fire("windowActivated");
        h.runTimers();
        assert.equal(h.pendingDescribes().length, 1);
        h.answerDescribes({ warpCursor: [50, 60] });
        h.runTimers();
        const warpLines = h.logs.filter((line) => line.indexOf("warp-desired ignored") !== -1);
        assert.equal(warpLines.length, 1, "exactly one bounded warp-ignored line");
        assert.ok((warpLines[0] as string).includes("x=50 y=60"), warpLines[0] as string);
        assert.equal(h.writes.length, 0, "no cursor write through the workspace surface");
        assert.deepEqual(h.cursor.pos, { x: 321, y: 77 }, "cursor position unchanged");
        // The capability stays false in the next capture.
        h.fire("windowActivated");
        h.runTimers();
        const next = h.pendingDescribes();
        assert.equal(next.length, 1);
        assert.equal(reportOf(next[0] as Call)["cursorWarp"], false);
        h.answerDescribes();
        h.runTimers();
    });
});
