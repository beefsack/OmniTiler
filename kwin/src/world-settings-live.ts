// Live settings seams for the world production entry (D1-D5):
// validated startup snapshots plus per-dispatch live re-reads. Exactly
// the entry's previous closures, relocated without behavior change:
// - `workspaceMode` and `shortcutProfile` are startup-consumed (legacy
//   parity): snapshotted once, never re-read for behavior; drift reports
//   through `checkStartupDrift` for the restart-required log.
// - Gaps, same-axis move, migration refill, and the default-tiling seed
//   re-resolve live so a deliberate Options reload applies without
//   restart and without touching retained state.

import { readDomainGaps, type DomainGaps } from "./domain-gap";
import { readShortcutProfileValue, type ShortcutProfile } from "./shortcut-profile";

// Consolidated config carriage (behavior/defaults/timing unchanged):
// validated KWin script startup tokens previously in same-axis-move.ts,
// migration-source-refill.ts (carriage only; the pure
// `selectMigrationSourceRefill` selector is retired: the shared core
// `select_migration_source_refill` in crates/tiler-core/src/output_selection.rs
// owns the refill decision), born-maximized.ts, and fixed-size-predicate.ts.
// Rust owns every policy semantic; KWin only carries these validated tokens.
// Entries resolve at startup and re-resolve only on the deliberate Options
// `configChanged` reload owned by the production entry; no polling, reseed,
// or in-flight mutation. Changing a setting never rebuilds or reclassifies
// retained windows, only subsequent admissions/moves/migrations.

// R-MOV-03 global KDE `sameAxisMove` with `group-with-neighbor` default and
// `swap-with-neighbor` alternative.
export const SAME_AXIS_MOVE_DEFAULT = "group-with-neighbor" as const;
export const SAME_AXIS_MOVE_SWAP = "swap-with-neighbor" as const;

export type SameAxisMove = typeof SAME_AXIS_MOVE_DEFAULT | typeof SAME_AXIS_MOVE_SWAP;

export function normalizeSameAxisMove(value: unknown): SameAxisMove {
    if (value === SAME_AXIS_MOVE_DEFAULT || value === SAME_AXIS_MOVE_SWAP) {
        return value;
    }
    return SAME_AXIS_MOVE_DEFAULT;
}

// R-WS-12 G-37 global KDE `migrationSourceRefill` with
// `last-remaining-workspace` default and `most-recently-used-workspace`
// alternative. Absent/invalid values fall back to
// `last-remaining-workspace`.
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

// R-MAX-06 global KDE `bornMaximized` with `tile-on-launch` default and
// `keep-maximized-overlay` alternative. Exact tokens match Windows
// `core.born_maximized`.
export const BORN_MAXIMIZED_DEFAULT = "tile-on-launch" as const;
export const BORN_MAXIMIZED_OVERLAY = "keep-maximized-overlay" as const;

export type BornMaximized = typeof BORN_MAXIMIZED_DEFAULT | typeof BORN_MAXIMIZED_OVERLAY;

export function normalizeBornMaximized(value: unknown): BornMaximized {
    if (value === BORN_MAXIMIZED_DEFAULT || value === BORN_MAXIMIZED_OVERLAY) {
        return value;
    }
    return BORN_MAXIMIZED_DEFAULT;
}

// R-SPC-04 D1 global KDE `fixedSizePredicate` with `both-axes-fixed` default
// and `either-axis-fixed` alternative.
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

function readGlobalConfigValue(key: string, fallback: unknown): unknown {
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
        return normalizeSameAxisMove(readGlobalConfigValue("sameAxisMove", SAME_AXIS_MOVE_DEFAULT));
    } catch (error) {
        void error;
        return SAME_AXIS_MOVE_DEFAULT;
    }
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
            readGlobalConfigValue("migrationSourceRefill", MIGRATION_SOURCE_REFILL_DEFAULT),
        );
    } catch (error) {
        void error;
        return MIGRATION_SOURCE_REFILL_DEFAULT;
    }
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
        return normalizeBornMaximized(readGlobalConfigValue("bornMaximized", BORN_MAXIMIZED_DEFAULT));
    } catch (error) {
        void error;
        return BORN_MAXIMIZED_DEFAULT;
    }
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
            readGlobalConfigValue("fixedSizePredicate", FIXED_SIZE_PREDICATE_DEFAULT),
        );
    } catch (error) {
        void error;
        return FIXED_SIZE_PREDICATE_DEFAULT;
    }
}

// D2 anchor fallback tokens (main.xml `anchorFallback`, catalog
// `admission.anchor-fallback`): tolerant readers backfill the default on
// missing/invalid input, mirroring the core `normalize_wire`.
export const ANCHOR_FALLBACK_DEFAULT = "largest-splittable-window" as const;
const ANCHOR_FALLBACK_TOKENS: ReadonlySet<string> = new Set([
    "largest-splittable-window",
    "closest-splittable-relative",
    "no-search",
]);

