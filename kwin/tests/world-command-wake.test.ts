// External-command wait wake delivery (B1, strict v4): a foreign queued
// command completes the long-held WaitWorldWake, and the adapter drains it
// with a fresh capture WITHOUT any native event. Re-arming, lease,
// watchdog, stale, in-flight, and rotation behavior included. Offline only:
// scripted in-memory transport, no D-Bus, no KWin.
//
// STRICT V4 SYNTHETIC replies (hand-authored inline, never real-core
// output): the handshake reply echoes the producer adapter lifetime and
// mints the serving generation, the revisioned settings snapshot must read
// accepted before the first report flies, captures echo source correlation
// with scripted obs/rev advancing together (separate from nativeInvalid),
// and terminal results stay requester-only on the take lane.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { startWorldAdapterEntry } from "../src/world-entry";
import { isCoreWorldReply, isWorldWaitReply, parseWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

interface FakeDesktop {
    id: string;
}

const desk = (id: string): FakeDesktop => ({ id });

function windowRow(frame: [number, number, number, number]): Record<string, unknown> {
    return {
        win: "win-a", out: "out", ws: "ws1",
        alloc: [...frame], frame: [...frame],
        frame_auth: "controlled", frame_hold: "",
        member_auth: "controlled", member_hold: "",
        frame_op: "", clear_op: "",
        floating: false, float_origin: "",
        cmdSeq: null, visibility: null,
        visibility_auth: "untouched", visibility_hold: "",
        state: [],
    };
}

function makeSurface() {
    const ws1 = desk("ws1");
    const ws2 = desk("ws2");
    const wins = [
        {
            internalId: "win-a", normalWindow: true, managed: true, deleted: false, transient: false, transientFor: null, dialog: false, modal: false, wantsInput: true,
            frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            // Truthful undecorated client rect: the fake carries no
            // decoration, so the client mirrors the frame exactly.
            clientGeometry: { x: 0, y: 0, width: 200, height: 200 },
            output: { name: "out" }, desktops: [ws1],
            minimized: false, hidden: false, fullscreen: false, maximized: false, sticky: false,
            keepAbove: false, keepBelow: false, minSize: null, maxSize: null,
            // Exact KWin overlay seams: `fullScreen`, `maximizeMode` with
            // the `setMaximize` Q_INVOKABLE, and `onAllDesktops`.
            fullScreen: false,
            maximizeMode: 0,
            onAllDesktops: false,
            setMaximize: function (this: { maximizeMode: number }, vertically: boolean, horizontally: boolean): void {
                this.maximizeMode = vertically && horizontally ? 3 : 0;
            },
        },
    ];
    const screens = [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }];
    const desktops = [ws1, ws2];
    const world = {
        screens, desktops, wins,
        activeScreen: { name: "out" },
        activeWindow: wins[0] as unknown,
        currentDesktop: ws1,
        geometryWrites: [] as string[],
    };
    for (const w of wins) {
        let frame = w.frameGeometry;
        Object.defineProperty(w, "frameGeometry", {
            configurable: true,
            get: () => frame,
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                frame = rect;
                // Keep the truthful undecorated client tracking the frame so
                // no capture pairs a fresh frame with a stale client.
                (w as unknown as Record<string, unknown>)["clientGeometry"] = { ...rect };
                world.geometryWrites.push(`${w.internalId}@${rect.x},${rect.y},${rect.width},${rect.height}`);
                return true;
            },
        });
    }
    const surface = {
        windowList: () => world.wins,
        screens: world.screens,
        desktops: world.desktops,
        activeScreen: world.activeScreen,
        cursorPos: { x: 2, y: 100 },
        get activeWindow() {
            return world.activeWindow;
        },
        set activeWindow(ref: unknown) {
            world.activeWindow = ref;
        },
        get currentDesktop() {
            return world.currentDesktop;
        },
        set currentDesktop(ref: unknown) {
            world.currentDesktop = ref as FakeDesktop;
        },
        clientArea: () => ({ x: 0, y: 0, width: 800, height: 600 }),
        currentDesktopForScreen: () => world.currentDesktop,
        setCurrentDesktopForScreen: (deskRef: unknown) => {
            world.currentDesktop = deskRef as FakeDesktop;
        },
        sendClientToScreen: () => undefined,
        createDesktop: () => null as unknown as object,
        removeDesktop: () => undefined,
    };
    return { surface, world };
}

