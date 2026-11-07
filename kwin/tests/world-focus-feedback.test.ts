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

const fxRow = (id: string, kind: string, target: string, awaitShown = false): Record<string, unknown> => ({
    id, kind, target, rev: "1", cause: "explicit-command", cmdSeq: "1", awaitShown,
});

function replyText(opts: {
    effects: Record<string, unknown>[];
    windows?: Record<string, unknown>[];
    obs?: string;
    rev?: string;
    clock?: string;
}): string {
    return JSON.stringify({
        v: 4, adapter: "test-adapter", gen: "gen-1", req: "req-1",
        obs: opts.obs ?? "1", rev: opts.rev ?? "1", ready: true, consumed: false,
        source: "req-1", nativeInvalid: "1", clock: opts.clock ?? "1",
        gestureSettled: null, gestureAssoc: null,
        operationSettled: [], nativeCreations: [],
        warpCursor: null,
        intentWrites: [], switchSettled: [], diagnostics: [],
        presentation: DISABLED_PRESENTATION,
        scopes: [{ scope: { kind: "workspace", out: "out", ws: "ws1" }, auth: "controlled", reason: "" }],
        workspaces: [], outputs: [],
        windows: opts.windows ?? [winRow("win-b", "ws1")],
        transfers: [],
        effects: opts.effects,
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
            adjacent: {}, complete: true,
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

function acceptParsed(_state: WorldAdapterState, text: string): ApplyEnv["reply"] {
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
        isActiveWindow: (ref) => ref === targetRef && fg.current === "win-b",
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
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b")] }));
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
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b")] }));
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
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b")] }));
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
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b")] }));
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
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b")] }));
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
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b")] }));
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
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b")] }));
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

    it("records no focus rows for raise/stack, including absent targets", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({
            effects: [
                fxRow("fx-9", "raise", "win-b"),
                fxRow("fx-10", "stack", "win-missing"),
            ],
        }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        let calls = 0;
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            focusWindow: () => { calls += 1; return true; },
        });
        const result = applyWorldReply(env);
        assert.equal(calls, 0);
        assert.equal(result.effects, 0);
        assert.deepEqual(state.focusFeedbacksForReport(), []);
    });

    it("never replays: duplicates record once, ack retires, stamps never restamp", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b")], clock: "9" }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        const env = () => baseEnv(state, observation, reply, fg, targetRef);
        applyWorldReply(env());
        applyWorldReply(env());
        let rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1, "spent effect never records twice");
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
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b", true)] }));
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

    it("binds lifetime mismatch to false and keeps absence null, never false", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b", true)] }));
        assert.deepEqual(state.acceptReply(reply, "req-1", state.currentInvalid), { applied: true, reason: "accepted" });
        const targetRef = {};
        const observation = makeObs(targetRef);
        const fg: FgModel = { current: null };
        // Reentrant lifetime replacement under a stable wrapper: the ref
        // still lists, but its actual id names another lifetime.
        const env = baseEnv(state, observation, reply, fg, targetRef, {
            readWindowId: () => "win-other",
            readLiveVisibility: () => ({ minimized: false, hostHidden: false, visible: null }),
        });
        const result = applyWorldReply(env);
        assert.equal(result.effects, 0, "mismatched lifetime never actuates");
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "not-called");
        assert.equal(rows[0]?.reason, "identity-stale");
        for (const b of rows[0]?.arrival ?? []) {
            assert.equal(b.before.lifetimeMatched, false, "mismatch binds false, never unknown");
        }
    });

    it("leaves lifetime unknown and foreground unproven on an unreadable production id", () => {
        const state = setupState();
        const reply = acceptParsed(state, replyText({ effects: [fxRow("fx-1", "focus", "win-b", true)] }));
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
        assert.ok((rows[0]?.arrival.length ?? 0) > 0, "awaitShown grant still samples");
        for (const b of rows[0]?.arrival ?? []) {
            assert.equal(b.before.lifetimeMatched, null, "unreadable id rides unknown, never matched");
            assert.equal(b.after.lifetimeMatched, null, "unreadable id rides unknown, never matched");
        }
    });

    it("stamps bracket obs from the reply and the write stamp from the clock, diverged", () => {
        const state = setupState();
        // Broker obs, local fence, and dispatch clock deliberately
        // diverge: obs names the issuing observation, the fence the
        // current local watermark, the clock the broker epoch.
        const reply = acceptParsed(state, replyText({
            effects: [fxRow("fx-1", "focus", "win-b", true)],
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
        });
        const result = applyWorldReply(env);
        assert.equal(result.effects, 1);
        const rows = state.focusFeedbacksForReport();
        assert.equal(rows.length, 1);
        assert.ok((rows[0]?.arrival.length ?? 0) > 0, "awaitShown grant carries brackets");
        for (const b of rows[0]?.arrival ?? []) {
            assert.equal(b.obs, "100", "bracket obs is the reply obs, never the fence");
            assert.equal(b.before.obs, "100");
            assert.equal(b.after.obs, "100");
        }
        assert.equal(rows[0]?.attemptMs, "7", "write stamp is the broker clock, never wall-clock");
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
        assert.equal(host.isActiveWindow(winB), true);
        assert.equal(host.isActiveWindow(winA), false);
    });
});
