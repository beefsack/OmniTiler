// Offline unit coverage for the world decimal/parse/state boundary (D1-D5):
// bounded decimal strings (no Number precision), strict reply parsing, and
// the adapter lifetime state machine (freshness, dirty/ack, creation,
// ambiguous holds). No D-Bus, no KWin.

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
    cmdMigrateWorkspace,
    cmdSelectNumbered,
    cmdSelectRelative,
    cmdSelectTrailing,
    cmdSend,
    cmdSendRelative,
    isWorldRejection,
    parseWorldReply,
} from "../src/world-report";
import { WorldAdapterState } from "../src/world-state";

const ACCEPTED = (
    overrides: Record<string, unknown> = {},
): string => JSON.stringify({
    v: 1,
    adapter: "test-adapter",
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
        assert.equal(cmdSelectRelative("out", 2), null);
        assert.notEqual(cmdSendRelative("win-a", "out", -1, false), null);
        assert.equal(cmdMigrateWorkspace("out", "out", "ws"), null);
        assert.notEqual(cmdMigrateWorkspace("a", "b", "ws"), null);
        assert.notEqual(cmdSelectTrailing("out"), null);
    });

    it("parses the golden ready reply and bounded rejections, refuses the rest", () => {
        const parsed = parseWorldReply(ACCEPTED());
        assert.ok(parsed !== null && !isWorldRejection(parsed));
        if (!isWorldRejection(parsed) && parsed !== null) {
            assert.equal(parsed.rev, "7");
            assert.equal(parsed.obs, "3");
            assert.equal(parsed.ready, true);
        }
        const rejection = parseWorldReply('{"v":1,"outcome":"rejected","kind":"busy","message":"busy","gen":"gen-1"}');
        assert.ok(isWorldRejection(rejection));
        // Bare-number counters refuse: no f64 precision-loss path.
        assert.equal(parseWorldReply(ACCEPTED().replace('"obs":"3"', '"obs":3')), null);
        assert.equal(parseWorldReply(ACCEPTED().replace('"rev":"7"', '"rev":"007"')), null);
        assert.equal(parseWorldReply("not json"), null);
    });
});

