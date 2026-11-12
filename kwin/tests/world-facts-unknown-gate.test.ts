// Content-gated unknown transitions for the effect + gesture facts lanes.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface Call {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

interface FactCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

interface Timer {
    delay: number;
    cb: () => void;
    cancelled: boolean;
}

function makeSurface() {
    const ws1 = { id: "ws1" };
    const ws2 = { id: "ws2" };
    const wins = [
        {
            internalId: "win-a", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws1],
            minimized: false, hidden: false, fullscreen: false, maximized: false, sticky: false,
            keepAbove: false, keepBelow: false,
            minSize: { width: 100, height: 100 },
            maxSize: { width: 2147483647, height: 2147483647 },
            pid: 1234,
            resourceClass: "Example",
            caption: "Window",
            windowRole: "normal",
            desktopFileName: "example.app",
            resizeable: true,
        },
        {
            internalId: "win-b", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws2],
            minimized: true, hidden: false, fullscreen: false, maximized: false, sticky: false,
            keepAbove: false, keepBelow: false,
            minSize: { width: 100, height: 100 },
            maxSize: { width: 2147483647, height: 2147483647 },
            pid: 1235,
            resourceClass: "Example",
            caption: "Second",
            windowRole: "normal",
            desktopFileName: "example.app",
            resizeable: true,
        },
    ];
    const screens = [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const desktops = [ws1, ws2];
    const world = {
        screens, desktops, wins,
        activeScreen: { name: "out" },
        activeWindow: wins[0],
        currentDesktop: ws1,
    };
    const surface = {
        windowList: () => world.wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        get activeWindow() {
            return world.activeWindow;
        },
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1040 }),
        currentDesktopForScreen: () => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown) => {
            world.currentDesktop = deskRef as { id: string };
        },
        sendClientToOutput: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface };
}

// Genuine 18-key v4 window row (retired 13-key shape refuses).
function winRow(win: string, ws: string): Record<string, unknown> {
    return {
        win, out: "out", ws,
        alloc: [10, 10, 300, 300], frame: [10, 10, 300, 300],
        frame_auth: "controlled", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", floating: false, float_origin: "", clear_op: "",
        cmdSeq: null,
        visibility: { shown: true, rev: "1", fallback: false },
        visibility_auth: "controlled", visibility_hold: "",
        state: [],
    };
}

const EFFECT_SNAP = {
    instance: "effect-1",
    rev: "3",
    invalidation: "7",
    chord: 1,
    chordKnown: 1,
    gap: 0.0,
    width: 3.0,
    extension: -1.0,
};

function effectReply(over: Record<string, unknown> = {}): string {
    return JSON.stringify({ v: 1, changed: 0, superseded: 0, available: 1, ...EFFECT_SNAP, ...over });
}

function gestureStarted(over: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 1, instance: "gesture-1", serial: "1", revision: "1",
        phase: "started", win: "win-a", kind: "move",
        startRect: [0, 0, 200, 200], startCursor: [10, 10], cursor: [20, 20],
        modalMove: 1, changed: 1, superseded: 0, available: 1, ...over,
    });
}

