// Bounded gap setting: validated KWin script startup configuration.
//
// KCM/native settings `innerGap` and `outerGap` default to 8 each, bounded
// inclusively 0..64. Rust owns projection; KWin only carries these values.
// Absent/invalid/out-of-range values fall back to 8, preserving the default
// effective (inner, outer) = (8, 8). Entries resolve at startup and re-resolve
// only on the deliberate Options `configChanged` reload owned by
// the production entry; there is no polling, reseed, or in-flight mutation.

export const DOMAIN_GAP_DEFAULT = 8;
export const DOMAIN_GAP_MIN = 0;
export const DOMAIN_GAP_MAX = 64;
export const OUTER_DOMAIN_GAP_DEFAULT = 8;

export interface DomainGaps {
    readonly innerGap: number;
    readonly outerGap: number;
}

export function normalizeGap(value: unknown): number {
    if (
        typeof value === "number" &&
        Number.isInteger(value) &&
        value >= DOMAIN_GAP_MIN &&
        value <= DOMAIN_GAP_MAX
    ) {
        return value;
    }
    if (typeof value === "string") {
        const text = value.trim();
        if (/^-?\d+$/.test(text)) {
            const parsed = Number(text);
            if (
                Number.isInteger(parsed) &&
                parsed >= DOMAIN_GAP_MIN &&
                parsed <= DOMAIN_GAP_MAX
            ) {
                return parsed;
            }
        }
    }
    return DOMAIN_GAP_DEFAULT;
}

function readGlobalConfig(key: string, fallback: number): unknown {
    try {
        if (typeof readConfig === "function") {
            return readConfig(key, fallback);
        }
    } catch (error) {
        void error;
    }
    return fallback;
}

export function readInnerGapValue(readInnerGapFn?: () => unknown): number {
    try {
        if (readInnerGapFn !== undefined) {
            try {
                return normalizeGap(readInnerGapFn());
            } catch (error) {
                void error;
                return DOMAIN_GAP_DEFAULT;
            }
        }
        return normalizeGap(readGlobalConfig("innerGap", DOMAIN_GAP_DEFAULT));
    } catch (error) {
        void error;
        return DOMAIN_GAP_DEFAULT;
    }
}

export function readOuterGapValue(readOuterGapFn?: () => unknown): number {
    try {
        if (readOuterGapFn !== undefined) {
            try {
                return normalizeGap(readOuterGapFn());
            } catch (error) {
                void error;
                return OUTER_DOMAIN_GAP_DEFAULT;
            }
        }
        return normalizeGap(readGlobalConfig("outerGap", OUTER_DOMAIN_GAP_DEFAULT));
    } catch (error) {
        void error;
        return OUTER_DOMAIN_GAP_DEFAULT;
    }
}

export function readDomainGaps(overrides?: {
    readonly readInnerGapFn?: (() => unknown) | undefined;
    readonly readOuterGapFn?: (() => unknown) | undefined;
}): DomainGaps {
    let innerGap = DOMAIN_GAP_DEFAULT;
    let outerGap = OUTER_DOMAIN_GAP_DEFAULT;
    try {
        innerGap = readInnerGapValue(overrides !== undefined ? overrides.readInnerGapFn : undefined);
    } catch (error) {
        void error;
        innerGap = DOMAIN_GAP_DEFAULT;
    }
    try {
        outerGap = readOuterGapValue(overrides !== undefined ? overrides.readOuterGapFn : undefined);
    } catch (error) {
        void error;
        outerGap = OUTER_DOMAIN_GAP_DEFAULT;
    }
    return { innerGap: innerGap, outerGap: outerGap };
}

// Compatibility aliases for the historical fixed defaults. No import-time
// config reads: entries resolve startup-bound values via readDomainGaps().
export const DOMAIN_GAP = DOMAIN_GAP_DEFAULT;
export const OUTER_DOMAIN_GAP = OUTER_DOMAIN_GAP_DEFAULT;
