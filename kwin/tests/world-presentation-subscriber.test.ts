// Read-only presentation subscriber (Stage 5): event-driven
// ReadWorldPresentation consumption with generation, request, causal
// coverage, and transport fencing. No polling, no one-shot/write/result
// lanes: the subscriber paints accepted core shapes only (border,
// underlay, drop preview) and hides on anything it cannot prove fresh.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
    createWorldPresentationSubscriber,
    PRESENTATION_READ_METHOD,
} from "../src/world-presentation";
import { isWorldPresentationReply, parseWorldReply } from "../src/world-report";

const GEN = "gen-1";

function projection(
    req: string, rev: string, obs: string,
    extra?: Record<string, unknown>, gen: string = GEN,
): string {
    return JSON.stringify({
        v: 3, outcome: "presentation", gen, req, rev, obs,
        coveredInstance: "effect-1", coveredInvalid: "5",
        scopes: [{ scope: "out/ws", auth: "controlled", reason: "" }],
        workspaces: [{ id: "ws", out: "out", bind: "ws", scope: "out", order: 0, mode: "tiled", exist: true, hold: "" }],
        outputs: [{ out: "out", view: "ws", mode: "tiled", hold: "" }],
        windows: [{
            win: "win-a", out: "out", ws: "ws",
            alloc: [4, 4, 792, 592], frame: [4, 4, 792, 592],
            frame_auth: "controlled", frame_hold: "",
            member_auth: "controlled", member_hold: "",
            floating: false, float_origin: "",
        }],
        presentation: {
            foreground: "win-a",
            border: { visible: true, reason: "", inner: [10, 10, 300, 300], outer: [8, 8, 304, 304] },
            underlay: {
                status: "found", reason: "", baseRev: "11",
                homeOut: "out", homeWs: "ws", group: "grp-1", leaf: "leaf-a", win: "win-a",
                union: [4, 4, 792, 592], outer: [0, 0, 800, 600],
                members: [
                    { leaf: "leaf-b", win: "win-b", rect: [4, 4, 392, 592] },
                    { leaf: "leaf-a", win: "win-a", rect: [404, 4, 392, 592] },
                ],
            },
            pager: [],
        },
        preview: null,
        ...(extra ?? {}),
    });
}

interface Call { payload: string; callback: (reply: unknown) => void }
interface Timer { delay: number; cb: () => void; cancelled: boolean }
interface Harness {
    calls: Call[];
    paints: string[];
    hides: number;
    clears: number;
    logs: string[];
    gen: string | null;
    stopped: boolean;
    invalidation: { instance: string; counter: string } | null;
    liveGesture: { instance: string; serial: string; coreId: string } | null;
    timers: Timer[];
    sub: ReturnType<typeof createWorldPresentationSubscriber>;
    readReqs: () => string[];
    answerAll: (make: (req: string) => string) => void;
    fireWatchdog: () => boolean;
    liveWatchdogs: () => number;
}

function setup(): Harness {
    const calls: Call[] = [];
    const paints: string[] = [];
    let hides = 0;
    let clears = 0;
    const logs: string[] = [];
    const timers: Timer[] = [];
    const h = {} as Harness;
    h.calls = calls;
    h.paints = paints;
    h.logs = logs;
    h.timers = timers;
    h.gen = GEN;
    h.stopped = false;
    h.invalidation = { instance: "effect-1", counter: "5" };
    h.liveGesture = null;
    h.sub = createWorldPresentationSubscriber({
        callDbus: (_s, _p, _i, method, payload, callback) => {
            assert.equal(method, PRESENTATION_READ_METHOD);
            calls.push({ payload, callback });
        },
        getGeneration: () => h.gen,
        getInvalidation: () => h.invalidation,
        getGestureIdentity: () => h.liveGesture,
        isStopped: () => h.stopped,
        log: (m) => { logs.push(m); },
        scheduleOnce: (delay, cb) => {
            const timer: Timer = { delay, cb, cancelled: false };
            timers.push(timer);
            return () => { timer.cancelled = true; };
        },
        renderer: {
            renderPresentation: (payload) => {
                paints.push(payload);
                return true;
            },
            hidePresentation: () => { hides += 1; },
            clearPresentation: () => { clears += 1; },
        },
    });
    Object.defineProperty(h, "hides", { get: () => hides });
    Object.defineProperty(h, "clears", { get: () => clears });
    h.readReqs = () => calls.map((c) => (JSON.parse(c.payload) as { req: string }).req);
    h.answerAll = (make) => {
        for (const call of calls.splice(0)) {
            const req = (JSON.parse(call.payload) as { req: string }).req;
            call.callback(make(req));
        }
    };
    // Fire the currently armed watchdog timer, if any. Returns whether one
    // fired. Cancelled timers never fire.
    h.fireWatchdog = () => {
        const timer = timers.find((t) => !t.cancelled);
        if (timer === undefined) {
            return false;
        }
        timer.cancelled = true;
        timer.cb();
        return true;
    };
    h.liveWatchdogs = () => timers.filter((t) => !t.cancelled).length;
    return h;
}

