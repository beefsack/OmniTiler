// Stage 5 full-interleaving freshness contracts: production
// `startWorldAdapterEntry` routing only - real host signals, real captures,
// real presentation completion routes, real renderer endpoint calls.
//
// Row 1 (geometry): hold P5; read cached P6 captured before the member
// frame move; advance the native invalidation witness through the real
// effect-facts route (frame mutation plus facts waiter completion, the
// exact production signal for a frame move the script cannot observe
// itself); deliver P6 before the post-move capture is accepted. P6 must
// stay hidden until a causally covering capture arrives, even though its
// (rev, obs) is newer. The frame move drives the actual frame
// move/facts/native counter route (never a desktop switch).
// Row 2 (preview): open gesture A, paint its preview, finish A (terminal
// settled + acked), start B, then drain/publish/read the stale A preview
// entirely inside B. It must never paint in B; the insertion neighbor is
// not the mover. The already-sent native half (an A paint delivered after
// B starts) is pinned at the consumer in
// kwin/native-effect/presentationconsumer_test.cpp.
// Row 3 (focus): show A, activate B, deliver an in-transit old A read,
// accept the post-activation capture, return to A; then drive the
// error-hide path (malformed read hides display-only) and deliver another
// old A paint. Neither in-transit paint may resurrect the cleared paint;
// production activation/error-hide routes must preserve the ordering
// evidence and only an explicitly replaced stream resets it. An unknown
// facts interval hides display-only without resetting the stream; the
// same-instance return recovers without replacement.
//
// All four assert on visible renderer outcomes (SetPresentation payloads),
// not field presence. Broker-projection contracts live in
// crates/omnitiler/tests/stage5-causal-freshness.rs.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";

interface Render { method: string; args: ReadonlyArray<unknown> }
interface Call { method: string; payload: string; callback: (reply: unknown) => void }
interface Timer { delay: number; cb: () => void }

const GEN = "gen-1";
const BROKER = "broker-1";
const EFFECT = "effect-1";

function projection(
    req: string, rev: string, obs: string,
    opts: {
        foreground: string;
        preview: Record<string, unknown> | null;
        coveredInstance?: string | null;
        coveredInvalid?: string | null;
    },
): string {
    return JSON.stringify({
        v: 2, outcome: "presentation", gen: GEN, req, rev, obs,
        coveredInstance: opts.coveredInstance ?? EFFECT,
        coveredInvalid: opts.coveredInvalid ?? "0",
        scopes: [{ scope: "out/ws", auth: "controlled", reason: "" }],
        workspaces: [{ id: "ws1", out: "out-1", bind: "ws1", scope: "out-1", order: 0, mode: "tiled", exist: true, hold: "" }],
        outputs: [{ out: "out-1", view: "ws1", mode: "tiled", hold: "" }],
        windows: [{
            win: "win-a", out: "out-1", ws: "ws1",
            alloc: [4, 4, 792, 592], frame: [4, 4, 792, 592],
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "",
            floating: false, float_origin: "",
        }],
        presentation: {
            foreground: opts.foreground,
            border: { visible: true, reason: "", inner: [10, 10, 300, 300], outer: [8, 8, 304, 304] },
            underlay: {
                status: "found", reason: "", baseRev: "11",
                homeOut: "out-1", homeWs: "ws1", group: "grp-1", leaf: "leaf-a", win: "win-a",
                union: [4, 4, 792, 592], outer: [0, 0, 800, 600],
                members: [
                    { leaf: "leaf-b", win: "win-b", rect: [4, 4, 392, 592] },
                    { leaf: "leaf-a", win: "win-a", rect: [404, 4, 392, 592] },
                ],
            },
            pager: [],
        },
        preview: opts.preview,
    });
}

interface FakeWin {
    internalId: string;
    frame: { x: number; y: number; width: number; height: number };
    output: { name: string };
    desktops: Array<{ id: string; name: string }>;
    [key: string]: unknown;
}

