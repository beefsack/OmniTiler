// World v4 connection family (handshake + revisioned settings): the exact
// connection shapes routed by `type` through
// `tiler_protocol::connection_protocol::route_connection_request` on the
// single discovery D-Bus method. No report/reply/capture vocabulary lives
// here; the report lane (world-report) speaks the same v4 contract
// (WORLD_CONTRACT_VERSION), and there is deliberately no dual protocol
// route: handshake, settings, and report each keep a single v4 shape.
//
// Key orders below mirror the Rust codecs exactly (handshake_request,
// settings, handshake_reply, settings_reply, error); decoders accept order
// differences but exact duplicates compare decoded content.

import {
    isCanonicalU64,
    isGenerationId,
    isOpaqueId,
    WORLD_MAX_REPLY_BYTES,
    WORLD_MAX_REQUEST_BYTES,
    utf8ByteLength,
} from "./world-seq";

// Connection wire version: v4 only. The report lane shares the same
// WORLD_CONTRACT_VERSION; connection messages always carry 4.
export const WORLD_CONNECTION_VERSION = 4;

// Fixed own-process identity token. The KWin scripting surface exposes no
// own-PID, executable-path, or bundle-id API (verified against the v6.8.1
// and v6.6.0 pins: `Window::pid` is per-window only, the base returns -1,
// and no script global names the host process), and the adapter performs
// no D-Bus own-PID lookup: the native -1 sentinel below is the only
// own-process evidence. The token is grammar-valid in the opaque Id
// namespace (lowercase, hyphen, length 13), so it carries exactly like a
// native decimal PID without ever colliding with one.
export const OWN_PROCESS_ID_TOKEN = "kwin-internal";

// Closed FactName catalog in contract order. Unavailable facts ride unique
// and in this order; unknown names refuse.
export const FACT_NAME_CATALOG: ReadonlyArray<string> = [
    "privileged",
    "managed",
    "acceptsInput",
    "activationInhibited",
    "toolWindow",
    "owned",
    "transient",
    "modal",
    "resizable",
    "above",
    "below",
    "owner",
    "parent",
    "role",
    "appId",
    "executable",
    "processId",
    "class",
    "title",
    "activationPolicy",
    "layer",
];

// Fixed KWin host capabilities (hard host limits, never per-report
// values). Truthful per the script-API and actuator audit: host-owned
// native domains, equal-count virtual desktops, and automatic birth clear
// hold; the native selector scope follows the 6.7 per-output API
// (`supportsPerOutputDesktops`: per-output where the read method exists,
// all-outputs on the 6.6 floor, where per-output requests degrade to
// shared). Independent recoverable per-window hide is NOT claimed: the
// adapter owns no minimize/hide setter, minimized windows hold fail-closed
// in world-apply, and native `hidden` only aliases into eligibility, so
// windowSuppression is false and selection rides the host-owned native
// route. The core above-owner route IS claimed: the adapter carries
// StateTarget actuation plus persist-before-write preimages, while the
// automatic keepAbove in world-native is retired (dormant helpers stay
// for the separate cull; no local classification writes), so above is
// true.
// Native maximize (setMaximize/maximizeMode), fullscreen (fullScreen),
// sticky (onAllDesktops), and tiled-float transfer (float intent store)
// have real actuators and stay true; cursor warp is false (the adapter
// never warps, warpCursor stays null, desired warps only log). Unavailable
// classification inputs are exactly the script-unreadable ones: no
// privileged, activationInhibited, toolWindow, or owned flags, no
// activationPolicy/layer, and no executable identity producer (processId
// IS supported via the pid map below; `resizable` stays supported via the
// script `resizeable` bool, src/window.h:541 at the v6.6.0 pin).
export const KWIN_UNAVAILABLE_FACTS: ReadonlyArray<string> = [
    "privileged",
    "activationInhibited",
    "toolWindow",
    "owned",
    "executable",
    "activationPolicy",
    "layer",
];

export interface KwinApplicationIdentity {
    readonly appId: string | null;
    readonly executable: string | null;
    readonly processId: string | null;
}

export interface KwinHostCapabilities {
    readonly hostOwned: boolean;
    readonly equalCount: boolean;
    readonly sharedNativeRoster: boolean;
    readonly birthClear: boolean;
    readonly nativeSelectorScope: "per-output" | "all-outputs";
    readonly windowSuppression: boolean;
    readonly maximize: boolean;
    readonly fullscreen: boolean;
    readonly nativeSticky: boolean;
    readonly above: boolean;
    readonly warp: boolean;
    readonly tiledFloatTransfer: boolean;
    readonly unavailableFacts: ReadonlyArray<string>;
}

