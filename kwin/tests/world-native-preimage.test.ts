// Native preimage store controller tests (offline only, no D-Bus, no KWin).
//
// Covers the corrected prerequisite contract: nothing mutates or persists
// before a usable seed for the live generation; strict v1 seed codec;
// full-snapshot writes that merge concurrent arms; per-lane failure
// isolation without retry or spontaneous resume (property clear preserves
// the mark, window-gone cleanup releases it); same-generation seed no-op;
// reseed hiding; rotation-dropped queued writes; stop/generation fencing.
// Never performs native writes: the only bus methods used are
// ReadNativePreimage/WriteNativePreimage.

import { describe, it } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { join } from "node:path";
import {
    createNativePreimageStore,
    mapWorldStatePropertyToPreimage,
    NATIVE_PREIMAGE_MAX_RECORDS,
    NATIVE_PREIMAGE_VERSION,
    type NativePreimageLane,
    type NativePreimageProperty,
    type NativePreimageTransport,
} from "../src/world-native-preimage";

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
    const transport: NativePreimageTransport = {
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
    const countCalls = (method: string): number => calls.filter((c) => c.method === method).length;
    return {
        transport, calls, timers, logs,
        kicks: () => hydratedKicks,
        setStopped: (v: boolean) => {
            stopped = v;
        },
        setGen: (v: string | null) => {
            gen = v;
        },
        fireDelay, lastCall, countCalls,
    };
}

function requestCorrelation(call: BusCall): string {
    return (JSON.parse(call.payload) as Record<string, unknown>)["correlation_id"] as string;
}

function okReply(correlation: string, records: ReadonlyArray<Record<string, unknown>>): string {
    return JSON.stringify({
        v: NATIVE_PREIMAGE_VERSION,
        correlation_id: correlation,
        outcome: "ok",
        records,
        stored: records.length,
        returned: records.length,
    });
}

function fullRow(win: string): Record<string, unknown> {
    return {
        win, maximize_mode: 3, fullscreen: true,
        desktops: ["desk-1"], stacking: { above: true, below: false },
    };
}

function seedOk(
    f: ReturnType<typeof makeTransport>,
    records: ReadonlyArray<Record<string, unknown>>,
): void {
    const read = f.lastCall("ReadNativePreimage");
    read.callback(okReply(requestCorrelation(read), records));
}

