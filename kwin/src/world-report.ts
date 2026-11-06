// World v4 report/reply/envelope JSON shapes (strict v4, no v1/v2/v3 route).
// Keys mirror tiler-protocol/src/world_protocol.rs exactly; the broker
// (crates/omnitiler/src/world_broker.rs) owns sequencing, so the inner
// report below carries NO broker-owned keys (owner, adapter, gen, obs,
// reqSeq, req, cmdSeq, command, monotonicMs): carrying any of them refuses
// as unknown-field server-side. Commands below are the SOLE workspace
// policy route: every selection/send/migration shortcut builds one of
// these, never a synchronous TS desktop decision.

import { isCanonicalU64, isCorrelationId, isGenerationId, isOpaqueId, utf8ByteLength, WORLD_CONTRACT_VERSION, WORLD_MAX_REPLY_BYTES } from "./world-seq";
import type { RawWindowFact, WorldObservation } from "./world-observe";
import { rawNativeGestureJson, type NativeGestureSnapshot } from "./world-gesture";
import type { MigrationSourceRefill } from "./world-settings-live";

export type WorldMode = "per-output-local" | "global-unique" | "shared";

// Typed send source (v3 exact): a genuinely semantic explicit output, or
// one of the two core-owned implicit fallbacks. Legacy string/null `out`
// payloads refuse server-side.
export type SendSource =
    | { explicit: string }
    | { implicit: "home-then-action" | "home-then-cached-live" };

// Exact v3 command vocabulary (tiler-protocol world_protocol.rs
// decode_command). direction-only intents carry no endpoint; selection
// outputs are null for core-resolved (cursor-first) or an opaque id for a
// genuinely semantic explicit output; send-numbered index 0 is the trailing
// path; migrate-directional is direction-only (the core resolves the action
// source from the neutral report facts).
export type WorldCommand =
    | { op: "focus"; win: string; direction: string }
    | { op: "send"; win: string; out: string; ws: string; follow: boolean }
    | { op: "switch-view"; out: string; ws: string }
    | { op: "select-workspace"; out: string; ws: string }
    | { op: "select-numbered"; out: string | null; index: number }
    | { op: "select-relative"; out: string | null; delta: -1 | 1 }
    | { op: "select-previous"; out: string | null }
    | { op: "select-trailing"; out: string | null }
    | { op: "send-relative"; win: string; source: SendSource; delta: -1 | 1; follow: boolean }
    | { op: "send-numbered"; win: string; source: SendSource; index: number; follow: boolean }
    | { op: "set-workspace-tiling"; out: string; ws: string; tiled: boolean }
    | { op: "toggle-workspace-tiling"; out: string; ws: string }
    | { op: "toggle-maximize"; win: string }
    | { op: "toggle-float"; win: string; rect: [number, number, number, number] | null }
    | { op: "migrate-workspace"; from: string; to: string; ws: string }
    | { op: "focus-directional"; win: string; direction: string }
    | { op: "move-directional"; win: string; direction: string; sameAxis: string }
    | { op: "send-directional"; win: string; direction: string; follow: boolean }
    | { op: "migrate-directional"; direction: string }
    | { op: "operation"; command: Record<string, unknown> };

export type WorldAuthorityScope =
    | { kind: "output"; out: string }
    | { kind: "workspace"; out: string; ws: string };

// Closed scope-authority token plus its hold reason. Controlled/untouched
// rows carry reason ""; hold rows carry exactly one closed hold token.
export type WorldScopeAuth = "controlled" | "untouched" | "hold";
export type WorldHoldReason =
    | "" | "unreadable" | "incomplete-inventory" | "gesture-barrier" | "refused-target"
    | "ambiguous-creation" | "occupied-removal" | "constraint-clamp" | "minimized";

export interface WorldReplyScope {
    scope: WorldAuthorityScope;
    auth: WorldScopeAuth;
    reason: WorldHoldReason;
}
export interface WorldReplyWorkspace {
    // `id` is the stable core logical workspace id; `bind` is the
    // host-owned native desktop id bound to it ("" when unbound).
    id: string;
    out: string;
    bind: string;
    scope: WorldAuthorityScope;
    order: number;
    mode: "tiled" | "floating";
    exist: boolean;
    hold: WorldHoldReason;
    // Explicit trailing-spare designation, present only when true; the
    // parser fills false when the key is absent. Present-but-false refuses.
    trailing: boolean;
}
export interface WorldReplyOutput {
    out: string;
    view: string;
    mode: "tiled" | "floating";
    hold: WorldHoldReason;
}
// Composed visibility target (outbound only): the desired shown state,
// never a native unminimize command. `rev` is the world revision at target
// authorization. `fallback` is core permission for hide-only fallback;
// never set for shows. Null on the row means no visibility target.
export interface WorldReplyVisibility {
    shown: boolean;
    rev: string;
    fallback: boolean;
}
// Independent native-state target (outbound only): the core-resolved target
// for one property. `cmdSeq` names the originating explicit command, never
// a parsed operation id.
export interface WorldReplyStateTarget {
    property: "fullscreen" | "maximized" | "sticky" | "above";
    value: boolean;
    auth: WorldScopeAuth;
    reason: WorldHoldReason;
    cmdSeq: string | null;
}
export interface WorldReplyWindow {
    win: string;
    out: string;
    ws: string;
    alloc: [number, number, number, number] | null;
    frame: [number, number, number, number] | null;
    frame_auth: WorldScopeAuth;
    frame_hold: WorldHoldReason;
    member_auth: WorldScopeAuth;
    member_hold: WorldHoldReason;
    // Explicit retained float-placement operation (`frame_op`) or "".
    // The adapter writes float geometry ONLY under a non-empty token,
    // exactly once; ordinary floats carry "" and never take frame writes.
    frame_op: string;
    // Explicit ordinary-float classification: true for every retained float
    // exception row in a controlled scope (user and capacity floats alike).
    // The adapter never infers float from a missing allocation alone: only
    // a floating row with untouched frame authority converges float intent.
    floating: boolean;
    // Diagnostic float origin: "" for ordinary floats and non-floats,
    // "capacity" for capacity floats. Provenance only, never behavior.
    float_origin: "" | "capacity";
    // Explicit retained clear operation (`clear_op`) or "": directional
    // maximize-clear verdicts spend exactly once like frame_op.
    clear_op: string;
    // Explicit command correlation for the published frame/clear target:
    // non-null only with a published explicit retained operation token.
    // Quoted decimal on the wire, null when absent.
    cmdSeq: string | null;
    // Composed visibility target, if any; independent visibility write
    // authority below it. Null/untouched/empty until the lane migrates.
    visibility: WorldReplyVisibility | null;
    visibility_auth: WorldScopeAuth;
    visibility_hold: WorldHoldReason;
    // Independent native-state targets, at most one per property.
    state: WorldReplyStateTarget[];
}
export interface WorldReplyTransfer {
    op: string;
    kind: "window" | "view" | "migration";
    win: string;
    out: string;
    ws: string;
    follow: boolean;
    held: boolean;
    // Retained originating command correlation (null on commandless work,
    // never a parsed id). Quoted decimal on the wire, null when absent.
    cmdSeq: string | null;
}
// Correlated per-command outcome coexisting with the snapshot (mirrors
// tiler-core world_report::CommandResult exactly; unknown ops refuse).
// Move/resize/tiled-admit/focus/send results ride the snapshot,
// transfers, and effects with `result: null`: the snapshot IS the result.
// The actuator lane strips the terminal result service-side (the reply the
// script receives carries null); the take lane serves it requester-only.
export interface WorldDragHover {
    domainOut: string; domainWs: string; leaf: string; win: string;
    rev: string; prior: { group: string; edge: "left" | "right" | "top" | "bottom" | "center" } | null;
    priorOut: string; priorWs: string;
}
export type WorldDirectionalOutcome =
    | "focus-refused-floating" | "move-refused-floating"
    | "focus-refused-workspace-floating" | "move-refused-workspace-floating"
    | "move-refused-fullscreen" | "focus-refused-maximize"
    | "focus-float-retained" | "focus-unfocused-retained"
    | "focus-float-refused-stale" | "move-float-refused-stale"
    | "move-float-refused-invalid-geometry" | "move-float-refused-constraints"
    | "move-float-unknown-output" | "focus-float-applied" | "move-float-applied"
    | "move-maximize-clear";
export type WorldDirectionalCause =
    | "no-target" | "unknown-window" | "focus-present" | "no-source" | "no-adjacent-output";
export type WorldRejectKind =
    | "focus-unknown" | "send-target-unknown" | "send-window-unknown" | "send-refused"
    | "view-output-unknown" | "view-target-unknown" | "transfer-in-flight" | "direction-invalid"
    | "scope-held" | "selection-unknown" | "migration-refused" | "operation-refused"
    | "tiling-target-unknown" | "state-unsupported" | "state-unknown" | "fullscreen-not-exitable"
    | "stale-render-target";
export type WorldReplyResult =
    | {
        op: "drag-preview"; rev: string;
        domainOut: string; domainWs: string; sourceOut: string; sourceWs: string;
        rect: [number, number, number, number];
        hover: WorldDragHover | null;
        win: string;
    }
    | { op: "toggle-float"; win: string; rect: [number, number, number, number] | null; noSpace: boolean }
    | { op: "maximize"; win: string; maximized: boolean; clearFullscreen: boolean; wasMaximized: boolean }
    | {
        op: "directional"; outcome: WorldDirectionalOutcome;
        win: string | null; rect: [number, number, number, number] | null; cause: WorldDirectionalCause | null;
    }
    | { op: "rejected"; kind: WorldRejectKind }
    | {
        op: "migrate-workspace"; direction: string;
        sourceOut: string; sourceWs: string; targetOut: string; targetWs: string;
        members: string[]; floats: number;
    };
export interface WorldReplyEffect {
    id: string;
    kind: "focus" | "raise" | "stack";
    target: string;
    rev: string;
    // Typed correlation cause: explicit-command carries the dispatching
    // command in cmdSeq; the other causes carry null and must never
    // actuate focus on adapters keeping observed-focus authority.
    cause: "explicit-command" | "automatic-admission" | "focus-follows-cursor";
    // Originating command sequence for explicit-command effects, null
    // otherwise. Quoted decimal on the wire, null when absent.
    cmdSeq: string | null;
    // Arrival-ready grant. Only an explicit selection may grant this.
    awaitShown: boolean;
}
// Terminal retained-operation verdicts incorporated by this report, at most
// one entry per retired operation (directional maximize-clear verdicts and
// explicit float-placement verdicts). Only `float-frame-proven` means native
// observation confirmed the target; a clear continuation means the intent
// resumed, never that the final frame arrived.
export interface WorldReplyOperationSettlement {
    op: string;
    win: string;
    outcome:
    | "continued-tiled" | "continued-float" | "refused-proven"
    | "abandoned-external-clear" | "abandoned-focus-changed" | "abandoned-view-changed"
    | "abandoned-homing-changed" | "abandoned-closed" | "abandoned-fullscreen"
    | "superseded" | "continuation-refused"
    | "float-frame-proven" | "float-refused-proven" | "float-abandoned";
    // Empty, a closed reject kind, or one of 16 narrow settlement tokens.
    cause: string;
}
// One terminal selection event: the existing parent transfer/view
// operation, the terminal accounting outcome, the frozen switch target
// failures (empty on complete), the core-derived foreground outcome, and
// whether separately authorized fallback hides ran (never on this lane).
export interface WorldReplySwitchEvent {
    op: string;
    outcome: "complete" | "completed-with-failures";
    failed: string[];
    focus: "proven" | "refused" | "unproven";
    fallback: boolean;
}
// One narrow-hold diagnostic: the held scope plus the affected lane, the
// identifiable row id, and the closed diagnostic kind. No dedup: repeated
// rows are independent evidence.
export interface WorldReplyDiagnostic {
    scope: WorldAuthorityScope;
    row: "screen" | "window" | "native" | "attempt" | "presentation";
    id: string | null;
    kind: "unreadable" | "invalid-fact" | "conflicting-fact";
}
// Explicit native creation authorization (capability-gated core desire):
// at most one entry per outstanding core-owned creation, each executed at
// most once per stable `op`. An unbound workspace row without a desire row
// authorizes nothing. Core owns binding; the adapter never owns a
// logical binding table.
export interface WorldNativeCreation {
    op: string;
    ws: string;
    out: string;
}
// Raw native creation evidence for the report lane: the mechanical facts
// behind one native desktop create call (live native orders immediately
// before/after plus the host-correlated created id when the host can prove
// which fresh desktop its call produced). `created` null means
// uncorrelated. At most one entry per outstanding creation op; receipts
// persist across reports until the core incorporates them.
export interface WorldCreationAttempt {
    op: string;
    ws: string;
    out: string;
    setter: "accepted" | "refused" | "threw";
    before: string[];
    after: string[];
    created: string | null;
    attemptMs: string;
}
// Mechanical outcome of one native setter call. Pure actuator disposition,
// never a policy conclusion: `accepted` means the setter reported success
// (the caller still re-reads; success alone never proves completion),
// `refused` is an explicit native refusal, `threw` is a throwing/ambiguous
// call that may have executed.
export type FrameSetter = "accepted" | "refused" | "threw";
// One normalized raw frame attempt: the mechanical facts behind a single
// native frame write. `target` echoes the desired frame as correlation;
// `readback` is the immediate post-write frame (`null` unreadable);
// `dpiBefore`/`dpiAfter` are the live window DPI around the write (`null`
// unreadable, or the adapter coordinate space has no per-window basis;
// never guessed); `attemptMs` is the adapter monotonic clock at the write;
// `op` correlates an explicit float-placement write to its retained core
// operation (`""` for ordinary writes, never inferred); `pre` is the live
// frame immediately before the write (`null` unreadable; `Some` without an
// operation id refuses server-side). At most one latest-wins entry per
// window.
export interface WorldFrameAttempt {
    win: string;
    target: [number, number, number, number];
    setter: FrameSetter;
    readback: [number, number, number, number] | null;
    dpiBefore: number | null;
    dpiAfter: number | null;
    attemptMs: string;
    op: string;
    pre: [number, number, number, number] | null;
}
// One normalized raw membership attempt: the mechanical facts behind a
// single native membership write for one window under one retained
// operation. `pre`/`post` are the live homings immediately before/after
// the write. Under host-owned workspace domains the adapter cannot express
// native homing in the logical attempt fields (translation through the
// adapter binding table would forge logical evidence and poison core
// third-location proof), so both ride `null` there: the setter plus exact
// logical target correlation is the whole row, and the core holds the
// flight ambiguous without re-actuation. The fresh same-turn native
// desktop ids ride `nativePre`/`nativePost` (null when unreadable); the
// core translates them through its own per-output bindings. At most one
// latest-wins entry per operation window.
export interface WorldMemberAttempt {
    op: string;
    win: string;
    out: string;
    ws: string;
    setter: FrameSetter;
    pre: { out: string; ws: string } | null;
    post: { out: string; ws: string } | null;
    nativePre?: string | null;
    nativePost?: string | null;
    attemptMs: string;
}
// One normalized raw view attempt: the mechanical facts behind a single
// native view write for one owning phase (standalone view id, window
// follow parent, migration-target parent, or refill child id). Same
// host-owned null-evidence rule as membership: `pre`/`post`/`base` ride
// `null`, never translated native ids. The fresh same-turn native current
// ids ride `nativePre` (live current before the write), `nativePost`
// (live current immediately after), and `nativeBase` (observation-time
// current for the target output, null when unreadable). The core
// translates them through its own bindings for the attempt's target
// output. At most one latest-wins entry per owning view phase.
export interface WorldViewAttempt {
    op: string;
    out: string;
    ws: string;
    setter: FrameSetter;
    pre: string | null;
    post: string | null;
    base: string | null;
    nativePre?: string | null;
    nativePost?: string | null;
    nativeBase?: string | null;
    attemptMs: string;
}
// One normalized raw maximize-clear attempt: the mechanical facts behind a
// single native nonactivating clear (fresh pre/post maximized states plus
// the setter disposition). The core owns confirmation, proven refusal,
// ambiguity, and abandonment; a setter boolean alone never proves arrival.
export interface WorldClearAttempt {
    op: string;
    win: string;
    setter: FrameSetter;
    pre: boolean | null;
    post: boolean | null;
    attemptMs: string;
}
// Uniform per-reply presentation (mirrors
// tiler-core world_sizing::encode_presentation exactly): the tick-time
// foreground token the shapes were computed for (the renderer revalidates
// the live foreground against it and never paints stale shapes), the
// border shape, the underlay shape, and the pager content. The adapter
// never derives policy from it.
export interface WorldPresentationBorder {
    visible: boolean;
    reason: "" | "disabled" | "no-target" | "minimized" | "maximized" | "fullscreen" | "host-hidden" | "hidden-workspace" | "shell" | "suspended";
    // Anchored border target: the subject window, or null when no target.
    subject: string | null;
    // Exact border outsets (any sign i32), or null when no target.
    outsets: [number, number, number, number] | null;
    // Border width / corner radius (bare ints >= 0), or null when no target.
    width: number | null;
    radius: number | null;
    // Frame witness proving the border geometry, or null when unproven.
    witness: { win: string; obs: string; frame: [number, number, number, number]; foreground: string } | null;
}
export type WorldPresentationUnderlay =
    | {
        status: "found"; reason: "";
        baseRev: string;
        homeOut: string; homeWs: string;
        group: string; leaf: string; win: string;
        union: [number, number, number, number];
        outer: [number, number, number, number];
        members: Array<{ leaf: string; win: string; rect: [number, number, number, number] }>;
    }
    | {
        status: "hidden";
        reason: "disabled" | "minimized" | "maximized" | "fullscreen" | "floating" | "pending" | "diverged" | "root-leaf" | "focus-changed" | "no-group" | "idle" | "suspended";
        baseRev: string | null;
    };
