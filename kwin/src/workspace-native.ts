// Production KWin dynamic workspace shortcut routing plus the per-output
// desktop capability seam. No tiling, ordering, geometry, trailing-lifecycle,
// displacement, or focus policy lives here; Rust owns every structural
// decision (trailing lifecycle in crates/tiler-core/src/workspace.rs
// `plan_trailing`, displacement in crates/tiler-core/src/workspace_scope.rs
// `choose_displaced_destination`, refill in
// crates/tiler-core/src/output_selection.rs `select_migration_source_refill`)
// and the world entry owns observation plus safe actuation through public
// KWin state only.

import { connectSignal, readSignal, type SignalDetach, type SignalHandler } from "./signal-capability";

export type WorkspaceMode = "per-output-local" | "global-unique" | "shared";

export const DEFAULT_WORKSPACE_MODE: WorkspaceMode = "per-output-local";

export function parseWorkspaceMode(value: unknown): WorkspaceMode {
    if (value === "per-output-local" || value === "global-unique" || value === "shared") {
        return value;
    }
    return DEFAULT_WORKSPACE_MODE;
}

const SHIFT_DIGIT_SYMBOL_ALIAS: ReadonlyMap<number, string> = new Map([
    [1, "!"],
    [2, "@"],
    [3, "#"],
    [4, "$"],
    [5, "%"],
    [6, "^"],
    [7, "&"],
    [8, "*"],
    [9, "("],
    [0, ")"],
]);

export function symbolForDigit(digit: number): string | null {
    const symbol = SHIFT_DIGIT_SYMBOL_ALIAS.get(digit);
    return symbol === undefined ? null : symbol;
}

export interface WorkspaceShortcutRow {
    readonly action: string;
    readonly text: string;
    readonly sequence: string;
    readonly kind: "select" | "move" | "move-stay" | "previous" | "relative" | "send-relative" | "send-relative-stay";
    readonly index: number;
    readonly delta?: -1 | 1;
}

