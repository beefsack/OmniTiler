// Float-origin directional focus/move (native only, moved verbatim from the
// production entry): a floating/sticky active window focuses the nearest
// same-domain float/sticky candidate by the top-left axis rule with exactly
// one native activeWindow write (no Planner dispatch, no geometry writes),
// and half-snaps with exactly one native frameGeometry write plus focus
// retention (no dispatch). Left/Right focus misses with adjacency fall back
// to the Planner cross-output remembered-tiled target via a full-state
// operation command with floatSubject true; Up/Down misses and misses
// without adjacency retain with no writes. Planner owns all layout; this
// module never mirrors workspace policy. Game safety: never auto-focus a
// fullscreen target; fullscreen/maximized subjects never move.

import type { WorldObservation } from "./world-observe";
import type { WorldCommand } from "./world-report";
import { isLiveWindowListed, liveWindowId } from "./world-native";

export type FloatDirection = "left" | "right" | "up" | "down";

type ObservedFloatWin = WorldObservation["domains"][number]["windows"][number];

// Minimal injected context: the production entry owns lifecycle state,
// flight ordering, and transport. Busy is one boolean owned by the entry;
// observation and setters arrive as callbacks, never copied state.
export interface FloatNavEnv {
    readonly surface: object;
    readonly readProp: (value: object, property: string) => unknown;
    readonly log: (message: string) => void;
    readonly isBusy: () => boolean;
    readonly activeWindowId: () => string | null;
    readonly observe: () => WorldObservation | null;
    readonly liveRefById: (id: string) => object | null;
    readonly submitCommand: (command: WorldCommand, label: string) => boolean;
    readonly invalidate: () => void;
    readonly scheduleDispatch: () => void;
}

export interface FloatNav {
    readonly handleFloatFocus: (direction: string) => "not-float" | "handled" | "failed";
    readonly handleFloatMove: (direction: string) => "not-float" | "handled" | "failed";
}

function isFloatDirection(value: unknown): value is FloatDirection {
    return value === "left" || value === "right" || value === "up" || value === "down";
}

export function findObservedWin(obs: WorldObservation, id: string): ObservedFloatWin | null {
    for (const domain of obs.domains) {
        for (const w of domain.windows) {
            if (w.win === id) {
                return w;
            }
        }
    }
    return null;
}

export function selectLocalFloatTarget(
    obs: WorldObservation,
    subject: { win: string; out: string; ws: string; rect: { x: number; y: number } },
    direction: FloatDirection,
): ObservedFloatWin | null {
    const vertical = direction === "up" || direction === "down";
    const negative = direction === "up" || direction === "left";
    const sticky: ObservedFloatWin[] = [];
    const ordinary: ObservedFloatWin[] = [];
    for (const domain of obs.domains) {
        for (const entry of domain.windows) {
            if (entry.win === subject.win || entry.out !== subject.out || entry.ws !== subject.ws) {
                continue;
            }
            if (entry.floating !== true && entry.sticky !== true) {
                continue;
            }
            if (entry.sticky === true) {
                sticky.push(entry);
            } else {
                ordinary.push(entry);
            }
        }
    }
    let best: ObservedFloatWin | null = null;
    let bestDist = 0;
    const consider = (entry: ObservedFloatWin): void => {
        const delta = vertical ? entry.rect.y - subject.rect.y : entry.rect.x - subject.rect.x;
        if (negative) {
            if (delta > 0) {
                return;
            }
        } else if (delta <= 0) {
            return;
        }
        const dist = delta < 0 ? -delta : delta;
        if (best === null || dist < bestDist) {
            best = entry;
            bestDist = dist;
        } else if (dist === bestDist && !negative) {
            best = entry;
        }
    };
    for (const entry of sticky) {
        consider(entry);
    }
    for (const entry of ordinary) {
        consider(entry);
    }
    return best;
}