interface Call {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

interface FakeTimer {
    delay: number;
    cb: () => void;
    live: boolean;
}

function startFlow() {
    const { surface, world } = makeSurface();
    const calls: Call[] = [];
    const timers: FakeTimer[] = [];
    const logs: string[] = [];
    const handle = startWorldAdapterEntry({
        workspace: surface,
        owner: "owner-a",
        adapter: "test-adapter",
        callDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            const entry: FakeTimer = { delay, cb, live: true };
            timers.push(entry);
            return () => {
                entry.live = false;
            };
        },
        log: (message: string) => {
            logs.push(message);
        },
        gestureActive: () => false,
        registerShortcutFn: () => true,
        readWorkspaceModeFn: () => "per-output-local",
    });
    assert.ok(handle !== null);
    return { handle: handle as NonNullable<typeof handle>, calls, timers, world, logs };
}

function lastCall(calls: Call[], method: string): Call {
    const call = [...calls].reverse().find((c) => c.method === method);
    assert.ok(call !== undefined, `expected ${method}`);
    return call as Call;
}

function countCalls(calls: Call[], method: string): number {
    return calls.filter((c) => c.method === method).length;
}

// The read-only presentation subscriber arms its own 25 s host-deadline
// watchdog per read flight (Stage 5, finding A). Offline harnesses share
// one timer pool across lanes, so wait-lane assertions skip the
// presentation retry timer explicitly instead of counting or firing it:
// the marker changes neither its delay nor its cadence, and the skipped
// lane keeps its own coverage in world-presentation-subscriber.test.ts.
function isPresentationTimer(t: FakeTimer): boolean {
    return (t.cb as unknown as Record<string, unknown>)["presentationReadWatchdog"] === true;
}

function sweepTimers(
    flow: ReturnType<typeof startFlow>,
    pred: (delay: number) => boolean,
    cap: number,
): number {
    let fired = 0;
    for (let i = 0; i < flow.timers.length && fired < cap; i += 1) {
        const t = flow.timers[i] as FakeTimer;
        if (!t.live) {
            flow.timers.splice(i, 1);
            i -= 1;
            continue;
        }
        if (pred(t.delay) && !isPresentationTimer(t)) {
            flow.timers.splice(i, 1);
            i -= 1;
            fired += 1;
            t.cb();
        }
    }
    return fired;
}

function runShortTimers(flow: ReturnType<typeof startFlow>): void {
    sweepTimers(flow, (delay) => delay <= 120, 50);
}

function runHostTimers(flow: ReturnType<typeof startFlow>): number {
    return sweepTimers(flow, (delay) => delay === 25000, 1);
}

function liveHostTimers(flow: ReturnType<typeof startFlow>): number {
    return flow.timers.filter((t) => t.live && t.delay === 25000 && !isPresentationTimer(t)).length;
}

function answerHandshake(flow: ReturnType<typeof startFlow>, gen: string): void {
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
    assert.ok(found !== undefined, "expected DiscoverWorld/handshake");
    const req = (JSON.parse((found as Call).payload) as Record<string, unknown>)["req"] as string;
    (found as Call).callback(JSON.stringify({
        type: "handshake-reply", v: 4, adapter: "test-adapter", req,
        ready: false, kind: "", gen,
    }));
}

function answerSettings(flow: ReturnType<typeof startFlow>, gen: string): void {
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
    assert.ok(found !== undefined, "expected DiscoverWorld/settings");
    const body = JSON.parse((found as Call).payload) as Record<string, unknown>;
    (found as Call).callback(JSON.stringify({
        type: "settings-reply", v: 4, adapter: "test-adapter", gen,
        req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
    }));
}

function acceptCapture(
    gen: string,
    envelope: Record<string, unknown>,
    rev: string,
): string {
    // STRICT V4 SYNTHETIC accept (hand-authored inline, never real-core
    // output): the producer adapter lifetime, the serving generation, the
    // capture source correlation, the native watermark, and scripted
    // obs/rev advancing together (separate from nativeInvalid); the
    // terminal result stays null (the take lane serves it
    // requester-only).
    const text = JSON.stringify({
        v: 4, adapter: "test-adapter", gen, req: envelope["source"], obs: rev, rev,
        ready: true, consumed: false,
        source: envelope["source"], nativeInvalid: envelope["nativeInvalid"], clock: "7",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: [], outputs: [],
        windows: [windowRow([10, 10, 300, 300])],
        transfers: [], effects: [], result: null,
    });
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed), "authored accept binds strict v4");
    return text;
}

