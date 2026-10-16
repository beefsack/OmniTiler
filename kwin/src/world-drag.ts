// Drag-oracle verdict route for the world adapter (D2/D3): Started/Finish
// cursor capture, press-third override, strict no-op cancelled verdicts,
// tiled move-drop plus resize routing through world Operation commands,
// continuous latest-wins drag-preview with renderer actuation, and
// per-gesture clear/fencing. Observation for the verdict route only;
// workspace policy stays in core. No KWin or D-Bus imports beyond the
// oracle pull entry and signal helpers: the entry drives this with captured
// observations, transport results, and renderer hooks.

import { connectSignal, readSignal } from "./signal-capability";
import {
    identifyGrabbedEdges,
    identifyPressGrabbed,
    resolveOracleResizeTargets,
    startDragOraclePullEntry,
    type DragOracleFinishContext,
    type DragOracleVerdict,
    type OracleGrabbed,
} from "./drag-oracle-pull";
import { isOpaqueId } from "./world-seq";
import type { WorldObservation } from "./world-observe";
import type { WorldCommand, WorldReplyResult } from "./world-report";

export interface WorldDragHover {
    readonly domainOut: string;
    readonly domainWs: string;
    readonly leaf: string;
    readonly win: string;
    readonly rev: string;
    readonly prior: { readonly group: string; readonly edge: string } | null;
    readonly priorOut: string;
    readonly priorWs: string;
}

export interface WorldDragEnv {
    surface: object;
    log: (message: string) => void;
    token: (value: unknown) => string;
    readProp: (value: object, property: string) => unknown;
    observeForCommand: () => WorldObservation | null;
    submitCommand: (command: WorldCommand, label: string) => boolean;
    isStopped: () => boolean;
    trackDetach: (detach: () => void) => void;
    // Native preview renderer (the effect remains the renderer; no policy
    // moves here): derived core rects render, terminal gestures clear.
    setPreview: (rect: { x: number; y: number; w: number; h: number }) => boolean;
    clearPreview: () => void;
    oracleCallDbus?:
        | ((
            service: string, path: string, iface: string, method: string,
            callback: (reply: unknown) => void,
        ) => void)
        | undefined;
}

export interface WorldDragRoute {
    readonly attach: () => void;
    readonly stop: () => void;
    // Continuous latest-wins preview: an accepted world reply renders the
    // derived core rect (identical-rect repeats never re-set), chains the
    // carried hover prior into the next preview and the finish drop, and
    // immediately dispatches the coalesced pending tick (no later Stepped
    // event required). A finished gesture fences its late replies: the
    // finish snapshot keeps the last completed hover prior.
    readonly notifyReplyAccepted: (result: WorldReplyResult | null) => void;
    // Stale replies settle nothing: the fence clears so the next tick
    // retries, with one bounded line (the core never saw the preview).
    readonly notifyStaleReply: (reason: string) => void;
}

interface StartRecord {
    rect: { x: number; y: number; w: number; h: number };
    id: string;
    grabbed: OracleGrabbed;
    source: { out: string; ws: string } | null;
    // Started move versus resize state: previews and move-drops arm only
    // for move gestures; resize gestures keep the record for the resize
    // verdict route but never preview.
    move: boolean;
    epoch: number;
    finished: boolean;
    // Last completed hover prior from the core result chain (null until the
    // first accepted preview). Forwarded verbatim into the next preview and
    // snapshotted at Finish for the drop; mismatched carries are never
    // invented here (the core validates advisory-style).
    hover: WorldDragHover | null;
}

interface FinishSnapshot {
    pointerFinish: { x: number; y: number } | null;
    hover: WorldDragHover | null;
}

