// Correlated one-shot focus feedback (v4): fresh foreground pre/post,
// setter-independent proof, no false rows for raise/stack, fence
// discipline across reentrant probes, lifetime binding, divergent
// counters, and lane caps. Direct applyWorldReply unit tests plus
// host-level shown-witness tests; no live KWin.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { applyWorldReply, type ApplyEnv } from "../src/world-apply";
import { WorldAdapterState } from "../src/world-state";
import { parseWorldReply, isCoreWorldReply } from "../src/world-report";
import { DISABLED_PRESENTATION } from "./world-wire-harness";
import { createWorldApplyHost } from "../src/world-native";

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

const focusReqRow = (id: string, target: string, opts?: {
    cause?: string; rev?: string; cmdSeq?: string | null; afterShown?: Record<string, unknown> | null;
}): Record<string, unknown> => {
    const cause = opts?.cause ?? "automatic-admission";
    const explicit = cause === "explicit-command";
    return {
        id, target, rev: opts?.rev ?? "1", cause,
        out: "out", ws: "ws1",
        cmdSeq: explicit ? (opts?.cmdSeq ?? "1") : null,
        afterShown: opts?.afterShown !== undefined ? opts.afterShown : null,
    };
};

// Frozen selection dependencies for one explicit-command request: the
// existing parent operation, the issuing observation counter, and the
// affected scopes. Views must sit inside selected.
const afterShownFor = (members: Array<{ win: string; frame: [number, number, number, number] | null }>, opts?: {
    op?: string; obs?: string; views?: Array<{ out: string; ws: string }>;
    selected?: Array<{ out: string; ws: string }>; departing?: Array<{ out: string; ws: string }>;
}): Record<string, unknown> => {
    const selected = opts?.selected ?? [{ out: "out", ws: "ws1" }];
    return {
        op: opts?.op ?? "op-1", obs: opts?.obs ?? "1",
        members: members.map((m) => ({ win: m.win, out: "out", ws: "ws1", frame: m.frame })),
        views: opts?.views ?? [{ out: "out", ws: "ws1" }],
        selected,
        departing: opts?.departing ?? [],
    };
};

const wsBindRow = (ws: string): Record<string, unknown> => ({
    id: ws, out: "out", bind: ws,
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
        obs: opts.obs ?? "1", rev: opts.rev ?? "1", ready: true, consumed: false,
        source: "req-1", nativeInvalid: "1", clock: opts.clock ?? "1",
        gestureSettled: null, gestureAssoc: null,
        focusRequest: opts.focusRequest,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: opts.workspaces ?? [], outputs: [],
        windows: opts.windows ?? [winRow("win-b", "ws1")],
        transfers: [],
        effects: [],
        result: null,
    });
}

