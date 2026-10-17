// Startup-consumed shortcut profile setting (legacy parity with
// plan-adapter-entry `readShortcutProfile`): validated KWin script startup
// configuration naming the directional shortcut catalog. All three
// configured profiles share one directional core (legacy
// `planShortcutCatalog` takes the profile but registers the same rows),
// so the value is consumed once at startup and never re-read for
// behavior. A configChanged drift against the startup snapshot logs
// restart-required and is never adopted here; registration never rewrites
// active bindings or foreign records (offline only, no KGlobalAccel
// writes). Absent/invalid values fall back to `cosmic`.

export const SHORTCUT_PROFILE_DEFAULT = "cosmic" as const;

export type ShortcutProfile = "cosmic" | "hyprland" | "bspwm";

export function normalizeShortcutProfile(value: unknown): ShortcutProfile {
    if (value === "cosmic" || value === "hyprland" || value === "bspwm") {
        return value;
    }
    return SHORTCUT_PROFILE_DEFAULT;
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

export function readShortcutProfileValue(readProfileFn?: () => unknown): ShortcutProfile {
    try {
        if (readProfileFn !== undefined) {
            try {
                return normalizeShortcutProfile(readProfileFn());
            } catch (error) {
                void error;
                return SHORTCUT_PROFILE_DEFAULT;
            }
        }
        return normalizeShortcutProfile(readGlobalConfig("shortcutProfile", SHORTCUT_PROFILE_DEFAULT));
    } catch (error) {
        void error;
        return SHORTCUT_PROFILE_DEFAULT;
    }
}
