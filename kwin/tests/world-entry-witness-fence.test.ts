// Production-entry FrameWitness fence regressions (Stage 5 v2 producer):
// direct `startWorldAdapterEntry` captures through a scripted strict-v4
// broker. Every test drives the REAL capture fence in world-entry.ts
// (start/end watermark around the actual capture, retained capture mark,
// generation/effect source comparison) and asserts on the emitted
// DescribeWorld report bytes: witness minted vs null, report alive.
// Unit-level derive coverage lives in world-report-v4.test.ts; these are
// the entry-boundary regressions for the capture fence.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { isCanonicalU64 } from "../src/world-seq";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface CapturedCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

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
    clientGeometry: { x: number; y: number; width: number; height: number } | null;
    output: { name: string };
    desktops: FakeDesktop[];
    minimized: boolean;
    hidden: boolean;
    fullscreen: boolean;
    maximized: boolean;
    sticky: boolean;
    keepAbove: boolean;
    keepBelow: boolean;
    minSize: null;
    maxSize: null;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
}

interface FakeSignal {
    readonly handlers: Array<(...args: Array<unknown>) => void>;
    readonly signal: {
        connect: (handler: (...args: Array<unknown>) => void) => void;
        disconnect: (handler: (...args: Array<unknown>) => void) => void;
    };
}

function fakeSignal(): FakeSignal {
    const handlers: Array<(...args: Array<unknown>) => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: (...args: Array<unknown>) => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: (...args: Array<unknown>) => void): void => {
                const index = handlers.indexOf(handler);
                if (index >= 0) {
                    handlers.splice(index, 1);
                }
            },
        },
    };
}

const desk = (id: string): FakeDesktop => ({ id, name: id });

function win(id: string, out: string, ws: FakeDesktop, x: number): FakeWindow {
    return {
        internalId: id, managed: true, deleted: false, transient: false, transientFor: null,
        dialog: false, modal: false, wantsInput: true,
        normalWindow: true,
        frameGeometry: { x, y: 0, width: 200, height: 200 },
        clientGeometry: { x, y: 0, width: 200, height: 200 },
        output: { name: out },
        desktops: [ws],
        minimized: false, hidden: false, fullscreen: false, maximized: false, sticky: false,
        keepAbove: false, keepBelow: false,
        minSize: null, maxSize: null,
        fullScreen: false, maximizeMode: 0, onAllDesktops: false,
    };
}

function factsBytes(instance: string, invalidation: string, rev = "1"): string {
    return JSON.stringify({
        v: 1, instance, rev, invalidation,
        chord: 1, chordKnown: 1, gap: 0, width: 4, extension: 3,
        changed: 1, superseded: 0, available: 1,
    });
}

interface Harness {
    calls: CapturedCall[];
    effectCalls: CapturedCall[];
    timers: Array<{ delay: number; cb: () => void }>;
    signals: Record<string, FakeSignal>;
    wins: FakeWindow[];
    activeRef: { current: FakeWindow };
    hook: { fireOnRead: (() => void) | null };
    handle: ReturnType<typeof startWorldAdapterEntry>;
    boundGen: { value: string };
    answeredEffect: Set<CapturedCall>;
    rejectNext: { payload: string | null };
    holdQueue: { value: boolean };
    runTimers: (maxDelay?: number) => void;
    describes: () => CapturedCall[];
    lastReport: () => Record<string, unknown>;
}

