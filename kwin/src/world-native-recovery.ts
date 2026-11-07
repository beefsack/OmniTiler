// Startup native recovery coordinator (staged seam, NOT production-invoked).
//
// Restores durable preimage ORIGINALS for touched slots only, after an exact
// live UUID join, then requires a FRESH later observation that matches the
// originals. Raw setter acceptance alone never completes.
//
// Rules:
// * Touched slots only; null untouched means no write. Exact axes (mode
//   bit0 vertical, bit1 horizontal), exact desktop memberships (empty means
//   all-desktops), coupled stacking (above first, then below only when
//   still different).
// * Desktop restore resolves EVERY stored id to exactly one live desktop
//   ref with a non-null unique id; missing/duplicate/null rejects the whole
//   slot with zero native calls, never a partial write. One assignment.
// * Freshness is strict: the observation watermark must be strictly newer
//   than the attempt watermark, the provided fences must exactly equal the
//   live fences, and a note arriving reentrantly during the slot's own
//   setter never clears. A no-op setter emits no homing signal, so after a
//   run with >=1 attempt the coordinator requests one fresh observation
//   (never reentrantly, fenced by gen/reset/stop); the entry answers with
//   invalidate plus dispatch, never fabricated evidence.
// * Exact async fences plus live re-resolution before EACH native call;
//   stale pre-call means zero attempts for that slot. One attempt per slot
//   per generation including refusal/throw; no blind retry, no fabricated
//   StateAttempt.
// * Completion clears the slot, then ONE housekeeping flush must attest the
//   cleared snapshot (exact stored ack, still current) before unblocking.
//   A refused clear, a failed ack, or a stale ack keeps the slot blocked
//   for the generation with no retry: later notes never re-flush.
// * Prune normalizes string live ids (braced or bare); any non-string or
//   invalid id aborts the prune. Drops only proved ended lifetimes and
//   persists the removal with one housekeeping flush. Held/incomplete
//   absence over-retains.
// * Seed failure or a missing store blocks lanes with usable tiling; no
//   retries. Bounded per-(win,slot) startup state; no history or counters.

import { compareU64, isCanonicalU64 } from "./world-seq";
import { normalizeNativeId } from "./native-id";
import type {
    NativePreimageProperty,
    NativePreimageStore,
} from "./world-native-preimage";

export type NativeRecoverySlot = NativePreimageProperty;

export type NativeRecoveryObserved =
    | number
    | boolean
    | ReadonlyArray<string>
    | { readonly above: boolean; readonly below: boolean }
    | null;

export interface NativeRecoveryFences {
    readonly generation: string | null;
    readonly settingsRev: string | null;
    readonly acceptedRev: string | null;
    readonly watermark: string;
    readonly clock: string;
    readonly stopped: boolean;
}

export interface NativeRecoveryDesktop {
    readonly id: string | null;
    readonly ref: object;
}

export interface NativeRecoveryHost {
    readonly readProp: (value: object, property: string) => unknown;
    readonly log: (message: string) => void;
    readonly roster: () => ReadonlyArray<{ readonly id: string | null; readonly ref: object }> | null;
    readonly isStillListed: (ref: object) => boolean;
    readonly readWindowId: (ref: object) => string | null;
    readonly readMaximizeMode: (ref: object) => number | null;
    readonly readFullscreen: (ref: object) => boolean | null;
    readonly readDesktops: (ref: object) => string[] | null;
    readonly readStacking: (ref: object) => { above: boolean; below: boolean } | null;
    readonly listDesktops: () => ReadonlyArray<NativeRecoveryDesktop> | null;
    readonly setMaximize: (ref: object, vertical: boolean, horizontal: boolean) => boolean;
    readonly setFullscreen: (ref: object, value: boolean) => boolean;
    readonly setDesktops: (ref: object, desktops: ReadonlyArray<object>) => boolean;
    readonly setKeepAbove: (ref: object, value: boolean) => boolean;
    readonly setKeepBelow: (ref: object, value: boolean) => boolean;
}

export interface NativeRecoveryDeps {
    readonly store: NativePreimageStore;
    readonly getFences: () => NativeRecoveryFences;
    readonly log: (message: string) => void;
    // Request one fresh observation after a restore run attempted >=1
    // native call. Invoked at most once per such run, never reentrantly
    // during a setter, fenced by generation/reset/stop. The entry answers
    // with a real invalidate plus dispatch (fresh capture, no-op or not),
    // never fabricated evidence or clock.
    readonly requestObservation: () => void;
}

