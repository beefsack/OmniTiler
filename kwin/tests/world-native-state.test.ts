// Native StateTarget executor tests (offline only, no D-Bus, no KWin).
//
// Uses the REAL accepted preimage client (`createNativePreimageStore`)
// over a fake bus plus the REAL `WorldAdapterState` evidence map and
// fake KWin windows with async-applied readback lag (setter accepted but
// the bool readback unchanged until later), throws/refusal, nullable
// facts, and roster duplicates. No production entry wiring here.
//
// Evidence rule under test: a StateAttempt exists ONLY for an actual
// native setter invocation (accepted, explicit refusal, threw). Paths
// with no invocation (unreadable preimage, unavailable storage, failed
// persist, stale fences, lost authority/liveness, external preimage
// race, missing native method, unowned above release) carry ZERO rows.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
    createNativePreimageStore,
    mapWorldStatePropertyToPreimage,
    NATIVE_PREIMAGE_VERSION,
    type NativePreimageTransport,
} from "../src/world-native-preimage";
import {
    createNativeStateExecutor,
    createNativeStateHost,
    type NativeStateFences,
    type NativeStateHost,
} from "../src/world-native-state";
import { createNativeRecoveryCoordinator } from "../src/world-native-recovery";
import { WorldAdapterState } from "../src/world-state";
import type {
    WorldReply,
    WorldStateProperty,
} from "../src/world-report";

// ---------------------------------------------------------------- fakes

interface FakeDesk {
    id: string;
}

interface FakeWin {
    internalId: string;
    maximizeMode: unknown;
    fullScreen: unknown;
    onAllDesktops: unknown;
    desktops: FakeDesk[] | unknown;
    keepAbove: unknown;
    keepBelow: unknown;
    // Applied-lag: setMaximize records requested but only flips the
    // readback when lag is off.
    requestedMax: { v: boolean; h: boolean } | null;
    requestedFs: boolean | null;
    requestedSticky: boolean | null;
    requestedAbove: boolean | null;
    requestedBelow: boolean | null;
    lagMax: boolean;
    lagFs: boolean;
    throwOn: Set<string>;
    refuseOn: Set<string>;
    setMaximize?: (...a: ReadonlyArray<unknown>) => void;
    calls: string[];
}

function makeWin(over: Partial<FakeWin> & { internalId: string }): FakeWin {
    const { internalId, ...rest } = over;
    const w: FakeWin = {
        internalId,
        maximizeMode: 0,
        fullScreen: false,
        onAllDesktops: false,
        desktops: [{ id: "desk-1" }],
        keepAbove: false,
        keepBelow: false,
        requestedMax: null,
        requestedFs: null,
        requestedSticky: null,
        requestedAbove: null,
        requestedBelow: null,
        lagMax: false,
        lagFs: false,
        throwOn: new Set(),
        refuseOn: new Set(),
        calls: [],
        ...rest,
    } as FakeWin;
    w.setMaximize = function (...args: ReadonlyArray<unknown>): void {
        w.calls.push("setMaximize");
        if (w.throwOn.has("setMaximize")) {
            throw new Error("native-throw");
        }
        if (w.refuseOn.has("setMaximize")) {
            return;
        }
        const v = args[0] as boolean;
        const h = args[1] as boolean;
        w.requestedMax = { v, h };
        if (!w.lagMax) {
            w.maximizeMode = v && h ? 3 : v ? 1 : h ? 2 : 0;
        }
    };
    return w;
}

function readProp(value: object, property: string): unknown {
    try {
        return Reflect.get(value, property);
    } catch (error) {
        void error;
        return undefined;
    }
}

// Reflect.set wrapper honoring throwOn/refuseOn plus applied lag for
// fullscreen/sticky/above/below.
function trackingSet(win: FakeWin, ref: object, prop: string, value: unknown): boolean {
    (win.calls as string[]).push(`set:${prop}`);
    if (win.throwOn.has(prop)) {
        throw new Error("native-throw");
    }
    if (win.refuseOn.has(prop)) {
        return false;
    }
    if (prop === "fullScreen") {
        win.requestedFs = value as boolean;
        if (!win.lagFs) {
            win.fullScreen = value;
        }
        return true;
    }
    if (prop === "onAllDesktops") {
        win.requestedSticky = value as boolean;
        win.onAllDesktops = value;
        if (value === true) {
            win.desktops = [{ id: "desk-1" }];
        }
        return true;
    }
    if (prop === "keepAbove") {
        win.requestedAbove = value as boolean;
        win.keepAbove = value;
        // KWin exclusivity: setting above clears below (pinned window.cpp).
        if (value === true) {
            win.keepBelow = false;
        }
        return true;
    }
    if (prop === "keepBelow") {
        win.requestedBelow = value as boolean;
        win.keepBelow = value;
        if (value === true) {
            win.keepAbove = false;
        }
        return true;
    }
    return Reflect.set(ref, prop, value);
}

interface BusCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
    answered: boolean;
}

function makeBus(gen: () => string | null) {
    const calls: BusCall[] = [];
    const logs: string[] = [];
    let stopped = false;
    let kicks = 0;
    const transport: NativePreimageTransport = {
        route: { service: "svc", object: "/obj", iface: "iface" },
        callDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback, answered: false });
        },
        scheduleOnce: (delay, cb) => {
            void delay;
            void cb;
            return () => undefined;
        },
        log: (m) => {
            logs.push(m);
        },
        isStopped: () => stopped,
        getGeneration: () => gen(),
        onHydrated: () => {
            kicks += 1;
        },
    };
    return {
        transport, calls, logs,
        setStopped: (v: boolean) => {
            stopped = v;
        },
        kicks: () => kicks,
    };
}

type StateTargetRow = {
    property: WorldStateProperty;
    value: boolean;
    auth: "controlled" | "untouched" | "hold";
    reason: "" | "minimized";
    cmdSeq: string | null;
};

function target(
    property: WorldStateProperty,
    value: boolean,
    over: Partial<StateTargetRow> = {},
): StateTargetRow {
    return {
        property, value,
        auth: "controlled", reason: "", cmdSeq: null,
        ...over,
    };
}

function replyFor(
    gen: string,
    rev: string,
    rows: Array<{ win: string; state: StateTargetRow[] }>,
): WorldReply {
    return {
        gen, rev,
        windows: rows.map((r) => ({ win: r.win, state: r.state })),
    } as unknown as WorldReply;
}

interface Harness {
    wins: FakeWin[];
    surface: object;
    host: NativeStateHost;
    state: WorldAdapterState;
    store: ReturnType<typeof createNativePreimageStore>;
    bus: ReturnType<typeof makeBus>;
    fences: NativeStateFences;
    auth: Map<string, boolean>;
    run: (reply: WorldReply) => void;
    writes: (mode: "ok" | "fail", onlyWin?: string) => void;
    exec: ReturnType<typeof createNativeStateExecutor>;    setterCalls: (winId: string) => string[];
    spentCount: () => number;
    setFence: (patch: Partial<NativeStateFences>) => void;
    setGen: (gen: string) => void;
    writeCount: () => number;
    coord: ReturnType<typeof createNativeRecoveryCoordinator> | null;
    logs: () => string[];
}

