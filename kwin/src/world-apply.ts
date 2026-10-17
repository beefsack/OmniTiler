// Safe actuation of a full desired snapshot (D2/D3): per-property diffs
// against the latest actual host observation, with safety, gesture,
// lifetime, and freshness rechecked immediately before every setter and
// during apply. Canonical allocation versus effective frames, bounded triple
// clamp reassertions, no authority over game/excluded/overlay rows, float
// sends as membership without frame writes, and one-shot effects spent
// before dispatch and never repeated.
//
// Identity rule: membership/view actuation reports under the stable
// core transfer operation id (`reply.transfers[].op`, `op-<cmdSeq>`)
// exactly, never an invented `send-<win>`/`view-<win>` token, so feedback
// settles the retained core target. Every possibly-executing transfer/view
// arms possibly-executed BEFORE the native setter and never repeats while
// ambiguous; Applied is marked only on fresh read-back proof (a setter
// boolean alone is not proof). Throws and partial writes stay ambiguous;
// refused-proven requires fresh proof that nothing happened or an explicit
// native refusal. External input that demonstrably moved the subject
// elsewhere settles superseded-external; mere mismatches hold.

import type { ObservedWindow, WorldObservation } from "./world-observe";
import { type WorldReply, type WorldReplyTransfer, type WorldReplyWindow } from "./world-report";
import { WorldAdapterState } from "./world-state";

export interface ApplyEnv {
    state: WorldAdapterState;
    observation: WorldObservation;
    reply: WorldReply;
    log: (message: string) => void;
    // Native setters through public KWin state only.
    isStillListed: (ref: object) => boolean;
    isActiveWindow: (ref: object) => boolean;
    readLiveRect: (ref: object) => { x: number; y: number; w: number; h: number } | null;
    readLiveMembership: (ref: object) => { out: string; ws: string } | null;
    // Native overlay lifetime recheck immediately before a setter: null
    // (unreadable) fails closed with a narrow hold. Defined booleans must
    // match the observation.
    readLiveSafety: (ref: object) => {
        minimized: boolean | null;
        fullscreen: boolean | null;
        maximized: boolean | null;
        sticky: boolean | null;
    } | null;
    setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => boolean;
    setMembership: (ref: object, wsNativeId: string, wsRef: object | null) => boolean;
    sendToOutput: (ref: object, outRef: object | null) => boolean;
    resolveOutput: (out: string) => object | null;
    resolveDesktop: (ws: string) => object | null;
    switchView: (desktopRef: object, outRef: object) => boolean;
    createDesktop: () => string | null;
    removeDesktop: (nativeId: string) => boolean;
    focusWindow: (ref: object) => boolean;
    desktopOccupants: (nativeId: string) => { total: number; readable: boolean };
    desktopVisible: (nativeId: string) => boolean | null;
    // Per-output live view (6.7): the current desktop id actually shown on
    // exactly `out`, or null when unreadable. Follow/view/migration proof
    // must use this (target output exact), never any-output visibility: a
    // desktop visible on the wrong output proves nothing.
    readLiveView: (out: string) => string | null;
    desktopById: (nativeId: string) => object | null;
    minDesktops: number;
}

export interface ApplyResult {
    geometryWrites: number;
    membershipWrites: number;
    viewWrites: number;
    creations: number;
    removals: number;
    effects: number;
    held: number;
    stoppedMidApply: boolean;
}

function sameRect(a: { x: number; y: number; w: number; h: number }, b: [number, number, number, number]): boolean {
    return a.x === b[0] && a.y === b[1] && a.w === b[2] && a.h === b[3];
}

function clampKeyFor(win: string, frame: [number, number, number, number], observed: ObservedWindow): string {
    const hints = `min${observed.hints.minW ?? "-"}/${observed.hints.minH ?? "-"}max${observed.hints.maxW ?? "-"}/${observed.hints.maxH ?? "-"}`;
    return `${win}|${frame[0]},${frame[1]},${frame[2]},${frame[3]}|${hints}`;
}

// Reply freshness at setter time: the bound generation still matches, the
// accepted obs/rev still name this reply (CURRENT watermark, not a stale
// dispatch copy), and the watermark has not advanced since apply started.
// Self-sent feedback dirty alone never stops apply; only a moved watermark
// (native external input, including synchronous setter echoes that have not
// yet converged) or observation-only catchup does.
function replyFresh(env: ApplyEnv, reply: WorldReply, entryObs: string): boolean {
    const { state } = env;
    if (state.observationOnlyCatchup) {
        return false;
    }
    if (state.boundGen === null || state.boundGen !== reply.gen) {
        return false;
    }
    if (state.hasAccepted && (reply.obs !== state.lastAcceptedObs || reply.rev !== state.lastAcceptedRev)) {
        return false;
    }
    if (reply.obs !== entryObs || state.currentObs !== entryObs) {
        return false;
    }
    return true;
}

function midApplyStopped(env: ApplyEnv, entryObs: string): boolean {
    return env.state.observationOnlyCatchup || env.state.currentObs !== entryObs;
}

// Freshness/safety recheck immediately before a setter: the ref must still
// be listed, still homed where expected (observed homing, or the expected
// echo after a first membership step), still in the observed overlay
// lifetime (a flip to minimized/fullscreen/maximized/sticky since the
// observation fails closed; unreadable safety fails closed too), the
// generation still bound to this accepted reply, the watermark still
// current, and no gesture barrier for geometry.
function preSetterOk(
    env: ApplyEnv,
    reply: WorldReply,
    entryObs: string,
    observed: ObservedWindow,
    needGeometry: boolean,
    expectLive?: { out: string; ws: string },
): boolean {
    const { state, observation } = env;
    if (!replyFresh(env, reply, entryObs)) {
        return false;
    }
    void state;
    if (needGeometry && observation.gesture) {
        return false;
    }
    if (!env.isStillListed(observed.ref)) {
        return false;
    }
    let live: { out: string; ws: string } | null = null;
    try {
        live = env.readLiveMembership(observed.ref);
    } catch (error) {
        void error;
        return false;
    }
    const want = expectLive ?? { out: observed.out, ws: observed.ws };
    if (live === null || live.out !== want.out || live.ws !== want.ws) {
        return false;
    }
    let safety: ReturnType<ApplyEnv["readLiveSafety"]> = null;
    try {
        safety = env.readLiveSafety(observed.ref);
    } catch (error) {
        void error;
        return false;
    }
    if (safety === null) {
        return false;
    }
    if (safety.minimized === true && !observed.minimized) {
        return false;
    }
    if (safety.fullscreen === true && !observed.fullscreen) {
        return false;
    }
    if (safety.maximized === true && !observed.maximized) {
        return false;
    }
    if (safety.sticky === true && !observed.sticky) {
        return false;
    }
    return true;
}