export function normalizeAnchorFallback(value: unknown): string {
    return typeof value === "string" && ANCHOR_FALLBACK_TOKENS.has(value)
        ? value
        : ANCHOR_FALLBACK_DEFAULT;
}

// D3 no-space result tokens (main.xml `noSpaceResult`, catalog
// `admission.no-space`): tolerant readers backfill float.
export const NO_SPACE_RESULT_DEFAULT = "float" as const;
const NO_SPACE_RESULT_TOKENS: ReadonlySet<string> = new Set(["float", "allow-overlap"]);

export function normalizeNoSpaceResult(value: unknown): string {
    return typeof value === "string" && NO_SPACE_RESULT_TOKENS.has(value)
        ? value
        : NO_SPACE_RESULT_DEFAULT;
}

export interface WorldSettingsLiveOverrides {
    readonly readWorkspaceModeFn?: () => unknown;
    readonly readProfileFn?: () => unknown;
    readonly readDefaultTiledFn?: () => unknown;
    readonly readInnerGapFn?: () => unknown;
    readonly readOuterGapFn?: () => unknown;
    readonly readSameAxisMoveFn?: () => unknown;
    readonly readMigrationSourceRefillFn?: () => unknown;
    readonly readBornMaximizedFn?: () => unknown;
    readonly readAnchorFallbackFn?: () => unknown;
    readonly readNoSpaceResultFn?: () => unknown;
}

export interface WorldSettingsLive {
    // Startup-consumed snapshot (legacy parity): all profiles share one
    // directional core; registration never rewrites active or foreign
    // bindings.
    readonly startupProfile: ShortcutProfile;
    // Startup adoption (legacy parity): the initial requested mode is
    // preserved per activation; a new instance adopts the changed value.
    readonly startupWorkspaceMode: unknown;
    // Gap snapshot seeding startup; every capture re-resolves live.
    readonly gaps: DomainGaps;
    readonly readMode: () => unknown;
    readonly readDefaultTiled: () => boolean;
    readonly readGapLive: () => number;
    readonly readOuterGapLive: () => number;
    readonly readSameAxisLive: () => string;
    readonly readRefillLive: () => MigrationSourceRefill;
    // R-MAX-06 born-maximized mode (main.xml `bornMaximized`): validated
    // token carried on every full report, re-resolved per dispatch so a
    // deliberate Options reload applies to subsequent admissions with no
    // restart. The core owns admission/clear policy from this ingress;
    // the adapter never decides.
    readonly readBornMaximizedLive: () => BornMaximized;
    // D2/D3 best-effort capacity settings (main.xml `anchorFallback`,
    // `noSpaceResult`): validated tokens carried on every full report,
    // re-resolved per dispatch so a deliberate Options reload applies to
    // subsequent capacity decisions with no restart and no retained-state
    // reflow. Missing/invalid values backfill the defaults.
    readonly readAnchorFallbackLive: () => string;
    readonly readNoSpaceResultLive: () => string;
    // Drifted startup-consumed keys since activation, for the
    // restart-required log. Never adopted here.
    checkStartupDrift: () => string[];
}

