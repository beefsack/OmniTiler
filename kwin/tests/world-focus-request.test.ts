// Retained focus request (v4): the adapter applies only the latest
// publication, spends once per bound id before the native attempt or the
// safety no-call, and correlates raw focusFeedback by exact
// (id,target,rev) with arrival only when afterShown is non-null. A null
// envelope (or a newer publication on the next reply) cancels unstarted
// old work through bounded current-record/retired-revision accounting;
// stale envelopes and late callbacks cannot reactivate it.
// Direct applyWorldReply unit tests plus state-accounting tests; no live
// KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply, type ApplyEnv } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { parseWorldReply, isCoreWorldReply, type WorldFocusRequest } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";

type Obs = ApplyEnv["observation"];

const winRow = (winId: string, ws: string): Record<string, unknown> => ({
    win: winId, out: "out", ws,
    alloc: null, frame: null,
    frame_auth: "controlled", frame_hold: "",
    member_auth: "controlled", member_hold: "",
    frame_op: "",
    clear_op: "",
    floating: false, float_origin: "",
    cmdSeq: null,
    visibility: null, visibility_auth: "untouched", visibility_hold: "",
    state: [],
});

const focusRow = (id: string, target: string, rev: string, opts?: {
    cause?: string; out?: string; ws?: string; cmdSeq?: string | null;
    afterShown?: Record<string, unknown> | null;
}): Record<string, unknown> => {
    const cause = opts?.cause ?? "positive-close";
    const cmdSeq = opts?.cmdSeq !== undefined ? opts.cmdSeq : null;
    return {
        id, target, rev, cause, out: opts?.out ?? "out", ws: opts?.ws ?? "ws1",
        cmdSeq, afterShown: opts?.afterShown !== undefined ? opts.afterShown : null,
    };
};

// Frozen selection dependencies for one explicit-command request: the
// existing parent operation, the issuing observation counter, and the
// affected scopes. Views must sit inside selected.
const afterShownFor = (target: string): Record<string, unknown> => ({
    op: "op-1", obs: "1",
    members: [{ win: target, out: "out", ws: "ws1", frame: null }],
    views: [{ out: "out", ws: "ws1" }],
    selected: [{ out: "out", ws: "ws1" }],
    departing: [],
});

const wsBindRow = (ws: string): Record<string, unknown> => ({
    id: ws, out: "out", bind: ws === "ws1" ? "native-1" : ws === "ws2" ? "native-2" : `native-${ws}`,
    scope: { kind: "output", out: "out" },
    order: 0, mode: "tiled", exist: true, hold: "",
});

function replyText(opts: {
    focusRequest: Record<string, unknown> | null;
    windows?: Record<string, unknown>[];
    workspaces?: Record<string, unknown>[];
    obs?: string;
    rev?: string;
    clock?: string;
}): string {
    return JSON.stringify({
        v: 4, adapter: "test-adapter", gen: "gen-1", req: "req-1",
        obs: opts.obs ?? "1", rev: opts.rev ?? "7", ready: true, consumed: false,
        source: "req-1", nativeInvalid: "1", clock: opts.clock ?? "1",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: opts.workspaces ?? [wsBindRow("ws1")], outputs: [],
        windows: opts.windows ?? [winRow("win-a", "ws1"), winRow("win-c", "ws1")],
        transfers: [],
        effects: [],
        focusRequest: opts.focusRequest,
        result: null,
    });
}

interface WinSpec {
    win: string;
    fullscreen?: boolean;
    floating?: boolean;
    sticky?: boolean;
    minimized?: boolean;
    out?: string;
    ws?: string;
}

