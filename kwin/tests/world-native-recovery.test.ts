// Startup native recovery coordinator tests (offline only, no D-Bus, no KWin).
//
// Uses the REAL accepted preimage client (`createNativePreimageStore`) over
// a fake bus plus fake KWin windows and desktops. Covers: touched slots
// only (null untouched means no write); exact axes, exact desktop
// memberships via resolved live refs, and coupled stacking; missing/
// duplicate/null desktop refs with zero setter calls; UUID reject/dupe/
// miss; seed failure; no pre-restoration flush; completion only on a
// strictly-later matching observation plus a current stored ack; no retry
// on refusal/throw; per-call cancellation on stale gen/settings/acceptedRev/
// watermark/clock/lifetime; reentrant notes during the setter never clear;
// stale acks never unblock; prune persists ended-lifetime removal;
// runtime arms never mistaken for leftovers; string-only public ids.

import { describe, it } from "node:test";
import assert from "node:assert/strict";

import {
    createNativePreimageStore,
    NATIVE_PREIMAGE_VERSION,
    type NativePreimageTransport,
} from "../src/world-native-preimage";
import {
    createNativeRecoveryCoordinator,
    type NativeRecoveryFences,
    type NativeRecoveryHost,
    type NativeRecoveryObserved,
} from "../src/world-native-recovery";

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
    throwOn: Set<string>;
    refuseOn: Set<string>;
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
        throwOn: new Set(),
        refuseOn: new Set(),
        calls: [],
        ...rest,
    } as FakeWin;
    (w as unknown as Record<string, unknown>)["setMaximize"] = function (...args: ReadonlyArray<unknown>): void {
        w.calls.push(`setMaximize=${String(args[0])},${String(args[1])}`);
        if (w.throwOn.has("setMaximize")) {
            throw new Error("native-throw");
        }
        if (w.refuseOn.has("setMaximize")) {
            return;
        }
        const v = args[0] as boolean;
        const hh = args[1] as boolean;
        w.maximizeMode = v && hh ? 3 : v ? 1 : hh ? 2 : 0;
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
        scheduleOnce: () => () => undefined,
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
        writeCount: () => calls.filter((c) => c.method === "WriteNativePreimage").length,
    };
}

function answerSeed(bus: ReturnType<typeof makeBus>, records: ReadonlyArray<Record<string, unknown>>): void {
    const read = bus.calls.find((c) => c.method === "ReadNativePreimage" && !c.answered);
    assert.ok(read !== undefined, "expected seed read");
    const corr = (JSON.parse(read.payload) as Record<string, unknown>)["correlation_id"] as string;
    read.answered = true;
    read.callback(JSON.stringify({
        v: NATIVE_PREIMAGE_VERSION, correlation_id: corr,
        outcome: "ok", records, stored: records.length, returned: records.length,
    }));
}

function answerSeedDegraded(bus: ReturnType<typeof makeBus>, reason = "unavailable"): void {
    const read = bus.calls.find((c) => c.method === "ReadNativePreimage" && !c.answered);
    assert.ok(read !== undefined, "expected seed read");
    const corr = (JSON.parse(read.payload) as Record<string, unknown>)["correlation_id"] as string;
    read.answered = true;
    read.callback(JSON.stringify({
        v: NATIVE_PREIMAGE_VERSION, correlation_id: corr,
        outcome: "degraded", records: [], stored: 0, returned: 0, reason,
    }));
}

function answerFlushes(bus: ReturnType<typeof makeBus>, ok: boolean): number {
    const pending = bus.calls.filter((c) => c.method === "WriteNativePreimage" && !c.answered);
    for (const call of pending) {
        const body = JSON.parse(call.payload) as Record<string, unknown>;
        const corr = body["correlation_id"] as string;
        const rows = body["records"] as unknown[];
        call.answered = true;
        call.callback(ok
            ? JSON.stringify({ v: NATIVE_PREIMAGE_VERSION, correlation_id: corr, outcome: "stored", stored: rows.length })
            : JSON.stringify({ v: NATIVE_PREIMAGE_VERSION, correlation_id: corr, outcome: "rejected", reason: "unavailable" }));
    }
    return pending.length;
}

function row(win: string, over: Record<string, unknown> = {}): Record<string, unknown> {
    return { win, maximize_mode: null, fullscreen: null, desktops: null, stacking: null, ...over };
}

