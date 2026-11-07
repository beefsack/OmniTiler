// Native StateTarget executor (production-wired through the world entry).
//
// Runs the four parsed `reply.windows[].state[]` targets (fullscreen,
// maximized, sticky, above) via the accepted preimage client: arm the
// property preimage, persist through one exact `stored` flush ack, then run
// the native setter. No production caller yet.
//
// Authority: only `controlled` + empty reason arms/stores/writes.
// Hold/untouched/missing rows take zero storage and zero native calls.
//
// Evidence: `recordStateAttempt` runs ONLY for an actual native invocation
// (accepted, refused, threw). Silent drops (unreadable, unavailable,
// failed persist, stale fences, lost authority/liveness, external race,
// missing method, unowned above release) record nothing and spend nothing.
//
// Spent markers (in-memory, per generation): one entry per (window,
// property) holding the last INVOKED target/cmdSeq identity. A different
// identity replaces; Hold/Untouched never clear. An accepted reply row for
// a listed window that lacks a property at ANY authority clears that
// property (even an empty state row), before the eligibility early-continue.
// Whole-window absence never clears; only prune/generation-reset/stop do.
// Caps/routes are generation-static and the above latch is terminal in
// place, so a cleared gap cannot return as a fresh Controlled command.
//
// Freeze-before-invoke: once all pre-call cancels pass, the pending lane is
// removed, rev/target/cmdSeq/clock are frozen in locals, and spent is set
// BEFORE the setter. A reentrant run sees spent (never pending) and cannot
// relabel the frozen attempt. Evidence uses the frozen identity; the outer
// never writes spent after the call, so it cannot overwrite newer spent.
// Throws are spent with raw facts.
//
// Above ownership changes ONLY on accepted results: an accepted acquire
// installs the durable pre-acquisition pair; an accepted release deletes
// it. Refused/threw acquires install nothing, so a later release is a
// silent unowned drop (no write/attempt/spent).
//
// Per-lane guards: generation/settings/rev/watermark fences, per-property
// authority, stable-id roster re-resolution before EACH call, the
// captured-pre guard (live slot must equal the freshly captured pre), and
// a canonical clock stamp. The above-false release revalidates fences,
// authority, and the live ref between its two native subcalls (pinned KWin
// window.cpp: setKeepAbove(true) clears below, so the first setter can run
// reentrant code); a lapsed second subcall keeps the first call's raw fact.
// Completion is fenced by generation/reset/stop (no old-generation record
// or ownership leak); prune and reentrant ownership changes gate ownership
// only, so same-generation evidence for an actual call stands. Paired
// fullscreen-false + maximize persists both then clears first with NO
// applied-fullscreen gate between; a non-accepted clear skips maximize with
// no fabricated attempt.

import { decodeList } from "./qml-list";
import { normalizeNativeId } from "./native-id";
import { readNativeAlias } from "./native-alias";
import { isCanonicalU64, isOpaqueId } from "./world-seq";
import {
    mapWorldStatePropertyToPreimage,
    type NativePreimageStore,
} from "./world-native-preimage";
import type { WorldAdapterState } from "./world-state";
import type {
    FrameSetter,
    WorldReply,
    WorldStateProperty,
} from "./world-report";

const MAX_ROSTER_WINDOWS = 10000;
const MAX_WINDOW_DESKTOPS = 4096;

const STATE_PROPERTIES: ReadonlyArray<WorldStateProperty> = [
    "fullscreen",
    "maximized",
    "sticky",
    "above",
];

function token(value: unknown): string {
    if (typeof value === "string" && value.length > 0 && value.length <= 64 &&
        /^[a-z0-9-]+$/.test(value)) {
        return value;
    }
    return "unknown";
}

export interface NativeStateFences {
    readonly generation: string | null;
    readonly settingsRev: string | null;
    readonly acceptedRev: string;
    readonly watermark: string;
    readonly clock: string;
    readonly stopped: boolean;
}

export interface NativeStateRosterEntry {
    readonly id: string | null;
    readonly ref: object;
}