export interface NativeRecoveryCoordinator {
    readonly seed: (gen: string | null) => void;
    readonly reset: (generation: string | null) => void;
    readonly prune: (live: ReadonlySet<string>) => void;
    readonly stop: () => void;
    readonly isBlocked: (win: unknown, property: string, gen: string | null) => boolean;
    readonly restorePending: (host: NativeRecoveryHost) => void;
    readonly noteObservation: (
        win: unknown, slot: string, observed: NativeRecoveryObserved,
        fences: NativeRecoveryFences,
    ) => void;
    readonly pendingCountForTest: () => number;
}

type SlotValue = number | boolean | string[] | { above: boolean; below: boolean };

interface RecoveryEntry {
    readonly win: string;
    readonly slot: NativeRecoverySlot;
    readonly want: SlotValue;
    attempted: boolean;
    attemptWm: string | null;
    invoking: boolean;
    cleared: boolean;
}

function isSlot(value: string): value is NativeRecoverySlot {
    return value === "maximize_mode" || value === "fullscreen" ||
        value === "desktops" || value === "stacking";
}

function laneKey(win: string, slot: NativeRecoverySlot): string {
    return `${win}\u0000${slot}`;
}

// Public unknown-id guard: only strings reach the normalizer, so numbers,
// null, or objects are never coerced into ids.
function asId(win: unknown): string | null {
    if (typeof win !== "string") {
        return null;
    }
    return normalizeNativeId(win);
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

function valuesEqual(slot: NativeRecoverySlot, live: SlotValue | null, want: SlotValue): boolean {
    if (live === null) {
        return false;
    }
    if (slot === "maximize_mode" || slot === "fullscreen") {
        return live === want;
    }
    if (slot === "desktops") {
        if (!Array.isArray(live) || !Array.isArray(want)) {
            return false;
        }
        return live.length === want.length && live.every((d, i) => d === (want as string[])[i]);
    }
    if (typeof live !== "object" || Array.isArray(live) ||
        typeof want !== "object" || Array.isArray(want)) {
        return false;
    }
    const l = live as { above: boolean; below: boolean };
    const w = want as { above: boolean; below: boolean };
    return l.above === w.above && l.below === w.below;
}

function asSlotValue(slot: NativeRecoverySlot, observed: NativeRecoveryObserved): SlotValue | null {
    switch (slot) {
        case "maximize_mode":
            return typeof observed === "number" && Number.isInteger(observed) &&
                observed >= 0 && observed <= 3 ? observed : null;
        case "fullscreen":
            return typeof observed === "boolean" ? observed : null;
        case "desktops": {
            if (!Array.isArray(observed)) {
                return null;
            }
            for (const m of observed) {
                if (typeof m !== "string") {
                    return null;
                }
            }
            return [...(observed as string[])];
        }
        case "stacking":
            return typeof observed === "object" && observed !== null && !Array.isArray(observed) &&
                typeof (observed as { above: unknown }).above === "boolean" &&
                typeof (observed as { below: unknown }).below === "boolean"
                ? { above: (observed as { above: boolean }).above, below: (observed as { below: boolean }).below }
                : null;
    }
}

function fencesEqual(a: NativeRecoveryFences, b: NativeRecoveryFences): boolean {
    return a.generation === b.generation &&
        a.settingsRev === b.settingsRev &&
        a.acceptedRev === b.acceptedRev &&
        a.watermark === b.watermark &&
        a.clock === b.clock;
}

export function createNativeRecoveryCoordinator(deps: NativeRecoveryDeps): NativeRecoveryCoordinator {
    const { store, log } = deps;
    const slots = new Map<string, RecoveryEntry>();
    let seedGen: string | null | undefined;
    let initGen: string | null | undefined;
    let stopped = false;
    // Observation-request batching: set when any setter runs, fired once
    // when the outermost restore run ends. Nested runs never fire.
    let restoreDepth = 0;
    let observationOwed = false;

    const readFences = (): NativeRecoveryFences | null => {
        try {
            return deps.getFences();
        } catch (error) {
            void error;
            return null;
        }
    };

    const usable = (): boolean => {
        if (stopped || seedGen === undefined) {
            return false;
        }
        try {
            return store.isAvailable(seedGen);
        } catch (error) {
            void error;
            return false;
        }
    };

    // One-time capture of the startup snapshot for the seed generation.
    // Read-only; false until the store holds a usable seed for the live
    // generation, so runtime arms can never precede the capture.
    const ensureInit = (): boolean => {
        if (initGen === seedGen) {
            return true;
        }
        if (!usable() || seedGen === undefined) {
            return false;
        }
        let rows: ReturnType<NativePreimageStore["snapshot"]>;
        try {
            rows = store.snapshot();
        } catch (error) {
            void error;
            return false;
        }
        slots.clear();
        for (const row of rows) {
            const id = normalizeNativeId(row.win);
            if (id === null) {
                continue;
            }
            const touched: Array<{ slot: NativeRecoverySlot; want: SlotValue }> = [];
            if (row.maximize_mode !== null) {
                touched.push({ slot: "maximize_mode", want: row.maximize_mode });
            }
            if (row.fullscreen !== null) {
                touched.push({ slot: "fullscreen", want: row.fullscreen });
            }
            if (row.desktops !== null) {
                touched.push({ slot: "desktops", want: [...row.desktops] });
            }
            if (row.stacking !== null) {
                touched.push({ slot: "stacking", want: { ...row.stacking } });
            }
            for (const t of touched) {
                slots.set(laneKey(id, t.slot), {
                    win: id, slot: t.slot, want: copyValue(t.want),
                    attempted: false, attemptWm: null, invoking: false,
                    cleared: false,
                });
            }
        }
        initGen = seedGen;
        log(`omnitiler:world:native-recovery-seed outcome=ready pending=${slots.size}`);
        return true;
    };

    const resolveLive = (host: NativeRecoveryHost, win: string): object | null => {
        let roster: ReturnType<NativeRecoveryHost["roster"]>;
        try {
            roster = host.roster();
        } catch (error) {
            void error;
            return null;
        }
        if (roster === null) {
            return null;
        }
        let found: object | null = null;
        let count = 0;
        for (const entry of roster) {
            if (entry.id === win) {
                count += 1;
                found = entry.ref;
            } else if (entry.id === null) {
                return null;
            }
        }
        if (count !== 1 || found === null) {
            return null;
        }
        let id: string | null = null;
        try {
            id = host.readWindowId(found);
        } catch (error) {
            void error;
            return null;
        }
        if (id !== win) {
            return null;
        }
        try {
            if (!host.isStillListed(found)) {
                return null;
            }
        } catch (error) {
            void error;
            return null;
        }
        return found;
    };

    // Resolve EVERY stored desktop id to exactly one live desktop ref with
    // a non-null unique id. Missing, duplicate, or null rejects the whole
    // slot: no partial writes. Empty resolves to no refs (sticky).
    const resolveDesktops = (host: NativeRecoveryHost, want: string[]): object[] | null => {
        let all: ReturnType<NativeRecoveryHost["listDesktops"]>;
        try {
            all = host.listDesktops();
        } catch (error) {
            void error;
            return null;
        }
        if (all === null) {
            return null;
        }
        const refs: object[] = [];
        const used = new Set<object>();
        for (const id of want) {
            let found: object | null = null;
            let count = 0;
            for (const d of all) {
                if (d.id === null) {
                    return null;
                }
                let norm: string | null = null;
                try {
                    norm = normalizeNativeId(d.id);
                } catch (error) {
                    void error;
                    return null;
                }
                if (norm === id) {
                    count += 1;
                    found = d.ref;
                }
            }
            if (count !== 1 || found === null || used.has(found)) {
                return null;
            }
            used.add(found);
            refs.push(found);
        }
        return refs;
    };

    const methodMissing = (host: NativeRecoveryHost, ref: object, slot: NativeRecoverySlot): boolean => {
        try {
            switch (slot) {
                case "maximize_mode":
                    return typeof host.readProp(ref, "setMaximize") !== "function";
                case "fullscreen":
                    return host.readProp(ref, "fullScreen") === undefined;
                case "desktops":
                    return host.readProp(ref, "desktops") === undefined;
                case "stacking":
                    return host.readProp(ref, "keepAbove") === undefined;
            }
        } catch (error) {
            void error;
            return true;
        }
    };

    const restorePending = (host: NativeRecoveryHost): void => {
        if (stopped || !ensureInit()) {
            return;
        }
        // Per-run snapshot bound: currency is decided per call, never
        // frozen at init, so a settings change cancels stale runs without
        // deadlocking later ones.
        const first = readFences();
        if (first === null || stopped || first.stopped ||
            first.generation === null || first.generation !== seedGen ||
            first.settingsRev === null || first.acceptedRev === null ||
            !isCanonicalU64(first.clock)) {
            return;
        }
        const runGen = seedGen;
        restoreDepth += 1;
        try {
        const work = [...slots.values()].filter((e) => !e.attempted && !e.cleared && !e.invoking);
        for (const entry of work) {
            const key = laneKey(entry.win, entry.slot);
            if (slots.get(key) !== entry || entry.attempted || entry.cleared || entry.invoking) {
                continue;
            }
            const fences = readFences();
            if (fences === null || stopped || fences.stopped ||
                !fencesEqual(fences, first) || !isCanonicalU64(fences.clock)) {
                continue;
            }
            const live = resolveLive(host, entry.win);
            if (live === null) {
                continue;
            }
            if (methodMissing(host, live, entry.slot)) {
                log(`omnitiler:world:native-recovery-dropped slot=${entry.slot} reason=unsupported`);
                continue;
            }
            let desktops: object[] | null = null;
            if (entry.slot === "desktops") {
                desktops = resolveDesktops(host, entry.want as string[]);
                if (desktops === null) {
                    continue;
                }
            }
            entry.attempted = true;
            entry.attemptWm = fences.watermark;
            entry.invoking = true;
            observationOwed = true;
            let outcome: "accepted" | "refused" | "threw" = "refused";
            try {
                switch (entry.slot) {
                    case "maximize_mode": {
                        const mode = entry.want as number;
                        outcome = host.setMaximize(live, (mode & 1) !== 0, (mode & 2) !== 0)
                            ? "accepted" : "refused";
                        break;
                    }
                    case "fullscreen":
                        outcome = host.setFullscreen(live, entry.want as boolean)
                            ? "accepted" : "refused";
                        break;
                    case "desktops":
                        outcome = host.setDesktops(live, desktops as object[])
                            ? "accepted" : "refused";
                        break;
                    case "stacking": {
                        const want = entry.want as { above: boolean; below: boolean };
                        let firstOk = false;
                        try {
                            firstOk = host.setKeepAbove(live, want.above);
                        } catch (error) {
                            void error;
                            outcome = "threw";
                            break;
                        }
                        if (!firstOk) {
                            outcome = "refused";
                            break;
                        }
                        outcome = "accepted";
                        // Coupled pair second subcall: a same-generation
                        // reset, prune, or stop drops the entry, so the
                        // guard below lapses the second call while the
                        // first call's accepted fact stands.
                        if (slots.get(key) !== entry) {
                            break;
                        }
                        const second = readFences();
                        if (second === null || stopped || second.stopped ||
                            !fencesEqual(second, first) || !isCanonicalU64(second.clock)) {
                            break;
                        }
                        const fresh = resolveLive(host, entry.win);
                        if (fresh === null) {
                            break;
                        }
                        let cur: { above: boolean; below: boolean } | null = null;
                        try {
                            cur = host.readStacking(fresh);
                        } catch (error) {
                            void error;
                            cur = null;
                        }
                        if (cur !== null && cur.below !== want.below) {
                            try {
                                if (!host.setKeepBelow(fresh, want.below)) {
                                    outcome = "refused";
                                }
                            } catch (error) {
                                void error;
                                outcome = "threw";
                            }
                        }
                        break;
                    }
                }
            } catch (error) {
                void error;
                outcome = "threw";
            } finally {
                entry.invoking = false;
            }
            log(`omnitiler:world:native-recovery-restored slot=${entry.slot} outcome=${outcome}`);
        }
        } finally {
            restoreDepth -= 1;
        }
        // One fresh-observation request per batch with >=1 attempt, never
        // reentrantly during a setter: only the outermost run fires, after
        // its loop, fenced by generation/reset/stop. A no-op setter emits
        // no homing signal, so without this the strictly-newer watermark
        // gate could never complete.
        if (restoreDepth === 0 && observationOwed) {
            observationOwed = false;
            const live = readFences();
            if (live !== null && !stopped && !live.stopped &&
                live.generation !== null && live.generation === runGen &&
                live.generation === seedGen) {
                try {
                    deps.requestObservation();
                } catch (error) {
                    void error;
                }
            }
        }
    };

    const noteObservation = (
        win: unknown, slot: string, observed: NativeRecoveryObserved,
        fences: NativeRecoveryFences,
    ): void => {
        const id = asId(win);
        if (id === null || !isSlot(slot) || stopped || fences.stopped) {
            return;
        }
        const key = laneKey(id, slot);
        const entry = slots.get(key);
        // Observe-before-restore never completes, and a note arriving
        // reentrantly during the slot's own setter never clears: the
        // semantic gate is observed equality strictly AFTER invocation.
        if (entry === undefined || !entry.attempted || entry.invoking) {
            return;
        }
        // Single housekeeping attempt per lane: once cleared (flushed or
        // owed), later notes never re-flush. A missing/failed/stale ack
        // leaves the lane blocked for the generation with no retry.
        // Currency comes from the live fences, never the caller's word
        // alone: the provided tuple must exactly equal the live tuple.
        const live = readFences();
        if (live === null || stopped || live.stopped || !fencesEqual(fences, live) ||
            live.generation === null || live.generation !== seedGen ||
            live.settingsRev === null || live.acceptedRev === null ||
            !isCanonicalU64(live.clock)) {
            return;
        }
        // Strictly later: the same watermark (including a preexisting
        // observation presented after invocation) never completes.
        if (entry.attemptWm === null || !isCanonicalU64(entry.attemptWm) ||
            !isCanonicalU64(fences.watermark) ||
            compareU64(fences.watermark, entry.attemptWm) <= 0) {
            return;
        }
        const value = asSlotValue(slot, observed);
        if (!valuesEqual(slot, value, entry.want)) {
            return;
        }
        // Single housekeeping attempt per lane: once cleared, later notes
        // never re-flush. A missing/failed/stale ack leaves the lane
        // blocked for the generation with no retry.
        if (entry.cleared) {
            return;
        }
        let clearedOk = false;
        try {
            clearedOk = store.clear(id, slot);
        } catch (error) {
            void error;
            clearedOk = false;
        }
        if (!clearedOk) {
            return;
        }
        entry.cleared = true;
        log(`omnitiler:world:native-recovery-cleared slot=${slot} outcome=cleared`);
        const frozenGen = seedGen;
        const frozenSettings = live.settingsRev;
        const clearWm = fences.watermark;
        try {
            store.flush([], (ok) => {
                if (!ok) {
                    return;
                }
                const current = readFences();
                if (current === null || stopped || current.stopped ||
                    current.generation !== frozenGen ||
                    current.settingsRev !== frozenSettings) {
                    return;
                }
                if (!isCanonicalU64(current.watermark) ||
                    compareU64(current.watermark, clearWm) < 0) {
                    return;
                }
                if (!usable()) {
                    return;
                }
                // The slot must still read cleared: a rearmed slot never
                // unblocks on a stale ack.
                let rec: ReturnType<NativePreimageStore["recordFor"]>;
                try {
                    rec = store.recordFor(id);
                } catch (error) {
                    void error;
                    return;
                }
                if (rec !== undefined) {
                    const v = rec[slot];
                    if (v !== null) {
                        return;
                    }
                }
                if (slots.get(key) !== entry) {
                    return;
                }
                slots.delete(key);
                log(`omnitiler:world:native-recovery-complete slot=${slot} outcome=stored`);
            });
        } catch (error) {
            void error;
        }
    };

    return {
        seed: (gen: string | null): void => {
            if (stopped || seedGen === gen) {
                return;
            }
            slots.clear();
            seedGen = gen;
            initGen = undefined;
            try {
                ensureInit();
            } catch (error) {
                void error;
            }
        },
        reset: (generation: string | null): void => {
            slots.clear();
            seedGen = generation;
            initGen = undefined;
        },
        prune: (live: ReadonlySet<string>): void => {
            // Normalize string live ids (braced or bare) at the boundary.
            // Any non-string or invalid id aborts: never infer an ended
            // lifetime from a malformed live list.
            const proven = new Set<string>();
            for (const id of live) {
                if (typeof id !== "string") {
                    return;
                }
                const norm = normalizeNativeId(id);
                if (norm === null) {
                    return;
                }
                proven.add(norm);
            }
            let cleared = 0;
            for (const [key, entry] of [...slots]) {
                if (!proven.has(entry.win)) {
                    slots.delete(key);
                    try {
                        if (store.clear(entry.win)) {
                            cleared += 1;
                        }
                    } catch (error) {
                        void error;
                    }
                }
            }
            // Persist the ended-lifetime removal: the ack proves the
            // pruned snapshot durable. Log-only completion; a failed ack
            // leaves memory dropped with the next flush covering bytes.
            if (cleared > 0) {
                try {
                    store.flush([], (ok) => {
                        log(`omnitiler:world:native-recovery-prune outcome=${ok ? "stored" : "failed"} cleared=${cleared}`);
                    });
                } catch (error) {
                    void error;
                }
            }
        },
        stop: (): void => {
            stopped = true;
            slots.clear();
        },
        isBlocked: (win: unknown, property: string, gen: string | null): boolean => {
            const id = asId(win);
            if (id === null || !isSlot(property) || stopped) {
                return true;
            }
            if (seedGen === undefined || gen !== seedGen) {
                return true;
            }
            try {
                if (!ensureInit() || !usable()) {
                    return true;
                }
            } catch (error) {
                void error;
                return true;
            }
            return slots.has(laneKey(id, property));
        },
        restorePending,
        noteObservation,
        pendingCountForTest: () => slots.size,
    };
}