describe("world native preimage (recovery transport prerequisite)", () => {
    it("source stays ASCII with an escaped lane separator", () => {
        const source = readFileSync(join(process.cwd(), "src/world-native-preimage.ts"), "utf8");
        assert.ok(!source.includes(String.fromCharCode(0)), "NUL byte in source");
        assert.ok(source.includes("\\u0000"), "escaped separator missing");
    });

    it("maps WorldStateProperty to preimage slots mechanically", () => {
        assert.equal(mapWorldStatePropertyToPreimage("maximized"), "maximize_mode");
        assert.equal(mapWorldStatePropertyToPreimage("fullscreen"), "fullscreen");
        assert.equal(mapWorldStatePropertyToPreimage("sticky"), "desktops");
        assert.equal(mapWorldStatePropertyToPreimage("above"), "stacking");
    });

    it("disabled controllers bypass the bus hydrated but unavailable", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, false);
        assert.equal(store.enabled, false);
        assert.equal(store.isHydrated("gen-1"), true);
        assert.equal(store.isAvailable("gen-1"), false);
        store.seed("gen-1");
        assert.deepEqual(f.calls, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), false);
        assert.equal(store.clear("win-a"), false);
        assert.equal(store.canWrite("win-a", "maximized", "gen-1"), false);
        let settled: boolean | null = null;
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            settled = ok;
        });
        assert.equal(settled, false);
        assert.deepEqual(f.calls, []);
        store.stop();
    });

    it("early arm, clear, and flush refuse with no bus writes", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        // Before any seed: every mutation/persist path refuses.
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), false);
        assert.equal(store.clear("win-a"), false);
        let settled: boolean | null = null;
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            settled = ok;
        });
        assert.equal(settled, false);
        assert.deepEqual(f.calls, []);
        assert.deepEqual(store.snapshot(), []);
        // A failed seed leaves the same refusals: no pre-seed runtime state
        // may overwrite durable startup preimages, and no write runs.
        store.seed("gen-1");
        f.fireDelay(2000);
        assert.equal(store.isHydrated("gen-1"), true);
        assert.equal(store.isAvailable("gen-1"), false);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), false);
        assert.equal(store.clear("win-a"), false);
        settled = null;
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            settled = ok;
        });
        assert.equal(settled, false);
        assert.equal(f.countCalls("WriteNativePreimage"), 0);
        assert.deepEqual(store.snapshot(), []);
    });

    it("seed imports nullable and coupled forms with exact counts", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        assert.equal(store.isHydrated("gen-1"), false);
        assert.equal(store.canWrite("win-a", "maximized", "gen-1"), false);
        store.seed("gen-1");
        const read = f.lastCall("ReadNativePreimage");
        const body = JSON.parse(read.payload) as Record<string, unknown>;
        assert.equal(body["v"], 1);
        assert.ok(typeof body["correlation_id"] === "string");
        assert.deepEqual(Object.keys(body).sort(), ["correlation_id", "v"]);
        read.callback(okReply(requestCorrelation(read), [
            fullRow("win-full"),
            { win: "win-part", maximize_mode: null, fullscreen: false, desktops: null, stacking: null },
            { win: "win-sticky", maximize_mode: null, fullscreen: null, desktops: [], stacking: null },
            { win: "win-zero", maximize_mode: 0, fullscreen: null, desktops: null, stacking: null },
            // Both-true stacking is a genuine forced-rule read.
            { win: "win-both", maximize_mode: null, fullscreen: null, desktops: null, stacking: { above: true, below: true } },
        ]));
        // Stored before consumer callback: availability is set synchronously.
        assert.equal(store.isHydrated("gen-1"), true);
        assert.equal(store.isAvailable("gen-1"), true);
        assert.equal(f.kicks(), 1);
        assert.equal(store.canWrite("win-full", "maximized", "gen-1"), true);
        assert.equal(store.canWrite("win-full", "above", "gen-1"), true);
        assert.deepEqual(store.recordFor("win-sticky"), {
            win: "win-sticky", maximize_mode: null, fullscreen: null,
            desktops: [], stacking: null,
        });
        assert.equal(store.snapshot().length, 5);
        for (const line of f.logs) {
            assert.ok(!line.includes("win-full"), `id leaked: ${line}`);
            assert.ok(!line.includes("desk-1"), `payload leaked: ${line}`);
        }
    });

    it("seed rejects out-of-shape and out-of-range rows as corrupt", () => {
        const bad: ReadonlyArray<Record<string, unknown> | null> = [
            { win: "a", maximize_mode: 4, fullscreen: null, desktops: null, stacking: null },
            { win: "a", maximize_mode: null, fullscreen: null, desktops: null, stacking: null },
            { win: "a", maximize_mode: null, fullscreen: null, desktops: ["d1", "d1"], stacking: null },
            { win: "a", maximize_mode: null, fullscreen: null, desktops: null },
            { win: "a", maximize_mode: null, fullscreen: null, desktops: null, stacking: null, sticky: false },
            { win: "a", maximize_mode: "3", fullscreen: null, desktops: null, stacking: null },
            { win: "a", maximize_mode: 3.5, fullscreen: null, desktops: null, stacking: null },
            { win: "a", maximize_mode: 3, fullscreen: "yes", desktops: null, stacking: null },
            { win: "a", maximize_mode: 3, fullscreen: null, desktops: {}, stacking: null },
            { win: "a", maximize_mode: 3, fullscreen: null, desktops: null, stacking: { above: true } },
            { win: "a", maximize_mode: 3, fullscreen: null, desktops: null, stacking: { above: true, below: false, extra: 1 } },
            { win: "bad id", maximize_mode: 0, fullscreen: null, desktops: null, stacking: null },
            // Non-string subjects coerce under normalizeNativeId but the
            // service requires as_str: all reject.
            { win: null, maximize_mode: 0, fullscreen: null, desktops: null, stacking: null },
            { win: true, maximize_mode: 0, fullscreen: null, desktops: null, stacking: null },
            { win: 123, maximize_mode: 0, fullscreen: null, desktops: null, stacking: null },
            { win: {}, maximize_mode: 0, fullscreen: null, desktops: null, stacking: null },
            // Non-string desktop members likewise reject.
            { win: "a", maximize_mode: null, fullscreen: null, desktops: [null], stacking: null },
            { win: "a", maximize_mode: null, fullscreen: null, desktops: [true], stacking: null },
            { win: "a", maximize_mode: null, fullscreen: null, desktops: [123], stacking: null },
            { win: "a", maximize_mode: null, fullscreen: null, desktops: [{}], stacking: null },
            null,
        ];
        for (const row of bad) {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            const read = f.lastCall("ReadNativePreimage");
            read.callback(okReply(requestCorrelation(read), row === null ? [null as unknown as Record<string, unknown>] : [row]));
            assert.equal(store.isHydrated("gen-1"), true);
            assert.equal(store.isAvailable("gen-1"), false);
            assert.deepEqual(store.snapshot(), []);
            assert.ok(f.logs.some((l) => l.includes("reason=corrupt")), f.logs.join("\n"));
        }
        {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            const read = f.lastCall("ReadNativePreimage");
            read.callback(okReply(requestCorrelation(read), [fullRow("a"), fullRow("a")]));
            assert.equal(store.isAvailable("gen-1"), false);
            assert.deepEqual(store.snapshot(), []);
        }
    });

    it("seed fails closed on rejected, malformed, count, timeout, and transport loss", () => {
        {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            const read = f.lastCall("ReadNativePreimage");
            read.callback(JSON.stringify({
                v: 1, correlation_id: requestCorrelation(read),
                outcome: "rejected", records: [], reason: "not-kwin-owner",
            }));
            assert.equal(store.isHydrated("gen-1"), true);
            assert.equal(store.isAvailable("gen-1"), false);
            assert.equal(store.canWrite("win-a", "above", "gen-1"), false);
            assert.equal(f.countCalls("ReadNativePreimage"), 1);
        }
        {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            const read = f.lastCall("ReadNativePreimage");
            read.callback(JSON.stringify({
                v: 1, correlation_id: requestCorrelation(read),
                outcome: "degraded", records: [], stored: 0, returned: 0,
                reason: "namespace-unavailable",
            }));
            assert.equal(store.isAvailable("gen-1"), false);
        }
        const malformed: Array<(corr: string) => string> = [
            () => "not json{{{",
            (corr) => JSON.stringify({
                v: 1, correlation_id: corr, outcome: "ok",
                records: [fullRow("win-a")], stored: 7, returned: 1,
            }),
            (corr) => JSON.stringify({
                v: 1, correlation_id: corr, outcome: "ok",
                records: [fullRow("win-a")], stored: 1, returned: 1, extra: true,
            }),
            (corr) => JSON.stringify({
                v: 1, correlation_id: corr, outcome: "bogus",
                records: [], stored: 0, returned: 0, reason: "corrupt",
            }),
        ];
        for (const reply of malformed) {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            const read = f.lastCall("ReadNativePreimage");
            read.callback(reply(requestCorrelation(read)));
            assert.equal(store.isHydrated("gen-1"), true);
            assert.equal(store.isAvailable("gen-1"), false);
            assert.deepEqual(store.snapshot(), []);
        }
        {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            f.fireDelay(2000);
            assert.equal(store.isAvailable("gen-1"), false);
            assert.ok(f.logs.some((l) => l.includes("reason=timeout")), f.logs.join("\n"));
            assert.equal(f.countCalls("ReadNativePreimage"), 1);
        }
        {
            const f = makeTransport();
            const store = createNativePreimageStore(
                { ...f.transport, callDbus: () => { throw new Error("bus down"); } },
                true,
            );
            store.seed("gen-1");
            assert.equal(store.isHydrated("gen-1"), true);
            assert.equal(store.isAvailable("gen-1"), false);
            assert.ok(f.logs.some((l) => l.includes("reason=transport")), f.logs.join("\n"));
        }
    });

    it("synchronous seed timer failure settles degraded without escaping", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(
            { ...f.transport, scheduleOnce: () => { throw new Error("timer down"); } },
            true,
        );
        store.seed("gen-1");
        assert.equal(store.isHydrated("gen-1"), true);
        assert.equal(store.isAvailable("gen-1"), false);
        assert.ok(f.logs.some((l) => l.includes("reason=transport")), f.logs.join("\n"));
    });

    it("settled seeds never reread the same generation", () => {
        {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            seedOk(f, [fullRow("win-a")]);
            assert.equal(store.isAvailable("gen-1"), true);
            store.seed("gen-1");
            assert.equal(f.countCalls("ReadNativePreimage"), 1);
            assert.equal(store.isAvailable("gen-1"), true);
            assert.deepEqual(store.recordFor("win-a"), {
                win: "win-a", maximize_mode: 3, fullscreen: true,
                desktops: ["desk-1"], stacking: { above: true, below: false },
            });
        }
        {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            f.fireDelay(2000);
            assert.equal(store.isAvailable("gen-1"), false);
            store.seed("gen-1");
            assert.equal(f.countCalls("ReadNativePreimage"), 1);
            assert.equal(store.isAvailable("gen-1"), false);
        }
    });

    it("reseed hides the old settlement until it lands", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, [fullRow("win-a")]);
        assert.equal(store.isAvailable("gen-1"), true);
        f.setGen("gen-2");
        store.seed("gen-2");
        assert.equal(store.isHydrated("gen-1"), false);
        assert.equal(store.isAvailable("gen-1"), false);
        assert.equal(store.isAvailable("gen-2"), false);
        assert.equal(store.canWrite("win-a", "maximized", "gen-1"), false);
        const read = f.lastCall("ReadNativePreimage");
        read.callback(okReply(requestCorrelation(read), [fullRow("win-b")]));
        assert.equal(store.isAvailable("gen-2"), true);
        assert.deepEqual(store.snapshot().map((r) => r.win), ["win-b"]);
    });

    it("foreign seed correlation stays owed until the exact ack lands", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        const read = f.lastCall("ReadNativePreimage");
        const sent = requestCorrelation(read);
        read.callback(JSON.stringify({
            v: 1, correlation_id: "preimage-seed-999", outcome: "ok",
            records: [fullRow("win-a")], stored: 1, returned: 1,
        }));
        assert.equal(store.isHydrated("gen-1"), false);
        assert.equal(store.isAvailable("gen-1"), false);
        read.callback(okReply(sent, [fullRow("win-a")]));
        assert.equal(store.isAvailable("gen-1"), true);
    });

    it("arm merges per-property first-wins and refuses bad lanes", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        assert.equal(store.arm("win-a", { fullscreen: true }), true);
        // Re-arm keeps the original preimage, never the mutated read.
        assert.equal(store.arm("win-a", { maximize_mode: 1 }), false);
        assert.deepEqual(store.recordFor("win-a"), {
            win: "win-a", maximize_mode: 3, fullscreen: true,
            desktops: null, stacking: null,
        });
        assert.equal(store.arm("win-b", { stacking: { above: false, below: true } }), true);
        assert.deepEqual(store.snapshot().map((r) => r.win), ["win-a", "win-b"]);
        // Invalid arms refuse: all-null, out-of-range, bad subject.
        assert.equal(store.arm("win-c", {}), false);
        assert.equal(store.arm("win-c", { maximize_mode: null, fullscreen: null }), false);
        assert.equal(store.arm("win-c", { maximize_mode: 4 }), false);
        assert.equal(store.arm("bad id", { maximize_mode: 0 }), false);
        assert.equal(store.recordFor("win-c"), undefined);
        // Property clear drops one slot; unknown clears report false.
        assert.equal(store.clear("win-a", "fullscreen"), true);
        assert.deepEqual(store.recordFor("win-a"), {
            win: "win-a", maximize_mode: 3, fullscreen: null,
            desktops: null, stacking: null,
        });
        assert.equal(store.clear("win-a", "fullscreen"), false);
        assert.equal(store.clear("ghost"), false);
    });

    it("arm enforces the record bound for new windows only", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        for (let i = 0; i < NATIVE_PREIMAGE_MAX_RECORDS; i += 1) {
            assert.equal(store.arm(`win-${i}`, { maximize_mode: 0 }), true);
        }
        assert.equal(store.snapshot().length, NATIVE_PREIMAGE_MAX_RECORDS);
        assert.equal(store.arm("win-overflow", { maximize_mode: 0 }), false);
        assert.equal(store.recordFor("win-overflow"), undefined);
        // Existing windows may still arm further properties at the bound.
        assert.equal(store.arm("win-0", { fullscreen: true }), true);
    });

    it("flush persists the full snapshot and authorizes only exact stored acks", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, [fullRow("win-seed")]);
        assert.equal(store.arm("win-a", { maximize_mode: 2, desktops: [] }), true);
        let settled: boolean | null = null;
        let calls = 0;
        store.flush(
            [{ win: "win-a", property: "maximized" }, { win: "win-a", property: "sticky" }],
            (ok) => {
                calls += 1;
                settled = ok;
            },
        );
        const write = f.lastCall("WriteNativePreimage");
        const body = JSON.parse(write.payload) as Record<string, unknown>;
        assert.equal(body["v"], 1);
        const rows = body["records"] as Array<Record<string, unknown>>;
        assert.equal(rows.length, 2);
        assert.deepEqual(Object.keys(rows[0] as Record<string, unknown>).sort(),
            ["desktops", "fullscreen", "maximize_mode", "stacking", "win"]);
        const armed = rows.find((r) => r["win"] === "win-a") as Record<string, unknown>;
        assert.deepEqual(armed, {
            win: "win-a", maximize_mode: 2, fullscreen: null,
            desktops: [], stacking: null,
        });
        write.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(write),
            outcome: "stored", stored: 2,
        }));
        assert.equal(settled, true);
        assert.equal(calls, 1);
        write.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(write),
            outcome: "stored", stored: 2,
        }));
        assert.equal(calls, 1);
        const g = makeTransport();
        {
            const second = createNativePreimageStore(g.transport, true);
            second.seed("gen-1");
            seedOk(g, [fullRow("win-seed")]);
            let done: boolean | null = null;
            second.flush([{ win: "win-seed", property: "above" }], (ok) => {
                done = ok;
            });
            const w = g.lastCall("WriteNativePreimage");
            w.callback(JSON.stringify({ v: 1, correlation_id: requestCorrelation(w), outcome: "stored", stored: 99 }));
            assert.equal(done, false);
        }
    });

    it("serializes two concurrent flushes with merged snapshots and no lost updates", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        const first: Array<boolean | null> = [null];
        const second: Array<boolean | null> = [null];
        const firstLane: NativePreimageLane = { win: "win-a", property: "maximize_mode" };
        const secondLane: NativePreimageLane = { win: "win-b", property: "fullscreen" };
        store.flush([firstLane], (ok) => {
            first[0] = ok;
        });
        // A second arm lands while the first write is outstanding; the
        // queued flush must carry it instead of overwriting it.
        assert.equal(store.arm("win-b", { fullscreen: false }), true);
        store.flush([secondLane], (ok) => {
            second[0] = ok;
        });
        const writes = f.calls.filter((c) => c.method === "WriteNativePreimage");
        assert.equal(writes.length, 1);
        const firstRows = (JSON.parse((writes[0] as BusCall).payload) as Record<string, unknown>)["records"] as unknown[];
        assert.equal(firstRows.length, 1);
        (writes[0] as BusCall).callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(writes[0] as BusCall),
            outcome: "stored", stored: 1,
        }));
        assert.equal(first[0], true);
        assert.equal(second[0], null);
        const queued = f.calls.filter((c) => c.method === "WriteNativePreimage");
        assert.equal(queued.length, 2);
        const secondRows = (JSON.parse((queued[1] as BusCall).payload) as Record<string, unknown>)["records"] as Array<Record<string, unknown>>;
        assert.equal(secondRows.length, 2);
        assert.deepEqual(secondRows.map((r) => r["win"]), ["win-a", "win-b"]);
        (queued[1] as BusCall).callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(queued[1] as BusCall),
            outcome: "stored", stored: 2,
        }));
        assert.equal(second[0], true);
        assert.deepEqual(store.recordFor("win-b"), {
            win: "win-b", maximize_mode: null, fullscreen: false,
            desktops: null, stacking: null,
        });
    });

    it("rotation between queued writes drops the stale entry without dispatch", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        assert.equal(store.arm("win-b", { fullscreen: true }), true);
        const first: Array<boolean | null> = [null];
        const second: Array<boolean | null> = [null];
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            first[0] = ok;
        });
        store.flush([{ win: "win-b", property: "fullscreen" }], (ok) => {
            second[0] = ok;
        });
        assert.equal(f.countCalls("WriteNativePreimage"), 1);
        // The active response lands after a generation rotation: it settles
        // false without disabling, and the stale queued entry settles false
        // without ever dispatching.
        f.setGen("gen-2");
        const write = f.lastCall("WriteNativePreimage");
        write.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(write),
            outcome: "stored", stored: 2,
        }));
        assert.equal(first[0], false);
        assert.equal(second[0], false);
        assert.equal(f.countCalls("WriteNativePreimage"), 1);
        assert.equal(store.isLaneDisabled("win-a", "maximized"), false);
        assert.equal(store.isLaneDisabled("win-b", "fullscreen"), false);
        assert.ok(store.recordFor("win-a") !== undefined);
        assert.ok(store.recordFor("win-b") !== undefined);
    });

    it("flush failure disables only its lanes; property clear never resumes them", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        assert.equal(store.arm("win-b", { fullscreen: true }), true);
        let failed: boolean | null = null;
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            failed = ok;
        });
        const first = f.lastCall("WriteNativePreimage");
        first.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(first),
            outcome: "rejected", reason: "store-unavailable",
        }));
        assert.equal(failed, false);
        assert.equal(store.isLaneDisabled("win-a", "maximized"), true);
        assert.equal(store.canWrite("win-a", "maximized", "gen-1"), false);
        assert.equal(store.isLaneDisabled("win-b", "fullscreen"), false);
        assert.equal(store.canWrite("win-b", "fullscreen", "gen-1"), true);
        // A later unrelated successful snapshot does not resume the lane,
        // and arming it refuses.
        let ok: boolean | null = null;
        store.flush([{ win: "win-b", property: "fullscreen" }], (done) => {
            ok = done;
        });
        const second = f.lastCall("WriteNativePreimage");
        second.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(second),
            outcome: "stored", stored: 2,
        }));
        assert.equal(ok, true);
        assert.equal(store.isLaneDisabled("win-a", "maximized"), true);
        assert.equal(store.arm("win-a", { maximize_mode: 0 }), false);
        // Property clear is restore housekeeping, not failure recovery: the
        // mark survives, so the lane cannot resume through a clear.
        assert.equal(store.clear("win-a", "maximize_mode"), true);
        assert.equal(store.isLaneDisabled("win-a", "maximized"), true);
        assert.equal(store.arm("win-a", { maximize_mode: 0 }), false);
        assert.equal(f.countCalls("WriteNativePreimage"), 2);
        // Window-gone cleanup releases that window's marks for a fresh start.
        assert.equal(store.clear("win-a"), true);
        assert.equal(store.isLaneDisabled("win-a", "maximized"), false);
        assert.equal(store.arm("win-a", { maximize_mode: 0 }), true);
    });

    it("write failure shapes match the service strictly and stay fail-closed", () => {
        // 4-key failed reply with rejected (invalid-members) is a genuine
        // service verdict: settles false and disables, never success.
        {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            seedOk(f, []);
            assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
            let done: boolean | null = null;
            store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
                done = ok;
            });
            const w = f.lastCall("WriteNativePreimage");
            w.callback(JSON.stringify({
                v: 1, correlation_id: requestCorrelation(w),
                outcome: "rejected", reason: "invalid-members",
            }));
            assert.equal(done, false);
            assert.equal(store.isLaneDisabled("win-a", "maximized"), true);
            assert.ok(f.logs.some((l) => l.includes("outcome=rejected") && l.includes("reason=invalid-members")), f.logs.join("\n"));
        }
        // 5-key rejection verdict likewise settles false and disables.
        {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            seedOk(f, []);
            assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
            let done: boolean | null = null;
            store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
                done = ok;
            });
            const w = f.lastCall("WriteNativePreimage");
            w.callback(JSON.stringify({
                v: 1, correlation_id: requestCorrelation(w),
                outcome: "rejected", records: [], reason: "unauthorized",
            }));
            assert.equal(done, false);
            assert.equal(store.isLaneDisabled("win-a", "maximized"), true);
        }
        // 4-key unavailable verdict settles false and disables.
        {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            seedOk(f, []);
            assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
            let done: boolean | null = null;
            store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
                done = ok;
            });
            const w = f.lastCall("WriteNativePreimage");
            w.callback(JSON.stringify({
                v: 1, correlation_id: requestCorrelation(w),
                outcome: "unavailable", reason: "namespace-unavailable",
            }));
            assert.equal(done, false);
            assert.equal(store.isLaneDisabled("win-a", "maximized"), true);
        }
        // Crossed shapes fail closed: rejected must not ride records, and
        // unavailable never rides the 5-key shape.
        const crossed: Array<(corr: string) => string> = [
            (corr) => JSON.stringify({ v: 1, correlation_id: corr, outcome: "rejected", records: [{ win: "x" }], reason: "unauthorized" }),
            (corr) => JSON.stringify({ v: 1, correlation_id: corr, outcome: "rejected", reason: "unauthorized", extra: true }),
            (corr) => JSON.stringify({ v: 1, correlation_id: corr, outcome: "unavailable", records: [], reason: "store-unavailable" }),
            (corr) => JSON.stringify({ v: 1, correlation_id: corr, outcome: "stored" }),
        ];
        for (const reply of crossed) {
            const f = makeTransport();
            const store = createNativePreimageStore(f.transport, true);
            store.seed("gen-1");
            seedOk(f, []);
            assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
            let done: boolean | null = null;
            store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
                done = ok;
            });
            const w = f.lastCall("WriteNativePreimage");
            w.callback(reply(requestCorrelation(w)));
            assert.equal(done, false);
            assert.equal(store.isLaneDisabled("win-a", "maximized"), true);
        }
    });

    it("write ignores foreign correlations and stale acks never clobber fresher state", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        let settled: boolean | null = null;
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            settled = ok;
        });
        const write = f.lastCall("WriteNativePreimage");
        write.callback(JSON.stringify({ v: 1, correlation_id: "preimage-write-999", outcome: "stored", stored: 1 }));
        assert.equal(settled, null);
        assert.equal(store.arm("win-b", { fullscreen: true }), true);
        f.fireDelay(2000);
        assert.equal(settled, false);
        assert.equal(store.isLaneDisabled("win-a", "maximized"), true);
        assert.ok(store.recordFor("win-b") !== undefined);
        assert.equal(store.isLaneDisabled("win-b", "fullscreen"), false);
    });

    it("stop and generation fences settle writes once without late effects", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, [fullRow("win-a")]);
        let settled: boolean | null = null;
        let calls = 0;
        store.flush([{ win: "win-a", property: "above" }], (ok) => {
            calls += 1;
            settled = ok;
        });
        const write = f.lastCall("WriteNativePreimage");
        const corr = requestCorrelation(write);
        store.stop();
        assert.equal(settled, false);
        assert.equal(calls, 1);
        write.callback(JSON.stringify({ v: 1, correlation_id: corr, outcome: "stored", stored: 1 }));
        assert.equal(calls, 1);
        assert.throws(() => f.fireDelay(2000));
        assert.equal(store.arm("win-b", { fullscreen: true }), false);
        assert.equal(store.canWrite("win-a", "above", "gen-1"), false);
        const g = makeTransport();
        {
            const rotated = createNativePreimageStore(g.transport, true);
            rotated.seed("gen-1");
            seedOk(g, [fullRow("win-a")]);
            let done: boolean | null = null;
            rotated.flush([{ win: "win-a", property: "above" }], (ok) => {
                done = ok;
            });
            const w = g.lastCall("WriteNativePreimage");
            g.setGen("gen-2");
            w.callback(JSON.stringify({ v: 1, correlation_id: requestCorrelation(w), outcome: "stored", stored: 1 }));
            assert.equal(done, false);
            assert.equal(rotated.isLaneDisabled("win-a", "above"), false);
        }
    });

    it("seed rounds fence on nonce, generation, and stop", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        const first = f.lastCall("ReadNativePreimage");
        store.seed("gen-2");
        const second = f.lastCall("ReadNativePreimage");
        assert.notEqual(first, second);
        first.callback(okReply(requestCorrelation(first), [fullRow("win-a")]));
        assert.equal(store.isHydrated("gen-2"), false);
        f.setGen("gen-3");
        second.callback(okReply(requestCorrelation(second), [fullRow("win-a")]));
        assert.equal(store.isHydrated("gen-2"), false);
        f.setGen("gen-2");
        store.seed("gen-2");
        const third = f.lastCall("ReadNativePreimage");
        store.stop();
        third.callback(okReply(requestCorrelation(third), [fullRow("win-a")]));
        assert.equal(store.isHydrated("gen-2"), false);
    });

    it("queued lane cleared before dispatch cancels without sending", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        assert.equal(store.arm("win-b", { fullscreen: true }), true);
        const first: Array<boolean | null> = [null];
        const second: Array<boolean | null> = [null];
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            first[0] = ok;
        });
        store.flush([{ win: "win-b", property: "fullscreen" }], (ok) => {
            second[0] = ok;
        });
        // The queued lane is cleared before the first ack pumps the queue:
        // the first request still attests its intact lane, the queued one
        // cancels without ever dispatching and without disabling.
        assert.equal(store.clear("win-b", "fullscreen"), true);
        const write = f.lastCall("WriteNativePreimage");
        write.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(write),
            outcome: "stored", stored: 2,
        }));
        assert.equal(first[0], true);
        assert.equal(second[0], false);
        assert.equal(f.countCalls("WriteNativePreimage"), 1);
        assert.equal(store.isLaneDisabled("win-b", "fullscreen"), false);
        assert.ok(f.logs.some((l) => l.includes("outcome=canceled") && l.includes("reason=lane-changed")), f.logs.join("\n"));
    });

    it("clear and rearm before ack denies authority to the stale request", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        let first: boolean | null = null;
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            first = ok;
        });
        // Restore housekeeping replaces the requested value before the ack:
        // the stored snapshot holds the original, but the live lane no
        // longer matches it, so no authority is granted and nothing disables.
        assert.equal(store.clear("win-a", "maximize_mode"), true);
        assert.equal(store.arm("win-a", { maximize_mode: 1 }), true);
        const write = f.lastCall("WriteNativePreimage");
        write.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(write),
            outcome: "stored", stored: 1,
        }));
        assert.equal(first, false);
        assert.deepEqual(store.recordFor("win-a"), {
            win: "win-a", maximize_mode: 1, fullscreen: null,
            desktops: null, stacking: null,
        });
        assert.equal(store.isLaneDisabled("win-a", "maximized"), false);
        // The fresh value authorizes through its own request.
        let second: boolean | null = null;
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            second = ok;
        });
        const next = f.lastCall("WriteNativePreimage");
        assert.notEqual(next, write);
        next.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(next),
            outcome: "stored", stored: 1,
        }));
        assert.equal(second, true);
    });

    it("unrelated arm before ack keeps the first attestation true", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        let first: boolean | null = null;
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            first = ok;
        });
        // An unrelated arm merges into the snapshot without touching the
        // requested lane, so its attestation still holds. The ack carries
        // the sent count (one row): the later arm rides the next write.
        assert.equal(store.arm("win-b", { fullscreen: false }), true);
        const write = f.lastCall("WriteNativePreimage");
        write.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(write),
            outcome: "stored", stored: 1,
        }));
        assert.equal(first, true);
    });

    it("failed lane cancels its queued twin without another dispatch", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        assert.equal(store.arm("win-b", { fullscreen: true }), true);
        const seen: Array<boolean | null> = [null, null, null];
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            seen[0] = ok;
        });
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            seen[1] = ok;
        });
        store.flush([{ win: "win-b", property: "fullscreen" }], (ok) => {
            seen[2] = ok;
        });
        const first = f.lastCall("WriteNativePreimage");
        first.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(first),
            outcome: "rejected", reason: "invalid-members",
        }));
        // The twin cancels on the fresh disabled mark without dispatching;
        // the unrelated lane still sends the full snapshot (which may
        // contain the failed lane's bytes without resuming it).
        assert.deepEqual(seen, [false, false, null]);
        assert.equal(f.countCalls("WriteNativePreimage"), 2);
        const third = f.lastCall("WriteNativePreimage");
        const rows = (JSON.parse(third.payload) as Record<string, unknown>)["records"] as unknown[];
        assert.equal(rows.length, 2);
        third.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(third),
            outcome: "stored", stored: 2,
        }));
        assert.deepEqual(seen, [false, false, true]);
        assert.equal(store.isLaneDisabled("win-a", "maximized"), true);
        assert.equal(store.canWrite("win-b", "fullscreen", "gen-1"), true);
    });

    it("missing, invalid, and disabled lanes refuse without dispatch", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        const refused: boolean[] = [];
        // Missing at enqueue: nothing armed.
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            refused.push(ok);
        });
        // Invalid lane list: no valid lane survives normalization.
        store.flush([{ win: "win-a", property: "bogus" }], (ok) => {
            refused.push(ok);
        });
        assert.deepEqual(refused, [false, false]);
        assert.equal(f.countCalls("WriteNativePreimage"), 0);
        assert.ok(f.logs.some((l) => l.includes("outcome=refused")), f.logs.join("\n"));
        // Disabled at enqueue after a timed-out request.
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        let timed: boolean | null = null;
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            timed = ok;
        });
        assert.equal(f.countCalls("WriteNativePreimage"), 1);
        f.fireDelay(2000);
        assert.equal(timed, false);
        let disabledCall: boolean | null = null;
        store.flush([{ win: "win-a", property: "maximize_mode" }], (ok) => {
            disabledCall = ok;
        });
        assert.equal(disabledCall, false);
        assert.equal(f.countCalls("WriteNativePreimage"), 1);
        // Empty lanes are housekeeping: the ack proves the sent snapshot
        // only and authorizes zero native writes.
        let kept: boolean | null = null;
        store.flush([], (ok) => {
            kept = ok;
        });
        assert.equal(f.countCalls("WriteNativePreimage"), 2);
        const write = f.lastCall("WriteNativePreimage");
        write.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(write),
            outcome: "stored", stored: 1,
        }));
        assert.equal(kept, true);
        assert.equal(store.isLaneDisabled("win-a", "maximized"), true);
    });

    it("stale provided generation reads unavailable", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, [fullRow("win-a")]);
        assert.equal(store.isAvailable("gen-1"), true);
        assert.equal(store.canWrite("win-a", "maximized", "gen-1"), true);
        f.setGen("gen-2");
        assert.equal(store.isAvailable("gen-1"), false);
        assert.equal(store.canWrite("win-a", "maximized", "gen-1"), false);
        assert.equal(store.isHydrated("gen-1"), true);
    });

    it("clear with an invalid property refuses without mutation", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, [fullRow("win-a")]);
        const before = store.snapshot();
        assert.equal(store.clear("win-a", "bogus" as unknown as NativePreimageProperty), false);
        assert.deepEqual(store.snapshot(), before);
        assert.deepEqual(store.disabledLanes(), []);
    });

    it("mixed valid and invalid lanes refuse without dispatch", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, []);
        assert.equal(store.arm("win-a", { maximize_mode: 3 }), true);
        // One bogus lane voids the whole attestation: the valid lane is
        // never dispatched, never attested, and nothing disables.
        let done: boolean | null = null;
        store.flush(
            [{ win: "win-a", property: "maximize_mode" }, { win: "win-a", property: "bogus" }],
            (ok) => {
                done = ok;
            },
        );
        assert.equal(done, false);
        assert.equal(f.countCalls("WriteNativePreimage"), 0);
        assert.equal(store.isLaneDisabled("win-a", "maximized"), false);
        assert.deepEqual(store.recordFor("win-a"), {
            win: "win-a", maximize_mode: 3, fullscreen: null,
            desktops: null, stacking: null,
        });
        assert.ok(f.logs.some((l) => l.includes("outcome=refused") && l.includes("reason=invalid-lanes")), f.logs.join("\n"));
        // Duplicate valid lanes still dedupe into one attested request.
        let dup: boolean | null = null;
        store.flush(
            [{ win: "win-a", property: "maximized" }, { win: "win-a", property: "maximize_mode" }],
            (ok) => {
                dup = ok;
            },
        );
        assert.equal(f.countCalls("WriteNativePreimage"), 1);
        const write = f.lastCall("WriteNativePreimage");
        write.callback(JSON.stringify({
            v: 1, correlation_id: requestCorrelation(write),
            outcome: "stored", stored: 1,
        }));
        assert.equal(dup, true);
    });

    it("uses only the preimage bus methods and logs no ids or bytes", () => {
        const f = makeTransport();
        const store = createNativePreimageStore(f.transport, true);
        store.seed("gen-1");
        seedOk(f, [fullRow("win-a")]);
        assert.equal(store.arm("win-b", { desktops: ["desk-1"] }), true);
        let done: boolean | null = null;
        store.flush([{ win: "win-b", property: "sticky" }], (ok) => {
            done = ok;
        });
        const w = f.lastCall("WriteNativePreimage");
        w.callback(JSON.stringify({ v: 1, correlation_id: requestCorrelation(w), outcome: "stored", stored: 2 }));
        assert.equal(done, true);
        for (const c of f.calls) {
            assert.ok(c.method === "ReadNativePreimage" || c.method === "WriteNativePreimage", c.method);
        }
        for (const line of f.logs) {
            assert.ok(!line.includes("win-a"), `id leaked: ${line}`);
            assert.ok(!line.includes("desk-1"), `payload leaked: ${line}`);
        }
    });
});
