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
import { type WorldReply, type WorldReplyTransfer, type WorldReplyWindow, type WorldReplyResult, type FrameSetter } from "./world-report";
import { isCanonicalU64, isOpaqueId } from "./world-seq";
import { WorldAdapterState } from "./world-state";

export interface ApplyEnv {
    state: WorldAdapterState;
    observation: WorldObservation;
    reply: WorldReply;
    // Local native-invalidation fence sampled at dispatch (the capture
    // flight watermark): mid-apply invalidation compares against this, so
    // native input landing mid-apply stops remaining unsafe writes. Never
    // the broker-owned obs.
    dispatchWatermark: string;
    // Terminal result for the filed command that this capture drained
    // (requester-only via TakeCommandResult, never the capture lane):
    // migration source provenance when it names this transfer's
    // target, else the paired refill child or observed homing decides.
    // Never member authority: the desired rows homed at the target decide
    // the member set. Null when the snapshot alone is the outcome.
    takeResult: WorldReplyResult | null;
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
    // KWin 6.7 void creation (workspace_wrapper.h, virtualdesktops.cpp):
    // required zero-based position in [0, count] plus name, returns
    // nothing. Proof comes only from fresh complete enumeration
    // (listDesktops) - same-turn for the causal id, or the first
    // successful later readback completing a deferred receipt,
    // uncorrelated - never a return value.
    createDesktop: (position: number, name: string) => void;
    // Fresh complete native inventory for creation proof (id plus native
    // name per desktop). Null when unreadable or incomplete: hold, never
    // guess. Optional only for offline doubles; production always
    // provides it.
    listDesktops?: () => Array<{ readonly id: string; readonly name: string }> | null;
    // Friendly rename of a newly-created exactly-attributed desktop
    // (`VirtualDesktop.name` WRITE `setName` at pinned v6.7.5): attempted
    // only on the proven bound native while it still carries its logical
    // token name, best-effort after the binding is stored. A missing
    // capability, a failed write, or a failed read-back never loses the
    // id-based binding and never re-creates. Optional only for offline
    // doubles; production always provides it.
    renameDesktop?: (nativeId: string, name: string) => boolean;
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
// bound rev still names this reply, and the local native-invalidation
// watermark has not advanced since the dispatch fence was sampled.
// Sequencing (obs) is broker-owned and never compared here; a moved local
// watermark (native external input, including synchronous setter echoes
// that have not yet converged) or observation-only catchup fails closed.
function replyFresh(env: ApplyEnv, reply: WorldReply, entryObs: string): boolean {
    const { state } = env;
    if (state.observationOnlyCatchup) {
        return false;
    }
    if (state.boundGen === null || state.boundGen !== reply.gen) {
        return false;
    }
    if (state.hasAccepted && reply.rev !== state.lastAcceptedRev) {
        return false;
    }
    if (state.currentInvalid !== entryObs) {
        return false;
    }
    return true;
}

function midApplyStopped(env: ApplyEnv, entryObs: string): boolean {
    return env.state.observationOnlyCatchup || env.state.currentInvalid !== entryObs;
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

// Fresh same-turn native evidence for host-owned attempt rows (never a
// binding table): the adapter reads live native ids immediately
// before/after each setter and reports them raw; the core translates
// through its own per-output bindings. Logical pre/post/base stay null.
// Unreadable or non-opaque reads ride null (unknown), never a guess.
function liveNativeWsOf(home: { out: string; ws: string } | null): string | null {
    if (home === null || !isOpaqueId(home.ws)) {
        return null;
    }
    return home.ws;
}

function readNativeWsNow(env: ApplyEnv, ref: object): string | null {
    try {
        return liveNativeWsOf(env.readLiveMembership(ref));
    } catch (error) {
        void error;
        return null;
    }
}

function readNativeViewNow(env: ApplyEnv, out: string): string | null {
    try {
        const current = env.readLiveView(out);
        return typeof current === "string" && isOpaqueId(current) ? current : null;
    } catch (error) {
        void error;
        return null;
    }
}

function observationNativeView(observation: WorldObservation, out: string): string | null {
    const current = observation.topology.find((entry) => entry.out === out)?.current;
    return typeof current === "string" && current !== "" && isOpaqueId(current) ? current : null;
}

// Friendly KDE-style name for a proven desktop: `Desktop N` with the
// native one-based position in the proving enumeration (mirroring KWin's
// own `defaultName`, i18n `Desktop %1`). Position comes from the snapshot
// that proved the binding, never from reply order. A logical token
// (`[a-z0-9-]`) can never collide with this shape (space, uppercase).
function friendlyDesktopName(
    live: ReadonlyArray<{ readonly id: string; readonly name: string }>,
    bound: string,
): string | null {
    const at = live.findIndex((d) => d.id === bound);
    if (at < 0) {
        return null;
    }
    return `Desktop ${at + 1}`;
}

// Rename exactly the proven desktop while it still carries its logical
// token name. Adopted, unproven, already-renamed, and existing desktops
// never match (their names differ from the token). Best-effort after the
// causal created id is proven same-turn: any failure keeps the receipt,
// never re-creates, and never steals another desktop.
function renameProvenDesktop(
    env: ApplyEnv,
    live: ReadonlyArray<{ readonly id: string; readonly name: string }>,
    logical: string,
    bound: string | null,
): void {
    if (env.renameDesktop === undefined || bound === null) {
        return;
    }
    const entry = live.find((d) => d.id === bound);
    if (entry === undefined || entry.name !== logical) {
        return;
    }
    const friendly = friendlyDesktopName(live, bound);
    if (friendly === null || entry.name === friendly) {
        return;
    }
    try {
        env.renameDesktop(bound, friendly);
    } catch (error) {
        void error;
    }
}

// Pinned KWin native echo (window.cpp `finishInteractiveOutputChange`,
// called at the tail of `sendToOutput` and on `outputChanged`): moving a
// single-desktop window whose workspace is the old output's current onto a
// new output synchronously re-homes it onto the NEW output's current
// desktop. Proven T05 lag shape: the desktop echo lands synchronously while
// `readLiveMembership` still names the SOURCE output until native output
// propagation completes (destination reports later). Immediately after our
// own output setter inside the same armed operation, exactly
// (targetOut, capturedTargetCurrent) and exactly
// (sourceOut, capturedTargetCurrent) are the expected echoes of our own
// call, never external input. Tolerate ONLY those two: the
// source observed workspace must still be the source-output current, the
// captured target current must name a valid desktop, the fresh views must
// be unchanged, the same window must still be listed, and
// safety/lifetime/freshness must still hold. Anything else (third
// workspaces, refreshed currents, view races, overlay flips, missing
// views, unknown membership, native throws) fails closed through the
// existing superseded/hold paths. The caller then sets the requested
// target workspace on the same flight, never a retry and never a
// pre-switched destination view.
// Bounded gate tag for per-member migration diagnostics ("ok" only for
// the exact expected echo); callers treat any non-"ok" tag as a refusal
// with identical behavior.
type NativeEchoGate =
    | "ok"
    | "skipped"
    | "null-view"
    | "shape"
    | "src-current"
    | "bad-target"
    | "view-race"
    | "unlisted"
    | "stale";
function tolerateNativeOutputEcho(
    env: ApplyEnv,
    reply: WorldReply,
    entryObs: string,
    observed: ObservedWindow,
    srcViewBefore: string | null,
    tgtViewBefore: string | null,
    midHome: { out: string; ws: string },
    targetOut: string,
): NativeEchoGate {
    if (srcViewBefore === null || tgtViewBefore === null) {
        return "null-view";
    }
    // Exact expected echoes only: the synchronous target-output echo and
    // the proven same-flight source-output lag echo (T05 desktop lands
    // before output propagation). Any other output/desktop is a third
    // location.
    if (midHome.ws !== tgtViewBefore) {
        return "shape";
    }
    if (midHome.out !== targetOut && midHome.out !== observed.out) {
        return "shape";
    }
    // The echo only fires for a window homed on the source current.
    if (srcViewBefore !== observed.ws) {
        return "src-current";
    }
    // The captured target current must name a valid desktop.
    if (safeResolve(() => env.desktopById(tgtViewBefore)) === null) {
        return "bad-target";
    }
    // Fresh views must be unchanged: a raced view switch or an unreadable
    // view fails closed instead of adopting the echo.
    let srcNow: string | null = null;
    let tgtNow: string | null = null;
    try {
        srcNow = env.readLiveView(observed.out);
    } catch (error) {
        void error;
        srcNow = null;
    }
    try {
        tgtNow = env.readLiveView(targetOut);
    } catch (error) {
        void error;
        tgtNow = null;
    }
    if (srcNow !== srcViewBefore || tgtNow !== tgtViewBefore) {
        return "view-race";
    }
    let listed = false;
    try {
        listed = env.isStillListed(observed.ref);
    } catch (error) {
        void error;
        listed = false;
    }
    if (!listed) {
        return "unlisted";
    }
    return betweenStepsOk(env, reply, entryObs, observed) ? "ok" : "stale";
}

// Bounded per-member homing shape for migration diagnostics: pure string
// comparison over already-read homing, no new native reads, no raw ids.
type MigrationHomeShape = "target" | "source" | "partial" | "third" | "unreadable";
function migrationHomeShape(
    home: { out: string; ws: string } | null,
    observed: ObservedWindow,
    target: { out: string; ws: string },
): MigrationHomeShape {
    if (home === null) {
        return "unreadable";
    }
    if (home.out === target.out && home.ws === target.ws) {
        return "target";
    }
    if (home.out === observed.out && home.ws === observed.ws) {
        return "source";
    }
    if ((home.out === target.out && home.ws === observed.ws) ||
        (home.out === observed.out && home.ws === target.ws)) {
        return "partial";
    }
    return "third";
}

function liveViewShowsOn(env: ApplyEnv, out: string, wsNativeId: string): boolean {
    try {
        return env.readLiveView(out) === wsNativeId;
    } catch (error) {
        void error;
        return false;
    }
}

// Logical->native workspace resolution for actuation (D2): reply
// transfer, view, and migration rows name the stable core LOGICAL
// workspace id; native setters, live membership, and live views speak
// NATIVE desktop ids. Resolve through the accepted reply's own workspace
// rows for the exact output, never by ordinal or native string equality.
// Unbound or unknown targets resolve to null and hold (never guess).
function nativeWsOf(reply: WorldReply, out: string, logical: string): string | null {
    for (const row of reply.workspaces) {
        if (row.out === out && row.id === logical) {
            return row.bind !== "" ? row.bind : null;
        }
    }
    return null;
}

// Live-view proof for a logical transfer/view target: unknown or unbound
// targets never prove (callers hold).
function liveViewShowsLogical(env: ApplyEnv, reply: WorldReply, out: string, logical: string): boolean {
    const native = nativeWsOf(reply, out, logical);
    return native !== null && liveViewShowsOn(env, out, native);
}

// Log-only per-member migration trace: never throws, never actuates, never
// changes settlement. All fields are bounded tags or counts, no raw ids.
function traceMigrate(env: ApplyEnv, message: string): void {
    try {
        env.log(message);
    } catch (error) {
        void error;
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
    const { state } = env;
    if (!replyFresh(env, reply, entryObs)) {
        return false;
    }
    const targetNative = nativeWsOf(reply, transfer.out, transfer.ws);
    const wsRef = targetNative === null ? null : safeResolve(() => env.desktopById(targetNative));
    const outRef = safeResolve(() => env.resolveOutput(transfer.out));
    if (wsRef === null || outRef === null) {
        return false;
    }
    // Fresh same-turn native currents around the write; the observation
    // baseline is the entry observation's current for the target output.
    const viewPre = readNativeViewNow(env, transfer.out);
    const viewBase = observationNativeView(env.observation, transfer.out);
    let threw = false;
    let ok = false;
    try {
        ok = env.switchView(wsRef, outRef);
    } catch (error) {
        void error;
        threw = true;
    }
    const viewPost = readNativeViewNow(env, transfer.out);
    // One native view write ran: the mechanical disposition rides the next
    // report under the owning phase identity (the window follow parent or
    // the migration parent). Logical pre/post/base stay null; the fresh
    // native currents ride the native keys for core-side translation.
    state.recordViewAttempt({
        op: transfer.op, out: transfer.out, ws: transfer.ws,
        setter: threw ? "threw" : ok ? "accepted" : "refused",
        pre: null, post: null, base: null,
        nativePre: viewPre, nativePost: viewPost, nativeBase: viewBase,
        attemptMs: isCanonicalU64(state.lastClock) ? state.lastClock : "0",
    });
    return targetNative !== null && liveViewShowsOn(env, transfer.out, targetNative);
}

// Same-flight view switch for one migration's paired target/refill views
// (3b): the entry freshness check already passed immediately above, and
// the sibling switch's own synchronous `currentDesktopChanged` advanced
// the watermark in between. Rechecking entry freshness here would fail
// closed on our own actuation: the refill would arm possibly-executed
// without ever attempting, then never retry, wedging the parent (and the
// core twin) forever. Single-threaded run-to-completion means no external
// input interleaves between the two synchronous switches, so the entry
// check plus exact-output read-back proof is the complete safety case:
// wrong-output visibility never proves, throws stay ambiguous, and any
// later external view move settles superseded instead of fighting.
// Same-flight view-switch disposition: whether fresh proof shows the
// target, plus the mechanical setter disposition (`null` when no setter
// ran). The entry freshness check above is the freshness proof;
// read-back decides.
interface SameFlightView {
    proven: boolean;
    setter: FrameSetter | null;
}

function actuateViewSameFlightNative(
    env: ApplyEnv,
    out: string,
    nativeWs: string,
): SameFlightView {
    const wsRef = safeResolve(() => env.desktopById(nativeWs));
    const outRef = safeResolve(() => env.resolveOutput(out));
    if (wsRef === null || outRef === null) {
        return { proven: false, setter: null };
    }
    let threw = false;
    let ok = false;
    try {
        ok = env.switchView(wsRef, outRef);
    } catch (error) {
        void error;
        threw = true;
    }
    return {
        proven: liveViewShowsOn(env, out, nativeWs),
        setter: threw ? "threw" : ok ? "accepted" : "refused",
    };
}

function actuateViewSameFlight(
    env: ApplyEnv,
    reply: WorldReply,
    transfer: WorldReplyTransfer,
): SameFlightView {
    const targetNative = nativeWsOf(reply, transfer.out, transfer.ws);
    if (targetNative === null) {
        return { proven: false, setter: null };
    }
    return actuateViewSameFlightNative(env, transfer.out, targetNative);
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
    // Follow-view target in native terms for causality bookkeeping (live
    // membership and views speak native; the reply row names logical).
    const targetNative = nativeWsOf(reply, transfer.out, transfer.ws);
    const appliedWs = targetNative ?? transfer.ws;
    if (!transfer.follow || (targetNative !== null && liveViewShowsOn(env, transfer.out, targetNative))) {
        state.settleAttempt(transfer.op, "applied", "");
        state.noteAppliedWindowTarget(win, transfer.op, transfer.out, appliedWs);
    } else if (actuateViewForTransfer(env, reply, entryObs, transfer)) {
        result.viewWrites += 1;
        state.settleAttempt(transfer.op, "applied", "");
        state.noteAppliedWindowTarget(win, transfer.op, transfer.out, appliedWs);
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
    // this dispatch fence, never against the dirty flag (self-sent dirty
    // must not stop multi-window geometry/membership).
    const entryObs = env.dispatchWatermark;
    // Original write stamp for every mechanical attempt this apply
    // records: the broker dispatch-clock reading, never wall-clock. "0"
    // means unreported; redelivery after a lost acknowledgement reuses
    // the stored stamps (the core consumes each idempotently).
    const stamp = isCanonicalU64(state.lastClock) ? state.lastClock : "0";
    const byWin = new Map<string, ObservedWindow>();
    for (const domain of observation.domains) {
        for (const w of domain.windows) {
            if (!byWin.has(w.win)) {
                byWin.set(w.win, w);
            }
        }
    }
    // Minimized lifetimes carry full local carriers (live ref plus native
    // sticky) so conversion membership actuates on them; domain rows above
    // win the dedupe when a lifetime appears in both.
    for (const m of observation.minimized) {
        if (!byWin.has(m.win)) {
            byWin.set(m.win, m);
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
                // Authoritative convergence: the fresh live frame proves an
                // earlier target under the same hints already converged
                // (sync or delayed). Reset exactly that context; unrelated
                // unachieved targets keep their own budgets.
                state.resetClamp(clampKeyFor(row.win, [liveRect.x, liveRect.y, liveRect.w, liveRect.h], observed));
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
                        // until a meaningful target/constraint change. Only
                        // exact read-back convergence clears the just-written
                        // context; anything else consumes one bounded retry
                        // so a genuinely clamped target still spends.
                        let converged: { x: number; y: number; w: number; h: number } | null = null;
                        try {
                            converged = env.readLiveRect(observed.ref);
                        } catch (error) {
                            void error;
                            converged = null;
                        }
                        // Mechanical fact only: the setter disposition plus
                        // the immediate readback (never a verdict). The core
                        // owns clamp/transient suppression over this plus
                        // the fresh facts; the local budget above stays as
                        // the host retry fence.
                        state.recordFrameAttempt({
                            win: row.win, target: [targetFrame[0], targetFrame[1], targetFrame[2], targetFrame[3]],
                            setter: "accepted", readback: converged === null ? null : [converged.x, converged.y, converged.w, converged.h],
                            dpiBefore: null, dpiAfter: null, attemptMs: stamp, op: "", pre: null,
                        });
                        if (converged !== null && sameRect(converged, targetFrame)) {
                            state.resetClamp(key);
                        } else {
                            state.noteClampAttempt(key);
                        }
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
                        // A setter ran, so its disposition plus the immediate
                        // readback rides the next report (refused or threw,
                        // never a verdict).
                        const sweepSetter: FrameSetter = threw ? "threw" : "refused";
                        state.recordFrameAttempt({
                            win: row.win, target: [targetFrame[0], targetFrame[1], targetFrame[2], targetFrame[3]],
                            setter: sweepSetter, readback: proof === null ? null : [proof.x, proof.y, proof.w, proof.h],
                            dpiBefore: null, dpiAfter: null, attemptMs: stamp, op: "", pre: null,
                        });
                        if (proof !== null && sameRect(proof, targetFrame)) {
                            state.resetClamp(key);
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
        // Native target for every setter, live comparison, and read-back
        // below (the reply row names the logical workspace). Unbound or
        // unknown targets hold with no attempt recorded.
        const targetNative = nativeWsOf(reply, targetOut, targetWs);
        if (targetNative === null) {
            result.held += 1;
            continue;
        }
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
        if (liveHome.out === targetOut && liveHome.ws === targetNative) {
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
            (liveHome.out === observed.out && liveHome.ws === targetNative);
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
        // Minimized lifetimes actuate membership only, never an output
        // send: conversion never changes output, so a cross-output target
        // for a minimized window holds fail-closed here.
        if (observed.minimized && needOutMove) {
            result.held += 1;
            continue;
        }
        const outRef: object | null = needOutMove ? safeResolve(() => env.resolveOutput(targetOut)) : null;
        const wsRef = safeResolve(() => env.desktopById(targetNative));
        if ((needOutMove && outRef === null) || wsRef === null) {
            result.held += 1;
            continue;
        }
        // Fresh per-output currents BEFORE the native output call, so the
        // pinned synchronous echo below is verifiable (see
        // tolerateNativeOutputEcho). Nulls fail closed at the check.
        let echoSrcView: string | null = null;
        let echoTgtView: string | null = null;
        if (needOutMove) {
            try {
                echoSrcView = env.readLiveView(observed.out);
            } catch (error) {
                void error;
                echoSrcView = null;
            }
            try {
                echoTgtView = env.readLiveView(targetOut);
            } catch (error) {
                void error;
                echoTgtView = null;
            }
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
                state.recordMemberAttempt({
                    op, win: row.win, out: targetOut, ws: targetWs,
                    setter: threw ? "threw" : outOk ? "accepted" : "refused",
                    pre: null, post: null,
                    nativePre: liveNativeWsOf(liveHome),
                    nativePost: readNativeWsNow(env, observed.ref),
                    attemptMs: stamp,
                });
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
        if (midHome !== null && midHome.out === targetOut && midHome.ws === targetNative) {
            if (outAttempted) {
                if (outOk) {
                    result.membershipWrites += 1;
                }
                // The output setter ran and homing already shows the
                // target: record the real disposition (threw stays
                // ambiguous, refused-but-arrived stays refused).
                state.recordMemberAttempt({
                    op, win: row.win, out: targetOut, ws: targetWs,
                    setter: threw ? "threw" : outOk ? "accepted" : "refused",
                    pre: null, post: null,
                    nativePre: liveNativeWsOf(liveHome),
                    nativePost: liveNativeWsOf(midHome),
                    attemptMs: stamp,
                });
            }
            settleFollowView(env, reply, entryObs, transfer, row.win, result);
            continue;
        }
        // The exact expected native echo of our own output setter is
        // tolerated (same flight continues to the desktop set below);
        // every other third location still proves external input.
        const echoGate: NativeEchoGate = outAttempted && !threw && outOk && midHome !== null
            ? tolerateNativeOutputEcho(env, reply, entryObs, observed, echoSrcView, echoTgtView, midHome, targetOut)
            : "skipped";
        const echoOk = echoGate === "ok";
        if (midHome === null || (!echoOk && ((midHome.out !== observed.out && midHome.out !== targetOut) || (midHome.ws !== observed.ws && midHome.ws !== targetNative)))) {
            // Third-location proof arrived between the two setters: external
            // input supersedes; never fight it.
            if (outAttempted) {
                state.recordMemberAttempt({
                    op, win: row.win, out: targetOut, ws: targetWs,
                    setter: threw ? "threw" : outOk ? "accepted" : "refused",
                    pre: null, post: null,
                    nativePre: liveNativeWsOf(liveHome),
                    nativePost: liveNativeWsOf(midHome),
                    attemptMs: stamp,
                });
            }
            state.settleAttempt(op, "superseded-external", "");
            if (outAttempted && outOk) {
                result.membershipWrites += 1;
            }
            continue;
        }
        if (midHome.ws !== targetNative) {
            wsAttempted = true;
            try {
                wsOk = wsRef !== null && env.setMembership(observed.ref, targetNative, wsRef);
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
        // The mechanical disposition plus the immediate homing readback
        // rides the next report when a setter ran. Logical pre/post stay
        // null; the fresh native desktop ids ride the native keys.
        if (outAttempted || wsAttempted) {
            const memberSetter: FrameSetter = threw
                ? "threw"
                : (outAttempted && !outOk) || (wsAttempted && !wsOk)
                    ? "refused"
                    : "accepted";
            state.recordMemberAttempt({
                op, win: row.win, out: targetOut, ws: targetWs,
                setter: memberSetter, pre: null, post: null,
                nativePre: liveNativeWsOf(liveHome),
                nativePost: liveNativeWsOf(proof),
                attemptMs: stamp,
            });
        }
        if (proof !== null && proof.out === targetOut && proof.ws === targetNative) {
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
        if (observed.minimized || observed.fullscreen || observed.sticky || observed.maximized) {
            // Overlay guard: minimized, fullscreen, sticky, and maximized
            // lifetimes never take an explicit float frame write. Bare
            // floating never blocks: the observer couples floating into
            // fitExcluded, so testing fitExcluded here would refuse every
            // converged intentional float its core-directed placement and
            // hold the retained operation forever. Fixed-size windows ride
            // the setter truthfully (an explicit refusal records refused
            // evidence and the core settles it) instead of holding blind.
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
        let floatOk = false;
        try {
            floatOk = env.setGeometry(observed.ref, target);
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
        // One explicit placement write ran: the mechanical disposition
        // plus the immediate pre-write and post-write frames rides the
        // next report under the stable operation correlation. The setter
        // alone never proves completion; the core settles from this
        // evidence plus the fresh facts.
        state.recordFrameAttempt({
            win: row.win, target: [target.x, target.y, target.w, target.h],
            setter: threw ? "threw" : floatOk ? "accepted" : "refused",
            readback: proof === null ? null : [proof.x, proof.y, proof.w, proof.h],
            dpiBefore: null, dpiAfter: null, attemptMs: stamp,
            op, pre: [liveRect.x, liveRect.y, liveRect.w, liveRect.h],
        });
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
    // 1c. Explicit retained maximize-clear verdicts: window rows carrying a
    // stable core clear operation id (`clear_op`) settle exactly once per
    // operation. Proof-only (Stage3 parks native clear actuation): a live
    // read-back proving the window is already unmaximized spends the token
    // and settles applied; a still-maximized target holds (the core retains
    // the desire, the adapter never fights the user and never invents a
    // write). Unreadable safety fails closed with a narrow hold.
    for (const row of reply.windows as WorldReplyWindow[]) {
        const op = row.clear_op ?? "";
        if (op === "") {
            continue;
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        if (!replyFresh(env, reply, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        const observed = byWin.get(row.win);
        if (observed === undefined) {
            // Window gone: spend the operation so a removed lifetime never
            // wedges later replies. The core settles closed lifetimes from
            // explicit close observation.
            state.markClearSpent(op);
            continue;
        }
        if (state.clearSpent.has(op)) {
            // Already settled for this operation (including across duplicate
            // replies): never re-settle. Unsettled holds re-evaluate per
            // reply until proof arrives.
            continue;
        }
        if (observation.gesture) {
            result.held += 1;
            continue;
        }
        let maximizedNow: boolean | null = null;
        try {
            const safety = env.readLiveSafety(observed.ref);
            maximizedNow = safety === null ? null : safety.maximized;
        } catch (error) {
            void error;
            maximizedNow = null;
        }
        if (maximizedNow === null) {
            result.held += 1;
            continue;
        }
        if (maximizedNow === false) {
            state.markClearSpent(op);
            state.noteFeedback({ op, kind: "property", outcome: "applied", hold: "" });
            continue;
        }
        result.held += 1;
    }
    if (result.stoppedMidApply) {
        return result;
    }
    // 2. Native creation: explicit core desires only (`nativeCreations`,
    // at most one entry per outstanding stable op). An unbound workspace
    // row without a desire row authorizes nothing and never creates. Each
    // desire executes at most once per stable op: spent BEFORE the native
    // call (synchronous KWin signals make the call reentrant), then the
    // raw receipt rides the next report lane until the core incorporates
    // it. The fast path records same-turn before/after plus the causal
    // created id from the unique fresh delta; when the same-turn post-call
    // readback is unavailable, execution evidence is preserved locally as
    // incomplete (never fabricated) and completes once against the first
    // successful later full native-order readback, uncorrelated. Core owns
    // binding; the adapter never maps native ids to logical ids and never
    // guesses by ordinal or name. Dependent membership writes stay blocked
    // until the core publishes the bind.
    // Removal only after a fresh all-window occupancy/min-count/pin/mapping
    // visibility gate; unknown occupancy keeps.
    state.incorporateCreationBindings(reply);
    for (const desire of reply.nativeCreations) {
        const op = desire.op;
        const logical = desire.ws;
        const out = desire.out;
        if (op === "" || logical === "" || out === "") {
            result.held += 1;
            continue;
        }
        if (state.creationSpent.has(op)) {
            // Already executed for this stable op (receipt retained or
            // incorporated): never re-execute.
            continue;
        }
        if (!replyFresh(env, reply, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
        // Fresh complete pre-attempt enumeration per individual desire:
        // each desire in the same reply starts from current native truth,
        // never the shared stale observation, so a sibling creation never
        // leaks into this desire's baseline. Unreadable holds with no
        // native call and no receipt, so a later readable inventory can
        // proceed (the desire is retained core-side).
        let before: Array<{ readonly id: string; readonly name: string }> | null = null;
        try {
            before = env.listDesktops?.() ?? null;
        } catch (error) {
            void error;
            before = null;
        }
        if (before === null) {
            state.noteFeedback({ op, kind: "creation", outcome: "unattempted", hold: "incomplete-inventory" });
            result.held += 1;
            continue;
        }
        const beforeIds = before.map((d) => d.id);
        // Spend the stable op BEFORE the native call: synchronous desktop
        // signals fire inside the call, and a reentrant apply must see the
        // op as executed rather than firing a second desktop.
        if (!state.markCreationSpent(op)) {
            continue;
        }
        state.armPossiblyExecuted(op, "creation", out);
        // Zero-based append position in [0, count] (virtualdesktops.cpp
        // clamp); the logical token rides as the native name only as a
        // diagnostic (identity comes from the id delta, never the name).
        let threw = false;
        try {
            env.createDesktop(beforeIds.length, logical);
        } catch (error) {
            void error;
            threw = true;
        }
        // Post-call proof: read the full ids immediately after the call
        // in the same JS turn; a unique fresh delta proves the created id.
        // No delta (cap no-op, delayed visibility) records an uncorrelated
        // receipt; several fresh ids never bind here (the core holds
        // ambiguous rather than guessing). Position is never identity.
        // When this readback is unavailable, nothing is emitted: the
        // execution evidence is preserved locally as incomplete and the
        // next capture completes it once against the first successfully
        // observed full native order, uncorrelated (causality is lost
        // across the JS turn).
        let after: Array<{ readonly id: string; readonly name: string }> | null = null;
        try {
            after = env.listDesktops?.() ?? null;
        } catch (error) {
            void error;
            after = null;
        }
        const stamp = isCanonicalU64(state.lastClock) ? state.lastClock : "0";
        if (after === null) {
            state.deferCreationReceipt({
                op, ws: logical, out,
                setter: threw ? "threw" : "accepted",
                before: beforeIds,
                attemptMs: stamp,
            });
            result.held += 1;
        } else {
            const afterIds = after.map((d) => d.id);
            const beforeSet = new Set(beforeIds);
            const fresh = afterIds.filter((id) => !beforeSet.has(id));
            const created = fresh.length === 1 ? (fresh[0] as string) : null;
            state.recordCreationAttempt({
                op, ws: logical, out,
                setter: threw ? "threw" : "accepted",
                before: beforeIds, after: afterIds, created,
                attemptMs: stamp,
            });
            if (created !== null) {
                renameProvenDesktop(env, after, logical, created);
                result.creations += 1;
            } else {
                result.held += 1;
            }
        }
        if (midApplyStopped(env, entryObs)) {
            result.stoppedMidApply = true;
            break;
        }
    }
    if (result.stoppedMidApply) {
        return result;
    }
    // Native removal dedup: the aligned grid repeats one native per
    // output, so several rows can name the same native binding. Every
    // gate below keys on the native id only, so the first row decides and
    // repeats skip silently (no duplicate native call, no double hold).
    const removalSeen = new Set<string>();
    for (const ws of reply.workspaces) {
        if (ws.exist || ws.bind === "" || ws.hold !== "") {
            continue;
        }
        if (removalSeen.has(ws.bind)) {
            continue;
        }
        removalSeen.add(ws.bind);
        // Desired absence is eligibility only: re-enumerate occupants
        // (including unmanaged/floating/minimized/fullscreen/maximized via
        // the fresh desktop occupant count), visibility, pins, mappings, and
        // minimum count. Unknown occupancy keeps. Outstanding transfers or
        // desired window rows referencing this workspace retain it
        // (transaction lifetime): removal must not race an in-flight
        // send/view/migration. Desired omission alone never removes.
        const referenced = reply.transfers.some((t) => t.out === ws.out && t.ws === ws.id) ||
            reply.windows.some((w) => w.out === ws.out && w.ws === ws.id);
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
        // Native target for proof and setters (the reply row names the
        // logical workspace). Unbound or unknown targets hold.
        const targetNative = nativeWsOf(reply, t.out, t.ws);
        // Read-back confirmation first: the TARGET output actually showing
        // the target settles even a held/ambiguous operation with no new
        // native call. Any-output visibility never settles: the target
        // output exact view decides.
        if (targetNative !== null && liveViewShowsOn(env, t.out, targetNative)) {
            state.settleAttempt(t.op, "applied", "");
            state.noteAppliedViewTarget(t.out, t.op, targetNative);
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
        const wsRef = targetNative === null ? null : safeResolve(() => env.desktopById(targetNative));
        const outRef = safeResolve(() => env.resolveOutput(t.out));
        if (wsRef === null || outRef === null) {
            result.held += 1;
            continue;
        }
        state.armPossiblyExecuted(t.op, "view", `${t.out}/${t.ws}`);
        // Fresh same-turn native currents around the write; the baseline
        // is the entry observation's current for the target output.
        const viewPre = readNativeViewNow(env, t.out);
        const viewBase = observationNativeView(observation, t.out);
        let threw = false;
        let ok = false;
        try {
            ok = env.switchView(wsRef, outRef);
        } catch (error) {
            void error;
            threw = true;
        }
        const viewPost = readNativeViewNow(env, t.out);
        // One native view write ran: the mechanical disposition rides the
        // next report under the owning view identity. Logical pre/post/base
        // stay null; the fresh native currents ride the native keys.
        state.recordViewAttempt({
            op: t.op, out: t.out, ws: t.ws,
            setter: threw ? "threw" : ok ? "accepted" : "refused",
            pre: null, post: null, base: null,
            nativePre: viewPre, nativePost: viewPost, nativeBase: viewBase,
            attemptMs: stamp,
        });
        if (targetNative !== null && liveViewShowsOn(env, t.out, targetNative)) {
            result.viewWrites += 1;
            state.settleAttempt(t.op, "applied", "");
            state.noteAppliedViewTarget(t.out, t.op, targetNative);
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
    // view). Members come from the core-published desired rows homed at the
    // target under a controlled target scope (never the requester-only
    // terminal, which external clients cannot expose to the actuator): a
    // missing, held, or untouched scope authorizes no view and no
    // settlement, while an empty desired set under a controlled scope is
    // complete and proceeds to the view gates below. The source output
    // comes from the advisory correlated result, else the paired refill
    // child, else the pending members' shared observed homing. Each member
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
        // Core-published actuation scope for the target cell (mirroring the
        // Windows v2 `scope_actuatable` gate): Controlled actuates, and
        // Untouched (core-owned floating workspaces) actuates only when its
        // output row exists unheld and no gesture barrier is up. Missing
        // scopes, Hold, and Untouched with a held/missing output or an
        // active gesture authorize no write and no retirement: holds with
        // no view, never missing-equals-empty. No member/frame authority
        // bypass: the row gates below keep their own Controlled checks.
        const scopeAuthority = reply.scopes.find((s) => s.scope === `${t.out}/${t.ws}`)?.auth;
        const outputUnheld = reply.outputs.some((o) => o.out === t.out && o.hold === "");
        const scopeActuatable = scopeAuthority === "controlled" ||
            (scopeAuthority === "untouched" && outputUnheld && !observation.gesture);
        if (!scopeActuatable) {
            result.held += 1;
            continue;
        }
        // Desired member rows homed at the target: never the requester-only
        // terminal, which cannot reach an external client's actuator. An
        // empty desired set under the controlled scope above is complete and
        // proceeds to the view gates; any member not yet observable holds
        // without inventing writes for unknown ids.
        const members = reply.windows
            .filter((w) => w.out === t.out && w.ws === t.ws)
            .map((w) => w.win);
        // Advisory source provenance for the refill view only (never member
        // authority): the correlated result when it names this transfer's
        // target, else the paired child or observed homing below.
        const advisorySourceOut = env.takeResult !== null && env.takeResult.op === "migrate-workspace" &&
            env.takeResult.targetOut === t.out && env.takeResult.targetWs === t.ws
            ? env.takeResult.sourceOut
            : null;
        if (members.some((id) => !byWin.has(id))) {
            // Not every member is observable yet (actuation lag or a stale
            // snapshot): hold without inventing writes for unknown ids.
            result.held += 1;
            continue;
        }
        // Native target for every live comparison and setter below (the
        // reply row names the logical workspace). Unbound or unknown
        // targets hold with no attempt recorded.
        const targetNative = nativeWsOf(reply, t.out, t.ws);
        if (targetNative === null) {
            result.held += 1;
            continue;
        }
        const targetLogical = { out: t.out, ws: targetNative };
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
        let sourceOut: string | null = advisorySourceOut;
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
        if (sourceOut === t.out) {
            // Members already homed at the target: observed homing no
            // longer names the source, and the target must never refill
            // itself with a remaining row. The refill waits for a reply
            // that names the source (paired child or correlated result).
            sourceOut = null;
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
                if (home === null || home.out !== t.out || home.ws !== targetNative) {
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
        let scanIdx = -1;
        for (const id of members) {
            scanIdx += 1;
            const observed = byWin.get(id);
            if (observed === undefined) {
                continue;
            }
            const home = liveHomeOf(observed.ref);
            const pre = migrationHomeShape(home, observed, targetLogical);
            if (home === null) {
                traceMigrate(env, `omnitiler:world:migrate-prescan op=${t.op} idx=${scanIdx}/${members.length} pre=${pre} branch=held-unreadable`);
                result.held += 1;
                superseded = true;
                break;
            }
            if (home.out === t.out && home.ws === targetNative) {
                traceMigrate(env, `omnitiler:world:migrate-prescan op=${t.op} idx=${scanIdx}/${members.length} pre=${pre} branch=already-homed`);
                continue;
            }
            const atSource = home.out === observed.out && home.ws === observed.ws;
            const partialProgress =
                (home.out === t.out && home.ws === observed.ws) ||
                (home.out === observed.out && home.ws === targetNative);
            if (!atSource && !partialProgress) {
                traceMigrate(env, `omnitiler:world:migrate-prescan op=${t.op} idx=${scanIdx}/${members.length} pre=${pre} branch=superseded`);
                state.settleAttempt(t.op, "superseded-external", "");
                superseded = true;
                break;
            }
            traceMigrate(env, `omnitiler:world:migrate-prescan op=${t.op} idx=${scanIdx}/${members.length} pre=${pre} branch=pending`);
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
        let actIdx = -1;
        for (const id of members) {
            actIdx += 1;
            const observed = byWin.get(id);
            if (observed === undefined) {
                continue;
            }
            let home = liveHomeOf(observed.ref);
            const pre = migrationHomeShape(home, observed, targetLogical);
            // Fresh same-turn native pre for every attempt row below: the
            // live desktop id before any setter in this iteration.
            const memberPreWs = liveNativeWsOf(home);
            if (home === null) {
                traceMigrate(env, `omnitiler:world:migrate-member op=${t.op} idx=${actIdx}/${members.length} pre=${pre} srcView=null tgtView=null echo=skipped set=skipped proof=unreadable branch=failed-home-unreadable`);
                result.held += 1;
                failed = true;
                break;
            }
            if (home.out === t.out && home.ws === targetNative) {
                traceMigrate(env, `omnitiler:world:migrate-member op=${t.op} idx=${actIdx}/${members.length} pre=${pre} srcView=na tgtView=na echo=skipped set=skipped proof=target branch=already-homed`);
                state.noteAppliedWindowTarget(id, t.op, t.out, targetNative);
                continue;
            }
            if (!preSetterOk(env, reply, entryObs, observed, false)) {
                traceMigrate(env, `omnitiler:world:migrate-member op=${t.op} idx=${actIdx}/${members.length} pre=${pre} srcView=na tgtView=na echo=skipped set=skipped proof=${pre} branch=failed-preset`);
                result.stoppedMidApply = true;
                break;
            }
            const needOutMove = home.out !== t.out;
            const outRef: object | null = needOutMove ? safeResolve(() => env.resolveOutput(t.out)) : null;
            const wsRef = safeResolve(() => env.desktopById(targetNative));
            if ((needOutMove && outRef === null) || wsRef === null) {
                traceMigrate(env, `omnitiler:world:migrate-member op=${t.op} idx=${actIdx}/${members.length} pre=${pre} srcView=na tgtView=na echo=skipped set=skipped proof=${pre} branch=failed-resolve`);
                result.held += 1;
                failed = true;
                break;
            }
            if (!armed) {
                armed = true;
                state.armPossiblyExecuted(t.op, "transfer", `${t.out}/${t.ws}`);
            }
            // Fresh per-output currents BEFORE the native output call, so
            // the pinned synchronous echo below is verifiable per member.
            // Nulls fail closed at the check.
            let echoSrcView: string | null = null;
            let echoTgtView: string | null = null;
            if (needOutMove && outRef !== null) {
                try {
                    echoSrcView = env.readLiveView(observed.out);
                } catch (error) {
                    void error;
                    echoSrcView = null;
                }
                try {
                    echoTgtView = env.readLiveView(t.out);
                } catch (error) {
                    void error;
                    echoTgtView = null;
                }
            }
            let threw = false;
            let outOk = false;
            if (needOutMove && outRef !== null) {
                try {
                    outOk = env.sendToOutput(observed.ref, outRef);
                } catch (error) {
                    void error;
                    threw = true;
                }
                if (!betweenStepsOk(env, reply, entryObs, observed)) {
                    traceMigrate(env, `omnitiler:world:migrate-member op=${t.op} idx=${actIdx}/${members.length} pre=${pre} srcView=${echoSrcView === null ? "null" : "ok"} tgtView=${echoTgtView === null ? "null" : "ok"} echo=skipped set=skipped proof=${pre} branch=failed-between`);
                    state.recordMemberAttempt({
                        op: t.op, win: id, out: t.out, ws: t.ws,
                        setter: threw ? "threw" : outOk ? "accepted" : "refused",
                        pre: null, post: null,
                        nativePre: memberPreWs,
                        nativePost: readNativeWsNow(env, observed.ref),
                        attemptMs: stamp,
                    });
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
            if (home !== null && home.out === t.out && home.ws === targetNative) {
                traceMigrate(env, `omnitiler:world:migrate-member op=${t.op} idx=${actIdx}/${members.length} pre=${pre} srcView=${echoSrcView === null ? "null" : "ok"} tgtView=${echoTgtView === null ? "null" : "ok"} echo=skipped set=skipped proof=target branch=homed-no-write`);
                if (needOutMove && outRef !== null) {
                    result.membershipWrites += 1;
                    state.recordMemberAttempt({
                        op: t.op, win: id, out: t.out, ws: t.ws,
                        setter: threw ? "threw" : outOk ? "accepted" : "refused",
                        pre: null, post: null,
                        nativePre: memberPreWs,
                        nativePost: liveNativeWsOf(home),
                        attemptMs: stamp,
                    });
                }
                state.noteAppliedWindowTarget(id, t.op, t.out, targetNative);
                continue;
            }
            // The exact expected native echo of our own output setter is
            // tolerated (the same flight continues to the desktop set
            // below); a throw or any other third location still fails
            // closed for the whole migration.
            const echoGate: NativeEchoGate = needOutMove && outOk && !threw && home !== null
                ? tolerateNativeOutputEcho(env, reply, entryObs, observed, echoSrcView, echoTgtView, home, t.out)
                : "skipped";
            const echoOk = echoGate === "ok";
            const midShape = migrationHomeShape(home, observed, targetLogical);
            if (home === null || (!echoOk && ((home.out !== observed.out && home.out !== t.out) ||
                (home.ws !== observed.ws && home.ws !== targetNative)))) {
                traceMigrate(env, `omnitiler:world:migrate-member op=${t.op} idx=${actIdx}/${members.length} pre=${pre} srcView=${echoSrcView === null ? "null" : "ok"} tgtView=${echoTgtView === null ? "null" : "ok"} echo=${echoGate} set=skipped proof=${midShape} branch=superseded`);
                if (needOutMove && outRef !== null) {
                    state.recordMemberAttempt({
                        op: t.op, win: id, out: t.out, ws: t.ws,
                        setter: threw ? "threw" : outOk ? "accepted" : "refused",
                        pre: null, post: null,
                        nativePre: memberPreWs,
                        nativePost: liveNativeWsOf(home),
                        attemptMs: stamp,
                    });
                }
                state.settleAttempt(t.op, "superseded-external", "");
                superseded = true;
                break;
            }
            let setTag = "skipped";
            if (home.ws !== targetNative && wsRef !== null) {
                try {
                    if (env.setMembership(observed.ref, targetNative, wsRef)) {
                        result.membershipWrites += 1;
                        setTag = "done-true";
                    } else {
                        setTag = "done-false";
                    }
                } catch (error) {
                    void error;
                    setTag = "threw";
                }
            }
            let proof: { out: string; ws: string } | null = null;
            try {
                proof = env.readLiveMembership(observed.ref);
            } catch (error) {
                void error;
                proof = null;
            }
            // One setter ran: the disposition rides the next report under
            // the stable migration identity. No setter ran means no row.
            if ((needOutMove && outRef !== null) || setTag !== "skipped") {
                const migrateSetter: FrameSetter = threw || setTag === "threw"
                    ? "threw"
                    : setTag === "done-false" || (needOutMove && !outOk)
                        ? "refused"
                        : "accepted";
                state.recordMemberAttempt({
                    op: t.op, win: id, out: t.out, ws: t.ws,
                    setter: migrateSetter, pre: null, post: null,
                    nativePre: memberPreWs,
                    nativePost: liveNativeWsOf(proof),
                    attemptMs: stamp,
                });
            }
            const proofShape = migrationHomeShape(proof, observed, targetLogical);
            if (proof !== null && proof.out === t.out && proof.ws === targetNative) {
                traceMigrate(env, `omnitiler:world:migrate-member op=${t.op} idx=${actIdx}/${members.length} pre=${pre} srcView=${echoSrcView === null ? "null" : "ok"} tgtView=${echoTgtView === null ? "null" : "ok"} echo=${echoGate} set=${setTag} proof=${proofShape} branch=homed`);
                state.noteAppliedWindowTarget(id, t.op, t.out, targetNative);
            } else {
                traceMigrate(env, `omnitiler:world:migrate-member op=${t.op} idx=${actIdx}/${members.length} pre=${pre} srcView=${echoSrcView === null ? "null" : "ok"} tgtView=${echoTgtView === null ? "null" : "ok"} echo=${echoGate} set=${setTag} proof=${proofShape} branch=held-unproven`);
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
        // shows the migrated workspace; the source refills only to the
        // paired refill child. Absence of a paired child authorizes no
        // source view: the adapter never invents a refill. NO follow/refill
        // view switches until ALL fresh member arrival proof is in: a
        // partial failure or stale membership leaves every view untouched
        // so unmoved members stay visible.
        // Each proves independently on its exact output; the transfer
        // settles applied only with all members homed and both views
        // proven. Wrong-output visibility never proves.
        const membersProven = !failed && !unproven;
        const targetShown = liveViewShowsLogical(env, reply, t.out, t.ws);
        let refillChild = pairedRefill;
        // Newer external input supersedes a proven-then-abandoned refill
        // child: drop it for this apply with no replacement.
        let refillAbandoned = false;
        if (refillChild !== undefined && sourceOut !== null) {
            const appliedView = state.appliedViewTarget.get(sourceOut);
            const topoNow = observation.topology.find((entry) => entry.out === sourceOut);
            if (appliedView !== undefined && appliedView.op === refillChild.op &&
                topoNow !== undefined && topoNow.current !== appliedView.ws) {
                state.settleAttempt(refillChild.op, "superseded-external", "");
                refillChild = undefined;
                refillAbandoned = true;
            }
        }
        // Both views proven-then-abandoned (user switch, not lag): the
        // refill above was just superseded as abandoned AND the target
        // output also left its applied view under this same migration.
        // Retrying the target alone would fight the user's switch while
        // core settles it as external input, so skip it with no
        // replacement and no attempt row. Target-only lag (refill still
        // shown) still retries below.
        let targetAbandoned = false;
        if (refillAbandoned) {
            const appliedTarget = state.appliedViewTarget.get(t.out);
            const topoTgt = observation.topology.find((entry) => entry.out === t.out);
            if (appliedTarget !== undefined && appliedTarget.op === t.op &&
                topoTgt !== undefined && topoTgt.current !== appliedTarget.ws) {
                targetAbandoned = true;
            }
        }
        let refillWs: string | null = null;
        let refillOp = t.op;
        if (refillChild !== undefined) {
            refillWs = refillChild.ws;
            refillOp = refillChild.op;
        }
        const refillNative = refillWs === null || refillChild === undefined
            ? refillWs
            : nativeWsOf(reply, refillChild.out, refillChild.ws);
        const refillShown = refillWs === null || (sourceOut !== null && refillNative !== null && liveViewShowsOn(env, sourceOut, refillNative));
        let viewsProven = targetShown && refillShown;
        if (membersProven && !viewsProven && replyFresh(env, reply, entryObs)) {
            if (!targetShown && !targetAbandoned) {
                // Same-flight switch: the entry check above is the
                // freshness proof; read-back decides.
                const targetPre = readNativeViewNow(env, t.out);
                const targetBase = observationNativeView(observation, t.out);
                const targetView = actuateViewSameFlight(env, reply, t);
                if (targetView.setter !== null) {
                    // One native target-view write ran under the stable
                    // migration identity: mechanical fact only. Logical
                    // pre/post/base stay null; fresh natives ride the
                    // native keys.
                    state.recordViewAttempt({
                        op: t.op, out: t.out, ws: t.ws,
                        setter: targetView.setter, pre: null, post: null, base: null,
                        nativePre: targetPre,
                        nativePost: readNativeViewNow(env, t.out),
                        nativeBase: targetBase,
                        attemptMs: stamp,
                    });
                }
                if (targetView.proven) {
                    result.viewWrites += 1;
                }
            }
            if (refillWs !== null && refillChild !== undefined && sourceOut !== null && refillNative !== null && !liveViewShowsOn(env, sourceOut, refillNative)) {
                if (state.isHeldAmbiguous(refillOp)) {
                    // Never retry an ambiguous refill view under a fresh
                    // revision: hold for proof instead of a second call.
                    result.held += 1;
                } else {
                    // Stable child op identity arms possibly-executed
                    // BEFORE the view setter, with proof/refusal
                    // feedback. Same-flight switch: the entry check
                    // above already proved freshness, so the target
                    // switch's own synchronous signal cannot fail this
                    // attempt closed.
                    state.armPossiblyExecuted(refillOp, "view", `${sourceOut}/${refillWs}`);
                    const refillPre = readNativeViewNow(env, sourceOut);
                    const refillBase = observationNativeView(observation, sourceOut);
                    const refillView = actuateViewSameFlight(env, reply, { ...refillChild, follow: false });
                    if (refillView.setter !== null) {
                        state.recordViewAttempt({
                            op: refillOp, out: refillChild.out, ws: refillChild.ws,
                            setter: refillView.setter, pre: null, post: null, base: null,
                            nativePre: refillPre,
                            nativePost: readNativeViewNow(env, sourceOut),
                            nativeBase: refillBase,
                            attemptMs: stamp,
                        });
                    }
                    if (refillView.proven) {
                        result.viewWrites += 1;
                        state.settleAttempt(refillOp, "applied", "");
                        state.noteAppliedViewTarget(sourceOut, refillOp, refillNative ?? refillWs);
                    } else {
                        result.held += 1;
                    }
                }
            }
            viewsProven = liveViewShowsLogical(env, reply, t.out, t.ws) &&
                (refillWs === null || (sourceOut !== null && refillNative !== null && liveViewShowsOn(env, sourceOut, refillNative)));
        }
        // All members homed is rechecked from the per-member proofs above:
        // any held member blocks settlement without another native call.
        if (!failed && !unproven && viewsProven) {
            state.settleAttempt(t.op, "applied", "");
            if (sourceOut !== null) {
                state.noteAppliedViewTarget(t.out, t.op, targetNative);
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
        return !liveViewShowsLogical(env, reply, t.out, t.ws);
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
        // Focused-float preservation: automatic admission intents
        // (af-<window>; world_report.rs mints fx-<seq> only for executed
        // explicit focus commands) never activate over a focused
        // floating subject, which covers automatic fixed-size floats
        // (observed floating) plus manual/sticky. Explicit fx-*
        // workspace/send-follow focus always actuates below. No geometry,
        // stacking, or keep-above change here.
        const focusedId = observation.focusedGlobal;
        if (typeof focusedId === "string" && focusedId !== "" && focusedId !== fx.target && fx.id.startsWith("af-")) {
            const focused = byWin.get(focusedId);
            if (focused !== undefined && (focused.floating === true || focused.sticky === true)) {
                const reason = focused.sticky === true
                    ? "sticky-active"
                    : focused.fixedAuto === true
                        ? "automatic-active"
                        : "floating-active";
                env.log(`omnitiler:world:focus-skipped kind=effect reason=${reason} window=${focusedId} target=${fx.target} id=${fx.id}`);
                state.noteFeedback({ op: fx.id, kind: "effect", outcome: "unattempted", hold: "refused-target" });
                result.held += 1;
                continue;
            }
        }
        // Overlay guard: game/excluded/overlay rows (fit-excluded,
        // fullscreen, sticky, floating, including first-seen fullscreen
        // holds) never take automatic focus/raise/stack writes. The effect
        // stays spent (at-most-once) with explicit feedback, so focus is
        // never stolen on a later unrelated report. Admission focus
        // (`af-<win>`, D4 newcomer intent) is the one exception: a
        // still-plain ordinary float takes the one-shot activation, where
        // ordinary is the core's reply classification (a floating reply
        // row, user or capacity alike), never observed floating alone: a
        // game/excluded float the core never classified stays refused.
        // Automatic fixed-size floats (observed fixedAuto without
        // fixedSuppress) never take admission activation: zero
        // geometry/focus writes by construction. Suppressed tiles stay on
        // the tiled path below. Fullscreen (game), maximized, and sticky
        // overlays refuse even there.
        const replyRow = reply.windows.find((w) => w.win === fx.target);
        const admissionFloatTarget =
            fx.id.startsWith("af-") &&
            observed.floating === true &&
            replyRow?.floating === true &&
            !(observed.fixedAuto === true && observed.fixedSuppress !== true);
        if (admissionFloatTarget) {
            if (observed.fullscreen || observed.maximized || observed.sticky) {
                state.noteFeedback({ op: fx.id, kind: "effect", outcome: "unattempted", hold: "refused-target" });
                result.held += 1;
                continue;
            }
        } else if (observed.fitExcluded || observed.fullscreen || observed.sticky || observed.floating) {
            state.noteFeedback({ op: fx.id, kind: "effect", outcome: "unattempted", hold: "refused-target" });
            result.held += 1;
            continue;
        }
        // D01 sibling fence for admission intent: a fullscreen/maximized
        // sibling in the target's own domain owns host activation, so an
        // automatic newcomer focus never fires behind it. The effect stays
        // spent with explicit feedback, never retried; explicit
        // command-issued focus rides a non-`af-` id and stays exempt.
        // Observed flags plus a live fullscreen/maximized recheck fence a
        // sibling that flipped mid-apply; unreadable live safety alone
        // never refuses.
        if (fx.id.startsWith("af-")) {
            const home = observation.domains.find((d) =>
                d.windows.some((w) => w.win === fx.target),
            );
            const siblingBlocked = home?.windows.some((w) => {
                if (w.win === fx.target) {
                    return false;
                }
                if (w.fullscreen || w.maximized) {
                    return true;
                }
                try {
                    const safety = env.readLiveSafety(w.ref);
                    return (
                        safety !== null &&
                        (safety.fullscreen === true || safety.maximized === true)
                    );
                } catch (error) {
                    void error;
                    return false;
                }
            });
            if (siblingBlocked === true) {
                state.noteFeedback({ op: fx.id, kind: "effect", outcome: "unattempted", hold: "refused-target" });
                result.held += 1;
                continue;
            }
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
        // Correlated attempt/result for focus setter runs: the outcome
        // reports synchronous setter acceptance only, with a separate
        // native active token; accepted-but-not-activated is never
        // misreported as refused.
        env.log(`omnitiler:world:automatic-focus-attempt req=${reply.req} rev=${reply.rev} target=${fx.target} reason=planned-focus`);
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
        env.log(`omnitiler:world:automatic-focus-result req=${reply.req} rev=${reply.rev} target=${fx.target} reason=planned-focus outcome=${threw ? "threw" : ok ? "setter-accepted" : "setter-refused"} active=${active ? "true" : "false"}`);
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