function makeHarness(): Harness {
    const ws1 = desk("ws1");
    const ws2 = desk("ws2");
    const wins = [win("win-a", "out", ws1, 0), win("win-b", "out", ws2, 300)];
    const activeRef = { current: wins[0] as FakeWindow };
    const hook: { fireOnRead: (() => void) | null } = { fireOnRead: null };
    const calls: CapturedCall[] = [];
    const effectCalls: CapturedCall[] = [];
    const timers: Array<{ delay: number; cb: () => void }> = [];
    let describeRev = 0;
    let replyObs = 0;
    const boundGen = { value: "gen-1" };
    const answeredEffect = new Set<CapturedCall>();
    const rejectNext: { payload: string | null } = { payload: null };
    const holdQueue = { value: false };
    const signals: Record<string, FakeSignal> = {
        windowAdded: fakeSignal(),
        windowRemoved: fakeSignal(),
        windowActivated: fakeSignal(),
        desktopsChanged: fakeSignal(),
        screensChanged: fakeSignal(),
        currentDesktopChanged: fakeSignal(),
        activeScreenChanged: fakeSignal(),
    };
    const surface = {
        windowList: () => wins,
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1, ws2],
        activeScreen: { name: "out" },
        cursorPos: { x: 2, y: 100 },
        windowAdded: (signals.windowAdded as FakeSignal).signal,
        windowRemoved: (signals.windowRemoved as FakeSignal).signal,
        windowActivated: (signals.windowActivated as FakeSignal).signal,
        desktopsChanged: (signals.desktopsChanged as FakeSignal).signal,
        screensChanged: (signals.screensChanged as FakeSignal).signal,
        currentDesktopChanged: (signals.currentDesktopChanged as FakeSignal).signal,
        activeScreenChanged: (signals.activeScreenChanged as FakeSignal).signal,
        get activeWindow() {
            const fire = hook.fireOnRead;
            hook.fireOnRead = null;
            if (fire !== null) {
                fire();
            }
            return activeRef.current;
        },
        currentDesktop: ws1,
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => ws1,
        setCurrentDesktopForScreen: () => undefined,
        sendClientToScreen: () => undefined,
        createDesktop: () => undefined,
        removeDesktop: () => undefined,
    };
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            calls.push({ method, payload, callback });
            if (method === "DescribeWorld") {
                if (rejectNext.payload !== null) {
                    const rejected = rejectNext.payload;
                    rejectNext.payload = null;
                    callback(rejected);
                    return;
                }
                const body = JSON.parse(payload) as { source: string; nativeInvalid: string };
                describeRev += 1;
                replyObs += 1;
                callback(acceptedReply(boundGen.value, body.source, body.nativeInvalid, String(describeRev)));
            } else if (method === "QueueWorldCommand") {
                if (holdQueue.value) {
                    return;
                }
                const qbody = JSON.parse(payload) as { client: string; clientCmd: string };
                callback(JSON.stringify({
                    v: 4, outcome: "queued", gen: boundGen.value,
                    client: qbody.client, clientCmd: qbody.clientCmd, globalCmd: "1",
                }));
            } else if (method === "TakeCommandResult") {
                callback(JSON.stringify({ v: 4, outcome: "pending", gen: boundGen.value }));
            }
        },
        effectCallDbus: (_service, _path, _iface, method, payload, callback) => {
            effectCalls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            const timer = { delay, cb };
            timers.push(timer);
            return () => undefined;
        },
        log: () => undefined,
        gestureActive: () => false,
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
    const describes = (): CapturedCall[] => calls.filter((c) => c.method === "DescribeWorld");
    const lastReport = (): Record<string, unknown> => {
        const found = [...describes()].reverse()[0];
        assert.ok(found !== undefined, "expected a DescribeWorld capture");
        return (JSON.parse(found.payload) as { report: Record<string, unknown> }).report;
    };
    return { calls, effectCalls, timers, signals, wins, activeRef, hook, handle, boundGen, answeredEffect, rejectNext, holdQueue, runTimers, describes, lastReport };
}