export interface NativeStateHost {
    readonly surface: object;
    readonly readProp: (value: object, property: string) => unknown;
    readonly log: (message: string) => void;
    readonly roster: () => NativeStateRosterEntry[] | null;
    readonly isStillListed: (ref: object) => boolean;
    readonly readWindowId: (ref: object) => string | null;
    readonly readMaximizeMode: (ref: object) => number | null;
    readonly readFullscreen: (ref: object) => boolean | null;
    readonly readDesktops: (ref: object) => string[] | null;
    readonly readStacking: (ref: object) => { above: boolean; below: boolean } | null;
    readonly setMaximize: (ref: object, vertical: boolean, horizontal: boolean) => boolean;
    readonly setFullscreen: (ref: object, value: boolean) => boolean;
    readonly setSticky: (ref: object, value: boolean) => boolean;
    readonly setKeepAbove: (ref: object, value: boolean) => boolean;
    readonly setKeepBelow: (ref: object, value: boolean) => boolean;
}

export function createNativeStateHost(
    surface: object,
    readProp: (value: object, property: string) => unknown,
    log: (message: string) => void,
): NativeStateHost {
    const safe = <T>(fn: () => T): T | null => {
        try {
            return fn();
        } catch (error) {
            void error;
            return null;
        }
    };
    return {
        surface,
        readProp,
        log,
        roster: () => {
            try {
                const fn = readProp(surface, "windowList");
                if (typeof fn !== "function") {
                    return null;
                }
                const list = decodeList(
                    Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []),
                    MAX_ROSTER_WINDOWS,
                );
                if (list === null) {
                    return null;
                }
                return list.map((item) => {
                    if (typeof item !== "object" || item === null) {
                        return { id: null, ref: item as object };
                    }
                    let id: string | null = null;
                    try {
                        id = normalizeNativeId(readProp(item as object, "internalId"));
                    } catch (error) {
                        void error;
                        id = null;
                    }
                    return { id, ref: item as object };
                });
            } catch (error) {
                void error;
                return null;
            }
        },
        isStillListed: (ref) => {
            try {
                const fn = readProp(surface, "windowList");
                if (typeof fn !== "function") {
                    return false;
                }
                const list = decodeList(
                    Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []),
                    MAX_ROSTER_WINDOWS,
                );
                if (list === null) {
                    return false;
                }
                let want: string | null = null;
                try {
                    want = normalizeNativeId(readProp(ref, "internalId"));
                } catch (error) {
                    void error;
                    return false;
                }
                if (want === null) {
                    return false;
                }
                for (const item of list) {
                    if (typeof item !== "object" || item === null) {
                        continue;
                    }
                    let id: string | null = null;
                    try {
                        id = normalizeNativeId(readProp(item as object, "internalId"));
                    } catch (error) {
                        void error;
                        continue;
                    }
                    if (id === want) {
                        return true;
                    }
                }
                return false;
            } catch (error) {
                void error;
                return false;
            }
        },
        readWindowId: (ref) => {
            try {
                return normalizeNativeId(readProp(ref, "internalId"));
            } catch (error) {
                void error;
                return null;
            }
        },
        readMaximizeMode: (ref) => {
            const v = safe(() => readProp(ref, "maximizeMode"));
            return typeof v === "number" && Number.isInteger(v) && v >= 0 && v <= 3 ? v : null;
        },
        readFullscreen: (ref) => {
            const v = safe(() => readProp(ref, "fullScreen"));
            if (typeof v === "boolean") {
                return v;
            }
            const legacy = safe(() => readNativeAlias(readProp, ref, "fullscreen"));
            return typeof legacy === "boolean" ? legacy : null;
        },
        readDesktops: (ref) => {
            const sticky = safe(() => readProp(ref, "onAllDesktops"));
            if (typeof sticky === "boolean") {
                if (sticky) {
                    return [];
                }
            } else {
                const legacy = safe(() => readNativeAlias(readProp, ref, "sticky"));
                if (typeof legacy !== "boolean") {
                    return null;
                }
                if (legacy) {
                    return [];
                }
            }
            const members = safe(() => decodeList(readProp(ref, "desktops"), MAX_WINDOW_DESKTOPS));
            if (members === null) {
                return null;
            }
            const seen = new Set<string>();
            const ids: string[] = [];
            for (const m of members) {
                if (typeof m !== "object" || m === null) {
                    return null;
                }
                let id: string | null = null;
                try {
                    id = normalizeNativeId(readProp(m as object, "id"));
                } catch (error) {
                    void error;
                    return null;
                }
                if (id === null || seen.has(id)) {
                    return null;
                }
                seen.add(id);
                ids.push(id);
            }
            return ids;
        },
        readStacking: (ref) => {
            const above = safe(() => readProp(ref, "keepAbove"));
            const below = safe(() => readProp(ref, "keepBelow"));
            return typeof above === "boolean" && typeof below === "boolean"
                ? { above, below }
                : null;
        },
        setMaximize: (ref, vertical, horizontal) => {
            const method = readProp(ref, "setMaximize");
            if (typeof method !== "function") {
                return false;
            }
            Reflect.apply(method as (...a: ReadonlyArray<unknown>) => unknown, ref, [vertical, horizontal]);
            return true;
        },
        setFullscreen: (ref, value) => {
            if (readProp(ref, "fullScreen") === undefined) {
                return false;
            }
            return Reflect.set(ref, "fullScreen", value);
        },
        setSticky: (ref, value) => {
            if (readProp(ref, "onAllDesktops") === undefined) {
                return false;
            }
            return Reflect.set(ref, "onAllDesktops", value);
        },
        setKeepAbove: (ref, value) => {
            if (readProp(ref, "keepAbove") === undefined) {
                return false;
            }
            return Reflect.set(ref, "keepAbove", value);
        },
        setKeepBelow: (ref, value) => {
            if (readProp(ref, "keepBelow") === undefined) {
                return false;
            }
            return Reflect.set(ref, "keepBelow", value);
        },
    };
}

