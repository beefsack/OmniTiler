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
    type FrameSetter,
    type WorldClearAttempt,
    type WorldCommand,
    type WorldCreationAttempt,
    type WorldFrameAttempt,
    type WorldMemberAttempt,
    type WorldReply,
    type WorldReplyResult,
    type WorldViewAttempt,
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
    // Exact facts this flight delivered (snapshots of the pending attempt
    // maps at dispatch): a bound ready reply consumes exactly-equal
    // entries, so a lost acknowledgement redelivers the same attempt
    // stamps while a newer update for the same key survives its
    // predecessor's ack. Never a verdict: the core owns every outcome.
    delivered: AttemptFlight;
}

// Mechanical write/readback facts delivered by one capture flight (see
// CaptureFlight.delivered). Snapshots only; the live pending maps keep
// latest-wins until the bound reply consumes them.
export interface AttemptFlight {
    frames: WorldFrameAttempt[];
    members: WorldMemberAttempt[];
    views: WorldViewAttempt[];
    clears: WorldClearAttempt[];
}

// Latest-wins bounds mirror the core report caps (world_report.rs
// MAX_WORLD_*): eviction drops the oldest entry past the cap so a report
// can never refuse over-cap. Clears share the member cap scale; the lane
// stays empty in production (no native clear actuation) but the bound
// keeps the shape honest.
const MAX_FRAME_ATTEMPTS = 1024;
const MAX_MEMBER_ATTEMPTS = 1024;
const MAX_VIEW_ATTEMPTS = 1024;
const MAX_CLEAR_ATTEMPTS = 1024;

function isSetter(value: unknown): value is FrameSetter {
    return value === "accepted" || value === "refused" || value === "threw";
}

function isRectTuple(value: unknown): value is [number, number, number, number] {
    return Array.isArray(value) && value.length === 4 &&
        (value as ReadonlyArray<unknown>).every((n) => typeof n === "number" && Number.isInteger(n));
}

