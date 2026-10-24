// World adapter lifetime state (broker era): generation/broker binding,
// transport fences, and dirty/ack retention. The sole broker owns ALL
// sequencing (obs/reqSeq/cmdSeq/cmdSeq) and its own clock: this adapter
// owns only two transport fences - the per-capture source correlation and
// the local native-invalidation watermark - plus the queue-client command
// counter. No KWin or D-Bus imports: the entry drives this with captured
// observations and transport results.

import { compareU64, incrementU64, isCanonicalU64, isOpaqueId } from "./world-seq";
import {
    isWorldRejection,
    type WorldCommand,
    type WorldCreationAttempt,
    type WorldReply,
    type WorldReplyResult,
} from "./world-report";

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

// Executed-but-unproven native creation: the native call ran (op spent)
// but the same-turn post-call readback was unavailable, so no causal id
// can be proven from that turn. Preserved locally - never emitted - until
// the first successful later full native-order readback completes it as
// an uncorrelated receipt (created null: causality is lost across the JS
// turn and the hybrid uncorrelated rule applies). The stamp stays the
// original write clock.
export interface PendingCreationReceipt {
    op: string;
    ws: string;
    out: string;
    setter: "accepted" | "threw";
    before: string[];
    attemptMs: string;
}

// One queued user intent awaiting the queue-then-capture route. keepSubject
// is LOCAL routing only (float-origin fallback keeps its press-time subject
// instead of rebinding to the current native focus): it never rides the
// wire, where every focus-directional is exactly {op,win,direction}.
export interface QueuedIntent {
    command: WorldCommand;
    keepSubject: boolean;
}

export interface CaptureFlight {
    source: string;
    watermark: string;
    // Queue-client command the fresh capture drains (FIFO head), if any.
    // Terminal results arrive requester-only via TakeCommandResult, never
    // in the capture reply.
    clientCmd: string | null;
}

export class WorldAdapterState {
    readonly adapter: string;
    readonly owner: string;
    // Queue-client namespace (stable per evaluation; the sender binds the
    // D-Bus unique name server-side).
    readonly client: string;
    boundGen: string | null = null;
    // Broker incarnation from authenticated discovery (the reply adapter):
    // authority stays with the broker; callbacks bind it before setters.
    brokerIncarnation: string | null = null;
    // LOCAL native-invalidation watermark (canonical u64): advanced
    // synchronously on every native invalidation, including during flight
    // and gestures, before any debounce. A pure transport fence echoed in
    // the capture envelope and bound in the response; never a sequence.
    currentInvalid = "0";
    // Broker dispatch-clock reading (foreign epoch milliseconds) from the
    // last capture response. Raw native attempt stamps must fit this epoch;
    // "0" means unreported and never suppresses.
    lastClock = "0";
    nextClientCmd = "1";
    inFlight: CaptureFlight | null = null;
    hasAccepted = false;
    lastAcceptedRev = "0";
    dirty = false;
    observationOnlyCatchup = false;
    // Dirty ack retention: explicit closes survive until an accepted reply
    // acknowledges them. (Old verdict `feedback` transport is deleted: no
    // verdicts ride the wire, so only closes retain.)
    pendingClosed: string[] = [];
    // Transfer/view attempts keyed by operation id.
    attempts = new Map<string, PendingAttempt>();
    possiblyExecuted = new Set<string>();
    spentEffects = new Set<string>();
    // Explicit retained float placements spent per stable core property
    // operation id (`frame_op`): the row token places exactly once, never
    // again for the same operation across duplicate or unrelated later
    // replies. Rows without the token (ordinary floats) never place.
    // `clear_op` spends exactly once the same way.
    floatSpent = new Set<string>();
    clearSpent = new Set<string>();
    // Last proven-applied native targets, for external-input causality: a
    // repeat of the SAME operation after the subject demonstrably left the
    // applied target proves newer external input (supersede), while a
    // distinct operation id is always a fresh actuation. Operation ids are
    // never reused, so same-op means same intent.
    appliedWindowTarget = new Map<string, { op: string; out: string; ws: string }>();
    appliedViewTarget = new Map<string, { op: string; ws: string }>();
    // Explicit native creation desires spent at most once per stable core
    // op (`op-native-<serial>`): spent BEFORE the native call so reentrant
    // invalidation (synchronous KWin signals) never double-executes.
    creationSpent = new Set<string>();
    // Raw creation receipts preserved across reports until the core
    // incorporates them (a bound row for the logical) or the generation
    // rotates. Unchanged redelivery is idempotent server-side. Core owns
    // binding; this table never maps native ids to logical ids.
    creationAttempts = new Map<string, WorldCreationAttempt>();
    // Executed-but-unproven creation evidence (op spent, post-call
    // readback unavailable): never emitted, completed once against the
    // first successful later full native-order readback. Cleared with
    // everything else on generation rotation.
    creationPending = new Map<string, PendingCreationReceipt>();
    // Bounded per-context clamp reassertions.
    clampAttempts = new Map<string, number>();
    // Last accepted desired snapshot for per-property diffing.
    lastReply: WorldReply | null = null;
    // Last terminal take result, for entry routing (drag-preview,
    // toggle-float, group highlight consume take results, never the
    // capture reply).
    lastTakeResult: WorldReplyResult | null = null;

