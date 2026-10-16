// Shortcut catalog registration for the world adapter (D2): every
// workspace/directional/output/migration action routes a Planner world
// command. No synchronous TS fallback: when a command cannot be submitted,
// the shortcut fails closed with one bounded line and native state is
// untouched. Runs once at startup; the entry drives it with submit
// callbacks and native accessors.

import {
    cmdSelectNumbered,
    cmdSelectPrevious,
    cmdSelectRelative,
    cmdSelectTrailing,
    cmdSendRelative,
    cmdSetWorkspaceTiling,
    type WorldCommand,
} from "./world-report";
import { workspaceShortcutCatalog } from "./workspace-native";

export type WorldShortcutRegisterFn =
    | ((name: string, text: string, sequence: string, callback: () => void) => boolean)
    | undefined;

export interface WorldShortcutsEnv {
    readProp: (value: object, property: string) => unknown;
    isDirection: (value: unknown) => value is "left" | "right" | "up" | "down";
    shortcutFailed: (action: string) => void;
    submitCommand: (command: WorldCommand, label: string) => boolean;
    handleFloatFocus: (direction: string) => "not-float" | "handled" | "failed";
    handleFloatMove: (direction: string) => "not-float" | "handled" | "failed";
    submitCatalogMove: (action: string, index: number, follow: boolean) => void;
    submitOutputSend: (direction: string, follow: boolean) => boolean;
    submitMigration: (direction: string) => boolean;
    submitFloat: (label: string) => boolean;
    setStickyNative: (ref: object, id: string, sticky: boolean) => boolean;
    setMaximizedNative: (ref: object, maximized: boolean) => boolean;
    setFullscreenNative: (ref: object, fullscreen: boolean) => boolean;
    activeOutputName: () => string | null;
    activeWindowId: () => string | null;
    activeWindowRef: () => object | null;
    currentScopeId: () => string;
    getWorkspaceTilingSnapshot: () => { scope: string; tiled: boolean; defaultTiled: boolean };
    registerShortcutFn?: WorldShortcutRegisterFn;
}