function captureRejection(gen: string, envelope: Record<string, unknown>): string {
    return JSON.stringify({
        v: 4, outcome: "rejected", kind: "snapshot-invalid",
        message: "snapshot is invalid", gen,
        source: envelope["source"], nativeInvalid: envelope["nativeInvalid"], clock: "7",
        gestureAssoc: null,
    });
}

function wakeReply(gen: string, cursor: string): string {
    return JSON.stringify({ v: 4, outcome: "wake", gen, cursor });
}

function leaseReply(gen: string, cursor: string): string {
    return JSON.stringify({ v: 4, outcome: "lease", gen, cursor });
}

function generationMismatch(gen: string): string {
    return JSON.stringify({
        v: 4, outcome: "rejected", kind: "generation-mismatch",
        message: "generation mismatch", gen, source: "", nativeInvalid: "0", clock: "1",
        gestureAssoc: null,
    });
}

function discoverFlow(flow: ReturnType<typeof startFlow>, gen: string): void {
    // Genuine v4 connection bootstrap: the handshake reply echoes the
    // producer adapter lifetime and mints the serving generation; the
    // revisioned settings snapshot must read accepted before the first
    // report flies.
    answerHandshake(flow, gen);
    answerSettings(flow, gen);
}

// Settle the discovery plus the automatic first capture, leaving the
// adapter idle with exactly one WaitWorldWake outstanding.
function settleIdle(flow: ReturnType<typeof startFlow>, gen: string): Call {
    discoverFlow(flow, gen);
    // The settings accept releases the discovery-triggered observation
    // report on the debounce; pump it so the auto capture is outstanding.
    runShortTimers(flow);
    const auto = lastCall(flow.calls, "DescribeWorld");
    auto.callback(acceptCapture(gen, JSON.parse(auto.payload) as Record<string, unknown>, "1"));
    const wait = lastCall(flow.calls, "WaitWorldWake");
    const body = JSON.parse(wait.payload) as Record<string, unknown>;
    assert.equal(body["gen"], gen);
    assert.equal(body["cursor"], "0");
    return wait;
}

