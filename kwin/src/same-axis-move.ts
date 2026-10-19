// Bounded same-axis move setting: validated KWin script startup configuration.
//
// R-MOV-03 global KDE `sameAxisMove` with `group-with-neighbor` default and
// `swap-with-neighbor` alternative. Rust owns move semantics; KWin only carries the
// configured token on the move wire command. Absent/invalid values fall back
// to `group-with-neighbor`. Entries resolve at startup and re-resolve only on the
// deliberate Options `configChanged` reload owned by the production entry;
// there is no polling, reseed, or in-flight mutation.

export const SAME_AXIS_MOVE_DEFAULT = "group-with-neighbor" as const;
export const SAME_AXIS_MOVE_SWAP = "swap-with-neighbor" as const;

export type SameAxisMove = typeof SAME_AXIS_MOVE_DEFAULT | typeof SAME_AXIS_MOVE_SWAP;

export function normalizeSameAxisMove(value: unknown): SameAxisMove {
    if (value === SAME_AXIS_MOVE_DEFAULT || value === SAME_AXIS_MOVE_SWAP) {
        return value;
    }
    return SAME_AXIS_MOVE_DEFAULT;
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

export function readSameAxisMoveValue(readSameAxisMoveFn?: () => unknown): SameAxisMove {
    try {
        if (readSameAxisMoveFn !== undefined) {
            try {
                return normalizeSameAxisMove(readSameAxisMoveFn());
            } catch (error) {
                void error;
                return SAME_AXIS_MOVE_DEFAULT;
            }
        }
        return normalizeSameAxisMove(readGlobalConfig("sameAxisMove", SAME_AXIS_MOVE_DEFAULT));
    } catch (error) {
        void error;
        return SAME_AXIS_MOVE_DEFAULT;
    }
}