export function registerWorldShortcuts(env: WorldShortcutsEnv): void {
    const {
        readProp, isDirection, shortcutFailed, submitCommand,
        handleFloatFocus, handleFloatMove,
        submitCatalogMove, submitOutputSend, submitMigration, submitFloat,
        setStickyNative, setMaximizedNative, setFullscreenNative,
        activeOutputName, activeWindowId, activeWindowRef,
        currentScopeId, getWorkspaceTilingSnapshot,
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
            // Float-origin first (legacy parity, native): a floating/sticky
            // subject focuses natively or falls back with floatSubject true.
            // Tiled subjects route the Planner command below.
            const floatOutcome = handleFloatFocus(d);
            if (floatOutcome === "handled") {
                return;
            }
            if (floatOutcome === "failed") {
                shortcutFailed(`focus-${d}`);
                return;
            }
            const w = focusedWin();
            // Legacy axis rule: left/right may cross outputs, up/down
            // stay local single-domain. Core resolves the pair from the
            // carried adjacency; without adjacency it refuses.
            const cross = d === "left" || d === "right";
            if (!isDirection(d) || w === null || !submitCommand({ op: "operation", command: { op: "focus", win: w, direction: d, crossOutput: cross, floatSubject: false } }, `focus-${d}`)) {
                shortcutFailed(`focus-${d}`);
            }
        };
        const submitMove = (d: string): void => {
            // Float-origin moves half-snap natively (no Planner path for
            // float moves); tiled subjects route the Planner command below.
            const floatOutcome = handleFloatMove(d);
            if (floatOutcome === "handled") {
                return;
            }
            if (floatOutcome === "failed") {
                shortcutFailed(`move-${d}`);
                return;
            }
            const w = focusedWin();
            if (w === null) {
                shortcutFailed(`move-${d}`);
                return;
            }
            if (!isDirection(d) || !submitCommand({ op: "operation", command: { op: "move", win: w, direction: d, crossOutput: true, sameAxis: "group-with-neighbor" } }, `move-${d}`)) {
                shortcutFailed(`move-${d}`);
            }
        };
        const submitResize = (d: string, mode: string): void => {
            const w = focusedWin();
            if (w === null || !isDirection(d) || (mode !== "inwards" && mode !== "outwards")) {
                shortcutFailed(`resize-${mode}-${d}`);
                return;
            }
            if (!submitCommand({ op: "operation", command: { op: "resize", win: w, direction: d, mode, press: 1 } }, `resize-${mode}-${d}`)) {
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
        // Window-state toggles (legacy parity): float/sticky/maximize/
        // fullscreen/orientation route the same handlers as the facade.
        // Sticky/maximize/fullscreen are synchronous native toggles with
        // pre-setter safety rechecks; float/orientation are world commands.
        tryRegister("omnitiler-toggle-float", "Toggle floating window", "Meta+G", () => {
            submitFloat("toggle-float");
        });
        tryRegister("omnitiler-toggle-sticky", "Toggle sticky floating window", "Meta+Shift+G", () => {
            const id = activeWindowId();
            const ref = activeWindowRef();
            const stickyRaw = ref !== null ? readProp(ref, "onAllDesktops") : undefined;
            const sticky = typeof stickyRaw === "boolean" ? stickyRaw : ref !== null ? readProp(ref, "sticky") : undefined;
            if (id === null || ref === null || !setStickyNative(ref, id, sticky !== true)) {
                shortcutFailed("toggle-sticky");
            }
        });
        tryRegister("omnitiler-toggle-maximize", "Toggle maximize window", "Meta+M", () => {
            const ref = activeWindowRef();
            const mode = ref !== null ? readProp(ref, "maximizeMode") : undefined;
            const maximized = typeof mode === "number" ? mode !== 0 : ref !== null ? readProp(ref, "maximized") === true : false;
            if (ref === null || !setMaximizedNative(ref, !maximized)) {
                shortcutFailed("toggle-maximize");
            }
        });
        tryRegister("omnitiler-toggle-fullscreen", "Toggle fullscreen window", "Meta+F11", () => {
            const ref = activeWindowRef();
            const fsRaw = ref !== null ? readProp(ref, "fullScreen") : undefined;
            const fullscreen = typeof fsRaw === "boolean" ? fsRaw : ref !== null ? readProp(ref, "fullscreen") === true : false;
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
                    const out = outName();
                    if (out === null) {
                        shortcutFailed(row.action);
                        return;
                    }
                    const cmd = row.index === 0 ? cmdSelectTrailing(out) : cmdSelectNumbered(out, row.index);
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
                    const out = outName();
                    if (out === null || !submitCommand(cmdSelectPrevious(out), row.action)) {
                        shortcutFailed(row.action);
                    }
                });
            } else if (kind === "relative") {
                const delta = row.delta ?? 1;
                tryRegister(row.action, row.text, row.sequence, () => {
                    const out = outName();
                    const cmd = out === null ? null : cmdSelectRelative(out, delta);
                    if (cmd === null || !submitCommand(cmd, row.action)) {
                        shortcutFailed(row.action);
                    }
                });
            } else if (kind === "send-relative" || kind === "send-relative-stay") {
                const delta = row.delta ?? 1;
                const follow = kind === "send-relative";
                tryRegister(row.action, row.text, row.sequence, () => {
                    const out = outName();
                    const w = focusedWin();
                    const cmd = out === null || w === null ? null : cmdSendRelative(w, out, delta, follow);
                    if (cmd === null || !submitCommand(cmd, row.action)) {
                        shortcutFailed(row.action);
                    }
                });
            }
        }
        tryRegister("omnitiler-toggle-workspace-tiling", "Toggle tiling for current workspace", "Meta+Y", () => {
            // Planner-owned tiling toggle (D2): the source mode is the last
            // ready reply's tiling mode for the current scope, never TS
            // policy. Legacy Meta+Y/tray behavior preserved: toggle and wait
            // for the next accepted reply; failures fail closed with one
            // bounded line and no native writes.
            const out = outName();
            const scope = currentScopeId();
            if (out === null || out === "" || scope === "") {
                shortcutFailed("toggle-workspace-tiling");
                return;
            }
            const snapshot = getWorkspaceTilingSnapshot();
            if (snapshot.scope === "" || snapshot.scope !== scope) {
                shortcutFailed("toggle-workspace-tiling");
                return;
            }
            if (!submitCommand(cmdSetWorkspaceTiling(out, scope, !snapshot.tiled), "toggle-workspace-tiling")) {
                shortcutFailed("toggle-workspace-tiling");
            }
        });
    }
}
