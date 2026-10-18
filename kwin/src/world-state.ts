// World adapter lifetime state (D1-D5): generation binding, observation /
// request / command sequencing, freshness acceptance, dirty/ack retention,
// creation one-flight, and event-driven recovery. No KWin or D-Bus imports:
// the entry drives this with captured observations and transport results.

import { compareU64, incrementU64, isCanonicalU64 } from "./world-seq";
import { isWorldRejection, type WorldCommand, type WorldReply } from "./world-report";

export type AttemptOutcome =
    | "unattempted"
    | "possibly-executed"
    | "refused-proven"
    | "superseded-external"
    | "applied";

export interface AttemptFeedback {
    op: string;
    kind: "transfer" | "view" | "creation" | "effect" | "property";
    outcome: AttemptOutcome;
    hold: string;
}

export interface PendingAttempt {
    op: string;
    kind: AttemptFeedback["kind"];
    target: string;
}

// One in-flight native creation per logical id, with enumeration recovery.
export interface PendingCreation {
    logical: string;
    out: string;
    bound: string | null;
    // Native inventory baseline captured BEFORE the create attempt: recovery
    // adopts exactly one fresh id beyond this baseline. Never rebuilt from
    // the current list at recover time (identical lists always recover
    // nothing).
    knownBefore: string[];
}

export class WorldAdapterState {
    readonly adapter: string;
    readonly owner: string;
    boundGen: string | null = null;
    // CURRENT local watermark: advanced synchronously on every invalidation,
    // including during flight and gestures, before any debounce.
    currentObs = "0";
    lastAcceptedObs = "0";
    lastAcceptedRev = "0";
    lastAcceptedReq = "0";
    hasAccepted = false;
    // Distinct high-watermarks: request seq per dispatch, command seq per
    // explicit user command. Never conflated.
    nextReqSeq = "1";
    nextCmdSeq = "1";
    cmdHighwater: string | null = null;
    inFlightReq: { reqSeq: string; obs: string; cmdSeq: string | null; reqId: string } | null = null;
    dirty = false;
    observationOnlyCatchup = false;
    // Dirty ack retention: closes and feedback survive until an accepted
    // reply acknowledges them.
    pendingClosed: string[] = [];
    pendingFeedback: AttemptFeedback[] = [];
    // Transfer/view attempts keyed by operation id.
    attempts = new Map<string, PendingAttempt>();
    possiblyExecuted = new Set<string>();
    spentEffects = new Set<string>();
    // Explicit retained float placements spent per stable core property
    // operation id (`frame_op`, `op-<cmdSeq>`): the row token places exactly
    // once, never again for the same operation across duplicate or unrelated
    // later replies. Rows without the token (ordinary floats) never place.
    floatSpent = new Set<string>();
    // Last proven-applied native targets, for external-input causality: a
    // repeat of the SAME operation after the subject demonstrably left the
    // applied target proves newer external input (supersede), while a
    // distinct operation id is always a fresh actuation. Operation ids are
    // never reused (`op-<cmdSeq>` monotonic), so same-op means same intent.
    appliedWindowTarget = new Map<string, { op: string; out: string; ws: string }>();
    appliedViewTarget = new Map<string, { op: string; ws: string }>();
    pendingCreations: PendingCreation[] = [];
    // Bounded per-context clamp reassertions (window+target+constraints).
    clampAttempts = new Map<string, number>();
    // Last accepted desired snapshot for per-property diffing.
    lastReply: WorldReply | null = null;

    constructor(adapter: string, owner: string) {
        this.adapter = adapter;
        this.owner = owner;
    }

    // Synchronous invalidation: advance CURRENT obs BEFORE debounce/dispatch,
    // even with a request in flight or a gesture barrier active.
    invalidate(): string {
        const next = incrementU64(this.currentObs);
        if (next === null) {
            // Counter exhaustion: hold at max; the next discovery bootstraps
            // a new generation rather than wrapping in place.
            this.dirty = true;
            return this.currentObs;
        }
        this.currentObs = next;
        this.dirty = true;
        return this.currentObs;
    }

    noteClosed(id: string): void {
        if (!this.pendingClosed.includes(id)) {
            this.pendingClosed.push(id);
        }
        this.dirty = true;
    }