function makeObs(targetRef: object, opts?: { focusedGlobal?: string; gesture?: boolean }): Obs {
    return {
        mode: "per-output-local",
        topology: [{ out: "out", order: ["ws1"], current: "ws1", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
        bindings: [],
        domains: [{
            id: "out", ws: "ws1", bounds: { x: 0, y: 0, w: 800, h: 600 },
            gap: 8, outer: 0, focused: "win-b", readable: true, hold: "",
            complete: true,
            windows: [{
                win: "win-b", ref: targetRef, out: "out", ws: "ws1",
                rect: { x: 0, y: 0, w: 100, h: 100 },
                floating: false, fitExcluded: false, fullscreen: false,
                maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false, nativeWs: null,
                hints: { minW: null, minH: null, maxW: null, maxH: null },
                minimized: false,
            }],
            closed: [],
        }],
        minimized: [],
        predicate: "both-axes-fixed",
        complete: true,
        gesture: opts?.gesture ?? false,
        focusedGlobal: opts?.focusedGlobal ?? "",
        foregroundKnown: (opts?.focusedGlobal ?? "") !== "",
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

function acceptParsed(_state: WorldAdapterState, text: string): ApplyEnv["reply"] {    const parsed = parseWorldReply(text);
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
    targetRef: object,
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
        readWindowId: (ref) => (ref === targetRef ? "win-b" : null),
        readLiveRect: () => ({ x: 0, y: 0, w: 100, h: 100 }),
        readLiveMembership: () => ({ out: "out", ws: "ws1" }),
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
            if (ref === targetRef) {
                fg.current = "win-b";
                return true;
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

describe("world focus feedback rows", () => {
    it("records an accepted proven call with fresh pre/post, never capture-time", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ focusRequest: focusReqRow("focus-1", "win-b") , workspaces: [wsBindRow("ws1")] }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        // Capture-time foreground names another window; the fresh read
        // must win for both stamps.
        const observation = makeObs(targetRef, { focusedGlobal: "win-a" });
        const fg: FgModel = { current: "win-a" };
        const result = applyWorldReply(baseEnv(state, observation, reply, fg, targetRef));
        assert.equal(result.effects, 1);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.reason, "");
        assert.equal(rows[0]?.pre, "win-a");
        assert.equal(rows[0]?.post, "win-b");
        assert.equal(rows[0]?.foregroundProven, true);
        assert.deepEqual(rows[0]?.arrival, []);
        assert.equal(rows[0]?.attemptMs, "1");
    });

    it("records refused with the fresh post, unproven", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ focusRequest: focusReqRow("focus-1", "win-b") , workspaces: [wsBindRow("ws1")] }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef, { focusedGlobal: "win-a" });
        const fg: FgModel = { current: "win-a" };
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            focusWindow: () => false,
        });
        const result = applyWorldReply(env);
        assert.equal(result.effects, 0);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "refused");
        assert.equal(rows[0]?.reason, "native-refused");
        assert.equal(rows[0]?.pre, "win-a");
        assert.equal(rows[0]?.post, "win-a");
        assert.equal(rows[0]?.foregroundProven, false);
    });

    it("records a throw as ambiguous with the fresh post", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ focusRequest: focusReqRow("focus-1", "win-b") , workspaces: [wsBindRow("ws1")] }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            focusWindow: () => { throw new Error("boom"); },
        });
        const result = applyWorldReply(env);
        assert.equal(result.effects, 0);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "threw");
        assert.equal(rows[0]?.post, null);
        assert.equal(rows[0]?.foregroundProven, false);
    });

    it("records accepted-without-foreground as focus-unproven, never refused", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ focusRequest: focusReqRow("focus-1", "win-b") , workspaces: [wsBindRow("ws1")] }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: "win-a" };
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            // Setter claims success but the foreground never arrives.
            focusWindow: (ref) => ref === targetRef,
        });
        const result = applyWorldReply(env);
        assert.equal(result.effects, 0);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.reason, "focus-unproven");
        assert.equal(rows[0]?.post, "win-a");
        assert.equal(rows[0]?.foregroundProven, false);
    });

    it("proves setter-independently: refused yet foreground still proves", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ focusRequest: focusReqRow("focus-1", "win-b") , workspaces: [wsBindRow("ws1")] }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            // Native refuses, but the target is foreground anyway
            // (external convergence on the same turn).
            focusWindow: (ref) => {
                if (ref === targetRef) {
                    fg.current = "win-b";
                }
                return false;
            },
        });
        applyWorldReply(env);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "refused");
        assert.equal(rows[0]?.post, "win-b");
        assert.equal(rows[0]?.foregroundProven, true);
    });

    it("proves setter-independently: threw yet foreground still proves", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ focusRequest: focusReqRow("focus-1", "win-b") , workspaces: [wsBindRow("ws1")] }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            focusWindow: (ref) => {
                if (ref === targetRef) {
                    fg.current = "win-b";
                }
                throw new Error("boom-after-side-effect");
            },
        });
        applyWorldReply(env);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "threw");
        assert.equal(rows[0]?.post, "win-b");
        assert.equal(rows[0]?.foregroundProven, true);
    });

    it("requires a live listing before proof; raw post rides regardless", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ focusRequest: focusReqRow("focus-1", "win-b") , workspaces: [wsBindRow("ws1")] }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            isStillListed: () => false,
        });
        applyWorldReply(env);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        // Pre-setter gate fails on the vanished listing: narrow no-call
        // with the fresh foreground stamps.
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "identity-stale");
    });

    it("refuses a non-empty effects lane without actuating", () => {
        // The retired one-shot effect lane is required empty: a reply
        // carrying any effect row refuses at parse, so no focus row and
        // no native call can follow from it.
        const text = replyText({ focusRequest: null }).replace(
            '"effects":[]',
            '"effects":[{"id":"fx-9","kind":"raise","target":"win-b"}]',
        );
        assert.equal(parseWorldReply(text), null, "non-empty effects refuses");
    });

    it("never replays: duplicates record once, ack retires, stamps never restamp", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ focusRequest: focusReqRow("focus-1", "win-b"), clock: "9" }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        const env = () => baseEnv(state, observation, reply, fg, targetRef);
        applyWorldReply(env());
        applyWorldReply(env());
        let rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1, "spent request never records twice");
        assert.equal(rows[0]?.attemptMs, "9", "original broker-epoch stamp retained");
        // A later capture delivers the retained snapshot; the bound ready
        // reply retires exactly-equal rows.
        const flight = state.beginCapture("req-2", null);
        assert.ok(flight !== null);
        const retired = state.ackDelivered(flight.source, true);
        assert.equal(retired, 1);
        assert.deepEqual(state.focusFeedbacksForReport(), []);
    });

    it("emits a truthful superseded no-call when a probe moves the fence", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({
            focusRequest: focusReqRow("focus-1", "win-b", {
                cause: "explicit-command", afterShown: afterShownFor([{ win: "win-b", frame: null }]),
            }),
        }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: "win-a" };
        let focusCalls = 0;
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            // The arrival visibility probe fires a synchronous native signal
            // that advances the local fence mid-apply.
            readLiveVisibility: () => {
                state.invalidate();
                return null;
            },
            focusWindow: () => { focusCalls += 1; return true; },
        });
        const result = applyWorldReply(env);
        assert.equal(focusCalls, 0, "no actuation behind a moved fence");
        assert.equal(result.effects, 0);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "superseded");
    });

    it("defers on lifetime mismatch, resumes once the lifetime binds", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({
            focusRequest: focusReqRow("focus-1", "win-b", {
                cause: "explicit-command", afterShown: afterShownFor([{ win: "win-b", frame: null }]),
            }),
            workspaces: [wsBindRow("ws1")],
        }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        // Reentrant lifetime replacement under a stable wrapper: the ref
        // still lists, but its actual id names another lifetime. The frozen
        // shown proof fails, so the envelope stays unspent with no row.
        const dark = applyWorldReply(baseEnv(state, observation, reply, fg, targetRef, {
            readWindowId: () => "win-other",
            readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: true }),
            readLiveView: () => "ws1",
        }));
        assert.equal(dark.effects, 0, "mismatched lifetime never actuates");
        assert.deepEqual(state.focusFeedbacksForReport(), [], "mismatch deferral spends nothing");
        // The same latest snapshot resumes exactly once the lifetime binds.
        const lit = applyWorldReply(baseEnv(state, observation, reply, fg, targetRef, {
            readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: true }),
            readLiveView: () => "ws1",
        }));
        assert.equal(lit.effects, 1);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "accepted");
        for (const b of rows[0]?.arrival ?? []) {
            assert.equal(b.before.lifetimeMatched, true, "resumed bracket binds the lifetime");
            assert.equal(b.after.lifetimeMatched, true, "resumed bracket binds the lifetime");
        }
    });

    it("leaves foreground unproven on an unreadable production id without arrival sampling", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({
            // Admission cause without afterShown: no frozen proof to sample,
            // so the unreadable id rides the focus path itself.
            focusRequest: focusReqRow("focus-1", "win-b", { cause: "automatic-admission" }),
            workspaces: [wsBindRow("ws1")],
        }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        // The production reader exists but the id is unreadable (null)
        // while the ref still lists: the lifetime is unknown (never
        // matched), so the setter may still run but nothing proves.
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            readWindowId: () => null,
            readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: null }),
            readLiveView: () => "ws1",
            focusWindow: (ref) => {
                if (ref === targetRef) {
                    fg.current = "win-b";
                }
                return true;
            },
        });
        const result = applyWorldReply(env);
        assert.equal(result.effects, 0, "unreadable identity never proves");
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.reason, "focus-unproven");
        assert.equal(rows[0]?.post, "win-b");
        assert.equal(rows[0]?.foregroundProven, false);
        assert.deepEqual(rows[0]?.arrival ?? [], [], "no afterShown means no arrival sampling");
    });

    it("stamps bracket obs from the reply and the write stamp from the clock, diverged", () => {
        const state = setupState();
        // Broker obs, local fence, and dispatch clock deliberately
        // diverge: obs names the issuing observation, the fence the
        // current local watermark, the clock the broker epoch.
        const reply = acceptParsed(state, replyText({
            focusRequest: focusReqRow("focus-1", "win-b", {
                cause: "explicit-command", afterShown: afterShownFor([{ win: "win-b", frame: null }]),
            }),
            workspaces: [wsBindRow("ws1")],
            obs: "100",
            rev: "3",
            clock: "7",
        }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: true }),
            readLiveView: () => "ws1",
        });
        const result = applyWorldReply(env);
        assert.equal(result.effects, 1);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.ok((rows[0]?.arrival.length ?? 0) > 0, "afterShown request carries brackets");
        for (const b of rows[0]?.arrival ?? []) {
            assert.equal(b.obs, "100", "bracket obs is the reply obs, never the fence");
            assert.equal(b.before.obs, "100");
            assert.equal(b.after.obs, "100");
        }
        assert.equal(rows[0]?.attemptMs, "7", "write stamp is the broker clock, never wall-clock");
    });

    it("completes a pre-focus geometry stop with one superseded no-call, no replay", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({
            focusRequest: focusReqRow("focus-1", "win-b"),
            clock: "9",
            windows: [
                { ...winRow("win-b", "ws1"), frame: [10, 10, 200, 200], alloc: [10, 10, 200, 200] },
                // Second row so the window-loop top observes the fence
                // moved by the first row's geometry setter: a lone
                // geometry-only row `continue`s past the end-of-iteration
                // fence check, so without another row the move would only
                // surface later inside the focus loop itself.
                winRow("win-c", "ws1"),
            ],
        }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: "win-a" };
        let geometryCalls = 0;
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            // Synchronous native invalidation inside the geometry setter
            // (the live selector echo): the write lands, the fence moves,
            // and the apply stops before the focus section.
            setGeometry: () => {
                geometryCalls += 1;
                state.invalidate();
                // Reentrant native work moves the dispatch clock mid-apply:
                // rows must still carry the stamp captured before apply.
                state.recordClock("99");
                return true;
            },
            focusWindow: () => { throw new Error("focus must not run behind a moved fence"); },
        });
        const result = applyWorldReply(env);
        assert.equal(result.stoppedMidApply, true);
        assert.equal(geometryCalls, 1);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1, "early exit still spends the skipped focus request");
        assert.equal(rows[0]?.id, "focus-1");
        assert.equal(rows[0]?.target, "win-b");
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "superseded");
        assert.equal(rows[0]?.pre, "win-a");
        assert.equal(rows[0]?.post, "win-a");
        assert.equal(rows[0]?.foregroundProven, false);
        assert.deepEqual(rows[0]?.arrival, []);
        assert.equal(rows[0]?.attemptMs, "9", "original broker-epoch stamp retained");
        assert.equal(state.lastClock, "99", "reentrant clock move actually happened");
        // Duplicate redelivery spends nothing twice and restamps nothing.
        applyWorldReply(env);
        const again = state.focusFeedbacksForReport();
        assert.equal(again.length, 1, "spent request never records twice");
        assert.equal(again[0]?.attemptMs, "9");
    });
});

