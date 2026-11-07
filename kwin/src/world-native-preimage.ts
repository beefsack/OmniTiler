// Native preimage store controller (native recovery transport only, never policy).
//
// The service owns the `native-preimage.json` slot; this controller carries
// coupled recovery records unchanged through strict v1 ReadNativePreimage /
// WriteNativePreimage calls. One record per live subject (`win`, normalized
// like `normalizeNativeId`) carries nullable per-property preimages; null
// means untouched, never written, never restored:
//
// * `maximize_mode`: exact KWin maximize axes (0 restore, 1 vertical,
//   2 horizontal, 3 full), not a bool.
// * `fullscreen`: exact fullScreen flag.
// * `desktops`: observed desktop memberships (null untouched, [] sticky/all).
// * `stacking`: paired keepAbove/keepBelow flags as read; both may be true.
//
// One bounded read seeds the map before anything else: arm, clear, and flush
// all refuse until the seed for the exact live generation settles usable, so
// no empty/full runtime snapshot can overwrite durable startup preimages. A
// failed seed disables the whole native lane surface (ordinary tiling/opaque
// intent stay viable), never retries. A failed flush disables only that
// request's lanes; unrelated lanes and later unrelated successes never resume
// a failed lane, and property clear preserves the disabled mark (only a
// window-gone cleanup releases that window's marks). Full-snapshot writes
// serialize through one FIFO keyed to the enqueue generation, carrying
// immutable copies of the requested values. Before send and after the exact
// stored ack, every requested lane must still hold its original value on the
// live seed generation; otherwise the request settles false without
// authority (a stored snapshot proves bytes landed, never permission for
// changed lanes). An empty lane list is housekeeping: its ack proves only
// the sent snapshot, authorizing zero native writes. Stale or changed queued
// entries never dispatch after a generation rotation or lane mutation. Logs carry only
// the validated correlation (or `-`), op, outcome, record counts, and fixed
// reasons: never ids, owners, bus ids, paths, or payloads. No native writes
// run here (prerequisite only).

import { isCorrelationId, utf8ByteLength, WORLD_MAX_REPLY_BYTES } from "./world-seq";
import { normalizeNativeId } from "./native-id";
import type { WorldStateProperty } from "./world-report";

export const NATIVE_PREIMAGE_VERSION = 1;
export const NATIVE_PREIMAGE_READ_METHOD = "ReadNativePreimage";
export const NATIVE_PREIMAGE_WRITE_METHOD = "WriteNativePreimage";
export const NATIVE_PREIMAGE_TIMEOUT_MS = 2000;
export const NATIVE_PREIMAGE_MAX_RECORDS = 10000;
export const NATIVE_PREIMAGE_MAX_DESKTOPS = 4096;

export interface NativePreimageRoute {
    readonly service: string;
    readonly object: string;
    readonly iface: string;
}

