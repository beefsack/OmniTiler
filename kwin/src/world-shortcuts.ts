// Shortcut catalog registration for the world adapter (D2): every
// workspace/directional/output/migration action routes a Planner world
// command. No synchronous TS fallback: when a command cannot be submitted,
// the shortcut fails closed with one bounded line and native state is
// untouched. Runs once at startup; the entry drives it with submit
// callbacks and native accessors.

import {
    cmdFocusDirectional,
    cmdMoveDirectional,
    cmdSelectNumbered,
    cmdSelectPrevious,
    cmdSelectRelative,
    cmdSelectTrailing,
    cmdSendRelative,
    cmdToggleMaximize,
    cmdToggleWorkspaceTiling,
    implicitSource,
    type WorldCommand,
} from "./world-report";
import { workspaceShortcutCatalog } from "./workspace-native";
import { readNativeAlias } from "./native-alias";

export type WorldShortcutRegisterFn =
    | ((name: string, text: string, sequence: string, callback: () => void) => boolean)
    | undefined;

// Protocol u32 bound for the resize key-repeat index (mirrors the Rust
// `press_index: u32` gate): the counter saturates here instead of wrapping
// or losing precision, so a never-reset streak can never exceed the wire.
export const RESIZE_PRESS_MAX = 4294967295;

// Per-workspace tiling toggle action identity. Registered by
// registerWorldShortcuts below and shared with the native KCM shortcut
// reconciler expectations asserted in native-kcm-static tests.
// User decision 2026-10-10: the toggle binds Meta+Y by default. Every
// other currently-unbound action stays unbound.
export const WORKSPACE_TILING_TOGGLE_ACTION = "omnitiler-toggle-workspace-tiling";
export const WORKSPACE_TILING_TOGGLE_SEQUENCE = "Meta+Y";

// Shared key-repeat press counter: the key is window+direction+mode,
// index 0 is the initial press (12px per the core step schedule), each
// repeat increments by one. A changed structural signature (membership or
// focus, never geometry) resets to 0; the signature reader is the entry's
// fresh observation. Without a signature reader the counter still keys
// correctly but cannot see layout changes (offline-test degraded path only).
export function createResizePressCounter(
    readSignature?: () => string | null,
): (win: string, direction: string, mode: string) => number {
    let lastKey = "";
    let lastSignature: string | null = null;
    let signatureSeen = false;
    let next = 0;
    return (win: string, direction: string, mode: string): number => {
        const key = win + "\u0000" + direction + "\u0000" + mode;
        let signature: string | null = null;
        if (readSignature !== undefined) {
            try {
                signature = readSignature();
            } catch (error) {
                void error;
                signature = null;
            }
        }
        if (signature !== null) {
            if (signatureSeen && signature !== lastSignature) {
                lastKey = "";
                next = 0;
            }
            lastSignature = signature;
            signatureSeen = true;
        }
        if (key === lastKey) {
            const press = next;
            next = next >= RESIZE_PRESS_MAX ? RESIZE_PRESS_MAX : next + 1;
            return press;
        }
        lastKey = key;
        next = 1;
        return 0;
    };
}

export interface WorldShortcutsEnv {
    readProp: (value: object, property: string) => unknown;
    isDirection: (value: unknown) => value is "left" | "right" | "up" | "down";
    shortcutFailed: (action: string) => void;
    submitCommand: (command: WorldCommand, label: string) => boolean;
    submitCatalogMove: (action: string, index: number, follow: boolean) => void;
    submitOutputSend: (direction: string, follow: boolean) => boolean;
    submitMigration: (direction: string) => boolean;
    submitFloat: (label: string) => boolean;
    setStickyNative: (ref: object, id: string, sticky: boolean) => boolean;
    setFullscreenNative: (ref: object, fullscreen: boolean) => boolean;
    activeOutputName: () => string | null;
    activeWindowId: () => string | null;
    activeWindowRef: () => object | null;
    currentScopeId: () => string;
    registerShortcutFn?: WorldShortcutRegisterFn;
    readSameAxisMove?: () => string;
    // Structural membership/focus signature for the resize key-repeat
    // counter (legacy fingerprint parity): the entry supplies its fresh
    // observation signature; absent means key-only counting.
    readResizeSignature?: () => string | null;
}