function makeWin(id: string, out: { name: string }, desks: Array<{ id: string; name: string }>): FakeWin {
    const handlers: Record<string, Array<() => void>> = { outputChanged: [], desktopsChanged: [] };
    const sig = (name: string): { connect: (h: () => void) => void; disconnect: () => void } => ({
        connect: (h: () => void) => { handlers[name]?.push(h); },
        disconnect: () => undefined,
    });
    const win = {
        internalId: id, normalWindow: true, managed: true, deleted: false,
        transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
        output: out, desktops: desks,
        minimized: false, fullscreen: false, fullScreen: false, maximized: false, maximizeMode: 0,
        sticky: false, onAllDesktops: false, minSize: null, maxSize: null,
        keepAbove: false, keepBelow: false,
        outputChanged: sig("outputChanged"), desktopsChanged: sig("desktopsChanged"),
        interactiveMoveResizeStarted: { connect: () => undefined, disconnect: () => undefined },
        interactiveMoveResizeFinished: { connect: () => undefined, disconnect: () => undefined },
    } as unknown as FakeWin;
    Object.defineProperty(win, "frameGeometry", {
        configurable: true, enumerable: true,
        get: () => (win as unknown as { frame: unknown }).frame,
        set: () => true,
    });
    Object.defineProperty(win, "clientGeometry", {
        configurable: true, enumerable: true,
        get: () => (win as unknown as { frame: unknown }).frame,
        set: () => true,
    });
    win.frame = { x: 0, y: 0, width: 200, height: 200 };
    (win as unknown as Record<string, unknown>)["__fire"] = (name: string): void => {
        for (const h of [...(handlers[name] ?? [])]) {
            h();
        }
    };
    return win;
}

interface Harness {
    calls: Call[];
    renders: Render[];
    gestureCalls: Call[];
    effectCalls: Call[];
    wins: FakeWin[];
    nativeCounter: number;
    setActive: (id: string) => void;
    fire: (name: string) => void;
    fireWin: (win: FakeWin, name: string) => void;
    fireDelay: (delay: number) => void;
    settledHook: ((report: Record<string, unknown>) => { id: string; committed: boolean } | null) | null;
    answerDiscovery: () => void;
    answerDescribes: () => void;
    answerReads: (make: (req: string) => string) => void;
    answerEffectFacts: (changed: boolean) => void;
    answerEffectFactsUnavailable: () => void;
    pendingReads: () => Call[];
    pendingDescribes: () => Call[];
    drainReads: (make: (req: string) => string, rounds?: number) => void;
    answerGesture: (body: Record<string, unknown>) => void;
    stop: () => void;
}

