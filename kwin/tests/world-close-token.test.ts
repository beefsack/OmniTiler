// Unit 5 regression: exact serialized close-token retirement at capture
// incorporation, checked broker race (A sent, B natively proved while A in
// flight, accepted skipped apply retires A only), capped subset intersection,
// refusal/retry/fence/lifetime controls, exact terminal redelivery, and
// incomplete-inventory non-close.

import assert from "node:assert/strict";
import { describe, it } from "node:test";

import { createWorldCloseTracker } from "../src/world-close-tracker";
import { captureWorldObservation } from "../src/world-observe";
import { isCoreWorldReply, parseWorldReply } from "../src/world-report";
import { WorldAdapterState } from "../src/world-state";
import { startWorldAdapterEntry } from "../src/world-entry";
import { DISABLED_PRESENTATION } from "./world-wire-harness";
import { discover, flushReads, makeHarness } from "./world-producer-harness";

function validReplyJson(over: Record<string, unknown> = {}): string {
    return JSON.stringify({
        v: 4,
        adapter: "test-adapter",
        gen: "gen-1",
        req: "src-1",
        source: "src-1",
        obs: "1",
        rev: "1",
        ready: true,
        consumed: false,
        scopes: [{ scope: { kind: "output", out: "out" }, auth: "controlled", reason: "" }],
        workspaces: [],
        outputs: [],
        windows: [],
        transfers: [],
        effects: [],
        dragPreview: null,
        gestureSettled: null,
        gestureAssoc: null,
        gestureSettledAssoc: null,
        focusRequest: null,
        operationSettled: [],
        nativeCreations: [],
        warpCursor: null,
        presentation: DISABLED_PRESENTATION,
        intentDesired: [],
        switchSettled: [],
        diagnostics: [],
        nativeInvalid: "1",
        clock: "0",
        ...over,
    });
}

function makeMinimalSurface(ids: ReadonlyArray<string>, opts: { complete?: boolean; deletedIds?: ReadonlyArray<string> } = {}) {
    const ws1 = { id: "ws1" };
    const deletedSet = new Set(opts.deletedIds ?? []);
    const windows = ids.map((id) => ({
        internalId: id,
        resourceClass: "OmniTilerTestWindow",
        managed: true,
        deleted: deletedSet.has(id),
        transient: false,
        transientFor: null,
        dialog: false,
        modal: false,
        wantsInput: true,
        normalWindow: true,
        output: { name: "out" },
        desktops: [ws1],
        minimized: false,
        hidden: false,
        fullScreen: false,
        maximizeMode: 0,
        onAllDesktops: false,
        minSize: null,
        maxSize: null,
        frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
    }));
    const listObj = opts.complete === false
        ? {
            length: 10001,
            get 0() {
                return windows[0];
            },
        }
        : windows;
    return {
        windowList: () => listObj,
        screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
        desktops: [ws1],
        activeScreen: { name: "out" },
        activeWindow: windows[0] ?? null,
        currentDesktopForScreen: () => ws1,
        clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1080 }),
    };
}