function answerHandshake(h: Harness, gen = "gen-1"): void {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined, "expected DiscoverWorld/handshake");
    const req = (JSON.parse(found.payload) as Record<string, unknown>)["req"] as string;
    found.callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(h: Harness, gen = "gen-1"): void {
    const found = [...h.calls].reverse().find((c) => {
        if (c.method !== "DiscoverWorld") {
            return false;
        }
        try {
            return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
        } catch {
            return false;
        }
    });
    assert.ok(found !== undefined, "expected DiscoverWorld/settings");
    const body = JSON.parse(found.payload) as Record<string, unknown>;
    found.callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

function acceptedReply(gen: string, source: string, nativeInvalid: string, rev: string): string {
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: source, obs: rev, rev,
        ready: true, consumed: false,
        source, nativeInvalid, clock: "1",
        gestureSettled: null, gestureAssoc: null, operationSettled: [], nativeCreations: [],
        warpCursor: null, intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: [], outputs: [], windows: [], transfers: [], effects: [], closeFocus: null, result: null,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    return text;
}

function discover(h: Harness, gen = "gen-1"): void {
    answerHandshake(h, gen);
    answerSettings(h, gen);
    h.runTimers();
}

function answerFacts(h: Harness, bytes: string): void {
    const pending = h.effectCalls.filter(
        (c) => c.method === "WaitPresentationFacts" && !h.answeredEffect.has(c),
    );
    const waiter = pending[pending.length - 1];
    assert.ok(waiter !== undefined, "expected a held WaitPresentationFacts waiter");
    h.answeredEffect.add(waiter);
    waiter.callback(bytes);
}

function answerQueued(h: Harness): void {
    const call = [...h.calls].reverse().find((c) => c.method === "QueueWorldCommand");
    assert.ok(call !== undefined, "expected a filed QueueWorldCommand");
    const qbody = JSON.parse(call.payload) as { client: string; clientCmd: string };
    call.callback(JSON.stringify({
        v: 4, outcome: "queued", gen: h.boundGen.value,
        client: qbody.client, clientCmd: qbody.clientCmd, globalCmd: "1",
    }));
}

function envelopeOf(call: CapturedCall): Record<string, unknown> {
    return JSON.parse(call.payload) as Record<string, unknown>;
}

function fireHostOnce(h: Harness): void {
    // Single host-cadence sweep (25 s recovery/watchdog timers only): fires
    // the currently pending host timers once so the effect lane re-arms its
    // held waiter after an unknown stand-down. Timers scheduled by the sweep
    // itself are left pending, never cascaded.
    const pending = h.timers.filter((t) => t.delay >= 25000);
    for (const t of pending) {
        const index = h.timers.indexOf(t);
        if (index >= 0) {
            h.timers.splice(index, 1);
            t.cb();
        }
    }
}

function witnessOf(report: Record<string, unknown>): Record<string, unknown> | null {
    return (report["presentation"] as Record<string, unknown>)["witness"] as Record<string, unknown> | null;
}

function fire(h: Harness, name: string, ...args: Array<unknown>): void {
    const sig = h.signals[name] as FakeSignal | undefined;
    assert.ok(sig !== undefined, `expected fake signal ${name}`);
    for (const handler of [...sig.handlers]) {
        handler(...args);
    }
}

describe("production-entry FrameWitness fence", () => {
    it("mints the local witness on a quiet capture with valid cover", () => {
        const h = makeHarness();
        discover(h);
        answerFacts(h, factsBytes("effect-1", "7"));
        h.runTimers();
        const before = h.describes().length;
        h.runTimers();
        assert.ok(h.describes().length >= before, "capture flows");
        const report = h.lastReport();
        const witness = witnessOf(report);
        assert.ok(witness !== null, "quiet capture with valid cover mints a witness");
        assert.equal(witness["win"], "win-a");
        assert.equal(witness["foreground"], "win-a");
        assert.ok(isCanonicalU64(witness["obs"]), "witness obs is canonical");
        assert.deepEqual(witness["frame"], [0, 0, 200, 200]);
        const cover = report["nativeInvalidWitness"] as { instance: string; counter: string };
        assert.deepEqual(cover, { instance: "effect-1", counter: "7" });
        assert.notEqual(witness["obs"], cover.counter, "witness obs is the local watermark, never the effect counter");
        h.handle?.stop();
    });

    it("reentrant invalidation during the capture getter nulls the witness but keeps the report alive", () => {
        const h = makeHarness();
        discover(h);
        answerFacts(h, factsBytes("effect-1", "7"));
        h.runTimers();
        h.runTimers();
        const mark = witnessOf(h.lastReport())?.["obs"] as string;
        assert.ok(isCanonicalU64(mark), "precondition: quiet capture mints");
        // Arm a synchronous native invalidation inside the next capture's
        // activeWindow getter: the entry's own signal handler advances the
        // local watermark mid-capture.
        h.hook.fireOnRead = (): void => {
            fire(h, "desktopsChanged");
        };
        const sentBefore = h.describes().length;
        fire(h, "screensChanged");
        h.runTimers();
        assert.ok(h.describes().length > sentBefore, "straddling capture still sends");
        const sent = envelopeOf(h.describes()[sentBefore] as CapturedCall);
        const report = sent["report"] as Record<string, unknown>;
        assert.equal(witnessOf(report), null, "reentrant invalidation fails the witness closed");
        // The retained report witness stays nulled while the live envelope
        // watermark advances past the capture mark: proof the witness is
        // the retained capture mark, never later send-time state.
        assert.notEqual(sent["nativeInvalid"], mark, "envelope watermark advances past the straddled mark");
        assert.ok(isCanonicalU64(sent["nativeInvalid"]), "advanced envelope watermark is canonical");
        assert.equal(report["focused"], "win-a", "report stays alive on the same foreground");
        assert.deepEqual(
            report["nativeInvalidWitness"],
            { instance: "effect-1", counter: "7" },
            "effect cover still rides the straddled capture",
        );
        // The fence is per-capture: the next quiet capture mints again.
        h.runTimers();
        const recovered = h.lastReport();
        assert.ok(witnessOf(recovered) !== null, "later quiet capture recovers the witness");
        h.handle?.stop();
    });

    it("effect source change mid-capture nulls the witness but keeps the report alive", () => {
        const h = makeHarness();
        discover(h);
        answerFacts(h, factsBytes("effect-1", "7"));
        h.runTimers();
        h.runTimers();
        assert.ok(witnessOf(h.lastReport()) !== null, "precondition: quiet capture mints");
        // Answer a rotated effect instance inside the next capture's
        // getter: the end fence sees a different source than the start.
        h.hook.fireOnRead = (): void => {
            answerFacts(h, factsBytes("effect-2", "0", "2"));
        };
        const sentBefore = h.describes().length;
        fire(h, "screensChanged");
        h.runTimers();
        assert.ok(h.describes().length > sentBefore, "rotated capture still sends");
        const report = (JSON.parse(h.describes()[sentBefore]?.payload as string) as { report: Record<string, unknown> }).report;
        assert.equal(witnessOf(report), null, "effect rotation mid-capture fails the witness closed");
        assert.equal(report["focused"], "win-a", "report stays alive");
        // The next capture over the rotated source mints with fresh cover.
        h.runTimers();
        const next = h.lastReport();
        const minted = witnessOf(next);
        assert.ok(minted !== null, "post-rotation quiet capture mints");
        assert.deepEqual(
            next["nativeInvalidWitness"],
            { instance: "effect-2", counter: "0" },
            "cover follows the rotated source",
        );
        h.handle?.stop();
    });

    it("delayed queue reply sends the retained capture mark", () => {
        const h = makeHarness();
        discover(h);
        answerFacts(h, factsBytes("effect-1", "7"));
        h.runTimers();
        h.runTimers();
        const mark = witnessOf(h.lastReport())?.["obs"] as string;
        assert.ok(isCanonicalU64(mark), "precondition: quiet capture mints");
        // Hold the QueueWorld reply across the async broker gap: the capture
        // is filed but no DescribeWorld may fly before the reply lands.
        h.holdQueue.value = true;
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        const queued = h.calls.filter((c) => c.method === "QueueWorldCommand");
        assert.equal(queued.length, 1, "command files exactly one queue request");
        const sentBefore = h.describes().length;
        answerQueued(h);
        assert.equal(h.describes().length, sentBefore + 1, "queue accept sends exactly one capture");
        const sent = envelopeOf(h.describes()[sentBefore] as CapturedCall);
        const report = sent["report"] as Record<string, unknown>;
        const witness = witnessOf(report);
        assert.ok(witness !== null, "retained capture still proves");
        // No native signal crossed the gap, so the filed mark survives the
        // round trip byte-exact in both the witness and the envelope.
        assert.equal(witness["obs"], mark, "witness obs is the original capture mark");
        assert.equal(sent["nativeInvalid"], mark, "envelope watermark matches the retained mark");
        h.handle?.stop();
    });

    it("queue-gap invalidation recaptures instead of sending the stale mark", () => {
        const h = makeHarness();
        discover(h);
        answerFacts(h, factsBytes("effect-1", "7"));
        h.runTimers();
        h.runTimers();
        const mark = witnessOf(h.lastReport())?.["obs"] as string;
        assert.ok(isCanonicalU64(mark), "precondition: quiet capture mints");
        // Hold the QueueWorld reply, then invalidate across the gap: the
        // actual fileQueue path detects the changed mark and recaptures
        // instead of sending the stale observation.
        h.holdQueue.value = true;
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        assert.equal(h.calls.filter((c) => c.method === "QueueWorldCommand").length, 1, "command files");
        const sentBefore = h.describes().length;
        fire(h, "desktopsChanged");
        answerQueued(h);
        // The queue accept honestly defers to the invalidation-armed
        // debounce (no synchronous preempt of the armed timer, so the burst
        // file pump can never reenter dispatch): the debounce drains a fresh
        // capture after the filings settle.
        h.runTimers();
        const fresh = h.describes().slice(sentBefore);
        assert.ok(fresh.length >= 1, "gap invalidation drains a fresh capture");
        const first = envelopeOf(fresh[0] as CapturedCall);
        const firstReport = first["report"] as Record<string, unknown>;
        const firstWitness = witnessOf(firstReport);
        assert.ok(firstWitness !== null, "fresh capture proves");
        assert.notEqual(firstWitness["obs"], mark, "stale mark is never sent");
        assert.equal(first["nativeInvalid"], firstWitness["obs"], "envelope watermark matches the fresh mark");
        assert.ok(isCanonicalU64(firstWitness["obs"]), "fresh mark is canonical");
        assert.equal(firstReport["focused"], "win-a", "report stays alive");
        for (const call of fresh) {
            const report = (JSON.parse(call.payload) as { report: Record<string, unknown> }).report;
            const witness = witnessOf(report);
            if (witness !== null) {
                assert.notEqual(witness["obs"], mark, "no post-gap capture replays the stale mark");
            }
        }
        h.handle?.stop();
    });

    it("unavailable and malformed effect hide the witness while the report stays alive", () => {
        const h = makeHarness();
        discover(h);
        answerFacts(h, factsBytes("effect-1", "7"));
        h.runTimers();
        h.runTimers();
        assert.ok(witnessOf(h.lastReport()) !== null, "precondition: quiet capture mints");
        answerFacts(h, JSON.stringify({
            v: 1, instance: "effect-1", rev: "2", invalidation: "8",
            chord: 1, chordKnown: 1, gap: 0, width: 4, extension: 3,
            changed: 1, superseded: 0, available: 0,
        }));
        h.runTimers();
        const unavailable = h.lastReport();
        assert.equal(witnessOf(unavailable), null, "unavailable effect hides the witness");
        assert.equal(unavailable["focused"], "win-a", "report stays alive without cover");
        // The unknown lane stands down to the host-deadline recovery chain:
        // sweep it once so the waiter re-arms before the next scripted reply.
        fireHostOnce(h);
        answerFacts(h, factsBytes("effect-1", "9", "3"));
        h.runTimers();
        assert.ok(witnessOf(h.lastReport()) !== null, "valid cover recovers the witness");
        answerFacts(h, "not-json");
        h.runTimers();
        const malformed = h.lastReport();
        assert.equal(witnessOf(malformed), null, "malformed effect hides the witness");
        assert.equal(malformed["focused"], "win-a", "report stays alive on malformed cover");
        h.handle?.stop();
    });

    it("foreground lifetime join mints for the live window and hides for removed or minimized foreground", () => {
        const h = makeHarness();
        discover(h);
        answerFacts(h, factsBytes("effect-1", "7"));
        h.runTimers();
        h.handle?.requestWorkspaceRelative(1);
        h.runTimers();
        // Switch the live foreground to win-b: the witness must join the
        // exact same-capture in-scope lifetime with its own fresh frame.
        h.activeRef.current = h.wins[1] as FakeWindow;
        fire(h, "windowActivated", h.wins[1] as unknown);
        h.runTimers();
        const switched = h.lastReport();
        const joined = witnessOf(switched);
        assert.ok(joined !== null, "live foreground switch still mints");
        assert.equal(joined["win"], "win-b");
        assert.equal(joined["foreground"], "win-b");
        assert.deepEqual(joined["frame"], [300, 0, 200, 200]);
        // Minimized foreground hides narrowly.
        (h.wins[1] as FakeWindow).minimized = true;
        fire(h, "desktopsChanged");
        h.runTimers();
        const minimized = h.lastReport();
        assert.equal(witnessOf(minimized), null, "minimized foreground hides the witness");
        assert.equal(minimized["focused"], "win-b", "minimized foreground stays bound, report stays alive");
        assert.equal(minimized["foregroundKnown"], true, "minimized foreground stays known");
        // Removed foreground hides narrowly.
        (h.wins[1] as FakeWindow).minimized = false;
        (h.wins[1] as FakeWindow).deleted = true;
        fire(h, "desktopsChanged");
        h.runTimers();
        const removed = h.lastReport();
        assert.equal(witnessOf(removed), null, "removed foreground hides the witness");
        assert.equal(removed["focused"], "", "removed foreground is unknown, never a guessed id");
        assert.equal(removed["foregroundKnown"], false, "removed foreground rides known-false");
        h.handle?.stop();
    });

    it("generation rotation resets effect cover and mints again on fresh facts", () => {
        const h = makeHarness();
        discover(h);
        answerFacts(h, factsBytes("effect-1", "7"));
        h.runTimers();
        h.runTimers();
        assert.ok(witnessOf(h.lastReport()) !== null, "precondition: gen-1 capture mints");
        // A mid-capture generation change cannot occur through the real
        // entry: boundGen mutates only in async broker-reply callbacks,
        // never reentrantly inside the synchronous capture read. The
        // rotation fence is therefore exercised across the lifetime
        // boundary: the old effect cover must not cross it.
        h.rejectNext.payload = JSON.stringify({
            v: 4, outcome: "rejected", kind: "generation-mismatch", message: "stale", gen: "gen-2",
            source: "", nativeInvalid: "0", clock: "0", gestureAssoc: null,
        });
        fire(h, "screensChanged");
        h.runTimers();
        // The rejected capture above triggered rediscovery synchronously;
        // answer the NEW handshake/settings round with the new generation.
        h.boundGen.value = "gen-2";
        answerHandshake(h, "gen-2");
        answerSettings(h, "gen-2");
        h.runTimers();
        // The harness auto-answers every DescribeWorld at send time, so the
        // rebound capture is already accepted: assert on its emitted bytes.
        const reboundCalls = h.describes();
        assert.ok(reboundCalls.length > 0, "rebind emits an observation capture");
        const reboundSent = envelopeOf(reboundCalls[reboundCalls.length - 1] as CapturedCall);
        assert.equal(reboundSent["gen"], "gen-2", "rebound capture rides the new generation");
        const rebound = reboundSent["report"] as Record<string, unknown>;
        assert.equal(witnessOf(rebound), null, "rotation drops the old effect cover, failing closed");
        assert.equal(rebound["nativeInvalidWitness"], null, "no stale cover crosses the rotation");
        assert.equal(rebound["focused"], "win-a", "report stays alive across rotation");
        // Fresh facts for the new lifetime mint again with the new cover.
        answerFacts(h, factsBytes("effect-9", "0", "1"));
        h.runTimers();
        const fresh = h.lastReport();
        const minted = witnessOf(fresh);
        assert.ok(minted !== null, "fresh facts mint on the new lifetime");
        assert.deepEqual(
            fresh["nativeInvalidWitness"],
            { instance: "effect-9", counter: "0" },
            "cover follows the rotated source",
        );
        h.handle?.stop();
    });
});
