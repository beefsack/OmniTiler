// World adapter lifetime state (v4 producer era): sole-adapter generation
// binding, transport fences, and dirty/ack retention. The core owns ALL
// sequencing (obs/reqSeq/cmdSeq/cmdSeq) and its own clock: this adapter
// owns only two transport fences - the per-capture source correlation and
// the local native-invalidation watermark - plus the queue-client command
// counter. No KWin or D-Bus imports: the entry drives this with captured
// observations and transport results.

import { compareU64, incrementU64, isCanonicalU64, isOpaqueId } from "./world-seq";
import {
    encodeV4FocusFeedback,
    encodeV4StateAttempt,
    isWorldRejection,
    type FrameSetter,
    type WorldAfterShown,
    type WorldClearAttempt,
    type WorldCommand,
    type WorldCreationAttempt,
    type WorldFocusCause,
    type WorldFocusFeedback,
    type WorldFocusRequest,
    type WorldFrameAttempt,
    type WorldMemberAttempt,
    type WorldMemberRef,
    type WorldReply,
    type WorldReplyResult,
    type WorldStateAttempt,
    type WorldViewAttempt,
    type WorldViewRef,
} from "./world-report";

// One bounded retained focus-request record: the exact latest envelope
// tuple plus frozen dependencies and its spend bit. At most one lives
// here; older tuples retire into the monotonic `focusRetiredRev` fence
// (canonical u64, never an unbounded spent-id set). A fresh handshake
// resets both; transport loss keeping context retains them.
export interface FocusRequestRecord {
    id: string;
    target: string;
    rev: string;
    cause: WorldFocusCause;
    out: string;
    ws: string;
    cmdSeq: string | null;
    afterShown: WorldAfterShown | null;
    gen: string;
    spent: boolean;
}

// Freshness verdict for one observed focus-request envelope. `fresh` names
// the now-current unspent record (the caller spends before dispatch);
// `stale` refuses an old, foreign-generation, same-id-mismatched or
// same-revision-mismatched replay; `duplicate-spent` silences a
// re-observed spent record; `absent` retires the current record on a null
// envelope (no focus intent).
export type FocusEnvelope = "absent" | "stale" | "duplicate-spent" | "fresh";

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
// instead of converting to the implicit marker): it never rides the
// wire. clientCmd is minted at press; payload is the exact QueueWorldCommand
// bytes; filed tracks a filed-but-unresolved identity; filing tracks an
// in-flight QueueWorldCommand send; gen binds the serving generation.
export interface QueuedIntent {
    command: WorldCommand;
    keepSubject: boolean;
    clientCmd: string;
    payload: string;
    gen: string;
    filed: boolean;
    filing: boolean;
}

