// Offline unit coverage for the world decimal/parse/state boundary
// (strict v2, sole broker): bounded decimal strings (no Number precision),
// strict envelope/reply parsing, the broker wire codec, and the adapter
// lifetime state machine (transport fences, dirty/ack, creation, ambiguous
// holds). The broker owns ALL sequencing; the adapter owns only the
// per-capture source correlation, the local native-invalidation watermark,
// and the queue-client command counter. No D-Bus, no KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
    WORLD_MAX_REQUEST_BYTES,
    compareU64,
    incrementU64,
    isCanonicalU64,
    isCorrelationId,
    isGenerationId,
    isOpaqueId,
    utf8ByteLength,
    WORLD_U64_MAX,
} from "../src/world-seq";
import {
    buildCaptureEnvelope,
    buildDiscoveryRequest,
    buildQueueRequest,
    buildTakeRequest,
    cmdMigrateWorkspace,
    cmdSelectNumbered,
    cmdSelectRelative,
    cmdSend,
    cmdSendNumbered,
    explicitSource,
    isCoreWorldReply,
    isWorldQueueReply,
    isWorldRejection,
    parseWorldReply,
} from "../src/world-report";
import { WorldAdapterState } from "../src/world-state";

const DISABLED_PRESENTATION = {
    foreground: "",
    border: { visible: false, reason: "disabled", inner: null, outer: null },
    underlay: { status: "hidden", reason: "disabled", baseRev: null },
    pager: [],
};

const ACCEPTED = (
    overrides: Record<string, unknown> = {},
): string => JSON.stringify({
    v: 2,
    adapter: "broker-1",
    gen: "gen-1",
    req: "req-1",
    obs: "3",
    rev: "7",
    ready: true,
    consumed: false,
    scopes: [{ scope: "out/ws", auth: "controlled", reason: "" }],
    workspaces: [],
    outputs: [],
    windows: [],
    transfers: [],
    effects: [],
    result: null,
    gestureSettled: null,
    operationSettled: [], nativeCreations: [],
    warpCursor: null,
    presentation: DISABLED_PRESENTATION,
    source: "req-1",
    nativeInvalid: "1",
    clock: "0",
    ...overrides,
});

describe("world-seq decimal counters", () => {
    it("accepts canonical forms including u64 max, rejects the rest", () => {
        assert.equal(isCanonicalU64("0"), true);
        assert.equal(isCanonicalU64("7"), true);
        assert.equal(isCanonicalU64(WORLD_U64_MAX), true);
        for (const bad of ["", "007", "-1", "+1", " 1", "1 ", "18446744073709551616", "12a", "1.0"]) {
            assert.equal(isCanonicalU64(bad), false, bad);
        }
    });

    it("compares numerically without Number precision", () => {
        assert.equal(compareU64("9", "10"), -1);
        assert.equal(compareU64("10", "9"), 1);
        assert.equal(compareU64("18446744073709551615", "18446744073709551614"), 1);
        assert.equal(compareU64("3", "3"), 0);
    });

    it("increments by digits and never wraps past u64 max", () => {
        assert.equal(incrementU64("0"), "1");
        assert.equal(incrementU64("9"), "10");
        assert.equal(incrementU64("99"), "100");
        assert.equal(incrementU64("18446744073709551614"), WORLD_U64_MAX);
        assert.equal(incrementU64(WORLD_U64_MAX), null);
    });

    it("measures actual UTF-8 bytes and validates id alphabets", () => {
        assert.equal(utf8ByteLength("abc"), 3);
        assert.equal(utf8ByteLength("é"), 2);
        assert.equal(utf8ByteLength("win-1_X.Z"), 9);
        assert.equal(WORLD_MAX_REQUEST_BYTES, 4 * 1024 * 1024);
        assert.equal(isOpaqueId("win-1_X.Z"), true);
        assert.equal(isOpaqueId("bad id"), false);
        assert.equal(isOpaqueId(""), false);
        assert.equal(isGenerationId("w-123-abc"), true);
        assert.equal(isGenerationId("GEN"), false);
        assert.equal(isCorrelationId("req-1"), true);
    });
});