export function floatHalfSnapRect(
    bounds: { x: number; y: number; w: number; h: number },
    gapInner: number,
    direction: FloatDirection,
): { x: number; y: number; w: number; h: number } | null {
    if (!Number.isInteger(gapInner) || gapInner < 0 || gapInner > 64) {
        return null;
    }
    if (bounds.w <= 0 || bounds.h <= 0) {
        return null;
    }
    const inner = gapInner;
    const halfW = Math.trunc(bounds.w / 2);
    const halfH = Math.trunc(bounds.h / 2);
    const halfInner = Math.trunc(inner / 2);
    const threeHalfInner = Math.trunc((3 * inner) / 2);
    let rect: { x: number; y: number; w: number; h: number };
    if (direction === "left") {
        rect = { x: bounds.x + inner, y: bounds.y + inner, w: halfW - threeHalfInner, h: bounds.h - 2 * inner };
    } else if (direction === "right") {
        rect = { x: bounds.x + halfW + halfInner, y: bounds.y + inner, w: halfW - threeHalfInner, h: bounds.h - 2 * inner };
    } else if (direction === "up") {
        rect = { x: bounds.x + inner, y: bounds.y + inner, w: bounds.w - 2 * inner, h: halfH - threeHalfInner };
    } else {
        rect = { x: bounds.x + inner, y: bounds.y + halfH + halfInner, w: bounds.w - 2 * inner, h: halfH - threeHalfInner };
    }
    if (rect.w <= 0 || rect.h <= 0) {
        return null;
    }
    return rect;
}