    constructor(adapter: string, owner: string) {
        this.adapter = adapter;
        this.owner = owner;
        this.client = adapter;
    }

    // Synchronous invalidation: advance the LOCAL watermark BEFORE
    // debounce/dispatch, even with a request in flight or a gesture barrier
    // active. Never resets, never aliases a sequence.
    invalidate(): string {
        const next = incrementU64(this.currentInvalid);
        if (next === null) {
            // Counter exhaustion: hold at max; the next discovery bootstraps
            // a fresh generation rather than wrapping in place.
            this.dirty = true;
            return this.currentInvalid;
        }
        this.currentInvalid = next;
        this.dirty = true;
        return this.currentInvalid;
    }

    noteClosed(id: string): void {
        if (!this.pendingClosed.includes(id)) {
            this.pendingClosed.push(id);
        }
        this.dirty = true;
    }

    // Local attempt-outcome bookkeeping (possibly-executed gating for
    // mid-apply causality). Never a wire verdict: raw creation receipts ride
    // the report lane; other raw attempt lanes stay absent until truthful
    // producers exist (absent backfills [] server-side).
    noteFeedback(feedback: AttemptFeedback): void {
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
        this.dirty = true;
    }

    isHeldAmbiguous(op: string): boolean {
        return this.possiblyExecuted.has(op);
    }