export interface NativeStateExecutorDeps {
    readonly state: WorldAdapterState;
    readonly store: NativePreimageStore;
    readonly host: NativeStateHost;
    readonly getFences: () => NativeStateFences;
    readonly isStillAuthorized: (win: string, property: WorldStateProperty) => boolean;
}

export interface NativeStateExecutor {
    readonly run: (reply: WorldReply) => void;
    readonly prune: (live: ReadonlySet<string>) => void;
    readonly reset: (generation: string | null) => void;
    readonly stop: () => void;
    readonly spentCountForTest: () => number;
}

type SlotNative = number | boolean | string[] | { above: boolean; below: boolean };

interface PendingLane {
    readonly win: string;
    readonly property: WorldStateProperty;
    readonly target: boolean;
    readonly cmdSeq: string | null;
    rev: string;
    gen: string | null;
    settingsRev: string | null;
    watermark: string;
    readonly capturedPre: SlotNative;
    readonly durablePair: { above: boolean; below: boolean } | null;
    readonly ownedPair: { above: boolean; below: boolean } | null;
    cancelled: boolean;
}

function laneKey(win: string, property: WorldStateProperty): string {
    return `${win}\u0000${property}`;
}

function cloneSlot(v: SlotNative): SlotNative {
    if (typeof v === "number" || typeof v === "boolean") {
        return v;
    }
    return Array.isArray(v) ? [...v] : { ...v };
}