function startFlow() {
    const { surface } = makeSurface();
    const calls: Call[] = [];
    const timers: Timer[] = [];
    const logs: string[] = [];
    const effectCalls: FactCall[] = [];
    const gestureCalls: FactCall[] = [];
    let rev = "0";
    let clock = "0";
    const bump = (text: string): string => {
        const digits = text.split("");
        for (let i = digits.length - 1; i >= 0; i -= 1) {
            const d = (digits[i] as string).charCodeAt(0) - 48 + 1;
            if (d <= 9) {
                digits[i] = String(d);
                return digits.join("");
            }
            digits[i] = "0";
        }
        return `1${digits.join("")}`;
    };
    // Genuine strict v4 capture accept: 18-key rows, object scopes,
    // switch/diagnostic lanes, anchored presentation, broker-bound
    // source/watermark/clock correlation. Every authored accept is
    // verified with the production parser before the callback runs, so a
    // refused shape fails loudly instead of silently inflating counts.
    // rev strictly increases so every accept applies (never stale-rev).
    const acceptCapture = (gen: string, envelope: Record<string, unknown>): string => {
        rev = bump(rev);
        clock = bump(clock);
        return JSON.stringify({
            v: 4, adapter: "test-adapter", gen, req: envelope["source"], obs: "1", rev,
            ready: true, consumed: false,
            scopes: [{ scope: { kind: "output", out: "out" }, auth: "controlled", reason: "" }],
            workspaces: [
                { id: "ws1", out: "out", bind: "", scope: { kind: "output", out: "out" }, order: 0, mode: "tiled", exist: true, hold: "" },
                { id: "ws2", out: "out", bind: "", scope: { kind: "output", out: "out" }, order: 1, mode: "tiled", exist: true, hold: "" },
            ],
            outputs: [{ out: "out", view: "ws1", mode: "tiled", hold: "" }],
            windows: [winRow("win-a", "ws1"), winRow("win-b", "ws2")],
            transfers: [], effects: [], result: null,
            gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [], warpCursor: null,
            intentDesired: [],
            presentation: DISABLED_PRESENTATION,
            switchSettled: [], diagnostics: [],
            source: envelope["source"], nativeInvalid: envelope["nativeInvalid"], clock,
        });
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            if (method === "DescribeWorld") {
                calls.push({ method, payload, callback });
                const envelope = JSON.parse(payload) as Record<string, unknown>;
                const text = acceptCapture("gen-1", envelope);
                const parsed = parseWorldReply(text);
                assert.ok(isCoreWorldReply(parsed), "authored capture accept binds strict v4");
                callback(text);
                return;
            }
            // Handshake/settings (DiscoverWorld) are answered manually by
            // the test; presentation reads stay unanswered (their watchdog
            // is skipped by the timer helpers).
            calls.push({ method, payload, callback });
        },
        effectCallDbus: (_s, _p, _i, method, payload, callback) => {
            effectCalls.push({ method, payload, callback });
        },
        gestureCallDbus: (_s, _p, _i, method, payload, callback) => {
            gestureCalls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            const t: Timer = { delay, cb, cancelled: false };
            timers.push(t);
            return () => {
                t.cancelled = true;
            };
        },
        log: (m) => {
            logs.push(m);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    const isPresentationTimer = (t: Timer): boolean =>
        (t.cb as unknown as Record<string, unknown>)["presentationReadWatchdog"] === true;
    const pump = (maxDelay: number): void => {
        for (;;) {
            const at = timers.findIndex((t) => !t.cancelled && !isPresentationTimer(t) && t.delay <= maxDelay);
            if (at < 0) {
                return;
            }
            const next = timers.splice(at, 1)[0] as Timer;
            next.cb();
        }
    };
    // Fire every live 25 s host-deadline timer once (snapshot first so a
    // re-arm never cascades). Presentation read watchdogs are skipped.
    const fireHost = (): void => {
        const live = timers.filter((t) => !t.cancelled && !isPresentationTimer(t) && t.delay === 25000);
        for (const t of live) {
            t.cancelled = true;
            const at = timers.indexOf(t);
            if (at >= 0) {
                timers.splice(at, 1);
            }
        }
        for (const t of live) {
            t.cb();
        }
    };
    const lastCall = (method: string, type?: string): Call => {
        const found = [...calls].reverse().find((c) => {
            if (c.method !== method) {
                return false;
            }
            if (type === undefined) {
                return true;
            }
            try {
                return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === type;
            } catch (error) {
                void error;
                return false;
            }
        });
        assert.ok(found !== undefined, `expected ${method}${type === undefined ? "" : `/${type}`}`);
        return found as Call;
    };
    const answerHandshake = (gen = "gen-1"): void => {
        const disc = lastCall("DiscoverWorld", "handshake");
        const req = (JSON.parse(disc.payload) as Record<string, unknown>)["req"] as string;
        disc.callback(JSON.stringify({
            type: "handshake-reply", v: 4, adapter: "test-adapter", req,
            ready: false, kind: "", gen,
        }));
    };
    const answerSettings = (gen = "gen-1"): void => {
        const settings = lastCall("DiscoverWorld", "settings");
        const body = JSON.parse(settings.payload) as Record<string, unknown>;
        settings.callback(JSON.stringify({
            type: "settings-reply", v: 4, adapter: "test-adapter", gen,
            req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
        }));
    };
    // No reply-malformed / stale / mismatch / transport-error lines: every
    // counted capture was accepted and applied, never refused or stale.
    const assertHealthy = (): void => {
        for (const line of logs) {
            assert.ok(!line.includes("reply-malformed"), `refused accept: ${line}`);
            assert.ok(!line.includes("omnitiler:world:stale"), `stale accept: ${line}`);
            assert.ok(!line.includes("generation-mismatch"), `mismatch: ${line}`);
            assert.ok(!line.includes("transport-error"), `transport loss: ${line}`);
        }
    };
    const bind = (): void => {
        answerHandshake();
        answerSettings();
        pump(120);
        assertHealthy();
        assert.ok(describeCount() >= 1, "bind settles with an accepted capture");
        assert.equal(effectWaitCount(), 1, "effect waiter arms on bind");
        assert.equal(gestureWaitCount(), 1, "gesture waiter arms on bind");
    };
    const describeCount = (): number => calls.filter((c) => c.method === "DescribeWorld").length;
    const effectWaitCount = (): number => effectCalls.filter((c) => c.method === "WaitPresentationFacts").length;
    const gestureWaitCount = (): number => gestureCalls.filter((c) => c.method === "WaitGesture").length;
    const lastReport = (): Record<string, unknown> => {
        const call = [...calls].reverse().find((c) => c.method === "DescribeWorld");
        assert.ok(call !== undefined, "expected a capture");
        return (JSON.parse(call.payload) as { report: Record<string, unknown> }).report;
    };
    const countLogs = (needle: string): number => logs.filter((l) => l.includes(needle)).length;
    return {
        handle: handle as NonNullable<typeof handle>,
        calls, timers, logs, effectCalls, gestureCalls,
        pump, fireHost, bind, assertHealthy, describeCount, effectWaitCount, gestureWaitCount,
        lastReport, countLogs,
    };
}

describe("facts unknown-transition content gate (effect lane)", () => {
    it("repeated absent watchdog while initially unknown dispatches nothing but re-arms", () => {
        const flow = startFlow();
        flow.bind();
        const base = flow.describeCount();
        assert.deepEqual(flow.lastReport()["nativeInvalidWitness"], null);
        assert.equal((flow.lastReport()["presentation"] as Record<string, unknown>)["chordHeld"], false);
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.effectWaitCount(), 2, "unknown watchdog still re-arms for enabled-later recovery");
        assert.equal(flow.gestureWaitCount(), 2);
        assert.equal(flow.describeCount(), base, "already-unknown watchdog publishes no capture");
        assert.equal(flow.countLogs("effect-facts-host-timeout"), 0);
        assert.equal(flow.countLogs("gesture-facts-host-timeout"), 0);
        flow.assertHealthy();
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.effectWaitCount(), 3);
        assert.equal(flow.describeCount(), base, "steady unknown stays silent across deadlines");
        flow.assertHealthy();
        flow.handle.stop();
    });

    it("known->unknown publishes exactly one hide, then stays silent; valid recovery is adopted", () => {
        const flow = startFlow();
        flow.bind();
        const base = flow.describeCount();
        flow.effectCalls[0]?.callback(effectReply({ changed: 1 }));
        flow.pump(120);
        assert.equal(flow.effectWaitCount(), 2, "changed reply re-arms immediately");
        assert.equal(flow.describeCount(), base + 1);
        flow.assertHealthy();
        assert.equal((flow.lastReport()["presentation"] as Record<string, unknown>)["chordHeld"], true);
        assert.deepEqual(flow.lastReport()["nativeInvalidWitness"], { instance: "effect-1", counter: "7" });
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.effectWaitCount(), 3);
        assert.equal(flow.describeCount(), base + 2, "exactly one hide capture on known->unknown");
        flow.assertHealthy();
        assert.equal((flow.lastReport()["presentation"] as Record<string, unknown>)["chordHeld"], false);
        assert.deepEqual(flow.lastReport()["nativeInvalidWitness"], null);
        assert.equal(flow.countLogs("effect-facts-host-timeout"), 1);
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.effectWaitCount(), 4);
        assert.equal(flow.describeCount(), base + 2, "no second hide while already unknown");
        assert.equal(flow.countLogs("effect-facts-host-timeout"), 1);
        flow.effectCalls[3]?.callback(effectReply({ rev: "4", changed: 1 }));
        flow.pump(120);
        assert.equal(flow.describeCount(), base + 3, "enabled-later valid reply is adopted");
        flow.assertHealthy();
        assert.equal((flow.lastReport()["presentation"] as Record<string, unknown>)["chordHeld"], true);
        assert.deepEqual(flow.lastReport()["nativeInvalidWitness"], { instance: "effect-1", counter: "7" });
        flow.handle.stop();
    });

    it("malformed and unavailable replies while already unknown are silent but keep recovery", () => {
        const flow = startFlow();
        flow.bind();
        const base = flow.describeCount();
        const waits = flow.effectWaitCount();
        flow.effectCalls[waits - 1]?.callback(JSON.stringify({ v: 1, bogus: true }));
        flow.pump(120);
        assert.equal(flow.describeCount(), base, "malformed while unknown publishes nothing");
        assert.equal(flow.countLogs("effect-facts-malformed"), 0);
        flow.assertHealthy();
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.effectWaitCount(), waits + 1, "malformed lane still recovers on the host deadline");
        assert.equal(flow.describeCount(), base);
        flow.assertHealthy();
        const waits2 = flow.effectWaitCount();
        flow.effectCalls[waits2 - 1]?.callback(effectReply({ changed: 0, available: 0 }));
        flow.pump(120);
        assert.equal(flow.describeCount(), base, "unavailable while unknown publishes nothing");
        assert.equal(flow.countLogs("effect-facts-unavailable"), 0);
        flow.assertHealthy();
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.effectWaitCount(), waits2 + 1);
        assert.equal(flow.describeCount(), base);
        flow.assertHealthy();
        flow.effectCalls[waits2]?.callback(effectReply({ changed: 1 }));
        flow.pump(120);
        assert.equal(flow.describeCount(), base + 1, "lane adopts valid facts after silent intervals");
        flow.assertHealthy();
        assert.equal((flow.lastReport()["presentation"] as Record<string, unknown>)["chordHeld"], true);
        flow.handle.stop();
    });

    it("late callbacks after a watchdog re-arm and post-stop completions stay fenced", () => {
        const flow = startFlow();
        flow.bind();
        const base = flow.describeCount();
        const first = flow.effectCalls[0];
        assert.ok(first !== undefined);
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.describeCount(), base, "watchdog while unknown stays silent");
        const waits = flow.effectWaitCount();
        first.callback(effectReply({ changed: 1 }));
        flow.pump(120);
        assert.equal(flow.effectWaitCount(), waits, "settled late callback never re-arms");
        assert.equal(flow.describeCount(), base, "settled late callback never dispatches");
        flow.assertHealthy();
        flow.handle.stop();
        const live = flow.effectCalls[flow.effectCalls.length - 1];
        assert.ok(live !== undefined);
        live.callback(effectReply({ changed: 1 }));
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.describeCount(), base, "post-stop completions never dispatch");
        flow.handle.stop();
    });
});