describe("world-report commands and reply parsing", () => {
    it("builds only vocabulary-checked commands", () => {
        assert.deepEqual(cmdSend("win-a", "out", "ws2", true), { op: "send", win: "win-a", out: "out", ws: "ws2", follow: true });
        assert.equal(cmdSelectNumbered("out", 0), null);
        assert.equal(cmdSelectNumbered("out", 10), null);
        assert.notEqual(cmdSelectNumbered("out", 3), null);
        // Null output resolves in core; explicit strings stay valid.
        assert.deepEqual(cmdSelectNumbered(null, 3), { op: "select-numbered", out: null, index: 3 });
        assert.equal(cmdSelectRelative("out", 2), null);
        assert.notEqual(cmdSendNumbered("win-a", explicitSource("out"), 0, false), null);
        assert.equal(cmdSendNumbered("win-a", explicitSource("out"), 10, false), null);
        assert.equal(cmdMigrateWorkspace("out", "out", "ws"), null);
        assert.notEqual(cmdMigrateWorkspace("a", "b", "ws"), null);
    });

    it("parses the golden ready reply and bounded rejections, refuses the rest", () => {
        const parsed = parseWorldReply(ACCEPTED());
        assert.ok(parsed !== null && isCoreWorldReply(parsed));
        if (isCoreWorldReply(parsed)) {
            assert.equal(parsed.rev, "7");
            assert.equal(parsed.obs, "3");
            assert.equal(parsed.ready, true);
            assert.equal(parsed.source, "req-1");
            assert.equal(parsed.nativeInvalid, "1");
            assert.equal(parsed.clock, "0");
            assert.equal(parsed.result, null);
        }
        const rejection = parseWorldReply('{"v":2,"outcome":"rejected","kind":"busy","message":"busy","gen":"gen-1","source":"","nativeInvalid":"0","clock":"0"}');
        assert.ok(isWorldRejection(rejection));
        // v1 refuses: no compatibility route.
        assert.equal(parseWorldReply(ACCEPTED().replace('"v":2', '"v":1')), null);
        // Actuator lane with a terminal result refuses.
        assert.equal(parseWorldReply(ACCEPTED({ result: { op: "no-group", reason: "pending", rev: null } })), null);
        // Bare-number counters refuse: no f64 precision-loss path.
        assert.equal(parseWorldReply(ACCEPTED().replace('"obs":"3"', '"obs":3')), null);
        assert.equal(parseWorldReply(ACCEPTED().replace('"rev":"7"', '"rev":"007"')), null);
        assert.equal(parseWorldReply("not json"), null);
    });

    it("builds strict v2 envelopes with no broker keys inside the report", () => {
        const inner = buildCaptureEnvelope("a-1", "gen-1", "src-1", "1", "4", '{"v":2}');
        assert.deepEqual(JSON.parse(inner), {
            v: 2, adapter: "a-1", gen: "gen-1", source: "src-1", sourceSeq: "1", nativeInvalid: "4", report: { v: 2 },
        });
        assert.deepEqual(JSON.parse(buildDiscoveryRequest("a-1", "disc-9")), { v: 2, adapter: "a-1", req: "disc-9" });
        assert.deepEqual(
            JSON.parse(buildQueueRequest("panel-1", "gen-1", "3", { op: "select-trailing", out: null })),
            { v: 2, gen: "gen-1", client: "panel-1", clientCmd: "3", command: { op: "select-trailing", out: null } },
        );
        assert.deepEqual(JSON.parse(buildTakeRequest("panel-1", "gen-1", "3")), { v: 2, gen: "gen-1", client: "panel-1", clientCmd: "3" });
        const queued = parseWorldReply('{"v":2,"outcome":"queued","gen":"gen-1","client":"panel-1","clientCmd":"3","globalCmd":"9"}');
        assert.ok(isWorldQueueReply(queued));
    });
});