function makeObs(refs: Map<string, object>, specs: WinSpec[], opts?: { gesture?: boolean; focusedGlobal?: string; foregroundKnown?: boolean }): Obs {
    const toNative = (ws: string): string => (ws === "ws1" ? "native-1" : ws === "ws2" ? "native-2" : ws);
    const domainWs = toNative("ws1");
    return {
        mode: "per-output-local",
        topology: [{ out: "out", order: [toNative("ws1")], current: toNative("ws1"), complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
        bindings: [],
        domains: [{
            id: "out", ws: domainWs, bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 0, focused: "", readable: true, hold: "",
            complete: true,
            windows: specs.map((s) => ({
                win: s.win, ref: refs.get(s.win) as object,
                out: s.out ?? "out", ws: toNative(s.ws ?? "ws1"),
                rect: { x: 0, y: 0, w: 100, h: 100 },
                floating: s.floating ?? false, fitExcluded: false,
                fullscreen: s.fullscreen ?? false,
                maximized: false, sticky: s.sticky ?? false,
                fixedAuto: false, fixedSuppress: false, nativeWs: null,
                hints: { minW: null, minH: null, maxW: null, maxH: null },
                minimized: s.minimized ?? false,
            })),
            closed: [],
        }],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: opts?.gesture ?? false,
        focusedGlobal: opts?.focusedGlobal ?? "win-a",
        foregroundKnown: opts?.foregroundKnown ?? true,
        screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
        cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
        focusFollowsCursor: false, cursorWarp: false,
        switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
        presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
    };
}

interface FgModel {
    current: string | null;
}

function acceptParsed(text: string): ApplyEnv["reply"] {
    const parsed = parseWorldReply(text);
    assert.ok(parsed !== null && isCoreWorldReply(parsed));
    if (!isCoreWorldReply(parsed)) {
        throw new Error("unreachable");
    }
    return parsed;
}

function setupState(): WorldAdapterState {
    const state = new WorldAdapterState("test-adapter", "owner-a");
    state.onDiscovery("gen-1", "test-adapter");
    state.invalidate();
    const flight = state.beginCapture("req-1", null);
    assert.ok(flight !== null);
    return state;
}

function baseEnv(
    state: WorldAdapterState,
    observation: Obs,
    reply: ApplyEnv["reply"],
    fg: FgModel,
    refs: Map<string, object>,
    focusCalls: string[],
    overrides?: Partial<ApplyEnv>,
): ApplyEnv {
    return {
        state,
        observation,
        reply,
        dispatchWatermark: state.currentInvalid,
        takeResult: null,
        log: () => undefined,
        isStillListed: () => true,
        readActiveWindowId: () => fg.current,
        readWindowId: (ref) => {
            for (const [id, r] of refs) {
                if (r === ref) {
                    return id;
                }
            }
            return null;
        },
        readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
        readLiveMembership: (ref) => {
            for (const d of observation.domains) {
                for (const w of d.windows) {
                    if (w.ref === ref) {
                        return { out: w.out, ws: w.ws };
                    }
                }
            }
            return null;
        },
        readLiveSafety: () => ({ minimized: false, fullscreen: false, maximized: false, sticky: false }),
        readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: null }),
        setGeometry: () => { throw new Error("no geometry here"); },
        setMembership: () => { throw new Error("no membership here"); },
        sendToOutput: () => { throw new Error("no send here"); },
        resolveOutput: () => null,
        resolveDesktop: () => null,
        switchView: () => { throw new Error("no view here"); },
        createDesktop: () => null,
        removeDesktop: () => false,
        focusWindow: (ref) => {
            for (const [id, r] of refs) {
                if (r === ref) {
                    focusCalls.push(id);
                    fg.current = id;
                    return true;
                }
            }
            return false;
        },
        desktopOccupants: () => ({ total: 0, readable: true }),
        desktopVisible: () => false,
        readLiveView: () => null,
        desktopById: () => null,
        minDesktops: 2,
        ...overrides,
    };
}

function twoWindowRefs(): Map<string, object> {
    return new Map<string, object>([["win-a", {}], ["win-c", {}]]);
}

