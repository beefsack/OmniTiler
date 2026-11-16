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

declare const __OMNITILER_DEV_SETTINGS__: string | undefined;

export function readBakedDevSettings(): string | undefined {
    return typeof __OMNITILER_DEV_SETTINGS__ === "string" ? __OMNITILER_DEV_SETTINGS__ : undefined;
}

export interface IsolatedSettingsSnapshot {
    readonly gap?: number;
    readonly outer?: number;
    readonly defaultTiled?: boolean;
    readonly mode?: "per-output-local" | "global-unique" | "shared";
    readonly sameAxis?: SameAxisMove;
    readonly bornMaximized?: BornMaximized;
    readonly predicate?: FixedSizePredicate;
    readonly refill?: MigrationSourceRefill;
    readonly focusFollowsCursor?: boolean;
}

export function parseIsolatedSettingsSnapshot(raw: string): IsolatedSettingsSnapshot | null {
    if (typeof raw !== "string" || raw.length === 0 || raw.length > 4096) {
        return null;
    }
    let parsed: unknown;
    try {
        parsed = JSON.parse(raw);
    } catch (error) {
        void error;
        return null;
    }
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) {
        return null;
    }
    const obj = parsed as Record<string, unknown>;
    const result: {
        gap?: number;
        outer?: number;
        defaultTiled?: boolean;
        mode?: "per-output-local" | "global-unique" | "shared";
        sameAxis?: SameAxisMove;
        bornMaximized?: BornMaximized;
        predicate?: FixedSizePredicate;
        refill?: MigrationSourceRefill;
        focusFollowsCursor?: boolean;
    } = {};
    const keys = Object.keys(obj);
    for (let i = 0; i < keys.length; i += 1) {
        const key = keys[i];
        if (key === undefined) {
            continue;
        }
        const value = obj[key];
        if (key === "gap") {
            if (typeof value !== "number" || !Number.isInteger(value) || value < 0 || value > 64) {
                return null;
            }
            result.gap = value;
        } else if (key === "outer") {
            if (typeof value !== "number" || !Number.isInteger(value) || value < 0 || value > 64) {
                return null;
            }
            result.outer = value;
        } else if (key === "defaultTiled") {
            if (typeof value !== "boolean") {
                return null;
            }
            result.defaultTiled = value;
        } else if (key === "mode") {
            if (value !== "per-output-local" && value !== "global-unique" && value !== "shared") {
                return null;
            }
            result.mode = value;
        } else if (key === "sameAxis") {
            if (value !== SAME_AXIS_MOVE_DEFAULT && value !== SAME_AXIS_MOVE_SWAP) {
                return null;
            }
            result.sameAxis = value;
        } else if (key === "bornMaximized") {
            if (value !== BORN_MAXIMIZED_DEFAULT && value !== BORN_MAXIMIZED_OVERLAY) {
                return null;
            }
            result.bornMaximized = value;
        } else if (key === "predicate") {
            if (value !== FIXED_SIZE_PREDICATE_DEFAULT && value !== FIXED_SIZE_PREDICATE_EITHER) {
                return null;
            }
            result.predicate = value;
        } else if (key === "refill") {
            if (value !== MIGRATION_SOURCE_REFILL_DEFAULT && value !== MIGRATION_SOURCE_REFILL_MRU) {
                return null;
            }
            result.refill = value;
        } else if (key === "focusFollowsCursor") {
            if (typeof value !== "boolean") {
                return null;
            }
            result.focusFollowsCursor = value;
        } else {
            return null;
        }
    }
    return result;
}

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

// Cursor-first targeting (`settings_catalog` `output.switch-target` /
// `output.fresh-placement`, tokens from `cursor_targeting`): `cursor-output`
// default and `focused-output` alternative for switches; the three
// `FreshPlacement` tokens for admissions. Closed token sets only.
export const SWITCH_TARGET_DEFAULT = "cursor-output" as const;
export const SWITCH_TARGET_FOCUSED = "focused-output" as const;

export type SwitchTarget = typeof SWITCH_TARGET_DEFAULT | typeof SWITCH_TARGET_FOCUSED;

export function normalizeSwitchTarget(value: unknown): SwitchTarget {
    if (value === SWITCH_TARGET_DEFAULT || value === SWITCH_TARGET_FOCUSED) {
        return value;
    }
    return SWITCH_TARGET_DEFAULT;
}