describe("world focus lane caps", () => {
    it("evicts oldest-first past 1024 rows", () => {
        const state = new WorldAdapterState("test-adapter", "owner-a");
        for (let i = 0; i < 1025; i += 1) {
            const n = String(i + 1).padStart(4, "0");
            const ok = state.recordFocusFeedback({
                id: `fx-${n}`, target: "win-b", rev: "1",
                setter: "not-called", reason: "scope-held",
                pre: null, post: null, foregroundProven: false,
                arrival: [], attemptMs: "1",
            });
            assert.equal(ok, true);
        }
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1024);
        assert.ok(!rows.some((r) => r.id === "fx-0001"), "oldest row evicted");
        assert.ok(rows.some((r) => r.id === "fx-1025"), "newest row retained");
    });

    it("evicts oldest-first past 10000 total brackets", () => {
        const state = new WorldAdapterState("test-adapter", "owner-a");
        // Two rows of 6000 brackets each: the total exceeds the cap, so
        // the oldest row evicts and only the newest survives.
        const many = (id: string) => ({
            id, target: "win-b", rev: "1",
            setter: "accepted" as const, reason: "" as const,
            pre: null, post: "win-b", foregroundProven: true,
            arrival: Array.from({ length: 6000 }, () => ({
                win: "win-b", obs: "1",
                before: { obs: "1", lifetimeMatched: true as const, visible: true as const, suppressionOwned: false as const, minimized: false as const, hostHidden: false as const },
                home: { out: "out", ws: "ws1" },
                frame: [0, 0, 100, 100] as [number, number, number, number],
                after: { obs: "1", lifetimeMatched: true as const, visible: true as const, suppressionOwned: false as const, minimized: false as const, hostHidden: false as const },
            })),
            attemptMs: "1",
        });
        void many;
        assert.equal(state.recordFocusFeedback(many("fx-1")), true);
        assert.equal(state.recordFocusFeedback(many("fx-2")), true);
        const rows = state.focusFeedbacksForReport();
        const total = rows.reduce((n, r) => n + r.arrival.length, 0);
        assert.ok(total <= 10000, `total brackets bounded, got ${total}`);
        assert.deepEqual(rows.map((r) => r.id), ["fx-2"]);
    });
});

