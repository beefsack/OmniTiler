// Native state toggles for the world adapter: float with deferred B9
// maximized-unfloat parity plus sticky/maximize/fullscreen. Each toggle
// fences outstanding world applies BEFORE setters (no races) and reports an
// explicit outcome line; applied writes invalidate synchronously BEFORE the
// first setter so the next full report carries the authoritative
// observation. Sticky choreography keeps legacy parity natively (floating
// origin plus keepAbove/keepBelow); fullscreen/maximized overlays refuse
// for sticky, and game safety wins over parity. Explicit float toggles
// route fullscreen subjects through core (no adapter fullscreen refusal):
// the core owns membership beneath the retained overlay, and D5 forbids
// every subject write (native state, geometry, focus, stacking) while
// fullscreen holds. Setters use the exact KWin names (`setMaximize`,
// `fullScreen`, `onAllDesktops`, `maximizeMode` readback); a successful
// maximize/fullscreen issuance arms a pending attempt confirmed at the
// first fresh accepted observation (never the setter boolean, never a
// same-stack readback). Sticky stays synchronous with immediate readback.
// No workspace policy: core
// converges the tiling consequences from the next full report.

import type { WorldCommand, WorldReplyResult } from "./world-report";
import type { ApplyEnv } from "./world-apply";
import { readDesktopForOutput, writeDesktopForOutput } from "./workspace-native";
import { isOpaqueId } from "./world-seq";
import { decodeList } from "./qml-list";
import { normalizeNativeId } from "./native-id";
import { readNativeAlias } from "./native-alias";
import type { WorldObservation } from "./world-observe";

export interface WorldNativeEnv {
    surface: object;
    log: (message: string) => void;
    token: (value: unknown) => string;
    readProp: (value: object, property: string) => unknown;
    shortcutFailed: (action: string) => void;
    submitCommand: (command: WorldCommand, label: string) => boolean;
    // Settlement-only submission for the deferred B9 continuation: same
    // core command route as submitCommand but bypasses the entry's
    // newer-input cancellation hook so the continuation cannot cancel
    // itself. All other callers use submitCommand.
    submitSettledCommand: (command: WorldCommand, label: string) => boolean;
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
    // Stable-id live ref resolution (renewed wrappers per enumeration):
    // re-resolves the current wrapper for a canonical id, never ref identity.
    liveRefById: (id: string) => object | null;
}

export interface WorldNativeToggles {
    readonly submitFloat: (label: string) => boolean;
    readonly setStickyNative: (ref: object, id: string, sticky: boolean) => boolean;
    readonly setMaximizedNative: (ref: object, maximized: boolean, label?: string) => boolean;
    readonly setFullscreenNative: (ref: object, fullscreen: boolean, label?: string) => boolean;
    // Authorized maximize for a Planner toggle-maximize result. True on
    // successful issuance (pending the first fresh accepted observation
    // boundary); false on genuine pre-write fences and on genuine
    // invocation failure. Never same-stack readback, never retried. No
    // geometry/focus/stack writes.
    readonly applyAuthorizedMaximize: (result: Extract<WorldReplyResult, { op: "maximize" }>) => boolean;
    // B9 pending-unfloat settlement probe (entry-driven, exactly once on
    // the first accepted full-state report after the issued clear): the
    // first fresh report is the settlement boundary. A cleared window
    // still maximized, unreadable, refused, refocused, relifed,
    // re-scoped, fullscreened, defloated, busy, or mid-gesture terminates
    // the attempt narrowly; a later explicit press may retry. Never
    // retries setters and never decides tiling (core owns the command
    // through submitSettledCommand).
    readonly pollPendingUnfloat: (fresh: WorldObservation, gesture: boolean) => void;    // Terminates a pending attempt. Omitted cause drops silently (stop);
    // "newer-input" logs a supersede outcome without failing the old
    // label; any other cause fails that label narrowly.
    readonly cancelPendingUnfloat: (cause?: string) => void;
    // Pending native-state settlement (maximize/fullscreen): each setter
    // issues exactly once, then confirms exactly once at the first fresh
    // accepted observation boundary after issuance, never on the capture
    // that issued it. Successful issuance returns pending-compatible
    // true; genuine invocation failure returns false. A settled boundary
    // logs applied; a silent no-op refusal fails the caller label once.
    // One native attempt only, no retries/replay/timers. Stable
    // canonical ids only, scope from real output/desktops, accepted
    // observation authoritative. New commands supersede silently.
    readonly pollPendingNativeState: (fresh: WorldObservation, gesture: boolean) => void;
    readonly cancelPendingNativeState: (cause?: string) => void;
    // Automatic fixed-size float stacking (option A): project-owned
    // keepAbove above tiles, driven per accepted report with the fresh
    // observation. No geometry or focus writes, never a keepBelow write.
    readonly syncAutomaticFloatStacking: (fresh: WorldObservation) => void;
    // Releases project-owned keepAbove for one window (explicit unfloat
    // tile path). Closed/replaced references take zero cleanup writes;
    // an already-cleared above (later keepBelow exclusivity included)
    // releases with zero writes.
    readonly releaseAutomaticFloat: (id: string, reason: string) => void;
    // Best-effort release of all owned keepAbove on explicit stop only
    // (handle.stop(); the actual KWin script unload path is unharnessed
    // and makes no claim through this function).
    readonly releaseAllAutomaticFloats: (reason: string) => void;
}