export function createWorldSettingsLive(
    overrides: WorldSettingsLiveOverrides = {},
): WorldSettingsLive {
    // Startup-consumed shortcut profile (legacy parity): all profiles
    // share one directional core, so the validated value is snapshotted
    // once here and never re-read for behavior. Registration below keeps
    // the shared catalog and never rewrites active or foreign bindings.
    const startupProfile = readShortcutProfileValue(overrides.readProfileFn);
    const readWorkspaceModeValue = (): unknown => {
        if (overrides.readWorkspaceModeFn !== undefined) {
            try {
                return overrides.readWorkspaceModeFn();
            } catch (error) {
                void error;
                return "per-output-local";
            }
        }
        try {
            return readConfig("workspaceMode", "per-output-local");
        } catch (error) {
            void error;
            return "per-output-local";
        }
    };
    // Startup adoption (legacy parity): the initial requested mode is
    // preserved per activation and never re-read for behavior. Drift
    // against this snapshot logs restart-required on configChanged and
    // is never adopted here; a new instance adopts the changed value.
    const startupWorkspaceMode = readWorkspaceModeValue();
    const readMode = (): unknown => startupWorkspaceMode;
    const gaps = readDomainGaps({ readInnerGapFn: overrides.readInnerGapFn, readOuterGapFn: overrides.readOuterGapFn });
    // Live gap re-read (legacy Options configChanged parity): the snapshot
    // above seeds startup, but every capture re-resolves so a deliberate
    // settings reload applies to subsequent layouts with no restart and
    // no double inset (the core adopts via update-gaps, preserving
    // topology). No polling; the configChanged edge only invalidates.
    const readGapLive = (): number => {
        try {
            return readDomainGaps({
                readInnerGapFn: overrides.readInnerGapFn,
                readOuterGapFn: overrides.readOuterGapFn,
            }).innerGap;
        } catch (error) {
            void error;
            return gaps.innerGap;
        }
    };
    const readOuterGapLive = (): number => {
        try {
            return readDomainGaps({
                readInnerGapFn: overrides.readInnerGapFn,
                readOuterGapFn: overrides.readOuterGapFn,
            }).outerGap;
        } catch (error) {
            void error;
            return gaps.outerGap;
        }
    };
    // Same-axis move (legacy R-MOV-03 parity): validated token carried on
    // every move command, re-resolved per press so a deliberate Options
    // reload applies to subsequent moves with no tree rebuild.
    const readSameAxisLive = (): string => {
        try {
            return readSameAxisMoveValue(overrides.readSameAxisMoveFn);
        } catch (error) {
            void error;
            return "group-with-neighbor";
        }
    };
    // Migration source refill (R-WS-12 G-37): validated token carried on
    // every full report, re-resolved per dispatch so a deliberate Options
    // reload applies to subsequent migrations with no restart and no
    // committed-state reset. Core owns the refill selection; this only
    // carries the validated setting.
    const readRefillLive = (): MigrationSourceRefill => {
        try {
            return readMigrationSourceRefillValue(overrides.readMigrationSourceRefillFn);
        } catch (error) {
            void error;
            return "last-remaining-workspace";
        }
    };
    // Born-maximized admission mode (R-MAX-06 N6): validated token
    // carried on every full report, re-resolved per dispatch. Core owns
    // admission and the one-shot clear from this ingress.
    const readBornMaximizedLive = (): BornMaximized => {
        try {
            return readBornMaximizedValue(overrides.readBornMaximizedFn);
        } catch (error) {
            void error;
            return BORN_MAXIMIZED_DEFAULT;
        }
    };
    // D2 anchor fallback (main.xml `anchorFallback`): validated token
    // carried on every full report, re-resolved per dispatch so a
    // deliberate Options reload applies to subsequent capacity decisions
    // with no restart. Tolerant: missing/invalid backfills the default.
    const readAnchorFallbackLive = (): string => {
        try {
            if (overrides.readAnchorFallbackFn !== undefined) {
                try {
                    return normalizeAnchorFallback(overrides.readAnchorFallbackFn());
                } catch (error) {
                    void error;
                    return ANCHOR_FALLBACK_DEFAULT;
                }
            }
            return normalizeAnchorFallback(readConfig("anchorFallback", ANCHOR_FALLBACK_DEFAULT));
        } catch (error) {
            void error;
            return ANCHOR_FALLBACK_DEFAULT;
        }
    };
    // D3 no-space result (main.xml `noSpaceResult`): same live timing as
    // the anchor fallback; tolerant to the float default.
    const readNoSpaceResultLive = (): string => {
        try {
            if (overrides.readNoSpaceResultFn !== undefined) {
                try {
                    return normalizeNoSpaceResult(overrides.readNoSpaceResultFn());
                } catch (error) {
                    void error;
                    return NO_SPACE_RESULT_DEFAULT;
                }
            }
            return normalizeNoSpaceResult(readConfig("noSpaceResult", NO_SPACE_RESULT_DEFAULT));
        } catch (error) {
            void error;
            return NO_SPACE_RESULT_DEFAULT;
        }
    };
    // Saved default tiling (legacy `defaultTiled` parity): validated raw
    // saved value for the core seed policy. Non-boolean values backfill
    // `true`, matching the legacy default.
    const readDefaultTiled = (): boolean => {
        if (overrides.readDefaultTiledFn !== undefined) {
            try {
                return overrides.readDefaultTiledFn() !== false;
            } catch (error) {
                void error;
                return true;
            }
        }
        try {
            return readConfig("defaultTiled", true) !== false;
        } catch (error) {
            void error;
            return true;
        }
    };
    const checkStartupDrift = (): string[] => {
        const drifted: string[] = [];
        try {
            if (String(readWorkspaceModeValue()) !== String(startupWorkspaceMode)) {
                drifted.push("workspaceMode");
            }
        } catch (error) {
            void error;
        }
        try {
            if (readShortcutProfileValue(overrides.readProfileFn) !== startupProfile) {
                drifted.push("shortcutProfile");
            }
        } catch (error) {
            void error;
        }
        return drifted;
    };
    return {
        startupProfile,
        startupWorkspaceMode,
        gaps,
        readMode,
        readDefaultTiled,
        readGapLive,
        readOuterGapLive,
        readSameAxisLive,
        readRefillLive,
        readBornMaximizedLive,
        readAnchorFallbackLive,
        readNoSpaceResultLive,
        checkStartupDrift,
    };
}