describe("world adapter lifetime state", () => {
    const bound = (): WorldAdapterState => {
        const state = new WorldAdapterState("test-adapter", "owner-a");
        state.onDiscovery("gen-1", "broker-1");
        return state;
    };

    it("advances the local watermark synchronously, even during flight", () => {
        const state = bound();
        assert.equal(state.currentInvalid, "0");
        state.invalidate();
        assert.equal(state.currentInvalid, "1");
        state.invalidate();
        assert.equal(state.currentInvalid, "2");
        // The watermark advances even with a capture in flight.
        const flight = state.beginCapture("src-1", null);
        assert.ok(flight !== null && flight.watermark === "2");
        state.invalidate();
        assert.equal(state.currentInvalid, "3");
        assert.equal(state.inFlight?.source, "src-1");
    });

    it("mints queue-client command ids monotonically", () => {
        const state = bound();
        assert.equal(state.mintClientCmd(), "1");
        assert.equal(state.mintClientCmd(), "2");
        assert.equal(state.nextClientCmd, "3");
        // A fresh generation restarts the namespace at 1.
        state.onDiscovery("gen-2", "broker-2");
        assert.equal(state.mintClientCmd(), "1");
    });

    it("accepts only bound-generation incarnation source-correlated replies", () => {
        const state = bound();
        state.invalidate();
        state.invalidate();
        const flight = state.beginCapture("src-9", null);
        assert.ok(flight !== null);
        const reply = parseWorldReply(ACCEPTED({ req: "src-9", source: "src-9", obs: "12", rev: "1" }));
        assert.ok(reply !== null && isCoreWorldReply(reply));
        if (!isCoreWorldReply(reply)) {
            return;
        }
        const verdict = state.acceptReply(reply, "src-9", flight.watermark);
        assert.deepEqual(verdict, { applied: true, reason: "accepted" });
        assert.equal(state.lastClock, "0");
        // Duplicate rev refuses without setters.
        const dup = state.acceptReply(reply, "src-9", flight.watermark);
        assert.equal(dup.applied, false);
        // A reply bound to another incarnation never applies.
        state.beginCapture("src-10", null);
        const foreign = parseWorldReply(ACCEPTED({ adapter: "broker-9", req: "src-10", source: "src-10", rev: "2" }));
        assert.ok(foreign !== null && isCoreWorldReply(foreign));
        if (isCoreWorldReply(foreign)) {
            assert.deepEqual(state.acceptReply(foreign, "src-10", state.currentInvalid), { applied: false, reason: "adapter-mismatch" });
        }
        // Wrong generation never applies.
        const staleGen = parseWorldReply(ACCEPTED({ gen: "gen-2", req: "src-10", source: "src-10", rev: "9" }));
        assert.ok(staleGen !== null && isCoreWorldReply(staleGen));
        if (isCoreWorldReply(staleGen)) {
            assert.equal(state.acceptReply(staleGen, "src-10", state.currentInvalid).applied, false);
        }
    });

    it("records the broker clock for the foreign attempt epoch", () => {
        const state = bound();
        const flight = state.beginCapture("src-1", "5");
        assert.ok(flight !== null && flight.clientCmd === "5");
        const reply = parseWorldReply(ACCEPTED({ req: "src-1", source: "src-1", rev: "1", clock: "48" }));
        assert.ok(reply !== null && isCoreWorldReply(reply));
        if (isCoreWorldReply(reply)) {
            assert.deepEqual(state.acceptReply(reply, "src-1", flight.watermark), { applied: true, reason: "accepted" });
        }
        assert.equal(state.lastClock, "48");
    });

    it("retains closes until an accepted reply acks them", () => {
        const state = bound();
        state.noteClosed("gone-1");
        assert.equal(state.pendingClosed.length, 1);
        const flight = state.beginCapture("src-1", null);
        assert.ok(flight !== null);
        const reply = parseWorldReply(ACCEPTED({ req: "src-1", source: "src-1", rev: "1" }));
        assert.ok(reply !== null && isCoreWorldReply(reply));
        if (isCoreWorldReply(reply)) {
            state.acceptReply(reply, "src-1", flight.watermark);
        }
        assert.equal(state.pendingClosed.length, 0);
    });

    it("defers execution evidence without fabricating and completes it once, uncorrelated", () => {
        const state = bound();
        // Evidence is only ever kept for an executed call: defer requires
        // the op already spent.
        assert.equal(state.deferCreationReceipt({
            op: "op-native-1", ws: "wl-1", out: "out",
            setter: "accepted", before: ["ws1"], attemptMs: "7",
        }), false, "unspent op keeps no evidence");
        assert.ok(state.markCreationSpent("op-native-1"));
        assert.equal(state.deferCreationReceipt({
            op: "op-native-1", ws: "wl-1", out: "out",
            setter: "accepted", before: ["ws1"], attemptMs: "7",
        }), true);
        // While deferred, nothing rides the report lane: no `after: []`,
        // no `before` copy, no invented causal id.
        assert.equal(state.creationAttemptsForReport().length, 0, "deferred evidence never emits");
        // A null readback completes nothing and preserves the evidence.
        assert.equal(state.completeDeferredCreations(null), 0);
        assert.equal(state.creationAttemptsForReport().length, 0);
        // The first successful full readback completes once, uncorrelated:
        // created null (causality lost across the turn), after is the
        // actually observed order, the stamp stays the write clock.
        assert.equal(state.completeDeferredCreations(["ws1", "native-4"]), 1);
        const attempts = state.creationAttemptsForReport();
        assert.equal(attempts.length, 1);
        assert.equal(attempts[0]?.["op"], "op-native-1");
        assert.equal(attempts[0]?.["created"], null, "later turn never claims a causal id");
        assert.deepEqual(attempts[0]?.["after"], ["ws1", "native-4"], "after is the observed order, never fabricated");
        assert.equal(attempts[0]?.["attemptMs"], "7", "original write clock preserved");
        // Completion is once: a second flush completes nothing, and the
        // spent op never re-executes.
        assert.equal(state.completeDeferredCreations(["ws1", "native-4", "native-5"]), 0);
        assert.equal(state.creationAttemptsForReport().length, 1);
        assert.equal(state.markCreationSpent("op-native-1"), false);
    });

    it("generation rotation clears deferred evidence, receipts, and spent ops together", () => {
        const state = bound();
        assert.ok(state.markCreationSpent("op-native-1"));
        assert.equal(state.deferCreationReceipt({
            op: "op-native-1", ws: "wl-1", out: "out",
            setter: "threw", before: ["ws1"], attemptMs: "7",
        }), true);
        assert.equal(state.completeDeferredCreations(["ws1", "native-4"]), 1);
        assert.equal(state.creationAttemptsForReport().length, 1);
        const changed = state.onDiscovery("gen-2", "broker-2");
        assert.equal(changed, true);
        assert.equal(state.completeDeferredCreations(["ws1", "native-4"]), 0);
        assert.equal(state.creationAttemptsForReport().length, 0);
        assert.equal(state.markCreationSpent("op-native-1"), true, "spent resets with the generation");
    });

    it("spends one-shot effects and frame/clear tokens once with dispatch records", () => {
        const state = bound();
        assert.equal(state.markEffectSpent("fx-1"), true);
        assert.equal(state.markEffectSpent("fx-1"), false);
        assert.equal(state.markFloatSpent("op-3"), true);
        assert.equal(state.markFloatSpent("op-3"), false);
        assert.equal(state.markClearSpent("op-4"), true);
        assert.equal(state.markClearSpent("op-4"), false);
        state.markDispatched("op-9");
        assert.equal(state.isDispatched("op-9"), true);
        assert.equal(state.isDispatched("op-10"), false);
    });

    it("rebinds generation on discovery, cancelling flights and restarting the client namespace", () => {
        const state = bound();
        state.mintClientCmd();
        const flight = state.beginCapture("src-1", "1");
        assert.ok(flight !== null && state.inFlight !== null);
        const changed = state.onDiscovery("gen-2", "broker-2");
        assert.equal(changed, true);
        assert.equal(state.boundGen, "gen-2");
        assert.equal(state.brokerIncarnation, "broker-2");
        assert.equal(state.inFlight, null);
        assert.equal(state.hasAccepted, false);
        assert.equal(state.nextClientCmd, "1");
    });

    it("rejects delayed arbitrary replies by incarnation and exact source without setters or ack", () => {
        const state = bound();
        state.invalidate();
        const flight = state.beginCapture("src-7", null);
        assert.ok(flight !== null);
        const watermark = flight.watermark;
        state.noteClosed("gone-9");
        // Same gen/newer rev but a different broker incarnation: no
        // setters, no ack, catchup only.
        const foreign = parseWorldReply(ACCEPTED({ adapter: "broker-x", req: "src-7", source: "src-7", rev: "5" }));
        assert.ok(foreign !== null && isCoreWorldReply(foreign));
        if (isCoreWorldReply(foreign)) {
            // The foreign incarnation was never discovered, so binding
            // holds the discovered one: adapter-mismatch.
            assert.deepEqual(state.acceptReply(foreign, "src-7", watermark), { applied: false, reason: "adapter-mismatch" });
        }
        assert.equal(state.pendingClosed.length, 1);
        assert.equal(state.hasAccepted, false);
        assert.ok(state.inFlight !== null);
        // Same gen/newer rev but the wrong source: no setters, no ack.
        const wrongReq = parseWorldReply(ACCEPTED({ req: "src-999", source: "src-999", rev: "6" }));
        assert.ok(wrongReq !== null && isCoreWorldReply(wrongReq));
        if (isCoreWorldReply(wrongReq)) {
            assert.deepEqual(state.acceptReply(wrongReq, "src-7", watermark), { applied: false, reason: "stale-req" });
        }
        assert.equal(state.pendingClosed.length, 1);
        assert.equal(state.hasAccepted, false);
        // The exact flight still applies and acks.
        const good = parseWorldReply(ACCEPTED({ req: "src-7", source: "src-7", rev: "7" }));
        assert.ok(good !== null && isCoreWorldReply(good));
        if (isCoreWorldReply(good)) {
            assert.deepEqual(state.acceptReply(good, "src-7", watermark), { applied: true, reason: "accepted" });
        }
        assert.equal(state.pendingClosed.length, 0);
    });

    it("binds discovery only for the exact adapter and req", () => {
        const state = bound();
        const flight = state.beginCapture("src-1", null);
        assert.ok(flight !== null && state.inFlight !== null);
        // Wrong adapter: binds nothing, resets nothing.
        state.onDiscovery("gen-9", "broker-9", "other-adapter", "disc-1", "disc-1");
        assert.equal(state.boundGen, "gen-1");
        assert.ok(state.inFlight !== null);
        // Wrong req: binds nothing, resets nothing.
        state.onDiscovery("gen-9", "broker-9", "test-adapter", "disc-2", "disc-1");
        assert.equal(state.boundGen, "gen-1");
        assert.ok(state.inFlight !== null);
        // Exact match binds and cancels flights.
        const changed = state.onDiscovery("gen-9", "broker-9", "test-adapter", "disc-1", "disc-1");
        assert.equal(changed, true);
        assert.equal(state.boundGen, "gen-9");
        assert.equal(state.brokerIncarnation, "broker-9");
        assert.equal(state.inFlight, null);
        // Same generation rediscovery is not a change.
        assert.equal(state.onDiscovery("gen-9", "broker-9", "test-adapter", "disc-2", "disc-2"), false);
    });
});