describe("world live visibility host", () => {
    interface FakeWin {
        internalId: string;
        frameGeometry: { x: number; y: number; width: number; height: number };
        minimized?: boolean;
        hidden?: boolean;
    }
    const readProp = (value: object, property: string): unknown =>
        (value as Record<string, unknown>)[property];
    const setup = (wins: FakeWin[], windowAt: (point: unknown, count: number) => unknown) => {
        const surface = {
            windowList: () => wins,
            windowAt,
        };
        return createWorldApplyHost(surface, readProp);
    };
    const winB: FakeWin = { internalId: "win-b", frameGeometry: { x: 0, y: 0, width: 200, height: 200 } };
    const winA: FakeWin = { internalId: "win-a", frameGeometry: { x: 400, y: 0, width: 200, height: 200 } };

    it("proves shown only on the exact id among all matches (negative count)", () => {
        // A negative count returns every match, so occlusion cannot
        // falsify a positive: the witness still requires the exact
        // lifetime among all matches.
        const host = setup([winA, winB], () => [winA, winB]);
        assert.deepEqual(host.readLiveVisibility?.(winB), { minimized: null, hostHidden: null, visible: true });
        assert.deepEqual(host.readLiveVisibility?.(winA), { minimized: null, hostHidden: null, visible: true });
    });

    it("returns visible null on absence (show-desktop hides everything), never false", () => {
        const host = setup([winA, winB], () => []);
        assert.deepEqual(host.readLiveVisibility?.(winB), { minimized: null, hostHidden: null, visible: null });
    });

    it("returns visible null on lifetime mismatch, never a shown verdict", () => {
        const host = setup([winA], () => [winA]);
        assert.deepEqual(host.readLiveVisibility?.(winB), { minimized: null, hostHidden: null, visible: null });
    });

    it("reports distinct raw minimized/hostHidden facts and proves visible false", () => {
        const host = setup([winA, winB], () => { throw new Error("windowAt must not run behind proven hidden"); });
        assert.deepEqual(
            host.readLiveVisibility?.({ ...winB, minimized: true, hidden: false }),
            { minimized: true, hostHidden: false, visible: false },
        );
        assert.deepEqual(
            host.readLiveVisibility?.({ ...winB, minimized: false, hidden: true }),
            { minimized: false, hostHidden: true, visible: false },
        );
        const absent = setup([winA, winB], () => []);
        assert.deepEqual(
            absent.readLiveVisibility?.({ ...winB, minimized: false, hidden: false }),
            { minimized: false, hostHidden: false, visible: null },
        );
    });

    it("exposes fresh foreground and window ids", () => {
        const surface = {
            windowList: () => [winA, winB],
            activeWindow: winB,
        };
        const host = createWorldApplyHost(surface, readProp);
        assert.equal(host.readActiveWindowId?.(), "win-b");
        assert.equal(host.readWindowId?.(winB), "win-b");
    });
});