describe("world presentation subscriber", () => {
    it("seeds one read on generation bind and paints the accepted shapes", () => {
        const h = setup();
        assert.equal(h.calls.length, 0);
        h.sub.notifyGeneration(GEN);
        assert.equal(h.calls.length, 1);
        const req = h.readReqs()[0] as string;
        assert.match(req, /^pres-/);
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1);
        const painted = JSON.parse(h.paints[0] as string) as Record<string, unknown>;
        assert.equal(painted["v"], 1);
        assert.equal(painted["gen"], GEN);
        assert.equal(painted["rev"], "12");
        assert.equal(painted["foreground"], "win-a");
        // The causal coverage rides the native payload so the effect
        // fences it too.
        assert.equal(painted["coveredInstance"], "effect-1");
        assert.equal(painted["coveredInvalid"], "5");
        assert.deepEqual(painted["border"], { visible: 1, inner: [10, 10, 300, 300], outer: [8, 8, 304, 304] });
        // Underlay anchoring inputs ride verbatim: the held member id list
        // plus the already-verbatim core outer rect. Member rects never
        // cross; the native anchor matches ids in stacking order.
        assert.deepEqual(painted["underlay"], {
            status: "found", win: "win-a", outer: [0, 0, 800, 600], members: ["win-b", "win-a"],
        });
        assert.equal(painted["preview"], null);
    });

    it("coalesces invalidations into one flight, sends despite the race, then converges", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        h.sub.notifyInvalidated();
        h.sub.notifyInvalidated();
        assert.equal(h.calls.length, 1, "single flight while a read flies");
        // Continuity across all-motion invalidation: the in-flight reply
        // was invalidated while outstanding, but it still sends (the native
        // renderer decides every surface on exact causal proof) instead of
        // hiding the valid same-gesture preview; one bounded refetch then
        // converges on the covering projection.
        h.answerAll((r) => projection(r, "13", "13"));
        assert.equal(h.paints.length, 1, "dirty read still sends");
        assert.equal(h.calls.length, 1, "one refetch converges");
        // The refetch serves the same cached projection, which the (rev,
        // obs) dedupe skips without resending and without looping.
        h.answerAll((r) => projection(r, "13", "13"));
        assert.equal(h.paints.length, 1);
        assert.equal(h.calls.length, 0);
    });

    it("ignores out-of-order and rotated completions without resurrection", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        const first = h.calls.splice(0)[0] as Call;
        const firstReq = (JSON.parse(first.payload) as { req: string }).req;
        first.callback(projection(firstReq, "14", "14"));
        assert.equal(h.paints.length, 1);
        h.sub.notifyInvalidated();
        const second = h.calls.splice(0)[0] as Call;
        const secondReq = (JSON.parse(second.payload) as unknown as { req: string }).req;
        // A late duplicate of the superseded first completion (same old
        // correlation, newer content): fenced by the request nonce and
        // ignored, never resurrecting older content over the live flight.
        first.callback(projection(firstReq, "15", "15"));
        assert.equal(h.paints.length, 1);
        second.callback(projection(secondReq, "15", "15"));
        assert.equal(h.paints.length, 2);
        // A rotated-generation completion never applies or clears.
        h.gen = "gen-2";
        h.sub.notifyGeneration("gen-2");
        assert.equal(h.calls.length, 1);
        const paintsBefore = h.paints.length;
        second.callback(projection(secondReq, "16", "16"));
        assert.equal(h.paints.length, paintsBefore, "stale completion paints nothing");
    });

    it("clears immediately on generation rotation and reseeds the new one", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1);
        const clearsBefore = h.clears;
        // Production order: the entry binds the new generation before
        // notifying, so the live getter already reports it.
        h.gen = "gen-2";
        h.sub.notifyGeneration("gen-2");
        assert.ok(h.clears > clearsBefore, "rotation resets the stream immediately");
        assert.equal(h.calls.length, 1, "rotation reseeds one read");
        const req = h.readReqs()[0] as string;
        assert.match(req, /^pres-/);
    });

    it("reseeds on reconnect without rotation so a restarted service hides", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1);
        const callsBefore = h.calls.length;
        const hidesBefore = h.hides;
        // Same-generation rediscovery (service owner change or
        // rebind): hide display-only (evidence preserved) and force one
        // fresh read even though nothing rotated.
        h.gen = GEN;
        h.sub.notifyGeneration(GEN);
        assert.equal(h.calls.length, callsBefore + 1, "reconnect reseeds");
        assert.ok(h.hides > hidesBefore, "reconnect hides display-only");
        // The identical projection repaints (the seed reset the point).
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 2);
    });

    it("hides display-only on malformed, foreign-lane, and mismatched reads", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        // Malformed JSON hides display-only, never resetting the stream.
        h.answerAll(() => "not-json");
        assert.ok(h.hides >= 1, "malformed hides");
        assert.equal(h.clears, 1, "rotation-only clear count unchanged by the error");
        assert.equal(h.paints.length, 0);
        // A queued-lane reply on the read hides too.
        h.sub.notifyInvalidated();
        h.answerAll(() => JSON.stringify({ v: 3, outcome: "queued", gen: GEN, client: "c", clientCmd: "1", globalCmd: "1" }));
        assert.equal(h.paints.length, 0);
        // A mismatched correlation hides instead of painting.
        h.sub.notifyInvalidated();
        h.answerAll(() => projection("pres-0-stale", "12", "12"));
        assert.equal(h.paints.length, 0);
        // The next good read repaints (no wedged clear), and an equal
        // retransmit of it never resends.
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1);
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1, "equal retransmit after a hide never resends");
    });

    it("publishes and clears the drop preview without one-shot authority", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        h.liveGesture = { instance: "gesture-1", serial: "1", coreId: "1" };
        const withPreview = (req: string): string => projection(req, "12", "12", {
            preview: {
                rect: [0, 0, 396, 600], sourceOut: "out", sourceWs: "ws", win: "win-b",
                producerInstance: "gesture-1", producerSerial: "1",
            },
        });
        h.answerAll(withPreview);
        assert.equal(h.paints.length, 1);
        const painted = JSON.parse(h.paints[0] as string) as Record<string, unknown>;
        // Renderer-only shape: paint rect plus subject proof plus the
        // producing native gesture. The insertion neighbor `win` is not
        // the mover identity; hover chains, revision tags, and domain
        // resolution never ride.
        assert.deepEqual(painted["preview"], {
            rect: [0, 0, 396, 600], sourceOut: "out", sourceWs: "ws", win: "win-b",
            producerInstance: "gesture-1", producerSerial: "1",
        });
        // A null preview clears the drop target while the rest repaints.
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "13", "13"));
        assert.equal(h.paints.length, 2);
        assert.equal((JSON.parse(h.paints[1] as string) as Record<string, unknown>)["preview"], null);
        // One-shot lanes refuse on the read: result/transfers/effects never
        // ride alongside the paint.
        const parsed = parseWorldReply(projection("pres-x", "12", "12"));
        assert.ok(parsed !== null && isWorldPresentationReply(parsed));
        for (const lane of ["result", "transfers", "effects", "gestureSettled", "operationSettled", "warpCursor"] as const) {
            const tampered = JSON.parse(projection("pres-x", "12", "12")) as Record<string, unknown>;
            tampered[lane] = lane === "result" || lane === "gestureSettled" || lane === "warpCursor" ? null : [];
            assert.equal(parseWorldReply(JSON.stringify(tampered)), null, `${lane} refuses on the presentation lane`);
        }
    });

    it("formats no styling inputs: native styling stays native", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1);
        const text = h.paints[0] as string;
        for (const token of ["color", "Color", "width", "Width", "gap", "theme", "Theme", "radius"]) {
            // "width"/"gap" name no payload field: the geometry rides
            // precomputed rects, and style units converted once at ingress.
            assert.ok(!text.includes(`"${token}"`), token);
        }
    });

    it("stays dirty on transport failure and retries on the next event without polling", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        h.answerAll(() => ({} as unknown as string));
        assert.equal(h.paints.length, 0);
        // No self re-arm: the retry waits for the next event.
        assert.equal(h.calls.length, 0);
        h.sub.notifyInvalidated();
        assert.equal(h.calls.length, 1, "next event retries");
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1);
    });

    it("stops by dropping flights and clearing the paint", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        assert.equal(h.liveWatchdogs(), 1, "one watchdog armed per flight");
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1);
        const clearsBefore = h.clears;
        h.sub.stop();
        assert.ok(h.clears > clearsBefore);
        assert.equal(h.liveWatchdogs(), 0, "stop cancels the watchdog");
        h.stopped = true;
        h.sub.notifyInvalidated();
        h.sub.notifyGeneration(GEN);
        assert.equal(h.paints.length, 1, "stopped subscriber paints nothing");
        assert.equal(h.fireWatchdog(), false, "no watchdog survives stop");
    });

    it("hides stale shapes and retries once per host deadline on a lost reply", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        assert.equal(h.calls.length, 1);
        h.calls.splice(0);
        const hidesBefore = h.hides;
        // Finding A: the callback never arrives (B1-verified silent
        // error/timeout). The watchdog fires at the shared host deadline.
        assert.equal(h.fireWatchdog(), true);
        assert.ok(h.hides > hidesBefore, "lost reply hides display-only instead of retaining stale shapes");
        assert.equal(h.paints.length, 0);
        assert.equal(h.calls.length, 1, "exactly one retry per deadline (no hot loop)");
        assert.equal(h.liveWatchdogs(), 1, "the retry arms exactly one watchdog");
        // A second deadline with continued silence retries once more.
        h.calls.splice(0);
        assert.equal(h.fireWatchdog(), true);
        assert.equal(h.calls.length, 1, "one read per deadline while failing");
        assert.equal(h.paints.length, 0);
        // Recovery still paints when the transport answers.
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1);
        assert.equal(h.liveWatchdogs(), 0, "a terminal reply disarms its watchdog");
    });

    it("ignores stale watchdog timeouts and never clears a newer flight", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        const first = h.calls.splice(0)[0] as Call;
        assert.equal(h.liveWatchdogs(), 1);
        // Rotation drops the flight and its watchdog.
        h.gen = "gen-2";
        h.sub.notifyGeneration("gen-2");
        assert.equal(h.liveWatchdogs(), 1, "rotation arms the seed watchdog");
        const seedCalls = h.calls.length;
        // The orphaned first watchdog is fenced by nonce: firing the old
        // timer object must not clear the new flight. (Only one live
        // timer exists now; fire it twice to prove the first fire is the
        // live seed watchdog, not the orphan.)
        assert.equal(h.fireWatchdog(), true);
        assert.equal(h.calls.length, seedCalls + 1, "seed watchdog re-drives one read");
        // Answer the seed read; the orphaned first reply is nonce-fenced.
        h.answerAll((r) => projection(r, "13", "13", undefined, "gen-2"));
        assert.equal(h.paints.length, 1);
        const firstReq = (JSON.parse(first.payload) as { req: string }).req;
        first.callback(projection(firstReq, "14", "14"));
        assert.equal(h.paints.length, 1, "stale completion paints nothing");
    });

    it("paints a same-identity preview and strips a foreign or unproven one", () => {
        const withPreview = (
            req: string, rev = "12", obs = "12",
            producer: { producerInstance: string | null; producerSerial: string | null } = {
                producerInstance: "gesture-1", producerSerial: "1",
            },
        ): string => projection(req, rev, obs, {
            preview: {
                rect: [0, 0, 396, 600], sourceOut: "out", sourceWs: "ws", win: "win-b",
                ...producer,
            },
        });
        // Same live gesture identity: the live gesture's own preview paints.
        const h = setup();
        h.liveGesture = { instance: "gesture-1", serial: "1", coreId: "1" };
        h.sub.notifyGeneration(GEN);
        h.answerAll(withPreview);
        assert.equal(h.paints.length, 1);
        const painted = JSON.parse(h.paints[0] as string) as Record<string, unknown>;
        assert.deepEqual(painted["preview"], {
            rect: [0, 0, 396, 600], sourceOut: "out", sourceWs: "ws", win: "win-b",
            producerInstance: "gesture-1", producerSerial: "1",
        });
        // Foreign producer identity (the old gesture finished and a new
        // one started): border/underlay still paint from the accepted
        // projection, but the old preview is dropped (fail-closed hide);
        // the new gesture's own preview arrives with a newer projection.
        const h2 = setup();
        h2.liveGesture = { instance: "gesture-1", serial: "1", coreId: "1" };
        h2.sub.notifyGeneration(GEN);
        h2.liveGesture = { instance: "gesture-1", serial: "2", coreId: "2" };
        h2.answerAll(withPreview);
        assert.equal(h2.paints.length, 1, "border/underlay still paint");
        const stripped = JSON.parse(h2.paints[0] as string) as Record<string, unknown>;
        assert.equal(stripped["preview"], null, "foreign-producer preview hides");
        assert.ok(h2.logs.some((l) => l.includes("preview-crossed-gesture")));
        // An unproven preview (null producer) never arms the latch even
        // with a live gesture and newer content.
        h2.sub.notifyInvalidated();
        h2.answerAll((r) => withPreview(r, "13", "13", { producerInstance: null, producerSerial: null }));
        assert.equal((JSON.parse(h2.paints[1] as string) as Record<string, unknown>)["preview"], null);
        // A later same-identity read with a new projection paints it (no
        // wedged strip).
        h2.sub.notifyInvalidated();
        h2.liveGesture = { instance: "gesture-1", serial: "2", coreId: "2" };
        h2.answerAll((r) => projection(r, "14", "14", {
            preview: {
                rect: [0, 0, 396, 600], sourceOut: "out", sourceWs: "ws", win: "win-b",
                producerInstance: "gesture-1", producerSerial: "2",
            },
        }));
        assert.equal(h2.paints.length, 3);
        assert.notEqual((JSON.parse(h2.paints[2] as string) as Record<string, unknown>)["preview"], null);
    });

    it("resends after an effect reset even with an identical projection", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1);
        // Finding H: the effect restarted (new facts instance), so its
        // paint is gone. The subscriber forgets the painted point and
        // re-reads even though nothing else changed; the identical
        // projection repaints (the dedupe is bypassed by the reset).
        const clearsBefore = h.clears;
        h.sub.notifyEffectReset();
        assert.ok(h.clears >= clearsBefore, "reset clears through the renderer first");
        assert.equal(h.calls.length, 1, "reset re-reads without any new event");
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 2, "identical projection repaints after reset");
    });

    it("sends dirty reads for continuity, then converges with one refetch", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        const first = h.calls.splice(0)[0] as Call;
        // Invalidate twice during the flight: the reply still sends
        // (continuity across all-motion invalidation; the native renderer
        // decides every surface on exact causal proof) instead of hiding
        // the valid preview, and one bounded refetch then converges.
        h.sub.notifyInvalidated();
        h.sub.notifyInvalidated();
        const firstReq = (JSON.parse(first.payload) as { req: string }).req;
        first.callback(projection(firstReq, "20", "20"));
        assert.equal(h.paints.length, 1, "dirty read still sends");
        assert.equal(h.calls.length, 1, "one refetch converges");
        // The refetch serves the same cached projection, which the (rev,
        // obs) dedupe skips without resending and without looping.
        h.answerAll((r) => projection(r, "20", "20"));
        assert.equal(h.paints.length, 1);
        assert.equal(h.calls.length, 0);
        // Newer content on the converged read paints once.
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "21", "21"));
        assert.equal(h.paints.length, 2);
        const painted = JSON.parse(h.paints[1] as string) as Record<string, unknown>;
        assert.equal(painted["rev"], "21");
    });

    it("skips stale coverage without a hot loop and paints the covering read", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        h.answerAll((r) => projection(r, "12", "12", { coveredInvalid: "5" }));
        assert.equal(h.paints.length, 1);
        // The native witness advances (facts event) without a new accepted
        // capture yet: a read covering the old witness can never paint.
        h.invalidation = { instance: "effect-1", counter: "6" };
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "13", "13", { coveredInvalid: "5" }));
        assert.equal(h.paints.length, 1, "stale coverage never paints");
        assert.equal(h.calls.length, 0, "no self re-arm while the covering capture is missing");
        // The covering capture's notification re-drives one read, which paints.
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "14", "14", { coveredInvalid: "6" }));
        assert.equal(h.paints.length, 2, "covering paint renders");
        // Missing proof on either side hides: an unknown latest witness or
        // an uncovered projection never paints, and a foreign instance
        // never compares its counters.
        h.invalidation = null;
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "15", "15", { coveredInvalid: "6" }));
        assert.equal(h.paints.length, 2, "unknown latest hides");
        h.invalidation = { instance: "effect-2", counter: "6" };
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "16", "16", { coveredInstance: "effect-1", coveredInvalid: "6" }));
        assert.equal(h.paints.length, 2, "foreign instance never applies");
        // Null coverage with a known latest is malformed: hidden
        // display-only, dirty, and the next event retries into a paint.
        const hidesBefore = h.hides;
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "17", "17", {
            coveredInstance: "effect-2",
            coveredInvalid: null,
        }));
        assert.equal(h.paints.length, 2, "null coverage never paints");
        assert.ok(h.hides > hidesBefore, "malformed coverage hides");
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "18", "18", {
            coveredInstance: "effect-2", coveredInvalid: "6",
        }));
        assert.equal(h.paints.length, 3, "covering paint renders after the malformed leg");
    });

    it("hides display-only on unknown facts without resetting the stream", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1);
        const clearsBefore = h.clears;
        const hidesBefore = h.hides;
        // Unknown facts (teardown/timeout, not a proven instance
        // replacement): hide display-only, retain order/invalidation
        // evidence and the painted point, and re-read.
        h.invalidation = null;
        (h.sub as unknown as { notifyEffectUnknown: () => void }).notifyEffectUnknown();
        assert.ok(h.hides > hidesBefore, "unknown hides display-only");
        assert.equal(h.clears, clearsBefore, "unknown is not an explicit stream replacement");
        assert.equal(h.calls.length, 1, "unknown re-reads without any new event");
        // A stale retransmit of the hidden bytes can never resurrect: the
        // retained painted point covers them and latest is still unknown.
        h.answerAll((r) => projection(r, "12", "12"));
        assert.equal(h.paints.length, 1, "same-bytes retransmit after unknown never resurrects");
        // Same-instance return: no reset either (still no Clear), and a
        // strictly newer covering read paints.
        h.invalidation = { instance: "effect-1", counter: "5" };
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "13", "13"));
        assert.equal(h.paints.length, 2, "covering paint renders after same-instance return");
        assert.equal(h.clears, clearsBefore, "same-instance return never resets the stream");
    });
});
