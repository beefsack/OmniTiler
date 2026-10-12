// Dev-only COSMIC send-to-workspace entry (opt-in only, disabled by default).
//
// No normal startup route: src/entry.ts must not import this module and no
// controller route, tray, KCM, autostart, or shortcut registration may use it.
// The only activation is the explicit exported startWorkspaceSendAdapterEntry
// function, called by no production source. Source tests prove no production
// import and no shortcut/startup wiring.
//
// Observation is read-only public state only: the focused output (active
// window's output, primary screen when no active window exists), all
// VirtualDesktop objects (desktop count, target existence and ref), the
// current source desktop plus the requested target desktop, per-desktop work
// areas via clientArea, and normal windows on the focused output whose
// Window.desktops membership is exactly one of the two observed desktops.
// Only normal windows are observed; every other classification is skipped
// before the snapshot so it can never be sent. Native writes are direct
// frameGeometry writes in the shared canonical order plus only the mover's
// Window.desktops membership write, then follow (target desktop switch plus
// mover focus, no retry) only after a fresh exact native membership proof.
// There is no ack/verify protocol and no blocksPlan.
//
// The single D-Bus transport is com.omnitiler.Planner DescribePlan; no
// other method is invoked and same-UID authorization stays the Planner's own
// single check (never duplicated here).

import { normalizeNativeId } from "./native-id";
import { decodeList } from "./qml-list";
import { readDomainGaps } from "./domain-gap";
import { connectSignal, readSignal } from "./signal-capability";
import { hasPerOutputSwitch, readDesktopForOutput, writeDesktopForOutput } from "./workspace-native";
import {
    WORKSPACE_SEND_DBUS_SERVICE,
    WORKSPACE_SEND_START_METHOD,
    WorkspaceSendAdapter,
    WorkspaceSendObserved,
    workspaceFingerprint,
} from "./workspace-send-adapter";

export interface WorkspaceSendEntryOverrides {
    readonly workspace?: unknown;
    readonly callDbus?: (
        service: string,
        path: string,
        iface: string,
        method: string,
        payload: string,
        callback: (reply: unknown) => void,
    ) => void;
    readonly scheduleOnce?: (delayMs: number, callback: () => void) => () => void;
    readonly log?: (message: string) => void;
    readonly owner?: unknown;
    readonly generation?: unknown;
    readonly readInnerGapFn?: () => unknown;
    readonly readOuterGapFn?: () => unknown;
}

export interface WorkspaceSendEntryHandle {
    readonly stop: () => void;
    readonly requestSend: (targetWorkspace: unknown) => boolean;
    // Live flight state only. Deprecated alias `blocksPlan` was retired with
    // the transaction protocol: send flights never block Plan; terminal
    // settlement is observed through the adapter's `onSettled` edge (left
    // unbound in this legacy dev entry, which owns no Plan).
    readonly isInFlight: () => boolean;
    readonly isEnabled: () => boolean;
}

const ENTRY_LOG = "omnitiler:route-diag";
const ENTRY_READY = `${ENTRY_LOG} component=cosmic-send stage=start event=ready outcome=ready`;
const ENTRY_REJECT = `${ENTRY_LOG} component=cosmic-send stage=start event=refuse outcome=entry-invalid`;

const MAX_LIST = 1024;
const MAX_DESKTOPS = 32;
const MAX_ID_LEN = 128;

function readProp(value: object, property: string): unknown {
    try {
        return Reflect.get(value, property);
    } catch (error) {
        void error;
        return undefined;
    }
}