export interface WorldPresentationPagerWindow {
    win: string;
    frame: [number, number, number, number];
}
export interface WorldPresentationPagerWorkspace {
    id: string;
    ordinal: number;
    label: string;
    current: boolean;
    occupied: boolean;
    containsFocused: boolean;
    // Explicit trailing-spare designation, present only when true; the
    // parser fills false when the key is absent.
    trailing: boolean;
    windows: WorldPresentationPagerWindow[];
}
export interface WorldPresentationPagerOutput {
    out: string;
    workspaces: WorldPresentationPagerWorkspace[];
}
export interface WorldPresentation {
    foreground: string;
    // Issuing world revision label for the computed shapes.
    rev: string;
    // Neutral focused-fullscreen suspension verdict (stage 5): the observed
    // output whose shapes are suspended, or null when no focused
    // fullscreen holds. Absent on the wire means null; an explicit null
    // refuses.
    suspendedOutput: string | null;
    border: WorldPresentationBorder;
    underlay: WorldPresentationUnderlay;
    pager: WorldPresentationPagerOutput[];
}
export interface WorldReply {
    adapter: string;
    gen: string;
    req: string;
    obs: string;
    rev: string;
    ready: boolean;
    consumed: boolean;
    scopes: WorldReplyScope[];
    workspaces: WorldReplyWorkspace[];
    outputs: WorldReplyOutput[];
    windows: WorldReplyWindow[];
    transfers: WorldReplyTransfer[];
    effects: WorldReplyEffect[];
    result: WorldReplyResult | null;
    gestureSettled: { id: string; committed: boolean } | null;
    // Broker-only bound actuator association for the actual normalized
    // ingress (`{instance, serial, coreId}`), cached in the exact response
    // replay; null on every omitted or unsupported lane. The script
    // requires `gestureSettled.id === association.coreId` AND the flight
    // native pair `=== association` native pair before any ack fires.
    // Never a core/protocol field: the broker stamps it, the script vets it.
    gestureAssoc: { instance: string; serial: string; coreId: string } | null;
    operationSettled: WorldReplyOperationSettlement[];
    // Terminal selection events, at most one entry per existing parent
    // transfer/view operation.
    switchSettled: WorldReplySwitchEvent[];
    // Narrow-hold diagnostics: independent evidence rows, never deduped.
    diagnostics: WorldReplyDiagnostic[];
    // Core-owned native creation authorizations for this reply: explicit
    // desires only, at most one entry per outstanding creation op.
    nativeCreations: WorldNativeCreation[];
    // Desired cursor warp point (cross-output focus only) or null. The
    // adapter never warps (unsupported): null or ignored, never chased.
    warpCursor: [number, number] | null;
    // Accepted core intent writes (opaque session records, never policy):
    // string replaces, null deletes, at most one row per subject. The lane
    // is required on every v4 core reply (bootstrap and capture alike);
    // this adapter only applies and persists verbatim.
    intentWrites: WorldIntentWrite[];
    presentation: WorldPresentation;
    // Broker transport bindings: per-capture source correlation, the local
    // native-invalidation watermark the capture carried, and the broker
    // dispatch-clock reading (foreign epoch for raw attemptMs stamps).
    source: string;
    nativeInvalid: string;
    clock: string;
}
export interface WorldRejection {
    outcome: "rejected";
    kind: string;
    message: string;
    gen: string;
    source: string;
    nativeInvalid: string;
    clock: string;
}
// Queue-lane replies (QueueWorldCommand): filed or duplicate, never
// re-executing. globalCmd is the broker command sequence (null only when
// the duplicate predates bounded memory).
export interface WorldQueueReply {
    outcome: "queued" | "duplicate";
    gen: string;
    client: string;
    clientCmd: string;
    globalCmd: string | null;
}
// Take-lane replies (TakeCommandResult): pending while queued or
// undispatched; ready (with the core result, possibly null) or failed
// (with the explicit rejection kind/message) once settled; duplicate after
// bounded eviction (never re-executes).
export type WorldTakeReply =
    | { outcome: "pending"; gen: string }
    | { outcome: "ready"; gen: string; globalCmd: string; result: WorldReplyResult | null }
    | { outcome: "failed"; gen: string; globalCmd: string; kind: string; message: string }
    | { outcome: "duplicate"; gen: string };
// Presentation-lane replies (ReadWorldPresentation): snapshot plus
// presentation plus the renderer-only drop preview, no fx/results/write
// tokens. `preview` is the trimmed drop-preview geometry (paint `rect`,
// mover-home `sourceOut`/`sourceWs`, insertion-neighbor `win`) when the
// cached reply carried a drop preview, else null. `win` is the insertion
// neighbor, never the mover identity: the subscriber must not treat it as
// a mover match. `producerInstance`/`producerSerial` name the actual
// producing native gesture, or nulls when that identity is unproven (a
// renderer hides unproven previews, fail closed).
export interface WorldPreview {
    rect: [number, number, number, number];
    sourceOut: string;
    sourceWs: string;
    win: string;
    producerInstance: string | null;
    producerSerial: string | null;
}
export interface WorldPresentationReply {
    outcome: "presentation";
    gen: string;
    req: string;
    rev: string;
    obs: string;
    // Causal coverage the cached capture covered (nulls when unproven:
    // missing proof yields no drawable coverage, never guessed truth).
    coveredInstance: string | null;
    coveredInvalid: string | null;
    scopes: WorldReplyScope[];
    workspaces: WorldReplyWorkspace[];
    outputs: WorldReplyOutput[];
    windows: WorldReplyWindow[];
    presentation: WorldPresentation;
    preview: WorldPreview | null;
}
// Wait-lane replies (WaitWorldWake): the long-held call completes when
// queued work can drain. `wake` carries the advanced server epoch cursor
// (the follow-up capture drains it); `lease` is the idle server lease
// with the cursor unchanged (re-arm with the same cursor); `superseded`
// answers a waiter replaced by a newer registration.
export type WorldWaitReply =
    | { outcome: "wake"; gen: string; cursor: string }
    | { outcome: "lease"; gen: string; cursor: string }
    | { outcome: "superseded"; gen: string };

const DIRECTIONS = new Set(["left", "right", "up", "down"]);

function isRect(value: unknown): value is [number, number, number, number] {
    if (!Array.isArray(value) || value.length !== 4) {
        return false;
    }
    return (value as ReadonlyArray<unknown>).every(
        (n) => typeof n === "number" && Number.isInteger(n),
    );
}

function isI32(value: unknown): value is number {
    return typeof value === "number" && Number.isInteger(value) &&
        value >= -2147483648 && value <= 2147483647;
}

const GEOMETRY_BOUND = 16384;

// Carried-rectangle gate over adapter-normalized integer geometry (mirrors
// tiler-core bounds::valid_carried_rect): every reply/presentation/result
// rect must satisfy this, never a bare integer-quad check.
function isCarriedRect(value: unknown): value is [number, number, number, number] {
    if (!isRect(value)) {
        return false;
    }
    const [x, y, w, h] = value;
    return w > 0 && h > 0 &&
        x >= -GEOMETRY_BOUND && x <= GEOMETRY_BOUND &&
        y >= -GEOMETRY_BOUND && y <= GEOMETRY_BOUND &&
        w <= GEOMETRY_BOUND && h <= GEOMETRY_BOUND &&
        x + w <= 2147483647 && y + h <= 2147483647;
}

// Closed hold-reason vocabulary plus the empty non-hold marker.
const HOLD_OR_EMPTY = new Set([
    "", "unreadable", "incomplete-inventory", "gesture-barrier", "refused-target",
    "ambiguous-creation", "occupied-removal", "constraint-clamp", "minimized",
]);

// Staged v4 reply section caps (mirror the core report-side lane caps).
// Over-cap sections refuse the whole message.
const MAX_REPLY_SCOPES = 1024;
const MAX_REPLY_WORKSPACES = 1024;
const MAX_REPLY_OUTPUTS = 16;
const MAX_REPLY_WINDOWS = 10000;
const MAX_REPLY_TRANSFERS = 1024;
const MAX_REPLY_EFFECTS = 1024;
const MAX_REPLY_OPERATIONS = 1024;
const MAX_REPLY_NATIVE_CREATIONS = 256;
const MAX_REPLY_SWITCH_EVENTS = 1024;
const MAX_REPLY_DIAGNOSTICS = 10000;
const MAX_STATE_TARGETS_PER_WINDOW = 4;

// Closed v4 scope union: the output arm and workspace arm have disjoint key
// sets; no string/composite compatibility form is accepted.
function parseAuthorityScope(value: unknown): WorldAuthorityScope {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        throw new Error("scope-shape");
    }
    const o = value as Record<string, unknown>;
    if (o["kind"] === "output") {
        if (!hasExactKeys(o, ["kind", "out"]) || !isOpaqueId(o["out"])) {
            throw new Error("scope-output");
        }
        return { kind: "output", out: o["out"] as string };
    }
    if (o["kind"] === "workspace") {
        if (!hasExactKeys(o, ["kind", "out", "ws"]) || !isOpaqueId(o["out"]) || !isOpaqueId(o["ws"])) {
            throw new Error("scope-workspace");
        }
        return { kind: "workspace", out: o["out"] as string, ws: o["ws"] as string };
    }
    throw new Error("scope-kind");
}

// Closed authority pair: controlled/untouched rows carry reason "",
// hold rows carry exactly one closed hold token.
function parseAuthorityPair(
    o: Record<string, unknown>, authKey: string, reasonKey: string,
): { auth: WorldScopeAuth; reason: WorldHoldReason } {
    const auth = o[authKey];
    const reason = o[reasonKey];
    if (typeof auth !== "string" || typeof reason !== "string") {
        throw new Error("auth-shape");
    }
    if (auth === "controlled" || auth === "untouched") {
        if (reason !== "") {
            throw new Error("nonhold-reason");
        }
        return { auth, reason: "" };
    }
    if (auth === "hold") {
        if (reason === "" || !HOLD_OR_EMPTY.has(reason)) {
            throw new Error("hold-reason");
        }
        return { auth: "hold", reason: reason as WorldHoldReason };
    }
    throw new Error("auth");
}

// Quoted-decimal counter or null: JSON numbers refuse, never coerce.
function optU64orNull(value: unknown): string | null {
    if (value === null) {
        return null;
    }
    if (typeof value === "string" && isCanonicalU64(value)) {
        return value;
    }
    throw new Error("u64");
}

function reqOpaqueId(value: unknown): string {
    if (!isOpaqueId(value)) {
        throw new Error("opaque-id");
    }
    return value as string;
}

const EFFECT_KINDS = new Set(["focus", "raise", "stack"]);
const EFFECT_CAUSES = new Set(["explicit-command", "automatic-admission", "focus-follows-cursor"]);
const OPERATION_OUTCOMES = new Set([
    "continued-tiled", "continued-float", "refused-proven",
    "abandoned-external-clear", "abandoned-focus-changed", "abandoned-view-changed",
    "abandoned-homing-changed", "abandoned-closed", "abandoned-fullscreen",
    "superseded", "continuation-refused",
    "float-frame-proven", "float-refused-proven", "float-abandoned",
]);
// 16 narrow settlement tokens (anything else valid here is a reject kind).
const OPERATION_NARROW_CAUSES = new Set([
    "baseline-non-subject", "closed", "constraints", "early-verdict", "external",
    "fixed", "floating", "homing-loss", "invalid-direction", "invalid-geometry",
    "non-subject-focus", "refused-with-unmaximized", "sticky", "tiling-changed",
    "topology-unreadable", "unknown-output", "unknown-window",
]);
const REJECT_KINDS = new Set([
    "focus-unknown", "send-target-unknown", "send-window-unknown", "send-refused",
    "view-output-unknown", "view-target-unknown", "transfer-in-flight", "direction-invalid",
    "scope-held", "selection-unknown", "migration-refused", "operation-refused",
    "tiling-target-unknown", "state-unsupported", "state-unknown", "fullscreen-not-exitable",
    "stale-render-target",
]);
const SWITCH_OUTCOMES = new Set(["complete", "completed-with-failures"]);
const SWITCH_FOCUS = new Set(["proven", "refused", "unproven"]);
const DIAGNOSTIC_ROWS = new Set(["screen", "window", "native", "attempt", "presentation"]);
const DIAGNOSTIC_KINDS = new Set(["unreadable", "invalid-fact", "conflicting-fact"]);
// Anchored-border hide reasons (v4): no `cloaked` token on this lane.
const BORDER_V4_REASONS = new Set([
    "", "disabled", "no-target", "minimized", "maximized", "fullscreen",
    "host-hidden", "hidden-workspace", "shell", "suspended",
]);
const STATE_PROPERTIES = new Set(["fullscreen", "maximized", "sticky", "above"]);
const DRAG_EDGES = new Set(["left", "right", "top", "bottom", "center"]);

// Command constructors: the only workspace-policy vocabulary. Numbered
// index 1..9 (0 is the trailing path for send-numbered); relative delta
// -1/+1. Selection outputs are null for core-resolved (cursor-first) or an
// opaque id for a genuinely semantic explicit output.
export function cmdSelectWorkspace(out: string, ws: string): WorldCommand {
    return { op: "select-workspace", out, ws };
}
export function cmdSelectNumbered(out: string | null, index: number): WorldCommand | null {
    if (!Number.isInteger(index) || index < 1 || index > 9) {
        return null;
    }
    return { op: "select-numbered", out, index };
}
export function cmdSelectRelative(out: string | null, delta: number): WorldCommand | null {
    if (delta !== -1 && delta !== 1) {
        return null;
    }
    return { op: "select-relative", out, delta };
}
export function cmdSelectPrevious(out: string | null): WorldCommand {
    return { op: "select-previous", out };
}
export function cmdSelectTrailing(out: string | null): WorldCommand {
    return { op: "select-trailing", out };
}
export function cmdSend(win: string, out: string, ws: string, follow: boolean): WorldCommand {
    return { op: "send", win, out, ws, follow };
}
// Typed send sources: explicit names a genuinely semantic output, implicit
// selects a core-owned fallback chain. Never a bare string.
export function explicitSource(out: string): SendSource {
    return { explicit: out };
}
export function implicitSource(token: "home-then-action" | "home-then-cached-live"): SendSource {
    return { implicit: token };
}
export function cmdSendRelative(win: string, source: SendSource, delta: number, follow: boolean): WorldCommand | null {
    if (delta !== -1 && delta !== 1) {
        return null;
    }
    return { op: "send-relative", win, source, delta, follow };
}
// Numbered send over the core-resolved positional ring: index 0 is the
// trailing path (reuse or stable pending token under core), 1..9 are ring
// positions. Resolution is all core; the adapter never maps positions
// locally. Sends never view-switch, including stays.
export function cmdSendNumbered(win: string, source: SendSource, index: number, follow: boolean): WorldCommand | null {
    if (!Number.isInteger(index) || index < 0 || index > 9) {
        return null;
    }
    return { op: "send-numbered", win, source, index, follow };
}
// Planner-owned per-workspace tiling mode: explicit sets choose a mode;
// the neutral toggle flips the core-retained mode at execution time so
// racing toggles compose with no adapter snapshot policy. The adapter
// never decides tiling mode locally.
export function cmdSetWorkspaceTiling(out: string, ws: string, tiled: boolean): WorldCommand {
    return { op: "set-workspace-tiling", out, ws, tiled };
}
export function cmdToggleWorkspaceTiling(out: string, ws: string): WorldCommand {
    return { op: "toggle-workspace-tiling", out, ws };
}
export function cmdToggleMaximize(win: string): WorldCommand {
    return { op: "toggle-maximize", win };
}
export function cmdMigrateWorkspace(from: string, to: string, ws: string): WorldCommand | null {
    if (from === to) {
        return null;
    }
    return { op: "migrate-workspace", from, to, ws };
}
// Direction-only intents (core-owned topology): the adapter names the
// direction (plus window or follow flag) only; the core resolves the
// strict target. No float routing key on the wire: float-origin fallback
// keeps its press-time subject LOCALLY (queued-intent flag in the entry),
// the wire shape is identical.
export function cmdFocusDirectional(win: string, direction: string): WorldCommand | null {
    if (!DIRECTIONS.has(direction)) {
        return null;
    }
    return { op: "focus-directional", win, direction };
}
export function cmdMoveDirectional(win: string, direction: string, sameAxis: string): WorldCommand | null {
    if (!DIRECTIONS.has(direction)) {
        return null;
    }
    if (sameAxis !== "group-with-neighbor" && sameAxis !== "swap-with-neighbor") {
        return null;
    }
    return { op: "move-directional", win, direction, sameAxis };
}
export function cmdSendDirectional(win: string, direction: string, follow: boolean): WorldCommand | null {
    if (!DIRECTIONS.has(direction)) {
        return null;
    }
    return { op: "send-directional", win, direction, follow };
}
// Whole-workspace migration by direction only: the adapter names the
// direction; the core resolves the action source and its current view from
// the neutral report facts (actionForeground/actionCached). Legacy
// from/ws keys are deleted and refuse as unknown fields.
export function cmdMigrateDirectional(direction: string): WorldCommand | null {
    if (!DIRECTIONS.has(direction)) {
        return null;
    }
    return { op: "migrate-directional", direction };
}
export function cmdSwitchView(out: string, ws: string): WorldCommand {
    return { op: "switch-view", out, ws };
}
export function cmdFocus(win: string, direction: string): WorldCommand | null {
    if (!DIRECTIONS.has(direction)) {
        return null;
    }
    return { op: "focus", win, direction };
}