    settleAttempt(op: string, outcome: AttemptOutcome, hold = ""): void {
        void hold;
        if (outcome === "applied" || outcome === "refused-proven" || outcome === "superseded-external") {
            this.possiblyExecuted.delete(op);
            if (outcome !== "applied") {
                this.attempts.delete(op);
            }
        }
        this.dirty = true;
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
    // unrelated later replies still carrying the retained token). Rows
    // without the token (ordinary floats) never place.
    markFloatSpent(key: string): boolean {
        if (this.floatSpent.has(key)) {
            return false;
        }
        this.floatSpent.add(key);
        return true;
    }

    // Spend one explicit retained clear operation exactly once, like
    // frame_op above. `clear_op` carries `op-<cmdSeq>`-shaped tokens for
    // directional maximize-clear verdicts.
    markClearSpent(key: string): boolean {
        if (this.clearSpent.has(key)) {
            return false;
        }
        this.clearSpent.add(key);
        return true;
    }

    noteAppliedWindowTarget(win: string, op: string, out: string, ws: string): void {
        this.appliedWindowTarget.set(win, { op, out, ws });
    }

    noteAppliedViewTarget(out: string, op: string, ws: string): void {
        this.appliedViewTarget.set(out, { op, ws });
    }

    recordClock(clock: string): void {
        if (isCanonicalU64(clock)) {
            this.lastClock = clock;
        }
    }

    // Spend one explicit native creation desire before its possible native
    // call. Returns false when this exact stable op already executed
    // (never double-execute, including across duplicate replies and
    // unrelated later replies still carrying the retained desire).
    markCreationSpent(op: string): boolean {
        if (op === "") {
            return false;
        }
        if (this.creationSpent.has(op)) {
            return false;
        }
        this.creationSpent.add(op);
        return true;
    }

    // Record the raw receipt of one executed creation desire for the next
    // report lane. Survives across reports until the core incorporates it;
    // unchanged redelivery is idempotent. Invalid rows refuse (never
    // stored): empty op/ws/out, unknown setter, non-opaque native ids, or
    // a non-canonical stamp.
    recordCreationAttempt(attempt: WorldCreationAttempt): boolean {
        if (
            attempt.op === "" || attempt.ws === "" || attempt.out === "" ||
            (attempt.setter !== "accepted" && attempt.setter !== "refused" && attempt.setter !== "threw") ||
            !isCanonicalU64(attempt.attemptMs)
        ) {
            return false;
        }
        if (attempt.created !== null && !isOpaqueId(attempt.created)) {
            return false;
        }
        for (const id of [...attempt.before, ...attempt.after]) {
            if (!isOpaqueId(id)) {
                return false;
            }
        }
        this.creationAttempts.set(attempt.op, { ...attempt, before: [...attempt.before], after: [...attempt.after] });
        this.dirty = true;
        return true;
    }

    // Preserve execution evidence for a spent desire whose same-turn
    // post-call readback was unavailable. Never fabricated: no after-image
    // is invented here (`after: []` or a `before` copy would be false
    // evidence on the wire). Requires the op already spent (evidence is
    // only ever kept for an executed call, never a desire alone).
    // First completion wins; a duplicate defer for the same op keeps the
    // first evidence.
    deferCreationReceipt(receipt: PendingCreationReceipt): boolean {
        if (
            receipt.op === "" || receipt.ws === "" || receipt.out === "" ||
            (receipt.setter !== "accepted" && receipt.setter !== "threw") ||
            !isCanonicalU64(receipt.attemptMs) ||
            !this.creationSpent.has(receipt.op)
        ) {
            return false;
        }
        for (const id of receipt.before) {
            if (!isOpaqueId(id)) {
                return false;
            }
        }
        if (!this.creationPending.has(receipt.op)) {
            this.creationPending.set(receipt.op, { ...receipt, before: [...receipt.before] });
        }
        this.dirty = true;
        return true;
    }

    // Complete deferred execution evidence against the first successfully
    // observed post-call full native order. Uncorrelated only (created
    // null): a later-turn readback can never prove causality, so the
    // hybrid uncorrelated rule applies. Null readback completes nothing
    // and preserves the evidence. Returns the count completed.
    completeDeferredCreations(afterIds: string[] | null): number {
        if (afterIds === null || this.creationPending.size === 0) {
            return 0;
        }
        let done = 0;
        for (const [op, pending] of [...this.creationPending]) {
            if (this.recordCreationAttempt({
                op: pending.op, ws: pending.ws, out: pending.out, setter: pending.setter,
                before: [...pending.before], after: [...afterIds], created: null,
                attemptMs: pending.attemptMs,
            })) {
                this.creationPending.delete(op);
                done += 1;
            }
        }
        return done;
    }

    // Incorporate core-published bindings: a reply workspace row binding a
    // recorded attempt's logical on its output proves the core consumed the
    // receipt, so the attempt retires (and its possibly-executed arm
    // settles applied). Never invents binding locally: rows without a core
    // bind keep their receipts for the next report.
    incorporateCreationBindings(reply: WorldReply): void {
        if (this.creationAttempts.size === 0) {
            return;
        }
        for (const [op, attempt] of [...this.creationAttempts]) {
            const bound = reply.workspaces.some(
                (row) => row.out === attempt.out && row.id === attempt.ws && row.bind !== "",
            );
            if (bound) {
                this.creationAttempts.delete(op);
                this.settleAttempt(op, "applied", "");
            }
        }
    }

    // Ordered retained receipts for the report lane (insertion order is
    // stable per op; at most one entry per outstanding creation op).
    creationAttemptsForReport(): WorldCreationAttempt[] {
        return [...this.creationAttempts.values()].map((a) => ({ ...a, before: [...a.before], after: [...a.after] }));
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

    nextSourceId(counter: string): string {
        return `src-${counter}`;
    }

    // Mint one queue-client command id, or null on exhaustion (the caller
    // fails closed and re-establishes after re-discovery).
    mintClientCmd(): string | null {
        const id = this.nextClientCmd;
        const next = incrementU64(this.nextClientCmd);
        if (next === null) {
            return null;
        }
        this.nextClientCmd = next;
        return id;
    }

    // Freshness acceptance: only a bound-generation, broker-incarnation,
    // source-correlated reply whose watermark names the CURRENT local fence
    // mismatch-or-match applies setters. Anything else is observation-only
    // catchup with no setters. Single-flight alone never authenticates: a
    // delayed arbitrary reply (different incarnation or wrong source, even
    // with the same gen) applies nothing and acks nothing. A watermark
    // advance since dispatch (native input landed during flight) still
    // applies, then schedules follow-up via dirty.
    acceptReply(reply: WorldReply, dispatchSource: string, dispatchWatermark: string): { applied: boolean; reason: string } {
        if (this.boundGen === null || reply.gen !== this.boundGen) {
            this.observationOnlyCatchup = true;
            this.dirty = true;
            return { applied: false, reason: "generation-mismatch" };
        }
        if (this.brokerIncarnation === null || reply.adapter !== this.brokerIncarnation) {
            this.observationOnlyCatchup = true;
            this.dirty = true;
            return { applied: false, reason: "adapter-mismatch" };
        }
        if (this.inFlight === null || reply.req !== dispatchSource || reply.source !== dispatchSource) {
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
        void dispatchWatermark;
        this.lastAcceptedRev = reply.rev;
        this.hasAccepted = true;
        this.lastReply = reply;
        this.recordClock(reply.clock);
        this.inFlight = null;
        this.observationOnlyCatchup = false;
        // Ack: the accepted reply observed this report's closes.
        this.pendingClosed = [];
        if (this.currentInvalid !== dispatchWatermark) {
            // Native input landed during flight: the setters below run
            // against the dispatched observation, then follow-up observes
            // the newer watermark.
            this.dirty = true;
        }
        return { applied: true, reason: "accepted" };
    }

    noteRejectedOrTransportError(): void {
        // Busy/timeout/transport loss: retain dirty work and the queued
        // intent order, drop the flight (never silently replay a queued
        // head: it was already shifted at queue-send), and wait for the
        // next native event. No idle polling, no timer retry.
        this.inFlight = null;
        this.observationOnlyCatchup = true;
        this.dirty = true;
    }

    // Release exactly the consumed flight token so a freshness/correlation
    // rejection on its callback can redispatch observation-only catchup.
    // Never clears a newer reentrant flight: a source mismatch keeps the
    // current flight untouched (the stale callback owns nothing then).
    releaseFlight(source: string): boolean {
        if (this.inFlight !== null && this.inFlight.source === source) {
            this.inFlight = null;
            return true;
        }
        return false;
    }

    beginCapture(source: string, clientCmd: string | null): CaptureFlight | null {
        if (this.inFlight !== null) {
            return null;
        }
        const flight: CaptureFlight = { source, watermark: this.currentInvalid, clientCmd };
        this.inFlight = flight;
        this.dirty = false;
        this.observationOnlyCatchup = false;
        return flight;
    }

    onDiscovery(gen: string, incarnation: string, adapter?: string, req?: string, expectedReq?: string): boolean {
        // Authenticated discovery binds the generation plus the broker
        // incarnation only for the exact adapter namespace and the exact
        // outstanding discovery req. A delayed arbitrary reply (different
        // adapter or wrong req) binds nothing and resets nothing.
        if (adapter !== undefined && adapter !== this.adapter) {
            return false;
        }
        if (expectedReq !== undefined && req !== expectedReq) {
            return false;
        }
        // Authenticated discovery binds the generation; a changed generation
        // cancels flights, ordering, and desired caches, never native wins.
        // The queue-client namespace restarts at 1: commands queued under a
        // dead generation settle there and are uncollectible here.
        if (this.boundGen !== gen || this.brokerIncarnation !== incarnation) {
            this.boundGen = gen;
            this.brokerIncarnation = incarnation;
            this.inFlight = null;
            this.lastReply = null;
            this.lastTakeResult = null;
            this.hasAccepted = false;
            this.lastAcceptedRev = "0";
            this.nextClientCmd = "1";
            this.attempts.clear();
            this.possiblyExecuted.clear();
            this.spentEffects.clear();
            this.floatSpent.clear();
            this.clearSpent.clear();
            this.appliedWindowTarget.clear();
            this.appliedViewTarget.clear();
            this.creationSpent.clear();
            this.creationAttempts.clear();
            this.creationPending.clear();
            this.clampAttempts.clear();
            return true;
        }
        this.dirty = true;
        return false;
    }

    describeForLog(): string {
        const flight = this.inFlight === null
            ? "idle"
            : `flight-src-${this.inFlight.source}-nwinv-${this.inFlight.watermark}`;
        return `adapter=${this.adapter} gen=${this.boundGen ?? "-"} nwinv=${this.currentInvalid} rev=${this.lastAcceptedRev} ${flight} dirty=${this.dirty ? "1" : "0"}`;
    }

    isRejectionNotReady(value: unknown): boolean {
        return isWorldRejection(value as never) === true;
    }
}