export function workspaceShortcutCatalog(): ReadonlyArray<WorkspaceShortcutRow> {
    const rows: WorkspaceShortcutRow[] = [];
    for (let index = 1; index <= 9; index += 1) {
        rows.push({
            action: `omnitiler-workspace-${String(index)}`,
            text: `Focus workspace ${String(index)}`,
            sequence: `Meta+${String(index)}`,
            kind: "select",
            index,
        });
    }
    rows.push({
        action: "omnitiler-workspace-0",
        text: "Focus or create the trailing empty workspace",
        sequence: "Meta+0",
        kind: "select",
        index: 0,
    });
    for (let index = 1; index <= 9; index += 1) {
        rows.push({
            action: `omnitiler-move-workspace-${String(index)}`,
            text: `Move window to workspace ${String(index)}`,
            sequence: `Meta+Shift+${String(index)}`,
            kind: "move",
            index,
        });
        const symbol = symbolForDigit(index);
        if (symbol !== null) {
            rows.push({
                action: `omnitiler-move-workspace-${String(index)}-symbol`,
                text: `Move window to workspace ${String(index)} (shifted-symbol alias)`,
                sequence: `Meta+${symbol}`,
                kind: "move",
                index,
            });
        }
    }
    rows.push({
        action: "omnitiler-move-workspace-append",
        text: "Move window to a newly appended workspace",
        sequence: "Meta+Shift+0",
        kind: "move",
        index: 0,
    });
    const zeroSymbol = symbolForDigit(0);
    if (zeroSymbol !== null) {
        rows.push({
            action: "omnitiler-move-workspace-append-symbol",
            text: "Move window to a newly appended workspace (shifted-symbol alias)",
            sequence: `Meta+${zeroSymbol}`,
            kind: "move",
            index: 0,
        });
    }
    rows.push({
        action: "omnitiler-workspace-previous",
        text: "Toggle to the previous workspace",
        sequence: "Meta+Ctrl+Tab",
        kind: "previous",
        index: 0,
    });
    rows.push({
        action: "omnitiler-workspace-prev-h",
        text: "Previous workspace",
        sequence: "Meta+Ctrl+H",
        kind: "relative",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-workspace-prev-k",
        text: "Previous workspace",
        sequence: "Meta+Ctrl+K",
        kind: "relative",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-workspace-prev-left-arrow",
        text: "Previous workspace",
        sequence: "Meta+Ctrl+Left",
        kind: "relative",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-workspace-prev-up-arrow",
        text: "Previous workspace",
        sequence: "Meta+Ctrl+Up",
        kind: "relative",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-workspace-next-j",
        text: "Next workspace",
        sequence: "Meta+Ctrl+J",
        kind: "relative",
        index: 0,
        delta: 1,
    });
    rows.push({
        action: "omnitiler-workspace-next-l",
        text: "Next workspace",
        sequence: "Meta+Ctrl+L",
        kind: "relative",
        index: 0,
        delta: 1,
    });
    rows.push({
        action: "omnitiler-workspace-next-down-arrow",
        text: "Next workspace",
        sequence: "Meta+Ctrl+Down",
        kind: "relative",
        index: 0,
        delta: 1,
    });
    rows.push({
        action: "omnitiler-workspace-next-right-arrow",
        text: "Next workspace",
        sequence: "Meta+Ctrl+Right",
        kind: "relative",
        index: 0,
        delta: 1,
    });
    // R-WS-01/14 relative send-and-follow: same scoped ring as selection,
    // resolved once before transfer at the entry. Bound chords.
    rows.push({
        action: "omnitiler-send-prev-h",
        text: "Send window to the previous workspace",
        sequence: "Meta+Ctrl+Shift+H",
        kind: "send-relative",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-send-prev-k",
        text: "Send window to the previous workspace",
        sequence: "Meta+Ctrl+Shift+K",
        kind: "send-relative",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-send-prev-left-arrow",
        text: "Send window to the previous workspace",
        sequence: "Meta+Ctrl+Shift+Left",
        kind: "send-relative",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-send-prev-up-arrow",
        text: "Send window to the previous workspace",
        sequence: "Meta+Ctrl+Shift+Up",
        kind: "send-relative",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-send-next-j",
        text: "Send window to the next workspace",
        sequence: "Meta+Ctrl+Shift+J",
        kind: "send-relative",
        index: 0,
        delta: 1,
    });
    rows.push({
        action: "omnitiler-send-next-l",
        text: "Send window to the next workspace",
        sequence: "Meta+Ctrl+Shift+L",
        kind: "send-relative",
        index: 0,
        delta: 1,
    });
    rows.push({
        action: "omnitiler-send-next-down-arrow",
        text: "Send window to the next workspace",
        sequence: "Meta+Ctrl+Shift+Down",
        kind: "send-relative",
        index: 0,
        delta: 1,
    });
    rows.push({
        action: "omnitiler-send-next-right-arrow",
        text: "Send window to the next workspace",
        sequence: "Meta+Ctrl+Shift+Right",
        kind: "send-relative",
        index: 0,
        delta: 1,
    });
    // R-WS-01 absolute send-and-stay: numbered plus append, bindable but
    // unbound by default (empty sequence). Mirrors the follow move rows
    // including shifted-symbol aliases.
    for (let index = 1; index <= 9; index += 1) {
        rows.push({
            action: `omnitiler-stay-workspace-${String(index)}`,
            text: `Send window to workspace ${String(index)} without following`,
            sequence: "",
            kind: "move-stay",
            index,
        });
        const symbol = symbolForDigit(index);
        if (symbol !== null) {
            rows.push({
                action: `omnitiler-stay-workspace-${String(index)}-symbol`,
                text: `Send window to workspace ${String(index)} without following (shifted-symbol alias)`,
                sequence: "",
                kind: "move-stay",
                index,
            });
        }
    }
    rows.push({
        action: "omnitiler-stay-workspace-append",
        text: "Send window to a newly appended workspace without following",
        sequence: "",
        kind: "move-stay",
        index: 0,
    });
    const stayZeroSymbol = symbolForDigit(0);
    if (stayZeroSymbol !== null) {
        rows.push({
            action: "omnitiler-stay-workspace-append-symbol",
            text: "Send window to a newly appended workspace without following (shifted-symbol alias)",
            sequence: "",
            kind: "move-stay",
            index: 0,
        });
    }
    // R-WS-14 relative send-and-stay: bindable but unbound by default.
    rows.push({
        action: "omnitiler-send-stay-prev-h",
        text: "Send window to the previous workspace without following",
        sequence: "",
        kind: "send-relative-stay",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-send-stay-prev-k",
        text: "Send window to the previous workspace without following",
        sequence: "",
        kind: "send-relative-stay",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-send-stay-prev-left-arrow",
        text: "Send window to the previous workspace without following",
        sequence: "",
        kind: "send-relative-stay",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-send-stay-prev-up-arrow",
        text: "Send window to the previous workspace without following",
        sequence: "",
        kind: "send-relative-stay",
        index: 0,
        delta: -1,
    });
    rows.push({
        action: "omnitiler-send-stay-next-j",
        text: "Send window to the next workspace without following",
        sequence: "",
        kind: "send-relative-stay",
        index: 0,
        delta: 1,
    });
    rows.push({
        action: "omnitiler-send-stay-next-l",
        text: "Send window to the next workspace without following",
        sequence: "",
        kind: "send-relative-stay",
        index: 0,
        delta: 1,
    });
    rows.push({
        action: "omnitiler-send-stay-next-down-arrow",
        text: "Send window to the next workspace without following",
        sequence: "",
        kind: "send-relative-stay",
        index: 0,
        delta: 1,
    });
    rows.push({
        action: "omnitiler-send-stay-next-right-arrow",
        text: "Send window to the next workspace without following",
        sequence: "",
        kind: "send-relative-stay",
        index: 0,
        delta: 1,
    });
    return Object.freeze(rows);
}

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