describe("external-command wait wake delivery", () => {
    it("a foreign queued command wakes a capture with zero native events", () => {
        const flow = startFlow();
        const wait = settleIdle(flow, "gen-1");
        const describes = countCalls(flow.calls, "DescribeWorld");
        // No native event fires anywhere in this test: the wake alone must
        // start a capture.
        wait.callback(wakeReply("gen-1", "1"));
        assert.equal(
            countCalls(flow.calls, "DescribeWorld"),
            describes + 1,
            "wake starts a draining capture with no native event",
        );
        // The re-armed wait parks before the drain (continuous coverage).
        const rearmed = lastCall(flow.calls, "WaitWorldWake");
        assert.notEqual(rearmed, wait);
        assert.equal((JSON.parse(rearmed.payload) as Record<string, unknown>)["cursor"], "1");
        // The follow-up capture drains the foreign head.
        const drain = lastCall(flow.calls, "DescribeWorld");
        const body = JSON.parse(drain.payload) as Record<string, unknown>;
        assert.equal(body["gen"], "gen-1");
        drain.callback(acceptCapture("gen-1", body, "2"));
        assert.ok(
            flow.logs.some((l) => l.startsWith("omnitiler:world:applied rev=2")),
            "drained capture applies",
        );
        flow.handle.stop();
    });

    it("burst work drains capture-by-capture with no hot loop", () => {
        const flow = startFlow();
        const wait = settleIdle(flow, "gen-1");
        wait.callback(wakeReply("gen-1", "1"));
        const drain1 = lastCall(flow.calls, "DescribeWorld");
        drain1.callback(acceptCapture("gen-1", JSON.parse(drain1.payload) as Record<string, unknown>, "2"));
        const describes = countCalls(flow.calls, "DescribeWorld");
        // While more work remains the re-armed waiter wakes again.
        const wait2 = lastCall(flow.calls, "WaitWorldWake");
        wait2.callback(wakeReply("gen-1", "2"));
        assert.equal(countCalls(flow.calls, "DescribeWorld"), describes + 1);
        const drain2 = lastCall(flow.calls, "DescribeWorld");
        drain2.callback(acceptCapture("gen-1", JSON.parse(drain2.payload) as Record<string, unknown>, "3"));
        // The queue is empty now: the re-armed waiter parks at the new
        // cursor. A dirty follow-up from the accept may still run once;
        // settle it, then the stream goes quiet without events.
        assert.equal(
            (JSON.parse(lastCall(flow.calls, "WaitWorldWake").payload) as Record<string, unknown>)["cursor"],
            "2",
        );
        runShortTimers(flow);
        const followups = flow.calls.filter((c) => c.method === "DescribeWorld");
        const last = followups[followups.length - 1] as Call;
        if (last !== drain2) {
            last.callback(acceptCapture("gen-1", JSON.parse(last.payload) as Record<string, unknown>, "4"));
            runShortTimers(flow);
        }
        const settled = countCalls(flow.calls, "DescribeWorld");
        runShortTimers(flow);
        assert.equal(
            countCalls(flow.calls, "DescribeWorld"),
            settled,
            "no capture without work or events",
        );
        flow.handle.stop();
    });

    it("a rejected no-drain capture wakes nothing and never hot-loops", () => {
        const flow = startFlow();
        const wait = settleIdle(flow, "gen-1");
        // A fresh observation rejects without draining: the parked waiter
        // must not fire (no epoch advance in the fake server either), and
        // the rejection recovers on the next event only, never a retry.
        flow.handle.requestFocus("left");
        runShortTimers(flow);
        const queued = lastCall(flow.calls, "QueueWorldCommand");
        queued.callback(JSON.stringify({
            v: 4, outcome: "queued", gen: "gen-1",
            client: (JSON.parse(queued.payload) as Record<string, unknown>)["client"],
            clientCmd: (JSON.parse(queued.payload) as Record<string, unknown>)["clientCmd"],
            globalCmd: "1",
        }));
        const flight = lastCall(flow.calls, "DescribeWorld");
        const body = JSON.parse(flight.payload) as Record<string, unknown>;
        flight.callback(captureRejection("gen-1", body));
        const describes = countCalls(flow.calls, "DescribeWorld");
        runShortTimers(flow);
        assert.equal(
            countCalls(flow.calls, "DescribeWorld"),
            describes,
            "rejections recover on the next event, never a retry",
        );
        // The original waiter is still parked at the old cursor: nothing
        // was consumed and nothing was skipped.
        assert.equal(lastCall(flow.calls, "WaitWorldWake"), wait);
        assert.equal(
            (JSON.parse(wait.payload) as Record<string, unknown>)["cursor"],
            "0",
        );
        // The terminal settle still advanced the server epoch behind the
        // rejection, so a wake for the remaining work drains normally.
        wait.callback(wakeReply("gen-1", "1"));
        assert.equal(countCalls(flow.calls, "DescribeWorld"), describes + 1);
        flow.handle.stop();
    });

    it("a silent service re-arms the bound wait at the host deadline, then recovers to wake/drain", () => {
        const flow = startFlow();
        const wait = settleIdle(flow, "gen-1");
        const discs0 = countCalls(flow.calls, "DiscoverWorld");
        // The host kills the long-held call at 25 s with no callback. The
        // bound generation still holds, so the deadline ONLY re-arms the
        // wait with the same cursor: transport silence is never a
        // handshake trigger (world-entry.ts armWait: bound host deadline
        // re-arms; only an unbound generation rediscovers).
        assert.equal(runHostTimers(flow), 1, "wait25 fires the watchdog");
        assert.equal(
            countCalls(flow.calls, "DiscoverWorld"), discs0,
            "bound deadline sends no handshake",
        );
        assert.ok(
            flow.logs.some((l) => l.startsWith("omnitiler:world:wait-host-timeout recovery=rearm")),
            "watchdog logs its re-arm",
        );
        const rearmed = lastCall(flow.calls, "WaitWorldWake");
        assert.notEqual(rearmed, wait, "deadline re-arms exactly once");
        assert.equal(countCalls(flow.calls, "WaitWorldWake"), 2, "one re-arm per deadline");
        assert.equal((JSON.parse(rearmed.payload) as Record<string, unknown>)["gen"], "gen-1");
        assert.equal((JSON.parse(rearmed.payload) as Record<string, unknown>)["cursor"], "0");
        // The host-deadline chain persists without handshakes: a second
        // deadline re-arms exactly once more, linear and bounded.
        assert.equal(runHostTimers(flow), 1, "second deadline fires its watchdog");
        assert.equal(
            countCalls(flow.calls, "DiscoverWorld"), discs0,
            "still no handshake",
        );
        const rearmed2 = lastCall(flow.calls, "WaitWorldWake");
        assert.notEqual(rearmed2, rearmed, "one re-arm per deadline, never ping-pong");
        assert.equal(countCalls(flow.calls, "WaitWorldWake"), 3, "exactly one wait per deadline");
        assert.equal((JSON.parse(rearmed2.payload) as Record<string, unknown>)["gen"], "gen-1");
        assert.equal((JSON.parse(rearmed2.payload) as Record<string, unknown>)["cursor"], "0");
        // The rearmed waiter still wakes work with zero native events.
        rearmed2.callback(wakeReply("gen-1", "1"));
        const drain = lastCall(flow.calls, "DescribeWorld");
        drain.callback(acceptCapture("gen-1", JSON.parse(drain.payload) as Record<string, unknown>, "2"));
        assert.ok(flow.logs.some((l) => l.startsWith("omnitiler:world:applied rev=2")));
        flow.handle.stop();
    });

    it("a late stale callback after a watchdog re-arm never dispatches", () => {
        const flow = startFlow();
        const wait = settleIdle(flow, "gen-1");
        const discs0 = countCalls(flow.calls, "DiscoverWorld");
        // The host kills the long-held call at 25 s with no callback. The
        // generation stays bound, so the deadline ONLY re-arms the wait:
        // transport silence is never a handshake trigger (world-entry.ts
        // armWait: bound host deadline re-arms; only an unbound generation
        // rediscovers).
        assert.equal(runHostTimers(flow), 1, "wait25 fires the watchdog");
        assert.equal(
            countCalls(flow.calls, "DiscoverWorld"), discs0,
            "bound deadline sends no handshake",
        );
        const rearmed = lastCall(flow.calls, "WaitWorldWake");
        assert.notEqual(rearmed, wait);
        const describes = countCalls(flow.calls, "DescribeWorld");
        // The dead call's callback arrives late with a newer cursor: the
        // nonce fence ignores it (the re-armed waiter owns the slot).
        wait.callback(wakeReply("gen-1", "9"));
        assert.equal(countCalls(flow.calls, "DescribeWorld"), describes);
        // The live waiter is unaffected.
        rearmed.callback(wakeReply("gen-1", "1"));
        assert.equal(countCalls(flow.calls, "DescribeWorld"), describes + 1);
        flow.handle.stop();
    });

    it("a wake during a capture flight drains when the flight settles", () => {
        const flow = startFlow();
        settleIdle(flow, "gen-1");
        // Start a local command so a capture flight owns the wire.
        flow.handle.requestFocus("left");
        runShortTimers(flow);
        const queued = lastCall(flow.calls, "QueueWorldCommand");
        queued.callback(JSON.stringify({
            v: 4, outcome: "queued", gen: "gen-1",
            client: (JSON.parse(queued.payload) as Record<string, unknown>)["client"],
            clientCmd: (JSON.parse(queued.payload) as Record<string, unknown>)["clientCmd"],
            globalCmd: "1",
        }));
        const flight = lastCall(flow.calls, "DescribeWorld");
        assert.ok(flight !== undefined);
        const describes = countCalls(flow.calls, "DescribeWorld");
        // Foreign work wakes while the flight is outstanding: owed, not lost.
        const wait = lastCall(flow.calls, "WaitWorldWake");
        wait.callback(wakeReply("gen-1", "1"));
        assert.equal(
            countCalls(flow.calls, "DescribeWorld"),
            describes,
            "no second flight while one owns the wire",
        );
        // The flight settles (take collects pending, then the post-accept
        // tail drains the owed wake on the debounce).
        const body = JSON.parse(flight.payload) as Record<string, unknown>;
        flight.callback(acceptCapture("gen-1", body, "2"));
        const take = lastCall(flow.calls, "TakeCommandResult");
        take.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
        runShortTimers(flow);
        assert.equal(
            countCalls(flow.calls, "DescribeWorld"),
            describes + 1,
            "owed wake drains after the flight settles",
        );
        flow.handle.stop();
    });

    it("an idle lease re-arms with the same cursor and dispatches nothing", () => {
        const flow = startFlow();
        const wait = settleIdle(flow, "gen-1");
        const describes = countCalls(flow.calls, "DescribeWorld");
        wait.callback(leaseReply("gen-1", "0"));
        assert.equal(countCalls(flow.calls, "DescribeWorld"), describes);
        const rearmed = lastCall(flow.calls, "WaitWorldWake");
        assert.notEqual(rearmed, wait);
        assert.equal((JSON.parse(rearmed.payload) as Record<string, unknown>)["cursor"], "0");
        flow.handle.stop();
    });

    it("a generation mismatch holds the wait, rediscovers, and resets the cursor", () => {
        const flow = startFlow();
        settleIdle(flow, "gen-1");
        const wait = lastCall(flow.calls, "WaitWorldWake");
        wait.callback(generationMismatch("gen-2"));
        assert.ok(
            flow.logs.some((l) => l.startsWith("omnitiler:world:generation-mismatch")),
            "mismatch logs its recovery",
        );
        const waits = countCalls(flow.calls, "WaitWorldWake");
        // Held for rediscovery: no re-arm until the new generation binds.
        discoverFlow(flow, "gen-2");
        assert.equal(countCalls(flow.calls, "WaitWorldWake"), waits + 1);
        const rearmed = lastCall(flow.calls, "WaitWorldWake");
        const body = JSON.parse(rearmed.payload) as Record<string, unknown>;
        assert.equal(body["gen"], "gen-2");
        assert.equal(body["cursor"], "0", "cursor resets on a new generation");
        flow.handle.stop();
    });

    it("persistent wait rejections stand down to one host-deadline chain", () => {
        const flow = startFlow();
        settleIdle(flow, "gen-1");
        const rejectWaits = (): void => {
            for (const c of flow.calls) {
                if (c.method === "WaitWorldWake" && (c as unknown as Record<string, unknown>)["answered"] !== true) {
                    (c as unknown as Record<string, unknown>)["answered"] = true;
                    c.callback(JSON.stringify({
                        v: 4, outcome: "rejected", kind: "unknown-adapter",
                        message: "unknown adapter", gen: "gen-1",
                        source: "", nativeInvalid: "0", clock: "1",
                        gestureAssoc: null,
                    }));
                }
            }
        };
        // The first refusal holds the wait for the host deadline instead of
        // re-arming against the stale lifetime in a busy loop. The
        // generation stays bound, so the deadline stands the held wait
        // down (no re-arm, no bound handshake: a held flag alone never
        // authorizes one).
        const discs0 = countCalls(flow.calls, "DiscoverWorld");
        const describes0 = countCalls(flow.calls, "DescribeWorld");
        rejectWaits();
        assert.equal(countCalls(flow.calls, "WaitWorldWake"), 1);
        assert.equal(
            liveHostTimers(flow), 1,
            "exactly one host recovery chain",
        );
        assert.equal(
            countCalls(flow.calls, "DiscoverWorld"), discs0,
            "held refusal files no handshake",
        );
        assert.equal(
            countCalls(flow.calls, "DescribeWorld"), describes0,
            "held refusal dispatches nothing",
        );
        assert.ok(
            flow.logs.some((l) => l.startsWith("omnitiler:world:wait-rejected kind=unknown-adapter")),
            "refusal logs its hold",
        );
        assert.equal(runHostTimers(flow), 1, "recovery fires at the deadline");
        assert.equal(
            countCalls(flow.calls, "WaitWorldWake"), 1,
            "held: no re-arm at the deadline",
        );
        assert.equal(
            countCalls(flow.calls, "DiscoverWorld"), discs0,
            "held: no bound handshake at the deadline",
        );
        assert.ok(
            flow.logs.some((l) => l.startsWith("omnitiler:world:wait-recovery-held recovery=stand-down")),
            "deadline logs its stand-down",
        );
        assert.equal(
            liveHostTimers(flow), 0,
            "stood down: the chain fires once and schedules nothing further",
        );
        // The active producer is undisplaced: a local command still files
        // under the serving generation and drains.
        assert.ok(
            !flow.logs.some((l) => l.includes("queue-dropped")),
            "wait errors never displace the active producer",
        );
        flow.handle.requestFocus("left");
        runShortTimers(flow);
        const queued = lastCall(flow.calls, "QueueWorldCommand");
        assert.equal((JSON.parse(queued.payload) as Record<string, unknown>)["gen"], "gen-1");
        queued.callback(JSON.stringify({
            v: 4, outcome: "queued", gen: "gen-1",
            client: (JSON.parse(queued.payload) as Record<string, unknown>)["client"],
            clientCmd: (JSON.parse(queued.payload) as Record<string, unknown>)["clientCmd"],
            globalCmd: "7",
        }));
        // The filed head drains into exactly one fresh capture: the queue
        // callback sends it synchronously on the fileQueue path, and the
        // accepted drain files its take synchronously the same way.
        const flight = lastCall(flow.calls, "DescribeWorld");
        assert.equal((JSON.parse(flight.payload) as Record<string, unknown>)["gen"], "gen-1");
        const flightBody = JSON.parse(flight.payload) as Record<string, unknown>;
        flight.callback(acceptCapture("gen-1", flightBody, "2"));
        const take = lastCall(flow.calls, "TakeCommandResult");
        take.callback(JSON.stringify({ v: 4, outcome: "pending", gen: "gen-1" }));
        assert.ok(
            flow.logs.some((l) => l.startsWith("omnitiler:world:applied rev=2")),
            "filed command drains and applies under the held wait",
        );
        flow.handle.stop();
    });

    it("a superseded waiter stands down instead of re-superseding the owner", () => {
        const flow = startFlow();
        settleIdle(flow, "gen-1");
        const answerSuperseded = (): void => {
            for (const c of flow.calls) {
                if (c.method === "WaitWorldWake" && (c as unknown as Record<string, unknown>)["answered"] !== true) {
                    (c as unknown as Record<string, unknown>)["answered"] = true;
                    c.callback(JSON.stringify({ v: 4, outcome: "superseded", gen: "gen-1" }));
                }
            }
        };
        // A newer (possibly overlapping-script) waiter owns the slot: stand
        // down, exactly one host recovery timer, no rediscovery storm. The
        // generation stays bound throughout, so no deadline sends a
        // handshake (world-entry.ts scheduleWaitRecovery: a held
        // generation stands down; otherwise the wait re-arms).
        const discs0 = countCalls(flow.calls, "DiscoverWorld");
        answerSuperseded();
        assert.equal(countCalls(flow.calls, "WaitWorldWake"), 1);
        assert.equal(
            liveHostTimers(flow), 1,
            "stand-down keeps exactly one host recovery timer",
        );
        assert.equal(countCalls(flow.calls, "DiscoverWorld"), discs0);
        // One re-arm per deadline, linear and bounded, never ping-pong.
        runHostTimers(flow);
        answerSuperseded();
        assert.equal(countCalls(flow.calls, "WaitWorldWake"), 2);
        runHostTimers(flow);
        answerSuperseded();
        assert.equal(countCalls(flow.calls, "WaitWorldWake"), 3);
        assert.equal(countCalls(flow.calls, "DiscoverWorld"), discs0);
        flow.handle.stop();
    });

    it("a throwing wait transport cancels the watchdog once and keeps one retry chain", () => {
        const { surface } = makeSurface();
        const calls: Call[] = [];
        const timers: FakeTimer[] = [];
        const cancels: number[] = [];
        const logs: string[] = [];
        const handle = startWorldAdapterEntry({
            workspace: surface,
            owner: "owner-a",
            adapter: "test-adapter",
            callDbus: (_s, _p, _i, method, payload, callback) => {
                if (method === "WaitWorldWake") {
                    throw new Error("bus is gone");
                }
                calls.push({ method, payload, callback });
            },
            scheduleOnce: (delay, cb) => {
                const entry: FakeTimer = { delay, cb, live: true };
                timers.push(entry);
                const slot = cancels.length;
                cancels.push(0);
                return () => {
                    cancels[slot] = 1;
                    entry.live = false;
                };
            },
            log: (message: string) => {
                logs.push(message);
            },
            gestureActive: () => false,
            registerShortcutFn: () => true,
            readWorkspaceModeFn: () => "per-output-local",
        });
        assert.ok(handle !== null);
        // Genuine v4 connection bootstrap before the first report flies.
        {
            const hs = [...calls].reverse().find((c) => {
                if (c.method !== "DiscoverWorld") {
                    return false;
                }
                try {
                    return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "handshake";
                } catch {
                    return false;
                }
            });
            assert.ok(hs !== undefined, "expected DiscoverWorld/handshake");
            const req = (JSON.parse(hs.payload) as Record<string, unknown>)["req"] as string;
            hs.callback(JSON.stringify({
                type: "handshake-reply", v: 4, adapter: "test-adapter", req,
                ready: false, kind: "", gen: "gen-1",
            }));
            const st = [...calls].reverse().find((c) => {
                if (c.method !== "DiscoverWorld") {
                    return false;
                }
                try {
                    return (JSON.parse(c.payload) as Record<string, unknown>)["type"] === "settings";
                } catch {
                    return false;
                }
            });
            assert.ok(st !== undefined, "expected DiscoverWorld/settings");
            const body = JSON.parse(st.payload) as Record<string, unknown>;
            st.callback(JSON.stringify({
                type: "settings-reply", v: 4, adapter: "test-adapter", gen: "gen-1",
                req: body["req"], rev: body["rev"], status: "accepted", diagnostics: [],
            }));
            for (let i = 0; i < timers.length; i += 1) {
                const t = timers[i] as FakeTimer;
                if (t.delay <= 120) {
                    timers.splice(i, 1);
                    i -= 1;
                    t.cb();
                }
            }
        }
        const auto = lastCall(calls, "DescribeWorld");
        auto.callback(acceptCapture("gen-1", JSON.parse(auto.payload) as Record<string, unknown>, "1"));
        // The armed wait threw synchronously: its orphaned watchdog is
        // cancelled exactly once and exactly one retry chain lives.
        assert.equal(timers.filter((t) => t.live && t.delay === 25000 && !isPresentationTimer(t)).length, 1, "exactly one retry chain");
        const invokedAfterThrow = cancels.filter((c) => c === 1).length;
        assert.ok(invokedAfterThrow >= 1, "orphaned watchdog cancelled, not orphaned");
        assert.ok(logs.some((l) => l.startsWith("omnitiler:world:wait-transport")));
        // Firing the retry re-arms (and throws) without doubling the chain.
        // Fire exactly one: the retry arms its successor host-deadline
        // causally, and an uncapped sweep would collapse that cadence.
        // Cancelled entries never fire (a real stopped QTimer is silent).
        let fired = 0;
        for (let i = 0; i < timers.length && fired < 1; i += 1) {
            const t = timers[i] as FakeTimer;
            if (!t.live) {
                timers.splice(i, 1);
                i -= 1;
                continue;
            }
            if (t.delay === 25000 && !isPresentationTimer(t)) {
                timers.splice(i, 1);
                i -= 1;
                fired += 1;
                t.cb();
            }
        }
        assert.equal(fired, 1);
        assert.equal(timers.filter((t) => t.live && t.delay === 25000 && !isPresentationTimer(t)).length, 1, "retry never overwrites a live chain");
        assert.equal(
            cancels.filter((c) => c === 1).length, invokedAfterThrow + 1,
            "second orphaned watchdog cancelled exactly once",
        );
        handle?.stop();
    });

    it("wait wire shapes parse strictly", () => {
        assert.deepEqual(
            parseWorldReply(wakeReply("w-1", "4")),
            { outcome: "wake", gen: "w-1", cursor: "4" },
        );
        assert.deepEqual(
            parseWorldReply(leaseReply("w-1", "4")),
            { outcome: "lease", gen: "w-1", cursor: "4" },
        );
        const superseded = { v: 4, outcome: "superseded", gen: "w-1" };
        assert.ok(isWorldWaitReply(parseWorldReply(JSON.stringify(superseded))));
        assert.equal(parseWorldReply(JSON.stringify({ v: 1, outcome: "wake", gen: "w-1", cursor: "4" })), null);
        assert.equal(parseWorldReply(JSON.stringify({ v: 4, outcome: "wake", gen: "w-1", cursor: "04" })), null);
        assert.equal(
            parseWorldReply(JSON.stringify({ v: 4, outcome: "wake", gen: "w-1", cursor: "4", extra: 1 })),
            null,
        );
        assert.ok(!isWorldWaitReply(parseWorldReply(generationMismatch("w-1"))));
    });
});
