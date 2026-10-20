// Full world observation capture (D5): one complete normalized report on
// EVERY accepted update covering all outputs/workspaces/hidden windows, with
// explicit unreadable scope holds. Snapshot is the latest actual host state,
// never a desired cache. Native observation and safety stay independent of
// Planner: this module never decides tiling, order, membership, or cleanup.

import { decodeList } from "./qml-list";
import { normalizeNativeId } from "./native-id";
import { readNativeAlias } from "./native-alias";
import { normalizeScopeClassList, windowMatchesScopeClass } from "./scope-class";
import {
    FIXED_SIZE_PREDICATE_DEFAULT,
    FIXED_SIZE_PREDICATE_EITHER,
    normalizeFixedSizePredicate,
    readFixedSizePredicateValue,
} from "./fixed-size-predicate";

// Fixed-size admission predicate (same validated predicate tokens): a window is fixed-size
// when its declared min/max extents are finite, in range, non-sentinel, and
// equal on both axes (default) or either axis (either-axis-fixed). Pure
// classification; the core owns admission.
function isFixedSizeWindow(
    minSize: { readonly w: unknown; readonly h: unknown } | null | undefined,
    maxSize: { readonly w: unknown; readonly h: unknown } | null | undefined,
    predicate: unknown,
): boolean {
    if (minSize === null || minSize === undefined || maxSize === null || maxSize === undefined) {
        return false;
    }
    const bounds = [minSize.w, minSize.h, maxSize.w, maxSize.h];
    for (const bound of bounds) {
        if (typeof bound !== "number" || !Number.isInteger(bound)) {
            return false;
        }
        if (bound < 0 || bound > 16384) {
            return false;
        }
    }
    const minW = minSize.w as number;
    const minH = minSize.h as number;
    const maxW = maxSize.w as number;
    const maxH = maxSize.h as number;
    if (minW === 0 && minH === 0 && maxW === 0 && maxH === 0) {
        return false;
    }
    if (normalizeFixedSizePredicate(predicate) === FIXED_SIZE_PREDICATE_EITHER) {
        return (minW === maxW && minW !== 0) || (minH === maxH && minH !== 0);
    }
    return minW === maxW && minW !== 0 && minH === maxH && minH !== 0;
}
import {
    readDesktopForOutput,
    resolveEffectiveWorkspaceMode,
    type WorkspaceMode,
} from "./workspace-native";
import { isOpaqueId } from "./world-seq";

// Documented core bounds (tiler-core world_report): outputs 16, total
// native inventory 16*256, windows/closed/minimized 10000, bindings 4096,
// feedback 1024, domains 64. Overflow truncates with explicit incomplete
// holds, never a silent null report (null only when no output exists at
// all, when there is nothing to report).
const MAX_SCREENS = 16;
const MAX_DESKTOPS_TOTAL = 4096;
const MAX_NATIVE_PER_OUTPUT = 256;
const MAX_WINDOWS = 10000;
const MAX_CLOSED = 10000;
const MAX_DOMAINS = 64;
const MAX_WINDOW_DESKTOPS = 4096;

export interface ObservedWindow {
    win: string;
    ref: object;
    out: string;
    ws: string;
    rect: { x: number; y: number; w: number; h: number };
    floating: boolean;
    fitExcluded: boolean;
    fullscreen: boolean;
    maximized: boolean;
    sticky: boolean;
    fixedAuto: boolean;
    fixedSuppress: boolean;
    hints: { minW: number | null; minH: number | null; maxW: number | null; maxH: number | null };
    minimized: boolean;
}

export interface ObservedDomain {
    id: string;
    ws: string;
    bounds: { x: number; y: number; w: number; h: number };
    gap: number;
    outer: number;
    focused: string;
    readable: boolean;
    hold: string;
    // Deprecated wire-compat carrier, always empty: the core owns output
    // topology from the FULL `topology[].rect` rectangles and ignores any
    // carried adjacency, even when contradictory. Retained only so older
    // core decoders that require the key keep accepting reports.
    adjacent: Record<string, string>;
    windows: ObservedWindow[];
    closed: string[];
    complete: boolean;
}