interface DesktopEntry {
    readonly ref: object;
    readonly id: string;
    readonly num: number | null;
}

function readDesktopId(desktop: object): string | null {
    const raw = readProp(desktop, "id");
    return isOpaqueId(raw) ? (raw as string) : null;
}

// KWin 6.7 introduced per-output virtual desktops: the workspace
// currentDesktopForScreen/setCurrentDesktopForScreen methods plus the
// migration-only options.perOutputVirtualDesktops property. KWin 6.6
// exposes none of them; the shared workspace.currentDesktop is then current
// on every output. This is the single feature-detection owner: every other
// module must use these helpers and never read the 6.7-only symbols
// directly, so the API guard can confine them here.
function hasPerOutputDesktopRead(surface: unknown): boolean {
    if (typeof surface !== "object" || surface === null) {
        return false;
    }
    // The read method identifies the modern API; write sites separately
    // check the live flag and setter.
    return typeof readProp(surface as object, "currentDesktopForScreen") === "function";
}

export interface OutputDesktopRead {
    readonly ref: object;
    readonly id: string;
}

function readSharedDesktop(surface: object): OutputDesktopRead | null {
    const raw = readProp(surface, "currentDesktop");
    if (typeof raw !== "object" || raw === null) {
        return null;
    }
    const id = readDesktopId(raw as object);
    if (id === null) {
        return null;
    }
    return { ref: raw as object, id };
}

// Current desktop for one output: the per-output read on 6.7+, else the
// shared currentDesktop which is current on every output on 6.6. Null when
// unreadable; null never counts as selected downstream.
export function readDesktopForOutput(surface: unknown, output: object): OutputDesktopRead | null {
    if (typeof surface !== "object" || surface === null) {
        return null;
    }
    const target = surface as object;
    const fn = readProp(target, "currentDesktopForScreen");
    if (typeof fn === "function") {
        let current: unknown = undefined;
        try {
            current = Reflect.apply(fn as (...args: ReadonlyArray<never>) => unknown, target, [output]);
        } catch (error) {
            void error;
            return null;
        }
        if (typeof current !== "object" || current === null) {
            return null;
        }
        const id = readDesktopId(current as object);
        if (id === null) {
            return null;
        }
        return { ref: current as object, id };
    }
    return readSharedDesktop(target);
}