describe("world selection readiness", () => {
    // Frozen afterShown proof for selection-backed explicit focus: the
    // adapter samples and gates ONLY the frozen members (never rebuilt
    // from later rows, never enlarged), consults no transfer rows (the
    // parent may already have settled) and no command ticket. A
    // not-ready verdict leaves the envelope unspent with no row, so a
    // later latest snapshot of the same id resumes exactly once.
    const readyRefs = (): Map<string, object> => new Map<string, object>([
        ["win-b", {}], ["win-extra", {}], ["win-late", {}],
    ]);

    function readyObs(refMap: Map<string, object>, wins: string[]): Obs {
        return {
            mode: "per-output-local",
            topology: [{ out: "out", order: ["ws1"], current: "ws1", complete: true, rect: { x: 0, y: 0, w: 800, h: 600 }, primary: true }],
            bindings: [],
            domains: [{
                id: "out", ws: "ws1", bounds: { x: 0, y: 0, w: 800, h: 600 },
                gap: 8, outer: 0, focused: "win-b", readable: true, hold: "",
                complete: true,
                windows: wins.map((win) => ({
                    win, ref: refMap.get(win) as object, out: "out", ws: "ws1",
                    rect: { x: 0, y: 0, w: 100, h: 100 },
                    floating: false, fitExcluded: false, fullscreen: false,
                    maximized: false, sticky: false, fixedAuto: false, fixedSuppress: false, nativeWs: null,
                    hints: { minW: null, minH: null, maxW: null, maxH: null },
                    minimized: false,
                })),
                closed: [],
            }],
            minimized: [],
            predicate: "both-axes-fixed",
            complete: true,
            gesture: false,
            focusedGlobal: "win-a",
            foregroundKnown: true,
            screens: [], windowFacts: [], floatIntent: [], tileIntent: [], views: [], closed: [], hostOwned: false, equalCount: true, native: [],
            cursor: null, actionForeground: null, actionCached: null, actionOutputs: [],
            focusFollowsCursor: false, cursorWarp: false,
            switchTarget: "focused-output", freshPlacement: "focused-output-last-focused",
            presentation: { foreground: "", minimized: false, maximized: false, fullscreen: false, cloaked: false, hiddenWorkspace: false, shell: false, frame: null, borderEnabled: false, underlayEnabled: false, borderGap: 0, borderWidth: 0, underlayExtension: 0, chordHeld: false, moveWindow: "", paused: false, pagerOrder: [], pagerFrames: [] },
        };
    }

    function readyEnv(
        state: WorldAdapterState,
        observation: Obs,
        reply: ApplyEnv["reply"],
        refMap: Map<string, object>,
        fg: FgModel,
        calls: string[],
        opts?: {
            rect?: { x: number; y: number; w: number; h: number } | null;
            view?: string | null;
            visibility?: () => { minimized: boolean | null; hostHidden: boolean | null; visible: boolean | null } | null;
            readId?: (ref: object) => string | null;
        },
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
                if (opts?.readId !== undefined) {
                    return opts.readId(ref);
                }
                for (const [id, r] of refMap) {
                    if (r === ref) {
                        return id;
                    }
                }
                return null;
            },
            readLiveRect: () => opts?.rect !== undefined ? opts.rect : { x: 0, y: 0, w: 100, h: 100 },
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
            readLiveVisibility: opts?.visibility ?? (() => ({ minimized: false, hostHidden: false, visible: true })),
            setGeometry: () => { throw new Error("no geometry here"); },
            setMembership: () => { throw new Error("no membership here"); },
            sendToOutput: () => { throw new Error("no send here"); },
            resolveOutput: () => null,
            resolveDesktop: () => null,
            switchView: () => { throw new Error("no view here"); },
            createDesktop: () => null,
            removeDesktop: () => false,
            focusWindow: (ref) => {
                for (const [id, r] of refMap) {
                    if (r === ref) {
                        calls.push(id);
                        fg.current = id;
                        return true;
                    }
                }
                return false;
            },
            desktopOccupants: () => ({ total: 0, readable: true }),
            desktopVisible: () => false,
            readLiveView: () => opts?.view !== undefined ? opts.view : "ws1",
            desktopById: () => null,
            minDesktops: 2,
        };
    }

    function readyReply(afterShown: Record<string, unknown>, rev = "1", workspaces?: Record<string, unknown>[]): string {
        return replyText({
            focusRequest: focusReqRow("focus-5", "win-b", {
                cause: "explicit-command", cmdSeq: "5", afterShown,
            }),
            workspaces: workspaces ?? [wsBindRow("ws1")],
            rev,
        });
    }

    it("samples exactly the frozen members; late newcomers excluded, parent absent", () => {
        // No transfer rows anywhere (the parent already settled) and no
        // command correlation beyond the opaque ticket: readiness uses
        // only the frozen projection. win-late rides the observation but
        // is not a frozen member, so it never samples.
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor([
            { win: "win-b", frame: null },
            { win: "win-extra", frame: null },
        ])));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b", "win-extra", "win-late"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        const result = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(result.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.deepEqual((rows[0]?.arrival ?? []).map((b) => b.win), ["win-b", "win-extra"]);
    });

    it("withholds on an unreadable live frame for a null frozen frame, resumes once readable", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor([{ win: "win-b", frame: null }])));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        // A null frozen frame waives equality but still requires a
        // readable live frame: no observed rect at all defers unspent.
        const dark = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls, { rect: null }));
        assert.equal(dark.effects, 0);
        assert.deepEqual(calls, []);
        assert.deepEqual(state.focusFeedbacksForReport(), [], "deferral records no row and spends nothing");
        // Later latest snapshot with readable geometry resumes exactly once.
        const lit = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(lit.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        assert.equal(state.focusFeedbacksForReport().length, 1);
    });

    it("withholds on a mismatched frozen frame until the live geometry converges", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor([{ win: "win-b", frame: [10, 10, 200, 200] }])));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        const stale = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(stale.effects, 0, "frame mismatch defers, never refuses");
        assert.deepEqual(state.focusFeedbacksForReport(), []);
        const converged = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls, {
            rect: { x: 10, y: 10, w: 200, h: 200 },
        }));
        assert.equal(converged.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        assert.equal(state.focusFeedbacksForReport().length, 1);
    });

    it("withholds while a frozen view is not live, resumes once it shows", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor([{ win: "win-b", frame: null }])));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        const hidden = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls, { view: null }));
        assert.equal(hidden.effects, 0, "unproven frozen view defers, never refuses");
        assert.deepEqual(calls, []);
        assert.deepEqual(state.focusFeedbacksForReport(), []);
        const shown = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(shown.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        assert.equal(state.focusFeedbacksForReport().length, 1);
    });

    it("withholds while a frozen member sits in a departing scope", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor(
            [{ win: "win-b", frame: null }],
            { departing: [{ out: "out", ws: "ws1" }] },
        )));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        const result = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(result.effects, 0, "departing home never arrives from this snapshot");
        assert.deepEqual(calls, []);
        assert.deepEqual(state.focusFeedbacksForReport(), [], "departing deferral spends nothing");
    });

    it("withholds on an unbound frozen scope, resumes once the binding lands", () => {
        const state = setupState();
        const frozen = afterShownFor([{ win: "win-b", frame: null }]);
        const reply = acceptParsed(state, readyReply(frozen, "1", []));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        // No workspace row binds the frozen scope: the native target is
        // unknown, so the envelope stays unspent with no row (never a
        // literal fallback to the logical id).
        const dark = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(dark.effects, 0, "unbound scope never actuates");
        assert.deepEqual(calls, []);
        assert.deepEqual(state.focusFeedbacksForReport(), [], "unbound deferral spends nothing");
        // A newer publication of the same id carrying the binding adopts
        // and actuates exactly once.
        const bound = acceptParsed(state, readyReply(frozen, "2"));
        const flight = state.beginCapture("req-1", null);
        assert.ok(flight !== null);
        assert.deepEqual(state.acceptReply(bound, "req-1", flight.watermark), { applied: true, reason: "accepted" });
        const lit = applyWorldReply(readyEnv(state, observation, bound, refMap, fg, calls));
        assert.equal(lit.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        assert.equal(state.focusFeedbacksForReport().length, 1);
    });

    it("withholds on a minimized frozen member, resumes once shown", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor([{ win: "win-b", frame: null }])));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        const dark = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls, {
            visibility: () => ({ minimized: true, hostHidden: false, visible: false }),
        }));
        assert.equal(dark.effects, 0, "minimized member never actuates");
        assert.deepEqual(calls, []);
        assert.deepEqual(state.focusFeedbacksForReport(), [], "minimized deferral spends nothing");
        const lit = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(lit.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        assert.equal(state.focusFeedbacksForReport().length, 1);
    });

    it("withholds on a host-hidden frozen member, resumes once shown", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor([{ win: "win-b", frame: null }])));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        const dark = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls, {
            visibility: () => ({ minimized: false, hostHidden: true, visible: false }),
        }));
        assert.equal(dark.effects, 0, "host-hidden member never actuates");
        assert.deepEqual(calls, []);
        assert.deepEqual(state.focusFeedbacksForReport(), [], "host-hidden deferral spends nothing");
        const lit = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(lit.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        assert.equal(state.focusFeedbacksForReport().length, 1);
    });

    it("withholds on visible-false without minimized or host-hidden flags", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor([{ win: "win-b", frame: null }])));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        // A bare negative witness is still no positive proof: only an
        // explicit shown true satisfies the frozen proof.
        const dark = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls, {
            visibility: () => ({ minimized: false, hostHidden: false, visible: false }),
        }));
        assert.equal(dark.effects, 0, "visible-false never actuates");
        assert.deepEqual(calls, []);
        assert.deepEqual(state.focusFeedbacksForReport(), [], "visible-false deferral spends nothing");
        const lit = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(lit.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        assert.equal(state.focusFeedbacksForReport().length, 1);
    });

    it("withholds on an unreadable shown witness, resumes once proven", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor([{ win: "win-b", frame: null }])));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        const dark = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls, {
            visibility: () => ({ minimized: false, hostHidden: false, visible: null }),
        }));
        assert.equal(dark.effects, 0, "unreadable witness never actuates");
        assert.deepEqual(calls, []);
        assert.deepEqual(state.focusFeedbacksForReport(), [], "unreadable-witness deferral spends nothing");
        const lit = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(lit.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        assert.equal(state.focusFeedbacksForReport().length, 1);
    });

    it("withholds when a frozen member is absent from the native inventory", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor([
            { win: "win-b", frame: null },
            { win: "win-extra", frame: null },
        ])));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        // win-extra rides the frozen set but has no observed lifetime.
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        const dark = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(dark.effects, 0, "absent member never actuates");
        assert.deepEqual(calls, []);
        assert.deepEqual(state.focusFeedbacksForReport(), [], "absent-member deferral spends nothing");
        // The member may arrive while absent from the latest report: the
        // same snapshot resumes once the inventory carries it.
        const present = readyObs(refMap, ["win-b", "win-extra"]);
        const lit = applyWorldReply(readyEnv(state, present, reply, refMap, fg, calls));
        assert.equal(lit.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.deepEqual((rows[0]?.arrival ?? []).map((b) => b.win), ["win-b", "win-extra"]);
    });

    it("defers below the issuing watermark", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(
            afterShownFor([{ win: "win-b", frame: null }], { obs: "5" }),
        ));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        // Reply obs "1" predates the frozen issuing watermark "5": the
        // snapshot proves nothing about the issue, so it defers.
        const result = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls));
        assert.equal(result.effects, 0);
        assert.deepEqual(calls, []);
        assert.deepEqual(state.focusFeedbacksForReport(), []);
    });

    it("null envelope cancels unstarted readiness; a newer id then adopts once", () => {
        const state = setupState();
        const reply = acceptParsed(state, readyReply(afterShownFor([{ win: "win-b", frame: null }])));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const refMap = readyRefs();
        const observation = readyObs(refMap, ["win-b"]);
        const fg: FgModel = { current: "win-a" };
        const calls: string[] = [];
        // First snapshot cannot prove the frozen view: unstarted, no row.
        const deferred = applyWorldReply(readyEnv(state, observation, reply, refMap, fg, calls, { view: null }));
        assert.equal(deferred.effects, 0);
        assert.deepEqual(state.focusFeedbacksForReport(), []);
        // A null envelope retires the unstarted work: still no row, ever.
        const nullReply = acceptParsed(state, replyText({ focusRequest: null, workspaces: [wsBindRow("ws1")], rev: "2" }));
        const nullFlight = state.beginCapture("req-1", null);
        assert.ok(nullFlight !== null);
        assert.deepEqual(state.acceptReply(nullReply, "req-1", nullFlight.watermark), { applied: true, reason: "accepted" });
        applyWorldReply(readyEnv(state, observation, nullReply, refMap, fg, calls, { view: null }));
        assert.deepEqual(state.focusFeedbacksForReport(), [], "cancelled work never records");
        // A newer publication adopts and actuates exactly once.
        const newer = acceptParsed(state, replyText({
            focusRequest: focusReqRow("focus-9", "win-b", {
                cause: "explicit-command", rev: "9", cmdSeq: "9",
                afterShown: afterShownFor([{ win: "win-b", frame: null }]),
            }),
            workspaces: [wsBindRow("ws1")],
            rev: "3",
        }));
        const newerFlight = state.beginCapture("req-1", null);
        assert.ok(newerFlight !== null);
        assert.deepEqual(state.acceptReply(newer, "req-1", newerFlight.watermark), { applied: true, reason: "accepted" });
        const result = applyWorldReply(readyEnv(state, observation, newer, refMap, fg, calls));
        assert.equal(result.effects, 1);
        assert.deepEqual(calls, ["win-b"]);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.id, "focus-9");
    });
});