function isOpaqueId(value: unknown): value is string {
    if (typeof value !== "string" || value.length === 0 || value.length > MAX_ID_LEN) {
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

function resolveLexicalWorkspace(): unknown {
    try {
        const candidate: unknown = workspace;
        if (typeof candidate === "object" && candidate !== null) {
            return candidate;
        }
    } catch (error) {
        void error;
    }
    return null;
}

function toQuantizedInt(value: unknown): number | null {
    // KWin 6.7.4 exposes QRectF geometry at fractional scale (e.g. 1.25).
    // Quantize at the observation boundary; the Rust wire contract stays
    // integer. Non-finite inputs still reject fail-closed.
    if (typeof value !== "number" || !Number.isFinite(value)) {
        return null;
    }
    const rounded = Math.round(value);
    if (!Number.isSafeInteger(rounded)) {
        return null;
    }
    return rounded;
}

function isWorkAreaRect(rect: { x: number; y: number; w: number; h: number }): boolean {
    if (rect.w <= 0 || rect.h <= 0 || rect.w > 16384 || rect.h > 16384) {
        return false;
    }
    if (rect.x < -16384 || rect.x > 16384 || rect.y < -16384 || rect.y > 16384) {
        return false;
    }
    return true;
}

function readFrameRect(ref: object): { x: number; y: number; w: number; h: number } | null {
    const geometry = readProp(ref, "frameGeometry");
    if (typeof geometry !== "object" || geometry === null) {
        return null;
    }
    const record = geometry as Record<string, unknown>;
    const x = toQuantizedInt(record["x"]);
    const y = toQuantizedInt(record["y"]);
    const widthRaw = record["width"] !== undefined ? record["width"] : record["w"];
    const heightRaw = record["height"] !== undefined ? record["height"] : record["h"];
    const w = toQuantizedInt(widthRaw);
    const h = toQuantizedInt(heightRaw);
    if (x === null || y === null || w === null || h === null) {
        return null;
    }
    if (w <= 0 || h <= 0 || x < -16384 || x > 16384 || y < -16384 || y > 16384 || w > 16384 || h > 16384) {
        return null;
    }
    return { x, y, w, h };
}

function readWorkArea(
    surface: Record<string, unknown>,
    outputRef: object,
    desktopRef: object,
): { x: number; y: number; w: number; h: number } | null {
    const areaFn = readProp(surface, "clientArea");
    if (typeof areaFn !== "function") {
        return null;
    }
    let area: unknown = undefined;
    try {
        area = Reflect.apply(areaFn as (...args: ReadonlyArray<never>) => unknown, surface, [5, outputRef, desktopRef]);
    } catch (error) {
        void error;
        return null;
    }
    if (typeof area !== "object" || area === null) {
        return null;
    }
    const areaRecord = area as Record<string, unknown>;
    const bx = toQuantizedInt(areaRecord["x"]);
    const by = toQuantizedInt(areaRecord["y"]);
    const bwRaw = areaRecord["width"] !== undefined ? areaRecord["width"] : areaRecord["w"];
    const bhRaw = areaRecord["height"] !== undefined ? areaRecord["height"] : areaRecord["h"];
    const bw = toQuantizedInt(bwRaw);
    const bh = toQuantizedInt(bhRaw);
    if (bx === null || by === null || bw === null || bh === null) {
        return null;
    }
    if (!isWorkAreaRect({ x: bx, y: by, w: bw, h: bh })) {
        return null;
    }
    return { x: bx, y: by, w: bw, h: bh };
}

function readDesktopId(desktop: object): string | null {
    const raw = readProp(desktop, "id");
    return isOpaqueId(raw) ? (raw as string) : null;
}

// Observe the focused output's source and target desktops only. Returns null
// fail-closed on any native shape failure; the adapter turns the null into the
// scope-invalid refusal token and the active/membership cases into the exact
// absent-focus / non-tiled-focus / desktop-cap / last-desktop tokens.
function observeNative(
    liveWorkspace: unknown,
    cache: Map<string, string>,
    targetWorkspace: string,
    pinnedSourceWorkspace?: string,
): WorkspaceSendObserved | null {
    try {
        if (typeof liveWorkspace !== "object" || liveWorkspace === null) {
            return null;
        }
        const surface = liveWorkspace as Record<string, unknown>;
        let active: unknown = undefined;
        try {
            active = Reflect.get(surface, "activeWindow");
        } catch (error) {
            void error;
            return null;
        }
        const activeRef = typeof active === "object" && active !== null ? (active as object) : null;
        const screens = decodeList(readProp(surface, "screens"), MAX_LIST);
        if (screens === null || screens.length === 0) {
            return null;
        }
        let outputRef: object;
        if (activeRef !== null) {
            const activeOutput = readProp(activeRef, "output");
            if (typeof activeOutput !== "object" || activeOutput === null) {
                return null;
            }
            outputRef = activeOutput as object;
        } else {
            const first = screens[0];
            if (typeof first !== "object" || first === null) {
                return null;
            }
            outputRef = first as object;
        }
        const outputNameRaw = readProp(outputRef, "name");
        if (!isOpaqueId(outputNameRaw)) {
            return null;
        }
        const sourceOutput = outputNameRaw as string;
        // All VirtualDesktops (global in KWin): desktop count, target
        // existence, and the target desktop ref are derived from this list.
        // The retained exact source is resolved from this list when pinned;
        // otherwise the live current desktop is used for the initial request.
        const desktops = decodeList(readProp(surface, "desktops"), MAX_DESKTOPS);
        if (desktops === null || desktops.length === 0) {
            return null;
        }
        let sourceDesktopRef: object | null = null;
        let sourceWorkspace: string | null = null;
        if (pinnedSourceWorkspace !== undefined) {
            if (!isOpaqueId(pinnedSourceWorkspace)) {
                return null;
            }
            for (const item of desktops) {
                if (typeof item !== "object" || item === null) {
                    continue;
                }
                const desktop = item as object;
                if (readDesktopId(desktop) === pinnedSourceWorkspace) {
                    sourceDesktopRef = desktop;
                    sourceWorkspace = pinnedSourceWorkspace;
                    break;
                }
            }
            if (sourceDesktopRef === null || sourceWorkspace === null) {
                return null;
            }
        } else {
            // Capability seam: per-output source on 6.7+, shared current on
            // legacy-shared hosts.
            const read = readDesktopForOutput(surface, outputRef);
            if (read === null) {
                return null;
            }
            sourceDesktopRef = read.ref;
            sourceWorkspace = read.id;
        }
        let targetDesktopRef: object | null = null;
        let targetExists = false;
        for (const item of desktops) {
            if (typeof item !== "object" || item === null) {
                continue;
            }
            const desktop = item as object;
            const id = readDesktopId(desktop);
            if (id === targetWorkspace) {
                targetDesktopRef = desktop;
                targetExists = true;
            }
        }
        const sourceBounds = readWorkArea(surface, outputRef, sourceDesktopRef);
        if (sourceBounds === null) {
            return null;
        }
        let targetBounds: { x: number; y: number; w: number; h: number } | null = null;
        if (targetDesktopRef !== null) {
            targetBounds = readWorkArea(surface, outputRef, targetDesktopRef);
        }
        // Per-desktop bounds are required once a distinct target exists.
        if (targetDesktopRef !== null && targetBounds === null) {
            return null;
        }
        if (targetBounds === null) {
            targetBounds = { x: 0, y: 0, w: 1, h: 1 };
        }
        // Behavior-fenced actual current workspace on the recording output
        // (mirrors plan-adapter-entry observeSendTarget): unlike the pinned
        // source identity, which survives view switches, this names the live
        // view for the stay source-visibility fence. Null when unreadable;
        // null never counts as selected downstream.
        let currentWorkspace: string | null = null;
        try {
            // Capability seam: per-output read on 6.7+, shared current on
            // legacy-shared hosts. Null never counts as selected downstream.
            const read = readDesktopForOutput(surface, outputRef);
            if (read !== null) {
                currentWorkspace = read.id;
            }
        } catch (error) {
            void error;
            currentWorkspace = null;
        }
        const lister = readProp(surface, "windowList");
        if (typeof lister !== "function") {
            return null;
        }
        let rawList: unknown = undefined;
        try {
            rawList = Reflect.apply(lister as (...args: ReadonlyArray<never>) => unknown, surface, []);
        } catch (error) {
            void error;
            return null;
        }
        const windows = decodeList(rawList, MAX_LIST);
        if (windows === null) {
            return null;
        }
        type SendWindow = {
            id: string;
            ref: object;
            rect: { x: number; y: number; w: number; h: number };
            fullscreen: boolean;
            maximized: boolean;
            floating: boolean;
            sticky: boolean;
            fitExcluded: boolean;
        };
        const sourceWindows: SendWindow[] = [];
        const targetWindows: SendWindow[] = [];
        const seen = new Set<string>();
        for (const item of windows) {
            if (typeof item !== "object" || item === null) {
                continue;
            }
            const ref = item as object;
            if (readProp(ref, "normalWindow") !== true) {
                continue;
            }
            if (readProp(ref, "managed") !== true) {
                continue;
            }
            if (readProp(ref, "minimized") !== false) {
                continue;
            }
            // Exceptions/overlays stay observed with flags; mover stays gated below.
            const output = readProp(ref, "output");
            if (typeof output !== "object" || output === null) {
                continue;
            }
            if (readProp(output as object, "name") !== sourceOutput) {
                continue;
            }
            const membership = decodeList(readProp(ref, "desktops"), MAX_DESKTOPS);
            if (membership === null) {
                return null;
            }
            const onSource = membership.indexOf(sourceDesktopRef) >= 0;
            const onTarget = targetDesktopRef !== null && membership.indexOf(targetDesktopRef) >= 0;
            // A window on both observed desktops (or neither) belongs to no
            // single scope and is excluded; membership is always exact.
            if (onSource === onTarget) {
                continue;
            }
            let native: string | null = null;
            try {
                native = normalizeNativeId(readProp(ref, "internalId"));
            } catch (error) {
                void error;
                return null;
            }
            if (native === null) {
                return null;
            }
            const id = cache.get(native) ?? (cache.set(native, native), native);
            if (seen.has(id)) {
                return null;
            }
            seen.add(id);
            const rect = readFrameRect(ref);
            if (rect === null) {
                return null;
            }
            // Local flags only; the adapter normalizes fitExcluded to wire fit_excluded.
            const sticky = readProp(ref, "onAllDesktops") === true;
            const fullscreen = readProp(ref, "fullScreen") !== false;
            const maximized = readProp(ref, "maximizeMode") !== 0;
            const floating = sticky;
            const fitExcluded = floating || sticky || fullscreen || maximized;
            const flags = {
                fullscreen,
                maximized,
                floating,
                sticky,
                fitExcluded,
            };
            if (onSource) {
                sourceWindows.push({ id, ref, rect, ...flags });
            } else {
                targetWindows.push({ id, ref, rect, ...flags });
            }
        }
        let focusedId = "";
        let moverRef: object | null = null;
        if (activeRef !== null) {
            let activeNative: string | null = null;
            try {
                activeNative = normalizeNativeId(readProp(activeRef, "internalId"));
            } catch (error) {
                void error;
                return null;
            }
            if (activeNative !== null) {
                const activeId = cache.get(activeNative) ?? (cache.set(activeNative, activeNative), activeNative);
                for (const entry of sourceWindows) {
                    if (entry.id === activeId) {
                        // Tiled-only mover; maximized and fullscreen movers
                        // carry under G-D2/D28 (REQ-MAX-09) with native state
                        // untouched, while floating/sticky keep "" / null and
                        // refuse downstream. An otherwise unexplained fit
                        // exclusion (fitExcluded without an overlay) also
                        // refuses: overlays are admitted only while otherwise
                        // valid.
                        if (
                            entry.floating ||
                            entry.sticky ||
                            (entry.fitExcluded && !entry.maximized && !entry.fullscreen)
                        ) {
                            break;
                        }
                        focusedId = entry.id;
                        moverRef = entry.ref;
                        break;
                    }
                }
            }
        }
        const sourceSorted = sourceWindows.map((entry) => entry.id).sort();
        const targetSorted = targetWindows.map((entry) => entry.id).sort();
        const sourceFingerprint = String(workspaceFingerprint(sourceOutput, sourceWorkspace, sourceSorted));
        const targetWorkspaceId = targetDesktopRef !== null ? (readDesktopId(targetDesktopRef) ?? targetWorkspace) : targetWorkspace;
        const targetFingerprint = String(workspaceFingerprint(sourceOutput, targetWorkspaceId, targetSorted));
        const frozenSource = Object.freeze(
            sourceWindows.map((entry) =>
                Object.freeze({
                    id: entry.id,
                    ref: entry.ref,
                    rect: Object.freeze({ x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h }),
                    fullscreen: entry.fullscreen,
                    maximized: entry.maximized,
                    floating: entry.floating,
                    sticky: entry.sticky,
                    fitExcluded: entry.fitExcluded,
                    // Derived wire alias; internal working type holds fitExcluded only.
                    fit_excluded: entry.fitExcluded,
                }),
            ),
        );
        const frozenTarget = Object.freeze(
            targetWindows.map((entry) =>
                Object.freeze({
                    id: entry.id,
                    ref: entry.ref,
                    rect: Object.freeze({ x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h }),
                    fullscreen: entry.fullscreen,
                    maximized: entry.maximized,
                    floating: entry.floating,
                    sticky: entry.sticky,
                    fitExcluded: entry.fitExcluded,
                    // Derived wire alias; internal working type holds fitExcluded only.
                    fit_excluded: entry.fitExcluded,
                }),
            ),
        );
        return {
            sourceOutput,
            sourceWorkspace,
            sourceBounds: Object.freeze({ x: sourceBounds.x, y: sourceBounds.y, w: sourceBounds.w, h: sourceBounds.h }),
            targetOutput: sourceOutput,
            targetWorkspace: targetWorkspaceId,
            targetBounds:
                targetDesktopRef !== null
                    ? Object.freeze({ x: targetBounds.x, y: targetBounds.y, w: targetBounds.w, h: targetBounds.h })
                    : Object.freeze({ x: 0, y: 0, w: 1, h: 1 }),
            focusedId,
            sourceWindows: frozenSource,
            targetWindows: frozenTarget,
            activeRef,
            moverRef,
            targetDesktopRef,
            targetExists,
            desktopCount: desktops.length,
            sourceFingerprint,
            targetFingerprint,
            currentWorkspace,
        };
    } catch (error) {
        void error;
        return null;
    }
}

// Explicit opt-in activation only; called by no production source. Returns a
// stop handle on success, null fail-closed after logging one fixed token.
export function startWorkspaceSendAdapterEntry(
    overrides: WorkspaceSendEntryOverrides = {},
): WorkspaceSendEntryHandle | null {
    const liveWorkspace: unknown =
        overrides.workspace !== undefined ? overrides.workspace : resolveLexicalWorkspace();
    const log = overrides.log ?? ((message: string): void => {
        try {
            console.log(message);
        } catch (error) {
            void error;
        }
    });
    const fail = (): null => {
        try {
            log(ENTRY_REJECT);
        } catch (error) {
            void error;
        }
        return null;
    };
    if (typeof liveWorkspace !== "object" || liveWorkspace === null) {
        return fail();
    }
    let callDbus = overrides.callDbus;
    if (callDbus === undefined) {
        try {
            const native: unknown = callDBus;
            if (typeof native !== "function") {
                return fail();
            }
            const bound = native as (...args: ReadonlyArray<unknown>) => void;
            callDbus = (service, path, iface, method, payload, callback) => {
                // Exact session activation: StartServiceByName(service, 0) is
                // the only two-argument daemon call; the adapter passes the
                // well-known Planner name as payload with fixed flags 0.
                if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_START_METHOD) {
                    bound(service, path, iface, method, payload, 0, callback);
                    return;
                }
                bound(service, path, iface, method, payload, callback);
            };
        } catch (error) {
            void error;
            return fail();
        }
    }
    let scheduleOnce = overrides.scheduleOnce;
    if (scheduleOnce === undefined) {
        try {
            const ctor: unknown = QTimer;
            if (typeof ctor !== "function") {
                return fail();
            }
            scheduleOnce = (delayMs, callback) => {
                const timer = new (ctor as new () => QTimer)();
                timer.interval = delayMs;
                timer.singleShot = true;
                timer.timeout.connect(callback);
                timer.start();
                return () => {
                    try {
                        timer.stop();
                    } catch (error) {
                        void error;
                    }
                };
            };
        } catch (error) {
            void error;
            return fail();
        }
    }
    const nativeIds = new Map<string, string>();
    const domainGaps = readDomainGaps({
        readInnerGapFn: overrides.readInnerGapFn,
        readOuterGapFn: overrides.readOuterGapFn,
    });
    const adapter = new WorkspaceSendAdapter({
        callDbus,
        scheduleOnce,
        log,
        observe: (targetWorkspace, pinnedSourceWorkspace?) =>
            observeNative(liveWorkspace, nativeIds, targetWorkspace, pinnedSourceWorkspace),
        setGeometry: (target, rect) => {
            try {
                return Reflect.set(target, "frameGeometry", {
                    x: rect.x,
                    y: rect.y,
                    width: rect.w,
                    height: rect.h,
                });
            } catch (error) {
                void error;
                return false;
            }
        },
        readGeometry: (target) => {
            try {
                return readFrameRect(target);
            } catch (error) {
                void error;
                return null;
            }
        },
        setDesktops: (target, refs) => {
            try {
                return Reflect.set(target, "desktops", refs);
            } catch (error) {
                void error;
                return false;
            }
        },
        subscribeMoverDesktops: (moverRef, handler) => {
            try {
                return connectSignal(readSignal(moverRef, "desktopsChanged"), handler);
            } catch (error) {
                void error;
                return null;
            }
        },
        switchToTarget: (desktopRef) => {
            try {
                const surface = liveWorkspace as Record<string, unknown>;
                const screens = decodeList(readProp(surface, "screens"), MAX_LIST);
                const first = screens?.[0];
                const output = typeof first === "object" && first !== null ? (first as object) : null;
                // Dev-only harnesses often lack per-screen setters; fall back
                // to the shared currentDesktop write when present. Capability
                // seam owns the 6.7-only method reads.
                if (output !== null && hasPerOutputSwitch(surface)) {
                    return writeDesktopForOutput(surface, desktopRef, output);
                }
                if (Object.prototype.hasOwnProperty.call(surface, "currentDesktop")) {
                    return Reflect.set(surface, "currentDesktop", desktopRef);
                }
                return false;
            } catch (error) {
                void error;
                return false;
            }
        },
        focusWindow: (windowRef) => {
            try {
                (liveWorkspace as { activeWindow: unknown }).activeWindow = windowRef;
                return true;
            } catch (error) {
                void error;
                return false;
            }
        },
        },
        { innerGap: domainGaps.innerGap, outerGap: domainGaps.outerGap },
    );
    const enabled = adapter.enable({ owner: overrides.owner, generation: overrides.generation });
    if (!enabled) {
        return fail();
    }
    try {
        log(ENTRY_READY);
    } catch (error) {
        void error;
    }
    return {
        stop: () => {
            try {
                adapter.disable();
            } catch (error) {
                void error;
            }
        },
        requestSend: (targetWorkspace) => {
            try {
                return adapter.requestSend(targetWorkspace);
            } catch (error) {
                void error;
                return false;
            }
        },
        isInFlight: () => adapter.isInFlight,
        isEnabled: () => adapter.isEnabled,
    };
}