export interface CaptureFlight {
    source: string;
    watermark: string;
    // Queue-client command the fresh capture drains (FIFO head), if any.
    // Terminal results arrive requester-only via TakeCommandResult, never
    // in the capture reply.
    clientCmd: string | null;
    // Raw native gesture identity carried by this exact capture (terminal
    // phases only), or null. The ack after the core GestureSettled binds to
    // this flight identity plus the pending terminal, never to an arbitrary
    // accepted capture. The native window rides frozen at send: the settled
    // id is a numeric core ingress id, never a native row key.
    gesture: { instance: string; serial: string; win: string } | null;
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
    states: WorldStateAttempt[];
    focus: WorldFocusFeedback[];
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
// Latest-wins bound mirrors the core MAX_WORLD_STATE_ATTEMPTS cap: one
// row per window property, oldest-first eviction past the cap so a report
// can never refuse over-cap.
const MAX_STATE_ATTEMPTS = 1024;
// Latest-wins bound mirrors the core MAX_WORLD_FOCUS_FEEDBACK cap; the
// total arrival-bracket bound mirrors MAX_ARRIVAL_BRACKETS_TOTAL. Both
// evict oldest-first past the cap so a report can never refuse over-cap.
const MAX_FOCUS_FEEDBACK = 1024;
const MAX_FOCUS_ARRIVAL_TOTAL = 10_000;

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
    // Transfer/view attempts dispatched this generation (armed BEFORE the
    // first native setter). Dispatch-phase duplicate suppression only:
    // while an op is recorded here the apply loop never re-dispatches it,
    // so one stable operation actuates at most once per generation. The
    // recorded raw attempt rows ride until the bound reply consumes them;
    // the core owns every outcome (confirmation, refusal, supersession) and
    // retires its transfers through subsequent replies. Never cleared
    // except on generation rotation: a stale core repeat holds silently
    // rather than fighting the user with a second native write.
    dispatchedOps = new Set<string>();
    // Bounded retained focus-request accounting (at most one current record
    // plus a monotonic retired-revision fence, both canonical): the latest
    // publication adopts, older tuples retire into the fence, and a null
    // envelope retires the current record. Spend-once per bound id covers
    // native attempts and safety no-calls alike. Reset only on a fresh
    // handshake; transport loss keeping context retains both.
    focusCurrent: FocusRequestRecord | null = null;
    focusRetiredRev = "0";
    // Explicit retained float placements spent per stable core property
    // operation id (`frame_op`): the row token places exactly once, never
    // again for the same operation across duplicate or unrelated later
    // replies. Rows without the token (ordinary floats) never place.
    // `clear_op` spends exactly once the same way.
    floatSpent = new Set<string>();
    clearSpent = new Set<string>();
    // Last proven native targets, for last-moment external-input safety:
    // a repeat of the SAME operation after its subject demonstrably left
    // the proven target holds instead of re-actuating (never fights a user
    // switch-back while the core settles the stale repeat from fresh native
    // facts). Proven-target proof only, never an outcome verdict: nothing
    // here settles, retires, or reaps core transfers. Operation ids are
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
    // nullable DPI; clears carry the pre/post maximized states.
    frameAttempts = new Map<string, WorldFrameAttempt>();
    memberAttempts = new Map<string, WorldMemberAttempt>();
    viewAttempts = new Map<string, WorldViewAttempt>();
    clearAttempts = new Map<string, WorldClearAttempt>();
    // Raw native-state write/readback facts (latest-wins per window
    // property) drained into every report until a bound ready reply
    // consumes the delivered snapshot. Redelivery reuses the original
    // write stamps; the core consumes each idempotently. Rows carry the
    // exact core native-state target (property/target/authorizing rev),
    // the actual setter disposition, and nullable pre/post property
    // reads. No native spending happens here: this unit retains evidence
    // only, for the later state-spending lane to record through.
    stateAttempts = new Map<string, WorldStateAttempt>();
    // Correlated one-shot focus evidence (latest-wins per spent focus
    // request id) drained into every report until a bound ready reply consumes
    // the delivered snapshot. Redelivery reuses the original write
    // stamps; the core consumes each idempotently. Effects spend
    // at-most-once per generation, so one id retries never.
    focusFeedbacks = new Map<string, WorldFocusFeedback>();
    // Executed-but-unproven creation evidence (op spent, post-call
    // readback unavailable): never emitted, completed once against the
    // first successful later full native-order readback. Cleared with
    // everything else on generation rotation.
    creationPending = new Map<string, PendingCreationReceipt>();
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

    // Arm the dispatch record BEFORE any native setter for an operation.
    // A recorded op never re-dispatches this generation (at-most-once
    // native actuation per stable operation). The armed raw attempt rows
    // ride the report lane until the bound reply consumes them; the core
    // owns settlement. Never cleared except on generation rotation.
    markDispatched(op: string): void {
        this.dispatchedOps.add(op);
        this.dirty = true;
    }

    isDispatched(op: string): boolean {
        return this.dispatchedOps.has(op);
    }

