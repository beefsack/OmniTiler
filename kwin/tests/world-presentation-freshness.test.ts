// Stage 5 causal-freshness subscriber contracts: production
// `createWorldPresentationSubscriber` routing with the causal coverage and
// producer gates. The full render-hiding interleavings (real host signals,
// captures, reads, SetPresentation renders) are proven at the production
// entry route in `world-entry-freshness.test.ts`.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { createWorldPresentationSubscriber } from "../src/world-presentation";

const GEN = "gen-1";

function projection(
    req: string, rev: string, obs: string,
    extra?: Record<string, unknown>, gen: string = GEN,
): string {
    return JSON.stringify({
        v: 2, outcome: "presentation", gen, req, rev, obs,
        coveredInstance: "effect-1", coveredInvalid: "0",
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

function setup() {
    const calls: Call[] = [];
    const paints: string[] = [];
    let hides = 0;
    let clears = 0;
    const timers: Timer[] = [];
    let gen: string | null = GEN;
    let stopped = false;
    let invalidation: { instance: string; counter: string } | null = {
        instance: "effect-1", counter: "5",
    };
    let liveGesture: { instance: string; serial: string; coreId: string } | null = null;
    const sub = createWorldPresentationSubscriber({
        callDbus: (_s, _p, _i, _m, payload, callback) => {
            calls.push({ payload, callback });
        },
        getGeneration: () => gen,
        getInvalidation: () => invalidation,
        getGestureIdentity: () => liveGesture,
        isStopped: () => stopped,
        log: () => {},
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
    return {
        calls, paints,
        get hides() { return hides; },
        get clears() { return clears; },
        sub,
        setGen: (g: string | null) => { gen = g; },
        setInvalidation: (next: { instance: string; counter: string } | null) => { invalidation = next; },
        setLiveGesture: (next: { instance: string; serial: string; coreId: string } | null) => { liveGesture = next; },
        answerAll: (make: (req: string) => string) => {
            for (const call of calls.splice(0)) {
                const req = (JSON.parse(call.payload) as { req: string }).req;
                call.callback(make(req));
            }
        },
    };
}

describe("world presentation freshness (production subscriber routing)", () => {
    it("geometry: pre-move cached P6 (newer rev/obs, stale coverage) stays hidden", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        // P5 paints, covering the latest witness.
        h.answerAll((r) => projection(r, "5", "5", { coveredInvalid: "5" }));
        assert.equal(h.paints.length, 1);
        // The native frame move advances the witness (facts event); the
        // subscriber re-reads, but the broker cache still holds the
        // pre-move P6 (newer rev/obs, covered before the move). No
        // post-move capture has arrived yet.
        h.setInvalidation({ instance: "effect-1", counter: "6" });
        h.sub.notifyInvalidated();
        assert.equal(h.calls.length, 1, "move re-drives one read");
        h.answerAll((r) => projection(r, "6", "6", { coveredInvalid: "5" }));
        // Required: P6 must stay hidden until a causally covering capture
        // arrives, even though 6 > 5. Current routing paints it.
        assert.equal(h.paints.length, 1, "pre-move P6 must not paint after the move (stays hidden)");
        // The covering capture's read paints.
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "7", "7", { coveredInvalid: "6" }));
        assert.equal(h.paints.length, 2, "covering paint renders");
        const last = JSON.parse(h.paints[1] as string) as Record<string, unknown>;
        assert.equal(last["coveredInstance"], "effect-1");
        assert.equal(last["coveredInvalid"], "6");
    });

    it("preview: A preview drained entirely inside B never paints", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        // Gesture A is live; its producing preview paints while A is open.
        h.setLiveGesture({ instance: "gesture-1", serial: "1", coreId: "1" });
        const previewA = {
            rect: [0, 0, 396, 600], sourceOut: "out", sourceWs: "ws", win: "win-b",
            producerInstance: "gesture-1", producerSerial: "1",
        };
        h.answerAll((r) => projection(r, "12", "12", { coveredInvalid: "5", preview: previewA }));
        assert.equal(h.paints.length, 1, "reads the A preview while A is live");
        // Finish A and start B: the live identity is now B only. The
        // queued A preview is drained, published, and read entirely inside
        // B. The payload carries the producing identity, never just the
        // insertion neighbor.
        h.setLiveGesture({ instance: "gesture-1", serial: "2", coreId: "2" });
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "13", "13", { coveredInvalid: "5", preview: previewA }));
        assert.equal(h.paints.length, 2, "border/underlay still paint from the accepted projection");
        const painted = JSON.parse(h.paints[1] as string) as Record<string, unknown>;
        // Required: the A preview must not paint in B. Current routing
        // repaints it (insertion neighbor mistaken for the mover).
        assert.equal(painted["preview"], null, "A preview drained inside B must hide (producer is not live)");
        // An unproven preview (no producing identity) never arms either.
        const unproven = {
            rect: [0, 0, 396, 600], sourceOut: "out", sourceWs: "ws", win: "win-b",
            producerInstance: null, producerSerial: null,
        };
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "14", "14", { coveredInvalid: "5", preview: unproven }));
        const stripped = JSON.parse(h.paints[2] as string) as Record<string, unknown>;
        assert.equal(stripped["preview"], null, "unproven previews fail closed");
        // B's own producing preview paints while B is live.
        const previewB = {
            rect: [0, 0, 396, 600], sourceOut: "out", sourceWs: "ws", win: "win-b",
            producerInstance: "gesture-1", producerSerial: "2",
        };
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "15", "15", { coveredInvalid: "5", preview: previewB }));
        const live = JSON.parse(h.paints[3] as string) as Record<string, unknown>;
        assert.deepEqual(live["preview"], previewB, "the live gesture's own preview paints");
    });

    it("focus: in-transit old paint after a native clear or error never resurrects", () => {
        const h = setup();
        h.sub.notifyGeneration(GEN);
        // Show A: accepted paint covering the latest witness.
        h.answerAll((r) => projection(r, "5", "5", { coveredInvalid: "5" }));
        assert.equal(h.paints.length, 1);
        // Malformed read: display-only hide. The ordering and witness
        // evidence is preserved, so only strictly newer (rev, obs)
        // repaints; the same bytes can never resurrect.
        h.sub.notifyInvalidated();
        const clearsBefore = h.clears;
        for (const call of h.calls.splice(0)) {
            call.callback("not-json");
        }
        assert.equal(h.hides, 1, "malformed read hides display-only");
        assert.equal(h.clears, clearsBefore, "error-hide never resets the stream");
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "5", "5", { coveredInvalid: "5" }));
        assert.equal(h.paints.length, 1, "same-bytes retransmit after the error clear must not resurrect");
        // Activate B: the native witness advances (facts event). The held
        // in-transit old read (newer rev/obs, stale coverage) completes
        // before the post-activation capture is accepted: it must not
        // resurrect the cleared paint.
        h.setInvalidation({ instance: "effect-1", counter: "6" });
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "6", "6", { coveredInvalid: "5" }));
        assert.equal(h.paints.length, 1, "in-transit old paint must not resurrect after the B activation clear");
        // Return-to-A: the fresh covering paint applies and shows, and the
        // forwarded native payload carries the covered witness so the
        // native side fences it too.
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "7", "7", { coveredInvalid: "6" }));
        assert.equal(h.paints.length, 2, "covering paint renders");
        const last = JSON.parse(h.paints[1] as string) as Record<string, unknown>;
        assert.equal(last["coveredInstance"], "effect-1");
        assert.equal(last["coveredInvalid"], "6");
        // A foreign witness instance never applies: reused counters across
        // instances never compare.
        h.setInvalidation({ instance: "effect-2", counter: "1" });
        h.sub.notifyInvalidated();
        h.answerAll((r) => projection(r, "8", "8", { coveredInstance: "effect-1", coveredInvalid: "6" }));
        assert.equal(h.paints.length, 2, "foreign-instance paint never applies");
    });
});
