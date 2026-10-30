// Bounded terminal-drop rectangle diagnostics: one `drop-settled` summary
// per proven gesture settlement on the accepted path (flight-frozen
// phase/cursor, source homing, desired allocation, pre-actuation observed
// frame, destination work area, last painted preview with producer proof),
// plus `drop-observed` follow-ups on the next two accepted captures and one
// post-apply `drop-attempt` carrying pending-report mechanical evidence.
// Observed rows are reported authority only, never client-commit or final
// claims. Ordinary motion, open-gesture captures, and unsettled terminals
// log no drop lines.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { startWorldAdapterEntry } from "../src/world-entry";

const GEN = "gen-1";
const BROKER = "broker-1";
const EFFECT = "effect-1";

interface Call { method: string; payload: string; callback: (reply: unknown) => void }
interface Timer { delay: number; cb: () => void }

interface FakeWin {
    internalId: string;
    frame: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: Array<{ id: string; name: string }>;
    [key: string]: unknown;
}

function makeWin(id: string, frame: { x: number; y: number; width: number; height: number }): FakeWin {
    return {
        internalId: id, normalWindow: true, managed: true, deleted: false,
        transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        frameGeometry: { ...frame },
        clientGeometry: { ...frame },
        output: { name: "out-1" }, desktops: [{ id: "ws1", name: "Workspace 1" }],
        minimized: false, hidden: false, fullscreen: false, fullScreen: false, maximized: false, maximizeMode: 0,
        sticky: false, onAllDesktops: false, minSize: null, maxSize: null,
        keepAbove: false, keepBelow: false,
        frame, outputChanged: { connect: () => undefined, disconnect: () => undefined },
        desktopsChanged: { connect: () => undefined, disconnect: () => undefined },
        interactiveMoveResizeStarted: { connect: () => undefined, disconnect: () => undefined },
        interactiveMoveResizeFinished: { connect: () => undefined, disconnect: () => undefined },
    } as unknown as FakeWin;
}

function gestureBody(over: Record<string, unknown>): string {
    return JSON.stringify({
        v: 1, instance: "gesture-1", serial: "7", revision: "1",
        phase: "started", win: "win-a", kind: "move",
        startRect: [0, 0, 200, 200], startCursor: [10, 10], cursor: [20, 20],
        modalMove: 1, changed: 1, superseded: 0, available: 1, ...over,
    });
}

function windowRow(over: Record<string, unknown> = {}): Record<string, unknown> {
    return {
        win: "win-a", out: "out-1", ws: "ws1",
        alloc: [10, 20, 300, 400], frame: [10, 20, 300, 400],
        frame_auth: "controlled", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", clear_op: "",
        floating: false, float_origin: "", ...over,
    };
}

function projection(req: string, preview: Record<string, unknown> | null, rev = "9", obs = "9"): string {
    return JSON.stringify({
        v: 3, outcome: "presentation", gen: GEN, req, rev, obs,
        coveredInstance: EFFECT, coveredInvalid: "0",
        scopes: [{ scope: "out/ws", auth: "controlled", reason: "" }],
        workspaces: [{ id: "ws1", out: "out-1", bind: "ws1", scope: "out-1", order: 0, mode: "tiled", exist: true, hold: "" }],
        outputs: [{ out: "out-1", view: "ws1", mode: "tiled", hold: "" }],
        windows: [{
            win: "win-a", out: "out-1", ws: "ws1",
            alloc: [10, 20, 300, 400], frame: [10, 20, 300, 400],
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "",
            floating: false, float_origin: "",
        }],
        presentation: {
            foreground: "win-a",
            border: { visible: true, reason: "", inner: [10, 10, 300, 300], outer: [8, 8, 304, 304] },
            underlay: { status: "hidden", reason: "disabled", baseRev: null },
            pager: [],
        },
        preview,
    });
}

interface Harness {
    logs: string[];
    calls: Call[];
    gestureCalls: Call[];
    effectCalls: Call[];
    wins: FakeWin[];
    fireDelay: (delay: number) => void;
    fireInvalidation: () => void;
    answerDescribes: (make: (report: Record<string, unknown>, assoc: { instance: string; serial: string; coreId: string } | null) => { settled: { id: string; committed: boolean } | null }) => void;
    // Reply window rows served by subsequent describes. Starts converged
    // with the fake frame (no actuation), so the test can introduce the
    // drop's desired allocation exactly at the terminal verdict.
    setWindows: (rows: Array<Record<string, unknown>>) => void;
    answerReads: (make: (req: string) => string) => void;
    answerGesture: (body: string) => void;
    // Production D-Bus never answers synchronously: ack replies stay held
    // until the test collects them, so a settlement discharge cannot
    // supersede its own snapshot the way a sync answer would.
    answerAcks: () => void;
    answerEffectFacts: () => void;
    stop: () => void;
}

