// Bounded born-maximized admission setting: validated KWin script startup configuration.
//
// R-MAX-06 global KDE `bornMaximized` with `tile-on-launch` default and
// `keep-maximized-overlay` alternative. Exact tokens match Windows
// `core.born_maximized` (`tile-on-launch`, `keep-maximized-overlay`); the
// KDE script/kwinrc key is `bornMaximized` following the existing camelCase
// convention (`sameAxisMove` <-> `core.same_axis_move`,
// `fixedSizePredicate` <-> `core.fixed_size_predicate`). Absent/invalid
// values fall back to `tile-on-launch`. Entries resolve at startup and
// re-resolve only on the deliberate Options `configChanged` reload owned by
// the production entry; there is no polling, reseed, or in-flight mutation.
// Changing the setting never rebuilds or reclassifies retained windows, only
// subsequent admissions.

export const BORN_MAXIMIZED_DEFAULT = "tile-on-launch" as const;
export const BORN_MAXIMIZED_OVERLAY = "keep-maximized-overlay" as const;

export type BornMaximized = typeof BORN_MAXIMIZED_DEFAULT | typeof BORN_MAXIMIZED_OVERLAY;

export function normalizeBornMaximized(value: unknown): BornMaximized {
    if (value === BORN_MAXIMIZED_DEFAULT || value === BORN_MAXIMIZED_OVERLAY) {
        return value;
    }
    return BORN_MAXIMIZED_DEFAULT;
}

function readGlobalConfig(key: string, fallback: string): unknown {
    try {
        if (typeof readConfig === "function") {
            return readConfig(key, fallback);
        }
    } catch (error) {
        void error;
    }
    return fallback;
}

export function readBornMaximizedValue(readBornMaximizedFn?: () => unknown): BornMaximized {
    try {
        if (readBornMaximizedFn !== undefined) {
            try {
                return normalizeBornMaximized(readBornMaximizedFn());
            } catch (error) {
                void error;
                return BORN_MAXIMIZED_DEFAULT;
            }
        }
        return normalizeBornMaximized(readGlobalConfig("bornMaximized", BORN_MAXIMIZED_DEFAULT));
    } catch (error) {
        void error;
        return BORN_MAXIMIZED_DEFAULT;
    }
}