export interface WorldObservation {
    mode: WorkspaceMode;
    topology: Array<{ out: string; order: string[]; current: string; complete: boolean; rect: { x: number; y: number; w: number; h: number } | null; primary: boolean }>;
    bindings: Array<{ logical: string; native: string; out: string; pin: boolean; disp: boolean; managed: boolean }>;
    domains: ObservedDomain[];
    // Explicit minimized lifetimes, separate from active tiled domain
    // membership. Minimized windows are absent from `domains[].windows`;
    // the report carries them here so the core never tiles them and never
    // infers a close from their absence. Entries are full local
    // `ObservedWindow` carriers (including the live client ref and the
    // native sticky state) so the apply loop can membership-actuate them;
    // only win/out/ws/sticky ride the wire.
    minimized: ObservedWindow[];
    // Ingress fixed-size admission predicate wire token. The core applies it
    // to subsequent admissions only, never reclassifying retained windows.
    predicate: "both-axes-fixed" | "either-axis-fixed";
    complete: boolean;
    gesture: boolean;
    focusedGlobal: string;
}

function readProp(value: object, property: string): unknown {
    try {
        return Reflect.get(value, property);
    } catch (error) {
        void error;
        return undefined;
    }
}

function toInt(value: unknown): number | null {
    if (typeof value !== "number" || !Number.isFinite(value)) {
        return null;
    }
    const r = Math.round(value);
    return Number.isSafeInteger(r) ? r : null;
}

function readRect(ref: object): { x: number; y: number; w: number; h: number } | null {
    const g = readProp(ref, "frameGeometry");
    if (typeof g !== "object" || g === null) {
        return null;
    }
    const r = g as Record<string, unknown>;
    const x = toInt(r["x"]);
    const y = toInt(r["y"]);
    const w = toInt(r["width"] !== undefined ? r["width"] : r["w"]);
    const h = toInt(r["height"] !== undefined ? r["height"] : r["h"]);
    if (x === null || y === null || w === null || h === null || w <= 0 || h <= 0) {
        return null;
    }
    if (x < -16384 || x > 16384 || y < -16384 || y > 16384 || w > 16384 || h > 16384) {
        return null;
    }
    return { x, y, w, h };
}

function readClientArea(surface: object, outputRef: object, desktopRef: object): { x: number; y: number; w: number; h: number } | null {
    const fn = readProp(surface, "clientArea");
    if (typeof fn !== "function") {
        return null;
    }
    let area: unknown;
    try {
        area = Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, [0, outputRef, desktopRef]);
    } catch (error) {
        void error;
        return null;
    }
    if (typeof area !== "object" || area === null) {
        return null;
    }
    const r = area as Record<string, unknown>;
    const x = toInt(r["x"]);
    const y = toInt(r["y"]);
    const w = toInt(r["width"] !== undefined ? r["width"] : r["w"]);
    const h = toInt(r["height"] !== undefined ? r["height"] : r["h"]);
    if (x === null || y === null || w === null || h === null || w <= 0 || h <= 0) {
        return null;
    }
    return { x, y, w, h };
}

function readHintSize(value: unknown): { w: number; h: number } | null {
    if (typeof value !== "object" || value === null) {
        return null;
    }
    const r = value as Record<string, unknown>;
    const w = toInt(r["width"] !== undefined ? r["width"] : r["w"]);
    const h = toInt(r["height"] !== undefined ? r["height"] : r["h"]);
    if (w === null || h === null || w < 0 || h < 0) {
        return null;
    }
    return { w, h };
}

// Bounded inventory read with explicit truncation: over-cap lists return
// the first `maxLength` entries with complete=false (the caller holds the
// scope) instead of refusing the whole report. Null only when the value
// is not a list at all (unreadable inventory).
function decodeListCapped(value: unknown, maxLength: number): { items: ReadonlyArray<unknown>; complete: boolean } | null {
    const items = decodeList(value, maxLength);
    if (items !== null) {
        return { items, complete: true };
    }
    if (typeof value !== "object" || value === null) {
        return null;
    }
    if (Array.isArray(value)) {
        return value.length > maxLength
            ? { items: value.slice(0, maxLength), complete: false }
            : null;
    }
    let length: unknown = undefined;
    try {
        length = Reflect.get(value, "length");
    } catch (error) {
        void error;
        return null;
    }
    if (typeof length !== "number" || !Number.isInteger(length) || length < 0) {
        return null;
    }
    if (length <= maxLength) {
        return null;
    }
    const out: unknown[] = [];
    for (let i = 0; i < maxLength; i += 1) {
        let el: unknown = undefined;
        try {
            el = Reflect.get(value, String(i));
        } catch (error) {
            void error;
            return null;
        }
        if (el === undefined) {
            return null;
        }
        out.push(el);
    }
    return { items: out, complete: false };
}

