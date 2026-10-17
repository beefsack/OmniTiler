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
import {
    readMigrationSourceRefillValue,
    type MigrationSourceRefill,
} from "./migration-source-refill";
import { readSameAxisMoveValue } from "./same-axis-move";
import { readShortcutProfileValue, type ShortcutProfile } from "./shortcut-profile";

export interface WorldSettingsLiveOverrides {
    readonly readWorkspaceModeFn?: () => unknown;
    readonly readProfileFn?: () => unknown;
    readonly readDefaultTiledFn?: () => unknown;
    readonly readInnerGapFn?: () => unknown;
    readonly readOuterGapFn?: () => unknown;
    readonly readSameAxisMoveFn?: () => unknown;
    readonly readMigrationSourceRefillFn?: () => unknown;
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
        checkStartupDrift,
    };
}