function setup(
    wins: FakeWin[],
    seed: "ok-empty" | { degraded: string } = "ok-empty",
    opts: { recoveryAuth?: boolean } = {},
): Harness {
    let gen: string | null = "gen-1";
    const bus = makeBus(() => gen);
    const store = createNativePreimageStore(bus.transport, true);
    const answerSeed = (mode: "ok-empty" | { degraded: string }): void => {
        const read = bus.calls.find((c) => c.method === "ReadNativePreimage" && !c.answered) as BusCall | undefined;
        assert.ok(read !== undefined);
        const corr = (JSON.parse(read.payload) as Record<string, unknown>)["correlation_id"] as string;
        read.answered = true;
        if (mode === "ok-empty") {
            read.callback(JSON.stringify({
                v: NATIVE_PREIMAGE_VERSION, correlation_id: corr,
                outcome: "ok", records: [], stored: 0, returned: 0,
            }));
        } else {
            read.callback(JSON.stringify({
                v: NATIVE_PREIMAGE_VERSION, correlation_id: corr,
                outcome: "degraded", records: [], stored: 0, returned: 0,
                reason: mode.degraded,
            }));
        }
    };
    store.seed("gen-1");
    answerSeed(seed);
    assert.equal(store.isAvailable("gen-1"), seed === "ok-empty");

    const surface: Record<string, unknown> = {
        windowList: () => wins.map((w) => w),
    };
    const logs: string[] = [];
    const baseHost = createNativeStateHost(surface, readProp, (m) => {
        logs.push(m);
    });
    // Wrap property setters with throw/refuse/lag tracking per window.
    const byId = new Map(wins.map((w) => [w.internalId, w]));
    const host: NativeStateHost = {
        ...baseHost,
        roster: () => wins.map((w) => ({ id: w.internalId, ref: w as object })),
        setFullscreen: (ref, value) => {
            const w = byId.get((ref as FakeWin).internalId);
            if (w === undefined) {
                return false;
            }
            if (readProp(ref, "fullScreen") === undefined) {
                return false;
            }
            return trackingSet(w, ref, "fullScreen", value);
        },
        setSticky: (ref, value) => {
            const w = byId.get((ref as FakeWin).internalId);
            if (w === undefined) {
                return false;
            }
            if (readProp(ref, "onAllDesktops") === undefined) {
                return false;
            }
            return trackingSet(w, ref, "onAllDesktops", value);
        },
        setKeepAbove: (ref, value) => {
            const w = byId.get((ref as FakeWin).internalId);
            if (w === undefined) {
                return false;
            }
            if (readProp(ref, "keepAbove") === undefined) {
                return false;
            }
            return trackingSet(w, ref, "keepAbove", value);
        },
        setKeepBelow: (ref, value) => {
            const w = byId.get((ref as FakeWin).internalId);
            if (w === undefined) {
                return false;
            }
            if (readProp(ref, "keepBelow") === undefined) {
                return false;
            }
            return trackingSet(w, ref, "keepBelow", value);
        },
        setMaximize: (ref, v, h) => {
            const w = byId.get((ref as FakeWin).internalId);
            if (w === undefined) {
                return false;
            }
            if (w.throwOn.has("setMaximize")) {
                throw new Error("native-throw");
            }
            if (w.refuseOn.has("setMaximize")) {
                return false;
            }
            const method = readProp(ref, "setMaximize");
            if (typeof method !== "function") {
                return false;
            }
            w.calls.push("setMaximize");
            w.requestedMax = { v, h };
            if (!w.lagMax) {
                w.maximizeMode = v && h ? 3 : v ? 1 : h ? 2 : 0;
            }
            return true;
        },
    };

    const state = new WorldAdapterState("kwin-world-adapter", "owner-1");
    state.recordClock("9");
    const fences: NativeStateFences = {
        generation: "gen-1", settingsRev: "s-1", acceptedRev: "5",
        watermark: "1", clock: "9", stopped: false,
    };
    const fencesBox = { cur: fences };
    const auth = new Map<string, boolean>();
    // Optional production-shaped authorization: the entry's
    // isStillAuthorized consults the real recovery gate then the real
    // store gate (world-entry.ts), so the executor's pre-arm check drops
    // `stale` here exactly when the seed is unusable. Off by default:
    // existing callers keep the auth-map-only predicate.
    const coordBox: { coord: ReturnType<typeof createNativeRecoveryCoordinator> | null } = { coord: null };
    if (opts.recoveryAuth === true) {
        coordBox.coord = createNativeRecoveryCoordinator({
            store,
            getFences: () => ({ ...fencesBox.cur }),
            log: (m) => {
                logs.push(m);
            },
            requestObservation: () => undefined,
        });
        coordBox.coord.seed("gen-1");
    }
    const exec = createNativeStateExecutor({
        state,
        store,
        host,
        getFences: () => ({ ...fencesBox.cur }),
        isStillAuthorized: (win, property) => {
            if (auth.get(`${win}\u0000${property}`) === false) {
                return false;
            }
            if (opts.recoveryAuth === true) {
                const coord = coordBox.coord;
                if (coord === null) {
                    return false;
                }
                try {
                    if (coord.isBlocked(win, mapWorldStatePropertyToPreimage(property), fencesBox.cur.generation)) {
                        return false;
                    }
                } catch (error) {
                    void error;
                    return false;
                }
                try {
                    if (!store.isAvailable(fencesBox.cur.generation)) {
                        return false;
                    }
                } catch (error) {
                    void error;
                    return false;
                }
            }
            return auth.get(`${win}\u0000${property}`) ?? true;
        },
    });
    const writes = (mode: "ok" | "fail", onlyWin?: string): void => {
        // The store serializes FIFO: answer the oldest unanswered write,
        // which may release the next one. Loop until none unanswered.
        for (let guard = 0; guard < 50; guard += 1) {
            const call = bus.calls.find((c) => c.method === "WriteNativePreimage" && !c.answered);
            if (call === undefined) {
                return;
            }
            call.answered = true;
            const corrId = (JSON.parse(call.payload) as Record<string, unknown>)["correlation_id"] as string;
            if (mode === "fail" && (onlyWin === undefined || call.payload.includes(onlyWin))) {
                call.callback(JSON.stringify({
                    v: NATIVE_PREIMAGE_VERSION, correlation_id: corrId,
                    outcome: "rejected", reason: "store-unavailable",
                }));
            } else {
                const sent = (JSON.parse(call.payload) as { records: unknown[] }).records.length;
                call.callback(JSON.stringify({
                    v: NATIVE_PREIMAGE_VERSION, correlation_id: corrId,
                    outcome: "stored", stored: sent,
                }));
            }
        }
        throw new Error("write drain did not settle");
    };
    return {
        wins, surface, host, state, store, bus, fences, auth, exec,
        setFence: (patch: Partial<NativeStateFences>) => {
            Object.assign(fencesBox.cur, patch);
        },
        setGen: (g: string) => {
            gen = g;
            store.seed(g);
            answerSeed("ok-empty");
            assert.equal(store.isAvailable(g), true);
            coordBox.coord?.seed(g);
        },
        run: (reply) => {
            Object.assign(fencesBox.cur, { acceptedRev: reply.rev });
            exec.run(reply);
        },
        writes,
        setterCalls: (winId) => {
            const w = wins.find((x) => x.internalId === winId) as FakeWin | undefined;
            return w === undefined ? [] : [...w.calls];
        },
        spentCount: () => exec.spentCountForTest(),
        writeCount: () => bus.calls.filter((c) => c.method === "WriteNativePreimage").length,
        coord: coordBox.coord,
        logs: () => [...logs],
    };
}