export interface NativePreimageTransport {
    readonly route: NativePreimageRoute;
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

export interface NativePreimageStacking {
    readonly above: boolean;
    readonly below: boolean;
}

export interface NativePreimageRecord {
    readonly win: string;
    readonly maximize_mode: number | null;
    readonly fullscreen: boolean | null;
    readonly desktops: ReadonlyArray<string> | null;
    readonly stacking: NativePreimageStacking | null;
}

export type NativePreimageProperty = "maximize_mode" | "fullscreen" | "desktops" | "stacking";

export interface NativePreimageLane {
    readonly win: string;
    readonly property: NativePreimageProperty;
}

export interface NativePreimageArm {
    readonly maximize_mode?: number | null | undefined;
    readonly fullscreen?: boolean | null | undefined;
    readonly desktops?: ReadonlyArray<string> | null | undefined;
    readonly stacking?: NativePreimageStacking | null | undefined;
}

export interface NativePreimageStore {
    readonly enabled: boolean;
    // True once the seed round for this exact generation settled (imported
    // or degraded). Disabled controllers read hydrated without bus traffic.
    readonly isHydrated: (gen: string | null) => boolean;
    // True only when the settled seed for the exact provided generation is
    // usable (namespace/store verified) and that generation is still the
    // live transport generation. Seed-level failure disables the whole
    // native lane surface; tiling/opaque intent stay viable.
    readonly isAvailable: (gen: string | null) => boolean;
    // Availability only, never write permission: the caller must still await
    // an exact correlated stored flush ack attesting its requested lanes and
    // revalidate authority before any dependent native write. False for a
    // provided generation that is not the live transport generation.
    readonly canWrite: (win: unknown, property: string, gen: string | null) => boolean;
    // Fire one bounded read for this generation. No-ops while that same
    // round is outstanding and after it settles (success or failure): no
    // blind read retry. A new generation supersedes and hides the old
    // settlement until it lands.
    readonly seed: (gen: string | null) => void;
    // Capture observed preimages. Refuses (false) before a usable seed for
    // the live generation, on disabled lanes, on invalid input, past the
    // record bound, or when stopped. First touch wins per property:
    // re-arming an armed slot keeps the original, never the mutated read.
    // Null/undefined slots are untouched and merge past. At least one
    // non-null slot is required.
    readonly arm: (win: string, partial: NativePreimageArm) => boolean;
    // Drop retained preimages; refuses like arm before a usable seed, and
    // refuses an invalid property with no mutation. A property clear is
    // restore housekeeping and preserves that lane's disabled mark. A
    // whole-window clear is lifetime cleanup for an
    // executor-established gone window and releases that window's marks.
    readonly clear: (win: string, property?: NativePreimageProperty) => boolean;
    // Retained full snapshot sorted by win, for startup recovery joins and
    // the next flush. Copies; the executor joins exact live UUIDs itself.
    readonly snapshot: () => NativePreimageRecord[];
    readonly recordFor: (win: string) => NativePreimageRecord | undefined;
    readonly disabledLanes: () => NativePreimageLane[];
    readonly isLaneDisabled: (win: string, property: string) => boolean;
    // Persist the full snapshot (retained rows for every window; empty
    // clears the slot). Refuses (false, no bus write) before a usable seed
    // for the live generation, when stopped, for any invalid requested lane
    // (duplicates dedupe), or when any requested lane is missing/null or
    // disabled at enqueue.
    // Serialized FIFO: concurrent calls queue under their enqueue
    // generation with immutable copies of the requested values and send the
    // then-current snapshot. Before send and after the exact correlated
    // `stored` ack, every requested lane must still hold its original
    // value, stay enabled, and stay on the live seed generation; otherwise
    // the request settles false (a stored snapshot proves the bytes landed,
    // never authority for changed lanes). The completion reports explicit
    // per-request success exactly once: true attests the requested lanes
    // remain stored and current. Failure disables only this request's
    // lanes. An empty lane list is housekeeping: its ack proves only the
    // sent snapshot, authorizing zero native writes.
    readonly flush: (
        lanes: ReadonlyArray<NativePreimageLane | { readonly win: string; readonly property: string }>,
        onSettled?: (ok: boolean) => void,
    ) => void;
    readonly stop: () => void;
}

// Mechanical map from the existing WorldStateProperty vocabulary to the
// preimage slot it arms: maximized is axes, sticky is desktop membership,
// above is the coupled stacking pair.
export function mapWorldStatePropertyToPreimage(property: WorldStateProperty): NativePreimageProperty {
    switch (property) {
        case "maximized":
            return "maximize_mode";
        case "fullscreen":
            return "fullscreen";
        case "sticky":
            return "desktops";
        case "above":
            return "stacking";
    }
}

function toPreimageProperty(value: string): NativePreimageProperty | null {
    if (value === "maximize_mode" || value === "fullscreen" ||
        value === "desktops" || value === "stacking") {
        return value;
    }
    if (value === "maximized" || value === "fullscreen" ||
        value === "sticky" || value === "above") {
        return mapWorldStatePropertyToPreimage(value as WorldStateProperty);
    }
    return null;
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

// Lane separator as an ASCII escape: opaque ids never contain NUL, so the
// join is collision-free and the file stays NUL-free.
const LANE_SEP = "\u0000";

interface RetainedSlots {
    maximize_mode: number | null;
    fullscreen: boolean | null;
    desktops: string[] | null;
    stacking: NativePreimageStacking | null;
}

// One non-null slot value, copied at flush enqueue for later attestation.
type SlotValue = number | boolean | string[] | NativePreimageStacking;

interface ExpectedLane {
    readonly win: string;
    readonly property: NativePreimageProperty;
    readonly value: SlotValue;
}

function isPreimageProperty(value: unknown): value is NativePreimageProperty {
    return value === "maximize_mode" || value === "fullscreen" ||
        value === "desktops" || value === "stacking";
}

function slotValue(cur: RetainedSlots, prop: NativePreimageProperty): SlotValue | null {
    switch (prop) {
        case "maximize_mode":
            return cur.maximize_mode;
        case "fullscreen":
            return cur.fullscreen;
        case "desktops":
            return cur.desktops;
        case "stacking":
            return cur.stacking;
    }
}

function copyValue(value: SlotValue): SlotValue {
    if (typeof value === "number" || typeof value === "boolean") {
        return value;
    }
    if (Array.isArray(value)) {
        return [...value];
    }
    return { above: value.above, below: value.below };
}

function valuesEqual(a: SlotValue, b: SlotValue): boolean {
    if (typeof a === "number" || typeof a === "boolean") {
        return a === b;
    }
    if (typeof b === "number" || typeof b === "boolean") {
        return false;
    }
    if (Array.isArray(a) || Array.isArray(b)) {
        return Array.isArray(a) && Array.isArray(b) &&
            a.length === b.length && a.every((v, i) => v === b[i]);
    }
    return a.above === (b as NativePreimageStacking).above &&
        a.below === (b as NativePreimageStacking).below;
}

function copyRecord(win: string, r: RetainedSlots): NativePreimageRecord {
    return {
        win,
        maximize_mode: r.maximize_mode,
        fullscreen: r.fullscreen,
        desktops: r.desktops === null ? null : [...r.desktops],
        stacking: r.stacking === null ? null : { above: r.stacking.above, below: r.stacking.below },
    };
}

// Strict v1 record codec, exactly matching the service wire/file shape:
// all five keys present, explicit nulls, modes 0..3, bool fullscreen, id
// memberships without duplicates, exact {above,below} stacking (both-true
// allowed), and at least one non-null slot. Null on any deviation.
function parseStoredRow(value: unknown): { win: string; slots: RetainedSlots } | null {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        return null;
    }
    const o = value as Record<string, unknown>;
    if (!hasExactKeys(o, ["win", "maximize_mode", "fullscreen", "desktops", "stacking"])) {
        return null;
    }
    const win = typeof o["win"] === "string" ? normalizeNativeId(o["win"]) : null;
    if (win === null) {
        return null;
    }
    let maximize_mode: number | null = null;
    const mm = o["maximize_mode"];
    if (mm !== null) {
        if (typeof mm !== "number" || !Number.isInteger(mm) || mm < 0 || mm > 3) {
            return null;
        }
        maximize_mode = mm;
    }
    let fullscreen: boolean | null = null;
    if (o["fullscreen"] !== null) {
        if (typeof o["fullscreen"] !== "boolean") {
            return null;
        }
        fullscreen = o["fullscreen"] as boolean;
    }
    let desktops: string[] | null = null;
    if (o["desktops"] !== null) {
        const ds = o["desktops"];
        if (!Array.isArray(ds) || ds.length > NATIVE_PREIMAGE_MAX_DESKTOPS) {
            return null;
        }
        const seen = new Set<string>();
        const ids: string[] = [];
        for (const m of ds) {
            if (typeof m !== "string") {
                return null;
            }
            const id = normalizeNativeId(m);
            if (id === null || seen.has(id)) {
                return null;
            }
            seen.add(id);
            ids.push(id);
        }
        desktops = ids;
    }
    let stacking: NativePreimageStacking | null = null;
    if (o["stacking"] !== null) {
        const st = o["stacking"];
        if (typeof st !== "object" || st === null || Array.isArray(st)) {
            return null;
        }
        const so = st as Record<string, unknown>;
        if (!hasExactKeys(so, ["above", "below"])) {
            return null;
        }
        if (typeof so["above"] !== "boolean" || typeof so["below"] !== "boolean") {
            return null;
        }
        stacking = { above: so["above"] as boolean, below: so["below"] as boolean };
    }
    const slots: RetainedSlots = { maximize_mode, fullscreen, desktops, stacking };
    if (slots.maximize_mode === null && slots.fullscreen === null &&
        slots.desktops === null && slots.stacking === null) {
        return null;
    }
    return { win, slots };
}

function encodeRow(win: string, r: RetainedSlots): string {
    const mm = r.maximize_mode === null ? "null" : String(r.maximize_mode);
    const fs = r.fullscreen === null ? "null" : (r.fullscreen ? "true" : "false");
    const ds = r.desktops === null ? "null" : `[${r.desktops.map((d) => JSON.stringify(d)).join(",")}]`;
    const st = r.stacking === null
        ? "null"
        : `{"above":${r.stacking.above ? "true" : "false"},"below":${r.stacking.below ? "true" : "false"}}`;
    return `{"win":${JSON.stringify(win)},"maximize_mode":${mm},"fullscreen":${fs},"desktops":${ds},"stacking":${st}}`;
}

export function createNativePreimageStore(
    transport: NativePreimageTransport, enabled: boolean,
): NativePreimageStore {
    const retained = new Map<string, RetainedSlots>();
    const disabled = new Set<string>();
    // Last generation whose seed round settled (imported or degraded);
    // undefined before the first settlement and during a reseed, so a new
    // generation never observes the old settlement mid-flight.
    let hydratedGen: string | null | undefined;
    let seedUsable = false;
    let seedNonce = 0;
    let seedOutstanding: number | null = null;
    let seedGen: string | null = null;
    let seedTimerCancel: (() => void) | null = null;
    let seedCorrelation = "-";
    let writeNonce = 0;
    interface WriteEntry {
        lanes: NativePreimageLane[];
        expected: ExpectedLane[];
        done: (ok: boolean) => void;
        cancelTimer: () => void;
        settled: boolean;
        sentRows: number;
        correlation: string;
        gen: string | null;
    }
    const writeQueue: WriteEntry[] = [];
    let writeActive: WriteEntry | null = null;
    let stopped = false;

    const laneKey = (win: string, property: NativePreimageProperty): string => `${win}${LANE_SEP}${property}`;

    // Seed gate shared by arm/clear/flush/canWrite: a usable seed for the
    // exact live generation on a live transport. Nothing mutates or
    // persists before startup recovery lands.
    const seedLive = (): boolean => {
        if (!enabled || stopped || transport.isStopped()) {
            return false;
        }
        const live = transport.getGeneration();
        return hydratedGen !== undefined && hydratedGen === live && seedUsable;
    };

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
            `omnitiler:world:preimage-seed outcome=${outcome} correlation=${sanitizeCorrelation(seedCorrelation)}${extra}`,
        );
        try {
            transport.onHydrated();
        } catch (error) {
            void error;
        }
    };

    const degradeSeed = (nonce: number, settledGen: string | null, reason: string): void => {
        retained.clear();
        seedUsable = false;
        finishSeed(nonce, settledGen, "degraded", ` reason=${reason} stored=0 returned=0`);
    };

    const seed = (gen: string | null): void => {
        if (!enabled || stopped || transport.isStopped()) {
            return;
        }
        if (seedOutstanding !== null && seedGen === gen) {
            return;
        }
        if (hydratedGen !== undefined && hydratedGen === gen) {
            // Settled for this generation already (imported or degraded):
            // never a blind read retry.
            return;
        }
        cancelSeedTimer();
        seedNonce += 1;
        const nonce = seedNonce;
        seedOutstanding = nonce;
        seedGen = gen;
        // Hide the old settlement for the reseed flight.
        hydratedGen = undefined;
        seedUsable = false;
        seedCorrelation = `preimage-seed-${nonce}`;
        const payload = `{"v":${NATIVE_PREIMAGE_VERSION},"correlation_id":${JSON.stringify(seedCorrelation)}}`;
        try {
            seedTimerCancel = transport.scheduleOnce(NATIVE_PREIMAGE_TIMEOUT_MS, () => {
                seedTimerCancel = null;
                if (stopped || transport.isStopped() || seedOutstanding !== nonce) {
                    return;
                }
                if (transport.getGeneration() !== gen) {
                    return;
                }
                degradeSeed(nonce, gen, "timeout");
            });
        } catch (error) {
            void error;
            degradeSeed(nonce, gen, "transport");
            return;
        }
        try {
            transport.callDbus(
                transport.route.service, transport.route.object, transport.route.iface,
                NATIVE_PREIMAGE_READ_METHOD, payload, (reply: unknown) => {
                    if (stopped || transport.isStopped() || seedOutstanding !== nonce) {
                        return;
                    }
                    if (transport.getGeneration() !== gen) {
                        return;
                    }
                    if (typeof reply !== "string" || utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        degradeSeed(nonce, gen, "unreadable");
                        return;
                    }
                    let value: unknown;
                    try {
                        value = JSON.parse(reply);
                    } catch (error) {
                        void error;
                        degradeSeed(nonce, gen, "corrupt");
                        return;
                    }
                    if (typeof value !== "object" || value === null || Array.isArray(value)) {
                        degradeSeed(nonce, gen, "corrupt");
                        return;
                    }
                    const r = value as Record<string, unknown>;
                    if (r["v"] !== NATIVE_PREIMAGE_VERSION || typeof r["outcome"] !== "string") {
                        degradeSeed(nonce, gen, "corrupt");
                        return;
                    }
                    if (r["correlation_id"] !== seedCorrelation) {
                        if (typeof r["correlation_id"] === "string") {
                            // Foreign correlation: never this round's ack.
                            // Stay owed until timeout or supersede.
                            return;
                        }
                        degradeSeed(nonce, gen, "corrupt");
                        return;
                    }
                    const outcome = r["outcome"] as string;
                    if (outcome === "ok") {
                        if (!hasExactKeys(r, ["v", "correlation_id", "outcome", "records", "stored", "returned"])) {
                            degradeSeed(nonce, gen, "corrupt");
                            return;
                        }
                        const raw = r["records"];
                        if (!Array.isArray(raw) || raw.length > NATIVE_PREIMAGE_MAX_RECORDS) {
                            degradeSeed(nonce, gen, "corrupt");
                            return;
                        }
                        const seen = new Set<string>();
                        const rows: Array<{ win: string; slots: RetainedSlots }> = [];
                        for (const row of raw) {
                            const parsed = parseStoredRow(row);
                            if (parsed === null || seen.has(parsed.win)) {
                                degradeSeed(nonce, gen, "corrupt");
                                return;
                            }
                            seen.add(parsed.win);
                            rows.push(parsed);
                        }
                        if (!Number.isInteger(r["stored"]) || (r["stored"] as number) !== rows.length ||
                            !Number.isInteger(r["returned"]) || (r["returned"] as number) !== rows.length) {
                            degradeSeed(nonce, gen, "corrupt");
                            return;
                        }
                        retained.clear();
                        for (const row of rows) {
                            retained.set(row.win, row.slots);
                        }
                        seedUsable = true;
                        finishSeed(nonce, gen, "ok", ` stored=${rows.length} returned=${rows.length}`);
                        return;
                    }
                    if (outcome === "degraded") {
                        if (!hasExactKeys(r, ["v", "correlation_id", "outcome", "records", "stored", "returned", "reason"])) {
                            degradeSeed(nonce, gen, "corrupt");
                            return;
                        }
                        if (!Array.isArray(r["records"]) || (r["records"] as unknown[]).length !== 0 ||
                            r["stored"] !== 0 || r["returned"] !== 0) {
                            degradeSeed(nonce, gen, "corrupt");
                            return;
                        }
                        const reason = READ_DEGRADED_REASONS.has(fixedReason(r["reason"], ""))
                            ? (r["reason"] as string)
                            : "degraded";
                        degradeSeed(nonce, gen, reason);
                        return;
                    }
                    if (outcome === "rejected") {
                        if (!hasExactKeys(r, ["v", "correlation_id", "outcome", "records", "reason"])) {
                            degradeSeed(nonce, gen, "corrupt");
                            return;
                        }
                        if (!Array.isArray(r["records"]) || (r["records"] as unknown[]).length !== 0) {
                            degradeSeed(nonce, gen, "corrupt");
                            return;
                        }
                        const reason = READ_DEGRADED_REASONS.has(fixedReason(r["reason"], ""))
                            ? (r["reason"] as string)
                            : "rejected";
                        degradeSeed(nonce, gen, reason);
                        return;
                    }
                    degradeSeed(nonce, gen, "malformed");
                },
            );
        } catch (error) {
            void error;
            degradeSeed(nonce, gen, "transport");
        }
    };

    const disableLanes = (lanes: ReadonlyArray<NativePreimageLane>): void => {
        for (const lane of lanes) {
            disabled.add(laneKey(lane.win, lane.property));
        }
    };

    // Entry still on the live seed generation it was enqueued under.
    const entryLive = (entry: WriteEntry): boolean => {
        if (!enabled || stopped || transport.isStopped()) {
            return false;
        }
        return hydratedGen !== undefined && hydratedGen === entry.gen &&
            entry.gen === transport.getGeneration() && seedUsable;
    };

    // First blocking reason for the immutable requested lanes, or null when
    // every lane still holds its enqueue value and stays enabled.
    const laneBlock = (entry: WriteEntry): string | null => {
        for (const exp of entry.expected) {
            if (disabled.has(laneKey(exp.win, exp.property))) {
                return "lane-disabled";
            }
            const cur = retained.get(exp.win);
            const v = cur === undefined ? null : slotValue(cur, exp.property);
            if (v === null || !valuesEqual(v, exp.value)) {
                return "lane-changed";
            }
        }
        return null;
    };

    // Settles one entry exactly once without pumping; callers pump after.
    const settleEntry = (entry: WriteEntry, ok: boolean, line: string, disableOnFail: boolean): void => {
        if (entry.settled) {
            return;
        }
        entry.settled = true;
        try {
            entry.cancelTimer();
        } catch (error) {
            void error;
        }
        if (writeActive === entry) {
            writeActive = null;
        }
        if (!ok && disableOnFail) {
            disableLanes(entry.lanes);
        }
        transport.log(line);
        try {
            entry.done(ok);
        } catch (error) {
            void error;
        }
    };

    const pumpWrites = (): void => {
        if (writeActive !== null) {
            return;
        }
        // Stale or changed queued entries never dispatch: after a generation
        // rotation, a lane clear/rearm, or a lane disable (possibly
        // reentrant through a just-run completion) they settle canceled
        // here, before any snapshot is built. No disable on cancel.
        while (writeQueue.length > 0) {
            const head = writeQueue[0] as WriteEntry;
            const live = entryLive(head);
            const blocked = live ? laneBlock(head) : null;
            if (live && blocked === null) {
                break;
            }
            const reason = !live ? "stale" : (blocked as string);
            writeQueue.shift();
            settleEntry(
                head,
                false,
                `omnitiler:world:preimage-write outcome=canceled reason=${reason} correlation=${sanitizeCorrelation(head.correlation)} stored=-`,
                false,
            );
        }
        if (stopped || transport.isStopped() || writeActive !== null || writeQueue.length === 0) {
            return;
        }
        const entry = writeQueue.shift() as WriteEntry;
        writeActive = entry;
        const wins = [...retained.keys()].sort();
        const rows = wins.map((win) => encodeRow(win, retained.get(win) as RetainedSlots));
        entry.sentRows = rows.length;
        const payload =
            `{"v":${NATIVE_PREIMAGE_VERSION},"correlation_id":${JSON.stringify(entry.correlation)},"records":[${rows.join(",")}]}`;
        if (utf8ByteLength(payload) > WORLD_MAX_REPLY_BYTES) {
            settleEntry(
                entry,
                false,
                `omnitiler:world:preimage-write outcome=rejected reason=oversize correlation=${sanitizeCorrelation(entry.correlation)} stored=-`,
                true,
            );
            pumpWrites();
            return;
        }
        try {
            entry.cancelTimer = transport.scheduleOnce(NATIVE_PREIMAGE_TIMEOUT_MS, () => {
                settleEntry(
                    entry,
                    false,
                    `omnitiler:world:preimage-write outcome=timeout reason=timeout correlation=${sanitizeCorrelation(entry.correlation)} stored=-`,
                    true,
                );
                pumpWrites();
            });
        } catch (error) {
            void error;
            settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=transport correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
            pumpWrites();
            return;
        }
        try {
            transport.callDbus(
                transport.route.service, transport.route.object, transport.route.iface,
                NATIVE_PREIMAGE_WRITE_METHOD, payload, (reply: unknown) => {
                    if (stopped || transport.isStopped()) {
                        settleEntry(entry, false, `omnitiler:world:preimage-write outcome=dropped reason=stopped correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, false);
                        pumpWrites();
                        return;
                    }
                    if (transport.getGeneration() !== entry.gen) {
                        settleEntry(entry, false, `omnitiler:world:preimage-write outcome=dropped reason=stale correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, false);
                        pumpWrites();
                        return;
                    }
                    if (typeof reply !== "string" || utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=unreadable correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
                        pumpWrites();
                        return;
                    }
                    let value: unknown;
                    try {
                        value = JSON.parse(reply);
                    } catch (error) {
                        void error;
                        settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
                        pumpWrites();
                        return;
                    }
                    if (typeof value !== "object" || value === null || Array.isArray(value)) {
                        settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
                        pumpWrites();
                        return;
                    }
                    const r = value as Record<string, unknown>;
                    if (r["v"] !== NATIVE_PREIMAGE_VERSION || typeof r["outcome"] !== "string") {
                        settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
                        pumpWrites();
                        return;
                    }
                    if (r["correlation_id"] !== entry.correlation) {
                        if (typeof r["correlation_id"] === "string") {
                            // Foreign correlation: never this write's ack.
                            // Stay owed until timeout or supersede.
                            return;
                        }
                        settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
                        pumpWrites();
                        return;
                    }
                    const outcome = r["outcome"] as string;
                    if (outcome === "stored") {
                        // Closed shape with the exact sent count: a missing
                        // or mismatched count never synthesizes success, and
                        // success never undisables previously failed lanes.
                        if (!hasExactKeys(r, ["v", "correlation_id", "outcome", "stored"])) {
                            settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
                            pumpWrites();
                            return;
                        }
                        if (typeof r["stored"] !== "number" || r["stored"] !== entry.sentRows) {
                            settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=stored-mismatch correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
                            pumpWrites();
                            return;
                        }
                        // The ack proves the sent snapshot landed. Authority
                        // needs more: every requested lane must still hold
                        // its immutable enqueue value on the live seed
                        // generation. A clear/rearm after send settles false
                        // even though the bytes stored; never authorize a
                        // rearmed or missing preimage. No disable here: the
                        // lane did not fail, the request just proves nothing.
                        if (!entryLive(entry)) {
                            settleEntry(entry, false, `omnitiler:world:preimage-write outcome=dropped reason=stale correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, false);
                            pumpWrites();
                            return;
                        }
                        const changed = laneBlock(entry);
                        if (changed !== null) {
                            settleEntry(entry, false, `omnitiler:world:preimage-write outcome=canceled reason=${changed} correlation=${sanitizeCorrelation(entry.correlation)} stored=${entry.sentRows}`, false);
                            pumpWrites();
                            return;
                        }
                        settleEntry(entry, true, `omnitiler:world:preimage-write outcome=stored correlation=${sanitizeCorrelation(entry.correlation)} stored=${entry.sentRows}`, false);
                        pumpWrites();
                        return;
                    }
                    if (outcome === "rejected" || outcome === "unavailable") {
                        // Service shapes: rejected rides the 5-key rejection
                        // or the 4-key failed reply (invalid-members);
                        // unavailable rides the 4-key failed reply only.
                        // Anything else settles false as malformed; every
                        // failure path disables only this request's lanes.
                        const shape4 = hasExactKeys(r, ["v", "correlation_id", "outcome", "reason"]);
                        const shape5 = hasExactKeys(r, ["v", "correlation_id", "outcome", "records", "reason"]) &&
                            Array.isArray(r["records"]) && (r["records"] as unknown[]).length === 0;
                        const shapeOk = outcome === "rejected" ? (shape4 || shape5) : shape4;
                        if (!shapeOk) {
                            settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
                            pumpWrites();
                            return;
                        }
                        const reason = WRITE_FAILURE_REASONS.has(fixedReason(r["reason"], ""))
                            ? (r["reason"] as string)
                            : outcome;
                        settleEntry(entry, false, `omnitiler:world:preimage-write outcome=${outcome} reason=${reason} correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
                        pumpWrites();
                        return;
                    }
                    settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=malformed correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
                    pumpWrites();
                },
            );
        } catch (error) {
            void error;
            settleEntry(entry, false, `omnitiler:world:preimage-write outcome=degraded reason=transport correlation=${sanitizeCorrelation(entry.correlation)} stored=-`, true);
            pumpWrites();
        }
    };

    return {
        enabled,
        isHydrated: (gen: string | null): boolean => {
            if (!enabled) {
                return true;
            }
            return hydratedGen !== undefined && hydratedGen === gen;
        },
        isAvailable: (gen: string | null): boolean => {
            if (!enabled || stopped || transport.isStopped()) {
                return false;
            }
            return hydratedGen !== undefined && hydratedGen === gen &&
                gen === transport.getGeneration() && seedUsable;
        },
        canWrite: (win: unknown, property: string, gen: string | null): boolean => {
            if (!enabled || stopped || transport.isStopped()) {
                return false;
            }
            if (hydratedGen === undefined || hydratedGen !== gen ||
                gen !== transport.getGeneration() || !seedUsable) {
                return false;
            }
            const id = normalizeNativeId(win);
            const prop = toPreimageProperty(property);
            if (id === null || prop === null) {
                return false;
            }
            return !disabled.has(laneKey(id, prop));
        },
        seed,
        arm: (win: string, partial: NativePreimageArm): boolean => {
            if (!seedLive()) {
                return false;
            }
            const id = normalizeNativeId(win);
            if (id === null || typeof partial !== "object" || partial === null) {
                return false;
            }
            let mode: number | null | undefined;
            if (partial.maximize_mode !== undefined && partial.maximize_mode !== null) {
                const m = partial.maximize_mode;
                if (typeof m !== "number" || !Number.isInteger(m) || m < 0 || m > 3) {
                    return false;
                }
                mode = m;
            }
            let full: boolean | null | undefined;
            if (partial.fullscreen !== undefined && partial.fullscreen !== null) {
                if (typeof partial.fullscreen !== "boolean") {
                    return false;
                }
                full = partial.fullscreen;
            }
            let desks: string[] | null | undefined;
            if (partial.desktops !== undefined && partial.desktops !== null) {
                const ds = partial.desktops;
                if (!Array.isArray(ds) || ds.length > NATIVE_PREIMAGE_MAX_DESKTOPS) {
                    return false;
                }
                const seen = new Set<string>();
                const ids: string[] = [];
                for (const m of ds) {
                    const nid = normalizeNativeId(m);
                    if (nid === null || seen.has(nid)) {
                        return false;
                    }
                    seen.add(nid);
                    ids.push(nid);
                }
                desks = ids;
            }
            let stack: NativePreimageStacking | null | undefined;
            if (partial.stacking !== undefined && partial.stacking !== null) {
                const st = partial.stacking;
                if (typeof st !== "object" || st === null ||
                    typeof st.above !== "boolean" || typeof st.below !== "boolean") {
                    return false;
                }
                stack = { above: st.above, below: st.below };
            }
            if (mode === undefined && full === undefined && desks === undefined && stack === undefined) {
                // All-null arm: the service rejects empty slots; refuse here.
                return false;
            }
            const props: NativePreimageProperty[] = [];
            if (mode !== undefined) {
                props.push("maximize_mode");
            }
            if (full !== undefined) {
                props.push("fullscreen");
            }
            if (desks !== undefined) {
                props.push("desktops");
            }
            if (stack !== undefined) {
                props.push("stacking");
            }
            for (const prop of props) {
                if (disabled.has(laneKey(id, prop))) {
                    return false;
                }
            }
            if (!retained.has(id) && retained.size >= NATIVE_PREIMAGE_MAX_RECORDS) {
                return false;
            }
            let cur = retained.get(id);
            if (cur === undefined) {
                cur = { maximize_mode: null, fullscreen: null, desktops: null, stacking: null };
                retained.set(id, cur);
            }
            let changed = false;
            if (mode !== undefined && cur.maximize_mode === null) {
                cur.maximize_mode = mode;
                changed = true;
            }
            if (full !== undefined && cur.fullscreen === null) {
                cur.fullscreen = full;
                changed = true;
            }
            if (desks !== undefined && cur.desktops === null) {
                cur.desktops = desks;
                changed = true;
            }
            if (stack !== undefined && cur.stacking === null) {
                cur.stacking = stack;
                changed = true;
            }
            return changed;
        },
        clear: (win: string, property?: NativePreimageProperty): boolean => {
            if (!seedLive()) {
                return false;
            }
            const id = normalizeNativeId(win);
            if (id === null) {
                return false;
            }
            if (property !== undefined && !isPreimageProperty(property)) {
                return false;
            }
            if (property === undefined) {
                // Lifetime cleanup for an executor-established gone window:
                // releases this window's disabled marks with its slots.
                const kept = retained.delete(id);
                let released = false;
                for (const prop of ["maximize_mode", "fullscreen", "desktops", "stacking"] as const) {
                    if (disabled.delete(laneKey(id, prop))) {
                        released = true;
                    }
                }
                return kept || released;
            }
            // Restore housekeeping: preserves the lane's disabled mark, so a
            // failed lane never resumes through a property clear.
            const cur = retained.get(id);
            if (cur === undefined || cur[property] === null) {
                return false;
            }
            cur[property] = null;
            if (cur.maximize_mode === null && cur.fullscreen === null &&
                cur.desktops === null && cur.stacking === null) {
                retained.delete(id);
            }
            return true;
        },
        snapshot: (): NativePreimageRecord[] => {
            const wins = [...retained.keys()].sort();
            return wins.map((w) => copyRecord(w, retained.get(w) as RetainedSlots));
        },
        recordFor: (win: string): NativePreimageRecord | undefined => {
            const id = normalizeNativeId(win);
            if (id === null) {
                return undefined;
            }
            const cur = retained.get(id);
            return cur === undefined ? undefined : copyRecord(id, cur);
        },
        disabledLanes: (): NativePreimageLane[] => {
            const out: NativePreimageLane[] = [];
            for (const key of disabled) {
                const sep = key.indexOf(LANE_SEP);
                if (sep < 0) {
                    continue;
                }
                out.push({ win: key.slice(0, sep), property: key.slice(sep + 1) as NativePreimageProperty });
            }
            out.sort((a, b) => (a.win < b.win ? -1 : a.win > b.win ? 1 : a.property < b.property ? -1 : 1));
            return out;
        },
        isLaneDisabled: (win: string, property: string): boolean => {
            const id = normalizeNativeId(win);
            const prop = toPreimageProperty(property);
            if (id === null || prop === null) {
                return false;
            }
            return disabled.has(laneKey(id, prop));
        },
        flush: (lanes, onSettled?: (ok: boolean) => void): void => {
            const done = (ok: boolean): void => {
                try {
                    onSettled?.(ok);
                } catch (error) {
                    void error;
                }
            };
            if (!enabled) {
                done(false);
                return;
            }
            if (stopped || transport.isStopped()) {
                done(false);
                return;
            }
            // Enqueue generation: the entry never relabels to a newer one.
            const gen = transport.getGeneration();
            if (hydratedGen === undefined || hydratedGen !== gen || !seedUsable) {
                done(false);
                return;
            }
            const clean: NativePreimageLane[] = [];
            const seen = new Set<string>();
            let invalid = false;
            for (const lane of lanes) {
                if (typeof lane !== "object" || lane === null) {
                    invalid = true;
                    break;
                }
                const id = normalizeNativeId((lane as { win?: unknown }).win);
                const raw = (lane as { property?: unknown }).property;
                const prop = toPreimageProperty(typeof raw === "string" ? raw : "");
                if (id === null || prop === null) {
                    invalid = true;
                    break;
                }
                const key = laneKey(id, prop);
                if (seen.has(key)) {
                    continue;
                }
                seen.add(key);
                clean.push({ win: id, property: prop });
            }
            writeNonce += 1;
            const correlation = `preimage-write-${writeNonce}`;
            const refuse = (reason: string): void => {
                transport.log(
                    `omnitiler:world:preimage-write outcome=refused reason=${reason} correlation=${sanitizeCorrelation(correlation)} stored=-`,
                );
                done(false);
            };
            if (invalid) {
                // Any invalid requested lane voids the whole attestation:
                // silently dropping it could callback true for lanes the
                // caller never meant to authorize.
                refuse("invalid-lanes");
                return;
            }
            // Immutable copies of the requested non-null values. The ack
            // later attests exactly these originals, never whatever the
            // lanes hold by then.
            const expected: ExpectedLane[] = [];
            for (const lane of clean) {
                if (disabled.has(laneKey(lane.win, lane.property))) {
                    refuse("lane-disabled");
                    return;
                }
                const cur = retained.get(lane.win);
                const v = cur === undefined ? null : slotValue(cur, lane.property);
                if (v === null) {
                    refuse("lane-missing");
                    return;
                }
                expected.push({ win: lane.win, property: lane.property, value: copyValue(v) });
            }
            writeQueue.push({
                lanes: clean,
                expected,
                done,
                cancelTimer: () => undefined,
                settled: false,
                sentRows: 0,
                correlation,
                gen,
            });
            pumpWrites();
        },
        stop: (): void => {
            stopped = true;
            cancelSeedTimer();
            seedOutstanding = null;
            if (writeActive !== null) {
                const active = writeActive;
                writeActive = null;
                settleEntry(
                    active,
                    false,
                    `omnitiler:world:preimage-write outcome=dropped reason=stopped correlation=${sanitizeCorrelation(active.correlation)} stored=-`,
                    false,
                );
            }
            while (writeQueue.length > 0) {
                const entry = writeQueue.shift() as WriteEntry;
                settleEntry(
                    entry,
                    false,
                    `omnitiler:world:preimage-write outcome=dropped reason=stopped correlation=${sanitizeCorrelation(entry.correlation)} stored=-`,
                    false,
                );
            }
        },
    };
}
