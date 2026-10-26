// World v2 report/reply/envelope JSON shapes (strict v2, no v1 route).
// Keys mirror tiler-protocol/src/world_protocol.rs exactly; the broker
// (crates/omnitiler/src/world_broker.rs) owns sequencing, so the inner
// report below carries NO broker-owned keys (owner, adapter, gen, obs,
// reqSeq, req, cmdSeq, command, monotonicMs): carrying any of them refuses
// as unknown-field server-side. Commands below are the SOLE workspace
// policy route: every selection/send/migration shortcut builds one of
// these, never a synchronous TS desktop decision.

import { isCanonicalU64, isCorrelationId, isGenerationId, isOpaqueId, WORLD_CONTRACT_VERSION } from "./world-seq";
import type { WorldObservation } from "./world-observe";
import type { MigrationSourceRefill } from "./world-settings-live";

export type WorldMode = "per-output-local" | "global-unique" | "shared";

// Typed send source (v2 exact): a genuinely semantic explicit output, or
// one of the two core-owned implicit fallbacks. Legacy string/null `out`
// payloads refuse server-side.
export type SendSource =
    | { explicit: string }
    | { implicit: "home-then-action" | "home-then-cached-live" };

// Exact v2 command vocabulary (tiler-protocol world_protocol.rs
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
    | { op: "migrate-workspace"; from: string; to: string; ws: string }
    | { op: "focus-directional"; win: string; direction: string }
    | { op: "move-directional"; win: string; direction: string; sameAxis: string }
    | { op: "send-directional"; win: string; direction: string; follow: boolean }
    | { op: "migrate-directional"; direction: string }
    | { op: "operation"; command: Record<string, unknown> };

export interface WorldReplyScope {
    scope: string;
    auth: "controlled" | "untouched" | "hold";
    reason: string;
}
export interface WorldReplyWorkspace {
    // `id` is the stable core logical workspace id; `bind` is the
    // host-owned native desktop id bound to it ("" when unbound).
    id: string;
    out: string;
    bind: string;
    scope: string;
    order: number;
    mode: string;
    exist: boolean;
    hold: string;
}
export interface WorldReplyOutput {
    out: string;
    view: string;
    mode: string;
    hold: string;
}
export interface WorldReplyWindow {
    win: string;
    out: string;
    ws: string;
    alloc: [number, number, number, number] | null;
    frame: [number, number, number, number] | null;
    frame_auth: string;
    frame_hold: string;
    member_auth: string;
    member_hold: string;
    // Explicit retained float-placement operation (`frame_op`) or "".
    // The adapter writes float geometry ONLY under a non-empty token,
    // exactly once; ordinary floats carry "" and never take frame writes.
    frame_op: string;
    // Explicit retained clear operation (`clear_op`) or "": directional
    // maximize-clear verdicts spend exactly once like frame_op.
    clear_op: string;
    // Explicit ordinary-float classification: true for every retained float
    // exception row in a controlled scope (user and capacity floats alike).
    // The adapter never infers float from a missing allocation alone: only
    // a floating row with untouched frame authority converges float intent.
    floating: boolean;
    // Diagnostic float origin: "" for ordinary floats and non-floats,
    // "capacity" for capacity floats. Provenance only, never behavior.
    float_origin: string;
}
export interface WorldReplyTransfer {
    op: string;
    kind: "window" | "view" | "migration";
    win: string;
    out: string;
    ws: string;
    follow: boolean;
    held: boolean;
}
// Correlated per-command outcome coexisting with the snapshot (mirrors
// tiler-core world_report::CommandResult exactly; unknown ops refuse).
// Move/resize/tiled-admit/focus/send results ride the snapshot,
// transfers, and effects with `result: null`: the snapshot IS the result.
// The actuator lane (DescribeWorld) ALWAYS carries null: terminal results
// are requester-only via TakeCommandResult.
export type WorldReplyResult =
    | {
        op: "drag-preview"; rev: string;
        domainOut: string; domainWs: string; sourceOut: string; sourceWs: string;
        rect: [number, number, number, number];
        hover: {
            domainOut: string; domainWs: string; leaf: string; win: string;
            rev: string; prior: { group: string; edge: string } | null;
            priorOut: string; priorWs: string;
        } | null;
    }
    | { op: "toggle-float"; win: string; rect: [number, number, number, number] | null; noSpace: boolean }
    | {
        op: "active-group"; rev: string; group: string; leaf: string; win: string;
        bounds: [number, number, number, number];
        members: Array<{ leaf: string; win: string; rect: [number, number, number, number] }>;
    }
    | { op: "no-group"; reason: string; rev: string | null }
    | {
        op: "migrate-workspace"; direction: string;
        sourceOut: string; sourceWs: string; targetOut: string; targetWs: string;
        members: string[]; floats: number;
    };