function attemptsOf(h: Harness, win: string, property: WorldStateProperty) {
    return h.state.statesForReport().filter((a) => a.win === win && a.property === property);
}

// ---------------------------------------------------------------- tests

describe("world native state executor (StateTarget mechanics)", () => {
    it("hold and untouched rows with misleading values take zero storage and zero native calls", () => {
        const h = setup([makeWin({ internalId: "win-a", fullScreen: true, maximizeMode: 3 })]);
        const before = h.writeCount();
        h.run(replyFor("gen-1", "5", [
            { win: "win-a", state: [target("fullscreen", false, { auth: "hold", reason: "minimized" })] },
            { win: "win-a", state: [target("maximized", false, { auth: "untouched", reason: "" })] },
        ]));
        h.writes("ok");
        assert.equal(h.writeCount(), before);
        assert.deepEqual(h.setterCalls("win-a"), []);
        assert.deepEqual(h.state.statesForReport(), []);
    });

    it("missing state rows write nothing", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [] }]));
        h.writes("ok");
        assert.deepEqual(h.setterCalls("win-a"), []);
        assert.deepEqual(h.state.statesForReport(), []);
    });

    it("maximizes through setMaximize axes for each preimage mode", () => {
        for (const mode of [0, 1, 2, 3]) {
            const h = setup([makeWin({ internalId: "win-a", maximizeMode: mode })]);
            h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("maximized", true)] }]));
            h.writes("ok");
            assert.ok(h.setterCalls("win-a").includes("setMaximize"), `mode=${mode}`);
            const rows = attemptsOf(h, "win-a", "maximized");
            assert.equal(rows.length, 1);
            assert.equal(rows[0]?.setter, "accepted");
            assert.equal(rows[0]?.rev, "5");
            assert.equal(rows[0]?.attemptMs, "9");
            assert.equal(rows[0]?.target, true);
            assert.equal(rows[0]?.pre, mode !== 0);
            // No fake `maximized` property write: only the Q_INVOKABLE ran.
            assert.ok(!h.setterCalls("win-a").some((c) => c === "set:maximized"));
            // Preimage slot kept the exact axes.
            assert.equal(h.store.recordFor("win-a")?.maximize_mode, mode);
        }
    });

    it("fullscreen uses the exact fullScreen flag and preserves maximize axes", () => {
        const h = setup([makeWin({ internalId: "win-a", fullScreen: false, maximizeMode: 1 })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.ok(h.setterCalls("win-a").includes("set:fullScreen"));
        const rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.pre, false);
        assert.equal(rows[0]?.post, true);
        // Ordinary fullscreen entries never unmaximize.
        assert.equal(h.wins[0]?.maximizeMode, 1);
    });

    it("sticky persists the full desktop set and writes onAllDesktops", () => {
        const h = setup([makeWin({
            internalId: "win-a",
            onAllDesktops: false,
            desktops: [{ id: "desk-1" }, { id: "desk-2" }],
        })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("sticky", true)] }]));
        h.writes("ok");
        assert.deepEqual(h.store.recordFor("win-a")?.desktops, ["desk-1", "desk-2"]);
        assert.ok(h.setterCalls("win-a").includes("set:onAllDesktops"));
        const rows = attemptsOf(h, "win-a", "sticky");
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.pre, false);
        assert.equal(rows[0]?.post, true);
    });

    it("sticky-all preimage rides [] and sticky-off writes false", () => {
        const h = setup([makeWin({ internalId: "win-a", onAllDesktops: true })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("sticky", false)] }]));
        h.writes("ok");
        assert.deepEqual(h.store.recordFor("win-a")?.desktops, []);
        assert.ok(h.setterCalls("win-a").includes("set:onAllDesktops"));
        assert.equal(attemptsOf(h, "win-a", "sticky")[0]?.pre, true);
    });

    it("above-true persists both stacking flags and writes keepAbove", () => {
        const h = setup([makeWin({
            internalId: "win-a", keepAbove: false, keepBelow: true,
        })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        // Preimage carries BOTH raw flags (mechanics: no core policy claim).
        assert.deepEqual(h.store.recordFor("win-a")?.stacking, { above: false, below: true });
        h.writes("ok");
        assert.ok(h.setterCalls("win-a").includes("set:keepAbove"));
        const rows = attemptsOf(h, "win-a", "above");
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.pre, false);
        assert.equal(rows[0]?.post, true);
    });

    it("above-false restores the owned saved pair, not an unconditional below policy", () => {
        const h = setup([makeWin({
            internalId: "win-a", keepAbove: true, keepBelow: false,
        })]);
        // First acquire above with a saved below=true pair.
        h.wins[0]!.keepAbove = false;
        h.wins[0]!.keepBelow = true;
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        assert.equal(h.wins[0]?.keepAbove, true);
        // Now release: the saved pair (false,true) must come back.
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("above", false)] }]));
        h.writes("ok");
        const calls = h.setterCalls("win-a");
        assert.ok(calls.includes("set:keepAbove"));
        assert.ok(calls.includes("set:keepBelow"));
        assert.equal(h.wins[0]?.keepAbove, false);
        assert.equal(h.wins[0]?.keepBelow, true);
    });

    it("setter accepted with lagging applied readback records post unchanged (success is not completion)", () => {
        const h = setup([makeWin({ internalId: "win-a", fullScreen: false, lagFs: true })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        const rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.pre, false);
        // Applied flag lagged: the bool is unchanged even though the call
        // was accepted. The core completes this from observation, not here.
        assert.equal(rows[0]?.post, false);
    });

    it("throws record threw and explicit refusal records refused", () => {
        const threw = setup([makeWin({ internalId: "win-a" })]);
        threw.wins[0]?.throwOn.add("fullScreen");
        threw.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        threw.writes("ok");
        assert.equal(attemptsOf(threw, "win-a", "fullscreen")[0]?.setter, "threw");

        const refused = setup([makeWin({ internalId: "win-a" })]);
        refused.wins[0]?.refuseOn.add("fullScreen");
        refused.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        refused.writes("ok");
        assert.equal(attemptsOf(refused, "win-a", "fullscreen")[0]?.setter, "refused");
    });

    it("definitely missing methods take zero calls and zero attempts", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        delete (h.wins[0] as Partial<FakeWin>).setMaximize;
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("maximized", true)] }]));
        h.writes("ok");
        // Zero native invocations: the missing Q_INVOKABLE fails closed
        // with no invented native-call fact and no no-retry marker.
        assert.deepEqual(h.setterCalls("win-a"), []);
        assert.deepEqual(attemptsOf(h, "win-a", "maximized"), []);
    });

    it("nullable preimages fail closed with zero attempts and no clobber", () => {
        const h = setup([makeWin({ internalId: "win-a", maximizeMode: "bad" })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("maximized", true)] }]));
        h.writes("ok");
        assert.deepEqual(h.setterCalls("win-a"), []);
        // No native invocation happened: zero StateAttempt, not a refusal.
        assert.deepEqual(attemptsOf(h, "win-a", "maximized"), []);
    });

    it("failed storage carries zero attempts; unrelated windows stay viable", () => {
        const h = setup([
            makeWin({ internalId: "win-a" }),
            makeWin({ internalId: "win-b" }),
        ]);
        h.run(replyFor("gen-1", "5", [
            { win: "win-a", state: [target("fullscreen", true)] },
            { win: "win-b", state: [target("fullscreen", true)] },
        ]));
        // Fail win-a's flush, succeed win-b's: per-window batching keeps
        // the failure narrow (FIFO order: win-a first).
        const first = h.bus.calls.find((c) => c.method === "WriteNativePreimage" && !c.answered) as BusCall;
        first.answered = true;
        const corr1 = (JSON.parse(first.payload) as Record<string, unknown>)["correlation_id"] as string;
        first.callback(JSON.stringify({
            v: NATIVE_PREIMAGE_VERSION, correlation_id: corr1,
            outcome: "rejected", reason: "store-unavailable",
        }));
        h.writes("ok");
        // Storage failure wrote nothing native: zero attempts for win-a.
        assert.deepEqual(h.setterCalls("win-a"), []);
        assert.deepEqual(attemptsOf(h, "win-a", "fullscreen"), []);
        assert.equal(attemptsOf(h, "win-b", "fullscreen")[0]?.setter, "accepted");
        assert.ok(h.setterCalls("win-b").includes("set:fullScreen"));
    });

    it("single-window storage failure writes nothing native and records nothing", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("fail");
        assert.deepEqual(h.setterCalls("win-a"), []);
        assert.deepEqual(h.state.statesForReport(), []);
    });

    it("original evidence survives republish: no retry, no restamp", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        const r1 = replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]);
        h.run(r1);
        h.writes("ok");
        const before = h.setterCalls("win-a").length;
        // Same authority republished under an advanced rev: not a fresh
        // command (same null cmdSeq, same target), so no second write and
        // the original stamp is retained, never replaced.
        const r2 = replyFor("gen-1", "6", [{ win: "win-a", state: [target("fullscreen", true)] }]);
        h.run(r2);
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").length, before);
        const rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "5");
        assert.equal(rows[0]?.attemptMs, "9");
    });

    it("fresh typed cmdSeq re-arms the same target", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        h.run(replyFor("gen-1", "5", [
            { win: "win-a", state: [target("fullscreen", true, { cmdSeq: "11" })] },
        ]));
        h.writes("ok");
        const before = h.setterCalls("win-a").length;
        assert.ok(before > 0);
        h.run(replyFor("gen-1", "6", [
            { win: "win-a", state: [target("fullscreen", true, { cmdSeq: "12" })] },
        ]));
        h.writes("ok");
        assert.ok(h.setterCalls("win-a").length > before);
    });

    it("stale fences after persist write nothing and record nothing", () => {
        const cases: Array<{ name: string; mutate: (h: Harness) => void }> = [
            { name: "generation", mutate: (h) => { h.setFence({ generation: "gen-2" }); } },
            { name: "settings", mutate: (h) => { h.setFence({ settingsRev: "s-2" }); } },
            {
                name: "watermark", mutate: (h) => { h.setFence({ watermark: "2" }); },
            },
            {
                name: "authority", mutate: (h) => { h.auth.set("win-a\u0000above", false); },
            },
        ];
        for (const c of cases) {
            const h = setup([makeWin({ internalId: "win-a" })]);
            h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
            c.mutate(h);
            h.writes("ok");
            assert.deepEqual(h.setterCalls("win-a"), [], c.name);
            assert.deepEqual(attemptsOf(h, "win-a", "above"), [], c.name);
        }
        // Liveness loss after persist: the window leaves the roster.
        {
            const h = setup([makeWin({ internalId: "win-a" })]);
            h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
            h.wins.length = 0;
            h.writes("ok");
            assert.deepEqual(h.setterCalls("win-a"), []);
            assert.deepEqual(attemptsOf(h, "win-a", "above"), []);
        }
        // Accepted-rev supersession after persist.
        {
            const h = setup([makeWin({ internalId: "win-a" })]);
            h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
            h.setFence({ acceptedRev: "6" });
            h.writes("ok");
            assert.deepEqual(h.setterCalls("win-a"), []);
            assert.deepEqual(attemptsOf(h, "win-a", "above"), []);
        }
    });

    it("external native change while storage is pending never clobbers and records nothing", () => {
        const h = setup([makeWin({
            internalId: "win-a", keepAbove: false, keepBelow: false,
        })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        // External input flips stacking before the stored ack lands.
        h.wins[0]!.keepAbove = true;
        h.writes("ok");
        assert.deepEqual(h.setterCalls("win-a"), []);
        assert.deepEqual(attemptsOf(h, "win-a", "above"), []);
    });

    it("paired fullscreen-clear plus maximize persists both, orders clear first, and ignores applied lag", () => {
        const h = setup([makeWin({
            internalId: "win-a", fullScreen: true, maximizeMode: 0, lagFs: true,
        })]);
        h.run(replyFor("gen-1", "5", [{
            win: "win-a",
            state: [
                target("fullscreen", false, { cmdSeq: "21" }),
                target("maximized", true, { cmdSeq: "21" }),
            ],
        }]));
        // Both preimages persist before either setter runs.
        assert.equal(h.store.recordFor("win-a")?.fullscreen, true);
        assert.equal(h.store.recordFor("win-a")?.maximize_mode, 0);
        h.writes("ok");
        const calls = h.setterCalls("win-a");
        const fsAt = calls.indexOf("set:fullScreen");
        const mxAt = calls.indexOf("setMaximize");
        assert.ok(fsAt >= 0 && mxAt >= 0 && fsAt < mxAt, `order: ${calls.join(",")}`);
        // The applied fullscreen flag still reads true (Wayland lag) but
        // the back-to-back maximize ran anyway: no applied gate between.
        assert.equal(h.wins[0]?.fullScreen, true);
        assert.equal(h.wins[0]?.maximizeMode, 3);
        assert.equal(attemptsOf(h, "win-a", "fullscreen").length, 1);
        assert.equal(attemptsOf(h, "win-a", "maximized").length, 1);
        assert.equal(attemptsOf(h, "win-a", "maximized")[0]?.setter, "accepted");
    });

    it("paired maximize never runs when the fullscreen clear definitely refuses", () => {
        const h = setup([makeWin({ internalId: "win-a", fullScreen: true, maximizeMode: 0 })]);
        h.wins[0]?.refuseOn.add("fullScreen");
        h.run(replyFor("gen-1", "5", [{
            win: "win-a",
            state: [
                target("fullscreen", false, { cmdSeq: "31" }),
                target("maximized", true, { cmdSeq: "31" }),
            ],
        }]));
        h.writes("ok");
        const calls = h.setterCalls("win-a");
        assert.ok(calls.includes("set:fullScreen"));
        assert.ok(!calls.includes("setMaximize"), `unexpected maximize: ${calls.join(",")}`);
        assert.equal(attemptsOf(h, "win-a", "fullscreen")[0]?.setter, "refused");
        // The skipped maximize was never attempted: not-called, no row.
        assert.deepEqual(attemptsOf(h, "win-a", "maximized"), []);
    });

    it("duplicate roster ids fail closed with zero attempts", () => {
        const h = setup([
            makeWin({ internalId: "win-a" }),
            makeWin({ internalId: "win-a" }),
        ]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        for (const w of h.wins) {
            assert.deepEqual(w.calls.filter((c) => c.startsWith("set:")), []);
        }
        assert.deepEqual(attemptsOf(h, "win-a", "fullscreen"), []);
    });

    it("write payload carries the exact preimage slots", () => {
        const h = setup([makeWin({
            internalId: "win-a", maximizeMode: 2, fullScreen: true,
            onAllDesktops: false, desktops: [{ id: "desk-9" }],
            keepAbove: true, keepBelow: false,
        })]);
        // Establish owned above first: the above-false lane below needs an
        // owned pair and must not arm the current pre-release values.
        h.wins[0]!.keepAbove = false;
        h.wins[0]!.keepBelow = true;
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        assert.equal(attemptsOf(h, "win-a", "above")[0]?.setter, "accepted");
        h.run(replyFor("gen-1", "6", [{
            win: "win-a",
            state: [
                target("maximized", false),
                target("above", false),
            ],
        }]));
        const call = h.bus.calls.find((c) => c.method === "WriteNativePreimage" && !c.answered) as BusCall;
        assert.ok(call !== undefined);
        const body = JSON.parse(call.payload) as { records: Array<Record<string, unknown>> };
        assert.equal(body.records.length, 1);
        const row = body.records[0] as Record<string, unknown>;
        assert.equal(row["maximize_mode"], 2);
        // First-wins arm: the retained stacking is the pre-acquisition
        // pair {false,true} from the owned above-true, not the live slot.
        assert.deepEqual(row["stacking"], { above: false, below: true });
        h.writes("ok");
        assert.equal(attemptsOf(h, "win-a", "maximized")[0]?.setter, "accepted");
        assert.equal(attemptsOf(h, "win-a", "above")[0]?.setter, "accepted");
    });

    it("commandless above acquire-release-resume runs three calls; republish adds none", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("above", false)] }]));
        h.writes("ok");
        const beforeResume = h.setterCalls("win-a").length;
        h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        // Three real native calls: acquire, owned release, resume.
        assert.equal(h.setterCalls("win-a").length - beforeResume, 1);
        assert.equal(h.setterCalls("win-a").filter((c) => c === "set:keepAbove").length, 3);
        // Same-identity republish of the resumed true: no fourth call, and
        // the original rev-7 row is retained, never overwritten.
        h.run(replyFor("gen-1", "8", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").filter((c) => c === "set:keepAbove").length, 3);
        const rows = attemptsOf(h, "win-a", "above");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "7");
        assert.equal(rows[0]?.setter, "accepted");
    });

    it("lagged fullscreen acquire then maximize then late apply lets a fresh release succeed", () => {
        const h = setup([makeWin({ internalId: "win-a", fullScreen: false, lagFs: true })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.equal(attemptsOf(h, "win-a", "fullscreen")[0]?.setter, "accepted");
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("maximized", true)] }]));
        h.writes("ok");
        assert.equal(attemptsOf(h, "win-a", "maximized")[0]?.setter, "accepted");
        // KWin applies the requested fullscreen late.
        h.wins[0]!.fullScreen = true;
        h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [target("fullscreen", false)] }]));
        h.writes("ok");
        const rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.rev, "7");
        assert.equal(rows[0]?.pre, true);
        assert.ok(h.setterCalls("win-a").filter((c) => c === "set:fullScreen").length >= 2);
    });

    it("rev5 pending plus same-identity rev6 repub writes once under rev6 with no rev5 row", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        // Republish before either ack lands: duplicate pending, no second
        // persist, relabeled to the newest authorizing rev.
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        assert.equal(h.writeCount(), 1);
        h.writes("ok");
        assert.deepEqual(h.setterCalls("win-a"), ["set:fullScreen"]);
        const rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "6");
        assert.equal(rows[0]?.setter, "accepted");
        // Redelivery afterwards: no second call, no overwrite.
        h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.deepEqual(h.setterCalls("win-a"), ["set:fullScreen"]);
        assert.equal(attemptsOf(h, "win-a", "fullscreen")[0]?.rev, "6");
    });

    it("many typed commands on one live property keep a single marker", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        for (const [i, seq] of ["11", "12", "13", "14", "15"].entries()) {
            h.run(replyFor("gen-1", String(5 + i), [
                { win: "win-a", state: [target("fullscreen", true, { cmdSeq: seq })] },
            ]));
            h.writes("ok");
        }
        assert.equal(h.setterCalls("win-a").length, 5);
        // Latest-attempt markers replace in place: bounded to one entry.
        assert.equal(h.spentCount(), 1);
        assert.equal(attemptsOf(h, "win-a", "fullscreen").length, 1);
    });

    it("generation rotation drops the old ack silently and unblocks the new generation", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        // Rotate before the old ack lands.
        h.setGen("gen-2");
        h.setFence({ generation: "gen-2" });
        h.writes("ok");
        assert.deepEqual(h.setterCalls("win-a"), []);
        assert.deepEqual(h.state.statesForReport(), []);
        // The new generation runs cleanly.
        h.run(replyFor("gen-2", "8", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.deepEqual(h.setterCalls("win-a"), ["set:fullScreen"]);
        const rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "8");
        assert.equal(rows[0]?.setter, "accepted");
    });

    it("above-false without an owned pair writes nothing and records nothing", () => {
        const h = setup([makeWin({ internalId: "win-a", keepAbove: true, keepBelow: false })]);
        const before = h.writeCount();
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", false)] }]));
        h.writes("ok");
        assert.equal(h.wins[0]?.keepAbove, true);
        assert.deepEqual(h.setterCalls("win-a"), []);
        assert.deepEqual(attemptsOf(h, "win-a", "above"), []);
        assert.equal(h.writeCount(), before);
    });

    it("reentrant authority loss skips only the affected lane; liveness loss isolates too", () => {
        // A reentrant fullscreen setter revokes the sticky authority: the
        // fullscreen write stands, the sticky lane drops with no row.
        {
            const h = setup([makeWin({ internalId: "win-a" })]);
            const hostAny = h.host as unknown as { setFullscreen: NativeStateHost["setFullscreen"] };
            const prev = hostAny.setFullscreen;
            hostAny.setFullscreen = (ref, value) => {
                h.auth.set("win-a\u0000sticky", false);
                return prev(ref, value);
            };
            h.run(replyFor("gen-1", "5", [{
                win: "win-a",
                state: [target("fullscreen", true), target("sticky", true)],
            }]));
            h.writes("ok");
            assert.equal(attemptsOf(h, "win-a", "fullscreen")[0]?.setter, "accepted");
            assert.deepEqual(attemptsOf(h, "win-a", "sticky"), []);
            assert.ok(h.setterCalls("win-a").includes("set:fullScreen"));
            assert.ok(!h.setterCalls("win-a").includes("set:onAllDesktops"));
        }
        // A reentrant setter that ends the window's lifetime blocks the
        // next independent lane while keeping the first write's evidence.
        {
            const h = setup([makeWin({ internalId: "win-a" })]);
            const hostAny = h.host as unknown as { setFullscreen: NativeStateHost["setFullscreen"] };
            const prev = hostAny.setFullscreen;
            hostAny.setFullscreen = (ref, value) => {
                const out = prev(ref, value);
                h.wins.length = 0;
                return out;
            };
            h.run(replyFor("gen-1", "5", [{
                win: "win-a",
                state: [target("fullscreen", true), target("sticky", true)],
            }]));
            h.writes("ok");
            assert.equal(attemptsOf(h, "win-a", "fullscreen")[0]?.setter, "accepted");
            assert.deepEqual(attemptsOf(h, "win-a", "sticky"), []);
        }
    });
});

