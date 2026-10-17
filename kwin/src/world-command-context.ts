// Observation-derived local command context for the world production
// entry: workspace-id references per command, binding retention, the
// structural resize signature plus press counter, cross-output scope,
// and adjacent-target resolution. Exactly the entry's previous closures,
// relocated without behavior change. Lifecycle state stays owned by the
// entry; this only reads through the injected seams.

import { createResizePressCounter } from "./world-shortcuts";
import { isOpaqueId } from "./world-seq";
import type { WorldCommand } from "./world-report";
import type { WorldObservation } from "./world-observe";

export interface WorldCommandContextDeps {
    readonly observe: () => WorldObservation | null;
    readonly activeWindowId: () => string | null;
    readonly isDirection: (value: unknown) => value is "left" | "right" | "up" | "down";
    readonly isCreationBound: (nativeId: string) => boolean;
    readonly activeCommand: () => WorldCommand | null;
}

export interface WorldCommandContext {
    readonly commandWorkspaceRefs: (command: WorldCommand | null) => Set<string>;
    readonly pinRetained: (nativeId: string) => boolean;
    readonly observeForCommand: () => WorldObservation | null;
    readonly resizeSignature: () => string | null;
    readonly nextResizePress: (win: string, direction: string, mode: string) => number;
    readonly crossForDirection: (direction: string) => boolean;
    readonly adjacentTarget: (
        obs: WorldObservation,
        out: string,
        direction: string,
    ) => { out: string; ws: string } | null;
}

export function createWorldCommandContext(
    deps: WorldCommandContextDeps,
): WorldCommandContext {
    // Native workspace ids referenced by one world command: direct workspace
    // targets plus operation inner targets/sources. Positional commands
    // (numbered/relative/trailing) resolve in core and name no native.
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
        const cmd = deps.activeCommand();
        if (cmd !== null && commandWorkspaceRefs(cmd).has(nativeId)) {
            return true;
        }
        return false;
    };

    const observeForCommand = (): WorldObservation | null => deps.observe();

    // Structural membership/focus signature for the resize key-repeat
    // counter (legacy `planFingerprint` parity: domain-scoped ids plus
    // focus, never geometry). A confirmed layout change resets the press
    // index to 0; pure share changes from resizes keep it stepping.
    const resizeSignature = (): string | null => {
        let obs: WorldObservation | null = null;
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

    // Cross-output scope for one press direction: true only when the
    // focused window's live domain reports an adjacent output there.
    // The core refuses cross-output requests without adjacency, so an
    // unconditional cross flag would refuse every directional action on
    // a single-output host (and at any edgeless side); without adjacency
    // the command stays local single-domain. Unreadable state fails
    // closed to local scope.
    const crossForDirection = (direction: string): boolean => {
        if (!deps.isDirection(direction)) {
            return false;
        }
        const w = deps.activeWindowId();
        const obs = observeForCommand();
        if (w === null || obs === null) {
            return false;
        }
        for (const domain of obs.domains) {
            for (const win of domain.windows) {
                if (win.win === w) {
                    return domain.adjacent[direction] !== undefined;
                }
            }
        }
        return false;
    };

    const adjacentTarget = (obs: WorldObservation, out: string, direction: string): { out: string; ws: string } | null => {
        const domain = obs.domains.find((d) => d.id === out);
        const targetOut = domain?.adjacent[direction] ?? null;
        if (targetOut === null) {
            return null;
        }
        const topo = obs.topology.find((t) => t.out === targetOut);
        if (topo === undefined || topo.current === "") {
            return null;
        }
        return { out: targetOut, ws: topo.current };
    };

    return {
        commandWorkspaceRefs,
        pinRetained,
        observeForCommand,
        resizeSignature,
        nextResizePress,
        crossForDirection,
        adjacentTarget,
    };
}
