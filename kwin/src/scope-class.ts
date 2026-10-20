// Dev/test admission scope (platform-only): optional allowlist of exact
// window classes for repeatable fixture runs. Empty means whole world.
// Matching is exact `resourceClass` equality (Wayland app_id), never
// substring/prefix/case-folded; unreadable identity never matches.
// Representation is one comma-joined string; repeated flags accumulate,
// duplicates collapse. Foreign windows never enter the report.

const SCOPE_CLASS_PATTERN = /^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$/;

// Validated scope allowlist: missing/empty disables the filter (whole
// world). A string splits on commas; an array carries entries directly.
// Every entry must match the bounded class charset exactly, with no empty
// entries; anything else returns null so launchers fail closed before any
// mutation. Duplicates collapse, order preserved.
export function normalizeScopeClassList(value: unknown): readonly string[] | null {
    if (value === undefined || value === null) {
        return [];
    }
    const parts: string[] = [];
    if (typeof value === "string") {
        if (value === "") {
            return [];
        }
        for (const part of value.split(",")) {
            if (!SCOPE_CLASS_PATTERN.test(part)) {
                return null;
            }
            parts.push(part);
        }
    } else if (Array.isArray(value)) {
        if (value.length === 0) {
            return [];
        }
        for (const entry of value) {
            if (typeof entry !== "string" || !SCOPE_CLASS_PATTERN.test(entry)) {
                return null;
            }
            parts.push(entry);
        }
    } else {
        return null;
    }
    return [...new Set(parts)];
}

// Compile-time scope baked by the dev launcher build (esbuild define
// `__OMNITILER_SCOPE_CLASS__` from comma-joined `OMNITILER_SCOPE_CLASS`).
// Ordinary `npm run build` without the env var bakes "" (unscoped).
declare const __OMNITILER_SCOPE_CLASS__: string | undefined;

export function readBakedScopeClass(): string | undefined {
    return typeof __OMNITILER_SCOPE_CLASS__ === "string" ? __OMNITILER_SCOPE_CLASS__ : undefined;
}

export function windowMatchesScopeClass(
    readProp: (value: object, property: string) => unknown,
    ref: object,
    scope: readonly string[],
): boolean {
    if (scope.length === 0) {
        return true;
    }
    let resourceClass: unknown = undefined;
    try {
        resourceClass = readProp(ref, "resourceClass");
    } catch (error) {
        void error;
        return false;
    }
    return typeof resourceClass === "string" && scope.includes(resourceClass);
}