export function defaultKwinCapabilities(nativePerOutput: boolean = false): KwinHostCapabilities {
    return {
        hostOwned: true,
        equalCount: true,
        sharedNativeRoster: true,
        birthClear: true,
        nativeSelectorScope: nativePerOutput ? "per-output" : "all-outputs",
        windowSuppression: false,
        maximize: true,
        fullscreen: true,
        nativeSticky: true,
        above: true,
        warp: false,
        tiledFloatTransfer: true,
        unavailableFacts: [...KWIN_UNAVAILABLE_FACTS],
    };
}

// The fixed own application identity for this connection: null appId (no
// truthful native bundle/application id in the script surface, never a
// fallback verdict), null executable (no producer, declared unavailable),
// and the fixed processId token (supported, never null).
export function ownApplicationIdentity(): KwinApplicationIdentity {
    return { appId: null, executable: null, processId: OWN_PROCESS_ID_TOKEN };
}

// Map one raw native window pid (the script `Window.pid` int) to the
// carried v4 processId text in the opaque Id namespace. The native -1 is
// the KWin base-class value inherited by InternalWindows (own internals,
// verified at both pins): it maps to the own token so the core own-process
// exclusion holds. Real positive client PIDs ride as decimal Id text.
// Zero, other non-positive values, non-integers, and unreadables carry
// null (supported-but-unreadable holds only the dependent row, never a
// fabricated negative). Never substitutes appId/resourceClass text.
export function mapNativePid(value: unknown): string | null {
    if (typeof value !== "number" || !Number.isInteger(value)) {
        return null;
    }
    if (value === -1) {
        return OWN_PROCESS_ID_TOKEN;
    }
    if (value <= 0) {
        return null;
    }
    const text = String(value);
    return isOpaqueId(text) ? text : null;
}

export interface HandshakeReply {
    readonly adapter: string;
    readonly req: string;
    readonly gen: string | null;
    readonly ready: boolean;
    readonly kind: string;
}

export interface SettingsDiagnostic {
    readonly key: string;
    readonly kind: string;
}

export interface SettingsReply {
    readonly adapter: string;
    readonly gen: string;
    readonly req: string;
    readonly rev: string;
    readonly status: "accepted" | "stale" | "revision-reused";
    readonly diagnostics: ReadonlyArray<SettingsDiagnostic>;
}

export interface ConnectionError {
    readonly kind: string;
    readonly message: string;
    readonly gen: string;
}

// Bounded error on the connection lane: the broker-bound rejection
// {v,outcome,kind,message,gen,source,nativeInvalid,clock,gestureAssoc}.
// gestureAssoc is always null here (never a fabricated binding).
export function parseConnectionError(text: string): ConnectionError | null {
    if (utf8ByteLength(text) > WORLD_MAX_REPLY_BYTES) {
        return null;
    }
    let value: unknown;
    try {
        value = JSON.parse(text);
    } catch (error) {
        void error;
        return null;
    }
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        return null;
    }
    const r = value as Record<string, unknown>;
    const keys = [...Object.keys(r)].sort();
    const want = ["clock", "gen", "gestureAssoc", "kind", "message", "nativeInvalid", "outcome", "source", "v"].sort();
    if (keys.length !== want.length || !keys.every((k, i) => k === want[i])) {
        return null;
    }
    if (r["v"] !== WORLD_CONNECTION_VERSION || r["outcome"] !== "rejected") {
        return null;
    }
    if (typeof r["kind"] !== "string" || (r["kind"] as string) === "") {
        return null;
    }
    if (typeof r["message"] !== "string" || utf8ByteLength(r["message"] as string) > 4096) {
        return null;
    }
    const gen = r["gen"];
    if (typeof gen !== "string" || gen.length > 64 || (gen !== "" && !isGenerationId(gen))) {
        return null;
    }
    if (r["source"] !== "") {
        return null;
    }
    if (typeof r["nativeInvalid"] !== "string" || !isCanonicalU64(r["nativeInvalid"] as string)) {
        return null;
    }
    if (typeof r["clock"] !== "string" || !isCanonicalU64(r["clock"] as string)) {
        return null;
    }
    if (r["gestureAssoc"] !== null) {
        return null;
    }
    return { kind: r["kind"] as string, message: r["message"] as string, gen: gen as string };
}

function isIdentityText(value: unknown): value is string {
    return (
        typeof value === "string" &&
        value.length > 0 &&
        utf8ByteLength(value) <= 4096 &&
        !value.includes("\0")
    );
}

