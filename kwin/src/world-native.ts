// Synchronous native state toggles for the world adapter: float with B9
// maximized-unfloat parity plus sticky/maximize/fullscreen. Each toggle
// fences outstanding world applies BEFORE setters (no races) and reports an
// explicit outcome line; applied writes invalidate synchronously BEFORE the
// first setter so the next full report carries the authoritative
// observation. Sticky choreography keeps legacy parity natively (floating
// origin plus keepAbove/keepBelow); fullscreen/maximized overlays refuse
// before any adoption or setter and game safety wins over parity. Setters
// use the exact KWin names (`setMaximize`, `fullScreen`, `onAllDesktops`,
// `maximizeMode` readback); success is the truthful readback, never the
// setter boolean. No workspace policy: core converges the tiling
// consequences from the next full report.

import type { WorldCommand } from "./world-report";
import type { ApplyEnv } from "./world-apply";
import { readDesktopForOutput, writeDesktopForOutput } from "./workspace-native";
import { isOpaqueId } from "./world-seq";

export interface WorldNativeEnv {
    surface: object;
    log: (message: string) => void;
    token: (value: unknown) => string;
    readProp: (value: object, property: string) => unknown;
    shortcutFailed: (action: string) => void;
    submitCommand: (command: WorldCommand, label: string) => boolean;
    invalidate: () => void;
    scheduleDispatch: () => void;
    persistFloatIntent: () => void;
    // Live frame read for the unfloat capture: a user-moved/resized float
    // carries its live rect so the next float restores it. Null (unreadable)
    // fails the unfloat closed.
    readLiveRect: (ref: object) => { x: number; y: number; w: number; h: number } | null;
    // True while a DescribeWorld flight or a queued command is outstanding:
    // toggles fail closed until it settles.
    isWorldBusy: () => boolean;
    isFloating: (win: string) => boolean;
    noteFloating: (win: string) => void;
    clearFloating: (win: string) => void;
    activeWindowId: () => string | null;
    activeWindowRef: () => object | null;
}

export interface WorldNativeToggles {
    readonly submitFloat: (label: string) => boolean;
    readonly setStickyNative: (ref: object, id: string, sticky: boolean) => boolean;
    readonly setMaximizedNative: (ref: object, maximized: boolean) => boolean;
    readonly setFullscreenNative: (ref: object, fullscreen: boolean) => boolean;
}

// Exact KWin reads with the offline lowercase fallback only when the
// native property is absent (mirrors the world observation seam; tests
// use real-property fakes so the fallback never masks a regression).
function readMaximized(readProp: WorldNativeEnv["readProp"], ref: object): boolean | null {
    try {
        const mode = readProp(ref, "maximizeMode");
        if (typeof mode === "number") {
            return mode !== 0;
        }
        const legacy = readProp(ref, "maximized");
        return typeof legacy === "boolean" ? legacy : null;
    } catch (error) {
        void error;
        return null;
    }
}

function readFullscreen(readProp: WorldNativeEnv["readProp"], ref: object): boolean | null {
    try {
        const native = readProp(ref, "fullScreen");
        if (typeof native === "boolean") {
            return native;
        }
        const legacy = readProp(ref, "fullscreen");
        return typeof legacy === "boolean" ? legacy : null;
    } catch (error) {
        void error;
        return null;
    }
}

function readSticky(readProp: WorldNativeEnv["readProp"], ref: object): boolean | null {
    try {
        const native = readProp(ref, "onAllDesktops");
        if (typeof native === "boolean") {
            return native;
        }
        const legacy = readProp(ref, "sticky");
        return typeof legacy === "boolean" ? legacy : null;
    } catch (error) {
        void error;
        return null;
    }
}

// Exact KWin maximize write through the Q_INVOKABLE (never a fake
// `maximized` property): missing method fails closed; the caller proves
// through the `maximizeMode` readback.
function invokeMaximize(readProp: WorldNativeEnv["readProp"], ref: object, maximized: boolean): boolean {
    try {
        const method = readProp(ref, "setMaximize");
        if (typeof method !== "function") {
            return false;
        }
        Reflect.apply(method as (...a: ReadonlyArray<unknown>) => unknown, ref, [maximized, maximized]);
        return true;
    } catch (error) {
        void error;
        return false;
    }
}