describe("world adapter lifetime state", () => {
    const bound = (): WorldAdapterState => {
        const state = new WorldAdapterState("test-adapter", "owner-a");
        state.onDiscovery("gen-1");
        return state;
    };

    it("advances obs synchronously and separates request/command highwaters", () => {
        const state = bound();
        assert.equal(state.currentObs, "0");
        state.invalidate();
        assert.equal(state.currentObs, "1");
        state.invalidate();
        assert.equal(state.currentObs, "2");
        const begun = state.beginDispatch({ op: "select-trailing", out: "out" });
        assert.ok(begun !== null && begun.cmdSeq !== null);
        assert.equal(begun.reqSeq, "1");
        assert.equal(begun.cmdSeq, "1");
        assert.equal(state.nextReqSeq, "2");
        assert.equal(state.nextCmdSeq, "2");
        // Second dispatch while in flight refuses at the entry, but the
        // state itself tracks the single flight.
        assert.equal(state.inFlightReq?.reqSeq, "1");
    });

    it("accepts only bound-generation ready newer-rev current-obs replies", () => {
        const state = bound();
        state.invalidate();
        state.invalidate();
        const begun = state.beginDispatch(null);
        assert.ok(begun !== null);
        const dispatchObs = state.currentObs;
        const reply = parseWorldReply(ACCEPTED({ obs: dispatchObs, rev: "1" }));
        assert.ok(reply !== null && !isWorldRejection(reply));
        const verdict = state.acceptReply(reply, dispatchObs);
        assert.deepEqual(verdict, { applied: true, reason: "accepted" });
        // Duplicate rev refuses without setters.
        const dup = state.acceptReply(reply, dispatchObs);
        assert.equal(dup.applied, false);
        // Newer input invalidates the watermark: the next reply on the old
        // obs is stale-obs catchup, never applied. A fresh flight names the
        // current req, so the stale obs (not the req) decides.
        state.invalidate();
        const begun2 = state.beginDispatch(null);
        assert.ok(begun2 !== null);
        const stale = parseWorldReply(ACCEPTED({ obs: dispatchObs, rev: "2", req: begun2.reqId }));
        assert.ok(stale !== null && !isWorldRejection(stale));
        const staleVerdict = state.acceptReply(stale, dispatchObs);
        assert.deepEqual(staleVerdict, { applied: false, reason: "stale-obs" });
        assert.equal(state.observationOnlyCatchup, true);
        // Wrong generation never applies.
        const foreign = parseWorldReply(ACCEPTED({ obs: state.currentObs, rev: "9" }));
        assert.ok(foreign !== null && !isWorldRejection(foreign));
        if (!isWorldRejection(foreign) && foreign !== null) {
            foreign.gen = "gen-2";
            assert.equal(state.acceptReply(foreign, state.currentObs).applied, false);
        }
    });

    it("retains closes and feedback until an accepted reply acks them", () => {
        const state = bound();
        state.noteClosed("gone-1");
        state.noteFeedback({ op: "op-5", kind: "transfer", outcome: "possibly-executed", hold: "" });
        assert.equal(state.pendingClosed.length, 1);
        const begun = state.beginDispatch(null);
        assert.ok(begun !== null);
        const reply = parseWorldReply(ACCEPTED({ obs: state.currentObs, rev: "1" }));
        assert.ok(reply !== null && !isWorldRejection(reply));
        state.acceptReply(reply, state.currentObs);
        assert.equal(state.pendingClosed.length, 0);
        // Possibly-executed feedback is retained (held, never replayed).
        assert.equal(state.pendingFeedback.some((f) => f.op === "op-5"), true);
        assert.equal(state.isHeldAmbiguous("op-5"), true);
    });

    it("holds one creation flight per logical id and recovers by enumeration", () => {
        const state = bound();
        const first = state.startCreation("wl-1", "out");
        assert.ok(first !== null);
        assert.equal(state.startCreation("wl-1", "out"), null);
        // Ambiguous result: enumeration with exactly one fresh native id binds.
        assert.equal(state.recoverCreationByEnumeration("wl-1", ["ws", "native-9"], ["ws"]), "native-9");
        assert.equal(state.creationFor("wl-1")?.bound, "native-9");
    });

    it("never binds one native desktop to two logical tokens", () => {
        const state = bound();
        const a = state.startCreation("wl-a", "out", ["ws1"]);
        assert.ok(a !== null);
        // Same-flight proof binds by exact native name.
        assert.equal(state.recoverCreationByName("wl-a", [
            { id: "ws1", name: "WS1" },
            { id: "native-4", name: "wl-a" },
        ]), "native-4");
        // A sibling token from a stale baseline must not adopt it: name
        // mismatch and already claimed.
        const b = state.startCreation("wl-b", "out", ["ws1"]);
        assert.ok(b !== null);
        assert.equal(state.recoverCreationByName("wl-b", [
            { id: "ws1", name: "WS1" },
            { id: "native-4", name: "wl-a" },
        ]), null, "sibling desktop never adopted");
        assert.equal(state.recoverCreationByEnumeration("wl-b", ["ws1", "native-4"], ["ws1"]), null, "any-fresh-id fallback never steals a claimed native");
        assert.equal(state.creationFor("wl-b")?.bound, null);
        // An unrelated user desktop never binds either.
        assert.equal(state.recoverCreationByName("wl-b", [
            { id: "ws1", name: "WS1" },
            { id: "native-4", name: "wl-a" },
            { id: "user-9", name: "User 9" },
        ]), null);
    });

    it("spends one-shot effects once and settles attempts explicitly", () => {
        const state = bound();
        assert.equal(state.markEffectSpent("fx-1"), true);
        assert.equal(state.markEffectSpent("fx-1"), false);
        state.armPossiblyExecuted("op-9", "transfer", "out/ws2");
        assert.equal(state.isHeldAmbiguous("op-9"), true);
        state.settleAttempt("op-9", "refused-proven", "refused-target");
        assert.equal(state.isHeldAmbiguous("op-9"), false);
    });

    it("rebinds generation on discovery, cancelling flights but keeping natives", () => {
        const state = bound();
        state.beginDispatch({ op: "select-trailing", out: "out" });
        assert.ok(state.inFlightReq !== null);
        state.onDiscovery("gen-2");
        assert.equal(state.boundGen, "gen-2");
        assert.equal(state.inFlightReq, null);
        assert.equal(state.hasAccepted, false);
    });

    it("rejects delayed arbitrary replies by adapter and exact req without setters or ack", () => {
        const state = bound();
        state.invalidate();
        const begun = state.beginDispatch(null);
        assert.ok(begun !== null);
        const flightObs = state.currentObs;
        const flightReq = begun.reqId;
        state.noteClosed("gone-9");
        // Same gen/obs/newer rev but a different adapter namespace: no
        // setters, no ack, catchup only.
        const foreign = parseWorldReply(ACCEPTED({ obs: flightObs, rev: "5", req: flightReq, adapter: "other-adapter" }));
        assert.ok(foreign !== null && !isWorldRejection(foreign));
        if (!isWorldRejection(foreign) && foreign !== null) {
            const verdict = state.acceptReply(foreign, flightObs);
            assert.deepEqual(verdict, { applied: false, reason: "adapter-mismatch" });
        }
        assert.equal(state.pendingClosed.length, 1);
        assert.equal(state.hasAccepted, false);
        assert.ok(state.inFlightReq !== null);
        // Same gen/obs/newer rev but the wrong req: no setters, no ack.
        const wrongReq = parseWorldReply(ACCEPTED({ obs: flightObs, rev: "6", req: "req-999" }));
        assert.ok(wrongReq !== null && !isWorldRejection(wrongReq));
        if (!isWorldRejection(wrongReq) && wrongReq !== null) {
            const verdict = state.acceptReply(wrongReq, flightObs);
            assert.deepEqual(verdict, { applied: false, reason: "stale-req" });
        }
        assert.equal(state.pendingClosed.length, 1);
        assert.equal(state.hasAccepted, false);
        // The exact flight still applies and acks.
        const good = parseWorldReply(ACCEPTED({ obs: flightObs, rev: "7", req: flightReq }));
        assert.ok(good !== null && !isWorldRejection(good));
        if (!isWorldRejection(good) && good !== null) {
            assert.deepEqual(state.acceptReply(good, flightObs), { applied: true, reason: "accepted" });
        }
        assert.equal(state.pendingClosed.length, 0);
    });

    it("binds discovery only for the exact adapter and req", () => {
        const state = bound();
        state.beginDispatch({ op: "select-trailing", out: "out" });
        assert.ok(state.inFlightReq !== null);
        // Wrong adapter: binds nothing, resets nothing.
        state.onDiscovery("gen-9", "other-adapter", "disc-1", "disc-1");
        assert.equal(state.boundGen, "gen-1");
        assert.ok(state.inFlightReq !== null);
        // Wrong req: binds nothing, resets nothing.
        state.onDiscovery("gen-9", "test-adapter", "disc-2", "disc-1");
        assert.equal(state.boundGen, "gen-1");
        assert.ok(state.inFlightReq !== null);
        // Exact match binds and cancels flights.
        state.onDiscovery("gen-9", "test-adapter", "disc-1", "disc-1");
        assert.equal(state.boundGen, "gen-9");
        assert.equal(state.inFlightReq, null);
    });
});