// Strict bootstrap/discovery reply parse: the core bootstrap shape with
// `ready:false`, WITHOUT the broker transport bindings (no capture has
// flown yet, so there is nothing to bind). Binds empty source, zero
// watermark, and zero clock so downstream code treats it uniformly. The
// 4MiB wire gate applies before JSON parsing.
export function parseDiscoveryReply(text: string): WorldReply | WorldRejection | null {
    if (utf8ByteLength(text) > WORLD_MAX_REPLY_BYTES) {
        return null;
    }
    let value: unknown;
    try {
        value = JSON.parse(text);
    } catch (error) {
        void error;
        return null;
    }
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        return null;
    }
    const r = value as Record<string, unknown>;
    if (r["v"] !== WORLD_CONTRACT_VERSION) {
        return null;
    }
    if (r["outcome"] === "rejected") {
        const parsed = parseWorldReply(text);
        return isWorldRejection(parsed) ? parsed : null;
    }
    if (typeof r["outcome"] === "string" || typeof r["ready"] !== "boolean") {
        return null;
    }
    // The bootstrap lane carries exactly the 23 core keys (no capture has
    // flown yet): parseCoreReply enforces the exact set and defaults the
    // absent transport bindings without validation, never a guess.
    const parsed = parseCoreReply(r, true);
    if (parsed === null || parsed.ready !== false) {
        return null;
    }
    return parsed;
}

// Strict reply parse: ready full-state shape, bounded rejection, or the
// queue/take/presentation lane shapes. Unknown fields on the core reply
// refuse; counters must be canonical decimal strings. v4 only: v3 refuses
// on the missing switchSettled/diagnostics lanes. The 4MiB wire gate
// applies before JSON parsing.
export function parseWorldReply(
    text: string,
): WorldReply | WorldRejection | WorldQueueReply | WorldTakeReply | WorldPresentationReply | WorldWaitReply | null {
    if (utf8ByteLength(text) > WORLD_MAX_REPLY_BYTES) {
        return null;
    }
    let value: unknown;
    try {
        value = JSON.parse(text);
    } catch (error) {
        void error;
        return null;
    }
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        return null;
    }
    const r = value as Record<string, unknown>;
    if (r["v"] !== WORLD_CONTRACT_VERSION) {
        return null;
    }
    const outcome = r["outcome"];
    if (outcome === "rejected") {
        // Genuine v4 broker rejection (world_broker.rs `bind_response`):
        // the core 5-key error plus the bound source watermark, clock,
        // and the broker-owned `gestureAssoc` (always null on rejections,
        // never a fabricated binding). Missing or non-null gestureAssoc
        // refuses: the wire shape is exact, never optional.
        if (!hasExactKeys(r, ["v", "outcome", "kind", "message", "gen", "source", "nativeInvalid", "clock", "gestureAssoc"])) {
            return null;
        }
        if (
            typeof r["kind"] !== "string" || typeof r["message"] !== "string" ||
            typeof r["gen"] !== "string" || typeof r["source"] !== "string" ||
            (r["source"] !== "" && !isCorrelationId(r["source"])) ||
            typeof r["nativeInvalid"] !== "string" || typeof r["clock"] !== "string" ||
            !isCanonicalU64(r["nativeInvalid"]) || !isCanonicalU64(r["clock"]) ||
            r["gestureAssoc"] !== null
        ) {
            return null;
        }
        return {
            outcome: "rejected", kind: r["kind"] as string, message: r["message"] as string, gen: r["gen"] as string,
            source: r["source"] as string, nativeInvalid: r["nativeInvalid"] as string,
            clock: r["clock"] as string,
        };
    }
    if (typeof outcome === "string") {
        return parseLaneReply(r, outcome);
    }
    return parseCoreReply(r, false);
}

function hasExactKeys(r: Record<string, unknown>, keys: ReadonlyArray<string>): boolean {
    const got = [...Object.keys(r)].sort();
    const want = [...keys].sort();
    return got.length === want.length && got.every((k, i) => k === want[i]);
}

function parseLaneReply(
    r: Record<string, unknown>,
    outcome: string,
): WorldQueueReply | WorldTakeReply | WorldPresentationReply | WorldWaitReply | null {
    const gen = typeof r["gen"] === "string" ? (r["gen"] as string) : null;
    if (gen === null || !isGenerationId(gen)) {
        return null;
    }
    if (outcome === "queued" || outcome === "duplicate") {
        if (outcome === "duplicate" && r["client"] === undefined) {
            // Take-lane duplicate carries no client binding.
            if (!hasExactKeys(r, ["v", "outcome", "gen"])) {
                return null;
            }
            return { outcome: "duplicate", gen };
        }
        if (!hasExactKeys(r, ["v", "outcome", "gen", "client", "clientCmd", "globalCmd"])) {
            return null;
        }
        if (typeof r["client"] !== "string" || !isOpaqueId(r["client"])) {
            return null;
        }
        if (typeof r["clientCmd"] !== "string" || !isCanonicalU64(r["clientCmd"])) {
            return null;
        }
        const global = r["globalCmd"];
        if (!(typeof global === "string" || global === null)) {
            return null;
        }
        if (typeof global === "string" && !isCanonicalU64(global)) {
            return null;
        }
        return {
            outcome, gen, client: r["client"] as string, clientCmd: r["clientCmd"] as string,
            globalCmd: global,
        };
    }
    if (outcome === "pending") {
        if (!hasExactKeys(r, ["v", "outcome", "gen"])) {
            return null;
        }
        return { outcome: "pending", gen };
    }
    if (outcome === "ready") {
        if (!hasExactKeys(r, ["v", "outcome", "gen", "globalCmd", "result"])) {
            return null;
        }
        if (typeof r["globalCmd"] !== "string" || !isCanonicalU64(r["globalCmd"])) {
            return null;
        }
        let result: WorldReplyResult | null = null;
        try {
            result = parseResult(r["result"]);
        } catch (error) {
            void error;
            return null;
        }
        return { outcome: "ready", gen, globalCmd: r["globalCmd"] as string, result };
    }
    if (outcome === "failed") {
        if (!hasExactKeys(r, ["v", "outcome", "gen", "globalCmd", "kind", "message"])) {
            return null;
        }
        if (
            typeof r["globalCmd"] !== "string" || !isCanonicalU64(r["globalCmd"]) ||
            typeof r["kind"] !== "string" || typeof r["message"] !== "string"
        ) {
            return null;
        }
        return {
            outcome: "failed", gen, globalCmd: r["globalCmd"] as string,
            kind: r["kind"] as string, message: r["message"] as string,
        };
    }
    if (outcome === "presentation") {
        return parsePresentationReply(r, gen);
    }
    if (outcome === "wake" || outcome === "lease") {
        if (!hasExactKeys(r, ["v", "outcome", "gen", "cursor"])) {
            return null;
        }
        const cursor = r["cursor"];
        if (typeof cursor !== "string" || !isCanonicalU64(cursor)) {
            return null;
        }
        return { outcome, gen, cursor };
    }
    if (outcome === "superseded") {
        if (!hasExactKeys(r, ["v", "outcome", "gen"])) {
            return null;
        }
        return { outcome: "superseded", gen };
    }
    return null;
}

function parseScopeRow(e: unknown): WorldReplyScope {
    if (typeof e !== "object" || e === null || Array.isArray(e)) {
        throw new Error("scope-keys");
    }
    const o = e as Record<string, unknown>;
    if (!hasExactKeys(o, ["scope", "auth", "reason"])) {
        throw new Error("scope-keys");
    }
    const scope = parseAuthorityScope(o["scope"]);
    const { auth, reason } = parseAuthorityPair(o, "auth", "reason");
    return { scope, auth, reason };
}

function parseWorkspaceRow(e: unknown): WorldReplyWorkspace {
    if (typeof e !== "object" || e === null || Array.isArray(e)) {
        throw new Error("ws-keys");
    }
    const o = e as Record<string, unknown>;
    // Exact v4 workspace shape: id/out/bind/scope/order/mode/exist/hold
    // plus the optional explicit `trailing` marker (present only when
    // true; absent means false; present-but-false refuses).
    const keys = Object.keys(o);
    if (
        (keys.length !== 8 && keys.length !== 9) ||
        !("id" in o && "out" in o && "bind" in o && "scope" in o &&
            "order" in o && "mode" in o && "exist" in o && "hold" in o) ||
        (keys.length === 9 && o["trailing"] !== true)
    ) {
        throw new Error("ws-keys");
    }
    const bind = o["bind"];
    if (
        !isOpaqueId(o["id"]) || !isOpaqueId(o["out"]) ||
        (bind !== "" && !isOpaqueId(bind)) ||
        typeof o["order"] !== "number" || !Number.isInteger(o["order"]) || (o["order"] as number) < 0 ||
        (o["mode"] !== "tiled" && o["mode"] !== "floating") ||
        typeof o["exist"] !== "boolean" ||
        typeof o["hold"] !== "string" || !HOLD_OR_EMPTY.has(o["hold"] as string)
    ) {
        throw new Error("ws");
    }
    return {
        id: o["id"] as string, out: o["out"] as string, bind: bind as string,
        scope: parseAuthorityScope(o["scope"]),
        order: o["order"] as number, mode: o["mode"] as "tiled" | "floating",
        exist: o["exist"] as boolean, hold: o["hold"] as WorldHoldReason,
        trailing: keys.length === 9,
    };
}

function parseOutputRow(e: unknown): WorldReplyOutput {
    if (typeof e !== "object" || e === null || Array.isArray(e)) {
        throw new Error("out-keys");
    }
    const o = e as Record<string, unknown>;
    if (!hasExactKeys(o, ["out", "view", "mode", "hold"])) {
        throw new Error("out-keys");
    }
    const view = o["view"];
    if (
        !isOpaqueId(o["out"]) ||
        (view !== "" && !isOpaqueId(view)) ||
        (o["mode"] !== "tiled" && o["mode"] !== "floating") ||
        typeof o["hold"] !== "string" || !HOLD_OR_EMPTY.has(o["hold"] as string)
    ) {
        throw new Error("out");
    }
    return {
        out: o["out"] as string, view: view as string,
        mode: o["mode"] as "tiled" | "floating", hold: o["hold"] as WorldHoldReason,
    };
}

// Core window row (exact v4, 18 keys): the actuator lane always carries
// the retained write tokens and the visibility/state lanes (possibly
// empty/null), never an absent key. Any unknown (retired or future) field
// refuses.
function parseVisibilityTarget(value: unknown): WorldReplyVisibility {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        throw new Error("visibility-keys");
    }
    const o = value as Record<string, unknown>;
    if (!hasExactKeys(o, ["shown", "rev", "fallback"])) {
        throw new Error("visibility-keys");
    }
    if (
        typeof o["shown"] !== "boolean" || typeof o["rev"] !== "string" ||
        !isCanonicalU64(o["rev"] as string) || typeof o["fallback"] !== "boolean"
    ) {
        throw new Error("visibility");
    }
    // A show never carries hide-fallback permission.
    if ((o["shown"] as boolean) && (o["fallback"] as boolean)) {
        throw new Error("visibility-show-fallback");
    }
    return { shown: o["shown"] as boolean, rev: o["rev"] as string, fallback: o["fallback"] as boolean };
}

function parseStateTarget(value: unknown): WorldReplyStateTarget {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        throw new Error("state-keys");
    }
    const o = value as Record<string, unknown>;
    if (!hasExactKeys(o, ["property", "value", "auth", "reason", "cmdSeq"])) {
        throw new Error("state-keys");
    }
    if (typeof o["property"] !== "string" || !STATE_PROPERTIES.has(o["property"] as string)) {
        throw new Error("state-property");
    }
    if (typeof o["value"] !== "boolean") {
        throw new Error("state-value");
    }
    const { auth, reason } = parseAuthorityPair(o, "auth", "reason");
    return {
        property: o["property"] as WorldReplyStateTarget["property"],
        value: o["value"] as boolean, auth, reason,
        cmdSeq: optU64orNull(o["cmdSeq"]),
    };
}

function parseWindowRow(e: unknown): WorldReplyWindow {
    if (typeof e !== "object" || e === null || Array.isArray(e)) {
        throw new Error("win-keys");
    }
    const o = e as Record<string, unknown>;
    if (!hasExactKeys(o, [
        "win", "out", "ws", "alloc", "frame",
        "frame_auth", "frame_hold", "member_auth", "member_hold",
        "frame_op", "floating", "float_origin", "clear_op", "cmdSeq",
        "visibility", "visibility_auth", "visibility_hold", "state",
    ])) {
        throw new Error("win-keys");
    }
    if (
        !isOpaqueId(o["win"]) || !isOpaqueId(o["out"]) || !isOpaqueId(o["ws"]) ||
        (typeof o["frame_op"] !== "string" || (o["frame_op"] !== "" && !isOpaqueId(o["frame_op"]))) ||
        (typeof o["clear_op"] !== "string" || (o["clear_op"] !== "" && !isOpaqueId(o["clear_op"]))) ||
        typeof o["floating"] !== "boolean" ||
        (o["float_origin"] !== "" && o["float_origin"] !== "capacity")
    ) {
        throw new Error("win");
    }
    const alloc = o["alloc"];
    const frame = o["frame"];
    if (!(alloc === null || isCarriedRect(alloc)) || !(frame === null || isCarriedRect(frame))) {
        throw new Error("rect");
    }
    const framePair = parseAuthorityPair(o, "frame_auth", "frame_hold");
    const memberPair = parseAuthorityPair(o, "member_auth", "member_hold");
    const visibilityPair = parseAuthorityPair(o, "visibility_auth", "visibility_hold");
    const visibilityRaw = o["visibility"];
    const visibility = visibilityRaw === null ? null : parseVisibilityTarget(visibilityRaw);
    const stateRaw = o["state"];
    if (!Array.isArray(stateRaw) || stateRaw.length > MAX_STATE_TARGETS_PER_WINDOW) {
        throw new Error("state-count");
    }
    const seen = new Set<string>();
    const state = (stateRaw as ReadonlyArray<unknown>).map((item) => {
        const target = parseStateTarget(item);
        if (seen.has(target.property)) {
            throw new Error("state-target-count");
        }
        seen.add(target.property);
        return target;
    });
    return {
        win: o["win"] as string, out: o["out"] as string, ws: o["ws"] as string,
        alloc: (alloc as [number, number, number, number] | null) ?? null,
        frame: (frame as [number, number, number, number] | null) ?? null,
        frame_auth: framePair.auth, frame_hold: framePair.reason,
        member_auth: memberPair.auth, member_hold: memberPair.reason,
        frame_op: o["frame_op"] as string,
        floating: o["floating"] as boolean, float_origin: o["float_origin"] as "" | "capacity",
        clear_op: o["clear_op"] as string,
        cmdSeq: optU64orNull(o["cmdSeq"]),
        visibility,
        visibility_auth: visibilityPair.auth, visibility_hold: visibilityPair.reason,
        state,
    };
}

// Presentation window row: the projection strips the retained write
// tokens server-side (keys absent). Any carried frame_op/clear_op,
// result, transfer, effect, or other one-shot lane here refuses.
function parsePresentationWindowRow(e: unknown): Omit<WorldReplyWindow, "frame_op" | "clear_op"> {
    if (typeof e !== "object" || e === null || Array.isArray(e)) {
        throw new Error("win");
    }
    const rec = e as Record<string, unknown>;
    if ("frame_op" in rec || "clear_op" in rec) {
        throw new Error("presentation-write-token");
    }
    const full = parseWindowRow({ ...rec, frame_op: "", clear_op: "" });
    const { frame_op: _frameOp, clear_op: _clearOp, ...rest } = full;
    void _frameOp;
    void _clearOp;
    return rest;
}

function parseTransferRow(e: unknown): WorldReplyTransfer {
    if (typeof e !== "object" || e === null || Array.isArray(e)) {
        throw new Error("transfer-keys");
    }
    const o = e as Record<string, unknown>;
    if (!hasExactKeys(o, ["op", "kind", "win", "out", "ws", "follow", "held", "cmdSeq"])) {
        throw new Error("transfer-keys");
    }
    const kind = o["kind"];
    const win = o["win"];
    // Window transfers name the subject; view/migration transfers carry "".
    if (kind === "window") {
        if (!isOpaqueId(win)) {
            throw new Error("tr-win");
        }
    } else if (kind === "view" || kind === "migration") {
        if (win !== "") {
            throw new Error("tr-win");
        }
    } else {
        throw new Error("tr-kind");
    }
    if (
        !isOpaqueId(o["op"]) || !isOpaqueId(o["out"]) || !isOpaqueId(o["ws"]) ||
        typeof o["follow"] !== "boolean" || typeof o["held"] !== "boolean"
    ) {
        throw new Error("tr");
    }
    return {
        op: o["op"] as string, kind: kind as WorldReplyTransfer["kind"], win: win as string,
        out: o["out"] as string, ws: o["ws"] as string,
        follow: o["follow"] as boolean, held: o["held"] as boolean,
        cmdSeq: optU64orNull(o["cmdSeq"]),
    };
}