function checkUnavailableFacts(facts: unknown): facts is ReadonlyArray<string> {
    if (!Array.isArray(facts)) {
        return false;
    }
    let prev = -1;
    const seen = new Set<string>();
    for (const fact of facts) {
        if (typeof fact !== "string") {
            return false;
        }
        const at = FACT_NAME_CATALOG.indexOf(fact);
        if (at < 0 || seen.has(fact) || at <= prev) {
            return false;
        }
        seen.add(fact);
        prev = at;
    }
    return true;
}

function checkIdentityConsistency(
    identity: { readonly appId: unknown; readonly executable: unknown; readonly processId: unknown },
    unavailable: ReadonlyArray<string>,
): boolean {
    const has = (name: string): boolean => unavailable.indexOf(name) >= 0;
    if (identity.appId !== null && identity.appId !== undefined && !isIdentityText(identity.appId)) {
        return false;
    }
    if (identity.executable !== null && identity.executable !== undefined) {
        if (!isIdentityText(identity.executable)) {
            return false;
        }
        if (has("executable")) {
            return false;
        }
    } else if (!has("executable")) {
        return false;
    }
    if (identity.processId !== null && identity.processId !== undefined) {
        if (!isOpaqueId(identity.processId)) {
            return false;
        }
        if (has("processId")) {
            return false;
        }
    } else if (!has("processId")) {
        return false;
    }
    return true;
}

function esc(value: string): string {
    return JSON.stringify(value) ?? "null";
}

// Exact v4 handshake request. Fail-closed (null) on any invalid field:
// the entry never sends a malformed handshake.
export function buildHandshakeRequest(
    adapter: string,
    req: string,
    identity: KwinApplicationIdentity,
    capabilities: KwinHostCapabilities,
): string | null {
    if (!isOpaqueId(adapter) || !isOpaqueId(req)) {
        return null;
    }
    if (
        (identity.appId !== null && !isIdentityText(identity.appId)) ||
        (identity.executable !== null && !isIdentityText(identity.executable)) ||
        (identity.processId !== null && !isOpaqueId(identity.processId))
    ) {
        return null;
    }
    if (!checkUnavailableFacts(capabilities.unavailableFacts)) {
        return null;
    }
    if (
        !checkIdentityConsistency(identity, capabilities.unavailableFacts)
    ) {
        return null;
    }
    if (
        typeof capabilities.hostOwned !== "boolean" ||
        typeof capabilities.equalCount !== "boolean" ||
        typeof capabilities.sharedNativeRoster !== "boolean" ||
        typeof capabilities.birthClear !== "boolean" ||
        (capabilities.nativeSelectorScope !== "per-output" &&
            capabilities.nativeSelectorScope !== "all-outputs") ||
        typeof capabilities.windowSuppression !== "boolean" ||
        typeof capabilities.maximize !== "boolean" ||
        typeof capabilities.fullscreen !== "boolean" ||
        typeof capabilities.nativeSticky !== "boolean" ||
        typeof capabilities.above !== "boolean" ||
        typeof capabilities.warp !== "boolean" ||
        typeof capabilities.tiledFloatTransfer !== "boolean"
    ) {
        return null;
    }
    const id = (v: string | null): string => (v === null ? "null" : esc(v));
    const facts = capabilities.unavailableFacts.map(esc).join(",");
    const out =
        `{"type":"handshake","v":${WORLD_CONNECTION_VERSION},"adapter":${esc(adapter)},"req":${esc(req)},` +
        `"applicationIdentity":{"appId":${id(identity.appId)},"executable":${id(identity.executable)},"processId":${id(identity.processId)}},` +
        `"capabilities":{"hostOwned":${capabilities.hostOwned},"equalCount":${capabilities.equalCount},"sharedNativeRoster":${capabilities.sharedNativeRoster},` +
        `"birthClear":${capabilities.birthClear},"nativeSelectorScope":${esc(capabilities.nativeSelectorScope)},` +
        `"windowSuppression":${capabilities.windowSuppression},"maximize":${capabilities.maximize},` +
        `"fullscreen":${capabilities.fullscreen},"nativeSticky":${capabilities.nativeSticky},` +
        `"above":${capabilities.above},"warp":${capabilities.warp},` +
        `"tiledFloatTransfer":${capabilities.tiledFloatTransfer},"unavailableFacts":[${facts}]}}`;
    if (utf8ByteLength(out) > WORLD_MAX_REQUEST_BYTES) {
        return null;
    }
    return out;
}