export interface ObserveEnv {
    workspace: unknown;
    readMode: () => unknown;
    readGap: () => number;
    readOuterGap: () => number;
    gestureActive: () => boolean;
    knownClosed: () => ReadonlyArray<string>;
    logicalBindingOf: (nativeId: string, out?: string) => { logical: string; out: string; pin: boolean; disp: boolean; managed: boolean } | null;
    // Aligned per-output workspaces (KDE production): the same native
    // desktop id exists on every output with a distinct logical identity
    // per (output, native), so bindings replicate per output instead of
    // collapsing through one native->logical row. Off preserves the
    // disjoint single-row behavior for existing callers and tests.
    replicateBindingsPerOutput?: boolean;
    // Adapter-owned intentional-float membership (observation input, never
    // policy): converged from accepted Planner exception rows. Absent reads
    // as tiled.
    isFloating?: (win: string) => boolean;
    // Fixed-size admission predicate seam (default: validated KWin script
    // startup configuration). Classifies fresh fixed-size admissions for the
    // fixedAuto/fitExcluded wire flags; never policy.
    readFixedPredicate?: () => unknown;
    // Dev/test admission scope (platform-only): an allowlist of exact
    // native `resourceClass` identities (comma-joined string or array).
    // Empty preserves whole-world behavior. Foreign windows are skipped
    // like non-normal windows, never marked unreadable.
    scopeClass?: string | readonly string[];
    // Fixed-window tile override (native only): the Rust-owned intent store
    // tile set carries adapter-asserted tile wins (D3 suppress signal). A
    // tile-overridden window carries fixedSuppress=true so the core never
    // auto-floats it, and stays tiled locally. Never policy.
    isTileOverride?: (win: string) => boolean;
    // Adapter-asserted fixed-size origin decoration (read-only): the
    // intentional-float/suppress provenance for one stable id and live ref,
    // or null when unrecorded. Never classifies fresh admissions; the core
    // owns admission from the carried flags.
    fixedOriginOf?: (win: string, ref: object) => { readonly fixedAuto: boolean; readonly fixedSuppress: boolean } | null;
}

// FULL output rectangles for the core-owned topology authority (the
// membership and selection decisions stay in core). The adapter reports
// every live screen's FULL geometry in `topology[].rect` plus separate
// per-desktop work-area `bounds`/outer gaps, and always emits an empty
// wire-compat `adjacent` map that the core ignores. No adapter-side
// target derivation exists; direction-only commands carry no endpoint.
function readScreenGeometry(ref: object): { x: number; y: number; w: number; h: number } | null {
    let geometry: unknown = undefined;
    try {
        geometry = readProp(ref, "geometry");
    } catch (error) {
        void error;
        return null;
    }
    if (typeof geometry !== "object" || geometry === null) {
        return null;
    }
    const r = geometry as Record<string, unknown>;
    const x = toInt(r["x"]);
    const y = toInt(r["y"]);
    const w = toInt(r["width"] !== undefined ? r["width"] : r["w"]);
    const h = toInt(r["height"] !== undefined ? r["height"] : r["h"]);
    if (x === null || y === null || w === null || h === null || w <= 0 || h <= 0) {
        return null;
    }
    return { x, y, w, h };
}