describe("Unit 5 close-token retirement", () => {
    it("retires only A on accepted-but-skipped reply when B closes during flight (checked broker)", async () => {
        const h = await makeHarness("w1", ["w1", "w2", "w3"]);
        try {
            // Settle initial handshake and initial 3-window admission through the checked broker.
            await discover(h);
            await flushReads(h);

            // Close A (w1): remove from live list and fire windowRemoved.
            const idxA = h.wins.findIndex((w) => w.internalId === "w1");
            assert.ok(idxA >= 0);
            h.wins.splice(idxA, 1);
            h.fire("windowRemoved");
            h.runTimers();

            // Capture flight for A is now in flight; find its DescribeWorld call
            // and get the real checked broker reply WITHOUT delivering it yet.
            const flightAIdx = h.calls.findIndex((c, idx) => !h.answered.has(idx) && c.method === "DescribeWorld");
            assert.ok(flightAIdx >= 0, "expected DescribeWorld flight for close A");
            const flightACall = h.calls[flightAIdx]!;
            const flightAPayload = JSON.parse(flightACall.payload) as { source: string; report: { closed: string[] } };
            assert.deepEqual(flightAPayload.report.closed, ["w1"]);

            const checkedReplyA = await h.bridge.send("DescribeWorld", flightACall.payload);

            // While A is in flight, close B (w2) natively!
            h.geometryWrites.length = 0;
            h.focused.length = 0;
            const idxB = h.wins.findIndex((w) => w.internalId === "w2");
            assert.ok(idxB >= 0);
            h.wins.splice(idxB, 1);
            h.fire("windowRemoved");

            // Deliver A's bound ready reply: accepted, but watermark is superseded.
            h.answered.add(flightAIdx);
            h.replies.set(flightAIdx, checkedReplyA);
            flightACall.callback(checkedReplyA);

            // Answer any TakeCommandResult from A's skipped reply.
            for (let i = 0; i < h.calls.length; i += 1) {
                if (!h.answered.has(i) && h.calls[i]!.method === "TakeCommandResult") {
                    const takeCall = h.calls[i]!;
                    const takeReply = await h.bridge.send("TakeCommandResult", takeCall.payload);
                    h.answered.add(i);
                    h.replies.set(i, takeReply);
                    takeCall.callback(takeReply);
                }
            }

            assert.equal(h.geometryWrites.length, 0, "zero geometry writes from skipped reply A");
            assert.equal(h.focused.length, 0, "zero focus writes from skipped reply A");

            // Next capture must carry B (w2) ONLY, never A (w1).
            h.runTimers();
            const flightBIdx = h.calls.findIndex((c, idx) => !h.answered.has(idx) && c.method === "DescribeWorld");
            assert.ok(flightBIdx >= 0, "expected follow-up DescribeWorld flight for close B");
            const flightBCall = h.calls[flightBIdx]!;
            const flightBPayload = JSON.parse(flightBCall.payload) as { source: string; report: { closed: string[] } };
            assert.deepEqual(flightBPayload.report.closed, ["w2"], "A retired at incorporation; only B reaches next capture");

            // Settle B through the checked broker and verify native reflow occurs.
            const checkedReplyB = await h.bridge.send("DescribeWorld", flightBCall.payload);
            h.answered.add(flightBIdx);
            h.replies.set(flightBIdx, checkedReplyB);
            flightBCall.callback(checkedReplyB);
            for (let i = 0; i < h.calls.length; i += 1) {
                if (!h.answered.has(i) && h.calls[i]!.method === "TakeCommandResult") {
                    const takeCall = h.calls[i]!;
                    const takeReply = await h.bridge.send("TakeCommandResult", takeCall.payload);
                    h.answered.add(i);
                    h.replies.set(i, takeReply);
                    takeCall.callback(takeReply);
                }
            }
            assert.ok(h.geometryWrites.length > 0, "later reflow executes after B settles");
        } finally {
            h.handle.stop();
            await h.bridge.close();
        }
    });

    it("consumes only intersection of tracked closed and final capped/filtered observation.closed", () => {
        const tracker = createWorldCloseTracker();
        const initialIds: string[] = [];
        for (let i = 0; i < 10000; i += 1) {
            initialIds.push(`win-${i}`);
        }
        tracker.syncLive({ windowList: () => initialIds.map((id) => ({ internalId: id })) });
        tracker.noteRemovedEvent();
        tracker.syncLive({ windowList: () => [] });
        assert.equal(tracker.snapshotClosed().length, 10000);

        // Serialized cap invariant: tracked closes ride first, lifecycle-only
        // deletes append after them, so overflow cuts the lifecycle tail.
        const obs = captureWorldObservation({
            nativePerOutput: true,
            workspace: makeMinimalSurface(["win-0", "deleted-extra-1", "deleted-extra-2"], { deletedIds: ["deleted-extra-1", "deleted-extra-2"] }),
            readMode: () => "vertical",
            readGap: () => 0,
            readOuterGap: () => 0,
            gestureActive: () => false,
            knownClosed: () => tracker.snapshotClosed(),
            logicalBindingOf: () => ({ logical: "ws1", out: "out", pin: false, disp: false, managed: true }),
        });
        assert.ok(obs !== null);
        assert.equal(obs.closed.includes("win-0"), false, "live win-0 filtered from serialized closed");
        assert.equal(obs.closed.length, 10000, "serialized closed hits MAX_CLOSED");
        assert.equal(obs.closed[0], "win-1", "tracked entries first in tracked order");
        assert.equal(obs.closed[9998], "win-9999", "last tracked entry survives the cap");
        assert.equal(obs.closed[9999], "deleted-extra-1", "pre-cap lifecycle entry rides last");
        assert.equal(obs.closed.includes("deleted-extra-2"), false, "overflow lifecycle tail cut, tracked ids intact");
        assert.equal(obs.closed.filter((id) => id.startsWith("win-")).length, 9999, "all 9999 non-live tracked closes serialized");

        const token = tracker.freezeToken("gen-1", "src-1", obs.closed);
        assert.equal(token.closed.length, 9999, "freeze intersects to tracked only; lifecycle-only never becomes a credit");
        const retired = tracker.retireToken(token, "gen-1", "src-1");
        assert.equal(retired, 9999);
        assert.deepEqual(tracker.snapshotClosed(), ["win-0"]);
    });

    it("does not retire on lost, mismatched, not-ready, refused, or wrong-lifetime/gen ACKs", () => {
        const trackerA = createWorldCloseTracker();
        const trackerB = createWorldCloseTracker();
        trackerA.syncLive({ windowList: () => [{ internalId: "win-1" }] });
        trackerA.noteRemovedEvent();
        trackerA.syncLive({ windowList: () => [] });

        const state = new WorldAdapterState("test-adapter", "test-owner");
        state.onDiscovery("gen-1", "test-adapter");

        const token = trackerA.freezeToken("gen-1", "src-1", ["win-1"]);
        assert.equal(Object.isFrozen(token), true);
        assert.equal(Object.isFrozen(token.closed), true);

        // Wrong tracker lifetime.
        assert.equal(trackerB.retireToken(token, "gen-1", "src-1"), 0);
        // Wrong generation.
        assert.equal(trackerA.retireToken(token, "gen-2", "src-1"), 0);
        assert.equal(trackerA.retireToken(token, null, "src-1"), 0);
        // Wrong source.
        assert.equal(trackerA.retireToken(token, "gen-1", "src-2"), 0);
        assert.deepEqual(trackerA.snapshotClosed(), ["win-1"]);

        // Not-ready reply in WorldAdapterState.
        const flight = state.beginCapture("src-1", null, token);
        assert.ok(flight !== null);
        const notReady = parseWorldReply(validReplyJson({ ready: false }));
        assert.ok(notReady !== null && isCoreWorldReply(notReady));
        if (isCoreWorldReply(notReady)) {
            assert.equal(state.acceptReply(notReady, "src-1", flight.watermark).applied, false);
        }
        assert.deepEqual(trackerA.snapshotClosed(), ["win-1"]);

        // Generation rotation fences old flight; fresh capture in gen-2 redelivers win-1.
        state.onDiscovery("gen-2", "test-adapter");
        assert.equal(trackerA.retireToken(token, state.boundGen, "src-1"), 0);
        assert.deepEqual(trackerA.snapshotClosed(), ["win-1"]);
    });

    it("rebinds exact frozen close token on terminal redelivery without consuming newer closes", () => {
        const ws1 = { id: "ws1" };
        let liveIds = ["win-a", "win-b"];
        const surface = {
            windowList: () => liveIds.map((id) => ({
                internalId: id,
                resourceClass: "OmniTilerTestWindow",
                managed: true,
                deleted: false,
                transient: false,
                transientFor: null,
                dialog: false,
                modal: false,
                wantsInput: true,
                normalWindow: true,
                output: { name: "out" },
                desktops: [ws1],
                minimized: false,
                hidden: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
                minSize: null,
                maxSize: null,
                frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            })),
            screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
            desktops: [ws1],
            activeScreen: { name: "out" },
            activeWindow: null,
            currentDesktopForScreen: () => ws1,
            clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1080 }),
            windowAdded: { connect: () => {}, disconnect: () => {} },
            windowRemoved: { connect: () => {}, disconnect: () => {} },
            windowActivated: { connect: () => {}, disconnect: () => {} },
            currentDesktopChanged: { connect: () => {}, disconnect: () => {} },
            screensChanged: { connect: () => {}, disconnect: () => {} },
            virtualDesktopsChanged: { connect: () => {}, disconnect: () => {} },
        };

        const tracker = createWorldCloseTracker();
        tracker.syncLive(surface);
        // Close win-a -> freeze token for terminal capture.
        liveIds = ["win-b"];
        tracker.noteRemovedEvent();
        tracker.syncLive(surface);
        const tokenA = tracker.freezeToken("gen-1", "src-1", tracker.snapshotClosed());
        assert.deepEqual(tokenA.closed, ["win-a"]);

        const state = new WorldAdapterState("test-adapter", "test-owner");
        state.onDiscovery("gen-1", "test-adapter");
        const flight1 = state.beginCapture("src-1", null, tokenA);
        assert.ok(flight1 !== null);
        const watermark1 = flight1.watermark;

        // Flight 1 times out; meanwhile win-b also closes.
        state.noteRejectedOrTransportError();
        liveIds = [];
        tracker.noteRemovedEvent();
        tracker.syncLive(surface);
        assert.deepEqual(tracker.snapshotClosed(), ["win-a", "win-b"]);

        // Replay rebinds original tokenA (NOT fresh [win-a, win-b]).
        const replay = state.beginReplayCapture("src-1", watermark1, null, tokenA);
        assert.ok(replay !== null);
        assert.deepEqual(replay.closedToken?.closed, ["win-a"]);

        const reply = parseWorldReply(validReplyJson({ ready: true }));
        assert.ok(reply !== null && isCoreWorldReply(reply));
        if (isCoreWorldReply(reply)) {
            assert.equal(state.acceptReply(reply, "src-1", watermark1).applied, true);
            assert.equal(tracker.retireToken(replay.closedToken!, state.boundGen, reply.source), 1);
        }
        // Only win-a retired; win-b survives!
        assert.deepEqual(tracker.snapshotClosed(), ["win-b"]);
    });

    it("does not fabricate closes from an incomplete windowList", () => {
        const tracker = createWorldCloseTracker();
        tracker.syncLive(makeMinimalSurface(["win-a", "win-b"]));
        tracker.noteRemovedEvent();
        // Incomplete list (over-cap / unreadable) defers without fabricating closes.
        tracker.syncLive(makeMinimalSurface(["win-a"], { complete: false }));
        assert.deepEqual(tracker.snapshotClosed(), []);
        // Subsequent complete list promotes only the actually absent window.
        tracker.syncLive(makeMinimalSurface(["win-a"]));
        assert.deepEqual(tracker.snapshotClosed(), ["win-b"]);
    });

    it("entry integration: does not retire close when reply is refused or generation rotates", () => {
        const ws1 = { id: "ws1" };
        let liveIds = ["win-a", "win-b"];
        const windowRemovedHandlers: Array<() => void> = [];
        const calls: Array<{ method: string; payload: string; callback: (r: unknown) => void }> = [];
        const logs: string[] = [];
        const timers: Array<{ delay: number; cb: () => void; cancelled: boolean }> = [];
        const runTimers = (maxDelay = 200): void => {
            let ran = true;
            while (ran) {
                ran = false;
                for (const entry of timers.splice(0)) {
                    if (!entry.cancelled && entry.delay <= maxDelay) {
                        ran = true;
                        entry.cb();
                    } else if (!entry.cancelled) {
                        timers.push(entry);
                    }
                }
            }
        };
        const surface = {
            windowList: () => liveIds.map((id) => ({
                internalId: id,
                resourceClass: "OmniTilerTestWindow",
                managed: true,
                deleted: false,
                transient: false,
                transientFor: null,
                dialog: false,
                modal: false,
                wantsInput: true,
                normalWindow: true,
                output: { name: "out" },
                desktops: [ws1],
                minimized: false,
                hidden: false,
                fullScreen: false,
                maximizeMode: 0,
                onAllDesktops: false,
                minSize: null,
                maxSize: null,
                frameGeometry: { x: 0, y: 0, width: 200, height: 200 },
            })),
            screens: [{ name: "out", geometry: { x: 0, y: 0, width: 1920, height: 1080 } }],
            desktops: [ws1],
            activeScreen: { name: "out" },
            activeWindow: null,
            currentDesktopForScreen: () => ws1,
            clientArea: () => ({ x: 0, y: 0, width: 1920, height: 1080 }),
            windowAdded: { connect: () => {}, disconnect: () => {} },
            windowRemoved: {
                connect: (fn: () => void) => { windowRemovedHandlers.push(fn); },
                disconnect: () => {},
            },
            windowActivated: { connect: () => {}, disconnect: () => {} },
            currentDesktopChanged: { connect: () => {}, disconnect: () => {} },
            screensChanged: { connect: () => {}, disconnect: () => {} },
            virtualDesktopsChanged: { connect: () => {}, disconnect: () => {} },
        };
        const handle = startWorldAdapterEntry({
            options: { perOutputVirtualDesktops: true },
            workspace: surface,
            owner: "test-owner",
            adapter: "test-adapter",
            scopeClass: "OmniTilerTestWindow",
            callDbus: (_s, _p, _i, method, payload, callback) => {
                if (method === "ReadFloatIntent") {
                    const body = JSON.parse(payload) as { correlation_id?: string };
                    callback(JSON.stringify({ v: 2, correlation_id: body.correlation_id ?? "1", ok: true, records: [] }));
                    return;
                }
                if (method === "WriteFloatIntent") {
                    const body = JSON.parse(payload) as { correlation_id?: string };
                    callback(JSON.stringify({ v: 2, correlation_id: body.correlation_id ?? "1", ok: true }));
                    return;
                }
                calls.push({ method, payload, callback });
            },
            scheduleOnce: (delay, cb) => {
                const entry = { delay, cb, cancelled: false };
                timers.push(entry);
                return () => { entry.cancelled = true; };
            },
            log: (m) => { logs.push(m); },
        });
        assert.ok(handle !== null);
        try {
            const settleDiscover = (gen: string): void => {
                for (let step = 0; step < 4; step += 1) {
                    const idx = calls.findIndex((c) => c.method === "DiscoverWorld");
                    if (idx < 0) {
                        break;
                    }
                    const disc = calls.splice(idx, 1)[0]!;
                    const body = JSON.parse(disc.payload) as { type?: string; req?: string; rev?: string };
                    if (body.type === "handshake") {
                        disc.callback(JSON.stringify({
                            type: "handshake-reply", v: 4, adapter: "test-adapter",
                            req: body.req ?? "1", ready: false, kind: "", gen,
                        }));
                    } else {
                        disc.callback(JSON.stringify({
                            type: "settings-reply", v: 4, adapter: "test-adapter",
                            gen, req: body.req ?? "1", rev: body.rev ?? "1",
                            status: "accepted", diagnostics: [],
                        }));
                    }
                }
            };
            const nextDescribe = (): { method: string; payload: string; callback: (r: unknown) => void } | undefined => {
                const idx = calls.findIndex((c) => c.method === "DescribeWorld");
                return idx >= 0 ? calls.splice(idx, 1)[0] : undefined;
            };
            // Complete initial DiscoverWorld (handshake + settings).
            settleDiscover("gen-1");
            const cap0 = nextDescribe();
            assert.ok(cap0 !== undefined);
            cap0.callback(validReplyJson({
                req: JSON.parse(cap0.payload).source,
                source: JSON.parse(cap0.payload).source,
            }));
            runTimers();

            // Close win-a -> DescribeWorld sends closed: ["win-a"].
            liveIds = ["win-b"];
            windowRemovedHandlers[0]?.();
            runTimers();
            const cap1 = nextDescribe();
            assert.ok(cap1 !== undefined);
            assert.deepEqual(JSON.parse(cap1.payload).report.closed, ["win-a"]);

            // Refuse cap1 with busy -> win-a must NOT retire.
            // Next native event triggers a fresh capture carrying win-a.
            cap1.callback(JSON.stringify({
                v: 4, outcome: "rejected", kind: "busy", message: "busy",
                gen: "gen-1", source: JSON.parse(cap1.payload).source, nativeInvalid: "1",
                clock: "0", gestureAssoc: null, gestureSettledAssoc: null,
            }));
            windowRemovedHandlers[0]?.();
            runTimers();
            const cap2 = nextDescribe();
            assert.ok(cap2 !== undefined);
            assert.deepEqual(JSON.parse(cap2.payload).report.closed, ["win-a"], "still-owed close redelivers after refusal");

            // Generation-mismatch -> rediscover gen-2 -> fresh capture in gen-2 still carries win-a.
            cap2.callback(JSON.stringify({
                v: 4, outcome: "rejected", kind: "generation-mismatch", message: "stale",
                gen: "gen-1", source: JSON.parse(cap2.payload).source, nativeInvalid: "1",
                clock: "0", gestureAssoc: null, gestureSettledAssoc: null,
            }));
            settleDiscover("gen-2");
            windowRemovedHandlers[0]?.();
            runTimers();
            const cap3 = nextDescribe();
            assert.ok(cap3 !== undefined);
            assert.equal(JSON.parse(cap3.payload).gen, "gen-2");
            assert.deepEqual(JSON.parse(cap3.payload).report.closed, ["win-a"], "still-owed close redelivers in new generation");
            const closeAckLines = (): string[] => logs.filter((l) => l.includes("ev=close-ack"));
            const describeBody = (cap: { payload: string }): { gen: string; source: string; nativeInvalid: string; report: { closed: string[] } } => JSON.parse(cap.payload) as { gen: string; source: string; nativeInvalid: string; report: { closed: string[] } };
            assert.equal(closeAckLines().length, 0, "busy and generation-mismatch never emit close-ack");

            // Timeout (existing WORLD_TIMEOUT_MS flight timer): cap3 times out, exact close survives.
            const cap3Body = describeBody(cap3);
            const staleCallback = cap3.callback;
            runTimers(2000);
            // runTimers(2000) iterates newly scheduled timers up to 2000, so it
            // also fires the follow-up catchup capture's own timeout in the
            // same call: the catchup DescribeWorld queued here is already
            // settled/dead, never the active flight. Drain it so cap4 below
            // is the live pending capture from the next native event.
            const timedOutQueued = nextDescribe();
            assert.ok(timedOutQueued !== undefined, "timeout chain queues a catchup capture in the same runTimers call");
            assert.equal(describeBody(timedOutQueued).gen, "gen-2");
            assert.deepEqual(describeBody(timedOutQueued).report.closed, ["win-a"], "timed-out catchup still carries the exact close");
            assert.equal(calls.filter((c) => c.method === "DescribeWorld").length, 0, "only the consumed catchup was queued by runTimers(2000)");
            assert.equal(closeAckLines().length, 0, "timeout never emits close-ack");
            windowRemovedHandlers[0]?.();
            runTimers();
            const cap4 = nextDescribe();
            assert.ok(cap4 !== undefined, "fresh capture after timeout");
            assert.notEqual(describeBody(cap4).source, describeBody(timedOutQueued).source, "cap4 is the live pending capture, not the consumed catchup");
            assert.equal(describeBody(cap4).gen, "gen-2");
            assert.deepEqual(describeBody(cap4).report.closed, ["win-a"], "exact close survives flight timeout");
            const cap4Body = describeBody(cap4);

            // Delayed stale bound-ready for timed-out cap3: cannot retire, cannot disturb cap4.
            const describeBeforeStale = calls.filter((c) => c.method === "DescribeWorld").length;
            staleCallback(validReplyJson({
                gen: "gen-2", req: cap3Body.source, source: cap3Body.source,
                nativeInvalid: cap3Body.nativeInvalid, rev: "2", obs: "2",
            }));
            assert.equal(calls.filter((c) => c.method === "DescribeWorld").length, describeBeforeStale, "stale callback dispatches nothing while newer flight owns the wire");
            assert.equal(closeAckLines().length, 0, "stale bound-ready never emits close-ack");

            // Actual not-ready on undisturbed cap4: auto-dispatches next capture still carrying win-a.
            cap4.callback(validReplyJson({
                gen: "gen-2", req: cap4Body.source, source: cap4Body.source,
                nativeInvalid: cap4Body.nativeInvalid, ready: false, rev: "2", obs: "2",
            }));
            runTimers();
            const cap5 = nextDescribe();
            assert.ok(cap5 !== undefined, "undisturbed cap4 auto-dispatches after not-ready");
            assert.equal(describeBody(cap5).gen, "gen-2");
            assert.deepEqual(describeBody(cap5).report.closed, ["win-a"], "exact close survives actual not-ready reply");
            assert.notEqual(describeBody(cap5).source, cap3Body.source, "recovery mints a fresh source, never the stale one");
            assert.equal(closeAckLines().length, 0, "not-ready never emits close-ack");

            // Transport failure on cap5 (non-string reply): exact close survives.
            cap5.callback(null);
            assert.equal(closeAckLines().length, 0, "transport failure never emits close-ack");
            windowRemovedHandlers[0]?.();
            runTimers();
            const cap6 = nextDescribe();
            assert.ok(cap6 !== undefined, "fresh capture after transport failure");
            assert.deepEqual(describeBody(cap6).report.closed, ["win-a"], "exact close survives transport failure");

            // Busy refusal on cap6: exact close survives, branch emits no close-ack.
            cap6.callback(JSON.stringify({
                v: 4, outcome: "rejected", kind: "busy", message: "busy",
                gen: "gen-2", source: describeBody(cap6).source, nativeInvalid: "1",
                clock: "0", gestureAssoc: null, gestureSettledAssoc: null,
            }));
            assert.equal(closeAckLines().length, 0, "busy never emits close-ack");
            windowRemovedHandlers[0]?.();
            runTimers();
            const cap7 = nextDescribe();
            assert.ok(cap7 !== undefined, "fresh capture after busy refusal");
            assert.deepEqual(describeBody(cap7).report.closed, ["win-a"], "exact close survives busy refusal in next serialized report");

            // Generic refused reply on cap7: exact close survives.
            cap7.callback(JSON.stringify({
                v: 4, outcome: "rejected", kind: "operation-refused", message: "refused",
                gen: "gen-2", source: describeBody(cap7).source, nativeInvalid: "1",
                clock: "0", gestureAssoc: null, gestureSettledAssoc: null,
            }));
            assert.equal(closeAckLines().length, 0, "refused never emits close-ack");
            windowRemovedHandlers[0]?.();
            runTimers();
            const cap8 = nextDescribe();
            assert.ok(cap8 !== undefined, "fresh capture after generic refusal");
            assert.deepEqual(describeBody(cap8).report.closed, ["win-a"], "exact close survives refused reply in next serialized report");

            // Genuine bound-ready on cap8 retires through the real retirement path.
            const cap8Body = describeBody(cap8);
            cap8.callback(validReplyJson({
                gen: "gen-2", req: cap8Body.source, source: cap8Body.source,
                nativeInvalid: cap8Body.nativeInvalid, rev: "2", obs: "2",
            }));
            windowRemovedHandlers[0]?.();
            runTimers();
            const cap9 = nextDescribe();
            assert.ok(cap9 !== undefined, "fresh capture after genuine ready");
            assert.deepEqual(describeBody(cap9).report.closed, [], "genuine bound-ready retires exact close once");
            const ack = closeAckLines().find((l) => l.includes(`req=${cap8Body.source}`));
            assert.ok(ack !== undefined, "close-ack emitted exactly for the retiring flight");
            assert.ok(ack.includes("sent=1"), `exact token sent: ${ack}`);
            assert.ok(ack.includes("retired=1"), `exact token retired: ${ack}`);
            assert.equal(closeAckLines().length, 1, "only the genuine ready emits close-ack");
        } finally {
            handle.stop();
        }
    });

    it("freezeToken intersects with tracked closes at freeze so lifecycle-only entries never become credits", () => {
        const tracker = createWorldCloseTracker();
        let live = ["win-a", "win-b"];
        const surface = {
            windowList: () => live.map((internalId) => ({ internalId })),
        };
        tracker.syncLive(surface);
        live = ["win-b"];
        tracker.noteRemovedEvent();
        tracker.syncLive(surface);

        // Freeze with win-a (tracked) and win-lifecycle-only (not tracked at freeze).
        const token = tracker.freezeToken("gen-1", "src-1", ["win-a", "win-lifecycle-only"]);
        assert.deepEqual(token.closed, ["win-a"]);

        // Even if win-lifecycle-only is later added and natively closed, token cannot retire it.
        live = ["win-b", "win-lifecycle-only"];
        tracker.syncLive(surface);
        live = ["win-b"];
        tracker.noteRemovedEvent();
        tracker.syncLive(surface);
        assert.deepEqual(tracker.snapshotClosed(), ["win-a", "win-lifecycle-only"]);

        assert.equal(tracker.retireToken(token, "gen-1", "src-1"), 1);
        assert.deepEqual(tracker.snapshotClosed(), ["win-lifecycle-only"]);
    });

    it("acceptReply rejects a stale flight source even when another flight is active", () => {
        const state = new WorldAdapterState("test-adapter", "test-owner");
        state.onDiscovery("gen-1", "test-adapter");
        const flight1 = state.beginCapture("src-1", null, null);
        assert.ok(flight1 !== null);
        // Simulate flight1 cleared and replaced by flight2.
        assert.equal(state.releaseFlight("src-1"), true);
        const flight2 = state.beginCapture("src-2", null, null);
        assert.ok(flight2 !== null);

        const staleReply = parseWorldReply(validReplyJson({
            gen: "gen-1",
            req: "src-1",
            source: "src-1",
            rev: "1",
            obs: "1",
        }));
        assert.ok(staleReply !== null && "ready" in staleReply);
        const verdict = state.acceptReply(staleReply, "src-1", "0");
        assert.deepEqual(verdict, { applied: false, reason: "stale-req" });
        assert.equal(state.inFlight, flight2, "newer active flight must remain untouched");
    });

    it("emits only previously proven fixture closes at serialization, never foreign tracker inventory", async () => {
        const h = await makeHarness("w1", ["w1", "w2", "w3"], { scopeClass: "OmniTilerTestWindow" });
        try {
            await discover(h);
            await flushReads(h);
            const closedSent = () => h.logs.filter((l) => l.includes("ev=fixture-closed-sent"));
            assert.ok(closedSent().some((l) => l.includes("closed=-")), "empty intersection is emitted");
            const priorPayloads = h.calls
                .filter((c) => c.method === "DescribeWorld")
                .map((c) => JSON.parse(c.payload) as { gen: string; source: string; sourceSeq: string; report: { closed: string[] } });
            assert.ok(priorPayloads.length > 0, "prior capture exists before pending foreign addition");
            const priorPayload = priorPayloads[priorPayloads.length - 1]!;
            assert.deepEqual(priorPayload.report.closed, [], "prior capture carries no closes");
            const priorLine = closedSent().find((l) => l.includes(`src=${priorPayload.source}`));
            assert.ok(priorLine !== undefined, `prior fixture-closed-sent for ${priorPayload.source}`);
            assert.ok(priorLine.endsWith("closed=-"), `prior intersection empty: ${priorLine}`);
            assert.ok(priorLine.includes(`gen=${priorPayload.gen}`), `prior gen exact: ${priorLine}`);
            assert.ok(priorLine.includes(`seq=${priorPayload.sourceSeq}`), `prior seq exact: ${priorLine}`);
            // Foreign window joins the full tracker inventory but never
            // an in-scope windowFact, so it is never proven fixture.
            const foreign = { ...h.wins[0]!, internalId: "foreign-1", resourceClass: "kwrite" };
            h.wins.push(foreign as never);
            h.fire("windowAdded");
            h.runTimers();
            await flushReads(h);
            const addedIdx = h.calls.findIndex((c, idx) => !h.answered.has(idx) && c.method === "DescribeWorld");
            if (addedIdx >= 0) {
                const addedCall = h.calls[addedIdx]!;
                const addedReply = await h.bridge.send("DescribeWorld", addedCall.payload);
                h.answered.add(addedIdx);
                h.replies.set(addedIdx, addedReply);
                addedCall.callback(addedReply);
            }
            // Close A (proven) and the foreign window together.
            h.wins.splice(h.wins.findIndex((w) => w.internalId === "w1"), 1);
            h.wins.splice(h.wins.findIndex((w) => w.internalId === "foreign-1"), 1);
            h.fire("windowRemoved");
            h.runTimers();
            const payloads = h.calls
                .filter((c) => c.method === "DescribeWorld")
                .map((c) => JSON.parse(c.payload) as { gen: string; source: string; sourceSeq: string; report: { closed: string[] } });
            const payload = payloads.find((p) => p.report.closed.includes("w1"));
            assert.ok(payload !== undefined, "expected capture whose serialized closed carries proven window");
            assert.ok(payload.report.closed.includes("foreign-1"), "tracker inventories foreign close in payload");
            const line = closedSent().find((l) => l.includes(`src=${payload.source}`));
            assert.ok(line !== undefined, `expected fixture-closed-sent for ${payload.source}`);
            assert.ok(line.includes("closed=w1"), `proven fixture close emitted: ${line}`);
            assert.equal(line.includes("foreign-1"), false, `foreign close never emitted: ${line}`);
            assert.ok(line.endsWith("closed=w1"), `exact close oracle, proven only: ${line}`);
            assert.ok(line.includes(`seq=${payload.sourceSeq}`), `capture seq emitted: ${line}`);
            assert.ok(line.includes(`gen=${payload.gen}`), `capture gen exact: ${line}`);
        } finally {
            h.handle.stop();
            await h.bridge.close();
        }
    });

    it("emits no fixture close trace without exact fixture scope", async () => {
        const h = await makeHarness("w1", ["w1", "w2"], {});
        try {
            await discover(h);
            await flushReads(h);
            h.wins.splice(h.wins.findIndex((w) => w.internalId === "w1"), 1);
            h.fire("windowRemoved");
            h.runTimers();
            assert.equal(h.logs.some((l) => l.includes("ev=fixture-closed-sent")), false);
        } finally {
            h.handle.stop();
            await h.bridge.close();
        }
    });
});