export function createWorldNativeToggles(env: WorldNativeEnv): WorldNativeToggles {
    const { surface, log, token, readProp } = env;
    // Sticky origins: floating membership before a sticky-on, plus
    // keepAbove/keepBelow before it, restored on sticky-off.
    const stickyPrevFloating = new Map<string, boolean>();
    const stickyPrevAbove = new Map<string, { above: boolean; below: boolean }>();

    // Fence outstanding world applies BEFORE setters (no races): an
    // in-flight DescribeWorld or a queued command fails the toggle closed
    // with one bounded line; the caller retries after the flight settles.
    const worldApplyFenced = (label: string): boolean => {
        if (env.isWorldBusy()) {
            log(`omnitiler:world:native-fenced label=${token(label)} reason=world-apply-in-flight`);
            return true;
        }
        return false;
    };

    const isListed = (ref: object): boolean => {
        try {
            const fn = readProp(surface, "windowList");
            if (typeof fn !== "function") {
                return false;
            }
            const list = Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []) as ReadonlyArray<unknown>;
            return Array.isArray(list) && (list as ReadonlyArray<unknown>).includes(ref);
        } catch (error) {
            void error;
            return false;
        }
    };

    // Float toggle with B9 maximized-unfloat parity (native only). Fence
    // order is load-bearing: busy fence, then the overlay recheck, then
    // the maximize clear (when a floating maximized window needs it), and
    // only then the toggle-float operation. Rect capture follows legacy
    // frame semantics: float selects the session-retained placement (the
    // request carries no rect, so the session never recomputes a center
    // for a window that has floated before); unfloat carries the live
    // frame rect so a user-moved/resized float is retained for the next
    // float. A failed clear or an unreadable unfloat frame refuses with no
    // command; a cleared maximize proves through the readback.
    const submitFloat = (label: string): boolean => {
        const w = env.activeWindowId();
        const ref = env.activeWindowRef();
        if (w === null || ref === null) {
            env.shortcutFailed(label);
            return false;
        }
        if (worldApplyFenced("float")) {
            env.shortcutFailed(label);
            return false;
        }
        const fullscreen = readFullscreen(readProp, ref);
        if (fullscreen === true) {
            log(`omnitiler:world:float-refused-overlay window=${token(w)}`);
            env.shortcutFailed(label);
            return false;
        }
        const floating = env.isFloating(w);
        const maximized = readMaximized(readProp, ref);
        if (floating && maximized === true && fullscreen === false) {
            log(`omnitiler:world:float-unfloat-maximize-clear window=${token(w)} outcome=issued`);
            env.invalidate();
            if (!invokeMaximize(readProp, ref, false)) {
                log(`omnitiler:world:float-refused-maximize-clear window=${token(w)}`);
                env.shortcutFailed(label);
                return false;
            }
            env.scheduleDispatch();
            if (readMaximized(readProp, ref) !== false) {
                log(`omnitiler:world:float-refused-maximize-clear window=${token(w)}`);
                env.shortcutFailed(label);
                return false;
            }
        }
        // Unfloat carries the live frame; float carries no rect (session
        // retained placement wins, never a recomputed center).
        let rect: [number, number, number, number] | null = null;
        if (floating) {
            let live: { x: number; y: number; w: number; h: number } | null = null;
            try {
                live = env.readLiveRect(ref);
            } catch (error) {
                void error;
                live = null;
            }
            if (live === null) {
                log(`omnitiler:world:float-refused-unreadable window=${token(w)}`);
                env.shortcutFailed(label);
                return false;
            }
            rect = [live.x, live.y, live.w, live.h];
        }
        if (!env.submitCommand({ op: "operation", command: { op: "toggle-float", win: w, rect } }, "float")) {
            env.shortcutFailed(label);
            return false;
        }
        return true;
    };

    const setStickyNative = (ref: object, id: string, sticky: boolean): boolean => {
        try {
            if (worldApplyFenced("sticky")) {
                return false;
            }
            if (!isListed(ref)) {
                return false;
            }
            if (readFullscreen(readProp, ref) === true || readMaximized(readProp, ref) === true) {
                log(`omnitiler:world:sticky-refused-overlay window=${token(id)}`);
                return false;
            }
            if (sticky) {
                if (readSticky(readProp, ref) === true) {
                    return true;
                }
                stickyPrevFloating.set(id, env.isFloating(id));
                const above = readProp(ref, "keepAbove");
                const below = readProp(ref, "keepBelow");
                if (typeof above === "boolean" && typeof below === "boolean") {
                    stickyPrevAbove.set(id, { above, below });
                }
                // Synchronous invalidation BEFORE the first setter so the
                // next full report carries the authoritative observation.
                env.invalidate();
                let invoked = false;
                try {
                    invoked = Reflect.set(ref, "onAllDesktops", true) === true;
                } catch (error) {
                    void error;
                    invoked = false;
                }
                if (typeof above === "boolean") {
                    try {
                        Reflect.set(ref, "keepAbove", true);
                    } catch (error) {
                        void error;
                    }
                }
                // Truthful readback: the setter boolean alone is not proof.
                if (invoked && readSticky(readProp, ref) === true) {
                    env.noteFloating(id);
                    env.scheduleDispatch();
                    env.persistFloatIntent();
                    log(`omnitiler:world:sticky-applied window=${token(id)} target=sticky outcome=applied`);
                    return true;
                }
                log(`omnitiler:world:sticky-refused window=${token(id)} target=sticky outcome=refused`);
                return false;
            }
            if (readSticky(readProp, ref) !== true) {
                return true;
            }
            env.invalidate();
            let invoked = false;
            try {
                invoked = Reflect.set(ref, "onAllDesktops", false) === true;
            } catch (error) {
                void error;
                invoked = false;
            }
            if (!invoked || readSticky(readProp, ref) === true) {
                log(`omnitiler:world:sticky-refused window=${token(id)} target=unsticky outcome=refused`);
                return false;
            }
            const prev = stickyPrevAbove.get(id);
            stickyPrevAbove.delete(id);
            const wasFloating = stickyPrevFloating.get(id);
            stickyPrevFloating.delete(id);
            try {
                if (prev !== undefined) {
                    Reflect.set(ref, "keepAbove", prev.above);
                    try {
                        if (readProp(ref, "keepBelow") !== prev.below) {
                            Reflect.set(ref, "keepBelow", prev.below);
                        }
                    } catch (error) {
                        void error;
                    }
                }
            } catch (error) {
                void error;
            }
            if (wasFloating === false) {
                env.clearFloating(id);
            }
            env.scheduleDispatch();
            env.persistFloatIntent();
            log(`omnitiler:world:sticky-applied window=${token(id)} target=unsticky outcome=applied`);
            return true;
        } catch (error) {
            void error;
            return false;
        }
    };

    const setMaximizedNative = (ref: object, maximized: boolean): boolean => {
        try {
            if (worldApplyFenced("maximize")) {
                return false;
            }
            if (!isListed(ref) || readFullscreen(readProp, ref) === true) {
                return false;
            }
            // Synchronous invalidation BEFORE the first setter.
            env.invalidate();
            if (!invokeMaximize(readProp, ref, maximized)) {
                log(`omnitiler:world:maximize-refused target=${maximized ? "maximized" : "restored"} outcome=refused`);
                return false;
            }
            env.scheduleDispatch();
            // Truthful readback through maximizeMode.
            if (readMaximized(readProp, ref) === maximized) {
                log(`omnitiler:world:maximize-applied target=${maximized ? "maximized" : "restored"} outcome=applied`);
                return true;
            }
            log(`omnitiler:world:maximize-refused target=${maximized ? "maximized" : "restored"} outcome=refused`);
            return false;
        } catch (error) {
            void error;
            return false;
        }
    };

    const setFullscreenNative = (ref: object, fullscreen: boolean): boolean => {
        try {
            if (worldApplyFenced("fullscreen")) {
                return false;
            }
            if (!isListed(ref)) {
                return false;
            }
            // Synchronous invalidation BEFORE the first setter.
            env.invalidate();
            try {
                if (readProp(ref, "fullScreen") === undefined) {
                    return false;
                }
                Reflect.set(ref, "fullScreen", fullscreen);
            } catch (error) {
                void error;
                return false;
            }
            env.scheduleDispatch();
            // Truthful readback: the setter boolean alone is not proof.
            if (readFullscreen(readProp, ref) === fullscreen) {
                log(`omnitiler:world:fullscreen-applied target=${fullscreen ? "fullscreen" : "restored"} outcome=applied`);
                return true;
            }
            log(`omnitiler:world:fullscreen-refused target=${fullscreen ? "fullscreen" : "restored"} outcome=refused`);
            return false;
        } catch (error) {
            void error;
            return false;
        }
    };

    return { submitFloat, setStickyNative, setMaximizedNative, setFullscreenNative };
}

