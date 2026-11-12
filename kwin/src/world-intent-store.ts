// Opaque intent store controller (native membership transport only, never policy).
//
// The core owns the inner `data` bytes; this controller stores them unchanged,
// never interprets inner JSON, and never synthesizes membership. One bounded
// ReadFloatIntent per generation hydrates the verified durable map before the
// first admissible report; every accepted core `intentDesired` row replaces
// that window's latest target (string means exact saved bytes, null means
// explicit desired slot absence, missing rows carry no storage authority and
// never delete). The reconciler compares the latest desired target against
// verified durable storage and persists the converged full snapshot through
// WriteFloatIntent only on difference; equality means zero disk writes.
// Same Planner service, intent methods only.
//
// v1 membership payloads are unsupported with no conversion: a non-v2 store
// body degrades empty, the next covering write replaces. The byte contents
// under this controller are never logged: logs carry only the validated
// correlation (or `-`), op, outcome, record counts, and fixed reasons.
//
// Fences (never polling, never timer retry): every read/write carries a nonce
// plus the sending generation; a stopped controller, a superseded nonce, a
// rotated generation, or a foreign correlation drops the callback. Seed
// imports touch the durable map only and never overwrite the runtime desired
// target, so late imports cannot overwrite runtime choices. Any failed seed
// round (timeout, unreadable, corrupt, malformed, transport, degraded or
// rejected verdict, unknown outcome) clears the durable map to the
// server-empty snapshot (records: []); the next covering write replaces the
// server bytes. A late or failed write settles its completion exactly once
// and never authorizes dependent native writes. Replies are closed-shape per
// outcome with exact counts: ok requires stored === returned ===
// records.length, stored requires stored === sent rows; anything else
// settles degraded (seed) or false (write), never synthesized success.

import { isCorrelationId, isOpaqueId, utf8ByteLength, WORLD_MAX_REPLY_BYTES } from "./world-seq";
import { normalizeNativeId } from "./native-id";
import type { WorldIntentDesire } from "./world-report";

export const INTENT_STORE_VERSION = 2;
export const INTENT_READ_METHOD = "ReadFloatIntent";
export const INTENT_WRITE_METHOD = "WriteFloatIntent";
export const INTENT_STORE_TIMEOUT_MS = 2000;
export const INTENT_MAX_RECORDS = 10000;
export const INTENT_MAX_DATA_BYTES = 1024;

export interface WorldIntentStoreRoute {
    readonly service: string;
    readonly object: string;
    readonly iface: string;
}

export interface WorldIntentStoreTransport {
    readonly route: WorldIntentStoreRoute;
    readonly callDbus: (
        service: string, path: string, iface: string, method: string,
        payload: string, callback: (reply: unknown) => void,
    ) => void;
    readonly scheduleOnce: (delayMs: number, callback: () => void) => () => void;
    readonly log: (message: string) => void;
    readonly isStopped: () => boolean;
    readonly getGeneration: () => string | null;
    // Single kick when a seed round settles (imported or degraded): the entry
    // redispatches once, never a poll or a retry.
    readonly onHydrated: () => void;
}

export interface WorldIntentRecord {
    readonly win: string;
    readonly data: string;
}

