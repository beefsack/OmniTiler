// Bounded fixed-size admission predicate: validated KWin script startup configuration.
//
// R-SPC-04 D1 global KDE `fixedSizePredicate` with `both-axes-fixed` default
// and `either-axis-fixed` alternative. The adapter owns admission
// classification (`isFixedSize`) and carries the non-default predicate on the
// planner request so the shared core classifies subsequent admissions
// identically. Absent/invalid values fall back to `both-axes-fixed`.
// Entries resolve at startup and re-resolve only on the deliberate Options
// `configChanged` reload owned by the production entry; there is no polling,
// reseed, or in-flight mutation. Changing the setting never reclassifies
// retained windows, only subsequent admissions.

export const FIXED_SIZE_PREDICATE_DEFAULT = "both-axes-fixed" as const;
export const FIXED_SIZE_PREDICATE_EITHER = "either-axis-fixed" as const;

export type FixedSizePredicate =
    | typeof FIXED_SIZE_PREDICATE_DEFAULT
    | typeof FIXED_SIZE_PREDICATE_EITHER;

export function normalizeFixedSizePredicate(value: unknown): FixedSizePredicate {
    if (
        value === FIXED_SIZE_PREDICATE_DEFAULT ||
        value === FIXED_SIZE_PREDICATE_EITHER
    ) {
        return value;
    }
    return FIXED_SIZE_PREDICATE_DEFAULT;
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

export function readFixedSizePredicateValue(readPredicateFn?: () => unknown): FixedSizePredicate {
    try {
        if (readPredicateFn !== undefined) {
            try {
                return normalizeFixedSizePredicate(readPredicateFn());
            } catch (error) {
                void error;
                return FIXED_SIZE_PREDICATE_DEFAULT;
            }
        }
        return normalizeFixedSizePredicate(
            readGlobalConfig("fixedSizePredicate", FIXED_SIZE_PREDICATE_DEFAULT),
        );
    } catch (error) {
        void error;
        return FIXED_SIZE_PREDICATE_DEFAULT;
    }
}
