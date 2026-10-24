// Observation-derived local command context for the world production
// entry: workspace-id references per command, binding retention, and the
// structural resize signature plus press counter. No topology selection
// lives here: the core owns output selection from retained FULL rects,
// so this module never derives targets or cross-output scope. Lifecycle
// state stays owned by the entry; this only reads through the injected
// seams.

import { createResizePressCounter } from "./world-shortcuts";
import { isOpaqueId } from "./world-seq";
import type { WorldCommand } from "./world-report";

export interface WorldCommandContextDeps {
    readonly observe: () => import("./world-observe").WorldObservation | null;
    readonly isDirection: (value: unknown) => value is "left" | "right" | "up" | "down";
    readonly isCreationBound: (nativeId: string) => boolean;
    readonly activeCommands: () => ReadonlyArray<WorldCommand>;
}

export interface WorldCommandContext {
    readonly commandWorkspaceRefs: (command: WorldCommand | null) => Set<string>;
    readonly pinRetained: (nativeId: string) => boolean;
    readonly observeForCommand: () => import("./world-observe").WorldObservation | null;
    readonly resizeSignature: () => string | null;
    readonly nextResizePress: (win: string, direction: string, mode: string) => number;
}

export function createWorldCommandContext(
    deps: WorldCommandContextDeps,
): WorldCommandContext {
    // Native workspace ids referenced by one world command: direct workspace
    // targets plus operation inner targets/sources. Positional commands
    // (numbered/relative/trailing) resolve in core and name no native.
    // Direction-only intents name no target: focus/move/send-directional
    // carry only window+direction, migrate-directional carries nothing (the
    // core resolves the action source from the neutral report facts).
    const commandWorkspaceRefs = (command: WorldCommand | null): Set<string> => {
        const refs = new Set<string>();
        if (command === null) {
            return refs;
        }
        const rec = command as unknown as Record<string, unknown>;
        const take = (value: unknown): void => {
            if (isOpaqueId(value)) {
                refs.add(value as string);
            }
        };
        const op = rec["op"];
        if (op === "send" || op === "switch-view" || op === "select-workspace") {
            take(rec["ws"]);
            take(rec["workspace"]);
        } else if (op === "migrate-workspace") {
            take(rec["ws"]);
        } else if (op === "operation") {
            const inner = rec["command"];
            if (typeof inner === "object" && inner !== null) {
                const m = inner as Record<string, unknown>;
                take(m["ws"]);
                take(m["target_workspace"]);
                const src = m["source"];
                if (typeof src === "object" && src !== null) {
                    take((src as Record<string, unknown>)["ws"]);
                }
            }
        }
        return refs;
    };

    // Adapter-asserted binding retention for the core trailing policy
    // (core retains pinned/displaced natives against cleanup): true while
    // the native id carries adapter-retained intent, an outstanding
    // creation binding or a workspace referenced by the dispatched
    // command. Hotplug displacement state lives in core (WorldEngine
    // displaced units, published as ordinary rows under the survivor),
    // never in the adapter: the adapter reports no per-workspace
    // displacement flag of its own, so `disp` stays false here and live
    // membership is pruned post-accept as before.
    const pinRetained = (nativeId: string): boolean => {
        if (deps.isCreationBound(nativeId)) {
            return true;
        }
        for (const cmd of deps.activeCommands()) {
            if (commandWorkspaceRefs(cmd).has(nativeId)) {
                return true;
            }
        }
        return false;
    };

    const observeForCommand = (): import("./world-observe").WorldObservation | null => deps.observe();

    // Structural membership/focus signature for the resize key-repeat
    // counter (legacy `planFingerprint` parity: domain-scoped ids plus
    // focus, never geometry). A confirmed layout change resets the press
    // index to 0; pure share changes from resizes keep it stepping.
    const resizeSignature = (): string | null => {
        let obs: import("./world-observe").WorldObservation | null = null;
        try {
            obs = observeForCommand();
        } catch (error) {
            void error;
            return null;
        }
        if (obs === null) {
            return null;
        }
        const ids: string[] = [];
        for (const domain of obs.domains) {
            if (!domain.readable) {
                continue;
            }
            for (const win of domain.windows) {
                ids.push(domain.id + "\u0000" + domain.ws + "\u0000" + win.win);
            }
        }
        ids.sort();
        return obs.focusedGlobal + "\n" + ids.join("\n");
    };
    const nextResizePress = createResizePressCounter(resizeSignature);

    return {
        commandWorkspaceRefs,
        pinRetained,
        observeForCommand,
        resizeSignature,
        nextResizePress,
    };
}