// ---------------------------------------------------------------- clearing
// Spent-marker clearing: one record per (win, property) binding identity
// (target+cmdSeq); a different identity replaces, a same-identity republish
// never rewrites. A present Hold/Untouched row protects its property; a
// listed window row lacking the property at any authority clears it (even
// an empty state row). Whole-window absence never clears. A refused/threw
// acquire spends the identity but installs no owned above pair, so a later
// release is a silent unowned release. An executed call records its frozen
// rev/target/cmdSeq/clock evidence; superseding never rewrites it.

describe("spent-marker clearing", () => {
    it("held row beside a controlled lane preserves the marker", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").filter((c) => c === "set:fullScreen").length, 1);
        // Held row present alongside a controlled lane: the marker survives.
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [
            target("fullscreen", true, { auth: "hold", reason: "minimized" }),
            target("above", true),
        ] }]));
        h.writes("ok");
        // Same-identity republish: no rewrite, rev-5 evidence retained.
        h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").filter((c) => c === "set:fullScreen").length, 1);
        const rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "5");
        assert.equal(rows[0]?.setter, "accepted");
    });

    it("untouched row beside a controlled lane preserves the marker", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").filter((c) => c === "set:fullScreen").length, 1);
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [
            target("fullscreen", true, { auth: "untouched", reason: "" }),
            target("above", true),
        ] }]));
        h.writes("ok");
        h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").filter((c) => c === "set:fullScreen").length, 1);
        const rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "5");
    });

    it("reentrant same-identity run keeps the acted-on rev", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        const hostAny = h.host as unknown as { setFullscreen: NativeStateHost["setFullscreen"] };
        const prev = hostAny.setFullscreen;
        let fired = false;
        hostAny.setFullscreen = (ref, value) => {
            if (!fired) {
                fired = true;
                // Same identity mid-setter: the executing lane keeps rev 5.
                h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("fullscreen", true)] }]));
            }
            return prev(ref, value);
        };
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").filter((c) => c === "set:fullScreen").length, 1);
        const rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "5");
        assert.equal(rows[0]?.setter, "accepted");
    });

    it("reentrant changed-identity run keeps frozen evidence and the newer marker", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        const hostAny = h.host as unknown as { setFullscreen: NativeStateHost["setFullscreen"] };
        const prev = hostAny.setFullscreen;
        let fired = false;
        hostAny.setFullscreen = (ref, value) => {
            // True side effect first, then the newer identity runs and its
            // ack is driven reentrantly: the nested false actually invokes
            // and spends before the outer returns.
            const out = prev(ref, value);
            if (!fired) {
                fired = true;
                h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("fullscreen", false)] }]));
                h.writes("ok");
            }
            return out;
        };
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        const fsCalls = () => h.setterCalls("win-a").filter((c) => c === "set:fullScreen").length;
        // Both identities really invoked: outer true, then nested false.
        assert.equal(fsCalls(), 2);
        // Frozen evidence for the executed outer call: retention is
        // completion-ordered, so the last-completing outer rev-5 row wins.
        let rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "5");
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.target, true);
        // Newer identity is spent: republishing false rewrites nothing.
        h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [target("fullscreen", false)] }]));
        h.writes("ok");
        assert.equal(fsCalls(), 2);
        rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "5");
        // Older identity differs from the spent marker: it re-arms and
        // records under its own rev.
        h.run(replyFor("gen-1", "8", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.equal(fsCalls(), 3);
        rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "8");
        assert.equal(rows[0]?.target, true);
    });

    it("refused above acquire leaves the release silent", () => {
        const h = setup([makeWin({ internalId: "win-a", keepAbove: false, keepBelow: false })]);
        h.wins[0]?.refuseOn.add("keepAbove");
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        const acq = attemptsOf(h, "win-a", "above");
        assert.equal(acq.length, 1);
        assert.equal(acq[0]?.setter, "refused");
        assert.equal(acq[0]?.rev, "5");
        h.wins[0]?.refuseOn.delete("keepAbove");
        const before = h.setterCalls("win-a").length;
        // No owned pair after refusal: the release is silent.
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("above", false)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").length, before);
        const rows = attemptsOf(h, "win-a", "above");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "refused");
        assert.equal(rows[0]?.rev, "5");
        assert.equal(rows[0]?.target, true);
        // Same-identity re-add never retries.
        h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").length, before);
    });

    it("threw above acquire leaves the release silent", () => {
        const h = setup([makeWin({ internalId: "win-a", keepAbove: false, keepBelow: false })]);
        h.wins[0]?.throwOn.add("keepAbove");
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        const acq = attemptsOf(h, "win-a", "above");
        assert.equal(acq.length, 1);
        assert.equal(acq[0]?.setter, "threw");
        assert.equal(acq[0]?.rev, "5");
        h.wins[0]?.throwOn.delete("keepAbove");
        const before = h.setterCalls("win-a").length;
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("above", false)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").length, before);
        const rows = attemptsOf(h, "win-a", "above");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "threw");
        assert.equal(rows[0]?.rev, "5");
        h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").length, before);
    });

    it("empty state row for a listed window clears the marker", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").filter((c) => c === "set:fullScreen").length, 1);
        // Accepted reply row with an empty state: the property is lacking
        // at any authority, so the marker clears even with no Hold row.
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [] }]));
        h.writes("ok");
        h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").filter((c) => c === "set:fullScreen").length, 2);
        const rows = attemptsOf(h, "win-a", "fullscreen");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "7");
        assert.equal(rows[0]?.setter, "accepted");
    });

    it("absent-property gap with Hold return writes nothing", () => {
        // Core can withhold a retained binding's row while keeping the
        // binding: above latch skips the row (world_above.rs), and
        // capability/route early returns skip all rows (world_above.rs,
        // world_fullscreen.rs, world_sticky.rs, world_maximize.rs).
        // A gap with lost evidence leaves core unspent (spend lands on
        // the first correlated attempt); the gap stays harmless because
        // the latch is terminal in place and caps/routes are
        // generation-static, so the withheld identity cannot be
        // reauthorized as a fresh Controlled command.
        const h = setup([makeWin({ internalId: "win-a", keepAbove: false, keepBelow: false })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        assert.equal(attemptsOf(h, "win-a", "above")[0]?.setter, "accepted");
        const aboveCalls = () => h.setterCalls("win-a").filter(
            (c) => c === "set:keepAbove" || c === "set:keepBelow",
        ).length;
        assert.equal(aboveCalls(), 1);
        // Gap: window row present with another property only (above
        // lacking, as if latched or capability-withheld).
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        // Hold return of the same above identity: never a native call,
        // never a marker restore, original rev-5 evidence retained.
        h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [
            target("above", true, { auth: "hold", reason: "minimized" }),
        ] }]));
        h.writes("ok");
        assert.equal(aboveCalls(), 1);
        const rows = attemptsOf(h, "win-a", "above");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "5");
        assert.equal(rows[0]?.setter, "accepted");
    });
});