describe("world retained focus request", () => {
    it("duplicate redelivery of one publication stays silent with exactly one setter", () => {
        // Single-publication duplicate-spent path only: one reply applies
        // once (exactly one native setter with truthful raw feedback), then
        // two redeliveries of that same publication stay silent. Skip
        // semantics across advancing publications (issuing rev frozen while
        // the reply rev advances) are proved by the genuine-producer test
        // world-focus-producer.test.ts, not here.
        const state = setupState();
        const reply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c" }]);
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        const first = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(first.effects, 1);
        assert.deepEqual(focusCalls, ["win-c"]);
        let rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.id, "focus-7");
        assert.equal(rows[0]?.target, "win-c");
        assert.equal(rows[0]?.rev, "7");
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.reason, "");
        assert.equal(rows[0]?.pre, "win-a");
        assert.equal(rows[0]?.post, "win-c");
        assert.equal(rows[0]?.foregroundProven, true);
        assert.deepEqual(rows[0]?.arrival, []);
        // Two unchanged publications: no second setter, no second row.
        const second = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(second.effects, 0);
        const third = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(third.effects, 0);
        assert.deepEqual(focusCalls, ["win-c"]);
        rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
    });

    it("already-target native fallback satisfies with zero setters", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c" }], { focusedGlobal: "win-c", foregroundKnown: true });
        const fg: FgModel = { current: "win-c" };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(result.effects, 0);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "");
        assert.equal(rows[0]?.pre, "win-c");
        assert.equal(rows[0]?.post, "win-c");
        assert.equal(rows[0]?.foregroundProven, true);
    });

    it("third-bound fresh foreground no-calls as foreground-changed", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs: Map<string, object> = new Map([["win-a", {}], ["win-b", {}], ["win-c", {}]]);
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-b" }, { win: "win-c" }], { focusedGlobal: "win-a", foregroundKnown: true });
        // Native input moved focus to a third live window after the
        // dispatch capture: never steal it back.
        const fg: FgModel = { current: "win-b" };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(result.effects, 0);
        assert.equal(result.held, 1);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "foreground-changed");
        assert.equal(rows[0]?.pre, "win-b");
        assert.equal(rows[0]?.post, "win-b");
        assert.equal(rows[0]?.foregroundProven, false);
        // The safety no-call spent: a duplicate publication never retries.
        const again = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(again.effects, 0);
        assert.deepEqual(focusCalls, []);
        assert.equal(state.focusFeedbacksForReport().length, 1);
    });

    it("foreign fresh foreground no-calls as foreground-changed", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c" }], { focusedGlobal: "win-a", foregroundKnown: true });
        // Fresh foreground names an unbound id outside the observation:
        // newer native input intervened, never dispatch behind it.
        const fg: FgModel = { current: "win-foreign" };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(result.effects, 0);
        assert.equal(result.held, 1);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "foreground-changed");
        assert.equal(rows[0]?.pre, "win-foreign");
        assert.equal(rows[0]?.foregroundProven, false);
    });

    it("unknown captured baseline no-calls as focus-unproven", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c" }], { focusedGlobal: "", foregroundKnown: false });
        // The dispatch capture never proved a foreground: a stable-looking
        // fresh read proves nothing, never dispatch blind.
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(result.effects, 0);
        assert.equal(result.held, 1);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "focus-unproven");
        assert.equal(rows[0]?.pre, "win-a");
        assert.equal(rows[0]?.foregroundProven, false);
    });

    it("known windowless baseline with a fresh window no-calls as foreground-changed", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c" }], { focusedGlobal: "", foregroundKnown: true });
        // Proven no-client at capture, now a window holds the foreground:
        // newer native input intervened, never dispatch behind it.
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(result.effects, 0);
        assert.equal(result.held, 1);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "foreground-changed");
        assert.equal(rows[0]?.pre, "win-a");
        assert.equal(rows[0]?.foregroundProven, false);
    });

    it("unknown foreground never dispatches blind and spends once", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c" }]);
        const fg: FgModel = { current: null };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(result.held, 1);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "focus-unproven");
        assert.equal(rows[0]?.foregroundProven, false);
        // The safety no-call spent: a duplicate publication never retries.
        const again = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(again.effects, 0);
        assert.deepEqual(focusCalls, []);
        assert.equal(state.focusFeedbacksForReport().length, 1);
    });

    it("observed fullscreen alone never withholds an automatic request", () => {
        // The automatic overlay barrier is deleted: observed flags never
        // withhold. Under benign live safety the automatic request
        // attempts the native setter exactly like any ordinary target.
        const state = setupState();
        const reply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c", fullscreen: true }]);
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(result.effects, 1);
        assert.deepEqual(focusCalls, ["win-c"]);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.reason, "");
    });

    it("drifted homing holds as a stale identity", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        // The survivor left the frozen source domain: never chase it.
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c", ws: "ws2" }]);
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(result.held, 1);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "identity-stale");
    });

    it("absent target holds as a stale identity", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({
            focusRequest: focusRow("focus-7", "win-c", "7"),
            windows: [winRow("win-a", "ws1")],
        }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs: Map<string, object> = new Map([["win-a", {}]]);
        const observation = makeObs(refs, [{ win: "win-a" }]);
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(result.held, 1);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "identity-stale");
    });

    it("null envelope retires; the same id then refuses as stale while a newer id adopts", () => {
        const state = setupState();
        const req = (id: string, target: string, rev: string): WorldFocusRequest => ({
            id, target, rev, cause: "positive-close", out: "out", ws: "ws1", cmdSeq: null, afterShown: null,
        });
        const oldReq = req("focus-7", "win-c", "7");
        assert.equal(state.observeFocusEnvelope(oldReq, "gen-1"), "fresh");
        assert.equal(state.observeFocusEnvelope(null, "gen-1"), "absent");
        // A null envelope cancelled the unstarted work: the same id is now
        // stale against the retired-revision fence.
        assert.equal(state.observeFocusEnvelope(oldReq, "gen-1"), "stale");
        const newReq = req("focus-9", "win-a", "9");
        assert.equal(state.observeFocusEnvelope(newReq, "gen-1"), "fresh");
        assert.ok(state.isCurrentFocusRequest(newReq, "gen-1"));
    });

    it("newer publication cancels unstarted old work without actuating it", () => {
        const state = setupState();
        const reply7 = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        const reply9 = acceptParsed(replyText({
            focusRequest: focusRow("focus-9", "win-a", "9"),
            rev: "9",
        }));
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c" }]);
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        // Only the newer publication ever applies: win-a is already
        // foreground there, so the newer request satisfies with zero
        // setters and the older id never dispatches.
        const asReq = (reply: ApplyEnv["reply"]): WorldFocusRequest => {
            const row = reply.focusRequest;
            assert.ok(row !== null);
            return row;
        };
        assert.equal(state.observeFocusEnvelope(asReq(reply7), "gen-1"), "fresh");
        assert.equal(state.observeFocusEnvelope(asReq(reply9), "gen-1"), "fresh");
        assert.equal(state.observeFocusEnvelope(asReq(reply7), "gen-1"), "stale");
        assert.deepEqual(state.acceptReply(reply9, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const result = applyWorldReply(baseEnv(state, observation, reply9, fg, refs, focusCalls));
        assert.equal(result.effects, 0);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.id, "focus-9");
        assert.equal(rows[0]?.reason, "");
    });

    it("same-revision mismatch refuses as stale without spending", () => {
        const state = setupState();
        const reqA: WorldFocusRequest = { id: "focus-a", target: "win-c", rev: "7", cause: "positive-close", out: "out", ws: "ws1", cmdSeq: null, afterShown: null };
        const reqB: WorldFocusRequest = { id: "focus-b", target: "win-a", rev: "7", cause: "positive-close", out: "out", ws: "ws1", cmdSeq: null, afterShown: null };
        assert.equal(state.observeFocusEnvelope(reqA, "gen-1"), "fresh");
        assert.equal(state.observeFocusEnvelope(reqB, "gen-1"), "stale");
        // The current record is untouched and unspent.
        assert.ok(state.isCurrentFocusRequest(reqA, "gen-1"));
        assert.equal(state.spendFocus("focus-a"), true);
    });

    it("foreign-generation envelopes refuse as stale", () => {
        const state = setupState();
        const req: WorldFocusRequest = { id: "focus-7", target: "win-c", rev: "7", cause: "positive-close", out: "out", ws: "ws1", cmdSeq: null, afterShown: null };
        assert.equal(state.observeFocusEnvelope(req, "gen-x"), "stale");
        assert.equal(state.observeFocusEnvelope(null, "gen-x"), "absent");
        assert.equal(state.observeFocusEnvelope(req, "gen-1"), "fresh");
    });

    it("fresh handshake resets accounting while transport loss keeps it", () => {
        const state = setupState();
        const req: WorldFocusRequest = { id: "focus-7", target: "win-c", rev: "7", cause: "positive-close", out: "out", ws: "ws1", cmdSeq: null, afterShown: null };
        assert.equal(state.observeFocusEnvelope(req, "gen-1"), "fresh");
        assert.equal(state.spendFocus("focus-7"), true);
        // Context-preserving transport loss retains the accounting: the
        // spent record still refuses replays.
        state.noteRejectedOrTransportError();
        assert.equal(state.observeFocusEnvelope(req, "gen-1"), "duplicate-spent");
        assert.ok(state.isCurrentFocusRequest(req, "gen-1"));
        // A fresh handshake requires fresh intent: the old tuple adopts
        // anew only on the new binding, never replays old work by itself.
        state.onDiscovery("gen-2", "test-adapter");
        assert.ok(!state.isCurrentFocusRequest(req, "gen-1"));
        assert.equal(state.observeFocusEnvelope(req, "gen-1"), "stale");
        assert.equal(state.observeFocusEnvelope(req, "gen-2"), "fresh");
    });

    it("null focusRequest retires without actuating", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ focusRequest: null }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c" }]);
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.deepEqual(focusCalls, []);
        assert.equal(state.focusFeedbacksForReport().length, 0);
        assert.equal(result.effects, 0);
    });

    it("explicit-command with afterShown carries exactly the frozen members", () => {
        // Explicit-command request carrying frozen selection dependencies:
        // the accepted setter proves the target and the row carries one
        // bracket per frozen member only (win-c here; win-a rides the
        // observation but is not a frozen member, so it never samples).
        const state = setupState();
        const reply = acceptParsed(replyText({
            focusRequest: focusRow("focus-5", "win-c", "5", {
                cause: "explicit-command", cmdSeq: "5", afterShown: afterShownFor("win-c"),
            }),
            workspaces: [wsBindRow("ws1")],
        }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c" }]);
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        const env = baseEnv(state, observation, reply, fg, refs, focusCalls, {
            readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: true }),
            readLiveView: () => "native-1",
        });
        const result = applyWorldReply(env);
        assert.equal(result.effects, 1);
        assert.deepEqual(focusCalls, ["win-c"]);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "accepted");
        assert.deepEqual((rows[0]?.arrival ?? []).map((b) => b.win), ["win-c"]);
        assert.equal((rows[0]?.arrival ?? [])[0]?.obs, "1");
        // The automatic positive-close twin rides empty arrival.
        const auto = setupState();
        const autoReply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(auto.acceptReply(autoReply, "req-1", auto.currentInvalid), { applied: true, reason: "accepted" });
        const autoRefs = twoWindowRefs();
        const autoResult = applyWorldReply(baseEnv(auto, makeObs(autoRefs, [{ win: "win-a" }, { win: "win-c" }]), autoReply, { current: "win-a" }, autoRefs, []));
        assert.equal(autoResult.effects, 1);
        assert.deepEqual(auto.focusFeedbacksForReport()[0]?.arrival, []);
    });

    it("observed fullscreen never withholds an explicit-command request either", () => {
        // Same fullscreen target: the explicit-command request proceeds to
        // the live mechanical gates exactly like the automatic twin above.
        // The skipped barrier was the observed-flag fence, never the live
        // mechanical recheck.
        const auto = setupState();
        const autoReply = acceptParsed(replyText({ focusRequest: focusRow("focus-7", "win-c", "7") }));
        assert.deepEqual(auto.acceptReply(autoReply, "req-1", auto.currentInvalid), { applied: true, reason: "accepted" });
        const autoRefs = twoWindowRefs();
        const autoCalls: string[] = [];
        const autoResult = applyWorldReply(baseEnv(
            auto, makeObs(autoRefs, [{ win: "win-a" }, { win: "win-c", fullscreen: true }]),
            autoReply, { current: "win-a" }, autoRefs, autoCalls,
        ));
        assert.deepEqual(autoCalls, ["win-c"]);
        assert.equal(autoResult.effects, 1);
        const explicit = setupState();
        const explicitReply = acceptParsed(replyText({
            focusRequest: focusRow("focus-5", "win-c", "5", { cause: "explicit-command", cmdSeq: "5" }),
        }));
        assert.deepEqual(explicit.acceptReply(explicitReply, "req-1", explicit.currentInvalid), { applied: true, reason: "accepted" });
        const explicitRefs = twoWindowRefs();
        const explicitCalls: string[] = [];
        // Live safety still disagrees here (readLiveSafety names no
        // fullscreen), so the explicit write proceeds: the barrier skipped
        // is the observed-flag fence, never the live mechanical recheck.
        const explicitResult = applyWorldReply(baseEnv(
            explicit, makeObs(explicitRefs, [{ win: "win-a" }, { win: "win-c", fullscreen: true }]),
            explicitReply, { current: "win-a" }, explicitRefs, explicitCalls,
        ));
        assert.deepEqual(explicitCalls, ["win-c"]);
        assert.equal(explicitResult.effects, 1);
    });

    it("reader refuses old shapes and malformed focusRequest rows", () => {
        const text = (focusRequest: unknown, effects: unknown = []): string => JSON.stringify({
            v: 4, adapter: "test-adapter", gen: "gen-1", req: "req-1",
            obs: "1", rev: "7", ready: true, consumed: false,
            source: "req-1", nativeInvalid: "1", clock: "1",
            gestureSettled: null, gestureAssoc: null,
            focusRequest,
            operationSettled: [], nativeCreations: [],
            warpCursor: null,
            intentWrites: [], switchSettled: [], diagnostics: [],
            presentation: DISABLED_PRESENTATION,
            scopes: [], workspaces: [], outputs: [],
            windows: [],
            transfers: [],
            effects,
            result: null,
        });
        const good = { id: "focus-7", target: "win-c", rev: "7", cause: "positive-close", out: "out", ws: "ws1", cmdSeq: null, afterShown: null };
        assert.ok(parseWorldReply(text(good)) !== null, "well-formed request parses");
        // Old closeFocus key refuses as an unknown field.
        const oldKey = JSON.parse(text(good)) as Record<string, unknown>;
        oldKey["closeFocus"] = oldKey["focusRequest"];
        delete oldKey["focusRequest"];
        assert.equal(parseWorldReply(JSON.stringify(oldKey)), null, "retired closeFocus key refuses");
        // Any effects row refuses: the lane is required empty.
        assert.equal(parseWorldReply(text(good, [{ id: "fx-1" }])), null, "non-empty effects refuses");
        // Unknown cause refuses.
        assert.equal(parseWorldReply(text({ ...good, cause: "fx-spell" })), null, "unknown cause refuses");
        // Explicit-command demands a command correlation ...
        assert.equal(parseWorldReply(text({ ...good, cause: "explicit-command", cmdSeq: null })), null, "explicit without cmdSeq refuses");
        // ... and automatic causes forbid one.
        assert.equal(parseWorldReply(text({ ...good, cmdSeq: "5" })), null, "automatic with cmdSeq refuses");
        // afterShown rides explicit-command only.
        assert.equal(
            parseWorldReply(text({ ...good, afterShown: afterShownFor("win-c") })),
            null, "automatic with afterShown refuses",
        );
        // Missing and extra keys refuse.
        const missing = { ...good } as Record<string, unknown>;
        delete missing["rev"];
        assert.equal(parseWorldReply(text(missing)), null, "missing key refuses");
        assert.equal(parseWorldReply(text({ ...good, extra: 1 })), null, "extra key refuses");
        // afterShown views must sit inside selected.
        const badViews = afterShownFor("win-c") as Record<string, unknown>;
        badViews["views"] = [{ out: "other", ws: "ws9" }];
        assert.equal(
            parseWorldReply(text({ ...good, cause: "explicit-command", cmdSeq: "5", afterShown: badViews })),
            null, "view outside selected refuses",
        );
    });
});