export function createFloatNav(env: FloatNavEnv): FloatNav {
    const { surface, readProp, log } = env;
    const isStillListedRef = (ref: object): boolean => {
        // Stable-id liveness: the scripting engine renews wrappers per
        // enumeration, so ref identity never survives a call.
        return isLiveWindowListed(surface, readProp, ref);
    };
    const handleFloatFocus = (direction: string): "not-float" | "handled" | "failed" => {
        if (!isFloatDirection(direction)) {
            return "failed";
        }
        if (env.isBusy()) {
            log("omnitiler:world:busy-refused kind=focus");
            return "failed";
        }
        const w = env.activeWindowId();
        if (w === null) {
            return "failed";
        }
        const obs = env.observe();
        if (obs === null) {
            return "failed";
        }
        const subject = findObservedWin(obs, w);
        if (subject === null || (subject.floating !== true && subject.sticky !== true)) {
            return "not-float";
        }
        if (subject.maximized && !subject.fullscreen) {
            log("omnitiler:world:focus-refused-maximize");
            return "failed";
        }
        const target = selectLocalFloatTarget(obs, subject, direction);
        if (target !== null) {
            // Native safety: never auto-focus a fullscreen (game/overlay)
            // target; fail closed with no writes.
            if (target.fullscreen) {
                log(`omnitiler:world:focus-float-refused-target direction=${direction}`);
                return "failed";
            }
            try {
                const active = readProp(surface, "activeWindow");
                const want = liveWindowId(readProp, subject.ref);
                if (typeof active !== "object" || active === null || want === null || liveWindowId(readProp, active) !== want) {
                    log(`omnitiler:world:focus-float-refused-stale direction=${direction}`);
                    return "failed";
                }
            } catch (error) {
                void error;
                return "failed";
            }
            if (!isStillListedRef(target.ref) || env.liveRefById(target.win) === null) {
                log(`omnitiler:world:focus-float-refused-stale direction=${direction}`);
                return "failed";
            }
            let ok = false;
            try {
                ok = Reflect.set(surface, "activeWindow", target.ref) || false;
            } catch (error) {
                void error;
                ok = false;
            }
            if (!ok) {
                log(`omnitiler:world:focus-float-write-failed direction=${direction}`);
                return "failed";
            }
            log(`omnitiler:world:focus-float-applied direction=${direction}`);
            env.invalidate();
            env.scheduleDispatch();
            return "handled";
        }
        if (direction === "up" || direction === "down") {
            log(`omnitiler:world:focus-float-retained direction=${direction} reason=no-target`);
            return "failed";
        }
        const domain = obs.domains.find((d) => d.id === subject.out && d.ws === subject.ws)
            ?? obs.domains.find((d) => d.id === subject.out);
        if (domain === undefined || (domain.adjacent[direction] ?? null) === null) {
            log(`omnitiler:world:focus-float-retained direction=${direction} reason=no-target`);
            return "failed";
        }
        const ok = env.submitCommand(
            { op: "operation", command: { op: "focus", win: w, direction, crossOutput: true, floatSubject: true } },
            `focus-${direction}`,
        );
        return ok ? "handled" : "failed";
    };
    const handleFloatMove = (direction: string): "not-float" | "handled" | "failed" => {
        if (!isFloatDirection(direction)) {
            return "failed";
        }
        if (env.isBusy()) {
            log("omnitiler:world:busy-refused kind=move");
            return "failed";
        }
        const w = env.activeWindowId();
        if (w === null) {
            return "failed";
        }
        const obs = env.observe();
        if (obs === null) {
            return "failed";
        }
        const subject = findObservedWin(obs, w);
        if (subject === null || (subject.floating !== true && subject.sticky !== true)) {
            return "not-float";
        }
        if (subject.fullscreen) {
            log("omnitiler:world:move-refused-fullscreen");
            return "failed";
        }
        if (subject.maximized) {
            log("omnitiler:world:move-refused-maximize");
            return "failed";
        }
        if (obs.gesture) {
            log(`omnitiler:world:move-float-refused-interactive direction=${direction}`);
            return "failed";
        }
        const domain = obs.domains.find((d) => d.id === subject.out && d.ws === subject.ws)
            ?? obs.domains.find((d) => d.id === subject.out);
        if (domain === undefined) {
            return "failed";
        }
        const rect = floatHalfSnapRect(domain.bounds, domain.gap, direction);
        if (rect === null) {
            log(`omnitiler:world:move-float-refused-invalid-geometry direction=${direction}`);
            return "failed";
        }
        const hints = subject.hints;
        if (hints.minW !== null && hints.minW > rect.w) {
            log(`omnitiler:world:move-float-refused-constraints direction=${direction}`);
            return "failed";
        }
        if (hints.minH !== null && hints.minH > rect.h) {
            log(`omnitiler:world:move-float-refused-constraints direction=${direction}`);
            return "failed";
        }
        if (hints.maxW !== null && hints.maxW > 0 && hints.maxW < rect.w) {
            log(`omnitiler:world:move-float-refused-constraints direction=${direction}`);
            return "failed";
        }
        if (hints.maxH !== null && hints.maxH > 0 && hints.maxH < rect.h) {
            log(`omnitiler:world:move-float-refused-constraints direction=${direction}`);
            return "failed";
        }
        try {
            const moveActive = readProp(surface, "activeWindow");
            const moveWant = liveWindowId(readProp, subject.ref);
            if (typeof moveActive !== "object" || moveActive === null || moveWant === null || liveWindowId(readProp, moveActive) !== moveWant) {
                log(`omnitiler:world:move-float-refused-stale direction=${direction}`);
                return "failed";
            }
        } catch (error) {
            void error;
            return "failed";
        }
        if (!isStillListedRef(subject.ref)) {
            log(`omnitiler:world:move-float-refused-stale direction=${direction}`);
            return "failed";
        }
        let written = false;
        try {
            written = Reflect.set(subject.ref, "frameGeometry", { x: rect.x, y: rect.y, width: rect.w, height: rect.h }) || false;
        } catch (error) {
            void error;
            written = false;
        }
        if (!written) {
            log(`omnitiler:world:move-float-write-failed direction=${direction}`);
            return "failed";
        }
        try {
            const focusActive = readProp(surface, "activeWindow");
            const focusWant = liveWindowId(readProp, subject.ref);
            if (typeof focusActive !== "object" || focusActive === null || focusWant === null || liveWindowId(readProp, focusActive) !== focusWant) {
                if (!(Reflect.set(surface, "activeWindow", subject.ref) || false)) {
                    log(`omnitiler:world:move-float-focus-failed direction=${direction}`);
                    return "failed";
                }
            }
        } catch (error) {
            void error;
            return "failed";
        }
        log(`omnitiler:world:move-float-applied direction=${direction}`);
        env.invalidate();
        env.scheduleDispatch();
        return "handled";
    };
    return { handleFloatFocus, handleFloatMove };
}