interface Harness {
    wins: FakeWin[];
    desks: FakeDesk[];
    host: NativeRecoveryHost;
    bus: ReturnType<typeof makeBus>;
    store: ReturnType<typeof createNativePreimageStore>;
    coord: ReturnType<typeof createNativeRecoveryCoordinator>;
    logs: string[];
    fences: NativeRecoveryFences;
    setFence: (patch: Partial<NativeRecoveryFences>) => void;
    rosterQueue: Array<FakeWin[] | null> | null;
    unlist: Set<string>;
    obsRequests: () => number;
}

function setup(wins: FakeWin[], desks: FakeDesk[] = [{ id: "desk-1" }]): Harness {
    const gen: string | null = "gen-1";
    const bus = makeBus(() => gen);
    const store = createNativePreimageStore(bus.transport, true);
    const logs: string[] = [];
    const fences: NativeRecoveryFences = {
        generation: "gen-1", settingsRev: "1", acceptedRev: "9",
        watermark: "5", clock: "7", stopped: false,
    };
    const coord = createNativeRecoveryCoordinator({
        store,
        getFences: () => ({ ...fences }),
        log: (m) => {
            logs.push(m);
        },
        // Counting only: tests advance fences explicitly to simulate the
        // entry invalidate-plus-dispatch answer.
        requestObservation: () => {
            obsCount += 1;
        },
    });
    const byId = new Map(wins.map((w) => [w.internalId, w]));
    let obsCount = 0;
    const h: Harness = {
        wins, desks, host: null as unknown as NativeRecoveryHost,
        bus, store, coord, logs, fences,
        setFence: (patch) => {
            Object.assign(h.fences, patch);
        },
        rosterQueue: null,
        unlist: new Set(),
        obsRequests: () => obsCount,
    };
    const track = (w: FakeWin, prop: string, apply: () => void): boolean => {
        w.calls.push(`set:${prop}`);
        if (w.throwOn.has(prop)) {
            throw new Error("native-throw");
        }
        if (w.refuseOn.has(prop)) {
            return false;
        }
        apply();
        return true;
    };
    h.host = {
        readProp,
        log: (m) => {
            logs.push(m);
        },
        roster: () => {
            if (h.rosterQueue !== null && h.rosterQueue.length > 0) {
                const next = h.rosterQueue.shift() as FakeWin[] | null;
                if (next === null) {
                    return null;
                }
                return next.map((w) => ({ id: w.internalId, ref: w as object }));
            }
            return h.wins.map((w) => ({ id: w.internalId, ref: w as object }));
        },
        isStillListed: (ref) => !h.unlist.has((ref as FakeWin).internalId),
        readWindowId: (ref) => {
            const id = (ref as FakeWin).internalId;
            return typeof id === "string" ? id : null;
        },
        readMaximizeMode: (ref) => {
            const v = readProp(ref, "maximizeMode");
            return typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= 3 ? v : null;
        },
        readFullscreen: (ref) => {
            const v = readProp(ref, "fullScreen");
            return typeof v === "boolean" ? v : null;
        },
        readDesktops: (ref) => {
            if (readProp(ref, "onAllDesktops") === true) {
                return [];
            }
            const members = readProp(ref, "desktops");
            if (!Array.isArray(members)) {
                return null;
            }
            return (members as FakeDesk[]).map((d) => d.id);
        },
        readStacking: (ref) => {
            const above = readProp(ref, "keepAbove");
            const below = readProp(ref, "keepBelow");
            return typeof above === "boolean" && typeof below === "boolean"
                ? { above, below } : null;
        },
        listDesktops: () => h.desks.map((d) => ({ id: d.id, ref: d as object })),
        setMaximize: (ref, vertical, horizontal) => {
            const w = byId.get((ref as FakeWin).internalId);
            assert.ok(w !== undefined);
            w.calls.push(`setMaximize=${vertical},${horizontal}`);
            if (w.throwOn.has("setMaximize")) {
                throw new Error("native-throw");
            }
            if (w.refuseOn.has("setMaximize")) {
                return false;
            }
            w.maximizeMode = vertical && horizontal ? 3 : vertical ? 1 : horizontal ? 2 : 0;
            return true;
        },
        setFullscreen: (ref, value) =>
            track(byId.get((ref as FakeWin).internalId) as FakeWin, "fullScreen", () => {
                ((ref as FakeWin).fullScreen as boolean) = value;
            }),
        setDesktops: (ref, desktops) => {
            const w = byId.get((ref as FakeWin).internalId) as FakeWin;
            w.calls.push(`set:desktops=${desktops.length}`);
            if (w.throwOn.has("desktops")) {
                throw new Error("native-throw");
            }
            if (w.refuseOn.has("desktops")) {
                return false;
            }
            w.desktops = [...(desktops as FakeDesk[])];
            return true;
        },
        setKeepAbove: (ref, value) =>
            track(byId.get((ref as FakeWin).internalId) as FakeWin, "keepAbove", () => {
                (ref as FakeWin).keepAbove = value;
                if (value === true) {
                    (ref as FakeWin).keepBelow = false;
                }
            }),
        setKeepBelow: (ref, value) =>
            track(byId.get((ref as FakeWin).internalId) as FakeWin, "keepBelow", () => {
                (ref as FakeWin).keepBelow = value;
                if (value === true) {
                    (ref as FakeWin).keepAbove = false;
                }
            }),
    };
    return h;
}

