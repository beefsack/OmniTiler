// Authoritative native gesture lifecycle transport (KWin script route,
// read-only): the effect-owned GestureLifecycleStore behind a held-waiter
// plus sticky-pull surface. Pure parsing plus canonical raw encoders only:
// the broker owns ALL mapping (native identity to core id), so the script
// never mints, folds, or maps. The entry carries the raw native snapshot
// plus raw same-capture script cursor/windowAt facts; the broker normalizes
// them into the core GestureIngress lane before stamping.

import { isCanonicalU64, isOpaqueId } from "./world-seq";

export const GESTURE_SERVICE = "com.omnitiler.Gesture";
export const GESTURE_OBJECT = "/com/omnitiler/Gesture";
export const GESTURE_INTERFACE = "com.omnitiler.Gesture1";
export const GESTURE_WAIT_METHOD = "WaitGesture";
export const GESTURE_LAST_METHOD = "LastGesture";
export const GESTURE_ACK_METHOD = "AckGesture";
export const GESTURE_LEASE_MS = 20000;

export type NativeGesturePhase = "started" | "updated" | "released" | "cancelled" | "idle";

export interface NativeGestureSnapshot {
    readonly instance: string;
    readonly serial: string;
    readonly revision: string;
    readonly phase: NativeGesturePhase;
    readonly win: string;
    readonly kind: "" | "move" | "resize";
    readonly startRect: readonly [number, number, number, number] | null;
    readonly startCursor: readonly [number, number] | null;
    readonly cursor: readonly [number, number] | null;
    readonly modalMove: boolean;
    readonly changed: boolean;
    readonly superseded: boolean;
    readonly available: boolean;
}

function isIntCoord(n: unknown): n is number {
    return typeof n === "number" && Number.isInteger(n) && n >= -16384 && n <= 16384;
}

function parsePoint(value: unknown): readonly [number, number] | null | undefined {
    if (value === null) {
        return null;
    }
    if (!Array.isArray(value) || value.length !== 2) {
        return undefined;
    }
    const [x, y] = value as ReadonlyArray<unknown>;
    if (!isIntCoord(x) || !isIntCoord(y)) {
        return undefined;
    }
    return [x, y];
}

function parseRect(value: unknown): readonly [number, number, number, number] | null | undefined {
    if (value === null) {
        return null;
    }
    if (!Array.isArray(value) || value.length !== 4) {
        return undefined;
    }
    const [x, y, w, h] = value as ReadonlyArray<unknown>;
    if (!isIntCoord(x) || !isIntCoord(y)) {
        return undefined;
    }
    if (typeof w !== "number" || typeof h !== "number" || !Number.isInteger(w) || !Number.isInteger(h)) {
        return undefined;
    }
    if (w < 1 || h < 1 || w > 16384 || h > 16384) {
        return undefined;
    }
    return [x, y, w, h];
}

// Strict versioned parse of one gesture-facts snapshot (v:1). Oversize
// payloads refuse like malformed ones. Any other shape is unknown (never a
// partial snapshot: a half-parsed start with guessed facts would mix
// sources). Idle snapshots carry empty win/kind plus null facts.
export function parseNativeGesture(reply: unknown): NativeGestureSnapshot | null {
    if (typeof reply !== "string") {
        return null;
    }
    if (reply.length > 64 * 1024) {
        return null;
    }
    let value: unknown;
    try {
        value = JSON.parse(reply);
    } catch (error) {
        void error;
        return null;
    }
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        return null;
    }
    const r = value as Record<string, unknown>;
    if (r["v"] !== 1) {
        return null;
    }
    const instance = r["instance"];
    const serial = r["serial"];
    const revision = r["revision"];
    const phase = r["phase"];
    const win = r["win"];
    const kind = r["kind"];
    if (typeof instance !== "string" || instance === "" || instance.length > 128) {
        return null;
    }
    if (typeof serial !== "string" || !isCanonicalU64(serial)) {
        return null;
    }
    if (typeof revision !== "string" || !isCanonicalU64(revision)) {
        return null;
    }
    if (phase !== "started" && phase !== "updated" && phase !== "released" && phase !== "cancelled" && phase !== "idle") {
        return null;
    }
    if (typeof win !== "string" || (win !== "" && !isOpaqueId(win))) {
        return null;
    }
    if (kind !== "" && kind !== "move" && kind !== "resize") {
        return null;
    }
    const startRect = parseRect(r["startRect"]);
    if (startRect === undefined) {
        return null;
    }
    const startCursor = parsePoint(r["startCursor"]);
    if (startCursor === undefined) {
        return null;
    }
    const cursor = parsePoint(r["cursor"]);
    if (cursor === undefined) {
        return null;
    }
    const modalMove = r["modalMove"];
    const changed = r["changed"];
    const superseded = r["superseded"];
    const available = r["available"];
    if ((modalMove !== 0 && modalMove !== 1) || (changed !== 0 && changed !== 1) || (superseded !== 0 && superseded !== 1) || (available !== 0 && available !== 1)) {
        return null;
    }
    const idle = phase === "idle";
    if (idle) {
        if (serial !== "0" || win !== "" || kind !== "" || startRect !== null || startCursor !== null || cursor !== null) {
            return null;
        }
    } else {
        if (serial === "0" || win === "" || kind === "") {
            return null;
        }
        if (startRect === null) {
            return null;
        }
    }
    return {
        instance, serial, revision, phase: phase as NativeGesturePhase, win, kind: kind as "" | "move" | "resize",
        startRect, startCursor, cursor,
        modalMove: modalMove === 1, changed: changed === 1, superseded: superseded === 1, available: available === 1,
    };
}

export function gestureWaitRequest(instance: string, serial: string, revision: string, leaseMs: number): string {
    return JSON.stringify({ instance, serial, revision, leaseMs });
}

export function gestureAckRequest(instance: string, serial: string): string {
    return JSON.stringify({ instance, serial });
}

// Canonical raw snapshot encoder: the verbatim native v1 shape the broker
// normalizes (0/1 numeric flags, null-or-tuple points, null-or-rect start).
// The broker strips this lane before the strict decode; the script never
// maps it to core ingress itself.
export function rawNativeGestureJson(snapshot: NativeGestureSnapshot): string {
    const pt = (p: readonly [number, number] | null): string =>
        (p === null ? "null" : `[${p[0]},${p[1]}]`);
    const rect = (r: readonly [number, number, number, number] | null): string =>
        (r === null ? "null" : `[${r[0]},${r[1]},${r[2]},${r[3]}]`);
    const flag = (b: boolean): string => (b ? "1" : "0");
    return `{"v":1,"instance":${JSON.stringify(snapshot.instance)},` +
        `"serial":${JSON.stringify(snapshot.serial)},` +
        `"revision":${JSON.stringify(snapshot.revision)},` +
        `"phase":${JSON.stringify(snapshot.phase)},` +
        `"win":${JSON.stringify(snapshot.win)},` +
        `"kind":${JSON.stringify(snapshot.kind)},` +
        `"startRect":${rect(snapshot.startRect)},` +
        `"startCursor":${pt(snapshot.startCursor)},` +
        `"cursor":${pt(snapshot.cursor)},` +
        `"modalMove":${flag(snapshot.modalMove)},` +
        `"changed":${flag(snapshot.changed)},` +
        `"superseded":${flag(snapshot.superseded)},` +
        `"available":${flag(snapshot.available)}}`;
}
