// Retained intent desire store tests (offline only, no D-Bus, no KWin).
//
// Covers the desired-versus-durable contract: v2 seed imports durable only,
// degraded-empty failure policy, live-joined durable reports, whole-map
// desired replacement (string target, null retained absence, missing rows
// carry no authority), converged writes retaining out-of-scope records,
// exactly-once completion with captured-snapshot durability, serialization
// of racing desires, and generation/stop fencing. Never asserts on store
// bytes in logs.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import {
    createWorldIntentStore,
    INTENT_STORE_VERSION,
    type WorldIntentStoreTransport,
} from "../src/world-intent-store";

interface BusCall {
    method: string;
    payload: string;
    callback: (reply: unknown) => void;
}

interface Timer {
    delay: number;
    cb: () => void;
    cancelled: boolean;
}

function makeTransport() {
    const calls: BusCall[] = [];
    const timers: Timer[] = [];
    const logs: string[] = [];
    let hydratedKicks = 0;
    const transport: WorldIntentStoreTransport = {
        route: { service: "svc", object: "/obj", iface: "iface" },
        callDbus: (_s, _p, _i, method, payload, callback) => {
            calls.push({ method, payload, callback });
        },
        scheduleOnce: (delay, cb) => {
            const t: Timer = { delay, cb, cancelled: false };
            timers.push(t);
            return () => {
                t.cancelled = true;
            };
        },
        log: (m) => {
            logs.push(m);
        },
        isStopped: () => stopped,
        getGeneration: () => gen,
        onHydrated: () => {
            hydratedKicks += 1;
        },
    };
    let stopped = false;
    let gen: string | null = "gen-1";
    const fire = (index: number): void => {
        const t = timers[index] as Timer;
        timers.splice(index, 1);
        if (!t.cancelled) {
            t.cb();
        }
    };
    const fireDelay = (delay: number): void => {
        for (let i = 0; i < timers.length; i += 1) {
            const t = timers[i] as Timer;
            if (t.delay === delay && !t.cancelled) {
                fire(i);
                return;
            }
        }
        throw new Error(`no timer delay=${delay}`);
    };
    const lastCall = (method: string): BusCall => {
        const found = [...calls].reverse().find((c) => c.method === method);
        assert.ok(found !== undefined, `expected ${method}`);
        return found as BusCall;
    };
    const writeCount = (): number => calls.filter((c) => c.method === "WriteFloatIntent").length;
    return {
        transport, calls, timers, logs,
        kicks: () => hydratedKicks,
        setStopped: (v: boolean) => {
            stopped = v;
        },
        setGen: (v: string | null) => {
            gen = v;
        },
        fireDelay, lastCall, writeCount,
    };
}

const DATA_A = "{\"v\":1,\"float\":true}";
const DATA_B = "{\"v\":1,\"float\":false}";

function okReply(correlation: string, records: ReadonlyArray<{ win: string; data: string }>): string {
    return JSON.stringify({
        v: INTENT_STORE_VERSION,
        correlation_id: correlation,
        outcome: "ok",
        records,
        stored: records.length,
        returned: records.length,
    });
}

function requestCorrelation(call: BusCall): string {
    return (JSON.parse(call.payload) as Record<string, unknown>)["correlation_id"] as string;
}

function hydrateEmpty(f: ReturnType<typeof makeTransport>): void {
    const storeSeed = f.lastCall("ReadFloatIntent");
    storeSeed.callback(okReply(requestCorrelation(storeSeed), []));
}