function isStamp(value: unknown): value is string {
    return isCanonicalU64(value);
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
    // Raw mechanical write/readback facts (latest-wins per window,
    // operation window, owning view phase, and clear operation
    // respectively) drained into every report until a bound ready reply
    // consumes the delivered snapshot. Redelivery reuses the original
    // write stamps; the core consumes each idempotently. Host-owned
    // member/view rows carry null native-derived evidence (never
    // translated logical receipts); frames carry full rect evidence with
    // nullable DPI; clears stay empty (no native clear actuation).
    frameAttempts = new Map<string, WorldFrameAttempt>();
    memberAttempts = new Map<string, WorldMemberAttempt>();
    viewAttempts = new Map<string, WorldViewAttempt>();
    clearAttempts = new Map<string, WorldClearAttempt>();
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

    // Record one raw mechanical frame attempt for the next report lane.
    // Latest-wins per window; invalid rows (bad window id, setter, stamp,
    // target/readback/pre shapes, or a prewrite without an operation id)
    // refuse, never stored. Redelivery reuses the stored stamp: callers
    // never restamp.
    recordFrameAttempt(attempt: WorldFrameAttempt): boolean {
        if (
            !isOpaqueId(attempt.win) || !isSetter(attempt.setter) || !isStamp(attempt.attemptMs) ||
            !isRectTuple(attempt.target) ||
            (attempt.readback !== null && !isRectTuple(attempt.readback)) ||
            (attempt.dpiBefore !== null && !(Number.isInteger(attempt.dpiBefore) && (attempt.dpiBefore as number) >= 0)) ||
            (attempt.dpiAfter !== null && !(Number.isInteger(attempt.dpiAfter) && (attempt.dpiAfter as number) >= 0)) ||
            typeof attempt.op !== "string" ||
            (attempt.pre !== null && !isRectTuple(attempt.pre)) ||
            (attempt.op === "" && attempt.pre !== null)
        ) {
            return false;
        }
        this.frameAttempts.set(attempt.win, { ...attempt, target: [...attempt.target] as [number, number, number, number] });
        while (this.frameAttempts.size > MAX_FRAME_ATTEMPTS) {
            const oldest = this.frameAttempts.keys().next();
            if (oldest.done === true) {
                break;
            }
            this.frameAttempts.delete(oldest.value as string);
        }
        this.dirty = true;
        return true;
    }

    // Record one raw mechanical membership attempt. Latest-wins per
    // (operation, window); same refusal rules as frames (opaque
    // op/win/out/ws, null-or-shaped homings that are never translated
    // logical receipts). Fresh same-turn native desktop ids ride
    // nativePre/nativePost (null/empty unknown, opaque otherwise); the
    // core translates them through its own bindings.
    recordMemberAttempt(attempt: WorldMemberAttempt): boolean {
        const homingOk = (h: { out: string; ws: string } | null): boolean =>
            h === null || (isOpaqueId(h.out) && isOpaqueId(h.ws));
        const nativeOk = (v: string | null | undefined): boolean =>
            v === null || v === undefined || v === "" || isOpaqueId(v);
        if (
            !isOpaqueId(attempt.op) || !isOpaqueId(attempt.win) ||
            !isOpaqueId(attempt.out) || !isOpaqueId(attempt.ws) ||
            !isSetter(attempt.setter) || !isStamp(attempt.attemptMs) ||
            !homingOk(attempt.pre) || !homingOk(attempt.post) ||
            !nativeOk(attempt.nativePre) || !nativeOk(attempt.nativePost)
        ) {
            return false;
        }
        const clean: WorldMemberAttempt = {
            ...attempt,
            nativePre: attempt.nativePre === "" ? null : (attempt.nativePre ?? null),
            nativePost: attempt.nativePost === "" ? null : (attempt.nativePost ?? null),
        };
        this.memberAttempts.set(`${clean.op}\u0000${clean.win}`, { ...clean });
        while (this.memberAttempts.size > MAX_MEMBER_ATTEMPTS) {
            const oldest = this.memberAttempts.keys().next();
            if (oldest.done === true) {
                break;
            }
            this.memberAttempts.delete(oldest.value as string);
        }
        this.dirty = true;
        return true;
    }

    // Record one raw mechanical view attempt. Latest-wins per owning view
    // phase; empty pre/post/base/nativePre/nativePost/nativeBase normalize
    // to null (unknown, like the wire decoder), never to forged proof.
    recordViewAttempt(attempt: WorldViewAttempt): boolean {
        const viewOk = (v: string | null | undefined): boolean =>
            v === null || v === undefined || v === "" || isOpaqueId(v);
        if (
            !isOpaqueId(attempt.op) || !isOpaqueId(attempt.out) || !isOpaqueId(attempt.ws) ||
            !isSetter(attempt.setter) || !isStamp(attempt.attemptMs) ||
            !viewOk(attempt.pre) || !viewOk(attempt.post) || !viewOk(attempt.base) ||
            !viewOk(attempt.nativePre) || !viewOk(attempt.nativePost) || !viewOk(attempt.nativeBase)
        ) {
            return false;
        }
        const clean: WorldViewAttempt = {
            ...attempt,
            pre: attempt.pre === "" ? null : attempt.pre,
            post: attempt.post === "" ? null : attempt.post,
            base: attempt.base === "" ? null : attempt.base,
            nativePre: attempt.nativePre === "" ? null : (attempt.nativePre ?? null),
            nativePost: attempt.nativePost === "" ? null : (attempt.nativePost ?? null),
            nativeBase: attempt.nativeBase === "" ? null : (attempt.nativeBase ?? null),
        };
        this.viewAttempts.set(clean.op, clean);
        while (this.viewAttempts.size > MAX_VIEW_ATTEMPTS) {
            const oldest = this.viewAttempts.keys().next();
            if (oldest.done === true) {
                break;
            }
            this.viewAttempts.delete(oldest.value as string);
        }
        this.dirty = true;
        return true;
    }

    // Record one raw mechanical maximize-clear attempt. Latest-wins per
    // (operation, window). Production never calls this (no native clear
    // actuation); the shape stays honest for the parked lane.
    recordClearAttempt(attempt: WorldClearAttempt): boolean {
        const flagOk = (v: boolean | null): boolean => v === null || typeof v === "boolean";
        if (
            !isOpaqueId(attempt.op) || !isOpaqueId(attempt.win) ||
            !isSetter(attempt.setter) || !isStamp(attempt.attemptMs) ||
            !flagOk(attempt.pre) || !flagOk(attempt.post)
        ) {
            return false;
        }
        this.clearAttempts.set(`${attempt.op}\u0000${attempt.win}`, { ...attempt });
        while (this.clearAttempts.size > MAX_CLEAR_ATTEMPTS) {
            const oldest = this.clearAttempts.keys().next();
            if (oldest.done === true) {
                break;
            }
            this.clearAttempts.delete(oldest.value as string);
        }
        this.dirty = true;
        return true;
    }

    // Ordered retained facts for the report lane (sorted like the
    // Windows reference: windows, (operation, window), operations).
    framesForReport(): WorldFrameAttempt[] {
        return [...this.frameAttempts.values()]
            .sort((a, b) => (a.win < b.win ? -1 : a.win > b.win ? 1 : 0))
            .map((a) => ({ ...a, target: [...a.target] as [number, number, number, number] }));
    }

    memberAttemptsForReport(): WorldMemberAttempt[] {
        return [...this.memberAttempts.values()]
            .sort((a, b) => {
                const ka = `${a.op}\u0000${a.win}`;
                const kb = `${b.op}\u0000${b.win}`;
                return ka < kb ? -1 : ka > kb ? 1 : 0;
            })
            .map((a) => ({ ...a }));
    }

    viewAttemptsForReport(): WorldViewAttempt[] {
        return [...this.viewAttempts.values()]
            .sort((a, b) => (a.op < b.op ? -1 : a.op > b.op ? 1 : 0))
            .map((a) => ({ ...a }));
    }

    clearAttemptsForReport(): WorldClearAttempt[] {
        return [...this.clearAttempts.values()]
            .sort((a, b) => {
                const ka = `${a.op}\u0000${a.win}`;
                const kb = `${b.op}\u0000${b.win}`;
                return ka < kb ? -1 : ka > kb ? 1 : 0;
            })
            .map((a) => ({ ...a }));
    }

    // Consume the delivered snapshot of one bound flight: a bound ready
    // reply proves the core incorporated the delivered facts, so
    // exactly-equal entries retire while a newer update for the same key
    // survives. Unbound, not-ready, or unknown-source replies consume
    // nothing; a lost acknowledgement keeps the stamps for redelivery.
    // Returns the retired entry count.
    ackDelivered(source: string, ready: boolean): number {
        if (!ready || this.inFlight === null || this.inFlight.source !== source) {
            return 0;
        }
        let retired = 0;
        const sameFrame = (a: WorldFrameAttempt, b: WorldFrameAttempt): boolean =>
            a.win === b.win && a.target.join(",") === b.target.join(",") &&
            a.setter === b.setter &&
            (a.readback?.join(",") ?? "") === (b.readback?.join(",") ?? "") &&
            a.dpiBefore === b.dpiBefore && a.dpiAfter === b.dpiAfter &&
            a.attemptMs === b.attemptMs && a.op === b.op &&
            (a.pre?.join(",") ?? "") === (b.pre?.join(",") ?? "");
        for (const sent of this.inFlight.delivered.frames) {
            const live = this.frameAttempts.get(sent.win);
            if (live !== undefined && sameFrame(live, sent)) {
                this.frameAttempts.delete(sent.win);
                retired += 1;
            }
        }
        const sameMember = (a: WorldMemberAttempt, b: WorldMemberAttempt): boolean =>
            a.op === b.op && a.win === b.win && a.out === b.out && a.ws === b.ws &&
            a.setter === b.setter &&
            JSON.stringify(a.pre) === JSON.stringify(b.pre) &&
            JSON.stringify(a.post) === JSON.stringify(b.post) &&
            (a.nativePre ?? "") === (b.nativePre ?? "") &&
            (a.nativePost ?? "") === (b.nativePost ?? "") &&
            a.attemptMs === b.attemptMs;
        for (const sent of this.inFlight.delivered.members) {
            const live = this.memberAttempts.get(`${sent.op}\u0000${sent.win}`);
            if (live !== undefined && sameMember(live, sent)) {
                this.memberAttempts.delete(`${sent.op}\u0000${sent.win}`);
                retired += 1;
            }
        }
        const sameView = (a: WorldViewAttempt, b: WorldViewAttempt): boolean =>
            a.op === b.op && a.out === b.out && a.ws === b.ws &&
            a.setter === b.setter && (a.pre ?? "") === (b.pre ?? "") &&
            (a.post ?? "") === (b.post ?? "") && (a.base ?? "") === (b.base ?? "") &&
            (a.nativePre ?? "") === (b.nativePre ?? "") &&
            (a.nativePost ?? "") === (b.nativePost ?? "") &&
            (a.nativeBase ?? "") === (b.nativeBase ?? "") &&
            a.attemptMs === b.attemptMs;
        for (const sent of this.inFlight.delivered.views) {
            const live = this.viewAttempts.get(sent.op);
            if (live !== undefined && sameView(live, sent)) {
                this.viewAttempts.delete(sent.op);
                retired += 1;
            }
        }
        const sameClear = (a: WorldClearAttempt, b: WorldClearAttempt): boolean =>
            a.op === b.op && a.win === b.win && a.setter === b.setter &&
            a.pre === b.pre && a.post === b.post && a.attemptMs === b.attemptMs;
        for (const sent of this.inFlight.delivered.clears) {
            const live = this.clearAttempts.get(`${sent.op}\u0000${sent.win}`);
            if (live !== undefined && sameClear(live, sent)) {
                this.clearAttempts.delete(`${sent.op}\u0000${sent.win}`);
                retired += 1;
            }
        }
        return retired;
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
        // A bound validated ready reply proves the core incorporated the
        // delivered facts (attempts included): retire exactly-equal
        // delivered entries. Newer updates for the same keys survive.
        this.ackDelivered(dispatchSource, true);
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
        // Snapshot the exact facts this flight delivers: the bound reply
        // consumes exactly-equal entries, so redelivery after a lost
        // acknowledgement reuses the original write stamps.
        const flight: CaptureFlight = {
            source, watermark: this.currentInvalid, clientCmd,
            delivered: {
                frames: this.framesForReport(),
                members: this.memberAttemptsForReport(),
                views: this.viewAttemptsForReport(),
                clears: this.clearAttemptsForReport(),
            },
        };
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
            this.frameAttempts.clear();
            this.memberAttempts.clear();
            this.viewAttempts.clear();
            this.clearAttempts.clear();
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