export const FRESH_PLACEMENT_DEFAULT = "cursor-output-last-focused" as const;
export const FRESH_PLACEMENT_CURSOR_CLOSEST = "cursor-output-cursor-closest" as const;
export const FRESH_PLACEMENT_FOCUSED = "focused-output-last-focused" as const;

export type FreshPlacement = typeof FRESH_PLACEMENT_DEFAULT | typeof FRESH_PLACEMENT_CURSOR_CLOSEST | typeof FRESH_PLACEMENT_FOCUSED;

export function normalizeFreshPlacement(value: unknown): FreshPlacement {
    if (
        value === FRESH_PLACEMENT_DEFAULT ||
        value === FRESH_PLACEMENT_CURSOR_CLOSEST ||
        value === FRESH_PLACEMENT_FOCUSED
    ) {
        return value;
    }
    return FRESH_PLACEMENT_DEFAULT;
}

// Focus-follows-cursor (`settings_catalog` `output.focus-follows-cursor`):
// boolean only; any non-boolean falls back to `false`.
export const FOCUS_FOLLOWS_CURSOR_DEFAULT = false;

export function normalizeFocusFollowsCursor(value: unknown): boolean {
    return value === true;
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

export function readSwitchTargetValue(readSwitchTargetFn?: () => unknown): SwitchTarget {
    try {
        if (readSwitchTargetFn !== undefined) {
            try {
                return normalizeSwitchTarget(readSwitchTargetFn());
            } catch (error) {
                void error;
                return SWITCH_TARGET_DEFAULT;
            }
        }
        return normalizeSwitchTarget(readGlobalConfigValue("switchTarget", SWITCH_TARGET_DEFAULT));
    } catch (error) {
        void error;
        return SWITCH_TARGET_DEFAULT;
    }
}

export function readFreshPlacementValue(readFreshPlacementFn?: () => unknown): FreshPlacement {
    try {
        if (readFreshPlacementFn !== undefined) {
            try {
                return normalizeFreshPlacement(readFreshPlacementFn());
            } catch (error) {
                void error;
                return FRESH_PLACEMENT_DEFAULT;
            }
        }
        return normalizeFreshPlacement(readGlobalConfigValue("freshPlacement", FRESH_PLACEMENT_DEFAULT));
    } catch (error) {
        void error;
        return FRESH_PLACEMENT_DEFAULT;
    }
}

export function readFocusFollowsCursorValue(readFocusFollowsCursorFn?: () => unknown): boolean {
    try {
        if (readFocusFollowsCursorFn !== undefined) {
            try {
                return normalizeFocusFollowsCursor(readFocusFollowsCursorFn());
            } catch (error) {
                void error;
                return FOCUS_FOLLOWS_CURSOR_DEFAULT;
            }
        }
        return normalizeFocusFollowsCursor(readGlobalConfigValue("focusFollowsCursor", FOCUS_FOLLOWS_CURSOR_DEFAULT));
    } catch (error) {
        void error;
        return FOCUS_FOLLOWS_CURSOR_DEFAULT;
    }
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
    readonly readFixedPredicateFn?: () => unknown;
    readonly readSwitchTargetFn?: () => unknown;
    readonly readFreshPlacementFn?: () => unknown;
    readonly readFocusFollowsCursorFn?: () => unknown;
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
    readonly readPredicateLive: () => FixedSizePredicate;
    // Drifted startup-consumed keys since activation, for the
    // restart-required log. Never adopted here.
    checkStartupDrift: () => string[];
    // Raw v4 settings snapshot for the connection lane (K1a): the exact
    // adapter-sourced key/value pairs in canonical settings order. Keys
    // the adapter never sources (cursor warp, border/underlay presentation)
    // are omitted: `cursorWarp` stays unsupported (never sent), and the core
    // backfills the remaining defaults at startup.
    // `mode` carries the startup-consumed value (restart-only, never
    // live); every other key re-resolves live per snapshot.
    snapshotConnectionValues: () => Array<[string, unknown]>;
}

export function createWorldSettingsLive(
    overrides: WorldSettingsLiveOverrides = {},
    isolatedSettings: IsolatedSettingsSnapshot | null = null,
): WorldSettingsLive {
    if (isolatedSettings !== null) {
        const startupProfile: ShortcutProfile = "cosmic";
        const startupWorkspaceMode: unknown =
            isolatedSettings.mode !== undefined ? isolatedSettings.mode : "per-output-local";
        const gaps: DomainGaps = {
            innerGap: isolatedSettings.gap !== undefined ? isolatedSettings.gap : 8,
            outerGap: isolatedSettings.outer !== undefined ? isolatedSettings.outer : 8,
        };
        const readMode = (): unknown => startupWorkspaceMode;
        const readDefaultTiled = (): boolean =>
            isolatedSettings.defaultTiled !== undefined ? isolatedSettings.defaultTiled : true;
        const readGapLive = (): number => gaps.innerGap;
        const readOuterGapLive = (): number => gaps.outerGap;
        const readSameAxisLive = (): string =>
            isolatedSettings.sameAxis !== undefined ? isolatedSettings.sameAxis : SAME_AXIS_MOVE_DEFAULT;
        const readRefillLive = (): MigrationSourceRefill =>
            isolatedSettings.refill !== undefined ? isolatedSettings.refill : MIGRATION_SOURCE_REFILL_DEFAULT;
        const readBornMaximizedLive = (): BornMaximized =>
            isolatedSettings.bornMaximized !== undefined ? isolatedSettings.bornMaximized : BORN_MAXIMIZED_DEFAULT;
        const readPredicateLive = (): FixedSizePredicate =>
            isolatedSettings.predicate !== undefined ? isolatedSettings.predicate : FIXED_SIZE_PREDICATE_DEFAULT;
        const checkStartupDrift = (): string[] => [];
        const snapshotConnectionValues = (): Array<[string, unknown]> => {
            const values: Array<[string, unknown]> = [
                ["gap", readGapLive()],
                ["outer", readOuterGapLive()],
                ["defaultTiled", readDefaultTiled()],
                ["mode", startupWorkspaceMode],
                ["sameAxis", readSameAxisLive()],
                ["bornMaximized", readBornMaximizedLive()],
                ["predicate", readPredicateLive()],
                ["refill", readRefillLive()],
            ];
            if (isolatedSettings.focusFollowsCursor !== undefined) {
                values.push(["focusFollowsCursor", isolatedSettings.focusFollowsCursor]);
            }
            return values;
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
            readPredicateLive,
            checkStartupDrift,
            snapshotConnectionValues,
        };
    }
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
    // Raw v4 settings snapshot: adapter-sourced values only, canonical
    // settings order. Types ride raw (core validates); no clamp/default
    // beyond each reader's own normalization. `mode` is the
    // startup-consumed adoption (restart-only); live drift against it is
    // reported through checkStartupDrift and never adopted here.
    const readPredicateLive = (): FixedSizePredicate =>
        readFixedSizePredicateValue(overrides.readFixedPredicateFn);
    const readSwitchTargetLive = (): SwitchTarget => {
        try {
            return readSwitchTargetValue(overrides.readSwitchTargetFn);
        } catch (error) {
            void error;
            return SWITCH_TARGET_DEFAULT;
        }
    };
    const readFreshPlacementLive = (): FreshPlacement => {
        try {
            return readFreshPlacementValue(overrides.readFreshPlacementFn);
        } catch (error) {
            void error;
            return FRESH_PLACEMENT_DEFAULT;
        }
    };
    const readFocusFollowsCursorLive = (): boolean => {
        try {
            return readFocusFollowsCursorValue(overrides.readFocusFollowsCursorFn);
        } catch (error) {
            void error;
            return FOCUS_FOLLOWS_CURSOR_DEFAULT;
        }
    };
    const snapshotConnectionValues = (): Array<[string, unknown]> => [
        ["gap", readGapLive()],
        ["outer", readOuterGapLive()],
        ["defaultTiled", readDefaultTiled()],
        ["mode", startupWorkspaceMode],
        ["sameAxis", readSameAxisLive()],
        ["bornMaximized", readBornMaximizedLive()],
        ["predicate", readPredicateLive()],
        ["refill", readRefillLive()],
        ["switchTarget", readSwitchTargetLive()],
        ["freshPlacement", readFreshPlacementLive()],
        ["focusFollowsCursor", readFocusFollowsCursorLive()],
    ];
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
        readPredicateLive,
        checkStartupDrift,
        snapshotConnectionValues,
    };
}