function parseEffectRow(e: unknown): WorldReplyEffect {
    if (typeof e !== "object" || e === null || Array.isArray(e)) {
        throw new Error("effect-keys");
    }
    const o = e as Record<string, unknown>;
    if (!hasExactKeys(o, ["id", "kind", "target", "rev", "cause", "cmdSeq", "awaitShown"])) {
        throw new Error("effect-keys");
    }
    if (
        !isOpaqueId(o["id"]) ||
        typeof o["kind"] !== "string" || !EFFECT_KINDS.has(o["kind"] as string) ||
        !isOpaqueId(o["target"]) ||
        typeof o["rev"] !== "string" || !isCanonicalU64(o["rev"] as string) ||
        typeof o["cause"] !== "string" || !EFFECT_CAUSES.has(o["cause"] as string) ||
        typeof o["awaitShown"] !== "boolean"
    ) {
        throw new Error("fx");
    }
    const explicit = (o["cause"] as string) === "explicit-command";
    const cmdSeq = optU64orNull(o["cmdSeq"]);
    // Explicit-command effects carry the dispatching command; the other
    // causes carry null and never grant arrival-ready focus.
    if (explicit && cmdSeq === null) {
        throw new Error("effect-explicit-null-seq");
    }
    if (!explicit && cmdSeq !== null) {
        throw new Error("effect-non-explicit-seq");
    }
    if (!explicit && (o["awaitShown"] as boolean)) {
        throw new Error("effect-automatic-await-shown");
    }
    return {
        id: o["id"] as string, kind: o["kind"] as WorldReplyEffect["kind"],
        target: o["target"] as string, rev: o["rev"] as string,
        cause: o["cause"] as WorldReplyEffect["cause"],
        cmdSeq, awaitShown: o["awaitShown"] as boolean,
    };
}

function parseOperationSettledRow(e: unknown): WorldReplyOperationSettlement {
    if (typeof e !== "object" || e === null || Array.isArray(e)) {
        throw new Error("op-settled-keys");
    }
    const o = e as Record<string, unknown>;
    if (!hasExactKeys(o, ["op", "win", "outcome", "cause"])) {
        throw new Error("op-settled-keys");
    }
    if (
        !isOpaqueId(o["op"]) || !isOpaqueId(o["win"]) ||
        typeof o["outcome"] !== "string" || !OPERATION_OUTCOMES.has(o["outcome"] as string) ||
        typeof o["cause"] !== "string"
    ) {
        throw new Error("op-settled");
    }
    const cause = o["cause"] as string;
    if (cause !== "" && !REJECT_KINDS.has(cause) && !OPERATION_NARROW_CAUSES.has(cause)) {
        throw new Error("op-settled-cause");
    }
    return {
        op: o["op"] as string, win: o["win"] as string,
        outcome: o["outcome"] as WorldReplyOperationSettlement["outcome"], cause,
    };
}

function parseSwitchEventRow(e: unknown): WorldReplySwitchEvent {
    if (typeof e !== "object" || e === null || Array.isArray(e)) {
        throw new Error("switch-keys");
    }
    const o = e as Record<string, unknown>;
    if (!hasExactKeys(o, ["op", "outcome", "failed", "focus", "fallback"])) {
        throw new Error("switch-keys");
    }
    if (
        !isOpaqueId(o["op"]) ||
        typeof o["outcome"] !== "string" || !SWITCH_OUTCOMES.has(o["outcome"] as string) ||
        typeof o["focus"] !== "string" || !SWITCH_FOCUS.has(o["focus"] as string) ||
        typeof o["fallback"] !== "boolean"
    ) {
        throw new Error("switch");
    }
    const failedRaw = o["failed"];
    if (!Array.isArray(failedRaw) || failedRaw.length > MAX_REPLY_WINDOWS) {
        throw new Error("switch-failed");
    }
    const failed = (failedRaw as ReadonlyArray<unknown>).map((item) => reqOpaqueId(item));
    return {
        op: o["op"] as string, outcome: o["outcome"] as WorldReplySwitchEvent["outcome"],
        failed, focus: o["focus"] as WorldReplySwitchEvent["focus"],
        fallback: o["fallback"] as boolean,
    };
}

function parseDiagnosticRow(e: unknown): WorldReplyDiagnostic {
    if (typeof e !== "object" || e === null || Array.isArray(e)) {
        throw new Error("diagnostic-keys");
    }
    const o = e as Record<string, unknown>;
    if (!hasExactKeys(o, ["scope", "row", "id", "kind"])) {
        throw new Error("diagnostic-keys");
    }
    if (
        typeof o["row"] !== "string" || !DIAGNOSTIC_ROWS.has(o["row"] as string) ||
        (o["id"] !== null && !isOpaqueId(o["id"])) ||
        typeof o["kind"] !== "string" || !DIAGNOSTIC_KINDS.has(o["kind"] as string)
    ) {
        throw new Error("diagnostic");
    }
    return {
        scope: parseAuthorityScope(o["scope"]),
        row: o["row"] as WorldReplyDiagnostic["row"],
        id: o["id"] as string | null,
        kind: o["kind"] as WorldReplyDiagnostic["kind"],
    };
}

// Explicit native creation desire row: exactly {op,ws,out}, all non-empty
// strings. Unknown keys refuse like every other core row.
function parseNativeCreationRow(e: unknown): WorldNativeCreation {
    const o = e as Record<string, unknown>;
    if (Object.keys(o).length !== 3 || !("op" in o && "ws" in o && "out" in o)) {
        throw new Error("creation-keys");
    }
    if (typeof o["op"] !== "string" || typeof o["ws"] !== "string" || typeof o["out"] !== "string") {
        throw new Error("creation");
    }
    const op = o["op"] as string;
    const ws = o["ws"] as string;
    const out = o["out"] as string;
    if (op === "" || ws === "" || out === "") {
        throw new Error("creation-empty");
    }
    return { op, ws, out };
}

// Opaque intent write from the core (v4 `intentWrites` lane): the verified
// live subject plus the exact core record (null deletes the row). The
// adapter never interprets `data`: string replaces verbatim, null deletes.
// Exact two-key rows, opaque subject, null-or-`<=1024`-UTF-8-byte data, no
// duplicate subjects, at most 10000 rows (mirrors the core lane caps).
export interface WorldIntentWrite {
    readonly win: string;
    readonly data: string | null;
}

const MAX_INTENT_WRITES = 10000;
const MAX_INTENT_DATA_BYTES = 1024;

export function parseIntentWrites(value: unknown): WorldIntentWrite[] | null {
    if (!Array.isArray(value) || value.length > MAX_INTENT_WRITES) {
        return null;
    }
    const seen = new Set<string>();
    const out: WorldIntentWrite[] = [];
    for (const item of value) {
        if (typeof item !== "object" || item === null || Array.isArray(item)) {
            return null;
        }
        const o = item as Record<string, unknown>;
        if (Object.keys(o).length !== 2 || !("win" in o && "data" in o)) {
            return null;
        }
        if (!isOpaqueId(o["win"])) {
            return null;
        }
        const data = o["data"];
        if (data !== null && typeof data !== "string") {
            return null;
        }
        if (typeof data === "string" && utf8ByteLength(data) > MAX_INTENT_DATA_BYTES) {
            return null;
        }
        const win = o["win"] as string;
        if (seen.has(win)) {
            return null;
        }
        seen.add(win);
        out.push({ win, data: data as string | null });
    }
    return out;
}

// Exact v4 core reply shape: the 23 core keys the Rust canonical encoder
// emits (including the required opaque `intentWrites`, `switchSettled`,
// and `diagnostics` lanes), plus source/nativeInvalid/clock/gestureAssoc
// on the bound (capture) lane. Any unknown key refuses, and a
// present-but-malformed `intentWrites` lane refuses the whole reply. The
// bootstrap lane (no capture flown yet) carries exactly the 23 core keys;
// the bindings default without validation.
const CORE_REPLY_KEYS = ["v", "adapter", "gen", "req", "obs", "rev", "ready", "consumed",
    "scopes", "workspaces", "outputs", "windows", "transfers", "effects", "result",
    "gestureSettled", "operationSettled", "nativeCreations", "warpCursor", "presentation",
    "intentWrites", "switchSettled", "diagnostics"] as const;
const BOUND_REPLY_KEYS = [...CORE_REPLY_KEYS, "source", "nativeInvalid", "clock", "gestureAssoc"] as const;

function parseCoreReply(r: Record<string, unknown>, allowUnbound: boolean): WorldReply | null {
    const want: ReadonlyArray<string> = allowUnbound ? CORE_REPLY_KEYS : BOUND_REPLY_KEYS;
    if (!hasExactKeys(r, want)) {
        return null;
    }
    const str = (k: string): string | null => (typeof r[k] === "string" ? (r[k] as string) : null);
    const adapter = str("adapter");
    const gen = str("gen");
    const req = str("req");
    const obs = str("obs");
    const rev = str("rev");
    // Transport bindings are absent on the bootstrap reply (no capture has
    // flown yet): unbound reads default without validation, never a guess.
    const source = str("source") ?? (allowUnbound ? "" : null);
    const nativeInvalid = str("nativeInvalid") ?? (allowUnbound ? "0" : null);
    const clock = str("clock") ?? (allowUnbound ? "0" : null);
    if (
        adapter === null || !isOpaqueId(adapter) ||
        gen === null || !isGenerationId(gen) ||
        req === null || !isCorrelationId(req) ||
        obs === null || !isCanonicalU64(obs) ||
        rev === null || !isCanonicalU64(rev) ||
        source === null ||
        (!allowUnbound && source === "") ||
        (source !== "" && !isCorrelationId(source)) ||
        nativeInvalid === null || !isCanonicalU64(nativeInvalid) ||
        clock === null || !isCanonicalU64(clock) ||
        typeof r["ready"] !== "boolean" ||
        typeof r["consumed"] !== "boolean"
    ) {
        return null;
    }
    const arr = (k: string): ReadonlyArray<unknown> | null => {
        const v = r[k];
        return Array.isArray(v) ? (v as ReadonlyArray<unknown>) : null;
    };
    const scopesRaw = arr("scopes");
    const wsRaw = arr("workspaces");
    const outRaw = arr("outputs");
    const winRaw = arr("windows");
    const trRaw = arr("transfers");
    const fxRaw = arr("effects");
    const opSettledRaw = arr("operationSettled");
    const creationsRaw = arr("nativeCreations");
    const switchRaw = arr("switchSettled");
    const diagRaw = arr("diagnostics");
    if (!scopesRaw || !wsRaw || !outRaw || !winRaw || !trRaw || !fxRaw || !opSettledRaw || !creationsRaw || !switchRaw || !diagRaw) {
        return null;
    }
    if (
        scopesRaw.length > MAX_REPLY_SCOPES || wsRaw.length > MAX_REPLY_WORKSPACES ||
        outRaw.length > MAX_REPLY_OUTPUTS || winRaw.length > MAX_REPLY_WINDOWS ||
        trRaw.length > MAX_REPLY_TRANSFERS || fxRaw.length > MAX_REPLY_EFFECTS ||
        opSettledRaw.length > MAX_REPLY_OPERATIONS || creationsRaw.length > MAX_REPLY_NATIVE_CREATIONS ||
        switchRaw.length > MAX_REPLY_SWITCH_EVENTS || diagRaw.length > MAX_REPLY_DIAGNOSTICS
    ) {
        return null;
    }
    const unique = <T>(rows: T[], key: (row: T) => string, err: string): void => {
        const seen = new Set<string>();
        for (const row of rows) {
            const k = key(row);
            if (seen.has(k)) {
                throw new Error(err);
            }
            seen.add(k);
        }
    };
    try {
        const scopes: WorldReplyScope[] = scopesRaw.map(parseScopeRow);
        unique(scopes, (s) => s.scope.kind === "output" ? `out:${s.scope.out}` : `ws:${s.scope.out}:${s.scope.ws}`, "duplicate-scope");
        const workspaces: WorldReplyWorkspace[] = wsRaw.map(parseWorkspaceRow);
        unique(workspaces, (w) => `${w.out}:${w.id}`, "duplicate-workspace");
        const outputs: WorldReplyOutput[] = outRaw.map(parseOutputRow);
        unique(outputs, (o) => o.out, "duplicate-output");
        const windows: WorldReplyWindow[] = winRaw.map(parseWindowRow);
        unique(windows, (w) => w.win, "duplicate-window");
        const transfers: WorldReplyTransfer[] = trRaw.map(parseTransferRow);
        unique(transfers, (t) => t.op, "duplicate-transfer");
        const effects: WorldReplyEffect[] = fxRaw.map(parseEffectRow);
        unique(effects, (e) => e.id, "duplicate-effect");
        const operationSettled: WorldReplyOperationSettlement[] = opSettledRaw.map(parseOperationSettledRow);
        unique(operationSettled, (o) => o.op, "duplicate-operation");
        const nativeCreations: WorldNativeCreation[] = creationsRaw.map(parseNativeCreationRow);
        unique(nativeCreations, (c) => c.op, "duplicate-native-creation");
        const switchSettled: WorldReplySwitchEvent[] = switchRaw.map(parseSwitchEventRow);
        unique(switchSettled, (s) => s.op, "duplicate-switch");
        // Diagnostics are independent evidence rows: no dedup.
        const diagnostics: WorldReplyDiagnostic[] = diagRaw.map(parseDiagnosticRow);
        // The parser decodes the correlated result truthfully (null or the
        // exact op shape). Actuator-side null-stripping is broker behavior
        // (strip_result), never a parser default.
        const result = parseResult(r["result"]);
        const gestureRaw = r["gestureSettled"];
        let gestureSettled: WorldReply["gestureSettled"] = null;
        if (gestureRaw !== null && gestureRaw !== undefined) {
            if (typeof gestureRaw !== "object" || Array.isArray(gestureRaw)) {
                throw new Error("gesture-settled");
            }
            const g = gestureRaw as Record<string, unknown>;
            if (!hasExactKeys(g, ["id", "committed"])) {
                throw new Error("gesture-settled");
            }
            if (typeof g["id"] !== "string" || !isCanonicalU64(g["id"] as string)) {
                throw new Error("gesture-settled-id");
            }
            if (typeof g["committed"] !== "boolean") {
                throw new Error("gesture-settled");
            }
            gestureSettled = { id: g["id"] as string, committed: g["committed"] as boolean };
        }
        // Broker-only bound actuator association: null on omitted lanes,
        // otherwise exactly {instance, serial, coreId} with canonical
        // counters. Malformed shapes refuse like any other binding.
        const assocRaw = r["gestureAssoc"] ?? (allowUnbound ? null : null);
        let gestureAssoc: WorldReply["gestureAssoc"] = null;
        if (assocRaw !== null && assocRaw !== undefined) {
            const a = assocRaw as Record<string, unknown>;
            if (
                typeof a["instance"] !== "string" || (a["instance"] as string) === "" ||
                typeof a["serial"] !== "string" || !isCanonicalU64(a["serial"] as string) ||
                typeof a["coreId"] !== "string" || !isCanonicalU64(a["coreId"] as string) ||
                (a["coreId"] as string) === "0"
            ) {
                throw new Error("gesture-assoc");
            }
            gestureAssoc = {
                instance: a["instance"] as string,
                serial: a["serial"] as string,
                coreId: a["coreId"] as string,
            };
        } else if (!allowUnbound && (r["gestureAssoc"] === undefined)) {
            throw new Error("gesture-assoc-missing");
        }
        const warpRaw = r["warpCursor"];
        let warpCursor: WorldReply["warpCursor"] = null;
        if (warpRaw !== null && warpRaw !== undefined) {
            if (!Array.isArray(warpRaw) || warpRaw.length !== 2) {
                throw new Error("warp");
            }
            const pair = warpRaw as ReadonlyArray<unknown>;
            if (
                typeof pair[0] !== "number" || !Number.isInteger(pair[0] as number) ||
                typeof pair[1] !== "number" || !Number.isInteger(pair[1] as number) ||
                (pair[0] as number) < -GEOMETRY_BOUND || (pair[0] as number) > GEOMETRY_BOUND ||
                (pair[1] as number) < -GEOMETRY_BOUND || (pair[1] as number) > GEOMETRY_BOUND
            ) {
                throw new Error("warp");
            }
            warpCursor = [pair[0] as number, pair[1] as number];
        }
        const presentation = parsePresentation(r["presentation"]);
        // Accepted core intent writes: the required v4 lane; malformed
        // refuses the whole reply.
        const intentWrites = parseIntentWrites(r["intentWrites"]);
        if (intentWrites === null) {
            throw new Error("intent-writes");
        }
        return {
            adapter, gen, req, obs, rev,
            ready: r["ready"] as boolean, consumed: r["consumed"] as boolean,
            scopes, workspaces, outputs, windows, transfers, effects, result,
            gestureSettled, gestureAssoc, operationSettled, nativeCreations, warpCursor, intentWrites, presentation,
            switchSettled, diagnostics,
            source, nativeInvalid, clock,
        };
    } catch (error) {
        void error;
        return null;
    }
}