describe("facts unknown-transition content gate (gesture lane)", () => {
    it("repeated absent watchdog while initially unknown dispatches nothing but re-arms", () => {
        const flow = startFlow();
        flow.bind();
        const base = flow.describeCount();
        assert.equal(flow.lastReport()["nativeGesture"], null);
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.gestureWaitCount(), 2, "unknown watchdog still re-arms for enabled-later recovery");
        assert.equal(flow.describeCount(), base, "already-unknown watchdog publishes no capture");
        flow.assertHealthy();
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.gestureWaitCount(), 3);
        assert.equal(flow.describeCount(), base, "steady unknown stays silent across deadlines");
        assert.equal(flow.countLogs("gesture-facts-host-timeout"), 0);
        flow.assertHealthy();
        flow.handle.stop();
    });

    it("known->unknown publishes exactly one hide, then stays silent; valid recovery is adopted", () => {
        const flow = startFlow();
        flow.bind();
        const base = flow.describeCount();
        flow.gestureCalls[0]?.callback(gestureStarted());
        flow.pump(120);
        assert.equal(flow.gestureWaitCount(), 2, "open reply re-arms immediately");
        assert.equal(flow.describeCount(), base + 1);
        flow.assertHealthy();
        assert.deepEqual(flow.lastReport()["nativeGesture"], {
            v: 1, instance: "gesture-1", serial: "1", revision: "1",
            phase: "started", win: "win-a", kind: "move",
            startRect: [0, 0, 200, 200], startCursor: [10, 10], cursor: [20, 20],
            modalMove: 1, changed: 1, superseded: 0, available: 1,
        });
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.gestureWaitCount(), 3);
        assert.equal(flow.describeCount(), base + 2, "exactly one hide capture on known->unknown");
        flow.assertHealthy();
        assert.equal(flow.lastReport()["nativeGesture"], null);
        assert.equal(flow.countLogs("gesture-facts-host-timeout"), 1);
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.gestureWaitCount(), 4);
        assert.equal(flow.describeCount(), base + 2, "no second hide while already unknown");
        flow.gestureCalls[3]?.callback(gestureStarted({ serial: "2", revision: "2" }));
        flow.pump(120);
        assert.equal(flow.describeCount(), base + 3, "enabled-later open is adopted");
        flow.assertHealthy();
        assert.equal((flow.lastReport()["nativeGesture"] as Record<string, unknown>)["serial"], "2");
        flow.handle.stop();
    });

    it("malformed and unavailable replies while already unknown are silent but keep recovery", () => {
        const flow = startFlow();
        flow.bind();
        const base = flow.describeCount();
        const waits = flow.gestureWaitCount();
        flow.gestureCalls[waits - 1]?.callback(JSON.stringify({ v: 1, bogus: true }));
        flow.pump(120);
        assert.equal(flow.describeCount(), base, "malformed while unknown publishes nothing");
        assert.equal(flow.countLogs("gesture-facts-malformed"), 0);
        flow.assertHealthy();
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.gestureWaitCount(), waits + 1, "malformed lane still recovers on the host deadline");
        assert.equal(flow.describeCount(), base);
        flow.assertHealthy();
        const waits2 = flow.gestureWaitCount();
        flow.gestureCalls[waits2 - 1]?.callback(gestureStarted({ available: 0, serial: "0", phase: "idle", win: "", kind: "", startRect: null, startCursor: null, cursor: null }));
        flow.pump(120);
        assert.equal(flow.describeCount(), base, "unavailable while unknown publishes nothing");
        assert.equal(flow.countLogs("gesture-facts-unavailable"), 0);
        flow.assertHealthy();
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.gestureWaitCount(), waits2 + 1);
        assert.equal(flow.describeCount(), base);
        flow.assertHealthy();
        flow.gestureCalls[waits2]?.callback(gestureStarted({ serial: "9", revision: "9" }));
        flow.pump(120);
        assert.equal(flow.describeCount(), base + 1, "lane adopts a later open after silent intervals");
        flow.assertHealthy();
        assert.equal((flow.lastReport()["nativeGesture"] as Record<string, unknown>)["serial"], "9");
        flow.handle.stop();
    });

    it("late callbacks after a watchdog re-arm and post-stop completions stay fenced", () => {
        const flow = startFlow();
        flow.bind();
        const base = flow.describeCount();
        const first = flow.gestureCalls[0];
        assert.ok(first !== undefined);
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.describeCount(), base, "watchdog while unknown stays silent");
        const waits = flow.gestureWaitCount();
        first.callback(gestureStarted());
        flow.pump(120);
        assert.equal(flow.gestureWaitCount(), waits, "settled late callback never re-arms");
        assert.equal(flow.describeCount(), base, "settled late callback never dispatches");
        assert.equal(flow.countLogs("gesture-facts-stale"), 0, "settled transport silence is not a stale fence hit");
        flow.assertHealthy();
        flow.handle.stop();
        const live = flow.gestureCalls[flow.gestureCalls.length - 1];
        assert.ok(live !== undefined);
        live.callback(gestureStarted({ serial: "3", revision: "3" }));
        flow.fireHost();
        flow.pump(120);
        assert.equal(flow.describeCount(), base, "post-stop completions never dispatch");
        flow.handle.stop();
    });
});