export interface WorldReplyEffect {
    id: string;
    kind: string;
    target: string;
    rev: string;
}
// Terminal retained-operation verdicts incorporated by this report, at most
// one entry per retired operation (directional maximize-clear verdicts and
// explicit float-placement verdicts). Only `float-frame-proven` means native
// observation confirmed the target; a clear continuation means the intent
// resumed, never that the final frame arrived.
export interface WorldReplyOperationSettlement {
    op: string;
    win: string;
    outcome: string;
    cause: string;
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
    reason: "" | "disabled" | "no-target" | "minimized" | "maximized" | "fullscreen" | "cloaked" | "hidden-workspace" | "shell" | "suspended";
    inner: [number, number, number, number] | null;
    outer: [number, number, number, number] | null;
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
    windows: WorldPresentationPagerWindow[];
}
export interface WorldPresentationPagerOutput {
    out: string;
    workspaces: WorldPresentationPagerWorkspace[];
}
export interface WorldPresentation {
    foreground: string;
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
    operationSettled: WorldReplyOperationSettlement[];
    // Core-owned native creation authorizations for this reply: explicit
    // desires only, at most one entry per outstanding creation op.
    nativeCreations: WorldNativeCreation[];
    // Desired cursor warp point (cross-output focus only) or null. The
    // adapter never warps (unsupported): null or ignored, never chased.
    warpCursor: [number, number] | null;
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
// presentation only, no fx/results/write tokens.
export interface WorldPresentationReply {
    outcome: "presentation";
    gen: string;
    req: string;
    rev: string;
    obs: string;
    scopes: WorldReplyScope[];
    workspaces: WorldReplyWorkspace[];
    outputs: WorldReplyOutput[];
    windows: WorldReplyWindow[];
    presentation: WorldPresentation;
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
// watermark, and zero clock so downstream code treats it uniformly.
export function parseDiscoveryReply(text: string): WorldReply | WorldRejection | null {
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
    // The bootstrap lane carries exactly the 19 core keys (no capture has
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
// refuse; counters must be canonical decimal strings. v2 only: v1 refuses.
export function parseWorldReply(
    text: string,
): WorldReply | WorldRejection | WorldQueueReply | WorldTakeReply | WorldPresentationReply | WorldWaitReply | null {
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
        if (!hasExactKeys(r, ["v", "outcome", "kind", "message", "gen", "source", "nativeInvalid", "clock"])) {
            return null;
        }
        if (
            typeof r["kind"] !== "string" || typeof r["message"] !== "string" ||
            typeof r["gen"] !== "string" || typeof r["source"] !== "string" ||
            (r["source"] !== "" && !isCorrelationId(r["source"])) ||
            typeof r["nativeInvalid"] !== "string" || typeof r["clock"] !== "string" ||
            !isCanonicalU64(r["nativeInvalid"]) || !isCanonicalU64(r["clock"])
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
    const o = e as Record<string, unknown>;
    if (Object.keys(o).length !== 3 || !("scope" in o && "auth" in o && "reason" in o)) {
        throw new Error("scope-keys");
    }
    if (typeof o["scope"] !== "string" || typeof o["auth"] !== "string" || typeof o["reason"] !== "string") {
        throw new Error("scope");
    }
    const auth = o["auth"] as string;
    if (auth !== "controlled" && auth !== "untouched" && auth !== "hold") {
        throw new Error("auth");
    }
    return { scope: o["scope"] as string, auth, reason: o["reason"] as string };
}

function parseWorkspaceRow(e: unknown): WorldReplyWorkspace {
    const o = e as Record<string, unknown>;
    // Exact v2 workspace shape: id/out/bind/scope/order/mode/exist/hold.
    // The retired v1 `logical` field (like any unknown field) refuses.
    if (
        Object.keys(o).length !== 8 ||
        !("id" in o && "out" in o && "bind" in o && "scope" in o &&
            "order" in o && "mode" in o && "exist" in o && "hold" in o)
    ) {
        throw new Error("ws-keys");
    }
    if (
        typeof o["id"] !== "string" || typeof o["out"] !== "string" ||
        typeof o["bind"] !== "string" ||
        typeof o["scope"] !== "string" || typeof o["order"] !== "number" ||
        !Number.isInteger(o["order"]) || typeof o["mode"] !== "string" ||
        typeof o["exist"] !== "boolean" || typeof o["hold"] !== "string"
    ) {
        throw new Error("ws");
    }
    return {
        id: o["id"] as string, out: o["out"] as string, bind: o["bind"] as string,
        scope: o["scope"] as string,
        order: o["order"] as number, mode: o["mode"] as string,
        exist: o["exist"] as boolean, hold: o["hold"] as string,
    };
}

function parseOutputRow(e: unknown): WorldReplyOutput {
    const o = e as Record<string, unknown>;
    if (Object.keys(o).length !== 4 || !("out" in o && "view" in o && "mode" in o && "hold" in o)) {
        throw new Error("out-keys");
    }
    if (typeof o["out"] !== "string" || typeof o["view"] !== "string" || typeof o["mode"] !== "string" || typeof o["hold"] !== "string") {
        throw new Error("out");
    }
    const mode = o["mode"] as string;
    if (mode !== "tiled" && mode !== "floating") {
        throw new Error("out-mode");
    }
    return { out: o["out"] as string, view: o["view"] as string, mode, hold: o["hold"] as string };
}

// Core window row: the actuator lane always carries the retained write
// tokens (possibly empty), never an absent key. Exact keys: any unknown
// (retired or future) field refuses.
function parseWindowRow(e: unknown): WorldReplyWindow {
    const o = e as Record<string, unknown>;
    if (
        Object.keys(o).length !== 13 ||
        !("win" in o && "out" in o && "ws" in o && "alloc" in o && "frame" in o &&
            "frame_auth" in o && "frame_hold" in o && "member_auth" in o &&
            "member_hold" in o && "frame_op" in o && "clear_op" in o &&
            "floating" in o && "float_origin" in o)
    ) {
        throw new Error("win-keys");
    }
    if (
        typeof o["win"] !== "string" || typeof o["out"] !== "string" ||
        typeof o["ws"] !== "string" || typeof o["frame_auth"] !== "string" ||
        typeof o["frame_hold"] !== "string" || typeof o["member_auth"] !== "string" ||
        typeof o["member_hold"] !== "string" || typeof o["frame_op"] !== "string" ||
        typeof o["clear_op"] !== "string" ||
        typeof o["floating"] !== "boolean" || typeof o["float_origin"] !== "string"
    ) {
        throw new Error("win");
    }
    const alloc = o["alloc"];
    const frame = o["frame"];
    if (!(alloc === null || isRect(alloc)) || !(frame === null || isRect(frame))) {
        throw new Error("rect");
    }
    const floatOrigin = o["float_origin"] as string;
    if (floatOrigin !== "" && floatOrigin !== "capacity") {
        throw new Error("float-origin");
    }
    return {
        win: o["win"] as string, out: o["out"] as string, ws: o["ws"] as string,
        alloc: (alloc as [number, number, number, number] | null) ?? null,
        frame: (frame as [number, number, number, number] | null) ?? null,
        frame_auth: o["frame_auth"] as string, frame_hold: o["frame_hold"] as string,
        member_auth: o["member_auth"] as string, member_hold: o["member_hold"] as string,
        frame_op: o["frame_op"] as string, clear_op: o["clear_op"] as string,
        floating: o["floating"] as boolean, float_origin: floatOrigin,
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
    const o = e as Record<string, unknown>;
    if (
        Object.keys(o).length !== 7 ||
        !("op" in o && "kind" in o && "win" in o && "out" in o && "ws" in o && "follow" in o && "held" in o)
    ) {
        throw new Error("transfer-keys");
    }
    const kind = o["kind"] as string;
    if (
        typeof o["op"] !== "string" || (kind !== "window" && kind !== "view" && kind !== "migration") ||
        typeof o["win"] !== "string" || typeof o["out"] !== "string" ||
        typeof o["ws"] !== "string" || typeof o["follow"] !== "boolean" || typeof o["held"] !== "boolean"
    ) {
        throw new Error("tr");
    }
    return {
        op: o["op"] as string, kind, win: o["win"] as string,
        out: o["out"] as string, ws: o["ws"] as string,
        follow: o["follow"] as boolean, held: o["held"] as boolean,
    };
}

function parseEffectRow(e: unknown): WorldReplyEffect {
    const o = e as Record<string, unknown>;
    if (Object.keys(o).length !== 4 || !("id" in o && "kind" in o && "target" in o && "rev" in o)) {
        throw new Error("effect-keys");
    }
    if (
        typeof o["id"] !== "string" || typeof o["kind"] !== "string" ||
        typeof o["target"] !== "string" || typeof o["rev"] !== "string" ||
        !isCanonicalU64(o["rev"] as string)
    ) {
        throw new Error("fx");
    }
    return { id: o["id"] as string, kind: o["kind"] as string, target: o["target"] as string, rev: o["rev"] as string };
}

function parseOperationSettledRow(e: unknown): WorldReplyOperationSettlement {
    const o = e as Record<string, unknown>;
    if (Object.keys(o).length !== 4 || !("op" in o && "win" in o && "outcome" in o && "cause" in o)) {
        throw new Error("op-settled-keys");
    }
    if (
        typeof o["op"] !== "string" || typeof o["win"] !== "string" ||
        typeof o["outcome"] !== "string" || typeof o["cause"] !== "string"
    ) {
        throw new Error("op-settled");
    }
    return {
        op: o["op"] as string, win: o["win"] as string,
        outcome: o["outcome"] as string, cause: o["cause"] as string,
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

// Exact v2 core reply shape: the 20 core keys the Rust canonical encoder
// emits, plus source/nativeInvalid/clock on the bound (capture) lane. Any
// unknown key refuses. The bootstrap lane (no capture flown yet) carries
// exactly the 20 core keys; the bindings default without validation.
const CORE_REPLY_KEYS = ["v", "adapter", "gen", "req", "obs", "rev", "ready", "consumed",
    "scopes", "workspaces", "outputs", "windows", "transfers", "effects", "result",
    "gestureSettled", "operationSettled", "nativeCreations", "warpCursor", "presentation"] as const;
const BOUND_REPLY_KEYS = [...CORE_REPLY_KEYS, "source", "nativeInvalid", "clock"] as const;

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
    if (!scopesRaw || !wsRaw || !outRaw || !winRaw || !trRaw || !fxRaw || !opSettledRaw || !creationsRaw) {
        return null;
    }
    try {
        const scopes: WorldReplyScope[] = scopesRaw.map(parseScopeRow);
        const workspaces: WorldReplyWorkspace[] = wsRaw.map(parseWorkspaceRow);
        const outputs: WorldReplyOutput[] = outRaw.map(parseOutputRow);
        const windows: WorldReplyWindow[] = winRaw.map(parseWindowRow);
        const transfers: WorldReplyTransfer[] = trRaw.map(parseTransferRow);
        const effects: WorldReplyEffect[] = fxRaw.map(parseEffectRow);
        const operationSettled: WorldReplyOperationSettlement[] = opSettledRaw.map(parseOperationSettledRow);
        const nativeCreations: WorldNativeCreation[] = creationsRaw.map(parseNativeCreationRow);
        // The actuator lane never carries a terminal result (requester-only
        // via TakeCommandResult): a non-null result here is malformed.
        if (r["result"] !== null && r["result"] !== undefined) {
            throw new Error("actuator-result");
        }
        const gestureRaw = r["gestureSettled"];
        let gestureSettled: WorldReply["gestureSettled"] = null;
        if (gestureRaw !== null && gestureRaw !== undefined) {
            const g = gestureRaw as Record<string, unknown>;
            if (typeof g["id"] !== "string" || typeof g["committed"] !== "boolean") {
                throw new Error("gesture-settled");
            }
            gestureSettled = { id: g["id"] as string, committed: g["committed"] as boolean };
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
                typeof pair[1] !== "number" || !Number.isInteger(pair[1] as number)
            ) {
                throw new Error("warp");
            }
            warpCursor = [pair[0] as number, pair[1] as number];
        }
        const presentation = parsePresentation(r["presentation"]);
        return {
            adapter, gen, req, obs, rev,
            ready: r["ready"] as boolean, consumed: r["consumed"] as boolean,
            scopes, workspaces, outputs, windows, transfers, effects, result: null,
            gestureSettled, operationSettled, nativeCreations, warpCursor, presentation,
            source, nativeInvalid, clock,
        };
    } catch (error) {
        void error;
        return null;
    }
}

const BORDER_REASONS = new Set([
    "", "disabled", "no-target", "minimized", "maximized", "fullscreen",
    "cloaked", "hidden-workspace", "shell", "suspended",
]);
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
    if (typeof b["visible"] !== "boolean" || typeof b["reason"] !== "string" || !BORDER_REASONS.has(b["reason"] as string)) {
        throw new Error("presentation-border-shape");
    }
    for (const k of ["inner", "outer"] as const) {
        const v = b[k];
        if (!(v === null || isRect(v))) {
            throw new Error("presentation-border-rect");
        }
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
        if (
            u["reason"] !== "" || typeof u["baseRev"] !== "string" || !isCanonicalU64(u["baseRev"] as string) ||
            !isOpaqueId(u["homeOut"]) || !isOpaqueId(u["homeWs"]) ||
            !isOpaqueId(u["group"]) || !isOpaqueId(u["leaf"]) || !isOpaqueId(u["win"])
        ) {
            throw new Error("presentation-underlay-found");
        }
        const union = u["union"];
        const outer = u["outer"];
        if (!isRect(union) || !isRect(outer)) {
            throw new Error("presentation-underlay-rect");
        }
        const membersRaw = u["members"];
        if (!Array.isArray(membersRaw)) {
            throw new Error("presentation-underlay-members");
        }
        const members = (membersRaw as ReadonlyArray<unknown>).map((m) => {
            if (typeof m !== "object" || m === null || Array.isArray(m)) {
                throw new Error("presentation-underlay-member");
            }
            const e = m as Record<string, unknown>;
            if (!isOpaqueId(e["leaf"]) || !isOpaqueId(e["win"]) || !isRect(e["rect"])) {
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
    if (!Array.isArray(pagerRaw)) {
        throw new Error("presentation-pager");
    }
    const pager = (pagerRaw as ReadonlyArray<unknown>).map((s) => {
        if (typeof s !== "object" || s === null || Array.isArray(s)) {
            throw new Error("presentation-pager-out");
        }
        const e = s as Record<string, unknown>;
        if (!isOpaqueId(e["out"])) {
            throw new Error("presentation-pager-out-id");
        }
        const wsRaw = e["workspaces"];
        if (!Array.isArray(wsRaw)) {
            throw new Error("presentation-pager-ws");
        }
        const workspaces = (wsRaw as ReadonlyArray<unknown>).map((w) => {
            if (typeof w !== "object" || w === null || Array.isArray(w)) {
                throw new Error("presentation-pager-ws-row");
            }
            const r = w as Record<string, unknown>;
            if (
                !isOpaqueId(r["id"]) || typeof r["ordinal"] !== "number" ||
                !Number.isInteger(r["ordinal"] as number) || (r["ordinal"] as number) < 0 ||
                typeof r["label"] !== "string" ||
                typeof r["current"] !== "boolean" || typeof r["occupied"] !== "boolean" ||
                typeof r["containsFocused"] !== "boolean"
            ) {
                throw new Error("presentation-pager-ws-shape");
            }
            const winsRaw = r["windows"];
            if (!Array.isArray(winsRaw)) {
                throw new Error("presentation-pager-wins");
            }
            const windows = (winsRaw as ReadonlyArray<unknown>).map((x) => {
                if (typeof x !== "object" || x === null || Array.isArray(x)) {
                    throw new Error("presentation-pager-win");
                }
                const z = x as Record<string, unknown>;
                if (!isOpaqueId(z["win"]) || !isRect(z["frame"])) {
                    throw new Error("presentation-pager-win-shape");
                }
                return { win: z["win"] as string, frame: z["frame"] as [number, number, number, number] };
            });
            return {
                id: r["id"] as string, ordinal: r["ordinal"] as number, label: r["label"] as string,
                current: r["current"] as boolean, occupied: r["occupied"] as boolean,
                containsFocused: r["containsFocused"] as boolean, windows,
            };
        });
        return { out: e["out"] as string, workspaces };
    });
    if (
        Object.keys(o).length !== 4 || Object.keys(b).length !== 4 ||
        !["foreground", "border", "underlay", "pager"].every((k) => k in o)
    ) {
        throw new Error("presentation-keys");
    }
    return {
        foreground: o["foreground"] as string,
        border: {
            visible: b["visible"] as boolean, reason: b["reason"] as WorldPresentationBorder["reason"],
            inner: (b["inner"] as [number, number, number, number] | null) ?? null,
            outer: (b["outer"] as [number, number, number, number] | null) ?? null,
        },
        underlay,
        pager,
    };
}

// Presentation-lane reply (ReadWorldPresentation): the exact cached
// projection shape `project_reply` plus the serving generation and the read
// correlation bound by `project_read`: exactly
// {v,outcome,gen,req,rev,obs,scopes,workspaces,outputs,windows,
// presentation}. No core transport bindings (adapter/source/nativeInvalid/
// clock), no readiness/consumed flags, no one-shot lanes (result,
// transfers, effects, gesture/operation settlement, warp), and no retained
// write tokens (frame_op/clear_op keys are stripped server-side; any
// carried token refuses). Window rows share the core row shape minus the
// tokens; every other row shares the core row parser exactly.
function parsePresentationReply(r: Record<string, unknown>, gen: string): WorldPresentationReply | null {
    if (!hasExactKeys(r, ["v", "outcome", "gen", "req", "rev", "obs", "scopes", "workspaces", "outputs", "windows", "presentation"])) {
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
        return {
            outcome: "presentation", gen, req: r["req"] as string, rev, obs,
            scopes, workspaces, outputs, windows, presentation,
        };
    } catch (error) {
        void error;
        return null;
    }
}

const RESULT_OPS = new Set(["drag-preview", "toggle-float", "active-group", "no-group", "migrate-workspace"]);
const NO_GROUP_REASONS = new Set([
    "no-session", "diverged", "pending", "domain-mismatch", "stale-revision",
    "focus-mismatch", "focus-unmapped", "no-tree", "no-parent-group",
]);

// Strict result parse: null (or a missing key) means no correlated outcome;
// any present object must match its op shape exactly. u64 counters stay
// canonical strings. Used for the take lane only: the actuator capture lane
// always carries null.
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
        if (!isRect(v)) {
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
        if (
            typeof o["rev"] !== "string" || typeof o["domainOut"] !== "string" ||
            typeof o["domainWs"] !== "string" || typeof o["sourceOut"] !== "string" ||
            typeof o["sourceWs"] !== "string"
        ) {
            throw new Error("result-preview");
        }
        const hoverRaw = o["hover"];
        let hover: Extract<WorldReplyResult, { op: "drag-preview" }>["hover"] = null;
        if (hoverRaw !== null && hoverRaw !== undefined) {
            if (typeof hoverRaw !== "object" || Array.isArray(hoverRaw)) {
                throw new Error("result-hover");
            }
            const h = hoverRaw as Record<string, unknown>;
            const priorRaw = h["prior"];
            let prior: { group: string; edge: string } | null = null;
            if (priorRaw !== null && priorRaw !== undefined) {
                if (typeof priorRaw !== "object" || Array.isArray(priorRaw)) {
                    throw new Error("result-prior");
                }
                const p = priorRaw as Record<string, unknown>;
                if (typeof p["group"] !== "string" || typeof p["edge"] !== "string") {
                    throw new Error("result-prior");
                }
                const edge = p["edge"] as string;
                if (edge !== "left" && edge !== "right" && edge !== "top" && edge !== "bottom") {
                    throw new Error("result-prior-edge");
                }
                prior = { group: id(p["group"]), edge };
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
            rect: rect(o["rect"]), hover,
        };
    }
    if (op === "toggle-float") {
        const rectRaw = o["rect"];
        // Explicit-tile no-space retry marker: present only when
        // true; absent (older core) reads as a plain placement.
        const noSpaceRaw = o["noSpace"];
        if (noSpaceRaw !== undefined && typeof noSpaceRaw !== "boolean") {
            throw new Error("result-nospace");
        }
        return {
            op: "toggle-float", win: id(o["win"]),
            rect: rectRaw === null || rectRaw === undefined ? null : rect(rectRaw),
            noSpace: noSpaceRaw === true,
        };
    }
    if (op === "active-group") {
        const membersRaw = o["members"];
        if (!Array.isArray(membersRaw)) {
            throw new Error("result-members");
        }
        const members = (membersRaw as ReadonlyArray<unknown>).map((m) => {
            if (typeof m !== "object" || m === null || Array.isArray(m)) {
                throw new Error("result-member");
            }
            const e = m as Record<string, unknown>;
            return { leaf: id(e["leaf"]), win: id(e["win"]), rect: rect(e["rect"]) };
        });
        return {
            op: "active-group", rev: u64(o["rev"]), group: id(o["group"]), leaf: id(o["leaf"]), win: id(o["win"]),
            bounds: rect(o["bounds"]), members,
        };
    }
    if (op === "no-group") {
        if (typeof o["reason"] !== "string" || !NO_GROUP_REASONS.has(o["reason"] as string)) {
            throw new Error("result-reason");
        }
        const revRaw = o["rev"];
        return {
            op: "no-group", reason: o["reason"] as string,
            rev: revRaw === null || revRaw === undefined ? null : u64(revRaw),
        };
    }
    // migrate-workspace
    if (typeof o["direction"] !== "string" || !DIRECTIONS.has(o["direction"] as string)) {
        throw new Error("result-direction");
    }
    const membersRaw = o["members"];
    if (!Array.isArray(membersRaw)) {
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

// Compact canonical inner-report encoder: the FULL projected v2 observation
// WITHOUT the broker-owned namespace (service stamps owner/adapter/gen,
// obs/reqSeq/req/cmdSeq/command/monotonicMs). Validated wire ids/tokens use
// JSON.stringify so every value stays exact. Minimal injected context: the
// caller owns lifecycle state and the saved defaults; this only formats one
// observation. Old verdict `feedback` transport is deleted: mechanical
// creation evidence rides the tolerant `creationAttempts` lane (absent
// backfills []), never converted from verdicts.
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
    // projected v2 facts via the shared Rust predicates before the strict
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
        `{"out":${esc(n.out)},"order":[${n.order.map(esc).join(",")}],"current":${esc(n.current)}}`,
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
        `"anchorFallback":${esc(input.anchorFallback)},"noSpaceResult":${esc(input.noSpaceResult)}}`;
}