const UNDERLAY_HIDE_REASONS = new Set([
    "disabled", "minimized", "maximized", "fullscreen", "floating",
    "pending", "diverged", "root-leaf", "focus-changed", "no-group",
    "idle", "suspended",
]);

function parsePresentation(value: unknown): WorldPresentation {
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        throw new Error("presentation");
    }
    const o = value as Record<string, unknown>;
    if (
        typeof o["foreground"] !== "string" ||
        (o["foreground"] as string) !== "" && !isOpaqueId(o["foreground"])
    ) {
        throw new Error("presentation-foreground");
    }
    const borderRaw = o["border"];
    if (typeof borderRaw !== "object" || borderRaw === null || Array.isArray(borderRaw)) {
        throw new Error("presentation-border");
    }
    const b = borderRaw as Record<string, unknown>;
    if (!hasExactKeys(b, ["visible", "reason", "subject", "outsets", "width", "radius", "witness"])) {
        throw new Error("presentation-border-keys");
    }
    if (
        typeof b["visible"] !== "boolean" || typeof b["reason"] !== "string" ||
        !BORDER_V4_REASONS.has(b["reason"] as string)
    ) {
        throw new Error("presentation-border-shape");
    }
    // A visible border never carries a hide reason.
    if ((b["visible"] as boolean) && (b["reason"] as string) !== "") {
        throw new Error("presentation-border-visible-reason");
    }
    const subject = b["subject"];
    if (subject !== null && !isOpaqueId(subject)) {
        throw new Error("presentation-border-subject");
    }
    const outsets = b["outsets"];
    if (outsets !== null) {
        if (!Array.isArray(outsets) || outsets.length !== 4 || !(outsets as ReadonlyArray<unknown>).every(isI32)) {
            throw new Error("presentation-border-outsets");
        }
    }
    for (const k of ["width", "radius"] as const) {
        const v = b[k];
        if (v !== null && (!isI32(v) || (v as number) < 0)) {
            throw new Error("presentation-border-size");
        }
    }
    const witnessRaw = b["witness"];
    let witness: WorldPresentationBorder["witness"] = null;
    if (witnessRaw !== null && witnessRaw !== undefined) {
        if (typeof witnessRaw !== "object" || Array.isArray(witnessRaw)) {
            throw new Error("presentation-witness");
        }
        const w = witnessRaw as Record<string, unknown>;
        if (!hasExactKeys(w, ["win", "obs", "frame", "foreground"])) {
            throw new Error("presentation-witness-keys");
        }
        if (
            !isOpaqueId(w["win"]) ||
            typeof w["obs"] !== "string" || !isCanonicalU64(w["obs"] as string) ||
            !isCarriedRect(w["frame"]) || !isOpaqueId(w["foreground"])
        ) {
            throw new Error("presentation-witness");
        }
        witness = {
            win: w["win"] as string, obs: w["obs"] as string,
            frame: w["frame"] as [number, number, number, number],
            foreground: w["foreground"] as string,
        };
    }
    const underlayRaw = o["underlay"];
    if (typeof underlayRaw !== "object" || underlayRaw === null || Array.isArray(underlayRaw)) {
        throw new Error("presentation-underlay");
    }
    const u = underlayRaw as Record<string, unknown>;
    if (u["status"] !== "found" && u["status"] !== "hidden") {
        throw new Error("presentation-underlay-status");
    }
    let underlay: WorldPresentation["underlay"];
    if (u["status"] === "found") {
        if (!hasExactKeys(u, ["status", "reason", "baseRev", "homeOut", "homeWs", "group", "leaf", "win", "union", "outer", "members"])) {
            throw new Error("presentation-underlay-keys");
        }
        if (
            u["reason"] !== "" || typeof u["baseRev"] !== "string" || !isCanonicalU64(u["baseRev"] as string) ||
            !isOpaqueId(u["homeOut"]) || !isOpaqueId(u["homeWs"]) ||
            !isOpaqueId(u["group"]) || !isOpaqueId(u["leaf"]) || !isOpaqueId(u["win"])
        ) {
            throw new Error("presentation-underlay-found");
        }
        const union = u["union"];
        const outer = u["outer"];
        if (!isCarriedRect(union) || !isCarriedRect(outer)) {
            throw new Error("presentation-underlay-rect");
        }
        const membersRaw = u["members"];
        if (!Array.isArray(membersRaw) || membersRaw.length > MAX_REPLY_WINDOWS) {
            throw new Error("presentation-underlay-members");
        }
        const members = (membersRaw as ReadonlyArray<unknown>).map((m) => {
            if (typeof m !== "object" || m === null || Array.isArray(m)) {
                throw new Error("presentation-underlay-member");
            }
            const e = m as Record<string, unknown>;
            if (!hasExactKeys(e, ["leaf", "win", "rect"])) {
                throw new Error("presentation-underlay-member-keys");
            }
            if (!isOpaqueId(e["leaf"]) || !isOpaqueId(e["win"]) || !isCarriedRect(e["rect"])) {
                throw new Error("presentation-underlay-member-shape");
            }
            return {
                leaf: e["leaf"] as string, win: e["win"] as string,
                rect: e["rect"] as [number, number, number, number],
            };
        });
        underlay = {
            status: "found", reason: "",
            baseRev: u["baseRev"] as string,
            homeOut: u["homeOut"] as string, homeWs: u["homeWs"] as string,
            group: u["group"] as string, leaf: u["leaf"] as string, win: u["win"] as string,
            union: union as [number, number, number, number],
            outer: outer as [number, number, number, number],
            members,
        };
    } else {
        if (!hasExactKeys(u, ["status", "reason", "baseRev"])) {
            throw new Error("presentation-underlay-keys");
        }
        if (typeof u["reason"] !== "string" || !UNDERLAY_HIDE_REASONS.has(u["reason"] as string)) {
            throw new Error("presentation-underlay-reason");
        }
        const baseRev = u["baseRev"];
        if (!(baseRev === null || (typeof baseRev === "string" && isCanonicalU64(baseRev)))) {
            throw new Error("presentation-underlay-base");
        }
        underlay = {
            status: "hidden", reason: u["reason"] as WorldPresentationUnderlay extends { status: "hidden"; reason: infer R } ? R : never,
            baseRev: (baseRev as string | null) ?? null,
        };
    }
    const pagerRaw = o["pager"];
    if (!Array.isArray(pagerRaw) || pagerRaw.length > MAX_REPLY_OUTPUTS) {
        throw new Error("presentation-pager");
    }
    const pager = (pagerRaw as ReadonlyArray<unknown>).map((s) => {
        if (typeof s !== "object" || s === null || Array.isArray(s)) {
            throw new Error("presentation-pager-out");
        }
        const e = s as Record<string, unknown>;
        if (!hasExactKeys(e, ["out", "workspaces"]) || !isOpaqueId(e["out"])) {
            throw new Error("presentation-pager-out-id");
        }
        const wsRaw = e["workspaces"];
        if (!Array.isArray(wsRaw) || wsRaw.length > MAX_REPLY_WORKSPACES) {
            throw new Error("presentation-pager-ws");
        }
        const workspaces = (wsRaw as ReadonlyArray<unknown>).map((w) => {
            if (typeof w !== "object" || w === null || Array.isArray(w)) {
                throw new Error("presentation-pager-ws-row");
            }
            const r = w as Record<string, unknown>;
            const wsKeys = Object.keys(r);
            if (
                (wsKeys.length !== 7 && wsKeys.length !== 8) ||
                !("id" in r && "ordinal" in r && "label" in r && "current" in r &&
                    "occupied" in r && "containsFocused" in r && "windows" in r) ||
                (wsKeys.length === 8 && r["trailing"] !== true)
            ) {
                throw new Error("presentation-pager-ws-keys");
            }
            if (
                !isOpaqueId(r["id"]) || typeof r["ordinal"] !== "number" ||
                !Number.isInteger(r["ordinal"] as number) || (r["ordinal"] as number) < 0 ||
                (r["ordinal"] as number) > 4294967295 ||
                typeof r["label"] !== "string" ||
                typeof r["current"] !== "boolean" || typeof r["occupied"] !== "boolean" ||
                typeof r["containsFocused"] !== "boolean"
            ) {
                throw new Error("presentation-pager-ws-shape");
            }
            const winsRaw = r["windows"];
            if (!Array.isArray(winsRaw) || winsRaw.length > MAX_REPLY_WINDOWS) {
                throw new Error("presentation-pager-wins");
            }
            const windows = (winsRaw as ReadonlyArray<unknown>).map((x) => {
                if (typeof x !== "object" || x === null || Array.isArray(x)) {
                    throw new Error("presentation-pager-win");
                }
                const z = x as Record<string, unknown>;
                if (!hasExactKeys(z, ["win", "frame"]) || !isOpaqueId(z["win"]) || !isCarriedRect(z["frame"])) {
                    throw new Error("presentation-pager-win-shape");
                }
                return { win: z["win"] as string, frame: z["frame"] as [number, number, number, number] };
            });
            return {
                id: r["id"] as string, ordinal: r["ordinal"] as number, label: r["label"] as string,
                current: r["current"] as boolean, occupied: r["occupied"] as boolean,
                containsFocused: r["containsFocused"] as boolean,
                trailing: wsKeys.length === 8, windows,
            };
        });
        return { out: e["out"] as string, workspaces };
    });
    if (
        (Object.keys(o).length !== 5 && Object.keys(o).length !== 6) ||
        !["foreground", "rev", "border", "underlay", "pager"].every((k) => k in o) ||
        (Object.keys(o).length === 6 && !("suspendedOutput" in o))
    ) {
        throw new Error("presentation-keys");
    }
    if (typeof o["rev"] !== "string" || !isCanonicalU64(o["rev"] as string)) {
        throw new Error("presentation-rev");
    }
    // suspendedOutput is required-or-absent: an absent key fills null, an
    // explicit null (or any non-id) refuses.
    let suspendedOutput: string | null = null;
    if ("suspendedOutput" in o) {
        const suspendedRaw = o["suspendedOutput"];
        if (typeof suspendedRaw !== "string" || !isOpaqueId(suspendedRaw)) {
            throw new Error("presentation-suspended");
        }
        suspendedOutput = suspendedRaw;
    }
    return {
        foreground: o["foreground"] as string,
        rev: o["rev"] as string,
        suspendedOutput,
        border: {
            visible: b["visible"] as boolean, reason: b["reason"] as WorldPresentationBorder["reason"],
            subject: (b["subject"] as string | null) ?? null,
            outsets: (b["outsets"] as [number, number, number, number] | null) ?? null,
            width: (b["width"] as number | null) ?? null,
            radius: (b["radius"] as number | null) ?? null,
            witness,
        },
        underlay,
        pager,
    };
}

// Presentation-lane reply (ReadWorldPresentation): the exact cached
// projection shape `project_reply` plus the serving generation, the read
// correlation, and the causal coverage, bound by `project_read`: exactly
// {v,outcome,gen,req,rev,obs,coveredInstance,coveredInvalid,scopes,
// workspaces,outputs,windows,presentation,preview}. No core transport
// bindings (adapter/source/nativeInvalid/clock), no readiness/consumed
// flags, no one-shot lanes (result, transfers, effects, gesture/operation
// settlement, warp), and no retained write tokens (frame_op/clear_op keys
// are stripped server-side; any carried token refuses). Window rows share
// the core row shape minus the tokens; every other row shares the core row
// parser exactly. `preview` is the trimmed renderer-only drop preview
// (paint `rect`, mover-home source, insertion-neighbor window, plus the
// producing-gesture identity or nulls when unproven) or null; any other
// shape refuses. `coveredInstance`/`coveredInvalid` are the causal witness
// the cached capture covered (nulls when unproven); non-null values must
// be an opaque instance and a canonical counter.
function parsePreview(value: unknown): WorldPreview | null {
    if (value === null) {
        return null;
    }
    if (typeof value !== "object" || Array.isArray(value)) {
        throw new Error("presentation-preview");
    }
    const o = value as Record<string, unknown>;
    if (
        Object.keys(o).length !== 6 ||
        !("rect" in o && "sourceOut" in o && "sourceWs" in o && "win" in o &&
            "producerInstance" in o && "producerSerial" in o)
    ) {
        throw new Error("presentation-preview-keys");
    }
    if (!isRect(o["rect"])) {
        throw new Error("presentation-preview-rect");
    }
    if (!isOpaqueId(o["sourceOut"]) || !isOpaqueId(o["sourceWs"]) || !isOpaqueId(o["win"])) {
        throw new Error("presentation-preview-id");
    }
    const producerInstance = o["producerInstance"];
    const producerSerial = o["producerSerial"];
    if (producerInstance !== null && !isOpaqueId(producerInstance)) {
        throw new Error("presentation-preview-producer");
    }
    if (
        producerSerial !== null &&
        (typeof producerSerial !== "string" || !isCanonicalU64(producerSerial) || producerSerial === "0")
    ) {
        throw new Error("presentation-preview-producer");
    }
    if ((producerInstance === null) !== (producerSerial === null)) {
        throw new Error("presentation-preview-producer");
    }
    return {
        rect: o["rect"] as [number, number, number, number],
        sourceOut: o["sourceOut"] as string,
        sourceWs: o["sourceWs"] as string,
        win: o["win"] as string,
        producerInstance: producerInstance as string | null,
        producerSerial: producerSerial as string | null,
    };
}
function parsePresentationReply(r: Record<string, unknown>, gen: string): WorldPresentationReply | null {
    if (!hasExactKeys(r, ["v", "outcome", "gen", "req", "rev", "obs", "coveredInstance", "coveredInvalid", "scopes", "workspaces", "outputs", "windows", "presentation", "preview"])) {
        return null;
    }
    if (r["outcome"] !== "presentation" || r["gen"] !== gen) {
        return null;
    }
    if (typeof r["req"] !== "string" || !isCorrelationId(r["req"] as string)) {
        return null;
    }
    const rev = r["rev"];
    const obs = r["obs"];
    if (typeof rev !== "string" || !isCanonicalU64(rev) || typeof obs !== "string" || !isCanonicalU64(obs)) {
        return null;
    }
    const coveredInstance = r["coveredInstance"];
    const coveredInvalid = r["coveredInvalid"];
    if (coveredInstance !== null && !isOpaqueId(coveredInstance)) {
        return null;
    }
    if (coveredInvalid !== null && (typeof coveredInvalid !== "string" || !isCanonicalU64(coveredInvalid))) {
        return null;
    }
    if ((coveredInstance === null) !== (coveredInvalid === null)) {
        return null;
    }
    const scopesRaw = r["scopes"];
    const wsRaw = r["workspaces"];
    const outRaw = r["outputs"];
    const winRaw = r["windows"];
    if (!Array.isArray(scopesRaw) || !Array.isArray(wsRaw) || !Array.isArray(outRaw) || !Array.isArray(winRaw)) {
        return null;
    }
    try {
        const scopes = (scopesRaw as ReadonlyArray<unknown>).map(parseScopeRow);
        const workspaces = (wsRaw as ReadonlyArray<unknown>).map(parseWorkspaceRow);
        const outputs = (outRaw as ReadonlyArray<unknown>).map(parseOutputRow);
        const stripped = (winRaw as ReadonlyArray<unknown>).map(parsePresentationWindowRow);
        const windows: WorldReplyWindow[] = stripped.map((w) => ({ ...w, frame_op: "", clear_op: "" }));
        const presentation = parsePresentation(r["presentation"]);
        const preview = parsePreview(r["preview"]);
        return {
            outcome: "presentation", gen, req: r["req"] as string, rev, obs,
            coveredInstance: coveredInstance as string | null,
            coveredInvalid: coveredInvalid as string | null,
            scopes, workspaces, outputs, windows, presentation, preview,
        };
    } catch (error) {
        void error;
        return null;
    }
}

// ReadWorldPresentation request builder: the read-only presentation query
// carries only the contract version and the caller-minted read correlation
// (`req`). No generation (the serving generation binds server-side and the
// reply fences it), no one-shot, write, or result lanes.
export function buildPresentationReadRequest(req: string): string | null {
    if (!isCorrelationId(req)) {
        return null;
    }
    return JSON.stringify({ v: WORLD_CONTRACT_VERSION, req });
}

const RESULT_OPS = new Set(["drag-preview", "toggle-float", "maximize", "directional", "rejected", "migrate-workspace"]);
const DIRECTIONAL_CAUSES = new Set(["no-target", "unknown-window", "focus-present", "no-source", "no-adjacent-output"]);