function seedReady(h: Harness, records: ReadonlyArray<Record<string, unknown>>): void {
    h.store.seed("gen-1");
    answerSeed(h.bus, records);
    assert.equal(h.store.isAvailable("gen-1"), true);
    h.coord.seed("gen-1");
}

function fencesFor(h: Harness, over: Partial<NativeRecoveryFences> = {}): NativeRecoveryFences {
    return { ...h.fences, ...over };
}

function note(h: Harness, win: unknown, slot: string, observed: NativeRecoveryObserved, over: Partial<NativeRecoveryFences> = {}): void {
    h.coord.noteObservation(win, slot, observed, fencesFor(h, over));
}

// ---------------------------------------------------------------- tests

describe("world-native-recovery", () => {
    it("restores touched slots only; null untouched means no write", () => {
        const a = makeWin({ internalId: "win-a", maximizeMode: 0, fullScreen: false, desktops: [{ id: "desk-9" }], keepAbove: false, keepBelow: true });
        const b = makeWin({ internalId: "win-b" });
        const h = setup([a, b], [{ id: "desk-1" }]);
        seedReady(h, [
            row("win-a", { maximize_mode: 3, fullscreen: true, desktops: [], stacking: { above: true, below: false } }),
            row("win-b", { maximize_mode: 1 }),
        ]);
        assert.equal(h.coord.pendingCountForTest(), 5);
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, [
            "setMaximize=true,true",
            "set:fullScreen",
            "set:desktops=0",
            "set:keepAbove",
        ]);
        assert.deepEqual(b.calls, ["setMaximize=true,false"]);
        assert.equal(h.bus.writeCount(), 0);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        assert.equal(h.coord.isBlocked("win-b", "fullscreen", "gen-1"), false);
        assert.equal(h.coord.isBlocked("win-b", "stacking", "gen-1"), false);
    });

    it("preserves exact axes, exact memberships, and coupled stacking", () => {
        const wins = [0, 1, 2, 3].map((m) => makeWin({ internalId: `win-m${m}` }));
        const member = makeWin({ internalId: "win-member", desktops: [{ id: "desk-9" }] });
        const sticky = makeWin({ internalId: "win-sticky", desktops: [{ id: "desk-9" }] });
        const both = makeWin({ internalId: "win-both", keepAbove: false, keepBelow: false });
        const h = setup([...wins, member, sticky, both], [{ id: "d1" }, { id: "d2" }, { id: "d3" }]);
        seedReady(h, [
            row("win-m0", { maximize_mode: 0 }),
            row("win-m1", { maximize_mode: 1 }),
            row("win-m2", { maximize_mode: 2 }),
            row("win-m3", { maximize_mode: 3 }),
            row("win-member", { desktops: ["d1", "d2"] }),
            row("win-sticky", { desktops: [] }),
            row("win-both", { stacking: { above: true, below: true } }),
        ]);
        h.coord.restorePending(h.host);
        assert.deepEqual((wins[0] as FakeWin).calls, ["setMaximize=false,false"]);
        assert.deepEqual((wins[1] as FakeWin).calls, ["setMaximize=true,false"]);
        assert.deepEqual((wins[2] as FakeWin).calls, ["setMaximize=false,true"]);
        assert.deepEqual((wins[3] as FakeWin).calls, ["setMaximize=true,true"]);
        // Exact memberships, not the sticky bit: one assignment of both refs.
        assert.deepEqual(member.calls, ["set:desktops=2"]);
        assert.deepEqual((member.desktops as FakeDesk[]).map((d) => d.id), ["d1", "d2"]);
        assert.deepEqual(sticky.calls, ["set:desktops=0"]);
        assert.deepEqual(both.calls, ["set:keepAbove", "set:keepBelow"]);
    });

    it("rejects missing, duplicate, and null desktop refs with zero setter calls", () => {
        const missing = makeWin({ internalId: "win-missing" });
        const dupe = makeWin({ internalId: "win-dupe" });
        const fine = makeWin({ internalId: "win-fine" });
        const h = setup([missing, dupe, fine], [{ id: "d1" }, { id: "d1" }]);
        seedReady(h, [
            row("win-missing", { desktops: ["d9"] }),
            row("win-dupe", { desktops: ["d1"] }),
            row("win-fine", { desktops: ["d1"] }),
        ]);
        h.coord.restorePending(h.host);
        assert.deepEqual(missing.calls, []);
        assert.deepEqual(dupe.calls, []);
        assert.deepEqual(fine.calls, []);
        assert.equal(h.coord.pendingCountForTest(), 3);
        // Null desktop ids reject the whole slot as well.
        const n = makeWin({ internalId: "win-null" });
        const h2 = setup([n], [{ id: "d1" }]);
        (h2.desks as Array<{ id: string } | { id: null }>).push({ id: null });
        seedReady(h2, [row("win-null", { desktops: ["d1"] })]);
        h2.coord.restorePending(h2.host);
        assert.deepEqual(n.calls, []);
    });

    it("restores a retained slot once its desktop joins late", () => {
        const a = makeWin({ internalId: "win-a" });
        const h = setup([a], [{ id: "d1" }]);
        seedReady(h, [row("win-a", { desktops: ["d2"] })]);
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, []);
        h.desks.push({ id: "d2" });
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, ["set:desktops=1"]);
        assert.deepEqual((a.desktops as FakeDesk[]).map((d) => d.id), ["d2"]);
    });

    it("rejects duplicates and misses with zero attempts", () => {
        const dupe = makeWin({ internalId: "win-dupe" });
        const gone = makeWin({ internalId: "win-gone" });
        const fine = makeWin({ internalId: "win-fine" });
        const h = setup([dupe, gone, fine]);
        seedReady(h, [
            row("win-dupe", { fullscreen: true }),
            row("win-gone", { fullscreen: true }),
            row("win-fine", { fullscreen: true }),
        ]);
        h.rosterQueue = [
            [dupe, dupe, fine],
            [dupe, dupe, fine],
            [dupe, dupe, fine],
        ];
        h.coord.restorePending(h.host);
        assert.deepEqual(dupe.calls, []);
        assert.deepEqual(gone.calls, []);
        assert.deepEqual(fine.calls, ["set:fullScreen"]);
        h.rosterQueue = null;
        h.coord.restorePending(h.host);
        assert.deepEqual(dupe.calls, ["set:fullScreen"]);
        assert.deepEqual(fine.calls, ["set:fullScreen"]);
    });

    it("seed failure disables lanes without throwing; ordinary paths stay usable", () => {
        const a = makeWin({ internalId: "win-a" });
        const h = setup([a]);
        h.store.seed("gen-1");
        answerSeedDegraded(h.bus);
        assert.equal(h.store.isAvailable("gen-1"), false);
        h.coord.seed("gen-1");
        assert.equal(h.coord.pendingCountForTest(), 0);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, []);
        assert.equal(h.bus.writeCount(), 0);
        note(h, "win-a", "fullscreen", true);
        assert.deepEqual(a.calls, []);
        h.coord.seed("gen-1");
        h.coord.prune(new Set(["win-a"]));
        h.coord.reset("gen-2");
        h.coord.stop();
        assert.deepEqual(a.calls, []);
        assert.equal(h.bus.writeCount(), 0);
    });

    it("completes only on a strictly-later matching observation plus stored ack", () => {
        const a = makeWin({ internalId: "win-a", fullScreen: false });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        note(h, "win-a", "fullscreen", true, { watermark: "6" });
        assert.equal(h.bus.writeCount(), 0);
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, ["set:fullScreen"]);
        assert.equal(h.bus.writeCount(), 0);
        // Same-watermark equality is not later: still blocked, no flush.
        note(h, "win-a", "fullscreen", true);
        assert.equal(h.bus.writeCount(), 0);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        h.setFence({ watermark: "6", clock: "8" });
        note(h, "win-a", "fullscreen", true);
        assert.equal(h.bus.writeCount(), 1);
        assert.ok(h.store.recordFor("win-a") === undefined);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        assert.equal(answerFlushes(h.bus, true), 1);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), false);
        assert.equal(h.coord.pendingCountForTest(), 0);
    });

    it("a reentrant note during the setter never clears", () => {
        const a = makeWin({ internalId: "win-a", fullScreen: false });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        const inner = h.host.setFullscreen;
        (h.host as { setFullscreen: NativeRecoveryHost["setFullscreen"] }).setFullscreen = (ref, value) => {
            const ok = inner(ref, value);
            // Homing-like invalidation advances the watermark, then a
            // reentrant fresh observation arrives mid-invocation.
            h.setFence({ watermark: "6", clock: "8" });
            h.coord.noteObservation("win-a", "fullscreen", true, fencesFor(h));
            return ok;
        };
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, ["set:fullScreen"]);
        // Rejected while invoking: no clear, no flush.
        assert.ok(h.store.recordFor("win-a") !== undefined);
        assert.equal(h.bus.writeCount(), 0);
        // The same strictly-later observation after return completes.
        note(h, "win-a", "fullscreen", true);
        assert.equal(h.bus.writeCount(), 1);
        assert.equal(answerFlushes(h.bus, true), 1);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), false);
    });

    it("stale ack stays blocked with no retry; later notes never re-flush", () => {
        const a = makeWin({ internalId: "win-a", fullScreen: false });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        h.coord.restorePending(h.host);
        h.setFence({ watermark: "6", clock: "8" });
        note(h, "win-a", "fullscreen", true);
        assert.equal(h.bus.writeCount(), 1);
        // Settings rotate before the ack: the ack proves nothing current.
        h.setFence({ settingsRev: "2" });
        assert.equal(answerFlushes(h.bus, true), 1);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        assert.equal(h.coord.pendingCountForTest(), 1);
        // No retry: a fresh observation on current fences issues no new
        // attestation, and the single native attempt stands.
        h.setFence({ settingsRev: "1", watermark: "7", clock: "9" });
        note(h, "win-a", "fullscreen", true);
        assert.equal(h.bus.writeCount(), 1);
        assert.deepEqual(a.calls, ["set:fullScreen"]);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        assert.equal(h.coord.pendingCountForTest(), 1);
    });

    it("failed ack stays blocked with no retry", () => {
        const a = makeWin({ internalId: "win-a", fullScreen: false });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        h.coord.restorePending(h.host);
        h.setFence({ watermark: "6", clock: "8" });
        note(h, "win-a", "fullscreen", true);
        assert.equal(answerFlushes(h.bus, false), 1);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        assert.equal(h.bus.writeCount(), 1);
        h.setFence({ watermark: "7", clock: "9" });
        note(h, "win-a", "fullscreen", true);
        assert.equal(h.bus.writeCount(), 1);
        assert.deepEqual(a.calls, ["set:fullScreen"]);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        assert.equal(h.coord.pendingCountForTest(), 1);
    });

    it("actual lag over-retains, then a later matching observation completes", () => {
        const a = makeWin({ internalId: "win-a", fullScreen: false });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        h.coord.restorePending(h.host);
        h.setFence({ watermark: "6", clock: "8" });
        note(h, "win-a", "fullscreen", false);
        assert.equal(h.bus.writeCount(), 0);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        a.fullScreen = true;
        note(h, "win-a", "fullscreen", true);
        assert.equal(h.bus.writeCount(), 1);
        assert.equal(answerFlushes(h.bus, true), 1);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), false);
    });

    it("no second attempt on refusal or throw; later observation still completes", () => {
        const r = makeWin({ internalId: "win-r", fullScreen: false });
        const t = makeWin({ internalId: "win-t", fullScreen: false });
        r.refuseOn.add("fullScreen");
        t.throwOn.add("fullScreen");
        const h = setup([r, t]);
        seedReady(h, [row("win-r", { fullscreen: true }), row("win-t", { fullscreen: true })]);
        h.coord.restorePending(h.host);
        assert.deepEqual(r.calls, ["set:fullScreen"]);
        assert.deepEqual(t.calls, ["set:fullScreen"]);
        h.coord.restorePending(h.host);
        assert.deepEqual(r.calls, ["set:fullScreen"]);
        assert.deepEqual(t.calls, ["set:fullScreen"]);
        r.fullScreen = true;
        t.fullScreen = true;
        h.setFence({ watermark: "6", clock: "8" });
        note(h, "win-r", "fullscreen", true);
        note(h, "win-t", "fullscreen", true);
        assert.equal(answerFlushes(h.bus, true), 1);
        assert.equal(answerFlushes(h.bus, true), 1);
        assert.equal(h.coord.isBlocked("win-r", "fullscreen", "gen-1"), false);
        assert.equal(h.coord.isBlocked("win-t", "fullscreen", "gen-1"), false);
    });

    it("cancels on stale generation, settings, acceptedRev, clock, and stop", () => {
        const mk = (id: string): { h: Harness; w: FakeWin } => {
            const w = makeWin({ internalId: id });
            const h = setup([w]);
            seedReady(h, [row(id, { fullscreen: true })]);
            return { h, w };
        };
        {
            const { h, w } = mk("win-gen");
            h.setFence({ generation: "gen-2" });
            h.coord.restorePending(h.host);
            assert.deepEqual(w.calls, []);
        }
        {
            // Per-run binding: a settings change before the run re-binds,
            // but mid-run drift cancels the later slots with zero attempts.
            const a = makeWin({ internalId: "win-set-a" });
            const b = makeWin({ internalId: "win-set-b" });
            const h = setup([a, b]);
            seedReady(h, [row("win-set-a", { fullscreen: true }), row("win-set-b", { fullscreen: true })]);
            const innerSet = h.host.setFullscreen;
            let flipped = false;
            (h.host as { setFullscreen: NativeRecoveryHost["setFullscreen"] }).setFullscreen = (ref, value) => {
                const ok = innerSet(ref, value);
                if (!flipped) {
                    flipped = true;
                    h.setFence({ settingsRev: "2" });
                }
                return ok;
            };
            h.coord.restorePending(h.host);
            assert.deepEqual(a.calls, ["set:fullScreen"]);
            assert.deepEqual(b.calls, []);
            // The next run binds the new settings and restores the rest.
            h.coord.restorePending(h.host);
            assert.deepEqual(b.calls, ["set:fullScreen"]);
        }
        {
            // Same for acceptedRev: pre-run change re-binds, mid-run drift
            // cancels, and a stale note never clears.
            const a = makeWin({ internalId: "win-acc-a" });
            const b = makeWin({ internalId: "win-acc-b" });
            const h = setup([a, b]);
            seedReady(h, [row("win-acc-a", { fullscreen: true }), row("win-acc-b", { fullscreen: true })]);
            const innerSet = h.host.setFullscreen;
            let flipped = false;
            (h.host as { setFullscreen: NativeRecoveryHost["setFullscreen"] }).setFullscreen = (ref, value) => {
                const ok = innerSet(ref, value);
                if (!flipped) {
                    flipped = true;
                    h.setFence({ acceptedRev: "10" });
                }
                return ok;
            };
            h.coord.restorePending(h.host);
            assert.deepEqual(a.calls, ["set:fullScreen"]);
            assert.deepEqual(b.calls, []);
            note(h, "win-acc-b", "fullscreen", true, { acceptedRev: "9" });
            assert.equal(h.bus.writeCount(), 0);
            h.coord.restorePending(h.host);
            assert.deepEqual(b.calls, ["set:fullScreen"]);
        }
        {
            const { h, w } = mk("win-clock");
            h.setFence({ clock: "not-u64" });
            h.coord.restorePending(h.host);
            assert.deepEqual(w.calls, []);
        }
        {
            const { h, w } = mk("win-stopped");
            h.coord.stop();
            h.coord.restorePending(h.host);
            assert.deepEqual(w.calls, []);
        }
        {
            const a = makeWin({ internalId: "win-wm-a" });
            const b = makeWin({ internalId: "win-wm-b" });
            const h = setup([a, b]);
            seedReady(h, [row("win-wm-a", { fullscreen: true }), row("win-wm-b", { fullscreen: true })]);
            const innerSet = h.host.setFullscreen;
            let flipped = false;
            (h.host as { setFullscreen: NativeRecoveryHost["setFullscreen"] }).setFullscreen = (ref, value) => {
                const ok = innerSet(ref, value);
                if (!flipped) {
                    flipped = true;
                    h.setFence({ watermark: "6", clock: "8" });
                }
                return ok;
            };
            h.coord.restorePending(h.host);
            assert.deepEqual(a.calls, ["set:fullScreen"]);
            assert.deepEqual(b.calls, []);
            assert.equal(h.coord.pendingCountForTest(), 2);
            h.coord.restorePending(h.host);
            assert.deepEqual(a.calls, ["set:fullScreen"]);
            assert.deepEqual(b.calls, ["set:fullScreen"]);
        }
    });

    it("second stacking subcall lapses without lifetime while the first stands", () => {
        const a = makeWin({ internalId: "win-a", keepAbove: false, keepBelow: false });
        const h = setup([a]);
        seedReady(h, [row("win-a", { stacking: { above: true, below: true } })]);
        h.rosterQueue = [[a], []];
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, ["set:keepAbove"]);
        assert.equal(a.keepAbove, true);
    });

    it("same-generation reset fences the second subcall", () => {
        const a = makeWin({ internalId: "win-a", keepAbove: false, keepBelow: false });
        const h = setup([a]);
        seedReady(h, [row("win-a", { stacking: { above: true, below: true } })]);
        const inner = h.host.setKeepAbove;
        (h.host as { setKeepAbove: NativeRecoveryHost["setKeepAbove"] }).setKeepAbove = (ref, value) => {
            const ok = inner(ref, value);
            h.coord.reset("gen-2");
            return ok;
        };
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, ["set:keepAbove"]);
        assert.equal(h.coord.pendingCountForTest(), 0);
        assert.equal(h.coord.isBlocked("win-a", "stacking", "gen-1"), true);
    });

    it("stale observation fences never clear", () => {
        const a = makeWin({ internalId: "win-a", keepAbove: true, keepBelow: false });
        const h = setup([a]);
        seedReady(h, [row("win-a", { stacking: { above: true, below: false } })]);
        h.coord.restorePending(h.host);
        note(h, "win-a", "stacking", { above: true, below: false }, { watermark: "4" });
        assert.equal(h.bus.writeCount(), 0);
        note(h, "win-a", "stacking", { above: true, below: false }, { settingsRev: "2" });
        assert.equal(h.bus.writeCount(), 0);
        note(h, "win-a", "stacking", { above: true, below: false }, { generation: "gen-2" });
        assert.equal(h.bus.writeCount(), 0);
        note(h, "win-a", "stacking", { above: true, below: false }, { clock: "1" });
        assert.equal(h.bus.writeCount(), 0);
        assert.equal(h.coord.isBlocked("win-a", "stacking", "gen-1"), true);
        h.setFence({ watermark: "6", clock: "8" });
        note(h, "win-a", "stacking", { above: true, below: false });
        assert.equal(answerFlushes(h.bus, true), 1);
        assert.equal(h.coord.isBlocked("win-a", "stacking", "gen-1"), false);
    });

    it("rotation before the housekeeping ack fences the completion", () => {
        const a = makeWin({ internalId: "win-a", fullScreen: false });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        h.coord.restorePending(h.host);
        h.setFence({ watermark: "6", clock: "8" });
        note(h, "win-a", "fullscreen", true);
        assert.equal(h.bus.writeCount(), 1);
        h.coord.reset("gen-2");
        assert.equal(answerFlushes(h.bus, true), 1);
        assert.equal(h.coord.pendingCountForTest(), 0);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
    });

    it("incomplete absence over-retains; complete absence cleans up durably", () => {
        const a = makeWin({ internalId: "win-a" });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true, maximize_mode: 2 })]);
        h.coord.restorePending(h.host);
        assert.equal(h.coord.pendingCountForTest(), 2);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        h.coord.prune(new Set(["win-other"]));
        assert.equal(h.coord.pendingCountForTest(), 0);
        assert.ok(h.store.recordFor("win-a") === undefined);
        assert.equal(h.bus.writeCount(), 1);
        assert.equal(answerFlushes(h.bus, true), 1);
        assert.ok(h.logs.some((l) => l.includes("native-recovery-prune outcome=stored")));
    });

    it("prune with nothing absent issues no flush", () => {
        const a = makeWin({ internalId: "win-a" });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        h.coord.prune(new Set(["win-a"]));
        assert.equal(h.coord.pendingCountForTest(), 1);
        assert.equal(h.bus.writeCount(), 0);
    });

    it("no-op restore requests one fresh observation per batch", () => {
        const a = makeWin({ internalId: "win-a", fullScreen: true });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, ["set:fullScreen"]);
        assert.equal(h.obsRequests(), 1);
        // A repeat run attempts nothing new: no second request.
        h.coord.restorePending(h.host);
        assert.equal(h.obsRequests(), 1);
        // The requested observation arrives strictly newer and completes.
        h.setFence({ watermark: "6", clock: "8" });
        note(h, "win-a", "fullscreen", true);
        assert.equal(h.bus.writeCount(), 1);
        assert.equal(answerFlushes(h.bus, true), 1);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), false);
    });

    it("no observation request without an actual native call", () => {
        const a = makeWin({ internalId: "win-a" });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        h.unlist.add("win-a");
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, []);
        assert.equal(h.obsRequests(), 0);
        assert.equal(h.coord.pendingCountForTest(), 1);
    });

    it("prune normalizes braced ids; malformed live lists abort", () => {
        const id = "win-prune";
        const a = makeWin({ internalId: id });
        const h = setup([a]);
        seedReady(h, [row(id, { fullscreen: true })]);
        h.coord.prune(new Set([`{${id}}`]));
        assert.equal(h.coord.pendingCountForTest(), 1);
        assert.ok(h.store.recordFor(id) !== undefined);
        assert.equal(h.bus.writeCount(), 0);
        h.coord.prune(new Set([123 as unknown as string]));
        assert.equal(h.coord.pendingCountForTest(), 1);
        h.coord.prune(new Set(["!!!"]));
        assert.equal(h.coord.pendingCountForTest(), 1);
        assert.ok(h.store.recordFor(id) !== undefined);
        assert.equal(h.bus.writeCount(), 0);
    });

    it("runtime arms are never mistaken for startup leftovers", () => {
        const a = makeWin({ internalId: "win-a" });
        const fresh = makeWin({ internalId: "win-fresh" });
        const h = setup([a, fresh]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        assert.equal(h.store.arm("win-fresh", { fullscreen: true }), true);
        assert.equal(h.store.arm("win-fresh", { maximize_mode: 3 }), true);
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, ["set:fullScreen"]);
        assert.deepEqual(fresh.calls, []);
        assert.equal(h.coord.pendingCountForTest(), 1);
        h.coord.seed("gen-1");
        h.coord.restorePending(h.host);
        assert.deepEqual(a.calls, ["set:fullScreen"]);
        assert.deepEqual(fresh.calls, []);
    });

    it("fail-closed gates: invalid input, unknown generation, stopped, non-string ids", () => {
        const a = makeWin({ internalId: "win-a" });
        const h = setup([a]);
        seedReady(h, [row("win-a", { fullscreen: true })]);
        assert.equal(h.coord.isBlocked("!!!", "fullscreen", "gen-1"), true);
        assert.equal(h.coord.isBlocked("win-a", "bogus", "gen-1"), true);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-2"), true);
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", null), true);
        assert.equal(h.coord.isBlocked(123 as unknown as string, "fullscreen", "gen-1"), true);
        assert.equal(h.coord.isBlocked(null as unknown as string, "fullscreen", "gen-1"), true);
        note(h, 123 as unknown as string, "fullscreen", true, { watermark: "6" });
        assert.equal(h.bus.writeCount(), 0);
        h.coord.stop();
        assert.equal(h.coord.isBlocked("win-a", "fullscreen", "gen-1"), true);
        assert.equal(h.coord.pendingCountForTest(), 0);
    });
});