    // Schedule a follow-up capture after this apply resolved something the
    // core must observe promptly (a spent token, recorded proof, or held
    // retained desire). Pure transport scheduling, never a verdict: the
    // next capture carries the fresh facts and the core owns every
    // outcome. Counterpart to the record* methods, which all dirty the
    // same way when they store a row.
    noteFollowup(): void {
        this.dirty = true;
    }

    // Observe one coherent latest focus-request envelope. `null` retires
    // the current record into the fence (no focus intent: never actuate,
    // and old cached replays later refuse as stale). An exact full-tuple
    // plus frozen-dependency match on the bound generation returns `fresh`
    // (unspent) or `duplicate-spent` (spent). A foreign-generation envelope
    // refuses as `stale` (a new handshake reusing the same id/rev never
    // adopts over the old session, and an old cached replay never adopts
    // on the new binding). A same-id envelope with any tuple/dependency
    // mismatch refuses as `stale` (re-emission never changes id, target,
    // issuing revision, cause, command correlation or frozen dependencies).
    // Any other same-generation tuple adopts only when strictly newer than
    // both the current record and the fence (stable requests may first
    // arrive on a later envelope); same-revision mismatches and older
    // revisions refuse as `stale`. No cause-specific, precedence or
    // hover-dedupe logic: the caller applies the latest publication only.
    observeFocusEnvelope(req: WorldFocusRequest | null, gen: string): FocusEnvelope {
        if (req === null) {
            if (this.focusCurrent !== null) {
                if (compareU64(this.focusCurrent.rev, this.focusRetiredRev) > 0) {
                    this.focusRetiredRev = this.focusCurrent.rev;
                }
                this.focusCurrent = null;
            }
            return "absent";
        }
        if (this.boundGen === null || gen !== this.boundGen) {
            return "stale";
        }
        const frozen = JSON.stringify(req.afterShown ?? null);
        const current = this.focusCurrent;
        if (current !== null && current.id === req.id && current.gen === gen) {
            const same = current.target === req.target && current.rev === req.rev &&
                current.cause === req.cause && current.out === req.out && current.ws === req.ws &&
                current.cmdSeq === (req.cmdSeq ?? null) &&
                JSON.stringify(current.afterShown ?? null) === frozen;
            if (!same) {
                return "stale";
            }
            return current.spent ? "duplicate-spent" : "fresh";
        }
        if (compareU64(req.rev, this.focusRetiredRev) <= 0) {
            return "stale";
        }
        if (current !== null) {
            if (compareU64(req.rev, current.rev) <= 0) {
                return "stale";
            }
            if (compareU64(current.rev, this.focusRetiredRev) > 0) {
                this.focusRetiredRev = current.rev;
            }
        }
        this.focusCurrent = {
            id: req.id, target: req.target, rev: req.rev,
            cause: req.cause, out: req.out, ws: req.ws,
            cmdSeq: req.cmdSeq ?? null, afterShown: req.afterShown ?? null,
            gen, spent: false,
        };
        return "fresh";
    }

    // Spend the current focus-request record by exact id. Marks spent and
    // raises the fence to the record revision; false on no current record,
    // id mismatch, or an already-spent record (duplicate spend blocked).
    // Same-generation reconnects preserve the spent bit (only a fresh
    // handshake or a null/newer envelope retires it).
    spendFocus(id: string): boolean {
        const current = this.focusCurrent;
        if (current === null || current.id !== id || current.spent) {
            return false;
        }
        current.spent = true;
        if (compareU64(current.rev, this.focusRetiredRev) > 0) {
            this.focusRetiredRev = current.rev;
        }
        return true;
    }

