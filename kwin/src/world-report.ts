// World report/reply JSON shapes (D1-D5). Keys mirror
// tiler-protocol/src/world_protocol.rs exactly; unknown fields refuse.
// Command constructors below are the SOLE workspace policy route (D2): every
// selection/send/migration shortcut builds one of these, never a synchronous
// TS desktop decision.

import { isCanonicalU64, isCorrelationId, isGenerationId, isOpaqueId, WORLD_CONTRACT_VERSION } from "./world-seq";
import type { WorldObservation } from "./world-observe";
import type { MigrationSourceRefill } from "./migration-source-refill";

export type WorldMode = "per-output-local" | "global-unique" | "shared";
export type WorldCommand =
    | { op: "focus"; win: string; direction: string }
    | { op: "send"; win: string; out: string; ws: string; follow: boolean }
    | { op: "switch-view"; out: string; ws: string }
    | { op: "select-workspace"; out: string; ws: string }
    | { op: "select-numbered"; out: string; index: number }
    | { op: "select-relative"; out: string; delta: -1 | 1 }
    | { op: "select-previous"; out: string }
    | { op: "select-trailing"; out: string }
    | { op: "send-relative"; win: string; out: string; delta: -1 | 1; follow: boolean }
    | { op: "send-numbered"; win: string; out: string; index: number; follow: boolean }
    | { op: "set-workspace-tiling"; out: string; ws: string; tiled: boolean }
    | { op: "migrate-workspace"; from: string; to: string; ws: string }
    | { op: "operation"; command: Record<string, unknown> };