export function createWorldDragRoute(env: WorldDragEnv): WorldDragRoute {
    const { surface, log, token } = env;
    const readProp = env.readProp;
    // Drag-start capture: live rect plus grabbed edges at Started (press
    // override applies only after ref/identity verification at finish), the
    // homed source domain for cross-output drag pairs, the move/resize
    // state, and an epoch token so a newer Started always wins over a stale
    // Finish. Consumed once per finish.
    const oracleStarts = new Map<object, StartRecord>();
    const finishSnapshots = new Map<object, FinishSnapshot & { epoch: number }>();
    let epochCounter = 0;
    // Preview flight per gesture ref: at most one operation in flight;
    // Stepped ticks while fenced coalesce to the latest pointer and flush
    // immediately after the accepted reply.
    const previewFlight = new Set<object>();
    const previewPending = new Map<object, { x: number; y: number }>();
    const previewShown = new Map<object, { x: number; y: number; w: number; h: number }>();
    // Global single-flight correlation: the transport carries at most one
    // command, so at most one preview result can be outstanding.
    let inflightRef: object | null = null;
    const wiredWindows = new Set<object>();
    let oracleHandle: { stop: () => void } | null = null;

    const readCursor = (): { x: number; y: number } | null => {
        try {
            const cursor = readProp(surface, "cursorPos") as { x?: unknown; y?: unknown } | undefined;
            if (cursor !== undefined && cursor !== null && typeof cursor["x"] === "number" && typeof cursor["y"] === "number") {
                const x = Math.trunc(cursor["x"] as number);
                const y = Math.trunc(cursor["y"] as number);
                if (x >= -16384 && x <= 16384 && y >= -16384 && y <= 16384) {
                    return { x, y };
                }
            }
        } catch (error) {
            void error;
        }
        return null;
    };

    const hoverForCommand = (hover: WorldDragHover | null): unknown => {
        if (hover === null) {
            return null;
        }
        return {
            domainOut: hover.domainOut,
            domainWs: hover.domainWs,
            leaf: hover.leaf,
            win: hover.win,
            rev: hover.rev,
            prior: hover.prior === null
                ? null
                : { group: hover.prior.group, edge: hover.prior.edge },
            priorOut: hover.priorOut,
            priorWs: hover.priorWs,
        };
    };

    const clearShownOverlay = (ref: object, reason: string): void => {
        if (previewShown.has(ref)) {
            previewShown.delete(ref);
            try {
                env.clearPreview();
            } catch (error) {
                void error;
            }
            log(`omnitiler:world:drag-preview-cleared correlation=${token(reason)} reason=terminal`);
        }
    };

    // Drag-oracle verdict route: the single finished-handler LastVerdict
    // pull routes exactly one strict operation after a non-cancelled
    // verdict: a same-size move becomes a tiled drag-drop (finish pointer
    // proof plus the Started source domain and the finish-snapshotted hover
    // prior), a size change becomes a pointer-resize. Cancelled verdicts
    // are a strict no-op (preview cleared through the renderer, no
    // command); derive failures fail closed with exact bounded reasons. No
    // push, retry, or fallback. Starts are captured at Started (live rect
    // plus homed source); the press override applies only after
    // ref/identity verification here. Tiled-only: floating/sticky at either
    // end, fullscreen, and maximized refuse with no command. Gesture
    // lifetime barriers apply: geometry effects suspend while the gesture
    // is active, so drops dispatch after the gesture settles via the next
    // event (fail closed here, never forced mid-gesture).
    const routeOracleVerdict = (verdict: DragOracleVerdict, ctx: DragOracleFinishContext | undefined): void => {
        try {
            if (env.isStopped() || ctx === undefined) {
                return;
            }
            const snapshot = finishSnapshots.get(ctx.ref);
            finishSnapshots.delete(ctx.ref);
            const start = oracleStarts.get(ctx.ref);
            // Tokenized Finish versus newer Started: a Started epoch newer
            // than the finish token owns the ref now; the stale verdict
            // drops without consuming the new gesture.
            if (start !== undefined && start.epoch > ctx.finishEpoch) {
                log(`omnitiler:world:drag-stale-finish correlation=${token(verdict.correlation)}`);
                return;
            }
            oracleStarts.delete(ctx.ref);
            previewFlight.delete(ctx.ref);
            previewPending.delete(ctx.ref);
            if (inflightRef === ctx.ref) {
                inflightRef = null;
            }
            clearShownOverlay(ctx.ref, verdict.correlation);
            if (start === undefined) {
                log(`omnitiler:world:drag-start-missing correlation=${token(verdict.correlation)}`);
                return;
            }
            const liveObs = env.observeForCommand();
            let liveWin: { win: string; ref: object; fullscreen: boolean; maximized: boolean; floating: boolean; sticky: boolean } | undefined = undefined;
            if (liveObs !== null) {
                for (const domain of liveObs.domains) {
                    for (const w of domain.windows) {
                        if (w.win === verdict.windowIdentity) {
                            liveWin = w;
                            break;
                        }
                    }
                    if (liveWin !== undefined) {
                        break;
                    }
                }
            }
            if (liveWin === undefined || liveWin.ref !== ctx.ref) {
                log(`omnitiler:world:drag-unknown-window correlation=${token(verdict.correlation)}`);
                return;
            }
            if (liveWin.fullscreen || liveWin.maximized) {
                log(`omnitiler:world:drag-refused-overlay correlation=${token(verdict.correlation)}`);
                return;
            }
            if (liveWin.floating || liveWin.sticky) {
                log(`omnitiler:world:drag-refused-floating correlation=${token(verdict.correlation)}`);
                return;
            }
            if (!start.move) {
                // A resize gesture never move-drops: fall through to the
                // resize route below when the size changed, else a no-op.
                if (verdict.finalRect.w === start.rect.w && verdict.finalRect.h === start.rect.h) {
                    log(`omnitiler:world:drag-noop correlation=${token(verdict.correlation)}`);
                    return;
                }
            }
            const movedSize = verdict.finalRect.w !== start.rect.w || verdict.finalRect.h !== start.rect.h;
            const movedPos = verdict.finalRect.x !== start.rect.x || verdict.finalRect.y !== start.rect.y;
            if (!movedSize && movedPos && start.move) {
                // Tiled move-drop: the Finish pointer proof plus the Started
                // source domain and the finish-snapshotted hover prior (the
                // last completed core carry; null until the first accepted
                // preview). Cross-output pairs run through core
                // world_commands. No final-rect-center fallback: without
                // finish cursor proof the drop refuses like legacy.
                const pointer = snapshot?.pointerFinish ?? null;
                if (pointer === null) {
                    log(`omnitiler:world:drag-refused-coords correlation=${token(verdict.correlation)}`);
                    return;
                }
                const px = pointer.x;
                const py = pointer.y;
                if (px < -16384 || px > 16384 || py < -16384 || py > 16384) {
                    log(`omnitiler:world:drag-refused-coords correlation=${token(verdict.correlation)}`);
                    return;
                }
                const hover = snapshot?.hover ?? null;
                const command: WorldCommand = start.source !== null
                    ? {
                        op: "operation",
                        command: { op: "drag-drop", win: verdict.windowIdentity, x: px, y: py, hover: hoverForCommand(hover), source: { out: start.source.out, ws: start.source.ws } },
                    }
                    : {
                        op: "operation",
                        command: { op: "drag-drop", win: verdict.windowIdentity, x: px, y: py, hover: hoverForCommand(hover), source: null },
                    };
                if (!env.submitCommand(command, `drag-${verdict.correlation}`)) {
                    log(`omnitiler:world:drag-busy correlation=${token(verdict.correlation)}`);
                }
                return;
            }
            let grabbed: OracleGrabbed | null = start.grabbed;
            let pressAbsent = false;
            if (verdict.press !== undefined) {
                try {
                    const identified = identifyPressGrabbed(start.rect, verdict.press);
                    if (identified !== null) {
                        grabbed = identified.grabbed;
                    } else {
                        pressAbsent = true;
                    }
                } catch (error) {
                    void error;
                    pressAbsent = true;
                }
            }
            if (grabbed === null) {
                log(`omnitiler:world:drag-press-absent correlation=${token(verdict.correlation)}`);
                return;
            }
            if (pressAbsent) {
                log(`omnitiler:world:drag-press-unusable correlation=${token(verdict.correlation)}`);
            }
            const resolved = resolveOracleResizeTargets(start.rect, verdict.finalRect, grabbed);
            if (resolved === null || resolved.targets.length === 0) {
                log(`omnitiler:world:drag-noop correlation=${token(verdict.correlation)}`);
                return;
            }
            const first = resolved.targets[0] as { direction: string; boundary: number };
            const second = resolved.targets[1] as { direction: string; boundary: number } | undefined;
            const horizontal = (d: string): boolean => d === "left" || d === "right";
            if (second !== undefined && horizontal(first.direction) === horizontal(second.direction)) {
                log(`omnitiler:world:drag-refused-direction correlation=${token(verdict.correlation)}`);
                return;
            }
            const command: WorldCommand = second === undefined
                ? {
                    op: "operation",
                    command: { op: "pointer-resize", win: verdict.windowIdentity, direction: first.direction, boundary: first.boundary, direction2: null, boundary2: null },
                }
                : {
                    op: "operation",
                    command: { op: "pointer-resize", win: verdict.windowIdentity, direction: first.direction, boundary: first.boundary, direction2: second.direction, boundary2: second.boundary },
                };
            if (!env.submitCommand(command, `drag-${verdict.correlation}`)) {
                log(`omnitiler:world:drag-busy correlation=${token(verdict.correlation)}`);
            }
        } catch (error) {
            void error;
        }
    };

    const settleOracleVerdict = (verdict: DragOracleVerdict | null, ctx: DragOracleFinishContext | undefined): void => {
        try {
            if (ctx !== undefined) {
                oracleStarts.delete(ctx.ref);
                finishSnapshots.delete(ctx.ref);
                previewFlight.delete(ctx.ref);
                previewPending.delete(ctx.ref);
                if (inflightRef === ctx.ref) {
                    inflightRef = null;
                }
                clearShownOverlay(ctx.ref, verdict !== null ? verdict.correlation : "cancelled");
            }
            if (verdict !== null && verdict.cancelled === true && ctx !== undefined) {
                // Strict no-op cancelled: clear through the renderer, no
                // command, no retry. Fenced per gesture lifetime.
                log(`omnitiler:world:drag-preview-cleared correlation=${token(verdict.correlation)} reason=cancelled`);
            }
        } catch (error) {
            void error;
        }
    };

    // Drag-preview during the gesture: at most one operation in flight per
    // gesture ref, from the live cursor plus the Started source domain and
    // the last completed hover prior (null until the first accepted
    // preview; never invented). Ticks while fenced coalesce to the latest
    // pointer and flush immediately after the accepted reply. Move gestures
    // only; resize gestures never preview. Busy/single-flight refuses fail
    // closed with one bounded line and release the fence for retry.
    const submitPreviewAt = (ref: object, pointer: { x: number; y: number }): void => {
        try {
            if (env.isStopped() || previewFlight.has(ref)) {
                return;
            }
            const start = oracleStarts.get(ref);
            if (start === undefined || start.finished || !start.move) {
                return;
            }
            const command: WorldCommand = start.source !== null
                ? { op: "operation", command: { op: "drag-preview", win: start.id, x: pointer.x, y: pointer.y, hover: hoverForCommand(start.hover), source: { out: start.source.out, ws: start.source.ws } } }
                : { op: "operation", command: { op: "drag-preview", win: start.id, x: pointer.x, y: pointer.y, hover: hoverForCommand(start.hover), source: null } };
            previewFlight.add(ref);
            inflightRef = ref;
            if (!env.submitCommand(command, `drag-preview-${start.id}`)) {
                previewFlight.delete(ref);
                if (inflightRef === ref) {
                    inflightRef = null;
                }
                log(`omnitiler:world:drag-preview-busy window=${token(start.id)}`);
            }
        } catch (error) {
            void error;
        }
    };

    const submitDragPreview = (ref: object): void => {
        try {
            if (env.isStopped() || previewFlight.has(ref)) {
                // Fenced: coalesce to the latest pointer; the accepted
                // reply flushes it without waiting for another tick.
                if (previewFlight.has(ref)) {
                    const pointer = readCursor();
                    if (pointer !== null) {
                        previewPending.set(ref, pointer);
                    }
                }
                return;
            }
            const start = oracleStarts.get(ref);
            if (start === undefined || start.finished || !start.move) {
                return;
            }
            const pointer = readCursor();
            if (pointer === null) {
                return;
            }
            previewPending.delete(ref);
            submitPreviewAt(ref, pointer);
        } catch (error) {
            void error;
        }
    };

    // Finish pointer capture for the move-drop route plus the
    // finish-snapshotted hover prior: the cursor at Finish, read
    // synchronously in the finish context factory (never at reply), and
    // the last completed core hover carry. Later preview replies fence
    // against this snapshot (never steer the finished verdict).
    const makeFinishContext = (ref: object): DragOracleFinishContext => {
        const start = oracleStarts.get(ref);
        const epoch = start !== undefined ? start.epoch : epochCounter;
        const pointerFinish = readCursor();
        const hover = start?.hover ?? null;
        if (start !== undefined) {
            start.finished = true;
        }
        previewPending.delete(ref);
        finishSnapshots.set(ref, { pointerFinish, hover, epoch });
        clearShownOverlay(ref, start !== undefined ? start.id : "finish");
        return { ref, finishEpoch: epoch, ...(pointerFinish !== null ? { pointerFinish } : {}) };
    };

    const captureStart = (item: object): void => {
        try {
            const g = readProp(item, "frameGeometry") as Record<string, unknown> | undefined;
            if (typeof g !== "object" || g === null) {
                return;
            }
            const x = g["x"];
            const y = g["y"];
            const w = g["width"] !== undefined ? g["width"] : g["w"];
            const h = g["height"] !== undefined ? g["height"] : g["h"];
            if (typeof x !== "number" || typeof y !== "number" || typeof w !== "number" || typeof h !== "number") {
                return;
            }
            const raw = readProp(item, "internalId");
            const id = typeof raw === "string" ? raw : (() => {
                try {
                    return String(raw);
                } catch (error) {
                    void error;
                    return "";
                }
            })();
            if (!isOpaqueId(id)) {
                return;
            }
            // Started move-versus-resize state from the live gesture flags
            // (never assumed): previews and move-drops arm only for move;
            // resize keeps the record for the resize verdict route.
            let move = false;
            try {
                const moveRaw = readProp(item, "move");
                const resizeRaw = readProp(item, "resize");
                move = moveRaw === true;
                void resizeRaw;
            } catch (error) {
                void error;
                move = false;
            }
            // Grabbed edges at Started from the live cursor
            // (nearest-edge/corner, or KWin thirds deep inside); the
            // verified press override at finish replaces these.
            let grabbed: OracleGrabbed | null = null;
            try {
                const cursor = readProp(surface, "cursorPos") as { x?: unknown; y?: unknown } | undefined;
                const px = cursor !== undefined && cursor !== null ? cursor["x"] : undefined;
                const py = cursor !== undefined && cursor !== null ? cursor["y"] : undefined;
                const pointer = typeof px === "number" && typeof py === "number"
                    ? { x: Math.round(px), y: Math.round(py) }
                    : null;
                const rect = { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) };
                const identified = identifyGrabbedEdges(rect, pointer);
                grabbed = identified === null ? null : identified.grabbed;
            } catch (error) {
                void error;
                grabbed = null;
            }
            if (grabbed === null) {
                return;
            }
            // Homed source domain at Started for cross-output drag pairs
            // (observation only).
            let source: { out: string; ws: string } | null = null;
            try {
                const outputRaw = readProp(item, "output");
                const out = typeof outputRaw === "object" && outputRaw !== null
                    ? readProp(outputRaw as object, "name")
                    : null;
                const members = readProp(item, "desktops");
                if (typeof out === "string" && isOpaqueId(out) && Array.isArray(members)) {
                    for (const m of members as ReadonlyArray<unknown>) {
                        if (typeof m === "object" && m !== null && isOpaqueId(readProp(m as object, "id"))) {
                            source = { out, ws: readProp(m as object, "id") as string };
                            break;
                        }
                    }
                }
            } catch (error) {
                void error;
            }
            epochCounter += 1;
            clearShownOverlay(item, id);
            oracleStarts.set(item, {
                rect: { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) },
                id,
                grabbed,
                source,
                move,
                epoch: epochCounter,
                finished: false,
                hover: null,
            });
            previewFlight.delete(item);
            previewPending.delete(item);
            finishSnapshots.delete(item);
        } catch (error) {
            void error;
        }
    };

    const wireWindow = (item: object): void => {
        if (wiredWindows.has(item)) {
            return;
        }
        wiredWindows.add(item);
        const started = readSignal(item, "interactiveMoveResizeStarted");
        const detach = connectSignal(started, () => {
            try {
                captureStart(item);
            } catch (error) {
                void error;
            }
        });
        if (detach !== null) {
            env.trackDetach(detach);
        }
        // Stepped preview (when the host exposes it): latest-wins
        // drag-preview operations from the live cursor plus the Started
        // source and the chained hover prior. Absent signals simply never
        // preview; the drop route is unaffected.
        try {
            const stepped = readSignal(item, "interactiveMoveResizeStepped");
            const previewDetach = connectSignal(stepped, () => {
                try {
                    submitDragPreview(item);
                } catch (error) {
                    void error;
                }
            });
            if (previewDetach !== null) {
                env.trackDetach(previewDetach);
            }
        } catch (error) {
            void error;
        }
    };

    const attach = (): void => {
        try {
            const oracleCall = env.oracleCallDbus !== undefined
                ? (service: string, path: string, iface: string, method: string, callback: (reply: unknown) => void): void => {
                    try {
                        env.oracleCallDbus?.(service, path, iface, method, callback);
                    } catch (error) {
                        void error;
                    }
                }
                : undefined;
            const entryOpts = { workspace: surface, log, routePointer: routeOracleVerdict, onSettled: settleOracleVerdict, makeFinishContext };
            const handle = oracleCall !== undefined
                ? startDragOraclePullEntry({ ...entryOpts, callDbus: oracleCall })
                : startDragOraclePullEntry(entryOpts);
            if (handle !== null) {
                oracleHandle = handle;
                void oracleHandle;
            }
        } catch (error) {
            void error;
        }
        // Drag-start capture through public signals only (observation for
        // the verdict route, never policy): the windows present at attach
        // plus every window added later (a Started without a record can
        // never preview or drop). Removals settle the gesture maps so old
        // subscriptions never accumulate.
        try {
            const fn = readProp(surface, "windowList");
            if (typeof fn === "function") {
                const list = Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []) as ReadonlyArray<unknown>;
                if (Array.isArray(list)) {
                    for (const item of list) {
                        if (typeof item !== "object" || item === null) {
                            continue;
                        }
                        wireWindow(item as object);
                    }
                }
            }
        } catch (error) {
            void error;
        }
        try {
            const addedDetach = connectSignal(readSignal(surface, "windowAdded"), (added) => {
                try {
                    if (typeof added === "object" && added !== null) {
                        wireWindow(added as object);
                    }
                } catch (error) {
                    void error;
                }
            });
            if (addedDetach !== null) {
                env.trackDetach(addedDetach);
            }
        } catch (error) {
            void error;
        }
        try {
            const removedDetach = connectSignal(readSignal(surface, "windowRemoved"), (removed) => {
                try {
                    if (typeof removed === "object" && removed !== null) {
                        const ref = removed as object;
                        oracleStarts.delete(ref);
                        finishSnapshots.delete(ref);
                        previewFlight.delete(ref);
                        previewPending.delete(ref);
                        wiredWindows.delete(ref);
                        if (inflightRef === ref) {
                            inflightRef = null;
                        }
                        clearShownOverlay(ref, "removed");
                    }
                } catch (error) {
                    void error;
                }
            });
            if (removedDetach !== null) {
                env.trackDetach(removedDetach);
            }
        } catch (error) {
            void error;
        }
    };

    const stop = (): void => {
        if (oracleHandle !== null) {
            try {
                oracleHandle.stop();
            } catch (error) {
                void error;
            }
            oracleHandle = null;
        }
        oracleStarts.clear();
        finishSnapshots.clear();
        previewFlight.clear();
        previewPending.clear();
        previewShown.clear();
        wiredWindows.clear();
        inflightRef = null;
    };

    // Settle the preview flight on the accepted reply that carried it: the
    // derived core rect renders (identical-rect repeats never re-set), the
    // carried hover prior chains into the next preview and the finish drop,
    // and the coalesced pending tick dispatches immediately from the latest
    // pointer. Finished gestures fence their late replies (the finish
    // snapshot already kept the last completed hover prior). Source
    // mismatches clear without rendering: a preview for another binding
    // never steers this gesture.
    const notifyReplyAccepted = (result: WorldReplyResult | null): void => {
        try {
            const ref = inflightRef;
            inflightRef = null;
            if (ref === null) {
                return;
            }
            previewFlight.delete(ref);
            const start = oracleStarts.get(ref);
            if (start === undefined || start.finished) {
                previewPending.delete(ref);
                return;
            }
            if (result !== null && result.op === "drag-preview") {
                if (start.source !== null &&
                    (result.sourceOut !== start.source.out || result.sourceWs !== start.source.ws)) {
                    previewPending.delete(ref);
                    clearShownOverlay(ref, start.id);
                    log(`omnitiler:world:drag-preview-mismatch window=${token(start.id)}`);
                    return;
                }
                const shown = previewShown.get(ref);
                const same = shown !== undefined &&
                    shown.x === result.rect[0] && shown.y === result.rect[1] &&
                    shown.w === result.rect[2] && shown.h === result.rect[3];
                if (!same) {
                    let ok = false;
                    try {
                        ok = env.setPreview({ x: result.rect[0], y: result.rect[1], w: result.rect[2], h: result.rect[3] });
                    } catch (error) {
                        void error;
                        ok = false;
                    }
                    if (ok) {
                        if (shown === undefined) {
                            log(`omnitiler:world:drag-preview-shown window=${token(start.id)}`);
                        }
                        previewShown.set(ref, { x: result.rect[0], y: result.rect[1], w: result.rect[2], h: result.rect[3] });
                    }
                }
                start.hover = result.hover;
            }
            const pending = previewPending.get(ref);
            if (pending !== undefined) {
                previewPending.delete(ref);
                submitPreviewAt(ref, pending);
            }
        } catch (error) {
            void error;
        }
    };

    const notifyStaleReply = (reason: string): void => {
        try {
            if (inflightRef !== null) {
                previewFlight.delete(inflightRef);
                inflightRef = null;
            }
            if (previewFlight.size > 0 || previewPending.size > 0) {
                previewFlight.clear();
                previewPending.clear();
                log(`omnitiler:world:drag-preview-cleared reason=${token(reason)}`);
            }
        } catch (error) {
            void error;
        }
    };

    return { attach, stop, notifyReplyAccepted, notifyStaleReply };
}