    noteFeedback(feedback: AttemptFeedback): void {
        const at = this.pendingFeedback.findIndex((f) => f.op === feedback.op);
        if (at >= 0) {
            this.pendingFeedback[at] = feedback;
        } else {
            this.pendingFeedback.push(feedback);
        }
        if (feedback.outcome === "possibly-executed") {
            this.possiblyExecuted.add(feedback.op);
        } else if (feedback.outcome === "applied" || feedback.outcome === "refused-proven") {
            this.possiblyExecuted.delete(feedback.op);
        }
        this.dirty = true;
    }

    // Arm possibly-executed BEFORE any native setter for an ambiguous
    // attempt. Never repeat that op until fresh unambiguous proof arrives.
    armPossiblyExecuted(op: string, kind: AttemptFeedback["kind"], target: string): void {
        this.attempts.set(op, { op, kind, target });
        this.possiblyExecuted.add(op);
        this.noteFeedback({ op, kind, outcome: "possibly-executed", hold: "" });
    }

    isHeldAmbiguous(op: string): boolean {
        return this.possiblyExecuted.has(op);
    }

    settleAttempt(op: string, outcome: AttemptOutcome, hold = ""): void {
        const known = this.attempts.get(op);
        this.noteFeedback({ op, kind: known?.kind ?? "transfer", outcome, hold });
        if (outcome === "applied" || outcome === "refused-proven" || outcome === "superseded-external") {
            this.possiblyExecuted.delete(op);
            if (outcome !== "applied") {
                this.attempts.delete(op);
            }
        }
    }

    markEffectSpent(id: string): boolean {
        if (this.spentEffects.has(id)) {
            return false;
        }
        this.spentEffects.add(id);
        return true;
    }

    // Spend one explicit retained float placement before its possible
    // setter. Returns false when this exact property operation already
    // placed (never double-place, including across duplicate replies and
    // unrelated later replies still carrying the retained token).
    markFloatSpent(key: string): boolean {
        if (this.floatSpent.has(key)) {
            return false;
        }
        this.floatSpent.add(key);
        return true;
    }

    noteAppliedWindowTarget(win: string, op: string, out: string, ws: string): void {
        this.appliedWindowTarget.set(win, { op, out, ws });
    }

    noteAppliedViewTarget(out: string, op: string, ws: string): void {
        this.appliedViewTarget.set(out, { op, ws });
    }

    creationFor(logical: string): PendingCreation | null {
        return this.pendingCreations.find((c) => c.logical === logical) ?? null;
    }

    // At most one in-flight creation per logical id. Returns null when a
    // flight is already outstanding (caller must recover by enumeration).
    // Captures the pre-attempt native baseline for later recovery.
    startCreation(logical: string, out: string, knownBefore: ReadonlyArray<string> = []): PendingCreation | null {
        const existing = this.creationFor(logical);
        if (existing !== null && existing.bound === null) {
            return null;
        }
        if (existing !== null) {
            return existing;
        }
        const row: PendingCreation = { logical, out, bound: null, knownBefore: [...knownBefore] };
        this.pendingCreations.push(row);
        return row;
    }

    bindCreation(logical: string, native: string): void {
        const row = this.creationFor(logical);
        if (row === null) {
            return;
        }
        // Never share one native binding between two logical tokens.
        if (this.claimedNative(native, logical)) {
            return;
        }
        row.bound = native;
    }

    // A native id already bound to a different pending logical token is
    // never adopted or rebound elsewhere.
    claimedNative(native: string, exceptLogical?: string): boolean {
        return this.pendingCreations.some((c) => c.logical !== exceptLogical && c.bound === native);
    }

    // Recover a creation binding by enumeration: when the logical token has
    // no binding but the native inventory grew by exactly one id beyond the
    // pre-attempt baseline, adopt it. Null when ambiguous. An explicit
    // baseline overrides the stored one; otherwise the stored pre-attempt
    // baseline applies (never a freshly rebuilt identical list).
    recoverCreationByEnumeration(logical: string, liveNative: ReadonlyArray<string>, knownBefore?: ReadonlyArray<string>): string | null {
        const row = this.creationFor(logical);
        if (row === null || row.bound !== null) {
            return row?.bound ?? null;
        }
        const base = knownBefore ?? row.knownBefore;
        const fresh = liveNative.filter((id) => !base.includes(id));
        if (fresh.length === 1) {
            const bound = fresh[0] as string;
            if (this.claimedNative(bound, logical)) {
                return null;
            }
            row.bound = bound;
            return bound;
        }
        return null;
    }