// The connection selects the setter path. An output-specific write never
// falls back to the shared setter, even if the live option changed.
export function writeDesktopForOutput(
    surface: unknown,
    desktop: object,
    output: object,
    nativePerOutput = hasPerOutputDesktopRead(surface),
    optionsSource?: unknown,
    log?: (line: string) => void,
): boolean {
    if (typeof surface !== "object" || surface === null) {
        return false;
    }
    const target = surface as object;
    if (nativePerOutput) {
        const fn = readProp(target, "setCurrentDesktopForScreen");
        const status = perOutputDesktopStatus(target, optionsSource);
        if (status !== "enabled") {
            const reason = status === "disabled" ? "kwin-per-output-off"
                : status === "unreadable" ? "kwin-per-output-unreadable" : "kwin-per-output-unsupported";
            log?.(`omnitiler:world:per-output-unavailable stage=pre-setter reason=${reason}`);
            return false;
        }
        if (typeof fn !== "function") {
            return false;
        }
        try {
            Reflect.apply(fn as (...args: ReadonlyArray<never>) => unknown, target, [desktop, output]);
            return true;
        } catch (error) {
            void error;
            return false;
        }
    }
    // With an unreadable modern option we cannot prove the shared setter
    // will reach all outputs; KWin may switch only the active output.
    const status = perOutputDesktopStatus(target, optionsSource);
    if (status === "enabled" || status === "unreadable") {
        const reason = status === "enabled" ? "kwin-per-output-on" : "kwin-per-output-unreadable";
        log?.(`omnitiler:world:per-output-unavailable stage=pre-setter reason=${reason}`);
        return false;
    }
    try {
        // Reflect.set reports a refused shared write as false; callers
        // read back, so propagate it instead of claiming success.
        return Reflect.set(target, "currentDesktop", desktop);
    } catch (error) {
        void error;
        return false;
    }
}

// Separate workspace modes replicate one native binding per output with a
// distinct logical each; shared keeps one row per native.
export function isSeparateWorkspaceMode(mode: unknown): boolean {
    return mode === "per-output-local" || mode === "global-unique";
}

// Workspace-mode observations follow the negotiated capability, not a
// second live probe or changes to the requested core setting.
export function resolveEffectiveWorkspaceMode(mode: WorkspaceMode, nativePerOutput: boolean): WorkspaceMode {
    return nativePerOutput === true ? mode : "shared";
}

// Strict options gate: a missing, unreadable, or non-boolean value is
// distinct from false for diagnostics but does not authorize selection.
export function readPerOutputVirtualDesktopsFlag(source: unknown): boolean | null {
    if (typeof source !== "object" || source === null) {
        return null;
    }
    const raw = readProp(source, "perOutputVirtualDesktops");
    return typeof raw === "boolean" ? raw : null;
}

export type PerOutputDesktopStatus = "unsupported" | "enabled" | "disabled" | "unreadable";

export function perOutputDesktopStatus(
    surface: unknown, source: unknown,
): PerOutputDesktopStatus {
    if (!hasPerOutputDesktopRead(surface)) {
        return "unsupported";
    }
    const flag = readPerOutputVirtualDesktopsFlag(source);
    return flag === true ? "enabled" : flag === false ? "disabled" : "unreadable";
}

export function supportsPerOutputDesktops(surface: unknown, source?: unknown): boolean {
    return perOutputDesktopStatus(surface, source) === "enabled";
}

export function connectPerOutputDesktopNotify(
    surface: unknown, source: unknown, handler: SignalHandler,
): SignalDetach | null {
    if (!hasPerOutputDesktopRead(surface) || typeof source !== "object" || source === null) {
        return null;
    }
    return connectSignal(readSignal(source, "perOutputVirtualDesktopsChanged"), handler);
}

export function orderedDesktopEntries(entries: ReadonlyArray<DesktopEntry>): ReadonlyArray<DesktopEntry> {
    const indexed = entries.map((entry, index) => ({ entry, index }));
    const allNumbered = indexed.every((item) => item.entry.num !== null);
    if (allNumbered) {
        const sorted = indexed.slice().sort((a, b) => {
            const na = a.entry.num as number;
            const nb = b.entry.num as number;
            if (na !== nb) {
                return na - nb;
            }
            return a.index - b.index;
        });
        return Object.freeze(sorted.map((item) => item.entry));
    }
    return Object.freeze(indexed.map((item) => item.entry));
}
