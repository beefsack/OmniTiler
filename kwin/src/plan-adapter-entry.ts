// Live KWin wiring for the bounded DescribePlan adapter.
//
// Production startup route: src/entry.ts calls startPlanAdapterEntry with the
// fixed session owner/generation. Observation is read-only public state only
// (active window, window list, output names, desktop membership, normalized
// native ids, quantized frame extents, active-output work area). Only
// normal windows are observed; every other window kind is skipped before the
// snapshot so it can never be sent, and Rust owns all remaining policy. The
// planner requests are DescribePlan carrying one JSON string. Workspace sends
// also use the documented bus-daemon presence and activation methods before
// pinning a unique Planner owner. Reply geometries are applied by the adapter in the
// shared canonical order. Directional focus/move shortcuts and
// direction-plus-mode resize shortcuts from the configured profile catalog
// map to parameterized plan commands. Window and scope signals feed one
// debounced fresh-snapshot resync owned by the adapter. Foreground plus
// hidden-domain observation feed one shared single-flight: at startup and on
// window open/move/scope signals the adapter also adopts/reconciles
// non-visible (output, workspace) domains without switching desktop
// visibility or writing native focus. Hidden snapshots carry a deterministic
// domain-local structural anchor as focusedId and only drive
// admit/remove/reconcile geometry on exact hidden-domain refs. No tiling, order,
// membership, or rejection policy lives here. Diagnostics are always-on and
// bounded: the adapter's cmd route entry/terminal, rejected-kind, per-member
// write, scope-transition, echo-fence, and refusal lines, plus one bounded
// shortcut-failed line and one bounded plan-ready startup line.

import { DomainGaps, readDomainGaps } from "./domain-gap";
import { readMigrationSourceRefillValue } from "./migration-source-refill";
import { SameAxisMove, readSameAxisMoveValue } from "./same-axis-move";
import { BornMaximized, readBornMaximizedValue } from "./born-maximized";
import {
    FixedSizePredicate,
    readFixedSizePredicateValue,
} from "./fixed-size-predicate";
import { identifyGrabbedEdges, identifyPressGrabbed, resolveOracleResizeTargets, startDragOraclePullEntry, DragOracleFinishContext, DragOracleVerdict, OracleGrabbed, OracleGrabSource } from "./drag-oracle-pull";
import { decodeList } from "./qml-list";
import {
    DRAG_MEASURE_LATER_TIMEOUT_MS,
    DRAG_MEASURE_VERDICT_TIMEOUT_MS,
    DragMeasurePointer,
    DragMeasureRect,
    describeMeasureEdge,
    formatDragMeasureLine,
    readMeasurePointer,
    readMeasureRect,
} from "./drag-measure";
import { normalizeNativeId } from "./native-id";
import {
    ActiveGroupObserved,
    GROUP_HIGHLIGHT_CLEAR_METHOD,
    GROUP_HIGHLIGHT_INTERFACE,
    GROUP_HIGHLIGHT_OBJECT,
    GROUP_HIGHLIGHT_SERVICE,
    GROUP_HIGHLIGHT_SET_METHOD,
    startActiveGroupHighlight,
} from "./active-group-highlight";
import { PLAN_DBUS_SERVICE, PLAN_INTERFACE, PLAN_METHOD, PLAN_OBJECT, PLAN_SERVICE, PLAN_START_FLAGS, PLAN_START_METHOD, PlanAdapter, PlanDirection, PlanDomain, PlanDragPreviewResult, PlanObserved, PlanObservedWindow, PlanResizeMode, PlanSnapshot, DirectionalObservation, PlanWindowConstraints, planDirectionalFingerprint, planFingerprint, snapshotOf } from "./plan-adapter";
import { processGeneration } from "./tray-publisher";
import { PLAN_SOURCE_REV } from "./source-rev";
import { connectSignal, readSignal } from "./signal-capability";
import { KWIN_TRACE_ENABLED } from "./trace";
import { WorkspaceNativeAdapter, hasPerOutputSwitch, parseDefaultTiled, readDesktopForOutput, readPerOutputVirtualDesktopsFlag, supportsPerOutputDesktops, writeDesktopForOutput, workspaceShortcutCatalog } from "./workspace-native";
import {
    WORKSPACE_SEND_DBUS_SERVICE,
    WORKSPACE_SEND_START_FLAGS,
    WORKSPACE_SEND_START_METHOD,
    WorkspaceSendAdapter,
    WorkspaceFollowNativeDiagnostic,
    WorkspaceMigrateObserved,
    WorkspaceMigratePin,
    WorkspaceSendObserved,
    WorkspaceSendSettled,
    isMigrateDirection,
    workspaceFingerprint as workspaceSendFingerprint,
} from "./workspace-send-adapter";

export interface PlanEntryOverrides {
    readonly workspace?: unknown;
    readonly oracleCallDbus?: (
        service: string,
        path: string,
        iface: string,
        method: string,
        callback: (reply: unknown) => void,
    ) => void;
    readonly callDbus?: (
        service: string,
        path: string,
        iface: string,
        method: string,
        payload: string,
        callback: (reply: unknown) => void,
    ) => void;
    readonly highlightCallDbus?: (
        service: string,
        path: string,
        iface: string,
        method: string,
        ...args: ReadonlyArray<unknown>
    ) => void;
    readonly scheduleOnce?: (delayMs: number, callback: () => void) => () => void;
    readonly log?: (message: string) => void;
    readonly owner?: unknown;
    readonly generation?: unknown;
    // Q3 settled intentional-float persistence: when true, the entry feeds
    // the adapter a complete live inventory plus a hydration sink and runs
    // one bounded ReadFloatIntent before first plan admission; settled
    // float/unfloat/close then persist via WriteFloatIntent. Omitted by
    // tests without intent traffic (zero adapter behavior change);
    // production sets it.
    readonly floatIntent?: boolean;
    // Optional deterministic per-script-instance group effect stream
    // generation. Test seam only: production omits it so the entry mints one
    // fresh token from the shared tray source. Planner identity stays fixed.
    readonly groupGeneration?: unknown;
    readonly registerShortcutFn?: (
        name: string,
        text: string,
        sequence: string,
        callback: () => void,
    ) => boolean;
    readonly readProfileFn?: () => unknown;
    readonly readWorkspaceModeFn?: () => unknown;
    readonly readTilingDefaultFn?: () => unknown;
    readonly readInnerGapFn?: () => unknown;
    readonly readOuterGapFn?: () => unknown;
    readonly readSameAxisMoveFn?: () => unknown;
    readonly readFixedSizePredicateFn?: () => unknown;
    readonly readBornMaximizedFn?: () => unknown;
    readonly readMigrationSourceRefillFn?: () => unknown;
    readonly options?: unknown;
    // Workspace tiling menu state: invoked whenever the tray snapshot
    // (scope, tiled, default) changes so entry.ts can bump the publisher
    // revision immediately. Heartbeat also converges drift once per second.
    readonly onWorkspaceTilingChanged?: (snapshot: WorkspaceTilingSnapshot) => void;
}

export interface WorkspaceTilingSnapshot {
    readonly scope: string;
    readonly tiled: boolean;
    readonly defaultTiled: boolean;
}

export const WORKSPACE_TILING_TOGGLE_ACTION = "omnitiler-toggle-workspace-tiling";
export const WORKSPACE_TILING_TOGGLE_TEXT = "Toggle tiling for current workspace";
// User decision 2026-10-10: the per-workspace tiling toggle binds Meta+Y by
// default. Every other currently-unbound action stays unbound.
export const WORKSPACE_TILING_TOGGLE_SEQUENCE = "Meta+Y";

export interface PlanEntryHandle {
    readonly stop: () => void;
    readonly requestFocus: (direction: unknown) => void;
    readonly requestMove: (direction: unknown) => void;
    readonly requestResize: (direction: unknown, mode: unknown) => void;
    readonly requestToggleOrientation: () => void;
    readonly requestFloat: () => void;
    readonly requestSticky: () => void;
    readonly requestMaximize: () => void;
    readonly requestFullscreen: () => void;
    readonly requestWorkspaceSelect: (index: unknown) => void;
    readonly requestWorkspaceMove: (index: unknown, follow?: unknown) => void;
    readonly requestWorkspacePrevious: () => void;
    readonly requestWorkspaceRelative: (delta: unknown) => void;
    readonly requestWorkspaceRelativeMove: (delta: unknown, follow?: unknown) => void;
    readonly requestSendToOutput: (direction: unknown, follow?: unknown) => void;
    readonly requestWorkspaceMigrate: (direction: unknown) => void;
    readonly getWorkspaceTilingSnapshot: () => WorkspaceTilingSnapshot;
    readonly requestWorkspaceTilingToggle: () => void;
}

export interface PlanShortcutRow {
    readonly action: string;
    readonly text: string;
    readonly sequence: string;
    readonly op: "focus" | "move" | "resize" | "float" | "sticky" | "maximize" | "fullscreen" | "toggle-orientation";
    readonly direction: PlanDirection | null;
    readonly mode: PlanResizeMode | null;
}

export interface PlanOutputSendShortcutRow {
    readonly action: string;
    readonly text: string;
    readonly sequence: string;
    readonly direction: PlanDirection;
    readonly follow: boolean;
}

// Explicit output-send shortcut catalog (REQ-OUT-04, item 5.3): follow forms
// bind Meta+Ctrl+Alt+arrows and +H/J/K/L (H left, J down, K up, L right);
// stay forms register bindable but unbound (empty sequence) for user
// rebinding. Native catalog/preset sync stays a later bounded unit.
export function planOutputSendShortcutCatalog(): ReadonlyArray<PlanOutputSendShortcutRow> {
    const rows: PlanOutputSendShortcutRow[] = [];
    const dirs: ReadonlyArray<{ direction: PlanDirection; key: string; arrow: string }> = [
        { direction: "left", key: "H", arrow: "Left" },
        { direction: "down", key: "J", arrow: "Down" },
        { direction: "up", key: "K", arrow: "Up" },
        { direction: "right", key: "L", arrow: "Right" },
    ];
    for (const entry of dirs) {
        rows.push({
            action: `omnitiler-send-output-${entry.direction}`,
            text: `Send window to output ${entry.direction} (follow)`,
            sequence: `Meta+Ctrl+Alt+${entry.key}`,
            direction: entry.direction,
            follow: true,
        });
        rows.push({
            action: `omnitiler-send-output-${entry.direction}-arrow`,
            text: `Send window to output ${entry.direction} (follow)`,
            sequence: `Meta+Ctrl+Alt+${entry.arrow}`,
            direction: entry.direction,
            follow: true,
        });
        rows.push({
            action: `omnitiler-send-output-${entry.direction}-stay`,
            text: `Send window to output ${entry.direction} (stay)`,
            sequence: "",
            direction: entry.direction,
            follow: false,
        });
    }
    return Object.freeze(rows);
}

export interface PlanWorkspaceMigrateShortcutRow {
    readonly action: string;
    readonly text: string;
    readonly sequence: string;
    readonly direction: PlanDirection;
}

// Whole-workspace output migration shortcut catalog (R-WS-12): follow-only
// forms in four directions, bindable but unbound by default (empty sequence)
// for user rebinding. Native catalog/preset sync stays a later bounded unit.
export function planWorkspaceMigrateShortcutCatalog(): ReadonlyArray<PlanWorkspaceMigrateShortcutRow> {
    const rows: PlanWorkspaceMigrateShortcutRow[] = [];
    const dirs: ReadonlyArray<PlanDirection> = ["left", "right", "up", "down"];
    for (const direction of dirs) {
        rows.push({
            action: `omnitiler-migrate-workspace-${direction}`,
            text: `Migrate workspace to output ${direction} (follow)`,
            sequence: "",
            direction,
        });
    }
    return Object.freeze(rows);
}

const MAX_LIST = 1024;
const MAX_DESKTOPS = 32;
const MAX_ID_LEN = 128;
// C1 startup attach recovery: no fixed attempt budget. Each future native
// event that needs neither a windowList read nor an enabled adapter
// (workspace windowAdded, Options configChanged) runs one fresh full attach
// until success or stop; a session that can still make progress emits one of
// those edges, and a session that emits nothing has nothing to tile yet.
// stop() cancels the wait. Only transitions are logged: one failed line at
// wait start, one recovered line on success; re-attempts stay silent.

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

function resolveLexicalOptions(): unknown {
    try {
        const candidate: unknown = options;
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

type FrameRectFailure =
    | "frame-rect-missing"
    | "frame-rect-coordinate-invalid"
    | "frame-rect-size-invalid"
    | "frame-rect-coordinate-out-of-range"
    | "frame-rect-size-out-of-range";

type FrameRectRead = { x: number; y: number; w: number; h: number } | FrameRectFailure;

function readFrameRect(ref: object): FrameRectRead {
    const geometry = readProp(ref, "frameGeometry");
    if (typeof geometry !== "object" || geometry === null) {
        return "frame-rect-missing";
    }
    const record = geometry as Record<string, unknown>;
    const x = toQuantizedInt(record["x"]);
    const y = toQuantizedInt(record["y"]);
    if (x === null || y === null) {
        return "frame-rect-coordinate-invalid";
    }
    const widthRaw = record["width"] !== undefined ? record["width"] : record["w"];
    const heightRaw = record["height"] !== undefined ? record["height"] : record["h"];
    const w = toQuantizedInt(widthRaw);
    const h = toQuantizedInt(heightRaw);
    if (w === null || h === null || w <= 0 || h <= 0) {
        return "frame-rect-size-invalid";
    }
    if (x < -16384 || x > 16384 || y < -16384 || y > 16384) {
        return "frame-rect-coordinate-out-of-range";
    }
    if (w > 16384 || h > 16384) {
        return "frame-rect-size-out-of-range";
    }
    return { x, y, w, h };
}

function readConstraintSize(value: unknown): { w: number; h: number } | null {
    if (typeof value !== "object" || value === null) {
        return null;
    }
    const record = value as Record<string, unknown>;
    const width = toQuantizedInt(record["width"] !== undefined ? record["width"] : record["w"]);
    const height = toQuantizedInt(record["height"] !== undefined ? record["height"] : record["h"]);
    if (width === null || height === null || width < 0 || height < 0) {
        return null;
    }
    return { w: width, h: height };
}

function readWindowConstraints(ref: object): PlanWindowConstraints {
    const resizeable = readProp(ref, "resizeable");
    return {
        resizeable: typeof resizeable === "boolean" ? resizeable : null,
        minSize: readConstraintSize(readProp(ref, "minSize")),
        maxSize: readConstraintSize(readProp(ref, "maxSize")),
    };
}

// Directional shortcut catalog. All three configured profiles share one
// directional core: focus and move on letters plus arrows, resize
// parameterized by direction plus inwards/outwards mode. Both resize families
// have an arrow alias: shrink on Meta+Alt+Shift+arrows and grow on
// Meta+Alt+Left/Down/Up/Right, matching the same letter-pattern rows. The
// profile value is validated against the configured catalog names; unknown
// values fall back to the shared core so no unmapped input ever registers.
export function planShortcutCatalog(profile: unknown): ReadonlyArray<PlanShortcutRow> {
    void profile;
    const rows: PlanShortcutRow[] = [];
    const dirs: ReadonlyArray<{ direction: PlanDirection; key: string; arrow: string }> = [
        { direction: "left", key: "H", arrow: "Left" },
        { direction: "down", key: "J", arrow: "Down" },
        { direction: "up", key: "K", arrow: "Up" },
        { direction: "right", key: "L", arrow: "Right" },
    ];
    for (const entry of dirs) {
        rows.push({
            action: `omnitiler-focus-${entry.direction}`,
            text: `Focus window ${entry.direction}`,
            sequence: `Meta+${entry.key}`,
            op: "focus",
            direction: entry.direction,
            mode: null,
        });
        rows.push({
            action: `omnitiler-focus-${entry.direction}-arrow`,
            text: `Focus window ${entry.direction}`,
            sequence: `Meta+${entry.arrow}`,
            op: "focus",
            direction: entry.direction,
            mode: null,
        });
        rows.push({
            action: `omnitiler-move-${entry.direction}`,
            text: `Move window ${entry.direction}`,
            sequence: `Meta+Shift+${entry.key}`,
            op: "move",
            direction: entry.direction,
            mode: null,
        });
        rows.push({
            action: `omnitiler-move-${entry.direction}-arrow`,
            text: `Move window ${entry.direction}`,
            sequence: `Meta+Shift+${entry.arrow}`,
            op: "move",
            direction: entry.direction,
            mode: null,
        });
        rows.push({
            action: `omnitiler-resize-outwards-${entry.direction}`,
            text: `Grow window towards ${entry.direction}`,
            sequence: `Meta+Alt+${entry.key}`,
            op: "resize",
            direction: entry.direction,
            mode: "outwards",
        });
        rows.push({
            action: `omnitiler-resize-inwards-${entry.direction}`,
            text: `Shrink window from ${entry.direction}`,
            sequence: `Meta+Alt+Shift+${entry.key}`,
            op: "resize",
            direction: entry.direction,
            mode: "inwards",
        });
        rows.push({
            action: `omnitiler-resize-inwards-${entry.direction}-arrow`,
            text: `Shrink window from ${entry.direction}`,
            sequence: `Meta+Alt+Shift+${entry.arrow}`,
            op: "resize",
            direction: entry.direction,
            mode: "inwards",
        });
        rows.push({
            action: `omnitiler-resize-outwards-${entry.direction}-arrow`,
            text: `Grow window towards ${entry.direction}`,
            sequence: `Meta+Alt+${entry.arrow}`,
            op: "resize",
            direction: entry.direction,
            mode: "outwards",
        });
    }
    rows.push(
        {
            action: "omnitiler-toggle-float",
            text: "Toggle floating window",
            sequence: "Meta+G",
            op: "float",
            direction: null,
            mode: null,
        },
        {
            action: "omnitiler-toggle-sticky",
            text: "Toggle sticky floating window",
            sequence: "Meta+Shift+G",
            op: "sticky",
            direction: null,
            mode: null,
        },
        {
            action: "omnitiler-toggle-maximize",
            text: "Toggle maximize window",
            sequence: "Meta+M",
            op: "maximize",
            direction: null,
            mode: null,
        },
        {
            action: "omnitiler-toggle-fullscreen",
            text: "Toggle fullscreen window",
            sequence: "Meta+F11",
            op: "fullscreen",
            direction: null,
            mode: null,
        },
        {
            action: "omnitiler-toggle-orientation",
            text: "Toggle split orientation",
            sequence: "Meta+O",
            op: "toggle-orientation",
            direction: null,
            mode: null,
        },
    );
    return Object.freeze(rows);
}

function readShortcutProfile(readProfileFn: (() => unknown) | undefined): string {
    if (readProfileFn !== undefined) {
        let value: unknown = undefined;
        try {
            value = readProfileFn();
        } catch (error) {
            void error;
            return "cosmic";
        }
        if (value === "cosmic" || value === "hyprland" || value === "bspwm") {
            return value as string;
        }
        return "cosmic";
    }
    try {
        const value: unknown = readConfig("shortcutProfile", "cosmic");
        if (value === "cosmic" || value === "hyprland" || value === "bspwm") {
            return value as string;
        }
    } catch (error) {
        void error;
    }
    return "cosmic";
}

function readWorkspaceModeValue(readModeFn: (() => unknown) | undefined): unknown {
    if (readModeFn !== undefined) {
        try {
            return readModeFn();
        } catch (error) {
            void error;
            return "per-output-local";
        }
    }
    try {
        return readConfig("workspaceMode", "per-output-local");
    } catch (error) {
        void error;
        return "per-output-local";
    }
}

function readTilingDefaultValue(readDefaultFn: (() => unknown) | undefined): unknown {
    if (readDefaultFn !== undefined) {
        try {
            return readDefaultFn();
        } catch (error) {
            void error;
            return true;
        }
    }
    try {
        return readConfig("defaultTiled", true);
    } catch (error) {
        void error;
        return true;
    }
}

function readWorkAreaFor(
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
        // PlacementArea (0) is the per-output usable area. WorkArea (5) is
        // the whole work area across all screens and must never size a
        // per-output domain.
        area = Reflect.apply(areaFn as (...args: ReadonlyArray<never>) => unknown, surface, [0, outputRef, desktopRef]);
    } catch (error) {
        void error;
        return null;
    }
    if (typeof area !== "object" || area === null) {
        return null;
    }
    const record = area as Record<string, unknown>;
    const bx = toQuantizedInt(record["x"]);
    const by = toQuantizedInt(record["y"]);
    const bwRaw = record["width"] !== undefined ? record["width"] : record["w"];
    const bhRaw = record["height"] !== undefined ? record["height"] : record["h"];
    const bw = toQuantizedInt(bwRaw);
    const bh = toQuantizedInt(bhRaw);
    if (bx === null || by === null || bw === null || bh === null) {
        return null;
    }
    if (bw <= 0 || bh <= 0 || bw > 16384 || bh > 16384 || bx < -16384 || bx > 16384 || by < -16384 || by > 16384) {
        return null;
    }
    return { x: bx, y: by, w: bw, h: bh };
}

// FULL output rectangle for directional adjacency (item 5.2): the Output
// `geometry` rect, not any clientArea work area, so panel gaps cannot break
// edge-touch selection. Read-only public property; null fail-closed on any
// unreadable shape so the caller refuses rather than inventing topology.
function readFullOutputRect(outputRef: object): { x: number; y: number; w: number; h: number } | null {
    let geometry: unknown = undefined;
    try {
        geometry = readProp(outputRef, "geometry");
    } catch (error) {
        void error;
        return null;
    }
    if (typeof geometry !== "object" || geometry === null) {
        return null;
    }
    const record = geometry as Record<string, unknown>;
    const bx = toQuantizedInt(record["x"]);
    const by = toQuantizedInt(record["y"]);
    const bwRaw = record["width"] !== undefined ? record["width"] : record["w"];
    const bhRaw = record["height"] !== undefined ? record["height"] : record["h"];
    const bw = toQuantizedInt(bwRaw);
    const bh = toQuantizedInt(bhRaw);
    if (bx === null || by === null || bw === null || bh === null) {
        return null;
    }
    if (bw <= 0 || bh <= 0 || bw > 16384 || bh > 16384 || bx < -16384 || bx > 16384 || by < -16384 || by > 16384) {
        return null;
    }
    return { x: bx, y: by, w: bw, h: bh };
}

// One live read of every output's FULL rectangle plus its current logical
// workspace. Shared by directional adjacency selection and explicit output
// send. Null fail-closed on any unreadable shape so callers refuse rather
// than inventing topology.
interface OutputTopologyEntry {
    readonly ref: object;
    readonly name: string;
    readonly desktopRef: object;
    readonly workspace: string;
    readonly bounds: { x: number; y: number; w: number; h: number };
}

function readOutputTopology(liveWorkspace: unknown): OutputTopologyEntry[] | null {
    try {
        if (typeof liveWorkspace !== "object" || liveWorkspace === null) {
            return null;
        }
        const surface = liveWorkspace as Record<string, unknown>;
        const screens = decodeList(readProp(surface, "screens"), MAX_LIST);
        if (screens === null || screens.length === 0 || screens.length > MAX_LIST) {
            return null;
        }
        // Current desktop per output via the workspace-native capability
        // seam: the per-output read on 6.7+, the shared currentDesktop (then
        // current on every output) on legacy-shared hosts. Null fail-closed.
        const entries: OutputTopologyEntry[] = [];
        for (const item of screens) {
            if (typeof item !== "object" || item === null) {
                return null;
            }
            const ref = item as object;
            const nameRaw = readProp(ref, "name");
            if (!isOpaqueId(nameRaw)) {
                return null;
            }
            const read = readDesktopForOutput(surface, ref);
            if (read === null) {
                return null;
            }
            const desktopRef = read.ref;
            const desktopIdRaw = read.id;
            // FULL output rectangle: panel gaps must not break adjacency.
            const bounds = readFullOutputRect(ref);
            if (bounds === null) {
                return null;
            }
            entries.push({
                ref,
                name: nameRaw as string,
                desktopRef,
                workspace: desktopIdRaw as string,
                bounds,
            });
        }
        // Duplicate output names cannot select: fail closed rather than
        // resolving an ambiguous identity.
        const seenNames = new Set<string>();
        for (const entry of entries) {
            if (seenNames.has(entry.name)) {
                return null;
            }
            seenNames.add(entry.name);
        }
        return entries;
    } catch (error) {
        void error;
        return null;
    }
}

// Unique reciprocal exact edge-touch selection on FULL output rectangles in
// the commanded direction. FOCUS-ONLY legacy: returns the single adjacent
// entry, null for a confirmed no-candidate condition, or "ambiguous" for
// refusal. Focus navigation keeps this policy unchanged. Moves, sends, and
// migration use the position-based selectors below. No output wrapping.
function selectAdjacentOutput(
    entries: ReadonlyArray<OutputTopologyEntry>,
    source: OutputTopologyEntry,
    direction: string,
): OutputTopologyEntry | null | "ambiguous" {
    const overlapY = (
        a: { y: number; h: number },
        b: { y: number; h: number },
    ): number => Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
    const overlapX = (
        a: { x: number; w: number },
        b: { x: number; w: number },
    ): number => Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
    const touches = (entry: OutputTopologyEntry): boolean => {
        if (direction === "left") {
            return entry.bounds.x + entry.bounds.w === source.bounds.x && overlapY(entry.bounds, source.bounds) > 0;
        }
        if (direction === "right") {
            return source.bounds.x + source.bounds.w === entry.bounds.x && overlapY(entry.bounds, source.bounds) > 0;
        }
        if (direction === "up") {
            return entry.bounds.y + entry.bounds.h === source.bounds.y && overlapX(entry.bounds, source.bounds) > 0;
        }
        if (direction === "down") {
            return source.bounds.y + source.bounds.h === entry.bounds.y && overlapX(entry.bounds, source.bounds) > 0;
        }
        return false;
    };
    const candidates = entries.filter((entry) => entry.name !== source.name && touches(entry));
    if (candidates.length === 0) {
        return null;
    }
    if (candidates.length !== 1) {
        return "ambiguous";
    }
    const target = candidates[0] as OutputTopologyEntry;
    // Reciprocal uniqueness: the target's opposite side must resolve back
    // to exactly one candidate across the complete topology, and it must be
    // the source. A merely symmetric edge touch is tautological (selection
    // already established it); a target touched by two sources on its
    // opposite side is ambiguous and refuses.
    const opposite = direction === "left" ? "right" : direction === "right" ? "left" : direction === "up" ? "down" : "up";
    const touchesReverse = (entry: OutputTopologyEntry): boolean => {
        if (opposite === "left") {
            return entry.bounds.x + entry.bounds.w === target.bounds.x && overlapY(entry.bounds, target.bounds) > 0;
        }
        if (opposite === "right") {
            return target.bounds.x + target.bounds.w === entry.bounds.x && overlapY(entry.bounds, target.bounds) > 0;
        }
        if (opposite === "up") {
            return entry.bounds.y + entry.bounds.h === target.bounds.y && overlapX(entry.bounds, target.bounds) > 0;
        }
        return target.bounds.y + target.bounds.h === entry.bounds.y && overlapX(entry.bounds, target.bounds) > 0;
    };
    const reverse = entries.filter((entry) => entry.name !== target.name && touchesReverse(entry));
    if (reverse.length !== 1 || (reverse[0] as OutputTopologyEntry).name !== source.name) {
        return "ambiguous";
    }
    return target;
}

// Shared FULL-rectangle candidate helper for the 2026-10-09 position-based
// selectors (moves/sends/migration). Lists every exact edge-touch candidate
// with positive overlap plus its shared edge interval. Deterministic
// left/top/name order. Mirrors tiler-core::output_selection.
function listPositionCandidates(
    entries: ReadonlyArray<OutputTopologyEntry>,
    source: OutputTopologyEntry,
    direction: string,
): Array<{ entry: OutputTopologyEntry; edgeStart: number; edgeEnd: number }> {
    const overlapY = (a: { y: number; h: number }, b: { y: number; h: number }): number =>
        Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
    const overlapX = (a: { x: number; w: number }, b: { x: number; w: number }): number =>
        Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
    const out: Array<{ entry: OutputTopologyEntry; edgeStart: number; edgeEnd: number }> = [];
    for (const entry of entries) {
        if (entry.name === source.name) {
            continue;
        }
        let touched = false;
        if (direction === "left") {
            touched = entry.bounds.x + entry.bounds.w === source.bounds.x && overlapY(entry.bounds, source.bounds) > 0;
        } else if (direction === "right") {
            touched = source.bounds.x + source.bounds.w === entry.bounds.x && overlapY(entry.bounds, source.bounds) > 0;
        } else if (direction === "up") {
            touched = entry.bounds.y + entry.bounds.h === source.bounds.y && overlapX(entry.bounds, source.bounds) > 0;
        } else if (direction === "down") {
            touched = source.bounds.y + source.bounds.h === entry.bounds.y && overlapX(entry.bounds, source.bounds) > 0;
        }
        if (!touched) {
            continue;
        }
        let start = 0;
        let end = 0;
        if (direction === "left" || direction === "right") {
            start = Math.max(source.bounds.y, entry.bounds.y);
            end = Math.min(source.bounds.y + source.bounds.h, entry.bounds.y + entry.bounds.h);
        } else {
            start = Math.max(source.bounds.x, entry.bounds.x);
            end = Math.min(source.bounds.x + source.bounds.w, entry.bounds.x + entry.bounds.w);
        }
        if (!(end - start > 0)) {
            continue;
        }
        out.push({ entry, edgeStart: start, edgeEnd: end });
    }
    out.sort((a, b) => {
        if (a.entry.bounds.x !== b.entry.bounds.x) {
            return a.entry.bounds.x - b.entry.bounds.x;
        }
        if (a.entry.bounds.y !== b.entry.bounds.y) {
            return a.entry.bounds.y - b.entry.bounds.y;
        }
        return a.entry.name < b.entry.name ? -1 : a.entry.name > b.entry.name ? 1 : 0;
    });
    return out;
}

// Windowed selection for exhausted moves and explicit sends: shared edge
// containing the moving window centre projection wins (horizontal uses y,
// vertical uses x, half-open [start, end) in doubled coordinates so odd
// extents keep exact .5 centres); else largest window-span overlap with the
// shared edge; final left/top/name. Null mover rect refuses ("ambiguous").
// Candidates already guarantee forward edge-touch, so no second check.
function selectWindowedAdjacentOutput(
    entries: ReadonlyArray<OutputTopologyEntry>,
    source: OutputTopologyEntry,
    direction: string,
    mover: { x: number; y: number; w: number; h: number } | null,
): OutputTopologyEntry | null | "ambiguous" {
    if (mover === null || !(mover.w > 0 && mover.h > 0)) {
        return "ambiguous";
    }
    const candidates = listPositionCandidates(entries, source, direction);
    if (candidates.length === 0) {
        return null;
    }
    const horizontal = direction === "left" || direction === "right";
    const overlapWith = (c: { edgeStart: number; edgeEnd: number }): number => {
        if (horizontal) {
            return Math.max(0, Math.min(mover.y + mover.h, c.edgeEnd) - Math.max(mover.y, c.edgeStart));
        }
        return Math.max(0, Math.min(mover.x + mover.w, c.edgeEnd) - Math.max(mover.x, c.edgeStart));
    };
    const centred = (c: { edgeStart: number; edgeEnd: number }): boolean => {
        const doubled = horizontal ? 2 * mover.y + mover.h : 2 * mover.x + mover.w;
        return 2 * c.edgeStart <= doubled && doubled < 2 * c.edgeEnd;
    };
    const ranked = [...candidates].sort((a, b) => {
        const centreDiff = Number(centred(b)) - Number(centred(a));
        if (centreDiff !== 0) {
            return centreDiff;
        }
        // Literal contract tie-break: span overlap ranks only when neither
        // shared edge contains the centre. Two containing edges skip
        // straight to left/top/name.
        if (centred(a) && centred(b)) {
            // fall through to left/top/name below
        } else {
            const diff = overlapWith(b) - overlapWith(a);
            if (diff !== 0) {
                return diff;
            }
        }
        if (a.entry.bounds.x !== b.entry.bounds.x) {
            return a.entry.bounds.x - b.entry.bounds.x;
        }
        if (a.entry.bounds.y !== b.entry.bounds.y) {
            return a.entry.bounds.y - b.entry.bounds.y;
        }
        return a.entry.name < b.entry.name ? -1 : a.entry.name > b.entry.name ? 1 : 0;
    });
    return (ranked[0] as { entry: OutputTopologyEntry }).entry;
}

// Migration selection: largest shared edge, then left/top/name. Never uses
// window position. Candidates already guarantee forward edge-touch. Never
// ambiguous: empty is no-target, otherwise the ranked head wins.
function selectMigrationAdjacentOutput(
    entries: ReadonlyArray<OutputTopologyEntry>,
    source: OutputTopologyEntry,
    direction: string,
): OutputTopologyEntry | null {
    const candidates = listPositionCandidates(entries, source, direction);
    if (candidates.length === 0) {
        return null;
    }
    const ranked = [...candidates].sort((a, b) => {
        const diff = b.edgeEnd - b.edgeStart - (a.edgeEnd - a.edgeStart);
        if (diff !== 0) {
            return diff;
        }
        if (a.entry.bounds.x !== b.entry.bounds.x) {
            return a.entry.bounds.x - b.entry.bounds.x;
        }
        if (a.entry.bounds.y !== b.entry.bounds.y) {
            return a.entry.bounds.y - b.entry.bounds.y;
        }
        return a.entry.name < b.entry.name ? -1 : a.entry.name > b.entry.name ? 1 : 0;
    });
    return (ranked[0] as { entry: OutputTopologyEntry }).entry;
}

// STRICT TRUE live native perOutputVirtualDesktops gate for whole-workspace
// migration only (R-WS-12): true passes, false refuses, anything unreadable
// refuses. Existing mode behavior is untouched and no native option/config
// write ever happens here. The source is the override when tests supply one,
// else the live lexical KWin `options` global. The reader itself lives in the
// workspace-native capability seam (imported above) so the 6.7-only property
// stays confined there; on legacy-shared hosts the policy gate below refuses
// on the unsupported API before consulting it.
export interface MigrateWorkspacePolicy {
    readonly mode: string;
    readonly perOutput: boolean | null;
}

// Read-only fixed-size origin decoration for migration observation: the
// adapter-asserted provenance for one stable id and live ref, or null when
// unrecorded or replaced. Never classifies; the planner owns admission.
export interface MigrateOriginResolver {
    readonly originOf: (
        id: string,
        ref: object,
    ) => { readonly fixedAuto: boolean; readonly fixedSuppress: boolean } | null;
    // Live tiling mode for the migrating workspace id. Absent resolvers
    // read tiled; the adapter's own mode hook stays authoritative and the
    // two must agree at dispatch.
    readonly tilingOf?: (workspaceId: string) => boolean;
}

export type MigrateWorkspaceObservation =
    | { readonly status: "ready"; readonly observed: WorkspaceMigrateObserved }
    | { readonly status: "no-target"; readonly observed: null }
    | { readonly status: "invalid"; readonly observed: null; readonly reason?: string };

// Production whole-workspace migration observation (R-WS-12): the source
// output's current workspace as a FULL normal-client view INCLUDING sticky
// (sticky rides with sticky/fit-excluded flags and stays homed on the
// source), plus the adjacent target output's FULL current view for the
// affected-view overlay gate and the drift fence. Selection ranks FULL
// output rectangles by largest shared edge, then left/top (panel gaps
// cannot break adjacency): a confirmed no-candidate condition is
// `no-target` (quiet no-op), ambiguous or unreadable evidence is `invalid`
// and must refuse before any mutation. No output wrapping. Carried domain
// bounds stay per-desktop work areas (never tiled under panels): the source
// area for the migrating desktop plus the target-output area for that same
// backing desktop. The focused id names the migrated active client, the
// sticky active client, or "" (a dangling focus refuses downstream, never
// fabricated). Minimized windows ride no wire but still trip the protected
// tripwire when fullscreen/maximized. Null fail-closed everywhere.
export function observeMigrateWorkspace(
    liveWorkspace: unknown,
    cache: Map<string, string>,
    floatingIds: ReadonlySet<string>,
    gaps: DomainGaps,
    direction: string,
    policy: MigrateWorkspacePolicy,
    owners?: Map<string, object>,
    pinned?: WorkspaceMigratePin,
    origins?: MigrateOriginResolver,
): MigrateWorkspaceObservation {
    const gap = (reason: string): MigrateWorkspaceObservation => ({ status: "invalid", observed: null, reason });
    const invalid: MigrateWorkspaceObservation = { status: "invalid", observed: null };
    const noTarget: MigrateWorkspaceObservation = { status: "no-target", observed: null };
    try {
        if (!isMigrateDirection(direction)) {
            return invalid;
        }
        void gaps;
        if (typeof liveWorkspace !== "object" || liveWorkspace === null) {
            return invalid;
        }
        const surface = liveWorkspace as Record<string, unknown>;
        let active: unknown = undefined;
        try {
            active = Reflect.get(surface, "activeWindow");
        } catch (error) {
            void error;
            active = undefined;
        }
        const activeRef = typeof active === "object" && active !== null ? (active as object) : null;
        const topology = readOutputTopology(liveWorkspace);
        if (topology === null || topology.length === 0) {
            return invalid;
        }
        // Source output: the flight pin, the active window's output, or
        // the live activeScreen. No first-screen default: an unreadable
        // exact live output fails closed.
        let sourceOutput = "";
        if (pinned !== undefined) {
            if (!isOpaqueId(pinned.sourceOutput) || !isOpaqueId(pinned.sourceWorkspace) || !isOpaqueId(pinned.targetOutput)) {
                return invalid;
            }
            sourceOutput = pinned.sourceOutput;
        } else if (activeRef !== null) {
            const activeOutput = readProp(activeRef, "output");
            if (typeof activeOutput !== "object" || activeOutput === null) {
                return invalid;
            }
            const nameRaw = readProp(activeOutput as object, "name");
            if (!isOpaqueId(nameRaw)) {
                return invalid;
            }
            sourceOutput = nameRaw as string;
        } else {
            let screen: unknown = undefined;
            try {
                screen = Reflect.get(surface, "activeScreen");
            } catch (error) {
                void error;
                screen = undefined;
            }
            if (typeof screen !== "object" || screen === null) {
                return invalid;
            }
            const nameRaw = readProp(screen as object, "name");
            if (!isOpaqueId(nameRaw)) {
                return invalid;
            }
            sourceOutput = nameRaw as string;
        }
        const sourceEntry = topology.find((entry) => entry.name === sourceOutput);
        if (sourceEntry === undefined) {
            return invalid;
        }
        const migratingId = pinned !== undefined ? pinned.sourceWorkspace : sourceEntry.workspace;
        // Phase-aware source view: pre-write the source must still show the
        // migrating id; after the flight's own source refill it must show
        // the frozen refill instead. Anything else is drift.
        const expectedSourceView = pinned?.expectViews?.source ?? migratingId;
        if (sourceEntry.workspace !== expectedSourceView) {
            return invalid;
        }
        // Whole-workspace migration ranks by largest shared FULL edge, then
        // left/top/name, never window position. No candidate is no-target;
        // unreadable topology refuses.
        const selected = selectMigrationAdjacentOutput(topology, sourceEntry, direction);
        if (selected === null) {
            return noTarget;
        }
        if (pinned !== undefined && selected.name !== pinned.targetOutput) {
            return invalid;
        }
        const targetEntry = selected;
        // Phase-aware target view: pre-write a target already showing the
        // migrating workspace is a stale scope; after the flight's own
        // target switch it must show exactly that.
        const expectedTargetView = pinned?.expectViews?.target;
        if (expectedTargetView !== undefined) {
            if (targetEntry.workspace !== expectedTargetView) {
                return invalid;
            }
        } else if (targetEntry.workspace === migratingId) {
            return invalid;
        }
        const desktops = decodeList(readProp(surface, "desktops"), MAX_DESKTOPS);
        if (desktops === null || desktops.length === 0) {
            return invalid;
        }
        let migratedDesktopRef: object | null = null;
        for (const item of desktops) {
            if (typeof item !== "object" || item === null) {
                continue;
            }
            if (readProp(item as object, "id") === migratingId) {
                migratedDesktopRef = item as object;
                break;
            }
        }
        if (migratedDesktopRef === null) {
            return invalid;
        }
        const sourceBounds = readWorkAreaFor(surface, sourceEntry.ref, migratedDesktopRef);
        if (sourceBounds === null) {
            return invalid;
        }
        const targetBounds = readWorkAreaFor(surface, targetEntry.ref, migratedDesktopRef);
        if (targetBounds === null) {
            return invalid;
        }
        const lister = readProp(surface, "windowList");
        if (typeof lister !== "function") {
            return invalid;
        }
        let rawList: unknown = undefined;
        try {
            rawList = Reflect.apply(lister as (...args: ReadonlyArray<never>) => unknown, surface, []);
        } catch (error) {
            void error;
            return invalid;
        }
        const windows = decodeList(rawList, MAX_LIST);
        if (windows === null) {
            return invalid;
        }
        const memberOf = (ref: object, workspaceId: string): boolean | null => {
            const membership = decodeList(readProp(ref, "desktops"), MAX_DESKTOPS);
            if (membership === null) {
                return null;
            }
            for (const member of membership) {
                if (typeof member !== "object" || member === null) {
                    return null;
                }
                if (readProp(member as object, "id") === workspaceId) {
                    return true;
                }
            }
            return false;
        };
        type CollectedWindow = {
            id: string;
            ref: object;
            rect: { x: number; y: number; w: number; h: number };
            output: string;
            floating: boolean;
            sticky: boolean;
            fullscreen: boolean;
            maximized: boolean;
            fitExcluded: boolean;
            minimized: boolean;
            transient: boolean;
            fixedAuto: boolean;
            fixedSuppress: boolean;
            minSize: { w: number; h: number } | null;
            maxSize: { w: number; h: number } | null;
        };
        type RelatedWindow = {
            id: string;
            ref: object;
            output: string;
            parentId: string | null;
            transient: boolean;
            minimized: boolean;
            floating: boolean;
            sticky: boolean;
            fullscreen: boolean;
            maximized: boolean;
            fitExcluded: boolean;
        };
        const sourceWindows: CollectedWindow[] = [];
        const targetViewWindows: CollectedWindow[] = [];
        const relatedWindows: RelatedWindow[] = [];
        const seen = new Set<string>();
        const memberRefs = new Set<object>();
        let protectedPresent = false;
        // Transient walk: resolve the topmost migrating ancestor of one
        // window through `transientFor`, bounded and cycle-guarded. Returns
        // the carried member ref reached, "unlinked" when the chain ends
        // outside the migrating set, or null when classification is
        // impossible (missing API, cycle, bound, unreadable hop): the
        // caller stops with the exact transient-unknown gap instead of
        // inventing a refusal or a silent carry.
        const transientMemberParent = (start: object): object | "unlinked" | null => {
            const visited = new Set<object>([start]);
            let current = start;
            for (let hop = 0; hop < 8; hop += 1) {
                let parent: unknown = undefined;
                try {
                    parent = readProp(current, "transientFor");
                } catch (error) {
                    void error;
                    return null;
                }
                if (parent === null || parent === undefined) {
                    return "unlinked";
                }
                if (typeof parent !== "object") {
                    return null;
                }
                if (memberRefs.has(parent as object)) {
                    return parent as object;
                }
                if (visited.has(parent as object)) {
                    return null;
                }
                visited.add(parent as object);
                current = parent as object;
            }
            return null;
        };
        const isInView = (ref: object, outputName: string, workspaceId: string): boolean | null => {
            if (outputName !== sourceOutput && outputName !== targetEntry.name) {
                return false;
            }
            const onWorkspace = memberOf(ref, workspaceId);
            if (onWorkspace === null) {
                return null;
            }
            if (!onWorkspace) {
                return false;
            }
            return true;
        };
        for (const item of windows) {
            if (typeof item !== "object" || item === null) {
                continue;
            }
            const ref = item as object;
            if (readProp(ref, "normalWindow") !== true) {
                continue;
            }
            const managed = readProp(ref, "managed");
            if (managed !== undefined && managed !== true) {
                continue;
            }
            const output = readProp(ref, "output");
            if (typeof output !== "object" || output === null) {
                continue;
            }
            const outputNameRaw = readProp(output as object, "name");
            if (!isOpaqueId(outputNameRaw)) {
                return invalid;
            }
            const outputName = outputNameRaw as string;
            // Migrating members may already sit on either output once the
            // flight pin is observed across transfer; the affected target
            // view always rides along for the overlay gate and drift fence.
            const inScopeOutput = outputName === sourceOutput || outputName === targetEntry.name;
            if (!inScopeOutput) {
                continue;
            }
            const sticky = readProp(ref, "onAllDesktops") === true;
            const onMigrating = memberOf(ref, migratingId);
            if (onMigrating === null) {
                return invalid;
            }
            const onTargetCurrent = memberOf(ref, targetEntry.workspace);
            if (onTargetCurrent === null) {
                return invalid;
            }
            // Source view: migrating-desktop members on the source output,
            // plus sticky windows living on the source output (they stay
            // homed there). Under a flight pin, migrated members already
            // sitting on the target output still belong here by backing id
            // so post-transfer verification keeps the frozen set. Minimized
            // members ride along natively (no wire, no geometry, no focus).
            // Target view: the target current desktop plus sticky living
            // there, affected-view gate and drift fence only, never moved.
            const inSourceView =
                (outputName === sourceOutput && (onMigrating || sticky)) ||
                (pinned !== undefined && outputName === targetEntry.name && onMigrating && !sticky);
            const inTargetView =
                outputName === targetEntry.name && (onTargetCurrent || sticky);
            if (!inSourceView && !inTargetView) {
                continue;
            }
            const fullscreen = readProp(ref, "fullScreen") !== false;
            const maximized = readProp(ref, "maximizeMode") !== 0;
            const minimized = readProp(ref, "minimized") === true;
            let native: string | null = null;
            try {
                native = normalizeNativeId(readProp(ref, "internalId"));
            } catch (error) {
                void error;
                return invalid;
            }
            if (native === null) {
                return invalid;
            }
            const id = internNativeId(cache, native, ref, owners);
            if (seen.has(id)) {
                return invalid;
            }
            seen.add(id);
            const frame = readFrameRect(ref);
            if (typeof frame === "string") {
                return invalid;
            }
            // Adapter-asserted fixed-size provenance decorates the
            // observation (read-only; the planner owns admission).
            // Automatic floats observe floating like the ordinary
            // observation path (isEffectivelyFloating parity): they ride
            // the wire as floating exceptions with fixed_auto and keep
            // their origin across the migration. Tile overrides stay
            // tiled by explicit user win.
            let fixedAuto = false;
            let fixedSuppress = false;
            if (origins !== undefined) {
                let origin: { readonly fixedAuto: boolean; readonly fixedSuppress: boolean } | null = null;
                try {
                    origin = origins.originOf(id, ref);
                } catch (error) {
                    void error;
                    origin = null;
                }
                if (origin !== null) {
                    fixedAuto = origin.fixedAuto === true;
                    fixedSuppress = origin.fixedSuppress === true;
                }
            }
            const floating = floatingIds.has(id) || sticky || fixedAuto;
            const fitExcluded = floating || sticky || fullscreen || maximized;
            if (fullscreen || maximized) {
                protectedPresent = true;
            }
            const transient = readProp(ref, "transient") === true;
            let minSize: { w: number; h: number } | null = null;
            let maxSize: { w: number; h: number } | null = null;
            try {
                const constraints = readWindowConstraints(ref);
                if (constraints.minSize !== null) {
                    minSize = { w: constraints.minSize.w, h: constraints.minSize.h };
                }
                if (constraints.maxSize !== null) {
                    maxSize = { w: constraints.maxSize.w, h: constraints.maxSize.h };
                }
            } catch (error) {
                void error;
            }
            const collected: CollectedWindow = {
                id,
                ref,
                rect: { x: frame.x, y: frame.y, w: frame.w, h: frame.h },
                output: outputName,
                floating,
                sticky,
                fullscreen,
                maximized,
                fitExcluded,
                minimized,
                transient,
                fixedAuto,
                fixedSuppress,
                minSize,
                maxSize,
            };
            memberRefs.add(ref);
            // A sticky window on the source output belongs to the migrating
            // view (it stays homed on the source); anywhere else it only
            // joins the affected target view for the gate.
            if (inSourceView) {
                sourceWindows.push(collected);
            } else {
                targetViewWindows.push(collected);
            }
        }
        // Partition pass: non-sticky transient descendants of carried
        // migrating members move with their parent implicitly, so they
        // leave the carried lists for related tracking (no explicit
        // setter, no wire, never reclassified). Sticky transients stay
        // carried-sticky (never touched either way). A transient chain
        // that cannot be resolved stops with transient-unknown, and a
        // parent that left the carried set between passes fails closed.
        const carriedByRef = new Map<object, { list: CollectedWindow[]; index: number }>();
        for (const list of [sourceWindows, targetViewWindows]) {
            list.forEach((entry, index) => {
                carriedByRef.set(entry.ref, { list, index });
            });
        }
        const memberIdByRef = new Map<object, string>();
        for (const entry of sourceWindows) {
            memberIdByRef.set(entry.ref, entry.id);
        }
        // Topmost carried migrating ancestor: climb past detached entries
        // so a transient chain never names another related window.
        const topCarriedParent = (start: object): string | "unlinked" | null => {
            let current: object | "unlinked" | null = transientMemberParent(start);
            const climbed = new Set<object>();
            while (current !== null && current !== "unlinked") {
                const carriedId = memberIdByRef.get(current);
                if (carriedId !== undefined) {
                    return carriedId;
                }
                if (climbed.has(current)) {
                    return null;
                }
                climbed.add(current);
                let next: unknown = undefined;
                try {
                    next = readProp(current, "transientFor");
                } catch (error) {
                    void error;
                    return null;
                }
                if (typeof next !== "object" || next === null) {
                    return "unlinked";
                }
                current = next as object;
            }
            return current;
        };
        const detachToRelated: CollectedWindow[] = [];
        for (const entry of [...sourceWindows, ...targetViewWindows]) {
            if (entry.sticky || entry.transient !== true) {
                continue;
            }
            // Reachability only here: any migrating member in the chain
            // detaches this entry. Topmost resolution runs after the
            // splice against the surviving carried set.
            const reached = transientMemberParent(entry.ref);
            if (reached === null) {
                return gap("transient-unknown");
            }
            if (reached === "unlinked") {
                continue;
            }
            detachToRelated.push(entry);
        }
        for (const entry of detachToRelated) {
            const slot = carriedByRef.get(entry.ref);
            if (slot !== undefined) {
                slot.list.splice(slot.index, 1);
            }
        }
        // Rebuild the ref index after partition: detached entries are
        // related now, never carried.
        carriedByRef.clear();
        for (const list of [sourceWindows, targetViewWindows]) {
            list.forEach((entry, index) => {
                carriedByRef.set(entry.ref, { list, index });
            });
        }
        memberIdByRef.clear();
        for (const entry of sourceWindows) {
            memberIdByRef.set(entry.ref, entry.id);
        }
        for (const entry of detachToRelated) {
            const parentId = topCarriedParent(entry.ref);
            if (parentId === null) {
                return gap("transient-unknown");
            }
            if (parentId === "unlinked") {
                // The transient parent left the carried set between passes
                // (external drift): fail closed, never guess.
                return invalid;
            }
            relatedWindows.push({
                id: entry.id,
                ref: entry.ref,
                output: entry.output,
                parentId,
                transient: true,
                minimized: entry.minimized,
                floating: entry.floating,
                sticky: false,
                fullscreen: entry.fullscreen,
                maximized: entry.maximized,
                fitExcluded: entry.fitExcluded,
            });
            if (entry.fullscreen || entry.maximized) {
                protectedPresent = true;
            }
        }
        // Related pass over uncollected windows on scope outputs: transient
        // descendants (any classification, even dialog/non-normal) plus
        // protected clients in either affected view. Never moved, never
        // wired; the gate sees what the carried lists cannot. Detached
        // entries are already related and never re-examined.
        const detachedRefs = new Set<object>(detachToRelated.map((entry) => entry.ref));
        for (const item of windows) {
            if (typeof item !== "object" || item === null) {
                continue;
            }
            const ref = item as object;
            if (carriedByRef.has(ref) || detachedRefs.has(ref)) {
                continue;
            }
            const output = readProp(ref, "output");
            if (typeof output !== "object" || output === null) {
                continue;
            }
            const outputNameRaw = readProp(output as object, "name");
            if (outputNameRaw !== sourceOutput && outputNameRaw !== targetEntry.name) {
                continue;
            }
            const outputName = outputNameRaw as string;
            const isTransient = readProp(ref, "transient") === true;
            let parentId: string | null = null;
            if (isTransient) {
                const parent = transientMemberParent(ref);
                if (parent === null) {
                    return gap("transient-unknown");
                }
                if (parent !== "unlinked") {
                    const carriedId = memberIdByRef.get(parent);
                    if (carriedId === undefined) {
                        return invalid;
                    }
                    parentId = carriedId;
                }
            }
            const fullscreen = readProp(ref, "fullScreen") !== false;
            const maximized = readProp(ref, "maximizeMode") !== 0;
            if (parentId === null && !fullscreen && !maximized) {
                continue;
            }
            if (parentId === null) {
                // Protected view-local client: prove affected-view scope
                // before gating on it.
                const inSource = isInView(ref, outputName, migratingId);
                if (inSource === null) {
                    return invalid;
                }
                const inTarget = isInView(ref, outputName, targetEntry.workspace);
                if (inTarget === null) {
                    return invalid;
                }
                const sticky = readProp(ref, "onAllDesktops") === true;
                const scoped =
                    (outputName === sourceOutput && (inSource || sticky)) ||
                    (outputName === targetEntry.name && (inTarget || sticky));
                if (!scoped) {
                    continue;
                }
            }
            let native: string | null = null;
            try {
                native = normalizeNativeId(readProp(ref, "internalId"));
            } catch (error) {
                void error;
                return invalid;
            }
            if (native === null) {
                return invalid;
            }
            const id = internNativeId(cache, native, ref, owners);
            if (seen.has(id)) {
                return invalid;
            }
            seen.add(id);
            const minimized = readProp(ref, "minimized") === true;
            const sticky = readProp(ref, "onAllDesktops") === true;
            const floating = floatingIds.has(id) || sticky;
            relatedWindows.push({
                id,
                ref,
                output: outputName,
                parentId,
                transient: isTransient,
                minimized,
                floating,
                sticky,
                fullscreen,
                maximized,
                fitExcluded: floating || sticky || fullscreen || maximized,
            });
            if (fullscreen || maximized) {
                protectedPresent = true;
            }
        }
        // Focus names a carried visible member only: sticky actives stay
        // homed (null echo downstream) and minimized clients never take
        // focus. Anything else blanks, never fabricated.
        let focusedId = "";
        if (activeRef !== null) {
            let activeNative: string | null = null;
            try {
                activeNative = normalizeNativeId(readProp(activeRef, "internalId"));
            } catch (error) {
                void error;
                return invalid;
            }
            if (activeNative !== null) {
                const activeId = internNativeId(cache, activeNative, activeRef, owners);
                for (const entry of sourceWindows) {
                    if (entry.id === activeId && !entry.minimized) {
                        focusedId = entry.id;
                        break;
                    }
                }
            }
        }
        const sourceSorted = [...sourceWindows, ...relatedWindows].map((entry) => entry.id).sort();
        const targetSorted = targetViewWindows.map((entry) => entry.id).sort();
        let sourceTiled = true;
        if (origins?.tilingOf !== undefined) {
            try {
                sourceTiled = origins.tilingOf(migratingId);
            } catch (error) {
                void error;
                return invalid;
            }
        }
        return {
            status: "ready",
            observed: {
                direction,
                sourceOutput,
                sourceWorkspace: migratingId,
                sourceBounds: { x: sourceBounds.x, y: sourceBounds.y, w: sourceBounds.w, h: sourceBounds.h },
                targetOutput: targetEntry.name,
                targetWorkspace: migratingId,
                targetBounds: { x: targetBounds.x, y: targetBounds.y, w: targetBounds.w, h: targetBounds.h },
                targetCurrentWorkspace: targetEntry.workspace,
                sourceCurrentWorkspace: sourceEntry.workspace,
                focusedId,
                activeRef,
                sourceWindows: Object.freeze(
                    sourceWindows.map((entry) =>
                        Object.freeze({
                            id: entry.id,
                            ref: entry.ref,
                            rect: Object.freeze({ x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h }),
                            output: entry.output,
                            floating: entry.floating,
                            sticky: entry.sticky,
                            fullscreen: entry.fullscreen,
                            maximized: entry.maximized,
                            fitExcluded: entry.fitExcluded,
                            minimized: entry.minimized,
                            transient: entry.transient,
                            fixedAuto: entry.fixedAuto,
                            fixedSuppress: entry.fixedSuppress,
                            ...(entry.minSize === null ? {} : { minSize: Object.freeze({ ...entry.minSize }) }),
                            ...(entry.maxSize === null ? {} : { maxSize: Object.freeze({ ...entry.maxSize }) }),
                        }),
                    ),
                ),
                targetViewWindows: Object.freeze(
                    targetViewWindows.map((entry) =>
                        Object.freeze({
                            id: entry.id,
                            ref: entry.ref,
                            rect: Object.freeze({ x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h }),
                            output: entry.output,
                            floating: entry.floating,
                            sticky: entry.sticky,
                            fullscreen: entry.fullscreen,
                            maximized: entry.maximized,
                            fitExcluded: entry.fitExcluded,
                            minimized: entry.minimized,
                            transient: entry.transient,
                            fixedAuto: entry.fixedAuto,
                            fixedSuppress: entry.fixedSuppress,
                            ...(entry.minSize === null ? {} : { minSize: Object.freeze({ ...entry.minSize }) }),
                            ...(entry.maxSize === null ? {} : { maxSize: Object.freeze({ ...entry.maxSize }) }),
                        }),
                    ),
                ),
                relatedWindows: Object.freeze(
                    relatedWindows.map((entry) =>
                        Object.freeze({
                            id: entry.id,
                            ref: entry.ref,
                            output: entry.output,
                            parentId: entry.parentId,
                            transient: entry.transient,
                            minimized: entry.minimized,
                            floating: entry.floating,
                            sticky: entry.sticky,
                            fullscreen: entry.fullscreen,
                            maximized: entry.maximized,
                            fitExcluded: entry.fitExcluded,
                        }),
                    ),
                ),
                migratedDesktopRef,
                mode: policy.mode,
                perOutput: policy.perOutput,
                sourceTiled,
                desktopCount: desktops.length,
                protectedPresent,
                sourceFingerprint: String(workspaceSendFingerprint(sourceOutput, migratingId, sourceSorted)),
                targetViewFingerprint: String(
                    workspaceSendFingerprint(targetEntry.name, targetEntry.workspace, targetSorted),
                ),
            },
        };
    } catch (error) {
        void error;
        return invalid;
    }
}

// Production send observation for one target backing desktop. Mirrors the
// established send transport observation: focused output source and target,
// per-desktop work areas, and normal tiled windows on the focused output
// whose membership is exactly one of the two observed desktops. Structural
// send policy stays in WorkspaceSendAdapter and Rust.
export function observeSendTarget(
    liveWorkspace: unknown,
    cache: Map<string, string>,
    targetWorkspace: string,
    floatingIds: ReadonlySet<string>,
    pinnedSourceWorkspace?: string,
    owners?: Map<string, object>,
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
        const desktops = decodeList(readProp(surface, "desktops"), MAX_DESKTOPS);
        if (desktops === null || desktops.length === 0) {
            return null;
        }
        let sourceDesktopRef: object | null = null;
        let sourceWorkspace = "";
        if (pinnedSourceWorkspace !== undefined) {
            if (!isOpaqueId(pinnedSourceWorkspace)) {
                return null;
            }
            for (const item of desktops) {
                if (typeof item !== "object" || item === null) {
                    continue;
                }
                const desktop = item as object;
                if (readProp(desktop, "id") === pinnedSourceWorkspace) {
                    sourceDesktopRef = desktop;
                    sourceWorkspace = pinnedSourceWorkspace;
                    break;
                }
            }
            if (sourceDesktopRef === null) {
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
        let targetOrdinal = -1;
        for (let index = 0; index < desktops.length; index += 1) {
            const item = desktops[index];
            if (typeof item !== "object" || item === null) {
                continue;
            }
            const desktop = item as object;
            if (readProp(desktop, "id") === targetWorkspace) {
                targetDesktopRef = desktop;
                targetExists = true;
                targetOrdinal = index;
            }
        }
        // Session-local redacted follow-diagnostic ordinals only: target live
        // list position plus the KWin native desktop number, the selected
        // output live position, and the live current desktop for the selected
        // output (order/number plus redacted current-vs-target equality).
        // Best effort with -1 for anything unreadable; never raw ids and never
        // a behavior input. The current read below always runs, even when the
        // source is pinned, but never fails observation.
        let outputOrdinal = -1;
        try {
            for (let index = 0; index < screens.length; index += 1) {
                if (screens[index] === outputRef) {
                    outputOrdinal = index;
                    break;
                }
            }
        } catch (error) {
            void error;
            outputOrdinal = -1;
        }
        let targetNumber = -1;
        try {
            if (targetDesktopRef !== null) {
                const numRaw = readProp(targetDesktopRef, "x11DesktopNumber");
                if (typeof numRaw === "number" && Number.isInteger(numRaw) && numRaw > 0 && numRaw <= 1000000) {
                    targetNumber = numRaw;
                }
            }
        } catch (error) {
            void error;
            targetNumber = -1;
        }
        let currentOrdinal = -1;
        let currentNumber = -1;
        let currentIdEq = -1;
        let currentRefEq = -1;
        // Behavior-fenced actual current workspace on the recording output.
        // Unlike the pinned source identity above (which survives view
        // switches) and the diagnostic-only equality beside it (which
        // compares only the target and never gates behavior), this names the
        // live view for the stay source-visibility fence. Null when
        // unreadable; null never counts as selected downstream.
        let currentWorkspace: string | null = null;
        try {
            const resolveCurrent = (curRef: object | null): void => {
                if (curRef === null) {
                    return;
                }
                const curIdRaw = readProp(curRef, "id");
                if (!isOpaqueId(curIdRaw)) {
                    try {
                        if (targetDesktopRef !== null) {
                            currentRefEq = curRef === targetDesktopRef ? 1 : 0;
                        }
                    } catch (error) {
                        void error;
                    }
                    return;
                }
                const curId = curIdRaw as string;
                currentWorkspace = curId;
                for (let index = 0; index < desktops.length; index += 1) {
                    const item = desktops[index];
                    if (typeof item !== "object" || item === null) {
                        continue;
                    }
                    if (readProp(item as object, "id") === curId) {
                        currentOrdinal = index;
                        break;
                    }
                }
                try {
                    const numRaw = readProp(curRef, "x11DesktopNumber");
                    if (typeof numRaw === "number" && Number.isInteger(numRaw) && numRaw > 0 && numRaw <= 1000000) {
                        currentNumber = numRaw;
                    }
                } catch (error) {
                    void error;
                }
                try {
                    let flightTargetId: string | null = null;
                    if (targetDesktopRef !== null) {
                        const tgtRaw = readProp(targetDesktopRef, "id");
                        if (isOpaqueId(tgtRaw)) {
                            flightTargetId = tgtRaw as string;
                        } else if (isOpaqueId(targetWorkspace)) {
                            flightTargetId = targetWorkspace;
                        }
                    } else if (isOpaqueId(targetWorkspace)) {
                        flightTargetId = targetWorkspace;
                    }
                    if (flightTargetId !== null) {
                        currentIdEq = curId === flightTargetId ? 1 : 0;
                    }
                } catch (error) {
                    void error;
                }
                try {
                    if (targetDesktopRef !== null) {
                        currentRefEq = curRef === targetDesktopRef ? 1 : 0;
                    }
                } catch (error) {
                    void error;
                }
            };
            if (pinnedSourceWorkspace !== undefined) {
                // Capability seam: per-output read on 6.7+, shared current
                // on legacy-shared hosts. Diagnostic-only, never gates.
                const read = readDesktopForOutput(surface, outputRef);
                if (read !== null) {
                    resolveCurrent(read.ref);
                }
            } else if (sourceDesktopRef !== null) {
                resolveCurrent(sourceDesktopRef);
            }
        } catch (error) {
            void error;
            currentOrdinal = -1;
            currentNumber = -1;
            currentIdEq = -1;
            currentRefEq = -1;
        }
        if (sourceDesktopRef === null) {
            return null;
        }
        const sourceBounds = readWorkAreaFor(surface, outputRef, sourceDesktopRef);
        if (sourceBounds === null) {
            return null;
        }
        let targetBounds: { x: number; y: number; w: number; h: number } | null = null;
        if (targetDesktopRef !== null) {
            targetBounds = readWorkAreaFor(surface, outputRef, targetDesktopRef);
        }
        if (targetDesktopRef !== null && targetBounds === null) {
            return null;
        }
        if (targetBounds === null) {
            targetBounds = { x: 0, y: 0, w: 1, h: 1 };
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
            const managed = readProp(ref, "managed");
            if (managed !== undefined && managed !== true) {
                continue;
            }
            if (readProp(ref, "minimized") === true) {
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
            const id = internNativeId(cache, native, ref, owners);
            const membership = decodeList(readProp(ref, "desktops"), MAX_DESKTOPS);
            if (membership === null) {
                return null;
            }
            const memberIds = new Set<string>();
            for (const member of membership) {
                if (typeof member !== "object" || member === null) {
                    return null;
                }
                const memberRaw = readProp(member as object, "id");
                if (!isOpaqueId(memberRaw)) {
                    return null;
                }
                memberIds.add(memberRaw as string);
            }
            const targetWorkspaceId =
                targetDesktopRef !== null
                    ? ((readProp(targetDesktopRef, "id") as string) ?? targetWorkspace)
                    : targetWorkspace;
            const onSource = memberIds.has(sourceWorkspace);
            const onTarget = targetDesktopRef !== null && memberIds.has(targetWorkspaceId);
            if (onSource === onTarget) {
                continue;
            }
            if (seen.has(id)) {
                return null;
            }
            seen.add(id);
            const frame = readFrameRect(ref);
            if (typeof frame === "string") {
                return null;
            }
            // Local flags only; the adapter normalizes fitExcluded to wire fit_excluded.
            const sticky = readProp(ref, "onAllDesktops") === true;
            const fullscreen = readProp(ref, "fullScreen") !== false;
            const maximized = readProp(ref, "maximizeMode") !== 0;
            const floating = floatingIds.has(id) || sticky;
            const fitExcluded = floating || sticky || fullscreen || maximized;
            const flags = {
                fullscreen,
                maximized,
                floating,
                sticky,
                fitExcluded,
            };
            if (onSource) {
                sourceWindows.push({ id, ref, rect: { x: frame.x, y: frame.y, w: frame.w, h: frame.h }, ...flags });
            } else {
                targetWindows.push({ id, ref, rect: { x: frame.x, y: frame.y, w: frame.w, h: frame.h }, ...flags });
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
                const activeId = internNativeId(cache, activeNative, activeRef, owners);
                for (const entry of sourceWindows) {
                    if (entry.id === activeId) {
                        // Mover gate: tiled movers dispatch, as does a D16
                        // intentional float (floating without sticky) plain
                        // or with exactly one overlay. Sticky, dual-overlay,
                        // and otherwise unexplained fit-excluded movers keep
                        // "" / null and refuse downstream.
                        const entryIntentionalFloat = entry.floating && !entry.sticky;
                        if (
                            entry.sticky ||
                            (entryIntentionalFloat && entry.maximized && entry.fullscreen) ||
                            (!entryIntentionalFloat &&
                                (entry.floating ||
                                    (entry.fitExcluded && !entry.maximized && !entry.fullscreen)))
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
        const sourceFingerprint = String(workspaceSendFingerprint(sourceOutput, sourceWorkspace, sourceSorted));
        const resolvedTargetWorkspaceId =
            targetDesktopRef !== null
                ? ((readProp(targetDesktopRef, "id") as string) ?? targetWorkspace)
                : targetWorkspace;
        const targetFingerprint = String(workspaceSendFingerprint(sourceOutput, resolvedTargetWorkspaceId, targetSorted));
        return {
            sourceOutput,
            sourceWorkspace,
            sourceBounds: Object.freeze({ x: sourceBounds.x, y: sourceBounds.y, w: sourceBounds.w, h: sourceBounds.h }),
            targetOutput: sourceOutput,
            targetWorkspace: resolvedTargetWorkspaceId,
            targetBounds:
                targetDesktopRef !== null
                    ? Object.freeze({ x: targetBounds.x, y: targetBounds.y, w: targetBounds.w, h: targetBounds.h })
                    : Object.freeze({ x: 0, y: 0, w: 1, h: 1 }),
            focusedId,
            sourceWindows: Object.freeze(
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
            ),
            targetWindows: Object.freeze(
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
            ),
            activeRef,
            moverRef,
            targetDesktopRef,
            targetExists,
            desktopCount: desktops.length,
            sourceFingerprint,
            targetFingerprint,
            targetOrdinal,
            targetNumber,
            outputOrdinal,
            currentOrdinal,
            currentNumber,
            currentIdEq,
            currentRefEq,
            currentWorkspace,
        };
    } catch (error) {
        void error;
        return null;
    }
}

// Background-tiling hidden-domain observation for every non-foreground
// (output, workspace) pair with tiled members. The single active foreground
// is (activeWindow.output, currentDesktopForScreen(activeWindow.output))
// exactly as observeNative defines it, whenever a normal active window with
// a valid output/current desktop exists. Every other pair - including the
// activeScreen domain when it differs - is a background lifecycle domain.
// Read-only public surfaces only (screens, desktops, activeWindow,
// activeScreen, currentDesktopForScreen, clientArea, windowList), mirroring
// the observeSendTarget enumeration patterns and the observeNative
// eligibility rules (normal windows only, same output, desktop membership
// or sticky, normalized native ids, quantized frame extents). activeWindow
// is read solely to determine this boundary and never writes visibility or
// focus: each snapshot's focusedId is a deterministic domain-local
// structural anchor (spatial-first y, x, h, w, then id) and activeRef is
// that anchor's ref solely to satisfy the shared observation shape. When no
// valid active normal foreground exists, the safe default excludes
// (activeScreen, currentDesktopForScreen(activeScreen)) so hidden startup
// with no active window still has a visible domain to exclude. Hidden flights only admit/remove/reconcile geometry
// through the shared PlanAdapter single-flight. A verified-empty domain (no
// mapped members, readable bounds, output not tainted, no duplicate or
// unreadable frame) is reported explicitly as an empty snapshot (focusedId
// "", activeExcluded true, zero windows) so retirement has fresh, complete
// evidence. Exception-only domains with no eligible tiled member (all
// floating/sticky/fullscreen/maximized) are reported with their members so
// the adapter can protect a retained baseline, but the adapter never
// dispatches admission/reconcile/removal solely for them. Anything unreadable
// or tainted is omitted as unknown and must never be treated as empty.
// Omission fails closed per domain, per output, or overall. This observer
// reports every eligible background domain without inventing an empty one.
export function observeHiddenDomains(
    liveWorkspace: unknown,
    cache: Map<string, string>,
    floatingIds: ReadonlySet<string>,
    gaps: DomainGaps,
    reportEligibility?: (ref: object, reason: string | null) => void,
    owners?: Map<string, object>,
    // D23 close homing: last-known output/workspace per stable id, recorded
    // for every interned member (minimized windows observe normally here).
    // Best-effort: never fails the domain. Lets a later exact-ref close of
    // an initially-hidden client report its owning domain.
    homing?: Map<string, { output: string; workspace: string }>,
): ReadonlyArray<PlanObserved> {
    try {
        if (typeof liveWorkspace !== "object" || liveWorkspace === null) {
            return [];
        }
        const surface = liveWorkspace as Record<string, unknown>;
        const screens = decodeList(readProp(surface, "screens"), MAX_LIST);
        const desktops = decodeList(readProp(surface, "desktops"), MAX_DESKTOPS);
        if (screens === null || screens.length === 0 || desktops === null || desktops.length === 0) {
            return [];
        }
        // Foreground current reads go through the workspace-native
        // capability seam: the per-output read on 6.7+, the shared
        // currentDesktop (then current on every output) on legacy-shared
        // hosts.
        const lister = readProp(surface, "windowList");
        if (typeof lister !== "function") {
            return [];
        }
        const screenEntries: Array<{ ref: object; name: string }> = [];
        for (const item of screens) {
            if (typeof item !== "object" || item === null) {
                continue;
            }
            const nameRaw = readProp(item as object, "name");
            if (!isOpaqueId(nameRaw)) {
                continue;
            }
            screenEntries.push({ ref: item as object, name: nameRaw as string });
        }
        const desktopEntries: Array<{ ref: object; id: string }> = [];
        for (const item of desktops) {
            if (typeof item !== "object" || item === null) {
                continue;
            }
            const idRaw = readProp(item as object, "id");
            if (!isOpaqueId(idRaw)) {
                continue;
            }
            desktopEntries.push({ ref: item as object, id: idRaw as string });
        }
        if (screenEntries.length === 0 || desktopEntries.length === 0) {
            return [];
        }
        // Single active foreground by stable id, exactly the observeNative
        // domain whenever a normal active window with a valid output/current
        // desktop exists: (activeWindow.output,
        // currentDesktopForScreen(activeWindow.output)). activeWindow is read
        // solely to determine this boundary; it never writes visibility or
        // focus. When no valid active normal foreground exists, fall back to
        // (activeScreen, currentDesktopForScreen(activeScreen)) so hidden
        // startup with no active window still excludes a visible domain.
        // Every other pair - including the non-foreground screen's currently
        // displayed desktop - is a background lifecycle domain. An unreadable
        // boundary is unclassifiable: fail closed overall rather than risk
        // tiling the foreground via the background path.
        let foregroundKey: string | null = null;
        const activeWindowRaw = readProp(surface, "activeWindow");
        if (typeof activeWindowRaw === "object" && activeWindowRaw !== null) {
            const activeWindowRef = activeWindowRaw as object;
            if (readProp(activeWindowRef, "normalWindow") === true) {
                const activeWindowOutput = readProp(activeWindowRef, "output");
                if (typeof activeWindowOutput === "object" && activeWindowOutput !== null) {
                    const activeWindowNameRaw = readProp(activeWindowOutput as object, "name");
                    if (isOpaqueId(activeWindowNameRaw)) {
                        const activeWindowRead = readDesktopForOutput(surface, activeWindowOutput as object);
                        if (activeWindowRead !== null) {
                            foregroundKey = `${activeWindowNameRaw as string}\u0000${activeWindowRead.id}`;
                        }
                    }
                }
            }
        }
        if (foregroundKey === null) {
            const activeScreenRaw = readProp(surface, "activeScreen");
            if (typeof activeScreenRaw !== "object" || activeScreenRaw === null) {
                return [];
            }
            const activeScreenRef = activeScreenRaw as object;
            const activeNameRaw = readProp(activeScreenRef, "name");
            if (!isOpaqueId(activeNameRaw)) {
                return [];
            }
            const activeName = activeNameRaw as string;
            const activeRead = readDesktopForOutput(surface, activeScreenRef);
            if (activeRead === null) {
                return [];
            }
            foregroundKey = `${activeName}\u0000${activeRead.id}`;
        }
        let rawList: unknown = undefined;
        try {
            rawList = Reflect.apply(lister as (...args: ReadonlyArray<never>) => unknown, surface, []);
        } catch (error) {
            void error;
            return [];
        }
        const windows = decodeList(rawList, MAX_LIST);
        if (windows === null) {
            return [];
        }
        interface ParsedWindow {
            readonly ref: object;
            readonly output: string;
            readonly native: string;
            readonly sticky: boolean;
            readonly members: ReadonlyArray<object>;
        }
        const parsed: ParsedWindow[] = [];
        const taintedOutputs = new Set<string>();
        for (const item of windows) {
            if (typeof item !== "object" || item === null) {
                continue;
            }
            const ref = item as object;
            if (readProp(ref, "normalWindow") !== true) {
                reportEligibility?.(ref, "normal-window");
                continue;
            }
            const output = readProp(ref, "output");
            if (typeof output !== "object" || output === null) {
                reportEligibility?.(ref, "output-missing");
                continue;
            }
            const nameRaw = readProp(output as object, "name");
            if (!isOpaqueId(nameRaw)) {
                reportEligibility?.(ref, "output-missing");
                continue;
            }
            const outputName = nameRaw as string;
            // An undecodable identity or membership cannot be placed: quarantine
            // the output's hidden pairs rather than tiling a partial domain.
            let native: string | null = null;
            try {
                native = normalizeNativeId(readProp(ref, "internalId"));
            } catch (error) {
                void error;
                taintedOutputs.add(outputName);
                reportEligibility?.(ref, "native-id-unreadable");
                continue;
            }
            if (native === null) {
                taintedOutputs.add(outputName);
                reportEligibility?.(ref, "native-id-unreadable");
                continue;
            }
            const sticky = readProp(ref, "onAllDesktops") === true;
            const membership = decodeList(readProp(ref, "desktops"), MAX_DESKTOPS);
            if (membership === null) {
                taintedOutputs.add(outputName);
                reportEligibility?.(ref, "desktop-membership-unreadable");
                continue;
            }
            const members: object[] = [];
            for (const member of membership) {
                if (typeof member === "object" && member !== null) {
                    members.push(member as object);
                }
            }
            parsed.push({ ref, output: outputName, native, sticky, members });
        }
        const out: PlanObserved[] = [];
        for (const screen of screenEntries) {
            if (taintedOutputs.has(screen.name)) {
                continue;
            }
            for (const desktop of desktopEntries) {
                if (`${screen.name}\u0000${desktop.id}` === foregroundKey) {
                    continue;
                }
                const bounds = readWorkAreaFor(surface, screen.ref, desktop.ref);
                if (bounds === null) {
                    continue;
                }
                const seen = new Set<string>();
                let duplicate = false;
                let incomplete = false;
                const entries: Array<{
                    id: string;
                    ref: object;
                    rect: { x: number; y: number; w: number; h: number };
                    fullscreen: boolean;
                    maximized: boolean;
                    floating: boolean;
                    sticky: boolean;
                    resourceClass: string;
                }> = [];
                for (const candidate of parsed) {
                    if (candidate.output !== screen.name) {
                        continue;
                    }
                    if (!candidate.sticky && candidate.members.indexOf(desktop.ref) < 0) {
                        continue;
                    }
                    const id = internNativeId(cache, candidate.native, candidate.ref, owners);
                    if (seen.has(id)) {
                        duplicate = true;
                        break;
                    }
                    seen.add(id);
                    try {
                        homing?.set(id, { output: screen.name, workspace: desktop.id });
                    } catch (error) {
                        void error;
                    }
                    const frame = readFrameRect(candidate.ref);
                    if (typeof frame === "string") {
                        seen.delete(id);
                        incomplete = true;
                        reportEligibility?.(candidate.ref, frame);
                        continue;
                    }
                    entries.push({
                        id,
                        ref: candidate.ref,
                        rect: { x: frame.x, y: frame.y, w: frame.w, h: frame.h },
                        fullscreen: readProp(candidate.ref, "fullScreen") !== false,
                        maximized: readProp(candidate.ref, "maximizeMode") !== 0,
                        floating: floatingIds.has(id) || candidate.sticky,
                        sticky: candidate.sticky,
                        resourceClass: readResourceClass(candidate.ref),
                    });
                    reportEligibility?.(candidate.ref, null);
                }
                // Partial observation is unknown, never empty: duplicate ids or
                // any unreadable frame omits the whole domain fail-closed.
                if (duplicate || incomplete) {
                    continue;
                }
                if (entries.length === 0) {
                    // Verified-empty domain: no mapped members, readable
                    // bounds, output not tainted, no duplicate or unreadable
                    // frame. Explicit empty evidence for retirement; the
                    // adapter retires only from this shape, never from
                    // absence. activeRef is the stable desktop ref; the
                    // adapter never routes focus for hidden domains.
                    const emptyFingerprint = String(planFingerprint(screen.name, desktop.id, "", []));
                    const emptyBounds = { x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h };
                    const emptyInnerGap = gaps.innerGap;
                    const emptyOuterGap = gaps.outerGap;
                    const emptyActiveRef: object = desktop.ref;
                    const emptyExpected = emptyFingerprint;
                    out.push({
                        domainOutput: screen.name,
                        domainWorkspace: desktop.id,
                        domainBounds: Object.freeze({ x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h }),
                        domainGap: gaps.innerGap,
                        domainOuterGap: gaps.outerGap,
                        focusedId: "",
                        activeExcluded: true,
                        windows: Object.freeze([]),
                        activeRef: emptyActiveRef,
                        fingerprint: emptyExpected,
                        revalidate: () => {
                            try {
                                const fresh = observeHiddenDomains(liveWorkspace, cache, floatingIds, gaps, undefined, owners);
                                for (const candidate of fresh) {
                                    if (
                                        candidate.domainOutput !== screen.name ||
                                        candidate.domainWorkspace !== desktop.id
                                    ) {
                                        continue;
                                    }
                                    return (
                                        candidate.fingerprint === emptyExpected &&
                                        candidate.windows.length === 0 &&
                                        candidate.focusedId === "" &&
                                        candidate.activeExcluded === true &&
                                        candidate.domainBounds.x === emptyBounds.x &&
                                        candidate.domainBounds.y === emptyBounds.y &&
                                        candidate.domainBounds.w === emptyBounds.w &&
                                        candidate.domainBounds.h === emptyBounds.h &&
                                        candidate.domainGap === emptyInnerGap &&
                                        candidate.domainOuterGap === emptyOuterGap
                                    );
                                }
                                return false;
                            } catch (error) {
                                void error;
                                return false;
                            }
                        },
                    });
                    continue;
                }
                // Exception-only domains (all floating/sticky/fullscreen/
                // maximized) are reported with their members so a retained
                // baseline stays protected. The adapter never dispatches
                // admission/reconcile/removal solely for them; foreground
                // exception semantics are untouched and mixed domains keep
                // their Rust handling.
                const sortedIds = entries.map((entry) => entry.id).sort();
                const bySpatial = (a: (typeof entries)[number], b: (typeof entries)[number]): number =>
                    a.rect.y !== b.rect.y
                        ? a.rect.y - b.rect.y
                        : a.rect.x !== b.rect.x
                          ? a.rect.x - b.rect.x
                          : a.rect.h !== b.rect.h
                            ? a.rect.h - b.rect.h
                            : a.rect.w !== b.rect.w
                              ? a.rect.w - b.rect.w
                              : a.id < b.id
                                ? -1
                                : a.id > b.id
                                  ? 1
                                  : 0;
                // Mixed hidden domains must anchor session focus on a tiled
                // member: a spatial-first float would otherwise seed
                // focused_window on an exception while the retained Engine
                // session holds focus None and rejects the reconcile as a
                // focus-mismatch. Exception-only domains keep the
                // spatial-first member.
                const tiled = entries.filter((entry) => !entry.floating && !entry.sticky);
                const anchorPool = tiled.length > 0 ? tiled : entries;
                const anchor = [...anchorPool].sort(bySpatial)[0] as { id: string; ref: object };
                const anchorRef = anchor.ref;
                const anchorId = anchor.id;
                const fingerprint = String(planFingerprint(screen.name, desktop.id, anchorId, sortedIds));
                const frozenWindows = Object.freeze(
                    entries.map((entry) =>
                        Object.freeze({
                            id: entry.id,
                            ref: entry.ref,
                            rect: Object.freeze({ x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h }),
                            output: screen.name,
                            workspace: desktop.id,
                            fullscreen: entry.fullscreen,
                            maximized: entry.maximized,
                            floating: entry.floating,
                            sticky: entry.sticky,
                            resourceClass: entry.resourceClass,
                        }),
                    ),
                );
                const expected = fingerprint;
                const expectedBounds = { x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h };
                const expectedInnerGap = gaps.innerGap;
                const expectedOuterGap = gaps.outerGap;
                const expectedAnchorId = anchorId;
                const expectedAnchorRef = anchorRef;
                out.push({
                    domainOutput: screen.name,
                    domainWorkspace: desktop.id,
                    domainBounds: Object.freeze({ x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h }),
                    domainGap: gaps.innerGap,
                    domainOuterGap: gaps.outerGap,
                    focusedId: anchorId,
                    activeExcluded: false,
                    windows: frozenWindows,
                    activeRef: anchorRef,
                    fingerprint: expected,
                    revalidate: () => {
                        try {
                            const fresh = observeHiddenDomains(liveWorkspace, cache, floatingIds, gaps, undefined, owners);
                            for (const candidate of fresh) {
                                if (
                                    candidate.domainOutput !== screen.name ||
                                    candidate.domainWorkspace !== desktop.id ||
                                    candidate.fingerprint !== expected
                                ) {
                                    continue;
                                }
                                if (
                                    candidate.domainBounds.x !== expectedBounds.x ||
                                    candidate.domainBounds.y !== expectedBounds.y ||
                                    candidate.domainBounds.w !== expectedBounds.w ||
                                    candidate.domainBounds.h !== expectedBounds.h ||
                                    candidate.domainGap !== expectedInnerGap ||
                                    candidate.domainOuterGap !== expectedOuterGap ||
                                    candidate.focusedId !== expectedAnchorId ||
                                    candidate.activeRef !== expectedAnchorRef ||
                                    candidate.activeExcluded !== false
                                ) {
                                    return false;
                                }
                                if (candidate.windows.length !== frozenWindows.length) {
                                    return false;
                                }
                                for (const entry of frozenWindows) {
                                    let matched = false;
                                    for (const other of candidate.windows) {
                                        if (other.id === entry.id) {
                                            matched = true;
                                            if (other.ref !== entry.ref) {
                                                return false;
                                            }
                                            // Flag-exact: a floating/sticky
                                            // flip between observe and
                                            // revalidate must fail closed.
                                            if (other.floating !== entry.floating) {
                                                return false;
                                            }
                                            if (other.sticky !== entry.sticky) {
                                                return false;
                                            }
                                            if (
                                                other.rect.x !== entry.rect.x ||
                                                other.rect.y !== entry.rect.y ||
                                                other.rect.w !== entry.rect.w ||
                                                other.rect.h !== entry.rect.h ||
                                                other.fullscreen !== entry.fullscreen ||
                                                other.maximized !== entry.maximized
                                            ) {
                                                return false;
                                            }
                                            break;
                                        }
                                    }
                                    if (!matched) {
                                        return false;
                                    }
                                }
                                return true;
                            }
                            return false;
                        } catch (error) {
                            void error;
                            return false;
                        }
                    },
                });
            }
        }
        // Report every eligible background domain; the adapter distinguishes
        // explicit empty evidence from omitted, unreadable domains.
        return Object.freeze(out);
    } catch (error) {
        void error;
        return [];
    }
}

function internNativeId(
    cache: Map<string, string>,
    native: string,
    ref?: object,
    owners?: Map<string, object>,
): string {
    if (ref !== undefined && owners !== undefined) {
        try {
            owners.set(native, ref);
        } catch (error) {
            void error;
        }
    }
    const known = cache.get(native);
    if (known !== undefined) {
        return known;
    }
    cache.set(native, native);
    return native;
}

function readNativeId(ref: object): string | null {
    let raw: unknown = undefined;
    try {
        raw = Reflect.get(ref, "internalId");
    } catch (error) {
        void error;
        return null;
    }
    return normalizeNativeId(raw);
}

function readResourceClass(ref: object): string {
    const value = readProp(ref, "resourceClass");
    return isOpaqueId(value) ? value : "unknown";
}

// Q3 complete live inventory for float-intent pruning: every window in the
// Q3 complete live inventory for float-intent pruning: every window in the
// single workspace.windowList decode, unscoped (all outputs and desktops,
// including minimized and hidden). Complete only when every item carries a
// readable, normalizable id; any structural failure (unreadable list,
// non-object item, unreadable/mistyped/un-normalizable id) reports
// complete=false: the read then omits live and nothing is pruned on
// partial evidence. Scoped absence never evicts.
//
// The optional owners map records id to exact live ref for every id read,
// including on the partial path: ordinary observers skip ineligible
// windows (non-normal, unassigned output, tainted pairs), so without this
// a hydrated survivor excluded from observation would never gain an owner
// and its verified close could not clear markers. Recording refs never
// prunes: only a complete inventory attests liveness, and removal never
// reads the dead object.
export function readCompleteIntentInventory(
    liveWorkspace: unknown,
    owners?: Map<string, object>,
): {
    complete: boolean;
    ids: ReadonlyArray<string>;
} {
    const incomplete: { complete: boolean; ids: ReadonlyArray<string> } = {
        complete: false,
        ids: Object.freeze([]),
    };
    try {
        if (typeof liveWorkspace !== "object" || liveWorkspace === null) {
            return incomplete;
        }
        const surface = liveWorkspace as Record<string, unknown>;
        const lister = readProp(surface, "windowList");
        if (typeof lister !== "function") {
            return incomplete;
        }
        let rawList: unknown = undefined;
        try {
            rawList = Reflect.apply(lister as (...args: ReadonlyArray<never>) => unknown, surface, []);
        } catch (error) {
            void error;
            return incomplete;
        }
        const windows = decodeList(rawList, MAX_LIST);
        if (windows === null) {
            return incomplete;
        }
        const seen = new Set<string>();
        const ids: string[] = [];
        for (const item of windows) {
            if (typeof item !== "object" || item === null) {
                return incomplete;
            }
            const ref = item as object;
            // Strict complete proof: any unreadable, mistyped, or
            // un-normalizable id fails the whole inventory closed, so the
            // read omits live and nothing prunes on partial evidence. No
            // per-type assumptions: normalWindow readability is never
            // consulted, and non-normal windows with usable ids simply join
            // the attested set (extra live ids never affect pruning).
            let raw: unknown = undefined;
            let readable = true;
            try {
                raw = Reflect.get(ref, "internalId");
            } catch (error) {
                void error;
                readable = false;
            }
            if (!readable || (typeof raw !== "string" && typeof raw !== "number")) {
                return incomplete;
            }
            const id = normalizeNativeId(raw);
            if (id === null) {
                return incomplete;
            }
            if (owners !== undefined) {
                try {
                    owners.set(id, ref);
                } catch (error) {
                    void error;
                }
            }
            if (!seen.has(id)) {
                seen.add(id);
                ids.push(id);
            }
        }
        return { complete: true, ids: Object.freeze(ids) };
    } catch (error) {
        void error;
        return incomplete;
    }
}

type EligibilityReporter = (ref: object, reason: string | null) => void;

function observeNative(
    liveWorkspace: unknown,
    cache: Map<string, string>,
    floatingIds: ReadonlySet<string>,
    gaps: DomainGaps,
    reportEligibility?: EligibilityReporter,
    owners?: Map<string, object>,
    // D23 close homing: last-known output/workspace per stable id, recorded
    // for observed members and minimized-omitted frames alike (best-effort:
    // an unreadable minimized id simply records nothing and never fails the
    // domain). Lets a later exact-ref close report its owning domain without
    // ever reading the dead object.
    homing?: Map<string, { output: string; workspace: string }>,
): PlanObserved | null {
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
        // D23 MNZ-03: a null active window (sole minimize empties native
        // focus) still resolves the domain below via the active screen
        // instead of failing closed here. Anything unreadable there stays
        // null.
        let activeRef: object | null = null;
        if (typeof active === "object" && active !== null) {
            activeRef = active as object;
            if (readProp(activeRef, "normalWindow") !== true) {
                reportEligibility?.(activeRef, "active-normal-window");
                return null;
            }
        }
        // Domain output: the active window's output when focused, else the
        // active screen (sole-minimize null focus retains the workspace).
        let activeOutput: object;
        if (activeRef !== null) {
            const output = readProp(activeRef, "output");
            if (typeof output !== "object" || output === null) {
                return null;
            }
            activeOutput = output as object;
        } else {
            let screen: unknown = undefined;
            try {
                screen = Reflect.get(surface, "activeScreen");
            } catch (error) {
                void error;
                return null;
            }
            if (typeof screen !== "object" || screen === null) {
                return null;
            }
            activeOutput = screen as object;
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
        const outputNameRaw = readProp(activeOutput, "name");
        if (!isOpaqueId(outputNameRaw)) {
            return null;
        }
        const domainOutput = outputNameRaw as string;
        // Capability seam: per-output desktop on 6.7+, shared current (then
        // current on every output) on legacy-shared hosts.
        const areaFn = readProp(surface, "clientArea");
        if (typeof areaFn !== "function") {
            return null;
        }
        const desktopRead = readDesktopForOutput(surface, activeOutput);
        if (desktopRead === null) {
            return null;
        }
        const desktopRef = desktopRead.ref;
        const domainWorkspace = desktopRead.id;
        let area: unknown = undefined;
        try {
            // PlacementArea (0): per-output usable area. WorkArea (5) is
            // global across screens and must never size this domain.
            area = Reflect.apply(areaFn as (...args: ReadonlyArray<never>) => unknown, surface, [
                0,
                activeOutput,
                desktopRef,
            ]);
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
        if (bw <= 0 || bh <= 0 || bw > 16384 || bh > 16384 || bx < -16384 || bx > 16384 || by < -16384 || by > 16384) {
            return null;
        }
        const domainBounds = { x: bx, y: by, w: bw, h: bh };
        const seen = new Set<string>();
        // D23: whether an in-scope minimized frame was omitted below. Only
        // with such evidence does an emptied domain converge explicitly;
        // without it the domain stays unknown, as before.
        let minimizedEvidence = false;
        const entries: Array<{
            id: string;
            ref: object;
            rect: { x: number; y: number; w: number; h: number };
            output: string;
            workspace: string;
            fullscreen: boolean;
            maximized: boolean;
            floating: boolean;
            sticky: boolean;
            resourceClass: string;
        }> = [];
        for (const item of windows) {
            if (typeof item !== "object" || item === null) {
                continue;
            }
            const ref = item as object;
            // Only normal windows are observed. Every other classification
            // is skipped before the snapshot; Rust owns all remaining policy.
            if (readProp(ref, "normalWindow") !== true) {
                reportEligibility?.(ref, "normal-window");
                continue;
            }
            const output = readProp(ref, "output");
            if (typeof output !== "object" || output === null) {
                reportEligibility?.(ref, "output-missing");
                continue;
            }
            const nameRaw = readProp(output as object, "name");
            if (nameRaw !== domainOutput) {
                reportEligibility?.(ref, "output-mismatch");
                continue;
            }
            const allDesktops = readProp(ref, "onAllDesktops") === true;
            const membership = decodeList(readProp(ref, "desktops"), MAX_DESKTOPS);
            if (membership === null) {
                return null;
            }
            let onDesktop = false;
            for (const member of membership) {
                if (member === desktopRef) {
                    onDesktop = true;
                    break;
                }
            }
            if (!onDesktop && !allDesktops) {
                reportEligibility?.(ref, "desktop-mismatch");
                continue;
            }
            // D23 MNZ-01: minimized frames are omitted from the converged
            // observation so a native minimize reaches core as an omission
            // (stored old slot, sibling reflow). Omitted before id/frame
            // reads, so a stale or unreadable minimized frame can never
            // fail the whole domain closed; a native restore re-observes
            // the live frame for the old-slot restore leg.
            if (readProp(ref, "minimized") === true) {
                minimizedEvidence = true;
                reportEligibility?.(ref, "minimized-omitted");
                try {
                    const native = readNativeId(ref);
                    if (native !== null && homing !== undefined) {
                        homing.set(native, { output: domainOutput, workspace: domainWorkspace });
                    }
                } catch (error) {
                    void error;
                }
                continue;
            }
            // Read and normalize the native id while the object is live;
            // stable plan ids come from the string cache. Never read an id
            // from a removal signal payload.
            const native = readNativeId(ref);
            if (native === null) {
                return null;
            }
            const id = internNativeId(cache, native, ref, owners);
            if (seen.has(id)) {
                return null;
            }
            seen.add(id);
            const frame = readFrameRect(ref);
            if (typeof frame === "string") {
                // Complete-observation convergence: an unreadable frame for
                // any otherwise eligible member quarantines the whole
                // foreground domain until a complete observation is
                // available, so a temporary read can never trigger a
                // transient remove/re-admit. Partial is unknown, never
                // empty and never a survivor-only dispatch.
                reportEligibility?.(ref, frame);
                return null;
            }
            const rect = frame;
            // Fullscreen is a compositor-owned overlay state orthogonal to the
            // tree: the window stays observed (identity/position/share
            // retained) but must never be actuated or reflowed. Exact
            // `!== false` check mirrors the standalone adapters; revalidation
            // re-reads the property through this same observe path.
            // Maximize mirrors fullscreen: a nonzero `maximizeMode` (1 vertical,
            // 2 horizontal, 3 full) collapses to one boolean, so horizontal and
            // vertical maximize are deliberately not modeled in the engine. The
            // exact `!== 0` check mirrors the standalone adapters; revalidation
            // re-reads the property through this same observe path.
            entries.push({
                id,
                ref,
                rect,
                output: domainOutput,
                workspace: domainWorkspace,
                fullscreen: readProp(ref, "fullScreen") !== false,
                maximized: readProp(ref, "maximizeMode") !== 0,
                floating: floatingIds.has(id) || allDesktops,
                sticky: allDesktops,
                resourceClass: readResourceClass(ref),
            });
            reportEligibility?.(ref, null);
            try {
                homing?.set(id, { output: domainOutput, workspace: domainWorkspace });
            } catch (error) {
                void error;
            }
        }
        if (entries.length === 0) {
            // D23 MNZ-03: an emptied domain with minimize evidence converges
            // explicitly (retain workspace, focus none, stored slot) instead
            // of failing closed. Without such evidence the domain stays
            // unknown, as before (empty startup awaits the next window).
            if (!minimizedEvidence) {
                return null;
            }
            const emptyFingerprint = String(planFingerprint(domainOutput, domainWorkspace, "", []));
            const emptyActiveRef: object = activeRef ?? desktopRef;
            const emptyExpected = emptyFingerprint;
            return {
                domainOutput,
                domainWorkspace,
                domainBounds: Object.freeze({ x: domainBounds.x, y: domainBounds.y, w: domainBounds.w, h: domainBounds.h }),
                domainGap: gaps.innerGap,
                domainOuterGap: gaps.outerGap,
                focusedId: "",
                activeExcluded: true,
                windows: Object.freeze([]),
                activeRef: emptyActiveRef,
                fingerprint: emptyExpected,
                revalidate: () => {
                    try {
                        const fresh = observeNative(liveWorkspace, cache, floatingIds, gaps, reportEligibility, owners);
                        if (
                            fresh === null ||
                            fresh.fingerprint !== emptyExpected ||
                            fresh.activeRef !== emptyActiveRef ||
                            fresh.activeExcluded !== true
                        ) {
                            return false;
                        }
                        return fresh.windows.length === 0 && fresh.focusedId === "";
                    } catch (error) {
                        void error;
                        return false;
                    }
                },
            };
        }
        if (activeRef === null) {
            // Null focus with surviving members stays unknown until the
            // host refocuses (fail closed, as before); the emptied sole
            // case converged above.
            return null;
        }
        const activeNative = readNativeId(activeRef);
        if (activeNative === null) {
            return null;
        }
        const activeNativeId = internNativeId(cache, activeNative, activeRef, owners);
        const activeExcluded = floatingIds.has(activeNativeId) || readProp(activeRef, "onAllDesktops") === true;
        let activeId: string | null = null;
        for (const entry of entries) {
            if (entry.id === activeNativeId) {
                activeId = entry.id;
                break;
            }
        }
        if (activeId === null) {
            // D23: the active window was omitted as minimized. Name it
            // truthfully: core holds no membership for it and falls back
            // to retained/MRU focus. Anything else unnamed stays
            // fail-closed, as before.
            if (readProp(activeRef, "minimized") === true) {
                activeId = activeNativeId;
            } else {
                return null;
            }
        }
        const sortedIds = entries.map((entry) => entry.id).sort();
        const fingerprint = String(planFingerprint(domainOutput, domainWorkspace, activeId, sortedIds));
        const frozenWindows = Object.freeze(
            entries.map((entry) =>
                Object.freeze({
                    id: entry.id,
                    ref: entry.ref,
                    rect: Object.freeze({ x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h }),
                    output: entry.output,
                    workspace: entry.workspace,
                    fullscreen: entry.fullscreen,
                    maximized: entry.maximized,
                    floating: entry.floating,
                    sticky: entry.sticky,
                    resourceClass: entry.resourceClass,
                }),
            ),
        );
        const expected = fingerprint;
        const capturedActive = activeRef;
        return {
            domainOutput,
            domainWorkspace,
            domainBounds: Object.freeze({ x: domainBounds.x, y: domainBounds.y, w: domainBounds.w, h: domainBounds.h }),
            domainGap: gaps.innerGap,
            domainOuterGap: gaps.outerGap,
            focusedId: activeId,
            activeExcluded,
            windows: frozenWindows,
            activeRef: activeRef,
            fingerprint: expected,
            revalidate: () => {
                try {
                    const fresh = observeNative(liveWorkspace, cache, floatingIds, gaps, reportEligibility, owners);
                    if (
                        fresh === null ||
                        fresh.fingerprint !== expected ||
                        fresh.activeRef !== capturedActive ||
                        fresh.activeExcluded !== activeExcluded
                    ) {
                        return false;
                    }
                    for (const entry of frozenWindows) {
                        let matchFound = false;
                        for (const candidate of fresh.windows) {
                            if (candidate.id === entry.id) {
                                matchFound = true;
                                if (candidate.ref !== entry.ref) {
                                    return false;
                                }
                                if (candidate.fullscreen !== entry.fullscreen) {
                                    return false;
                                }
                                if (candidate.maximized !== entry.maximized) {
                                    return false;
                                }
                                // Flag-exact: a floating/sticky flip between
                                // observe and revalidate must fail closed.
                                if (candidate.floating !== entry.floating) {
                                    return false;
                                }
                                if (candidate.sticky !== entry.sticky) {
                                    return false;
                                }
                                if (
                                    candidate.rect.x !== entry.rect.x ||
                                    candidate.rect.y !== entry.rect.y ||
                                    candidate.rect.w !== entry.rect.w ||
                                    candidate.rect.h !== entry.rect.h
                                ) {
                                    return false;
                                }
                                break;
                            }
                        }
                        if (!matchFound) {
                            return false;
                        }
                    }
                    return true;
                } catch (error) {
                    void error;
                    return false;
                }
            },
        };
    } catch (error) {
        void error;
        return null;
    }
}

// Production directional observation for four-direction move plus
// left/right focus (active DescribePlan route only): source plus the
// reciprocal adjacent output's current logical workspace (which may differ
// in workspace id), bounded max two domains. Reuses only
// documented/project-used public properties: `screens`,
// `currentDesktopForScreen`, Output `geometry`, `clientArea`, `windowList`,
// `window.output.name`, `window.desktops`, `activeWindow`. Moves rank
// multiple FULL-rectangle edge-touch candidates positionally (mover-centre
// projection, then span overlap, then left/top); focus keeps the legacy
// unique reciprocal rule (panel gaps cannot block selection); carried domain
// bounds stay per-desktop WORK AREAS exactly as before (never tiled under
// panels).
// Distinct workspace ids are allowed. The typed outcome keeps local
// behavior for a confirmed no-adjacent condition only (`no-target`);
// ambiguous, partial, or unreadable evidence is `invalid` and must refuse
// before any local mutation. No output wrapping.
export function observeDirectionalDomain(
    liveWorkspace: unknown,
    cache: Map<string, string>,
    floatingIds: ReadonlySet<string>,
    gaps: DomainGaps,
    direction: string,
    reportEligibility?: EligibilityReporter,
    owners?: Map<string, object>,
    pinnedSource?: { readonly output: string; readonly workspace: string; readonly targetOutput?: string },
    forMove?: boolean,
): DirectionalObservation {
    const invalid: DirectionalObservation = { status: "invalid", observed: null };
    const noTarget: DirectionalObservation = { status: "no-target", observed: null };
    try {
        if (direction !== "left" && direction !== "right" && direction !== "up" && direction !== "down") {
            return invalid;
        }
        const surface = liveWorkspace as Record<string, unknown>;
        // Pinned flight source (R4 re-observation across transfer): the
        // frozen dispatch pair, resolved without the live active window
        // (which sits on the target then). Dispatch always observes
        // unpinned via the active domain below.
        const pinned = pinnedSource !== undefined;
        if (pinned && (!isOpaqueId(pinnedSource.output) || !isOpaqueId(pinnedSource.workspace))) {
            return invalid;
        }
        // Pinned R4 re-observation must name the dispatch-selected target:
        // without it the fence cannot verify the same pair and must refuse
        // rather than re-rank (re-ranking could pick another output after a
        // partial transfer).
        if (pinned && !isOpaqueId(pinnedSource.targetOutput)) {
            return invalid;
        }
        // Shared window-list read for both scope collections below.
        const lister = readProp(surface, "windowList");
        if (typeof lister !== "function") {
            return invalid;
        }
        let rawList: unknown = undefined;
        try {
            rawList = Reflect.apply(lister as (...args: ReadonlyArray<never>) => unknown, surface, []);
        } catch (error) {
            void error;
            return invalid;
        }
        const windows = decodeList(rawList, MAX_LIST);
        if (windows === null) {
            return invalid;
        }
        // Source inputs: the live active domain unpinned (dispatch), or the
        // frozen flight pair pinned (R4 fences across transfer, where the
        // still-active mover sits on the target and the active domain would
        // be the wrong scope with meaningless focus). Pinned skips
        // observeNative entirely.
        let domainOutput: string;
        let domainWorkspace: string;
        let sourceGap: number;
        let sourceOuterGap: number;
        let sourceWindowsSeed: ReadonlyArray<{ readonly id: string }>;
        let sourceWindowsFull: ReadonlyArray<PlanObservedWindow> = [];
        let sourceFocusedId: string;
        let sourceActiveExcluded: boolean | undefined;
        let sourceActiveRef: object;
        let sourceBoundsForFence: { x: number; y: number; w: number; h: number } | null;
        if (!pinned) {
            const source = observeNative(liveWorkspace, cache, floatingIds, gaps, reportEligibility, owners);
            if (source === null || source.windows.length === 0) {
                // No readable source: delegate to the single-domain path,
                // which reports the observation failure accurately.
                return noTarget;
            }
            domainOutput = source.domainOutput;
            domainWorkspace = source.domainWorkspace;
            sourceGap = source.domainGap;
            sourceOuterGap = source.domainOuterGap;
            sourceWindowsSeed = source.windows;
            sourceWindowsFull = source.windows;
            sourceFocusedId = source.focusedId;
            sourceActiveExcluded = source.activeExcluded;
            sourceActiveRef = source.activeRef;
            sourceBoundsForFence = source.domainBounds;
        } else {
            domainOutput = (pinnedSource as { output: string; workspace: string }).output;
            domainWorkspace = (pinnedSource as { output: string; workspace: string }).workspace;
            sourceGap = gaps.innerGap;
            sourceOuterGap = gaps.outerGap;
            sourceWindowsSeed = [];
            sourceFocusedId = "";
            sourceActiveExcluded = true;
            let liveActive: object | null = null;
            try {
                const candidate: unknown = Reflect.get(surface, "activeWindow");
                if (typeof candidate === "object" && candidate !== null) {
                    liveActive = candidate as object;
                }
            } catch (error) {
                void error;
                liveActive = null;
            }
            if (liveActive === null) {
                return invalid;
            }
            sourceActiveRef = liveActive;
            sourceBoundsForFence = null;
        }
        // A confirmed single output has no adjacent candidate: keep local
        // behavior. The lone screen entry must still be a readable output
        // identity matching the source; a malformed screen list refuses
        // (unreadable topology) rather than silently going local.
        const screensEarly = decodeList(readProp(surface, "screens"), MAX_LIST);
        if (screensEarly !== null && screensEarly.length === 1) {
            const only = screensEarly[0];
            if (typeof only !== "object" || only === null) {
                return invalid;
            }
            const onlyNameRaw = readProp(only as object, "name");
            if (!isOpaqueId(onlyNameRaw) || (onlyNameRaw as string) !== domainOutput) {
                return invalid;
            }
            return noTarget;
        }
        // Per-output currents resolve inside readOutputTopology through the
        // capability seam (shared current on legacy-shared hosts); only the
        // area function is required here.
        const areaFn = readProp(surface, "clientArea");
        if (typeof areaFn !== "function") {
            return invalid;
        }
        const entries = readOutputTopology(liveWorkspace);
        if (entries === null || entries.length === 0) {
            return invalid;
        }
        interface ScreenBounds {
            readonly ref: object;
            readonly name: string;
            readonly desktopRef: object;
            readonly workspace: string;
            readonly bounds: { x: number; y: number; w: number; h: number };
        }
        interface ScopeWindow {
            readonly id: string;
            readonly ref: object;
            readonly rect: { x: number; y: number; w: number; h: number };
            readonly fullscreen: boolean;
            readonly maximized: boolean;
            readonly floating: boolean;
            readonly sticky: boolean;
            readonly resourceClass: string;
        }
        // One scope's eligible windows with the exact observeNative
        // eligibility (normal windows only, same output, desktop membership
        // or sticky, normalized ids, quantized frames). Seen ids accumulate
        // across both scopes so a duplicate fails closed. Null on invalid
        // evidence (unreadable membership/native/frame or duplicate id),
        // which the caller refuses rather than planning a partial target.
        // The decoded window list rides along explicitly so narrowing holds.
        const collectScopeWindows = (
            scope: ScreenBounds,
            decoded: ReadonlyArray<unknown>,
            seenIds: Set<string>,
        ): ScopeWindow[] | null => {
            const collected: ScopeWindow[] = [];
            for (const item of decoded) {
                if (typeof item !== "object" || item === null) {
                    continue;
                }
                const ref = item as object;
                if (readProp(ref, "normalWindow") !== true) {
                    continue;
                }
                const output = readProp(ref, "output");
                if (typeof output !== "object" || output === null) {
                    continue;
                }
                if (readProp(output as object, "name") !== scope.name) {
                    continue;
                }
                const allDesktops = readProp(ref, "onAllDesktops") === true;
                const membership = decodeList(readProp(ref, "desktops"), MAX_DESKTOPS);
                if (membership === null) {
                    return null;
                }
                let onDesktop = false;
                for (const member of membership) {
                    if (member === scope.desktopRef) {
                        onDesktop = true;
                        break;
                    }
                }
                if (!onDesktop && !allDesktops) {
                    continue;
                }
                const native = readNativeId(ref);
                if (native === null) {
                    return null;
                }
                const id = internNativeId(cache, native, ref, owners);
                if (seenIds.has(id)) {
                    return null;
                }
                seenIds.add(id);
                const frame = readFrameRect(ref);
                if (typeof frame === "string") {
                    // An unreadable scope frame is invalid evidence, never a
                    // silently dropped window: downstream Rust would otherwise
                    // plan against a partial scope.
                    return null;
                }
                collected.push({
                    id,
                    ref,
                    rect: { x: frame.x, y: frame.y, w: frame.w, h: frame.h },
                    fullscreen: readProp(ref, "fullScreen") !== false,
                    maximized: readProp(ref, "maximizeMode") !== 0,
                    floating: floatingIds.has(id) || allDesktops,
                    sticky: allDesktops,
                    resourceClass: readResourceClass(ref),
                });
            }
            return collected;
        };
        const sourceEntry = entries.find((entry) => entry.name === domainOutput);
        if (sourceEntry === undefined) {
            return invalid;
        }
        // The source output's current workspace must match the carried
        // source workspace; otherwise the desktop changed under us: fail
        // closed.
        if (sourceEntry.workspace !== domainWorkspace) {
            return invalid;
        }
        // Focus keeps the legacy unique reciprocal policy; moves (exhausted
        // directional crossing, all four directions) rank multiple FULL-rect
        // candidates by window-centre projection, then window-span overlap,
        // then left/top. No candidate is no-target (local behavior);
        // ambiguous/unreadable evidence is invalid.
        // Pinned R4 re-observation verifies the dispatch-selected target is
        // still a live edge-touching candidate and returns that same pair.
        // It never re-ranks: the mover sits on the target mid-flight, so its
        // relocated frame must not select. A changed or removed target
        // refuses here; existing fences and recovery own the terminal.
        let selected: OutputTopologyEntry | null | "ambiguous";
        if (forMove === false) {
            selected = selectAdjacentOutput(entries, sourceEntry, direction);
        } else if (!pinned) {
            let moverRect: { x: number; y: number; w: number; h: number } | null = null;
            for (const entry of sourceWindowsFull) {
                if (entry.id === sourceFocusedId) {
                    moverRect = { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h };
                    break;
                }
            }
            selected = selectWindowedAdjacentOutput(entries, sourceEntry, direction, moverRect);
        } else {
            const pinnedTarget = (pinnedSource as { targetOutput?: unknown }).targetOutput as string;
            const match = listPositionCandidates(entries, sourceEntry, direction).find(
                (candidate) => candidate.entry.name === pinnedTarget,
            );
            selected = match === undefined ? "ambiguous" : match.entry;
        }
        if (selected === null) {
            return noTarget;
        }
        if (selected === "ambiguous") {
            return invalid;
        }
        const targetEntry: ScreenBounds = selected;
        const seen = new Set<string>();
        // Pinned source windows (R4 re-observation across transfer): the
        // live active window sits on the target then, so the active-derived
        // source part would be the wrong scope. Collected here from the
        // frozen scope exactly like the target part. One shared id set
        // across both scopes fails closed on duplicate native identities.
        if (!pinned) {
            for (const entry of sourceWindowsSeed) {
                seen.add(entry.id);
            }
        }
        const targetWindows = collectScopeWindows(targetEntry, windows, seen);
        if (targetWindows === null) {
            return invalid;
        }
        let pinnedSourceWindows: ScopeWindow[] | null = null;
        if (pinned) {
            pinnedSourceWindows = collectScopeWindows(sourceEntry, windows, seen);
            if (pinnedSourceWindows === null) {
                return invalid;
            }
        }
        const targetAdjacent: Record<string, string> =
            direction === "left"
                ? { right: sourceEntry.name }
                : direction === "right"
                  ? { left: sourceEntry.name }
                  : direction === "up"
                    ? { down: sourceEntry.name }
                    : { up: sourceEntry.name };
        const sourceAdjacent: Record<string, string> = { [direction]: targetEntry.name };
        // FULL rectangles select the neighbor (panel gaps cannot block), but
        // carried placement stays per-desktop WORK AREAS exactly as before:
        // tiling under panels is never produced. Unpinned, the source work
        // area must equal the source observation bounds (the retained
        // equality fence); otherwise the desktop changed under us: fail
        // closed. Pinned scope only requires a readable area. An unreadable
        // target work area is invalid evidence, never a dropped domain.
        const sourceArea = readWorkAreaFor(surface, sourceEntry.ref, sourceEntry.desktopRef);
        if (sourceArea === null) {
            return invalid;
        }
        if (sourceBoundsForFence !== null) {
            if (
                sourceArea.x !== sourceBoundsForFence.x ||
                sourceArea.y !== sourceBoundsForFence.y ||
                sourceArea.w !== sourceBoundsForFence.w ||
                sourceArea.h !== sourceBoundsForFence.h
            ) {
                return invalid;
            }
        }
        const targetArea = readWorkAreaFor(surface, targetEntry.ref, targetEntry.desktopRef);
        if (targetArea === null) {
            return invalid;
        }
        const carriedSourceBounds = { x: sourceArea.x, y: sourceArea.y, w: sourceArea.w, h: sourceArea.h };
        const domains: ReadonlyArray<PlanDomain> = Object.freeze([
            Object.freeze({
                output: domainOutput,
                workspace: domainWorkspace,
                bounds: { x: carriedSourceBounds.x, y: carriedSourceBounds.y, w: carriedSourceBounds.w, h: carriedSourceBounds.h },
                gap: sourceGap,
                outerGap: sourceOuterGap,
                adjacent: Object.freeze(sourceAdjacent),
            }),
            Object.freeze({
                output: targetEntry.name,
                workspace: targetEntry.workspace,
                bounds: { x: targetArea.x, y: targetArea.y, w: targetArea.w, h: targetArea.h },
                gap: gaps.innerGap,
                outerGap: gaps.outerGap,
                adjacent: Object.freeze(targetAdjacent),
            }),
        ]);
        // Source part: the live active-domain windows, except under a
        // differing flight pin (post-transfer the active window sits on the
        // target) where the frozen scope collection above stands in with
        // the pinned identity. Focus there is intentionally empty: R4
        // structural fences ignore it.
        const pinnedMapped: ReadonlyArray<PlanObservedWindow> = pinned
            ? (pinnedSourceWindows as ScopeWindow[]).map((entry) => ({
                  id: entry.id,
                  ref: entry.ref,
                  rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
                  output: domainOutput,
                  workspace: domainWorkspace,
                  fullscreen: entry.fullscreen,
                  maximized: entry.maximized,
                  floating: entry.floating,
                  sticky: entry.sticky,
                  resourceClass: entry.resourceClass,
              }))
            : sourceWindowsFull;
        const combinedWindows = Object.freeze([
            ...pinnedMapped.map((entry) =>
                Object.freeze({
                    id: entry.id,
                    ref: entry.ref,
                    rect: Object.freeze({ x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h }),
                    output: entry.output,
                    workspace: entry.workspace,
                    fullscreen: entry.fullscreen,
                    maximized: entry.maximized,
                    floating: entry.floating === true,
                    sticky: entry.sticky === true,
                    resourceClass: entry.resourceClass ?? "unknown",
                }),
            ),
            ...targetWindows.map((entry) =>
                Object.freeze({
                    id: entry.id,
                    ref: entry.ref,
                    rect: Object.freeze({ x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h }),
                    output: targetEntry.name,
                    workspace: targetEntry.workspace,
                    fullscreen: entry.fullscreen,
                    maximized: entry.maximized,
                    floating: entry.floating,
                    sticky: entry.sticky,
                    resourceClass: entry.resourceClass,
                }),
            ),
        ]);
        // Full-evidence fingerprint: the exact wire fields Rust re-derives
        // (ordered domain primitives, focused id, every window with rect and
        // exception flags), so an altered target rect/bounds/adjacency fails
        // request validation upstream.
        const fitExcluded = (floating: boolean, sticky: boolean, fullscreen: boolean, maximized: boolean): boolean =>
            floating || sticky || fullscreen || maximized;
        const fingerprintWindows = [
            ...pinnedMapped.map((entry) => ({
                window: entry.id,
                output: entry.output,
                workspace: entry.workspace,
                rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
                floating: entry.floating === true,
                fitExcluded: fitExcluded(
                    entry.floating === true,
                    entry.sticky === true,
                    entry.fullscreen,
                    entry.maximized,
                ),
            })),
            ...targetWindows.map((entry) => ({
                window: entry.id,
                output: targetEntry.name,
                workspace: targetEntry.workspace,
                rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
                floating: entry.floating,
                fitExcluded: fitExcluded(entry.floating, entry.sticky, entry.fullscreen, entry.maximized),
            })),
        ];
        const fingerprint = String(
            planDirectionalFingerprint(domains, sourceFocusedId, fingerprintWindows),
        );
        const expectedFingerprint = fingerprint;
        const capturedActive = sourceActiveRef;
        const observed: PlanObserved = {
            domainOutput,
            domainWorkspace,
            domainBounds: Object.freeze({ x: carriedSourceBounds.x, y: carriedSourceBounds.y, w: carriedSourceBounds.w, h: carriedSourceBounds.h }),
            domainGap: sourceGap,
            domainOuterGap: sourceOuterGap,
            focusedId: sourceFocusedId,
            ...(pinned ? { activeExcluded: true as const } : (sourceActiveExcluded === undefined ? {} : { activeExcluded: sourceActiveExcluded })),
            domains,
            windows: combinedWindows,
            activeRef: sourceActiveRef,
            fingerprint: expectedFingerprint,
            revalidate: () => {
                try {
                    const fresh = observeDirectionalDomain(
                        liveWorkspace,
                        cache,
                        floatingIds,
                        gaps,
                        direction,
                        reportEligibility,
                        owners,
                        pinnedSource,
                        forMove,
                    );
                    if (fresh.status !== "ready" || fresh.observed === null) {
                        return false;
                    }
                    if (fresh.observed.fingerprint !== expectedFingerprint) {
                        return false;
                    }
                    if (fresh.observed.activeRef !== capturedActive) {
                        return false;
                    }
                    if (fresh.observed.windows.length !== combinedWindows.length) {
                        return false;
                    }
                    for (const entry of combinedWindows) {
                        let matched = false;
                        for (const candidate of fresh.observed.windows) {
                            if (candidate.id === entry.id) {
                                matched = true;
                                if (candidate.ref !== entry.ref) {
                                    return false;
                                }
                                // Flag-exact: a floating/sticky flip between
                                // observe and revalidate must fail closed.
                                if (candidate.floating !== entry.floating) {
                                    return false;
                                }
                                if (candidate.sticky !== entry.sticky) {
                                    return false;
                                }
                                if (
                                    candidate.rect.x !== entry.rect.x ||
                                    candidate.rect.y !== entry.rect.y ||
                                    candidate.rect.w !== entry.rect.w ||
                                    candidate.rect.h !== entry.rect.h ||
                                    candidate.output !== entry.output ||
                                    candidate.workspace !== entry.workspace
                                ) {
                                    return false;
                                }
                                break;
                            }
                        }
                        if (!matched) {
                            return false;
                        }
                    }
                    void capturedActive;
                    return true;
                } catch (error) {
                    void error;
                    return false;
                }
            },
        };
        return { status: "ready", observed };
    } catch (error) {
        void error;
        return invalid;
    }
}

// Production output-send observation for explicit send-to-output (REQ-OUT-04,
// item 5.3/5.4): the source output workspace plus the commanded output's
// current workspace (destination, whose current workspace the entry resolves
// once), each with its own windows. Without a pin (dispatch time) the source
// is the active mover's output with its live current workspace. With a
// flight pin (adapter re-observation across transfer and arrival) the source
// output and workspace are the frozen flight pair: deriving the source from
// the live active window would collapse target===source once the still-
// active mover lands on the destination, and geometry/arrival could never
// confirm. The live source current workspace is still reported separately
// in `currentWorkspace` for the stay visibility fence. Read-only public
// state only, mirroring observeSendTarget enumeration and eligibility
// (normal windows only, per-output membership or sticky, normalized native
// ids, quantized frame extents, per-desktop work areas for carried bounds).
// Selection uses FULL output rectangles (panel gaps cannot block): the
// entry resolves the target with the same windowed rule as directional
// moves before probing here; carried domain bounds stay per-desktop work
// areas like workspace send.
// Null fail-closed on ambiguous/unreadable topology (the adapter refuses) or
// missing candidates (the entry no-ops before dispatch).
export function observeOutputSendTarget(
    liveWorkspace: unknown,
    cache: Map<string, string>,
    targetOutput: string,
    floatingIds: ReadonlySet<string>,
    pinnedSourceWorkspace?: string,
    pinnedSourceOutput?: string,
    owners?: Map<string, object>,
): WorkspaceSendObserved | null {
    try {
        if (typeof liveWorkspace !== "object" || liveWorkspace === null) {
            return null;
        }
        if (!isOpaqueId(targetOutput)) {
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
        const outputRefByName = (name: string): object | null => {
            for (const item of screens) {
                if (typeof item === "object" && item !== null && readProp(item as object, "name") === name) {
                    return item as object;
                }
            }
            return null;
        };
        // Flight-pinned source (adapter re-observation): the frozen dispatch
        // pair, never the live active window. Unpinned (dispatch probe): the
        // active mover's output, else the first screen. A half-pinned pair
        // fails closed; a pinned output that no longer resolves fails closed.
        const pinned = pinnedSourceWorkspace !== undefined || pinnedSourceOutput !== undefined;
        let sourceOutputRef: object | null = null;
        let sourceOutput = "";
        if (pinned) {
            if (!isOpaqueId(pinnedSourceWorkspace) || !isOpaqueId(pinnedSourceOutput)) {
                return null;
            }
            sourceOutputRef = outputRefByName(pinnedSourceOutput as string);
            if (sourceOutputRef === null) {
                return null;
            }
            sourceOutput = pinnedSourceOutput as string;
        } else if (activeRef !== null) {
            const activeOutput = readProp(activeRef, "output");
            if (typeof activeOutput === "object" && activeOutput !== null) {
                sourceOutputRef = activeOutput as object;
            } else {
                return null;
            }
            const sourceNameRaw = readProp(sourceOutputRef, "name");
            if (!isOpaqueId(sourceNameRaw)) {
                return null;
            }
            sourceOutput = sourceNameRaw as string;
        } else {
            const first = screens[0];
            if (typeof first !== "object" || first === null) {
                return null;
            }
            sourceOutputRef = first as object;
            const sourceNameRaw = readProp(sourceOutputRef, "name");
            if (!isOpaqueId(sourceNameRaw)) {
                return null;
            }
            sourceOutput = sourceNameRaw as string;
        }
        if (targetOutput === sourceOutput) {
            return null;
        }
        const targetOutputRef = outputRefByName(targetOutput);
        if (targetOutputRef === null) {
            return null;
        }
        // Capability seam: per-output currents on 6.7+, shared current on
        // legacy-shared hosts. Null fail-closed.
        const readCurrent = (outputRef: object): { ref: object; id: string } | null => {
            const read = readDesktopForOutput(surface, outputRef);
            if (read === null) {
                return null;
            }
            return { ref: read.ref, id: read.id };
        };
        const sourceCurrent = readCurrent(sourceOutputRef);
        const targetCurrent = readCurrent(targetOutputRef);
        if (sourceCurrent === null || targetCurrent === null) {
            return null;
        }
        const desktops = decodeList(readProp(surface, "desktops"), MAX_DESKTOPS);
        if (desktops === null || desktops.length === 0) {
            return null;
        }
        // Pinned source workspace: the frozen flight workspace with its live
        // desktop ref resolved by stable id (KWin may return fresh wrappers
        // per read). A pinned workspace that no longer exists fails closed.
        // Unpinned: the live source current from above.
        let sourceWorkspace = sourceCurrent.id;
        let sourceDesktopRef = sourceCurrent.ref;
        if (pinned) {
            const wanted = pinnedSourceWorkspace as string;
            let resolved: object | null = null;
            for (const item of desktops) {
                if (typeof item === "object" && item !== null && readProp(item as object, "id") === wanted) {
                    resolved = item as object;
                    break;
                }
            }
            if (resolved === null) {
                return null;
            }
            sourceWorkspace = wanted;
            sourceDesktopRef = resolved;
        }
        let targetExists = false;
        let targetOrdinal = -1;
        for (let index = 0; index < desktops.length; index += 1) {
            const item = desktops[index];
            if (typeof item !== "object" || item === null) {
                continue;
            }
            if (readProp(item as object, "id") === targetCurrent.id) {
                targetExists = true;
                targetOrdinal = index;
            }
        }
        if (!targetExists) {
            return null;
        }
        const sourceBounds = readWorkAreaFor(surface, sourceOutputRef, sourceDesktopRef);
        const targetBounds = readWorkAreaFor(surface, targetOutputRef, targetCurrent.ref);
        if (sourceBounds === null || targetBounds === null) {
            return null;
        }
        // Behavior-fenced actual current workspace on the source output for
        // the stay source-visibility fence: always the LIVE current, even
        // with a flight pin (which freezes scope, never visibility). Null
        // when unreadable; null never counts as selected downstream.
        const currentWorkspace: string | null = sourceCurrent.id;
        let sourceOrdinal = -1;
        for (let index = 0; index < screens.length; index += 1) {
            if (screens[index] === sourceOutputRef) {
                sourceOrdinal = index;
                break;
            }
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
            const managed = readProp(ref, "managed");
            if (managed !== undefined && managed !== true) {
                continue;
            }
            if (readProp(ref, "minimized") === true) {
                continue;
            }
            const output = readProp(ref, "output");
            if (typeof output !== "object" || output === null) {
                continue;
            }
            const outputName = readProp(output as object, "name");
            const onSourceOutput = outputName === sourceOutput;
            const onTargetOutput = outputName === targetOutput;
            if (!onSourceOutput && !onTargetOutput) {
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
            const id = internNativeId(cache, native, ref, owners);
            const membership = decodeList(readProp(ref, "desktops"), MAX_DESKTOPS);
            if (membership === null) {
                return null;
            }
            const memberIds = new Set<string>();
            for (const member of membership) {
                if (typeof member !== "object" || member === null) {
                    return null;
                }
                const memberRaw = readProp(member as object, "id");
                if (!isOpaqueId(memberRaw)) {
                    return null;
                }
                memberIds.add(memberRaw as string);
            }
            const sticky = readProp(ref, "onAllDesktops") === true;
            const inScope = onSourceOutput
                ? memberIds.has(sourceWorkspace) || sticky
                : memberIds.has(targetCurrent.id) || sticky;
            if (!inScope) {
                continue;
            }
            if (seen.has(id)) {
                return null;
            }
            seen.add(id);
            const frame = readFrameRect(ref);
            if (typeof frame === "string") {
                return null;
            }
            const fullscreen = readProp(ref, "fullScreen") !== false;
            const maximized = readProp(ref, "maximizeMode") !== 0;
            const floating = floatingIds.has(id) || sticky;
            const fitExcluded = floating || sticky || fullscreen || maximized;
            const flags = { fullscreen, maximized, floating, sticky, fitExcluded };
            if (onSourceOutput) {
                sourceWindows.push({ id, ref, rect: { x: frame.x, y: frame.y, w: frame.w, h: frame.h }, ...flags });
            } else {
                targetWindows.push({ id, ref, rect: { x: frame.x, y: frame.y, w: frame.w, h: frame.h }, ...flags });
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
                const activeId = internNativeId(cache, activeNative, activeRef, owners);
                for (const entry of sourceWindows) {
                    if (entry.id === activeId) {
                        // Mover gate: tiled movers dispatch, as does a D16
                        // intentional float (floating without sticky) plain
                        // or with exactly one overlay. Sticky, dual-overlay,
                        // and otherwise unexplained fit-excluded movers keep
                        // "" / null and refuse downstream as non-tiled-focus.
                        const entryIntentionalFloat = entry.floating && !entry.sticky;
                        if (
                            entry.sticky ||
                            (entryIntentionalFloat && entry.maximized && entry.fullscreen) ||
                            (!entryIntentionalFloat &&
                                (entry.floating ||
                                    (entry.fitExcluded && !entry.maximized && !entry.fullscreen)))
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
        // The destination desktop ref is the live current desktop wrapper on
        // the target output (resolved once here).
        let targetDesktopRef: object | null = null;
        for (const item of desktops) {
            if (typeof item === "object" && item !== null && readProp(item as object, "id") === targetCurrent.id) {
                targetDesktopRef = item as object;
                break;
            }
        }
        const sourceSorted = sourceWindows.map((entry) => entry.id).sort();
        const targetSorted = targetWindows.map((entry) => entry.id).sort();
        const sourceFingerprint = String(workspaceSendFingerprint(sourceOutput, sourceWorkspace, sourceSorted));
        const targetFingerprint = String(workspaceSendFingerprint(targetOutput, targetCurrent.id, targetSorted));
        const freezeWindows = (list: SendWindow[]) =>
            Object.freeze(
                list.map((entry) =>
                    Object.freeze({
                        id: entry.id,
                        ref: entry.ref,
                        rect: Object.freeze({ x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h }),
                        fullscreen: entry.fullscreen,
                        maximized: entry.maximized,
                        floating: entry.floating,
                        sticky: entry.sticky,
                        fitExcluded: entry.fitExcluded,
                        fit_excluded: entry.fitExcluded,
                    }),
                ),
            );
        return {
            sourceOutput,
            sourceWorkspace,
            sourceBounds: Object.freeze({ x: sourceBounds.x, y: sourceBounds.y, w: sourceBounds.w, h: sourceBounds.h }),
            targetOutput,
            targetWorkspace: targetCurrent.id,
            targetBounds: Object.freeze({ x: targetBounds.x, y: targetBounds.y, w: targetBounds.w, h: targetBounds.h }),
            focusedId,
            sourceWindows: freezeWindows(sourceWindows),
            targetWindows: freezeWindows(targetWindows),
            activeRef,
            moverRef,
            targetDesktopRef,
            targetExists,
            desktopCount: desktops.length,
            sourceFingerprint,
            targetFingerprint,
            targetOrdinal,
            targetNumber: -1,
            outputOrdinal: sourceOrdinal,
            currentOrdinal: -1,
            currentNumber: -1,
            currentIdEq: -1,
            currentRefEq: -1,
            currentWorkspace,
        };
    } catch (error) {
        void error;
        return null;
    }
}

// Explicit production activation; called once by src/entry.ts and directly
// by focused tests with overrides. The single attempt below returns a stop
// handle on success, null fail-closed (silently: only the adapter's two
// bounded line shapes may be logged anywhere on this path). The exported
// startPlanAdapterEntry wrapper retries a transient enable failure on later
// native events (see below); this single attempt never retries itself. The
// optional enable-cause callback reports the actual failed subscription
// stage kind from this attempt's enable loop (no second windowList probe).
function startPlanAdapterEntryOnce(
    overrides: PlanEntryOverrides = {},
    reportEnableCause?: (cause: string) => void,
): PlanEntryHandle | null {
    const liveWorkspace: unknown =
        overrides.workspace !== undefined ? overrides.workspace : resolveLexicalWorkspace();
    const log = overrides.log ?? ((message: string): void => {
        try {
            console.log(message);
        } catch (error) {
            void error;
        }
    });
    if (typeof liveWorkspace !== "object" || liveWorkspace === null) {
        return null;
    }
    let callDbus = overrides.callDbus;
    if (callDbus === undefined) {
        try {
            const native: unknown = callDBus;
            if (typeof native !== "function") {
                return null;
            }
            const bound = native as (...args: ReadonlyArray<unknown>) => void;
            callDbus = (service, path, iface, method, payload, callback) => {
                if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_START_METHOD) {
                    bound(service, path, iface, method, payload, WORKSPACE_SEND_START_FLAGS, callback);
                    return;
                }
                if (service === PLAN_DBUS_SERVICE && method === PLAN_START_METHOD) {
                    bound(service, path, iface, method, payload, PLAN_START_FLAGS, callback);
                    return;
                }
                bound(service, path, iface, method, payload, callback);
            };
        } catch (error) {
            void error;
            return null;
        }
    }
    let scheduleOnce = overrides.scheduleOnce;
    if (scheduleOnce === undefined) {
        try {
            const ctor: unknown = QTimer;
            if (typeof ctor !== "function") {
                return null;
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
            return null;
        }
    }
    const surface = liveWorkspace as Record<string, unknown>;
    const sub = (name: string, handler: () => void): (() => void) | null => {
        try {
            return connectSignal(readSignal(surface, name), handler);
        } catch (error) {
            void error;
            return null;
        }
    };
    const subWindowGeometry = (handler: () => void): (() => void) | null => {
        try {
            const lister = surface["windowList"];
            if (typeof lister !== "function") {
                return null;
            }
            let raw: unknown = undefined;
            try {
                raw = Reflect.apply(lister as (...args: ReadonlyArray<never>) => unknown, surface, []);
            } catch (error) {
                void error;
                return null;
            }
            const list = decodeList(raw, MAX_LIST);
            if (list === null) {
                return null;
            }
            const detaches: Array<() => void> = [];
            const seen = new Set<object>();
            const connectOne = (item: object): void => {
                if (seen.has(item)) {
                    return;
                }
                const detach = connectSignal(readSignal(item, "frameGeometryChanged"), handler);
                if (detach === null) {
                    return;
                }
                seen.add(item);
                detaches.push(detach);
            };
            for (const item of list) {
                if (typeof item === "object" && item !== null) {
                    connectOne(item as object);
                }
            }
            const addedDetach = connectSignal(readSignal(surface, "windowAdded"), (added) => {
                if (typeof added === "object" && added !== null) {
                    connectOne(added as object);
                }
                handler();
            });
            if (addedDetach !== null) {
                detaches.push(addedDetach);
            }
            if (detaches.length === 0) {
                return null;
            }
            return (): void => {
                for (const detach of detaches) {
                    try {
                        detach();
                    } catch (error) {
                        void error;
                    }
                }
            };
        } catch (error) {
            void error;
            return null;
        }
    };
    // Best-effort per-window fullscreen-state subscription.
    // `fullScreenChanged` is a documented per-window signal but is never
    // assumed present: attach where the feature-detecting seam exposes a
    // connectable signal, and keep per-window connections in sync with the
    // live window set. `windowAdded` subscribes the new window (idempotent per
    // object identity), `windowRemoved` detaches the removed window's signal
    // so old subscriptions never accumulate, and the returned detach releases
    // everything. enable() never fails when the source lacks the signal.
    const subWindowFullscreen = (handler: () => void): (() => void) | null => {
        try {
            const lister = surface["windowList"];
            if (typeof lister !== "function") {
                return null;
            }
            const seen = new Set<object>();
            const windowDetaches = new Map<object, () => void>();
            const topDetaches: Array<() => void> = [];
            const connectOne = (ref: object): void => {
                if (seen.has(ref)) {
                    return;
                }
                const detach = connectSignal(readSignal(ref, "fullScreenChanged"), handler);
                if (detach === null) {
                    return;
                }
                seen.add(ref);
                windowDetaches.set(ref, detach);
            };
            const connectAll = (): void => {
                let raw: unknown = undefined;
                try {
                    raw = Reflect.apply(lister as (...args: ReadonlyArray<never>) => unknown, surface, []);
                } catch (error) {
                    void error;
                    return;
                }
                const list = decodeList(raw, MAX_LIST);
                if (list === null) {
                    return;
                }
                for (const item of list) {
                    if (typeof item === "object" && item !== null) {
                        connectOne(item as object);
                    }
                }
            };
            const dropOne = (ref: object): void => {
                const detach = windowDetaches.get(ref);
                if (detach === undefined) {
                    return;
                }
                windowDetaches.delete(ref);
                seen.delete(ref);
                try {
                    detach();
                } catch (error) {
                    void error;
                }
            };
            connectAll();
            const addedDetach = connectSignal(readSignal(surface, "windowAdded"), (added) => {
                if (typeof added === "object" && added !== null) {
                    connectOne(added as object);
                } else {
                    connectAll();
                }
                handler();
            });
            if (addedDetach !== null) {
                topDetaches.push(addedDetach);
            }
            const removedDetach = connectSignal(readSignal(surface, "windowRemoved"), (removed) => {
                if (typeof removed === "object" && removed !== null) {
                    dropOne(removed as object);
                }
            });
            if (removedDetach !== null) {
                topDetaches.push(removedDetach);
            }
            return (): void => {
                for (const detach of windowDetaches.values()) {
                    try {
                        detach();
                    } catch (error) {
                        void error;
                    }
                }
                windowDetaches.clear();
                seen.clear();
                for (const detach of topDetaches) {
                    try {
                        detach();
                    } catch (error) {
                        void error;
                    }
                }
            };
        } catch (error) {
            void error;
            return null;
        }
    };
    // Native state writes require their exact notify signal. Unlike fullscreen,
    // a missing signal cannot leave a state-write fence blind.
    const subWindowRequiredSignal = (
        signalName: string,
        failureToken: string,
        handler: (target?: object) => void,
        hard: boolean,
    ): (() => void) | null => {
        try {
            const lister = surface["windowList"];
            if (typeof lister !== "function") {
                return null;
            }
            const seen = new Set<object>();
            const windowDetaches = new Map<object, () => void>();
            const topDetaches: Array<() => void> = [];
            // A missing maximize signal leaves the window tiled; ordinary
            // observations still read its current maximizeMode.
            let maximizeMissingLogged = false;
            // Only required signals can refuse an eligible normal window.
            const connectOne = (ref: object): boolean => {
                if (seen.has(ref)) {
                    return true;
                }
                const detach = connectSignal(readSignal(ref, signalName), () => handler(ref));
                if (detach === null) {
                    if (signalName === "maximizedChanged" && readProp(ref, "normalWindow") === true) {
                        if (!maximizeMissingLogged) {
                            maximizeMissingLogged = true;
                            try {
                                log(
                                    "omnitiler:plan:maximize-signal-unavailable cause=maximizedChanged-unconnectable recovery=fresh-maximizeMode-read",
                                );
                            } catch (error) {
                                void error;
                            }
                        }
                        return true;
                    }
                    return !hard || readProp(ref, "normalWindow") !== true;
                }
                seen.add(ref);
                windowDetaches.set(ref, detach);
                return true;
            };
            const connectAll = (): { list: ReadonlyArray<unknown> | null; ok: boolean } => {
                let raw: unknown = undefined;
                try {
                    raw = Reflect.apply(lister as (...args: ReadonlyArray<never>) => unknown, surface, []);
                } catch (error) {
                    void error;
                    return { list: null, ok: true };
                }
                const list = decodeList(raw, MAX_LIST);
                if (list === null) {
                    return { list: null, ok: true };
                }
                let ok = true;
                for (const item of list) {
                    if (typeof item === "object" && item !== null) {
                        if (!connectOne(item as object)) {
                            ok = false;
                        }
                    }
                }
                return { list, ok };
            };
            const dropOne = (ref: object): void => {
                const detach = windowDetaches.get(ref);
                if (detach === undefined) {
                    return;
                }
                windowDetaches.delete(ref);
                seen.delete(ref);
                try {
                    detach();
                } catch (error) {
                    void error;
                }
            };
            const detachAll = (): void => {
                for (const detach of windowDetaches.values()) {
                    try {
                        detach();
                    } catch (error) {
                        void error;
                    }
                }
                windowDetaches.clear();
                seen.clear();
                for (const detach of topDetaches) {
                    try {
                        detach();
                    } catch (error) {
                        void error;
                    }
                }
                topDetaches.length = 0;
            };
            const initial = connectAll();
            if (initial.list !== null && !initial.ok) {
                // Startup refusal for a required signal other than
                // best-effort `maximizedChanged` (which never returns false
                // above). Release every per-window subscription made before
                // the failure so the failed enable leaves nothing attached,
                // then fail closed.
                detachAll();
                return null;
            }
            const addedDetach = connectSignal(readSignal(surface, "windowAdded"), (added) => {
                const ok =
                    typeof added === "object" && added !== null ? connectOne(added as object) : connectAll().ok;
                if (!ok) {
                    // A window added after enable lacks a required signal
                    // other than best-effort `maximizedChanged`: fail closed
                    // with the exact token instead of leaving an enabled/blind
                    // adapter. Every subscription of this kind is released and
                    // the adapter is disabled.
                    try {
                        log(failureToken);
                    } catch (error) {
                        void error;
                    }
                    detachAll();
                    try {
                        adapter.disable();
                    } catch (error) {
                        void error;
                    }
                    return;
                }
                handler();
            });
            if (addedDetach !== null) {
                topDetaches.push(addedDetach);
            }
            const removedDetach = connectSignal(readSignal(surface, "windowRemoved"), (removed) => {
                if (typeof removed === "object" && removed !== null) {
                    dropOne(removed as object);
                }
            });
            if (removedDetach !== null) {
                topDetaches.push(removedDetach);
            }
            return detachAll;
        } catch (error) {
            void error;
            return null;
        }
    };
    // String-keyed native identity cache: normalized internalId to stable
    // plan id (the same normalized string, interned). Never keyed by Window.
    // Eviction is explicit when the adapter identifies a removed string id.
    // Exact-ref ownership parallels the cache: every interned id records the
    // live ref observed with it, so the exact native removal signal evicts
    // only ids still owned by that ref. Incomplete observations still record
    // ownership at intern time but never infer departure.
    const nativeIds = new Map<string, string>();
    const nativeOwners = new Map<string, object>();
    // D23 close homing: last-known output/workspace per stable id, recorded
    // by the foreground and hidden observers for observed and
    // minimized-omitted members. Read at exact-ref close time to report the
    // owning domain without ever reading the dead object. Evicted with the
    // id on verified removal.
    const lastHoming = new Map<string, { output: string; workspace: string }>();
    const sendNativeIds = new Map<string, string>();
    const floatingIds = new Set<string>();
    // Deliberate tiler reload gap configuration: resolved at startup, then
    // re-read only on the KWin Options `configChanged` signal. That signal is
    // emitted after `org.kde.KWin /KWin reconfigure` reparses kwinrc
    // (dbusinterface.cpp delegates to Workspace::reconfigure; workspace.cpp
    // slotReconfigure reparses and calls Options::updateSettings which emits
    // configChanged; scripting.cpp exposes that Options singleton as the
    // script global `options`). The running script is never restarted by that
    // call (the Scripting load path returns -1 when already loaded, Script::run
    // returns early when running), so this subscription is the pickup route. Ordinary
    // reads and unchanged saves send nothing and never fire it. No shortcut
    // re-registration, no script/plugin lifecycle, no in-flight reset.
    let domainGaps: DomainGaps = readDomainGaps({
        readInnerGapFn: overrides.readInnerGapFn,
        readOuterGapFn: overrides.readOuterGapFn,
    });
    // R-MOV-03 same-axis move mode: resolved at startup, then re-read only
    // on the KWin Options `configChanged` signal for subsequent moves. No
    // tree rebuild, no resync, no topology work on change.
    let sameAxisMove: SameAxisMove = readSameAxisMoveValue(overrides.readSameAxisMoveFn);
    // R-SPC-04 D1 fixed-size admission predicate: resolved at startup, then
    // re-read only on the KWin Options `configChanged` signal for subsequent
    // admissions. No reclassification, no resync, no topology work on
    // change; the adapter reads the entry-owned value live per request and
    // carries it on the planner wire.
    let fixedSizePredicate: FixedSizePredicate = readFixedSizePredicateValue(
        overrides.readFixedSizePredicateFn,
    );
    // R-MAX-06 born-maximized admission mode: resolved at startup, then
    // re-read only on the KWin Options `configChanged` signal for subsequent
    // admissions. No rebuild, no reclassification, no resync, no topology
    // work on change; the adapter reads the entry-owned value live per
    // admission.
    let bornMaximized: BornMaximized = readBornMaximizedValue(overrides.readBornMaximizedFn);
    // R-MAX-03 floating-first sightings: stable ids first observed on an
    // untiled (floating) domain. Their later tiled-domain admission keeps
    // the maximized overlay over the reserved slot under both R-MAX-06
    // settings with no one-shot clear; only born tiled-first admissions
    // read the R-MAX-06 mode. Bounded (never a lifecycle): evicted on
    // verified native removal, dropped with the entry on stop.
    const floatingFirstIds = new Set<string>();
    const noteFloatingFirstSighted = (ids: ReadonlyArray<string>): void => {
        try {
            for (const id of ids) {
                if (typeof id !== "string" || id.length === 0) {
                    continue;
                }
                if (!floatingFirstIds.has(id)) {
                    if (floatingFirstIds.size >= 4096) {
                        const oldest = floatingFirstIds.values().next();
                        if (!oldest.done) {
                            floatingFirstIds.delete(oldest.value as string);
                        }
                    }
                    floatingFirstIds.add(id);
                }
            }
        } catch (error) {
            void error;
        }
    };
    // R-WS-12 G-37 migration source refill: the workspace-native map
    // commit reads it per migration from the entry-pushed value (startup via
    // the env closure below, then only on the KWin Options `configChanged`
    // signal). No destination, history, or lifecycle work on change.
    // Startup-consumed settings snapshot: workspaceMode and shortcutProfile
    // are never re-read for behavior. A configChanged drift against this
    // snapshot is logged restart-required, never adopted here.
    const startupProfile = readShortcutProfile(overrides.readProfileFn);
    const startupWorkspaceMode = readWorkspaceModeValue(overrides.readWorkspaceModeFn);
    const eligibilityReasons = new Map<string, string>();
    const reportEligibility: EligibilityReporter = (ref, reason): void => {
        if (!KWIN_TRACE_ENABLED) {
            return;
        }
        const id = readNativeId(ref);
        if (id === null) {
            if (reason !== null) {
                try {
                    log(`omnitiler:plan:observe-excluded reason=${reason} window=unknown resource_class=unknown`);
                } catch (error) {
                    void error;
                }
            }
            return;
        }
        if (reason === null) {
            eligibilityReasons.delete(id);
            return;
        }
        if (eligibilityReasons.get(id) === reason) {
            return;
        }
        eligibilityReasons.set(id, reason);
        try {
            log(`omnitiler:plan:observe-excluded reason=${reason} window=${id} resource_class=${readResourceClass(ref)}`);
        } catch (error) {
            void error;
        }
    };
    const forgetNativeId = (id: string): void => {
        nativeIds.delete(id);
        sendNativeIds.delete(id);
        nativeOwners.delete(id);
        lastHoming.delete(id);
        floatingFirstIds.delete(id);
        eligibilityReasons.delete(id);
        // Verified removal also drops the in-progress float mark: a closed
        // window must not keep observing floating on id reuse before the
        // next live sighting re-marks it. The durable settled set is
        // evicted separately through the adapter below.
        floatingIds.delete(id);
    };
    // Entry-owned highlight refresh edge: set once the highlight bridge
    // starts, invoked exactly once per successful geometry-plan boundary.
    let highlightRefresh: (() => void) | null = null;
    // Entry-owned reattach for a startup subscription failure: retried only
    // on later existing Plan-applied or Options configChanged events.
    let retryHighlightAttach: () => void = () => {};
    // Send flights never block Plan. Terminal send settlement arrives
    // through the send adapter's single `onSettled` edge below, which
    // forces a one-shot complete source AND target reconcile through
    // Plan's existing single-flight foreground+hidden chain.
    // Actual failed subscription stage kind from this attempt's enable
    // loop, reported once via reportEnableCause when enable refuses.
    let failedSubscribeKind: string | null = null;
    // Workspace tiling owner, assigned after the adapter. Null before
    // enable reads as tiled.
    let workspaceNativeRef: WorkspaceNativeAdapter | null = null;
    const isEntryDomainTiled = (output: string, workspace: string): boolean => {
        void output;
        try {
            const ref = workspaceNativeRef;
            return ref === null ? true : ref.isTiled(workspace);
        } catch (error) {
            void error;
            return true;
        }
    };
    const adapter = new PlanAdapter({
        callDbus,
        scheduleOnce,
        log,
        // Q3 settled intent bridge, opt-in only: the complete unscoped
        // inventory plus the hydration sink. The adapter runs one bounded
        // read before first plan admission and persists settled snapshots
        // afterwards. Without the flag the adapter sees neither hook and
        // behaves exactly as before.
        ...(overrides.floatIntent === true
            ? {
                  observeCompleteInventory: () => readCompleteIntentInventory(liveWorkspace, nativeOwners),
                  onIntentHydrated: (members, tile) => {
                      try {
                          for (const id of members) {
                              floatingIds.add(id);
                          }
                          // Tile overrides stay tiled (default): ensure a
                          // stale float never survives for them. Tile refs
                          // hydrate lazily in the adapter classifier.
                          if (Array.isArray(tile)) {
                              for (const id of tile) {
                                  floatingIds.delete(id);
                              }
                          }
                      } catch (error) {
                          void error;
                      }
                  },
              }
            : {}),
        isInteractiveResizeActive: () => {
            try {
                // Observed-state gate: a tiled move hold survives while KWin
                // still reports move===true, even past the old
                // missing-Finished bound (no Started timer exists). A hold
                // whose observed gesture already exited without a Finished
                // signal releases here so ordinary reconcile can proceed; a
                // Finished-phase verdict guard keeps its hold until
                // route/settle.
                for (const ref of [...interactiveMoveRefs]) {
                    let awaitingVerdict = false;
                    try {
                        for (const ctx of moveGuardCancels.keys()) {
                            if (ctx.ref === ref) {
                                awaitingVerdict = true;
                                break;
                            }
                        }
                    } catch (error) {
                        void error;
                        awaitingVerdict = true;
                    }
                    if (awaitingVerdict) {
                        continue;
                    }
                    let live: { move: boolean; resize: boolean } | null = null;
                    try {
                        live = readLiveState(ref);
                    } catch (error) {
                        void error;
                        live = null;
                    }
                    if (live !== null && live.move !== true) {
                        try {
                            interactiveMoveRefs.delete(ref);
                        } catch (error) {
                            void error;
                        }
                        try {
                            if (oracleStarts.get(ref)?.move === true) oracleStarts.delete(ref);
                        } catch (error) {
                            void error;
                        }
                        try {
                            clearMovePreviewFull(ref);
                        } catch (error) {
                            void error;
                        }
                    }
                }
                // Observed-state gate: a resize hold survives while KWin
                // still reports resize===true (no Started timer exists). A
                // hold whose observed gesture already exited without a
                // Finished signal releases here so ordinary reconcile can
                // proceed. Unreadable state keeps the hold; removal releases
                // it separately.
                for (const ref of [...interactiveResizeRefs]) {
                    if (!isLiveResizeStillActive(ref)) {
                        try {
                            interactiveResizeRefs.delete(ref);
                        } catch (error) {
                            void error;
                        }
                        try {
                            const start = oracleStarts.get(ref);
                            if (start !== undefined && start.move === false && start.resize === true) {
                                oracleStarts.delete(ref);
                            }
                        } catch (error) {
                            void error;
                        }
                    }
                }
            } catch (error) {
                void error;
            }
            return interactiveResizeRefs.size > 0 || interactiveMoveRefs.size > 0;
        },
        onPlannedApplied: () => {
            try {
                highlightRefresh?.();
            } catch (error) {
                void error;
            }
            try {
                retryHighlightAttach();
            } catch (error) {
                void error;
            }
        },
        readSameAxisMove: () => sameAxisMove,
        readFixedSizePredicate: () => fixedSizePredicate,
        readBornMaximized: () => bornMaximized,
        isFloatingFirst: (id) => {
            try {
                return floatingFirstIds.has(id);
            } catch (error) {
                void error;
                return false;
            }
        },
        observe: () => {
            const seen = observeNative(liveWorkspace, nativeIds, floatingIds, domainGaps, reportEligibility, nativeOwners, lastHoming);
            if (seen === null) {
                return null;
            }
            // Floating workspaces are unmanaged: hide the foreground
            // observation so automatic tiling, Meta+G, and group queries
            // skip it. Release-domain snapshots bypass this filter via the
            // raw observers below.
            if (!isEntryDomainTiled(seen.domainOutput, seen.domainWorkspace)) {
                // R-MAX-03: record floating-first sightings so the later
                // tiled-domain admission keeps the overlay under both
                // born-maximized settings.
                try {
                    noteFloatingFirstSighted(seen.windows.map((entry) => entry.id));
                } catch (error) {
                    void error;
                }
                return null;
            }
            return seen;
        },
        observeHidden: () => {
            const list = observeHiddenDomains(liveWorkspace, nativeIds, floatingIds, domainGaps, reportEligibility, nativeOwners, lastHoming);
            const out: PlanObserved[] = [];
            for (const entry of list) {
                if (isEntryDomainTiled(entry.domainOutput, entry.domainWorkspace)) {
                    out.push(entry);
                } else {
                    // R-MAX-03: record floating-first sightings as above.
                    try {
                        noteFloatingFirstSighted(entry.windows.map((member) => member.id));
                    } catch (error) {
                        void error;
                    }
                }
            }
            return Object.freeze(out);
        },
        observeDirectional: (direction, pinnedSource?, forMove?) => {
            const outcome = observeDirectionalDomain(
                liveWorkspace,
                nativeIds,
                floatingIds,
                domainGaps,
                direction,
                reportEligibility,
                nativeOwners,
                pinnedSource,
                forMove,
            );
            if (outcome.status !== "ready" || outcome.observed === null) {
                return outcome;
            }
            const seen = outcome.observed;
            if (!isEntryDomainTiled(seen.domainOutput, seen.domainWorkspace)) {
                return { status: "invalid", observed: null };
            }
            const domains = seen.domains;
            if (domains !== undefined) {
                for (const entry of domains) {
                    if (!isEntryDomainTiled(entry.output, entry.workspace)) {
                        return { status: "invalid", observed: null };
                    }
                }
            }
            return outcome;
        },
        isDomainTiled: (output, workspace) => isEntryDomainTiled(output, workspace),
        clearMaximize: (target) => {
            try {
                const method = readProp(target, "setMaximize");
                if (typeof method !== "function") {
                    return "missing";
                }
                Reflect.apply(method as (...args: ReadonlyArray<unknown>) => unknown, target, [false, false]);
                return "invoked";
            } catch (error) {
                void error;
                return "threw";
            }
        },
        setMaximize: (target, maximized) => {
            try {
                const method = readProp(target, "setMaximize");
                if (typeof method !== "function") {
                    return "missing";
                }
                Reflect.apply(method as (...args: ReadonlyArray<unknown>) => unknown, target, [maximized, maximized]);
                return "invoked";
            } catch (error) {
                void error;
                return "threw";
            }
        },
        setFullscreen: (target, fullscreen) => {
            try {
                if (readProp(target, "fullScreen") === undefined) {
                    return "missing";
                }
                return Reflect.set(target, "fullScreen", fullscreen) ? "invoked" : "threw";
            } catch (error) {
                void error;
                return "threw";
            }
        },
        setAllDesktops: (target, allDesktops) => {
            try {
                const probe = connectSignal(readSignal(target, "desktopsChanged"), () => {});
                if (probe === null) {
                    return "missing";
                }
                probe();
                return Reflect.set(target, "onAllDesktops", allDesktops) ? "invoked" : "threw";
            } catch (error) {
                void error;
                return "threw";
            }
        },
        readKeepAbove: (target) => {
            try {
                const value = readProp(target, "keepAbove");
                return typeof value === "boolean" ? value : null;
            } catch (error) {
                void error;
                return null;
            }
        },
        readKeepBelow: (target) => {
            try {
                const value = readProp(target, "keepBelow");
                return typeof value === "boolean" ? value : null;
            } catch (error) {
                void error;
                return null;
            }
        },
        setKeepAbove: (target, keepAbove) => {
            try {
                const current = readProp(target, "keepAbove");
                const below = readProp(target, "keepBelow");
                if (typeof current !== "boolean" || typeof below !== "boolean") {
                    return "missing";
                }
                if (!Reflect.set(target, "keepAbove", keepAbove)) {
                    return "threw";
                }
                const actualAbove = readProp(target, "keepAbove");
                const actualBelow = readProp(target, "keepBelow");
                return actualAbove === keepAbove && (!keepAbove || actualBelow === false) ? "invoked" : "refused";
            } catch (error) {
                void error;
                return "threw";
            }
        },
        setKeepBelow: (target, keepBelow) => {
            try {
                if (typeof readProp(target, "keepBelow") !== "boolean") {
                    return "missing";
                }
                if (!Reflect.set(target, "keepBelow", keepBelow)) {
                    return "threw";
                }
                const actualAbove = readProp(target, "keepAbove");
                const actualBelow = readProp(target, "keepBelow");
                return actualBelow === keepBelow && (!keepBelow || actualAbove === false) ? "invoked" : "refused";
            } catch (error) {
                void error;
                return "threw";
            }
        },
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
        setFloating: (id, floating) => {
            if (floating) {
                floatingIds.add(id);
            } else {
                floatingIds.delete(id);
            }
        },
        // Production R4 cross-output transfer: exact Output object plus exact
        // target VirtualDesktop refs through public typed surfaces only.
        // sendClientToScreen initiates transfer only; the adapter proves the
        // resulting output, desktop membership, geometry, and focus
        // asynchronously before ack/verify commit.
        resolveOutput: (name) => {
            try {
                const screens = decodeList(readProp(surface, "screens"), MAX_LIST);
                if (screens === null) {
                    return null;
                }
                for (const item of screens) {
                    if (typeof item === "object" && item !== null && readProp(item as object, "name") === name) {
                        return item as object;
                    }
                }
                return null;
            } catch (error) {
                void error;
                return null;
            }
        },
        resolveDesktop: (workspaceId) => {
            try {
                const desktops = decodeList(readProp(surface, "desktops"), MAX_DESKTOPS);
                if (desktops === null) {
                    return null;
                }
                for (const item of desktops) {
                    if (typeof item === "object" && item !== null && readProp(item as object, "id") === workspaceId) {
                        return item as object;
                    }
                }
                return null;
            } catch (error) {
                void error;
                return null;
            }
        },
        sendClientToScreen: (mover, output) => {
            try {
                const send = readProp(surface, "sendClientToScreen");
                if (typeof send !== "function") {
                    return false;
                }
                Reflect.apply(send as (...args: ReadonlyArray<unknown>) => unknown, surface, [mover, output]);
                return true;
            } catch (error) {
                void error;
                return false;
            }
        },
        setDesktops: (mover, desktops) => {
            try {
                const probe = connectSignal(readSignal(mover, "desktopsChanged"), () => {});
                if (probe === null) {
                    return false;
                }
                probe();
                return Reflect.set(mover, "desktops", [...desktops]);
            } catch (error) {
                void error;
                return false;
            }
        },
        readOutputName: (ref) => {
            try {
                // Lifetime first: retained properties may stay readable on a
                // removed or replaced ref, so values alone never prove the
                // window is live. The ref must be the identical object in
                // the current window list.
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
                const list = decodeList(rawList, MAX_LIST);
                if (list === null) {
                    return null;
                }
                let listed = false;
                for (const item of list) {
                    if (item === ref) {
                        listed = true;
                        break;
                    }
                }
                if (!listed) {
                    return null;
                }
                const output = readProp(ref, "output");
                if (typeof output !== "object" || output === null) {
                    return null;
                }
                const name = readProp(output as object, "name");
                return isOpaqueId(name) ? (name as string) : null;
            } catch (error) {
                void error;
                return null;
            }
        },
        readDesktopIds: (ref) => {
            try {
                // Lifetime first: see readOutputName above. Never trust
                // retained membership bytes from an unlisted ref.
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
                const list = decodeList(rawList, MAX_LIST);
                if (list === null) {
                    return null;
                }
                let listed = false;
                for (const item of list) {
                    if (item === ref) {
                        listed = true;
                        break;
                    }
                }
                if (!listed) {
                    return null;
                }
                const membership = decodeList(readProp(ref, "desktops"), MAX_DESKTOPS);
                if (membership === null) {
                    return null;
                }
                const ids: string[] = [];
                for (const member of membership) {
                    if (typeof member !== "object" || member === null) {
                        return null;
                    }
                    const id = readProp(member as object, "id");
                    if (!isOpaqueId(id)) {
                        return null;
                    }
                    ids.push(id as string);
                }
                return Object.freeze(ids);
            } catch (error) {
                void error;
                return null;
            }
        },
        readGeometry: (ref) => {
            try {
                const frame = readFrameRect(ref);
                return typeof frame === "string" ? null : { x: frame.x, y: frame.y, w: frame.w, h: frame.h };
            } catch (error) {
                void error;
                return null;
            }
        },
        readWindowConstraints: (ref) => readWindowConstraints(ref),
        // Bounded R4 signal fences grounded in the documented notify
        // signals: outputChanged carries the old output (re-read for the
        // current value), desktopsChanged for membership, and
        // frameGeometryChanged per changed window. One-shot: the adapter
        // detaches after each echo and on every terminal path.
        subscribeMoverOutput: (moverRef, handler) => {
            try {
                return connectSignal(readSignal(moverRef, "outputChanged"), handler);
            } catch (error) {
                void error;
                return null;
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
        subscribeWindowGeometry: (windowRef, handler) => {
            try {
                return connectSignal(readSignal(windowRef, "frameGeometryChanged"), handler);
            } catch (error) {
                void error;
                return null;
            }
        },
        setActive: (target) => {
            try {
                (liveWorkspace as { activeWindow: unknown }).activeWindow = target;
                return true;
            } catch (error) {
                void error;
                return false;
            }
        },
        active: () => {
            try {
                const current = (liveWorkspace as { activeWindow: unknown }).activeWindow;
                return typeof current === "object" && current !== null
                    ? (current as object)
                    : null;
            } catch (error) {
                void error;
                return null;
            }
        },
        subscribe: (kind, handler) => {
            if (kind === "geometry") {
                const detach = subWindowGeometry(handler);
                if (detach === null) {
                    failedSubscribeKind = "geometry";
                    throw new Error("plan-entry-signal-failed");
                }
                return detach;
            }
            if (kind === "scope") {
                const first = sub("screensChanged", handler);
                const second = sub("currentDesktopChanged", handler);
                if (first === null || second === null) {
                    if (first !== null) {
                        try {
                            first();
                        } catch (error) {
                            void error;
                        }
                    }
                    failedSubscribeKind = "scope";
                    throw new Error("plan-entry-signal-failed");
                }
                return (): void => {
                    try {
                        first();
                    } catch (error) {
                        void error;
                    }
                    try {
                        second();
                    } catch (error) {
                        void error;
                    }
                };
            }
            if (kind === "fullscreen") {
                const detach = subWindowFullscreen(handler);
                if (detach === null) {
                    return (): void => {};
                }
                return detach;
            }
            if (kind === "maximize") {
                const detach = subWindowRequiredSignal("maximizedChanged", "omnitiler:plan:maximize-refused-signal", handler, true);
                if (detach === null) {
                    // Infrastructure failure (e.g. unreadable window list):
                    // refuse fail-closed with the exact maximize-specific
                    // token. A merely unconnectable per-window
                    // `maximizedChanged` never reaches here; that window stays
                    // tiled on fresh `maximizeMode` reads with a single
                    // best-effort unavailable line from the subscription.
                    try {
                        log("omnitiler:plan:maximize-refused-signal");
                    } catch (error) {
                        void error;
                    }
                    failedSubscribeKind = "maximize";
                    throw new Error("plan-entry-maximize-signal-failed");
                }
                return detach;
            }
            if (kind === "desktops") {
                const detach = subWindowRequiredSignal("desktopsChanged", "omnitiler:plan:sticky-refused-signal", handler, false);
                if (detach === null) {
                    return (): void => {};
                }
                return detach;
            }
            const name =
                kind === "added"
                    ? "windowAdded"
                    : kind === "removed"
                      ? "windowRemoved"
                      : "windowActivated";
            const detach = sub(name, handler);
            if (detach === null) {
                failedSubscribeKind = typeof kind === "string" ? kind : "enable-refused";
                throw new Error("plan-entry-signal-failed");
            }
            return detach;
        },
        noteRemoved: (id) => {
            try {
                const homing = lastHoming.get(id);
                forgetNativeId(id);
                // D23: an explicitly closed id (tiled or minimized-omitted)
                // reports its owning domain for slot cleanup; a bare absence
                // stays a minimize. Homing unknown means the planner never
                // saw the id, so there is no slot to clear.
                if (homing !== undefined) {
                    try {
                        adapter.noteClosedWindow(id, homing.output, homing.workspace);
                    } catch (error) {
                        void error;
                    }
                }
            } catch (error) {
                void error;
            }
            try {
                adapter.noteNativeRemovedId(id);
            } catch (error) {
                void error;
            }
        },
        noteNativeRemoved: (ref) => {
            try {
                for (const [id, owner] of nativeOwners) {
                    if (owner === ref) {
                        const homing = lastHoming.get(id);
                        forgetNativeId(id);
                        // Verified exact-ref close: propagate the live-time
                        // id to the adapter so fixed bookkeeping and settled
                        // intent membership evict without ever reading the
                        // dead object.
                        try {
                            adapter.noteNativeRemovedId(id);
                        } catch (error) {
                            void error;
                        }
                        // D23 close-versus-minimize report, including
                        // minimized-omitted clients whose homing was recorded
                        // while live. Same-turn delivery rides the debounced
                        // resync; failures retain for the next signal.
                        if (homing !== undefined) {
                            try {
                                adapter.noteClosedWindow(id, homing.output, homing.workspace);
                            } catch (error) {
                                void error;
                            }
                        }
                    }
                }
            } catch (error) {
                void error;
            }
        },
    });
    const enabled = adapter.enable({ owner: overrides.owner, generation: overrides.generation });
    if (!enabled) {
        try {
            reportEnableCause?.(failedSubscribeKind ?? "enable-refused");
        } catch (error) {
            void error;
        }
        return null;
    }
    // Q3: one bounded intent read before first plan admission. Hydration
    // populates floatingIds above, so the initial planning observes
    // recovered floats as floating (intentional, never automatic) with
    // their live frames preserved and no actuation of their own.
    if (overrides.floatIntent === true) {
        try {
            adapter.startIntentBootstrap();
        } catch (error) {
            void error;
        }
    }
    const initial = observeNative(liveWorkspace, nativeIds, floatingIds, domainGaps, reportEligibility, nativeOwners, lastHoming);
    const initialHidden = observeHiddenDomains(liveWorkspace, nativeIds, floatingIds, domainGaps, reportEligibility, nativeOwners, lastHoming);
    // Distinguish a foreground/hidden eligible startup from an empty one for
    // diagnostics. Both retain the enabled observer for later observations.
    let hasEligibleHidden = false;
    for (const entry of initialHidden) {
        if (entry.windows.length === 0) {
            continue;
        }
        let eligible = false;
        for (const member of entry.windows) {
            if (!member.floating && !member.sticky && !member.fullscreen && !member.maximized) {
                eligible = true;
                break;
            }
        }
        if (eligible) {
            hasEligibleHidden = true;
            break;
        }
    }
    const emptyStartup = (initial === null || initial.windows.length === 0) && !hasEligibleHidden;
    if (emptyStartup) {
        // Keep the observer so the next complete window observation can tile.
        try {
            log(
                `omnitiler:plan:empty-startup owner=${String(overrides.owner)} generation=${String(overrides.generation)} cause=no-eligible-windows recovery=await-next-window`,
            );
        } catch (error) {
            void error;
        }
    }
    // Startup session context from the existing owner/generation provenance
    // plus the compiled-in source revision: one bounded ready line identifies
    // the plan session for the whole journal so every later cmd/write/rejected
    // line is attributable.
    try {
        log(
            `omnitiler:plan:ready owner=${String(overrides.owner)} generation=${String(overrides.generation)} source=${PLAN_SOURCE_REV}`,
        );
    } catch (error) {
        void error;
    }
    adapter.requestResync();
    const profile = readShortcutProfile(overrides.readProfileFn);
    const catalog = planShortcutCatalog(profile);
    let registerFn = overrides.registerShortcutFn;
    if (registerFn === undefined) {
        try {
            const native: unknown = registerShortcut;
            if (typeof native !== "function") {
                // Automatic observation-driven tiling operates independently
                // of shortcuts: keep the enabled observer/handle and skip
                // only shortcut registration. Re-register on explicit reload.
                try {
                    log(
                        `omnitiler:plan:shortcut-catalog-unavailable owner=${String(overrides.owner)} generation=${String(overrides.generation)} cause=register-shortcut-missing recovery=automatic-tiling-continue`,
                    );
                } catch (error) {
                    void error;
                }
            } else {
                const bound = native as (
                    name: string,
                    text: string,
                    sequence: string,
                    callback: () => void,
                ) => boolean;
                registerFn = (name, text, sequence, callback) => bound(name, text, sequence, callback);
            }
        } catch (error) {
            void error;
            try {
                log(
                    `omnitiler:plan:shortcut-catalog-unavailable owner=${String(overrides.owner)} generation=${String(overrides.generation)} cause=register-shortcut-threw recovery=automatic-tiling-continue`,
                );
            } catch (inner) {
                void inner;
            }
        }
    }
    if (registerFn !== undefined) {
        for (const row of catalog) {
            const action = row.action;
            const text = row.text;
            const sequence = row.sequence;
            const op = row.op;
            const direction = row.direction;
            const mode = row.mode;
            try {
                const ok =
                    op === "resize"
                        ? registerFn(action, text, sequence, () => adapter.requestResize(direction, mode))
                        : op === "move"
                          ? registerFn(action, text, sequence, () => adapter.requestMove(direction))
                          : op === "float"
                            ? registerFn(action, text, sequence, () => adapter.requestFloat())
                            : op === "sticky"
                              ? registerFn(action, text, sequence, () => adapter.requestSticky())
                              : op === "maximize"
                                ? registerFn(action, text, sequence, () => adapter.requestMaximize())
                                : op === "fullscreen"
                                  ? registerFn(action, text, sequence, () => adapter.requestFullscreen())
                                  : op === "toggle-orientation"
                                    ? registerFn(action, text, sequence, () => adapter.requestToggleOrientation())
                                    : registerFn(action, text, sequence, () => adapter.requestFocus(direction));
                if (ok !== true) {
                    try {
                        log(`omnitiler:plan:shortcut-failed action=${action} sequence=${sequence}`);
                    } catch (error) {
                        void error;
                    }
                } else if (action === "omnitiler-toggle-float") {
                    // KGlobalAccel keeps both registrations and dispatches the
                    // earliest serial holder. Grid View is already registered by
                    // KWin, so this new action is visible in Settings but cannot
                    // receive Meta+G until the user resolves that conflict there.
                    try {
                        log("omnitiler:plan:shortcut-dispatch-shadowed action=omnitiler-toggle-float sequence=Meta+G holder_component=kwin holder_action=Grid View");
                    } catch (error) {
                        void error;
                    }
                } else if (action === "omnitiler-toggle-maximize") {
                    try {
                        log("omnitiler:plan:shortcut-dispatch-shadowed action=omnitiler-toggle-maximize sequence=Meta+M holder_component=kwin holder_action=KrohnkiteMonocleLayout");
                    } catch (error) {
                        void error;
                    }
                }
            } catch (error) {
                void error;
                try {
                    log(`omnitiler:plan:shortcut-failed action=${action} sequence=${sequence}`);
                } catch (inner) {
                    void inner;
                }
            }
        }
        // Explicit output-send shortcuts (REQ-OUT-04, item 5.3): follow forms
        // carry the Meta+Ctrl+Alt arm, stay forms register unbound. The
        // callbacks close over requestOutputSend, which is initialized below
        // before any shortcut can fire.
        for (const row of planOutputSendShortcutCatalog()) {
            try {
                const ok = registerFn(row.action, row.text, row.sequence, () =>
                    requestOutputSend(row.direction, row.follow),
                );
                if (ok !== true) {
                    try {
                        log(`omnitiler:plan:shortcut-failed action=${row.action} sequence=${row.sequence}`);
                    } catch (error) {
                        void error;
                    }
                }
            } catch (error) {
                void error;
                try {
                    log(`omnitiler:plan:shortcut-failed action=${row.action} sequence=${row.sequence}`);
                } catch (inner) {
                    void inner;
                }
            }
        }
        // Whole-workspace migration shortcuts (R-WS-12): follow-only forms
        // in four directions, registered bindable but unbound. The callbacks
        // close over requestWorkspaceMigrate, initialized below before any
        // shortcut can fire.
        for (const row of planWorkspaceMigrateShortcutCatalog()) {
            try {
                const ok = registerFn(row.action, row.text, row.sequence, () =>
                    requestWorkspaceMigrate(row.direction),
                );
                if (ok !== true) {
                    try {
                        log(`omnitiler:plan:shortcut-failed action=${row.action} sequence=${row.sequence}`);
                    } catch (error) {
                        void error;
                    }
                }
            } catch (error) {
                void error;
                try {
                    log(`omnitiler:plan:shortcut-failed action=${row.action} sequence=${row.sequence}`);
                } catch (inner) {
                    void inner;
                }
            }
        }
    }
    // Production dynamic workspace route: the native adapter owns the
    // project-owned backing-desktop mapping and lifecycle observation; the
    // existing send adapter owns the sole structural same-output tiled move
    // through Rust, which commits its planned topology synchronously in the
    // same call, then follows to the Rust-planned target desktop after its
    // own fresh native mover-membership confirmation. Every terminal flight
    // settles through `onSettled` into one forced Plan source+target
    // refresh; Plan is never blocked.
    const workspaceNative = new WorkspaceNativeAdapter({
        getWorkspace: () => liveWorkspace,
        readWorkspaceMode: () => readWorkspaceModeValue(overrides.readWorkspaceModeFn),
        readTilingDefault: () => readTilingDefaultValue(overrides.readTilingDefaultFn),
        readMigrationSourceRefill: () => readMigrationSourceRefillValue(overrides.readMigrationSourceRefillFn),
        log,
    });
    workspaceNative.enable();
    workspaceNativeRef = workspaceNative;
    const getWorkspaceTilingSnapshot = (): WorkspaceTilingSnapshot => {
        try {
            const scope = workspaceNative.currentScopeId() ?? "";
            const current = workspaceNative.isCurrentTiled();
            const defaultTiled = workspaceNative.getDefaultTiled();
            return { scope, tiled: current === null ? defaultTiled : current, defaultTiled };
        } catch (error) {
            void error;
            return { scope: "", tiled: true, defaultTiled: true };
        }
    };
    const emitWorkspaceTiling = (): void => {
        try {
            overrides.onWorkspaceTilingChanged?.(getWorkspaceTilingSnapshot());
        } catch (error) {
            void error;
        }
    };
    // Retile marks tiled only after the exact domain release confirms;
    // failures retry on a later lifecycle edge.
    const confirmedReleases = new Set<string>();
    const pendingRetiles = new Set<string>();
    const dispatchedReleases = new Set<string>();
    let pendingRetileIds: string[] = [];
    const releaseDomainKey = (output: string, workspace: string): string => `${output}\u0000${workspace}`;
    const collectAllSnapshots = (): PlanSnapshot[] => {
        const out: PlanSnapshot[] = [];
        const seen = new Set<string>();
        const push = (snapshot: PlanSnapshot): void => {
            const key = releaseDomainKey(snapshot.domainOutput, snapshot.domainWorkspace);
            if (!seen.has(key)) {
                seen.add(key);
                out.push(snapshot);
            }
        };
        try {
            const foreground = observeNative(liveWorkspace, nativeIds, floatingIds, domainGaps, reportEligibility, nativeOwners, lastHoming);
            if (foreground !== null) {
                push(snapshotOf(foreground));
            }
        } catch (error) {
            void error;
        }
        try {
            const hidden = observeHiddenDomains(liveWorkspace, nativeIds, floatingIds, domainGaps, reportEligibility, nativeOwners, lastHoming);
            for (const entry of hidden) {
                push(snapshotOf(entry));
            }
        } catch (error) {
            void error;
        }
        return out;
    };
    const collectReleaseSnapshots = (workspaceId: string): PlanSnapshot[] =>
        collectAllSnapshots().filter((snapshot) => snapshot.domainWorkspace === workspaceId);
    const markTiledAndResync = (ids: ReadonlyArray<string>): void => {
        for (const id of ids) {
            workspaceNative.setTiled(id, true);
        }
        workspaceNative.handleTopologySignal();
        emitWorkspaceTiling();
        // Q2 enable recheck (D6): reset by CURRENT sightings per domain,
        // then the resync observation classifies. Omitted members keep
        // their records; see adapter retileAutomaticFixed.
        try {
            for (const id of ids) {
                for (const snapshot of collectReleaseSnapshots(id)) {
                    adapter.retileAutomaticFixed(
                        snapshot.domainOutput,
                        snapshot.domainWorkspace,
                        snapshot.windows.map((entry) => entry.id),
                    );
                }
            }
        } catch (error) {
            void error;
        }
        log(`omnitiler:plan:workspace-floating tiled=true awaiting-release=0 confirmed=true`);
        adapter.requestResync();
        highlightRefresh?.();
    };
    const dispatchRelease = (snapshot: PlanSnapshot, key: string): void => {
        if (dispatchedReleases.has(key)) {
            return;
        }
        dispatchedReleases.add(key);
        try {
            adapter.requestDomainRelease(snapshot, (outcome) => {
                dispatchedReleases.delete(key);
                if (outcome === "released") {
                    confirmedReleases.add(key);
                    log(`omnitiler:plan:workspace-released outcome=released windows=${String(snapshot.windows.length)}`);
                } else {
                    log(`omnitiler:plan:workspace-released outcome=${outcome} recovery=retry-on-event`);
                    return;
                }
                if (!pendingRetiles.has(key)) {
                    return;
                }
                pendingRetiles.delete(key);
                if (pendingRetiles.size > 0) {
                    return;
                }
                const retileIds = pendingRetileIds;
                pendingRetileIds = [];
                try {
                    markTiledAndResync(retileIds);
                } catch (error) {
                    void error;
                }
            });
        } catch (error) {
            dispatchedReleases.delete(key);
            void error;
        }
    };
    const retryPendingReleases = (): void => {
        if (pendingRetiles.size === 0) {
            return;
        }
        // Retry pending domain keys against fresh observations so the retry
        // carries current geometry. Runs only on lifecycle edges.
        try {
            const byKey = new Map<string, PlanSnapshot>();
            for (const snapshot of collectAllSnapshots()) {
                byKey.set(releaseDomainKey(snapshot.domainOutput, snapshot.domainWorkspace), snapshot);
            }
            for (const key of [...pendingRetiles]) {
                if (confirmedReleases.has(key)) {
                    continue;
                }
                const snapshot = byKey.get(key);
                if (snapshot !== undefined) {
                    dispatchRelease(snapshot, key);
                }
            }
        } catch (error) {
            void error;
        }
    };
    const toggleWorkspaceTiling = (): void => {
        try {
            const scope = workspaceNative.currentScopeId();
            if (scope === null) {
                log("omnitiler:plan:workspace-tiling-refused reason=no-scope");
                return;
            }
            const ids = [scope];
            const target = !workspaceNative.isTiled(scope);
            if (target === false) {
                for (const id of ids) {
                    workspaceNative.setTiled(id, false);
                }
                workspaceNative.handleTopologySignal();
                emitWorkspaceTiling();
                // Floating leaves windows in place. A pending retile intent
                // for these domains is cancelled.
                const snapshots: PlanSnapshot[] = [];
                for (const id of ids) {
                    for (const snapshot of collectReleaseSnapshots(id)) {
                        snapshots.push(snapshot);
                    }
                }
                if (snapshots.length === 0) {
                    try {
                        log("omnitiler:plan:workspace-floating windows=0 release=none");
                    } catch (error) {
                        void error;
                    }
                    return;
                }
                for (const snapshot of snapshots) {
                    const key = releaseDomainKey(snapshot.domainOutput, snapshot.domainWorkspace);
                    confirmedReleases.delete(key);
                    pendingRetiles.delete(key);
                }
                if (pendingRetiles.size === 0) {
                    pendingRetileIds = [];
                }
                for (const snapshot of snapshots) {
                    const key = releaseDomainKey(snapshot.domainOutput, snapshot.domainWorkspace);
                    dispatchRelease(snapshot, key);
                }
                try {
                    log(`omnitiler:plan:workspace-floating tiled=false domains=${String(snapshots.length)}`);
                } catch (error) {
                    void error;
                }
                try {
                    highlightRefresh?.();
                } catch (error) {
                    void error;
                }
                return;
            }
            // Retile stays floating until the release confirms.
            const retileSnapshots: PlanSnapshot[] = [];
            for (const id of ids) {
                for (const snapshot of collectReleaseSnapshots(id)) {
                    retileSnapshots.push(snapshot);
                }
            }
            const unconfirmed: PlanSnapshot[] = [];
            for (const snapshot of retileSnapshots) {
                const key = releaseDomainKey(snapshot.domainOutput, snapshot.domainWorkspace);
                if (!confirmedReleases.has(key)) {
                    unconfirmed.push(snapshot);
                }
            }
            if (unconfirmed.length === 0) {
                try {
                    markTiledAndResync(ids);
                } catch (error) {
                    void error;
                }
                return;
            }
            for (const snapshot of unconfirmed) {
                const key = releaseDomainKey(snapshot.domainOutput, snapshot.domainWorkspace);
                pendingRetiles.add(key);
            }
            pendingRetileIds = [...ids];
            for (const snapshot of unconfirmed) {
                const key = releaseDomainKey(snapshot.domainOutput, snapshot.domainWorkspace);
                dispatchRelease(snapshot, key);
            }
            try {
                log(`omnitiler:plan:workspace-floating tiled=false awaiting-release=${String(unconfirmed.length)} pending-retile=true`);
            } catch (error) {
                void error;
            }
        } catch (error) {
            void error;
        }
    };
    const emitNativeFollow = (
        diagnostic: WorkspaceFollowNativeDiagnostic,
        event: string,
        outcome: string,
        detail: {
            readonly api: string;
            readonly returnKind: string;
            readonly callOrdinal: number;
            readonly callTotal: number;
            readonly selection: string;
            readonly mode: string;
            readonly outputs: number;
            readonly targetOrdinal: number;
            readonly currentIdEq: number;
            readonly activeIdEq: number;
            readonly exception: string;
        },
    ): void => {
        if (!KWIN_TRACE_ENABLED) {
            return;
        }
        try {
            log(
                `omnitiler:route-diag component=cosmic-send stage=follow correlation=${diagnostic.correlation} generation=${String(overrides.generation)} revision=${String(diagnostic.revision)} event=${event} outcome=${outcome} diag_seq=${String(diagnostic.nextSequence())} api=${detail.api} return_kind=${detail.returnKind} call_ord=${String(detail.callOrdinal)} call_total=${String(detail.callTotal)} selection=${detail.selection} mode=${detail.mode} outputs=${String(detail.outputs)} tgt_ord=${String(detail.targetOrdinal)} cur_id_eq=${String(detail.currentIdEq)} active_id_eq=${String(detail.activeIdEq)} exception=${detail.exception}`,
            );
        } catch (error) {
            void error;
        }
    };
    const nativeDetail = (
        api: string,
        returnKind: string,
        callOrdinal: number,
        callTotal: number,
        selection: string,
        mode: string,
        outputs: number,
        targetOrdinal: number,
        currentIdEq: number,
        activeIdEq: number,
        exception: string,
    ) => ({ api, returnKind, callOrdinal, callTotal, selection, mode, outputs, targetOrdinal, currentIdEq, activeIdEq, exception });
    const workspaceSend = new WorkspaceSendAdapter(
        {
        callDbus,
        scheduleOnce,
        log,
        observe: (targetWorkspace, pinnedSourceWorkspace?) =>
            observeSendTarget(liveWorkspace, sendNativeIds, targetWorkspace, floatingIds, pinnedSourceWorkspace, nativeOwners),
        // Single terminal-send edge: every settled flight (arrival, failed
        // native write, stale/rejected reply, missing callback, deadline,
        // closed mover, disable with a live flight) forces one complete
        // source AND target reconcile through Plan's single-flight
        // foreground+hidden chain, bypassing the equal-applied-evidence
        // quiet path once per involved domain. Unreadable domains quarantine
        // and retry on the next complete observation; nothing is invented
        // and the Plan baseline is never reset here. The send adapter
        // releases source/target retention before this callback, so
        // lifecycle cleanup may now prune under the normal rules.
        onSettled: (settled: WorkspaceSendSettled) => {
            try {
                adapter.notifySendSettled(settled);
            } catch (error) {
                void error;
            }
            try {
                workspaceNative.handleTopologySignal();
            } catch (error) {
                void error;
            }
            emitWorkspaceTiling();
        },
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
        setDesktops: (target, refs) => {
            try {
                return Reflect.set(target, "desktops", refs);
            } catch (error) {
                void error;
                return false;
            }
        },
        isDomainTiled: (output, workspace) => isEntryDomainTiled(output, workspace),
        subscribeMoverDesktops: (moverRef, handler) => {
            try {
                return connectSignal(readSignal(moverRef, "desktopsChanged"), handler);
            } catch (error) {
                void error;
                return null;
            }
        },
        // Cross-output transfer capabilities for explicit output send.
        // Exact Output object resolution by stable session name; the mover
        // desktop-membership write stays setDesktops above. Output reads
        // prove arrival; the outputChanged fence covers delayed transfer.
        observeOutput: (targetOutput, targetWorkspace, pinnedSourceWorkspace?, pinnedSourceOutput?) => {
            const observed = observeOutputSendTarget(
                liveWorkspace,
                sendNativeIds,
                targetOutput,
                floatingIds,
                pinnedSourceWorkspace,
                pinnedSourceOutput,
                nativeOwners,
            );
            // The entry resolves the destination workspace once; a drifted
            // live workspace here is stale scope, never a retarget. A pinned
            // source must round-trip exactly when pinned.
            if (observed === null || observed.targetWorkspace !== targetWorkspace) {
                return null;
            }
            if (pinnedSourceOutput !== undefined && observed.sourceOutput !== pinnedSourceOutput) {
                return null;
            }
            if (pinnedSourceWorkspace !== undefined && observed.sourceWorkspace !== pinnedSourceWorkspace) {
                return null;
            }
            return observed;
        },
        resolveOutput: (name) => {
            try {
                const screens = decodeList(readProp(surface, "screens"), MAX_LIST);
                if (screens === null) {
                    return null;
                }
                for (const item of screens) {
                    if (typeof item === "object" && item !== null && readProp(item as object, "name") === name) {
                        return item as object;
                    }
                }
                return null;
            } catch (error) {
                void error;
                return null;
            }
        },
        sendClientToScreen: (mover, output) => {
            try {
                const sender = readProp(surface, "sendClientToScreen");
                if (typeof sender !== "function") {
                    return false;
                }
                Reflect.apply(sender as (...args: ReadonlyArray<never>) => unknown, surface, [mover, output]);
                return true;
            } catch (error) {
                void error;
                return false;
            }
        },
        readOutputName: (ref) => {
            try {
                const output = readProp(ref, "output");
                if (typeof output !== "object" || output === null) {
                    return null;
                }
                const nameRaw = readProp(output as object, "name");
                return isOpaqueId(nameRaw) ? (nameRaw as string) : null;
            } catch (error) {
                void error;
                return null;
            }
        },
        readDesktopIds: (ref) => {
            try {
                const membership = decodeList(readProp(ref, "desktops"), MAX_DESKTOPS);
                if (membership === null) {
                    return null;
                }
                const ids: string[] = [];
                for (const member of membership) {
                    if (typeof member !== "object" || member === null) {
                        return null;
                    }
                    const id = readProp(member as object, "id");
                    if (!isOpaqueId(id)) {
                        return null;
                    }
                    ids.push(id as string);
                }
                return Object.freeze(ids);
            } catch (error) {
                void error;
                return null;
            }
        },
        // Read-only live mover evidence for cross-output mid-write fences:
        // the retained ref must still be the identical live object in the
        // current window list with its readable native identity and current
        // exception flags. Null on anything unreadable, unlisted, or
        // exceptional-state-unreadable: closed or replaced movers fail
        // closed before any later setter. Never invents identity.
        readMoverLive: (moverRef) => {
            try {
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
                const list = decodeList(rawList, MAX_LIST);
                if (list === null) {
                    return null;
                }
                let listed = false;
                for (const item of list) {
                    if (item === moverRef) {
                        listed = true;
                        break;
                    }
                }
                if (!listed) {
                    return null;
                }
                const native = readNativeId(moverRef);
                if (native === null) {
                    return null;
                }
                const sticky = readProp(moverRef, "onAllDesktops") === true;
                return {
                    id: native,
                    floating: floatingIds.has(native) || sticky,
                    sticky,
                    fullscreen: readProp(moverRef, "fullScreen") !== false,
                    maximized: readProp(moverRef, "maximizeMode") !== 0,
                };
            } catch (error) {
                void error;
                return null;
            }
        },
        subscribeMoverOutput: (moverRef, handler) => {
            try {
                return connectSignal(readSignal(moverRef, "outputChanged"), handler);
            } catch (error) {
                void error;
                return null;
            }
        },
        // R-WS-12 whole-workspace migration observation: FULL source view
        // plus the affected target current view, resolved once via
        // FULL-rectangle adjacency. The adapter re-observes with the frozen
        // pin across transfer; a drifted scope returns null and fails
        // closed. Workspace policy (mode plus the STRICT TRUE native
        // perOutput flag) is captured live on every observation so mode or
        // flag drift stales the flight.
        observeMigrate: (direction, pinned) => {
            try {
                const policy = {
                    mode: workspaceNative.getMode(),
                    perOutput: readPerOutputVirtualDesktopsFlag(
                        overrides.options !== undefined ? overrides.options : resolveLexicalOptions(),
                    ),
                };
                const seen = observeMigrateWorkspace(
                    liveWorkspace,
                    sendNativeIds,
                    floatingIds,
                    domainGaps,
                    direction,
                    policy,
                    nativeOwners,
                    pinned === undefined
                        ? undefined
                        : {
                              sourceOutput: pinned.sourceOutput,
                              sourceWorkspace: pinned.sourceWorkspace,
                              targetOutput: pinned.targetOutput,
                              ...(pinned.expectViews === undefined
                                  ? {}
                                  : {
                                        expectViews: {
                                            source: pinned.expectViews.source,
                                            target: pinned.expectViews.target,
                                        },
                                    }),
                          },
                    {
                        originOf: (id, ref) => {
                            try {
                                const kind = adapter.originOfFixedClient(id, ref);

                                if (kind === null) {
                                    return null;
                                }
                                return { fixedAuto: kind === "auto", fixedSuppress: kind === "suppress" };
                            } catch (error) {
                                void error;
                                return null;
                            }
                        },
                        tilingOf: (workspaceId) => {
                            try {
                                return workspaceNative.isTiled(workspaceId);
                            } catch (error) {
                                void error;
                                return true;
                            }
                        },
                    },
                );
                return seen.status === "ready" ? seen.observed : null;
            } catch (error) {
                void error;
                return null;
            }
        },
        // R-WS-12 session map commit: moves the backing workspace id after
        // the target output's current workspace. Map-only; native views
        // run through switchMigrateView below.
        commitWorkspaceMap: (sourceOutput, targetOutput, workspaceId) => {
            try {
                const screens = decodeList(readProp(surface, "screens"), MAX_LIST);
                if (screens === null) {
                    return null;
                }
                let sourceRef: object | null = null;
                let targetRef: object | null = null;
                for (const item of screens) {
                    if (typeof item !== "object" || item === null) {
                        continue;
                    }
                    if (readProp(item as object, "name") === sourceOutput) {
                        sourceRef = item as object;
                    }
                    if (readProp(item as object, "name") === targetOutput) {
                        targetRef = item as object;
                    }
                }
                if (sourceRef === null || targetRef === null) {
                    return null;
                }
                return workspaceNative.commitWorkspaceMigration(sourceRef, targetRef, workspaceId);
            } catch (error) {
                void error;
                return null;
            }
        },
        // R-WS-12 per-output view switch with an immediate readback: the
        // named desktop id must read back current on the named output.
        // True only on a confirmed switch; never retries, never claims.
        switchMigrateView: (desktopId, outputName) => {
            try {
                if (!isOpaqueId(desktopId) || !isOpaqueId(outputName)) {
                    return false;
                }
                const screens = decodeList(readProp(surface, "screens"), MAX_LIST);
                const desktops = decodeList(readProp(surface, "desktops"), MAX_DESKTOPS);
                if (screens === null || desktops === null) {
                    return false;
                }
                let outputRef: object | null = null;
                for (const item of screens) {
                    if (typeof item === "object" && item !== null && readProp(item as object, "name") === outputName) {
                        outputRef = item as object;
                        break;
                    }
                }
                let desktopRef: object | null = null;
                for (const item of desktops) {
                    if (typeof item === "object" && item !== null && readProp(item as object, "id") === desktopId) {
                        desktopRef = item as object;
                        break;
                    }
                }
                // Capability seam: per-output switch on 6.7+, shared
                // write/readback on legacy-shared hosts where one desktop is
                // current everywhere. Whole-workspace migration itself is
                // refused upstream on 6.6, so this only runs where supported.
                if (outputRef === null || desktopRef === null) {
                    return false;
                }
                if (!writeDesktopForOutput(surface, desktopRef, outputRef)) {
                    return false;
                }
                const read = readDesktopForOutput(surface, outputRef);
                return read !== null && read.id === desktopId;
            } catch (error) {
                void error;
                return false;
            }
        },
        // R-WS-12 read-only live active output (the KWin activeScreen
        // name), or null when unreadable. Never switches: the pre-slot
        // expectation proof for empty/sticky-active output activation.
        readActiveOutput: () => {
            try {
                let screen: unknown = undefined;
                try {
                    screen = Reflect.get(liveWorkspace as object, "activeScreen");
                } catch (error) {
                    void error;
                    return null;
                }
                if (typeof screen !== "object" || screen === null) {
                    return null;
                }
                const nameRaw = readProp(screen as object, "name");
                return isOpaqueId(nameRaw) ? (nameRaw as string) : null;
            } catch (error) {
                void error;
                return null;
            }
        },
        // R-WS-12 native directional output-switch slot for
        // empty/sticky-active migrations (KWin useractions
        // switchToOutput/setActiveOutput). Invoked only when the target
        // output is not already active, with the source proven active
        // first. True means the slot was invoked, never a claim: the
        // adapter reads back the active output itself. No retry, and no
        // focus setter ever rides along.
        switchActiveOutput: (direction) => {
            try {
                const slot =
                    direction === "left"
                        ? "slotSwitchToLeftScreen"
                        : direction === "right"
                          ? "slotSwitchToRightScreen"
                          : direction === "up"
                            ? "slotSwitchToAboveScreen"
                            : "slotSwitchToBelowScreen";
                const fn = readProp(surface, slot);
                if (typeof fn !== "function") {
                    return false;
                }
                Reflect.apply(fn as (...args: ReadonlyArray<never>) => unknown, surface, []);
                return true;
            } catch (error) {
                void error;
                return false;
            }
        },
        switchToTarget: (desktopRef, diagnostic) => {
            try {
                const followSelection = selectFollowOutput();
                const screens = followSelection.screens;
                if (screens.length === 0) {
                    emitNativeFollow(diagnostic, "native-switch-before", "unavailable", nativeDetail("unknown", "void", -1, -1, "unknown", "unknown", -1, -1, -1, -1, "none"));
                    return false;
                }
                const selection = followSelection.selection;
                const activeOutput = followSelection.output;
                if (activeOutput === null) {
                    emitNativeFollow(diagnostic, "native-switch-before", "unavailable", nativeDetail("unknown", "void", -1, -1, selection, "unknown", screens.length, -1, -1, -1, "none"));
                    return false;
                }
                // Legacy-shared hosts (KWin 6.6) have no per-output switch:
                // one shared currentDesktop write covers every output and one
                // shared readback confirms it. Partial shapes keep the
                // api-missing refusal below.
                if (!supportsPerOutputDesktops(surface)) {
                    const legacyIdRaw = readProp(desktopRef, "id");
                    if (!isOpaqueId(legacyIdRaw)) {
                        emitNativeFollow(diagnostic, "native-switch-before", "target-unreadable", nativeDetail("missing", "void", -1, -1, selection, "unknown", screens.length, -1, -1, -1, "none"));
                        return false;
                    }
                    const legacyTargetId = legacyIdRaw as string;
                    const legacyDesktops = decodeList(readProp(surface, "desktops"), MAX_DESKTOPS);
                    const legacyOrdinal = legacyDesktops === null ? -1 : legacyDesktops.indexOf(desktopRef);
                    emitNativeFollow(diagnostic, "native-switch-before", "ready", nativeDetail("missing", "void", -1, screens.length, selection, "shared", screens.length, legacyOrdinal, -1, -1, "none"));
                    emitNativeFollow(diagnostic, "native-switch-call", "attempted", nativeDetail("missing", "void", 0, screens.length, selection, "shared", screens.length, legacyOrdinal, -1, -1, "none"));
                    if (!writeDesktopForOutput(surface, desktopRef, activeOutput)) {
                        emitNativeFollow(diagnostic, "native-switch-call", "unconfirmed", nativeDetail("missing", "void", 0, screens.length, selection, "shared", screens.length, legacyOrdinal, -1, -1, "none"));
                        return false;
                    }
                    emitNativeFollow(diagnostic, "native-switch-call", "returned-void", nativeDetail("missing", "void", 0, screens.length, selection, "shared", screens.length, legacyOrdinal, -1, -1, "none"));
                    const legacyRead = readDesktopForOutput(surface, activeOutput);
                    const legacyEq = legacyRead !== null && legacyRead.id === legacyTargetId ? 1 : 0;
                    emitNativeFollow(diagnostic, "native-switch-readback", legacyEq === 1 ? "observed" : "mismatch", nativeDetail("missing", "void", 0, screens.length, selection, "shared", screens.length, legacyOrdinal, legacyEq, -1, "none"));
                    return legacyEq === 1;
                }
                if (!hasPerOutputSwitch(surface)) {
                    emitNativeFollow(diagnostic, "native-switch-before", "api-missing", nativeDetail("missing", "void", -1, -1, selection, "unknown", screens.length, -1, -1, -1, "none"));
                    return false;
                }
                // Shared mode shows one desktop everywhere, so follow every
                // output; otherwise follow only the active output. Mapping
                // itself stays owned by WorkspaceNativeAdapter and rebuilds on
                // the next topology signal. The setter is void in KWin
                // scripting (WorkspaceWrapper::setCurrentDesktopForScreen) and
                // never reports success, so confirm with one immediate direct
                // current-desktop read by stable desktop id (KWin may return a
                // fresh wrapper object per read); a mismatch reports false
                // with no retry so follow stays truthful while commit is
                // preserved.
                const targetIdRaw = readProp(desktopRef, "id");
                if (!isOpaqueId(targetIdRaw)) {
                    emitNativeFollow(diagnostic, "native-switch-before", "target-unreadable", nativeDetail("available", "void", -1, -1, selection, "unknown", screens.length, -1, -1, -1, "none"));
                    return false;
                }
                const targetId = targetIdRaw as string;
                const desktops = decodeList(readProp(surface, "desktops"), MAX_DESKTOPS);
                const targetOrdinal = desktops === null ? -1 : desktops.indexOf(desktopRef);
                const mode = workspaceNative.getMode() === "shared" ? "shared" : "single";
                const selected = mode === "shared" ? screens : [activeOutput];
                emitNativeFollow(diagnostic, "native-switch-before", "ready", nativeDetail("available", "void", -1, selected.length, selection, mode, screens.length, targetOrdinal, -1, -1, "none"));
                const confirmCurrent = (output: object, callOrdinal: number): boolean => {
                    try {
                        // Capability seam (per-output on 6.7+, shared on
                        // legacy-shared hosts, which never reach here).
                        const read = readDesktopForOutput(surface, output);
                        if (read === null) {
                            emitNativeFollow(diagnostic, "native-switch-readback", "unreadable", nativeDetail("available", "void", callOrdinal, selected.length, selection, mode, screens.length, targetOrdinal, -1, -1, "none"));
                            return false;
                        }
                        const currentEq = read.id === targetId ? 1 : 0;
                        emitNativeFollow(diagnostic, "native-switch-readback", currentEq === 1 ? "observed" : "mismatch", nativeDetail("available", "void", callOrdinal, selected.length, selection, mode, screens.length, targetOrdinal, currentEq, -1, "none"));
                        return currentEq === 1;
                    } catch (error) {
                        void error;
                        emitNativeFollow(diagnostic, "native-switch-readback", "exception", nativeDetail("available", "void", callOrdinal, selected.length, selection, mode, screens.length, targetOrdinal, -1, -1, "caught"));
                        return false;
                    }
                };
                if (mode === "shared") {
                    for (let index = 0; index < screens.length; index += 1) {
                        const output = screens[index];
                        if (typeof output !== "object" || output === null) {
                            emitNativeFollow(diagnostic, "native-switch-call", "skipped-non-object-output", nativeDetail("available", "void", index, selected.length, selection, mode, screens.length, targetOrdinal, -1, -1, "none"));
                            return false;
                        }
                        emitNativeFollow(diagnostic, "native-switch-call", "attempted", nativeDetail("available", "void", index, selected.length, selection, mode, screens.length, targetOrdinal, -1, -1, "none"));
                        if (!writeDesktopForOutput(surface, desktopRef, output as object)) {
                            return false;
                        }
                        emitNativeFollow(diagnostic, "native-switch-call", "returned-void", nativeDetail("available", "void", index, selected.length, selection, mode, screens.length, targetOrdinal, -1, -1, "none"));
                    }
                    for (let index = 0; index < screens.length; index += 1) {
                        const output = screens[index];
                        if (typeof output !== "object" || output === null) {
                            emitNativeFollow(diagnostic, "native-switch-readback", "skipped-non-object-output", nativeDetail("available", "void", index, selected.length, selection, mode, screens.length, targetOrdinal, -1, -1, "none"));
                            return false;
                        }
                        if (!confirmCurrent(output as object, index)) {
                            return false;
                        }
                    }
                    return true;
                }
                emitNativeFollow(diagnostic, "native-switch-call", "attempted", nativeDetail("available", "void", 0, 1, selection, mode, screens.length, targetOrdinal, -1, -1, "none"));
                if (!writeDesktopForOutput(surface, desktopRef, activeOutput)) {
                    return false;
                }
                emitNativeFollow(diagnostic, "native-switch-call", "returned-void", nativeDetail("available", "void", 0, 1, selection, mode, screens.length, targetOrdinal, -1, -1, "none"));
                return confirmCurrent(activeOutput, 0);
            } catch (error) {
                void error;
                emitNativeFollow(diagnostic, "native-switch-after", "exception", nativeDetail("unknown", "void", -1, -1, "unknown", "unknown", -1, -1, -1, -1, "caught"));
                return false;
            }
        },
        focusWindow: (windowRef, diagnostic) => {
            try {
                const targetId = readNativeId(windowRef);
                if (targetId === null) {
                    emitNativeFollow(diagnostic, "native-focus-before", "target-unreadable", nativeDetail("property", "direct-assignment", -1, 1, "mover", "single", -1, -1, -1, -1, "none"));
                    return false;
                }
                emitNativeFollow(diagnostic, "native-focus-before", "ready", nativeDetail("property", "direct-assignment", -1, 1, "mover", "single", -1, -1, -1, -1, "none"));
                emitNativeFollow(diagnostic, "native-focus-call", "attempted", nativeDetail("property", "direct-assignment", 0, 1, "mover", "single", -1, -1, -1, -1, "none"));
                (liveWorkspace as { activeWindow: unknown }).activeWindow = windowRef;
                emitNativeFollow(diagnostic, "native-focus-call", "returned-no-api-result", nativeDetail("property", "direct-assignment", 0, 1, "mover", "single", -1, -1, -1, -1, "none"));
                const active = Reflect.get(liveWorkspace as object, "activeWindow");
                const activeId = typeof active === "object" && active !== null ? readNativeId(active as object) : null;
                const activeEq = activeId === null ? -1 : activeId === targetId ? 1 : 0;
                emitNativeFollow(diagnostic, "native-focus-after", activeEq === -1 ? "unreadable" : activeEq === 1 ? "observed" : "mismatch", nativeDetail("property", "direct-assignment", 0, 1, "mover", "single", -1, -1, -1, activeEq, "none"));
                return activeEq === 1;
            } catch (error) {
                void error;
                emitNativeFollow(diagnostic, "native-focus-after", "exception", nativeDetail("property", "direct-assignment", -1, 1, "mover", "single", -1, -1, -1, -1, "caught"));
                return false;
            }
        },
        },
        { innerGap: domainGaps.innerGap, outerGap: domainGaps.outerGap },
    );
    // Terminal send semantics: the send adapter may enable once at startup.
    // Thereafter a terminal disable (pending mismatch, owner loss, refusal,
    // or reset correlation sequence) stays fail-closed. No normal workspace
    // shortcut may restore it.
    // Bounded flight-lifetime ordering: while the send flight is live, the
    // native cleanup must not prune its exact pending source/target ids even
    // when empty and non-visible after follow. Provider-read per cleanup, so
    // settlement (which clears the pin before `onSettled`) restores normal
    // pruning with no extra state.
    try {
        workspaceNative.setRetentionProvider(() => workspaceSend.pendingWorkspaces);
    } catch (error) {
        void error;
    }
    try {
        workspaceSend.enable({ owner: overrides.owner, generation: overrides.generation });
    } catch (error) {
        void error;
    }
    const requestWorkspaceSelect = (index: unknown): void => {
        try {
            if (index === 0) {
                workspaceNative.selectTrailingOrCreate();
                return;
            }
            if (typeof index !== "number" || !Number.isInteger(index)) {
                return;
            }
            workspaceNative.selectLogical(index);
        } catch (error) {
            void error;
        }
    };
    const requestWorkspacePrevious = (): void => {
        try {
            workspaceNative.selectPrevious();
        } catch (error) {
            void error;
        }
    };
    const requestWorkspaceRelative = (delta: unknown): void => {
        try {
            if (delta !== -1 && delta !== 1) {
                return;
            }
            workspaceNative.selectRelative(delta);
        } catch (error) {
            void error;
        }
    };
    const readActiveMover = (): object | null => {
        try {
            const surface = liveWorkspace as Record<string, unknown>;
            const active = readProp(surface, "activeWindow");
            if (typeof active !== "object" || active === null) {
                return null;
            }
            return active as object;
        } catch (error) {
            void error;
            return null;
        }
    };
    const resolveDesktopRef = (workspaceId: string): object | null => {
        try {
            const surface = liveWorkspace as Record<string, unknown>;
            const desktops = decodeList(readProp(surface, "desktops"), MAX_DESKTOPS);
            if (desktops === null) {
                return null;
            }
            for (const item of desktops) {
                if (typeof item === "object" && item !== null && readProp(item as object, "id") === workspaceId) {
                    return item as object;
                }
            }
            return null;
        } catch (error) {
            void error;
            return null;
        }
    };
    const writeMoverDesktops = (mover: object, targetRef: object): boolean => {
        try {
            const probe = connectSignal(readSignal(mover, "desktopsChanged"), () => {});
            if (probe === null) {
                return false;
            }
            probe();
            const applied = Reflect.set(mover, "desktops", [targetRef]);
            if (applied !== true) {
                return false;
            }
            // Immediate readback: never claim native-moved when the desktop
            // write did not take (shared moverOnDesktop proof below).
            return moverOnDesktop(mover, targetRef);
        } catch (error) {
            void error;
            return false;
        }
    };
    // Shared active-output selection for desktop follow: the active
    // window's output, else the active screen, else the first live screen.
    // The selection source rides along for follow diagnostics. Screens ride
    // along so shared-mode fan-out and the snapshot use one reading.
    const selectFollowOutput = (): { output: object | null; selection: string; screens: ReadonlyArray<unknown> } => {
        try {
            const surface = liveWorkspace as Record<string, unknown>;
            let screens: ReadonlyArray<unknown> = [];
            try {
                const decoded = decodeList(readProp(surface, "screens"), MAX_LIST);
                if (decoded !== null) {
                    screens = decoded;
                }
            } catch (error) {
                void error;
            }
            try {
                const active = Reflect.get(surface, "activeWindow");
                if (typeof active === "object" && active !== null) {
                    const output = readProp(active as object, "output");
                    if (typeof output === "object" && output !== null) {
                        return { output: output as object, selection: "active-window", screens };
                    }
                }
            } catch (error) {
                void error;
            }
            try {
                const screen = Reflect.get(surface, "activeScreen");
                if (typeof screen === "object" && screen !== null) {
                    return { output: screen as object, selection: "active-screen", screens };
                }
            } catch (error) {
                void error;
            }
            const first = screens[0];
            if (typeof first === "object" && first !== null) {
                return { output: first as object, selection: "first-screen", screens };
            }
            return { output: null, selection: "first-screen", screens };
        } catch (error) {
            void error;
            return { output: null, selection: "active-window", screens: [] };
        }
    };
    // Exact single-target membership proof by wrapper identity, falling
    // back to stable desktop id since KWin may return fresh wrappers per
    // read. Shared by the post-write readback and the pre-switch arrival
    // proof; both treat anything else as unproven.
    const moverOnDesktop = (mover: object, targetRef: object): boolean => {
        try {
            const targetId = readProp(targetRef, "id");
            const members = decodeList(readProp(mover, "desktops"), MAX_DESKTOPS);
            if (members === null) {
                return false;
            }
            for (const member of members) {
                if (member === targetRef) {
                    return true;
                }
            }
            if (isOpaqueId(targetId)) {
                for (const member of members) {
                    if (typeof member === "object" && member !== null && readProp(member as object, "id") === targetId) {
                        return true;
                    }
                }
            }
            return false;
        } catch (error) {
            void error;
            return false;
        }
    };
    // Follow/stay selection shared by absolute and relative sends: only an
    // exact false stays on the source; anything else (including absent)
    // follows the mover into the target.
    const parseFollowFlag = (value: unknown): boolean => value !== false;
    // Q3 bootstrap fence shared by every send trigger (workspace absolute
    // and relative moves, output sends, and the shared resolved transfer):
    // while the single intent read is outstanding a hydration candidate
    // still observes tiled, so no send may actuate membership, geometry,
    // or focus meanwhile. Same bounded deferred shape as plan dispatch; no
    // queue, no replay. Returns true when the caller must refuse.
    const refuseSendWhileBootstrapPending = (event: string, kind: string, reqOrd: number): boolean => {
        let pending = false;
        try {
            pending = adapter.isIntentBootstrapPending();
        } catch (error) {
            void error;
            pending = false;
        }
        if (!pending) {
            return false;
        }
        try {
            log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=${event} outcome=intent-bootstrap-deferred follow=not-reached gate=pre-commit phase=entry reason=intent-bootstrap-pending req_ord=${String(reqOrd)} inflight_stage=idle`);
            log(`omnitiler:plan:intent-bootstrap-deferred kind=${kind}`);
        } catch (error) {
            void error;
        }
        return true;
    };
    const requestWorkspaceMove = (index: unknown, followRaw?: unknown): void => {
        try {
            const follow = parseFollowFlag(followRaw);
            if (typeof index !== "number" || !Number.isInteger(index) || index < 0 || index > 9) {
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-move outcome=invalid-logical-target follow=not-reached gate=pre-commit phase=entry reason=invalid-logical-target req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            if (refuseSendWhileBootstrapPending("workspace-move", "workspace-move", index)) {
                return;
            }
            if (!workspaceSend.isEnabled || workspaceSend.isInFlight || adapter.isInFlight) {
                const outcome = !workspaceSend.isEnabled
                    ? "disabled"
                    : workspaceSend.isInFlight
                      ? "busy-send"
                      : "busy-plan";
                try {
                    let correlation = "";
                    let inflightStage = "idle";
                    try {
                        correlation = workspaceSend.activeCorrelation;
                        inflightStage = workspaceSend.activeStage;
                    } catch (error) {
                        void error;
                    }
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation=${correlation} generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-move outcome=${outcome} follow=not-reached gate=pre-commit phase=entry reason=${outcome} req_ord=${String(index)} inflight_stage=${inflightStage}`);
                    log("omnitiler:plan:busy-refused kind=workspace-move");
                } catch (error) {
                    void error;
                }
                return;
            }
            const target =
                index === 0 ? workspaceNative.resolveOrAppendMoveTarget() : workspaceNative.resolveMoveTarget(index);
            if (target === null) {
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-move outcome=target-unresolved follow=not-reached gate=pre-commit phase=entry reason=target-unresolved req_ord=${String(index)} inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            transferResolvedWorkspace(target, index, follow);
        } catch (error) {
            void error;
        }
    };
    // Native-only follow for floating-boundary sends: switch to the target
    // desktop then focus the mover, each with an immediate readback and no
    // retry. The follow output is snapshotted before the membership write
    // (KWin may refocus a survivor on another output, or null, as a result
    // of the write) and the mover arrival is re-proved before any switch.
    // Returns the bounded follow token for the entry outcome line.
    const followNativeToTarget = (
        targetRef: object,
        mover: object,
        preWrite: { output: object | null; screens: ReadonlyArray<unknown> },
    ): string => {
        try {
            const surface = liveWorkspace as Record<string, unknown>;
            const screens = preWrite.screens;
            if (screens.length === 0) {
                return "native-unavailable";
            }
            const targetId = readProp(targetRef, "id");
            if (!isOpaqueId(targetId)) {
                return "native-unavailable";
            }
            // Exact mover arrival proof before any switch: the membership
            // write must still name the target (same proof as the post-write
            // readback). A stale membership refuses without setters.
            if (!moverOnDesktop(mover, targetRef)) {
                return "arrival-unconfirmed";
            }
            // Switch capability: legacy-shared hosts (KWin 6.6) use one shared
            // write/readback; partial shapes keep native-unavailable. The
            // helpers stay fail-closed, so 6.7+ behavior is unchanged.
            const legacySharedSwitch = !supportsPerOutputDesktops(surface);
            if (!legacySharedSwitch && !hasPerOutputSwitch(surface)) {
                return "native-unavailable";
            }
            const shared = workspaceNative.getMode() === "shared";
            const selected: object[] = [];
            if (shared) {
                for (const output of screens) {
                    if (typeof output === "object" && output !== null) {
                        selected.push(output as object);
                    }
                }
            } else if (preWrite.output !== null) {
                selected.push(preWrite.output);
            } else {
                return "native-unavailable";
            }
            if (selected.length === 0) {
                return "native-unavailable";
            }
            for (const output of selected) {
                if (!writeDesktopForOutput(surface, targetRef, output)) {
                    return "switch-unconfirmed";
                }
                const read = readDesktopForOutput(surface, output);
                if (read === null || read.id !== targetId) {
                    return "switch-unconfirmed";
                }
            }
            // Fresh arrival proof immediately before focus: the switch
            // setters run synchronously through KWin signals that may move
            // or close the mover between the pre-switch proof and now. A
            // stale membership refuses focus without a setter; the verified
            // switch above still stands.
            if (!moverOnDesktop(mover, targetRef)) {
                return "arrival-unconfirmed";
            }
            const moverId = readNativeId(mover);
            if (moverId === null) {
                return "focus-unconfirmed";
            }
            try {
                (liveWorkspace as { activeWindow: unknown }).activeWindow = mover;
            } catch (error) {
                void error;
                return "focus-unconfirmed";
            }
            const active = Reflect.get(surface, "activeWindow");
            const activeId = typeof active === "object" && active !== null ? readNativeId(active as object) : null;
            return activeId === moverId ? "followed" : "focus-unconfirmed";
        } catch (error) {
            void error;
            return "native-unavailable";
        }
    };
    const requestWorkspaceRelativeMove = (delta: unknown, followRaw?: unknown): void => {
        try {
            const follow = parseFollowFlag(followRaw);
            if (delta !== -1 && delta !== 1) {
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-move outcome=invalid-logical-target follow=not-reached gate=pre-commit phase=entry reason=invalid-logical-target req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            if (refuseSendWhileBootstrapPending("workspace-move", "workspace-move", -1)) {
                return;
            }
            if (!workspaceSend.isEnabled || workspaceSend.isInFlight || adapter.isInFlight) {
                const outcome = !workspaceSend.isEnabled
                    ? "disabled"
                    : workspaceSend.isInFlight
                      ? "busy-send"
                      : "busy-plan";
                try {
                    let correlation = "";
                    let inflightStage = "idle";
                    try {
                        correlation = workspaceSend.activeCorrelation;
                        inflightStage = workspaceSend.activeStage;
                    } catch (error) {
                        void error;
                    }
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation=${correlation} generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-move outcome=${outcome} follow=not-reached gate=pre-commit phase=entry reason=${outcome} req_ord=-1 inflight_stage=${inflightStage}`);
                    log("omnitiler:plan:busy-refused kind=workspace-move");
                } catch (error) {
                    void error;
                }
                return;
            }
            // Resolve the relative target once before transfer over the item
            // 1 scoped ring; the transfer below never re-resolves.
            const target = workspaceNative.resolveRelativeMoveTarget(delta);
            if (target === null) {
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-move outcome=target-unresolved follow=not-reached gate=pre-commit phase=entry reason=target-unresolved req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            transferResolvedWorkspace(target, -1, follow);
        } catch (error) {
            void error;
        }
    };
    // Shared absolute/relative transfer for one resolved target id: the
    // second busy gate, the floating-boundary native path with explicit
    // follow/stay, else the Rust-planned tiled send with the same selection.
    const transferResolvedWorkspace = (target: string, reqOrd: number, follow: boolean): void => {
        try {
            // Shared send fence: every workspace route refuses here while
            // the intent bootstrap is outstanding (see the helper).
            if (refuseSendWhileBootstrapPending("workspace-move", "workspace-move", reqOrd)) {
                return;
            }
            if (!workspaceSend.isEnabled || workspaceSend.isInFlight || adapter.isInFlight) {
                const outcome = !workspaceSend.isEnabled
                    ? "disabled"
                    : workspaceSend.isInFlight
                      ? "busy-send"
                      : "busy-plan";
                try {
                    let correlation = "";
                    let inflightStage = "idle";
                    try {
                        correlation = workspaceSend.activeCorrelation;
                        inflightStage = workspaceSend.activeStage;
                    } catch (error) {
                        void error;
                    }
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation=${correlation} generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-move outcome=${outcome} follow=not-reached gate=pre-commit phase=entry reason=${outcome} req_ord=${String(reqOrd)} inflight_stage=${inflightStage}`);
                    log("omnitiler:plan:busy-refused kind=workspace-move");
                } catch (error) {
                    void error;
                }
                return;
            }
            // Diagnostic-only handoff: the validated logical ordinal (0
            // permitted for the trailing target, -1 for a relative send)
            // travels into the send flight for follow logs and never gates
            // request behavior.
            // Floating-boundary sends stay native-only: when either side is
            // an unmanaged workspace, move membership natively with no Rust
            // two-domain geometry. Stay preserves native focus/visibility
            // (no desktop switch, no focus write); follow (the default)
            // switches the active output to the target and focuses the mover
            // with an immediate readback and no retry. Either way only the
            // tiled side reflows through the ordinary complete-observation
            // resync, preserving floating geometry.
            try {
                const probed = observeSendTarget(liveWorkspace, sendNativeIds, target, floatingIds, undefined, nativeOwners);
                const sourceId = probed === null ? workspaceNative.currentScopeId() : probed.sourceWorkspace;
                const sourceTiled = sourceId === null ? true : workspaceNative.isTiled(sourceId);
                const targetTiled = workspaceNative.isTiled(target);
                if (!sourceTiled || !targetTiled) {
                    const mover = readActiveMover();
                    const targetRef = resolveDesktopRef(target);
                    if (mover === null || targetRef === null) {
                        try {
                            log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-move outcome=native-unavailable follow=not-reached gate=pre-commit phase=entry reason=native-unavailable req_ord=${String(reqOrd)} inflight_stage=idle`);
                        } catch (error) {
                            void error;
                        }
                        return;
                    }
                    // Native membership preservation: a sticky (onAllDesktops)
                    // or multi-home mover cannot be expressed as a
                    // single-desktop write, so refuse instead of silently
                    // stripping native semantics. Only positive evidence
                    // refuses: unreadable membership still attempts the write
                    // so broken movers report native-failed truthfully below.
                    // Ordinary single-home moves proceed unchanged.
                    let refusal: string | null = null;
                    try {
                        if (readProp(mover, "onAllDesktops") === true) {
                            refusal = "sticky";
                        } else {
                            const members = decodeList(readProp(mover, "desktops"), MAX_DESKTOPS);
                            if (members !== null && members.length > 1) {
                                refusal = "multi-home";
                            }
                        }
                    } catch (error) {
                        void error;
                    }
                    if (refusal !== null) {
                        try {
                            log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-move outcome=native-refused follow=not-reached gate=floating-boundary phase=entry reason=${refusal} req_ord=${String(reqOrd)} inflight_stage=idle`);
                        } catch (error) {
                            void error;
                        }
                        return;
                    }
                    // Snapshot the follow output before the membership write:
                    // KWin may refocus a survivor on another output (or null)
                    // as a result of the write; follow must address the
                    // pre-write output.
                    const preWriteSelection = selectFollowOutput();
                    const moved = writeMoverDesktops(mover, targetRef);
                    // Explicit follow/stay on the floating-boundary path:
                    // stay keeps the preservation above (no desktop switch,
                    // no focus write); follow re-proves arrival, switches to
                    // the target and focuses the mover with an immediate
                    // readback, no retry.
                    // The commit stands either way; a failed follow only
                    // degrades the follow token below.
                    let followToken = "not-reached";
                    if (!moved) {
                        followToken = "not-reached";
                    } else if (!follow) {
                        followToken = "stayed";
                    } else {
                        followToken = followNativeToTarget(targetRef, mover, preWriteSelection);
                    }
                    // The tiled side reflows through the ordinary
                    // complete-observation resync below, preserving floating
                    // geometry; the floating side receives no geometry.
                    try {
                        workspaceNative.handleTopologySignal();
                    } catch (error) {
                        void error;
                    }
                    try {
                        adapter.requestResync();
                    } catch (error) {
                        void error;
                    }
                    emitWorkspaceTiling();
                    // Report the actual native write outcome: moved means the
                    // membership write applied, failed means it did not. The
                    // follow token above reports stay/follow truthfully: stay
                    // never switches or focuses, follow reports its readback.
                    const nativeOutcome = moved ? "native-moved" : "native-failed";
                    try {
                        log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-move outcome=${nativeOutcome} follow=${followToken} gate=floating-boundary phase=entry reason=floating-boundary req_ord=${String(reqOrd)} inflight_stage=idle`);
                    } catch (error) {
                        void error;
                    }
                    if (!moved) {
                        try {
                            log("omnitiler:plan:workspace-send-native-failed");
                        } catch (error) {
                            void error;
                        }
                    }
                    return;
                }
            } catch (error) {
                void error;
            }
            workspaceSend.requestSend(target, reqOrd, follow);
        } catch (error) {
            void error;
        }
    };
    // Explicit output send for one commanded direction (REQ-OUT-04, item
    // 5.3/5.4): resolve the adjacent output via FULL-rectangle selection,
    // then either move membership-only across a floating-workspace boundary
    // or dispatch the Rust-planned tiled output send with the same explicit
    // follow/stay selection as workspace send. No candidate is a silent
    // no-op; ambiguous/unreadable topology refuses before any write.
    const requestOutputSend = (directionRaw: unknown, followRaw?: unknown): void => {
        try {
            const follow = parseFollowFlag(followRaw);
            const direction = directionRaw as string;
            if (direction !== "left" && direction !== "right" && direction !== "up" && direction !== "down") {
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=invalid-direction follow=not-reached gate=pre-commit phase=entry reason=invalid-direction req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            // Q3 bootstrap fence: same deferred shape as workspace send (see
            // the shared helper); no output transfer may observe a
            // hydration candidate as tiled mid-bootstrap.
            if (refuseSendWhileBootstrapPending("output-send", "output-send", -1)) {
                return;
            }
            if (!workspaceSend.isEnabled || workspaceSend.isInFlight || adapter.isInFlight) {
                const outcome = !workspaceSend.isEnabled
                    ? "disabled"
                    : workspaceSend.isInFlight
                      ? "busy-send"
                      : "busy-plan";
                try {
                    let correlation = "";
                    let inflightStage = "idle";
                    try {
                        correlation = workspaceSend.activeCorrelation;
                        inflightStage = workspaceSend.activeStage;
                    } catch (error) {
                        void error;
                    }
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation=${correlation} generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=${outcome} follow=not-reached gate=pre-commit phase=entry reason=${outcome} req_ord=-1 inflight_stage=${inflightStage}`);
                    log("omnitiler:plan:busy-refused kind=output-send");
                } catch (error) {
                    void error;
                }
                return;
            }
            // Resolve the adjacent output once via FULL-rectangle selection
            // from the active output. Panel gaps cannot break edge-touch.
            const topology = readOutputTopology(liveWorkspace);
            if (topology === null || topology.length === 0) {
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=scope-invalid follow=not-reached gate=pre-commit phase=entry reason=topology-unreadable req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            const activeMoverForSource = readActiveMover();
            let sourceName: string | null = null;
            if (activeMoverForSource !== null) {
                const moverOutput = readProp(activeMoverForSource, "output");
                if (typeof moverOutput === "object" && moverOutput !== null) {
                    const moverNameRaw = readProp(moverOutput as object, "name");
                    if (isOpaqueId(moverNameRaw)) {
                        sourceName = moverNameRaw as string;
                    }
                }
            }
            if (sourceName === null) {
                const first = topology[0] as OutputTopologyEntry | undefined;
                if (first === undefined) {
                    return;
                }
                sourceName = first.name;
            }
            const sourceEntry = topology.find((entry) => entry.name === sourceName);
            if (sourceEntry === undefined) {
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=scope-invalid follow=not-reached gate=pre-commit phase=entry reason=source-unknown req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            // Explicit sends rank like exhausted moves: shared edge containing
            // the moving window centre projection, then window-span overlap,
            // then left/top. The mover frame must read; unreadable refuses.
            let moverRect: { x: number; y: number; w: number; h: number } | null = null;
            if (activeMoverForSource !== null) {
                const frame = readFrameRect(activeMoverForSource);
                if (typeof frame !== "string") {
                    moverRect = { x: frame.x, y: frame.y, w: frame.w, h: frame.h };
                }
            }
            const selected = selectWindowedAdjacentOutput(topology, sourceEntry, direction, moverRect);
            if (selected === null) {
                // No candidate is a no-op: log the quiet no-target outcome
                // with no write and no flight.
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=no-target follow=not-reached gate=pre-commit phase=entry reason=no-adjacent-output req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            if (selected === "ambiguous") {
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=refused follow=not-reached gate=pre-commit phase=entry reason=ambiguous-topology req_ord=-1 inflight_stage=idle`);
                    log("omnitiler:plan:output-send-refused-ambiguous");
                } catch (error) {
                    void error;
                }
                return;
            }
            const targetOutput = selected.name;
            const targetWorkspace = selected.workspace;
            // Probe the exact cross-output scope for the floating-boundary
            // decision and the tiled dispatch. A drifted destination or an
            // unreadable scope refuses before any write.
            const probed = observeOutputSendTarget(
                liveWorkspace,
                sendNativeIds,
                targetOutput,
                floatingIds,
                undefined,
                undefined,
                nativeOwners,
            );
            if (probed === null || probed.targetWorkspace !== targetWorkspace) {
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=scope-invalid follow=not-reached gate=pre-commit phase=entry reason=scope-unreadable req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            const sourceTiled = isEntryDomainTiled(probed.sourceOutput, probed.sourceWorkspace);
            const targetTiled = isEntryDomainTiled(probed.targetOutput, probed.targetWorkspace);
            if (!sourceTiled || !targetTiled) {
                // Floating-workspace boundaries stay native-only: transfer
                // output plus desktop membership with no Rust geometry. The
                // tiled side reflows through the ordinary resync; the float
                // frame is unchanged. The mover is the probe-gated tiled
                // window only: an empty probe mover (sticky, intentional
                // float, fullscreen, maximized, or absent focus) refuses
                // here, never falling back to the raw active window. The
                // live active ref must still be exactly the probe mover ref
                // before any output setter. Follow re-proves the live
                // destination workspace on the target output plus arrival
                // before focus; stay re-proves source visibility before it
                // may claim stayed.
                const targetRef = probed.targetDesktopRef;
                let targetOutputRef: object | null = null;
                let sourceOutputRef: object | null = null;
                for (const entry of topology) {
                    if (entry.name === targetOutput) {
                        targetOutputRef = entry.ref;
                    }
                    if (entry.name === probed.sourceOutput) {
                        sourceOutputRef = entry.ref;
                    }
                }
                const liveActive = readActiveMover();
                if (
                    probed.focusedId === "" ||
                    probed.moverRef === null ||
                    liveActive === null ||
                    liveActive !== probed.moverRef ||
                    targetRef === null ||
                    targetOutputRef === null ||
                    sourceOutputRef === null
                ) {
                    try {
                        log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=native-refused follow=not-reached gate=floating-boundary phase=entry reason=non-tiled-focus req_ord=-1 inflight_stage=idle`);
                    } catch (error) {
                        void error;
                    }
                    return;
                }
                const mover = probed.moverRef;
                let refusal: string | null = null;
                try {
                    if (readProp(mover, "onAllDesktops") === true) {
                        refusal = "sticky";
                    } else {
                        const members = decodeList(readProp(mover, "desktops"), MAX_DESKTOPS);
                        if (members !== null && members.length > 1) {
                            refusal = "multi-home";
                        }
                    }
                } catch (error) {
                    void error;
                }
                if (refusal !== null) {
                    try {
                        log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=native-refused follow=not-reached gate=floating-boundary phase=entry reason=${refusal} req_ord=-1 inflight_stage=idle`);
                    } catch (error) {
                        void error;
                    }
                    return;
                }
                // Exact-lifetime recheck immediately before the first output
                // setter: the active window must still be the probe mover.
                if (readActiveMover() !== mover) {
                    try {
                        log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=native-refused follow=not-reached gate=floating-boundary phase=entry reason=active-changed req_ord=-1 inflight_stage=idle`);
                    } catch (error) {
                        void error;
                    }
                    return;
                }
                let transferred = false;
                try {
                    const sender = readProp(surface, "sendClientToScreen");
                    if (typeof sender === "function") {
                        Reflect.apply(sender as (...args: ReadonlyArray<never>) => unknown, surface, [mover, targetOutputRef]);
                        transferred = true;
                    }
                } catch (error) {
                    void error;
                    transferred = false;
                }
                const moved = transferred && writeMoverDesktops(mover, targetRef);
                // Source-visibility read for the stay claim below: the live
                // current workspace on the recording output (shared current
                // on legacy-shared hosts). Unreadable never counts as stayed.
                let sourceSelected = false;
                try {
                    if (sourceOutputRef !== null) {
                        const read = readDesktopForOutput(surface, sourceOutputRef);
                        sourceSelected = read !== null && read.id === probed.sourceWorkspace;
                    }
                } catch (error) {
                    void error;
                    sourceSelected = false;
                }
                let followToken = "not-reached";
                if (!moved) {
                    followToken = "not-reached";
                } else if (!follow) {
                    // A synchronous external view switch revokes visibility:
                    // never claim stayed while viewing elsewhere.
                    followToken = sourceSelected ? "stayed" : "arrival-unconfirmed";
                } else {
                    followToken = focusOutputMover(mover, targetRef, targetOutput, targetOutputRef, targetWorkspace);
                }
                try {
                    workspaceNative.handleTopologySignal();
                } catch (error) {
                    void error;
                }
                try {
                    adapter.requestResync();
                } catch (error) {
                    void error;
                }
                emitWorkspaceTiling();
                const nativeOutcome = moved ? "native-moved" : "native-failed";
                try {
                    log(`omnitiler:route-diag component=cosmic-send stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=output-send outcome=${nativeOutcome} follow=${followToken} gate=floating-boundary phase=entry reason=floating-boundary req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                if (!moved) {
                    try {
                        log("omnitiler:plan:workspace-send-native-failed");
                    } catch (error) {
                        void error;
                    }
                }
                return;
            }
            workspaceSend.requestSendToOutput(targetOutput, targetWorkspace, follow);
        } catch (error) {
            void error;
        }
    };
    // Whole-workspace output migration for one commanded direction
    // (R-WS-12): the active workspace keeps its backing id and moves to the
    // adjacent output via FULL-rectangle selection, then the Rust-planned
    // retained rekey commits through the shared send-transport flight with
    // the same busy gates as workspace/output sends. Follow-only: the
    // target shows the migrated workspace and the source refills. No
    // candidate is a silent no-op; ambiguous/unreadable topology refuses
    // before any write.
    const requestWorkspaceMigrate = (directionRaw: unknown): void => {
        try {
            const direction = directionRaw as string;
            if (!isMigrateDirection(direction)) {
                try {
                    log(`omnitiler:route-diag component=workspace-migrate stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-migrate outcome=invalid-direction follow=not-reached gate=pre-commit phase=entry reason=invalid-direction req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            if (refuseSendWhileBootstrapPending("workspace-migrate", "workspace-migrate", -1)) {
                return;
            }
            if (!workspaceSend.isEnabled || workspaceSend.isInFlight || adapter.isInFlight) {
                const outcome = !workspaceSend.isEnabled
                    ? "disabled"
                    : workspaceSend.isInFlight
                      ? "busy-send"
                      : "busy-plan";
                try {
                    let correlation = "";
                    let inflightStage = "idle";
                    try {
                        correlation = workspaceSend.activeCorrelation;
                        inflightStage = workspaceSend.activeStage;
                    } catch (error) {
                        void error;
                    }
                    log(`omnitiler:route-diag component=workspace-migrate stage=entry correlation=${correlation} generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-migrate outcome=${outcome} follow=not-reached gate=pre-commit phase=entry reason=${outcome} req_ord=-1 inflight_stage=${inflightStage}`);
                    log("omnitiler:plan:busy-refused kind=workspace-migrate");
                } catch (error) {
                    void error;
                }
                return;
            }
            // Dispatch probe: no adjacent candidate is a quiet no-target
            // no-op with no write and no flight; ambiguous/unreadable scope
            // refuses here before dispatch. The adapter re-observes
            // authoritatively at dispatch and across the flight.
            //
            // Workspace policy gates before the topology probe: mode and
            // the strict native flag refuse with their exact reasons even
            // when the topology itself would also refuse (a shared setup
            // showing one desktop everywhere logs mode-shared, never a
            // topology artifact). No lifecycle runs on this command path:
            // the ordinary minimum-two/trailing maintenance converges
            // independently on topology signals, and no refused command
            // may create, remove, or switch a desktop.
            const policy = {
                mode: workspaceNative.getMode(),
                perOutput: readPerOutputVirtualDesktopsFlag(
                    overrides.options !== undefined ? overrides.options : resolveLexicalOptions(),
                ),
            };
            // Policy before probe: mode and the strict native flag refuse
            // with exact reasons before any topology read, so a shared
            // same-id setup reports mode-shared rather than a topology
            // artifact. The adapter enforces the same gates
            // authoritatively at dispatch.
            if (policy.mode !== "per-output-local" && policy.mode !== "global-unique") {
                const outcome = policy.mode === "shared" ? "mode-shared" : "mode-invalid";
                try {
                    log(`omnitiler:route-diag component=workspace-migrate stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-migrate outcome=${outcome} follow=not-reached gate=pre-commit phase=entry reason=${outcome} req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            // Legacy-shared hosts (KWin 6.6) expose no per-output API: refuse
            // as unsupported without consulting the absent options property.
            // A missing property on 6.7+ below stays a genuine unreadable.
            if (!supportsPerOutputDesktops(liveWorkspace)) {
                const outcome = "per-output-unsupported";
                try {
                    log(`omnitiler:route-diag component=workspace-migrate stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-migrate outcome=${outcome} follow=not-reached gate=pre-commit phase=entry reason=${outcome} req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            if (policy.perOutput !== true) {
                const outcome = policy.perOutput === false ? "per-output-disabled" : "per-output-unreadable";
                try {
                    log(`omnitiler:route-diag component=workspace-migrate stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-migrate outcome=${outcome} follow=not-reached gate=pre-commit phase=entry reason=${outcome} req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            const probed = observeMigrateWorkspace(
                liveWorkspace,
                sendNativeIds,
                floatingIds,
                domainGaps,
                direction,
                policy,
                nativeOwners,
                undefined,
            );
            if (probed.status === "no-target") {
                try {
                    log(`omnitiler:route-diag component=workspace-migrate stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-migrate outcome=no-target follow=not-reached gate=pre-commit phase=entry reason=no-adjacent-output req_ord=-1 inflight_stage=idle`);
                } catch (error) {
                    void error;
                }
                return;
            }
            if (probed.status !== "ready" || probed.observed === null) {
                try {
                    log(`omnitiler:route-diag component=workspace-migrate stage=entry correlation= generation=${String(overrides.generation)} revision=0 diag_seq=-1 event=workspace-migrate outcome=refused follow=not-reached gate=pre-commit phase=entry reason=scope-unreadable req_ord=-1 inflight_stage=idle`);
                    log("omnitiler:plan:workspace-migrate-refused-scope");
                } catch (error) {
                    void error;
                }
                return;
            }
            workspaceSend.requestMigrateWorkspace(direction);
        } catch (error) {
            void error;
        }
    };
    // Native-only focus for floating-boundary output sends: reprove the live
    // destination workspace on the target output, prove the mover sits on
    // the destination output in the destination desktop membership, then
    // focus it with an immediate readback and no retry. No desktop switch
    // ever runs: the destination is already visible on its output.
    const focusOutputMover = (
        mover: object,
        targetRef: object,
        targetOutput: string,
        targetOutputRef: object,
        targetWorkspace: string,
    ): string => {
        try {
            try {
                // Capability seam: per-output reproof on 6.7+, shared current
                // on legacy-shared hosts. Anything unreadable refuses.
                const read = readDesktopForOutput(surface, targetOutputRef);
                if (read === null || read.id !== targetWorkspace) {
                    return "arrival-unconfirmed";
                }
            } catch (error) {
                void error;
                return "arrival-unconfirmed";
            }
            if (!moverOnDesktop(mover, targetRef)) {
                return "arrival-unconfirmed";
            }
            const output = readProp(mover, "output");
            if (typeof output !== "object" || output === null || readProp(output as object, "name") !== targetOutput) {
                return "arrival-unconfirmed";
            }
            const moverId = readNativeId(mover);
            if (moverId === null) {
                return "focus-unconfirmed";
            }
            try {
                (liveWorkspace as { activeWindow: unknown }).activeWindow = mover;
            } catch (error) {
                void error;
                return "focus-unconfirmed";
            }
            const active = Reflect.get(surface, "activeWindow");
            const activeId = typeof active === "object" && active !== null ? readNativeId(active as object) : null;
            return activeId === moverId ? "followed" : "focus-unconfirmed";
        } catch (error) {
            void error;
            return "native-unavailable";
        }
    };
    if (registerFn !== undefined) {
        for (const row of workspaceShortcutCatalog()) {
            const action = row.action;
            const text = row.text;
            const sequence = row.sequence;
            const kind = row.kind;
            const index = row.index;
            const delta = row.delta;
            try {
                const ok =
                    kind === "move"
                        ? registerFn(action, text, sequence, () => requestWorkspaceMove(index, true))
                        : kind === "move-stay"
                          ? registerFn(action, text, sequence, () => requestWorkspaceMove(index, false))
                          : kind === "previous"
                            ? registerFn(action, text, sequence, () => requestWorkspacePrevious())
                            : kind === "relative"
                              ? registerFn(action, text, sequence, () => requestWorkspaceRelative(delta))
                              : kind === "send-relative"
                                ? registerFn(action, text, sequence, () => requestWorkspaceRelativeMove(delta, true))
                                : kind === "send-relative-stay"
                                  ? registerFn(action, text, sequence, () => requestWorkspaceRelativeMove(delta, false))
                                  : registerFn(action, text, sequence, () => requestWorkspaceSelect(index));
                if (ok !== true) {
                    try {
                        log(`omnitiler:plan:shortcut-failed action=${action} sequence=${sequence}`);
                    } catch (error) {
                        void error;
                    }
                }
            } catch (error) {
                void error;
                try {
                    log(`omnitiler:plan:shortcut-failed action=${action} sequence=${sequence}`);
                } catch (inner) {
                    void inner;
                }
            }
        }
        // Project-owned toggle for the tray menu (invoked over KGlobalAccel
        // by action name, never by a physical key) plus the Meta+Y default
        // binding. The menu waits for the next published snapshot rather
        // than assuming this dispatch applied.
        try {
            const ok = registerFn(WORKSPACE_TILING_TOGGLE_ACTION, WORKSPACE_TILING_TOGGLE_TEXT, WORKSPACE_TILING_TOGGLE_SEQUENCE, () => toggleWorkspaceTiling());
            if (ok !== true) {
                try {
                    log(`omnitiler:plan:shortcut-failed action=${WORKSPACE_TILING_TOGGLE_ACTION} sequence=${WORKSPACE_TILING_TOGGLE_SEQUENCE}`);
                } catch (error) {
                    void error;
                }
            }
        } catch (error) {
            void error;
            try {
                log(`omnitiler:plan:shortcut-failed action=${WORKSPACE_TILING_TOGGLE_ACTION} sequence=${WORKSPACE_TILING_TOGGLE_SEQUENCE}`);
            } catch (inner) {
                void inner;
            }
        }
    }
    const workspaceDetaches: Array<() => void> = [];
    const workspaceWindowDetaches = new Map<object, () => void>();
    const trackWorkspaceDetach = (detach: (() => void) | null): void => {
        if (detach !== null) {
            workspaceDetaches.push(detach);
        }
    };
    const trackWorkspaceWindowDetach = (ref: object, detach: (() => void) | null): void => {
        if (detach === null || workspaceWindowDetaches.has(ref)) {
            return;
        }
        workspaceWindowDetaches.set(ref, detach);
        workspaceDetaches.push(detach);
    };
    const dropWorkspaceWindowDetach = (ref: object): void => {
        const detach = workspaceWindowDetaches.get(ref);
        if (detach === undefined) {
            return;
        }
        workspaceWindowDetaches.delete(ref);
        const at = workspaceDetaches.indexOf(detach);
        if (at >= 0) {
            workspaceDetaches.splice(at, 1);
        }
        try {
            detach();
        } catch (error) {
            void error;
        }
    };
    const readTopologyIdsForPrune = (
        property: string,
        idKey: string,
        maxLength: number,
    ): ReadonlyArray<string> | null => {
        try {
            const raw = readProp(surface, property);
            const list = decodeList(raw, maxLength);
            if (list === null || list.length === 0) {
                return null;
            }
            const out: string[] = [];
            const seen = new Set<string>();
            for (const item of list) {
                if (typeof item !== "object" || item === null) {
                    return null;
                }
                const idRaw = readProp(item as object, idKey);
                if (!isOpaqueId(idRaw)) {
                    return null;
                }
                const id = idRaw as string;
                if (seen.has(id)) {
                    return null;
                }
                seen.add(id);
                out.push(id);
            }
            return out.length === 0 ? null : out;
        } catch (error) {
            void error;
            return null;
        }
    };
    const pruneDragRestoreForTopologySignal = (): void => {
        try {
            const workspaceIds = readTopologyIdsForPrune("desktops", "id", MAX_DESKTOPS);
            const outputNames = readTopologyIdsForPrune("screens", "name", MAX_LIST);
            try {
                adapter.pruneDragRestoreForTopology(workspaceIds, outputNames);
            } catch (error) {
                void error;
            }
        } catch (error) {
            void error;
        }
    };
    trackWorkspaceDetach(
        sub("desktopsChanged", () => {
            pruneDragRestoreForTopologySignal();
            workspaceNative.handleTopologySignal();
            try {
                retryPendingReleases();
            } catch (error) {
                void error;
            }
            emitWorkspaceTiling();
        }),
    );
    trackWorkspaceDetach(
        sub("currentDesktopChanged", () => {
            workspaceNative.handleTopologySignal();
            try {
                retryPendingReleases();
            } catch (error) {
                void error;
            }
            emitWorkspaceTiling();
        }),
    );
    trackWorkspaceDetach(
        sub("screensChanged", () => {
            pruneDragRestoreForTopologySignal();
            workspaceNative.handleTopologySignal();
            try {
                retryPendingReleases();
            } catch (error) {
                void error;
            }
            emitWorkspaceTiling();
        }),
    );
    trackWorkspaceDetach(
        sub("windowAdded", () => {
            workspaceNative.handleTopologySignal();
            try {
                retryPendingReleases();
            } catch (error) {
                void error;
            }
            emitWorkspaceTiling();
        }),
    );
    trackWorkspaceDetach(
        sub("windowRemoved", () => {
            workspaceNative.handleTopologySignal();
            try {
                retryPendingReleases();
            } catch (error) {
                void error;
            }
            emitWorkspaceTiling();
        }),
    );
    try {
        const lister = surface["windowList"];
        if (typeof lister === "function") {
            let raw: unknown = undefined;
            try {
                raw = Reflect.apply(lister as (...args: ReadonlyArray<never>) => unknown, surface, []);
            } catch (error) {
                void error;
                raw = undefined;
            }
            const list = decodeList(raw, MAX_LIST);
            if (list !== null) {
                const seenWindows = new Set<object>();
                for (const item of list) {
                    if (typeof item !== "object" || item === null || seenWindows.has(item as object)) {
                        continue;
                    }
                    seenWindows.add(item as object);
                    trackWorkspaceWindowDetach(
                        item as object,
                        connectSignal(readSignal(item as object, "desktopsChanged"), () =>
                            workspaceNative.handleTopologySignal(),
                        ),
                    );
                }
            }
            trackWorkspaceDetach(
                connectSignal(readSignal(surface, "windowAdded"), (added) => {
                    if (typeof added === "object" && added !== null) {
                        trackWorkspaceWindowDetach(
                            added as object,
                            connectSignal(readSignal(added as object, "desktopsChanged"), () =>
                                workspaceNative.handleTopologySignal(),
                            ),
                        );
                    }
                    workspaceNative.handleTopologySignal();
                }),
            );
            trackWorkspaceDetach(
                connectSignal(readSignal(surface, "windowRemoved"), (removed) => {
                    if (typeof removed === "object" && removed !== null) {
                        dropWorkspaceWindowDetach(removed as object);
                    }
                    workspaceNative.handleTopologySignal();
                }),
            );
        }
    } catch (error) {
        void error;
    }
    // Oracle route: preserve start rect, move/resize classification, start
    // pointer, and grabbed edge(s) at Started, then on a non-cancelled
    // LastVerdict route the grabbed final edge target(s) from the
    // authoritative final rect (never the finish pointer). Non-grabbed edge
    // deltas are ignored, never a rejection; a grabbed corner routes both
    // axes in exactly one atomic dual-axis pointer-resize intent through
    // the shared single-flight (never two concurrent requests); the
    // single-axis wire shape is unchanged. Cancelled is a strict no-op
    // with one bounded normal-mode log at the pull layer; unusable-edge
    // failures (missing grab, zero move) fail closed with exact bounded
    // correlated reasons.
    // Every finish carries an opaque per-finish token (exact Window object
    // plus finish epoch) from its signal through the D-Bus pull back to the
    // route/settle callbacks. Route/settle consume only a start whose epoch
    // is <= that finish token; an old reply faced with a newer start fails
    // closed and never clears the newer start. Cancelled verdicts never reach
    // the pointer route and never change a share.
    interface OracleStart { id: string; rect: { x: number; y: number; w: number; h: number }; move: boolean; resize: boolean; epoch: number; grabbed: OracleGrabbed | null; grabSource: OracleGrabSource | "missing"; pointerStart: { x: number; y: number } | null; floatingStart: boolean; domainOutput: string; domainWorkspace: string }
    const oracleStarts = new Map<object, OracleStart>();
    const interactiveResizeRefs = new Set<object>();
    // Tiled-move suppression mirrors the resize hold: while a tiled member is
    // being moved, ordinary reconcile stays suppressed exactly as during
    // resize. Floating moves never enter this set and stay native-only.
    // The hold lasts from Started until the correlated verdict settles
    // (route or settle) or the bounded per-finish timer releases it, so a
    // missing/unavailable reply cannot block later work and no successful
    // drag terminal is invented without a validated drag-N correlation.
    const interactiveMoveRefs = new Set<object>();
    const moveGuardCancels = new Map<DragOracleFinishContext, () => void>();
    // Resize-hold release: no Started-keyed expiry exists. The hold lasts
    // from Started until Finished/removal, or until the observed-state gate
    // above sees KWin report resize!==true without a Finished signal. A
    // still-resizing window is never retiled mid-gesture.
    // Tiled-move hold release: no Started-keyed expiry exists. The hold
    // lasts from Started until Finished/route/settle/removal, or until the
    // observed-state gate above sees KWin report move!==true without a
    // Finished signal. A still-moving window is never retiled mid-drag.
    // One move-drag preview session per tiled move Started: local
    // `drag-<epoch>` correlation, one outstanding preview plus latest-pointer
    // coalescing. Finish fences late replies but keeps the prior for the
    // verdict transfer; settle/timeout/removal/stop drop the session. Preview
    // never touches group highlight.
    const DRAG_PREVIEW_SET_METHOD = "SetDragTargetPreview";
    const DRAG_PREVIEW_CLEAR_METHOD = "ClearDragTargetPreview";
    interface MovePreviewSession {
        correlation: string;
        epoch: number;
        id: string;
        busy: boolean;
        pending: { x: number; y: number } | null;
        finished: boolean;
        overlay: boolean;
    }
    const movePreviewSessions = new Map<object, MovePreviewSession>();
    const overlayCall = (method: string, ...args: ReadonlyArray<unknown>): boolean => {
        try {
            const call =
                typeof overrides.highlightCallDbus === "function"
                    ? (overrides.highlightCallDbus as (...callArgs: ReadonlyArray<unknown>) => void)
                    : typeof callDBus === "function"
                      ? (callDBus as (...callArgs: ReadonlyArray<unknown>) => void)
                      : null;
            if (call === null) {
                return false;
            }
            (call as (service: string, path: string, iface: string, method: string, ...rest: ReadonlyArray<unknown>) => void)(
                GROUP_HIGHLIGHT_SERVICE,
                GROUP_HIGHLIGHT_OBJECT,
                GROUP_HIGHLIGHT_INTERFACE,
                method,
                ...args,
            );
            return true;
        } catch (error) {
            void error;
            return false;
        }
    };
    const clearMovePreviewFull = (ref: object): void => {
        try {
            const session = movePreviewSessions.get(ref);
            if (session !== undefined) {
                if (session.overlay) {
                    log(`omnitiler:route-diag:drag-preview-cleared correlation=${session.correlation} reason=terminal`);
                }
                movePreviewSessions.delete(ref);
                adapter.clearDragPreview(session.correlation);
            }
            overlayCall(DRAG_PREVIEW_CLEAR_METHOD);
        } catch (error) {
            void error;
        }
    };
    const settleMovePreview = (ctx: DragOracleFinishContext): void => {
        const session = movePreviewSessions.get(ctx.ref);
        if (session === undefined || session.epoch > ctx.finishEpoch) {
            return;
        }
        clearMovePreviewFull(ctx.ref);
    };
    const finishMovePreview = (ref: object): void => {
        try {
            const session = movePreviewSessions.get(ref);
            if (session === undefined) {
                return;
            }
            if (session.overlay) {
                log(`omnitiler:route-diag:drag-preview-cleared correlation=${session.correlation} reason=finish`);
            }
            session.finished = true;
            session.pending = null;
            session.overlay = false;
            adapter.invalidateDragPreviewReply(session.correlation);
            overlayCall(DRAG_PREVIEW_CLEAR_METHOD);
        } catch (error) {
            void error;
        }
    };
    const validMovePreview = (ref: object, session: MovePreviewSession): OracleStart | null => {
        if (movePreviewSessions.get(ref) !== session || session.finished) {
            return null;
        }
        const start = oracleStarts.get(ref);
        if (start === undefined || start.epoch !== session.epoch || start.id !== session.id) {
            return null;
        }
        if (!interactiveMoveRefs.has(ref) || start.move !== true || start.floatingStart) {
            return null;
        }
        return start;
    };
    let oracleEpoch = 0;
    const readLiveState = (target: object): { move: boolean; resize: boolean } | null => {
        try {
            const move = Reflect.get(target, "move");
            const resize = Reflect.get(target, "resize");
            if ((move !== true && move !== false) || (resize !== true && resize !== false)) return null;
            return { move, resize };
        } catch (error) {
            void error;
            return null;
        }
    };
    // Observed-state hold guard: a tiled move hold must never release while
    // KWin still reports move===true, even for async finish/verdict
    // anomalies. Unreadable state cannot prove that the move ended; removal
    // releases the hold separately.
    const isLiveMoveStillActive = (target: object): boolean => {
        try {
            return readLiveState(target)?.move !== false;
        } catch (error) {
            void error;
            return true;
        }
    };
    // Observed-state hold guard: a resize hold must never release while
    // KWin still reports resize===true. Unreadable state cannot prove that
    // the resize ended; removal releases the hold separately.
    const isLiveResizeStillActive = (target: object): boolean => {
        try {
            return readLiveState(target)?.resize !== false;
        } catch (error) {
            void error;
            return true;
        }
    };
    const captureOracleStart = (ref: object): void => {
        try {
            const observed = observeNative(liveWorkspace, nativeIds, floatingIds, domainGaps, reportEligibility, nativeOwners, lastHoming);
            if (observed === null) {
                clearMovePreviewFull(ref);
                return;
            }
            for (const entry of observed.windows) {
                if (entry.ref === ref) {
                    const state = readLiveState(ref);
                    if (state === null) {
                        oracleStarts.delete(ref);
                        clearMovePreviewFull(ref);
                        return;
                    }
                    // Route-owned start pointer capture: always read, never
                    // trace-gated. Trace-only measurement keeps its own
                    // separate capture. Fail-closed null, never throws.
                    let pointerStart: { x: number; y: number } | null = null;
                    try {
                        pointerStart = readMeasurePointer(liveWorkspace);
                    } catch (error) {
                        void error;
                    }
                    const rect = { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h };
                    let grabbed: OracleGrabbed | null = null;
                    let grabSource: OracleGrabSource | "missing" = "missing";
                    try {
                        const identified = identifyGrabbedEdges(rect, pointerStart);
                        if (identified !== null) {
                            grabbed = identified.grabbed;
                            grabSource = identified.source;
                        }
                    } catch (error) {
                        void error;
                    }
                    oracleStarts.set(ref, { id: entry.id, rect, move: state.move, resize: state.resize, epoch: (oracleEpoch += 1), grabbed, grabSource, pointerStart, floatingStart: (entry as { floating?: unknown }).floating === true, domainOutput: observed.domainOutput, domainWorkspace: observed.domainWorkspace });
                    if (state.move === false && state.resize === true) {
                        clearMovePreviewFull(ref);
                        if (!interactiveResizeRefs.has(ref)) {
                            interactiveResizeRefs.add(ref);
                            adapter.setInteractiveResizeActive(true);
                        }
                        // No Started-keyed expiry exists: the hold lasts until
                        // Finished/removal, or until the observed-state gate
                        // sees resize!==true. A still-resizing window is never
                        // retiled mid-gesture.
                    }
                    // Tiled-move hold: any move gesture on a tiled member
                    // suppresses ordinary reconcile exactly as a resize hold.
                    // Floating (including sticky, which observes as floating)
                    // never enters the hold and stays native-only. The start
                    // floating state is recorded so a move that STARTS
                    // floating stays unaffected even if tiled at finish.
                    // No Started-keyed expiry exists: the hold lasts until
                    // Finished/route/settle/removal, or until the observed
                    // state gate sees move!==true. A normal Finish hands the
                    // hold to the per-finish guard below.
                    if (state.move === true) {
                        let floating = false;
                        try {
                            floating = (entry as { floating?: unknown }).floating === true;
                        } catch (error) {
                            void error;
                            floating = false;
                        }
                        if (!floating) {
                            if (!interactiveMoveRefs.has(ref)) {
                                interactiveMoveRefs.add(ref);
                                adapter.setInteractiveResizeActive(true);
                            }
                            const started = oracleStarts.get(ref);
                            if (started !== undefined) {
                                armMovePreview(ref, started);
                            }
                        } else {
                            clearMovePreviewFull(ref);
                        }
                    }
                    return;
                }
            }
            oracleStarts.delete(ref);
            clearMovePreviewFull(ref);
        } catch (error) {
            void error;
            clearMovePreviewFull(ref);
        }
    };
    const makeOracleFinishContext = (ref: object): DragOracleFinishContext => {
        oracleEpoch += 1;
        // Finish-time script pointer capture for the tiled move-drop route:
        // the native verdict carries no pointer, so workspace.cursorPos at
        // FINISH is captured here synchronously before the async oracle pull
        // and carried in the context. The route must never read cursorPos at
        // reply time. Fail-closed null, never throws.
        let pointerFinish: { x: number; y: number } | null = null;
        try {
            pointerFinish = readMeasurePointer(liveWorkspace);
        } catch (error) {
            void error;
        }
        return { ref, finishEpoch: oracleEpoch, pointerFinish };
    };
    // Token-guarded consumption: delete the finish's own captured start only
    // when it predates the finish token. A newer Started (larger epoch) that
    // lands before the async reply is always kept. Never touches other
    // windows and never broad-clears.
    const takeOwnStart = (ctx: DragOracleFinishContext | undefined): OracleStart | null => {
        try {
            if (ctx === undefined) return null;
            const start = oracleStarts.get(ctx.ref);
            if (start === undefined) return null;
            if (start.epoch > ctx.finishEpoch) return null;
            oracleStarts.delete(ctx.ref);
            return start;
        } catch (error) {
            void error;
            return null;
        }
    };
    const onMovePreviewReply = (ref: object, session: MovePreviewSession, result: PlanDragPreviewResult | null): void => {
        try {
            session.busy = false;
            const start = validMovePreview(ref, session);
            if (start === null) {
                return;
            }
            if (result === null) {
                if (session.overlay) {
                    session.overlay = false;
                    log(`omnitiler:route-diag:drag-preview-cleared correlation=${session.correlation} reason=refused`);
                    overlayCall(DRAG_PREVIEW_CLEAR_METHOD);
                }
            } else if (overlayCall(DRAG_PREVIEW_SET_METHOD, result.rect.x, result.rect.y, result.rect.w, result.rect.h)) {
                if (!session.overlay) {
                    log(`omnitiler:route-diag:drag-preview-shown correlation=${session.correlation}`);
                }
                session.overlay = true;
            }
            const next = session.pending;
            if (next !== null && !session.finished) {
                session.pending = null;
                dispatchMovePreview(ref, session, next);
            }
        } catch (error) {
            void error;
        }
    };
    const dispatchMovePreview = (
        ref: object,
        session: MovePreviewSession,
        pointer: { x: number; y: number },
    ): void => {
        try {
            const start = validMovePreview(ref, session);
            if (start === null) {
                return;
            }
            session.busy = true;
            let ok = false;
            try {
                ok = adapter.requestDragPreview(
                    start.id,
                    pointer.x,
                    pointer.y,
                    session.correlation,
                    { output: start.domainOutput, workspace: start.domainWorkspace },
                    (result) => {
                        onMovePreviewReply(ref, session, result);
                    },
                );
            } catch (error) {
                void error;
            }
            if (!ok && session.busy) {
                session.busy = false;
            }
        } catch (error) {
            void error;
            session.busy = false;
        }
    };
    const handleMoveStepped = (ref: object): void => {
        try {
            const session = movePreviewSessions.get(ref);
            if (session === undefined) {
                return;
            }
            const start = validMovePreview(ref, session);
            if (start === null) {
                return;
            }
            const pointer = readMeasurePointer(liveWorkspace);
            if (pointer === null) {
                return;
            }
            if (session.busy) {
                session.pending = { x: pointer.x, y: pointer.y };
                return;
            }
            dispatchMovePreview(ref, session, pointer);
        } catch (error) {
            void error;
        }
    };
    const armMovePreview = (ref: object, start: OracleStart): void => {
        try {
            if (start.move !== true || start.floatingStart) {
                clearMovePreviewFull(ref);
                return;
            }
            const previous = movePreviewSessions.get(ref);
            if (previous !== undefined && previous.epoch !== start.epoch) {
                clearMovePreviewFull(ref);
            }
            movePreviewSessions.set(ref, {
                correlation: `drag-${String(start.epoch)}`,
                epoch: start.epoch,
                id: start.id,
                busy: false,
                pending: null,
                finished: false,
                overlay: false,
            });
        } catch (error) {
            void error;
        }
    };
    // AR8 offline drag measurement (trace-only, read-only): one bounded
    // correlated record per interactive move/resize finish carrying the
    // finish frameGeometry, the first later frameGeometryChanged for the
    // same window or a timeout marker, the route's captured start rect (or
    // missing when the route holds no capture for the finish), the pulled
    // verdict (cancelled/reason/final geometry), the deriveOracleEdge result
    // over those rects, pointer start/finish via workspace.cursorPos, and
    // the existing drag correlation. Opt-in via KWIN_TRACE_ENABLED; when
    // disabled nothing is captured, subscribed, scheduled, or logged. Reads
    // frameGeometry/cursorPos only: never calls DescribePlan, never writes
    // geometry, never touches the oracle route above. The start rect is
    // peeked (never consumed) from the route's own oracleStarts under its
    // epoch guard; pointers are captured separately at Started. Keyed by
    // the oracle finish context (one pending per initiated pull, never by
    // window alone), so overlapping finishes keep separate records and a
    // reply always meets its own finish: the pending is created in the
    // finish context factory before the pull runs, so even a synchronous
    // D-Bus callback settles after creation regardless of finish-listener
    // order. A fired-but-unreadable later geometry completes as missing
    // (never misreported as timeout), and a per-pending verdict timeout
    // bounds finishes whose reply never arrives. A finish followed by
    // window removal still emits one partial record (arrived sides kept,
    // unarrived verdict as missing, unarrived later as missing rather than
    // timeout). When the global oracle pull failed to attach, the entry
    // finished handler arms a standalone pending per finish whose verdict
    // side completes via the verdict timeout. No titles, no window
    // identity strings, no raw native ids.
    interface MeasurePending {
        ref: object;
        seq: number;
        start: DragMeasureRect | null;
        finish: DragMeasureRect | null;
        later: DragMeasureRect | null;
        laterDone: boolean;
        laterTimeout: boolean;
        verdict: DragOracleVerdict | null;
        verdictMissing: boolean;
        verdictDone: boolean;
        pointerStart: DragMeasurePointer | null;
        pointerFinish: DragMeasurePointer | null;
        cancelLaterTimer: (() => void) | null;
        cancelVerdictTimer: (() => void) | null;
        detachLater: (() => void) | null;
    }
    const measureStarts = new Map<object, DragMeasurePointer | null>();
    // Route captures already claimed by a standalone finish. The standalone
    // path cannot consume oracleStarts (route-owned), so each route capture
    // may back at most one standalone record: a second finish without an
    // intervening Started reports start=missing. A new Started re-arms.
    const measureStandaloneClaimed = new Set<object>();
    const measurePending = new Map<DragOracleFinishContext, MeasurePending>();
    let measureSeq = 0;
    // True once the global oracle pull attached. Only then does a finish
    // context (and a later settle) exist; otherwise the entry finished
    // handler owns a standalone measurement below.
    let measurePullAvailable = false;
    const releaseMeasurePending = (ctx: DragOracleFinishContext, pending: MeasurePending): void => {
        try {
            measurePending.delete(ctx);
        } catch (error) {
            void error;
        }
        try {
            pending.detachLater?.();
        } catch (error) {
            void error;
        }
        try {
            pending.cancelLaterTimer?.();
        } catch (error) {
            void error;
        }
        try {
            pending.cancelVerdictTimer?.();
        } catch (error) {
            void error;
        }
    };
    const dropMeasureWindow = (ref: object): void => {
        try {
            for (const [ctx, pending] of [...measurePending]) {
                if (pending.ref !== ref) {
                    continue;
                }
                releaseMeasurePending(ctx, pending);
            }
        } catch (error) {
            void error;
        }
    };
    const tryEmitMeasure = (ctx: DragOracleFinishContext): void => {
        try {
            const pending = measurePending.get(ctx);
            if (pending === undefined || !pending.verdictDone || (!pending.laterDone && !pending.laterTimeout)) {
                return;
            }
            emitMeasureRecord(ctx, pending);
        } catch (error) {
            void error;
        }
    };
    const emitMeasureRecord = (ctx: DragOracleFinishContext, pending: MeasurePending): void => {
        try {
            releaseMeasurePending(ctx, pending);
            const correlation =
                !pending.verdictMissing && pending.verdict !== null ? pending.verdict.correlation : "none";
            try {
                log(
                    formatDragMeasureLine({
                        seq: pending.seq,
                        correlation,
                        start: pending.start,
                        finish: pending.finish,
                        later: pending.later,
                        laterTimeout: pending.laterTimeout,
                        verdict: pending.verdict,
                        verdictMissing: pending.verdictMissing,
                        edge: describeMeasureEdge(pending.start, pending.finish),
                        pointerStart: pending.pointerStart,
                        pointerFinish: pending.pointerFinish,
                    }),
                );
            } catch (error) {
                void error;
            }
        } catch (error) {
            void error;
        }
    };
    // Removal completion: a finish followed by window removal before its
    // later geometry or verdict still yields one bounded partial record.
    // An arrived side is kept; an unarrived verdict completes as missing
    // (no reply can meaningfully arrive for a removed window) and an
    // unarrived later stays missing (never timeout: the timer never won).
    // Reads nothing from the removed window; no ids leave this path.
    const completeMeasureRemoval = (ref: object): void => {
        try {
            for (const [ctx, pending] of [...measurePending]) {
                if (pending.ref !== ref) {
                    continue;
                }
                if (!pending.verdictDone) {
                    pending.verdict = null;
                    pending.verdictMissing = true;
                    pending.verdictDone = true;
                }
                emitMeasureRecord(ctx, pending);
            }
        } catch (error) {
            void error;
        }
    };
    const captureMeasureStart = (ref: object): void => {
        if (!KWIN_TRACE_ENABLED) {
            return;
        }
        try {
            measureStarts.set(ref, readMeasurePointer(liveWorkspace));
            measureStandaloneClaimed.delete(ref);
        } catch (error) {
            void error;
        }
    };
    const takeMeasurePointer = (ref: object): DragMeasurePointer | null => {
        try {
            const pointer = measureStarts.get(ref) ?? null;
            try {
                measureStarts.delete(ref);
            } catch (error) {
                void error;
            }
            return pointer;
        } catch (error) {
            void error;
            return null;
        }
    };
    // The recorded start rect is the oracle route's own capture for this
    // finish (same epoch guard as takeOwnStart), never an independent
    // script read: peek only, the route still owns consumption. Missing
    // exactly when the route has no capture for this finish.
    const peekMeasureStartRect = (ctx: DragOracleFinishContext): DragMeasureRect | null => {
        try {
            const start = oracleStarts.get(ctx.ref);
            if (start === undefined || start.epoch > ctx.finishEpoch) {
                return null;
            }
            return { x: start.rect.x, y: start.rect.y, w: start.rect.w, h: start.rect.h };
        } catch (error) {
            void error;
            return null;
        }
    };
    const createMeasurePending = (ctx: DragOracleFinishContext): void => {
        if (!KWIN_TRACE_ENABLED) {
            return;
        }
        try {
            armMeasurePending(ctx, peekMeasureStartRect(ctx), takeMeasurePointer(ctx.ref));
        } catch (error) {
            void error;
        }
    };
    // Standalone finish for a globally unavailable oracle pull (no pull
    // listeners exist, so no ordering or duplication concern): the latest
    // unclaimed route capture is the start association (each capture backs
    // at most one record; a repeat finish without a new Started reports
    // missing) and the verdict side can only complete via the verdict
    // timeout. Route/pull untouched: with no handle neither ever runs, and
    // oracleStarts itself is never mutated here.
    const createMeasureStandaloneFinish = (ref: object): void => {
        if (!KWIN_TRACE_ENABLED || measurePullAvailable) {
            return;
        }
        try {
            let start: DragMeasureRect | null = null;
            try {
                const captured = oracleStarts.get(ref);
                if (captured !== undefined && !measureStandaloneClaimed.has(ref)) {
                    start = { x: captured.rect.x, y: captured.rect.y, w: captured.rect.w, h: captured.rect.h };
                    measureStandaloneClaimed.add(ref);
                }
            } catch (error) {
                void error;
            }
            armMeasurePending({ ref, finishEpoch: 0 }, start, takeMeasurePointer(ref));
        } catch (error) {
            void error;
        }
    };
    const armMeasurePending = (
        ctx: DragOracleFinishContext,
        start: DragMeasureRect | null,
        pointerStart: DragMeasurePointer | null,
    ): void => {
        try {
            const pending: MeasurePending = {
                ref: ctx.ref,
                seq: (measureSeq += 1),
                start,
                finish: readMeasureRect(ctx.ref),
                later: null,
                laterDone: false,
                laterTimeout: false,
                verdict: null,
                verdictMissing: false,
                verdictDone: false,
                pointerStart,
                pointerFinish: readMeasurePointer(liveWorkspace),
                cancelLaterTimer: null,
                cancelVerdictTimer: null,
                detachLater: null,
            };
            measurePending.set(ctx, pending);
            try {
                const detach = connectSignal(readSignal(ctx.ref, "frameGeometryChanged"), () => {
                    try {
                        const current = measurePending.get(ctx);
                        if (current === undefined || current.laterDone || current.laterTimeout) {
                            return;
                        }
                        // A fired change always completes the later slot, even
                        // when the rect itself is unreadable (missing, never
                        // timeout). Only the timer below reports timeout.
                        current.later = readMeasureRect(ctx.ref);
                        current.laterDone = true;
                        tryEmitMeasure(ctx);
                    } catch (error) {
                        void error;
                    }
                });
                pending.detachLater = detach;
            } catch (error) {
                void error;
            }
            try {
                pending.cancelLaterTimer = scheduleOnce(DRAG_MEASURE_LATER_TIMEOUT_MS, () => {
                    try {
                        const current = measurePending.get(ctx);
                        if (current === undefined || current.laterDone) {
                            return;
                        }
                        current.laterTimeout = true;
                        tryEmitMeasure(ctx);
                    } catch (error) {
                        void error;
                    }
                });
            } catch (error) {
                void error;
            }
            try {
                pending.cancelVerdictTimer = scheduleOnce(DRAG_MEASURE_VERDICT_TIMEOUT_MS, () => {
                    try {
                        const current = measurePending.get(ctx);
                        if (current === undefined || current.verdictDone) {
                            return;
                        }
                        current.verdict = null;
                        current.verdictMissing = true;
                        current.verdictDone = true;
                        tryEmitMeasure(ctx);
                    } catch (error) {
                        void error;
                    }
                });
            } catch (error) {
                void error;
            }
        } catch (error) {
            void error;
        }
    };
    // Per-finish measurement factory: wraps the existing oracle finish
    // context (same epoch token, same object the route and settle paths
    // already use) and creates this finish's pending before the pull runs.
    // Route/settle behavior is unchanged: the returned context is the
    // oracle's own.
    const measureMakeFinishContext = (ref: object): DragOracleFinishContext => {
        const ctx = makeOracleFinishContext(ref);
        try {
            createMeasurePending(ctx);
        } catch (error) {
            void error;
        }
        // Bounded move-guard release: when this finish belongs to a held
        // tiled move, arm one per-finish timer on the shared verdict bound.
        // Settle cancels it; if no verdict ever settles, the timer consumes
        // only this finish's own start (newer Started stays) and releases
        // the hold once through the ordinary resync. No marker and no drag
        // terminal is created here. No Started-keyed guard exists: the hold
        // lasts from Started until Finish hands it to this per-finish guard.
        try {
            if (interactiveMoveRefs.has(ref)) {
                const cancel = scheduleOnce(DRAG_MEASURE_VERDICT_TIMEOUT_MS, () => {
                    try {
                        moveGuardCancels.delete(ctx);
                        if (!interactiveMoveRefs.has(ref)) {
                            return;
                        }
                        const start = oracleStarts.get(ctx.ref);
                        if (start !== undefined && start.epoch > ctx.finishEpoch) {
                            return;
                        }
                        if (isLiveMoveStillActive(ref)) {
                            return;
                        }
                        takeOwnStart(ctx);
                        const had = interactiveMoveRefs.delete(ref);
                        if (had && interactiveMoveRefs.size === 0 && interactiveResizeRefs.size === 0) {
                            try { adapter.setInteractiveResizeActive(false); } catch (error) { void error; }
                        }
                        clearMovePreviewFull(ref);
                        try { log(`omnitiler:route-diag:drag-move-timeout correlation=none`); } catch (error) { void error; }
                    } catch (error) {
                        void error;
                    }
                });
                if (typeof cancel === "function") {
                    moveGuardCancels.set(ctx, cancel);
                }
            }
        } catch (error) {
            void error;
        }
        return ctx;
    };
    const feedMeasureVerdict = (ctx: DragOracleFinishContext | undefined, verdict: DragOracleVerdict | null): void => {
        if (!KWIN_TRACE_ENABLED) {
            return;
        }
        try {
            if (ctx === undefined) {
                return;
            }
            const pending = measurePending.get(ctx);
            if (pending === undefined || pending.verdictDone) {
                return;
            }
            pending.verdict = verdict;
            pending.verdictMissing = verdict === null;
            pending.verdictDone = true;
            tryEmitMeasure(ctx);
        } catch (error) {
            void error;
        }
    };
    const settleOracleVerdict = (verdict: DragOracleVerdict | null, ctx: DragOracleFinishContext | undefined): void => {
        try {
            void verdict;
            if (ctx !== undefined) {
                const cancel = moveGuardCancels.get(ctx);
                if (cancel !== undefined) {
                    moveGuardCancels.delete(ctx);
                    try { cancel(); } catch (error) { void error; }
                }
                if (interactiveMoveRefs.has(ctx.ref)) {
                    const start = oracleStarts.get(ctx.ref);
                    if (start !== undefined && start.epoch > ctx.finishEpoch) {
                        takeOwnStart(ctx);
                        settleMovePreview(ctx);
                        feedMeasureVerdict(ctx, verdict);
                        return;
                    }
                    if (isLiveMoveStillActive(ctx.ref)) {
                        settleMovePreview(ctx);
                        feedMeasureVerdict(ctx, verdict);
                        return;
                    }
                    takeOwnStart(ctx);
                    const had = interactiveMoveRefs.delete(ctx.ref);
                    if (had && interactiveMoveRefs.size === 0 && interactiveResizeRefs.size === 0) {
                        try { adapter.setInteractiveResizeActive(false); } catch (error) { void error; }
                    }
                    settleMovePreview(ctx);
                    feedMeasureVerdict(ctx, verdict);
                    return;
                }
            }
            takeOwnStart(ctx);
            if (ctx !== undefined) {
                settleMovePreview(ctx);
            }
            feedMeasureVerdict(ctx, verdict);
        } catch (error) {
            void error;
        }
    };
    // Every oracle drag rejection logs its exact reason with the parsed
    // drag-N correlation (validated closed-vocabulary tokens only; no
    // titles, no native ids, no secrets). Logging is best-effort and never
    // affects behavior.
    const routeOracleVerdict = (verdict: DragOracleVerdict, ctx: DragOracleFinishContext | undefined): void => {
        try {
            if (ctx === undefined) {
                try { log(`omnitiler:route-diag:drag-context-invalid correlation=${verdict.correlation}`); } catch (error) { void error; }
                return;
            }
            const observed = observeNative(liveWorkspace, nativeIds, floatingIds, domainGaps, reportEligibility, nativeOwners, lastHoming);
            if (observed === null) {
                takeOwnStart(ctx);
                try { log(`omnitiler:route-diag:drag-scope-invalid correlation=${verdict.correlation}`); } catch (error) { void error; }
                return;
            }
            let ref: object | null = null;
            for (const entry of observed.windows) {
                if (entry.id === verdict.windowIdentity) {
                    ref = entry.ref;
                    break;
                }
            }
            if (ref === null) {
                takeOwnStart(ctx);
                try { log(`omnitiler:route-diag:drag-unknown-window correlation=${verdict.correlation}`); } catch (error) { void error; }
                return;
            }
            if (ref !== ctx.ref) {
                takeOwnStart(ctx);
                try { log(`omnitiler:route-diag:drag-ref-mismatch correlation=${verdict.correlation}`); } catch (error) { void error; }
                return;
            }
            // A premature Finished/verdict must not turn a live native move
            // into a geometry-writing drop. Retain the hold and Start until
            // KWin reports idle or a later, genuine Finish arrives.
            if (interactiveMoveRefs.has(ctx.ref) && isLiveMoveStillActive(ctx.ref)) {
                try { log(`omnitiler:route-diag:drag-move-ignored correlation=${verdict.correlation}`); } catch (error) { void error; }
                return;
            }
            const start = takeOwnStart(ctx);
            if (start === null || start.id !== verdict.windowIdentity) {
                try { log(`omnitiler:route-diag:drag-start-missing correlation=${verdict.correlation}`); } catch (error) { void error; }
                return;
            }
            if (start.move === true) {
                // Tiled move-drop route: only a move that STARTS tiled and
                // FINISHES tiled dispatches one drag-drop intent through the
                // shared single-flight (never two concurrent requests), with
                // the finish-captured script pointer and the verdict window
                // identity. Anything touching floating stays native-only with
                // the historical ignored line and no marker: floating-at-start
                // never entered the hold, and tiled-at-finish after a
                // floating start must not restore. A start/finish mismatch
                // logs one bounded line (closed-vocabulary tokens only, no
                // ids or coordinates) so the decision stays observable.
                // Cancelled verdicts never reach this route (the pull settles
                // them directly), so this branch only sees ok-moved drops. A
                // refused or failed drop arms the existing one-shot restore
                // marker through the adapter, so failed drops never break
                // tiling. No focus is forced: an unfocused source dispatches
                // as observed and the Planner refuses it, converging through
                // the marker like any refusal.
                let floatingFinish = false;
                try {
                    for (const entry of observed.windows) {
                        if (entry.id === verdict.windowIdentity) {
                            floatingFinish = (entry as { floating?: unknown }).floating === true;
                            break;
                        }
                    }
                } catch (error) {
                    void error;
                    floatingFinish = false;
                }
                const floatingStart = start.floatingStart === true;
                if (floatingStart !== floatingFinish) {
                    try { log(`omnitiler:route-diag:drag-move-floating-mismatch correlation=${verdict.correlation} start=${floatingStart ? "floating" : "tiled"} finish=${floatingFinish ? "floating" : "tiled"}`); } catch (error) { void error; }
                }
                if (floatingFinish || floatingStart) {
                    const cancel = moveGuardCancels.get(ctx);
                    if (cancel !== undefined) {
                        moveGuardCancels.delete(ctx);
                        try { cancel(); } catch (error) { void error; }
                    }
                    if (!isLiveMoveStillActive(ctx.ref)) {
                        const had = interactiveMoveRefs.delete(ctx.ref);
                        if (had && interactiveMoveRefs.size === 0 && interactiveResizeRefs.size === 0) {
                            try { adapter.setInteractiveResizeActive(false); } catch (error) { void error; }
                        }
                    }
                    try { log(`omnitiler:route-diag:drag-move-ignored correlation=${verdict.correlation}`); } catch (error) { void error; }
                    return;
                }
                try {
                    const cancel = moveGuardCancels.get(ctx);
                    if (cancel !== undefined) {
                        moveGuardCancels.delete(ctx);
                        try { cancel(); } catch (error) { void error; }
                    }
                } catch (error) {
                    void error;
                }
                if (!isLiveMoveStillActive(ctx.ref)) {
                    try { interactiveMoveRefs.delete(ctx.ref); } catch (error) { void error; }
                }
                try {
                    const pointer = ctx.pointerFinish ?? null;
                    const session = movePreviewSessions.get(ctx.ref);
                    const previewArg =
                        session !== undefined &&
                        session.epoch === start.epoch &&
                        session.id === start.id &&
                        session.id === verdict.windowIdentity
                            ? session.correlation
                            : undefined;
                    const accepted = adapter.requestDragDrop(
                        verdict.windowIdentity,
                        pointer === null ? undefined : pointer.x,
                        pointer === null ? undefined : pointer.y,
                        verdict.correlation,
                        { output: start.domainOutput, workspace: start.domainWorkspace },
                        previewArg,
                    );
                    movePreviewSessions.delete(ctx.ref);
                    if (accepted !== true && previewArg !== undefined) {
                        adapter.clearDragPreview(previewArg);
                    }
                    try {
                        log(`omnitiler:route-diag:drag-drop-dispatched correlation=${verdict.correlation} accepted=${accepted === true ? "true" : "false"}`);
                    } catch (error) {
                        void error;
                    }
                } catch (error) {
                    void error;
                    try { log(`omnitiler:route-diag:drag-drop-thrown correlation=${verdict.correlation}`); } catch (_ignored) { /* fail-closed */ }
                }
                return;
            }
            if (!(start.move === false && start.resize === true)) {
                try { log(`omnitiler:route-diag:drag-start-invalid correlation=${verdict.correlation}`); } catch (error) { void error; }
                return;
            }
            // Grabbed-edge routing: the grabbed edge(s) captured at Started
            // select the final target edge(s) from the authoritative final
            // rect (final edge target, never the finish pointer).
            // Non-grabbed edge deltas are ignored, never a rejection. A
            // grabbed corner routes both axes in exactly one dual-axis
            // pointer-resize intent through the shared single-flight (never
            // two concurrent requests); the single-axis wire shape is
            // unchanged. Reject only when no usable edge remains: missing
            // grab, zero move on every grabbed axis, or gone/identity-invalid
            // above. Logging is best-effort and never affects behavior; no
            // native ids or secrets are logged.
            const formatGrabbed = (grabbed: OracleGrabbed): string => {
                const h = grabbed.horizontal ?? "-";
                const v = grabbed.vertical ?? "-";
                return `${h}+${v}`;
            };
            // Verified press override: only after the entry verified the same
            // finish ref, the verdict window identity, and a resize-only
            // start above. A present press classifies by exact KWin thirds
            // regardless of the 64px Started-pointer depth gate; an absent
            // (missing/stale/other-window) press keeps the Started
            // nearest-frame capture with one correlated press-absent line (no
            // raw native ids or coordinates at normal level). A present but
            // unusable press (outside the start frame) keeps the Started capture
            // the same way. The epoch-guarded start consumption above already prevents
            // any async stale-start route; the press never bypasses it.
            let grabbed: OracleGrabbed | null = start.grabbed;
            let grabSource: OracleGrabSource | "missing" = start.grabSource;
            if (verdict.press !== undefined) {
                let identified: { grabbed: OracleGrabbed; source: OracleGrabSource } | null = null;
                try {
                    identified = identifyPressGrabbed(start.rect, verdict.press);
                } catch (error) {
                    void error;
                    identified = null;
                }
                if (identified !== null) {
                    grabbed = identified.grabbed;
                    grabSource = identified.source;
                } else {
                    try { log(`omnitiler:route-diag:drag-press-fallback correlation=${verdict.correlation} source=${start.grabSource}`); } catch (error) { void error; }
                }
            } else {
                try { log(`omnitiler:route-diag:drag-press-fallback correlation=${verdict.correlation} source=${start.grabSource}`); } catch (error) { void error; }
            }
            if (grabbed === null) {
                try { log(`omnitiler:route-diag:drag-no-grabbed-edge correlation=${verdict.correlation} source=${grabSource}`); } catch (error) { void error; }
                return;
            }
            const resolved = resolveOracleResizeTargets(start.rect, verdict.finalRect, grabbed);
            if (resolved === null) {
                try { log(`omnitiler:route-diag:drag-zero-move correlation=${verdict.correlation} grabbed=${formatGrabbed(grabbed)} source=${grabSource}`); } catch (error) { void error; }
                return;
            }
            try {
                const targetText = resolved.targets.map((t) => `${t.direction}:${String(t.boundary)}`).join(",");
                const ignoredText = resolved.ignored.length > 0 ? resolved.ignored.join(",") : "none";
                log(`omnitiler:route-diag:drag-route correlation=${verdict.correlation} grabbed=${formatGrabbed(grabbed)} source=${grabSource} targets=${targetText} ignored=${ignoredText}`);
            } catch (error) {
                void error;
            }
            // The adapter emits its exact source-grounded refusal token for
            // every distinct failure cause (disabled, identity, direction,
            // boundary, observation failure, absent target, fullscreen or
            // maximized target). No catch-all pointer-refused line is added here.
            // Targets resolve horizontal-first, so a corner pair is
            // (direction, boundary) horizontal plus (direction2, boundary2)
            // vertical in exactly one atomic intent. The dispatch outcome
            // is logged with the drag correlation: accepted means one
            // intent entered the single flight (dispatched or deferred);
            // refused means the adapter's exact refusal token (logged
            // alongside by the adapter) rejected it and nothing was sent.
            // Completion (planned/applied/rejected) is logged by the
            // adapter under its own plan correlation.
            try {
                let accepted = false;
                // Drag correlation travels as optional entry request metadata:
                // adapter-validation or Planner rejection of this drop then
                // converges once via a drag-correlated reconcile (no retry).
                // Cancelled/zero-move never reach here (strict no-op above).
                if (resolved.targets.length === 2) {
                    const first = resolved.targets[0] as { direction: string; boundary: number };
                    const second = resolved.targets[1] as { direction: string; boundary: number };
                    accepted = adapter.requestPointerResize(verdict.windowIdentity, first.direction, first.boundary, second.direction, second.boundary, verdict.correlation);
                } else {
                    const only = resolved.targets[0] as { direction: string; boundary: number };
                    accepted = adapter.requestPointerResize(verdict.windowIdentity, only.direction, only.boundary, undefined, undefined, verdict.correlation);
                }
                try {
                    const targetText = resolved.targets.map((t) => `${t.direction}:${String(t.boundary)}`).join(",");
                    log(`omnitiler:route-diag:drag-dispatched correlation=${verdict.correlation} targets=${targetText} accepted=${accepted === true ? "true" : "false"}`);
                } catch (error) {
                    void error;
                }
            } catch (error) {
                void error;
                try { log(`omnitiler:route-diag:drag-dispatch-thrown correlation=${verdict.correlation}`); } catch (_ignored) { /* fail-closed */ }
            }
        } catch (error) {
            void error;
            try { log(`omnitiler:route-diag:drag-route-thrown correlation=${verdict.correlation}`); } catch (_ignored) { /* fail-closed */ }
        }
    };
    const oracleSeen = new Set<object>();
    const oracleDetaches: Array<() => void> = [];
    const oracleWindowDetaches = new Map<object, Array<() => void>>();
    const trackOracleDetach = (ref: object, detach: () => void): void => {
        try {
            oracleDetaches.push(detach);
            const owned = oracleWindowDetaches.get(ref);
            if (owned === undefined) {
                oracleWindowDetaches.set(ref, [detach]);
            } else {
                owned.push(detach);
            }
        } catch (error) {
            void error;
        }
    };
    const dropOracleWindow = (ref: object): void => {
        // Removed-window cleanup keyed by object identity only: never reads an
        // id from the removal payload. Runs the window's own Started detach
        // so dead signals stop firing, then drops captured state.
        try {
            const owned = oracleWindowDetaches.get(ref);
            if (owned !== undefined) {
                oracleWindowDetaches.delete(ref);
                for (const detach of owned) {
                    try { detach(); } catch (error) { void error; }
                }
            }
            oracleStarts.delete(ref);
            measureStarts.delete(ref);
            measureStandaloneClaimed.delete(ref);
            completeMeasureRemoval(ref);
            clearMovePreviewFull(ref);
            try {
                for (const [ctx, cancel] of [...moveGuardCancels]) {
                    if (ctx.ref === ref) {
                        moveGuardCancels.delete(ctx);
                        try { cancel(); } catch (error) { void error; }
                    }
                }
            } catch (error) {
                void error;
            }
            const hadResize = interactiveResizeRefs.delete(ref);
            const hadMove = interactiveMoveRefs.delete(ref);
            if ((hadResize || hadMove) && interactiveResizeRefs.size === 0 && interactiveMoveRefs.size === 0) {
                adapter.setInteractiveResizeActive(false);
            }
            oracleSeen.delete(ref);
        } catch (error) {
            void error;
        }
    };
    const attachOracleStartOne = (ref: object): void => {
        if (oracleSeen.has(ref)) return;
        let started: unknown = null;
        let finished: unknown = null;
        try {
            started = readSignal(ref, "interactiveMoveResizeStarted");
            finished = readSignal(ref, "interactiveMoveResizeFinished");
        } catch (error) { void error; return; }
        let startedDetach: (() => void) | null = null;
        let finishedDetach: (() => void) | null = null;
        try {
            startedDetach = connectSignal(started, () => {
                try { captureOracleStart(ref); } catch (error) { void error; }
                try { captureMeasureStart(ref); } catch (error) { void error; }
            });
            if (startedDetach === null) return;
            finishedDetach = connectSignal(finished, () => {
                finishMovePreview(ref);
                try {
                    if (interactiveResizeRefs.has(ref)) {
                        if (!isLiveResizeStillActive(ref)) {
                            interactiveResizeRefs.delete(ref);
                            if (interactiveResizeRefs.size === 0 && interactiveMoveRefs.size === 0) {
                                adapter.setInteractiveResizeActive(false);
                            }
                        }
                    } else if (interactiveMoveRefs.has(ref)) {
                        // Tiled move hold: keep suppression until the
                        // correlated verdict settles (route or settle) or the
                        // bounded per-finish timer releases it. When the
                        // global pull never attached, no verdict can arrive,
                        // so release now through the ordinary resync.
                        let available = false;
                        try { available = measurePullAvailable === true; } catch (error) { void error; }
                        if (!available && !isLiveMoveStillActive(ref)) {
                            const had = interactiveMoveRefs.delete(ref);
                            if (had && interactiveMoveRefs.size === 0 && interactiveResizeRefs.size === 0) {
                                try { adapter.setInteractiveResizeActive(false); } catch (error) { void error; }
                            }
                        }
                    }
                } catch (error) { void error; }
                // Standalone measurement only: active exactly when the
                // global oracle pull failed to attach (no pull listeners
                // exist for this finish). Never duplicates the
                // context-keyed path above.
                try { createMeasureStandaloneFinish(ref); } catch (error) { void error; }
            });
            if (finishedDetach === null) {
                startedDetach();
                return;
            }
        } catch (error) { void error; return; }
        if (startedDetach === null || finishedDetach === null) return;
        oracleSeen.add(ref);
        trackOracleDetach(ref, startedDetach);
        trackOracleDetach(ref, finishedDetach);
        // Stepped preview sampling is best-effort: a missing
        // interactiveMoveResizeStepped surface disables move previews for
        // this window without affecting Started/Finished routing.
        try {
            const steppedDetach = connectSignal(readSignal(ref, "interactiveMoveResizeStepped"), () => {
                try { handleMoveStepped(ref); } catch (error) { void error; }
            });
            if (steppedDetach !== null) {
                trackOracleDetach(ref, steppedDetach);
            }
        } catch (error) { void error; }
    };
    const attachOracleStartAll = (): void => {
        try {
            const lister = surface["windowList"];
            if (typeof lister !== "function") return;
            const raw = Reflect.apply(lister as (...args: ReadonlyArray<never>) => unknown, surface, []);
            const list = decodeList(raw, MAX_LIST);
            if (list === null) return;
            for (const item of list) {
                if (typeof item === "object" && item !== null) attachOracleStartOne(item as object);
            }
        } catch (error) { void error; }
    };
    attachOracleStartAll();
    try {
        const addedDetach = connectSignal(readSignal(surface, "windowAdded"), () => { attachOracleStartAll(); });
        if (addedDetach !== null) oracleDetaches.push(addedDetach);
    } catch (error) { void error; }
    // Removed-window oracle cleanup within the exact existing signal APIs:
    // windowRemoved carries the Window object, used here by identity only.
    try {
        const removedDetach = connectSignal(readSignal(surface, "windowRemoved"), (removed) => {
            try {
                if (typeof removed === "object" && removed !== null) dropOracleWindow(removed as object);
            } catch (error) { void error; }
        });
        if (removedDetach !== null) oracleDetaches.push(removedDetach);
    } catch (error) { void error; }
    let oracleStop: (() => void) | null = null;
    try {
        const oracleOverrides: import("./drag-oracle-pull").DragOraclePullOverrides =
            overrides.oracleCallDbus === undefined
                ? { workspace: liveWorkspace, log, routePointer: routeOracleVerdict, onSettled: settleOracleVerdict, makeFinishContext: measureMakeFinishContext }
                : { workspace: liveWorkspace, log, callDbus: overrides.oracleCallDbus, routePointer: routeOracleVerdict, onSettled: settleOracleVerdict, makeFinishContext: measureMakeFinishContext };
        const oracleHandle = startDragOraclePullEntry(oracleOverrides);
        if (oracleHandle !== null) {
            oracleStop = () => { try { oracleHandle.stop(); } catch (error) { void error; } };
            measurePullAvailable = true;
        }
    } catch (error) { void error; }
    // Temporary active-group highlight bridge: asks the existing DescribePlan
    // route with {"op":"active-group"} at startup and on focus/domain/tree
    // lifecycle changes, validates the exact bounded reply shape and identity,
    // and forwards engine-projected union bounds to the effect-owned
    // SetGroupHighlight(QString)/ClearGroupHighlight() endpoint. The Planner
    // request/reply identity stays on the fixed session owner/generation;
    // only the effect setter payload carries one fresh bounded
    // per-script-instance stream generation (minted here from the shared
    // tray-publisher token source) so the native monotonic policy accepts a
    // restarted script through its existing new-stream branch. No new
    // transport, no topology derivation, no /Effects. Every script error,
    // no-group, service loss, or lifecycle invalidation clears fail-closed.
    let highlightStop: (() => void) | null = null;
    let highlightAttachFailed = false;
    let entryStopped = false;
    // Minted once per entry instance: a restarted script (new instance)
    // mints a fresh stream while retries in this instance keep one stream.
    const groupStreamGeneration =
        typeof overrides.groupGeneration === "string" && isEntryGeneration(overrides.groupGeneration)
            ? overrides.groupGeneration
            : processGeneration();
    const attachHighlightBridge = (): void => {
        if (entryStopped) {
            return;
        }
        if (highlightStop !== null) {
            return;
        }
        try {
            const ownerRaw = overrides.owner;
            const generationRaw = overrides.generation;
            if (typeof ownerRaw === "string" && typeof generationRaw === "string") {
                let effectCall = overrides.highlightCallDbus;
                if (effectCall === undefined) {
                    try {
                        const native: unknown = callDBus;
                        if (typeof native === "function") {
                            const bound = native as (...args: ReadonlyArray<unknown>) => void;
                            effectCall = (service, path, iface, method, ...args) => {
                                bound(service, path, iface, method, ...args);
                            };
                        }
                    } catch (error) {
                        void error;
                    }
                }
                if (effectCall !== undefined) {
                    const effect = effectCall;
                    const highlight = startActiveGroupHighlight({
                        callDescribePlan: (payload, callback) => {
                            callDbus(PLAN_SERVICE, PLAN_OBJECT, PLAN_INTERFACE, PLAN_METHOD, payload, callback);
                        },
                        setHighlight: (payload) => {
                            effect(GROUP_HIGHLIGHT_SERVICE, GROUP_HIGHLIGHT_OBJECT, GROUP_HIGHLIGHT_INTERFACE, GROUP_HIGHLIGHT_SET_METHOD, payload);
                        },
                        clearHighlight: () => {
                            effect(GROUP_HIGHLIGHT_SERVICE, GROUP_HIGHLIGHT_OBJECT, GROUP_HIGHLIGHT_INTERFACE, GROUP_HIGHLIGHT_CLEAR_METHOD);
                        },
                        observe: (): ActiveGroupObserved | null => {
                            let seen: PlanObserved | null = null;
                            try {
                                seen = observeNative(liveWorkspace, nativeIds, floatingIds, domainGaps, reportEligibility, nativeOwners, lastHoming);
                            } catch (error) {
                                void error;
                                return null;
                            }
                            if (seen === null) {
                                return null;
                            }
                            // Floating workspaces clear the tiling group
                            // underlay: no group queries there.
                            if (!isEntryDomainTiled(seen.domainOutput, seen.domainWorkspace)) {
                                return null;
                            }
                            try {
                                return {
                                    domainOutput: seen.domainOutput,
                                    domainWorkspace: seen.domainWorkspace,
                                    domainBounds: {
                                        x: seen.domainBounds.x,
                                        y: seen.domainBounds.y,
                                        w: seen.domainBounds.w,
                                        h: seen.domainBounds.h,
                                    },
                                    domainGap: seen.domainGap,
                                    domainOuterGap: seen.domainOuterGap,
                                    focusedId: seen.focusedId,
                                    windows: seen.windows.map((entry) => ({
                                        id: entry.id,
                                        output: entry.output,
                                        workspace: entry.workspace,
                                        rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
                                        // Script-only lifecycle validity; never
                                        // serialized into the DescribePlan request.
                                        fullscreen: entry.fullscreen,
                                    })),
                                };
                            } catch (error) {
                                void error;
                                return null;
                            }
                        },
                        subscribe: (kind, handler) => {
                            if (kind === "focus") {
                                const detach = sub("windowActivated", handler);
                                if (detach === null) {
                                    throw new Error("plan-entry-highlight-signal-failed");
                                }
                                return detach;
                            }
                            if (kind === "fullscreen") {
                                // Best-effort per-window fullscreen state: uses the
                                // existing documented fullScreenChanged seam. Never
                                // fails enable; effect-side eligibility still hides
                                // when the source is unavailable.
                                try {
                                    const detach = subWindowFullscreen(handler);
                                    if (detach === null) {
                                        return (): void => {};
                                    }
                                    return detach;
                                } catch (error) {
                                    void error;
                                    return (): void => {};
                                }
                            }
                            if (kind === "domain") {
                                const first = sub("screensChanged", handler);
                                const second = sub("currentDesktopChanged", handler);
                                if (first === null || second === null) {
                                    if (first !== null) {
                                        try {
                                            first();
                                        } catch (error) {
                                            void error;
                                        }
                                    }
                                    throw new Error("plan-entry-highlight-signal-failed");
                                }
                                return (): void => {
                                    try {
                                        first();
                                    } catch (error) {
                                        void error;
                                    }
                                    try {
                                        second();
                                    } catch (error) {
                                        void error;
                                    }
                                };
                            }
                            const added = sub("windowAdded", handler);
                            const removed = sub("windowRemoved", handler);
                            if (added === null || removed === null) {
                                if (added !== null) {
                                    try {
                                        added();
                                    } catch (error) {
                                        void error;
                                    }
                                }
                                throw new Error("plan-entry-highlight-signal-failed");
                            }
                            return (): void => {
                                try {
                                    added();
                                } catch (error) {
                                    void error;
                                }
                                try {
                                    removed();
                                } catch (error) {
                                    void error;
                                }
                            };
                        },
                        log,
                        owner: ownerRaw,
                        generation: generationRaw,
                        groupGeneration: groupStreamGeneration,
                    });
                    if (highlight !== null) {
                        // Single observational refresh after each successful
                        // geometry-plan boundary, even when focus is unchanged.
                        // Geometry writes emit no highlight lifecycle signal, so
                        // this edge is the only post-plan refresh: no timers,
                        // no polling, no geometry subscription.
                        highlightRefresh = (): void => {
                            try {
                                highlight.refresh();
                            } catch (error) {
                                void error;
                            }
                        };
                        highlightStop = () => {
                            highlightRefresh = null;
                            try {
                                highlight.stop();
                            } catch (error) {
                                void error;
                            }
                        };
                        if (highlightAttachFailed) {
                            highlightAttachFailed = false;
                            try {
                                log("omnitiler:plan:highlight-attach stage=recovered");
                            } catch (error) {
                                void error;
                            }
                        }
                    } else if (!highlightAttachFailed) {
                        highlightAttachFailed = true;
                        try {
                            log("omnitiler:plan:highlight-attach stage=failed reason=bridge-unavailable");
                        } catch (error) {
                            void error;
                        }
                    }
                }
            }
        } catch (error) {
            if (!highlightAttachFailed && highlightStop === null && !entryStopped) {
                let bridgeExpected = false;
                try {
                    bridgeExpected =
                        typeof overrides.owner === "string" &&
                        typeof overrides.generation === "string" &&
                        (typeof overrides.highlightCallDbus === "function" || typeof callDBus === "function");
                } catch (expectedError) {
                    void expectedError;
                }
                if (bridgeExpected) {
                    highlightAttachFailed = true;
                    try {
                        log("omnitiler:plan:highlight-attach stage=failed reason=bridge-unavailable");
                    } catch (logError) {
                        void logError;
                    }
                }
            }
        }
    };
    retryHighlightAttach = (): void => {
        if (!highlightAttachFailed) {
            return;
        }
        try {
            attachHighlightBridge();
        } catch (error) {
            void error;
        }
    };
    attachHighlightBridge();
    let optionsConfigDetach: (() => void) | null = null;
    try {
        const optionsGlobal: unknown =
            overrides.options !== undefined ? overrides.options : resolveLexicalOptions();
        if (typeof optionsGlobal === "object" && optionsGlobal !== null) {
            optionsConfigDetach = connectSignal(
                readSignal(optionsGlobal as object, "configChanged"),
                () => {
                    try {
                        retryHighlightAttach();
                    } catch (error) {
                        void error;
                    }
                    // Startup-consumed drift note first: workspaceMode and
                    // shortcutProfile never re-read for behavior here. Drift
                    // is restart-required, never adopted.
                    try {
                        const drifted: string[] = [];
                        if (
                            String(readWorkspaceModeValue(overrides.readWorkspaceModeFn)) !== String(startupWorkspaceMode)
                        ) {
                            drifted.push("workspaceMode");
                        }
                        if (readShortcutProfile(overrides.readProfileFn) !== startupProfile) {
                            drifted.push("shortcutProfile");
                        }
                        if (drifted.length > 0) {
                            try {
                                log(
                                    `omnitiler:plan:config-reloaded stage=restart-required keys=${drifted.join(",")}`,
                                );
                            } catch (error) {
                                void error;
                            }
                        }
                    } catch (error) {
                        void error;
                    }
                    // Live default: newly discovered backing ids use the
                    // current default after configChanged; existing session
                    // states are retained.
                    try {
                        const parsed = parseDefaultTiled(readTilingDefaultValue(overrides.readTilingDefaultFn));
                        if (workspaceNative.setDefaultTiled(parsed)) {
                            try {
                                workspaceNative.handleTopologySignal();
                            } catch (error) {
                                void error;
                            }
                            emitWorkspaceTiling();
                            try {
                                log(`omnitiler:plan:config-reloaded stage=default-tiled tiled=${parsed ? "true" : "false"}`);
                            } catch (error) {
                                void error;
                            }
                        }
                    } catch (error) {
                        void error;
                    }
                    // R-MOV-03 live mode: re-read the same-axis move setting
                    // for subsequent moves only. No tree rebuild, no resync,
                    // no topology work; the adapter reads the entry-owned
                    // value live per move request.
                    try {
                        const reread = readSameAxisMoveValue(overrides.readSameAxisMoveFn);
                        if (reread !== sameAxisMove) {
                            sameAxisMove = reread;
                            try {
                                log(`omnitiler:plan:config-reloaded stage=same-axis-move mode=${reread}`);
                            } catch (error) {
                                void error;
                            }
                        }
                    } catch (error) {
                        void error;
                    }
                    // R-SPC-04 D1 live predicate: re-read the fixed-size
                    // predicate for subsequent admissions only. No
                    // reclassification, no resync, no topology work; the
                    // adapter reads the entry-owned value live per request
                    // and carries it on the planner wire.
                    try {
                        const reread = readFixedSizePredicateValue(
                            overrides.readFixedSizePredicateFn,
                        );
                        if (reread !== fixedSizePredicate) {
                            fixedSizePredicate = reread;
                            try {
                                log(`omnitiler:plan:config-reloaded stage=fixed-size-predicate predicate=${reread}`);
                            } catch (error) {
                                void error;
                            }
                        }
                    } catch (error) {
                        void error;
                    }
                    // R-MAX-06 live mode: re-read the born-maximized setting
                    // for subsequent admissions only. No rebuild, no
                    // reclassification, no resync, no topology work; the
                    // adapter reads the entry-owned value live per admission.
                    try {
                        const reread = readBornMaximizedValue(
                            overrides.readBornMaximizedFn,
                        );
                        if (reread !== bornMaximized) {
                            bornMaximized = reread;
                            try {
                                log(`omnitiler:plan:config-reloaded stage=born-maximized mode=${reread}`);
                            } catch (error) {
                                void error;
                            }
                        }
                    } catch (error) {
                        void error;
                    }
                    // R-WS-12 G-37 live refill: re-read the migration source
                    // refill for subsequent migrations only. No destination,
                    // history, or lifecycle work; the map commit reads the
                    // pushed value per migration.
                    try {
                        if (
                            workspaceNative.setMigrationSourceRefill(
                                readMigrationSourceRefillValue(overrides.readMigrationSourceRefillFn),
                            )
                        ) {
                            try {
                                log(`omnitiler:plan:config-reloaded stage=migration-source-refill refill=${workspaceNative.getMigrationSourceRefill()}`);
                            } catch (error) {
                                void error;
                            }
                        }
                    } catch (error) {
                        void error;
                    }
                    let next: DomainGaps;
                    try {
                        next = readDomainGaps({
                            readInnerGapFn: overrides.readInnerGapFn,
                            readOuterGapFn: overrides.readOuterGapFn,
                        });
                    } catch (error) {
                        void error;
                        return;
                    }
                    if (next.innerGap === domainGaps.innerGap && next.outerGap === domainGaps.outerGap) {
                        return;
                    }
                    domainGaps = next;
                    try {
                        workspaceSend.updateGaps({ innerGap: next.innerGap, outerGap: next.outerGap });
                    } catch (error) {
                        void error;
                    }
                    // Queued, applied unconfirmed: the validated pair is
                    // adopted for subsequent requests only. The update-gaps
                    // plan flight runs the ordinary retained route on the
                    // planner channel while workspace sends commit on the
                    // separate send channel, so no send outcome here proves
                    // the gap resync applied. Application stays unconfirmed;
                    // restart the session to guarantee pickup.
                    try {
                        log(
                            `omnitiler:plan:config-reloaded stage=re-read-queued innerGap=${String(next.innerGap)} outerGap=${String(next.outerGap)} applied-unconfirmed`,
                        );
                    } catch (error) {
                        void error;
                    }
                    try {
                        adapter.requestResync();
                    } catch (error) {
                        void error;
                    }
                },
            );
        }
    } catch (error) {
        void error;
        optionsConfigDetach = null;
    }
    return {
        stop: () => {
            entryStopped = true;
            workspaceNativeRef = null;
            for (const ref of [...movePreviewSessions.keys()]) {
                clearMovePreviewFull(ref);
            }
            try {
                adapter.disable();
            } catch (error) {
                void error;
            }
            try {
                workspaceSend.disable();
            } catch (error) {
                void error;
            }
            try {
                workspaceNative.disable();
            } catch (error) {
                void error;
            }
            for (const detach of workspaceDetaches.splice(0)) {
                try { detach(); } catch (error) { void error; }
            }
            workspaceWindowDetaches.clear();
            for (const detach of oracleDetaches) {
                try { detach(); } catch (error) { void error; }
            }
            try {
                for (const pending of [...measurePending.values()]) {
                    dropMeasureWindow(pending.ref);
                }
                measureStarts.clear();
                measureStandaloneClaimed.clear();
            } catch (error) { void error; }
            if (oracleStop !== null) {
                try { oracleStop(); } catch (error) { void error; }
            }
            if (highlightStop !== null) {
                try { highlightStop(); } catch (error) { void error; }
            }
            if (optionsConfigDetach !== null) {
                try { optionsConfigDetach(); } catch (error) { void error; }
            }
        },
        requestFocus: (direction) => {
            try {
                adapter.requestFocus(direction);
            } catch (error) {
                void error;
            }
        },
        requestMove: (direction) => {
            try {
                adapter.requestMove(direction);
            } catch (error) {
                void error;
            }
        },
        requestResize: (direction, mode) => {
            try {
                adapter.requestResize(direction, mode);
            } catch (error) {
                void error;
            }
        },
        requestToggleOrientation: () => {
            try {
                adapter.requestToggleOrientation();
            } catch (error) {
                void error;
            }
        },
        requestFloat: () => {
            try {
                adapter.requestFloat();
            } catch (error) {
                void error;
            }
        },
        requestSticky: () => {
            try {
                adapter.requestSticky();
            } catch (error) {
                void error;
            }
        },
        requestMaximize: () => {
            try {
                adapter.requestMaximize();
            } catch (error) {
                void error;
            }
        },
        requestFullscreen: () => {
            try {
                adapter.requestFullscreen();
            } catch (error) {
                void error;
            }
        },
        requestWorkspaceSelect: (index) => {
            try {
                requestWorkspaceSelect(index);
            } catch (error) {
                void error;
            }
        },
        requestWorkspaceMove: (index, follow) => {
            try {
                requestWorkspaceMove(index, follow);
            } catch (error) {
                void error;
            }
        },
        requestWorkspacePrevious: () => {
            try {
                requestWorkspacePrevious();
            } catch (error) {
                void error;
            }
        },
        requestWorkspaceRelative: (delta) => {
            try {
                requestWorkspaceRelative(delta);
            } catch (error) {
                void error;
            }
        },
        requestWorkspaceRelativeMove: (delta, follow) => {
            try {
                requestWorkspaceRelativeMove(delta, follow);
            } catch (error) {
                void error;
            }
        },
        requestSendToOutput: (direction, follow) => {
            try {
                requestOutputSend(direction, follow);
            } catch (error) {
                void error;
            }
        },
        requestWorkspaceMigrate: (direction) => {
            try {
                requestWorkspaceMigrate(direction);
            } catch (error) {
                void error;
            }
        },
        getWorkspaceTilingSnapshot: () => {
            try {
                return getWorkspaceTilingSnapshot();
            } catch (error) {
                void error;
                return { scope: "", tiled: true, defaultTiled: true };
            }
        },
        requestWorkspaceTilingToggle: () => {
            try {
                toggleWorkspaceTiling();
            } catch (error) {
                void error;
            }
        },
    };
}

// C1 startup attach recovery: a transient enable refusal at startup must
// not keep a null handle for the whole session. The single attempt above
// leaves nothing attached when enable refuses (the adapter releases its
// partial subscriptions before returning false, and no handle is returned),
// so a later native edge can safely run one fresh full attempt: shortcuts,
// workspace routes, oracle, and the highlight bridge all register exactly
// once, on the succeeding attempt. Retry rides only on signals that need
// neither a windowList read nor an enabled adapter (workspace windowAdded,
// workspace screensChanged, Options configChanged); all use the shared
// connector, so no timer exists to cancel or leak. A missing windowList
// function, invalid owner/generation, or no attachable retry edge stays
// terminal null exactly as before, and every terminal path stays silent.
// An unexpected throw (as opposed to a clean enable refusal returning
// null) may leave a partially attached attempt behind, so it stays terminal
// and never retries: the first-attempt throw returns null, and a
// retry-attempt throw detaches the wait and stays inert, keeping exactly
// one bounded failed line. While pending, the handle actuates nothing:
// every request is a no-op until the succeeding attempt adopts the real
// handle. stop() detaches the wait and forwards to the adopted handle when
// one exists. One bounded failed line at wait start, one recovered line on
// success; re-attempts themselves stay silent, with no attempt budget and
// no exhaustion. A hard-missing KWin transport/timer (native callDBus or
// QTimer absent with no override) returns null before adapter.enable and
// stays terminal silent with no attach, log, or retry; only a refused
// enable retries. The failed line names the actual refused subscribe kind
// from the single attempt's enable loop (no second windowList probe).
function isEntryGeneration(value: unknown): boolean {
    if (typeof value !== "string" || value.length === 0 || value.length > 64) {
        return false;
    }
    for (let index = 0; index < value.length; index += 1) {
        const code = value.charCodeAt(index);
        const ok = (code >= 97 && code <= 122) || (code >= 48 && code <= 57) || code === 45;
        if (!ok) {
            return false;
        }
    }
    return true;
}

export function startPlanAdapterEntry(overrides: PlanEntryOverrides = {}): PlanEntryHandle | null {
    let first: PlanEntryHandle | null = null;
    let firstCause: string | null = null;
    try {
        first = startPlanAdapterEntryOnce(overrides, (cause) => {
            firstCause = cause;
        });
    } catch (error) {
        void error;
        return null;
    }
    if (first !== null) {
        return first;
    }
    const liveWorkspace: unknown =
        overrides.workspace !== undefined ? overrides.workspace : resolveLexicalWorkspace();
    if (typeof liveWorkspace !== "object" || liveWorkspace === null) {
        return null;
    }
    const surface = liveWorkspace as Record<string, unknown>;
    if (!isOpaqueId(overrides.owner) || !isEntryGeneration(overrides.generation)) {
        return null;
    }
    if (typeof readProp(surface, "windowList") !== "function") {
        return null;
    }
    // Hard-missing KWin transport/timer is terminal: the single attempt
    // above already returned null before adapter.enable, so never attach,
    // log, or retry. Mirrors the override/native resolution in
    // startPlanAdapterEntryOnce: only a missing native matters when no
    // override supplies it.
    if (overrides.callDbus === undefined) {
        try {
            const native: unknown = callDBus;
            if (typeof native !== "function") {
                return null;
            }
        } catch (error) {
            void error;
            return null;
        }
    }
    if (overrides.scheduleOnce === undefined) {
        try {
            const ctor: unknown = QTimer;
            if (typeof ctor !== "function") {
                return null;
            }
        } catch (error) {
            void error;
            return null;
        }
    }
    const log = overrides.log ?? ((message: string): void => {
        try {
            console.log(message);
        } catch (error) {
            void error;
        }
    });
    const detaches: Array<() => void> = [];
    const detachWait = (): void => {
        for (const detach of detaches.splice(0)) {
            try {
                detach();
            } catch (error) {
                void error;
            }
        }
    };
    let stopped = false;
    let current: PlanEntryHandle | null = null;
    const attemptRecovery = (): void => {
        if (stopped || current !== null) {
            return;
        }
        let next: PlanEntryHandle | null = null;
        try {
            next = startPlanAdapterEntryOnce(overrides);
        } catch (error) {
            void error;
            detachWait();
            return;
        }
        if (next === null) {
            return;
        }
        current = next;
        detachWait();
        try {
            log("omnitiler:plan:entry-attach stage=recovered");
        } catch (error) {
            void error;
        }
    };
    const onNativeEvent = (): void => {
        try {
            attemptRecovery();
        } catch (error) {
            void error;
        }
    };
    try {
        const addedDetach = connectSignal(readSignal(surface, "windowAdded"), onNativeEvent);
        if (addedDetach !== null) {
            detaches.push(addedDetach);
        }
    } catch (error) {
        void error;
    }
    try {
        const screensDetach = connectSignal(readSignal(surface, "screensChanged"), onNativeEvent);
        if (screensDetach !== null) {
            detaches.push(screensDetach);
        }
    } catch (error) {
        void error;
    }
    try {
        const optionsGlobal: unknown =
            overrides.options !== undefined ? overrides.options : resolveLexicalOptions();
        if (typeof optionsGlobal === "object" && optionsGlobal !== null) {
            const configDetach = connectSignal(readSignal(optionsGlobal as object, "configChanged"), onNativeEvent);
            if (configDetach !== null) {
                detaches.push(configDetach);
            }
        }
    } catch (error) {
        void error;
    }
    if (detaches.length === 0) {
        return null;
    }
    const failedCause = firstCause ?? "enable-refused";
    try {
        log(`omnitiler:plan:entry-attach stage=failed cause=${failedCause} recovery=retry-on-native-event`);
    } catch (error) {
        void error;
    }
    const delegate = (action: (target: PlanEntryHandle) => void): void => {
        try {
            if (current !== null) {
                action(current);
            }
        } catch (error) {
            void error;
        }
    };
    return {
        stop: () => {
            stopped = true;
            detachWait();
            try {
                if (current !== null) {
                    current.stop();
                }
            } catch (error) {
                void error;
            }
        },
        requestFocus: (direction) => {
            delegate((target) => target.requestFocus(direction));
        },
        requestMove: (direction) => {
            delegate((target) => target.requestMove(direction));
        },
        requestResize: (direction, mode) => {
            delegate((target) => target.requestResize(direction, mode));
        },
        requestToggleOrientation: () => {
            delegate((target) => target.requestToggleOrientation());
        },
        requestFloat: () => {
            delegate((target) => target.requestFloat());
        },
        requestSticky: () => {
            delegate((target) => target.requestSticky());
        },
        requestMaximize: () => {
            delegate((target) => target.requestMaximize());
        },
        requestFullscreen: () => {
            delegate((target) => target.requestFullscreen());
        },
        requestWorkspaceSelect: (index) => {
            delegate((target) => target.requestWorkspaceSelect(index));
        },
        requestWorkspaceMove: (index, follow) => {
            delegate((target) => target.requestWorkspaceMove(index, follow));
        },
        requestWorkspacePrevious: () => {
            delegate((target) => target.requestWorkspacePrevious());
        },
        requestWorkspaceRelative: (delta) => {
            delegate((target) => target.requestWorkspaceRelative(delta));
        },
        requestWorkspaceRelativeMove: (delta, follow) => {
            delegate((target) => target.requestWorkspaceRelativeMove(delta, follow));
        },
        requestSendToOutput: (direction, follow) => {
            delegate((target) => target.requestSendToOutput(direction, follow));
        },
        requestWorkspaceMigrate: (direction) => {
            delegate((target) => target.requestWorkspaceMigrate(direction));
        },
        getWorkspaceTilingSnapshot: () => {
            if (current !== null) {
                try {
                    return current.getWorkspaceTilingSnapshot();
                } catch (error) {
                    void error;
                }
            }
            return { scope: "", tiled: true, defaultTiled: true };
        },
        requestWorkspaceTilingToggle: () => {
            delegate((target) => target.requestWorkspaceTilingToggle());
        },
    };
}
