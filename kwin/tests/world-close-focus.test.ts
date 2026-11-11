// Retained positive-close focus (v4): the adapter applies only the latest
// publication, spends once per bound id before the native attempt or the
// safety no-call, and correlates raw focusFeedback by exact
// (id,target,rev) with empty arrival. Null/newer envelopes cancel
// unstarted old work through bounded current-record/retired-revision
// accounting; stale envelopes and late callbacks cannot reactivate it.
// Direct applyWorldReply unit tests plus state-accounting tests; no live
// KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply, type ApplyEnv } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { parseWorldReply, isCoreWorldReply } from "../src/world-report";
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

const closeRow = (id: string, target: string, rev: string): Record<string, unknown> => ({
    id, target, rev, cause: "positive-close", out: "out", ws: "ws1",
});

function replyText(opts: {
    closeFocus: Record<string, unknown> | null;
    windows?: Record<string, unknown>[];
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
        workspaces: [], outputs: [],
        windows: opts.windows ?? [winRow("win-a", "ws1"), winRow("win-c", "ws1")],
        transfers: [],
        effects: [],
        closeFocus: opts.closeFocus,
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
    return {
        mode: "per-output-local",
        topology: [{ out: "out", order: ["ws1"], current: "ws1", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
        bindings: [],
        domains: [{
            id: "out", ws: "ws1", bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 0, focused: "", readable: true, hold: "",
            complete: true,
            windows: specs.map((s) => ({
                win: s.win, ref: refs.get(s.win) as object,
                out: s.out ?? "out", ws: s.ws ?? "ws1",
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

describe("world retained close focus", () => {
    it("duplicate redelivery of one publication stays silent with exactly one setter", () => {
        // Single-publication duplicate-spent path only: one reply applies
        // once (exactly one native setter with truthful raw feedback), then
        // two redeliveries of that same publication stay silent. Skip
        // semantics across advancing publications (issuing rev frozen while
        // the reply rev advances) are proved by the genuine-producer test
        // world-close-focus-producer.test.ts, not here.
        const state = setupState();
        const reply = acceptParsed(replyText({ closeFocus: closeRow("close-focus-7", "win-c", "7") }));
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
        assert.equal(rows[0]?.id, "close-focus-7");
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
        const reply = acceptParsed(replyText({ closeFocus: closeRow("close-focus-7", "win-c", "7") }));
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
        const reply = acceptParsed(replyText({ closeFocus: closeRow("close-focus-7", "win-c", "7") }));
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
        const reply = acceptParsed(replyText({ closeFocus: closeRow("close-focus-7", "win-c", "7") }));
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
        const reply = acceptParsed(replyText({ closeFocus: closeRow("close-focus-7", "win-c", "7") }));
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
        const reply = acceptParsed(replyText({ closeFocus: closeRow("close-focus-7", "win-c", "7") }));
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
        const reply = acceptParsed(replyText({ closeFocus: closeRow("close-focus-7", "win-c", "7") }));
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

    it("overlay target holds with informative safety no-call feedback", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ closeFocus: closeRow("close-focus-7", "win-c", "7") }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c", fullscreen: true }]);
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, refs, focusCalls));
        assert.equal(result.held, 1);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "fullscreen");
    });

    it("drifted homing holds as a stale identity", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ closeFocus: closeRow("close-focus-7", "win-c", "7") }));
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
            closeFocus: closeRow("close-focus-7", "win-c", "7"),
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
        const oldReq = { id: "close-focus-7", target: "win-c", rev: "7", out: "out", ws: "ws1" };
        assert.equal(state.observeCloseEnvelope(oldReq, "gen-1"), "fresh");
        assert.equal(state.observeCloseEnvelope(null, "gen-1"), "absent");
        // A null envelope cancelled the unstarted work: the same id is now
        // stale against the retired-revision fence.
        assert.equal(state.observeCloseEnvelope(oldReq, "gen-1"), "stale");
        const newReq = { id: "close-focus-9", target: "win-a", rev: "9", out: "out", ws: "ws1" };
        assert.equal(state.observeCloseEnvelope(newReq, "gen-1"), "fresh");
        assert.ok(state.isCurrentCloseRequest("close-focus-9", "win-a", "9", "out", "ws1", "gen-1"));
    });

    it("newer publication cancels unstarted old work without actuating it", () => {
        const state = setupState();
        const reply7 = acceptParsed(replyText({ closeFocus: closeRow("close-focus-7", "win-c", "7") }));
        const reply9 = acceptParsed(replyText({
            closeFocus: closeRow("close-focus-9", "win-a", "9"),
            rev: "9",
        }));
        const refs = twoWindowRefs();
        const observation = makeObs(refs, [{ win: "win-a" }, { win: "win-c" }]);
        const fg: FgModel = { current: "win-a" };
        const focusCalls: string[] = [];
        // Only the newer publication ever applies: win-a is already
        // foreground there, so the newer request satisfies with zero
        // setters and the older id never dispatches.
        assert.equal(state.observeCloseEnvelope(
            { id: "close-focus-7", target: "win-c", rev: "7", out: "out", ws: "ws1" }, "gen-1",
        ), "fresh");
        assert.equal(state.observeCloseEnvelope(
            { id: "close-focus-9", target: "win-a", rev: "9", out: "out", ws: "ws1" }, "gen-1",
        ), "fresh");
        assert.equal(state.observeCloseEnvelope(
            { id: "close-focus-7", target: "win-c", rev: "7", out: "out", ws: "ws1" }, "gen-1",
        ), "stale");
        void reply7;
        assert.deepEqual(state.acceptReply(reply9, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const result = applyWorldReply(baseEnv(state, observation, reply9, fg, refs, focusCalls));
        assert.equal(result.effects, 0);
        assert.deepEqual(focusCalls, []);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.id, "close-focus-9");
        assert.equal(rows[0]?.reason, "");
    });

    it("same-revision mismatch refuses as stale without spending", () => {
        const state = setupState();
        const reqA = { id: "close-focus-a", target: "win-c", rev: "7", out: "out", ws: "ws1" };
        const reqB = { id: "close-focus-b", target: "win-a", rev: "7", out: "out", ws: "ws1" };
        assert.equal(state.observeCloseEnvelope(reqA, "gen-1"), "fresh");
        assert.equal(state.observeCloseEnvelope(reqB, "gen-1"), "stale");
        // The current record is untouched and unspent.
        assert.ok(state.isCurrentCloseRequest("close-focus-a", "win-c", "7", "out", "ws1", "gen-1"));
        assert.equal(state.spendClose("close-focus-a"), true);
    });

    it("foreign-generation envelopes refuse as stale", () => {
        const state = setupState();
        const req = { id: "close-focus-7", target: "win-c", rev: "7", out: "out", ws: "ws1" };
        assert.equal(state.observeCloseEnvelope(req, "gen-x"), "stale");
        assert.equal(state.observeCloseEnvelope(null, "gen-x"), "absent");
        assert.equal(state.observeCloseEnvelope(req, "gen-1"), "fresh");
    });

    it("fresh handshake resets accounting while transport loss keeps it", () => {
        const state = setupState();
        const req = { id: "close-focus-7", target: "win-c", rev: "7", out: "out", ws: "ws1" };
        assert.equal(state.observeCloseEnvelope(req, "gen-1"), "fresh");
        assert.equal(state.spendClose("close-focus-7"), true);
        // Context-preserving transport loss retains the accounting: the
        // spent record still refuses replays.
        state.noteRejectedOrTransportError();
        assert.equal(state.observeCloseEnvelope(req, "gen-1"), "duplicate-spent");
        assert.ok(state.isCurrentCloseRequest("close-focus-7", "win-c", "7", "out", "ws1", "gen-1"));
        // A fresh handshake requires fresh intent: the old tuple adopts
        // anew only on the new binding, never replays old work by itself.
        state.onDiscovery("gen-2", "test-adapter");
        assert.ok(!state.isCurrentCloseRequest("close-focus-7", "win-c", "7", "out", "ws1", "gen-1"));
        assert.equal(state.observeCloseEnvelope(req, "gen-1"), "stale");
        assert.equal(state.observeCloseEnvelope(req, "gen-2"), "fresh");
    });

    it("null closeFocus retires without actuating", () => {
        const state = setupState();
        const reply = acceptParsed(replyText({ closeFocus: null }));
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
});