function setup(): Harness {
    const ws1 = { id: "ws1", name: "Workspace 1" };
    const ws2 = { id: "ws2", name: "Workspace 2" };
    const out1 = { name: "out-1" };
    const winA = makeWin("win-a", out1, [ws1]);
    const winB = makeWin("win-b", out1, [ws1]);
    const wins: FakeWin[] = [winA, winB];
    let activeId = "win-a";
    const currents = new Map([["out-1", "ws1"]]);
    const surfaceHandlers: Record<string, Array<(payload?: unknown) => void>> = {};
    const surfaceSig = (name: string): { connect: (h: (p?: unknown) => void) => void; disconnect: () => void } => ({
        connect: (h: (p?: unknown) => void) => {
            (surfaceHandlers[name] ??= []).push(h);
        },
        disconnect: () => undefined,
    });
    const calls: Call[] = [];
    const renders: Render[] = [];
    const gestureCalls: Call[] = [];
    const effectCalls: Call[] = [];
    const timers: Timer[] = [];
    let rev = 0;
    let nativeCounter = 0;
    const h = {} as Harness;
    h.calls = calls;
    h.renders = renders;
    h.gestureCalls = gestureCalls;
    h.effectCalls = effectCalls;
    h.wins = wins;
    h.settledHook = null;
    h.setActive = (id: string): void => {
        activeId = id;
    };
    const coreIds = new Map<string, string>();
    let nextCore = 1;
    const assocFor = (report: Record<string, unknown>): { instance: string; serial: string; coreId: string } | null => {
        const raw = report["nativeGesture"] as Record<string, unknown> | null | undefined;
        if (raw === null || raw === undefined) {
            return null;
        }
        const instance = raw["instance"];
        const serial = raw["serial"];
        if (typeof instance !== "string" || typeof serial !== "string" || serial === "0") {
            return null;
        }
        const key = `${instance}\u0000${serial}`;
        let id = coreIds.get(key);
        if (id === undefined) {
            id = String(nextCore);
            nextCore += 1;
            coreIds.set(key, id);
        }
        return { instance, serial, coreId: id };
    };
    const answerOneDescribe = (call: Call): void => {
        const body = JSON.parse(call.payload) as { source: string; nativeInvalid: string };
        rev += 1;
        const report = (JSON.parse(call.payload) as { report: Record<string, unknown> }).report;
        const assoc = assocFor(report);
        const settled = h.settledHook?.(report) ?? null;
        call.callback(JSON.stringify({
            v: 2, adapter: BROKER, gen: GEN, req: body.source, obs: String(rev), rev: String(rev),
            ready: true, consumed: false, scopes: [], workspaces: [], outputs: [],
            windows: [], transfers: [], effects: [], result: null,
            gestureSettled: settled, gestureAssoc: assoc,
            operationSettled: [], nativeCreations: [], warpCursor: null,
            source: body.source, nativeInvalid: body.nativeInvalid, clock: "1",
            presentation: { foreground: "", border: { visible: false, reason: "disabled", inner: null, outer: null }, underlay: { status: "hidden", reason: "disabled", baseRev: null }, pager: [] },
        }));
    };
    const handle = startWorldAdapterEntry({
        workspace: {
            windowList: () => wins,
            screens: [out1],
            desktops: [ws1, ws2],
            activeScreen: out1,
            get activeWindow() { return wins.find((w) => w.internalId === activeId) ?? null; },
            cursorPos: { x: 20, y: 20 },
            clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
            currentDesktop: ws1,
            currentDesktopForScreen: (out: unknown) => {
                const name = (out as { name: string }).name;
                return [ws1, ws2].find((d) => d.id === currents.get(name)) ?? null;
            },
            setCurrentDesktopForScreen: (desk: unknown, out: unknown) => {
                currents.set((out as { name: string }).name, (desk as { id: string }).id);
            },
            createDesktop: () => null as unknown as object,
            removeDesktop: () => undefined,
            windowAdded: surfaceSig("windowAdded"),
            windowActivated: surfaceSig("windowActivated"),
            desktopsChanged: surfaceSig("desktopsChanged"),
            screensChanged: surfaceSig("screensChanged"),
            currentDesktopChanged: surfaceSig("currentDesktopChanged"),
            activeScreenChanged: surfaceSig("activeScreenChanged"),
            windowRemoved: surfaceSig("windowRemoved"),
        },
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "DiscoverWorld") {
                const body = JSON.parse(payload) as { req: string };
                callback(JSON.stringify({
                    v: 2, adapter: BROKER, gen: GEN, req: body.req, obs: "0", rev: "0",
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
            if (method === "AckGesture") {
                callback(JSON.stringify({
                    v: 1, instance: "gesture-1", serial: "0", revision: "9",
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
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
        highlightCallDbus: (_service, _path, _iface, method, ...args) => {
            renders.push({ method, args });
        },
    });
    assert.ok(handle !== null);
    h.fire = (name: string): void => {
        for (const handler of [...(surfaceHandlers[name] ?? [])]) {
            handler();
        }
    };
    h.fireWin = (win: FakeWin, name: string): void => {
        const fire = (win as unknown as Record<string, unknown>)["__fire"] as ((n: string) => void) | undefined;
        assert.ok(typeof fire === "function", "window exposes its homing fire seam");
        fire(name);
    };
    h.fireDelay = (delay: number): void => {
        for (let i = timers.length - 1; i >= 0; i -= 1) {
            const t = timers[i] as Timer;
            if (t.delay === delay) {
                timers.splice(i, 1);
                t.cb();
            }
        }
    };
    h.answerDiscovery = (): void => {
        h.fireDelay(120);
    };
    h.answerDescribes = (): void => {
        for (let i = calls.length - 1; i >= 0; i -= 1) {
            const call = calls[i] as Call;
            if (call.method === "DescribeWorld") {
                calls.splice(i, 1);
                answerOneDescribe(call);
            }
        }
    };
    h.pendingReads = (): Call[] => calls.filter((c) => c.method === "ReadWorldPresentation");
    h.pendingDescribes = (): Call[] => calls.filter((c) => c.method === "DescribeWorld");
    h.answerReads = (make: (req: string) => string): void => {
        for (let i = calls.length - 1; i >= 0; i -= 1) {
            const call = calls[i] as Call;
            if (call.method === "ReadWorldPresentation") {
                calls.splice(i, 1);
                const req = (JSON.parse(call.payload) as { req: string }).req;
                call.callback(make(req));
            }
        }
    };
    h.answerEffectFacts = (changed: boolean): void => {
        const wait = [...effectCalls].reverse().find((c) => c.method === "WaitPresentationFacts");
        assert.ok(wait !== undefined, "held effect facts waiter exists");
        effectCalls.splice(effectCalls.indexOf(wait), 1);
        wait.callback(JSON.stringify({
            v: 1, instance: EFFECT, rev: "0", invalidation: String(nativeCounter),
            chord: 0, chordKnown: 0, gap: 0, width: 3, extension: -1,
            changed: changed ? 1 : 0, superseded: 0, available: 1,
        }));
    };
    h.answerEffectFactsUnavailable = (): void => {
        const wait = [...effectCalls].reverse().find((c) => c.method === "WaitPresentationFacts");
        assert.ok(wait !== undefined, "held effect facts waiter exists");
        effectCalls.splice(effectCalls.indexOf(wait), 1);
        wait.callback(JSON.stringify({
            v: 1, instance: EFFECT, rev: "0", invalidation: String(nativeCounter),
            chord: 0, chordKnown: 0, gap: 0, width: 3, extension: -1,
            changed: 0, superseded: 0, available: 0,
        }));
    };
    Object.defineProperty(h, "nativeCounter", {
        get: () => nativeCounter,
        set: (next: number) => { nativeCounter = next; },
    });
    // Drain pending reads in a bounded loop: a reply invalidated during
    // flight is skipped, not painted (finding E), and the chained refetch
    // surfaces as a new pending read. Used only where a paint is expected.
    h.drainReads = (make: (req: string) => string, rounds = 10): void => {
        for (let round = 0; round < rounds; round += 1) {
            const pending = calls.filter((c) => c.method === "ReadWorldPresentation");
            if (pending.length === 0) {
                break;
            }
            h.answerReads(make);
        }
    };
    h.answerGesture = (body: Record<string, unknown>): void => {
        const wait = [...gestureCalls].reverse().find((c) => c.method === "WaitGesture");
        assert.ok(wait !== undefined, "held WaitGesture exists");
        gestureCalls.splice(gestureCalls.indexOf(wait), 1);
        wait.callback(JSON.stringify(body));
    };
    h.stop = (): void => {
        handle?.stop();
    };
    return h;
}

function gestureBody(over: Record<string, unknown>): Record<string, unknown> {
    return {
        v: 1, instance: "gesture-1", serial: "1", revision: "1",
        phase: "started", win: "win-a", kind: "move",
        startRect: [0, 0, 200, 200], startCursor: [10, 10], cursor: [20, 20],
        modalMove: 1, changed: 1, superseded: 0, available: 1, ...over,
    };
}

function setsWithPreview(renders: Render[]): Render[] {
    return renders.filter((r) => {
        if (r.method !== "SetPresentation") {
            return false;
        }
        try {
            const payload = JSON.parse(r.args[0] as string) as Record<string, unknown>;
            return payload["preview"] !== null && payload["preview"] !== undefined;
        } catch {
            return false;
        }
    });
}

function setsWithForeground(renders: Render[], win: string): Render[] {
    return renders.filter((r) => {
        if (r.method !== "SetPresentation") {
            return false;
        }
        try {
            const payload = JSON.parse(r.args[0] as string) as Record<string, unknown>;
            return payload["foreground"] === win;
        } catch {
            return false;
        }
    });
}

function setsWithCovered(renders: Render[], covered: string): Render[] {
    return renders.filter((r) => {
        if (r.method !== "SetPresentation") {
            return false;
        }
        try {
            const payload = JSON.parse(r.args[0] as string) as Record<string, unknown>;
            return payload["coveredInvalid"] === covered;
        } catch {
            return false;
        }
    });
}

describe("world entry freshness (production routing)", () => {
    it("geometry: pre-move P6 delivered after the member frame move stays hidden", () => {
        const h = setup();
        try {
            h.answerDiscovery();
            // Seed the effect facts (witness 0) before the first capture so
            // it covers a proven value.
            h.answerEffectFacts(false);
            h.answerDescribes();
            h.drainReads((r) => projection(r, "5", "5", { foreground: "win-a", preview: null, coveredInvalid: "0" }));
            assert.equal(setsWithForeground(h.renders, "win-a").length, 1, "P5 paints");
            // Hold one read in flight: accept one more capture, keep its
            // read unanswered.
            h.fire("windowAdded");
            h.fireDelay(120);
            h.answerDescribes();
            assert.ok(h.pendingReads().length >= 1, "read held in flight");
            // Actual frame move through the production facts route: the
            // member frame mutates natively (no script frame signal
            // exists), the native witness advances, and the held facts
            // waiter completes with the new covered value, so the next
            // capture covers it.
            const winA = h.wins[0] as FakeWin;
            winA.frame = { x: 60, y: 10, width: 200, height: 200 };
            h.nativeCounter = 1;
            h.answerEffectFacts(true);
            h.fireDelay(120);
            assert.ok(h.pendingDescribes().length >= 1, "post-move capture dispatched, still unanswered");
            // The held read completes with cached pre-move P6 (newer
            // rev/obs, captured before the move) before the post-move
            // capture is accepted.
            const held = h.pendingReads();
            assert.equal(held.length, 1, "exactly the pre-move read flies");
            const setsBefore = h.renders.filter((r) => r.method === "SetPresentation").length;
            const heldReq = (JSON.parse((held[0] as Call).payload) as { req: string }).req;
            (held[0] as Call).callback(projection(heldReq, "6", "6", { foreground: "win-a", preview: null, coveredInvalid: "0" }));
            // Required: P6 stays hidden until the causally covered capture
            // arrives. Current routing paints it.
            assert.deepEqual(
                h.renders.filter((r) => r.method === "SetPresentation").slice(setsBefore),
                [],
                "pre-move P6 must not paint after the frame move",
            );
            // Accept the post-move capture and paint its covering read.
            h.answerDescribes();
            h.drainReads((r) => projection(r, "7", "7", { foreground: "win-a", preview: null, coveredInvalid: "1" }));
            assert.equal(setsWithCovered(h.renders, "1").length, 1, "covering paint renders");
        } finally {
            h.stop();
        }
    });

    it("preview: A preview drained entirely inside B never paints", () => {
        const h = setup();
        try {
            h.answerDiscovery();
            h.answerEffectFacts(false);
            h.answerDescribes();
            h.drainReads((r) => projection(r, "5", "5", { foreground: "win-a", preview: null, coveredInvalid: "0" }));
            assert.equal(setsWithForeground(h.renders, "win-a").length, 1);
            // Open gesture A through the held native waiter.
            h.answerGesture(gestureBody({ serial: "1", revision: "1", phase: "started" }));
            h.fireDelay(120);
            h.answerDescribes();
            // The insertion neighbor is win-b, never the mover win-a; the
            // producing identity is gesture-1/1.
            const previewA = {
                rect: [0, 0, 396, 600], sourceOut: "out-1", sourceWs: "ws1", win: "win-b",
                producerInstance: "gesture-1", producerSerial: "1",
            };
            h.drainReads((r) => projection(r, "12", "12", { foreground: "win-a", preview: previewA, coveredInvalid: "0" }));
            assert.equal(setsWithPreview(h.renders).length, 1, "A preview paints while A is live");
            const paintedBefore = setsWithPreview(h.renders).length;
            // Finish A: the released terminal settles against the bound
            // association and the entry acks exactly once.
            h.settledHook = () => ({ id: "1", committed: false });
            h.answerGesture(gestureBody({ serial: "1", revision: "2", phase: "released", cursor: [30, 30] }));
            h.fireDelay(120);
            h.answerDescribes();
            assert.ok(h.gestureCalls.some((c) => c.method === "AckGesture"), "terminal acked");
            h.settledHook = null;
            // Start B: a new native serial on the same effect instance.
            h.answerGesture(gestureBody({ serial: "2", revision: "3", phase: "started" }));
            h.fireDelay(120);
            h.answerDescribes();
            // Drain the stale A preview entirely inside B: the read issued
            // by the B capture completes inside B with the old A bytes
            // under a newer projection rev.
            h.answerReads((r) => projection(r, "13", "13", { foreground: "win-a", preview: previewA, coveredInvalid: "0" }));
            // Required: the A preview must not paint in B. The payload
            // carries the producing identity, so current routing strips
            // it instead of repainting.
            assert.deepEqual(
                setsWithPreview(h.renders).slice(paintedBefore),
                [],
                "stale A preview must not paint inside B",
            );
        } finally {
            h.stop();
        }
    });

    it("focus: in-transit old paint after activation or error-clear never resurrects", () => {
        const h = setup();
        const failures: string[] = [];
        try {
            h.answerDiscovery();
            h.answerEffectFacts(false);
            h.answerDescribes();
            h.drainReads((r) => projection(r, "5", "5", { foreground: "win-a", preview: null, coveredInvalid: "0" }));
            assert.equal(setsWithForeground(h.renders, "win-a").length, 1, "A paints");
            const paintedA = (): number => setsWithForeground(h.renders, "win-a").length;
            const baseA = paintedA();
            // Hold an in-transit read: accept one more capture, keep its
            // read unanswered.
            h.fire("windowAdded");
            h.fireDelay(120);
            h.answerDescribes();
            assert.ok(h.pendingReads().length >= 1, "in-transit read held");
            // Activate B through the production signal with live focus
            // moved. The native witness advances with the activation (facts
            // route), so the covering capture proves the new focus.
            h.setActive("win-b");
            h.fire("windowActivated");
            h.nativeCounter = 1;
            h.answerEffectFacts(true);
            h.fireDelay(120);
            // Deliver the old A read after the activation, before the
            // post-activation capture is accepted. It covers the pre-move
            // witness, so it must hide.
            h.answerReads((r) => projection(r, "6", "6", { foreground: "win-a", preview: null, coveredInvalid: "0" }));
            if (paintedA() !== baseA) {
                failures.push("in-transit old A paint resurrected after B activation");
            }
            // Accept the post-activation capture and paint B (recovery).
            h.answerDescribes();
            h.drainReads((r) => projection(r, "7", "7", { foreground: "win-b", preview: null, coveredInvalid: "1" }));
            if (setsWithForeground(h.renders, "win-b").length !== 1) {
                failures.push("post-activation B paint did not render (recovery broken)");
            }
            // Return to A through the production signal; the fresh capture
            // repaints A (recovery). Counted relative to the post-stale
            // state so the recovery leg stays meaningful with or without
            // the bug: with the bug the stale paint already added one.
            const afterStale = paintedA();
            h.setActive("win-a");
            h.fire("windowActivated");
            h.nativeCounter = 2;
            h.answerEffectFacts(true);
            h.fireDelay(120);
            h.answerDescribes();
            h.drainReads((r) => projection(r, "8", "8", { foreground: "win-a", preview: null, coveredInvalid: "2" }));
            if (paintedA() !== afterStale + 1) {
                failures.push("return-to-A fresh paint did not render (recovery broken)");
            }
            // Error-hide path: hold a read across a transport error. Accept
            // one more capture and keep its read unanswered...
            h.fire("windowAdded");
            h.fireDelay(120);
            h.answerDescribes();
            assert.ok(h.pendingReads().length >= 1, "error-leg read held");
            // ...the held read completes malformed: the renderer hides
            // display-only (HidePresentation, evidence preserved) and
            // stands down to the next event (no self re-arm).
            const clearsBeforeError = h.renders.filter((r) => r.method === "ClearPresentation").length;
            const hidesBeforeError = h.renders.filter((r) => r.method === "HidePresentation").length;
            for (let i = h.calls.length - 1; i >= 0; i -= 1) {
                const call = h.calls[i] as Call;
                if (call.method === "ReadWorldPresentation") {
                    h.calls.splice(i, 1);
                    call.callback("not-json");
                }
            }
            if (h.renders.filter((r) => r.method === "HidePresentation").length <= hidesBeforeError) {
                failures.push("malformed read did not hide display-only (error-hide broken)");
            }
            if (h.renders.filter((r) => r.method === "ClearPresentation").length !== clearsBeforeError) {
                failures.push("error-hide must not reset the stream");
            }
            // ...a fresh capture is accepted, and the new read is served
            // stale pre-error bytes before any covering capture lands.
            h.fire("windowAdded");
            h.fireDelay(120);
            h.answerDescribes();
            const setsBefore = h.renders.filter((r) => r.method === "SetPresentation").length;
            h.answerReads((r) => projection(r, "8", "8", { foreground: "win-a", preview: null, coveredInvalid: "2" }));
            if (h.renders.filter((r) => r.method === "SetPresentation").length !== setsBefore) {
                failures.push("old paint after the error clear resurrected");
            }
            assert.deepEqual(failures, [], `focus/error-hide freshness violated: ${failures.join("; ")}`);
        } finally {
            h.stop();
        }
    });

    it("unknown facts hide display-only without resetting the stream", () => {
        const h = setup();
        const failures: string[] = [];
        try {
            h.answerDiscovery();
            h.answerEffectFacts(false);
            h.answerDescribes();
            h.drainReads((r) => projection(r, "5", "5", { foreground: "win-a", preview: null, coveredInvalid: "0" }));
            assert.equal(setsWithForeground(h.renders, "win-a").length, 1, "A paints");
            const paintedA = (): number => setsWithForeground(h.renders, "win-a").length;
            const clearsBefore = h.renders.filter((r) => r.method === "ClearPresentation").length;
            const hidesBefore = h.renders.filter((r) => r.method === "HidePresentation").length;
            // Facts go unavailable (teardown): unknown hides display-only.
            // Unknown is not an explicit stream replacement, so no reset.
            h.answerEffectFactsUnavailable();
            if (h.renders.filter((r) => r.method === "HidePresentation").length <= hidesBefore) {
                failures.push("unknown facts did not hide display-only");
            }
            if (h.renders.filter((r) => r.method === "ClearPresentation").length !== clearsBefore) {
                failures.push("unknown facts must not reset the stream");
            }
            // The invalidation on unknown dispatches a witness-less capture;
            // the served null coverage can never paint or resurrect.
            h.fireDelay(120);
            h.answerDescribes();
            const setsBefore = h.renders.filter((r) => r.method === "SetPresentation").length;
            h.answerReads((r) => projection(r, "6", "6", {
                foreground: "win-a", preview: null, coveredInstance: null, coveredInvalid: null,
            }));
            if (h.renders.filter((r) => r.method === "SetPresentation").length !== setsBefore) {
                failures.push("unproven null coverage painted after unknown");
            }
            // Same-instance return: no reset either, and a strictly newer
            // covering read paints (recovery without replacement).
            h.fireDelay(25000);
            h.answerEffectFacts(false);
            if (h.renders.filter((r) => r.method === "ClearPresentation").length !== clearsBefore) {
                failures.push("same-instance return must not reset the stream");
            }
            h.fire("windowAdded");
            h.fireDelay(120);
            h.answerDescribes();
            h.drainReads((r) => projection(r, "6", "6", { foreground: "win-a", preview: null, coveredInvalid: "0" }));
            if (paintedA() !== 2) {
                failures.push("covering paint did not render after same-instance return");
            }
            assert.deepEqual(failures, [], `unknown-facts freshness violated: ${failures.join("; ")}`);
        } finally {
            h.stop();
        }
    });
});