// Strict result parse (mirrors tiler-core world_report::CommandResult
// exactly): null (or a missing key) means no correlated outcome; any
// present object must match its op shape exactly, with exact key sets.
// u64 counters stay canonical strings. The parser decodes truthfully:
// actuator-side null-stripping is broker behavior, never a parser default.
function parseResult(value: unknown): WorldReplyResult | null {
    if (value === undefined || value === null) {
        return null;
    }
    if (typeof value !== "object" || Array.isArray(value)) {
        throw new Error("result");
    }
    const o = value as Record<string, unknown>;
    const op = o["op"];
    if (typeof op !== "string" || !RESULT_OPS.has(op)) {
        throw new Error("result-op");
    }
    const id = (v: unknown): string => {
        if (!isOpaqueId(v)) {
            throw new Error("result-id");
        }
        return v as string;
    };
    const rect = (v: unknown): [number, number, number, number] => {
        if (!isCarriedRect(v)) {
            throw new Error("result-rect");
        }
        return v;
    };
    const u64 = (v: unknown): string => {
        if (typeof v !== "string" || !isCanonicalU64(v)) {
            throw new Error("result-rev");
        }
        return v;
    };
    if (op === "drag-preview") {
        if (!hasExactKeys(o, ["op", "rev", "domainOut", "domainWs", "sourceOut", "sourceWs", "rect", "hover", "win"])) {
            throw new Error("result-preview-keys");
        }
        const hoverRaw = o["hover"];
        let hover: Extract<WorldReplyResult, { op: "drag-preview" }>["hover"] = null;
        if (hoverRaw !== null) {
            if (typeof hoverRaw !== "object" || Array.isArray(hoverRaw)) {
                throw new Error("result-hover");
            }
            const h = hoverRaw as Record<string, unknown>;
            if (!hasExactKeys(h, ["domainOut", "domainWs", "leaf", "win", "rev", "prior", "priorOut", "priorWs"])) {
                throw new Error("result-hover-keys");
            }
            const priorRaw = h["prior"];
            let prior: { group: string; edge: "left" | "right" | "top" | "bottom" | "center" } | null = null;
            if (priorRaw !== null) {
                if (typeof priorRaw !== "object" || Array.isArray(priorRaw)) {
                    throw new Error("result-prior");
                }
                const p = priorRaw as Record<string, unknown>;
                if (!hasExactKeys(p, ["group", "edge"])) {
                    throw new Error("result-prior-keys");
                }
                const edge = p["edge"];
                if (typeof edge !== "string" || !DRAG_EDGES.has(edge)) {
                    throw new Error("result-prior-edge");
                }
                prior = { group: id(p["group"]), edge: edge as "left" | "right" | "top" | "bottom" | "center" };
            }
            hover = {
                domainOut: id(h["domainOut"]), domainWs: id(h["domainWs"]),
                leaf: id(h["leaf"]), win: id(h["win"]), rev: u64(h["rev"]),
                prior, priorOut: id(h["priorOut"]), priorWs: id(h["priorWs"]),
            };
        }
        return {
            op: "drag-preview", rev: u64(o["rev"]),
            domainOut: id(o["domainOut"]), domainWs: id(o["domainWs"]),
            sourceOut: id(o["sourceOut"]), sourceWs: id(o["sourceWs"]),
            rect: rect(o["rect"]), hover, win: id(o["win"]),
        };
    }
    if (op === "toggle-float") {
        const keys = Object.keys(o);
        // Explicit-tile no-space retry marker: present only when true;
        // present-but-false refuses.
        if (
            (keys.length !== 3 && keys.length !== 4) ||
            !("op" in o && "win" in o && "rect" in o) ||
            (keys.length === 4 && o["noSpace"] !== true)
        ) {
            throw new Error("result-float-keys");
        }
        const rectRaw = o["rect"];
        if (rectRaw !== null && !isCarriedRect(rectRaw)) {
            throw new Error("result-rect");
        }
        return {
            op: "toggle-float", win: id(o["win"]),
            rect: (rectRaw as [number, number, number, number] | null) ?? null,
            noSpace: keys.length === 4,
        };
    }
    if (op === "maximize") {
        if (!hasExactKeys(o, ["op", "win", "maximized", "clearFullscreen", "wasMaximized"])) {
            throw new Error("result-maximize-keys");
        }
        const maximized = o["maximized"];
        const clearFullscreen = o["clearFullscreen"];
        const wasMaximized = o["wasMaximized"];
        if (typeof maximized !== "boolean" || typeof clearFullscreen !== "boolean" || typeof wasMaximized !== "boolean") {
            throw new Error("result-maximize");
        }
        return {
            op: "maximize", win: id(o["win"]),
            maximized, clearFullscreen, wasMaximized,
        };
    }
    if (op === "directional") {
        if (!hasExactKeys(o, ["op", "outcome", "win", "rect", "cause"])) {
            throw new Error("result-directional-keys");
        }
        if (typeof o["outcome"] !== "string") {
            throw new Error("result-directional-outcome");
        }
        const outcome = o["outcome"];
        if (
            outcome !== "focus-refused-floating" && outcome !== "move-refused-floating" &&
            outcome !== "focus-refused-workspace-floating" && outcome !== "move-refused-workspace-floating" &&
            outcome !== "move-refused-fullscreen" && outcome !== "focus-refused-maximize" &&
            outcome !== "focus-float-retained" && outcome !== "focus-unfocused-retained" &&
            outcome !== "focus-float-refused-stale" && outcome !== "move-float-refused-stale" &&
            outcome !== "move-float-refused-invalid-geometry" && outcome !== "move-float-refused-constraints" &&
            outcome !== "move-float-unknown-output" && outcome !== "focus-float-applied" &&
            outcome !== "move-float-applied" && outcome !== "move-maximize-clear"
        ) {
            throw new Error("result-directional-outcome");
        }
        const winRaw = o["win"];
        if (winRaw !== null && !isOpaqueId(winRaw)) {
            throw new Error("result-directional-win");
        }
        const rectRaw = o["rect"];
        if (rectRaw !== null && !isCarriedRect(rectRaw)) {
            throw new Error("result-rect");
        }
        const causeRaw = o["cause"];
        if (causeRaw !== null && (typeof causeRaw !== "string" || !DIRECTIONAL_CAUSES.has(causeRaw))) {
            throw new Error("result-directional-cause");
        }
        return {
            op: "directional",
            outcome: outcome as Extract<WorldReplyResult, { op: "directional" }>["outcome"],
            win: winRaw as string | null,
            rect: rectRaw as [number, number, number, number] | null,
            cause: causeRaw as Extract<WorldReplyResult, { op: "directional" }>["cause"],
        };
    }
    if (op === "rejected") {
        if (!hasExactKeys(o, ["op", "kind"])) {
            throw new Error("result-rejected-keys");
        }
        if (typeof o["kind"] !== "string" || !REJECT_KINDS.has(o["kind"] as string)) {
            throw new Error("result-kind");
        }
        return { op: "rejected", kind: o["kind"] as WorldRejectKind };
    }
    // migrate-workspace
    if (!hasExactKeys(o, ["op", "direction", "sourceOut", "sourceWs", "targetOut", "targetWs", "members", "floats"])) {
        throw new Error("result-migration-keys");
    }
    if (typeof o["direction"] !== "string" || !DIRECTIONS.has(o["direction"] as string)) {
        throw new Error("result-direction");
    }
    const membersRaw = o["members"];
    if (!Array.isArray(membersRaw) || membersRaw.length > MAX_REPLY_WINDOWS) {
        throw new Error("result-members");
    }
    const members = (membersRaw as ReadonlyArray<unknown>).map((m) => id(m));
    const floats = o["floats"];
    if (typeof floats !== "number" || !Number.isInteger(floats) || floats < 0) {
        throw new Error("result-floats");
    }
    return {
        op: "migrate-workspace", direction: o["direction"] as string,
        sourceOut: id(o["sourceOut"]), sourceWs: id(o["sourceWs"]),
        targetOut: id(o["targetOut"]), targetWs: id(o["targetWs"]),
        members, floats,
    };
}

export function isWorldRejection(
    value: WorldReply | WorldRejection | WorldQueueReply | WorldTakeReply | WorldPresentationReply | WorldWaitReply | null,
): value is WorldRejection {
    return (
        value !== null &&
        (value as WorldRejection).kind !== undefined &&
        (value as { outcome?: unknown }).outcome === "rejected"
    );
}

// Narrow a parsed reply to the core full-state shape (bootstrap or
// capture): present rev with no lane outcome. Rejections and the
// queue/take/presentation lanes never satisfy this.
export function isCoreWorldReply(
    value: WorldReply | WorldRejection | WorldQueueReply | WorldTakeReply | WorldPresentationReply | WorldWaitReply | null,
): value is WorldReply {
    return (
        value !== null &&
        (value as { rev?: unknown }).rev !== undefined &&
        (value as { outcome?: unknown }).outcome === undefined
    );
}

export function isWorldQueueReply(    value: WorldReply | WorldRejection | WorldQueueReply | WorldTakeReply | WorldPresentationReply | WorldWaitReply | null,
): value is WorldQueueReply {
    if (value === null) {
        return false;
    }
    const outcome = (value as { outcome?: unknown }).outcome;
    return outcome === "queued" || (outcome === "duplicate" && (value as { client?: unknown }).client !== undefined);
}

// Narrow a parsed reply to the presentation lane (ReadWorldPresentation):
// snapshot plus presentation plus the renderer-only drop preview. Any
// other lane (including rejections) never satisfies this.
export function isWorldPresentationReply(
    value: WorldReply | WorldRejection | WorldQueueReply | WorldTakeReply | WorldPresentationReply | WorldWaitReply | null,
): value is WorldPresentationReply {
    if (value === null) {
        return false;
    }
    return (value as { outcome?: unknown }).outcome === "presentation";
}

// Narrow a parsed reply to the wait lane (WaitWorldWake): wake, lease, or
// superseded. Any other lane (including rejections) never satisfies this.
export function isWorldWaitReply(
    value: WorldReply | WorldRejection | WorldQueueReply | WorldTakeReply | WorldPresentationReply | WorldWaitReply | null,
): value is WorldWaitReply {
    if (value === null) {
        return false;
    }
    const outcome = (value as { outcome?: unknown }).outcome;
    return outcome === "wake" || outcome === "lease" || outcome === "superseded";
}

// Strict envelope builders: the exact broker wire (world_broker.rs). The
// inner report carries no broker-owned keys; the envelope binds the
// discovery-bound adapter, the serving generation, the fresh per-capture
// source correlation plus its per-lifetime monotonic capture sequence
// (canonical decimal u64, 1-based), and the local native-invalidation
// watermark.
export function buildDiscoveryRequest(adapter: string, req: string): string {
    return `{"v":${WORLD_CONTRACT_VERSION},"adapter":${JSON.stringify(adapter)},"req":${JSON.stringify(req)}}`;
}

export function buildCaptureEnvelope(
    adapter: string,
    gen: string,
    source: string,
    sourceSeq: string,
    nativeInvalid: string,
    innerReportJson: string,
): string {
    return `{"v":${WORLD_CONTRACT_VERSION},"adapter":${JSON.stringify(adapter)},"gen":${JSON.stringify(gen)},` +
        `"source":${JSON.stringify(source)},"sourceSeq":${JSON.stringify(sourceSeq)},"nativeInvalid":${JSON.stringify(nativeInvalid)},"report":${innerReportJson}}`;
}

export function buildQueueRequest(client: string, gen: string, clientCmd: string, command: WorldCommand): string {
    return `{"v":${WORLD_CONTRACT_VERSION},"gen":${JSON.stringify(gen)},"client":${JSON.stringify(client)},"clientCmd":${JSON.stringify(clientCmd)},"command":${JSON.stringify(command)}}`;
}

export function buildTakeRequest(client: string, gen: string, clientCmd: string): string {
    return `{"v":${WORLD_CONTRACT_VERSION},"gen":${JSON.stringify(gen)},"client":${JSON.stringify(client)},"clientCmd":${JSON.stringify(clientCmd)}}`;
}

// Strict wait envelope (world_broker.rs parse_wait_request): the
// discovery-bound producer lifetime, the serving generation, the waiter
// correlation fencing stale completions, and the last delivered wake
// cursor (canonical decimal u64, "0" before the first wake).
export function buildWaitRequest(adapter: string, gen: string, nonce: string, cursor: string): string {
    return `{"v":${WORLD_CONTRACT_VERSION},"gen":${JSON.stringify(gen)},"adapter":${JSON.stringify(adapter)},"nonce":${JSON.stringify(nonce)},"cursor":${JSON.stringify(cursor)}}`;
}

export function buildReadRequest(req: string): string {
    return `{"v":${WORLD_CONTRACT_VERSION},"req":${JSON.stringify(req)}}`;
}

// ---------------------------------------------------------------------------
// World v4 report producer (strict v4, single wire route).
//
// The inner report carries NO broker-owned namespace (adapter, gen, obs,
// reqSeq, req, cmdSeq, command, monotonicMs): the broker stamps those and
// carrying any of them refuses as unknown-field. The inner report carries
// no legacy classification verdicts and no v3 raw lane (screens/windows
// shapes below are the v4 native facts; retired keys such as floating,
// fitExcluded, fixedAuto, observedWs, mode, views, hostOwned, predicate,
// floatIntent, or birthClear refuse or hold server-side and are never
// emitted). Optional lanes the producer does not populate (cursorHit,
// frame/member/view/clear/creation attempts, gestureIngress, primary) are
// omitted, never nulled: omission backfills the observation-only default.
// Capture-only raw lanes (nativeGesture, cursorWindow,
// nativeInvalidWitness) ride for the broker to validate and strip before
// the strict decode; absent keys pass through.
//
// Canonical bytes mirror tiler-core world_sizing/world_report plus
// tiler-protocol world_protocol exactly: table key order, compact JSON
// without insignificant whitespace, `"`/`\` escapes, every C0 control as
// lowercase `\u00xx`, all other characters passed through. In particular
// the producer NEVER relies on JSON.stringify for string escaping: the
// KWin JSON.stringify short escapes (`\b \t \n \f \r`) are LIVE-unproved
// and would differ from the canonical writer, so the exact writer below
// is used for every string. Scale is exactly 1 on the KWin logical
// coordinate space (never a device-pixel ratio); every other producer
// number is an integer, spelled decimally. Any value that cannot be
// spelled canonically fails closed: the builder returns null and the
// entry must not transmit.
// ---------------------------------------------------------------------------

const V4_STRING_HEX = "0123456789abcdef";

// Canonical v4 string writer: byte-identical to the Rust canonical
// writers over the wire alphabet. `"` and `\` use short escapes, every
// C0 control spells lowercase `\u00xx` (including the five controls
// JSON.stringify would spell short: \b \t \n \f \r), all other
// characters pass through verbatim.
export function encodeV4String(value: string): string {
    let out = '"';
    for (let i = 0; i < value.length; i += 1) {
        const c = value.charCodeAt(i);
        if (c === 34) {
            out += '\\"';
        } else if (c === 92) {
            out += "\\\\";
        } else if (c < 32) {
            out += `\\u00${V4_STRING_HEX[(c >> 4) & 15] as string}${V4_STRING_HEX[c & 15] as string}`;
        } else {
            out += value.charAt(i);
        }
    }
    return `${out}"`;
}

// Canonical v4 integer writer: finite safe integers spell decimally
// (shortest round-trip, no exponent, -0 spells "0"). Anything else
// (non-numbers, non-integers, non-finite, unsafe) has no canonical
// spelling and fails closed as null.
function encodeV4Int(value: unknown): string | null {
    if (typeof value !== "number" || !Number.isSafeInteger(value)) {
        return null;
    }
    return value === 0 ? "0" : String(value);
}

function encodeV4Bool(value: boolean): string {
    return value ? "true" : "false";
}

function encodeV4NullableBool(value: boolean | null): string {
    return value === null ? "null" : encodeV4Bool(value);
}

function encodeV4NullableId(value: string | null): string | null {
    if (value === null) {
        return "null";
    }
    return isOpaqueId(value) ? encodeV4String(value) : null;
}

function encodeV4EmptyOrId(value: string): string | null {
    if (value === "") {
        return '""';
    }
    return isOpaqueId(value) ? encodeV4String(value) : null;
}

function encodeV4Rect(rect: { x: number; y: number; w: number; h: number } | null): string | null {
    if (rect === null) {
        return "null";
    }
    const x = encodeV4Int(rect.x);
    const y = encodeV4Int(rect.y);
    const w = encodeV4Int(rect.w);
    const h = encodeV4Int(rect.h);
    return x === null || y === null || w === null || h === null ? null : `[${x},${y},${w},${h}]`;
}

const V4_WINDOW_TYPES = new Set([
    "normal", "dialog", "sheet", "utility", "desktop", "dock",
    "menu", "tooltip", "notification", "splash", "other",
]);