function setup(): Harness {
    const ws1 = { id: "ws1", name: "Workspace 1" };
    const out1 = { name: "out-1", geometry: { x: 0, y: 0, width: 1920, height: 1080 } };
    const winA = makeWin("win-a", { x: 11, y: 21, width: 300, height: 380 });
    const winB = makeWin("win-b", { x: 420, y: 21, width: 300, height: 380 });
    const wins: FakeWin[] = [winA, winB];
    const logs: string[] = [];
    const calls: Call[] = [];
    const gestureCalls: Call[] = [];
    const effectCalls: Call[] = [];
    const timers: Timer[] = [];
    const invalidateHandlers: Array<() => void> = [];
    let rev = 0;
    // Harness broker identity map (production shape): first-seen native
    // identities allocate fresh numeric core ids in order.
    const coreIds = new Map<string, string>();
    let nextCore = 1;
    let describeWindows: Array<Record<string, unknown>> = [
        windowRow({ alloc: [11, 21, 300, 380], frame: [11, 21, 300, 380] }),
    ];
    const handle = startWorldAdapterEntry({
        workspace: {
            windowList: () => wins,
            screens: [out1],
            desktops: [ws1],
            activeScreen: out1,
            get activeWindow() { return winA; },
            cursorPos: { x: 20, y: 20 },
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktop: ws1,
            currentDesktopForScreen: () => ws1,
            setCurrentDesktopForScreen: () => undefined,
            createDesktop: () => null as unknown as object,
            removeDesktop: () => undefined,
            windowAdded: {
                connect: (h: () => void) => { invalidateHandlers.push(h); },
                disconnect: () => undefined,
            },
            windowRemoved: { connect: () => undefined, disconnect: () => undefined },
            windowActivated: { connect: () => undefined, disconnect: () => undefined },
            desktopsChanged: { connect: () => undefined, disconnect: () => undefined },
            screensChanged: { connect: () => undefined, disconnect: () => undefined },
            currentDesktopChanged: { connect: () => undefined, disconnect: () => undefined },
            activeScreenChanged: { connect: () => undefined, disconnect: () => undefined },
        },
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "DiscoverWorld") {
                const body = JSON.parse(payload) as { req: string };
                callback(JSON.stringify({
                    v: 3, adapter: BROKER, gen: GEN, req: body.req, obs: "0", rev: "0",
                    ready: false, consumed: false, scopes: [], workspaces: [], outputs: [],
                    windows: [], transfers: [], effects: [], result: null, gestureSettled: null,
                    operationSettled: [], nativeCreations: [], warpCursor: null,
                    presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
                }));
            }
        },
        effectCallDbus: (_s, _p, _i, method, payload, callback) => {
            effectCalls.push({ method, payload, callback });
        },
        gestureCallDbus: (_s, _p, _i, method, payload, callback) => {
            gestureCalls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            timers.push({ delay, cb });
            return () => undefined;
        },
        log: (m) => { logs.push(m); },
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        highlightCallDbus: () => undefined,
    });
    assert.ok(handle !== null);
    const h = {} as Harness;
    h.logs = logs;
    h.calls = calls;
    h.gestureCalls = gestureCalls;
    h.effectCalls = effectCalls;
    h.wins = wins;
    h.fireDelay = (delay: number): void => {
        for (let i = timers.length - 1; i >= 0; i -= 1) {
            const t = timers[i] as Timer;
            if (t.delay === delay) {
                timers.splice(i, 1);
                t.cb();
            }
        }
    };
    h.fireInvalidation = (): void => {
        for (const handler of [...invalidateHandlers]) {
            handler();
        }
    };
    h.answerDescribes = (make): void => {
        for (let i = calls.length - 1; i >= 0; i -= 1) {
            const call = calls[i] as Call;
            if (call.method !== "DescribeWorld") {
                continue;
            }
            calls.splice(i, 1);
            const body = JSON.parse(call.payload) as { source: string; nativeInvalid: string };
            rev += 1;
            const report = (JSON.parse(call.payload) as { report: Record<string, unknown> }).report;
            const raw = report["nativeGesture"] as Record<string, unknown> | null | undefined;
            let assoc: { instance: string; serial: string; coreId: string } | null = null;
            if (
                raw !== null && raw !== undefined &&
                typeof raw["instance"] === "string" && typeof raw["serial"] === "string" &&
                (raw["serial"] as string) !== "0"
            ) {
                const key = (raw["instance"] as string) + ":" + (raw["serial"] as string);
                let id = coreIds.get(key);
                if (id === undefined) {
                    id = String(nextCore);
                    nextCore += 1;
                    coreIds.set(key, id);
                }
                assoc = { instance: raw["instance"] as string, serial: raw["serial"] as string, coreId: id };
            }
            const settled = make(report, assoc);
            call.callback(JSON.stringify({
                v: 3, adapter: BROKER, gen: GEN, req: body.source, obs: String(rev), rev: String(rev),
                ready: true, consumed: false, scopes: [], workspaces: [], outputs: [],
                windows: describeWindows, transfers: [], effects: [], result: null,
                gestureSettled: settled.settled, gestureAssoc: assoc,
                operationSettled: [], nativeCreations: [], warpCursor: null,
                source: body.source, nativeInvalid: body.nativeInvalid, clock: "1",
                presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
            }));
        }
    };
    h.answerReads = (make): void => {
        for (let i = calls.length - 1; i >= 0; i -= 1) {
            const call = calls[i] as Call;
            if (call.method !== "ReadWorldPresentation") {
                continue;
            }
            calls.splice(i, 1);
            const req = (JSON.parse(call.payload) as { req: string }).req;
            call.callback(make(req));
        }
    };
    h.setWindows = (rows: Array<Record<string, unknown>>): void => {
        describeWindows = rows;
    };
    h.answerGesture = (body: string): void => {
        const wait = [...gestureCalls].reverse().find((c) => c.method === "WaitGesture");
        assert.ok(wait !== undefined, "held WaitGesture exists");
        gestureCalls.splice(gestureCalls.indexOf(wait), 1);
        wait.callback(body);
    };
    h.answerAcks = (): void => {
        for (let i = gestureCalls.length - 1; i >= 0; i -= 1) {
            const call = gestureCalls[i] as Call;
            if (call.method !== "AckGesture") {
                continue;
            }
            gestureCalls.splice(i, 1);
            call.callback(gestureBody({
                serial: "0", revision: "9", phase: "idle", win: "", kind: "",
                startRect: null, startCursor: null, cursor: null, modalMove: 0,
            }));
        }
    };
    h.answerEffectFacts = (): void => {
        const wait = [...effectCalls].reverse().find((c) => c.method === "WaitPresentationFacts");
        assert.ok(wait !== undefined, "held effect facts waiter exists");
        effectCalls.splice(effectCalls.indexOf(wait), 1);
        wait.callback(JSON.stringify({
            v: 1, instance: EFFECT, rev: "0", invalidation: "0",
            chord: 0, chordKnown: 0, gap: 0, width: 3, extension: -1,
            changed: 1, superseded: 0, available: 1,
        }));
    };
    h.stop = (): void => {
        handle?.stop();
    };
    return h;
}