function slotsEqual(property: WorldStateProperty, live: SlotNative | null, want: SlotNative): boolean {
    if (live === null) {
        return false;
    }
    if (property === "maximized" || property === "fullscreen") {
        return live === want;
    }
    if (property === "sticky") {
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

export function createNativeStateExecutor(deps: NativeStateExecutorDeps): NativeStateExecutor {
    const { state, store, host } = deps;
    const latest = new Map<string, { target: boolean; cmdSeq: string | null }>();
    const ownedAbove = new Map<string, { above: boolean; below: boolean }>();
    const pending = new Map<string, PendingLane>();
    // Completion fence (opaque object identity, no counters or history):
    // resetToken is replaced on reset/stop and gates post-setter evidence +
    // ownership. Prune marks an in-flight lane cancelled only when its own
    // window leaves the live set; the flag gates the second subcall and
    // post-setter ownership, while same-generation evidence for an actual
    // call stands. Reentrant ownership changes are detected by comparing
    // the actual ownedAbove.get(win) reference: every accepted acquisition
    // installs a fresh pair object and release deletes to undefined.
    let resetToken: object = {};
    const inFlight = new Set<PendingLane>();
    let boundGen: string | null | undefined;
    let stopped = false;

    const resolveLive = (win: string): object | null => {
        const roster = host.roster();
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

    const readSlot = (ref: object, property: WorldStateProperty): SlotNative | null => {
        try {
            switch (property) {
                case "maximized": {
                    const mode = host.readMaximizeMode(ref);
                    return mode === null ? null : mode;
                }
                case "fullscreen": {
                    const v = host.readFullscreen(ref);
                    return v === null ? null : v;
                }
                case "sticky": {
                    const ids = host.readDesktops(ref);
                    return ids === null ? null : [...ids];
                }
                case "above": {
                    const st = host.readStacking(ref);
                    return st === null ? null : { ...st };
                }
            }
        } catch (error) {
            void error;
            return null;
        }
    };

    const armPayload = (ref: object, property: WorldStateProperty)
        : { arm: Record<string, unknown>; original: SlotNative } | null => {
        const slot = readSlot(ref, property);
        if (slot === null) {
            return null;
        }
        switch (property) {
            case "maximized":
                return { arm: { maximize_mode: slot }, original: slot };
            case "fullscreen":
                return { arm: { fullscreen: slot }, original: slot };
            case "sticky":
                return { arm: { desktops: [...(slot as string[])] }, original: [...(slot as string[])] };
            case "above":
                return { arm: { stacking: { ...(slot as { above: boolean; below: boolean }) } }, original: { ...(slot as { above: boolean; below: boolean }) } };
        }
    };

    const storedOriginal = (win: string, property: WorldStateProperty): SlotNative | null => {
        const rec = store.recordFor(win);
        if (rec === undefined) {
            return null;
        }
        switch (property) {
            case "maximized":
                return rec.maximize_mode;
            case "fullscreen":
                return rec.fullscreen;
            case "sticky":
                return rec.desktops === null ? null : [...rec.desktops];
            case "above":
                return rec.stacking === null ? null : { ...rec.stacking };
        }
    };

    const evidencePre = (ref: object, property: WorldStateProperty): boolean | null => {
        try {
            switch (property) {
                case "maximized": {
                    const mode = host.readMaximizeMode(ref);
                    return mode === null ? null : mode !== 0;
                }
                case "fullscreen":
                    return host.readFullscreen(ref);
                case "sticky": {
                    const ids = host.readDesktops(ref);
                    return ids === null ? null : ids.length === 0;
                }
                case "above": {
                    const st = host.readStacking(ref);
                    return st === null ? null : st.above;
                }
            }
        } catch (error) {
            void error;
            return null;
        }
    };

    const methodMissing = (ref: object, property: WorldStateProperty): boolean => {
        try {
            switch (property) {
                case "maximized":
                    return typeof host.readProp(ref, "setMaximize") !== "function";
                case "fullscreen":
                    return host.readProp(ref, "fullScreen") === undefined;
                case "sticky":
                    return host.readProp(ref, "onAllDesktops") === undefined;
                case "above":
                    return host.readProp(ref, "keepAbove") === undefined;
            }
        } catch (error) {
            void error;
            return true;
        }
    };

    const dropped = (win: string, property: WorldStateProperty, reason: string): void => {
        host.log(
            `omnitiler:world:native-state-dropped target=${property} window=${token(win)} reason=${reason}`,
        );
    };

    const settle = (lane: PendingLane): void => {
        const key = laneKey(lane.win, lane.property);
        if (pending.get(key) === lane) {
            pending.delete(key);
        }
    };

    // Fresh authorization + live ref for the second native subcall of an
    // above-false release: the first setter may have run reentrant code that
    // changed fences, authority, or lifetime. Null blocks the second call;
    // the first call's raw fact stands and nothing is fabricated.
    const secondLive = (lane: PendingLane): object | null => {
        let fences: NativeStateFences;
        try {
            fences = deps.getFences();
        } catch (error) {
            void error;
            return null;
        }
        if (stopped || fences.stopped) {
            return null;
        }
        if (fences.generation === null || fences.generation !== lane.gen ||
            fences.settingsRev !== lane.settingsRev ||
            lane.rev !== fences.acceptedRev ||
            fences.watermark !== lane.watermark) {
            return null;
        }
        let auth = false;
        try {
            auth = deps.isStillAuthorized(lane.win, lane.property);
        } catch (error) {
            void error;
            auth = false;
        }
        if (!auth) {
            return null;
        }
        return resolveLive(lane.win);
    };

    const invoke = (
        ref: object,
        lane: PendingLane,
        target: boolean,
        saved: { above: boolean; below: boolean } | null,
    ): { setter: FrameSetter; post: boolean | null } => {
        const property = lane.property;
        try {
            switch (property) {
                case "maximized": {
                    let ok = false;
                    try {
                        ok = host.setMaximize(ref, target, target);
                    } catch (error) {
                        void error;
                        return { setter: "threw", post: evidencePre(ref, property) };
                    }
                    return { setter: ok ? "accepted" : "refused", post: evidencePre(ref, property) };
                }
                case "fullscreen": {
                    let ok = false;
                    try {
                        ok = host.setFullscreen(ref, target);
                    } catch (error) {
                        void error;
                        return { setter: "threw", post: evidencePre(ref, property) };
                    }
                    return { setter: ok ? "accepted" : "refused", post: evidencePre(ref, property) };
                }
                case "sticky": {
                    let ok = false;
                    try {
                        ok = host.setSticky(ref, target);
                    } catch (error) {
                        void error;
                        return { setter: "threw", post: evidencePre(ref, property) };
                    }
                    return { setter: ok ? "accepted" : "refused", post: evidencePre(ref, property) };
                }
                case "above": {
                    if (target) {
                        let ok = false;
                        try {
                            ok = host.setKeepAbove(ref, true);
                        } catch (error) {
                            void error;
                            return { setter: "threw", post: evidencePre(ref, property) };
                        }
                        return { setter: ok ? "accepted" : "refused", post: evidencePre(ref, property) };
                    }
                    if (saved === null) {
                        return { setter: "refused", post: evidencePre(ref, property) };
                    }
                    const frozenOwned = ownedAbove.get(lane.win);
                    let firstOk = false;
                    try {
                        firstOk = host.setKeepAbove(ref, saved.above);
                    } catch (error) {
                        void error;
                        return { setter: "threw", post: evidencePre(ref, property) };
                    }
                    if (!firstOk) {
                        return { setter: "refused", post: evidencePre(ref, property) };
                    }
                    // The first subcall may have run reentrant code (pinned
                    // KWin window.cpp: setKeepAbove(true) clears below, so a
                    // nested acquire can change hands here). Block the second
                    // subcall when fences/authority/lifetime lapsed, when its
                    // own window was pruned, or when ownership changed hands;
                    // the first call's accepted fact stands.
                    if (lane.cancelled ||
                        ownedAbove.get(lane.win) !== frozenOwned) {
                        return { setter: "accepted", post: evidencePre(ref, property) };
                    }
                    const fresh = secondLive(lane);
                    if (fresh === null) {
                        return { setter: "accepted", post: evidencePre(ref, property) };
                    }
                    try {
                        const cur = host.readStacking(fresh);
                        if (cur !== null && cur.below !== saved.below &&
                            !host.setKeepBelow(fresh, saved.below)) {
                            return { setter: "refused", post: evidencePre(fresh, property) };
                        }
                    } catch (error) {
                        void error;
                        return { setter: "threw", post: evidencePre(fresh, property) };
                    }
                    return { setter: "accepted", post: evidencePre(fresh, property) };
                }
            }
        } catch (error) {
            void error;
            return { setter: "threw", post: null };
        }
    };

    // Per-lane pre-call revalidation + freeze-before-invoke. All silent
    // drops settle without evidence and without touching spent. On success
    // the lane is removed, the identity frozen, and spent set BEFORE the
    // setter; evidence uses the frozen identity and the outer never rewrites
    // spent afterwards.
    const writeSingle = (lane: PendingLane): FrameSetter | null => {
        const key = laneKey(lane.win, lane.property);
        if (pending.get(key) !== lane || lane.cancelled) {
            return null;
        }
        let fences: NativeStateFences;
        try {
            fences = deps.getFences();
        } catch (error) {
            void error;
            settle(lane);
            return null;
        }
        if (fences.stopped) {
            settle(lane);
            return null;
        }
        if (fences.generation === null || fences.generation !== lane.gen ||
            fences.settingsRev !== lane.settingsRev ||
            lane.rev !== fences.acceptedRev ||
            fences.watermark !== lane.watermark) {
            settle(lane);
            dropped(lane.win, lane.property, "stale");
            return null;
        }
        let auth = false;
        try {
            auth = deps.isStillAuthorized(lane.win, lane.property);
        } catch (error) {
            void error;
            auth = false;
        }
        if (!auth) {
            settle(lane);
            dropped(lane.win, lane.property, "stale");
            return null;
        }
        const live = resolveLive(lane.win);
        if (live === null) {
            settle(lane);
            dropped(lane.win, lane.property, "observe-missing");
            return null;
        }
        if (!slotsEqual(lane.property, readSlot(live, lane.property), lane.capturedPre)) {
            settle(lane);
            dropped(lane.win, lane.property, "source-raced");
            return null;
        }
        if (lane.property === "above" && !lane.target && lane.ownedPair === null) {
            settle(lane);
            dropped(lane.win, lane.property, "unowned-release");
            return null;
        }
        if (methodMissing(live, lane.property)) {
            settle(lane);
            dropped(lane.win, lane.property, "unsupported");
            return null;
        }
        if (!isCanonicalU64(fences.clock)) {
            settle(lane);
            return null;
        }
        const frozenRev = lane.rev;
        const frozenTarget = lane.target;
        const frozenCmd = lane.cmdSeq;
        const frozenClock = fences.clock;
        const frozenReset = resetToken;
        const frozenOwned = ownedAbove.get(lane.win);
        pending.delete(key);
        latest.set(key, { target: frozenTarget, cmdSeq: frozenCmd });
        const pre = evidencePre(live, lane.property);
        const saved = lane.property === "above" && !frozenTarget ? lane.ownedPair : null;
        inFlight.add(lane);
        try {
            const result = invoke(live, lane, frozenTarget, saved);
            // Completion fence: generation/reset/stop gate the raw record AND
            // ownership (no old-generation injection). Same-generation evidence
            // for an actual call stands even when authority/rev moved reentrantly
            // (B2), liveness was lost after the call, or its own window was
            // pruned. Prune (lane cancelled) and reentrant ownership changes
            // gate ownership only.
            let curFences: NativeStateFences | null = null;
            try {
                curFences = deps.getFences();
            } catch (error) {
                void error;
                curFences = null;
            }
            const genOk = curFences !== null &&
                !stopped && !curFences.stopped &&
                curFences.generation !== null &&
                curFences.generation === lane.gen &&
                boundGen === lane.gen &&
                frozenReset === resetToken;
            if (!genOk) {
                host.log(
                    `omnitiler:world:native-state-${result.setter === "accepted" ? "pending" : "refused"} target=${lane.property} window=${token(lane.win)}`,
                );
                return result.setter;
            }
            try {
                state.recordStateAttempt({
                    win: lane.win,
                    property: lane.property,
                    target: frozenTarget,
                    rev: frozenRev,
                    setter: result.setter,
                    pre,
                    post: result.post,
                    attemptMs: frozenClock,
                });
            } catch (error) {
                void error;
            }
            try {
                state.noteFollowup();
            } catch (error) {
                void error;
            }
            if (result.setter === "accepted" && lane.property === "above" &&
                !lane.cancelled &&
                ownedAbove.get(lane.win) === frozenOwned) {
                if (frozenTarget) {
                    if (lane.durablePair !== null) {
                        ownedAbove.set(lane.win, { ...lane.durablePair });
                    }
                } else {
                    ownedAbove.delete(lane.win);
                }
            }
            host.log(
                `omnitiler:world:native-state-${result.setter === "accepted" ? "pending" : "refused"} target=${lane.property} window=${token(lane.win)}`,
            );
            return result.setter;
        } finally {
            inFlight.delete(lane);
        }
    };

    const onStored = (gen: string, lanes: PendingLane[], ok: boolean): void => {
        const active = lanes.filter(
            (l) => !l.cancelled && pending.get(laneKey(l.win, l.property)) === l,
        );
        if (active.length === 0) {
            return;
        }
        if (stopped) {
            for (const lane of active) {
                settle(lane);
            }
            return;
        }
        let fences: NativeStateFences;
        try {
            fences = deps.getFences();
        } catch (error) {
            void error;
            for (const lane of active) {
                settle(lane);
            }
            return;
        }
        if (boundGen !== fences.generation) {
            latest.clear();
            ownedAbove.clear();
            pending.clear();
            boundGen = fences.generation;
            return;
        }
        if (fences.generation === null || fences.generation !== gen) {
            for (const lane of active) {
                settle(lane);
                dropped(lane.win, lane.property, "stale");
            }
            return;
        }
        if (!ok) {
            for (const lane of active) {
                settle(lane);
                dropped(lane.win, lane.property, "preimage-unavailable");
            }
            return;
        }
        // Paired fullscreen-false + maximize: both already persisted; clear
        // first, then maximize with no applied gate between. A non-accepted
        // clear skips maximize with no fabricated attempt.
        const fs = active.find((a) => a.property === "fullscreen" && !a.target);
        const mx = active.find((a) => a.property === "maximized");
        if (fs !== undefined && mx !== undefined && fs.cmdSeq === mx.cmdSeq) {
            const fsSetter = writeSingle(fs);
            if (fsSetter !== "accepted") {
                settle(mx);
                dropped(mx.win, mx.property, "paired-clear-not-accepted");
            } else {
                writeSingle(mx);
            }
            for (const a of active) {
                if (a !== fs && a !== mx) {
                    writeSingle(a);
                }
            }
            return;
        }
        for (const a of active) {
            writeSingle(a);
        }
    };

    const run = (reply: WorldReply): void => {
        if (stopped) {
            return;
        }
        let fences: NativeStateFences;
        try {
            fences = deps.getFences();
        } catch (error) {
            void error;
            return;
        }
        if (fences.stopped) {
            return;
        }
        if (boundGen === undefined) {
            boundGen = fences.generation;
        } else if (boundGen !== fences.generation) {
            latest.clear();
            ownedAbove.clear();
            pending.clear();
            boundGen = fences.generation;
        }
        if (fences.generation === null || reply.gen !== fences.generation ||
            fences.settingsRev === null ||
            fences.acceptedRev !== reply.rev ||
            !isCanonicalU64(reply.rev) ||
            !isCanonicalU64(fences.clock)) {
            return;
        }
        const gen = fences.generation;
        const settingsRev = fences.settingsRev;
        const watermark = fences.watermark;

        for (const row of reply.windows) {
            if (stopped) {
                return;
            }
            if (!isOpaqueId(row.win)) {
                continue;
            }
            const win = row.win;
            // Clearing first: any listed-window property missing at ANY
            // authority clears spent (even empty state). Hold/untouched
            // presence protects. Whole-window absence never reaches here.
            const present = new Set(row.state.map((t) => t.property));
            for (const prop of STATE_PROPERTIES) {
                if (!present.has(prop)) {
                    latest.delete(laneKey(win, prop));
                }
            }
            const eligible = row.state.filter((t) => t.auth === "controlled" && t.reason === "");
            if (eligible.length === 0) {
                continue;
            }
            const seen = new Set<WorldStateProperty>();
            const targets = eligible.filter((t) => {
                if (seen.has(t.property)) {
                    return false;
                }
                seen.add(t.property);
                return true;
            });
            const live = resolveLive(win);
            if (live === null) {
                dropped(win, targets[0]?.property ?? "fullscreen", "observe-missing");
                continue;
            }
            const armed: PendingLane[] = [];
            for (const t of targets) {
                const key = laneKey(win, t.property);
                const pend = pending.get(key);
                if (pend !== undefined) {
                    if (!pend.cancelled && pend.target === t.value && pend.cmdSeq === t.cmdSeq) {
                        pend.rev = reply.rev;
                        pend.gen = gen;
                        pend.settingsRev = settingsRev;
                        pend.watermark = watermark;
                        continue;
                    }
                    pend.cancelled = true;
                    pending.delete(key);
                }
                const mark = latest.get(key);
                if (mark !== undefined && mark.target === t.value && mark.cmdSeq === t.cmdSeq) {
                    continue;
                }
                if (t.property === "above" && !t.value && !ownedAbove.has(win)) {
                    dropped(win, t.property, "unowned-release");
                    continue;
                }
                // Pre-arm authority/recovery gate (same-design gating as the
                // actual setter): a slot blocked by startup recovery or
                // refused by the live authority check must not arm or flush.
                // During a pending recovery clear-housekeeping ack, arming
                // here would rewrite the slot the ack verifies cleared and
                // wedge the lane permanently. Presence/spent/pending
                // semantics above are untouched; this only refuses fresh arms.
                let authorized = false;
                try {
                    authorized = deps.isStillAuthorized(win, t.property);
                } catch (error) {
                    void error;
                    authorized = false;
                }
                if (!authorized) {
                    dropped(win, t.property, "stale");
                    continue;
                }
                const captured = armPayload(live, t.property);
                if (captured === null) {
                    dropped(win, t.property, "unreadable");
                    continue;
                }
                if (!store.canWrite(win, t.property, gen)) {
                    dropped(win, t.property, "preimage-unavailable");
                    continue;
                }
                try {
                    store.arm(win, captured.arm as Parameters<NativePreimageStore["arm"]>[1]);
                } catch (error) {
                    void error;
                }
                const stored = storedOriginal(win, t.property);
                if (stored === null) {
                    dropped(win, t.property, "preimage-unavailable");
                    continue;
                }
                armed.push({
                    win,
                    property: t.property,
                    target: t.value,
                    cmdSeq: t.cmdSeq,
                    rev: reply.rev,
                    gen,
                    settingsRev,
                    watermark,
                    capturedPre: cloneSlot(captured.original),
                    durablePair: t.property === "above" &&
                        typeof stored === "object" && !Array.isArray(stored)
                        ? { ...(stored as { above: boolean; below: boolean }) }
                        : null,
                    ownedPair: t.property === "above" && !t.value
                        ? { ...(ownedAbove.get(win) as { above: boolean; below: boolean }) }
                        : null,
                    cancelled: false,
                });
            }
            if (armed.length === 0) {
                continue;
            }
            for (const lane of armed) {
                pending.set(laneKey(lane.win, lane.property), lane);
            }
            const lanes = armed.map((a) => ({
                win,
                property: mapWorldStatePropertyToPreimage(a.property),
            }));
            const snapshot = [...armed];
            try {
                store.flush(lanes, (ok) => {
                    onStored(gen, snapshot, ok);
                });
            } catch (error) {
                void error;
                for (const lane of armed) {
                    settle(lane);
                    dropped(lane.win, lane.property, "flush-threw");
                }
            }
        }
    };

    return {
        run,
        prune: (live) => {
            for (const lane of inFlight) {
                if (!live.has(lane.win)) {
                    lane.cancelled = true;
                }
            }
            for (const key of [...latest.keys()]) {
                const win = key.slice(0, key.indexOf("\u0000"));
                if (!live.has(win)) {
                    latest.delete(key);
                }
            }
            for (const win of [...ownedAbove.keys()]) {
                if (!live.has(win)) {
                    ownedAbove.delete(win);
                }
            }
            for (const [key, lane] of [...pending]) {
                if (!live.has(lane.win)) {
                    lane.cancelled = true;
                    pending.delete(key);
                }
            }
        },
        reset: (generation) => {
            resetToken = {};
            latest.clear();
            ownedAbove.clear();
            pending.clear();
            boundGen = generation;
        },
        stop: () => {
            stopped = true;
            resetToken = {};
            latest.clear();
            ownedAbove.clear();
            pending.clear();
        },
        spentCountForTest: () => latest.size,
    };
}