function encodeV4WindowRow(w: RawWindowFact, hostOwned: boolean): string | null {
    if (!isOpaqueId(w.win) || !isOpaqueId(w.out)) {
        return null;
    }
    const rect = encodeV4Rect(w.rectV4);
    const hints = encodeV4Hints(w.hintsV4);
    if (rect === null || hints === null) {
        return null;
    }
    const owner = w.transientFor;
    if (owner !== null && !isOpaqueId(owner)) {
        return null;
    }
    if (!V4_WINDOW_TYPES.has(w.windowType)) {
        return null;
    }
    for (const text of [w.role, w.appId, w.processId, w.class, w.title]) {
        if (text !== null && typeof text !== "string") {
            return null;
        }
    }
    // Native shown state (KWin `Window::isShown`, src/window.cpp at both
    // pins): !deleted && !hidden && !hiddenByShowDesktop && !minimized.
    // Deleted rows never reach this lane; minimized/hidden are
    // script-readable; hiddenByShowDesktop has no script Q_PROPERTY at
    // either pin, so `true` is unprovable and rides null (honest unknown
    // per the core VisibilityFact contract). `false` is proven whenever
    // minimized||hidden. Never an inferred `true`.
    const visible: boolean | null = (w.minimized || w.hidden) ? false : null;
    const esc = encodeV4String;
    const maximized = w.maximizeMode !== 0;
    let row = `{"win":${esc(w.win)},"out":${esc(w.out)},"rect":${rect},` +
        `"fullscreen":${encodeV4Bool(w.fullScreen)},"maximized":${encodeV4Bool(maximized)},` +
        `"sticky":${encodeV4Bool(w.onAllDesktops)},"minimized":${encodeV4Bool(w.minimized)},` +
        `"hints":${hints},`;
    if (hostOwned && w.nativeWsV4 !== null) {
        if (!isOpaqueId(w.nativeWsV4)) {
            return null;
        }
        row += `"nativeWs":${esc(w.nativeWsV4)},`;
    }
    row += `"hostHidden":${encodeV4Bool(w.hidden)},` +
        `"privileged":null,"managed":${encodeV4Bool(w.managed)},` +
        `"acceptsInput":${encodeV4Bool(w.wantsInput)},` +
        `"activationInhibited":null,"toolWindow":null,"owned":null,` +
        `"transient":${encodeV4Bool(w.transient)},"modal":${encodeV4Bool(w.modal)},` +
        `"resizable":${encodeV4NullableBool(w.resizable)},` +
        `"above":${encodeV4NullableBool(w.above)},"below":${encodeV4NullableBool(w.below)},` +
        `"owner":${owner === null ? "null" : esc(owner)},"parent":null,` +
        `"role":${w.role === null ? "null" : esc(w.role)},` +
        `"appId":${w.appId === null ? "null" : esc(w.appId)},` +
        `"executable":null,` +
        `"processId":${w.processId === null ? "null" : esc(w.processId)},` +
        `"class":${w.class === null ? "null" : esc(w.class)},` +
        `"title":${w.title === null ? "null" : esc(w.title)},` +
        `"type":${esc(w.windowType)},` +
        `"activationPolicy":null,"layer":null,` +
        `"fullscreenOwner":null,"fullscreenExitable":null,` +
        `"suppressionAvailable":false,` +
        `"visibility":{"visible":${encodeV4NullableBool(visible)},"suppressionOwned":false},` +
        `"lastWrite":null}`;
    return row;
}

function encodeV4RectTuple(rect: readonly [number, number, number, number] | null): string | null {
    if (rect === null) {
        return "null";
    }
    const x = encodeV4Int(rect[0]);
    const y = encodeV4Int(rect[1]);
    const w = encodeV4Int(rect[2]);
    const h = encodeV4Int(rect[3]);
    return x === null || y === null || w === null || h === null ? null : `[${x},${y},${w},${h}]`;
}
function encodeV4Hints(hints: {
    minW: number | null; minH: number | null; maxW: number | null; maxH: number | null;
} | null): string | null {
    if (hints === null) {
        return "null";
    }
    const bound = (b: number | null): string | null => (b === null ? "null" : encodeV4Int(b));
    const minW = bound(hints.minW);
    const minH = bound(hints.minH);
    const maxW = bound(hints.maxW);
    const maxH = bound(hints.maxH);
    return minW === null || minH === null || maxW === null || maxH === null
        ? null
        : `{"minW":${minW},"minH":${minH},"maxW":${maxW},"maxH":${maxH}}`;
}

// v4 presentation ingress reads the foreground row's native facts; the
// legacy ObservedPresentation carrier supplies only ordering/chord/frame
// context, never classification.

function encodeV4ScreenRow(s: {
    out: string;
    full: { x: number; y: number; w: number; h: number } | null;
    work: { x: number; y: number; w: number; h: number } | null;
    complete: boolean;
}): string | null {
    if (!isOpaqueId(s.out)) {
        return null;
    }
    const full = encodeV4Rect(s.full);
    const work = encodeV4Rect(s.work);
    if (full === null || work === null) {
        return null;
    }
    // KWin reports in logical pixels, so scale is exactly 1 (never a
    // physical/logical device-pixel ratio). Identity has no truthful
    // producer: no EDID metadata or connector proof exists in the script
    // surface, and substituting output names or ordinals for the connector
    // is forbidden, so every arm rides null. Primary is always omitted:
    // the contract primary is the native host-designated output, and the
    // active screen is not that proof.
    return `{"out":${encodeV4String(s.out)},"full":${full},"work":${work},` +
        `"complete":${encodeV4Bool(s.complete)},"scale":1,` +
        `"identity":{"manufacturer":null,"model":null,"serial":null,"connector":null}}`;
}

function encodeV4NativeRow(n: {
    out: string; order: string[]; current: string;
    occupancy: Array<{ native: string; count: number | null }>;
}): string | null {
    if (!isOpaqueId(n.out)) {
        return null;
    }
    const seen = new Set<string>();
    const order: string[] = [];
    for (const id of n.order) {
        if (!isOpaqueId(id) || seen.has(id)) {
            return null;
        }
        seen.add(id);
        order.push(encodeV4String(id));
    }
    const occupancy: string[] = [];
    for (const o of n.occupancy) {
        if (!isOpaqueId(o.native)) {
            return null;
        }
        const count = o.count === null ? "null" : encodeV4Int(o.count);
        if (count === null) {
            return null;
        }
        occupancy.push(`{"native":${encodeV4String(o.native)},"count":${count}}`);
    }
    // Empty current is not the non-ordered surface: it spells null, which
    // preserves a non-ordered surface truthfully instead of holding.
    const current = n.current === "" ? "null" : encodeV4NullableId(n.current);
    if (current === null) {
        return null;
    }
    return `{"out":${encodeV4String(n.out)},"order":[${order.join(",")}],` +
        `"current":${current},"occupancy":[${occupancy.join(",")}]}`;
}

function encodeV4Presentation(
    observation: WorldObservation,
    foregroundRow: RawWindowFact | null,
): string | null {
    const presentation = observation.presentation;
    const esc = encodeV4String;
    const foreground = encodeV4EmptyOrId(presentation.foreground);
    const moveWindow = encodeV4EmptyOrId(presentation.moveWindow);
    if (foreground === null || moveWindow === null) {
        return null;
    }
    // With no observed foreground every foreground-relative fact is
    // unknown (null), matching the empty-report witness; chord/ordering
    // facts ride as observed.
    const minimized = foregroundRow === null ? "null" : encodeV4Bool(foregroundRow.minimized);
    const maximized = foregroundRow === null ? "null" : encodeV4Bool(foregroundRow.maximizeMode !== 0);
    const fullscreen = foregroundRow === null ? "null" : encodeV4Bool(foregroundRow.fullScreen);
    const hostHidden = foregroundRow === null ? "null" : encodeV4Bool(foregroundRow.hidden);
    const hiddenWorkspace = foregroundRow === null
        ? "null"
        : encodeV4Bool(presentation.hiddenWorkspace);
    const frame = foregroundRow === null || foregroundRow.minimized
        ? "null"
        : encodeV4Rect(foregroundRow.rectV4);
    if (frame === null) {
        return null;
    }
    const pagerOrder: string[] = [];
    for (const id of presentation.pagerOrder) {
        if (!isOpaqueId(id)) {
            return null;
        }
        pagerOrder.push(esc(id));
    }
    const pagerFrames: string[] = [];
    for (const f of presentation.pagerFrames) {
        if (!isOpaqueId(f.win)) {
            return null;
        }
        const live = encodeV4Rect(f.live);
        if (live === null) {
            return null;
        }
        pagerFrames.push(`{"win":${esc(f.win)},"live":${live}}`);
    }
    // The adapter can never mint the broker-namespaced observation proof,
    // so the witness always rides null; the broker binds real coverage
    // from the stripped raw lane.
    const foregroundOut = foregroundRow === null
        ? "null"
        : encodeV4NullableId(foregroundRow.out);
    if (foregroundOut === null) {
        return null;
    }
    return `{"foreground":${foreground},"minimized":${minimized},` +
        `"maximized":${maximized},"fullscreen":${fullscreen},` +
        `"hostHidden":${hostHidden},"hiddenWorkspace":${hiddenWorkspace},` +
        `"frame":${frame},"chordHeld":${encodeV4Bool(presentation.chordHeld)},` +
        `"moveWindow":${moveWindow},"pagerOrder":[${pagerOrder.join(",")}],` +
        `"pagerFrames":[${pagerFrames.join(",")}],"witness":null,` +
        `"foregroundOut":${foregroundOut}}`;
}

export interface WorldReportV4IntentRecord {
    readonly win: string;
    readonly data: string;
}

// Minimal producer input for the v4 inner report. Attempt lanes ride the
// retained mechanical evidence the apply loop keeps (latest-wins per
// window/operation/phase); the producer emits only what it measured.
// stateAttempts, focusFeedback, and rendererInput have no producer yet:
// the adapter performs no visibility writes, keeps no focus correlation,
// and never mints renderer targets, so they ride their empty lanes until
// the actuator/record slices land (K1b2).
export interface WorldReportV4Input {
    readonly observation: WorldObservation;
    readonly creationAttempts: ReadonlyArray<WorldCreationAttempt>;
    readonly frameAttempts: ReadonlyArray<WorldFrameAttempt>;
    readonly memberAttempts: ReadonlyArray<WorldMemberAttempt>;
    readonly viewAttempts: ReadonlyArray<WorldViewAttempt>;
    readonly clearAttempts: ReadonlyArray<WorldClearAttempt>;
    readonly intentRecords: ReadonlyArray<WorldReportV4IntentRecord>;
}

export function buildWorldReportV4InnerJson(input: WorldReportV4Input): string | null {
    const { observation } = input;
    const esc = encodeV4String;
    const screens: string[] = [];
    for (const s of observation.screens) {
        const row = encodeV4ScreenRow(s);
        if (row === null) {
            return null;
        }
        screens.push(row);
    }
    const windows: string[] = [];
    const seenWindows = new Set<string>();
    for (const w of observation.windowFacts) {
        const row = encodeV4WindowRow(w, observation.hostOwned);
        if (row === null) {
            return null;
        }
        if (seenWindows.has(w.win)) {
            return null;
        }
        seenWindows.add(w.win);
        windows.push(row);
    }
    const closed: string[] = [];
    for (const id of observation.closed) {
        if (!isOpaqueId(id) || seenWindows.has(id)) {
            return null;
        }
        closed.push(esc(id));
    }
    const native: string[] = [];
    const seenNative = new Set<string>();
    for (const n of observation.native) {
        const row = encodeV4NativeRow(n);
        if (row === null || seenNative.has(n.out)) {
            return null;
        }
        seenNative.add(n.out);
        native.push(row);
    }
    const focused = encodeV4EmptyOrId(observation.focusedGlobal);
    const cursor = observation.cursor === null
        ? "null"
        : (() => {
            const x = encodeV4Int(observation.cursor.x);
            const y = encodeV4Int(observation.cursor.y);
            return x === null || y === null ? null : `[${x},${y}]`;
        })();
    if (focused === null || cursor === null) {
        return null;
    }
    const actionForeground = encodeV4NullableId(observation.actionForeground);
    const actionCached = encodeV4NullableId(observation.actionCached);
    if (actionForeground === null || actionCached === null) {
        return null;
    }
    const actionOutputs: string[] = [];
    for (const out of observation.actionOutputs) {
        if (!isOpaqueId(out)) {
            return null;
        }
        actionOutputs.push(esc(out));
    }
    const foregroundRow = observation.focusedGlobal === ""
        ? null
        : (observation.windowFacts.find((w) => w.win === observation.focusedGlobal) ?? null);
    const presentation = encodeV4Presentation(observation, foregroundRow);
    if (presentation === null) {
        return null;
    }
    const intentRecords: string[] = [];
    for (const record of input.intentRecords) {
        if (!isOpaqueId(record.win) || typeof record.data !== "string") {
            return null;
        }
        intentRecords.push(`{"win":${esc(record.win)},"data":${esc(record.data)}}`);
    }
    // Attempt lanes ride only when populated; omission backfills the
    // observation-only default. The producer omits every lane it did not
    // measure rather than emitting empty reassurance.
    let lanes = "";
    const attemptLane = (
        key: string,
        rows: ReadonlyArray<unknown>,
        encode: (row: never) => string | null,
    ): boolean => {
        if (rows.length === 0) {
            return true;
        }
        const encoded: string[] = [];
        for (const row of rows) {
            const bytes = encode(row as never);
            if (bytes === null) {
                return false;
            }
            encoded.push(bytes);
        }
        lanes += `,"${key}":[${encoded.join(",")}]`;
        return true;
    };
    const attemptOk =
        attemptLane("frameAttempts", input.frameAttempts, encodeV4FrameAttempt) &&
        attemptLane("memberAttempts", input.memberAttempts, encodeV4MemberAttempt) &&
        attemptLane("viewAttempts", input.viewAttempts, encodeV4ViewAttempt) &&
        attemptLane("clearAttempts", input.clearAttempts, encodeV4ClearAttempt) &&
        attemptLane("creationAttempts", input.creationAttempts, encodeV4CreationAttempt);
    if (!attemptOk) {
        return null;
    }
    // Capture-only raw lanes for the broker to validate and strip before
    // the strict decode. The script never mints gesture ingress: the
    // gesture lane always rides null and the broker folds the raw native
    // snapshot plus the same-capture windowAt fact into core ingress.
    const nativeGesture = (observation.nativeGesture ?? null) === null
        ? "null"
        : rawNativeGestureJson(observation.nativeGesture as NativeGestureSnapshot);
    const cursorWindow = (observation.cursorWindow ?? null) === null
        ? "null"
        : encodeV4NullableId(observation.cursorWindow as string);
    if (cursorWindow === null) {
        return null;
    }
    const witness = (observation.nativeInvalidWitness ?? null) === null
        ? "null"
        : `{"instance":${esc((observation.nativeInvalidWitness as { instance: string }).instance)},` +
          `"counter":${esc((observation.nativeInvalidWitness as { counter: string }).counter)}}`;
    return `{"v":${WORLD_CONTRACT_VERSION},` +
        `"screens":[${screens.join(",")}],"windows":[${windows.join(",")}],` +
        `"closed":[${closed.join(",")}],"native":[${native.join(",")}],` +
        `"focused":${focused},"complete":${encodeV4Bool(observation.complete)},` +
        `"gesture":${encodeV4Bool(observation.gesture)},"cursor":${cursor},` +
        `"actionForeground":${actionForeground},"actionCached":${actionCached},` +
        `"actionOutputs":[${actionOutputs.join(",")}],` +
        `"presentation":${presentation}${lanes},` +
        `"intentRecords":[${intentRecords.join(",")}],` +
        `"stateAttempts":[],"focusFeedback":[],"rendererInput":null,` +
        `"nativeGesture":${nativeGesture},"cursorWindow":${cursorWindow},` +
        `"nativeInvalidWitness":${witness}}`;
}

// Retained mechanical attempt lanes ride verbatim in canonical key order.
// These mirror the v3 attempt shapes the apply loop already retains; a
// row that cannot be spelled canonically fails the whole report closed.
function encodeV4FrameAttempt(a: WorldFrameAttempt): string | null {
    if (!isOpaqueId(a.win)) {
        return null;
    }
    // Ordinary writes ride op "" (never an invented operation); explicit
    // float placements ride their retained core operation id. Reuse the
    // shared empty-or-id writer so bad nonempty ids fail closed.
    const op = encodeV4EmptyOrId(a.op);
    if (op === null) {
        return null;
    }
    const target = encodeV4RectTuple(a.target);
    // readback/pre ride null when unreadable; only a bad rect fails.
    const readback = encodeV4RectTuple(a.readback);
    const pre = encodeV4RectTuple(a.pre);
    if (target === null || readback === null || pre === null) {
        return null;
    }
    // A pre-write frame without an operation has no core correlation and
    // holds server-side: fail the row closed rather than invent an op.
    if (a.op === "" && a.pre !== null) {
        return null;
    }
    const dpiBefore = a.dpiBefore === null ? "null" : encodeV4Int(a.dpiBefore);
    const dpiAfter = a.dpiAfter === null ? "null" : encodeV4Int(a.dpiAfter);
    if (dpiBefore === null || dpiAfter === null) {
        return null;
    }
    return `{"win":${encodeV4String(a.win)},"target":${target},` +
        `"setter":${encodeV4String(a.setter)},"readback":${readback},` +
        `"dpiBefore":${dpiBefore},"dpiAfter":${dpiAfter},` +
        `"attemptMs":${encodeV4String(a.attemptMs)},` +
        `"op":${op},"pre":${pre}}`;
}

function encodeV4MemberAttempt(a: WorldMemberAttempt): string | null {
    if (!isOpaqueId(a.op) || !isOpaqueId(a.win) || !isOpaqueId(a.out) || !isOpaqueId(a.ws)) {
        return null;
    }
    const homing = (h: { out: string; ws: string } | null): string | null => {
        if (h === null) {
            return "null";
        }
        return isOpaqueId(h.out) && isOpaqueId(h.ws)
            ? `{"out":${encodeV4String(h.out)},"ws":${encodeV4String(h.ws)}}`
            : null;
    };
    const pre = homing(a.pre);
    const post = homing(a.post);
    const nativePre = a.nativePre === undefined || a.nativePre === null
        ? "null"
        : encodeV4NullableId(a.nativePre);
    const nativePost = a.nativePost === undefined || a.nativePost === null
        ? "null"
        : encodeV4NullableId(a.nativePost);
    if (pre === null || post === null || nativePre === null || nativePost === null) {
        return null;
    }
    return `{"op":${encodeV4String(a.op)},"win":${encodeV4String(a.win)},` +
        `"out":${encodeV4String(a.out)},"ws":${encodeV4String(a.ws)},` +
        `"setter":${encodeV4String(a.setter)},"pre":${pre},"post":${post},` +
        `"nativePre":${nativePre},"nativePost":${nativePost},` +
        `"attemptMs":${encodeV4String(a.attemptMs)}}`;
}

