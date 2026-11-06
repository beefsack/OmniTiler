// Opaque intent store controller tests (offline only, no D-Bus, no KWin).
//
// Covers the bounded persistence contract: v2 seed import byte-identical,
// degraded-empty failure policy, live-joined reports, verbatim core-write
// application (string replaces, null deletes), full-snapshot writes that
// retain out-of-scope records, exactly-once completion, and
// generation/stop fencing. Never asserts on store bytes in logs.

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
    return {
        transport, calls, timers, logs,
        kicks: () => hydratedKicks,
        setStopped: (v: boolean) => {
            stopped = v;
        },
        setGen: (v: string | null) => {
            gen = v;
        },
        fireDelay, lastCall,
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

describe("world intent store (opaque persistence controller)", () => {
    it("disabled controllers bypass the bus hydrated and empty", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, false);
        assert.equal(store.enabled, false);
        assert.equal(store.isHydrated("gen-1"), true);
        store.seed("gen-1");
        assert.deepEqual(f.calls, []);
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
        assert.equal(store.incorporateWrites([{ win: "win-a", data: DATA_A }]), false);
        let settled: boolean | null = null;
        store.persist((ok) => {
            settled = ok;
        });
        assert.equal(settled, true);
        assert.deepEqual(f.calls, []);
        store.stop();
    });

    it("seed imports the v2 snapshot byte-identical and joins live only", () => {
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

    it("late hydration never overwrites runtime core writes", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const read = f.lastCall("ReadFloatIntent");
        assert.equal(store.incorporateWrites([{ win: "win-a", data: DATA_B }]), true);
        read.callback(okReply(requestCorrelation(read), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.isHydrated("gen-1"), true);
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), [
            { win: "win-a", data: DATA_B },
        ]);
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

    it("core writes replace, delete, and add; unchanged writes report false", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        f.lastCall("ReadFloatIntent").callback(okReply(requestCorrelation(f.lastCall("ReadFloatIntent")), [{ win: "win-a", data: DATA_A }]));
        assert.equal(store.incorporateWrites([{ win: "win-a", data: DATA_A }]), false);
        assert.equal(
            store.incorporateWrites([
                { win: "win-a", data: DATA_B },
                { win: "win-b", data: null },
                { win: "win-c", data: DATA_A },
            ]),
            true,
        );
        assert.deepEqual(store.recordsForReport(new Set(["win-a", "win-b", "win-c"])), [
            { win: "win-a", data: DATA_B },
            { win: "win-c", data: DATA_A },
        ]);
        assert.equal(store.incorporateWrites([{ win: "win-a", data: DATA_B }]), false);
    });

    it("persist writes the full snapshot and reports explicit success exactly once", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const seedRead = f.lastCall("ReadFloatIntent");
        seedRead.callback(okReply(requestCorrelation(seedRead), [
            { win: "win-a", data: DATA_A },
            { win: "ghost", data: DATA_B },
        ]));
        assert.equal(store.incorporateWrites([{ win: "win-b", data: DATA_A }]), true);
        let settled: boolean | null = null;
        let calls = 0;
        store.persist((ok) => {
            calls += 1;
            settled = ok;
        });
        const write = f.lastCall("WriteFloatIntent");
        const body = JSON.parse(write.payload) as Record<string, unknown>;
        assert.equal(body["v"], 2);
        assert.deepEqual(body["records"], [
            { win: "win-a", data: DATA_A },
            { win: "ghost", data: DATA_B },
            { win: "win-b", data: DATA_A },
        ]);
        write.callback(JSON.stringify({ v: 2, correlation_id: requestCorrelation(write), outcome: "stored", stored: 3 }));
        assert.equal(settled, true);
        assert.equal(calls, 1);
        write.callback(JSON.stringify({ v: 2, correlation_id: requestCorrelation(write), outcome: "stored", stored: 3 }));
        assert.equal(calls, 1);
        for (const line of f.logs) {
            assert.ok(!line.includes(DATA_A), `bytes leaked: ${line}`);
        }
    });

    it("persist failures settle false exactly once and never touch the map", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        f.lastCall("ReadFloatIntent").callback(okReply(requestCorrelation(f.lastCall("ReadFloatIntent")), [{ win: "win-a", data: DATA_A }]));
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
        const timeout = makeTransport();
        {
            const second = createWorldIntentStore(timeout.transport, true);
            second.seed("gen-1");
            const timeoutRead = timeout.lastCall("ReadFloatIntent");
            timeoutRead.callback(okReply(requestCorrelation(timeoutRead), []));
            let done: boolean | null = null;
            second.persist((ok) => {
                done = ok;
            });
            timeout.fireDelay(2000);
            assert.equal(done, false);
        }
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

    it("degraded verdict clears the imported map unless runtime writes landed", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const first = f.lastCall("ReadFloatIntent");
        first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), [
            { win: "win-a", data: DATA_A },
        ]);
        f.setGen("gen-2");
        store.seed("gen-2");
        const second = f.lastCall("ReadFloatIntent");
        second.callback(JSON.stringify({
            v: 2, correlation_id: requestCorrelation(second), outcome: "degraded",
            records: [], stored: 0, returned: 0, reason: "namespace-unavailable",
        }));
        assert.equal(store.isHydrated("gen-2"), true);
        assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
        const kept = makeTransport();
        {
            const guarded = createWorldIntentStore(kept.transport, true);
            guarded.seed("gen-1");
            const seedRead = kept.lastCall("ReadFloatIntent");
            seedRead.callback(okReply(requestCorrelation(seedRead), [{ win: "win-a", data: DATA_A }]));
            kept.setGen("gen-2");
            guarded.seed("gen-2");
            const reseed = kept.lastCall("ReadFloatIntent");
            assert.equal(guarded.incorporateWrites([{ win: "win-b", data: DATA_B }]), true);
            reseed.callback(JSON.stringify({
                v: 2, correlation_id: requestCorrelation(reseed), outcome: "degraded",
                records: [], stored: 0, returned: 0, reason: "corrupt",
            }));
            assert.deepEqual(guarded.recordsForReport(new Set(["win-a", "win-b"])), [
                { win: "win-a", data: DATA_A },
                { win: "win-b", data: DATA_B },
            ]);
        }
    });

    it("stored with a missing or mismatched count settles false without success", () => {
        for (const reply of ["missing", "mismatch"]) {
            const f = makeTransport();
            const store = createWorldIntentStore(f.transport, true);
            store.seed("gen-1");
            const seedRead = f.lastCall("ReadFloatIntent");
            seedRead.callback(okReply(requestCorrelation(seedRead), [{ win: "win-a", data: DATA_A }]));
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
        }
    });

    it("write ignores a foreign correlation and stays owed until timeout", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const seedRead = f.lastCall("ReadFloatIntent");
        seedRead.callback(okReply(requestCorrelation(seedRead), [{ win: "win-a", data: DATA_A }]));
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
    });

    it("reseed failures degrade empty even with an existing map", () => {
        // Timeout clears the reseeded map.
        {
            const f = makeTransport();
            const store = createWorldIntentStore(f.transport, true);
            store.seed("gen-1");
            const first = f.lastCall("ReadFloatIntent");
            first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
            f.setGen("gen-2");
            store.seed("gen-2");
            f.fireDelay(2000);
            assert.equal(store.isHydrated("gen-2"), true);
            assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
            assert.ok(f.logs.some((l) => l.includes("reason=timeout") && l.includes("stored=0")), f.logs.join("\n"));
        }
        // Corrupt body clears the reseeded map.
        {
            const f = makeTransport();
            const store = createWorldIntentStore(f.transport, true);
            store.seed("gen-1");
            const first = f.lastCall("ReadFloatIntent");
            first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
            f.setGen("gen-2");
            store.seed("gen-2");
            f.lastCall("ReadFloatIntent").callback("not json{{{");
            assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
        }
        // Unreadable (non-string) reply clears the reseeded map.
        {
            const f = makeTransport();
            const store = createWorldIntentStore(f.transport, true);
            store.seed("gen-1");
            const first = f.lastCall("ReadFloatIntent");
            first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
            f.setGen("gen-2");
            store.seed("gen-2");
            f.lastCall("ReadFloatIntent").callback(42);
            assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
            assert.ok(f.logs.some((l) => l.includes("reason=unreadable") && l.includes("stored=0")), f.logs.join("\n"));
        }
        // Count mismatch clears the reseeded map.
        {
            const f = makeTransport();
            const store = createWorldIntentStore(f.transport, true);
            store.seed("gen-1");
            const first = f.lastCall("ReadFloatIntent");
            first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
            f.setGen("gen-2");
            store.seed("gen-2");
            const read = f.lastCall("ReadFloatIntent");
            read.callback(JSON.stringify({
                v: INTENT_STORE_VERSION, correlation_id: requestCorrelation(read), outcome: "ok",
                records: [{ win: "win-a", data: DATA_A }], stored: 7, returned: 1,
            }));
            assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
        }
        // Transport throw on reseed clears the reseeded map.
        {
            const f = makeTransport();
            let shouldThrow = false;
            const throwing: typeof f.transport = {
                ...f.transport,
                callDbus: (
                    service: string, path: string, iface: string, method: string,
                    payload: string, callback: (reply: unknown) => void,
                ): void => {
                    if (shouldThrow) {
                        throw new Error("bus down");
                    }
                    f.transport.callDbus(service, path, iface, method, payload, callback);
                },
            };
            const store = createWorldIntentStore(throwing, true);
            store.seed("gen-1");
            const first = f.lastCall("ReadFloatIntent");
            first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
            shouldThrow = true;
            f.setGen("gen-2");
            store.seed("gen-2");
            assert.equal(store.isHydrated("gen-2"), true);
            assert.deepEqual(store.recordsForReport(new Set(["win-a"])), []);
            assert.ok(f.logs.some((l) => l.includes("reason=transport") && l.includes("stored=0")), f.logs.join("\n"));
        }
    });

    it("runtime-newer fence keeps the map across failed reseeds", () => {
        // Timeout with runtime writes landed keeps the map.
        {
            const f = makeTransport();
            const store = createWorldIntentStore(f.transport, true);
            store.seed("gen-1");
            const first = f.lastCall("ReadFloatIntent");
            first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
            f.setGen("gen-2");
            store.seed("gen-2");
            assert.equal(store.incorporateWrites([{ win: "win-b", data: DATA_B }]), true);
            f.fireDelay(2000);
            assert.equal(store.isHydrated("gen-2"), true);
            assert.deepEqual(store.recordsForReport(new Set(["win-a", "win-b"])), [
                { win: "win-a", data: DATA_A },
                { win: "win-b", data: DATA_B },
            ]);
        }
        // Degraded verdict with runtime writes landed keeps the map.
        {
            const f = makeTransport();
            const store = createWorldIntentStore(f.transport, true);
            store.seed("gen-1");
            const first = f.lastCall("ReadFloatIntent");
            first.callback(okReply(requestCorrelation(first), [{ win: "win-a", data: DATA_A }]));
            f.setGen("gen-2");
            store.seed("gen-2");
            assert.equal(store.incorporateWrites([{ win: "win-b", data: DATA_B }]), true);
            const reseed = f.lastCall("ReadFloatIntent");
            reseed.callback(JSON.stringify({
                v: INTENT_STORE_VERSION, correlation_id: requestCorrelation(reseed), outcome: "degraded",
                records: [], stored: 0, returned: 0, reason: "corrupt",
            }));
            assert.deepEqual(store.recordsForReport(new Set(["win-a", "win-b"])), [
                { win: "win-a", data: DATA_A },
                { win: "win-b", data: DATA_B },
            ]);
        }
    });
    it("stop settles writes once and cancels their timers", () => {
        const f = makeTransport();
        const store = createWorldIntentStore(f.transport, true);
        store.seed("gen-1");
        const seedRead = f.lastCall("ReadFloatIntent");
        seedRead.callback(okReply(requestCorrelation(seedRead), [{ win: "win-a", data: DATA_A }]));
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