// Native host bindings for the shared apply loop (moved verbatim from the
// production entry): every live read and setter the accepted reply actuates
// through. Minimal injected context is the live surface plus the reflective
// reader; lifecycle state and flight ordering stay owned by the entry, which
// spreads this host into its single applyWorldReply call. No copied state.
export type WorldApplyHost = Omit<ApplyEnv, "state" | "observation" | "reply" | "log">;

export function createWorldApplyHost(
    surface: object,
    readProp: (value: object, property: string) => unknown,
): WorldApplyHost {
    return {
        isStillListed: (ref) => {
            try {
                const fn = readProp(surface, "windowList");
                if (typeof fn !== "function") {
                    return false;
                }
                const list = Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []) as ReadonlyArray<unknown>;
                return Array.isArray(list) && (list as ReadonlyArray<unknown>).includes(ref);
            } catch (error) {
                void error;
                return false;
            }
        },
        isActiveWindow: (ref) => {
            try {
                return readProp(surface, "activeWindow") === ref;
            } catch (error) {
                void error;
                return false;
            }
        },
        readLiveRect: (ref) => {
            try {
                const g = readProp(ref, "frameGeometry") as Record<string, unknown> | undefined;
                if (typeof g !== "object" || g === null) {
                    return null;
                }
                const x = g["x"];
                const y = g["y"];
                const w = g["width"] !== undefined ? g["width"] : g["w"];
                const h = g["height"] !== undefined ? g["height"] : g["h"];
                if (typeof x !== "number" || typeof y !== "number" || typeof w !== "number" || typeof h !== "number") {
                    return null;
                }
                return { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) };
            } catch (error) {
                void error;
                return null;
            }
        },
        readLiveMembership: (ref) => {
            try {
                const outputRaw = readProp(ref, "output");
                const out = typeof outputRaw === "object" && outputRaw !== null
                    ? readProp(outputRaw as object, "name")
                    : null;
                const members = readProp(ref, "desktops");
                let ws: string | null = null;
                if (Array.isArray(members)) {
                    for (const m of members as ReadonlyArray<unknown>) {
                        if (typeof m === "object" && m !== null) {
                            const id = readProp(m as object, "id");
                            if (isOpaqueId(id)) {
                                ws = id as string;
                                break;
                            }
                        }
                    }
                }
                if (typeof out !== "string" || ws === null) {
                    return null;
                }
                return { out, ws };
            } catch (error) {
                void error;
                return null;
            }
        },
        readLiveSafety: (ref) => {
            // Native overlay lifetime at setter time, using
            // the legacy KWin property names (`fullScreen`,
            // `maximizeMode`, `onAllDesktops`, `minimized`)
            // with the offline lowercase aliases as fallback
            // when the native property is absent.
            try {
                const boolOf = (names: ReadonlyArray<string>): boolean | null => {
                    for (const name of names) {
                        const v = readProp(ref, name);
                        if (typeof v === "boolean") {
                            return v;
                        }
                    }
                    return null;
                };
                const modeRaw = readProp(ref, "maximizeMode");
                const maximized = typeof modeRaw === "number"
                    ? modeRaw !== 0
                    : boolOf(["maximized"]);
                return {
                    minimized: boolOf(["minimized"]),
                    fullscreen: boolOf(["fullScreen", "fullscreen"]),
                    maximized,
                    sticky: boolOf(["onAllDesktops", "sticky"]),
                };
            } catch (error) {
                void error;
                return null;
            }
        },
        setGeometry: (ref, rect) => {
            // Setter throws propagate to the apply loop, which
            // holds the attempt ambiguous (a throw may have
            // acted); only an explicit false with fresh proof
            // of no change settles refused.
            const setter = readProp(ref, "frameGeometry");
            void setter;
            return Reflect.set(ref, "frameGeometry", { x: rect.x, y: rect.y, width: rect.w, height: rect.h });
        },
        setMembership: (ref, wsNativeId, wsRef) => {
            if (wsRef === null) {
                return false;
            }
            void wsNativeId;
            return Reflect.set(ref, "desktops", [wsRef]);
        },
        sendToOutput: (ref, outRef) => {
            if (outRef === null) {
                return false;
            }
            const fn = readProp(surface, "sendClientToScreen");
            if (typeof fn !== "function") {
                return false;
            }
            Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, [ref, outRef]);
            return true;
        },
        resolveOutput: (out) => {
            try {
                const screens = readProp(surface, "screens");
                if (!Array.isArray(screens)) {
                    return null;
                }
                for (const s of screens as ReadonlyArray<unknown>) {
                    if (typeof s === "object" && s !== null && readProp(s as object, "name") === out) {
                        return s as object;
                    }
                }
                return null;
            } catch (error) {
                void error;
                return null;
            }
        },
        resolveDesktop: (wsId) => {
            try {
                const all = readProp(surface, "desktops");
                if (!Array.isArray(all)) {
                    return null;
                }
                for (const d of all as ReadonlyArray<unknown>) {
                    if (typeof d === "object" && d !== null && readProp(d as object, "id") === wsId) {
                        return d as object;
                    }
                }
                return null;
            } catch (error) {
                void error;
                return null;
            }
        },
        switchView: (desktopRef, outRef) => {
            try {
                return writeDesktopForOutput(surface, desktopRef, outRef);
            } catch (error) {
                void error;
                return false;
            }
        },
        createDesktop: () => {
            try {
                const fn = readProp(surface, "createDesktop");
                if (typeof fn !== "function") {
                    return null;
                }
                const created = Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []) as object;
                const id = readProp(created, "id");
                return isOpaqueId(id) ? (id as string) : null;
            } catch (error) {
                void error;
                return null;
            }
        },
        removeDesktop: (nativeId) => {
            try {
                const fn = readProp(surface, "removeDesktop");
                const all = readProp(surface, "desktops");
                if (typeof fn !== "function" || !Array.isArray(all)) {
                    return false;
                }
                for (const d of all as ReadonlyArray<unknown>) {
                    if (typeof d === "object" && d !== null && readProp(d as object, "id") === nativeId) {
                        Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, [d]);
                        return true;
                    }
                }
                return false;
            } catch (error) {
                void error;
                return false;
            }
        },
        focusWindow: (ref) => {
            return Reflect.set(surface, "activeWindow", ref) || false;
        },
        desktopOccupants: (nativeId) => {
            try {
                const fn = readProp(surface, "windowList");
                if (typeof fn !== "function") {
                    return { total: 0, readable: false };
                }
                const list = Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []) as ReadonlyArray<unknown>;
                if (!Array.isArray(list)) {
                    return { total: 0, readable: false };
                }
                let total = 0;
                for (const item of list as ReadonlyArray<unknown>) {
                    if (typeof item !== "object" || item === null) {
                        return { total: 0, readable: false };
                    }
                    const members = readProp(item as object, "desktops");
                    if (!Array.isArray(members)) {
                        return { total: 0, readable: false };
                    }
                    for (const m of members as ReadonlyArray<unknown>) {
                        if (typeof m === "object" && m !== null && readProp(m as object, "id") === nativeId) {
                            total += 1;
                            break;
                        }
                    }
                }
                return { total, readable: true };
            } catch (error) {
                void error;
                return { total: 0, readable: false };
            }
        },
        desktopVisible: (nativeId) => {
            // Per-output current visibility (6.7 helper
            // owned): a desktop is visible when it is current
            // on ANY live output. Legacy-shared hosts read
            // the shared currentDesktop through the same
            // helper. Null when unreadable (keeps).
            try {
                const screens = readProp(surface, "screens");
                if (!Array.isArray(screens) || screens.length === 0) {
                    return null;
                }
                let readable = false;
                for (const s of screens as ReadonlyArray<unknown>) {
                    if (typeof s !== "object" || s === null) {
                        continue;
                    }
                    let cur: { id: string } | null = null;
                    try {
                        cur = readDesktopForOutput(surface, s as object);
                    } catch (error) {
                        void error;
                        cur = null;
                    }
                    if (cur === null) {
                        continue;
                    }
                    readable = true;
                    if (cur.id === nativeId) {
                        return true;
                    }
                }
                return readable ? false : null;
            } catch (error) {
                void error;
                return null;
            }
        },
        desktopById: (nativeId) => {
            try {
                const all = readProp(surface, "desktops");
                if (!Array.isArray(all)) {
                    return null;
                }
                for (const d of all as ReadonlyArray<unknown>) {
                    if (typeof d === "object" && d !== null && readProp(d as object, "id") === nativeId) {
                        return d as object;
                    }
                }
                return null;
            } catch (error) {
                void error;
                return null;
            }
        },
        readLiveView: (out) => {
            // Per-output live view (6.7): the exact current
            // desktop on `out`, or null when unreadable.
            // Follow/view/migration proof uses this, never
            // any-output visibility.
            try {
                const screens = readProp(surface, "screens");
                if (!Array.isArray(screens)) {
                    return null;
                }
                for (const s of screens as ReadonlyArray<unknown>) {
                    if (typeof s !== "object" || s === null) {
                        continue;
                    }
                    if (readProp(s as object, "name") !== out) {
                        continue;
                    }
                    try {
                        const cur = readDesktopForOutput(surface, s as object);
                        return cur === null ? null : cur.id;
                    } catch (error) {
                        void error;
                        return null;
                    }
                }
                return null;
            } catch (error) {
                void error;
                return null;
            }
        },
        minDesktops: 2,
    };
}