function encodeV4ViewAttempt(a: WorldViewAttempt): string | null {
    if (!isOpaqueId(a.op) || !isOpaqueId(a.out) || !isOpaqueId(a.ws)) {
        return null;
    }
    const view = (v: string | null): string | null =>
        (v === null ? "null" : encodeV4NullableId(v));
    const pre = view(a.pre);
    const post = view(a.post);
    const base = view(a.base);
    const nativePre = a.nativePre === undefined || a.nativePre === null ? "null" : encodeV4NullableId(a.nativePre);
    const nativePost = a.nativePost === undefined || a.nativePost === null ? "null" : encodeV4NullableId(a.nativePost);
    const nativeBase = a.nativeBase === undefined || a.nativeBase === null ? "null" : encodeV4NullableId(a.nativeBase);
    if (pre === null || post === null || base === null ||
        nativePre === null || nativePost === null || nativeBase === null) {
        return null;
    }
    return `{"op":${encodeV4String(a.op)},"out":${encodeV4String(a.out)},` +
        `"ws":${encodeV4String(a.ws)},"setter":${encodeV4String(a.setter)},` +
        `"pre":${pre},"post":${post},"base":${base},` +
        `"nativePre":${nativePre},"nativePost":${nativePost},"nativeBase":${nativeBase},` +
        `"attemptMs":${encodeV4String(a.attemptMs)}}`;
}

function encodeV4ClearAttempt(a: WorldClearAttempt): string | null {
    if (!isOpaqueId(a.op) || !isOpaqueId(a.win)) {
        return null;
    }
    const flag = (v: boolean | null): string => (v === null ? "null" : encodeV4Bool(v));
    return `{"op":${encodeV4String(a.op)},"win":${encodeV4String(a.win)},` +
        `"setter":${encodeV4String(a.setter)},"pre":${flag(a.pre)},"post":${flag(a.post)},` +
        `"attemptMs":${encodeV4String(a.attemptMs)}}`;
}

function encodeV4CreationAttempt(a: WorldCreationAttempt): string | null {
    if (!isOpaqueId(a.op) || !isOpaqueId(a.ws) || !isOpaqueId(a.out)) {
        return null;
    }
    const before: string[] = [];
    for (const id of a.before) {
        if (!isOpaqueId(id)) {
            return null;
        }
        before.push(encodeV4String(id));
    }
    const after: string[] = [];
    for (const id of a.after) {
        if (!isOpaqueId(id)) {
            return null;
        }
        after.push(encodeV4String(id));
    }
    const created = a.created === null ? "null" : encodeV4NullableId(a.created);
    if (created === null) {
        return null;
    }
    return `{"op":${encodeV4String(a.op)},"ws":${encodeV4String(a.ws)},` +
        `"out":${encodeV4String(a.out)},"setter":${encodeV4String(a.setter)},` +
        `"before":[${before.join(",")}],"after":[${after.join(",")}],` +
        `"created":${created},"attemptMs":${encodeV4String(a.attemptMs)}}`;
}
// Legacy v3 inner-report builder: DEAD after the v4 cutover (kept only so
// the K1b2 unit can atomically replace the v3 wire tests/builders and the
// v4 reply decoder/actuator migration). Nothing calls this anymore.
export interface WorldInnerReportInput {
    readonly observation: WorldObservation;
    readonly defaultTiled: boolean;
    // Validated R-WS-12 G-37 migration source-refill token (main.xml
    // `migrationSourceRefill`): the core applies it to subsequent
    // migrations only. Resolved live per report by the entry, never
    // defaulted here.
    readonly refill: MigrationSourceRefill;
    // Validated R-MAX-06 born-maximized token (main.xml
    // `bornMaximized`): the core applies it to subsequent admissions
    // only. Resolved live per report by the entry, never defaulted here.
    readonly bornMaximized: string;
    // Validated D2/D3 capacity tokens (main.xml `anchorFallback`,
    // `noSpaceResult`): the core applies them to subsequent capacity
    // decisions only, never reflowing retained state on Apply.
    readonly anchorFallback: string;
    readonly noSpaceResult: string;
    // Raw native creation evidence retained by the apply loop (at most one
    // entry per outstanding creation op; empty when nothing was executed).
    // Core owns binding and settlement over the fresh native[] readback.
    readonly creationAttempts: ReadonlyArray<WorldCreationAttempt>;
    // Raw mechanical write/readback evidence retained by the apply loop
    // (latest-wins per window, operation window, owning view phase, and
    // clear operation respectively; empty when nothing was dispatched).
    // Core owns every verdict over the fresh facts.
    readonly frameAttempts: ReadonlyArray<WorldFrameAttempt>;
    readonly memberAttempts: ReadonlyArray<WorldMemberAttempt>;
    readonly viewAttempts: ReadonlyArray<WorldViewAttempt>;
    readonly clearAttempts: ReadonlyArray<WorldClearAttempt>;
}

export function buildWorldInnerReportJson(input: WorldInnerReportInput): string {
    const { observation } = input;
    const esc = (s: string): string => JSON.stringify(s);
    const rectOrNull = (r: { x: number; y: number; w: number; h: number } | readonly [number, number, number, number] | null): string => {
        if (r === null) {
            return "null";
        }
        // Tuples ride from retained attempt rows; objects ride from live
        // observations. Array.isArray does not narrow readonly tuples, so
        // both shapes normalize through explicit casts.
        const p: [number, number, number, number] = Array.isArray(r)
            ? (() => {
                const t = r as readonly [number, number, number, number];
                return [t[0], t[1], t[2], t[3]] as [number, number, number, number];
            })()
            : (() => {
                const o = r as { x: number; y: number; w: number; h: number };
                return [o.x, o.y, o.w, o.h] as [number, number, number, number];
            })();
        return `[${p[0]},${p[1]},${p[2]},${p[3]}]`;
    };
    const screens = observation.screens.map((s) =>
        `{"out":${esc(s.out)},"full":${rectOrNull(s.full)},"work":${rectOrNull(s.work)},` +
        `"gap":${s.gap},"outer":${s.outer},"complete":${s.complete ? "true" : "false"},"primary":${s.primary ? "true" : "false"}}`,
    ).join(",");
    // Raw broker-classified window lane: KWin-shaped native facts plus the
    // adapter-retained intent lanes. The broker normalizes these into
    // projected v3 facts via the shared Rust predicates before the strict
    // decode; no projected verdict rides here. Exact keys per row; the
    // broker's exact-keys gate refuses any drift.
    // Fractional native numbers ride verbatim (JSON round-trips f64
    // exactly); a non-finite value would serialize as null and refuse
    // server-side, never silently corrupt the lane.
    const clientNum = (n: number): string => JSON.stringify(n) ?? "null";
    const windows = observation.windowFacts.map((w) => {
        const frame = w.frameGeometry;
        const client = w.clientGeometry === null
            ? "null"
            : `{"x":${clientNum(w.clientGeometry.x)},"y":${clientNum(w.clientGeometry.y)},` +
              `"width":${clientNum(w.clientGeometry.width)},"height":${clientNum(w.clientGeometry.height)}}`;
        const size = (s: { width: number; height: number } | null): string =>
            s === null ? "null" : `{"width":${clientNum(s.width)},"height":${clientNum(s.height)}}`;
        const desktops = w.desktops.map(esc).join(",");
        return `{"win":${esc(w.win)},"out":${esc(w.out)},` +
            `"frameGeometry":{"x":${clientNum(frame.x)},"y":${clientNum(frame.y)},` +
            `"width":${clientNum(frame.width)},"height":${clientNum(frame.height)}},` +
            `"clientGeometry":${client},` +
            `"minSize":${size(w.minSize)},"maxSize":${size(w.maxSize)},` +
            `"desktops":[${desktops}],` +
            `"normalWindow":${w.normalWindow ? "true" : "false"},` +
            `"managed":${w.managed ? "true" : "false"},` +
            `"deleted":${w.deleted ? "true" : "false"},` +
            `"transient":${w.transient ? "true" : "false"},` +
            `"transientFor":${w.transientFor === null ? "null" : esc(w.transientFor)},` +
            `"dialog":${w.dialog ? "true" : "false"},` +
            `"modal":${w.modal ? "true" : "false"},` +
            `"wantsInput":${w.wantsInput ? "true" : "false"},` +
            `"fullScreen":${w.fullScreen ? "true" : "false"},` +
            `"hidden":${w.hidden ? "true" : "false"},` +
            `"maximizeMode":${w.maximizeMode},` +
            `"onAllDesktops":${w.onAllDesktops ? "true" : "false"},` +
            `"minimized":${w.minimized ? "true" : "false"}}`;
    }).join(",");
    const floatIntent = observation.floatIntent.map(esc).join(",");
    const tileIntent = observation.tileIntent.map(esc).join(",");
    const views = observation.views.map((v) =>
        `{"out":${esc(v.out)},"view":${esc(v.view)}}`,
    ).join(",");
    const closed = observation.closed.map(esc).join(",");
    const native = observation.native.map((n) =>
        `{"out":${esc(n.out)},"order":[${n.order.map(esc).join(",")}],"current":${esc(n.current)},` +
        `"occupancy":[${n.occupancy.map((o) => `{"native":${esc(o.native)},"count":${o.count === null ? "null" : String(o.count)}}`).join(",")}]}`,
    ).join(",");
    const cursor = observation.cursor === null ? "null" : `[${observation.cursor.x},${observation.cursor.y}]`;
    const actionOutputs = observation.actionOutputs.map(esc).join(",");
    const presentation = observation.presentation;
    const pagerOrder = presentation.pagerOrder.map(esc).join(",");
    const pagerFrames = presentation.pagerFrames.map((f) =>
        `{"win":${esc(f.win)},"live":${rectOrNull(f.live)},"float":${rectOrNull(f.float)},"tile":${rectOrNull(f.tile)}}`,
    ).join(",");
    const presentationJson =
        `{"foreground":${esc(presentation.foreground)},"minimized":${presentation.minimized ? "true" : "false"},` +
        `"maximized":${presentation.maximized ? "true" : "false"},"fullscreen":${presentation.fullscreen ? "true" : "false"},` +
        `"cloaked":${presentation.cloaked ? "true" : "false"},"hiddenWorkspace":${presentation.hiddenWorkspace ? "true" : "false"},` +
        `"shell":${presentation.shell ? "true" : "false"},"frame":${rectOrNull(presentation.frame)},` +
        `"borderEnabled":${presentation.borderEnabled ? "true" : "false"},"underlayEnabled":${presentation.underlayEnabled ? "true" : "false"},` +
        `"borderGap":${presentation.borderGap},"borderWidth":${presentation.borderWidth},"underlayExtension":${presentation.underlayExtension},` +
        `"chordHeld":${presentation.chordHeld ? "true" : "false"},"moveWindow":${esc(presentation.moveWindow)},` +
        `"paused":${presentation.paused ? "true" : "false"},"pagerOrder":[${pagerOrder}],"pagerFrames":[${pagerFrames}]}`;
    const creationAttempts = input.creationAttempts.map((a) =>
        `{"op":${esc(a.op)},"ws":${esc(a.ws)},"out":${esc(a.out)},` +
        `"setter":${esc(a.setter)},"before":[${a.before.map(esc).join(",")}],` +
        `"after":[${a.after.map(esc).join(",")}],` +
        `"created":${a.created === null ? "null" : esc(a.created)},"attemptMs":${JSON.stringify(a.attemptMs)}}`,
    ).join(",");
    const frameAttempts = input.frameAttempts.map((a) =>
        `{"win":${esc(a.win)},"target":${rectOrNull(a.target)},` +
        `"setter":${esc(a.setter)},"readback":${rectOrNull(a.readback)},` +
        `"dpiBefore":${a.dpiBefore === null ? "null" : a.dpiBefore},` +
        `"dpiAfter":${a.dpiAfter === null ? "null" : a.dpiAfter},` +
        `"attemptMs":${JSON.stringify(a.attemptMs)},` +
        `"op":${esc(a.op)},"pre":${rectOrNull(a.pre)}}`,
    ).join(",");
    const memberAttempts = input.memberAttempts.map((a) => {
        const homing = (h: { out: string; ws: string } | null): string =>
            h === null ? "null" : `{"out":${esc(h.out)},"ws":${esc(h.ws)}}`;
        const native = (v: string | null): string => (v === null ? "null" : esc(v));
        return `{"op":${esc(a.op)},"win":${esc(a.win)},"out":${esc(a.out)},"ws":${esc(a.ws)},` +
            `"setter":${esc(a.setter)},"pre":${homing(a.pre)},"post":${homing(a.post)},` +
            `"nativePre":${native(a.nativePre ?? null)},"nativePost":${native(a.nativePost ?? null)},` +
            `"attemptMs":${JSON.stringify(a.attemptMs)}}`;
    }).join(",");
    const viewAttempts = input.viewAttempts.map((a) => {
        const view = (v: string | null): string => (v === null ? "null" : esc(v));
        const native = (v: string | null): string => (v === null ? "null" : esc(v));
        return `{"op":${esc(a.op)},"out":${esc(a.out)},"ws":${esc(a.ws)},` +
            `"setter":${esc(a.setter)},"pre":${view(a.pre)},"post":${view(a.post)},"base":${view(a.base)},` +
            `"nativePre":${native(a.nativePre ?? null)},"nativePost":${native(a.nativePost ?? null)},"nativeBase":${native(a.nativeBase ?? null)},` +
            `"attemptMs":${JSON.stringify(a.attemptMs)}}`;
    }).join(",");
    const clearAttempts = input.clearAttempts.map((a) => {
        const flag = (v: boolean | null): string => (v === null ? "null" : (v ? "true" : "false"));
        return `{"op":${esc(a.op)},"win":${esc(a.win)},` +
            `"setter":${esc(a.setter)},"pre":${flag(a.pre)},"post":${flag(a.post)},` +
            `"attemptMs":${JSON.stringify(a.attemptMs)}}`;
    }).join(",");
    // The script never mints: gestureIngress always rides null and the
    // broker fills it from the raw native lane. nativeGesture is the
    // verbatim effect snapshot (or null when unknown); cursorWindow is the
    // same-capture script window fact (or null when unknown). The broker
    // strips all three raw keys before the strict decode.
    // nativeInvalidWitness is the Stage 5 causal coverage witness (or null
    // when the effect facts are unknown): the broker validates, strips,
    // and records it as this capture's coverage.
    const nativeGesture = (observation.nativeGesture ?? null) === null
        ? "null"
        : rawNativeGestureJson(
            observation.nativeGesture as NativeGestureSnapshot,
        );
    const cursorWindow = (observation.cursorWindow ?? null) === null
        ? "null"
        : esc(observation.cursorWindow as string);
    const witness = (observation.nativeInvalidWitness ?? null) === null
        ? "null"
        : `{"instance":${esc((observation.nativeInvalidWitness as { instance: string }).instance)},` +
          `"counter":${esc((observation.nativeInvalidWitness as { counter: string }).counter)}}`;
    return `{"v":${WORLD_CONTRACT_VERSION},"mode":${esc(observation.mode)},` +
        `"screens":[${screens}],"windows":[${windows}],"views":[${views}],"closed":[${closed}],` +
        `"hostOwned":${observation.hostOwned ? "true" : "false"},"equalCount":${observation.equalCount ? "true" : "false"},` +
        // KDE explicitly opts into the automatic birth-clear capability:
        // the core may clear a born maximized window once under
        // tile-on-launch (the held-fullscreen exit lane included).
        `"birthClear":true,` +
        `"native":[${native}],` +
        `"predicate":${esc(observation.predicate)},"refill":${esc(input.refill)},` +
        `"bornMaximized":${esc(input.bornMaximized)},` +
        `"defaultTiled":${input.defaultTiled ? "true" : "false"},"focused":${esc(observation.focusedGlobal)},` +
        `"complete":${observation.complete ? "true" : "false"},"gesture":${observation.gesture ? "true" : "false"},` +
        `"switchTarget":${esc(observation.switchTarget)},"freshPlacement":${esc(observation.freshPlacement)},` +
        `"cursor":${cursor},` +
        `"actionForeground":${observation.actionForeground === null ? "null" : esc(observation.actionForeground)},` +
        `"actionCached":${observation.actionCached === null ? "null" : esc(observation.actionCached)},` +
        `"actionOutputs":[${actionOutputs}],` +
        `"focusFollowsCursor":${observation.focusFollowsCursor ? "true" : "false"},` +
        `"cursorWarp":${observation.cursorWarp ? "true" : "false"},` +
        `"presentation":${presentationJson},"creationAttempts":[${creationAttempts}],` +
        `"frameAttempts":[${frameAttempts}],"memberAttempts":[${memberAttempts}],` +
        `"viewAttempts":[${viewAttempts}],"clearAttempts":[${clearAttempts}],` +
        `"floatIntent":[${floatIntent}],"tileIntent":[${tileIntent}],` +
        `"gestureIngress":null,"nativeGesture":${nativeGesture},"cursorWindow":${cursorWindow},` +
        `"nativeInvalidWitness":${witness},` +
        `"anchorFallback":${esc(input.anchorFallback)},"noSpaceResult":${esc(input.noSpaceResult)}}`;
}