// ---------------------------------------------------------------- mid-call fences
// Two local defects: the above-false release ran two native subcalls with
// no revalidation between them, and post-setter evidence/ownership crossed
// reset/generation/stop/prune. The second subcall revalidates fences,
// authority, and the live ref; completion is fenced by generation/reset/stop
// (prune gates ownership only, same-generation evidence stands).

describe("mid-call completion fences", () => {
    it("above release second subcall is blocked by mid-first-call loss, keeping the first raw fact", () => {
        const cases: Array<{ name: string; mutate: (h: Harness) => void }> = [
            { name: "settings", mutate: (h) => { h.setFence({ settingsRev: "s-2" }); } },
            { name: "authority", mutate: (h) => { h.auth.set("win-a\u0000above", false); } },
            { name: "liveness", mutate: (h) => { h.wins.length = 0; } },
        ];
        for (const c of cases) {
            const h = setup([makeWin({ internalId: "win-a", keepAbove: false, keepBelow: true })]);
            const w = h.wins[0] as unknown as { calls: string[] };
            h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
            h.writes("ok");
            assert.equal(attemptsOf(h, "win-a", "above")[0]?.setter, "accepted");
            // Release of the saved (false,true) pair needs two subcalls.
            const hostAny = h.host as unknown as { setKeepAbove: NativeStateHost["setKeepAbove"] };
            const prev = hostAny.setKeepAbove;
            let once = false;
            hostAny.setKeepAbove = (ref, value) => {
                if (!once && value === false) {
                    once = true;
                    c.mutate(h);
                }
                return prev(ref, value);
            };
            h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("above", false)] }]));
            h.writes("ok");
            // NB: read calls off the detached object: the liveness case
            // empties h.wins, so the harness id lookup cannot find it.
            const calls = [...w.calls];
            assert.ok(calls.includes("set:keepAbove"), c.name);
            assert.ok(!calls.includes("set:keepBelow"), `${c.name}: second subcall must not run`);
            const rows = attemptsOf(h, "win-a", "above");
            assert.equal(rows.length, 1, c.name);
            assert.equal(rows[0]?.setter, "accepted", c.name);
            assert.equal(rows[0]?.rev, "6", c.name);
            assert.equal(rows[0]?.target, false, c.name);
        }
    });

    it("reset mid-setter injects no old-generation evidence or ownership", () => {
        for (const gen of ["gen-2", "gen-1"]) {
            const h = setup([makeWin({ internalId: "win-a", keepAbove: false, keepBelow: false })]);
            const hostAny = h.host as unknown as { setKeepAbove: NativeStateHost["setKeepAbove"] };
            const prev = hostAny.setKeepAbove;
            let once = false;
            hostAny.setKeepAbove = (ref, value) => {
                if (!once) {
                    once = true;
                    h.exec.reset(gen);
                }
                return prev(ref, value);
            };
            h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
            h.writes("ok");
            assert.deepEqual(attemptsOf(h, "win-a", "above"), [], gen);
            const before = h.setterCalls("win-a").length;
            // No owned pair leaked: a later release is silent.
            h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("above", false)] }]));
            h.writes("ok");
            assert.equal(h.setterCalls("win-a").length, before, gen);
            assert.deepEqual(attemptsOf(h, "win-a", "above"), [], gen);
        }
    });

    it("stop mid-setter records nothing", () => {
        const h = setup([makeWin({ internalId: "win-a" })]);
        const hostAny = h.host as unknown as { setFullscreen: NativeStateHost["setFullscreen"] };
        const prev = hostAny.setFullscreen;
        let once = false;
        hostAny.setFullscreen = (ref, value) => {
            if (!once) {
                once = true;
                h.exec.stop();
            }
            return prev(ref, value);
        };
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        h.writes("ok");
        assert.deepEqual(attemptsOf(h, "win-a", "fullscreen"), []);
        assert.deepEqual(h.state.statesForReport(), []);
    });

    it("prune mid-acquire preserves same-generation evidence but installs no ownership", () => {
        const h = setup([makeWin({ internalId: "win-a", keepAbove: false, keepBelow: false })]);
        const hostAny = h.host as unknown as { setKeepAbove: NativeStateHost["setKeepAbove"] };
        const prev = hostAny.setKeepAbove;
        let once = false;
        hostAny.setKeepAbove = (ref, value) => {
            if (!once) {
                once = true;
                h.exec.prune(new Set());
            }
            return prev(ref, value);
        };
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        const rows = attemptsOf(h, "win-a", "above");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.rev, "5");
        // No ownership installed: a later release is silent.
        const before = h.setterCalls("win-a").length;
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("above", false)] }]));
        h.writes("ok");
        assert.equal(h.setterCalls("win-a").length, before);
        assert.equal(attemptsOf(h, "win-a", "above").length, 1);
    });

    it("no-op prune mid-acquire retains ownership so the release restores", () => {
        const h = setup([makeWin({ internalId: "win-a", keepAbove: false, keepBelow: true })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        assert.equal(attemptsOf(h, "win-a", "above")[0]?.setter, "accepted");
        // No-op prune: the live set still contains the in-flight window.
        const hostAny = h.host as unknown as { setKeepAbove: NativeStateHost["setKeepAbove"] };
        const prev = hostAny.setKeepAbove;
        let once = false;
        hostAny.setKeepAbove = (ref, value) => {
            if (!once && value === false) {
                once = true;
                h.exec.prune(new Set(["win-a"]));
            }
            return prev(ref, value);
        };
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("above", false)] }]));
        h.writes("ok");
        // Ownership retained: the release ran both subcalls and restored.
        assert.ok(h.setterCalls("win-a").includes("set:keepBelow"));
        const rows = attemptsOf(h, "win-a", "above");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.rev, "6");
        assert.equal(rows[0]?.target, false);
        assert.equal(h.wins[0]?.keepAbove, false);
        assert.equal(h.wins[0]?.keepBelow, true);
    });

    it("unrelated-window prune mid-acquire retains ownership so the release restores", () => {
        const h = setup([
            makeWin({ internalId: "win-a", keepAbove: false, keepBelow: true }),
            makeWin({ internalId: "win-b" }),
        ]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        assert.equal(attemptsOf(h, "win-a", "above")[0]?.setter, "accepted");
        // Prune drops win-b but keeps the in-flight win-a alive.
        const hostAny = h.host as unknown as { setKeepAbove: NativeStateHost["setKeepAbove"] };
        const prev = hostAny.setKeepAbove;
        let once = false;
        hostAny.setKeepAbove = (ref, value) => {
            if (!once && value === false) {
                once = true;
                h.exec.prune(new Set(["win-a"]));
            }
            return prev(ref, value);
        };
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("above", false)] }]));
        h.writes("ok");
        assert.ok(h.setterCalls("win-a").includes("set:keepBelow"));
        const rows = attemptsOf(h, "win-a", "above");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.setter, "accepted");
        assert.equal(rows[0]?.rev, "6");
        assert.equal(h.wins[0]?.keepAbove, false);
        assert.equal(h.wins[0]?.keepBelow, true);
    });

    it("accepted release completion never deletes a newer reentrant acquire ownership", () => {
        const h = setup([makeWin({ internalId: "win-a", keepAbove: false, keepBelow: true })]);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("above", true)] }]));
        h.writes("ok");
        assert.equal(attemptsOf(h, "win-a", "above")[0]?.setter, "accepted");
        // Release rev6, but a nested acquire rev7 completes inside the first
        // subcall: the outer must not clobber it (no stale second subcall,
        // no ownership delete).
        const hostAny = h.host as unknown as { setKeepAbove: NativeStateHost["setKeepAbove"] };
        const prev = hostAny.setKeepAbove;
        let once = false;
        hostAny.setKeepAbove = (ref, value) => {
            if (!once && value === false) {
                once = true;
                h.run(replyFor("gen-1", "7", [{ win: "win-a", state: [target("above", true)] }]));
                h.writes("ok");
            }
            return prev(ref, value);
        };
        h.run(replyFor("gen-1", "6", [{ win: "win-a", state: [target("above", false)] }]));
        h.writes("ok");
        const aboveCalls = () => h.setterCalls("win-a").filter((c) => c === "set:keepAbove").length;
        // Outer first subcall, nested acquire, and nothing else above-wise.
        assert.equal(aboveCalls(), 3);
        assert.ok(!h.setterCalls("win-a").includes("set:keepBelow"));
        // Last-completing outer evidence wins; newer acquire stays spent.
        const rows = attemptsOf(h, "win-a", "above");
        assert.equal(rows.length, 1);
        assert.equal(rows[0]?.rev, "6");
        assert.equal(rows[0]?.target, false);
        // Newer ownership survived: a fresh release actually writes.
        h.run(replyFor("gen-1", "8", [{ win: "win-a", state: [target("above", false)] }]));
        h.writes("ok");
        assert.equal(aboveCalls(), 4);
        const done = attemptsOf(h, "win-a", "above");
        assert.equal(done.length, 1);
        assert.equal(done[0]?.rev, "8");
        assert.equal(done[0]?.target, false);
    });

    it("stale-namespace usable-empty seed arms executor targets; old and corrupt envelopes stay blocked", () => {
        // Split-boundary note: these envelopes are asserted
        // field-equivalent to the Rust service replies, not byte-identical;
        // correlation ids and key order differ by design. The executor
        // authorization here consults the real recovery gate then the real
        // store gate, mirroring production entry wiring.
        const droppedStale = (h: Harness): string[] => h.logs().filter((l) =>
            l.includes("native-state-dropped") && l.includes("target=fullscreen") && l.includes("reason=stale"));
        const droppedAny = (h: Harness): string[] => h.logs().filter((l) =>
            l.includes("native-state-dropped") && l.includes("target=fullscreen"));
        // Control 1: the old service envelope for a bus-id or KWin-owner
        // mismatch. The composed authorization drops the target stale
        // pre-arm: zero persist, zero native calls, zero attempts.
        const oldNs = setup([makeWin({ internalId: "win-a", fullScreen: false })],
            { degraded: "namespace-mismatch" }, { recoveryAuth: true });
        assert.equal(oldNs.store.isAvailable("gen-1"), false);
        assert.equal(oldNs.coord?.isBlocked("win-a", "fullscreen", "gen-1"), true);
        oldNs.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        assert.equal(droppedStale(oldNs).length, 1);
        assert.equal(oldNs.writeCount(), 0);
        assert.deepEqual(oldNs.setterCalls("win-a"), []);
        assert.deepEqual(attemptsOf(oldNs, "win-a", "fullscreen"), []);
        assert.equal(oldNs.spentCount(), 0);
        oldNs.writes("ok");
        assert.equal(oldNs.writeCount(), 0);
        assert.deepEqual(oldNs.setterCalls("win-a"), []);
        // Control 2: corrupt stays blocking through the same composed path.
        const c = setup([makeWin({ internalId: "win-a", fullScreen: false })],
            { degraded: "corrupt" }, { recoveryAuth: true });
        assert.equal(c.store.isAvailable("gen-1"), false);
        assert.equal(c.coord?.isBlocked("win-a", "fullscreen", "gen-1"), true);
        c.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        assert.equal(droppedStale(c).length, 1);
        assert.equal(c.writeCount(), 0);
        assert.deepEqual(c.setterCalls("win-a"), []);
        assert.deepEqual(attemptsOf(c, "win-a", "fullscreen"), []);
        assert.equal(c.spentCount(), 0);
        c.writes("ok");
        assert.equal(c.writeCount(), 0);
        assert.deepEqual(c.setterCalls("win-a"), []);
        // Fixed envelope for a bus-id or KWin-owner mismatch: `ok` empty
        // with zero counts. Recovery unblocks, the target arms, persists
        // before any setter, then runs exactly once on the stored ack.
        const h = setup([makeWin({ internalId: "win-a", fullScreen: false })],
            "ok-empty", { recoveryAuth: true });
        assert.equal(h.store.isAvailable("gen-1"), true);
        assert.equal(h.coord?.pendingCountForTest(), 0);
        assert.equal(h.coord?.isBlocked("win-a", "fullscreen", "gen-1"), false);
        h.run(replyFor("gen-1", "5", [{ win: "win-a", state: [target("fullscreen", true)] }]));
        // Persist-before-write: the snapshot persists and the write
        // dispatches before any native setter runs; nothing drops stale.
        assert.deepEqual(droppedAny(h), []);
        assert.equal(h.writeCount(), 1);
        assert.equal(h.store.recordFor("win-a")?.fullscreen, false);
        assert.deepEqual(h.setterCalls("win-a"), []);
        assert.deepEqual(attemptsOf(h, "win-a", "fullscreen"), []);
        assert.equal(h.spentCount(), 0);
        h.writes("ok");
        // Exactly one actual setter on the exact stored ack.
        assert.deepEqual(droppedAny(h), []);
        assert.deepEqual(h.setterCalls("win-a"), ["set:fullScreen"]);
        assert.equal(attemptsOf(h, "win-a", "fullscreen").length, 1);
        assert.equal(attemptsOf(h, "win-a", "fullscreen")[0]?.setter, "accepted");
        assert.equal(h.spentCount(), 1);
    });
});