export function registerWorldShortcuts(env: WorldShortcutsEnv): void {
    const {
        readProp, isDirection, shortcutFailed, submitCommand,
        submitCatalogMove, submitOutputSend, submitMigration, submitFloat,
        setStickyNative, setFullscreenNative,
        activeOutputName, activeWindowId, activeWindowRef,
        currentScopeId,
    } = env;
    const register = env.registerShortcutFn ?? ((() => {
        try {
            const fn: unknown = registerShortcut;
            return typeof fn === "function" ? (fn as NonNullable<WorldShortcutRegisterFn>) : undefined;
        } catch (error) {
            void error;
            return undefined;
        }
    })());
    if (register !== undefined) {
        const tryRegister = (name: string, text: string, sequence: string, callback: () => void): void => {
            try {
                (register as NonNullable<WorldShortcutRegisterFn>)(name, text, sequence, callback);
            } catch (error) {
                void error;
            }
        };
        const outName = (): string | null => activeOutputName();
        const focusedWin = (): string | null => activeWindowId();
        const sameAxis = (): string => {
            try {
                const value = env.readSameAxisMove !== undefined ? env.readSameAxisMove() : "group-with-neighbor";
                return value === "swap-with-neighbor" ? value : "group-with-neighbor";
            } catch (error) {
                void error;
                return "group-with-neighbor";
            }
        };
        const nextPress = createResizePressCounter(env.readResizeSignature);
        // Directional catalog (legacy parity): focus/move on Meta+HJKL plus
        // arrows, resize out/in on Meta+Alt(+Shift) plus arrows. Every action
        // routes a Planner world command; failures close with one bounded
        // line and native state untouched. Output sends and workspace
        // migration extend the catalog with unbound world-only rows.
        const dirs = [
            { direction: "left", key: "H", arrow: "Left" },
            { direction: "down", key: "J", arrow: "Down" },
            { direction: "up", key: "K", arrow: "Up" },
            { direction: "right", key: "L", arrow: "Right" },
        ] as const;
        const submitFocus = (d: string): void => {
            // Command-only: every subject (tiled, floating, sticky) routes
            // the direction-only Planner command; the core owns the
            // float-focus lattice, crossing (left/right) and local fallback.
            const w = focusedWin();
            if (!isDirection(d) || w === null) {
                shortcutFailed(`focus-${d}`);
                return;
            }
            const cmd = cmdFocusDirectional(w, d);
            if (cmd === null || !submitCommand(cmd, `focus-${d}`)) {
                shortcutFailed(`focus-${d}`);
            }
        };
        const submitMove = (d: string): void => {
            // Command-only: every subject routes the direction-only Planner
            // command; the core owns float half-snap geometry, crossing
            // (all four directions) and local R1-R3 fallback.
            const w = focusedWin();
            if (w === null) {
                shortcutFailed(`move-${d}`);
                return;
            }
            // Same cross-output scope as focus: crossing applies only with
            // a strict FULL-topology target there, else the move stays local.
            if (!isDirection(d)) {
                shortcutFailed(`move-${d}`);
                return;
            }
            const cmd = cmdMoveDirectional(w, d, sameAxis());
            if (cmd === null || !submitCommand(cmd, `move-${d}`)) {
                shortcutFailed(`move-${d}`);
            }
        };
        const submitResize = (d: string, mode: string): void => {
            const w = focusedWin();
            if (w === null || !isDirection(d) || (mode !== "inwards" && mode !== "outwards")) {
                shortcutFailed(`resize-${mode}-${d}`);
                return;
            }
            if (!submitCommand({ op: "operation", command: { op: "resize", win: w, direction: d, mode, press: nextPress(w, d, mode) } }, `resize-${mode}-${d}`)) {
                shortcutFailed(`resize-${mode}-${d}`);
            }
        };
        for (const { direction: d, key, arrow } of dirs) {
            tryRegister(`omnitiler-focus-${d}`, `Focus window ${d}`, `Meta+${key}`, () => submitFocus(d));
            tryRegister(`omnitiler-focus-${d}-arrow`, `Focus window ${d}`, `Meta+${arrow}`, () => submitFocus(d));
            tryRegister(`omnitiler-move-${d}`, `Move window ${d}`, `Meta+Shift+${key}`, () => submitMove(d));
            tryRegister(`omnitiler-move-${d}-arrow`, `Move window ${d}`, `Meta+Shift+${arrow}`, () => submitMove(d));
            tryRegister(`omnitiler-resize-outwards-${d}`, `Grow window towards ${d}`, `Meta+Alt+${key}`, () => submitResize(d, "outwards"));
            tryRegister(`omnitiler-resize-outwards-${d}-arrow`, `Grow window towards ${d}`, `Meta+Alt+${arrow}`, () => submitResize(d, "outwards"));
            tryRegister(`omnitiler-resize-inwards-${d}`, `Shrink window from ${d}`, `Meta+Alt+Shift+${key}`, () => submitResize(d, "inwards"));
            tryRegister(`omnitiler-resize-inwards-${d}-arrow`, `Shrink window from ${d}`, `Meta+Alt+Shift+${arrow}`, () => submitResize(d, "inwards"));
            tryRegister(`omnitiler-send-output-${d}`, `Send window to output ${d}`, "", () => {
                if (!submitOutputSend(d, true)) {
                    shortcutFailed(`send-output-${d}`);
                }
            });
            tryRegister(`omnitiler-send-output-${d}-stay`, `Send window to output ${d} without following`, "", () => {
                if (!submitOutputSend(d, false)) {
                    shortcutFailed(`send-output-${d}-stay`);
                }
            });
            tryRegister(`omnitiler-migrate-workspace-${d}`, `Migrate workspace ${d}`, "", () => {
                if (!submitMigration(d)) {
                    shortcutFailed(`migrate-${d}`);
                }
            });
        }
        // Window-state toggles (legacy parity): float/maximize/orientation
        // route the same handlers as the facade. Sticky/fullscreen are
        // synchronous native toggles with pre-setter safety rechecks;
        // float/maximize/orientation are world commands.
        tryRegister("omnitiler-toggle-float", "Toggle floating window", "Meta+G", () => {
            submitFloat("toggle-float");
        });
        tryRegister("omnitiler-toggle-sticky", "Toggle sticky floating window", "Meta+Shift+G", () => {
            const id = activeWindowId();
            const ref = activeWindowRef();
            const stickyRaw = ref !== null ? readProp(ref, "onAllDesktops") : undefined;
            const sticky = typeof stickyRaw === "boolean" ? stickyRaw : ref !== null ? readNativeAlias(readProp, ref, "sticky") : undefined;
            if (id === null || ref === null || !setStickyNative(ref, id, sticky !== true)) {
                shortcutFailed("toggle-sticky");
            }
        });
        tryRegister("omnitiler-toggle-maximize", "Toggle maximize window", "Meta+M", () => {
            const w = focusedWin();
            if (w === null || !submitCommand(cmdToggleMaximize(w), "toggle-maximize")) {
                shortcutFailed("toggle-maximize");
            }
        });
        tryRegister("omnitiler-toggle-fullscreen", "Toggle fullscreen window", "Meta+F11", () => {
            const ref = activeWindowRef();
            const fsRaw = ref !== null ? readProp(ref, "fullScreen") : undefined;
            const fullscreen = typeof fsRaw === "boolean" ? fsRaw : ref !== null ? readNativeAlias(readProp, ref, "fullscreen") === true : false;
            if (ref === null || !setFullscreenNative(ref, !fullscreen)) {
                shortcutFailed("toggle-fullscreen");
            }
        });
        tryRegister("omnitiler-toggle-orientation", "Toggle split orientation", "Meta+O", () => {
            const w = focusedWin();
            if (w === null || !submitCommand({ op: "operation", command: { op: "toggle-orientation", win: w } }, "toggle-orientation")) {
                shortcutFailed("toggle-orientation");
            }
        });
        // Workspace catalog: Planner resolves every position locally
        // forbidden: submit the semantic command with the active output.
        for (const row of workspaceShortcutCatalog()) {
            const kind = row.kind;
            if (kind === "select") {
                tryRegister(row.action, row.text, row.sequence, () => {
                    // Neutral selection: core resolves the output from the
                    // cursor-first switch target (no cached active-output
                    // preselection on the wire).
                    const cmd = row.index === 0 ? cmdSelectTrailing(null) : cmdSelectNumbered(null, row.index);
                    if (cmd === null || !submitCommand(cmd, row.action)) {
                        shortcutFailed(row.action);
                    }
                });
            } else if (kind === "move") {
                tryRegister(row.action, row.text, row.sequence, () => {
                    submitCatalogMove(row.action, row.index, true);
                });
            } else if (kind === "move-stay") {
                tryRegister(row.action, row.text, row.sequence, () => {
                    submitCatalogMove(row.action, row.index, false);
                });
            } else if (kind === "previous") {
                tryRegister(row.action, row.text, row.sequence, () => {
                    if (!submitCommand(cmdSelectPrevious(null), row.action)) {
                        shortcutFailed(row.action);
                    }
                });
            } else if (kind === "relative") {
                const delta = row.delta ?? 1;
                tryRegister(row.action, row.text, row.sequence, () => {
                    const cmd = delta !== -1 && delta !== 1 ? null : cmdSelectRelative(null, delta);
                    if (cmd === null || !submitCommand(cmd, row.action)) {
                        shortcutFailed(row.action);
                    }
                });
            } else if (kind === "send-relative" || kind === "send-relative-stay") {
                const delta = row.delta ?? 1;
                const follow = kind === "send-relative";
                tryRegister(row.action, row.text, row.sequence, () => {
                    const w = focusedWin();
                    const cmd = w === null ? null : cmdSendRelative(w, implicitSource("home-then-cached-live"), delta, follow);
                    if (cmd === null || !submitCommand(cmd, row.action)) {
                        shortcutFailed(row.action);
                    }
                });
            }
        }
        tryRegister(WORKSPACE_TILING_TOGGLE_ACTION, "Toggle tiling for current workspace", WORKSPACE_TILING_TOGGLE_SEQUENCE, () => {
            // Planner-owned tiling toggle (D2): the neutral toggle intent
            // carries no adapter-preselected mode. Core flips the retained
            // mode at execution time, so racing toggles compose identically.
            // Failures fail closed with one bounded line and no native writes.
            const out = outName();
            const scope = currentScopeId();
            if (out === null || out === "" || scope === "") {
                shortcutFailed("toggle-workspace-tiling");
                return;
            }
            if (!submitCommand(cmdToggleWorkspaceTiling(out, scope), "toggle-workspace-tiling")) {
                shortcutFailed("toggle-workspace-tiling");
            }
        });
    }
}