function dropSettled(logs: string[]): string[] {
    return logs.filter((l) => l.indexOf("omnitiler:world:drop-settled") === 0);
}

function dropObserved(logs: string[]): string[] {
    return logs.filter((l) => l.indexOf("omnitiler:world:drop-observed") === 0);
}

function dropAttempt(logs: string[]): string[] {
    return logs.filter((l) => l.indexOf("omnitiler:world:drop-attempt") === 0);
}

function dropJson(line: string, prefix: string): Record<string, unknown> {
    assert.ok(line.indexOf(prefix) === 0, line);
    return JSON.parse(line.slice(prefix.length).trim()) as Record<string, unknown>;
}

describe("terminal drop diagnostics", () => {
    it("logs one bounded summary with correct rect/domain association plus two follow-ups", () => {
        const h = setup();
        try {
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: null }));
            h.answerEffectFacts();
            // Open gesture: no drop lines on the accepted open capture.
            h.answerGesture(gestureBody({ revision: "2", phase: "updated", cursor: [40, 40] }));
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: null }));
            assert.equal(dropSettled(h.logs).length, 0);
            assert.equal(dropObserved(h.logs).length, 0);
            // Paint the preview for the open gesture (producer proof).
            h.answerReads((req) => projection(req, {
                rect: [10, 20, 300, 400], sourceOut: "out-1", sourceWs: "ws1", win: "win-b",
                producerInstance: "gesture-1", producerSerial: "7",
            }));
            const painted = h.logs.filter((l) => l.indexOf("omnitiler:world:presentation-painted") === 0);
            assert.equal(painted.length, 1);
            assert.ok((painted[0] as string).indexOf("preview=rect previewRect=10,20,300,400 producer=gesture-1/7") !== -1, painted[0] as string);
            // Terminal release: exactly one summary line. The desired
            // allocation arrives with this verdict (off the observed frame).
            h.setWindows([windowRow()]);
            h.answerGesture(gestureBody({ revision: "3", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            h.answerDescribes((report, assoc) => {
                const raw = report["nativeGesture"] as Record<string, unknown> | null;
                if (raw !== null && raw["phase"] === "released" && assoc !== null) {
                    return { settled: { id: assoc.coreId, committed: true } };
                }
                return { settled: null };
            });
            const settled = dropSettled(h.logs);
            assert.equal(settled.length, 1, JSON.stringify(h.logs));
            const summary = dropJson(settled[0] as string, "omnitiler:world:drop-settled");
            assert.equal(summary["rev"], "3");
            assert.equal(summary["serial"], "7");
            assert.equal(summary["instance"], "gesture-1");
            assert.equal(summary["win"], "win-a");
            assert.equal(summary["settled"], "1");
            assert.equal(summary["committed"], true);
            assert.deepEqual(summary["dest"], { out: "out-1", ws: "ws1" });
            assert.deepEqual(summary["desired"], [10, 20, 300, 400]);
            assert.deepEqual(summary["desiredFrame"], [10, 20, 300, 400]);
            // Pre-actuation frame: the fake window still sits off-desired.
            assert.deepEqual(summary["observedPre"], { x: 11, y: 21, w: 300, h: 380 });
            assert.deepEqual(summary["observedAt"], { out: "out-1", ws: "ws1" });
            // Destination work area (client area, not the full screen).
            assert.deepEqual(summary["work"], { x: 0, y: 0, w: 800, h: 600 });
            // Release cursor rides the terminal facts line, not the summary.
            const terminal = h.logs.filter((l) => l.indexOf("omnitiler:world:gesture-facts-terminal") === 0);
            assert.equal(terminal.length, 1);
            assert.ok((terminal[0] as string).indexOf("phase=released serial=7 cursor=30,30") !== -1, terminal[0] as string);
            assert.equal(dropObserved(h.logs).length, 0);
            // Complete the native lifecycle, then observe post-actuation.
            // Pending-report evidence for the tracked window (no origin
            // beyond pending is claimed).
            const attempts = dropAttempt(h.logs);
            assert.equal(attempts.length, 1, JSON.stringify(h.logs));
            const attempt = dropJson(attempts[0] as string, "omnitiler:world:drop-attempt");
            assert.equal(attempt["rev"], "3");
            assert.equal(attempt["serial"], "7");
            assert.equal(attempt["instance"], "gesture-1");
            assert.equal(attempt["win"], "win-a");
            assert.equal(attempt["evidence"], "pending-report");
            const attemptFrames = attempt["frames"] as Array<Record<string, unknown>>;
            assert.equal(attemptFrames.length, 1);
            assert.deepEqual(attemptFrames[0]?.["target"], [10, 20, 300, 400]);
            assert.equal(attemptFrames[0]?.["setter"], "accepted");
            h.answerAcks();
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: null }));
            const observed = dropObserved(h.logs);
            assert.equal(observed.length, 1, JSON.stringify(h.logs));
            const follow = dropJson(observed[0] as string, "omnitiler:world:drop-observed");
            assert.equal(follow["serial"], "7");
            assert.equal(follow["instance"], "gesture-1");
            assert.equal(follow["win"], "win-a");
            assert.deepEqual(follow["frame"], { x: 10, y: 20, w: 300, h: 400 });
            assert.deepEqual(follow["at"], { out: "out-1", ws: "ws1" });
            // Reported reply authority rides each follow-up (never a
            // client-commit or final claim): desired plus the exact
            // frame/member authority strings and the window transfer.
            assert.deepEqual(follow["desired"], [10, 20, 300, 400]);
            assert.deepEqual(follow["desiredFrame"], [10, 20, 300, 400]);
            assert.equal(follow["frame_auth"], "controlled");
            assert.equal(follow["frame_hold"], "");
            assert.equal(follow["member_auth"], "controlled");
            assert.equal(follow["member_hold"], "");
            assert.equal(follow["transfer"], null);
            assert.ok(typeof follow["rev"] === "string" && (follow["rev"] as string) !== "");
            assert.ok(typeof follow["obs"] === "string" && (follow["obs"] as string) !== "");
            assert.ok(!("settled" in (follow as Record<string, unknown>)));
            // No new mechanical writes converged after the settlement
            // apply, so no further pending-report evidence yet.
            assert.equal(dropAttempt(h.logs).length, 1);
            // The summary never repeats; the second accepted capture
            // consumes the retained follow-up slot.
            assert.equal(dropSettled(h.logs).length, 1);
            h.fireInvalidation();
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: null }));
            const observed2 = dropObserved(h.logs);
            assert.equal(observed2.length, 2, JSON.stringify(h.logs));
            const follow2 = dropJson(observed2[1] as string, "omnitiler:world:drop-observed");
            assert.equal(follow2["serial"], "7");
            assert.equal(follow2["instance"], "gesture-1");
            assert.equal(follow2["win"], "win-a");
            assert.equal(follow2["rev"], "5");
            assert.deepEqual(follow2["frame"], { x: 10, y: 20, w: 300, h: 400 });
            assert.deepEqual(follow2["desired"], [10, 20, 300, 400]);
            assert.equal(follow2["frame_auth"], "controlled");
            assert.equal(follow2["transfer"], null);
            assert.equal(dropAttempt(h.logs).length, 1);
            // The slot is spent after exactly two accepted captures: a
            // third round logs nothing new and schedules no extra capture.
            h.fireInvalidation();
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: null }));
            assert.equal(dropSettled(h.logs).length, 1);
            assert.equal(dropObserved(h.logs).length, 2);
            assert.equal(dropAttempt(h.logs).length, 1);
            // No spontaneous capture: every dispatch was answered and none
            // remains pending without a native event.
            assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, 0);
        } finally {
            h.stop();
        }
    });

    it("pairs an async mismatched frame with reply hold authority across two follow-ups", () => {
        const h = setup();
        try {
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: null }));
            h.answerEffectFacts();
            // Asynchronous mismatched-frame fixture using incident
            // rectangles on this single-output harness (no scale facts,
            // not a repro of client internals): the setter reports
            // success but the immediate readback still shows the old
            // rect. Drive the fake through an accessor whose setter
            // stashes without converging.
            const winA = h.wins[0] as unknown as Record<string, unknown>;
            let live = { x: 11, y: 21, width: 300, height: 380 };
            Object.defineProperty(winA, "frameGeometry", {
                configurable: true,
                get: () => ({ ...live }),
                set: () => undefined,
            });
            h.setWindows([windowRow({ alloc: [8, 52, 1012, 1092], frame: [8, 52, 1012, 1092] })]);
            h.answerGesture(gestureBody({ revision: "3", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            h.answerDescribes((report, assoc) => {
                const raw = report["nativeGesture"] as Record<string, unknown> | null;
                if (raw !== null && raw["phase"] === "released" && assoc !== null) {
                    return { settled: { id: assoc.coreId, committed: true } };
                }
                return { settled: null };
            });
            assert.equal(dropSettled(h.logs).length, 1, JSON.stringify(h.logs));
            // Pending-report evidence of the async mismatch: the setter
            // was accepted yet the immediate readback never moved.
            const attempts = dropAttempt(h.logs);
            assert.equal(attempts.length, 1, JSON.stringify(h.logs));
            const attempt = dropJson(attempts[0] as string, "omnitiler:world:drop-attempt");
            assert.equal(attempt["evidence"], "pending-report");
            assert.equal(attempt["win"], "win-a");
            const frames = attempt["frames"] as Array<Record<string, unknown>>;
            assert.equal(frames.length, 1);
            assert.deepEqual(frames[0]?.["target"], [8, 52, 1012, 1092]);
            assert.equal(frames[0]?.["setter"], "accepted");
            assert.deepEqual(frames[0]?.["readback"], [11, 21, 300, 380]);
            assert.ok((attempts[0] as string).indexOf("commit") === -1, attempts[0] as string);
            // First follow-up still shows the pre-write frame: the later
            // native read has not landed yet.
            h.answerAcks();
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: null }));
            const first = dropObserved(h.logs);
            assert.equal(first.length, 1, JSON.stringify(h.logs));
            const follow1 = dropJson(first[0] as string, "omnitiler:world:drop-observed");
            assert.deepEqual(follow1["frame"], { x: 11, y: 21, w: 300, h: 380 });
            assert.deepEqual(follow1["desired"], [8, 52, 1012, 1092]);
            // The follow-up apply re-tried the still-mismatched frame, so
            // a second pending-report row rode out (bounded: only while
            // the tracked slot is active).
            const attempts2 = dropAttempt(h.logs);
            assert.equal(attempts2.length, 2, JSON.stringify(h.logs));
            const attempt2 = dropJson(attempts2[1] as string, "omnitiler:world:drop-attempt");
            assert.equal(attempt2["rev"], "3");
            assert.equal(attempt2["evidence"], "pending-report");
            const frames2 = attempt2["frames"] as Array<Record<string, unknown>>;
            assert.deepEqual(frames2[0]?.["target"], [8, 52, 1012, 1092]);
            assert.deepEqual(frames2[0]?.["readback"], [11, 21, 300, 380]);
            // A later native read shows height 1036 while the reply carries
            // production-shaped hold authority (frame_auth hold with the
            // constraint-clamp reason). The harness mocks the reply, so
            // this is report evidence only: not proof the live frame is
            // core-held, and not a claim about core clamp mechanics.
            live = { x: 8, y: 52, width: 1012, height: 1036 };
            h.setWindows([windowRow({
                alloc: [8, 52, 1012, 1092], frame: [8, 52, 1012, 1092],
                frame_auth: "hold", frame_hold: "constraint-clamp",
            })]);
            h.fireInvalidation();
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: null }));
            const second = dropObserved(h.logs);
            assert.equal(second.length, 2, JSON.stringify(h.logs));
            const follow2 = dropJson(second[1] as string, "omnitiler:world:drop-observed");
            assert.deepEqual(follow2["frame"], { x: 8, y: 52, w: 1012, h: 1036 });
            assert.deepEqual(follow2["desired"], [8, 52, 1012, 1092]);
            assert.equal(follow2["frame_auth"], "hold");
            assert.equal(follow2["frame_hold"], "constraint-clamp");
            assert.equal(follow2["transfer"], null);
            // Exactly two follow-ups, then the slot expires with no extra
            // captures or timers. Both pending-report rows stay on record;
            // the held reply rows take no writes here.
            h.fireInvalidation();
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: null }));
            assert.equal(dropSettled(h.logs).length, 1);
            assert.equal(dropObserved(h.logs).length, 2, JSON.stringify(h.logs));
            assert.equal(dropAttempt(h.logs).length, 2, JSON.stringify(h.logs));
            assert.equal(h.calls.filter((c) => c.method === "DescribeWorld").length, 0);
        } finally {
            h.stop();
        }
    });

    it("logs no drop lines for motion or for an unsettled terminal", () => {
        const h = setup();
        try {
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: null }));
            h.answerEffectFacts();
            // Per-motion updates: silent on the drop channel.
            for (const cursor of [[21, 21], [22, 22], [23, 23]]) {
                h.answerGesture(gestureBody({ revision: "2", phase: "updated", cursor }));
                h.fireDelay(120);
                h.answerDescribes(() => ({ settled: null }));
            }
            assert.equal(dropSettled(h.logs).length, 0);
            assert.equal(dropObserved(h.logs).length, 0);
            // Terminal with an unrelated settled id: no summary, no ack alias.
            h.answerGesture(gestureBody({ revision: "3", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            h.answerDescribes(() => ({ settled: { id: "99", committed: false } }));
            assert.equal(dropSettled(h.logs).length, 0);
            assert.equal(dropObserved(h.logs).length, 0);
            assert.equal(h.gestureCalls.filter((c) => c.method === "AckGesture").length, 0);
        } finally {
            h.stop();
        }
    });
});
