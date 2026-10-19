// Bounded migration source-refill setting: validated KWin script startup configuration.
//
// R-WS-12 G-37 global KDE `migrationSourceRefill` with
// `last-remaining-workspace` default and `most-recently-used-workspace`
// alternative. The pure `selectMigrationSourceRefill` selector resolves the
// refill id from the remembered per-output previous id plus the surviving
// source-output scoped backing ids: the previous id refills only when it
// survives among the remaining ids after excluding the migrated one, else
// the last remaining scoped id refills. Absent/invalid values fall back to
// `last-remaining-workspace`. Entries resolve at startup and re-resolve only
// on the deliberate Options `configChanged` reload owned by
// the production entry; there is no polling, reseed, or in-flight mutation.
// Changing the setting never moves an already-migrated workspace, only
// subsequent migrations. Destination insertion (D4) is unchanged.

export const MIGRATION_SOURCE_REFILL_DEFAULT = "last-remaining-workspace" as const;
export const MIGRATION_SOURCE_REFILL_MRU = "most-recently-used-workspace" as const;

export type MigrationSourceRefill =
    | typeof MIGRATION_SOURCE_REFILL_DEFAULT
    | typeof MIGRATION_SOURCE_REFILL_MRU;

export function normalizeMigrationSourceRefill(value: unknown): MigrationSourceRefill {
    if (
        value === MIGRATION_SOURCE_REFILL_DEFAULT ||
        value === MIGRATION_SOURCE_REFILL_MRU
    ) {
        return value;
    }
    return MIGRATION_SOURCE_REFILL_DEFAULT;
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

export function readMigrationSourceRefillValue(
    readRefillFn?: () => unknown,
): MigrationSourceRefill {
    try {
        if (readRefillFn !== undefined) {
            try {
                return normalizeMigrationSourceRefill(readRefillFn());
            } catch (error) {
                void error;
                return MIGRATION_SOURCE_REFILL_DEFAULT;
            }
        }
        return normalizeMigrationSourceRefill(
            readGlobalConfig("migrationSourceRefill", MIGRATION_SOURCE_REFILL_DEFAULT),
        );
    } catch (error) {
        void error;
        return MIGRATION_SOURCE_REFILL_DEFAULT;
    }
}

// Pure G-37 source-refill selector mirroring the shared-core
// `select_migration_source_refill` rule: the remembered previous id refills
// only under the most-recently-used value while it survives among the
// remaining source-output scoped backing ids (live, still assigned the
// source, surviving empties valid); otherwise the last remaining scoped id
// refills. Never recreates: null only when nothing remains.
export function selectMigrationSourceRefill(
    mode: MigrationSourceRefill,
    previous: string | null | undefined,
    remainingInScopeOrder: ReadonlyArray<string>,
): string | null {
    if (
        mode === MIGRATION_SOURCE_REFILL_MRU &&
        typeof previous === "string" &&
        previous !== "" &&
        remainingInScopeOrder.includes(previous)
    ) {
        return previous;
    }
    return remainingInScopeOrder[remainingInScopeOrder.length - 1] ?? null;
}