// Lifetime/freshness recheck between membership steps (output send, desktop
// set, follow view): tolerates the expected homing echo of the step that
// just ran and verifies no external invalidation or overlay flip instead of
// demanding stale homing.
function betweenStepsOk(
    env: ApplyEnv,
    reply: WorldReply,
    entryObs: string,
    observed: ObservedWindow,
): boolean {
    if (!replyFresh(env, reply, entryObs)) {
        return false;
    }
    if (!env.isStillListed(observed.ref)) {
        return false;
    }
    let safety: ReturnType<ApplyEnv["readLiveSafety"]> = null;
    try {
        safety = env.readLiveSafety(observed.ref);
    } catch (error) {
        void error;
        return false;
    }
    if (safety === null) {
        return false;
    }
    if (safety.minimized === true && !observed.minimized) {
        return false;
    }
    if (safety.fullscreen === true && !observed.fullscreen) {
        return false;
    }
    if (safety.maximized === true && !observed.maximized) {
        return false;
    }
    if (safety.sticky === true && !observed.sticky) {
        return false;
    }
    return true;
}

function safeResolve(resolve: () => object | null): object | null {
    try {
        return resolve();
    } catch (error) {
        void error;
        return null;
    }
}

function liveViewShowsOn(env: ApplyEnv, out: string, wsNativeId: string): boolean {
    try {
        return env.readLiveView(out) === wsNativeId;
    } catch (error) {
        void error;
        return false;
    }
}

// Follow-view after membership (D3): part of the same stable window
// transfer operation, already marked possibly-executed before the membership
// setter. Returns true only when the switch was actuated AND fresh visible
// proof arrived on the TARGET output; already-visible views report through
// the per-output live view instead so no phantom write is counted. Setter
// booleans are not proof: the target output's visible view decides. A
// desktop visible on the wrong output never proves.
function actuateViewForTransfer(
    env: ApplyEnv,
    reply: WorldReply,
    entryObs: string,
    transfer: WorldReplyTransfer,
): boolean {
    if (!replyFresh(env, reply, entryObs)) {
        return false;
    }
    const wsRef = safeResolve(() => env.desktopById(transfer.ws));
    const outRef = safeResolve(() => env.resolveOutput(transfer.out));
    if (wsRef === null || outRef === null) {
        return false;
    }
    try {
        env.switchView(wsRef, outRef);
    } catch (error) {
        void error;
        return liveViewShowsOn(env, transfer.out, transfer.ws);
    }
    return liveViewShowsOn(env, transfer.out, transfer.ws);
}

// Shared follow-view settlement for one window transfer whose membership
// proof already arrived: already-visible settles with no write counted,
// a proven switch settles with one view write, anything else holds the
// same operation ambiguous (never a separate view identity). Applied window
// targets are remembered for external-input causality.
function settleFollowView(
    env: ApplyEnv,
    reply: WorldReply,
    entryObs: string,
    transfer: WorldReplyTransfer,
    win: string,
    result: ApplyResult,
): void {
    const { state } = env;
    if (!transfer.follow || liveViewShowsOn(env, transfer.out, transfer.ws)) {
        state.settleAttempt(transfer.op, "applied", "");
        state.noteAppliedWindowTarget(win, transfer.op, transfer.out, transfer.ws);
    } else if (actuateViewForTransfer(env, reply, entryObs, transfer)) {
        result.viewWrites += 1;
        state.settleAttempt(transfer.op, "applied", "");
        state.noteAppliedWindowTarget(win, transfer.op, transfer.out, transfer.ws);
    } else {
        result.held += 1;
    }
}