describe("world intent store (retained desire controller)", () => {
    it("disabled controllers bypass the bus hydrated and empty", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, false);
        assert.equal(store.enabled, false);
        assert.equal(store.isHydrated("gen-1"), true);
        store.seed("gen-1");
        assert.deepEqual(f.calls, []);
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_A }], "gen-1"), false);
        assert.equal(store.needsWrite(), false);
        let settled: boolean | null = null;
        store.persist((ok) => {
            settled = ok;
        });
        assert.equal(settled, true);
        assert.deepEqual(f.calls, []);
        store.stop();
    });

    it("seed imports the v2 durable snapshot and joins live only", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        assert.equal(store.isHydrated("gen-1"), false);
        store.seed("gen-1");
        const read = f.lastCall("ReadFloatIntent");
        const body = JSON.parse(read.payload) as Record<string, unknown>;
        assert.equal(body["v"], 2);
        assert.ok(typeof body["correlation_id"] === "string");
        read.callback(okReply(requestCorrelation(read), [
            { win: "win-a", data: DATA_A },
            { win: "ghost", data: DATA_B },
        ]));
        assert.equal(store.isHydrated("gen-1"), true);
        assert.equal(f.kicks(), 1);
        assert.deepEqual(store.recordsForReport(new Set(["win-a", "win-b"])), [
            { win: "win-a", data: DATA_A },
        ]);
        // Empty desired equals durable: zero disk writes.
        assert.equal(store.needsWrite(), false);
        for (const line of f.logs) {
            assert.ok(!line.includes(DATA_A), `bytes leaked: ${line}`);
            assert.ok(!line.includes("ghost"), `id leaked: ${line}`);
        }
    });

    it("seed degrades empty on corrupt, timeout, and transport loss", () => {
        const corrupt = makeTransport();
        {
            const store = createWorldIntentStore(corrupt.transport, true);
            store.seed("gen-1");
            corrupt.lastCall("ReadFloatIntent").callback("not json{{{");
            assert.equal(store.isHydrated("gen-1"), true);
            assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
            assert.ok(corrupt.logs.some((l) => l.includes("reason=corrupt")), corrupt.logs.join("\n"));
        }
        const timeout = makeTransport();
        {
            const store = createWorldIntentStore(timeout.transport, true);
            store.seed("gen-1");
            timeout.fireDelay(2000);
            assert.equal(store.isHydrated("gen-1"), true);
            assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
            assert.ok(timeout.logs.some((l) => l.includes("reason=timeout")), timeout.logs.join("\n"));
        }
        const transportLoss = makeTransport();
        {
            const store = createWorldIntentStore(
                { ...transportLoss.transport, callDbus: () => { throw new Error("bus down"); } },
                true,
            );
            store.seed("gen-1");
            assert.equal(store.isHydrated("gen-1"), true);
            assert.ok(transportLoss.logs.some((l) => l.includes("reason=transport")), transportLoss.logs.join("\n"));
        }
    });

    it("late seed import updates durable without overwriting runtime desired", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const read = f.lastCall("ReadFloatIntent");
        // Runtime desire lands before the seed verdict.
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_B }], "gen-1"), true);
        assert.equal(store.needsWrite(), true);
        read.callback(okReply(requestCorrelation(read), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.isHydrated("gen-1"), true);
        // Durable imports the server bytes; the runtime target survives.
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), [
            { win: "win-a", data: DATA_A },
        ]);
        assert.equal(store.needsWrite(), true);
    });

    it("failed reseed clears durable but keeps the desired target", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const first = f.lastCall("ReadFloatIntent");
        first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_A }], "gen-1"), true);
        assert.equal(store.needsWrite(), false);
        f.setGen("gen-2");
        store.seed("gen-2");
        // New generation owns a fresh desired target.
        assert.equal(store.needsWrite(), false);
        assert.equal(store.setDesired([{ win: "win-b", data: DATA_B }], "gen-2"), true);
        const reseed = f.lastCall("ReadFloatIntent");
        reseed.callback(JSON.stringify({
            v: 2, correlation_id: requestCorrelation(reseed), outcome: "degraded",
            records: [], stored: 0, returned: 0, reason: "corrupt",
        }));
        assert.equal(store.isHydrated("gen-2"), true);
        assert.deepEqual(store.recordsForReport(new Set(["win-a", "win-b"])), []);
        // Desired survives the degraded durable base: still owed.
        assert.equal(store.needsWrite(), true);
    });

    it("setDesired replaces the whole map; null retains absence, missing rows grant nothing", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        hydrateEmpty(f);
        // First target: string replaces into owed state.
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_A }], "gen-1"), true);
        assert.equal(store.needsWrite(), true);
        // Same target reports no change.
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_A }], "gen-1"), false);
        // Null is an explicit retained absence: durable empty already
        // satisfies it, so nothing is owed.
        assert.equal(store.setDesired([{ win: "win-a", data: null }], "gen-1"), true);
        assert.equal(store.needsWrite(), false);
        // Missing rows carry no authority: an empty projection owes nothing
        // even with extra durable rows present.
        const g = makeTransport();
        const second = createWorldIntentStore(g.transport, true);
        second.seed("gen-1");
        const read = g.lastCall("ReadFloatIntent");
        read.callback(okReply(requestCorrelation(read), [
            { win: "win-a", data: DATA_A },
            { win: "ghost", data: DATA_B },
        ]));
        assert.equal(second.setDesired([], "gen-1"), false);
        assert.equal(second.needsWrite(), false);
        assert.deepEqual(second.recordsForReport(new Set(["win-a", "ghost"])), [
            { win: "win-a", data: DATA_A },
            { win: "ghost", data: DATA_B },
        ]);
    });

    it("setDesired ignores stale generations, stopped controllers, and malformed rows", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        hydrateEmpty(f);
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_A }], "gen-2"), false);
        assert.equal(store.needsWrite(), false);
        f.setStopped(true);
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_A }], "gen-1"), false);
        f.setStopped(false);
        // Malformed rows never enter the map: bad id, oversize data, and
        // non-string data are skipped; first-wins keeps uniqueness.
        assert.equal(store.setDesired([
            { win: "", data: DATA_A },
            { win: "win-a", data: "x".repeat(1025) },
            { win: "win-b", data: 42 as unknown as string },
            { win: "win-c", data: DATA_A },
            { win: "win-c", data: DATA_B },
        ], "gen-1"), true);
        assert.equal(store.needsWrite(), true);
        let settled: boolean | null = null;
        store.persist((ok) => {
            settled = ok;
        });
        const write = f.lastCall("WriteFloatIntent");
        const body = JSON.parse(write.payload) as Record<string, unknown>;
        assert.deepEqual(body["records"], [{ win: "win-c", data: DATA_A }]);
        write.callback(JSON.stringify({ v: 2, correlation_id: requestCorrelation(write), outcome: "stored", stored: 1 }));
        assert.equal(settled, true);
        assert.equal(store.needsWrite(), false);
    });

    it("equal desired means zero disk writes", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const read = f.lastCall("ReadFloatIntent");
        read.callback(okReply(requestCorrelation(read), [
            { win: "win-a", data: DATA_A },
            { win: "ghost", data: DATA_B },
        ]));
        // Exact match owes nothing and issues no bus write.
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_A }], "gen-1"), true);
        assert.equal(store.needsWrite(), false);
        assert.equal(f.writeCount(), 0);
        // Null target with absent durable row also owes nothing.
        assert.equal(store.setDesired([{ win: "win-new", data: null }], "gen-1"), true);
        assert.equal(store.needsWrite(), false);
        assert.equal(f.writeCount(), 0);
        // Extra durable rows outside the desired projection are retained,
        // never deleted by equality.
        assert.deepEqual(store.recordsForReport(new Set(["win-a", "ghost"])), [
            { win: "win-a", data: DATA_A },
            { win: "ghost", data: DATA_B },
        ]);
    });

    it("persist converges desired onto durable and reports explicit success exactly once", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const seedRead = f.lastCall("ReadFloatIntent");
        seedRead.callback(okReply(requestCorrelation(seedRead), [
            { win: "win-a", data: DATA_A },
            { win: "ghost", data: DATA_B },
        ]));
        assert.equal(store.setDesired([
            { win: "win-a", data: DATA_B },
            { win: "win-b", data: null },
            { win: "win-c", data: DATA_A },
        ], "gen-1"), true);
        let settled: boolean | null = null;
        let calls = 0;
        store.persist((ok) => {
            calls += 1;
            settled = ok;
        });
        const write = f.lastCall("WriteFloatIntent");
        const body = JSON.parse(write.payload) as Record<string, unknown>;
        assert.equal(body["v"], 2);
        // Converged: win-a replaced, win-c added, ghost retained, win-b
        // (null with no durable row) omitted.
        assert.deepEqual(body["records"], [
            { win: "ghost", data: DATA_B },
            { win: "win-a", data: DATA_B },
            { win: "win-c", data: DATA_A },
        ]);
        write.callback(JSON.stringify({ v: 2, correlation_id: requestCorrelation(write), outcome: "stored", stored: 3 }));
        assert.equal(settled, true);
        assert.equal(calls, 1);
        write.callback(JSON.stringify({ v: 2, correlation_id: requestCorrelation(write), outcome: "stored", stored: 3 }));
        assert.equal(calls, 1);
        assert.equal(store.needsWrite(), false);
        assert.deepEqual(store.recordsForReport(new Set(["win-a", "win-c", "ghost"])), [
            { win: "ghost", data: DATA_B },
            { win: "win-a", data: DATA_B },
            { win: "win-c", data: DATA_A },
        ]);
        for (const line of f.logs) {
            assert.ok(!line.includes(DATA_A), `bytes leaked: ${line}`);
        }
    });

    it("persist clears null-desired rows and retains the null target", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const seedRead = f.lastCall("ReadFloatIntent");
        seedRead.callback(okReply(requestCorrelation(seedRead), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.setDesired([{ win: "win-a", data: null }], "gen-1"), true);
        assert.equal(store.needsWrite(), true);
        let settled: boolean | null = null;
        store.persist((ok) => {
            settled = ok;
        });
        const write = f.lastCall("WriteFloatIntent");
        assert.deepEqual((JSON.parse(write.payload) as Record<string, unknown>)["records"], []);
        write.callback(JSON.stringify({ v: 2, correlation_id: requestCorrelation(write), outcome: "stored", stored: 0 }));
        assert.equal(settled, true);
        // Clear is durable now, but the null target stays retained.
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
        assert.equal(store.needsWrite(), false);
    });

    it("successful stored snapshot captures the sent bytes; racing desire stays owed", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        hydrateEmpty(f);
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_A }], "gen-1"), true);
        let settled: boolean | null = null;
        store.persist((ok) => {
            settled = ok;
        });
        const write = f.lastCall("WriteFloatIntent");
        // Newer desired lands while the call flies.
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_B }], "gen-1"), true);
        write.callback(JSON.stringify({ v: 2, correlation_id: requestCorrelation(write), outcome: "stored", stored: 1 }));
        assert.equal(settled, true);
        // Durable is the captured DATA_A snapshot; DATA_B is still owed.
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), [
            { win: "win-a", data: DATA_A },
        ]);
        assert.equal(store.needsWrite(), true);
    });

    it("persist failures settle false exactly once and never touch durable", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        f.lastCall("ReadFloatIntent").callback(okReply(requestCorrelation(f.lastCall("ReadFloatIntent")), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_B }], "gen-1"), true);
        let settled: boolean | null = null;
        let calls = 0;
        store.persist((ok) => {
            calls += 1;
            settled = ok;
        });
        const write = f.lastCall("WriteFloatIntent");
        write.callback(JSON.stringify({ v: 2, correlation_id: requestCorrelation(write), outcome: "rejected", reason: "unauthorized" }));
        assert.equal(settled, false);
        assert.equal(calls, 1);
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), [
            { win: "win-a", data: DATA_A },
        ]);
        // The desired target survives the failure: still owed, no auto-retry
        // issued by the store itself.
        assert.equal(store.needsWrite(), true);
        assert.equal(f.writeCount(), 1);
        const timeout = makeTransport();
        {
            const second = createWorldIntentStore(timeout.transport, true);
            second.seed("gen-1");
            const timeoutRead = timeout.lastCall("ReadFloatIntent");
            timeoutRead.callback(okReply(requestCorrelation(timeoutRead), []));
            assert.equal(second.setDesired([{ win: "win-a", data: DATA_A }], "gen-1"), true);
            let done: boolean | null = null;
            second.persist((ok) => {
                done = ok;
            });
            timeout.fireDelay(2000);
            assert.equal(done, false);
        }
    });

    it("seed rounds fence on nonce, generation, and stop", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const first = f.lastCall("ReadFloatIntent");
        store.seed("gen-2");
        const second = f.lastCall("ReadFloatIntent");
        assert.notEqual(first, second);
        first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.isHydrated("gen-1"), false);
        assert.equal(store.isHydrated("gen-2"), false);
        f.setGen("gen-3");
        second.callback(okReply(requestCorrelation(second), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.isHydrated("gen-2"), false);
        f.setGen("gen-2");
        store.seed("gen-2");
        const third = f.lastCall("ReadFloatIntent");
        store.stop();
        third.callback(okReply(requestCorrelation(third), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.isHydrated("gen-2"), false);
    });

    it("seed timers fence on stop and generation rotation", () => {
        const rotated = makeTransport();
        {
            const store = createWorldIntentStore(rotated.transport, true);
            store.seed("gen-1");
            rotated.setGen("gen-2");
            rotated.fireDelay(2000);
            assert.equal(store.isHydrated("gen-1"), false);
            assert.equal(store.isHydrated("gen-2"), false);
            assert.equal(rotated.kicks(), 0);
        }
        const halted = makeTransport();
        {
            const store = createWorldIntentStore(halted.transport, true);
            store.seed("gen-1");
            store.stop();
            // Stop cancels the seed timer outright: no late settle, no kick.
            assert.throws(() => halted.fireDelay(2000));
            assert.equal(store.isHydrated("gen-1"), false);
            assert.equal(halted.kicks(), 0);
        }
    });

    it("read ignores a foreign correlation and stays owed until timeout", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const read = f.lastCall("ReadFloatIntent");
        const sent = requestCorrelation(read);
        read.callback(JSON.stringify({
            v: 2, correlation_id: "intent-seed-999", outcome: "ok",
            records: [{ win: "win-a", data: DATA_A }], stored: 1, returned: 1,
        }));
        assert.equal(store.isHydrated("gen-1"), false);
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
        read.callback(okReply(sent, [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.isHydrated("gen-1"), true);
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), [
            { win: "win-a", data: DATA_A },
        ]);
    });

    it("read ok with mismatched counts or extra keys degrades empty", () => {
        for (const reply of [
            (corr: string): string => JSON.stringify({
                v: 2, correlation_id: corr, outcome: "ok",
                records: [{ win: "win-a", data: DATA_A }], stored: 7, returned: 1,
            }),
            (corr: string): string => JSON.stringify({
                v: 2, correlation_id: corr, outcome: "ok",
                records: [{ win: "win-a", data: DATA_A }], stored: 1, returned: 1, extra: true,
            }),
            (corr: string): string => JSON.stringify({
                v: 2, correlation_id: corr, outcome: "ok",
                records: [{ win: "win-a", data: DATA_A }], stored: 1,
            }),
        ]) {
            const f = makeTransport();
            const store = createWorldIntentStore(f.transport, true);
            store.seed("gen-1");
            const read = f.lastCall("ReadFloatIntent");
            read.callback(reply(requestCorrelation(read)));
            assert.equal(store.isHydrated("gen-1"), true);
            assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
        }
    });

    it("degraded verdict clears durable even with a surviving desired target", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const first = f.lastCall("ReadFloatIntent");
        first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_A }], "gen-1"), true);
        assert.equal(store.needsWrite(), false);
        f.setGen("gen-2");
        store.seed("gen-2");
        // New generation clears desired; re-target win-b before the verdict.
        assert.equal(store.setDesired([{ win: "win-b", data: DATA_B }], "gen-2"), true);
        const second = f.lastCall("ReadFloatIntent");
        second.callback(JSON.stringify({
            v: 2, correlation_id: requestCorrelation(second), outcome: "degraded",
            records: [], stored: 0, returned: 0, reason: "namespace-unavailable",
        }));
        assert.equal(store.isHydrated("gen-2"), true);
        assert.deepEqual(store.recordsForReport(new Set(["win-a", "win-b"])), []);
        // Desired survives degraded durable: still owed.
        assert.equal(store.needsWrite(), true);
    });

    it("stored with a missing or mismatched count settles false without success", () => {
        for (const reply of ["missing", "mismatch"]) {
            const f = makeTransport();
            const store = createWorldIntentStore(f.transport, true);
            store.seed("gen-1");
            const seedRead = f.lastCall("ReadFloatIntent");
            seedRead.callback(okReply(requestCorrelation(seedRead), [{ win: "win-a", data: DATA_A }]));
            assert.equal(store.setDesired([{ win: "win-a", data: DATA_B }], "gen-1"), true);
            let settled: boolean | null = null;
            store.persist((ok) => {
                settled = ok;
            });
            const write = f.lastCall("WriteFloatIntent");
            const corr = requestCorrelation(write);
            write.callback(reply === "missing"
                ? JSON.stringify({ v: 2, correlation_id: corr, outcome: "stored" })
                : JSON.stringify({ v: 2, correlation_id: corr, outcome: "stored", stored: 99 }));
            assert.equal(settled, false, reply);
            assert.deepEqual(store.recordsForReport(new Set(["win-a"])), [
                { win: "win-a", data: DATA_A },
            ]);
            assert.equal(store.needsWrite(), true);
        }
    });

    it("write ignores a foreign correlation and stays owed until timeout", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const seedRead = f.lastCall("ReadFloatIntent");
        seedRead.callback(okReply(requestCorrelation(seedRead), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_B }], "gen-1"), true);
        let settled: boolean | null = null;
        let calls = 0;
        store.persist((ok) => {
            calls += 1;
            settled = ok;
        });
        const write = f.lastCall("WriteFloatIntent");
        write.callback(JSON.stringify({ v: 2, correlation_id: "intent-write-999", outcome: "stored", stored: 1 }));
        assert.equal(settled, null);
        assert.equal(calls, 0);
        f.fireDelay(2000);
        assert.equal(settled, false);
        assert.equal(calls, 1);
        // Durable untouched by the foreign ack; desire still owed.
        assert.equal(store.needsWrite(), true);
    });

    it("stopped persist settles false and new generations reseed cleanly", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const first = f.lastCall("ReadFloatIntent");
        first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_A }], "gen-1"), true);
        // Timeout on a reseed clears the reseeded durable base.
        f.setGen("gen-2");
        store.seed("gen-2");
        f.fireDelay(2000);
        assert.equal(store.isHydrated("gen-2"), true);
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
        assert.ok(f.logs.some((l) => l.includes("reason=timeout") && l.includes("stored=0")), f.logs.join("\n"));
    });

    it("stop settles writes once and cancels their timers", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const seedRead = f.lastCall("ReadFloatIntent");
        seedRead.callback(okReply(requestCorrelation(seedRead), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.setDesired([{ win: "win-a", data: DATA_B }], "gen-1"), true);
        let settled: boolean | null = null;
        let calls = 0;
        store.persist((ok) => {
            calls += 1;
            settled = ok;
        });
        const write = f.lastCall("WriteFloatIntent");
        const corr = requestCorrelation(write);
        store.stop();
        assert.equal(settled, false);
        assert.equal(calls, 1);
        write.callback(JSON.stringify({ v: 2, correlation_id: corr, outcome: "stored", stored: 1 }));
        assert.equal(calls, 1);
        assert.throws(() => f.fireDelay(2000));
        assert.ok(!f.logs.some((l) => l.includes("intent-write outcome=timeout")), f.logs.join("\n"));
    });
});
