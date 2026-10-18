// Observation-derived local command context for the world production
// entry: workspace-id references per command, binding retention, the
// structural resize signature plus press counter, cross-output scope,
// and strict-topology target resolution. Lifecycle state stays owned by
// the entry; this only reads through the injected seams.

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

// Strict unique-reciprocal target from the retained FULL topology
// (core-owned; the adapter never emits an adjacency map). Exact
// edge-touch, positive overlap, no wrap; multiple forward touches or
// non-unique reverse touches omit the direction so callers refuse rather
// than guess. Mirrors the frozen oracle in
// tests/output-topology-parity.test.ts and the shared fixture in
// test-fixtures/output-topology-parity.json; do not align split edges
// with ranked migration/window selectors.
export function strictTopologyTarget(
    topology: WorldObservation["topology"],
    sourceOut: string,
    direction: string,
): string | null {
    const names: string[] = [];
    const rects: Array<{ x: number; y: number; w: number; h: number }> = [];
    for (const entry of topology) {
        if (entry.rect === null || entry.rect === undefined) {
            continue;
        }
        names.push(entry.out);
        rects.push(entry.rect);
    }
    const source = names.indexOf(sourceOut);
    if (source < 0) {
        return null;
    }
    const overlapY = (a: { y: number; h: number }, b: { y: number; h: number }): number =>
        Math.min(a.y + a.h, b.y + b.h) - Math.max(a.y, b.y);
    const overlapX = (a: { x: number; w: number }, b: { x: number; w: number }): number =>
        Math.min(a.x + a.w, b.x + b.w) - Math.max(a.x, b.x);
    const touches = (s: number, t: number, dir: string): boolean => {
        const src = rects[s] as { x: number; y: number; w: number; h: number };
        const tgt = rects[t] as { x: number; y: number; w: number; h: number };
        if (dir === "left") {
            return tgt.x + tgt.w === src.x && overlapY(tgt, src) > 0;
        }
        if (dir === "right") {
            return src.x + src.w === tgt.x && overlapY(tgt, src) > 0;
        }
        if (dir === "up") {
            return tgt.y + tgt.h === src.y && overlapX(tgt, src) > 0;
        }
        return src.y + src.h === tgt.y && overlapX(tgt, src) > 0;
    };
    const opposite = (dir: string): string =>
        dir === "left" ? "right" : dir === "right" ? "left" : dir === "up" ? "down" : "up";
    const cands: number[] = [];
    for (let t = 0; t < names.length; t += 1) {
        if (t !== source && touches(source, t, direction)) {
            cands.push(t);
        }
    }
    if (cands.length !== 1) {
        return null;
    }
    const target = cands[0] as number;
    let reverse = 0;
    let reverseHit = -1;
    for (let u = 0; u < names.length; u += 1) {
        if (u !== target && touches(target, u, opposite(direction))) {
            reverse += 1;
            reverseHit = u;
        }
    }
    if (reverse === 1 && reverseHit === source) {
        return names[target] as string;
    }
    return null;
}

export function strictTopologyHasTarget(
    topology: WorldObservation["topology"],
    sourceOut: string,
    direction: string,
): boolean {
    return strictTopologyTarget(topology, sourceOut, direction) !== null;
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
    // focused window's output has a strict unique-reciprocal FULL-topology
    // target there (core-owned; no adapter adjacency map is emitted).
    // The core refuses cross-output requests without a resolvable target,
    // so an unconditional cross flag would refuse every directional action
    // on a single-output host (and at any edgeless side); without a target
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
                    return strictTopologyTarget(obs.topology, domain.id, direction) !== null;
                }
            }
        }
        return false;
    };

    const adjacentTarget = (obs: WorldObservation, out: string, direction: string): { out: string; ws: string } | null => {
        const targetOut = strictTopologyTarget(obs.topology, out, direction);
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