export function applyWorldReply(env: ApplyEnv): ApplyResult {
    const { state, observation, reply } = env;
    const result: ApplyResult = {
        geometryWrites: 0, membershipWrites: 0, viewWrites: 0,
        creations: 0, removals: 0, effects: 0, held: 0, stoppedMidApply: false,
    };
    // Watermark at apply start: mid-apply invalidation compares against
    // this, never against the feedback-dirty flag (self-sent feedback must
    // not stop multi-window geometry/membership).
    const entryObs = state.currentObs;
    const byWin = new Map<string, ObservedWindow>();
    for (const domain of observation.domains) {
        for (const w of domain.windows) {
            if (!byWin.has(w.win)) {
                byWin.set(w.win, w);
            }
        }
    }
    const liveDesktopIds = new Set<string>();
    for (const t of observation.topology) {
        for (const id of t.order) {
            liveDesktopIds.add(id);
        }
    }
    // 1. Window rows: per-property diff. Frame writes need controlled frame
    // authority; membership writes need a retained core window transfer with
    // its stable operation identity plus controlled membership authority.
    // Untouched/held rows authorize nothing. Floats never take frame writes
    // from a send: membership only. Rows under an outstanding window
    // transfer with a different desired homing defer frames until
    // membership proof (see the arrival gate below).
    for (const row of reply.windows as WorldReplyWindow[]) {
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        const observed = byWin.get(row.win);
        if (observed === undefined) {
            result.held += 1;
            continue;
        }
        // Game/excluded/overlay guard: fit-excluded, fullscreen, and sticky
        // all-desktop rows never take automatic frame writes.
        const overlayGuarded = observed.fitExcluded || observed.fullscreen || observed.sticky;
        const frameControlled = row.frame_auth === "controlled" && row.frame_hold === "";
        const memberControlled = row.member_auth === "controlled" && row.member_hold === "";
        const targetFrame = row.frame ?? row.alloc;
        // Arrival gate: a row under an outstanding window transfer whose
        // desired homing differs from the observed homing takes no frame
        // write until membership proof arrives. The target frame belongs
        // to the destination output; writing it pre-move would place
        // foreign geometry on the source. Membership actuates below;
        // frames converge on the reports after arrival.
        const arrivalTransfer = reply.transfers.find((t) =>
            t.kind === "window" && t.win === row.win,
        );
        const arrivalPending = arrivalTransfer !== undefined &&
            (arrivalTransfer.out !== observed.out || arrivalTransfer.ws !== observed.ws);
        if (arrivalPending) {
            // Fall through to the membership path below without a frame
            // write; never hold-count a correctly deferred frame.
        } else if (targetFrame !== null && frameControlled && !overlayGuarded && !observed.floating) {
            let liveRect: { x: number; y: number; w: number; h: number } | null = null;
            try {
                liveRect = env.readLiveRect(observed.ref);
            } catch (error) {
                void error;
                liveRect = null;
            }
            if (liveRect !== null && !sameRect(liveRect, targetFrame)) {
                const key = clampKeyFor(row.win, targetFrame, observed);
                const attempts = state.clampAttempt(key);
                if (attempts < 3) {
                    if (!preSetterOk(env, reply, entryObs, observed, true)) {
                        result.stoppedMidApply = true;
                        break;
                    }
                    let ok = false;
                    let threw = false;
                    try {
                        ok = env.setGeometry(observed.ref, { x: targetFrame[0], y: targetFrame[1], w: targetFrame[2], h: targetFrame[3] });
                    } catch (error) {
                        void error;
                        threw = true;
                    }
                    if (!threw && ok) {
                        result.geometryWrites += 1;
                        // Accepted-host convergence: a later observation that
                        // still differs retries until the bounded budget ends,
                        // then the context is retained (never reset per reply)
                        // until a meaningful target/constraint change.
                        state.noteClampAttempt(key);
                    } else {
                        // Setter booleans are not proof: a false that already
                        // converged counts as converged; a throw (or a false
                        // with no convergence) stays a narrow hold, never a
                        // false proven refusal.
                        let proof: { x: number; y: number; w: number; h: number } | null = null;
                        try {
                            proof = env.readLiveRect(observed.ref);
                        } catch (error) {
                            void error;
                            proof = null;
                        }
                        if (proof !== null && sameRect(proof, targetFrame)) {
                            state.resetClamp(clampKeyFor(row.win, targetFrame, observed));
                        } else if (!threw && !ok) {
                            state.noteFeedback({ op: `prop-${row.win}`, kind: "property", outcome: "refused-proven", hold: "constraint-clamp" });
                            result.held += 1;
                        } else {
                            result.held += 1;
                        }
                    }
                } else {
                    // Bounded budget spent: hold narrowly, never the system.
                    result.held += 1;
                }
            } else if (liveRect !== null) {
                state.resetClamp(clampKeyFor(row.win, targetFrame, observed));
            } else {
                result.held += 1;
            }
        } else if (targetFrame !== null && frameControlled && (overlayGuarded || observed.floating)) {
            result.held += 1;
        }
        // Membership: only under a retained core window transfer naming this
        // window, reported under its stable operation id. No transfer
        // authorizes no move here. Read-back confirmation settles even
        // held/ambiguous operations; only unconfirmed ones hold without
        // repeating.
        const transfer = reply.transfers.find((t) => t.kind === "window" && t.win === row.win);
        if (!memberControlled || transfer === undefined) {
            continue;
        }
        const op = transfer.op;
        const targetOut = transfer.out;
        const targetWs = transfer.ws;
        let liveHome: { out: string; ws: string } | null = null;
        try {
            liveHome = env.readLiveMembership(observed.ref);
        } catch (error) {
            void error;
            liveHome = null;
        }
        if (liveHome === null) {
            result.held += 1;
            continue;
        }
        // Read-back confirmation first: already homed at the target
        // (observation lag after a previous flight, or external
        // convergence). Confirmation settles even a held/ambiguous
        // operation with no new native call; the follow view rides the
        // same operation when requested.
        if (liveHome.out === targetOut && liveHome.ws === targetWs) {
            settleFollowView(env, reply, entryObs, transfer, row.win, result);
            continue;
        }
        if (transfer.held || state.isHeldAmbiguous(op)) {
            result.held += 1;
            continue;
        }
        // Same-operation repeat after proven application elsewhere: the
        // fresh observation shows the window left the applied target, so a
        // stale core repeat proves newer external input and supersedes. A
        // distinct operation id is always a fresh actuation.
        const appliedMem = state.appliedWindowTarget.get(row.win);
        if (appliedMem !== undefined && appliedMem.op === op &&
            (observed.out !== appliedMem.out || observed.ws !== appliedMem.ws)) {
            state.settleAttempt(op, "superseded-external", "");
            continue;
        }
        const atSource = liveHome.out === observed.out && liveHome.ws === observed.ws;
        const partialProgress =
            (liveHome.out === targetOut && liveHome.ws === observed.ws) ||
            (liveHome.out === observed.out && liveHome.ws === targetWs);
        if (!atSource && !partialProgress) {
            // Fresh proof of newer external input elsewhere: the move is
            // superseded, never fought. Every other mismatch holds.
            state.settleAttempt(op, "superseded-external", "");
            continue;
        }
        if (!preSetterOk(env, reply, entryObs, observed, false)) {
            result.stoppedMidApply = true;
            break;
        }
        // Resolve before arming: unresolvable targets hold with no attempt
        // recorded, so the retained core target retries on a later report.
        const needOutMove = liveHome.out !== targetOut;
        const outRef: object | null = needOutMove ? safeResolve(() => env.resolveOutput(targetOut)) : null;
        const wsRef = safeResolve(() => env.desktopById(targetWs));
        if ((needOutMove && outRef === null) || wsRef === null) {
            result.held += 1;
            continue;
        }
        // Arm possibly-executed BEFORE the first native setter.
        state.armPossiblyExecuted(op, "transfer", `${targetOut}/${targetWs}`);
        let threw = false;
        let outOk = true;
        let outAttempted = false;
        if (needOutMove) {
            outAttempted = true;
            try {
                outOk = outRef !== null && env.sendToOutput(observed.ref, outRef);
            } catch (error) {
                void error;
                threw = true;
                outOk = false;
            }
            // Freshness between the output send and the desktop set, with
            // safe expected echoes: the homing may legitimately show the
            // new output already.
            if (!betweenStepsOk(env, reply, entryObs, observed)) {
                result.held += 1;
                if (midApplyStopped(env, entryObs)) {
                    result.stoppedMidApply = true;
                    break;
                }
                continue;
            }
        }
        let wsOk = true;
        let wsAttempted = false;
        // Re-read before the desktop set: the output step may have converged
        // the homing already, or proven an external move.
        let midHome: { out: string; ws: string } | null = null;
        try {
            midHome = env.readLiveMembership(observed.ref);
        } catch (error) {
            void error;
            midHome = null;
        }
        if (midHome !== null && midHome.out === targetOut && midHome.ws === targetWs) {
            if (outAttempted && outOk) {
                result.membershipWrites += 1;
            }
            settleFollowView(env, reply, entryObs, transfer, row.win, result);
            continue;
        }
        if (midHome === null || (midHome.out !== observed.out && midHome.out !== targetOut) || (midHome.ws !== observed.ws && midHome.ws !== targetWs)) {
            // Third-location proof arrived between the two setters: external
            // input supersedes; never fight it.
            state.settleAttempt(op, "superseded-external", "");
            if (outAttempted && outOk) {
                result.membershipWrites += 1;
            }
            continue;
        }
        if (midHome.ws !== targetWs) {
            wsAttempted = true;
            try {
                wsOk = wsRef !== null && env.setMembership(observed.ref, targetWs, wsRef);
            } catch (error) {
                void error;
                threw = true;
                wsOk = false;
            }
        }
        if ((outAttempted && outOk) || (wsAttempted && wsOk)) {
            result.membershipWrites += 1;
        }
        let proof: { out: string; ws: string } | null = null;
        try {
            proof = env.readLiveMembership(observed.ref);
        } catch (error) {
            void error;
            proof = null;
        }
        if (proof !== null && proof.out === targetOut && proof.ws === targetWs) {
            settleFollowView(env, reply, entryObs, transfer, row.win, result);
        } else if (!threw && (!outAttempted || !outOk) && (!wsAttempted || !wsOk) && proof !== null && proof.out === observed.out && proof.ws === observed.ws) {
            // Fresh proof nothing happened plus explicit native refusal on
            // every attempted setter: settle proven. Throws and partial
            // writes stay ambiguous (held, never repeated without proof).
            state.settleAttempt(op, "refused-proven", "refused-target");
            result.held += 1;
        } else {
            result.held += 1;
        }
        // Mid-apply invalidation stops remaining unsafe writes. Self-sent
        // feedback dirty never stops here; only a moved watermark does.
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
    }
    if (result.stoppedMidApply) {
        return result;
    }
    // 1b. Explicit retained float placement: window rows carrying a stable
    // core property operation id (`frame_op`, `op-<cmdSeq>`) place the exact
    // retained frame exactly once per operation. Ordinary floats carry "" and
    // never take frame writes here (the tiled path above already skips
    // floating windows, and no `Controlled` frame authority alone writes
    // float geometry). The correlated toggle-float result is provenance only
    // and never dispatches a second write. Each placement arms
    // possibly-executed BEFORE the native setter and settles applied only on
    // fresh read-back proof; throws and partial writes hold ambiguously
    // without repeating, and a provable post-apply manual move settles
    // superseded-external instead of fighting the user. Unrelated later
    // replies without the token never steal the native float frame.
    for (const row of reply.windows as WorldReplyWindow[]) {
        const op = row.frame_op ?? "";
        if (op === "") {
            continue;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        // Freshness before spending: a stale reply never consumes the
        // one-shot token.
        if (!replyFresh(env, reply, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        const observed = byWin.get(row.win);
        if (observed === undefined || row.frame === null) {
            // Window gone (or no placement rect): spend the operation so a
            // removed lifetime never wedges later replies. The core settles
            // closed lifetimes from explicit close observation.
            state.markFloatSpent(op);
            continue;
        }
        const rect: [number, number, number, number] = row.frame;
        const target = { x: rect[0], y: rect[1], w: rect[2], h: rect[3] };
        if (observed.minimized || observed.fullscreen || observed.sticky || observed.fitExcluded) {
            // Overlay/game guard: never an automatic float frame write.
            state.noteFeedback({ op, kind: "property", outcome: "unattempted", hold: "refused-target" });
            result.held += 1;
            continue;
        }
        if (observation.gesture) {
            // Gesture barrier suspends the placement like any geometry
            // effect; the user owns the frame meanwhile. Unspent, so the
            // retained target retries after the barrier lifts.
            result.held += 1;
            continue;
        }
        if (state.isHeldAmbiguous(op)) {
            result.held += 1;
            continue;
        }
        let liveRect: { x: number; y: number; w: number; h: number } | null = null;
        try {
            liveRect = env.readLiveRect(observed.ref);
        } catch (error) {
            void error;
            liveRect = null;
        }
        if (liveRect !== null && sameRect(liveRect, [target.x, target.y, target.w, target.h])) {
            state.markFloatSpent(op);
            state.noteFeedback({ op, kind: "property", outcome: "applied", hold: "" });
            state.noteAppliedWindowTarget(row.win, op, row.out, row.ws);
            state.resetClamp(clampKeyFor(row.win, [target.x, target.y, target.w, target.h], observed));
            continue;
        }
        if (liveRect === null) {
            result.held += 1;
            continue;
        }
        // Manual-change race: the native frame moved since the observation
        // captured it. Never fight it; hold until the next stable
        // observation proves the outcome.
        if (liveRect.x !== observed.rect.x || liveRect.y !== observed.rect.y ||
            liveRect.w !== observed.rect.w || liveRect.h !== observed.rect.h) {
            result.held += 1;
            continue;
        }
        // Stable post-apply departure: this same operation already applied
        // (read-back proof recorded), but the stable native frame has left
        // the target. That is provable newer external input: settle
        // superseded-external instead of fighting the user. A stable frame
        // that never reached the target and was never applied simply holds
        // above (the core keeps the target until observation or feedback
        // proves the outcome).
        {
            const appliedMem = state.appliedWindowTarget.get(row.win);
            if (appliedMem !== undefined && appliedMem.op === op &&
                (observed.rect.x !== target.x || observed.rect.y !== target.y ||
                    observed.rect.w !== target.w || observed.rect.h !== target.h)) {
                state.markFloatSpent(op);
                state.noteFeedback({ op, kind: "property", outcome: "superseded-external", hold: "" });
                continue;
            }
        }
        if (!state.markFloatSpent(op)) {
            // Already placed for this operation (including across duplicate
            // replies): never double-place.
            continue;
        }
        if (!preSetterOk(env, reply, entryObs, observed, true)) {
            result.stoppedMidApply = true;
            break;
        }
        // Arm possibly-executed BEFORE the first native setter.
        state.armPossiblyExecuted(op, "property", `${row.out}/${row.ws}`);
        let threw = false;
        try {
            env.setGeometry(observed.ref, target);
        } catch (error) {
            void error;
            threw = true;
        }
        let proof: { x: number; y: number; w: number; h: number } | null = null;
        try {
            proof = env.readLiveRect(observed.ref);
        } catch (error) {
            void error;
            proof = null;
        }
        if (proof !== null && sameRect(proof, [target.x, target.y, target.w, target.h])) {
            result.geometryWrites += 1;
            state.noteFeedback({ op, kind: "property", outcome: "applied", hold: "" });
            state.noteAppliedWindowTarget(row.win, op, row.out, row.ws);
            state.resetClamp(clampKeyFor(row.win, [target.x, target.y, target.w, target.h], observed));
        } else if (!threw && proof !== null && sameRect(proof, [liveRect.x, liveRect.y, liveRect.w, liveRect.h])) {
            // Fresh proof nothing happened plus an explicit native refusal:
            // settle proven.
            state.noteFeedback({ op, kind: "property", outcome: "refused-proven", hold: "constraint-clamp" });
            result.held += 1;
        } else {
            // Throw or partial write: ambiguous hold, never repeated
            // (already spent and armed possibly-executed).
            result.held += 1;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
    }
    if (result.stoppedMidApply) {
        return result;
    }
    // 2. Workspace lifecycle: creation one-flight per logical id with
    // enumeration recovery against the pre-attempt baseline; dependent
    // membership writes blocked until bound.
    // Removal only after a fresh all-window occupancy/min-count/pin/mapping
    // visibility gate; unknown occupancy keeps.
    for (const ws of reply.workspaces) {
        if (ws.bind !== "") {
            if (ws.logical !== "") {
                state.bindCreation(ws.logical, ws.bind);
            }
            continue;
        }
        // Unbound row: pending logical token. Exactly one flight per id,
        // armed before the native call and never repeated while ambiguous.
        const logical = ws.logical !== "" ? ws.logical : ws.id;
        if (logical === "") {
            continue;
        }
        const existing = state.creationFor(logical);
        if (existing !== null && existing.bound !== null) {
            continue;
        }
        if (existing !== null && existing.bound === null) {
            // Ambiguous create result: recover by enumeration against the
            // pre-attempt baseline before any further attempt; never append
            // blindly on identical replies.
            const recovered = state.recoverCreationByEnumeration(logical, [...liveDesktopIds]);
            if (recovered === null) {
                result.held += 1;
                continue;
            }
            continue;
        }
        if (state.isHeldAmbiguous(`create-${logical}`)) {
            result.held += 1;
            continue;
        }
        if (!replyFresh(env, reply, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        const flight = state.startCreation(logical, ws.out, [...liveDesktopIds]);
        if (flight === null) {
            result.held += 1;
            continue;
        }
        state.armPossiblyExecuted(`create-${logical}`, "creation", ws.out);
        let createdNative: string | null = null;
        let threw = false;
        try {
            createdNative = env.createDesktop();
        } catch (error) {
            void error;
            threw = true;
        }
        if (createdNative !== null && createdNative !== "") {
            state.bindCreation(logical, createdNative);
            state.settleAttempt(`create-${logical}`, "applied", "");
            result.creations += 1;
        } else if (threw) {
            // A throw may have created natively: stay ambiguous and recover
            // by enumeration, never blindly repeat.
            result.held += 1;
        } else {
            // Null/empty with no throw is still ambiguous (the native side
            // may have acted): hold for enumeration recovery, never claim a
            // proven refusal that would green-light a duplicate create.
            result.held += 1;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
    }
    if (result.stoppedMidApply) {
        return result;
    }
    for (const ws of reply.workspaces) {
        if (ws.exist || ws.bind === "" || ws.hold !== "") {
            continue;
        }
        // Desired absence is eligibility only: re-enumerate occupants
        // (including unmanaged/floating/minimized/fullscreen/maximized via
        // the fresh desktop occupant count), visibility, pins, mappings, and
        // minimum count. Unknown occupancy keeps. Outstanding transfers or
        // desired window rows referencing this workspace retain it
        // (transaction lifetime): removal must not race an in-flight
        // send/view/migration. Desired omission alone never removes.
        const referenced = reply.transfers.some((t) => t.ws === ws.bind) ||
            reply.windows.some((w) => w.ws === ws.bind);
        if (referenced) {
            state.noteFeedback({ op: `remove-${ws.bind}`, kind: "creation", outcome: "unattempted", hold: "occupied-removal" });
            result.held += 1;
            continue;
        }
        // Fresh per-removal gates against the current reply and watermark:
        // a removal decided on stale enumeration never runs. Truncated or
        // otherwise incomplete observation never authorizes blind removal:
        // the omitted tail could occupy this workspace.
        if (!replyFresh(env, reply, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        if (!observation.complete) {
            result.held += 1;
            continue;
        }
        const pinned = observation.bindings.some((b) => b.native === ws.bind && b.pin);
        if (pinned) {
            state.noteFeedback({ op: `remove-${ws.bind}`, kind: "creation", outcome: "unattempted", hold: "occupied-removal" });
            result.held += 1;
            continue;
        }
        const occupants = env.desktopOccupants(ws.bind);
        if (!occupants.readable) {
            result.held += 1;
            continue;
        }
        if (occupants.total > 0) {
            state.noteFeedback({ op: `remove-${ws.bind}`, kind: "creation", outcome: "unattempted", hold: "occupied-removal" });
            result.held += 1;
            continue;
        }
        const visible = env.desktopVisible(ws.bind);
        if (visible === null || visible) {
            result.held += 1;
            continue;
        }
        if (observation.topology.length > 0 && liveDesktopIds.size <= env.minDesktops) {
            result.held += 1;
            continue;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        let removed = false;
        try {
            removed = env.removeDesktop(ws.bind);
        } catch (error) {
            void error;
            removed = false;
        }
        if (removed) {
            result.removals += 1;
        } else {
            result.held += 1;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
    }
    if (result.stoppedMidApply) {
        return result;
    }
    // 3. Output views: retained core view transfer targets only (never
    // invented here). Each switch arms possibly-executed under the stable
    // transfer operation id first and settles applied only on fresh visible
    // proof; throws stay ambiguous, explicit refusals settle proven.
    for (const t of reply.transfers) {
        if (t.kind !== "view") {
            continue;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        // Paired migration-refill children stay owned by 3b while their
        // parent migration is outstanding in this reply: switching the
        // source view before member arrival proof would hide unmoved
        // members. Once the parent settles (absent here), this normal
        // path actuates the child.
        const refillParentPending = reply.transfers.some(
            (m) => m.kind === "migration" && t.op === `${m.op}-refill`,
        );
        if (refillParentPending) {
            continue;
        }
        const topo = observation.topology.find((entry) => entry.out === t.out);
        // Read-back confirmation first: the TARGET output actually showing
        // the target settles even a held/ambiguous operation with no new
        // native call. Any-output visibility never settles: the target
        // output exact view decides.
        if (liveViewShowsOn(env, t.out, t.ws)) {
            state.settleAttempt(t.op, "applied", "");
            state.noteAppliedViewTarget(t.out, t.op, t.ws);
            continue;
        }
        if (t.held || state.isHeldAmbiguous(t.op)) {
            result.held += 1;
            continue;
        }
        // Same-operation repeat after the output demonstrably left the
        // applied view: newer external view input supersedes the stale
        // repeat. Distinct operations always actuate.
        const appliedView = state.appliedViewTarget.get(t.out);
        if (appliedView !== undefined && appliedView.op === t.op &&
            topo !== undefined && topo.current !== appliedView.ws) {
            state.settleAttempt(t.op, "superseded-external", "");
            continue;
        }
        if (!replyFresh(env, reply, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        const wsRef = safeResolve(() => env.desktopById(t.ws));
        const outRef = safeResolve(() => env.resolveOutput(t.out));
        if (wsRef === null || outRef === null) {
            result.held += 1;
            continue;
        }
        state.armPossiblyExecuted(t.op, "view", `${t.out}/${t.ws}`);
        let threw = false;
        let ok = false;
        try {
            ok = env.switchView(wsRef, outRef);
        } catch (error) {
            void error;
            threw = true;
        }
        void ok;
        if (liveViewShowsOn(env, t.out, t.ws)) {
            result.viewWrites += 1;
            state.settleAttempt(t.op, "applied", "");
            state.noteAppliedViewTarget(t.out, t.op, t.ws);
        } else if (!threw) {
            state.settleAttempt(t.op, "refused-proven", "refused-target");
            result.held += 1;
        } else {
            result.held += 1;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
    }
    if (result.stoppedMidApply) {
        return result;
    }
    // 3b. Whole-workspace migration: one retained migration transfer moves
    // the full member set plus the follow views (the target output shows the
    // migrated workspace, the source refills to the core-retained refill
    // view, else its first remaining desired row). Members come from the correlated command result when it names
    // this transfer's target, else from the desired rows homed at the
    // target; never invented. The source output comes from the result, else
    // from the pending members' shared observed homing. Each member
    // actuates membership-only (D8: no geometry writes for fullscreen
    // members; tiled frames converge on later reports), exactly once per
    // stable operation id: confirmed members settle without new calls,
    // ambiguous ones hold without repeats, third-location proof supersedes
    // the whole migration. The transfer settles applied only when every
    // member is homed AND both views prove; partial progress holds
    // truthfully under the same stable id.
    for (const t of reply.transfers) {
        if (t.kind !== "migration") {
            continue;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        const migrationResult = reply.result !== null && reply.result.op === "migrate-workspace" &&
            reply.result.targetOut === t.out && reply.result.targetWs === t.ws
            ? reply.result
            : null;
        let members: string[] | null = null;
        if (migrationResult !== null) {
            members = [...migrationResult.members];
        } else {
            const homed = reply.windows
                .filter((w) => w.out === t.out && w.ws === t.ws)
                .map((w) => w.win);
            members = homed.length > 0 ? homed : null;
        }
        if (members === null || members.length === 0) {
            result.held += 1;
            continue;
        }
        if (members.some((id) => !byWin.has(id))) {
            // Not every member is observable yet (actuation lag or a stale
            // result): hold without inventing writes for unknown ids.
            result.held += 1;
            continue;
        }
        // Paired refill child: the core-chosen refill view for THIS
        // migration only (exact stable op pairing). Any other view to
        // the source (stale selects, policy views) is never chosen as
        // the refill.
        const pairedRefill = reply.transfers.find(
            (v) => v.kind === "view" && v.op === `${t.op}-refill` && v.ws !== "",
        );
        // Source output for the refill view: the correlated result names
        // it, else the paired refill child, else the pending members'
        // shared observed homing (all members leave the same source
        // scope). Never the target where members already arrived.
        let sourceOut: string | null = migrationResult !== null ? migrationResult.sourceOut : null;
        if (sourceOut === null && pairedRefill !== undefined && pairedRefill.out !== "") {
            sourceOut = pairedRefill.out;
        }
        if (sourceOut === null) {
            const firstId = members.length > 0 ? members[0] as string : "";
            const first = firstId !== "" ? byWin.get(firstId) : undefined;
            if (first !== undefined) {
                sourceOut = first.out;
            }
        }
        if (t.held || state.isHeldAmbiguous(t.op)) {
            // Late full-membership proof settles an ambiguous parent
            // without member replay (proof before hold); anything less
            // holds without another native call.
            let lateProof = members.length > 0;
            for (const id of members) {
                const observed = byWin.get(id);
                let home: { out: string; ws: string } | null = null;
                try {
                    home = observed !== undefined ? env.readLiveMembership(observed.ref) : null;
                } catch (error) {
                    void error;
                    home = null;
                }
                if (home === null || home.out !== t.out || home.ws !== t.ws) {
                    lateProof = false;
                    break;
                }
            }
            if (!lateProof) {
                result.held += 1;
                continue;
            }
        }
        const liveHomeOf = (ref: object): { out: string; ws: string } | null => {
            try {
                return env.readLiveMembership(ref);
            } catch (error) {
                void error;
                return null;
            }
        };
        // Third-location proof for any member: newer external input moved
        // it somewhere unexpected; the migration supersedes as a whole
        // rather than fighting per-member battles.
        let superseded = false;
        for (const id of members) {
            const observed = byWin.get(id);
            if (observed === undefined) {
                continue;
            }
            const home = liveHomeOf(observed.ref);
            if (home === null) {
                result.held += 1;
                superseded = true;
                break;
            }
            if (home.out === t.out && home.ws === t.ws) {
                continue;
            }
            const atSource = home.out === observed.out && home.ws === observed.ws;
            const partialProgress =
                (home.out === t.out && home.ws === observed.ws) ||
                (home.out === observed.out && home.ws === t.ws);
            if (!atSource && !partialProgress) {
                state.settleAttempt(t.op, "superseded-external", "");
                superseded = true;
                break;
            }
        }
        if (superseded) {
            if (midApplyStopped(env, entryObs)) {
                result.stoppedMidApply = true;
                break;
            }
            continue;
        }
        // Arm possibly-executed ONCE per stable operation before the first
        // member setter; every setter below rides the same identity.
        let armed = false;
        let failed = false;
        let unproven = false;
        for (const id of members) {
            const observed = byWin.get(id);
            if (observed === undefined) {
                continue;
            }
            let home = liveHomeOf(observed.ref);
            if (home === null) {
                result.held += 1;
                failed = true;
                break;
            }
            if (home.out === t.out && home.ws === t.ws) {
                state.noteAppliedWindowTarget(id, t.op, t.out, t.ws);
                continue;
            }
            if (!preSetterOk(env, reply, entryObs, observed, false)) {
                result.stoppedMidApply = true;
                break;
            }
            const needOutMove = home.out !== t.out;
            const outRef: object | null = needOutMove ? safeResolve(() => env.resolveOutput(t.out)) : null;
            const wsRef = safeResolve(() => env.desktopById(t.ws));
            if ((needOutMove && outRef === null) || wsRef === null) {
                result.held += 1;
                failed = true;
                break;
            }
            if (!armed) {
                armed = true;
                state.armPossiblyExecuted(t.op, "transfer", `${t.out}/${t.ws}`);
            }
            if (needOutMove && outRef !== null) {
                try {
                    env.sendToOutput(observed.ref, outRef);
                } catch (error) {
                    void error;
                }
                if (!betweenStepsOk(env, reply, entryObs, observed)) {
                    result.held += 1;
                    failed = true;
                    break;
                }
            }
            try {
                home = env.readLiveMembership(observed.ref);
            } catch (error) {
                void error;
                home = null;
            }
            if (home !== null && home.out === t.out && home.ws === t.ws) {
                result.membershipWrites += 1;
                state.noteAppliedWindowTarget(id, t.op, t.out, t.ws);
                continue;
            }
            if (home === null || (home.out !== observed.out && home.out !== t.out) ||
                (home.ws !== observed.ws && home.ws !== t.ws)) {
                state.settleAttempt(t.op, "superseded-external", "");
                superseded = true;
                break;
            }
            if (home.ws !== t.ws && wsRef !== null) {
                try {
                    if (env.setMembership(observed.ref, t.ws, wsRef)) {
                        result.membershipWrites += 1;
                    }
                } catch (error) {
                    void error;
                }
            }
            let proof: { out: string; ws: string } | null = null;
            try {
                proof = env.readLiveMembership(observed.ref);
            } catch (error) {
                void error;
                proof = null;
            }
            if (proof !== null && proof.out === t.out && proof.ws === t.ws) {
                state.noteAppliedWindowTarget(id, t.op, t.out, t.ws);
            } else {
                result.held += 1;
                unproven = true;
            }
            if (midApplyStopped(env, entryObs)) {
                result.stoppedMidApply = true;
                break;
            }
        }
        if (result.stoppedMidApply || superseded) {
            if (result.stoppedMidApply) {
                break;
            }
            continue;
        }
        // Follow views under the same stable operation: the target output
        // shows the migrated workspace; the source refills to the paired
        // refill child (R-WS-12 G-37 core result), else to its first
        // remaining desired row (replies predating the refill transfer
        // only). NO follow/refill view switches until ALL fresh member
        // arrival proof is in: a partial failure or stale membership
        // leaves every view untouched so unmoved members stay visible.
        // Each proves independently on its exact output; the transfer
        // settles applied only with all members homed and both views
        // proven. Wrong-output visibility never proves.
        const membersProven = !failed && !unproven;
        const targetShown = liveViewShowsOn(env, t.out, t.ws);
        let refillChild = pairedRefill;
        // Newer external input supersedes a proven-then-abandoned refill
        // child (same rule as the general view loop): drop it for this
        // apply and let the rows-lowest fallback serve pre-transfer
        // replies; the next core report converges without it.
        if (refillChild !== undefined && sourceOut !== null) {
            const appliedView = state.appliedViewTarget.get(sourceOut);
            const topoNow = observation.topology.find((entry) => entry.out === sourceOut);
            if (appliedView !== undefined && appliedView.op === refillChild.op &&
                topoNow !== undefined && topoNow.current !== appliedView.ws) {
                state.settleAttempt(refillChild.op, "superseded-external", "");
                refillChild = undefined;
            }
        }
        let refillWs: string | null = null;
        let refillOp = t.op;
        if (refillChild !== undefined) {
            refillWs = refillChild.ws;
            refillOp = refillChild.op;
        } else if (sourceOut !== null) {
            const candidates = reply.workspaces
                .filter((w) => w.out === sourceOut && w.exist && w.bind !== "" && w.bind !== t.ws)
                .sort((a, b) => a.order - b.order);
            if (candidates.length > 0) {
                refillWs = (candidates[0] as { bind: string }).bind;
            }
        }
        const refillShown = refillWs === null || (sourceOut !== null && liveViewShowsOn(env, sourceOut, refillWs));
        let viewsProven = targetShown && refillShown;
        if (membersProven && !viewsProven && replyFresh(env, reply, entryObs)) {
            if (!targetShown) {
                if (actuateViewForTransfer(env, reply, entryObs, t)) {
                    result.viewWrites += 1;
                }
            }
            if (refillWs !== null && sourceOut !== null && !liveViewShowsOn(env, sourceOut, refillWs)) {
                if (refillChild !== undefined) {
                    if (state.isHeldAmbiguous(refillOp)) {
                        // Never retry an ambiguous refill view under a fresh
                        // revision: hold for proof instead of a second call.
                        result.held += 1;
                    } else {
                        // Stable child op identity arms possibly-executed
                        // BEFORE the view setter, with proof/refusal
                        // feedback.
                        state.armPossiblyExecuted(refillOp, "view", `${sourceOut}/${refillWs}`);
                        if (actuateViewForTransfer(env, reply, entryObs, { ...refillChild, follow: false })) {
                            result.viewWrites += 1;
                            state.settleAttempt(refillOp, "applied", "");
                            state.noteAppliedViewTarget(sourceOut, refillOp, refillWs);
                        } else {
                            result.held += 1;
                        }
                    }
                } else {
                    // Legacy fallback for replies predating the refill
                    // transfer: the refill rides the parent operation with
                    // no separate arming, exactly as before.
                    const refillTransfer = { ...t, out: sourceOut, ws: refillWs };
                    if (actuateViewForTransfer(env, reply, entryObs, refillTransfer)) {
                        result.viewWrites += 1;
                    }
                }
            }
            viewsProven = liveViewShowsOn(env, t.out, t.ws) &&
                (refillWs === null || (sourceOut !== null && liveViewShowsOn(env, sourceOut, refillWs)));
        }
        // All members homed is rechecked from the per-member proofs above:
        // any held member blocks settlement without another native call.
        if (!failed && !unproven && viewsProven) {
            state.settleAttempt(t.op, "applied", "");
            if (sourceOut !== null) {
                state.noteAppliedViewTarget(t.out, t.op, t.ws);
            }
        } else {
            result.held += 1;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
    }
    if (result.stoppedMidApply) {
        return result;
    }
    // 4. One-shot focus: spent BEFORE the first possible native dispatch;
    // duplicates and repeated diffs never reapply. Lost/stale effects drop
    // (at-most-once), never steal focus later. Target-output switch proof
    // before focus: while a follow transfer's target view is not yet live on
    // its exact output, focus effects hold (the view must prove first); newer
    // external input supersedes while causality is ambiguous. Raise/stack
    // have no native actuation in this adapter: they spend at-most-once
    // with explicit feedback and never trigger a focus write.
    const followUnproven = reply.transfers.some((t) => {
        if (t.kind !== "window" || !t.follow) {
            return false;
        }
        return !liveViewShowsOn(env, t.out, t.ws);
    });
    for (const fx of reply.effects) {
        if (fx.kind !== "focus" && fx.kind !== "raise" && fx.kind !== "stack") {
            continue;
        }
        if (!state.markEffectSpent(fx.id)) {
            continue;
        }
        const observed = byWin.get(fx.target);
        if (observed === undefined) {
            state.noteFeedback({ op: fx.id, kind: "effect", outcome: "superseded-external", hold: "" });
            continue;
        }
        if (fx.kind !== "focus") {
            state.noteFeedback({ op: fx.id, kind: "effect", outcome: "unattempted", hold: "refused-target" });
            result.held += 1;
            continue;
        }
        if (followUnproven) {
            state.noteFeedback({ op: fx.id, kind: "effect", outcome: "unattempted", hold: "refused-target" });
            result.held += 1;
            continue;
        }
        // Overlay guard: game/excluded/overlay rows (fit-excluded,
        // fullscreen, sticky, floating, including first-seen fullscreen
        // holds) never take automatic focus/raise/stack writes. The effect
        // stays spent (at-most-once) with explicit feedback, so focus is
        // never stolen on a later unrelated report.
        if (observed.fitExcluded || observed.fullscreen || observed.sticky || observed.floating) {
            state.noteFeedback({ op: fx.id, kind: "effect", outcome: "unattempted", hold: "refused-target" });
            result.held += 1;
            continue;
        }
        if (observation.gesture || !replyFresh(env, reply, entryObs)) {
            if (midApplyStopped(env, entryObs) || observation.gesture) {
                state.noteFeedback({ op: fx.id, kind: "effect", outcome: "unattempted", hold: "gesture-barrier" });
                result.held += 1;
                continue;
            }
            result.stoppedMidApply = true;
            break;
        }
        if (!preSetterOk(env, reply, entryObs, observed, false)) {
            result.stoppedMidApply = true;
            break;
        }
        let ok = false;
        let threw = false;
        try {
            ok = env.focusWindow(observed.ref);
        } catch (error) {
            void error;
            threw = true;
        }
        let active = false;
        try {
            active = env.isActiveWindow(observed.ref);
        } catch (error) {
            void error;
            active = false;
        }
        if (!threw && ok && active) {
            result.effects += 1;
            state.noteFeedback({ op: fx.id, kind: "effect", outcome: "applied", hold: "" });
        } else if (!threw && !ok) {
            state.noteFeedback({ op: fx.id, kind: "effect", outcome: "refused-proven", hold: "" });
            result.held += 1;
        } else {
            // Throw, or a claimed focus with no active proof: hold narrowly
            // without a false applied claim. The effect stays spent
            // (at-most-once), so nothing repeats.
            state.noteFeedback({ op: fx.id, kind: "effect", outcome: "unattempted", hold: "refused-target" });
            result.held += 1;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
    }
    return result;
}
