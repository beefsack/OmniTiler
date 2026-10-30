// World-protocol decimal counters and byte bounds (D4/D5).
//
// Every global counter (obs, reqSeq, cmdSeq, reply rev) is a bounded
// canonical decimal STRING, never a JSON number: JS Number loses precision
// past 2^53. Compare numerically (length then lexicographic), increment via
// digit arrays. `.length` is an exact byte count only for validated ASCII
// payloads; utf8ByteLength measures actual UTF-8 bytes for the 4 MiB gates.

export const WORLD_CONTRACT_VERSION = 3;
export const WORLD_MAX_REQUEST_BYTES = 4 * 1024 * 1024;
export const WORLD_MAX_REPLY_BYTES = 4 * 1024 * 1024;
export const WORLD_MAX_ID_LEN = 128;
export const WORLD_MAX_GENERATION_LEN = 64;
export const WORLD_MAX_CORRELATION_LEN = 128;
export const WORLD_U64_MAX = "18446744073709551615";

export function isCanonicalU64(text: unknown): text is string {
    if (typeof text !== "string" || text.length === 0 || text.length > 20) {
        return false;
    }
    if (text === "0") {
        return true;
    }
    const first = text.charCodeAt(0);
    if (first < 49 || first > 57) {
        return false;
    }
    for (let i = 1; i < text.length; i += 1) {
        const c = text.charCodeAt(i);
        if (c < 48 || c > 57) {
            return false;
        }
    }
    if (text.length === 20 && text > WORLD_U64_MAX) {
        return false;
    }
    return true;
}

// Numeric comparison over canonical decimal strings: -1/0/1. Never converts
// through Number.
export function compareU64(a: string, b: string): number {
    if (a.length !== b.length) {
        return a.length < b.length ? -1 : 1;
    }
    if (a === b) {
        return 0;
    }
    return a < b ? -1 : 1;
}

// Increment a canonical decimal string. Null on exhaustion (u64 max wraps
// nowhere: the caller must re-bootstrap under a new generation).
export function incrementU64(text: string): string | null {
    if (!isCanonicalU64(text) || text === WORLD_U64_MAX) {
        return null;
    }
    const digits = text.split("");
    for (let i = digits.length - 1; i >= 0; i -= 1) {
        const d = (digits[i] as string).charCodeAt(0) - 48 + 1;
        if (d <= 9) {
            digits[i] = String(d);
            return digits.join("");
        }
        digits[i] = "0";
    }
    return `1${digits.join("")}`;
}

export function utf8ByteLength(text: string): number {
    let bytes = 0;
    for (let i = 0; i < text.length; i += 1) {
        const code = text.charCodeAt(i);
        if (code < 0x80) {
            bytes += 1;
        } else if (code < 0x800) {
            bytes += 2;
        } else if (code >= 0xd800 && code <= 0xdbff && i + 1 < text.length) {
            const next = text.charCodeAt(i + 1);
            if (next >= 0xdc00 && next <= 0xdfff) {
                bytes += 4;
                i += 1;
            } else {
                bytes += 3;
            }
        } else {
            bytes += 3;
        }
    }
    return bytes;
}

export function isOpaqueId(value: unknown): value is string {
    if (typeof value !== "string" || value.length === 0 || value.length > WORLD_MAX_ID_LEN) {
        return false;
    }
    for (let i = 0; i < value.length; i += 1) {
        const c = value.charCodeAt(i);
        const alnum =
            (c >= 48 && c <= 57) || (c >= 65 && c <= 90) || (c >= 97 && c <= 122);
        if (!(alnum || c === 45 || c === 95 || c === 46)) {
            return false;
        }
    }
    return true;
}

export function isGenerationId(value: unknown): value is string {
    if (typeof value !== "string" || value.length === 0 || value.length > WORLD_MAX_GENERATION_LEN) {
        return false;
    }
    for (let i = 0; i < value.length; i += 1) {
        const c = value.charCodeAt(i);
        if (!((c >= 97 && c <= 122) || (c >= 48 && c <= 57) || c === 45)) {
            return false;
        }
    }
    return true;
}

export function isCorrelationId(value: unknown): value is string {
    return isOpaqueId(value) && (value as string).length <= WORLD_MAX_CORRELATION_LEN;
}