export interface WorldIntentStore {
    readonly enabled: boolean;
    // True once the seed round for this exact generation settled (imported
    // or degraded). Disabled controllers read hydrated without bus traffic.
    readonly isHydrated: (gen: string | null) => boolean;
    // Fire one bounded read for this generation. Supersedes an outstanding
    // round; no-ops while that same round is outstanding. A new generation
    // clears the desired target; the durable map is replaced by the seed
    // verdict (imported or degraded-empty).
    readonly seed: (gen: string | null) => void;
    // Exact durable bytes for the live-joined report: only records whose
    // subject is in the captured native set ride. Out-of-scope records stay
    // stored.
    readonly recordsForReport: (live: ReadonlySet<string>) => WorldIntentRecord[];
    // Replace the latest desired target with the accepted reply's full
    // projection. Missing rows carry no authority and leave storage alone;
    // null is retained as an explicit desired absence. Ignores stale
    // generations, stopped controllers, and disabled controllers. True when
    // the desired map changed.
    readonly setDesired: (
        desires: ReadonlyArray<WorldIntentDesire>,
        gen: string | null,
    ) => boolean;
    // True when the latest desired target differs from verified durable
    // storage and a covering write is owed. Equality (including empty
    // desired) means zero disk writes.
    readonly needsWrite: () => boolean;
    // Persist the converged snapshot (durable plus desired applied;
    // out-of-scope durable records retained; desired null clears). The
    // completion reports explicit success exactly once: true only on a
    // bound `stored` reply, false on every failure, fence, or stop. On
    // success the durable map becomes the captured sent snapshot; newer
    // desired arrivals stay desired until the next covering write. Never
    // authorizes dependent native writes.
    readonly persist: (onSettled?: (ok: boolean) => void) => void;
    readonly stop: () => void;
}

function sanitizeCorrelation(value: string): string {
    return isCorrelationId(value) ? value : "-";
}

const WRITE_FAILURE_REASONS: ReadonlySet<string> = new Set([
    "invalid-request", "invalid-members", "unauthorized", "not-kwin-owner",
    "store-unavailable", "namespace-unavailable", "unavailable",
]);
const READ_DEGRADED_REASONS: ReadonlySet<string> = new Set([
    "namespace-unavailable", "unreadable", "corrupt", "namespace-mismatch",
    ...WRITE_FAILURE_REASONS,
]);

function fixedReason(value: unknown, fallback: string): string {
    if (typeof value === "string" && value.length > 0 && value.length <= 64 &&
        /^[a-z-]+$/.test(value)) {
        return value;
    }
    return fallback;
}

function hasExactKeys(o: Record<string, unknown>, keys: ReadonlyArray<string>): boolean {
    const got = Object.keys(o).sort();
    const want = [...keys].sort();
    return got.length === want.length && got.every((k, i) => k === want[i]);
}

function isRecordRow(value: unknown): value is { win: string; data: string } {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        return false;
    }
    const o = value as Record<string, unknown>;
    if (Object.keys(o).length !== 2 || typeof o["win"] !== "string" || typeof o["data"] !== "string") {
        return false;
    }
    return normalizeNativeId(o["win"]) !== null && utf8ByteLength(o["data"] as string) <= INTENT_MAX_DATA_BYTES;
}