const HANDSHAKE_KINDS: ReadonlySet<string> = new Set([
    "malformed",
    "oversized",
    "snapshot-invalid",
    "unknown-field",
    "unknown-value",
    "unsupported-version",
    "owner-invalid",
    "correlation-invalid",
    "generation-mismatch",
    "observation-stale",
    "request-superseded",
    "request-reused",
    "reply-oversize",
    "revision-exhausted",
    "creation-exhausted",
]);

// Exact v4 handshake-reply parse, mirroring the Rust shape rule: accepted
// handshakes carry ready:false with a non-null generation and empty kind;
// refusals carry ready:false with a null generation and a non-empty kind;
// ready:true with a non-null generation and empty kind is wire-valid but
// never binds here (the entry only accepts ready:false). Any other
// combination refuses. The reply byte bound runs before JSON.parse.
export function parseHandshakeReply(text: string): HandshakeReply | null {
    if (utf8ByteLength(text) > WORLD_MAX_REPLY_BYTES) {
        return null;
    }
    let value: unknown;
    try {
        value = JSON.parse(text);
    } catch (error) {
        void error;
        return null;
    }
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        return null;
    }
    const r = value as Record<string, unknown>;
    const keys = [...Object.keys(r)].sort();
    const want = ["adapter", "gen", "kind", "req", "ready", "type", "v"].sort();
    if (keys.length !== want.length || !keys.every((k, i) => k === want[i])) {
        return null;
    }
    if (r["type"] !== "handshake-reply" || r["v"] !== WORLD_CONNECTION_VERSION) {
        return null;
    }
    if (!isOpaqueId(r["adapter"]) || !isOpaqueId(r["req"])) {
        return null;
    }
    const gen = r["gen"];
    if (gen !== null && !isGenerationId(gen)) {
        return null;
    }
    if (typeof r["ready"] !== "boolean") {
        return null;
    }
    const ready = r["ready"] as boolean;
    if (typeof r["kind"] !== "string") {
        return null;
    }
    const kind = r["kind"] as string;
    if (kind !== "" && !HANDSHAKE_KINDS.has(kind)) {
        return null;
    }
    if (ready && (gen === null || kind !== "")) {
        return null;
    }
    if (!ready && kind !== "" && gen !== null) {
        return null;
    }
    if (!ready && kind === "" && gen === null) {
        return null;
    }
    return {
        adapter: r["adapter"] as string,
        req: r["req"] as string,
        gen: gen as string | null,
        ready,
        kind,
    };
}

// Canonical settings key order (mirrors the Rust SETTINGS_ORDER); unknown
// keys ride after, sorted. Known keys the adapter never sources (output
// targeting, cursor, border/underlay presentation) are omitted: the core
// backfills defaults at startup, and adding native config surface for
// them is later-lane work, not connection foundation.
export const SETTINGS_KEY_ORDER: ReadonlyArray<string> = [
    "gap",
    "outer",
    "defaultTiled",
    "mode",
    "sameAxis",
    "bornMaximized",
    "predicate",
    "refill",
    "switchTarget",
    "freshPlacement",
    "focusFollowsCursor",
    "cursorWarp",
    "borderEnabled",
    "underlayEnabled",
    "borderWidth",
    "borderGap",
    "borderRadius",
    "underlayExtension",
];

function orderSettingKeys(keys: ReadonlyArray<string>): string[] {
    const known = SETTINGS_KEY_ORDER.filter((k) => keys.indexOf(k) >= 0);
    const extra = keys.filter((k) => SETTINGS_KEY_ORDER.indexOf(k) < 0).sort();
    return [...known, ...extra];
}

function isFiniteJsonNumber(value: unknown): value is number {
    return typeof value === "number" && Number.isFinite(value);
}

function checkSettingValue(value: unknown): boolean {
    if (value === null || typeof value === "boolean" || typeof value === "string") {
        return true;
    }
    if (typeof value === "number") {
        return isFiniteJsonNumber(value);
    }
    if (Array.isArray(value)) {
        return (value as ReadonlyArray<unknown>).every(checkSettingValue);
    }
    if (typeof value === "object") {
        return Object.keys(value as Record<string, unknown>).every(
            (k) => utf8ByteLength(k) <= 4096 && checkSettingValue((value as Record<string, unknown>)[k]),
        );
    }
    return false;
}