    // Name-attributed recovery for the void KWin create path: bind only the
    // exact unique desktop whose native name matches the requested logical
    // token among ids fresh beyond the pre-attempt baseline. Unrelated
    // appended desktops (different names), duplicates, and already-claimed
    // natives never bind. An explicit baseline overrides the stored one.
    recoverCreationByName(
        logical: string,
        live: ReadonlyArray<{ readonly id: string; readonly name: string }>,
        knownBefore?: ReadonlyArray<string>,
    ): string | null {
        const row = this.creationFor(logical);
        if (row === null || row.bound !== null) {
            return row?.bound ?? null;
        }
        const base = new Set<string>(knownBefore ?? row.knownBefore);
        const candidates = live.filter((d) => d.name === logical && !base.has(d.id));
        if (candidates.length !== 1) {
            return null;
        }
        const bound = (candidates[0] as { id: string }).id;
        if (this.claimedNative(bound, logical)) {
            return null;
        }
        row.bound = bound;
        return bound;
    }

    // Core-authoritative handoff for locally-created bindings: once the
    // core acknowledges a real (non-fallback) binding for a native id or a
    // logical token, the adapter-owned pending creation is consumed so a
    // later legitimate migration/rehoming stays core-owned. Targeted to the
    // acknowledged native/logical pair only; never a map-wide clear.
    acknowledgeCreationBinding(native: string, logical: string): void {
        if (native === "" || logical === "") {
            return;
        }
        this.pendingCreations = this.pendingCreations.filter(
            (c) => c.bound !== native && c.logical !== logical,
        );
    }

    clampKey(win: string, target: string, constraints: string): string {
        return `${win}|${target}|${constraints}`;
    }

    clampAttempt(key: string): number {
        return this.clampAttempts.get(key) ?? 0;
    }

    noteClampAttempt(key: string): number {
        const next = (this.clampAttempts.get(key) ?? 0) + 1;
        this.clampAttempts.set(key, next);
        return next;
    }

    resetClamp(key: string): void {
        this.clampAttempts.delete(key);
    }

    nextRequestId(reqSeq: string): string {
        return `req-${reqSeq}`;
    }

    // Freshness acceptance: only a bound-generation, ready, newer-rev reply
    // whose adapter matches this adapter, whose req names the current
    // flight requestId, and whose obs equals the CURRENT watermark applies
    // setters. Anything else is observation-only catchup with no command
    // replay and no setters. Single-flight alone never authenticates: a
    // delayed arbitrary reply (different adapter or wrong req, even with the
    // same gen/obs/newer rev) applies nothing and acks nothing.
    acceptReply(reply: WorldReply, dispatchObs: string): { applied: boolean; reason: string } {
        if (this.boundGen === null || reply.gen !== this.boundGen) {
            this.observationOnlyCatchup = true;
            this.dirty = true;
            return { applied: false, reason: "generation-mismatch" };
        }
        if (reply.adapter !== this.adapter) {
            this.observationOnlyCatchup = true;
            this.dirty = true;
            return { applied: false, reason: "adapter-mismatch" };
        }
        if (this.inFlightReq === null || reply.req !== this.inFlightReq.reqId) {
            this.observationOnlyCatchup = true;
            this.dirty = true;
            return { applied: false, reason: "stale-req" };
        }
        if (!reply.ready) {
            this.observationOnlyCatchup = true;
            this.dirty = true;
            return { applied: false, reason: "not-ready" };
        }
        if (!isCanonicalU64(reply.rev) || !isCanonicalU64(reply.obs)) {
            this.observationOnlyCatchup = true;
            this.dirty = true;
            return { applied: false, reason: "malformed-counter" };
        }
        if (this.hasAccepted && compareU64(reply.rev, this.lastAcceptedRev) <= 0) {
            return { applied: false, reason: "stale-rev" };
        }
        if (reply.obs !== this.currentObs || reply.obs !== dispatchObs) {
            // Superseded by newer local input: catch up observation-only,
            // never replay the command.
            this.observationOnlyCatchup = true;
            this.dirty = true;
            return { applied: false, reason: "stale-obs" };
        }
        this.lastAcceptedRev = reply.rev;
        this.lastAcceptedObs = reply.obs;
        this.lastAcceptedReq = reply.req;
        this.hasAccepted = true;
        this.lastReply = reply;
        this.inFlightReq = null;
        this.observationOnlyCatchup = false;
        // Ack: the accepted reply observed this report's closes/feedback.
        this.pendingClosed = [];
        this.pendingFeedback = this.pendingFeedback.filter((f) => this.possiblyExecuted.has(f.op));
        if (this.cmdHighwater !== null) {
            void this.cmdHighwater;
        }
        return { applied: true, reason: "accepted" };
    }

