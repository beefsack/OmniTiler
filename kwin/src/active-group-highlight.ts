// Group-highlight effect payload boundary (script side).
//
// The world route forwards the core-projected active-group/no-group result
// to the effect-owned group-highlight D-Bus endpoint. This module owns only
// the endpoint constants and the bounded QString payload formatter:
// identity plus the ordered member window ids plus the engine-projected
// union bounds; member rectangles never cross to the renderer. Member
// order is preserved verbatim for native stacking choice. It never derives
// topology or native geometry, and it performs no transport.
//
// Effect endpoint (owned by the active-border effect, never the generic
// effect bus):
//   service   com.omnitiler.ActiveBorder
//   path      /com/omnitiler/ActiveBorder
//   interface com.omnitiler.ActiveBorder1
//   methods   SetGroupHighlight(QString) / ClearGroupHighlight()
//
// Diagnostics are always-on and bounded on the caller side.

export const GROUP_HIGHLIGHT_SERVICE = "com.omnitiler.ActiveBorder";
export const GROUP_HIGHLIGHT_OBJECT = "/com/omnitiler/ActiveBorder";
export const GROUP_HIGHLIGHT_INTERFACE = "com.omnitiler.ActiveBorder1";
export const GROUP_HIGHLIGHT_SET_METHOD = "SetGroupHighlight";
export const GROUP_HIGHLIGHT_CLEAR_METHOD = "ClearGroupHighlight";

// Bounded caps. The effect payload cap (65536) is retained from the legacy
// plan reply cap: the setter payload is a subset of that shape and stays
// within the same bound.
export const ACTIVE_GROUP_MAX_REPLY_BYTES = 65536;
export const ACTIVE_GROUP_MAX_ID_LEN = 128;
export const ACTIVE_GROUP_MAX_OWNER_LEN = 128;
export const ACTIVE_GROUP_MAX_GENERATION_LEN = 64;
export const ACTIVE_GROUP_MAX_CORRELATION_LEN = 128;

export interface ActiveGroupRect {
    readonly x: number;
    readonly y: number;
    readonly w: number;
    readonly h: number;
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFiniteInt(value: unknown): value is number {
    return typeof value === "number" && Number.isFinite(value) && Number.isInteger(value);
}

function isOpaqueId(value: unknown, maxLen: number, allowEmpty: boolean): value is string {
    if (typeof value !== "string") {
        return false;
    }
    if (value.length === 0) {
        return allowEmpty;
    }
    if (value.length > maxLen) {
        return false;
    }
    for (let index = 0; index < value.length; index += 1) {
        const code = value.charCodeAt(index);
        const alnum =
            (code >= 48 && code <= 57) || (code >= 65 && code <= 90) || (code >= 97 && code <= 122);
        if (!(alnum || code === 45 || code === 95 || code === 46)) {
            return false;
        }
    }
    return true;
}

function isGenerationId(value: unknown): value is string {
    if (typeof value !== "string" || value.length === 0 || value.length > ACTIVE_GROUP_MAX_GENERATION_LEN) {
        return false;
    }
    for (let index = 0; index < value.length; index += 1) {
        const code = value.charCodeAt(index);
        if (!((code >= 48 && code <= 57) || (code >= 97 && code <= 122) || code === 45)) {
            return false;
        }
    }
    return true;
}

function isTargetRect(value: unknown): value is ActiveGroupRect {
    if (!isRecord(value)) {
        return false;
    }
    const keys = Object.keys(value);
    if (keys.length !== 4 || keys.indexOf("x") < 0 || keys.indexOf("y") < 0 || keys.indexOf("w") < 0 || keys.indexOf("h") < 0) {
        return false;
    }
    const x = value["x"];
    const y = value["y"];
    const w = value["w"];
    const h = value["h"];
    if (!isFiniteInt(x) || !isFiniteInt(y) || !isFiniteInt(w) || !isFiniteInt(h)) {
        return false;
    }
    if (w <= 0 || h <= 0) {
        return false;
    }
    if (x < -16384 || x > 16384 || y < -16384 || y > 16384 || w > 16384 || h > 16384) {
        return false;
    }
    return true;
}

function isRevision(value: unknown): value is number {
    return isFiniteInt(value) && value >= 0 && value <= 9007199254740991;
}

// Formats the bounded QString payload forwarded to the owned effect setter.
// Carries only identity plus the ordered member window ids plus the
// engine-projected union bounds; member rectangles never cross to the
// renderer. Member order is preserved verbatim for native stacking choice.
export function formatGroupHighlightPayload(
    correlationId: string,
    owner: string,
    generation: string,
    revision: number,
    group: string,
    focusedWindow: string,
    members: ReadonlyArray<string>,
    bounds: ActiveGroupRect,
): string | null {
    if (!isOpaqueId(correlationId, ACTIVE_GROUP_MAX_CORRELATION_LEN, false)) {
        return null;
    }
    if (!isOpaqueId(owner, ACTIVE_GROUP_MAX_OWNER_LEN, false)) {
        return null;
    }
    if (!isGenerationId(generation)) {
        return null;
    }
    if (!isRevision(revision)) {
        return null;
    }
    if (!isOpaqueId(group, ACTIVE_GROUP_MAX_ID_LEN, false)) {
        return null;
    }
    if (!isOpaqueId(focusedWindow, ACTIVE_GROUP_MAX_ID_LEN, false)) {
        return null;
    }
    if (!Array.isArray(members as unknown) || members.length === 0) {
        return null;
    }
    const seenMembers = new Set<string>();
    for (const entry of members) {
        if (!isOpaqueId(entry, ACTIVE_GROUP_MAX_ID_LEN, false) || seenMembers.has(entry)) {
            return null;
        }
        seenMembers.add(entry);
    }
    if (!isTargetRect({ x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h })) {
        return null;
    }
    let payload = "";
    try {
        payload = JSON.stringify({
            v: 1,
            correlation_id: correlationId,
            owner,
            generation,
            revision,
            group,
            focused_window: focusedWindow,
            members: members.slice(),
            bounds: { x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h },
        });
    } catch (error) {
        void error;
        return null;
    }
    if (payload.length > ACTIVE_GROUP_MAX_REPLY_BYTES) {
        return null;
    }
    return payload;
}