// Exact v4 settings request. Values ride raw (no adapter clamp/default:
// the core validates each field); keys over 4096 bytes, duplicate keys,
// and non-finite numbers fail closed. rev is the independent per-lifetime
// settings revision (canonical u64, never the report/capture sequence).
export function buildSettingsRequest(
    adapter: string,
    gen: string,
    req: string,
    rev: string,
    values: ReadonlyArray<readonly [string, unknown]>,
): string | null {
    if (!isOpaqueId(adapter) || !isOpaqueId(req) || !isGenerationId(gen)) {
        return null;
    }
    if (!isCanonicalU64(rev)) {
        return null;
    }
    const seen = new Set<string>();
    for (const [key, value] of values) {
        if (typeof key !== "string" || key.length === 0 || utf8ByteLength(key) > 4096 || seen.has(key)) {
            return null;
        }
        seen.add(key);
        if (!checkSettingValue(value)) {
            return null;
        }
    }
    const ordered = orderSettingKeys([...seen]);
    const byKey = new Map<string, unknown>(values.map(([k, v]) => [k, v] as [string, unknown]));
    const body = ordered
        .map((k) => `${esc(k)}:${JSON.stringify(byKey.get(k) as unknown) ?? "null"}`)
        .join(",");
    const out =
        `{"type":"settings","v":${WORLD_CONNECTION_VERSION},"adapter":${esc(adapter)},` +
        `"gen":${esc(gen)},"req":${esc(req)},"rev":${esc(rev)},"values":{${body}}}`;
    if (utf8ByteLength(out) > WORLD_MAX_REQUEST_BYTES) {
        return null;
    }
    return out;
}

const SETTINGS_STATUSES: ReadonlySet<string> = new Set(["accepted", "stale", "revision-reused"]);
const SETTINGS_DIAGNOSTIC_KINDS: ReadonlySet<string> = new Set([
    "missing",
    "unknown-field",
    "wrong-type",
    "out-of-range",
    "unknown-value",
    "restart-required",
    "unsupported-mode",
    "unsupported-capability",
]);

// Exact v4 settings-reply parse: full envelope correlation plus revision
// disposition plus at most one diagnostic per affected catalog key. The
// reply byte bound runs before JSON.parse.
export function parseSettingsReply(text: string): SettingsReply | null {
    if (utf8ByteLength(text) > WORLD_MAX_REPLY_BYTES) {
        return null;
    }
    let value: unknown;
    try {
        value = JSON.parse(text);
    } catch (error) {
        void error;
        return null;
    }
    if (typeof value !== "object" || value === null || Array.isArray(value)) {
        return null;
    }
    const r = value as Record<string, unknown>;
    const keys = [...Object.keys(r)].sort();
    const want = ["adapter", "diagnostics", "gen", "req", "rev", "status", "type", "v"].sort();
    if (keys.length !== want.length || !keys.every((k, i) => k === want[i])) {
        return null;
    }
    if (r["type"] !== "settings-reply" || r["v"] !== WORLD_CONNECTION_VERSION) {
        return null;
    }
    if (!isOpaqueId(r["adapter"]) || !isOpaqueId(r["req"]) || !isGenerationId(r["gen"])) {
        return null;
    }
    if (typeof r["rev"] !== "string" || !isCanonicalU64(r["rev"] as string)) {
        return null;
    }
    if (typeof r["status"] !== "string" || !SETTINGS_STATUSES.has(r["status"] as string)) {
        return null;
    }
    if (!Array.isArray(r["diagnostics"])) {
        return null;
    }
    const seen = new Set<string>();
    const diagnostics: SettingsDiagnostic[] = [];
    for (const entry of r["diagnostics"] as ReadonlyArray<unknown>) {
        if (typeof entry !== "object" || entry === null || Array.isArray(entry)) {
            return null;
        }
        const d = entry as Record<string, unknown>;
        const dKeys = [...Object.keys(d)].sort();
        if (dKeys.length !== 2 || dKeys[0] !== "key" || dKeys[1] !== "kind") {
            return null;
        }
        if (typeof d["key"] !== "string" || utf8ByteLength(d["key"] as string) > 4096 || seen.has(d["key"] as string)) {
            return null;
        }
        if (typeof d["kind"] !== "string" || !SETTINGS_DIAGNOSTIC_KINDS.has(d["kind"] as string)) {
            return null;
        }
        seen.add(d["key"] as string);
        diagnostics.push({ key: d["key"] as string, kind: d["kind"] as string });
    }
    return {
        adapter: r["adapter"] as string,
        gen: r["gen"] as string,
        req: r["req"] as string,
        rev: r["rev"] as string,
        status: r["status"] as "accepted" | "stale" | "revision-reused",
        diagnostics,
    };
}