    noteRejectedOrTransportError(): void {
        // Busy/timeout/transport loss: retain dirty work, drop the command
        // (never silently coalesce or replay it), and wait for the next
        // native event. No idle polling, no timer retry.
        this.inFlightReq = null;
        this.observationOnlyCatchup = true;
        this.dirty = true;
    }

    // Release exactly the consumed flight token so a freshness/correlation
    // rejection on its callback can redispatch observation-only catchup.
    // Never clears a newer reentrant flight: a token mismatch keeps the
    // current flight untouched (the stale callback owns nothing then).
    releaseFlight(reqId: string): boolean {
        if (this.inFlightReq !== null && this.inFlightReq.reqId === reqId) {
            this.inFlightReq = null;
            return true;
        }
        return false;
    }

    beginDispatch(command: WorldCommand | null): { reqSeq: string; cmdSeq: string | null; reqId: string } | null {
        if (this.inFlightReq !== null) {
            return null;
        }
        const reqSeq = this.nextReqSeq;
        const nextReq = incrementU64(this.nextReqSeq);
        if (nextReq === null) {
            return null;
        }
        this.nextReqSeq = nextReq;
        let cmdSeq: string | null = null;
        if (command !== null) {
            cmdSeq = this.nextCmdSeq;
            const nextCmd = incrementU64(this.nextCmdSeq);
            if (nextCmd === null) {
                return null;
            }
            this.nextCmdSeq = nextCmd;
            this.cmdHighwater = this.cmdHighwater === null
                ? cmdSeq
                : (compareU64(cmdSeq, this.cmdHighwater) > 0 ? cmdSeq : this.cmdHighwater);
        }
        const reqId = this.nextRequestId(reqSeq);
        this.inFlightReq = { reqSeq, obs: this.currentObs, cmdSeq, reqId };
        this.dirty = false;
        this.observationOnlyCatchup = false;
        return { reqSeq, cmdSeq, reqId };
    }

    onDiscovery(gen: string, adapter?: string, req?: string, expectedReq?: string): void {
        // Authenticated discovery binds the generation only for the exact
        // adapter namespace and the exact outstanding discovery req. A
        // delayed arbitrary reply (different adapter or wrong req) binds
        // nothing and resets nothing.
        if (adapter !== undefined && adapter !== this.adapter) {
            return;
        }
        if (expectedReq !== undefined && req !== expectedReq) {
            return;
        }
        // Authenticated discovery binds the generation; a changed generation
        // cancels flights, ordering, and desired caches, never native wins.
        if (this.boundGen !== gen) {
            this.boundGen = gen;
            this.inFlightReq = null;
            this.lastReply = null;
            this.hasAccepted = false;
            this.lastAcceptedRev = "0";
            this.lastAcceptedObs = "0";
            this.cmdHighwater = null;
            this.attempts.clear();
            this.possiblyExecuted.clear();
            this.spentEffects.clear();
            this.floatSpent.clear();
            this.appliedWindowTarget.clear();
            this.appliedViewTarget.clear();
            this.pendingCreations = [];
            this.clampAttempts.clear();
        }
        this.dirty = true;
    }

    describeForLog(): string {
        const flight = this.inFlightReq === null ? "idle" : `flight-req-${this.inFlightReq.reqSeq}-obs-${this.inFlightReq.obs}`;
        return `adapter=${this.adapter} gen=${this.boundGen ?? "-"} obs=${this.currentObs} rev=${this.lastAcceptedRev} ${flight} dirty=${this.dirty ? "1" : "0"}`;
    }

    isRejectionNotReady(value: unknown): boolean {
        return isWorldRejection(value as never) === true;
    }
}