    // Whether the given focus identity is still the current record: exact
    // full-tuple plus frozen-dependency match on the bound generation
    // against the live record. A null envelope, a newer publication, or a
    // generation move retires it, so a delayed owner result naming anything
    // else is stale and must never stage. Spend state is ignored: the
    // dispatched request spends at dispatch, before its result arrives.
    isCurrentFocusRequest(req: WorldFocusRequest, gen: string): boolean {
        const current = this.focusCurrent;
        return current !== null &&
            current.id === req.id && current.target === req.target && current.rev === req.rev &&
            current.cause === req.cause && current.out === req.out && current.ws === req.ws &&
            current.cmdSeq === (req.cmdSeq ?? null) &&
            JSON.stringify(current.afterShown ?? null) === JSON.stringify(req.afterShown ?? null) &&
            current.gen === gen;
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
    // receipt, so the attempt retires and a follow-up observes the outcome.
    // Never invents binding locally: rows without a core bind keep their
    // receipts for the next report.
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
                this.noteFollowup();
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
    // op/win/out/ws, null-or-shaped WorkspaceRefs that are never
    // translated or rewritten). The refs carry the exact live homing in
    // the namespace the host capability allows; the core translates them
    // through its own bindings.
    recordMemberAttempt(attempt: WorldMemberAttempt): boolean {
        const refOk = (r: WorldMemberRef | null): boolean =>
            r === null || ((r.kind === "logical" || r.kind === "native") &&
                isOpaqueId(r.out) && isOpaqueId(r.ws));
        if (
            !isOpaqueId(attempt.op) || !isOpaqueId(attempt.win) ||
            !isOpaqueId(attempt.out) || !isOpaqueId(attempt.ws) ||
            !isSetter(attempt.setter) || !isStamp(attempt.attemptMs) ||
            !refOk(attempt.pre) || !refOk(attempt.post)
        ) {
            return false;
        }
        const clean: WorldMemberAttempt = {
            ...attempt,
            pre: attempt.pre === null ? null : { ...attempt.pre },
            post: attempt.post === null ? null : { ...attempt.post },
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
    // phase; null refs ride as unknown (never forged proof), and only the
    // native namespace is admitted: a logical view ref has no wire
    // spelling and refuses here, never reaching the report lane.
    recordViewAttempt(attempt: WorldViewAttempt): boolean {
        const refOk = (r: WorldViewRef | null): boolean =>
            r === null || (r.kind === "native" && isOpaqueId(r.out) && isOpaqueId(r.ws));
        if (
            !isOpaqueId(attempt.op) || !isOpaqueId(attempt.out) || !isOpaqueId(attempt.ws) ||
            !isSetter(attempt.setter) || !isStamp(attempt.attemptMs) ||
            !refOk(attempt.pre) || !refOk(attempt.post) || !refOk(attempt.base)
        ) {
            return false;
        }
        const clean: WorldViewAttempt = {
            ...attempt,
            pre: attempt.pre === null ? null : { ...attempt.pre },
            post: attempt.post === null ? null : { ...attempt.post },
            base: attempt.base === null ? null : { ...attempt.base },
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
    // (operation, window). Production records every dispatch disposition
    // (accepted/refused/threw with fresh pre/post maximized states); the
    // core owns confirmation, proven refusal, ambiguity, and abandonment.
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

    // Record one raw native-state attempt. Latest-wins per (window,
    // property): the core refuses two rows for one property as
    // duplicate-state-attempt, so a newer tuple for the same property
    // replaces its predecessor while an unrelated property (or window)
    // rides alongside. A same-tuple replay (identical window, property,
    // target, and authorizing rev) preserves the FIRST row: the original
    // stamp is never replaced by contradictory replay evidence.
    // Validation reuses the canonical wire writer (unknown property,
    // non-boolean target, bad rev, unknown setter, non-boolean pre/post,
    // or a non-canonical stamp refuses, never stored). Redelivery reuses
    // the stored stamp: callers never restamp.
    recordStateAttempt(attempt: WorldStateAttempt): boolean {
        if (encodeV4StateAttempt(attempt) === null) {
            return false;
        }
        const key = `${attempt.win}\u0000${attempt.property}`;
        const live = this.stateAttempts.get(key);
        if (
            live !== undefined &&
            live.target === attempt.target && live.rev === attempt.rev
        ) {
            return true;
        }
        this.stateAttempts.set(key, { ...attempt });
        while (this.stateAttempts.size > MAX_STATE_ATTEMPTS) {
            const oldest = this.stateAttempts.keys().next();
            if (oldest.done === true) {
                break;
            }
            this.stateAttempts.delete(oldest.value as string);
        }
        this.dirty = true;
        return true;
    }

    // Record one correlated one-shot focus evidence row. Latest-wins per
    // spent focus-request id. Validation reuses the canonical wire writer
    // (encodeV4FocusFeedback fails malformed rows closed: bad
    // id/target/rev, setter, reason, pre/post shapes, non-boolean proof,
    // bad brackets, or stamp); invalid rows refuse, never stored.
    // Redelivery reuses the stored stamp: callers never restamp.
    // Oldest-first eviction keeps both the row cap and the total
    // arrival-bracket cap so the report lane never refuses over-cap.
    recordFocusFeedback(row: WorldFocusFeedback): boolean {
        if (encodeV4FocusFeedback(row) === null) {
            return false;
        }
        const clean: WorldFocusFeedback = {
            ...row,
            arrival: row.arrival.map((b) => ({
                ...b,
                before: { ...b.before },
                after: { ...b.after },
                home: b.home === null ? null : { ...b.home },
                frame: b.frame === null ? null : [...b.frame] as [number, number, number, number],
            })),
        };
        this.focusFeedbacks.set(clean.id, clean);
        const arrivalTotal = (): number => {
            let total = 0;
            for (const r of this.focusFeedbacks.values()) {
                total += r.arrival.length;
            }
            return total;
        };
        while (this.focusFeedbacks.size > MAX_FOCUS_FEEDBACK || arrivalTotal() > MAX_FOCUS_ARRIVAL_TOTAL) {
            const oldest = this.focusFeedbacks.keys().next();
            if (oldest.done === true) {
                break;
            }
            this.focusFeedbacks.delete(oldest.value as string);
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

    // Ordered retained native-state rows for the report lane (sorted by
    // window, property, target, then authorizing rev for determinism).
    // Rows are flat facts, so a shallow copy snapshots each row: later
    // mutation of a returned row cannot rewrite a delivered snapshot.
    statesForReport(): WorldStateAttempt[] {
        return [...this.stateAttempts.values()]
            .sort((a, b) => {
                const ka = `${a.win}\u0000${a.property}\u0000${a.target ? "1" : "0"}\u0000${a.rev}`;
                const kb = `${b.win}\u0000${b.property}\u0000${b.target ? "1" : "0"}\u0000${b.rev}`;
                return ka < kb ? -1 : ka > kb ? 1 : 0;
            })
            .map((a) => ({ ...a }));
    }

    // Ordered retained focus rows for the report lane (sorted by effect
    // id for determinism). Copies bracket tuples so later mutation of a
    // retained row cannot rewrite a delivered snapshot.
    focusFeedbacksForReport(): WorldFocusFeedback[] {
        return [...this.focusFeedbacks.values()]
            .sort((a, b) => (a.id < b.id ? -1 : a.id > b.id ? 1 : 0))
            .map((a) => ({
                ...a,
                arrival: a.arrival.map((b) => ({
                    ...b,
                    before: { ...b.before },
                    after: { ...b.after },
                    home: b.home === null ? null : { ...b.home },
                    frame: b.frame === null ? null : [...b.frame] as [number, number, number, number],
                })),
            }));
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
            a.setter === b.setter &&
            JSON.stringify(a.pre) === JSON.stringify(b.pre) &&
            JSON.stringify(a.post) === JSON.stringify(b.post) &&
            JSON.stringify(a.base) === JSON.stringify(b.base) &&
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
        // State rows retire by exact full-row equality against the
        // delivered snapshot: a newer tuple for the same window property
        // never equals its predecessor, so it survives the older ack
        // while an unchanged redelivery retires. Unrelated properties
        // (different keys) are untouched by construction.
        const sameState = (a: WorldStateAttempt, b: WorldStateAttempt): boolean =>
            a.win === b.win && a.property === b.property && a.target === b.target &&
            a.rev === b.rev && a.setter === b.setter &&
            a.pre === b.pre && a.post === b.post && a.attemptMs === b.attemptMs;
        for (const sent of this.inFlight.delivered.states) {
            const live = this.stateAttempts.get(`${sent.win}\u0000${sent.property}`);
            if (live !== undefined && sameState(live, sent)) {
                this.stateAttempts.delete(`${sent.win}\u0000${sent.property}`);
                retired += 1;
            }
        }
        // Focus rows retire by exact full-row equality (request correlation
        // plus disposition plus brackets plus stamp): a newer row for the
        // same request id never exists (spent at-most-once), so equality
        // against the delivered snapshot is sufficient.
        for (const sent of this.inFlight.delivered.focus) {
            const live = this.focusFeedbacks.get(sent.id);
            if (live !== undefined && JSON.stringify(live) === JSON.stringify(sent)) {
                this.focusFeedbacks.delete(sent.id);
                retired += 1;
            }
        }
        return retired;
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

    // Freshness acceptance: only a bound-generation, producer-adapter,
    // source-correlated reply whose watermark names the CURRENT local fence
    // mismatch-or-match applies setters. Anything else is observation-only
    // catchup with no setters. Single-flight alone never authenticates: a
    // delayed arbitrary reply (wrong adapter or wrong source, even with
    // the same gen) applies nothing and acks nothing. A watermark
    // advance since dispatch (native input landed during flight) still
    // applies, then schedules follow-up via dirty.
    acceptReply(reply: WorldReply, dispatchSource: string, dispatchWatermark: string): { applied: boolean; reason: string } {
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
            gesture: null,
            delivered: {
                frames: this.framesForReport(),
                members: this.memberAttemptsForReport(),
                views: this.viewAttemptsForReport(),
                clears: this.clearAttemptsForReport(),
                states: this.statesForReport(),
                focus: this.focusFeedbacksForReport(),
            },
        };
        this.inFlight = flight;
        this.dirty = false;
        this.observationOnlyCatchup = false;
        return flight;
    }

    // Replay flight for an exact same-envelope terminal redelivery: the
    // bytes name the ORIGINAL watermark, so the flight binding must stamp
    // it too (beginCapture would stamp the current fence instead). Source
    // and generation bindings are unchanged: only the watermark rides back.
    beginReplayCapture(source: string, watermark: string, clientCmd: string | null): CaptureFlight | null {
        const flight = this.beginCapture(source, clientCmd);
        if (flight !== null) {
            flight.watermark = watermark;
        }
        return flight;
    }

    onDiscovery(gen: string, adapter?: string, req?: string, expectedReq?: string): boolean {
        // Authenticated discovery binds the sole adapter producer lifetime
        // plus the serving core generation, for the exact adapter namespace
        // and the exact outstanding discovery req. A delayed arbitrary reply
        // (different adapter or wrong req) binds nothing and resets nothing.
        if (adapter !== undefined && adapter !== this.adapter) {
            return false;
        }
        if (expectedReq !== undefined && req !== expectedReq) {
            return false;
        }
        // Every actual handshake resets (the core resets on every handshake),
        // even a repeat equal gen: an authenticated fresh handshake rotates
        // flights, ordering, and desired caches, never native wins. The
        // queue-client namespace restarts at 1: commands queued under a
        // dead generation settle there and are uncollectible here.
        this.boundGen = gen;
        this.inFlight = null;
        this.lastReply = null;
        this.lastTakeResult = null;
        this.hasAccepted = false;
        this.lastAcceptedRev = "0";
        this.nextClientCmd = "1";
        this.dispatchedOps.clear();
        this.focusCurrent = null;
        this.focusRetiredRev = "0";
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
        this.stateAttempts.clear();
        this.focusFeedbacks.clear();
        return true;
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