export function createWorldIntentStore(transport: WorldIntentStoreTransport, enabled: boolean): WorldIntentStore {
    // Verified durable storage: the last seeded or successfully stored
    // snapshot. Desired target: the latest accepted core projection.
    const durable = new Map<string, string>();
    const desired = new Map<string, string | null>();
    // Last generation whose seed round settled (imported or degraded);
    // undefined before the first settlement.
    let hydratedGen: string | null | undefined;
    let seedNonce = 0;
    let seedOutstanding: number | null = null;
    let seedGen: string | null = null;
    let seedTimerCancel: (() => void) | null = null;
    let seedCorrelation = "-";
    let writeNonce = 0;
    interface WriteFlight {
        done: (ok: boolean) => void;
        cancelTimer: () => void;
        settled: boolean;
    }
    const pendingWrites = new Map<number, WriteFlight>();
    let stopped = false;

    const cancelSeedTimer = (): void => {
        try {
            seedTimerCancel?.();
        } catch (error) {
            void error;
        }
        seedTimerCancel = null;
    };

    const finishSeed = (nonce: number, settledGen: string | null, outcome: string, extra: string): void => {
        if (seedOutstanding !== nonce) {
            return;
        }
        seedOutstanding = null;
        cancelSeedTimer();
        hydratedGen = settledGen;
        transport.log(
            `omnitiler:world:intent-seed outcome=${outcome} correlation=${sanitizeCorrelation(seedCorrelation)}${extra}`,
        );
        try {
            transport.onHydrated();
        } catch (error) {
            void error;
        }
    };

    // Failed-seed clear: every failed/degraded seed round degrades the
    // durable map to empty. Desired targets are never touched: late imports
    // cannot overwrite runtime choices, and the next covering write
    // converges the surviving desired target onto the degraded base.
    const degradeClearSeed = (nonce: number, settledGen: string | null, reason: string): void => {
        durable.clear();
        finishSeed(nonce, settledGen, "degraded", ` reason=${reason} stored=0 returned=0`);
    };

    const seed = (gen: string | null): void => {
        if (!enabled || stopped || transport.isStopped()) {
            return;
        }
        if (seedOutstanding !== null && seedGen === gen) {
            return;
        }
        cancelSeedTimer();
        seedNonce += 1;
        const nonce = seedNonce;
        seedOutstanding = nonce;
        seedGen = gen;
        // A new generation owns a fresh desired target: late rows from the
        // dead generation never carry authority here.
        desired.clear();
        seedCorrelation = `intent-seed-${nonce}`;
        const payload = `{"v":${INTENT_STORE_VERSION},"correlation_id":${JSON.stringify(seedCorrelation)}}`;
        seedTimerCancel = transport.scheduleOnce(INTENT_STORE_TIMEOUT_MS, () => {
            seedTimerCancel = null;
            if (stopped || transport.isStopped() || seedOutstanding !== nonce) {
                return;
            }
            if (transport.getGeneration() !== gen) {
                return;
            }
            // No server verdict before the deadline: degrade durable empty
            // (desired survives), still settle so dispatch proceeds.
            degradeClearSeed(nonce, gen, "timeout");
        });
        try {
            transport.callDbus(
                transport.route.service, transport.route.object, transport.route.iface,
                INTENT_READ_METHOD, payload, (reply: unknown) => {
                    if (stopped || transport.isStopped() || seedOutstanding !== nonce) {
                        return;
                    }
                    if (transport.getGeneration() !== gen) {
                        return;
                    }
                    if (typeof reply !== "string" || utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        degradeClearSeed(nonce, gen, "unreadable");
                        return;
                    }
                    let value: unknown;
                    try {
                        value = JSON.parse(reply);
                    } catch (error) {
                        void error;
                        degradeClearSeed(nonce, gen, "corrupt");
                        return;
                    }
                    if (typeof value !== "object" || value === null || Array.isArray(value)) {
                        degradeClearSeed(nonce, gen, "corrupt");
                        return;
                    }
                    const r = value as Record<string, unknown>;
                    if (r["v"] !== INTENT_STORE_VERSION || typeof r["outcome"] !== "string") {
                        degradeClearSeed(nonce, gen, "corrupt");
                        return;
                    }
                    if (r["correlation_id"] !== seedCorrelation) {
                        if (typeof r["correlation_id"] === "string") {
                            // Foreign correlation: never this round's ack.
                            // Stay owed until timeout or supersede.
                            return;
                        }
                        degradeClearSeed(nonce, gen, "corrupt");
                        return;
                    }
                    const outcome = r["outcome"] as string;
                    if (outcome === "ok") {
                        if (!hasExactKeys(r, ["v", "correlation_id", "outcome", "records", "stored", "returned"])) {
                            degradeClearSeed(nonce, gen, "corrupt");
                            return;
                        }
                        const rows = r["records"];
                        if (!Array.isArray(rows) || rows.length > INTENT_MAX_RECORDS) {
                            degradeClearSeed(nonce, gen, "corrupt");
                            return;
                        }
                        const seen = new Set<string>();
                        const next = new Map<string, string>();
                        for (const row of rows) {
                            if (!isRecordRow(row)) {
                                degradeClearSeed(nonce, gen, "corrupt");
                                return;
                            }
                            const win = normalizeNativeId((row as { win: string }).win) as string;
                            if (seen.has(win)) {
                                degradeClearSeed(nonce, gen, "corrupt");
                                return;
                            }
                            seen.add(win);
                            next.set(win, (row as { data: string }).data);
                        }
                        // Closed counts: the server stores and returns the
                        // same snapshot, so both counts must equal the row
                        // count. Anything else is corrupt, never imported.
                        if (!Number.isInteger(r["stored"]) || (r["stored"] as number) !== next.size ||
                            !Number.isInteger(r["returned"]) || (r["returned"] as number) !== next.size) {
                            degradeClearSeed(nonce, gen, "corrupt");
                            return;
                        }
                        // Import durable only; the runtime desired target
                        // survives every seed verdict.
                        durable.clear();
                        for (const [win, data] of next) {
                            durable.set(win, data);
                        }
                        finishSeed(nonce, gen, "ok", ` stored=${next.size} returned=${next.size}`);
                        return;
                    }
                    if (outcome === "degraded") {
                        // Server verdict with an empty snapshot
                        // (read_degraded_reply): honor the bytes.
                        if (!hasExactKeys(r, ["v", "correlation_id", "outcome", "records", "stored", "returned", "reason"])) {
                            degradeClearSeed(nonce, gen, "corrupt");
                            return;
                        }
                        if (!Array.isArray(r["records"]) || (r["records"] as unknown[]).length !== 0 ||
                            r["stored"] !== 0 || r["returned"] !== 0) {
                            degradeClearSeed(nonce, gen, "corrupt");
                            return;
                        }
                        const reason = READ_DEGRADED_REASONS.has(fixedReason(r["reason"], ""))
                            ? (r["reason"] as string)
                            : "degraded";
                        degradeClearSeed(nonce, gen, reason);
                        return;
                    }
                    if (outcome === "rejected") {
                        // Server verdict (intent_rejection): empty snapshot.
                        if (!hasExactKeys(r, ["v", "correlation_id", "outcome", "records", "reason"])) {
                            degradeClearSeed(nonce, gen, "corrupt");
                            return;
                        }
                        if (!Array.isArray(r["records"]) || (r["records"] as unknown[]).length !== 0) {
                            degradeClearSeed(nonce, gen, "corrupt");
                            return;
                        }
                        const reason = READ_DEGRADED_REASONS.has(fixedReason(r["reason"], ""))
                            ? (r["reason"] as string)
                            : "rejected";
                        degradeClearSeed(nonce, gen, reason);
                        return;
                    }
                    degradeClearSeed(nonce, gen, "malformed");
                },
            );
        } catch (error) {
            void error;
            degradeClearSeed(nonce, gen, "transport");
        }
    };

    const desiredEqualsDurable = (): boolean => {
        for (const [win, data] of desired) {
            if (data === null) {
                if (durable.has(win)) {
                    return false;
                }
            } else if (durable.get(win) !== data) {
                return false;
            }
        }
        return true;
    };

    return {
        enabled,
        isHydrated: (gen: string | null): boolean => {
            if (!enabled) {
                return true;
            }
            return hydratedGen !== undefined && hydratedGen === gen;
        },
        seed,
        recordsForReport: (live: ReadonlySet<string>): WorldIntentRecord[] => {
            if (!enabled) {
                return [];
            }
            const out: WorldIntentRecord[] = [];
            for (const [win, data] of durable) {
                if (live.has(win)) {
                    out.push({ win, data });
                }
            }
            return out;
        },
        setDesired: (desires: ReadonlyArray<WorldIntentDesire>, gen: string | null): boolean => {
            if (!enabled || stopped || transport.isStopped()) {
                return false;
            }
            if (transport.getGeneration() !== gen) {
                return false;
            }
            const next = new Map<string, string | null>();
            for (const row of desires) {
                if (typeof row !== "object" || row === null) {
                    continue;
                }
                const win = normalizeNativeId((row as { win?: unknown }).win);
                if (win === null) {
                    continue;
                }
                const data = (row as { data?: unknown }).data;
                if (data === null) {
                    if (!next.has(win)) {
                        next.set(win, null);
                    }
                    continue;
                }
                if (typeof data !== "string" || utf8ByteLength(data) > INTENT_MAX_DATA_BYTES) {
                    continue;
                }
                if (!next.has(win)) {
                    next.set(win, data);
                }
            }
            if (next.size === desired.size) {
                let equal = true;
                for (const [win, data] of next) {
                    if (!desired.has(win) || desired.get(win) !== data) {
                        equal = false;
                        break;
                    }
                }
                if (equal) {
                    return false;
                }
            }
            desired.clear();
            for (const [win, data] of next) {
                desired.set(win, data);
            }
            return true;
        },
        needsWrite: (): boolean => {
            if (!enabled) {
                return false;
            }
            return !desiredEqualsDurable();
        },
        persist: (onSettled?: (ok: boolean) => void): void => {
            const done = (ok: boolean): void => {
                try {
                    onSettled?.(ok);
                } catch (error) {
                    void error;
                }
            };
            if (!enabled) {
                done(true);
                return;
            }
            if (stopped || transport.isStopped()) {
                done(false);
                return;
            }
            const gen = transport.getGeneration();
            // Converged snapshot: durable base plus the latest desired
            // target applied (string replaces, null clears). Out-of-scope
            // durable records are retained; desired rows without authority
            // were never added to the map.
            const merged = new Map<string, string>(durable);
            for (const [win, data] of desired) {
                if (data === null) {
                    merged.delete(win);
                } else {
                    merged.set(win, data);
                }
            }
            const wins = [...merged.keys()].sort();
            const rows: string[] = [];
            for (const win of wins) {
                const data = merged.get(win) as string;
                if (!isOpaqueId(win) || utf8ByteLength(data) > INTENT_MAX_DATA_BYTES) {
                    continue;
                }
                rows.push(`{"win":${JSON.stringify(win)},"data":${JSON.stringify(data)}}`);
            }
            // Captured snapshot for the completion: on `stored` success the
            // durable map becomes exactly this, and the latest desired
            // target is compared afterwards. Never claim a newer target
            // durable.
            const captured = new Map<string, string>();
            for (const win of wins) {
                const data = merged.get(win) as string;
                if (isOpaqueId(win) && utf8ByteLength(data) <= INTENT_MAX_DATA_BYTES) {
                    captured.set(win, data);
                }
            }
            writeNonce += 1;
            const nonce = writeNonce;
            const correlation = `intent-write-${nonce}`;
            const payload =
                `{"v":${INTENT_STORE_VERSION},"correlation_id":${JSON.stringify(correlation)},"records":[${rows.join(",")}]}`;
            if (utf8ByteLength(payload) > WORLD_MAX_REPLY_BYTES) {
                transport.log(
                    `omnitiler:world:intent-write outcome=rejected reason=oversize correlation=${sanitizeCorrelation(correlation)} stored=-`,
                );
                done(false);
                return;
            }
            const flight: WriteFlight = { done, cancelTimer: () => undefined, settled: false };
            pendingWrites.set(nonce, flight);
            const settle = (ok: boolean, line: string): void => {
                if (flight.settled) {
                    return;
                }
                flight.settled = true;
                try {
                    flight.cancelTimer();
                } catch (error) {
                    void error;
                }
                pendingWrites.delete(nonce);
                if (ok) {
                    durable.clear();
                    for (const [win, data] of captured) {
                        durable.set(win, data);
                    }
                }
                transport.log(line);
                try {
                    done(ok);
                } catch (error) {
                    void error;
                }
            };
            try {
                flight.cancelTimer = transport.scheduleOnce(INTENT_STORE_TIMEOUT_MS, () => {
                    settle(
                        false,
                        `omnitiler:world:intent-write outcome=timeout reason=timeout correlation=${sanitizeCorrelation(correlation)} stored=-`,
                    );
                });
            } catch (error) {
                void error;
                settle(false, `omnitiler:world:intent-write outcome=degraded reason=transport correlation=${sanitizeCorrelation(correlation)} stored=-`);
                return;
            }
            try {
                transport.callDbus(
                    transport.route.service, transport.route.object, transport.route.iface,
                    INTENT_WRITE_METHOD, payload, (reply: unknown) => {
                        if (stopped || transport.isStopped()) {
                            settle(false, `omnitiler:world:intent-write outcome=dropped reason=stopped correlation=${sanitizeCorrelation(correlation)} stored=-`);
                            return;
                        }
                        if (transport.getGeneration() !== gen) {
                            settle(false, `omnitiler:world:intent-write outcome=dropped reason=stale correlation=${sanitizeCorrelation(correlation)} stored=-`);
                            return;
                        }
                        if (typeof reply !== "string" || utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                            settle(false, `omnitiler:world:intent-write outcome=degraded reason=unreadable correlation=${sanitizeCorrelation(correlation)} stored=-`);
                            return;
                        }
                        let value: unknown;
                        try {
                            value = JSON.parse(reply);
                        } catch (error) {
                            void error;
                            settle(false, `omnitiler:world:intent-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(correlation)} stored=-`);
                            return;
                        }
                        if (typeof value !== "object" || value === null || Array.isArray(value)) {
                            settle(false, `omnitiler:world:intent-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(correlation)} stored=-`);
                            return;
                        }
                        const r = value as Record<string, unknown>;
                        if (r["v"] !== INTENT_STORE_VERSION || typeof r["outcome"] !== "string") {
                            settle(false, `omnitiler:world:intent-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(correlation)} stored=-`);
                            return;
                        }
                        if (r["correlation_id"] !== correlation) {
                            if (typeof r["correlation_id"] === "string") {
                                // Foreign correlation: never this write's
                                // ack. Stay owed until timeout or supersede.
                                return;
                            }
                            settle(false, `omnitiler:world:intent-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(correlation)} stored=-`);
                            return;
                        }
                        const outcome = r["outcome"] as string;
                        if (outcome === "stored") {
                            // Closed shape with the exact sent count: a
                            // missing or mismatched count never synthesizes
                            // success.
                            if (!hasExactKeys(r, ["v", "correlation_id", "outcome", "stored"])) {
                                settle(false, `omnitiler:world:intent-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(correlation)} stored=-`);
                                return;
                            }
                            if (typeof r["stored"] !== "number" || r["stored"] !== rows.length) {
                                settle(false, `omnitiler:world:intent-write outcome=degraded reason=stored-mismatch correlation=${sanitizeCorrelation(correlation)} stored=-`);
                                return;
                            }
                            settle(true, `omnitiler:world:intent-write outcome=stored correlation=${sanitizeCorrelation(correlation)} stored=${rows.length}`);
                            return;
                        }
                        if (outcome === "rejected" || outcome === "unavailable") {
                            // Closed failure shapes (write_failed_reply, or
                            // the shared intent_rejection with empty
                            // records): anything else settles false as
                            // malformed.
                            const shape4 = hasExactKeys(r, ["v", "correlation_id", "outcome", "reason"]);
                            const shape5 = hasExactKeys(r, ["v", "correlation_id", "outcome", "records", "reason"]) &&
                                Array.isArray(r["records"]) && (r["records"] as unknown[]).length === 0;
                            if (!shape4 && !shape5) {
                                settle(false, `omnitiler:world:intent-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(correlation)} stored=-`);
                                return;
                            }
                            const reason = WRITE_FAILURE_REASONS.has(fixedReason(r["reason"], ""))
                                ? (r["reason"] as string)
                                : outcome;
                            settle(false, `omnitiler:world:intent-write outcome=${outcome} reason=${reason} correlation=${sanitizeCorrelation(correlation)} stored=-`);
                            return;
                        }
                        settle(false, `omnitiler:world:intent-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(correlation)} stored=-`);
                    },
                );
            } catch (error) {
                void error;
                settle(false, `omnitiler:world:intent-write outcome=degraded reason=transport correlation=${sanitizeCorrelation(correlation)} stored=-`);
            }
        },
        stop: (): void => {
            stopped = true;
            cancelSeedTimer();
            seedOutstanding = null;
            for (const [nonce, flight] of [...pendingWrites]) {
                pendingWrites.delete(nonce);
                flight.settled = true;
                try {
                    flight.cancelTimer();
                } catch (error) {
                    void error;
                }
                try {
                    flight.done(false);
                } catch (error) {
                    void error;
                }
            }
        },
    };
}