// Capture the full world: every output's current desktop plus every known
// desktop with members (hidden included). Unreadable scopes become explicit
// holds, never empty lists. Closed ids ride explicitly.
export function captureWorldObservation(env: ObserveEnv): WorldObservation | null {
    const surface = env.workspace;
    if (typeof surface !== "object" || surface === null) {
        return null;
    }
    const target = surface as object;
    let rawMode: unknown = "per-output-local";
    try {
        rawMode = env.readMode();
    } catch (error) {
        void error;
    }
    const requested: WorkspaceMode =
        rawMode === "global-unique" || rawMode === "shared" || rawMode === "per-output-local"
            ? rawMode
            : "per-output-local";
    const mode = resolveEffectiveWorkspaceMode(requested, target);
    const screensCapped = decodeListCapped(readProp(target, "screens"), MAX_SCREENS);
    const desktopsCapped = decodeListCapped(readProp(target, "desktops"), MAX_DESKTOPS_TOTAL);
    const winListCapped = decodeListCapped(
        (() => {
            const fn = readProp(target, "windowList");
            if (typeof fn !== "function") {
                return null;
            }
            try {
                return Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, target, []);
            } catch (error) {
                void error;
                return null;
            }
        })(),
        MAX_WINDOWS,
    );
    if (screensCapped === null || desktopsCapped === null || winListCapped === null) {
        return null;
    }
    const screens = screensCapped.items;
    const desktops = desktopsCapped.items;
    const winList = winListCapped.items;
    // Dev/test admission scope: exact `resourceClass` allowlist (empty
    // disables). Malformed input fails the whole observation; the entry
    // only passes validated input, so this guards direct callers.
    const scope = normalizeScopeClassList(env.scopeClass ?? "");
    if (scope === null) {
        return null;
    }
    // Over-cap inventories truncate the tail and mark the report
    // incomplete (fencing implicit-close inference), but the enumerated
    // head stays fully readable: a cut tail must not blank live scopes.
    // Null only when the value is not a list at all.
    const inventoryTruncated = !screensCapped.complete || !desktopsCapped.complete || !winListCapped.complete;
    // Desktop refs by id; null entries mark unreadable inventory.
    const desktopById = new Map<string, object>();
    let desktopsReadable = true;
    for (const item of desktops) {
        if (typeof item !== "object" || item === null) {
            desktopsReadable = false;
            continue;
        }
        const id = readProp(item as object, "id");
        if (!isOpaqueId(id)) {
            desktopsReadable = false;
            continue;
        }
        desktopById.set(id as string, item as object);
    }
    interface ScreenEntry { ref: object; name: string; current: string; currentRef: object | null }
    const screenEntries: ScreenEntry[] = [];
    let topologyComplete = desktopsReadable;
    for (const item of screens) {
        if (typeof item !== "object" || item === null) {
            topologyComplete = false;
            continue;
        }
        const ref = item as object;
        const name = readProp(ref, "name");
        if (!isOpaqueId(name)) {
            topologyComplete = false;
            continue;
        }
        const read = readDesktopForOutput(target, ref);
        if (read === null) {
            topologyComplete = false;
            screenEntries.push({ ref, name: name as string, current: "", currentRef: null });
            continue;
        }
        screenEntries.push({ ref, name: name as string, current: read.id, currentRef: read.ref });
    }
    if (screenEntries.length === 0) {
        return null;
    }
    // Per-window classification from public state only.
    const liveById = new Map<string, ObservedWindow>();
    let windowsReadable = true;
    let activeId = "";
    // Fixed-size predicate resolves once per capture (startup config plus
    // deliberate Options reload), never per window.
    const predicate = (() => {
        try {
            return readFixedSizePredicateValue(
                env.readFixedPredicate !== undefined ? env.readFixedPredicate : undefined,
            );
        } catch (error) {
            void error;
            return readFixedSizePredicateValue(undefined);
        }
    })();
    try {
        const active = readProp(target, "activeWindow");
        if (typeof active === "object" && active !== null) {
            const raw = normalizeNativeId(readProp(active as object, "internalId"));
            if (raw !== null) {
                activeId = raw;
                // A foreign focus never names an unmanaged window: under a
                // scope the global and per-domain focus read as unfocused
                // instead of pointing at foreign state.
                if (!windowMatchesScopeClass(readProp, active as object, scope)) {
                    activeId = "";
                }
            }
        }
    } catch (error) {
        void error;
    }
    for (const item of winList) {
        if (typeof item !== "object" || item === null) {
            windowsReadable = false;
            continue;
        }
        const ref = item as object;
        if (readProp(ref, "normalWindow") !== true) {
            continue;
        }
        // Scoped runs exclude foreign windows from observation entirely
        // (like non-normal windows): the core never sees them, so it never
        // manages, commands, or effects them. Never an unreadable hold.
        if (!windowMatchesScopeClass(readProp, ref, scope)) {
            continue;
        }
        const rawId = normalizeNativeId(readProp(ref, "internalId"));
        if (rawId === null) {
            windowsReadable = false;
            continue;
        }
        const rect = readRect(ref);
        const outputRaw = readProp(ref, "output");
        const outName = typeof outputRaw === "object" && outputRaw !== null
            ? readProp(outputRaw as object, "name")
            : undefined;
        const membersCapped = decodeListCapped(readProp(ref, "desktops"), MAX_WINDOW_DESKTOPS);
        if (rect === null || !isOpaqueId(outName) || membersCapped === null) {
            windowsReadable = false;
            continue;
        }
        // A cut homing tail keeps the head (the home desktop reads first),
        // so truncation never mis-homes here. An over-cap homing list still
        // holds: the tail membership is unenumerated, never assumed empty.
        const members = membersCapped.items;
        if (!membersCapped.complete) {
            windowsReadable = false;
            continue;
        }
        // Home workspace: first member desktop id (sticky rides homed here;
        // the core excludes sticky occupancy per-domain itself).
        let home = "";
        for (const m of members) {
            if (typeof m === "object" && m !== null && isOpaqueId(readProp(m as object, "id"))) {
                home = readProp(m as object, "id") as string;
                break;
            }
        }
        // Sticky native (onAllDesktops true) with an empty desktop list is
        // homed from the observed output current (never a fabricated id and
        // never whole-report unreadable). The homing feeds retention only, never
        // spare occupancy: sticky never occupies any backing cell.
        if (home === "") {
            let stickyEmpty = false;
            try {
                stickyEmpty = readProp(ref, "onAllDesktops") === true;
            } catch (error) {
                void error;
                stickyEmpty = false;
            }
            if (stickyEmpty && isOpaqueId(outName)) {
                const current = screenEntries.find((s) => s.name === (outName as string))?.current ?? "";
                if (current !== "" && desktopById.has(current)) {
                    home = current;
                } else {
                    continue;
                }
            } else {
                windowsReadable = false;
                continue;
            }
        }
        const min = readHintSize(readProp(ref, "minSize"));
        const max = readHintSize(readProp(ref, "maxSize"));
        const adapterFloat = (() => {
            try {
                return env.isFloating !== undefined && env.isFloating(rawId);
            } catch (error) {
                void error;
                return false;
            }
        })();
        // Fixed-size admission classification: fresh
        // admissions classify from declared size hints under the validated
        // predicate; recorded origins decorate retained automatic/suppress
        // provenance read-only. Never policy: the core owns admission from
        // these flags.
        const freshFixed = isFixedSizeWindow(
            min !== null ? { w: min.w, h: min.h } : null,
            max !== null ? { w: max.w, h: max.h } : null,
            predicate,
        );
        let fixedAuto = freshFixed;
        let fixedSuppress = false;
        if (env.fixedOriginOf !== undefined) {
            try {
                const origin = env.fixedOriginOf(rawId, ref);
                if (origin !== null) {
                    fixedAuto = origin.fixedAuto === true || freshFixed;
                    fixedSuppress = origin.fixedSuppress === true;
                }
            } catch (error) {
                void error;
            }
        }
        // Adapter-asserted tile override (D3 suppress signal): the
        // Rust-owned intent tile set pins this live-tile win, suppressing
        // automatic fixed-size float admission in core. A tile-overridden
        // fixed window stays tiled locally instead of floating.
        let tileOverride = false;
        if (env.isTileOverride !== undefined) {
            try {
                tileOverride = env.isTileOverride(rawId) === true;
            } catch (error) {
                void error;
            }
        }
        if (tileOverride) {
            fixedSuppress = true;
        }
        // Native overlay reads use the KWin property names
        // (`fullScreen`, `maximizeMode`, `onAllDesktops`): the lowercase
        // aliases exist only for offline test fakes and apply when the
        // native property is absent, never overriding a live native read.
        const fullScreenRaw = readProp(ref, "fullScreen");
        const fullscreenNow = fullScreenRaw === true ||
            (fullScreenRaw === undefined && readNativeAlias(readProp, ref, "fullscreen") === true);
        const modeRaw = readProp(ref, "maximizeMode");
        const maximizedNow = (typeof modeRaw === "number" && modeRaw !== 0) ||
            (modeRaw === undefined && readNativeAlias(readProp, ref, "maximized") === true);
        const allDesktopsRaw = readProp(ref, "onAllDesktops");
        const stickyNow = allDesktopsRaw === true ||
            (allDesktopsRaw === undefined && readNativeAlias(readProp, ref, "sticky") === true);
        const floatingNow = adapterFloat || stickyNow || (fixedAuto && !tileOverride);
        const fitExcludedNow = floatingNow || stickyNow || fullscreenNow || maximizedNow;
        liveById.set(rawId, {
            win: rawId, ref, out: outName as string, ws: home, rect,
            floating: floatingNow,
            fitExcluded: fitExcludedNow,
            fullscreen: fullscreenNow,
            maximized: maximizedNow,
            sticky: stickyNow,
            fixedAuto, fixedSuppress,
            hints: {
                minW: min ? min.w : null, minH: min ? min.h : null,
                maxW: max ? max.w : null, maxH: max ? max.h : null,
            },
            minimized: readProp(ref, "minimized") === true,
        });
    }
    // Truncated global inventories hold where the cut tail is unprovable:
    // an over-cap window/desktop/screen list omits unknown members, so the
    // affected scopes cannot enumerate completely. The head stays reported;
    // the scopes hold explicitly instead of converging partial membership.
    if (!winListCapped.complete) {
        windowsReadable = false;
    }
    if (!desktopsCapped.complete) {
        desktopsReadable = false;
        topologyComplete = false;
    }
    if (!screensCapped.complete) {
        topologyComplete = false;
    }
    // Domains: every (output, desktop) pair that is visible or has homed
    // members, plus every visible desktop even when empty (trailing spare
    // visibility). Group homed windows by output+workspace.
    const byDomain = new Map<string, ObservedWindow[]>();
    for (const w of liveById.values()) {
        const key = `${w.out}\u0000${w.ws}`;
        const arr = byDomain.get(key) ?? [];
        arr.push(w);
        byDomain.set(key, arr);
    }
    const screenRects: Array<{ x: number; y: number; w: number; h: number } | null> =
        screenEntries.map((s) => readScreenGeometry(s.ref));
    // No adapter-owned adjacency map is computed or emitted: the core owns
    // output topology from the FULL `topology[].rect` rectangles below and
    // ignores the deprecated wire-compat `adjacent` carrier (always empty).
    const gap = env.readGap();
    const outer = env.readOuterGap();
    const gesture = (() => {
        try {
            return env.gestureActive();
        } catch (error) {
            void error;
            return false;
        }
    })();
    const domains: ObservedDomain[] = [];
    const seenDomains = new Set<string>();
    const pushDomain = (out: string, outRef: object, ws: string): void => {
        const key = `${out}\u0000${ws}`;
        if (seenDomains.has(key)) {
            return;
        }
        seenDomains.add(key);
        const desktopRef = desktopById.get(ws) ?? null;
        const members = byDomain.get(key) ?? [];
        const readable = desktopRef !== null && windowsReadable;
        const hold = desktopRef === null ? "incomplete-inventory" : windowsReadable ? "" : "unreadable";
        const bounds = desktopRef !== null ? readClientArea(target, outRef, desktopRef) : null;
        const screenCurrent = screenEntries.find((s) => s.name === out)?.current ?? "";
        domains.push({
            id: out, ws,
            bounds: bounds ?? { x: 0, y: 0, w: 800, h: 600 },
            gap, outer,
            focused: ws === screenCurrent ? activeId : "",
            readable: readable && bounds !== null,
            hold: bounds === null ? "unreadable" : hold,
            adjacent: {},
            windows: readable ? members.filter((m) => !m.minimized) : [],
            closed: [],
            complete: readable && bounds !== null,
        });
    };
    for (const screen of screenEntries) {
        if (screen.current !== "") {
            pushDomain(screen.name, screen.ref, screen.current);
        }
    }
    // Hidden domains: homed pairs on non-current desktops.
    const visibleKeys = new Set(domains.map((d) => `${d.id}\u0000${d.ws}`));
    for (const [key, members] of byDomain) {
        const split = key.split("\u0000");
        const out = split[0] as string;
        const ws = split[1] as string;
        if (visibleKeys.has(key)) {
            continue;
        }
        const outRef = screenEntries.find((s) => s.name === out)?.ref ?? null;
        if (outRef === null) {
            continue;
        }
        void members;
        pushDomain(out, outRef, ws);
    }
    // Domain cap (core MAX_WORLD_DOMAINS): visible domains are pushed
    // first, so truncation drops hidden domains and the incomplete flag
    // fences implicit-close inference for the cut tail.
    let domainsTruncated = false;
    if (domains.length > MAX_DOMAINS) {
        domains.length = MAX_DOMAINS;
        domainsTruncated = true;
    }
    // Explicit minimized lifetimes, separate from active tiled domain
    // membership (D23): absent from `domains[].windows`, carried in
    // `minimized` so the core never tiles them and never infers a close.
    const minimized: WorldObservation["minimized"] = [];
    for (const w of liveById.values()) {
        if (w.minimized) {
            minimized.push(w);
        }
    }
    const closed = [...env.knownClosed()].filter((id) => isOpaqueId(id) && !liveById.has(id));
    if (closed.length > 0 && domains.length > 0) {
        (domains[0] as ObservedDomain).closed = closed.slice(0, MAX_CLOSED);
    }
    // Bindings from the adapter-owned mapping snapshot (logical<->native).
    // Replicated mode emits one row per (output, native) so the shared
    // native grid keeps a distinct logical identity per output cell;
    // only mapped cells emit (unmapped cells mint core-side). The
    // disjoint mode keeps one row per native as before.
    const bindings: WorldObservation["bindings"] = [];
    if (env.replicateBindingsPerOutput === true) {
        // Desktop-major order (all outputs per native) keeps the wire
        // byte-stable with the disjoint single-row order: one row per
        // native becomes one row per (output, native) in place.
        for (const id of desktopById.keys()) {
            for (const screen of screenEntries) {
                const bound = env.logicalBindingOf(id, screen.name);
                if (bound !== null) {
                    bindings.push({ logical: bound.logical, native: id, out: screen.name, pin: bound.pin, disp: bound.disp, managed: bound.managed });
                }
            }
        }
    } else {
        for (const id of desktopById.keys()) {
            const bound = env.logicalBindingOf(id);
            if (bound !== null) {
                bindings.push({ logical: bound.logical, native: id, out: bound.out, pin: bound.pin, disp: bound.disp, managed: bound.managed });
            }
        }
    }
    // Per-output native order (core MAX_NATIVE_PER_OUTPUT): truncation
    // marks the topology incomplete so lifecycle holds instead of
    // planning from a cut ring.
    const nativeOrder = [...desktopById.keys()];
    const orderTruncated = nativeOrder.length > MAX_NATIVE_PER_OUTPUT;
    const topologyCompleteAll = topologyComplete && !orderTruncated;
    // Live native output geometry plus the active-screen primary, per
    // output: independent native facts for the core-owned topology
    // authority and hotplug nearest-survivor placement (never window
    // frames). The primary mirrors the active-screen key.
    const activeName: string | null = (() => {
        try {
            const screen = readProp(target, "activeScreen");
            if (typeof screen === "object" && screen !== null) {
                const name = readProp(screen as object, "name");
                return typeof name === "string" ? name : null;
            }
        } catch (error) {
            void error;
        }
        return null;
    })();
    const liveScreens = screenEntries.filter((s) => s.current !== "");
    const topology = liveScreens.map((s) => ({
        out: s.name,
        order: orderTruncated ? nativeOrder.slice(0, MAX_NATIVE_PER_OUTPUT) : nativeOrder,
        current: s.current,
        complete: topologyCompleteAll,
        rect: screenRects[screenEntries.indexOf(s)] ?? null,
        primary: activeName !== null && activeName === s.name,
    }));
    const complete = topologyCompleteAll && windowsReadable && desktopsReadable && !domainsTruncated && !inventoryTruncated;
    const predicateWire = predicate === FIXED_SIZE_PREDICATE_EITHER ? FIXED_SIZE_PREDICATE_EITHER : FIXED_SIZE_PREDICATE_DEFAULT;
    return { mode, topology, bindings, domains, minimized, predicate: predicateWire, complete, gesture, focusedGlobal: activeId };
}