export interface WorldReplyScope {
    scope: string;
    auth: "controlled" | "untouched" | "hold";
    reason: string;
}
export interface WorldReplyWorkspace {
    id: string;
    out: string;
    bind: string;
    logical: string;
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
    // Explicit retained float-placement operation (`op-<cmdSeq>`) or "".
    // The adapter writes float geometry ONLY under a non-empty token,
    // exactly once; ordinary floats carry "" and never take frame writes.
    frame_op: string;
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
    | { op: "toggle-float"; win: string; rect: [number, number, number, number] | null }
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
}
export interface WorldRejection {
    kind: string;
    message: string;
    gen: string;
}

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
// index 1..9 (0 is the trailing path); relative delta -1/+1.
export function cmdSelectWorkspace(out: string, ws: string): WorldCommand {
    return { op: "select-workspace", out, ws };
}
export function cmdSelectNumbered(out: string, index: number): WorldCommand | null {
    if (!Number.isInteger(index) || index < 1 || index > 9) {
        return null;
    }
    return { op: "select-numbered", out, index };
}
export function cmdSelectRelative(out: string, delta: number): WorldCommand | null {
    if (delta !== -1 && delta !== 1) {
        return null;
    }
    return { op: "select-relative", out, delta };
}
export function cmdSelectPrevious(out: string): WorldCommand {
    return { op: "select-previous", out };
}
export function cmdSelectTrailing(out: string): WorldCommand {
    return { op: "select-trailing", out };
}
export function cmdSend(win: string, out: string, ws: string, follow: boolean): WorldCommand {
    return { op: "send", win, out, ws, follow };
}
export function cmdSendRelative(win: string, out: string, delta: number, follow: boolean): WorldCommand | null {
    if (delta !== -1 && delta !== 1) {
        return null;
    }
    return { op: "send-relative", win, out, delta, follow };
}
// Numbered send over the core-resolved positional ring (D2/D3): index 0 is
// the trailing path (reuse or stable pending token under core), 1..9 are
// ring positions. Resolution is all core; the adapter never maps positions
// locally. Sends never view-switch, including stays.
export function cmdSendNumbered(win: string, out: string, index: number, follow: boolean): WorldCommand | null {
    if (!Number.isInteger(index) || index < 0 || index > 9) {
        return null;
    }
    return { op: "send-numbered", win, out, index, follow };
}
// Planner-owned per-workspace tiling toggle (legacy Meta+Y parity): the
// adapter never decides tiling mode locally.
export function cmdSetWorkspaceTiling(out: string, ws: string, tiled: boolean): WorldCommand {
    return { op: "set-workspace-tiling", out, ws, tiled };
}
export function cmdMigrateWorkspace(from: string, to: string, ws: string): WorldCommand | null {
    if (from === to) {
        return null;
    }
    return { op: "migrate-workspace", from, to, ws };
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

// Strict reply parse: ready full-state shape or bounded rejection. Unknown
// fields/tokens refuse; counters must be canonical decimal strings.
export function parseWorldReply(text: string): WorldReply | WorldRejection | null {
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
    if (r["outcome"] === "rejected") {
        if (typeof r["kind"] !== "string" || typeof r["message"] !== "string" || typeof r["gen"] !== "string") {
            return null;
        }
        return { kind: r["kind"] as string, message: r["message"] as string, gen: r["gen"] as string };
    }
    if (r["v"] !== 1) {
        return null;
    }
    const str = (k: string): string | null => (typeof r[k] === "string" ? (r[k] as string) : null);
    const adapter = str("adapter");
    const gen = str("gen");
    const req = str("req");
    const obs = str("obs");
    const rev = str("rev");
    if (
        adapter === null || !isOpaqueId(adapter) ||
        gen === null || !isGenerationId(gen) ||
        req === null || !isCorrelationId(req) ||
        obs === null || !isCanonicalU64(obs) ||
        rev === null || !isCanonicalU64(rev) ||
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
    if (!scopesRaw || !wsRaw || !outRaw || !winRaw || !trRaw || !fxRaw) {
        return null;
    }
    try {
        const scopes: WorldReplyScope[] = scopesRaw.map((e) => {
            const o = e as Record<string, unknown>;
            if (typeof o["scope"] !== "string" || typeof o["auth"] !== "string" || typeof o["reason"] !== "string") {
                throw new Error("scope");
            }
            const auth = o["auth"] as string;
            if (auth !== "controlled" && auth !== "untouched" && auth !== "hold") {
                throw new Error("auth");
            }
            return { scope: o["scope"] as string, auth, reason: o["reason"] as string };
        });
        const workspaces: WorldReplyWorkspace[] = wsRaw.map((e) => {
            const o = e as Record<string, unknown>;
            if (
                typeof o["id"] !== "string" || typeof o["out"] !== "string" ||
                typeof o["bind"] !== "string" || typeof o["logical"] !== "string" ||
                typeof o["scope"] !== "string" || typeof o["order"] !== "number" ||
                !Number.isInteger(o["order"]) || typeof o["mode"] !== "string" ||
                typeof o["exist"] !== "boolean" || typeof o["hold"] !== "string"
            ) {
                throw new Error("ws");
            }
            return {
                id: o["id"] as string, out: o["out"] as string, bind: o["bind"] as string,
                logical: o["logical"] as string, scope: o["scope"] as string,
                order: o["order"] as number, mode: o["mode"] as string,
                exist: o["exist"] as boolean, hold: o["hold"] as string,
            };
        });
        const outputs: WorldReplyOutput[] = outRaw.map((e) => {
            const o = e as Record<string, unknown>;
            if (typeof o["out"] !== "string" || typeof o["view"] !== "string" || typeof o["mode"] !== "string" || typeof o["hold"] !== "string") {
                throw new Error("out");
            }
            const mode = o["mode"] as string;
            if (mode !== "tiled" && mode !== "floating") {
                throw new Error("out-mode");
            }
            return { out: o["out"] as string, view: o["view"] as string, mode, hold: o["hold"] as string };
        });
        const windows: WorldReplyWindow[] = winRaw.map((e) => {
            const o = e as Record<string, unknown>;
            if (
                typeof o["win"] !== "string" || typeof o["out"] !== "string" ||
                typeof o["ws"] !== "string" || typeof o["frame_auth"] !== "string" ||
                typeof o["frame_hold"] !== "string" || typeof o["member_auth"] !== "string" ||
                typeof o["member_hold"] !== "string" || typeof o["frame_op"] !== "string"
            ) {
                throw new Error("win");
            }
            const alloc = o["alloc"];
            const frame = o["frame"];
            if (!(alloc === null || isRect(alloc)) || !(frame === null || isRect(frame))) {
                throw new Error("rect");
            }
            return {
                win: o["win"] as string, out: o["out"] as string, ws: o["ws"] as string,
                alloc: (alloc as [number, number, number, number] | null) ?? null,
                frame: (frame as [number, number, number, number] | null) ?? null,
                frame_auth: o["frame_auth"] as string, frame_hold: o["frame_hold"] as string,
                member_auth: o["member_auth"] as string, member_hold: o["member_hold"] as string,
                frame_op: o["frame_op"] as string,
            };
        });
        const transfers: WorldReplyTransfer[] = trRaw.map((e) => {
            const o = e as Record<string, unknown>;
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
        });
        const effects: WorldReplyEffect[] = fxRaw.map((e) => {
            const o = e as Record<string, unknown>;
            if (
                typeof o["id"] !== "string" || typeof o["kind"] !== "string" ||
                typeof o["target"] !== "string" || typeof o["rev"] !== "string" ||
                !isCanonicalU64(o["rev"] as string)
            ) {
                throw new Error("fx");
            }
            return { id: o["id"] as string, kind: o["kind"] as string, target: o["target"] as string, rev: o["rev"] as string };
        });
        const result = parseResult(r["result"]);
        return {
            adapter, gen, req, obs, rev,
            ready: r["ready"] as boolean, consumed: r["consumed"] as boolean,
            scopes, workspaces, outputs, windows, transfers, effects, result,
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

// Strict result parse: null (or a missing key on pre-contract replies)
// means no correlated outcome; any present object must match its op shape
// exactly or the whole reply refuses. u64 counters stay canonical strings.
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
        return {
            op: "toggle-float", win: id(o["win"]),
            rect: rectRaw === null || rectRaw === undefined ? null : rect(rectRaw),
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

export function isWorldRejection(value: WorldReply | WorldRejection | null): value is WorldRejection {
    return (
        value !== null &&
        (value as WorldRejection).kind !== undefined &&
        (value as WorldReply).rev === undefined
    );
}

// Compact canonical DescribeWorld report encoder (moved verbatim from the
// production entry): validated wire ids/tokens use JSON.stringify so every
// value stays exact instead of a backslash/quote-only custom escaper.
// Minimal injected context: the caller owns lifecycle state and the saved
// default; this only formats one report.
export interface WorldReportJsonInput {
    readonly observation: WorldObservation;
    readonly reqSeq: string;
    readonly cmdSeq: string | null;
    readonly reqId: string;
    readonly command: WorldCommand | null;
    readonly owner: string;
    readonly adapter: string;
    readonly boundGen: string | null;
    readonly currentObs: string;
    readonly pendingFeedback: ReadonlyArray<{ op: string; kind: string; outcome: string; hold: string }>;
    readonly defaultTiled: boolean;
    // Validated R-WS-12 G-37 migration source-refill token (main.xml
    // `migrationSourceRefill`): the core applies it to subsequent
    // migrations only. Resolved live per report by the entry, never
    // defaulted here.
    readonly refill: MigrationSourceRefill;
}

export function buildWorldReportJson(input: WorldReportJsonInput): string | null {
    if (input.boundGen === null) {
        return null;
    }
    const { observation, reqSeq, cmdSeq, reqId, command } = input;
    const esc = (s: string): string => JSON.stringify(s);
    const topology = observation.topology.map((t) => {
        const rect = t.rect === null || t.rect === undefined
            ? "null"
            : `[${t.rect.x},${t.rect.y},${t.rect.w},${t.rect.h}]`;
        return `{"out":${esc(t.out)},"order":[${t.order.map(esc).join(",")}],"current":${esc(t.current)},"complete":${t.complete ? "true" : "false"},"rect":${rect},"primary":${t.primary ? "true" : "false"}}`;
    }).join(",");
    const bindings = observation.bindings.map((b) =>
        `{"logical":${esc(b.logical)},"native":${esc(b.native)},"out":${esc(b.out)},"pin":${b.pin ? "true" : "false"},"disp":${b.disp ? "true" : "false"},"managed":${b.managed ? "true" : "false"}}`,
    ).join(",");
    const domains = observation.domains.map((d) => {
        const wins = d.windows.map((w) =>
            `{"win":${esc(w.win)},"out":${esc(w.out)},"ws":${esc(w.ws)},"rect":[${w.rect.x},${w.rect.y},${w.rect.w},${w.rect.h}],"floating":${w.floating ? "true" : "false"},"fitExcluded":${w.fitExcluded ? "true" : "false"},"fullscreen":${w.fullscreen ? "true" : "false"},"maximized":${w.maximized ? "true" : "false"},"sticky":${w.sticky ? "true" : "false"},"fixedAuto":${w.fixedAuto ? "true" : "false"},"fixedSuppress":${w.fixedSuppress ? "true" : "false"},"hints":{"minW":${w.hints.minW ?? "null"},"minH":${w.hints.minH ?? "null"},"maxW":${w.hints.maxW ?? "null"},"maxH":${w.hints.maxH ?? "null"}}}`,
        ).join(",");
        const adjacent = Object.entries(d.adjacent).map(([k, v]) => `${esc(k)}:${esc(v)}`).join(",");
        return `{"id":${esc(d.id)},"ws":${esc(d.ws)},"bounds":[${d.bounds.x},${d.bounds.y},${d.bounds.w},${d.bounds.h}],"gap":${d.gap},"outer":${d.outer},"focused":${esc(d.focused)},"readable":${d.readable ? "true" : "false"},"hold":${esc(d.hold)},"adjacent":{${adjacent}},"windows":[${wins}],"closed":[${d.closed.map(esc).join(",")}]}`;
    }).join(",");
    // Bounded at the core feedback bound (MAX_WORLD_FEEDBACK): the
    // state retains dirty work, but the wire never carries more, so an
    // over-cap report refuses fail-closed instead of stalling.
    const feedback = input.pendingFeedback.slice(0, 1024).map((f) =>
        `{"op":${esc(f.op)},"kind":${esc(f.kind)},"outcome":${esc(f.outcome)},"hold":${esc(f.hold)}}`,
    ).join(",");
    const minimized = observation.minimized.map((m) =>
        `{"win":${esc(m.win)},"out":${esc(m.out)},"ws":${esc(m.ws)}}`,
    ).join(",");
    const commandJson = command === null ? "null" : JSON.stringify(command);
    const defaultTiled = input.defaultTiled;
    return `{"v":${WORLD_CONTRACT_VERSION},"owner":${esc(input.owner)},"adapter":${esc(input.adapter)},"gen":${esc(input.boundGen as string)},"obs":"${input.currentObs}","reqSeq":"${reqSeq}","req":${esc(reqId)},"cmdSeq":${cmdSeq === null ? "null" : `"${cmdSeq}"`},"command":${commandJson},"mode":${esc(observation.mode)},"topology":[${topology}],"bindings":[${bindings}],"domains":[${domains}],"minimized":[${minimized}],"predicate":${esc(observation.predicate)},"refill":${esc(input.refill)},"defaultTiled":${defaultTiled ? "true" : "false"},"focused":${esc(observation.focusedGlobal)},"complete":${observation.complete ? "true" : "false"},"gesture":${observation.gesture ? "true" : "false"},"feedback":[${feedback}]}`;
}