// Exact KWin reads with the offline lowercase fallback only when the
// native read is absent or type-invalid (mirrors the world observation
// seam; production fakes prefer real-property names and dedicated alias
// tests pin the fallback precedence).
function readMaximized(readProp: WorldNativeEnv["readProp"], ref: object): boolean | null {
    try {
        const mode = readProp(ref, "maximizeMode");
        if (typeof mode === "number") {
            return mode !== 0;
        }
        const legacy = readNativeAlias(readProp, ref, "maximized");
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
        const legacy = readNativeAlias(readProp, ref, "fullscreen");
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
        const legacy = readNativeAlias(readProp, ref, "sticky");
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

// KWin script lists (`windowList`, `screens`, `desktops`, per-window
// `desktops`) are QML array-likes, not JS arrays: raw Array.isArray is
// false on the live host while the observer decodes them with decodeList,
// so every raw-Array.isArray liveness read failed closed there and every
// accepted world reply applied as an all-zero silent no-op. Bounds mirror
// the observer caps. Wrapper identity across calls is unstable (fresh
// wrappers per enumeration), so liveness compares the stable opaque
// internalId, never ref identity.
const MAX_NATIVE_SCREENS = 16;
const MAX_NATIVE_DESKTOPS = 4096;
const MAX_NATIVE_WINDOWS = 10000;
const MAX_NATIVE_WINDOW_DESKTOPS = 4096;

type NativeReadProp = WorldNativeEnv["readProp"];

export function liveWindowId(readProp: NativeReadProp, ref: object): string | null {
    // Canonical native-id form (braced UUIDs unwrap to bare opaque ids),
    // shared with the observer: raw live ids are never compared directly.
    try {
        return normalizeNativeId(readProp(ref, "internalId"));
    } catch (error) {
        void error;
        return null;
    }
}

export function isLiveWindowListed(surface: object, readProp: NativeReadProp, ref: object): boolean {
    const id = liveWindowId(readProp, ref);
    if (id === null) {
        return false;
    }
    try {
        const fn = readProp(surface, "windowList");
        if (typeof fn !== "function") {
            return false;
        }
        const list = decodeList(Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []), MAX_NATIVE_WINDOWS);
        if (list === null) {
            return false;
        }
        for (const item of list) {
            if (typeof item === "object" && item !== null && liveWindowId(readProp, item) === id) {
                return true;
            }
        }
        return false;
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
    // Automatic fixed-size float stacking (option A): project-owned
    // keepAbove above tiles. Win-keyed ownership with no stored refs;
    // every write goes through the freshly resolved live ref after a
    // live overlay recheck, so stale captures never write. Preexisting
    // below admits nothing; preexisting above stays untouched/unowned.
    const autoOwned = new Set<string>();
    const autoNoReacquire = new Set<string>();
    const autoSkipNoted = new Set<string>();

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
        // Stable-id liveness: the scripting engine renews wrappers per
        // enumeration, so ref identity never survives a call.
        return isLiveWindowListed(surface, readProp, ref);
    };

    // B9 deferred maximized-unfloat (D1): the native `setMaximize`
    // settles asynchronously on the live host (`maximizeMode` 3->0
    // arrives after the setter returns), so an immediate same-stack
    // readback always misreads still-maximized and discards the live
    // settlement. Clear exactly once, then terminate on the FIRST fresh
    // accepted full-state report after the clear (the existing
    // invalidate+scheduleDispatch already schedules that observation; no
    // new timers, no retries, no per-window signal listening). An
    // unconfirmed clear ends the attempt and a later explicit press may
    // retry. No TS tiling policy: the core owns admission through the
    // submitted command. Stable canonical ids only, never ref identity
    // (wrappers renew per enumeration).
    let pendingUnfloat: { win: string; label: string; scope: string } | null = null;

    // Pending native-state settlement (maximize/fullscreen/authorized):
    // the same first-fresh-boundary discipline as B9 above. Armed after
    // successful issuance with the caller label plus the wanted flags;
    // the entry polls on the first accepted full-state report AFTER the
    // arming capture (poll runs before new issuance there, so the same
    // capture never confirms). Scope fences to the arming capture.
    let pendingNative: {
        win: string;
        label: string;
        wantMaximized: boolean | null;
        wantFullscreen: boolean | null;
        scope: string;
    } | null = null;

    // Live scope (`out/ws`) of a ref through the exact KWin names,
    // mirroring the apply host: null when unreadable (arm refuses).
    const readScopeOf = (ref: object): string | null => {
        try {
            const outputRaw = readProp(ref, "output");
            const out = typeof outputRaw === "object" && outputRaw !== null
                ? readProp(outputRaw as object, "name")
                : null;
            const members = decodeList(readProp(ref, "desktops"), MAX_NATIVE_WINDOW_DESKTOPS);
            if (typeof out !== "string" || members === null) {
                return null;
            }
            for (const m of members) {
                if (typeof m === "object" && m !== null) {
                    const id = readProp(m as object, "id");
                    if (isOpaqueId(id)) {
                        return `${out}/${id as string}`;
                    }
                }
            }
            return null;
        } catch (error) {
            void error;
            return null;
        }
    };

    const dropPending = (): void => {
        pendingUnfloat = null;
    };

    const cancelPending = (cause: string): void => {
        const pending = pendingUnfloat;
        if (pending === null) {
            return;
        }
        dropPending();
        log(`omnitiler:world:float-refused-maximize-clear window=${token(pending.win)} cause=${cause}`);
        env.shortcutFailed(pending.label);
    };

    const cancelPendingSilent = (cause: string): void => {
        const pending = pendingUnfloat;
        if (pending === null) {
            return;
        }
        const win = pending.win;
        dropPending();
        log(`omnitiler:world:float-unfloat-maximize-clear window=${token(win)} outcome=${cause}`);
    };

    // First-report settlement boundary (called once with the first
    // accepted full-state report after the issued clear; entry calls it
    // at most once per armed attempt because every outcome below drops
    // the pending state). Scope comes from the actual current capture;
    // overlays, maximize, and frames read live.
    const pollPendingUnfloat = (fresh: WorldObservation, gesture: boolean): void => {
        const pending = pendingUnfloat;
        if (pending === null) {
            return;
        }
        const win = pending.win;
        dropPending();
        const refuse = (cause: string): void => {
            log(`omnitiler:world:float-refused-maximize-clear window=${token(win)} cause=${cause}`);
            env.shortcutFailed(pending.label);
        };
        if (env.isWorldBusy()) {
            refuse("busy");
            return;
        }
        if (gesture) {
            refuse("gesture-raced");
            return;
        }
        if (env.activeWindowId() !== win) {
            refuse("focus-raced");
            return;
        }
        let live: object | null = null;
        try {
            live = env.liveRefById(win);
        } catch (error) {
            void error;
            live = null;
        }
        if (live === null || !isListed(live)) {
            refuse("observe-missing");
            return;
        }
        let scope: string | null = null;
        for (const domain of fresh.domains) {
            for (const entry of domain.windows) {
                if (entry.win === win) {
                    scope = `${domain.id}/${entry.ws}`;
                    break;
                }
            }
            if (scope !== null) {
                break;
            }
        }
        if (scope === null) {
            refuse("observe-missing");
            return;
        }
        if (scope !== pending.scope) {
            refuse("scope-raced");
            return;
        }
        if (readFullscreen(readProp, live) === true) {
            refuse("fullscreen-raced");
            return;
        }
        if (!env.isFloating(win)) {
            refuse("floating-lost");
            return;
        }
        const maximized = readMaximized(readProp, live);
        if (maximized !== false) {
            refuse(maximized === null ? "observe-missing" : "still-maximized");
            return;
        }
        let frame: { x: number; y: number; w: number; h: number } | null = null;
        try {
            frame = env.readLiveRect(live);
        } catch (error) {
            void error;
            frame = null;
        }
        if (frame === null) {
            refuse("unreadable");
            return;
        }
        const rect: [number, number, number, number] = [frame.x, frame.y, frame.w, frame.h];
        if (!env.submitSettledCommand(
            { op: "operation", command: { op: "toggle-float", win, rect } },
            "float",
        )) {
            refuse("submit-refused");
            return;
        }
        log(`omnitiler:world:float-unfloat-maximize-clear window=${token(win)} outcome=observed-cleared`);
    };

    const cancelPendingUnfloat = (cause?: string): void => {
        if (pendingUnfloat === null) {
            return;
        }
        if (cause === undefined) {
            dropPending();
            return;
        }
        if (cause === "newer-input") {
            cancelPendingSilent("newer-input-superseded");
            return;
        }
        cancelPending(cause);
    };

    // Pending native-state settlement helpers. Supersession is silent
    // (the newer press owns its own outcome); terminal/lifecycle
    // failures fail the arming label once, exactly like B9 above.
    const dropPendingNative = (): void => {
        pendingNative = null;
    };

    const supersedePendingNative = (): void => {
        const pending = pendingNative;
        if (pending === null) {
            return;
        }
        dropPendingNative();
        log(`omnitiler:world:native-pending-superseded window=${token(pending.win)} outcome=newer-input-superseded`);
    };

    const cancelPendingNativeState = (cause?: string): void => {
        const pending = pendingNative;
        if (pending === null) {
            return;
        }
        if (cause === undefined) {
            dropPendingNative();
            return;
        }
        if (cause === "newer-input") {
            supersedePendingNative();
            return;
        }
        dropPendingNative();
        const target = pending.wantFullscreen !== null && pending.wantMaximized !== null
            ? "maximized"
            : pending.wantFullscreen !== null
                ? (pending.wantFullscreen ? "fullscreen" : "restored")
                : (pending.wantMaximized === true ? "maximized" : "restored");
        const kind = pending.wantFullscreen !== null && pending.wantMaximized === null
            ? "fullscreen"
            : "maximize";
        log(`omnitiler:world:${kind}-refused target=${target} window=${token(pending.win)} reason=${cause}`);
        env.shortcutFailed(pending.label);
    };

    // First-report settlement boundary for an issued native setter.
    // Consumes exactly once; every fence terminates narrowly with the
    // arming label failed once, except supersession (which never reaches
    // here: newer input drops silently before the boundary).
    const pollPendingNativeState = (fresh: WorldObservation, gesture: boolean): void => {
        const pending = pendingNative;
        if (pending === null) {
            return;
        }
        const win = pending.win;
        const label = pending.label;
        dropPendingNative();
        const kind = pending.wantFullscreen !== null && pending.wantMaximized === null
            ? "fullscreen"
            : "maximize";
        const target = pending.wantFullscreen !== null && pending.wantMaximized !== null
            ? "maximized"
            : pending.wantFullscreen !== null
                ? (pending.wantFullscreen ? "fullscreen" : "restored")
                : (pending.wantMaximized === true ? "maximized" : "restored");
        const refuse = (cause: string): void => {
            log(`omnitiler:world:${kind}-refused target=${target} window=${token(win)} reason=${cause}`);
            env.shortcutFailed(label);
        };
        if (env.isWorldBusy()) {
            refuse("busy");
            return;
        }
        if (gesture) {
            refuse("gesture-raced");
            return;
        }
        if (env.activeWindowId() !== win) {
            refuse("focus-raced");
            return;
        }
        // Live focus ref must resolve to the same canonical win (mirrors
        // the applyAuthorizedMaximize focus fence); a probe failure fails
        // closed with no writes.
        let focused: object | null = null;
        try {
            focused = env.activeWindowRef();
        } catch (error) {
            void error;
            focused = null;
        }
        if (focused === null || liveWindowId(readProp, focused) !== win) {
            refuse("focus-raced");
            return;
        }
        let live: object | null = null;
        try {
            live = env.liveRefById(win);
        } catch (error) {
            void error;
            live = null;
        }
        if (live === null || liveWindowId(readProp, live) !== win || !isListed(live)) {
            refuse("observe-missing");
            return;
        }
        // A held or incomplete capture is never authoritative: consume
        // once and refuse narrowly, never wait or retry.
        if (fresh.complete !== true) {
            refuse("observation-held");
            return;
        }
        let scope: string | null = null;
        let domainHeld = false;
        for (const domain of fresh.domains) {
            for (const entry of domain.windows) {
                if (entry.win === win) {
                    scope = `${domain.id}/${entry.ws}`;
                    domainHeld = domain.hold !== "" || domain.readable !== true || domain.complete !== true;
                    break;
                }
            }
            if (scope !== null) {
                break;
            }
        }
        if (scope === null) {
            refuse("observe-missing");
            return;
        }
        if (domainHeld) {
            refuse("observation-held");
            return;
        }
        if (scope !== pending.scope) {
            refuse("scope-raced");
            return;
        }
        // Topology must completely cover the target output; a missing or
        // incomplete topology entry is held, never authoritative.
        let topoOk = false;
        for (const t of fresh.topology) {
            if (t.out === scope.slice(0, scope.indexOf("/"))) {
                topoOk = t.complete === true;
                break;
            }
        }
        if (!topoOk) {
            refuse("observation-held");
            return;
        }
        // Re-read the live scope at settlement: applyWorldReply may have
        // moved the window after issuance. Unreadable or mismatched live
        // scope rejects in addition to the capture scope above.
        const liveScope = readScopeOf(live);
        if (liveScope === null) {
            refuse("observe-missing");
            return;
        }
        if (liveScope !== pending.scope) {
            refuse("scope-raced");
            return;
        }
        const liveFullscreen = readFullscreen(readProp, live);
        const liveMaximized = readMaximized(readProp, live);
        if (pending.wantFullscreen !== null && pending.wantMaximized !== null) {
            if (liveFullscreen === null || liveMaximized === null) {
                refuse("observe-missing");
                return;
            }
            if (liveFullscreen === pending.wantFullscreen && liveMaximized === pending.wantMaximized) {
                log(`omnitiler:world:maximize-applied target=maximized window=${token(win)} clear=fullscreen`);
                return;
            }
            refuse("silent-refusal");
            return;
        }
        if (pending.wantFullscreen !== null) {
            if (liveFullscreen === null) {
                refuse("observe-missing");
                return;
            }
            if (liveFullscreen === pending.wantFullscreen) {
                log(`omnitiler:world:fullscreen-applied target=${target} window=${token(win)} outcome=applied`);
                return;
            }
            refuse("silent-refusal");
            return;
        }
        if (liveMaximized === null) {
            refuse("observe-missing");
            return;
        }
        // An ordinary maximize racing into fullscreen is a race, not a
        // silent host no-op: report it narrowly like the B9 guard.
        if (liveFullscreen === true) {
            refuse("fullscreen-raced");
            return;
        }
        if (liveMaximized === pending.wantMaximized) {
            log(`omnitiler:world:maximize-applied target=${target} window=${token(win)} outcome=applied`);
            return;
        }
        refuse("silent-refusal");
    };

    // Float toggle with B9 maximized-unfloat parity (native only). Fence
    // order is load-bearing: busy fence, then the maximize clear (when a
    // floating maximized non-fullscreen window needs it), and only then
    // the toggle-float operation. Fullscreen never refuses: explicit
    // intent routes through core, which owns base membership beneath the
    // retained overlay (siblings reflow, subject takes no writes per D5).
    // Rect capture follows legacy frame semantics, except fullscreen
    // never captures the observed fullscreen frame as a retained float
    // frame: float selects the session-retained placement (the request
    // carries no rect, so the session never recomputes a center for a
    // window that has floated before); unfloat carries the live frame
    // rect so a user-moved/resized float is retained for the next float,
    // but a fullscreen subject carries no rect so the durable placement
    // survives the round trip. A refused setter fails closed with no
    // command; an issued clear defers the command until observed
    // settlement (no same-stack readback: async KWin settlement arrives
    // after the setter returns).
    const submitFloat = (label: string): boolean => {
        const w = env.activeWindowId();
        const ref = env.activeWindowRef();
        if (w === null || ref === null) {
            if (pendingUnfloat !== null) {
                cancelPendingSilent("newer-input-superseded");
            }
            supersedePendingNative();
            env.shortcutFailed(label);
            return false;
        }
        if (worldApplyFenced("float")) {
            if (pendingUnfloat !== null) {
                cancelPendingSilent("newer-input-superseded");
            }
            supersedePendingNative();
            env.shortcutFailed(label);
            return false;
        }
        const fullscreen = readFullscreen(readProp, ref);
        const floating = env.isFloating(w);
        const maximized = readMaximized(readProp, ref);
        if (floating && maximized === true && fullscreen === false) {
            if (pendingUnfloat !== null) {
                cancelPendingSilent("newer-input-superseded");
            }
            supersedePendingNative();
            log(`omnitiler:world:float-unfloat-maximize-clear window=${token(w)} outcome=issued`);
            env.invalidate();
            let invoked = false;
            try {
                invoked = invokeMaximize(readProp, ref, false);
            } catch (error) {
                void error;
                invoked = false;
            }
            if (!invoked) {
                log(`omnitiler:world:float-refused-maximize-clear window=${token(w)} cause=native-write-refused`);
                env.shortcutFailed(label);
                return false;
            }
            // Arm scope fences the continuation to the current capture:
            // an unreadable scope refuses immediately (unobserved).
            const scope = readScopeOf(ref);
            if (scope === null) {
                log(`omnitiler:world:float-refused-maximize-clear window=${token(w)} cause=observe-missing`);
                env.shortcutFailed(label);
                return false;
            }
            env.scheduleDispatch();
            pendingUnfloat = { win: w, label, scope };
            return true;
        }
        if (pendingUnfloat !== null) {
            cancelPendingSilent("newer-input-superseded");
        }
        supersedePendingNative();
        // Unfloat carries the live frame, except a fullscreen subject
        // carries no rect (D5: the observed fullscreen frame is never the
        // retained float frame); float carries no rect (session retained
        // placement wins, never a recomputed center).
        let rect: [number, number, number, number] | null = null;
        if (floating && fullscreen !== true) {
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
            // Newer direct input supersedes a pending native settlement
            // even when this press later refuses (registered shortcuts
            // bypass the entry cancel hook).
            supersedePendingNative();
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

    const setMaximizedNative = (ref: object, maximized: boolean, label = "maximize"): boolean => {
        try {
            // Newer direct input supersedes any pending settlement first
            // (the new press owns its own outcome below).
            supersedePendingNative();
            if (worldApplyFenced("maximize")) {
                return false;
            }
            if (!isListed(ref) || readFullscreen(readProp, ref) === true) {
                return false;
            }
            const win = liveWindowId(readProp, ref);
            if (win === null) {
                return false;
            }
            // Synchronous invalidation BEFORE the first setter.
            env.invalidate();
            if (!invokeMaximize(readProp, ref, maximized)) {
                log(`omnitiler:world:maximize-refused target=${maximized ? "maximized" : "restored"} window=${token(win)} reason=native-write-refused`);
                return false;
            }
            // Arm scope fences the continuation to the current capture:
            // an unreadable scope refuses immediately (unobserved). No
            // same-stack readback: async settlement arrives after return.
            const scope = readScopeOf(ref);
            if (scope === null) {
                log(`omnitiler:world:maximize-refused target=${maximized ? "maximized" : "restored"} window=${token(win)} reason=observe-missing`);
                return false;
            }
            env.scheduleDispatch();
            pendingNative = { win, label, wantMaximized: maximized, wantFullscreen: null, scope };
            log(`omnitiler:world:maximize-pending target=${maximized ? "maximized" : "restored"} window=${token(win)} outcome=issued`);
            return true;
        } catch (error) {
            void error;
            return false;
        }
    };

    const setFullscreenNative = (ref: object, fullscreen: boolean, label = "toggle-fullscreen"): boolean => {
        try {
            supersedePendingNative();
            if (worldApplyFenced("fullscreen")) {
                return false;
            }
            if (!isListed(ref)) {
                return false;
            }
            const win = liveWindowId(readProp, ref);
            if (win === null) {
                return false;
            }
            // Synchronous invalidation BEFORE the first setter.
            env.invalidate();
            let invoked = false;
            try {
                if (readProp(ref, "fullScreen") === undefined) {
                    return false;
                }
                invoked = Reflect.set(ref, "fullScreen", fullscreen);
            } catch (error) {
                void error;
                return false;
            }
            if (!invoked) {
                log(`omnitiler:world:fullscreen-refused target=${fullscreen ? "fullscreen" : "restored"} window=${token(win)} reason=native-write-refused`);
                return false;
            }
            const scope = readScopeOf(ref);
            if (scope === null) {
                log(`omnitiler:world:fullscreen-refused target=${fullscreen ? "fullscreen" : "restored"} window=${token(win)} reason=observe-missing`);
                return false;
            }
            env.scheduleDispatch();
            pendingNative = { win, label, wantMaximized: null, wantFullscreen: fullscreen, scope };
            log(`omnitiler:world:fullscreen-pending target=${fullscreen ? "fullscreen" : "restored"} window=${token(win)} outcome=issued`);
            return true;
        } catch (error) {
            void error;
            return false;
        }
    };

    // Authorized maximize for the Planner toggle-maximize result. Fresh
    // identity/ref/list/focus/source-flag fences run before any write so no
    // stale accepted capture can actuate; worldApplyFenced runs once before
    // writes. Ordinary results delegate to setMaximizedNative (setMaximize
    // bool/bool only, never a fullscreen property write). The fullscreen
    // branch clears first through the exact KWin name (`fullScreen`) then
    // invokes maximize back-to-back with NO applied readback required
    // between the setters: script `fullScreen` reads the APPLIED flag
    // (window.h:332) while the WRITE setter flips the REQUESTED flag
    // synchronously (xdgshellwindow.cpp:1737), which is what isResizable()
    // reads (:682) and maximize() requires (:1773) before staging
    // m_nextStates plus scheduleConfigure (:879, :890). Applied flags
    // update only on client ack (:1080), so an applied-full readback
    // between setters would always abort on Wayland; the legacy
    // fullscreen guard is NOT reused here. Same-requested-mode maximize
    // is a host no-op, so both-flags clears full and retains max. Tags v6.7.4 (same mechanism at v6.8.1):
    // https://invent.kde.org/plasma/kwin/-/blob/v6.7.4/src/xdgshellwindow.cpp
    // https://invent.kde.org/plasma/kwin/-/blob/v6.7.4/src/window.h
    const applyAuthorizedMaximize = (result: Extract<WorldReplyResult, { op: "maximize" }>): boolean => {
        try {
            const target = result.maximized ? "maximized" : "restored";
            if (env.activeWindowId() !== result.win) {
                log(`omnitiler:world:maximize-refused target=${target} window=${token(result.win)} reason=focus-raced`);
                return false;
            }
            let live: object | null = null;
            try {
                live = env.liveRefById(result.win);
            } catch (error) {
                void error;
                live = null;
            }
            if (live === null || liveWindowId(readProp, live) !== result.win || !isListed(live)) {
                log(`omnitiler:world:maximize-refused target=${target} window=${token(result.win)} reason=observe-missing`);
                return false;
            }
            let focused: object | null = null;
            try {
                focused = env.activeWindowRef();
            } catch (error) {
                void error;
                focused = null;
            }
            if (focused === null || liveWindowId(readProp, focused) !== result.win) {
                log(`omnitiler:world:maximize-refused target=${target} window=${token(result.win)} reason=focus-raced`);
                return false;
            }
            // Source preimage: clearFullscreen IS the source fullscreen.
            const sourceFullscreen = readFullscreen(readProp, live);
            const sourceMaximized = readMaximized(readProp, live);
            if (sourceFullscreen === null || sourceMaximized === null ||
                sourceFullscreen !== result.clearFullscreen || sourceMaximized !== result.wasMaximized) {
                log(`omnitiler:world:maximize-refused target=${target} window=${token(result.win)} reason=source-raced`);
                return false;
            }
            if (!result.clearFullscreen) {
                return setMaximizedNative(live, result.maximized, "maximize");
            }
            // A newer authorized result supersedes any pending settlement;
            // the old press ends silently, the new one arms below.
            supersedePendingNative();
            if (worldApplyFenced("maximize")) {
                return false;
            }
            // Synchronous invalidation BEFORE the first setter.
            env.invalidate();
            let cleared = false;
            try {
                cleared = Reflect.set(live, "fullScreen", false);
            } catch (error) {
                void error;
                cleared = false;
            }
            if (!cleared) {
                log(`omnitiler:world:maximize-refused target=${target} window=${token(result.win)} reason=clear-refused`);
                return false;
            }
            let invoked = false;
            try {
                invoked = invokeMaximize(readProp, live, result.maximized);
            } catch (error) {
                void error;
                invoked = false;
            }
            if (!invoked) {
                log(`omnitiler:world:maximize-refused target=${target} window=${token(result.win)} reason=native-write-refused`);
                return false;
            }
            // Arm scope fences the continuation to the current capture;
            // unreadable scope refuses immediately. NO applied-fullscreen
            // gate between the back-to-back setters above (requested vs
            // applied flags), and NO same-stack final readback here: the
            // first fresh accepted boundary settles the attempt.
            const scope = readScopeOf(live);
            if (scope === null) {
                log(`omnitiler:world:maximize-refused target=${target} window=${token(result.win)} reason=observe-missing`);
                return false;
            }
            env.scheduleDispatch();
            pendingNative = {
                win: result.win,
                label: "maximize",
                wantMaximized: result.maximized,
                wantFullscreen: false,
                scope,
            };
            log(`omnitiler:world:maximize-pending target=${target} window=${token(result.win)} clear=fullscreen outcome=issued`);
            return true;
        } catch (error) {
            void error;
            return false;
        }
    };

    // Automatic fixed-size float stacking driver (option A). `fresh`
    // carries classification flags only; every stacking write re-reads
    // live overlay state first and resolves a fresh live ref, so a stale
    // capture never writes. A live-fullscreen target takes no write on
    // any path; a scope with live-verified fullscreen focus suspends
    // owned above and admits nothing.
    const resolveAutoLive = (win: string): object | null => {
        let live: object | null = null;
        try {
            live = env.liveRefById(win);
        } catch (error) {
            void error;
            return null;
        }
        if (live === null || !isListed(live)) {
            return null;
        }
        if (liveWindowId(readProp, live) !== win) {
            return null;
        }
        return live;
    };

    // Live overlay recheck immediately before every stacking write
    // (exact KWin names, offline alias fallback). Null or any overlay
    // fails closed with zero writes; ownership is retained for retry.
    // Native hidden aliases into live minimized (immediate eligibility,
    // not native minimization); a missing/unreadable hidden fails closed.
    const readAutoSafety = (ref: object): { fullscreen: boolean; maximized: boolean; minimized: boolean; sticky: boolean } | null => {
        try {
            const fsRaw = readProp(ref, "fullScreen");
            const fullscreen = typeof fsRaw === "boolean" ? fsRaw : readNativeAlias(readProp, ref, "fullscreen");
            const modeRaw = readProp(ref, "maximizeMode");
            const maximized = typeof modeRaw === "number" ? modeRaw !== 0 : readNativeAlias(readProp, ref, "maximized");
            const minimizedRaw = readProp(ref, "minimized");
            const hiddenRaw = readProp(ref, "hidden");
            const stickyRaw = readProp(ref, "onAllDesktops");
            const sticky = typeof stickyRaw === "boolean" ? stickyRaw : readNativeAlias(readProp, ref, "sticky");
            if (typeof fullscreen !== "boolean" || typeof maximized !== "boolean" ||
                typeof minimizedRaw !== "boolean" || typeof hiddenRaw !== "boolean" || typeof sticky !== "boolean") {
                return null;
            }
            return { fullscreen, maximized, minimized: minimizedRaw || hiddenRaw, sticky };
        } catch (error) {
            void error;
            return null;
        }
    };

    const writeAutoAbove = (live: object, win: string, want: boolean, target: string): boolean => {
        const safety = readAutoSafety(live);
        if (safety === null || safety.fullscreen || safety.maximized || safety.minimized || safety.sticky) {
            log(`omnitiler:world:automatic-keep-above window=${token(win)} target=${target} outcome=refused`);
            return false;
        }
        let invoked = false;
        try {
            invoked = Reflect.set(live, "keepAbove", want);
        } catch (error) {
            void error;
            invoked = false;
        }
        const state = readAutoAboveBelow(readProp, live);
        const ok = invoked && state !== null && state.above === want && (want === false || state.below === false);
        log(`omnitiler:world:automatic-keep-above window=${token(win)} target=${target} outcome=${ok ? "applied" : "refused"}`);
        return ok;
    };

    const dropAutoWin = (win: string): void => {
        autoOwned.delete(win);
        autoNoReacquire.delete(win);
        autoSkipNoted.delete(`${win} below`);
        autoSkipNoted.delete(`${win} suspended`);
    };

    const releaseAutomaticFloat = (id: string, reason: string): void => {
        try {
            if (!autoOwned.has(id)) {
                return;
            }
            const live = resolveAutoLive(id);
            if (live === null) {
                dropAutoWin(id);
                return;
            }
            const flags = readAutoAboveBelow(readProp, live);
            if (flags === null || !flags.above) {
                autoOwned.delete(id);
                if (flags !== null && flags.below) {
                    autoNoReacquire.add(id);
                }
                return;
            }
            // A refused write (live overlay) retains ownership for a
            // later retry with zero writes; only success drops it.
            if (writeAutoAbove(live, id, false, `restore-${reason}`)) {
                autoOwned.delete(id);
            }
        } catch (error) {
            void error;
        }
    };

    // Best-effort release of all owned keepAbove on explicit stop only.
    // Safety-guarded restores; refused targets stay owned but unwritten.
    // This covers handle.stop(); the actual KWin script unload path is
    // unharnessed and makes no claim through this function.
    const releaseAllAutomaticFloats = (reason: string): void => {
        try {
            for (const win of [...autoOwned]) {
                const live = resolveAutoLive(win);
                if (live === null) {
                    dropAutoWin(win);
                    continue;
                }
                const flags = readAutoAboveBelow(readProp, live);
                if (flags === null || !flags.above) {
                    autoOwned.delete(win);
                    continue;
                }
                if (writeAutoAbove(live, win, false, `restore-${reason}`)) {
                    autoOwned.delete(win);
                }
            }
            autoNoReacquire.clear();
            autoSkipNoted.clear();
        } catch (error) {
            void error;
        }
    };

    const syncAutomaticFloatStacking = (fresh: WorldObservation): void => {
        try {
            const entries = new Map<string, { entry: WorldObservation["domains"][number]["windows"][number]; scope: string }>();
            for (const domain of fresh.domains) {
                const scope = `${domain.id}/${domain.ws}`;
                for (const w of domain.windows) {
                    if (!entries.has(w.win)) {
                        entries.set(w.win, { entry: w, scope });
                    }
                }
            }
            for (const m of fresh.minimized) {
                if (!entries.has(m.win)) {
                    entries.set(m.win, { entry: m, scope: `${m.out}/${m.ws}` });
                }
            }
            // Suspend scopes with live-verified fullscreen focus; the
            // stale capture is only a fallback when live focus is
            // unreadable. Scopes use the live `out/ws` form with the
            // observation scope as fallback.
            const suspendScopes = new Set<string>();
            let liveFocusRead = false;
            try {
                const liveId = env.activeWindowId();
                if (liveId !== null) {
                    const liveRef = env.liveRefById(liveId);
                    if (liveRef !== null) {
                        const safety = readAutoSafety(liveRef);
                        if (safety !== null) {
                            liveFocusRead = true;
                            if (safety.fullscreen) {
                                const scope = readScopeOf(liveRef) ?? entries.get(liveId)?.scope ?? null;
                                if (scope !== null) {
                                    suspendScopes.add(scope);
                                }
                            }
                        }
                    }
                }
            } catch (error) {
                void error;
            }
            if (!liveFocusRead) {
                const focusedId = fresh.focusedGlobal;
                if (typeof focusedId === "string" && focusedId !== "") {
                    const focused = entries.get(focusedId);
                    if (focused !== undefined && focused.entry.fullscreen === true) {
                        suspendScopes.add(focused.scope);
                    }
                }
            }
            for (const win of [...autoOwned]) {
                const live = resolveAutoLive(win);
                if (live === null) {
                    dropAutoWin(win);
                    continue;
                }
                const flags = readAutoAboveBelow(readProp, live);
                if (flags === null) {
                    continue;
                }
                if (flags.below) {
                    autoOwned.delete(win);
                    autoNoReacquire.add(win);
                    log(`omnitiler:world:automatic-keep-above window=${token(win)} target=release reason=below-wins outcome=released`);
                    continue;
                }
                const current = entries.get(win);
                if (current !== undefined && (current.entry.fixedAuto !== true || current.entry.fixedSuppress === true)) {
                    // Lost automatic classification restores owned above
                    // once; a refused write retains for a later retry.
                    if (flags.above && !writeAutoAbove(live, win, false, "restore-classification-lost")) {
                        continue;
                    }
                    autoOwned.delete(win);
                    continue;
                }
                const scope = readScopeOf(live) ?? current?.scope ?? null;
                if (flags.above && scope !== null && suspendScopes.has(scope)) {
                    // A refused write (live overlay, including a
                    // fullscreen target itself) retains for retry.
                    if (!writeAutoAbove(live, win, false, "restore-suspended-fullscreen")) {
                        continue;
                    }
                    autoOwned.delete(win);
                    continue;
                }
                if (
                    current !== undefined &&
                    (current.entry.sticky === true || current.entry.fullscreen === true ||
                        current.entry.maximized === true || current.entry.minimized === true)
                ) {
                    continue;
                }
                if (!flags.above) {
                    autoOwned.delete(win);
                    autoNoReacquire.add(win);
                    log(`omnitiler:world:automatic-keep-above window=${token(win)} target=release reason=external-clear outcome=released`);
                    continue;
                }
            }
            for (const [win, current] of entries) {
                if (autoOwned.has(win)) {
                    continue;
                }
                const entry = current.entry;
                if (entry.fixedAuto !== true || entry.fixedSuppress === true) {
                    continue;
                }
                if (entry.floating !== true || entry.sticky === true || entry.fullscreen === true ||
                    entry.maximized === true || entry.minimized === true) {
                    continue;
                }
                const live = resolveAutoLive(win);
                if (live === null) {
                    continue;
                }
                const flags = readAutoAboveBelow(readProp, live);
                if (flags === null) {
                    log(`omnitiler:world:automatic-keep-above window=${token(win)} target=above outcome=missing`);
                    continue;
                }
                if (flags.below) {
                    const key = `${win} below`;
                    if (!autoSkipNoted.has(key)) {
                        autoSkipNoted.add(key);
                        log(`omnitiler:world:automatic-keep-above window=${token(win)} target=above outcome=skipped reason=preexisting-below`);
                    }
                    continue;
                }
                if (flags.above) {
                    // Preowned above stays untouched and untracked; an
                    // ended lifetime becomes eligible again below.
                    autoNoReacquire.delete(win);
                    continue;
                }
                if (autoNoReacquire.has(win)) {
                    continue;
                }
                const applicantScope = readScopeOf(live) ?? current.scope;
                if (suspendScopes.has(applicantScope)) {
                    const key = `${win} suspended`;
                    if (!autoSkipNoted.has(key)) {
                        autoSkipNoted.add(key);
                        log(`omnitiler:world:automatic-keep-above window=${token(win)} target=above outcome=skipped reason=fullscreen-sibling`);
                    }
                    continue;
                }
                autoSkipNoted.delete(`${win} below`);
                autoSkipNoted.delete(`${win} suspended`);
                if (writeAutoAbove(live, win, true, "above")) {
                    autoOwned.add(win);
                }
            }
        } catch (error) {
            void error;
        }
    };

    return { submitFloat, setStickyNative, setMaximizedNative, setFullscreenNative, applyAuthorizedMaximize, pollPendingUnfloat, cancelPendingUnfloat, pollPendingNativeState, cancelPendingNativeState, syncAutomaticFloatStacking, releaseAutomaticFloat, releaseAllAutomaticFloats };
}

function readAutoAboveBelow(readProp: WorldNativeEnv["readProp"], ref: object): { above: boolean; below: boolean } | null {
    try {
        const above = readProp(ref, "keepAbove");
        const below = readProp(ref, "keepBelow");
        if (typeof above !== "boolean" || typeof below !== "boolean") {
            return null;
        }
        return { above, below };
    } catch (error) {
        void error;
        return null;
    }
}

// Native host bindings for the shared apply loop (moved verbatim from the
// production entry): every live read and setter the accepted reply actuates
// through. Minimal injected context is the live surface plus the reflective
// reader; lifecycle state and flight ordering stay owned by the entry, which
// spreads this host into its single applyWorldReply call. No copied state.
export type WorldApplyHost = Omit<ApplyEnv, "state" | "observation" | "reply" | "dispatchWatermark" | "takeResult" | "log">;

export function createWorldApplyHost(
    surface: object,
    readProp: (value: object, property: string) => unknown,
): WorldApplyHost {
    return {
        isStillListed: (ref) => {
            // Stable-id liveness (see isLiveWindowListed): ref identity
            // never survives a scripting-engine enumeration.
            return isLiveWindowListed(surface, readProp, ref);
        },
        isActiveWindow: (ref) => {
            try {
                const active = readProp(surface, "activeWindow");
                if (typeof active !== "object" || active === null) {
                    return false;
                }
                const want = liveWindowId(readProp, ref);
                return want !== null && liveWindowId(readProp, active) === want;
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
                const members = decodeList(readProp(ref, "desktops"), MAX_NATIVE_WINDOW_DESKTOPS);
                let ws: string | null = null;
                if (members !== null) {
                    for (const m of members) {
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
            // Native overlay lifetime at setter time, using the exact KWin
            // property names (`fullScreen`, `maximizeMode`, `onAllDesktops`,
            // `minimized`, required native bool `hidden` at pinned v6.7.4):
            // the offline lowercase aliases apply only when
            // the native read is absent or type-invalid, never overriding a
            // well-typed native read. Native hidden aliases into live
            // minimized (immediate eligibility, not native minimization);
            // a missing/unreadable hidden fails the whole result (null),
            // never a false minimized. Reads stay lazy and in native-first
            // order; any throw fails the whole safety result (null), never
            // a single field.
            try {
                const boolOrNull = (value: unknown): boolean | null =>
                    typeof value === "boolean" ? value : null;
                const modeRaw = readProp(ref, "maximizeMode");
                const maximized = typeof modeRaw === "number"
                    ? modeRaw !== 0
                    : boolOrNull(readNativeAlias(readProp, ref, "maximized"));
                const minimizedRaw = readProp(ref, "minimized");
                const hiddenRaw = readProp(ref, "hidden");
                if (typeof hiddenRaw !== "boolean") {
                    return null;
                }
                const minimizedBase = boolOrNull(minimizedRaw);
                const minimized = minimizedBase === null
                    ? (hiddenRaw ? true : null)
                    : (minimizedBase || hiddenRaw);
                const fullScreenRaw = readProp(ref, "fullScreen");
                const fullscreen = boolOrNull(typeof fullScreenRaw === "boolean"
                    ? fullScreenRaw
                    : readNativeAlias(readProp, ref, "fullscreen"));
                const allDesktopsRaw = readProp(ref, "onAllDesktops");
                const sticky = boolOrNull(typeof allDesktopsRaw === "boolean"
                    ? allDesktopsRaw
                    : readNativeAlias(readProp, ref, "sticky"));
                return {
                    minimized,
                    fullscreen,
                    maximized,
                    sticky,
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
                const screens = decodeList(readProp(surface, "screens"), MAX_NATIVE_SCREENS);
                if (screens === null) {
                    return null;
                }
                for (const s of screens) {
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
                const all = decodeList(readProp(surface, "desktops"), MAX_NATIVE_DESKTOPS);
                if (all === null) {
                    return null;
                }
                for (const d of all) {
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
        createDesktop: (position, name) => {
            const fn = readProp(surface, "createDesktop");
            if (typeof fn !== "function") {
                throw new Error("createDesktop-unavailable");
            }
            // KWin 6.7 void API: required zero-based position in [0, count]
            // plus name, returns nothing. Throws (including Insufficient
            // arguments) propagate so the apply loop records the truthful
            // setter and either proves causally same-turn or defers the
            // execution evidence; no return value is ever read.
            Reflect.apply(fn as (...a: ReadonlyArray<unknown>) => unknown, surface, [position, name]);
        },
        listDesktops: () => {
            try {
                const all = decodeList(readProp(surface, "desktops"), MAX_NATIVE_DESKTOPS);
                if (all === null || all.length === 0) {
                    return null;
                }
                const out: Array<{ id: string; name: string }> = [];
                for (const d of all) {
                    if (typeof d !== "object" || d === null) {
                        return null;
                    }
                    const id = readProp(d as object, "id");
                    const name = readProp(d as object, "name");
                    if (!isOpaqueId(id) || typeof name !== "string") {
                        return null;
                    }
                    out.push({ id: id as string, name });
                }
                return out;
            } catch (error) {
                void error;
                return null;
            }
        },
        renameDesktop: (nativeId, name) => {
            // Scripted rename through the writable `VirtualDesktop.name`
            // Q_PROPERTY (WRITE `setName`, pinned v6.7.5
            // `src/virtualdesktops.h:60`): the apply loop calls this only
            // for a newly-created exactly-attributed desktop while it still
            // carries its logical token, never for adopted or existing
            // desktops. Truthful read-back decides; any failure keeps the
            // id-based creation proof (the caller never re-creates).
            try {
                const all = decodeList(readProp(surface, "desktops"), MAX_NATIVE_DESKTOPS);
                if (all === null) {
                    return false;
                }
                for (const d of all) {
                    if (typeof d === "object" && d !== null && readProp(d as object, "id") === nativeId) {
                        let ok = false;
                        try {
                            ok = Reflect.set(d as object, "name", name);
                        } catch (error) {
                            void error;
                            ok = false;
                        }
                        if (!ok) {
                            return false;
                        }
                        try {
                            return readProp(d as object, "name") === name;
                        } catch (error) {
                            void error;
                            return false;
                        }
                    }
                }
                return false;
            } catch (error) {
                void error;
                return false;
            }
        },
        removeDesktop: (nativeId) => {
            try {
                const fn = readProp(surface, "removeDesktop");
                const all = decodeList(readProp(surface, "desktops"), MAX_NATIVE_DESKTOPS);
                if (typeof fn !== "function" || all === null) {
                    return false;
                }
                for (const d of all) {
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
                const list = decodeList(Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []), MAX_NATIVE_WINDOWS);
                if (list === null) {
                    return { total: 0, readable: false };
                }
                let total = 0;
                for (const item of list) {
                    if (typeof item !== "object" || item === null) {
                        return { total: 0, readable: false };
                    }
                    const sticky = readProp(item as object, "onAllDesktops");
                    if (sticky === true) {
                        total += 1;
                        continue;
                    }
                    const members = decodeList(readProp(item as object, "desktops"), MAX_NATIVE_WINDOW_DESKTOPS);
                    if (members === null) {
                        return { total: 0, readable: false };
                    }
                    if (members.length === 0) {
                        total += 1;
                        continue;
                    }
                    if (typeof sticky !== "boolean") {
                        return { total: 0, readable: false };
                    }
                    let found = false;
                    for (const m of members) {
                        if (typeof m !== "object" || m === null) {
                            return { total: 0, readable: false };
                        }
                        const id = readProp(m as object, "id");
                        if (!isOpaqueId(id)) return { total: 0, readable: false };
                        if (id === nativeId) found = true;
                    }
                    if (found) total += 1;
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
                const screens = decodeList(readProp(surface, "screens"), MAX_NATIVE_SCREENS);
                if (screens === null || screens.length === 0) {
                    return null;
                }
                let readable = true;
                for (const s of screens) {
                    if (typeof s !== "object" || s === null) {
                        readable = false;
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
                        readable = false;
                        continue;
                    }
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
                const all = decodeList(readProp(surface, "desktops"), MAX_NATIVE_DESKTOPS);
                if (all === null) {
                    return null;
                }
                for (const d of all) {
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
                const screens = decodeList(readProp(surface, "screens"), MAX_NATIVE_SCREENS);
                if (screens === null) {
                    return null;
                }
                for (const s of screens) {
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
        clearMaximized: (ref) => {
            // Exact KWin nonactivating maximize clear through the pinned
            // v6.7.4 Q_INVOKABLE (`src/window.h:1107`
            // `setMaximize(bool, bool, const RectF &)`; maximizeMode is
            // read-only from scripts, so this is the only script write).
            // Missing method fails closed (false); throws propagate so the
            // apply loop records the truthful setter. The caller proves
            // through the `maximizeMode` readback, never this boolean.
            const method = readProp(ref, "setMaximize");
            if (typeof method !== "function") {
                return false;
            }
            Reflect.apply(method as (...a: ReadonlyArray<unknown>) => unknown, ref, [false, false]);
            return true;
        },
        minDesktops: 2,
    };
}
