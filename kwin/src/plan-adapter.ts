// Bounded DescribePlan adapter (single route).
//
// KWin-side observation and actuation only. Rust owns all tiling, order,
// membership, and rejection policy through the stateless DescribePlan route:
// the adapter sends the complete normalized observation (stable opaque ids,
// frame rectangles, output, workspace, focus) plus one parameterized command
// and applies the complete reply geometries in the shared canonical
// grow-before-shrink order. Rejections are always recoverable: the adapter
// never disables itself after a reply and a fresh observation recovers on
// the next command. Stale replies are fenced against newer observations by
// epoch plus correlation.
//
// Ordinary diagnostics retain terminal command outcomes, rejections, scope,
// fence, and refusal evidence. Dispatch and per-member geometry detail are
// trace-only so background reconciliation cannot flood the KWin journal. No
// captions or sensitive payload detail is logged beyond the stable opaque
// window id.

import { orderGeometryWrites } from "./geometry-order";
import { normalizeSameAxisMove } from "./same-axis-move";
import { BORN_MAXIMIZED_DEFAULT, normalizeBornMaximized } from "./born-maximized";
import {
    applyNewcomerOutputRoute,
    decideNewcomerOutputRoute,
    type NewcomerRouteDomain,
} from "./newcomer-output-route";
import {
    FIXED_SIZE_PREDICATE_DEFAULT,
    FIXED_SIZE_PREDICATE_EITHER,
    normalizeFixedSizePredicate,
} from "./fixed-size-predicate";
import { KWIN_TRACE_ENABLED } from "./trace";

export const PLAN_SERVICE = "com.omnitiler.Planner";
export const PLAN_OBJECT = "/com/omnitiler/Planner";
export const PLAN_INTERFACE = "com.omnitiler.Planner1";
export const PLAN_METHOD = "DescribePlan";

// Session D-Bus activation transport (one-flight, bounded, no poll/retry).
// Mirrors the workspace-send activation design minimally: NameHasOwner's
// strict boolean reply distinguishes presence; a present name is resolved via
// GetNameOwner and pinned; only a strictly absent name runs one
// StartServiceByName(service, 0) accepting 1 PrimaryOwner / 2 AlreadyOwner,
// then one post-start GetNameOwner before any planner method. Every planner
// call targets the pinned unique `:N.M` owner. No Legacy path.
export const PLAN_DBUS_SERVICE = "org.freedesktop.DBus";
export const PLAN_DBUS_OBJECT = "/org/freedesktop/DBus";
export const PLAN_DBUS_INTERFACE = "org.freedesktop.DBus";
export const PLAN_HAS_OWNER_METHOD = "NameHasOwner";
export const PLAN_GET_OWNER_METHOD = "GetNameOwner";
export const PLAN_START_METHOD = "StartServiceByName";
export const PLAN_START_FLAGS = 0;
export const PLAN_START_PRIMARY = 1;
export const PLAN_START_ALREADY = 2;

// Settled intent persistence (Q3 float plus D7 explicit fixed-window tile
// overrides): Rust-owned private runtime membership store over the existing
// Planner1 interface (same service, object, and interface as DescribePlan;
// well-known name so D-Bus activation can start a stopped planner).
// Membership only per side: normalized native ids, never geometry, focus,
// stacking, or sticky history. Float and tile sides stay disjoint.
// ReadFloatIntent {"v":1,"correlation_id","live"?[]} prunes both stored sets
// to the caller-attested complete inventory when live is present; the file
// is never mutated by a read. The ok reply carries members/stored/returned
// plus tile/tile_stored/tile_returned (absent tile reads as empty).
// WriteFloatIntent {"v":1,"correlation_id", "members":[full settled float],
// "tile"?[full tile overrides]} replaces both snapshots (each empty clears
// its side; absent tile means an empty full tile snapshot).
// Both methods share the planner's own intent lock (separate from plan),
// so concurrent intent calls fail as busy; KWin callDBus errors may never
// callback, so every intent call carries its own bounded deadline. Intent
// logs carry correlation, outcome, float and tile counts, and fixed reasons
// only.
export const INTENT_READ_METHOD = "ReadFloatIntent";
export const INTENT_WRITE_METHOD = "WriteFloatIntent";
// Full 1024-member snapshot with margin, mirroring the Rust wire caps.
export const INTENT_MAX_BYTES = 256 * 1024;
export const INTENT_MAX_MEMBERS = 1024;
export const INTENT_TIMEOUT_MS = 2000;

export const PLAN_CONTRACT_VERSION = 1;
// Planner request byte cap (mirrors the Rust codec `PLAN_MAX_REQUEST_BYTES`).
// `.length` is an exact byte count here: every string reaching request JSON
// is a fixed literal or passes isOpaqueId/isGeneration/isCorrelationId
// (charsets [A-Za-z0-9_.-] / [a-z0-9-], all <= 0x7F), and JSON numbers,
// booleans, and escapes are ASCII-only. Non-ASCII can never reach a request
// payload: validation rejects it before any build.
export const PLAN_MAX_REQUEST_BYTES = 1_048_576;
export const PLAN_MAX_REPLY_BYTES = 64 * 1024;
export const PLAN_TIMEOUT_MS = 2000;
export const PLAN_DEBOUNCE_MS = 120;
export const MAX_RECONCILE_ATTEMPTS = 3;
export const PLAN_MAX_CORRELATION_LEN = 128;
export const PLAN_MAX_OWNER_LEN = 128;
export const PLAN_MAX_GENERATION_LEN = 64;
export const PLAN_MAX_ID_LEN = 128;
export const PLAN_MAX_SEQ = 1000000;

// KWin's public Script API exposes Window.output read-only and has no public
// output-transfer operation. A desktop-membership setter and target geometry
// do not prove an output transfer, so active R4 moves stay unavailable
// unless the entry supplies every R4 native capability (exact Output object
// resolution, exact VirtualDesktop resolution, sendClientToScreen transfer,
// mover desktop-membership write, output/membership/geometry reads, and the
// outputChanged/desktopsChanged/frameGeometryChanged fences). The request
// `cross_output_transfer` flag is true only when all of them are present.
export function crossOutputTransferSupported(env: PlanAdapterEnv): boolean {
    return (
        typeof env.resolveOutput === "function" &&
        typeof env.resolveDesktop === "function" &&
        typeof env.sendClientToScreen === "function" &&
        typeof env.setDesktops === "function" &&
        typeof env.readOutputName === "function" &&
        typeof env.readDesktopIds === "function" &&
        typeof env.readGeometry === "function" &&
        typeof env.subscribeMoverOutput === "function" &&
        typeof env.subscribeMoverDesktops === "function" &&
        typeof env.subscribeWindowGeometry === "function"
    );
}

const LOG_PREFIX = "omnitiler:plan";

export type PlanDirection = "left" | "right" | "up" | "down";
export type PlanResizeMode = "inwards" | "outwards";
export type PlanSignal = "added" | "removed" | "activated" | "geometry" | "scope" | "fullscreen" | "maximize" | "desktops";
export type PlanOp = "admit" | "remove" | "move" | "focus" | "resize" | "reconcile" | "update-gaps" | "pointer-resize" | "toggle-float" | "toggle-orientation" | "drag-drop" | "release-domain";

// Read-only drag preview result for the overlay sender: the proposed source
// rectangle plus the verbatim carried hover state for the next preview or the
// final drop. `hoverPrior` is the exact Rust `hover_prior` wire value.
export interface PlanDragPreviewResult {
    readonly rect: PlanRect;
    readonly hoverPrior: unknown;
}
export type NativeStateWriteOutcome = "invoked" | "missing" | "threw";
export type MaximizeClearOutcome = NativeStateWriteOutcome;
export type KeepAboveWriteOutcome = NativeStateWriteOutcome | "refused";

export interface PlanRect {
    readonly x: number;
    readonly y: number;
    readonly w: number;
    readonly h: number;
}

export interface PlanWindowConstraints {
    readonly resizeable: boolean | null;
    readonly minSize: { readonly w: number; readonly h: number } | null;
    readonly maxSize: { readonly w: number; readonly h: number } | null;
}

export interface PlanObservedWindow {
    readonly id: string;
    readonly ref: object;
    readonly rect: PlanRect;
    readonly output: string;
    readonly workspace: string;
    readonly fullscreen: boolean;
    readonly maximized: boolean;
    readonly floating?: boolean;
    readonly sticky?: boolean;
    readonly resourceClass?: string;
}

// Production directional domain descriptor: bounded primitive per domain
// (output, workspace, work-area bounds, inner/outer gaps, four-direction
// reciprocal adjacency). At most two entries: source first, then the
// adjacent output's current logical workspace (which may differ in workspace
// id). FULL output rectangles select the neighbor adapter-side; carried
// bounds stay work areas. All four cardinal adjacency keys are admitted.
export interface PlanDomain {
    readonly output: string;
    readonly workspace: string;
    readonly bounds: PlanRect;
    readonly gap: number;
    readonly outerGap: number;
    readonly adjacent: Readonly<Partial<Record<"left" | "right" | "up" | "down", string>>>;
}

    // Typed production directional observation outcome (active route).
    // `ready` carries the validated two-domain observation; `no-target`
    // means a confirmed no-adjacent/single-output condition and keeps local
    // single-domain behavior; `invalid` covers ambiguous, unreadable, or
    // malformed two-domain evidence and must refuse before any local
    // mutation. Item 5 authorizes moves and output sends across outputs in
    // all four directions; focus consults this hook for left/right only
    // (up/down stay local single-domain).
export type DirectionalObservationStatus = "ready" | "no-target" | "invalid";

export interface DirectionalObservation {
    readonly status: DirectionalObservationStatus;
    readonly observed: PlanObserved | null;
}

export interface PlanObserved {
    readonly domainOutput: string;
    readonly domainWorkspace: string;
    readonly domainBounds: PlanRect;
    readonly domainGap: number;
    readonly domainOuterGap: number;
    readonly focusedId: string;
    // The active native window is intentionally floating and therefore not a
    // planner member. A surviving tiled member may supply focusedId solely for
    // the removal observation; interactive tiled commands must refuse.
    readonly activeExcluded?: boolean;
    // Production directional observation: source plus at most one
    // reciprocal adjacent domain in any of the four directions. Absent for legacy
    // single-domain observations. Windows carry their exact output/workspace
    // across both domains; the fingerprint binds the full observation.
    readonly domains?: ReadonlyArray<PlanDomain>;
    readonly windows: ReadonlyArray<PlanObservedWindow>;
    readonly activeRef: object;
    readonly fingerprint: string;
    readonly revalidate: () => boolean;
}

// Primitive-only snapshot retained across the async D-Bus boundary. Never
// holds Window objects, refs, or revalidation closures: ids, geometry values,
// resource classes, scope, and fingerprint only. Targets are resolved from a
// fresh synchronous observation while handling the reply.
export interface PlanSnapshotWindow {
    readonly id: string;
    readonly rect: PlanRect;
    readonly output: string;
    readonly workspace: string;
    readonly fullscreen: boolean;
    readonly maximized: boolean;
    readonly floating: boolean;
    readonly sticky?: boolean;
    readonly resourceClass: string;
    // AR12 client size hints captured freshly from `readWindowConstraints`
    // at observation time (primitive only, advisory, never retained state).
    // Absent when the host reports no hint there. Hints never join the
    // fingerprint and never affect snapshot equality/membership: they only
    // ride the request wire as `min_size`/`max_size`.
    readonly minSize?: { readonly w: number; readonly h: number };
    readonly maxSize?: { readonly w: number; readonly h: number };
    // Q2 fixed-size origin signals (D3/D6): adapter-asserted provenance for
    // the core classifier. `fixedAuto` marks classifier-created automatic
    // floats; `fixedSuppress` pins a live-tile win across hide, release,
    // and re-adoption. Absent unless true so older payloads stay identical.
    readonly fixedAuto?: boolean;
    readonly fixedSuppress?: boolean;
}

export interface PlanSnapshot {
    readonly domainOutput: string;
    readonly domainWorkspace: string;
    readonly domainBounds: PlanRect;
    readonly domainGap: number;
    readonly domainOuterGap: number;
    readonly focusedId: string;
    // Retained directional domains (primitive only, no refs). Absent for
    // legacy single-domain snapshots.
    readonly domains?: ReadonlyArray<PlanDomain>;
    readonly windows: ReadonlyArray<PlanSnapshotWindow>;
    readonly fingerprint: string;
}

export function snapshotOf(observed: PlanObserved): PlanSnapshot {
    const windows = observed.windows.map((entry) => ({
        id: entry.id,
        rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
        output: entry.output,
        workspace: entry.workspace,
        fullscreen: entry.fullscreen,
        maximized: entry.maximized,
        floating: entry.floating === true,
        sticky: entry.sticky === true,
        resourceClass: isOpaqueId(entry.resourceClass) ? entry.resourceClass : "unknown",
    }));
    const domains =
        observed.domains === undefined
            ? undefined
            : Object.freeze(
                  observed.domains.map((entry) =>
                      Object.freeze({
                          output: entry.output,
                          workspace: entry.workspace,
                          bounds: { x: entry.bounds.x, y: entry.bounds.y, w: entry.bounds.w, h: entry.bounds.h },
                          gap: entry.gap,
                          outerGap: entry.outerGap,
                          adjacent: Object.freeze({ ...(entry.adjacent as Record<string, string>) }),
                      }),
                  ),
              );
    return {
        domainOutput: observed.domainOutput,
        domainWorkspace: observed.domainWorkspace,
        domainBounds: {
            x: observed.domainBounds.x,
            y: observed.domainBounds.y,
            w: observed.domainBounds.w,
            h: observed.domainBounds.h,
        },
        domainGap: observed.domainGap,
        domainOuterGap: observed.domainOuterGap,
        focusedId: observed.focusedId,
        ...(domains === undefined ? {} : { domains }),
        windows,
        fingerprint: observed.fingerprint,
    };
}

// Clamp a rect into the domain bounds, preserving size where it fits. The
// result is always a valid contained carried rect; used only as the retained
// representation for a fullscreen window observed before any retained
// baseline exists (admitted while already fullscreen).
function clampCarriedRect(rect: PlanRect, bounds: PlanRect): PlanRect {
    const w = Math.min(rect.w, bounds.w);
    const h = Math.min(rect.h, bounds.h);
    const maxX = bounds.x + bounds.w - w;
    const maxY = bounds.y + bounds.h - h;
    return {
        x: Math.min(Math.max(rect.x, bounds.x), maxX),
        y: Math.min(Math.max(rect.y, bounds.y), maxY),
        w,
        h,
    };
}

function rectContained(inner: PlanRect, outer: PlanRect): boolean {
    return (
        inner.x >= outer.x &&
        inner.y >= outer.y &&
        inner.x + inner.w <= outer.x + outer.w &&
        inner.y + inner.h <= outer.y + outer.h
    );
}

// Directional domains equality: same length with identical primitive
// entries in order (output, workspace, bounds, gaps, adjacency). Binds the
// stale-scope fence to the full source+target observation so a stale target
// substitution cannot pass as fresh.
function domainsEqual(
    a: ReadonlyArray<PlanDomain> | undefined,
    b: ReadonlyArray<PlanDomain> | undefined,
): boolean {
    if (a === undefined || b === undefined) {
        return a === undefined && b === undefined;
    }
    if (a.length !== b.length) {
        return false;
    }
    for (let index = 0; index < a.length; index += 1) {
        const left = a[index] as PlanDomain;
        const right = b[index] as PlanDomain;
        if (
            left.output !== right.output ||
            left.workspace !== right.workspace ||
            left.bounds.x !== right.bounds.x ||
            left.bounds.y !== right.bounds.y ||
            left.bounds.w !== right.bounds.w ||
            left.bounds.h !== right.bounds.h ||
            left.gap !== right.gap ||
            left.outerGap !== right.outerGap
        ) {
            return false;
        }
        const leftKeys = Object.keys(left.adjacent).sort();
        const rightKeys = Object.keys(right.adjacent).sort();
        if (leftKeys.length !== rightKeys.length) {
            return false;
        }
        for (let keyIndex = 0; keyIndex < leftKeys.length; keyIndex += 1) {
            if (leftKeys[keyIndex] !== rightKeys[keyIndex]) {
                return false;
            }
            const key = leftKeys[keyIndex] as string;
            if (
                (left.adjacent as Record<string, string>)[key] !==
                (right.adjacent as Record<string, string>)[key]
            ) {
                return false;
            }
        }
    }
    return true;
}

function snapshotsEqual(a: PlanSnapshot, b: PlanSnapshot): boolean {
    if (
        a.domainOutput !== b.domainOutput ||
        a.domainWorkspace !== b.domainWorkspace ||
        a.domainGap !== b.domainGap ||
        a.domainOuterGap !== b.domainOuterGap ||
        a.focusedId !== b.focusedId ||
        a.fingerprint !== b.fingerprint ||
        !domainsEqual(a.domains, b.domains)
    ) {
        return false;
    }
    if (
        a.domainBounds.x !== b.domainBounds.x ||
        a.domainBounds.y !== b.domainBounds.y ||
        a.domainBounds.w !== b.domainBounds.w ||
        a.domainBounds.h !== b.domainBounds.h
    ) {
        return false;
    }
    if (a.windows.length !== b.windows.length) {
        return false;
    }
    const byId = new Map<string, PlanSnapshotWindow>();
    for (const entry of a.windows) {
        byId.set(entry.id, entry);
    }
    for (const entry of b.windows) {
        const other = byId.get(entry.id);
        if (other === undefined) {
            return false;
        }
        if (
            other.rect.x !== entry.rect.x ||
            other.rect.y !== entry.rect.y ||
            other.rect.w !== entry.rect.w ||
            other.rect.h !== entry.rect.h ||
            other.output !== entry.output ||
            other.workspace !== entry.workspace ||
            other.fullscreen !== entry.fullscreen ||
            other.maximized !== entry.maximized
        ) {
            return false;
        }
    }
    return true;
}

// Structural scope identity shared by PlanSnapshot and PlanObserved (the R4
// fences compare a fresh PlanObserved against the dispatch PlanSnapshot).
// Windows contribute identity only (id/output/workspace); rects, flags,
// focus, and fingerprint are never scope.
interface ScopeIdentity {
    readonly domainOutput: string;
    readonly domainWorkspace: string;
    readonly domainGap: number;
    readonly domainOuterGap: number;
    readonly domainBounds: PlanRect;
    readonly domains?: ReadonlyArray<PlanDomain> | undefined;
    readonly windows: ReadonlyArray<{ readonly id: string; readonly output: string; readonly workspace: string }>;
}

function sameDomainScalars(a: ScopeIdentity, b: ScopeIdentity): boolean {
    return (
        a.domainOutput === b.domainOutput &&
        a.domainWorkspace === b.domainWorkspace &&
        a.domainGap === b.domainGap &&
        a.domainOuterGap === b.domainOuterGap
    );
}

function sameDomainBounds(a: ScopeIdentity, b: ScopeIdentity): boolean {
    return (
        a.domainBounds.x === b.domainBounds.x &&
        a.domainBounds.y === b.domainBounds.y &&
        a.domainBounds.w === b.domainBounds.w &&
        a.domainBounds.h === b.domainBounds.h
    );
}

// Complete window-set identity ignoring rects: same length with identical
// per-id output/workspace membership.
function sameWindowMembership(a: ScopeIdentity, b: ScopeIdentity): boolean {
    if (a.windows.length !== b.windows.length) {
        return false;
    }
    const byId = new Map<string, { readonly output: string; readonly workspace: string }>();
    for (const entry of a.windows) {
        byId.set(entry.id, entry);
    }
    for (const entry of b.windows) {
        const other = byId.get(entry.id);
        if (other === undefined || other.output !== entry.output || other.workspace !== entry.workspace) {
            return false;
        }
    }
    return true;
}

function sameScope(a: PlanSnapshot, b: PlanSnapshot): boolean {
    if (!sameDomainScalars(a, b) || !sameDomainBounds(a, b)) {
        return false;
    }
    return sameWindowMembership(a, b);
}

// Work-area reprojection is valid only when the logical domain and complete
// window set are unchanged. Client rectangles are deliberately ignored here:
// they are drift inputs, never a source of retained shares.
function sameDomainAndWindowSet(a: PlanSnapshot, b: PlanSnapshot): boolean {
    if (!sameDomainScalars(a, b) || !domainsEqual(a.domains, b.domains)) {
        return false;
    }
    return sameWindowMembership(a, b);
}

function sameReprojectionScope(a: PlanSnapshot, b: PlanSnapshot): boolean {
    return sameDomainAndWindowSet(a, b) && sameDomainBounds(a, b);
}

// Foreground is strict; hidden forgives sticky multi-homed domain/rect.
interface RefreshClassification {
    readonly pureDrift: boolean;
    readonly converged: boolean;
    readonly rawRetainedOutOfBounds: boolean;
    readonly isEmpty: boolean;
    readonly appliedScope: { bounds: PlanRect; gap: number; outerGap: number } | null;
    readonly scopeEqual: boolean;
    readonly gapsEqual: boolean;
    readonly boundsChanged: boolean;
    readonly observedIds: ReadonlySet<string>;
}

// Hidden-domain decision with bounded diag metadata. Null means incomplete
// (no complete observation, no diag).
interface HiddenDecision {
    readonly intent: AutoIntent | null;
    readonly outcome: "equal" | "change" | "uncertain";
    readonly reason: string;
    readonly terminal: string;
}

// Reply-boundary flag-exactness: like floatingSkewed, but tolerates a
// toggle-float flight's own target reaching its intended end state. The
// toggle choreography observes the target flipped before the reply lands
// (pinned by the sticky/float toggle rows) and must still apply; any other
// member flipping, or the target flipping away from the intended end state,
// fails closed as stale. Baseline-diff skew detection is untouched.
function unexpectedFloatingSkewed(flight: PendingFlight, fresh: PlanSnapshot): boolean {
    const target = flight.floatTarget;
    const freshById = new Map<string, PlanSnapshotWindow>();
    for (const entry of fresh.windows) {
        if (!freshById.has(entry.id)) {
            freshById.set(entry.id, entry);
        }
    }
    for (const entry of flight.snapshot.windows) {
        const current = freshById.get(entry.id);
        if (current === undefined) {
            continue;
        }
        const wasFloating = entry.floating === true || entry.sticky === true;
        const isFloating = current.floating === true || current.sticky === true;
        if (wasFloating === isFloating) {
            continue;
        }
        if (target !== null && entry.id === target.window && target.floating === isFloating) {
            continue;
        }
        return true;
    }
    return false;
}

// Shared lag overlay fence: floating/sticky (with the toggle-float target
// tolerance above) plus fullscreen/maximized exactness. `rectsEqualExceptSource`
// compares membership/rect only, so a survivor flipping fullscreen/maximize
// would otherwise pass both the pre-transfer projection check and the
// post-transfer actual-destination check and apply wrong planned geometry.
// Used only by the two lag checks; all other fences keep their behavior.
function unexpectedOverlaySkewed(flight: PendingFlight, fresh: PlanSnapshot): boolean {
    if (unexpectedFloatingSkewed(flight, fresh)) {
        return true;
    }
    const freshById = new Map<string, PlanSnapshotWindow>();
    for (const entry of fresh.windows) {
        if (!freshById.has(entry.id)) {
            freshById.set(entry.id, entry);
        }
    }
    for (const entry of flight.snapshot.windows) {
        const current = freshById.get(entry.id);
        if (current === undefined) {
            continue;
        }
        if (entry.fullscreen !== current.fullscreen || entry.maximized !== current.maximized) {
            return true;
        }
    }
    return false;
}

// Pointer-only tolerance: identical to snapshotsEqual except the drag
// source rectangle may drift (final native echo before the D-Bus reply).
function rectsEqualExceptSource(a: PlanSnapshot, b: PlanSnapshot, sourceId: string): boolean {
    if (
        a.domainOutput !== b.domainOutput ||
        a.domainWorkspace !== b.domainWorkspace ||
        a.domainGap !== b.domainGap ||
        a.domainOuterGap !== b.domainOuterGap ||
        a.focusedId !== b.focusedId ||
        a.fingerprint !== b.fingerprint
    ) {
        return false;
    }
    if (
        a.domainBounds.x !== b.domainBounds.x ||
        a.domainBounds.y !== b.domainBounds.y ||
        a.domainBounds.w !== b.domainBounds.w ||
        a.domainBounds.h !== b.domainBounds.h
    ) {
        return false;
    }
    if (a.windows.length !== b.windows.length) {
        return false;
    }
    const byId = new Map<string, PlanSnapshotWindow>();
    for (const entry of a.windows) {
        byId.set(entry.id, entry);
    }
    for (const entry of b.windows) {
        const other = byId.get(entry.id);
        if (other === undefined || other.output !== entry.output || other.workspace !== entry.workspace) {
            return false;
        }
        if (entry.id === sourceId) {
            continue;
        }
        if (entry.fullscreen || entry.maximized) {
            continue;
        }
        if (
            other.rect.x !== entry.rect.x ||
            other.rect.y !== entry.rect.y ||
            other.rect.w !== entry.rect.w ||
            other.rect.h !== entry.rect.h
        ) {
            return false;
        }
    }
    return byId.has(sourceId);
}

export interface PlanAdapterEnv {
    readonly callDbus: (
        service: string,
        path: string,
        iface: string,
        method: string,
        payload: string,
        callback: (reply: unknown) => void,
    ) => void;
    readonly scheduleOnce: (delayMs: number, callback: () => void) => () => void;
    readonly log: (message: string) => void;
    readonly observe: () => PlanObserved | null;
    // Hidden-domain observation for background tiling: every non-visible
    // (output, workspace) domain with tiled members, each carrying a
    // deterministic domain-local structural anchor as focusedId (never native
    // focus). Absent in isolated core tests; the entry supplies it in
    // production. Hidden flights only ever admit/remove/reconcile geometry
    // and never route focus or interactive commands.
    readonly observeHidden?: () => ReadonlyArray<PlanObserved>;
    // Production directional observation for focus/move: source
    // plus the reciprocal adjacent output's current desktop
    // (bounded max two domains) with multi-domain windows. Absent in
    // isolated core tests. Focus uses left/right only; move probes all four
    // directions. The optional flight pin freezes the source pair across
    // transfer and arrival: deriving the source from the live active window
    // would flip it once the still-active mover lands on the destination.
    // R4 re-observation pins; dispatch observes unpinned. May return the typed
    // {@link DirectionalObservation} outcome or, for legacy mocks, a bare
    // {@link PlanObserved} (treated as ready) or null (treated as invalid).
    readonly observeDirectional?: (
        direction: PlanDirection,
        pinnedSource?: { readonly output: string; readonly workspace: string; readonly targetOutput?: string },
        forMove?: boolean,
    ) => DirectionalObservation | PlanObserved | null;
    readonly clearMaximize: (target: object) => MaximizeClearOutcome;
    readonly setMaximize?: (target: object, maximized: boolean) => NativeStateWriteOutcome;
    readonly setFullscreen?: (target: object, fullscreen: boolean) => NativeStateWriteOutcome;
    readonly setAllDesktops?: (target: object, allDesktops: boolean) => NativeStateWriteOutcome;
    readonly readKeepAbove?: (target: object) => boolean | null;
    readonly readKeepBelow?: (target: object) => boolean | null;
    readonly setKeepAbove?: (target: object, keepAbove: boolean) => KeepAboveWriteOutcome;
    readonly setKeepBelow?: (target: object, keepBelow: boolean) => KeepAboveWriteOutcome;
    readonly setGeometry: (target: object, rect: PlanRect) => boolean;
    readonly setFloating?: (id: string, floating: boolean) => void;
    // Workspace tiling gate: false means floating/unmanaged, so automatic
    // tiling and geometry writes skip that workspace. Releases bypass it.
    readonly isDomainTiled?: (output: string, workspace: string) => boolean;
    readonly setActive: (target: object) => boolean;
    readonly active: () => object | null;
    // Production R4 cross-output transfer capabilities. All ten must be
    // present for `cross_output_transfer: true`; any absence keeps R4
    // unavailable and local R1-R3 behavior unchanged. The entry binds them
    // to public typed surfaces only: exact Output object resolution by name,
    // exact VirtualDesktop resolution by id, workspace.sendClientToScreen
    // with the exact target Output object, the mover desktops write with the
    // exact target VirtualDesktop refs, synchronous output/membership/
    // geometry reads for proof, and one-shot outputChanged (old value
    // re-read), desktopsChanged, and frameGeometryChanged fences.
    readonly resolveOutput?: (name: string) => object | null;
    readonly resolveDesktop?: (workspace: string) => object | null;
    readonly sendClientToScreen?: (mover: object, output: object) => boolean;
    readonly setDesktops?: (mover: object, desktops: ReadonlyArray<object>) => boolean;
    readonly readOutputName?: (ref: object) => string | null;
    readonly readDesktopIds?: (ref: object) => ReadonlyArray<string> | null;
    // R-OUT-05 newcomer routing reads. `readIsTransient` reports whether the
    // live ref is a transient/dialog client (null when unclassifiable, which
    // refuses routing); `readShownWorkspace` reports the output's currently
    // shown workspace id; `readFullscreenOnDomain` reports whether any live
    // window on that output/workspace is fullscreen (null when unreadable).
    // Absent or null reads refuse routing before any write.
    readonly readIsTransient?: ((ref: object) => boolean | null) | undefined;
    readonly readShownWorkspace?: ((output: string) => string | null) | undefined;
    readonly readFullscreenOnDomain?: ((output: string, workspace: string) => boolean | null) | undefined;
    readonly readGeometry?: (ref: object) => PlanRect | null;
    // Trace-only native constraint read. It is optional so the planner route
    // remains usable on older script surfaces and isolated tests.
    readonly readWindowConstraints?: (ref: object) => PlanWindowConstraints | null;
    readonly subscribeMoverOutput?: (mover: object, handler: (old: unknown) => void) => (() => void) | null;
    readonly subscribeMoverDesktops?: (mover: object, handler: () => void) => (() => void) | null;
    readonly subscribeWindowGeometry?: (ref: object, handler: () => void) => (() => void) | null;
    readonly subscribe: (kind: PlanSignal, handler: (target?: object) => void) => () => void;
    readonly noteRemoved?: (id: string) => void;
    // Exact-ref removal hook: the entry evicts only ids still owned by this
    // live ref, without reading any removed-object properties. Covers
    // pre-apply ids with no applied slot; incomplete observations never infer
    // departure.
    readonly noteNativeRemoved?: (ref: object) => void;
    // Retired workspace-send coordination hook, ignored. Send flights never
    // block Plan: terminal send settlement arrives via `notifySendSettled`,
    // which forces a one-shot complete source AND target reconcile through
    // the existing single-flight chain. Retained as optional only so
    // obsolete harnesses still typecheck; production entries must not pass
    // it and the adapter never reads it.
    readonly isSendActive?: () => boolean;
    // Entry-owned native interaction guard. Ordinary retained reconciliation
    // must not compete with an active interactive edge resize.
    readonly isInteractiveResizeActive?: () => boolean;
    // Observational active-group refresh after exactly one successful
    // geometry-plan boundary (admit/move/remove/resize `planned-applied`).
    // Synchronous, single call, no retries/polling; must never block
    // foreground logical commands. Never invoked on stale/rejected/error
    // or unfinished boundaries.
    readonly onPlannedApplied?: (op: PlanOp) => void;
    // Q3 settled float-intent bridge plus D7 tile overrides (opt-in,
    // capability-gated): the entry supplies both hooks or neither, and only
    // then does the adapter run one bounded ReadFloatIntent before first plan
    // admission plus settled WriteFloatIntent snapshots afterwards.
    // observeCompleteInventory decodes the full live window list (all
    // outputs/desktops including minimized/hidden); complete=false (or
    // null/throw) means the inventory is unproven and the read omits live
    // (no pruning).
    // onIntentHydrated receives the adopted settled float membership plus
    // the restored tile overrides once per bootstrap so the entry can
    // populate its in-progress floating set before initial planning; never
    // invoked after shutdown.
    readonly observeCompleteInventory?: () => {
        readonly complete: boolean;
        readonly ids: ReadonlyArray<string>;
    } | null;
    readonly onIntentHydrated?: (members: ReadonlyArray<string>, tile: ReadonlyArray<string>) => void;
    // R-MOV-03 same-axis move mode for the move wire command. Read live per
    // move request so an entry-owned Options configChanged re-read applies
    // to subsequent moves with no tree rebuild. Absent/invalid resolves to
    // `group-with-neighbor` (wire-omitted, the historical default).
    readonly readSameAxisMove?: () => unknown;
    // R-SPC-04 D1 fixed-size admission predicate. Read live per request so
    // an entry-owned Options configChanged re-read applies to subsequent
    // admissions with no reclassification. Absent/invalid resolves to
    // `both-axes-fixed` (wire-omitted, the delivered default).
    readonly readFixedSizePredicate?: () => unknown;
    // R-MAX-06 born-maximized admission mode. Read live per admission so an
    // entry-owned Options configChanged re-read applies to subsequent
    // admissions with no rebuild. Absent/invalid resolves to
    // `tile-on-launch` (clear once, then admit ordinarily).
    readonly readBornMaximized?: () => unknown;
    // R-MAX-03 floating-first sighting: true when the id was first sighted
    // on an untiled (floating) domain. Such ids keep their maximized overlay
    // over the reserved slot under both born-maximized settings with no
    // one-shot clear; only born tiled-first admissions read the R-MAX-06
    // mode. Absent resolves to false.
    readonly isFloatingFirst?: (id: string) => boolean;
}

export interface PlanEnableAuth {
    readonly owner: unknown;
    readonly generation: unknown;
}

// Deterministic bounded observation fingerprint (FNV-1a 32-bit over
// `output\x1fworkspace\x1ffocused\x1fids...`, sorted ids). Sent as the
// numeric fingerprint; the string cache identity in observations carries the
// same value so dispatch, validation, and repeat tracking bind one value.
export function planFingerprint(
    domainOutput: string,
    domainWorkspace: string,
    focusedId: string,
    sortedIds: readonly string[],
): number {
    let hash = 2166136261;
    const feed = (text: string): void => {
        for (let index = 0; index < text.length; index += 1) {
            hash ^= text.charCodeAt(index) & 0xff;
            hash = Math.imul(hash, 16777619);
        }
    };
    feed(domainOutput);
    hash ^= 0x1f;
    hash = Math.imul(hash, 16777619);
    feed(domainWorkspace);
    hash ^= 0x1f;
    hash = Math.imul(hash, 16777619);
    feed(focusedId);
    for (const id of sortedIds) {
        hash ^= 0x1f;
        hash = Math.imul(hash, 16777619);
        feed(id);
    }
    return hash >>> 0;
}

// Wire-shaped window for the production directional fingerprint: exactly the
// fields carried on the request (id, output, workspace, rect, floating,
// fit-excluded).
export interface DirectionalFingerprintWindow {
    readonly window: string;
    readonly output: string;
    readonly workspace: string;
    readonly rect: PlanRect;
    readonly floating: boolean;
    readonly fitExcluded: boolean;
}

// Canonical production directional fingerprint (FNV-1a 32-bit) over the full
// two-domain evidence, byte-identical to Rust's `directional_fingerprint`:
// ordered domain primitives (output, workspace, raw bounds, gaps,
// left/right/up/down adjacency in fixed order), the focused id, and every
// window sorted by id. Any alteration of target rect, bounds, or adjacency
// changes the value and fails Rust request validation. Legacy single-domain
// requests keep `planFingerprint`.
export function planDirectionalFingerprint(
    domains: ReadonlyArray<PlanDomain>,
    focusedId: string,
    windows: ReadonlyArray<DirectionalFingerprintWindow>,
): number {
    let hash = 2166136261;
    const feed = (text: string): void => {
        for (let index = 0; index < text.length; index += 1) {
            hash ^= text.charCodeAt(index) & 0xff;
            hash = Math.imul(hash, 16777619);
        }
    };
    const sep = (code: number): void => {
        hash ^= code;
        hash = Math.imul(hash, 16777619);
    };
    for (let index = 0; index < domains.length; index += 1) {
        const entry = domains[index] as PlanDomain;
        if (index > 0) {
            sep(0x1e);
        }
        feed(entry.output);
        sep(0x1f);
        feed(entry.workspace);
        sep(0x1f);
        feed(String(entry.bounds.x));
        sep(0x1f);
        feed(String(entry.bounds.y));
        sep(0x1f);
        feed(String(entry.bounds.w));
        sep(0x1f);
        feed(String(entry.bounds.h));
        sep(0x1f);
        feed(String(entry.gap));
        sep(0x1f);
        feed(String(entry.outerGap));
        sep(0x1f);
        feed("left");
        sep(0x1f);
        feed((entry.adjacent as Record<string, string>)["left"] ?? "");
        sep(0x1f);
        feed("right");
        sep(0x1f);
        feed((entry.adjacent as Record<string, string>)["right"] ?? "");
        sep(0x1f);
        feed("up");
        sep(0x1f);
        feed((entry.adjacent as Record<string, string>)["up"] ?? "");
        sep(0x1f);
        feed("down");
        sep(0x1f);
        feed((entry.adjacent as Record<string, string>)["down"] ?? "");
    }
    sep(0x1f);
    feed(focusedId);
    const ordered = [...windows].sort((a, b) => (a.window < b.window ? -1 : a.window > b.window ? 1 : 0));
    for (const entry of ordered) {
        sep(0x1f);
        feed(entry.window);
        sep(0x1f);
        feed(entry.output);
        sep(0x1f);
        feed(entry.workspace);
        sep(0x1f);
        feed(String(entry.rect.x));
        sep(0x1f);
        feed(String(entry.rect.y));
        sep(0x1f);
        feed(String(entry.rect.w));
        sep(0x1f);
        feed(String(entry.rect.h));
        sep(0x1f);
        feed(entry.floating ? "1" : "0");
        sep(0x1f);
        feed(entry.fitExcluded ? "1" : "0");
    }
    return hash >>> 0;
}

function isOpaqueId(value: unknown): value is string {
    if (typeof value !== "string" || value.length === 0 || value.length > PLAN_MAX_ID_LEN) {
        return false;
    }
    for (let index = 0; index < value.length; index += 1) {
        const code = value.charCodeAt(index);
        const alnum =
            (code >= 48 && code <= 57) || (code >= 65 && code <= 90) || (code >= 97 && code <= 122);
        if (!(alnum || code === 45 || code === 95 || code === 46)) {
            return false;
        }
    }
    return true;
}

function isOwnerId(value: unknown): value is string {
    return (
        typeof value === "string" &&
        value.length > 0 &&
        value.length <= PLAN_MAX_OWNER_LEN &&
        isOpaqueId(value)
    );
}

// Exact D-Bus unique-owner shape (`:N.M`) for the pinned planner endpoint.
function isUniqueOwner(value: unknown): value is string {
    return typeof value === "string" && /^:[0-9]+\.[0-9]+$/.test(value);
}

function isGeneration(value: unknown): value is string {
    if (typeof value !== "string" || value.length === 0 || value.length > PLAN_MAX_GENERATION_LEN) {
        return false;
    }
    for (let index = 0; index < value.length; index += 1) {
        const code = value.charCodeAt(index);
        const ok = (code >= 97 && code <= 122) || (code >= 48 && code <= 57) || code === 45;
        if (!ok) {
            return false;
        }
    }
    return true;
}

function isCorrelationId(value: unknown): value is string {
    return (
        typeof value === "string" &&
        value.length > 0 &&
        value.length <= PLAN_MAX_CORRELATION_LEN &&
        isOpaqueId(value)
    );
}

// Drag-N correlation for drop-intent follow-up: validated closed vocabulary
// only (drag-<digits>), never titles, ids, or payload bytes.
function isDragCorrelation(value: unknown): value is string {
    return typeof value === "string" && value.length > 0 && value.length <= PLAN_MAX_CORRELATION_LEN && /^drag-[0-9]+$/.test(value);
}

// Log-safe fixed reason token for the float-intent bridge: Rust reasons are
// a closed vocabulary (namespace-unavailable, unreadable, corrupt,
// namespace-mismatch, unauthorized, not-kwin-owner, invalid-request,
// invalid-members, store-unavailable) plus local causes (transport,
// timeout, malformed, oversize). Anything outside the token alphabet logs
// as "-" so logs carry correlation, counts, and closed reasons only.
function sanitizeIntentReason(value: unknown): string {
    if (typeof value !== "string" || value.length === 0 || value.length > 64) {
        return "-";
    }
    for (let index = 0; index < value.length; index += 1) {
        const code = value.charCodeAt(index);
        const alnum =
            (code >= 48 && code <= 57) || (code >= 65 && code <= 90) || (code >= 97 && code <= 122);
        if (!(alnum || code === 45 || code === 95)) {
            return "-";
        }
    }
    return value;
}

// Rust `hover_prior` wire shape (read-only drag preview carry): exact drag
// source identity plus capture revision and the stored next hover (`prior`
// null/absent when the last hover was not a group edge). Validated here so
// only well-shaped carries ride the next preview or the final drop verbatim;
// semantic validity beyond shape stays advisory in Rust (invalid carries are
// ignored there, never refused).
function isValidHoverPrior(value: unknown): boolean {
    if (!isRecord(value)) {
        return false;
    }
    const keys = Object.keys(value);
    if (keys.length < 5 || keys.length > 6) {
        return false;
    }
    for (const key of ["domain_output", "domain_workspace", "source_leaf", "source_window", "revision"]) {
        if (!Object.prototype.hasOwnProperty.call(value, key)) {
            return false;
        }
    }
    if (
        !isOpaqueId(value["domain_output"]) ||
        !isOpaqueId(value["domain_workspace"]) ||
        !isOpaqueId(value["source_leaf"]) ||
        !isOpaqueId(value["source_window"])
    ) {
        return false;
    }
    const revision = value["revision"];
    if (!isFiniteInt(revision) || (revision as number) < 0 || (revision as number) > PLAN_MAX_SEQ) {
        return false;
    }
    if (!Object.prototype.hasOwnProperty.call(value, "prior")) {
        return keys.length === 5;
    }
    if (keys.length !== 6) {
        return false;
    }
    const prior = value["prior"];
    if (prior === null || prior === undefined) {
        return true;
    }
    if (!isRecord(prior) || !hasExactKeys(prior, ["group", "edge"])) {
        return false;
    }
    if (!isOpaqueId(prior["group"])) {
        return false;
    }
    const edge = prior["edge"];
    return edge === "left" || edge === "right" || edge === "top" || edge === "bottom";
}

function isDirection(value: unknown): value is PlanDirection {
    return value === "left" || value === "right" || value === "up" || value === "down";
}

// Float-origin subject gate: the focused window must resolve to a
// same-domain floating/sticky entry. Anything else (unknown, tiled,
// cross-homed, flag-flipped) refuses before any search or dispatch.
function floatSubjectEntry(observed: PlanObserved): PlanObservedWindow | null {
    for (const entry of observed.windows) {
        if (entry.id === observed.focusedId) {
            if (
                entry.output === observed.domainOutput &&
                entry.workspace === observed.domainWorkspace &&
                (entry.floating === true || entry.sticky === true)
            ) {
                return entry;
            }
            return null;
        }
    }
    return null;
}

// COSMIC float-origin focus target (source `shell/mod.rs` float search):
// strictly same-domain floating/sticky candidates compared by top-left
// coordinate on the requested axis only. Candidate order is the sticky
// layer first in native encounter order, then ordinary floats in native
// encounter order (KWin `windowList` order; ties can differ from COSMIC's
// native Space order). Up/Left admit non-positive deltas (equal included)
// and keep the first nearest tie; Down/Right require strictly positive
// deltas and keep the last nearest tie. Tiles are never candidates.
// Returns the winning entry, or null when there is no local target or the
// subject itself is not an eligible same-domain float/sticky window.
export function selectFloatFocusTarget(
    observed: PlanObserved,
    direction: PlanDirection,
): PlanObservedWindow | null {
    const subject = floatSubjectEntry(observed);
    if (subject === null) {
        return null;
    }
    const vertical = direction === "up" || direction === "down";
    const negative = direction === "up" || direction === "left";
    const sticky: PlanObservedWindow[] = [];
    const ordinary: PlanObservedWindow[] = [];
    for (const entry of observed.windows) {
        if (
            entry.id === observed.focusedId ||
            entry.output !== observed.domainOutput ||
            entry.workspace !== observed.domainWorkspace ||
            (entry.floating !== true && entry.sticky !== true)
        ) {
            continue;
        }
        if (entry.sticky === true) {
            sticky.push(entry);
        } else {
            ordinary.push(entry);
        }
    }
    let best: PlanObservedWindow | null = null;
    let bestDist = 0;
    const consider = (entry: PlanObservedWindow): void => {
        const delta = vertical ? entry.rect.y - subject.rect.y : entry.rect.x - subject.rect.x;
        if (negative) {
            if (delta > 0) {
                return;
            }
        } else if (delta <= 0) {
            return;
        }
        const dist = delta < 0 ? -delta : delta;
        if (best === null || dist < bestDist) {
            best = entry;
            bestDist = dist;
        } else if (dist === bestDist && !negative) {
            // Down/Right keep the last nearest tie; Up/Left keep the first.
            best = entry;
        }
    };
    for (const entry of sticky) {
        consider(entry);
    }
    for (const entry of ordinary) {
        consider(entry);
    }
    return best;
}

// COSMIC float half-snap (stateless first step, `floating/mod.rs`
// `TiledCorners::relative_geometry` at 3d55cba0, caller input
// `layers.non_exclusive_zone()` work-area plus `gaps()` tuple
// `(outer, inner)` with outer ignored): left/top origin `loc + inner`;
// right `x = loc.x + floor(W/2) + floor(inner/2)`; widths
// `floor(W/2) - floor(3*inner/2)`; height `H - 2*inner` (up/down
// analogous). Uses the current domain work-area bounds in global
// coordinates with `domainGap` as inner. Every repeated arrow requests
// the same half again; quarter/maximize/transfer complexity is deferred.
// Returns null for invalid/nonpositive input or output rectangles.
export function floatHalfSnapRect(
    bounds: PlanRect,
    gapInner: number,
    direction: PlanDirection,
): PlanRect | null {
    if (!isTargetRect({ x: bounds.x, y: bounds.y, w: bounds.w, h: bounds.h })) {
        return null;
    }
    if (!isFiniteInt(gapInner) || gapInner < 0 || gapInner > 64) {
        return null;
    }
    if (direction !== "left" && direction !== "right" && direction !== "up" && direction !== "down") {
        return null;
    }
    const inner = gapInner;
    const bx = bounds.x;
    const by = bounds.y;
    const w = bounds.w;
    const h = bounds.h;
    const halfW = Math.trunc(w / 2);
    const halfH = Math.trunc(h / 2);
    const halfInner = Math.trunc(inner / 2);
    const threeHalfInner = Math.trunc((3 * inner) / 2);
    let rect: PlanRect;
    if (direction === "left") {
        rect = { x: bx + inner, y: by + inner, w: halfW - threeHalfInner, h: h - 2 * inner };
    } else if (direction === "right") {
        rect = { x: bx + halfW + halfInner, y: by + inner, w: halfW - threeHalfInner, h: h - 2 * inner };
    } else if (direction === "up") {
        rect = { x: bx + inner, y: by + inner, w: w - 2 * inner, h: halfH - threeHalfInner };
    } else {
        rect = { x: bx + inner, y: by + halfH + halfInner, w: w - 2 * inner, h: halfH - threeHalfInner };
    }
    if (!isTargetRect(rect)) {
        return null;
    }
    return rect;
}

function isResizeMode(value: unknown): value is PlanResizeMode {
    return value === "inwards" || value === "outwards";
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFiniteInt(value: unknown): value is number {
    return typeof value === "number" && Number.isFinite(value) && Number.isInteger(value);
}

function isTargetRect(value: unknown): value is PlanRect {
    if (!isRecord(value)) {
        return false;
    }
    const keys = Object.keys(value);
    if (keys.length !== 4 || keys.indexOf("x") < 0 || keys.indexOf("y") < 0 || keys.indexOf("w") < 0 || keys.indexOf("h") < 0) {
        return false;
    }
    const x = value["x"];
    const y = value["y"];
    const w = value["w"];
    const h = value["h"];
    if (!isFiniteInt(x) || !isFiniteInt(y) || !isFiniteInt(w) || !isFiniteInt(h)) {
        return false;
    }
    if (w <= 0 || h <= 0) {
        return false;
    }
    if (x < -16384 || x > 16384 || y < -16384 || y > 16384 || w > 16384 || h > 16384) {
        return false;
    }
    return true;
}

// B6 minimum-origin placement: one meaningful native minimum extent,
// mirroring core `meaningful` (1..=GEOMETRY_BOUND). Unknown, zero,
// negative, or absurd values behave as absent, never as a floor.
function meaningfulMinExtent(value: unknown): number | null {
    if (!isFiniteInt(value)) {
        return null;
    }
    const extent = value as number;
    if (extent < 1 || extent > 16384) {
        return null;
    }
    return extent;
}

// Q2 fixed-size float admission predicate (D1), mirroring core
// `is_fixed_size_with`: min and max must BOTH be present with usable
// nonnegative vector sizes (0..=16384); the entire (0,0) vector does not
// count; zero is unset per axis (G-05): a fixed axis needs equal NONZERO
// bounds, so equal partial-zero vectors such as (640,0) or (0,480) pin
// nothing under `both-axes-fixed` but pin via their nonzero axis under
// `either-axis-fixed`. Unset bounds, negative values, unbounded sentinels,
// and out-of-contract values never count. The resizeable flag alone never
// counts. The final equality is predicate-selected: `both-axes-fixed`
// (default) needs a nonzero-equal pin on both axes, `either-axis-fixed`
// on either axis. Total over all inputs. The third `predicate` parameter
// defaults to the delivered both-axes behavior so legacy callers stay
// byte-identical.
export function isFixedSize(
    minSize: { readonly w: unknown; readonly h: unknown } | null | undefined,
    maxSize: { readonly w: unknown; readonly h: unknown } | null | undefined,
    predicate: unknown = FIXED_SIZE_PREDICATE_DEFAULT,
): boolean {
    if (minSize === null || minSize === undefined || maxSize === null || maxSize === undefined) {
        return false;
    }
    const bounds = [minSize.w, minSize.h, maxSize.w, maxSize.h];
    for (const bound of bounds) {
        if (!isFiniteInt(bound)) {
            return false;
        }
        const extent = bound as number;
        if (extent < 0 || extent > 16384) {
            return false;
        }
    }
    const minW = minSize.w as number;
    const minH = minSize.h as number;
    const maxW = maxSize.w as number;
    const maxH = maxSize.h as number;
    if (minW === 0 && minH === 0 && maxW === 0 && maxH === 0) {
        return false;
    }
    if (normalizeFixedSizePredicate(predicate) === FIXED_SIZE_PREDICATE_EITHER) {
        return (minW === maxW && minW !== 0) || (minH === maxH && minH !== 0);
    }
    return minW === maxW && minW !== 0 && minH === maxH && minH !== 0;
}

// Effective native target for an overconstrained tile: planned origin with
// each extent raised to the freshly observed declared minimum. Satisfied or
// unknown/sentinel extents pass through byte-identical (mirrors Windows
// `overconstrained_effective`: origin kept, only violated extents grow).
function overconstrainedEffective(
    planned: PlanRect,
    minSize: { readonly w: number; readonly h: number } | null | undefined,
): PlanRect {
    if (minSize === null || minSize === undefined) {
        return planned;
    }
    const minW = meaningfulMinExtent(minSize.w);
    const minH = meaningfulMinExtent(minSize.h);
    const w = minW === null ? planned.w : Math.max(planned.w, minW);
    const h = minH === null ? planned.h : Math.max(planned.h, minH);
    if (w === planned.w && h === planned.h) {
        return planned;
    }
    return { x: planned.x, y: planned.y, w, h };
}

function hasExactKeys(value: Record<string, unknown>, keys: readonly string[]): boolean {
    const actual = Object.keys(value);
    if (actual.length !== keys.length) {
        return false;
    }
    for (const key of keys) {
        if (!Object.prototype.hasOwnProperty.call(value, key)) {
            return false;
        }
    }
    return true;
}

// Bounded rejection token fields: lowercase dashes only. Never echo payload
// bytes other than the Planner's fixed token vocabulary.
function sanitizeKind(value: unknown): string {
    if (typeof value !== "string" || value.length === 0 || value.length > 64) {
        return "unknown";
    }
    for (let index = 0; index < value.length; index += 1) {
        const code = value.charCodeAt(index);
        const ok = (code >= 97 && code <= 122) || code === 45;
        if (!ok) {
            return "unknown";
        }
    }
    return value;
}

// Exact Engine gap-mismatch messages for the correlated update-gaps retry.
function sanitizeDetail(value: unknown): string | null {
    if (typeof value !== "string" || value.length === 0 || value.length > 64) {
        return null;
    }
    for (let index = 0; index < value.length; index += 1) {
        const code = value.charCodeAt(index);
        const ok = (code >= 97 && code <= 122) || code === 45;
        if (!ok) {
            return null;
        }
    }
    return value;
}

// Exact Engine gap-mismatch messages for the correlated update-gaps retry.
// Rejected gap replies carry no wire `detail`, so only `message` binds the
// signal: matching is exact-string equality against these fixed Engine
// literals, never substring or kind-only.
const GAP_MISMATCH_MESSAGES: readonly string[] = Object.freeze([
    "domain gap does not match retained state",
    "domain outer gap does not match retained state",
]);

function isGapMismatchMessage(value: unknown): boolean {
    return typeof value === "string" && GAP_MISMATCH_MESSAGES.indexOf(value) >= 0;
}

interface PlanGeometryEntry {
    readonly window: string;
    readonly leaf: string;
    readonly output: string;
    readonly workspace: string;
    readonly rect: PlanRect;
    // AR12 reply diagnostics. Absent on the wire unless true; default false
    // here. `overconstrained` is honored on every op (never reasserted);
    // `clientClamped` is honored only on reconcile/update-gaps.
    readonly overconstrained: boolean;
    readonly clientClamped: boolean;
}

interface PlanFocusBody {
    readonly domainOutput: string;
    readonly domainWorkspace: string;
    readonly leaf: string;
}

export interface PlanFocusOperation {
    readonly op: "focus";
    readonly domainOutput: string;
    readonly domainWorkspace: string;
    // Leafless for float-origin cross-output focus (null on the wire):
    // the subject is a floating/sticky exception with no tile leaf. Tile
    // operations always carry a leaf string.
    readonly fromLeaf: string | null;
    readonly toLeaf: string;
    readonly fromWindow: string;
    readonly toWindow: string;
    readonly direction: string;
    readonly route: ReadonlyArray<string>;
    readonly crossSourceOutput: string | null;
    readonly crossSourceWorkspace: string | null;
}

// Exact production cross-output move operation (R4 only): the mover, the
// captured source, the adjacent target with its current workspace, the
// commanded direction, the R4 rule, and the transfer capability. Absent for
// local R1/R2/R3 plans. The adapter fences every field before any
// membership/geometry/focus write; no looser alternate plan is accepted.
export interface PlanMoveOperation {
    readonly op: "move";
    readonly rule: string;
    readonly capability: string;
    readonly direction: string;
    readonly window: string;
    readonly leaf: string;
    readonly sourceOutput: string;
    readonly sourceWorkspace: string;
    readonly targetOutput: string;
    readonly targetWorkspace: string;
    readonly sourceRootChildIndex: number;
    readonly target: string;
}

interface PlannedReply {
    readonly correlationId: string;
    readonly baseRevision: number | null;
    readonly geometry: ReadonlyArray<PlanGeometryEntry>;
    readonly focus: PlanFocusBody | null;
    readonly floatGeometry: { readonly window: string; readonly rect: PlanRect } | null;
    // Exact cross-output operation/preconditions when the planner crossed
    // outputs (focus or R4 move). Absent for local plans. The adapter fences
    // them against the captured source before any actuation.
    readonly operation: PlanFocusOperation | PlanMoveOperation | null;
    readonly preconditions: ReadonlyArray<string> | null;
    // Raw wire operation/preconditions retained verbatim for the R4 verify
    // echo (Rust parses snake_case and compares typed equality).
    readonly rawOperation: unknown;
    readonly rawPreconditions: unknown;
}

function validateGeometryEntry(value: unknown): PlanGeometryEntry | null {
    if (!isRecord(value)) {
        return null;
    }
    // Required wire keys plus the optional AR12 diagnostics. Any other key
    // stays malformed (fail closed, mirroring the old exact-keys check).
    const allowed = new Set(["window", "leaf", "output", "workspace", "rect", "overconstrained", "client_clamped"]);
    for (const key of Object.keys(value)) {
        if (!allowed.has(key)) {
            return null;
        }
    }
    for (const key of ["window", "leaf", "output", "workspace", "rect"]) {
        if (!Object.prototype.hasOwnProperty.call(value, key)) {
            return null;
        }
    }
    if (
        !isOpaqueId(value["window"]) ||
        !isOpaqueId(value["leaf"]) ||
        !isOpaqueId(value["output"]) ||
        !isOpaqueId(value["workspace"])
    ) {
        return null;
    }
    const rawRect: unknown = value["rect"];
    if (!isTargetRect(rawRect)) {
        return null;
    }
    const over = value["overconstrained"];
    if (over !== undefined && typeof over !== "boolean") {
        return null;
    }
    const clamped = value["client_clamped"];
    if (clamped !== undefined && typeof clamped !== "boolean") {
        return null;
    }
    const rect = rawRect as unknown as Record<string, unknown>;
    return {
        window: value["window"] as string,
        leaf: value["leaf"] as string,
        output: value["output"] as string,
        workspace: value["workspace"] as string,
        rect: {
            x: rect["x"] as number,
            y: rect["y"] as number,
            w: rect["w"] as number,
            h: rect["h"] as number,
        },
        overconstrained: over === true,
        clientClamped: clamped === true,
    };
}

function validateFocusBody(value: unknown): PlanFocusBody | null {
    if (!isRecord(value)) {
        return null;
    }
    if (!hasExactKeys(value, ["domain_output", "domain_workspace", "leaf"])) {
        return null;
    }
    if (
        !isOpaqueId(value["domain_output"]) ||
        !isOpaqueId(value["domain_workspace"]) ||
        !isOpaqueId(value["leaf"])
    ) {
        return null;
    }
    return {
        domainOutput: value["domain_output"] as string,
        domainWorkspace: value["domain_workspace"] as string,
        leaf: value["leaf"] as string,
    };
}

function parseBaseRevision(reply: unknown): number | null {
    if (!isRecord(reply)) {
        return null;
    }
    const value = reply["base_revision"];
    if (typeof value !== "number" || !Number.isInteger(value) || value < 0 || value > PLAN_MAX_SEQ) {
        return null;
    }
    return value;
}

function validatePlanned(reply: unknown, correlationId: string): PlannedReply | null {
    if (!isRecord(reply)) {
        return null;
    }
    if (reply["v"] !== PLAN_CONTRACT_VERSION) {
        return null;
    }
    if (reply["correlation_id"] !== correlationId) {
        return null;
    }
    if (reply["outcome"] !== "planned") {
        return null;
    }
    const geometryRaw = reply["desired_geometry"];
    if (!Array.isArray(geometryRaw)) {
        return null;
    }
    const geometry: PlanGeometryEntry[] = [];
    const seen = new Set<string>();
    for (const entry of geometryRaw) {
        const valid = validateGeometryEntry(entry);
        if (valid === null || seen.has(valid.window)) {
            return null;
        }
        seen.add(valid.window);
        geometry.push(valid);
    }
    const focusRaw = reply["desired_focus"];
    if (focusRaw !== undefined && focusRaw !== null) {
        const focus = validateFocusBody(focusRaw);
        if (focus === null) {
            return null;
        }
        const floatGeometry = validateFloatGeometry(reply["float_geometry"]);
        if (floatGeometry === undefined) {
            return null;
        }
        const operation = validateOperation(reply["operation"]);
        if (operation === undefined) {
            return null;
        }
        const preconditions = validatePreconditions(reply["preconditions"]);
        if (preconditions === undefined) {
            return null;
        }
        // Cross focus operations must carry the exact adjacent-output
        // precondition; local focus operations must carry the same-domain
        // one. Move operations pair only with the move precondition set
        // (fenced by the active cross-focus path); any other pairing is
        // malformed.
        if (operation !== null && operation.op === "focus") {
            const hasAdjacent = preconditions !== null && preconditions.indexOf("focus-targets-adjacent-output") >= 0;
            const hasSame = preconditions !== null && preconditions.indexOf("focus-targets-same-domain") >= 0;
            const hasLeaf = preconditions !== null && preconditions.indexOf("focused-leaf-occupied-by-focused-window") >= 0;
            const hasFloat = preconditions !== null && preconditions.indexOf("focused-floating-window") >= 0;
            const isCross = operation.crossSourceOutput !== null || operation.crossSourceWorkspace !== null;
            if (isCross !== hasAdjacent || (!isCross && !hasSame)) {
                return null;
            }
            if (hasAdjacent && hasSame) {
                return null;
            }
            // Leafless operations bind exactly to the floating-subject
            // token: null from_leaf requires the float token (and never the
            // leaf token), a leaf string requires the reverse. The float
            // token additionally rides cross-output replies only.
            const leafless = operation.fromLeaf === null;
            if (leafless !== hasFloat || (hasFloat && (!isCross || hasLeaf || hasSame))) {
                return null;
            }
            if (!leafless && !hasLeaf) {
                return null;
            }
        } else if (operation !== null && operation.op === "move") {
            if (preconditions === null) {
                return null;
            }
            const allowed = new Set([
                "focused-leaf-occupied-by-focused-window",
                "source-root-membership-and-adjacent-same-workspace-output",
                "adapter-must-verify-postconditions",
            ]);
            if (preconditions.length !== 3) {
                return null;
            }
            for (const token of preconditions) {
                if (!allowed.has(token)) {
                    return null;
                }
            }
        } else if (preconditions !== null) {
            return null;
        }
        return { correlationId, baseRevision: parseBaseRevision(reply), geometry: Object.freeze(geometry), focus, floatGeometry, operation, preconditions, rawOperation: reply["operation"] ?? null, rawPreconditions: reply["preconditions"] ?? null };
    }
    const floatGeometry = validateFloatGeometry(reply["float_geometry"]);
    if (floatGeometry === undefined) {
        return null;
    }
    const operation = validateOperation(reply["operation"]);
    if (operation === undefined) {
        return null;
    }
    const preconditions = validatePreconditions(reply["preconditions"]);
    if (preconditions === undefined || operation !== null || preconditions !== null) {
        return null;
    }
    return { correlationId, baseRevision: parseBaseRevision(reply), geometry: Object.freeze(geometry), focus: null, floatGeometry, operation, preconditions, rawOperation: null, rawPreconditions: null };
}

// Operation envelope shared by focus and move replies: parsed by the `op`
// tag into the exact variant. Unknown tags and malformed variants are
// `undefined` (malformed reply); absent is `null`.
function validateOperation(value: unknown): PlanFocusOperation | PlanMoveOperation | null | undefined {
    if (value === undefined || value === null) {
        return null;
    }
    if (!isRecord(value)) {
        return undefined;
    }
    if (value["op"] === "focus") {
        return validateFocusOperation(value);
    }
    if (value["op"] === "move") {
        return validateMoveOperation(value);
    }
    return undefined;
}

function validateMoveOperation(value: unknown): PlanMoveOperation | null | undefined {
    if (!isRecord(value)) {
        return undefined;
    }
    const want = ["op", "rule", "capability", "direction", "window", "leaf", "source_output", "source_workspace", "target_output", "target_workspace", "source_root_child_index", "target"];
    if (!hasExactKeys(value, want)) {
        return undefined;
    }
    if (
        !isOpaqueId(value["window"]) ||
        !isOpaqueId(value["leaf"]) ||
        !isOpaqueId(value["source_output"]) ||
        !isOpaqueId(value["source_workspace"]) ||
        !isOpaqueId(value["target_output"]) ||
        !isOpaqueId(value["target_workspace"])
    ) {
        return undefined;
    }
    const direction = value["direction"];
    if (direction !== "left" && direction !== "right" && direction !== "up" && direction !== "down") {
        return undefined;
    }
    const rule = value["rule"];
    if (typeof rule !== "string" || rule.length === 0 || rule.length > 32) {
        return undefined;
    }
    const capability = value["capability"];
    if (typeof capability !== "string" || capability.length === 0 || capability.length > 64) {
        return undefined;
    }
    const sourceRootChildIndex = value["source_root_child_index"];
    if (!isFiniteInt(sourceRootChildIndex) || (sourceRootChildIndex as number) < 0) {
        return undefined;
    }
    const target = value["target"];
    if (target !== "empty" && target !== "occupied") {
        return undefined;
    }
    return {
        op: "move",
        rule: rule as string,
        capability: capability as string,
        direction: direction as string,
        window: value["window"] as string,
        leaf: value["leaf"] as string,
        sourceOutput: value["source_output"] as string,
        sourceWorkspace: value["source_workspace"] as string,
        targetOutput: value["target_output"] as string,
        targetWorkspace: value["target_workspace"] as string,
        sourceRootChildIndex: sourceRootChildIndex as number,
        target: target as string,
    };
}

function validateFocusOperation(value: unknown): PlanFocusOperation | null | undefined {
    if (value === undefined || value === null) {
        return null;
    }
    if (!isRecord(value)) {
        return undefined;
    }
    const keys = Object.keys(value);
    const want = ["op", "domain_output", "domain_workspace", "from_leaf", "to_leaf", "from_window", "to_window", "direction", "route", "cross_source_output", "cross_source_workspace"];
    if (keys.length !== want.length) {
        return undefined;
    }
    for (const key of want) {
        if (!Object.prototype.hasOwnProperty.call(value, key)) {
            return undefined;
        }
    }
    if (value["op"] !== "focus") {
        return undefined;
    }
    if (
        !isOpaqueId(value["domain_output"]) ||
        !isOpaqueId(value["domain_workspace"]) ||
        !isOpaqueId(value["to_leaf"]) ||
        !isOpaqueId(value["from_window"]) ||
        !isOpaqueId(value["to_window"])
    ) {
        return undefined;
    }
    // Leafless (null) from_leaf rides float-origin cross-output replies
    // only; whether it belongs there is decided with the preconditions and
    // the flight below, never here alone.
    const fromLeaf = value["from_leaf"];
    if (fromLeaf !== null && !isOpaqueId(fromLeaf)) {
        return undefined;
    }
    const direction = value["direction"];
    if (direction !== "left" && direction !== "right" && direction !== "up" && direction !== "down") {
        return undefined;
    }
    const route = value["route"];
    if (!Array.isArray(route) || route.length === 0) {
        return undefined;
    }
    const routeOut: string[] = [];
    for (const entry of route) {
        if (!isOpaqueId(entry)) {
            return undefined;
        }
        routeOut.push(entry as string);
    }
    const crossOutput = value["cross_source_output"];
    const crossWorkspace = value["cross_source_workspace"];
    if (crossOutput !== null && !isOpaqueId(crossOutput)) {
        return undefined;
    }
    if (crossWorkspace !== null && !isOpaqueId(crossWorkspace)) {
        return undefined;
    }
    return {
        op: "focus",
        domainOutput: value["domain_output"] as string,
        domainWorkspace: value["domain_workspace"] as string,
        fromLeaf: (fromLeaf as string | null) ?? null,
        toLeaf: value["to_leaf"] as string,
        fromWindow: value["from_window"] as string,
        toWindow: value["to_window"] as string,
        direction: direction as string,
        route: Object.freeze(routeOut),
        crossSourceOutput: (crossOutput as string | null) ?? null,
        crossSourceWorkspace: (crossWorkspace as string | null) ?? null,
    };
}

function validatePreconditions(value: unknown): ReadonlyArray<string> | null | undefined {
    if (value === undefined || value === null) {
        return null;
    }
    if (!Array.isArray(value) || value.length === 0 || value.length > 8) {
        return undefined;
    }
    const allowed = new Set([
        "focused-leaf-occupied-by-focused-window",
        "focused-floating-window",
        "target-leaf-occupied",
        "focus-targets-same-domain",
        "focus-targets-adjacent-output",
        "source-root-membership-and-adjacent-same-workspace-output",
        "adapter-must-verify-postconditions",
    ]);
    const out: string[] = [];
    for (const entry of value) {
        if (typeof entry !== "string" || !allowed.has(entry)) {
            return undefined;
        }
        out.push(entry);
    }
    return Object.freeze(out);
}

function validateFloatGeometry(value: unknown): { readonly window: string; readonly rect: PlanRect } | null | undefined {
    if (value === undefined || value === null) return null;
    if (!isRecord(value) || !hasExactKeys(value, ["window", "rect"]) || !isOpaqueId(value["window"]) || !isTargetRect(value["rect"])) return undefined;
    const rect = value["rect"] as unknown as Record<string, unknown>;
    return { window: value["window"] as string, rect: { x: rect["x"] as number, y: rect["y"] as number, w: rect["w"] as number, h: rect["h"] as number } };
}

// R-LAY-01 planned tiled reply detail: exact keys with the fixed
// kind/capability tokens and policy_version 1. True only for the standard
// toggle-orientation shape; any other shape is malformed.
function isToggleOrientationDetail(value: unknown): boolean {
    if (!isRecord(value)) {
        return false;
    }
    if (!hasExactKeys(value, ["kind", "policy_version", "capability"])) {
        return false;
    }
    return (
        value["kind"] === "toggle-orientation" &&
        value["capability"] === "toggle-orientation" &&
        value["policy_version"] === 1
    );
}

function validateObserved(observed: PlanObserved | null): observed is PlanObserved {
    if (observed === null || typeof observed !== "object") {
        return false;
    }
    if (!isOpaqueId(observed.domainOutput) || !isOpaqueId(observed.domainWorkspace)) {
        return false;
    }
    if (observed.activeExcluded !== undefined && typeof observed.activeExcluded !== "boolean") {
        return false;
    }
    if (observed.windows.length === 0) {
        return observed.activeExcluded === true && observed.focusedId === "";
    }
    if (!isOpaqueId(observed.focusedId)) {
        return false;
    }
    if (!Array.isArray(observed.windows as unknown)) {
        return false;
    }
    const windows = observed.windows;
    if (windows.length === 0) {
        return false;
    }
    if (
        !isTargetRect({
            x: observed.domainBounds.x,
            y: observed.domainBounds.y,
            w: observed.domainBounds.w,
            h: observed.domainBounds.h,
        })
    ) {
        return false;
    }
    if (!isFiniteInt(observed.domainGap) || observed.domainGap < 0 || observed.domainGap > 64) {
        return false;
    }
    if (!isFiniteInt(observed.domainOuterGap) || observed.domainOuterGap < 0 || observed.domainOuterGap > 64) {
        return false;
    }
    // Directional domains: at most two primitive entries, source first and
    // equal to the carried source domain, with distinct outputs and
    // reciprocal adjacency on one cardinal side when two are present.
    const domains = observed.domains;
    if (domains !== undefined) {
        if (!Array.isArray(domains) || domains.length === 0 || domains.length > 2) {
            return false;
        }
        const first = domains[0] as PlanDomain;
        if (
            first.output !== observed.domainOutput ||
            first.workspace !== observed.domainWorkspace ||
            first.bounds.x !== observed.domainBounds.x ||
            first.bounds.y !== observed.domainBounds.y ||
            first.bounds.w !== observed.domainBounds.w ||
            first.bounds.h !== observed.domainBounds.h ||
            first.gap !== observed.domainGap ||
            first.outerGap !== observed.domainOuterGap
        ) {
            return false;
        }
        const seenOutputs = new Set<string>();
        for (const entry of domains) {
            if (typeof entry !== "object" || entry === null) {
                return false;
            }
            const candidate = entry as PlanDomain;
            if (!isOpaqueId(candidate.output) || !isOpaqueId(candidate.workspace)) {
                return false;
            }
            if (!isTargetRect({ x: candidate.bounds.x, y: candidate.bounds.y, w: candidate.bounds.w, h: candidate.bounds.h })) {
                return false;
            }
            if (!isFiniteInt(candidate.gap) || candidate.gap < 0 || candidate.gap > 64) {
                return false;
            }
            if (!isFiniteInt(candidate.outerGap) || candidate.outerGap < 0 || candidate.outerGap > 64) {
                return false;
            }
            if (typeof candidate.adjacent !== "object" || candidate.adjacent === null || Array.isArray(candidate.adjacent)) {
                return false;
            }
            for (const key of Object.keys(candidate.adjacent)) {
                if (key !== "left" && key !== "right" && key !== "up" && key !== "down") {
                    return false;
                }
                const target = (candidate.adjacent as Record<string, unknown>)[key];
                if (!isOpaqueId(target)) {
                    return false;
                }
            }
            if (seenOutputs.has(candidate.output)) {
                return false;
            }
            seenOutputs.add(candidate.output);
        }
        if (domains.length === 2) {
            const firstEntry = domains[0] as PlanDomain;
            const secondEntry = domains[1] as PlanDomain;
            let reciprocal = false;
            for (const direction of ["left", "right", "up", "down"] as const) {
                const opposite =
                    direction === "left"
                        ? "right"
                        : direction === "right"
                          ? "left"
                          : direction === "up"
                            ? "down"
                            : "up";
                if (
                    (firstEntry.adjacent as Record<string, string>)[direction] === secondEntry.output &&
                    (secondEntry.adjacent as Record<string, string>)[opposite] === firstEntry.output
                ) {
                    reciprocal = true;
                    break;
                }
            }
            if (!reciprocal) {
                return false;
            }
        }
    }
    const homedPairs = new Set<string>();
    if (domains !== undefined) {
        for (const entry of domains) {
            homedPairs.add(`${(entry as PlanDomain).output}\u0000${(entry as PlanDomain).workspace}`);
        }
    } else {
        homedPairs.add(`${observed.domainOutput}\u0000${observed.domainWorkspace}`);
    }
    const seen = new Set<string>();
    let focusedFound = false;
    for (const entry of windows) {
        if (typeof entry !== "object" || entry === null) {
            return false;
        }
        const candidate = entry as PlanObservedWindow;
        if (!isOpaqueId(candidate.id) || typeof candidate.ref !== "object" || candidate.ref === null) {
            return false;
        }
        if (typeof candidate.fullscreen !== "boolean") {
            return false;
        }
        if (typeof candidate.maximized !== "boolean") {
            return false;
        }
        if (candidate.floating !== undefined && typeof candidate.floating !== "boolean") {
            return false;
        }
        if (!homedPairs.has(`${candidate.output}\u0000${candidate.workspace}`)) {
            return false;
        }
        if (
            !isTargetRect({ x: candidate.rect.x, y: candidate.rect.y, w: candidate.rect.w, h: candidate.rect.h })
        ) {
            return false;
        }
        if (seen.has(candidate.id)) {
            return false;
        }
        seen.add(candidate.id);
        if (candidate.id === observed.focusedId) {
            focusedFound = true;
            // Directional requests plan from the source focus: the focused
            // window must live in the source domain, never the target.
            if (
                domains !== undefined &&
                (candidate.output !== observed.domainOutput ||
                    candidate.workspace !== observed.domainWorkspace)
            ) {
                return false;
            }
        }
    }
    if (!focusedFound) {
        return false;
    }
    if (typeof observed.fingerprint !== "string" || observed.fingerprint.length === 0) {
        return false;
    }
    if (typeof observed.activeRef !== "object" || observed.activeRef === null) {
        return false;
    }
    if (typeof observed.revalidate !== "function") {
        return false;
    }
    return true;
}

interface PendingFlight {
    readonly correlation: string;
    readonly op: PlanOp;
    readonly epoch: number;
    readonly plannerSession: number;
    readonly snapshot: PlanSnapshot;
    readonly removed: string | null;
    readonly windowCount: number;
    readonly pointerSource: string | null;
    // Drop-intent correlation: set on pointer-resize and drag-drop flights
    // dispatched from a non-cancelled oracle drop (drag-N). Rejection or
    // terminal failure of such a flight feeds the coalesced per-domain
    // restore marker below, never a per-drag queue. pointerSource carries
    // the dragged window for both families (edge target for pointer-resize,
    // drop window for drag-drop).
    readonly dragSource?: string | null;
    // Domain key of the restore marker this reconcile was dispatched for.
    // Never a drag correlation: satisfaction and failure terminals resolve
    // through the marker map, so overlapping drops share one dispatch.
    // restoreMarkerSeq fences the marker incarnation: an evicted marker
    // settled unavailable must never let its late flight settle a recreated
    // marker under the same domain key.
    readonly restoreMarker?: string | null;
    readonly restoreMarkerSeq?: number | null;
    readonly workAreaReprojection: boolean;
    readonly admissionMaximizeClears: ReadonlyArray<string>;
    readonly floatTarget: { readonly window: string; readonly floating: boolean } | null;
    readonly stickyTarget: { readonly window: string; readonly previousFloating: boolean } | null;
    // Staged Q2 fixed-size lifecycle change for an explicit user command
    // (D3): applied only when the flight's plan is applied, dropped on
    // every failure terminal without ever touching the authoritative
    // record. `kind` null deletes the record (intentional float);
    // otherwise it pins a suppress tile win. Exact-ref guarded at commit.
    readonly fixedStage: { readonly id: string; readonly ref: object; readonly kind: string | null } | null;
    // True for a hidden-domain (background) flight: geometry only, never
    // focus or interactive commands. Serialized through the same
    // single-flight and send-blocking as foreground.
    readonly background: boolean;
    // Directional command direction for Left/Right focus/move (else null).
    // Selects the directional re-observation at apply time so a stale target
    // substitution fails closed before any write.
    readonly direction: PlanDirection | null;
    // Pinned-owner transport payload retained across activation steps.
    readonly requestPayload: string;
    // D23 close ids carried on this flight's reconcile request (`closed_windows`).
    // Cleared from `pendingClosed` only on planner delivery (matching
    // `planned`); every failure terminal retains them. Empty for non-reconcile
    // ops, which never carry closes.
    readonly closedSent: ReadonlyArray<string>;
    // Original request revision carried at dispatch (always 0 on the request
    // phase). Cancellation echoes exactly this value, never a base revision
    // learned from a stale probe.
    readonly requestRevision: number;
    // Original command for one pre-write replan.
    readonly body: Record<string, unknown>;
    // Prevents a second replan when the replacement flight is also stale.
    readonly replanned: boolean;
    // True when dispatched from confirmed-loss recovery: a terminal failure
    // stays bounded without a second identity probe, so a failed recovery
    // never loops.
    readonly isRecovery: boolean;
}

// Production R4 cross-output move flight (immediate commit, no wire
// ack/verify/cancel/status). Retained after the R4 `planned` reply across
// native transfer plus bounded delayed arrival. Exactly one flight
// serializes through the shared single-flight; while live every other
// PlanAdapter operation refuses busy and no replay ever occurs.
interface R4Flight {
    readonly flight: number;
    readonly session: number;
    readonly correlation: string;
    readonly baseRevision: number;
    readonly epoch: number;
    readonly op: PlanOp;
    readonly snapshot: PlanSnapshot;
    readonly planned: PlannedReply;
    readonly moverId: string;
    readonly moverRef: object;
    readonly targetOutput: string;
    readonly targetWorkspace: string;
    readonly targetOutputRef: object;
    readonly targetDesktopRef: object;
    readonly byRef: ReadonlyMap<string, object>;
    readonly direction: PlanDirection;
    followed: boolean;
    detaches: Array<() => void>;
    settled: boolean;
}

interface AutoIntent {
    readonly op: PlanOp;
    readonly snapshot: PlanSnapshot;
    readonly removed: string | null;
    readonly body: Record<string, unknown>;
    readonly pointerSource?: string | null;
    readonly dragSource?: string | null;
    // Domain key of the restore marker this reconcile converges. Set only
    // on marker reconciles built by maybeDispatchDragRestore; never queued
    // through superseding intents. restoreMarkerSeq carries the marker
    // incarnation (see PendingFlight).
    readonly restoreMarker?: string | null;
    readonly restoreMarkerSeq?: number | null;
    readonly workAreaReprojection?: boolean;
    readonly admissionMaximizeClears?: ReadonlyArray<string>;
    readonly floatTarget?: { readonly window: string; readonly floating: boolean } | null;
    readonly stickyTarget?: { readonly window: string; readonly previousFloating: boolean } | null;
    // Staged Q2 fixed-size lifecycle change for an explicit user command
    // (D3): applied only when the flight's plan is applied, dropped on
    // every failure terminal without ever touching the authoritative
    // record. `kind` null deletes the record (intentional float);
    // otherwise it pins a suppress tile win. Exact-ref guarded at commit.
    readonly fixedStage?: { readonly id: string; readonly ref: object; readonly kind: string | null } | null;
    readonly background?: boolean;
    readonly direction?: PlanDirection | null;
    readonly replanned?: boolean;
}

interface PointerEcho {
    readonly correlation: string;
    readonly source: string;
    readonly scope: PlanSnapshot;
    readonly neighbours: ReadonlyArray<{ window: string; rect: PlanRect }>;
}

// Coalesced restore marker for rejected drops in one exact domain. `drags`
// carries every rejected drag-N correlation still needing its own terminal
// log; `dispatched` records that the marker's single reconcile attempt was
// used (sent or attempted), so no second dispatch ever follows.
interface DragRestoreMarker {
    readonly output: string;
    readonly workspace: string;
    readonly drags: string[];
    readonly seq: number;
    dispatched: boolean;
}

// Bound on drag correlations coalesced into one marker. Overflow fails closed
// with a truthful correlated `unavailable` terminal naming `plan=none`, never
// a silent drop and never a fabricated success.
const MAX_DRAG_RESTORE_DRAGS = 64;
// Bound on ops whose successful application satisfies a marker: exactly the
// ops that write/apply a domain's full retained/projected geometry.
// Focus-only and toggle-float plans never satisfy, however they settle.
const DRAG_RESTORE_SATISFYING_OPS: ReadonlySet<PlanOp> = new Set([
    "admit",
    "remove",
    "move",
    "resize",
    "reconcile",
    "update-gaps",
    "pointer-resize",
    "drag-drop",
]);

function snapshotsEqualAllowingAdmissionMaximize(
    fresh: PlanSnapshot,
    expected: PlanSnapshot,
    cleared: ReadonlyArray<string>,
): boolean {
    if (
        fresh.domainOutput !== expected.domainOutput ||
        fresh.domainWorkspace !== expected.domainWorkspace ||
        fresh.domainGap !== expected.domainGap ||
        fresh.domainOuterGap !== expected.domainOuterGap ||
        fresh.focusedId !== expected.focusedId ||
        fresh.fingerprint !== expected.fingerprint ||
        fresh.domainBounds.x !== expected.domainBounds.x ||
        fresh.domainBounds.y !== expected.domainBounds.y ||
        fresh.domainBounds.w !== expected.domainBounds.w ||
        fresh.domainBounds.h !== expected.domainBounds.h ||
        fresh.windows.length !== expected.windows.length
    ) {
        return false;
    }
    const clearedIds = new Set(cleared);
    const expectedById = new Map<string, PlanSnapshotWindow>();
    for (const entry of expected.windows) {
        expectedById.set(entry.id, entry);
    }
    for (const entry of fresh.windows) {
        const other = expectedById.get(entry.id);
        if (
            other === undefined ||
            entry.rect.x !== other.rect.x ||
            entry.rect.y !== other.rect.y ||
            entry.rect.w !== other.rect.w ||
            entry.rect.h !== other.rect.h ||
            entry.output !== other.output ||
            entry.workspace !== other.workspace ||
            entry.fullscreen !== other.fullscreen ||
            (entry.maximized !== other.maximized && !(clearedIds.has(entry.id) && entry.maximized))
        ) {
            return false;
        }
    }
    return true;
}

export class PlanAdapter {
    private enabled = false;
    private owner = "";
    private generation = "";
    private detaches: Array<() => void> = [];
    private inFlight = false;
    private token = 0;
    private activeToken = 0;
    private callbackSeen = false;
    private cancelTimer: (() => void) | null = null;
    private debounceCancel: (() => void) | null = null;
    private pending: PendingFlight | null = null;
    private deferredAuto: AutoIntent | null = null;
    private epoch = 0;
    private seq = 0;
    // Bounded correlation rotation: after PLAN_MAX_SEQ correlations the
    // sequence wraps with a rotation prefix (seqEpoch) so correlations are
    // never reused within a bounded length. Same Engine session/topology:
    // rotation never bumps plannerSession and never clears applied evidence.
    private seqEpoch = 0;
    // Per-id applied evidence (rect, output/workspace, floating/sticky/
    // fullscreen/maximized), written only on applied replies. Read-only hint
    // for overlays, first-admission maximize, and drag lookup; never authority
    // for Plan membership. The optional minimumPlaced marker records that the
    // retained rect is minimum-guarded (raised, floor-touching, or
    // host-undershot) still awaiting host confirmation; it keeps bounded
    // reassertion accounting across the converged echo of our own write.
    // Set from the effective placement every apply; cleared by feasible
    // projections with slack, acceptance, and member lifecycle (entry
    // deletion).
    private appliedById = new Map<string, { rect: PlanRect; output: string; workspace: string; floating: boolean; sticky: boolean; fullscreen: boolean; maximized: boolean; minimumPlaced?: boolean }>();
    // Per-domain applied scope (bounds, gap, outerGap), keyed by domain
    // output/workspace. Written only on successful applied plan replies
    // alongside appliedById; never admission or membership authority.
    private appliedScopeByDomain = new Map<string, { bounds: PlanRect; gap: number; outerGap: number }>();
    // First-seen fullscreen hold: ids ever observed non-fullscreen versus ids
    // first seen fullscreen and still held. A first-seen fullscreen window
    // rides the wire as a synthetic floating exception (planner observation
    // only, never native float) so it takes no Rust tile slot while initially
    // fullscreen. First non-fullscreen observation releases to normal fresh
    // admission; later fullscreen retains its slot. Already tiled ids
    // (applied evidence) and ever-seen-normal ids never hold. Each entry
    // pins the exact native ref observed first: a reused id with a different
    // ref never inherits the old marker, and the exact native removal signal
    // evicts even marker-only ids with no applied slot.
    private seenNonFullscreen = new Map<string, object>();
    private heldInitialFullscreen = new Map<string, object>();
    // R-MAX-03 floating-first sightings: stable ids observed on an untiled
    // (floating) domain, recorded at the workspace-floating skip before any
    // dispatch. Their later tiled-domain admission keeps the maximized
    // overlay over the reserved slot under both R-MAX-06 settings with no
    // one-shot clear; only born tiled-first admissions read the R-MAX-06
    // mode. Lifetime-bound (pruned on verified native removal, cleared on
    // enable/disable) and retained across planner recovery like the Windows
    // latches; bounded at 4096 ids with oldest-first eviction.
    private floatingFirstSeen = new Set<string>();
    // R-OUT-05 focused-output newcomer routing (User 2026-10-10, 0.1 triage
    // D06): stable ids already placed (never re-routed), the `windowAdded`
    // birth ref (one-shot per foreground refresh), and the anchor domain
    // from the last round whose active window was an eligible managed
    // member. Lifetime-bound like the markers above: cleared on
    // enable/disable, pruned on verified native removal. Bounded at 4096
    // ids with oldest-first eviction.
    private routeSeenIds = new Set<string>();
    private routeBirthRef: object | null = null;
    private routeAnchor: (NewcomerRouteDomain & { round: number }) | null = null;
    private routeRound = 0;
    private reconcileAttempts = 0;
    // One-shot forced complete reconciliation for send-settled domains,
    // keyed by domain output/workspace. A terminal send flight carries its
    // exact source+target keys via `notifySendSettled`; the next foreground
    // or hidden refresh then dispatches a complete reconcile for each
    // forced domain even when the equal-applied-evidence optimization would
    // otherwise stay quiet. Each key is consumed once when its reconcile
    // dispatches. Unreadable or omitted domains are never consumed and never
    // synthesized: absence stays unknown and the next complete observation
    // retries. No transaction, no Plan block, no queue.
    private sendForcedDomains = new Set<string>();
    // D23 close-versus-minimize distinction: verified native closes keyed by
    // stable id with the owning domain homing read at close time. A bare
    // absence from observation stays a minimize (stored old slot); a pending
    // close rides the next reconcile as `closed_windows` so the planner
    // forgets the slot instead of minting one. Entries clear only on
    // planner delivery (a correlation-matching `planned` reply); every
    // failure terminal (rejected/diverged/timeout/transport/never-sent)
    // retains them for the next signal-driven resync. Never a retry ledger,
    // never a poll: one debounced resync per close signal, retention after.
    // Never evicted while undelivered: losing an authorized close would leak
    // its planner slot. Stays small in practice (entries clear on delivery).
    private pendingClosed = new Map<string, { output: string; workspace: string }>();
    // Last dispatched domain shape per domain key (bounds, gap, outerGap),
    // recorded at dispatch from the primitive-only snapshot. Lets a
    // sole-minimize close (whose observation is null) still deliver its
    // cleanup as an explicit empty-domain reconcile on the retained shape.
    // Never membership authority: windows always ride empty here.
    private lastDomainByKey = new Map<string, { bounds: PlanRect; gap: number; outerGap: number }>();
    // D23 planner-accepted explicit emptiness per domain key. Set only when
    // an empty reconcile applies; cleared when a non-empty snapshot
    // dispatches or the domain is released. Gates null-observation close
    // synthesis: unknown must never upgrade to empty on a merely
    // dispatched (possibly rejected) shape.
    private explicitEmptyByKey = new Set<string>();
    // Per-domain background reconcile accounting, keyed by domain
    // output/workspace. Foreground counters above are never touched by
    // hidden-domain flights so background drift can never block foreground.
    private backgroundAttempts = new Map<string, number>();
    // Reentrancy guard for the finishFlight hidden-domain chain: a
    // synchronously failing background dispatch must not recurse.
    private chainingHidden = false;
    // Once-per-chain bound for hidden domains: every complete hidden domain
    // visited in one refresh/finishFlight chain is recorded here so unchanged
    // domains cannot self-chain endlessly. Cleared only at a new top-level
    // debounced refresh; the next independent signal revisits genuinely new
    // evidence.
    private hiddenVisited = new Set<string>();
    private repeatFocused: string | null = null;
    private repeatDirection: PlanDirection | null = null;
    private repeatMode: PlanResizeMode | null = null;
    private repeatNext = 0;
    private repeatFingerprint = "";
    private pointerEcho: PointerEcho | null = null;
    // Trace-only post-write evidence. One entry per written window avoids
    // turning repeated client or pointer updates into an unbounded trace.
    private constraintTracePending = new Map<string, { correlation: string; resourceClass: string; requested: PlanRect }>();
    private maximizeAdmissionEcho: object | null = null;
    private maximizeAdmissionAttempts = new Set<string>();
    private maximizeToggleEcho: { ref: object; id: string; resourceClass: string } | null = null;
    private stickyEcho: { ref: object; id: string; resourceClass: string; allDesktops: boolean; previousFloating: boolean } | null = null;
    private stickyPreviousFloating = new Map<string, boolean>();
    // Adopted sticky floats with unknown same-runtime origin: an eligible
    // normal window already native-sticky (onAllDesktops true with an empty
    // native desktop list) observed without a recorded origin is adopted as a
    // sticky float with prior-float semantics only. No tiled slot is guessed,
    // no history is persisted, and unstick never claims planner admission.
    private adoptedSticky = new Set<string>();
    // Q2 fixed-size lifecycle (D1-D8): one record per live client, keyed by
    // stable id and fenced by the exact native ref observed with it. `auto`
    // marks classifier-created automatic floats (membership only, D8);
    // `suppress` pins a live-tile win: explicit user tile (unfloat,
    // sticky-off-to-tile, held fullscreen release, workspace retile) or a
    // pinned normal first admission that blocks later core reclassification
    // when hints turn fixed (D2/D3). Intentional floats (toggle-float,
    // sticky) carry no record (D6). A reused id with a different ref never
    // inherits (checked at classification); removal evicts; restart clears
    // (D7). Records are never evicted for absence from one scoped snapshot
    // or minimize: hide/show and cross-domain/background snapshots keep
    // them, and the stored domain refreshes from each sighting so retile
    // targets the current homing.
    private fixedClients = new Map<string, { ref: object; kind: string; domain: string; reported: boolean }>();
    // The native setters own mutual exclusivity. Retain only the prior pair so
    // a project float can restore an initial keep-below choice exactly.
    private keepAbovePrevious = new Map<string, { ref: object; above: boolean; below: boolean }>();
    // Owner-pinned Planner transport plus confirmed-loss recovery. The
    // in-memory Planner survives sleep: same-owner failures retain applied
    // evidence and never rebuild. Only actual absence/identity evidence
    // (strict NameHasOwner false or a changed unique owner, via normal
    // activation or one bounded post-terminal probe) triggers a fresh
    // session. No polling, retry, or Legacy path.
    private pinnedOwner: string | null = null;
    private activationStep = 0;
    private plannerSession = 0;
    private knownOwner: string | null = null;
    private nextIsRecovery = false;
    private probeToken = 0;
    private activeProbe = 0;
    private probeCancel: (() => void) | null = null;
    // Q3 settled float-intent bridge: canonical durable membership (explicit
    // ordinary float only; never automatic fixed floats, suppress pins, or
    // onAllDesktops history). intentActive gates every intent path so
    // harnesses without the entry hooks see zero behavior change;
    // intentReady opens first plan admission once the single bootstrap read
    // settles. One shared in-flight flag serializes the read and all
    // writes (mirroring the planner's own intent lock); at most ONE latest
    // full snapshot coalesces behind it, never a retry ledger. A failed
    // write retains local intent; the next settled event re-sends the full
    // set. The set survives planner recovery untouched (no rehydration).
    private intentActive = false;
    private intentReady = false;
    private intentBusy = false;
    private intentCallToken = 0;
    private intentSeq = 0;
    private intentMembers = new Set<string>();
    private intentTileMembers = new Set<string>();
    private intentPendingWrite: { readonly members: ReadonlyArray<string>; readonly tile: ReadonlyArray<string> } | null = null;
    private intentDeadlineCancel: (() => void) | null = null;
    // Bootstrap close-race fence: verified closes that land while the
    // single read is in flight, when the local set cannot prove membership
    // yet. Subtracted at every read settle so the callback never revives a
    // close into hydration or the next snapshot. Ephemeral: cleared at
    // every settle and on disable; never a retry ledger, never recovery.
    // Bounded by the live inventory like every member id.
    private intentBootstrapClosed = new Set<string>();
    // Live production R4 flight (immediate commit, native plus bounded
    // delayed arrival). While non-null the shared single-flight stays held
    // and every other PlanAdapter operation refuses busy; completion or
    // terminal failure always clears it exactly once with no replay.
    private r4Flight: R4Flight | null = null;
    // Bounded reentrancy guard for the synchronous R4 native-write stack:
    // KWin signals delivered synchronously from a setter must not follow or
    // settle mid-stack; the immediate post-write observation covers sync
    // arrival. Delayed arrival stays observable through the armed one-shot
    // mover signals. Never a transaction-lifetime Plan block.
    private r4WriteDepth = 0;
    // Bounded arrival deadline for delayed R4 arrival. Armed alongside the
    // one-shot mover signals after native writes; cleared on every R4
    // terminal. The unanswered-request deadline stays the existing dispatch
    // `cancelTimer`; stale timers are fenced by flight/session.
    private r4ArrivalTimer: (() => void) | null = null;
    // Coalesced per-domain restore markers for rejected drops. Each rejected
    // drop (adapter-validation refusal, Planner rejection, terminal failure
    // of its pointer flight, or a superseded deferred pointer that never
    // dispatched) appends its validated drag-N correlation to the marker
    // scoped to the drop's exact output/workspace. Overlapping rejected
    // drops share one marker; every correlation gets its own terminal log
    // naming the satisfying or failing plan correlation. Drops with no
    // domain evidence at all fail closed immediately with an `unavailable`
    // terminal and never create a marker, so no unrelated plan can satisfy
    // them. Markers never ride the single deferred slot: while it is busy
    // the marker persists, and exactly one existing-route reconcile
    // dispatches per marker when free, unless an applied plan for the same
    // domain already satisfied it. A failed marker reconcile logs one
    // terminal per drag and never retries. Keyed by domain
    // output/workspace.
    private dragRestore = new Map<string, DragRestoreMarker>();
    private dragRestoreSeq = 0;
    // Explicit no-write release-domain queue through the shared single-flight.
    private releaseQueue: Array<{
        readonly snapshot: PlanSnapshot;
        readonly onSettled: ((outcome: string) => void) | undefined;
    }> = [];
    private releaseCallbacks = new Map<string, (outcome: string) => void>();
    // Bounded read-only drag preview state (no timers, no caps, no queue):
    // one entry per drag-N correlation carrying the last validated
    // `hover_prior` plus the one live reply fence (sequence, scope,
    // owner/generation, callback). Entry-driven clearing; terminals consume.
    private dragPreviewSeq = 0;
    private lastPreviewRefuseKey: string | null = null;
    private dragPreview = new Map<
        string,
        {
            prior?: unknown;
            live?: {
                seq: number;
                scope: string;
                owner: string;
                generation: string;
                window: string;
                correlation: string;
                callback: (result: PlanDragPreviewResult | null) => void;
            };
        }
    >();

    constructor(private readonly env: PlanAdapterEnv) {}

    get isEnabled(): boolean {
        return this.enabled;
    }

    get isInFlight(): boolean {
        return this.inFlight;
    }

    // Global R4 pending gate: true while an R4 move flight holds the shared
    // single-flight across native transfer plus ack/verify. Entry and tests
    // use it alongside isInFlight to block interleaving work.
    get isR4InFlight(): boolean {
        return this.r4Flight !== null;
    }

    enable(auth: PlanEnableAuth): boolean {
        if (this.enabled) {
            return false;
        }
        if (!isRecord(auth as unknown as Record<string, unknown>)) {
            return false;
        }
        if (!isOwnerId(auth.owner) || !isGeneration(auth.generation)) {
            return false;
        }
        const kinds: ReadonlyArray<PlanSignal> = ["added", "removed", "activated", "geometry", "scope", "fullscreen", "maximize", "desktops"];
        const attached: Array<() => void> = [];
        for (const kind of kinds) {
            let detach: (() => void) | null = null;
            try {
                detach = this.env.subscribe(kind, (target) => this.onSignal(kind, target));
            } catch (error) {
                void error;
                detach = null;
            }
            if (typeof detach !== "function") {
                for (const done of attached) {
                    try {
                        done();
                    } catch (error) {
                        void error;
                    }
                }
                return false;
            }
            attached.push(detach);
        }
        this.detaches = attached;
        this.owner = auth.owner as string;
        this.generation = auth.generation as string;
        this.enabled = true;
        this.inFlight = false;
        this.pending = null;
        this.r4Flight = null;
        this.deferredAuto = null;
        this.epoch = 0;
        this.appliedById.clear();
        this.appliedScopeByDomain.clear();
        this.seenNonFullscreen.clear();
        this.heldInitialFullscreen.clear();
        this.floatingFirstSeen.clear();
        this.fixedClients.clear();
        this.routeSeenIds.clear();
        this.routeBirthRef = null;
        this.routeAnchor = null;
        this.routeRound = 0;
        this.settleDragRestoreUnavailable();
        this.reconcileAttempts = 0;
        this.sendForcedDomains.clear();
        this.backgroundAttempts.clear();
        this.pointerEcho = null;
        this.constraintTracePending.clear();
        this.maximizeAdmissionEcho = null;
        this.maximizeAdmissionAttempts.clear();
        this.maximizeToggleEcho = null;
        this.stickyEcho = null;
        this.stickyPreviousFloating.clear();
        this.adoptedSticky.clear();
        this.keepAbovePrevious.clear();
        this.pinnedOwner = null;
        this.activationStep = 0;
        this.plannerSession = 0;
        this.knownOwner = null;
        this.nextIsRecovery = false;
        this.activeProbe = 0;
        this.clearProbeTimer();
        this.r4WriteDepth = 0;
        this.dragPreview.clear();
        this.lastPreviewRefuseKey = null;
        this.clearRepeat();
        // Fresh intent session: the entry re-runs the single bootstrap read
        // after every enable. Late intent callbacks from a previous life are
        // already fenced by the disable token bump below.
        this.intentActive = false;
        this.intentReady = false;
        this.intentBusy = false;
        this.intentSeq = 0;
        this.intentMembers.clear();
        this.intentTileMembers.clear();
        this.intentPendingWrite = null;
        this.intentDeadlineCancel = null;
        this.intentBootstrapClosed.clear();
        return true;
    }

    disable(): void {
        if (!this.enabled && this.detaches.length === 0) {
            return;
        }
        this.enabled = false;
        this.restoreAllKeepAbove();
        this.inFlight = false;
        this.pending = null;
        this.clearR4Flight();
        this.deferredAuto = null;
        this.appliedById.clear();
        this.appliedScopeByDomain.clear();
        this.seenNonFullscreen.clear();
        this.heldInitialFullscreen.clear();
        this.floatingFirstSeen.clear();
        this.fixedClients.clear();
        this.routeSeenIds.clear();
        this.routeBirthRef = null;
        this.routeAnchor = null;
        this.routeRound = 0;
        this.settleDragRestoreUnavailable();
        this.reconcileAttempts = 0;
        this.sendForcedDomains.clear();
        this.backgroundAttempts.clear();
        this.pointerEcho = null;
        this.constraintTracePending.clear();
        this.maximizeAdmissionEcho = null;
        this.maximizeAdmissionAttempts.clear();
        this.maximizeToggleEcho = null;
        this.stickyEcho = null;
        this.stickyPreviousFloating.clear();
        this.adoptedSticky.clear();
        this.keepAbovePrevious.clear();
        this.pinnedOwner = null;
        this.activationStep = 0;
        this.knownOwner = null;
        this.nextIsRecovery = false;
        this.activeProbe = 0;
        this.clearProbeTimer();
        this.r4WriteDepth = 0;
        this.dragPreview.clear();
        this.lastPreviewRefuseKey = null;
        this.clearRepeat();
        this.clearTimer();
        this.clearR4ArrivalTimer();
        this.clearDebounce();
        // Shutdown fence: bump the intent token so in-flight read/write
        // callbacks and deadline timers land discarded, then drop the
        // session. No retry, no flush of the coalesced snapshot.
        this.intentCallToken += 1;
        this.clearIntentDeadline();
        this.intentActive = false;
        this.intentReady = false;
        this.intentBusy = false;
        this.intentSeq = 0;
        this.intentMembers.clear();
        this.intentTileMembers.clear();
        this.intentPendingWrite = null;
        this.intentBootstrapClosed.clear();
        for (const detach of this.detaches) {
            try {
                detach();
            } catch (error) {
                void error;
            }
        }
        this.detaches = [];
    }

    // Directional observation outcome for focus/move. `ready`
    // carries a validated two-domain observation; `no-target` is a confirmed
    // no-adjacent/single-output condition that keeps local single-domain
    // behavior; `invalid` (ambiguous, unreadable, malformed) must refuse
    // before any local mutation. `legacy` covers hook-absent
    // isolated tests, which stay on the normal single-domain observation.
    // Item 5 authorizes moves and output sends across outputs, not vertical
    // focus: focus consults the hook for left/right only (up/down stay
    // legacy single-domain); move probes all four directions.
    private readDirectional(
        direction: PlanDirection,
        forMove: boolean,
    ): { kind: "ready"; observed: PlanObserved } | { kind: "no-target" } | { kind: "invalid" } | { kind: "legacy" } {
        if (!forMove && direction !== "left" && direction !== "right") {
            return { kind: "legacy" };
        }
        const hook = this.env.observeDirectional;
        if (typeof hook !== "function") {
            return { kind: "legacy" };
        }
        let raw: DirectionalObservation | PlanObserved | null = null;
        try {
            raw = hook(direction, undefined, forMove);
        } catch (error) {
            void error;
            return { kind: "invalid" };
        }
        if (raw === null || raw === undefined) {
            return { kind: "invalid" };
        }
        if (typeof raw === "object" && "status" in raw) {
            const outcome = raw as DirectionalObservation;
            if (outcome.status === "ready") {
                return validateObserved(outcome.observed) ? { kind: "ready", observed: outcome.observed as PlanObserved } : { kind: "invalid" };
            }
            if (outcome.status === "no-target") {
                return { kind: "no-target" };
            }
            return { kind: "invalid" };
        }
        // Legacy mock shape: a bare observation is a ready candidate that
        // still must validate; anything else is invalid, never silent local.
        return validateObserved(raw as PlanObserved | null)
            ? { kind: "ready", observed: raw as PlanObserved }
            : { kind: "invalid" };
    }

    // Directional re-observation for a flight: the same commanded direction
    // re-resolves source plus the same adjacent target. Anything unreadable,
    // ambiguous, changed, or newly targetless fails closed (null) before any
    // write.
    private freshDirectionalForFlight(flightState: PendingFlight): PlanObserved | null {
        const direction = flightState.direction;
        if (!isDirection(direction)) {
            return null;
        }
        const outcome = this.readDirectional(direction, flightState.op === "move");
        return outcome.kind === "ready" ? outcome.observed : null;
    }

    requestFocus(direction: unknown): void {
        if (!this.enabled) {
            this.logToken(`${LOG_PREFIX}:focus-refused-disabled`);
            return;
        }
        if (!isDirection(direction)) {
            this.logToken(`${LOG_PREFIX}:focus-refused-invalid-direction`);
            return;
        }
        if (this.r4Flight !== null) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=focus`);
            return;
        }
        if (this.inFlight) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=focus`);
            return;
        }
        const directional = this.readDirectional(direction, false);
        if (directional.kind === "invalid") {
            this.logToken(`${LOG_PREFIX}:focus-refused-ambiguous`);
            return;
        }
        const observed =
            directional.kind === "ready" ? directional.observed : this.freshObserved();
        if (observed === null) {
            this.logToken(`${LOG_PREFIX}:focus-refused-observe`);
            return;
        }
        if (observed.activeExcluded) {
            this.requestFloatFocus(observed, directional.kind === "ready", direction);
            return;
        }
        if (!this.isTiledDomain(observed.domainOutput, observed.domainWorkspace)) {
            this.logToken(`${LOG_PREFIX}:focus-refused-workspace-floating`);
            return;
        }
        // G-06 maximized directional focus fence (REQ-MAX-08): directional
        // focus is fenced while the focused tiled window is maximized
        // (no-op; leave via unmaximize or Alt+Tab). Fullscreen stays
        // exempt: it carries no geometry write. Mirrors the shared-core
        // opt-in fence; this local fence avoids a wasted dispatch.
        if (
            this.windowIsMaximized(observed, observed.focusedId) &&
            !this.windowIsFullscreen(observed, observed.focusedId)
        ) {
            this.logToken(`${LOG_PREFIX}:focus-refused-maximize`);
            return;
        }
        const snapshot = this.carriedSnapshot(observed);
        this.noteObservation(snapshot.fingerprint);
        this.dispatch({
            op: "focus",
            snapshot,
            removed: null,
            body: { op: "focus", window: snapshot.focusedId, direction },
            direction,
        });
    }

    // COSMIC float-origin directional focus (ordinary and sticky floats share
    // the floating candidate layer; tile-origin behavior above is unchanged).
    // Local search runs synchronously over the existing observation via
    // `selectFloatFocusTarget` and actuates with exactly one `setActive`
    // (fenced by revalidation plus active identity, no planner dispatch, no
    // geometry writes). A Left/Right miss with a ready directional
    // observation routes to the shared cross-output remembered-tiled
    // target (`float_subject`); Up/Down misses and misses with no adjacent
    // output retain focus. Unknown/non-floating subjects refuse fail-closed.
    private requestFloatFocus(observed: PlanObserved, crossEligible: boolean, direction: PlanDirection): void {
        // Ineligible subjects refuse immediately: no local actuation and no
        // cross dispatch, even on a miss that would otherwise fall through.
        const subjectEntry = floatSubjectEntry(observed);
        if (subjectEntry === null) {
            this.logToken(`${LOG_PREFIX}:focus-refused-floating`);
            return;
        }
        // G-06 fence for the floating origin (REQ-MAX-08): a maximized
        // float subject fences directional focus like a maximized tile.
        // Fullscreen stays exempt here as on the tile-origin path.
        if (subjectEntry.maximized === true && subjectEntry.fullscreen !== true) {
            this.logToken(`${LOG_PREFIX}:focus-refused-maximize`);
            return;
        }
        const target = selectFloatFocusTarget(observed, direction);
        if (target !== null) {
            let valid = false;
            try {
                valid = observed.revalidate();
            } catch (error) {
                void error;
                valid = false;
            }
            if (!valid) {
                this.logToken(`${LOG_PREFIX}:focus-float-refused-stale direction=${direction}`);
                return;
            }
            let currentActive: object | null = null;
            try {
                currentActive = this.env.active();
            } catch (error) {
                void error;
                currentActive = null;
            }
            if (currentActive !== observed.activeRef) {
                this.logToken(`${LOG_PREFIX}:focus-float-refused-stale direction=${direction}`);
                return;
            }
            if (currentActive !== target.ref) {
                let focused = false;
                try {
                    focused = this.env.setActive(target.ref) === true;
                } catch (error) {
                    void error;
                    focused = false;
                }
                if (!focused) {
                    this.logToken(`${LOG_PREFIX}:focus-float-write-failed direction=${direction}`);
                    return;
                }
            }
            this.logToken(`${LOG_PREFIX}:focus-float-applied direction=${direction}`);
            return;
        }
        if (direction === "up" || direction === "down" || !crossEligible) {
            this.logToken(`${LOG_PREFIX}:focus-float-retained direction=${direction} reason=no-target`);
            return;
        }
        if (!this.isTiledDomain(observed.domainOutput, observed.domainWorkspace)) {
            this.logToken(`${LOG_PREFIX}:focus-refused-workspace-floating`);
            return;
        }
        const snapshot = this.carriedSnapshot(observed);
        this.noteObservation(snapshot.fingerprint);
        this.dispatch({
            op: "focus",
            snapshot,
            removed: null,
            body: { op: "focus", window: snapshot.focusedId, direction, float_subject: true },
            direction,
        });
    }

    // COSMIC float-origin half-snap (stateless first step, ordinary and
    // sticky share the floating layer; tile-origin move above is unchanged).
    // Synchronous explicit geometry: exactly one `setGeometry` with the
    // COSMIC `relative_geometry` half for the direction, then focus retention
    // on the same subject. No Engine admission, no dispatch, no overlay
    // clear, no transfer/maximize, no reassertion/learning/auto-float, no
    // repeat state: every repeated arrow requests the same half again.
    private requestFloatMove(observed: PlanObserved, direction: PlanDirection): void {
        const subject = floatSubjectEntry(observed);
        if (subject === null) {
            this.logToken(`${LOG_PREFIX}:move-refused-floating`);
            return;
        }
        if (!this.isTiledDomain(observed.domainOutput, observed.domainWorkspace)) {
            this.logToken(`${LOG_PREFIX}:move-refused-workspace-floating`);
            return;
        }
        if (this.windowIsFullscreen(observed, observed.focusedId)) {
            this.logToken(`${LOG_PREFIX}:move-refused-fullscreen`);
            return;
        }
        if (this.windowIsMaximized(observed, observed.focusedId)) {
            this.logToken(`${LOG_PREFIX}:move-refused-maximize`);
            return;
        }
        const rect = floatHalfSnapRect(observed.domainBounds, observed.domainGap, direction);
        if (rect === null) {
            this.logToken(`${LOG_PREFIX}:move-float-refused-invalid-geometry direction=${direction}`);
            return;
        }
        if (this.interactiveResizeActive()) {
            this.logToken(`${LOG_PREFIX}:move-float-refused-interactive direction=${direction}`);
            return;
        }
        let valid = false;
        try {
            valid = observed.revalidate();
        } catch (error) {
            void error;
            valid = false;
        }
        if (!valid) {
            this.logToken(`${LOG_PREFIX}:move-float-refused-stale direction=${direction}`);
            return;
        }
        let currentActive: object | null = null;
        try {
            currentActive = this.env.active();
        } catch (error) {
            void error;
            currentActive = null;
        }
        if (currentActive !== observed.activeRef || currentActive !== subject.ref) {
            this.logToken(`${LOG_PREFIX}:move-float-refused-stale direction=${direction}`);
            return;
        }
        try {
            const reader = this.env.readWindowConstraints;
            if (typeof reader === "function") {
                const constraints = reader(subject.ref);
                if (constraints !== null && typeof constraints === "object") {
                    const typed = constraints as PlanWindowConstraints;
                    if (typed.resizeable === false) {
                        this.logToken(`${LOG_PREFIX}:move-float-refused-constraints direction=${direction}`);
                        return;
                    }
                    const min = typed.minSize;
                    if (min !== null && min !== undefined && Number.isInteger(min.w) && Number.isInteger(min.h)) {
                        if (min.w > rect.w || min.h > rect.h) {
                            this.logToken(`${LOG_PREFIX}:move-float-refused-constraints direction=${direction}`);
                            return;
                        }
                    }
                    const max = typed.maxSize;
                    if (max !== null && max !== undefined && Number.isInteger(max.w) && Number.isInteger(max.h)) {
                        if ((max.w > 0 && max.w < rect.w) || (max.h > 0 && max.h < rect.h)) {
                            this.logToken(`${LOG_PREFIX}:move-float-refused-constraints direction=${direction}`);
                            return;
                        }
                    }
                }
            }
        } catch (error) {
            void error;
        }
        let written = false;
        try {
            written = this.env.setGeometry(subject.ref, rect) === true;
        } catch (error) {
            void error;
            written = false;
        }
        if (!written) {
            this.logToken(`${LOG_PREFIX}:move-float-write-failed direction=${direction}`);
            return;
        }
        const resourceClass = isOpaqueId(subject.resourceClass) ? subject.resourceClass : "unknown";
        if (!this.retainFloatFocus(subject.id, subject.ref, resourceClass)) {
            this.logToken(`${LOG_PREFIX}:move-float-focus-failed direction=${direction}`);
            return;
        }
        this.logToken(`${LOG_PREFIX}:move-float-applied direction=${direction}`);
    }

    // A fullscreen focused window keeps its planner-tree slot but is never
    // actuated or reflowed: directional move/resize on it would change its
    // retained position/share, so it is refused fail-closed before dispatch.
    // Focus is exempt: it carries no geometry write.
    private windowIsFullscreen(observed: PlanObserved, windowId: string): boolean {
        for (const entry of observed.windows) {
            if (entry.id === windowId && entry.fullscreen) {
                return true;
            }
        }
        return false;
    }

    // A maximized focused window mirrors fullscreen isolation: it keeps its
    // planner-tree slot but is never actuated or reflowed. Directional
    // move/resize on it would change its retained position/share, so it is
    // refused fail-closed before dispatch. Focus is exempt: it carries no
    // geometry write.
    private windowIsMaximized(observed: PlanObserved, windowId: string): boolean {
        for (const entry of observed.windows) {
            if (entry.id === windowId && entry.maximized) {
                return true;
            }
        }
        return false;
    }

    // Carried snapshot for dispatch and reply-boundary comparison: a
    // fullscreen or maximized member carries its per-id applied rectangle
    // (the last planned projection for that id on the same output/workspace)
    // in place of the compositor-owned fullscreen or maximized frame rect,
    // which can exceed the work area. A member with no applied projection yet is
    // clamped into the domain bounds. The raw frame rect is never carried
    // for a fullscreen or maximized member. A known tiled member can
    // transiently report an out-of-bounds frame while KWin applies a state
    // change, so carry its applied projection rather than churning the
    // complete snapshot. Unknown non-overlay windows keep their observed rects.
    // Applied evidence is read-only here; only applied replies mutate it.
    // Initial-fullscreen hold is overlaid first (synthetic floating for
    // first-seen fullscreen only) so dispatch, quiet equality, reprojection,
    // and foreground/hidden paths agree from one snapshot shape.
    private withInitialFullscreenHold(observed: PlanObserved): PlanObserved {
        let changed = false;
        const windows = observed.windows.map((entry) => {
            if (!entry.fullscreen) {
                this.seenNonFullscreen.set(entry.id, entry.ref);
                const heldRef = this.heldInitialFullscreen.get(entry.id);
                if (heldRef !== undefined) {
                    this.heldInitialFullscreen.delete(entry.id);
                    if (heldRef === entry.ref) {
                        this.logToken(`${LOG_PREFIX}:initial-fullscreen-released window=${entry.id}`);
                        // D5: born-fullscreen first exit is fresh admission.
                        // No pin here: the classifier below floats fixed
                        // windows automatic with the CURRENT predicate and
                        // tiles the rest. A reused id with a different ref
                        // already released above without inheriting.
                    }
                    changed = true;
                }
                return entry;
            }
            const seenRef = this.seenNonFullscreen.get(entry.id);
            if (seenRef !== undefined) {
                if (seenRef === entry.ref) {
                    return entry;
                }
                this.seenNonFullscreen.delete(entry.id);
            }
            const heldRef = this.heldInitialFullscreen.get(entry.id);
            if (heldRef !== undefined) {
                if (heldRef === entry.ref) {
                    if (entry.floating === true) {
                        return entry;
                    }
                    changed = true;
                    return { ...entry, floating: true };
                }
                this.heldInitialFullscreen.delete(entry.id);
            }
            if (this.appliedById.has(entry.id)) {
                return entry;
            }
            this.heldInitialFullscreen.set(entry.id, entry.ref);
            this.logToken(`${LOG_PREFIX}:initial-fullscreen-held window=${entry.id}`);
            changed = true;
            if (entry.floating === true) {
                return entry;
            }
            return { ...entry, floating: true };
        });
        if (!changed) {
            return observed;
        }
        return { ...observed, windows: Object.freeze(windows) };
    }

    private carriedSnapshot(observed: PlanObserved): PlanSnapshot {
        const held = this.withInitialFullscreenHold(observed);
        const snapshot = snapshotOf(held);
        const windows = snapshot.windows.map((entry) => {
            const evidence = this.appliedById.get(entry.id);
            const retainedRect =
                evidence !== undefined && evidence.output === entry.output && evidence.workspace === entry.workspace
                    ? evidence.rect
                    : undefined;
            const bounds = this.workAreaFor(snapshot, entry.output, entry.workspace) ?? snapshot.domainBounds;
            if (!entry.fullscreen && !entry.maximized && (retainedRect === undefined || rectContained(entry.rect, bounds))) {
                return entry;
            }
            const rect =
                retainedRect !== undefined
                    ? clampCarriedRect(retainedRect, bounds)
                    : clampCarriedRect(entry.rect, bounds);
            return { ...entry, rect: { x: rect.x, y: rect.y, w: rect.w, h: rect.h } };
        });
        const hinted = this.attachHintSizes({ ...snapshot, windows: Object.freeze(windows) }, observed);
        return this.withFixedSizeFloats(hinted, held);
    }

    // Per-domain applied scope: bounds, gap, outerGap keyed by
    // output/workspace. Written only on successful applied plan replies
    // alongside appliedById; never admission or membership authority.
    private appliedScopeFor(
        snapshot: Pick<PlanSnapshot, "domainOutput" | "domainWorkspace">,
    ): { bounds: PlanRect; gap: number; outerGap: number } | null {
        return this.appliedScopeByDomain.get(this.domainKey(snapshot)) ?? null;
    }

    // Same-domain window-set match from per-id applied evidence only: every
    // observed id carries same output/workspace evidence and no extra applied
    // id remains in the domain. Flags, rects, bounds, and gaps never count.
    private appliedWindowSetMatches(fresh: PlanSnapshot): boolean {
        const observedIds = new Set<string>();
        for (const entry of fresh.windows) {
            observedIds.add(entry.id);
            const evidence = this.appliedById.get(entry.id);
            if (
                evidence === undefined ||
                evidence.output !== entry.output ||
                evidence.workspace !== entry.workspace
            ) {
                return false;
            }
        }
        for (const [id, evidence] of this.appliedById) {
            if (
                evidence.output === fresh.domainOutput &&
                evidence.workspace === fresh.domainWorkspace &&
                !observedIds.has(id)
            ) {
                return false;
            }
        }
        return true;
    }

    // The carried snapshot preserves overlay slots; raw geometry still detects
    // out-of-bounds drift. No raw scan is needed after a membership/flag change.
    private classifyCompleteObservation(
        freshSnapshot: PlanSnapshot,
        rawObserved: PlanObserved,
        forgiveStickyMultiHomed: boolean,
    ): RefreshClassification {
        let pureDrift = true;
        let converged = true;
        const observedIds = new Set<string>();
        for (const entry of freshSnapshot.windows) {
            observedIds.add(entry.id);
            const evidence = this.appliedById.get(entry.id);
            const floating = entry.floating === true;
            const sticky = entry.sticky === true;
            if (evidence === undefined || evidence.floating !== floating || evidence.sticky !== sticky) {
                pureDrift = false;
                converged = false;
                break;
            }
            const multiHomed = forgiveStickyMultiHomed && sticky && evidence.sticky === true;
            if (!multiHomed && (evidence.output !== entry.output || evidence.workspace !== entry.workspace)) {
                pureDrift = false;
                converged = false;
                break;
            }
            if (
                !multiHomed &&
                (evidence.rect.x !== entry.rect.x ||
                    evidence.rect.y !== entry.rect.y ||
                    evidence.rect.w !== entry.rect.w ||
                    evidence.rect.h !== entry.rect.h)
            ) {
                converged = false;
            }
        }
        if (pureDrift) {
            for (const [id, evidence] of this.appliedById) {
                if (
                    evidence.output === freshSnapshot.domainOutput &&
                    evidence.workspace === freshSnapshot.domainWorkspace &&
                    !observedIds.has(id)
                ) {
                    pureDrift = false;
                    converged = false;
                    break;
                }
            }
        }
        const appliedScope = this.appliedScopeFor(freshSnapshot);
        const gapsEqual =
            appliedScope !== null &&
            appliedScope.gap === freshSnapshot.domainGap &&
            appliedScope.outerGap === freshSnapshot.domainOuterGap;
        const boundsChanged =
            appliedScope !== null &&
            (appliedScope.bounds.x !== freshSnapshot.domainBounds.x ||
                appliedScope.bounds.y !== freshSnapshot.domainBounds.y ||
                appliedScope.bounds.w !== freshSnapshot.domainBounds.w ||
                appliedScope.bounds.h !== freshSnapshot.domainBounds.h);
        let rawRetainedOutOfBounds = false;
        if (pureDrift) {
            for (const entry of rawObserved.windows) {
                if (entry.fullscreen || entry.maximized) {
                    continue;
                }
                const evidence = this.appliedById.get(entry.id);
                if (
                    evidence !== undefined &&
                    evidence.output === entry.output &&
                    evidence.workspace === entry.workspace &&
                    !rectContained(entry.rect, freshSnapshot.domainBounds)
                ) {
                    rawRetainedOutOfBounds = true;
                    break;
                }
            }
        }
        if (rawRetainedOutOfBounds) {
            converged = false;
        }
        return {
            pureDrift,
            converged,
            rawRetainedOutOfBounds,
            isEmpty: freshSnapshot.windows.length === 0,
            appliedScope,
            scopeEqual: gapsEqual && !boundsChanged,
            gapsEqual,
            boundsChanged,
            observedIds,
        };
    }

    private logRefreshClassification(
        route: "foreground" | "hidden",
        outcome: "equal" | "change" | "uncertain",
        reason: string,
        terminal: string,
    ): void {
        // Quiet equality is trace-only: background reconciliation must not
        // flood the ordinary journal. All other terminals stay visible.
        if (terminal === "quiet" && !KWIN_TRACE_ENABLED) {
            return;
        }
        const correlation = this.pending?.correlation ?? "none";
        const generation = this.generation.length > 0 ? this.generation : "-";
        this.logToken(
            `omnitiler:route-diag component=cosmic-plan route=plan stage=refresh event=${route} outcome=${outcome} reason=${sanitizeKind(reason)} terminal=${sanitizeKind(terminal)} correlation=${correlation} generation=${generation}`,
        );
    }

    // Whether any before-apply per-id evidence exists for a single domain:
    // an applied id homed to the domain, or an observed id carrying any
    // applied slot (including a sticky multi-homed slot last written by
    // another domain). Absent only before the first apply of the domain
    // (fresh worker or seeded retained baseline without applies).
    private hasAppliedEvidenceFor(snapshot: PlanSnapshot): boolean {
        for (const entry of snapshot.windows) {
            if (this.appliedById.has(entry.id)) {
                return true;
            }
        }
        for (const [, evidence] of this.appliedById) {
            if (evidence.output === snapshot.domainOutput && evidence.workspace === snapshot.domainWorkspace) {
                return true;
            }
        }
        return false;
    }

    // Tiled membership/flag difference versus before-apply per-id evidence:
    // an id-set change or a floating/sticky flip on a retained id, scoped
    // to a single domain. Overlays (fullscreen/maximized), focus,
    // fingerprint, rects, bounds, and gaps never count. Sticky windows are
    // multi-homed across domains sharing one per-id slot: matching sticky
    // flags forgive domain homing so consecutive domain applies do not
    // flap. Extra applied ids homed to the domain stay strict. True when
    // no evidence exists yet (fresh seeds converge like the retired admit).
    private appliedMembershipOrFlagsChanged(snapshot: PlanSnapshot): boolean {
        const observedIds = new Set<string>();
        for (const entry of snapshot.windows) {
            observedIds.add(entry.id);
            const evidence = this.appliedById.get(entry.id);
            if (evidence === undefined) {
                return true;
            }
            const floating = entry.floating === true;
            const sticky = entry.sticky === true;
            if (evidence.floating !== floating || evidence.sticky !== sticky) {
                return true;
            }
            const multiHomed = sticky && evidence.sticky === true;
            if (!multiHomed && (evidence.output !== entry.output || evidence.workspace !== entry.workspace)) {
                return true;
            }
        }
        if (!this.hasAppliedEvidenceFor(snapshot)) {
            return true;
        }
        for (const [id, evidence] of this.appliedById) {
            if (
                evidence.output === snapshot.domainOutput &&
                evidence.workspace === snapshot.domainWorkspace &&
                !observedIds.has(id)
            ) {
                return true;
            }
        }
        return false;
    }

    // Reprojection carries the prior planner allocation for every member. The
    // current client rectangles are drift inputs only, and are clamped solely
    // to keep the transport representation inside the new work area. Source is
    // per-id applied evidence for same-domain entries only; never baseline
    // authority.
    private reprojectionSnapshot(observed: PlanObserved): PlanSnapshot {
        const held = this.withInitialFullscreenHold(observed);
        const snapshot = snapshotOf(held);
        const windows = snapshot.windows.map((entry) => {
            const evidence = this.appliedById.get(entry.id);
            const carried =
                evidence !== undefined &&
                evidence.output === entry.output &&
                evidence.workspace === entry.workspace
                    ? evidence.rect
                    : entry.rect;
            const rect = clampCarriedRect(carried, snapshot.domainBounds);
            return { ...entry, rect: { x: rect.x, y: rect.y, w: rect.w, h: rect.h } };
        });
        const hinted = this.attachHintSizes({ ...snapshot, windows: Object.freeze(windows) }, observed);
        return this.withFixedSizeFloats(hinted, held);
    }

    // AR12: capture client size hints freshly from the live refs behind one
    // observation. Best-effort and fail-closed: a missing reader, a throw, or
    // an unreadable value leaves that member hintless (absent on the wire).
    // Values ride through verbatim; meaningfulness (positive, in-bound) is
    // judged in core, never here.
    private hintSizesFor(ref: object): { minSize?: { readonly w: number; readonly h: number }; maxSize?: { readonly w: number; readonly h: number } } {
        try {
            const reader = this.env.readWindowConstraints;
            if (typeof reader !== "function") {
                return {};
            }
            const constraints = reader(ref);
            if (constraints === null || typeof constraints !== "object") {
                return {};
            }
            const out: { minSize?: { readonly w: number; readonly h: number }; maxSize?: { readonly w: number; readonly h: number } } = {};
            const min = (constraints as PlanWindowConstraints).minSize;
            if (min !== null && min !== undefined && Number.isInteger(min.w) && Number.isInteger(min.h)) {
                out.minSize = { w: min.w, h: min.h };
            }
            const max = (constraints as PlanWindowConstraints).maxSize;
            if (max !== null && max !== undefined && Number.isInteger(max.w) && Number.isInteger(max.h)) {
                out.maxSize = { w: max.w, h: max.h };
            }
            return out;
        } catch (error) {
            void error;
            return {};
        }
    }

    // Record an explicit user tile win for the same live client (D3):
    // any automatic mark is replaced by a suppress pin on the exact
    // native ref. A reused id with a different ref never inherits it
    // (checked at classification), and verified removal evicts it.
    private noteFixedTileOverride(id: string, ref: object): void {
        const prior = this.fixedClients.get(id);
        const domain = prior !== undefined && prior.ref === ref ? prior.domain : "";
        this.fixedClients.set(id, { ref, kind: "suppress", domain, reported: true });
    }

    // Drop fixed-size identity when a window becomes an intentional float
    // (explicit float, sticky-on): intentional floats never share
    // automatic identity (D6). No override is recorded.
    private dropAutomaticFixed(id: string): void {
        this.fixedClients.delete(id);
    }

    // Commit a staged explicit-command lifecycle change once the flight's
    // plan is applied (D3): the authoritative record moves only on proof.
    // Every failure terminal drops the staged change without ever having
    // touched it, so a refused/failed unfloat keeps automatic membership
    // and a failed float keeps its tile pin. Exact-ref guarded: a replaced
    // native reference reclassifies fresh and the stale stage is dropped.
    private commitFixedStage(stage: PendingFlight["fixedStage"]): void {
        if (stage === null) {
            return;
        }
        const current = this.fixedClients.get(stage.id);
        if (current !== undefined && current.ref !== stage.ref) {
            return;
        }
        if (stage.kind === null) {
            if (current !== undefined) {
                this.fixedClients.delete(stage.id);
            }
            return;
        }
        const domain = current !== undefined ? current.domain : "";
        this.fixedClients.set(stage.id, { ref: stage.ref, kind: stage.kind, domain, reported: true });
    }

    // Logical floating for operation validation: native floating plus
    // classifier-automatic floats (which observe natively tiled, since the
    // entry never calls intentional setters for them). Keeps Meta+G and
    // sticky commands consistent with the outgoing wire (D3/D6).
    private isEffectivelyFloating(observed: PlanObserved, id: string): boolean {
        for (const entry of observed.windows) {
            if (entry.id === id) {
                if (entry.floating === true || entry.sticky === true) {
                    return true;
                }
                const record = this.fixedClients.get(id);
                return record !== undefined && record.kind === "auto" && record.ref === entry.ref;
            }
        }
        return false;
    }

    // Bounded fixed-size classification diagnostic for one dispatch
    // (D1-D8): correlation plus evaluated/classified counts and the fixed
    // decision token only, never window ids, geometry, domains, or
    // content. Phase word is classification: core summarizes actual
    // committed admission separately. Emits only when this dispatch newly
    // classifies automatic floats, so ordinary frames stay silent.
    private noteFixedSizeDispatch(snapshot: PlanSnapshot, op: string, correlation: string): void {
        let evaluated = 0;
        let classified = 0;
        for (const entry of snapshot.windows) {
            if (entry.minSize !== undefined || entry.maxSize !== undefined) {
                evaluated += 1;
            }
            if (entry.fixedAuto === true) {
                const record = this.fixedClients.get(entry.id);
                if (record !== undefined && record.kind === "auto" && !record.reported) {
                    classified += 1;
                }
            }
        }
        if (classified === 0) {
            return;
        }
        for (const entry of snapshot.windows) {
            if (entry.fixedAuto === true) {
                const record = this.fixedClients.get(entry.id);
                if (record !== undefined && record.kind === "auto") {
                    record.reported = true;
                }
            }
        }
        this.logToken(
            `${LOG_PREFIX}:fixed-size-classification op=${op} correlation=${correlation} evaluated=${String(evaluated)} classified=${String(classified)} reason=fixed-equal phase=classify`,
        );
    }

    // Evict fixed-size bookkeeping for one verified native removal by
    // stable id (D3 lifetime fence). Same-runtime hide/show and minimize
    // omission never route here, so they retain identity. A verified close
    // also evicts settled intent membership (exact id resolved while live
    // by the entry, never read from the dead object) and persists the full
    // set; anything else keeps it.
    noteNativeRemovedId(id: string): void {
        this.fixedClients.delete(id);
        this.floatingFirstSeen.delete(id);
        this.routeSeenIds.delete(id);
        if (!this.intentActive) {
            return;
        }
        if (!this.intentReady) {
            // Read still in flight: the local set is empty-or-partial, so a
            // delete may prove nothing. Fence the verified close for the
            // settle merge and queue the full snapshot regardless, so the
            // post-settle pump persists the corrected set.
            if (this.intentBootstrapClosed.size < INTENT_MAX_MEMBERS) {
                this.intentBootstrapClosed.add(id);
            }
            this.scheduleIntentWrite();
            return;
        }
        const floated = this.intentMembers.delete(id);
        const tiled = this.intentTileMembers.delete(id);
        if (floated || tiled) {
            this.scheduleIntentWrite();
        }
    }

    // Test seam: current settled intent membership (durable set only).
    getIntentMembers(): ReadonlyArray<string> {
        return Object.freeze([...this.intentMembers]);
    }

    // D23 verified native close with owning-domain homing (entry-resolved
    // while live, never read from the dead object). Queues the id for the
    // next reconcile as `closed_windows` and triggers one debounced resync.
    // Minimized-omitted closes queue exactly like tiled ones. Invalid ids
    // refuse silently; queued ids persist until planner delivery (never
    // evicted: losing an authorized close would leak its slot). Stale
    // entries (the id live-observed again) drop at the next build so a
    // contradictory live+closed row never dispatches.
    noteClosedWindow(id: string, output: string, workspace: string): void {
        if (!this.enabled) {
            return;
        }
        if (!isOpaqueId(id) || !isOpaqueId(output) || !isOpaqueId(workspace)) {
            return;
        }
        try {
            this.pendingClosed.set(id, { output, workspace });
        } catch (error) {
            void error;
            return;
        }
        this.logToken(`${LOG_PREFIX}:close-queued window=${id}`);
        this.requestResync();
    }

    // Test seam: pending close ids awaiting delivery.
    getPendingClosed(): ReadonlyArray<string> {
        return Object.freeze([...this.pendingClosed.keys()]);
    }

    // Closes scoped to one snapshot domain, excluding ids currently observed
    // live there (a live re-sighting proves id reuse: the close is stale and
    // drops here so the wire never carries a contradictory live+closed row).
    private closedForSnapshot(snapshot: PlanSnapshot): string[] {
        const live = new Set<string>();
        try {
            for (const entry of snapshot.windows) {
                live.add(entry.id);
            }
        } catch (error) {
            void error;
        }
        const out: string[] = [];
        try {
            for (const [id, homing] of this.pendingClosed) {
                if (homing.output !== snapshot.domainOutput || homing.workspace !== snapshot.domainWorkspace) {
                    continue;
                }
                if (live.has(id)) {
                    this.pendingClosed.delete(id);
                    this.logToken(`${LOG_PREFIX}:close-stale window=${id}`);
                    continue;
                }
                out.push(id);
            }
        } catch (error) {
            void error;
        }
        out.sort();
        return out;
    }

    // Clear delivered closes: only ids still homed where they were sent
    // (an id reused under new homing keeps its newer record).
    private clearClosedSent(sent: ReadonlyArray<string>, output: string, workspace: string): void {
        try {
            for (const id of sent) {
                const homing = this.pendingClosed.get(id);
                if (homing !== undefined && homing.output === output && homing.workspace === workspace) {
                    this.pendingClosed.delete(id);
                }
            }
        } catch (error) {
            void error;
        }
    }

    // Test seam: current settled tile-override membership (durable only).
    getIntentTileMembers(): ReadonlyArray<string> {
        return Object.freeze([...this.intentTileMembers]);
    }

    // Read-only fixed-size origin query for whole-workspace migration
    // (R-WS-12): the adapter-asserted provenance for one stable id, or null
    // when unrecorded or the live ref was replaced. Exact-ref guarded like
    // classification; never classifies, never mutates, never writes.
    originOfFixedClient(id: string, ref: object): "auto" | "suppress" | null {
        try {
            const record = this.fixedClients.get(id);
            if (record === undefined || record.ref !== ref) {
                return null;
            }
            if (record.kind === "auto") {
                return "auto";
            }
            // Ordinary "pinned" admissions ride the suppress wire shape so
            // migration preserves the tile; only the enable seam tells
            // them apart.
            if (record.kind === "suppress" || record.kind === "pinned") {
                return "suppress";
            }
            return null;
        } catch (error) {
            void error;
            return null;
        }
    }

    // Test seam: true once the single bootstrap read settles (first plan
    // admission is gated on it while the bridge is active).
    isIntentReady(): boolean {
        return this.intentReady;
    }

    // Entry seam for the send routes: true while the single bootstrap read
    // is outstanding. Workspace/output sends refuse on it (same bounded
    // deferred log as plan dispatch) so a hydration candidate never
    // actuates as tiled mid-bootstrap. No queue, no replay.
    isIntentBootstrapPending(): boolean {
        return this.intentActive && !this.intentReady;
    }

    // Q3 bootstrap: one bounded ReadFloatIntent before first plan admission.
    // Capability-gated (both entry hooks required) and idempotent; returns
    // false with zero behavior change when inactive, unenabled, or already
    // started. Targets the well-known planner name so a stopped planner
    // activates; the reply is fenced by call token plus correlation, and
    // shutdown plus late callbacks are discarded. Any terminal (ok,
    // degraded, rejected, malformed, transport, timeout) opens the gate
    // and runs one catch-up resync; only an ok reply adopts membership.
    startIntentBootstrap(): boolean {
        if (!this.enabled || this.intentActive) {
            return false;
        }
        if (
            typeof this.env.observeCompleteInventory !== "function" ||
            typeof this.env.onIntentHydrated !== "function" ||
            typeof this.env.callDbus !== "function" ||
            typeof this.env.scheduleOnce !== "function"
        ) {
            return false;
        }
        this.intentActive = true;
        this.intentReady = false;
        this.intentBusy = true;
        this.intentCallToken += 1;
        const token = this.intentCallToken;
        const correlation = this.intentCorrelation();
        if (correlation === null) {
            this.settleIntentRead(token, "-", "degraded", "malformed", [], 0, 0, [], 0, 0);
            return true;
        }
        // Complete inventory only: an unproven list omits live so the read
        // never prunes on partial evidence. Scoped absence never evicts.
        let live: ReadonlyArray<string> | null = null;
        try {
            const inventory = this.env.observeCompleteInventory();
            if (
                inventory !== null &&
                inventory !== undefined &&
                inventory.complete === true &&
                Array.isArray(inventory.ids) &&
                inventory.ids.length <= INTENT_MAX_MEMBERS
            ) {
                const seen = new Set<string>();
                const ids: string[] = [];
                let valid = true;
                for (const id of inventory.ids) {
                    if (!isOpaqueId(id) || seen.has(id)) {
                        valid = false;
                        break;
                    }
                    seen.add(id);
                    ids.push(id);
                }
                if (valid) {
                    live = ids;
                }
            }
        } catch (error) {
            void error;
            live = null;
        }
        const body: Record<string, unknown> = { v: 1, correlation_id: correlation };
        if (live !== null) {
            body["live"] = live;
        }
        let payload = "";
        try {
            payload = JSON.stringify(body);
        } catch (error) {
            void error;
            payload = "";
        }
        if (payload === "" || payload.length > INTENT_MAX_BYTES) {
            this.settleIntentRead(token, correlation, "degraded", "oversize", [], 0, 0, [], 0, 0);
            return true;
        }
        try {
            this.intentDeadlineCancel = this.env.scheduleOnce(INTENT_TIMEOUT_MS, () =>
                this.onIntentReadTimeout(token, correlation),
            );
        } catch (error) {
            void error;
            this.intentDeadlineCancel = null;
            this.settleIntentRead(token, correlation, "degraded", "transport", [], 0, 0, [], 0, 0);
            return true;
        }
        try {
            this.env.callDbus(
                PLAN_SERVICE,
                PLAN_OBJECT,
                PLAN_INTERFACE,
                INTENT_READ_METHOD,
                payload,
                (reply) => this.onIntentReadReply(reply, token, correlation),
            );
        } catch (error) {
            void error;
            this.clearIntentDeadline();
            this.settleIntentRead(token, correlation, "degraded", "transport", [], 0, 0, [], 0, 0);
        }
        return true;
    }

    private intentCorrelation(): string | null {
        const candidate = `${this.generation}-i${String(this.intentSeq)}`;
        if (!isCorrelationId(candidate)) {
            return null;
        }
        this.intentSeq += 1;
        return candidate;
    }

    private clearIntentDeadline(): void {
        const cancel = this.intentDeadlineCancel;
        this.intentDeadlineCancel = null;
        if (cancel !== null) {
            try {
                cancel();
            } catch (error) {
                void error;
            }
        }
    }

    private intentReadLog(
        correlation: string,
        outcome: string,
        stored: number,
        returned: number,
        tileStored: number,
        tileReturned: number,
        reason: string,
    ): void {
        try {
            this.env.log(
                `${LOG_PREFIX}:intent-read correlation=${correlation} outcome=${outcome} stored=${String(stored)} returned=${String(returned)} tile_stored=${String(tileStored)} tile_returned=${String(tileReturned)} reason=${reason}`,
            );
        } catch (error) {
            void error;
        }
    }

    private intentWriteLog(
        correlation: string,
        outcome: string,
        stored: number | null,
        tileStored: number | null,
        reason: string,
    ): void {
        try {
            this.env.log(
                `${LOG_PREFIX}:intent-write correlation=${correlation} outcome=${outcome} stored=${stored === null ? "-" : String(stored)} tile_stored=${tileStored === null ? "-" : String(tileStored)} reason=${reason}`,
            );
        } catch (error) {
            void error;
        }
    }

    private onIntentReadReply(reply: unknown, token: number, correlation: string): void {
        if (!this.intentActive || token !== this.intentCallToken || !this.intentBusy) {
            return;
        }
        if (typeof reply !== "string" || reply.length > INTENT_MAX_BYTES) {
            this.settleIntentRead(token, correlation, "degraded", "malformed", [], 0, 0, [], 0, 0);
            return;
        }
        let parsed: unknown = null;
        try {
            parsed = JSON.parse(reply);
        } catch (error) {
            void error;
            this.settleIntentRead(token, correlation, "degraded", "malformed", [], 0, 0, [], 0, 0);
            return;
        }
        if (!isRecord(parsed) || parsed["v"] !== 1 || parsed["correlation_id"] !== correlation) {
            this.settleIntentRead(token, correlation, "degraded", "malformed", [], 0, 0, [], 0, 0);
            return;
        }
        const outcome = parsed["outcome"];
        if (outcome === "ok") {
            const raw = parsed["members"];
            if (!Array.isArray(raw) || raw.length > INTENT_MAX_MEMBERS) {
                this.settleIntentRead(token, correlation, "degraded", "malformed", [], 0, 0, [], 0, 0);
                return;
            }
            const seen = new Set<string>();
            const members: string[] = [];
            for (const entry of raw) {
                if (!isOpaqueId(entry) || seen.has(entry)) {
                    this.settleIntentRead(token, correlation, "degraded", "malformed", [], 0, 0, [], 0, 0);
                    return;
                }
                seen.add(entry);
                members.push(entry);
            }
            // Tile side is additive and backward compatible: absent means an
            // old-format reply with no overrides; present must validate like
            // float and stay disjoint, else the whole read is malformed.
            const rawTile = parsed["tile"];
            let tile: string[] = [];
            if (rawTile !== undefined) {
                if (!Array.isArray(rawTile) || rawTile.length > INTENT_MAX_MEMBERS) {
                    this.settleIntentRead(token, correlation, "degraded", "malformed", [], 0, 0, [], 0, 0);
                    return;
                }
                for (const entry of rawTile) {
                    if (!isOpaqueId(entry) || seen.has(entry)) {
                        this.settleIntentRead(token, correlation, "degraded", "malformed", [], 0, 0, [], 0, 0);
                        return;
                    }
                    seen.add(entry);
                    tile.push(entry);
                }
            }
            const stored = parsed["stored"];
            const returned = parsed["returned"];
            const tileStored = rawTile === undefined ? 0 : parsed["tile_stored"];
            const tileReturned = rawTile === undefined ? 0 : parsed["tile_returned"];
            // Counts are validated, never trusted: safe non-negative
            // integers within the member bound, with returned exactly the
            // adopted membership length. Anything else is malformed.
            if (
                typeof stored !== "number" ||
                !Number.isInteger(stored) ||
                stored < 0 ||
                stored > INTENT_MAX_MEMBERS ||
                returned !== members.length
            ) {
                this.settleIntentRead(token, correlation, "degraded", "malformed", [], 0, 0, [], 0, 0);
                return;
            }
            if (rawTile !== undefined) {
                if (
                    typeof tileStored !== "number" ||
                    !Number.isInteger(tileStored) ||
                    tileStored < 0 ||
                    tileStored > INTENT_MAX_MEMBERS ||
                    tileReturned !== tile.length
                ) {
                    this.settleIntentRead(token, correlation, "degraded", "malformed", [], 0, 0, [], 0, 0);
                    return;
                }
            }
            this.settleIntentRead(token, correlation, "ok", "-", members, stored as number, returned as number, tile, tileStored as number, tileReturned as number);
            return;
        }
        if (outcome === "degraded" || outcome === "rejected") {
            this.settleIntentRead(token, correlation, outcome, sanitizeIntentReason(parsed["reason"]), [], 0, 0, [], 0, 0);
            return;
        }
        this.settleIntentRead(token, correlation, "degraded", "malformed", [], 0, 0, [], 0, 0);
    }

    private onIntentReadTimeout(token: number, correlation: string): void {
        if (!this.intentActive || token !== this.intentCallToken || !this.intentBusy) {
            return;
        }
        // Transport silence (missing callback, busy planner, dead service)
        // is diagnosed exactly like any other degraded read: proceed
        // empty with fixed reason and counts, never block admission.
        this.settleIntentRead(token, correlation, "degraded", "timeout", [], 0, 0, [], 0, 0);
    }

    // Single read terminal: adopts ok membership (unioned with any settled
    // events that landed mid-bootstrap, whose coalesced snapshot then
    // refreshes to the merged truth), keeps local mutations on every other
    // terminal, then opens the gate, hydrates the entry, and runs one
    // catch-up resync so the first plan admission observes hydrated floats.
    // Verified closes fenced during the read subtract from every terminal
    // so the callback never revives them; the fence clears here, once the
    // bootstrap lifecycle ends.
    private settleIntentRead(
        token: number,
        correlation: string,
        outcome: "ok" | "degraded" | "rejected",
        reason: string,
        members: ReadonlyArray<string>,
        stored: number,
        returned: number,
        tileMembers: ReadonlyArray<string>,
        tileStored: number,
        tileReturned: number,
    ): void {
        if (!this.intentActive || token !== this.intentCallToken) {
            return;
        }
        this.intentCallToken += 1;
        this.clearIntentDeadline();
        this.intentBusy = false;
        if (outcome === "ok") {
            const merged = new Set<string>(members);
            const mergedTile = new Set<string>(tileMembers);
            if (this.intentPendingWrite !== null) {
                for (const id of this.intentPendingWrite.members) {
                    merged.add(id);
                }
                for (const id of this.intentPendingWrite.tile) {
                    mergedTile.add(id);
                }
            }
            for (const id of this.intentBootstrapClosed) {
                merged.delete(id);
                mergedTile.delete(id);
            }
            // Float wins any cross-set collision so gaming/intent floats stay
            // untouched; tile never revives a float.
            for (const id of merged) {
                mergedTile.delete(id);
            }
            this.intentMembers = merged;
            this.intentTileMembers = mergedTile;
            if (this.intentPendingWrite !== null) {
                this.intentPendingWrite = { members: [...merged], tile: [...mergedTile] };
            }
        } else {
            for (const id of this.intentBootstrapClosed) {
                this.intentMembers.delete(id);
                this.intentTileMembers.delete(id);
            }
            if (this.intentPendingWrite !== null) {
                this.intentPendingWrite = { members: [...this.intentMembers], tile: [...this.intentTileMembers] };
            }
        }
        this.intentBootstrapClosed.clear();
        this.intentReady = true;
        this.intentReadLog(correlation, outcome, stored, returned, tileStored, tileReturned, reason);
        try {
            this.env.onIntentHydrated?.([...this.intentMembers], [...this.intentTileMembers]);
        } catch (error) {
            void error;
        }
        if (this.enabled) {
            try {
                this.requestResync();
            } catch (error) {
                void error;
            }
        }
        this.pumpIntent();
    }

    // Settled membership checkpoint (D3): called only on the applied
    // boundary that already committed the Q2 stage, so the durable record
    // moves exactly when native application is proven. No staging
    // machinery beyond the full snapshot below.
    // D7 fixed-window-only tile gate for an applied unfloat: the durable
    // tile side moves only when the override is fixed under the current
    // selected predicate at dispatch hints, carries fixed-auto origin in
    // the flight snapshot, or already holds durable tile identity (which
    // retains through hint loss). Ordinary non-fixed unfloats clear float
    // without creating tile so a later fixed gain recomputes float.
    private unfloatPersistsTileOverride(id: string, flightState: PendingFlight): boolean {
        if (this.intentTileMembers.has(id)) {
            return true;
        }
        const snapshot = flightState.snapshot;
        for (const entry of snapshot.windows) {
            if (entry.id !== id) {
                continue;
            }
            if (entry.fixedAuto === true) {
                return true;
            }
            try {
                if (isFixedSize(entry.minSize, entry.maxSize, this.readFixedSizePredicate())) {
                    return true;
                }
            } catch (error) {
                void error;
            }
            return false;
        }
        return false;
    }

    private updateSettledIntentFromFlight(flightState: PendingFlight): void {
        if (!this.intentActive) {
            return;
        }
        const target = flightState.floatTarget;
        if (target === null) {
            return;
        }
        if (target.floating) {
            this.intentMembers.add(target.window);
            this.intentTileMembers.delete(target.window);
        } else {
            this.intentMembers.delete(target.window);
            if (this.unfloatPersistsTileOverride(target.window, flightState)) {
                this.intentTileMembers.add(target.window);
            }
        }
        this.scheduleIntentWrite();
    }

    // Queue the full settled snapshot behind the single in-flight intent
    // call (one latest coalescing, never a retry ledger). A failed send
    // keeps this exact snapshot dropped but retains local intent: the next
    // settled update re-sends the full set.
    private scheduleIntentWrite(): void {
        if (!this.intentActive) {
            return;
        }
        this.intentPendingWrite = { members: [...this.intentMembers], tile: [...this.intentTileMembers] };
        this.pumpIntent();
    }

    private pumpIntent(): void {
        if (!this.intentActive || !this.intentReady || this.intentBusy) {
            return;
        }
        const snapshot = this.intentPendingWrite;
        if (snapshot === null) {
            return;
        }
        if (snapshot.members.length > INTENT_MAX_MEMBERS || snapshot.tile.length > INTENT_MAX_MEMBERS) {
            this.intentWriteLog("-", "unavailable", snapshot.members.length, snapshot.tile.length, "oversize");
            return;
        }
        const correlation = this.intentCorrelation();
        if (correlation === null) {
            this.intentWriteLog("-", "unavailable", snapshot.members.length, snapshot.tile.length, "malformed");
            return;
        }
        let payload = "";
        try {
            payload = JSON.stringify({ v: 1, correlation_id: correlation, members: snapshot.members, tile: snapshot.tile });
        } catch (error) {
            void error;
            this.intentWriteLog(correlation, "unavailable", snapshot.members.length, snapshot.tile.length, "oversize");
            return;
        }
        if (payload === "" || payload.length > INTENT_MAX_BYTES) {
            this.intentWriteLog(correlation, "unavailable", snapshot.members.length, snapshot.tile.length, "oversize");
            return;
        }
        this.intentCallToken += 1;
        const token = this.intentCallToken;
        const sentCount = snapshot.members.length;
        const sentTile = snapshot.tile.length;
        try {
            this.intentDeadlineCancel = this.env.scheduleOnce(INTENT_TIMEOUT_MS, () =>
                this.onIntentWriteTimeout(token, correlation, sentCount, sentTile),
            );
        } catch (error) {
            void error;
            this.intentDeadlineCancel = null;
            // Diagnosed transport terminal without sending: the pending
            // snapshot is preserved for the next settled update (no
            // autonomous retry), matching the read path.
            this.intentWriteLog(correlation, "unavailable", sentCount, sentTile, "transport");
            return;
        }
        this.intentPendingWrite = null;
        this.intentBusy = true;
        try {
            this.env.callDbus(
                PLAN_SERVICE,
                PLAN_OBJECT,
                PLAN_INTERFACE,
                INTENT_WRITE_METHOD,
                payload,
                (reply) => this.onIntentWriteReply(reply, token, correlation, sentCount, sentTile),
            );
        } catch (error) {
            void error;
            this.clearIntentDeadline();
            this.intentBusy = false;
            this.intentWriteLog(correlation, "unavailable", null, null, "transport");
            // Deliver only already-queued settled updates (a synchronous
            // setter may have coalesced a newer snapshot mid-call); never
            // an autonomous retry of this failed snapshot.
            this.pumpIntent();
        }
    }

    // Store ACK terminal, logged distinctly from planned-applied: stored
    // carries the count; rejected/unavailable carry the fixed reason and
    // keep local intent for the next settled update. A newer coalesced
    // snapshot always pumps next, so full snapshots never reorder.
    private onIntentWriteReply(reply: unknown, token: number, correlation: string, sentCount: number, sentTile: number): void {
        if (!this.intentActive || token !== this.intentCallToken || !this.intentBusy) {
            return;
        }
        this.clearIntentDeadline();
        this.intentBusy = false;
        if (typeof reply === "string" && reply.length <= INTENT_MAX_BYTES) {
            try {
                const parsed: unknown = JSON.parse(reply);
                if (isRecord(parsed) && parsed["v"] === 1 && parsed["correlation_id"] === correlation) {
                    const outcome = parsed["outcome"];
                    if (outcome === "stored") {
                        // The store ACK must echo the sent full snapshot
                        // counts exactly; anything else is malformed. Old
                        // planners omit tile_stored: accept when the float
                        // count matches and no tile was sent.
                        const ackTile = parsed["tile_stored"];
                        if (parsed["stored"] !== sentCount) {
                            this.intentWriteLog(correlation, "unavailable", null, null, "malformed");
                            this.pumpIntent();
                            return;
                        }
                        if (ackTile === undefined) {
                            if (sentTile !== 0) {
                                this.intentWriteLog(correlation, "unavailable", null, null, "malformed");
                                this.pumpIntent();
                                return;
                            }
                        } else if (ackTile !== sentTile) {
                            this.intentWriteLog(correlation, "unavailable", null, null, "malformed");
                            this.pumpIntent();
                            return;
                        }
                        this.intentWriteLog(correlation, "stored", sentCount, sentTile, "-");
                        this.pumpIntent();
                        return;
                    }
                    if (outcome === "rejected" || outcome === "unavailable") {
                        this.intentWriteLog(correlation, outcome, null, null, sanitizeIntentReason(parsed["reason"]));
                        this.pumpIntent();
                        return;
                    }
                }
            } catch (error) {
                void error;
            }
        }
        this.intentWriteLog(correlation, "unavailable", null, null, "malformed");
        this.pumpIntent();
    }

    private onIntentWriteTimeout(token: number, correlation: string, _sentCount: number, _sentTile: number): void {
        if (!this.intentActive || token !== this.intentCallToken || !this.intentBusy) {
            return;
        }
        this.intentBusy = false;
        this.intentWriteLog(correlation, "unavailable", null, null, "timeout");
        this.pumpIntent();
    }

    // Evict fixed-size bookkeeping for one verified native removal by
    // exact object (D3): relocation survivors carry a different live ref
    // and keep their records; truly removed refs drop.
    private evictFixedByRef(ref: object): void {
        for (const [id, record] of [...this.fixedClients]) {
            if (record.ref === ref) {
                this.fixedClients.delete(id);
            }
        }
    }

    // D6 enable seam: reset automatic and ordinary-admission pins for
    // the CURRENTLY SIGHTED members of one domain so the resync
    // re-evaluates every window under CURRENT hints/predicate. Reset
    // keys on the sighted id set from the confirming observation, never
    // on the stored domain (refreshed only by dispatch sightings, so a
    // move while minimized/background-omitted would otherwise reset a
    // foreign mark or miss the new homing). Members absent from the
    // sightings keep their records, so omitted automatic identity is
    // never erased. Explicit tile overrides (suppress), intentional
    // floats and sticky (recordless) survive. No writes here; the
    // resync observation carries the outcome.
    retileAutomaticFixed(output: string, workspace: string, sightedIds: ReadonlyArray<string>): void {
        void output;
        void workspace;
        const sighted = new Set<string>();
        for (const id of sightedIds) {
            if (typeof id === "string") {
                sighted.add(id);
            }
        }
        for (const [id, record] of [...this.fixedClients]) {
            if (sighted.has(id) && (record.kind === "auto" || record.kind === "pinned")) {
                this.fixedClients.delete(id);
            }
        }
    }

    // Q2 fixed-size membership-only classification (D1-D8) over one hinted
    // snapshot with live refs from its observation. Automatic floats gain
    // floating membership plus fixed_auto provenance only: no geometry,
    // focus, keep-above, or input interference happens here or because of
    // this flag (D8). Admission-only in both directions (D2): a pinned
    // normal live client that later reports fixed hints stays tiled via
    // fixed_suppress, and a retained automatic float stays floating across
    // hint loss. Brand-new fixed windows float (D1); explicit user tile
    // wins (D3); born fullscreen bypasses (D5); born maximized floats
    // beneath its native overlay with no reserved tile (D4); sticky and
    // intentional floats stay intentional (D6). No record is ever evicted
    // for absence from one scoped snapshot: cross-domain, background, and
    // minimized-omission snapshots keep foreign records, and each sighting
    // refreshes the stored domain so retile targets current homing.
    private withFixedSizeFloats(snapshot: PlanSnapshot, observed: PlanObserved): PlanSnapshot {
        const predicate = this.readFixedSizePredicate();
        const refById = new Map<string, object>();
        for (const entry of observed.windows) {
            if (!refById.has(entry.id)) {
                refById.set(entry.id, entry.ref);
            }
        }
        const windows = snapshot.windows.map((entry) => {
            const ref = refById.get(entry.id);
            if (ref === undefined) {
                return entry;
            }
            // Exact-ref lifetime fence: a replaced native reference under
            // a reused id starts over and classifies again (D3).
            const prior = this.fixedClients.get(entry.id);
            if (prior !== undefined && prior.ref !== ref) {
                this.fixedClients.delete(entry.id);
            }
            // Verified native sticky adopts as intentional (D6): any
            // record for the same ref drops, rides without provenance.
            if (entry.sticky === true) {
                const live = this.fixedClients.get(entry.id);
                if (live !== undefined && live.ref === ref) {
                    this.fixedClients.delete(entry.id);
                }
                const { fixedAuto: _dropped, fixedSuppress: _drop2, ...rest } = entry;
                void _dropped;
                void _drop2;
                return { ...rest };
            }
            const record = this.fixedClients.get(entry.id);
            const liveDomain = `${entry.output}\u0000${entry.workspace}`;
            // D7 restart hydration: a matched stored override plants kind=
            // suppress even when current hints are non-fixed, so the saved
            // explicit win stays authoritative through hint/predicate loss
            // and a later D6 enable keeps it (enable drops auto/pinned only).
            // Wire suppression still rides only on fixed hints via the shared
            // suppress path below; hintless rows stay byte-identical plain.
            // Sticky already adopted above; intentional-float entries keep
            // their floating flag through the plain return, and fullscreen
            // rows keep planner overlay fences.
            if (record === undefined && this.intentActive && this.intentReady && this.intentTileMembers.has(entry.id)) {
                this.fixedClients.set(entry.id, { ref, kind: "suppress", domain: liveDomain, reported: true });
                if (isFixedSize(entry.minSize, entry.maxSize, predicate)) {
                    const { fixedAuto: _dropped, ...rest } = entry;
                    void _dropped;
                    return { ...rest, fixedSuppress: true as const };
                }
                const { fixedAuto: _dropped, fixedSuppress: _drop2, ...rest } = entry;
                void _dropped;
                void _drop2;
                return { ...rest };
            }
            // Retained automatic floats keep identity across hint changes
            // (D2): still floating with origin, still no writes. Refresh
            // homing. Enable reset deletes these records first, so this
            // path never skips the D6 recheck.
            if (record !== undefined && record.kind === "auto" && record.ref === ref) {
                record.domain = liveDomain;
                return entry.floating === true
                    ? { ...entry, fixedAuto: true as const }
                    : { ...entry, floating: true, fixedAuto: true as const };
            }
            // Tile pins stay tiled across hint changes (D2/D3): explicit
            // "suppress" wins and ordinary "pinned" admissions share this
            // path and wire shape. Refresh homing; never float from here.
            // Wire suppression rides only on fixed hints, so hintless and
            // non-fixed rows stay byte-identical.
            if (record !== undefined && (record.kind === "suppress" || record.kind === "pinned") && record.ref === ref) {
                record.domain = liveDomain;
                if (!isFixedSize(entry.minSize, entry.maxSize, predicate)) {
                    const { fixedAuto: _dropped, fixedSuppress: _drop2, ...rest } = entry;
                    void _dropped;
                    void _drop2;
                    return { ...rest };
                }
                const { fixedAuto: _dropped, ...rest } = entry;
                void _dropped;
                return { ...rest, fixedSuppress: true as const };
            }
            // Held born-fullscreen bypasses the classifier (D5): synthetic
            // floating only, never automatic. Later fullscreen retains its
            // slot and is never actuated.
            if (this.heldInitialFullscreen.get(entry.id) === ref) {
                return entry;
            }
            if (entry.fullscreen) {
                return entry;
            }
            // Intentional floats are never automatic (D6): they ride
            // without provenance and pin nothing. (Sticky observations
            // adopt above.)
            if (entry.floating === true) {
                return entry;
            }
            if (!isFixedSize(entry.minSize, entry.maxSize, predicate)) {
                // Pin ordinary first admission: a tiled live client seen
                // normal records "pinned" for later. Distinct from explicit
                // "suppress" wins: enable reset drops pins but keeps wins.
                // Wire stays byte-identical until hints are fixed.
                this.fixedClients.set(entry.id, { ref, kind: "pinned", domain: liveDomain, reported: true });
                const { fixedAuto: _dropped, fixedSuppress: _drop2, ...rest } = entry;
                void _dropped;
                void _drop2;
                return { ...rest };
            }
            this.fixedClients.set(entry.id, { ref, kind: "auto", domain: liveDomain, reported: false });
            return { ...entry, floating: true, fixedAuto: true as const };
        });
        return { ...snapshot, windows: Object.freeze(windows) };
    }

    // Attach freshly read hint sizes to every snapshot member backed by the
    // given observation (matched by id). Members without a live ref keep
    // their existing hints.
    private attachHintSizes(snapshot: PlanSnapshot, observed: PlanObserved): PlanSnapshot {
        if (typeof this.env.readWindowConstraints !== "function") {
            return snapshot;
        }
        const refById = new Map<string, object>();
        for (const entry of observed.windows) {
            if (!refById.has(entry.id)) {
                refById.set(entry.id, entry.ref);
            }
        }
        if (refById.size === 0) {
            return snapshot;
        }
        const windows = snapshot.windows.map((entry) => {
            const ref = refById.get(entry.id);
            if (ref === undefined) {
                return entry;
            }
            const hints = this.hintSizesFor(ref);
            if (hints.minSize === undefined && hints.maxSize === undefined) {
                return entry;
            }
            return {
                ...entry,
                ...(hints.minSize === undefined ? {} : { minSize: hints.minSize }),
                ...(hints.maxSize === undefined ? {} : { maxSize: hints.maxSize }),
            };
        });
        return { ...snapshot, windows };
    }

    // Fresh declared minimum behind one live ref at actuation time.
    // Best-effort and fail-closed: a missing reader, a throw, or an
    // unreadable value keeps the planned rect (unknown means no hint, never
    // a learned or persistent floor). Integer shape mirrors hintSizesFor;
    // meaningfulness (1..16384) is judged in overconstrainedEffective.
    private freshMinFor(ref: object): { readonly w: number; readonly h: number } | null {
        try {
            const reader = this.env.readWindowConstraints;
            if (typeof reader !== "function") {
                return null;
            }
            const constraints = reader(ref);
            if (constraints === null || typeof constraints !== "object") {
                return null;
            }
            const min = (constraints as PlanWindowConstraints).minSize;
            if (min === null || min === undefined || !Number.isInteger(min.w) || !Number.isInteger(min.h)) {
                return null;
            }
            return { w: min.w, h: min.h };
        } catch (error) {
            void error;
            return null;
        }
    }

    // Effective write target for one planned member: overconstrained members
    // rise to the freshly observed declared minimum at the planned origin;
    // all other members keep the planned rect. `minimumPlaced` marks a member
    // whose minimum actually raised an extent (for the structured
    // diagnostic); satisfied or unknown/sentinel minima pass through quiet.
    // `minimumGuarded` marks minimum-driven accounting independently of the
    // planner flag: the plan touches the declared floor, or the live
    // observation undershoots it. This also covers planner-adapted
    // projections that satisfy the minimum without flagging (the host may
    // still hold them short). Ordinary feasible members (plan with slack
    // above the floor, observation honoring it) stay unguarded.
    private effectiveTargetFor(
        entry: PlanGeometryEntry,
        refById: ReadonlyMap<string, object>,
        observed?: PlanRect,
    ): { rect: PlanRect; minimumPlaced: boolean; minimumGuarded: boolean } {
        const ref = refById.get(entry.window);
        const minSize = ref === undefined ? null : this.freshMinFor(ref);
        const minW = minSize === null ? null : meaningfulMinExtent(minSize.w);
        const minH = minSize === null ? null : meaningfulMinExtent(minSize.h);
        const guarded =
            (minW !== null && entry.rect.w <= minW) ||
            (minH !== null && entry.rect.h <= minH) ||
            (observed !== undefined &&
                ((minW !== null && observed.w < minW) || (minH !== null && observed.h < minH)));
        if (!entry.overconstrained) {
            return { rect: entry.rect, minimumPlaced: false, minimumGuarded: guarded };
        }
        const effective = overconstrainedEffective(entry.rect, minSize);
        if (effective.w === entry.rect.w && effective.h === entry.rect.h) {
            return { rect: entry.rect, minimumPlaced: false, minimumGuarded: guarded };
        }
        return { rect: effective, minimumPlaced: true, minimumGuarded: guarded };
    }

    requestMove(direction: unknown): void {
        if (!this.enabled) {
            this.logToken(`${LOG_PREFIX}:move-refused-disabled`);
            return;
        }
        if (!isDirection(direction)) {
            this.logToken(`${LOG_PREFIX}:move-refused-invalid-direction`);
            return;
        }
        if (this.r4Flight !== null) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=move`);
            return;
        }
        if (this.inFlight) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=move`);
            return;
        }
        const directional = this.readDirectional(direction, true);
        if (directional.kind === "invalid") {
            this.logToken(`${LOG_PREFIX}:move-refused-ambiguous`);
            return;
        }
        const observed =
            directional.kind === "ready" ? directional.observed : this.freshObserved();
        if (observed === null) {
            this.logToken(`${LOG_PREFIX}:move-refused-observe`);
            return;
        }
        // Fullscreen precedence (both origins): a fullscreen subject is
        // never cleared or moved.
        if (this.windowIsFullscreen(observed, observed.focusedId)) {
            this.logToken(`${LOG_PREFIX}:move-refused-fullscreen`);
            return;
        }
        // G-06 maximized directional move (REQ-MAX-08): unmaximize first
        // with exactly one native clear, before ordinary tiled/float
        // routing, for a maximized focused window on either origin. No
        // clear for non-maximized subjects, for fullscreen (above), or for
        // a float-ineligible subject (unmanaged: the established float
        // route refuses it unchanged below). After a confirmed clear the
        // move routes via a FRESH directional observation, never carried
        // pre-clear topology: floats take the established half-snap,
        // tiles take the ordinary planner move. Reuses the B9
        // intentional-unfloat discipline (exact-reference echo fence, one
        // attempt, identity/focus/fullscreen/domain/busy guards, no
        // speculative delayed intent or retry). An unconfirmed clear
        // refuses narrowly with no structural move; a later press can
        // retry. Focus is unchanged by this command.
        if (this.windowIsMaximized(observed, observed.focusedId)) {
            if (observed.activeExcluded && floatSubjectEntry(observed) === null) {
                this.requestFloatMove(observed, direction);
                return;
            }
            if (!observed.activeExcluded && !this.isTiledDomain(observed.domainOutput, observed.domainWorkspace)) {
                this.logToken(`${LOG_PREFIX}:move-refused-workspace-floating`);
                return;
            }
            const windowId = observed.focusedId;
            const mover = observed.windows.find((entry) => entry.id === windowId);
            const nativeRef = mover?.ref;
            const resourceClass =
                mover !== undefined && isOpaqueId(mover.resourceClass) ? mover.resourceClass : "unknown";
            if (nativeRef === undefined) {
                this.logToken(`${LOG_PREFIX}:move-refused-maximize-clear window=${windowId} resource_class=${resourceClass} cause=observe-missing recovery=retry-on-next-press`);
                return;
            }
            this.logToken(`${LOG_PREFIX}:move-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=issued`);
            this.maximizeAdmissionEcho = nativeRef;
            this.logToken(`${LOG_PREFIX}:maximize-admission-echo-armed`);
            let outcome: MaximizeClearOutcome = "threw";
            try {
                outcome = this.env.clearMaximize(nativeRef);
            } catch (error) {
                void error;
            }
            this.logToken(`${LOG_PREFIX}:move-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=${outcome}`);
            if (this.maximizeAdmissionEcho !== null) {
                this.maximizeAdmissionEcho = null;
                this.logToken(`${LOG_PREFIX}:maximize-admission-echo-cleared-no-signal`);
            }
            if (outcome !== "invoked") {
                this.logToken(`${LOG_PREFIX}:move-refused-maximize-clear window=${windowId} resource_class=${resourceClass} cause=native-write-${outcome} recovery=retry-on-next-press`);
                return;
            }
            // Fresh directional routing: a stale pre-clear topology must
            // never drive a transfer if outputs/views changed synchronously.
            const reroll = this.readDirectional(direction, true);
            if (reroll.kind === "invalid") {
                this.logToken(`${LOG_PREFIX}:move-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=directional-raced`);
                this.logToken(`${LOG_PREFIX}:move-refused-maximize-clear window=${windowId} resource_class=${resourceClass} cause=directional-raced recovery=retry-on-next-press`);
                return;
            }
            let liveObserved: PlanObserved | null = null;
            try {
                liveObserved = reroll.kind === "ready" ? reroll.observed : this.freshObserved();
            } catch (error) {
                void error;
                liveObserved = null;
            }
            const current = liveObserved?.windows.find((entry) => entry.id === windowId);
            const settled =
                current !== undefined && current.ref === nativeRef && !current.maximized ? current : undefined;
            if (liveObserved === null || settled === undefined) {
                const seen =
                    liveObserved === null
                        ? "observed-missing"
                        : current === undefined || current.ref !== nativeRef
                          ? "observed-absent"
                          : "observed-maximized";
                const cause =
                    liveObserved === null
                        ? "observe-missing"
                        : current === undefined || current.ref !== nativeRef
                          ? "identity-raced"
                          : "still-maximized";
                this.logToken(`${LOG_PREFIX}:move-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=${seen}`);
                this.logToken(`${LOG_PREFIX}:move-refused-maximize-clear window=${windowId} resource_class=${resourceClass} cause=${cause} recovery=retry-on-next-press`);
                return;
            }
            if (
                liveObserved.domainOutput !== observed.domainOutput ||
                liveObserved.domainWorkspace !== observed.domainWorkspace
            ) {
                this.logToken(`${LOG_PREFIX}:move-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=domain-raced`);
                this.logToken(`${LOG_PREFIX}:move-refused-maximize-clear window=${windowId} resource_class=${resourceClass} cause=domain-raced recovery=retry-on-next-press`);
                return;
            }
            if (settled.fullscreen || liveObserved.focusedId !== windowId) {
                const seen = settled.fullscreen ? "observed-fullscreen" : "observed-focus-raced";
                const cause = settled.fullscreen ? "fullscreen-raced" : "focus-raced";
                this.logToken(`${LOG_PREFIX}:move-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=${seen}`);
                this.logToken(`${LOG_PREFIX}:move-refused-maximize-clear window=${windowId} resource_class=${resourceClass} cause=${cause} recovery=retry-on-next-press`);
                return;
            }
            if (this.r4Flight !== null || this.inFlight) {
                this.logToken(`${LOG_PREFIX}:busy-refused kind=move`);
                return;
            }
            this.logToken(`${LOG_PREFIX}:move-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=observed-cleared`);
            // Route on the fresh observation: a cleared float takes the
            // established half-snap with all its guards; a cleared tile
            // takes the ordinary planner move below.
            if (liveObserved.activeExcluded) {
                this.requestFloatMove(liveObserved, direction);
                return;
            }
            if (!this.isTiledDomain(liveObserved.domainOutput, liveObserved.domainWorkspace)) {
                this.logToken(`${LOG_PREFIX}:move-refused-workspace-floating`);
                return;
            }
            this.dispatchMove(liveObserved, direction);
            return;
        }
        if (observed.activeExcluded) {
            this.requestFloatMove(observed, direction);
            return;
        }
        if (!this.isTiledDomain(observed.domainOutput, observed.domainWorkspace)) {
            this.logToken(`${LOG_PREFIX}:move-refused-workspace-floating`);
            return;
        }
        this.dispatchMove(observed, direction);
    }

    // Ordinary tiled planner-move dispatch shared by the direct path and
    // the G-06 post-clear path (which supplies its fresh observation).
    private dispatchMove(observed: PlanObserved, direction: PlanDirection): void {
        const snapshot = this.carriedSnapshot(observed);
        this.noteObservation(snapshot.fingerprint);
        if (KWIN_TRACE_ENABLED && typeof this.env.readWindowConstraints === "function") {
            for (const entry of observed.windows) {
                this.traceConstraints(
                    "pre-plan",
                    "plan",
                    entry.id,
                    entry.output,
                    isOpaqueId(entry.resourceClass) ? entry.resourceClass : "unknown",
                    entry.ref,
                    this.workAreaFor(snapshot, entry.output, entry.workspace),
                    null,
                    entry.rect,
                );
            }
        }
        this.dispatch({
            op: "move",
            snapshot,
            removed: null,
            body: {
                op: "move",
                window: snapshot.focusedId,
                direction,
                // Rust preserves local R1/R2/R3 but stages R4 only when the
                // adapter transfers outputs. True solely when every R4
                // native capability is supplied.
                ...(snapshot.domains?.length === 2
                    ? { cross_output_transfer: crossOutputTransferSupported(this.env) }
                    : {}),
                // R-MOV-03 same-axis mode. The default group-with-neighbor
                // stays wire-omitted so historical requests are
                // byte-identical; swap-with-neighbor carries explicitly.
                // Invalid resolves to default.
                ...(this.readSameAxisMove() === "swap-with-neighbor"
                    ? { same_axis_move: "swap-with-neighbor" }
                    : {}),
            },
            direction,
        });
    }

    private readSameAxisMove(): string {
        try {
            const reader = this.env.readSameAxisMove;
            if (typeof reader !== "function") {
                return "group-with-neighbor";
            }
            return normalizeSameAxisMove(reader());
        } catch (error) {
            void error;
            return "group-with-neighbor";
        }
    }

    private readFixedSizePredicate(): string {
        try {
            const reader = this.env.readFixedSizePredicate;
            if (typeof reader !== "function") {
                return FIXED_SIZE_PREDICATE_DEFAULT;
            }
            return normalizeFixedSizePredicate(reader());
        } catch (error) {
            void error;
            return FIXED_SIZE_PREDICATE_DEFAULT;
        }
    }

    private readBornMaximized(): string {
        try {
            const reader = this.env.readBornMaximized;
            if (typeof reader !== "function") {
                return BORN_MAXIMIZED_DEFAULT;
            }
            return normalizeBornMaximized(reader());
        } catch (error) {
            void error;
            return BORN_MAXIMIZED_DEFAULT;
        }
    }

    private isFloatingFirst(id: string): boolean {
        try {
            if (this.floatingFirstSeen.has(id)) {
                return true;
            }
            const reader = this.env.isFloatingFirst;
            if (typeof reader !== "function") {
                return false;
            }
            return reader(id) === true;
        } catch (error) {
            void error;
            return false;
        }
    }

    // R-OUT-05 routing attempt. Exact birth identity only: the candidate must
    // be the signaled birth ref, the active window, the single unknown id,
    // and cross-output against a fresh anchor, with no flight holding the
    // single-flight. Returns the observation to continue with, or null to
    // abort on uncertain post-write state. Seen ids and the anchor advance on
    // every continued path, so a newcomer is never re-routed after admission
    // or native user movement.
    private maybeRouteNewcomerToManagedOutput(fresh: PlanObserved, birthRef: object | null): PlanObserved | null {
        this.routeRound += 1;
        const anchor = this.routeAnchor;
        const anchorFresh = anchor !== null && this.routeRound - anchor.round <= 1;
        const unknown: PlanObservedWindow[] = [];
        for (const entry of fresh.windows) {
            if (!this.routeSeenIds.has(entry.id) && !this.appliedById.has(entry.id)) {
                unknown.push(entry);
            }
        }
        const candidate = unknown.length === 1 ? (unknown[0] as PlanObservedWindow) : null;
        let continued: PlanObserved | null = fresh;
        if (birthRef === null) {
            // Ordinary round: bookkeeping only, silent.
        } else if (candidate === null) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-${unknown.length === 0 ? "birth-unobserved" : "multi-unknown"}`);
        } else if (candidate.ref !== birthRef) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-birth-mismatch window=${candidate.id}`);
        } else if (fresh.activeRef !== candidate.ref) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-inactive-newcomer window=${candidate.id}`);
        } else if (anchor === null || !anchorFresh) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-${anchor === null ? "no-managed-domain" : "stale-anchor"} window=${candidate.id}`);
        } else if (fresh.domainOutput === anchor.output) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-same-output window=${candidate.id}`);
        } else if (this.inFlight || this.r4Flight !== null || this.deferredAuto !== null) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-flight-busy window=${candidate.id}`);
        } else {
            continued = this.attemptNewcomerRoute(fresh, candidate, anchor);
        }
        if (continued === null) {
            return null;
        }
        this.noteRouteSeen(continued);
        this.updateRouteAnchor(continued);
        return continued;
    }

    private noteRouteSeen(observed: PlanObserved): void {
        try {
            for (const entry of observed.windows) {
                if (this.routeSeenIds.size >= 4096) {
                    const oldest = this.routeSeenIds.values().next();
                    if (oldest.done) {
                        break;
                    }
                    this.routeSeenIds.delete(oldest.value);
                }
                this.routeSeenIds.add(entry.id);
            }
        } catch (error) {
            void error;
        }
    }

    // Anchor housekeeping: the domain whose active window is an eligible
    // managed member (observed flags only). An overlay, floating, or sticky
    // focus never anchors, so a fullscreen/float-held view cannot bless a
    // later route; the target itself is re-proved live below anyway.
    private updateRouteAnchor(observed: PlanObserved): void {
        try {
            const active = observed.windows.find((entry) => entry.ref === observed.activeRef);
            if (active === undefined) {
                return;
            }
            if (active.fullscreen || active.maximized || active.floating === true || active.sticky === true) {
                return;
            }
            this.routeAnchor = { output: observed.domainOutput, workspace: observed.domainWorkspace, round: this.routeRound };
        } catch (error) {
            void error;
        }
    }

    // D06 synchronous capture on the native activated notification. Only a
    // known managed target identity reanchors: the signaled ref must be the
    // live active window and the fresh observation's activeRef, and the
    // focused member must be observed-eligible (no overlay/float/sticky),
    // tiled, unheld, non-transient, non-fixed, and already seen or applied.
    // An unknown newcomer's own activation (including a pre-added birth
    // activation) never reanchors; a null/absent payload, a stale target, or
    // any unreadable guard refuses silently. Round stays current so the next
    // debounced refresh treats the capture as fresh.
    private captureRouteAnchorOnActivated(target: object | undefined): void {
        try {
            if (typeof target !== "object" || target === null) {
                return;
            }
            let currentActive: object | null = null;
            try {
                currentActive = this.env.active();
            } catch (error) {
                void error;
                currentActive = null;
            }
            if (currentActive !== target) {
                return;
            }
            const fresh = this.freshObserved();
            if (fresh === null || fresh.activeRef !== target) {
                return;
            }
            const active = fresh.windows.find((entry) => entry.ref === fresh.activeRef);
            if (active === undefined) {
                return;
            }
            if (active.fullscreen || active.maximized || active.floating === true || active.sticky === true) {
                return;
            }
            if (!this.isTiledDomain(fresh.domainOutput, fresh.domainWorkspace)) {
                return;
            }
            if (this.heldInitialFullscreen.get(active.id) === active.ref) {
                return;
            }
            if (!this.routeSeenIds.has(active.id) && !this.appliedById.has(active.id)) {
                return;
            }
            let transient: boolean | null = null;
            try {
                transient = this.env.readIsTransient?.(active.ref) ?? null;
            } catch (error) {
                void error;
                transient = null;
            }
            if (transient !== false) {
                return;
            }
            const hints = this.hintSizesFor(active.ref);
            try {
                if (isFixedSize(hints.minSize, hints.maxSize, this.readFixedSizePredicate())) {
                    return;
                }
            } catch (error) {
                void error;
                return;
            }
            try {
                const record = this.fixedClients.get(active.id);
                if (record !== undefined && record.ref === active.ref && (record.kind === "auto" || record.kind === "suppress")) {
                    return;
                }
            } catch (error) {
                void error;
                return;
            }
            this.routeAnchor = { output: fresh.domainOutput, workspace: fresh.domainWorkspace, round: this.routeRound };
        } catch (error) {
            void error;
        }
    }

    private attemptNewcomerRoute(
        fresh: PlanObserved,
        candidate: PlanObservedWindow,
        anchor: NewcomerRouteDomain,
    ): PlanObserved | null {
        // Fixed-size provenance: the carried snapshot plus a synchronous
        // constraint-hint check, since auto-classification can lag birth by
        // a round. Hints mark fixed-size windows (games among them), never a
        // claim that every fixed window is a game.
        const snapshot = this.carriedSnapshot(fresh);
        const snapEntry = snapshot.windows.find((entry) => entry.id === candidate.id);
        const hints = this.hintSizesFor(candidate.ref);
        const fixedHint = isFixedSize(hints.minSize, hints.maxSize, this.readFixedSizePredicate());
        const decision = decideNewcomerOutputRoute({
            newcomerOutput: fresh.domainOutput,
            flags: {
                fullscreen: snapEntry?.fullscreen ?? candidate.fullscreen,
                maximized: snapEntry?.maximized ?? candidate.maximized,
                floating: (snapEntry?.floating ?? candidate.floating) === true,
                sticky: (snapEntry?.sticky ?? candidate.sticky) === true,
                fixedAuto: snapEntry?.fixedAuto === true || fixedHint ? true : undefined,
                fixedSuppress: snapEntry?.fixedSuppress,
            },
            unknownCount: 1,
            addedSinceRefresh: true,
            lastManaged: anchor,
            flightBusy: false,
        });
        if (!decision.ok) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-${decision.reason} window=${candidate.id}`);
            return fresh;
        }
        // Transient/dialog: unclassifiable refuses. Read live per attempt;
        // the foreground observation carries no transient signal.
        let transient: boolean | null = null;
        try {
            transient = this.env.readIsTransient?.(candidate.ref) ?? null;
        } catch (error) {
            void error;
            transient = null;
        }
        if (transient !== false) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-${transient === true ? "transient" : "transient-unknown"} window=${candidate.id}`);
            return fresh;
        }
        // Stale-target guards, all before writes: the anchor desktop must
        // still be shown on a present, tiled, overlay-free output.
        let shown: string | null = null;
        try {
            shown = this.env.readShownWorkspace?.(decision.target.output) ?? null;
        } catch (error) {
            void error;
            shown = null;
        }
        if (shown === null || shown !== decision.target.workspace) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-stale-target window=${candidate.id}`);
            return fresh;
        }
        if (!this.isTiledDomain(decision.target.output, decision.target.workspace)) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-target-floating window=${candidate.id}`);
            return fresh;
        }
        let overlay: boolean | null = null;
        try {
            overlay = this.env.readFullscreenOnDomain?.(decision.target.output, decision.target.workspace) ?? null;
        } catch (error) {
            void error;
            overlay = null;
        }
        if (overlay !== false) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-${overlay === true ? "target-overlay" : "target-overlay-unknown"} window=${candidate.id}`);
            return fresh;
        }
        const sendTransfer = this.env.sendClientToScreen;
        if (sendTransfer === undefined) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=refused-no-transfer window=${candidate.id}`);
            return fresh;
        }
        const setDesktops = this.env.setDesktops;
        const newcomerRef = candidate.ref;
        const observedOutput = fresh.domainOutput;
        let mutated = false;
        const outcome = applyNewcomerOutputRoute(
            {
                resolveOutput: (name) => this.env.resolveOutput?.(name) ?? null,
                resolveDesktop: (workspace) => this.env.resolveDesktop?.(workspace) ?? null,
                sendClientToScreen: (mover, output) => {
                    mutated = true;
                    return sendTransfer(mover, output);
                },
                setDesktops:
                    setDesktops === undefined
                        ? undefined
                        : (mover, desktops) => {
                              mutated = true;
                              return setDesktops(mover, desktops);
                          },
                readOutputName: (ref) => this.env.readOutputName?.(ref) ?? null,
                readDesktopIds: (ref) => this.env.readDesktopIds?.(ref) ?? null,
                setActive: (target) => this.env.setActive(target),
                reobserve: (ref) => {
                    const next = this.freshObserved();
                    if (next === null) {
                        return null;
                    }
                    for (const entry of next.windows) {
                        if (entry.ref === ref) {
                            // Post-move safety recheck before any focus write:
                            // transient state plus floating/sticky/fixed
                            // flags, from the fresh observation and the
                            // synchronous hint read.
                            let moved: boolean | null = null;
                            try {
                                moved = this.env.readIsTransient?.(ref) ?? null;
                            } catch (error) {
                                void error;
                                moved = null;
                            }
                            if (moved !== false) {
                                return null;
                            }
                            if (entry.floating === true || entry.sticky === true) {
                                return null;
                            }
                            const movedHints = this.hintSizesFor(ref);
                            if (isFixedSize(movedHints.minSize, movedHints.maxSize, this.readFixedSizePredicate())) {
                                return null;
                            }
                            return { output: entry.output, workspace: entry.workspace, fullscreen: entry.fullscreen, maximized: entry.maximized };
                        }
                    }
                    return null;
                },
            },
            newcomerRef,
            observedOutput,
            decision.target,
        );
        if (outcome === "routed" || outcome === "routed-focus-failed") {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=${outcome} window=${candidate.id}`);
            return this.freshObserved();
        }
        if (!mutated) {
            this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=${outcome} window=${candidate.id}`);
            return fresh;
        }
        // A write landed but the sequence failed: the pre-write observation
        // is stale ground. Continue only on re-observed truth, else abort.
        this.logToken(`${LOG_PREFIX}:newcomer-route epoch=${this.epoch} outcome=${outcome}-mutated window=${candidate.id}`);
        return this.freshObserved();
    }

    // R-MAX-03 recorder for the workspace-floating skips: every id in an
    // untiled-domain observation keeps its overlay on later tiled-domain
    // admission. Bounded, silent, never dispatches.
    private noteFloatingFirstSighted(observed: PlanObserved): void {
        try {
            for (const entry of observed.windows) {
                const id = entry.id;
                if (typeof id !== "string" || id.length === 0 || this.floatingFirstSeen.has(id)) {
                    continue;
                }
                if (this.floatingFirstSeen.size >= 4096) {
                    const oldest = this.floatingFirstSeen.values().next();
                    if (oldest.done) {
                        return;
                    }
                    this.floatingFirstSeen.delete(oldest.value);
                }
                this.floatingFirstSeen.add(id);
            }
        } catch (error) {
            void error;
        }
    }

    requestResize(direction: unknown, mode: unknown): void {
        if (!this.enabled) {
            this.logToken(`${LOG_PREFIX}:resize-refused-disabled`);
            return;
        }
        if (!isDirection(direction)) {
            this.logToken(`${LOG_PREFIX}:resize-refused-invalid-direction`);
            return;
        }
        if (!isResizeMode(mode)) {
            this.logToken(`${LOG_PREFIX}:resize-refused-invalid-mode`);
            return;
        }
        if (this.r4Flight !== null) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=resize`);
            return;
        }
        if (this.inFlight) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=resize`);
            return;
        }
        const observed = this.freshObserved();
        if (observed === null) {
            this.logToken(`${LOG_PREFIX}:resize-refused-observe`);
            return;
        }
        if (observed.activeExcluded) {
            this.logToken(`${LOG_PREFIX}:resize-refused-floating`);
            return;
        }
        if (!this.isTiledDomain(observed.domainOutput, observed.domainWorkspace)) {
            this.logToken(`${LOG_PREFIX}:resize-refused-workspace-floating`);
            return;
        }
        if (this.windowIsFullscreen(observed, observed.focusedId)) {
            this.logToken(`${LOG_PREFIX}:resize-refused-fullscreen`);
            return;
        }
        if (this.windowIsMaximized(observed, observed.focusedId)) {
            this.logToken(`${LOG_PREFIX}:resize-refused-maximize`);
            return;
        }
        const snapshot = this.carriedSnapshot(observed);
        this.noteObservation(snapshot.fingerprint);
        let pressIndex = 0;
        if (
            this.repeatFocused === snapshot.focusedId &&
            this.repeatDirection === direction &&
            this.repeatMode === mode
        ) {
            pressIndex = this.repeatNext;
        }
        this.repeatFocused = snapshot.focusedId;
        this.repeatDirection = direction;
        this.repeatMode = mode;
        this.repeatNext = pressIndex + 1;
        this.dispatch({
            op: "resize",
            snapshot,
            removed: null,
            body: { op: "resize", window: snapshot.focusedId, direction, mode, press_index: pressIndex },
        });
    }

    // R-LAY-01 toggle-orientation: flip the focused leaf's immediate parent
    // axis (including root), preserving order/shares/focus. Lone root leaf
    // is a Rust no-op (`unchanged`); floating subjects refuse as `not-tiled`.
    // Pre-dispatch fences mirror requestResize exactly (floating focus,
    // floating workspace, focused fullscreen/maximized overlay) plus the
    // focused target must be tiled. Sibling overlays stay dispatchable: the
    // carried snapshot preserves their slots and apply skips their native
    // writes, so the retained overlay slot is never disturbed.
    requestToggleOrientation(): void {
        if (!this.enabled) {
            this.logToken(`${LOG_PREFIX}:toggle-orient-refused-disabled`);
            return;
        }
        if (this.r4Flight !== null) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-orientation`);
            return;
        }
        if (this.inFlight) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-orientation`);
            return;
        }
        const observed = this.freshObserved();
        if (observed === null) {
            this.logToken(`${LOG_PREFIX}:toggle-orient-refused-observe`);
            return;
        }
        if (observed.activeExcluded) {
            this.logToken(`${LOG_PREFIX}:toggle-orient-refused-floating`);
            return;
        }
        if (!this.isTiledDomain(observed.domainOutput, observed.domainWorkspace)) {
            this.logToken(`${LOG_PREFIX}:toggle-orient-refused-workspace-floating`);
            return;
        }
        const target = observed.windows.find((entry) => entry.id === observed.focusedId);
        if (target === undefined) {
            this.logToken(`${LOG_PREFIX}:toggle-orient-refused-observe`);
            return;
        }
        if (target.floating === true) {
            this.logToken(`${LOG_PREFIX}:toggle-orient-refused-floating`);
            return;
        }
        if (this.windowIsFullscreen(observed, observed.focusedId)) {
            this.logToken(`${LOG_PREFIX}:toggle-orient-refused-fullscreen`);
            return;
        }
        if (this.windowIsMaximized(observed, observed.focusedId)) {
            this.logToken(`${LOG_PREFIX}:toggle-orient-refused-maximize`);
            return;
        }
        const snapshot = this.carriedSnapshot(observed);
        this.noteObservation(snapshot.fingerprint);
        this.dispatch({
            op: "toggle-orientation",
            snapshot,
            removed: null,
            body: { op: "toggle-orientation", window: snapshot.focusedId },
        });
    }

    requestFloat(): void {
        if (!this.enabled) {
            this.logToken(`${LOG_PREFIX}:float-refused-disabled`);
            return;
        }
        if (this.r4Flight !== null) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-float`);
            return;
        }
        if (this.inFlight) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-float`);
            return;
        }
        const observed = this.freshObserved();
        if (observed === null) {
            this.logToken(`${LOG_PREFIX}:float-refused-observe`);
            return;
        }
        const target = observed.windows.find((entry) => entry.id === observed.focusedId);
        if (target === undefined) {
            this.logToken(`${LOG_PREFIX}:float-refused-observe`);
            return;
        }
        if (!this.isTiledDomain(observed.domainOutput, observed.domainWorkspace)) {
            this.logToken(`${LOG_PREFIX}:float-refused-workspace-floating`);
            return;
        }
        const resourceClass = isOpaqueId(target.resourceClass) ? target.resourceClass : "unknown";
        if (target.sticky === true) {
            // Option A (2026-09-25): Meta+G on a sticky window always returns
            // to tile. Reuse the existing sticky-off path with a tiled origin
            // so the echo fence, keep-above restore, and tiling re-invoke
            // apply uniformly to prior-tiled, prior-float, and adopted
            // origins. Meta+Shift+G (requestSticky) keeps honoring the tracked
            // origin; no native all-desktops logic is duplicated here.
            // Fullscreen/maximized overlays refuse exactly like requestSticky,
            // before any adoption or native setter.
            if (target.fullscreen) {
                this.logToken(`${LOG_PREFIX}:sticky-refused-fullscreen window=${target.id} resource_class=${resourceClass}`);
                return;
            }
            if (target.maximized) {
                this.logToken(`${LOG_PREFIX}:sticky-refused-maximize window=${target.id} resource_class=${resourceClass}`);
                return;
            }
            if (this.stickyPreviousFloating.get(target.id) === undefined) {
                if (!this.adoptStickyUnknown(target, resourceClass)) {
                    this.logToken(`${LOG_PREFIX}:sticky-refused-untracked window=${target.id} resource_class=${resourceClass}`);
                    return;
                }
            }
            this.issueSticky(target, false, false);
            return;
        }
        const floating = this.isEffectivelyFloating(observed, target.id);
        if (!floating && observed.activeExcluded) {
            this.logToken(`${LOG_PREFIX}:float-refused-not-tiled`);
            return;
        }
        if (!floating && target.fullscreen) {
            this.logToken(`${LOG_PREFIX}:float-refused-fullscreen`);
            return;
        }
        if (!floating && target.maximized) {
            this.logToken(`${LOG_PREFIX}:float-refused-maximize`);
            return;
        }
        // B9 maximized unfloat: one native clear, then refetch and fall
        // through to the common unfloat path below. Narrow failures return
        // before any dispatch with no stuck operation.
        let liveObserved = observed;
        let liveTarget = target;
        if (floating && target.maximized && !target.fullscreen) {
            const windowId = target.id;
            const nativeRef = target.ref;
            this.logToken(`${LOG_PREFIX}:float-unfloat-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=issued`);
            this.maximizeAdmissionEcho = nativeRef;
            this.logToken(`${LOG_PREFIX}:maximize-admission-echo-armed`);
            let outcome: MaximizeClearOutcome = "threw";
            try {
                outcome = this.env.clearMaximize(nativeRef);
            } catch (error) {
                void error;
            }
            this.logToken(`${LOG_PREFIX}:float-unfloat-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=${outcome}`);
            if (this.maximizeAdmissionEcho !== null) {
                this.maximizeAdmissionEcho = null;
                this.logToken(`${LOG_PREFIX}:maximize-admission-echo-cleared-no-signal`);
            }
            if (outcome !== "invoked") {
                this.logToken(`${LOG_PREFIX}:float-refused-maximize-clear window=${windowId} resource_class=${resourceClass} cause=native-write-${outcome} recovery=retry-on-next-press`);
                return;
            }
            let fresh: PlanObserved | null = null;
            try {
                fresh = this.freshObserved();
            } catch (error) {
                void error;
                fresh = null;
            }
            const current = fresh?.windows.find((entry) => entry.id === windowId);
            const settled =
                current !== undefined && current.ref === nativeRef && !current.maximized
                    ? current
                    : undefined;
            if (fresh === null || settled === undefined) {
                const seen =
                    fresh === null
                        ? "observed-missing"
                        : current === undefined || current.ref !== nativeRef
                          ? "observed-absent"
                          : "observed-maximized";
                const cause =
                    fresh === null
                        ? "observe-missing"
                        : current === undefined || current.ref !== nativeRef
                          ? "identity-raced"
                          : "still-maximized";
                this.logToken(`${LOG_PREFIX}:float-unfloat-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=${seen}`);
                this.logToken(`${LOG_PREFIX}:float-refused-maximize-clear window=${windowId} resource_class=${resourceClass} cause=${cause} recovery=retry-on-next-press`);
                return;
            }
            if (settled.fullscreen || fresh.focusedId !== windowId || !this.isEffectivelyFloating(fresh, windowId)) {
                const seen = settled.fullscreen
                    ? "observed-fullscreen"
                    : fresh.focusedId !== windowId
                      ? "observed-focus-raced"
                      : "observed-unfloated";
                const cause = settled.fullscreen
                    ? "fullscreen-raced"
                    : fresh.focusedId !== windowId
                      ? "focus-raced"
                      : "floating-lost";
                this.logToken(`${LOG_PREFIX}:float-unfloat-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=${seen}`);
                this.logToken(`${LOG_PREFIX}:float-refused-maximize-clear window=${windowId} resource_class=${resourceClass} cause=${cause} recovery=retry-on-next-press`);
                return;
            }
            if (!this.isTiledDomain(fresh.domainOutput, fresh.domainWorkspace)) {
                this.logToken(`${LOG_PREFIX}:float-refused-workspace-floating`);
                return;
            }
            if (this.r4Flight !== null || this.inFlight) {
                this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-float`);
                return;
            }
            this.logToken(`${LOG_PREFIX}:float-unfloat-maximize-clear window=${windowId} resource_class=${resourceClass} outcome=observed-cleared`);
            liveObserved = fresh;
            liveTarget = settled;
        }
        const snapshot = this.carriedSnapshot(liveObserved);
        // Float selects the session-retained placement (the request carries no
        // rect, so the session never recomputes a center for a window that has
        // floated before). Unfloat carries the live frame rect so a user
        // moved/resized float is retained for the next float.
        const rect = floating
            ? { x: liveTarget.rect.x, y: liveTarget.rect.y, w: liveTarget.rect.w, h: liveTarget.rect.h }
            : null;
        // Q2 explicit user tile (D3): the unfloat stages a suppress pin
        // for the same live client, committed only when the plan is
        // applied; an intentional float stages a drop of any automatic
        // mark (D6). A refused or failed flight leaves the authoritative
        // record untouched, so classification keeps the proven identity.
        this.dispatch({
            op: "toggle-float",
            snapshot,
            removed: floating ? null : liveTarget.id,
            body: rect === null ? { op: "toggle-float", window: liveTarget.id } : { op: "toggle-float", window: liveTarget.id, float_rect: rect },
            floatTarget: { window: liveTarget.id, floating: !floating },
            fixedStage: { id: liveTarget.id, ref: liveTarget.ref, kind: floating ? "suppress" : null },
        });
    }

    requestMaximize(): void {
        if (!this.enabled) {
            this.logToken(`${LOG_PREFIX}:maximize-refused-disabled`);
            return;
        }
        if (this.r4Flight !== null) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-maximize`);
            return;
        }
        if (this.inFlight) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-maximize`);
            return;
        }
        const observed = this.freshObserved();
        if (observed === null) {
            this.logToken(`${LOG_PREFIX}:maximize-refused-observe`);
            return;
        }
        const target = observed.windows.find((entry) => entry.id === observed.focusedId);
        if (target === undefined) {
            this.logToken(`${LOG_PREFIX}:maximize-refused-observe`);
            return;
        }
        const resourceClass = isOpaqueId(target.resourceClass) ? target.resourceClass : "unknown";
        if (target.fullscreen) {
            this.logToken(`${LOG_PREFIX}:maximize-refused-fullscreen window=${target.id} resource_class=${resourceClass}`);
            return;
        }
        const wanted = !target.maximized;
        this.maximizeToggleEcho = { ref: target.ref, id: target.id, resourceClass };
        this.logToken(`${LOG_PREFIX}:maximize-toggle window=${target.id} resource_class=${resourceClass} target=${wanted ? "maximized" : "restored"} outcome=issued`);
        this.logToken(`${LOG_PREFIX}:maximize-toggle-echo-armed`);
        let outcome: NativeStateWriteOutcome = "threw";
        try {
            outcome = this.env.setMaximize === undefined ? "missing" : this.env.setMaximize(target.ref, wanted);
        } catch (error) {
            void error;
        }
        this.logToken(`${LOG_PREFIX}:maximize-toggle window=${target.id} resource_class=${resourceClass} target=${wanted ? "maximized" : "restored"} outcome=${outcome}`);
        if (outcome !== "invoked") {
            // A missing/throwing native write never retries automatically;
            // a later deliberate identical press retries once.
            this.logToken(`${LOG_PREFIX}:maximize-retry-armed window=${target.id} resource_class=${resourceClass} target=${wanted ? "maximized" : "restored"} cause=native-write-${outcome} recovery=retry-on-next-press`);
        }
        if (this.maximizeToggleEcho !== null) {
            this.maximizeToggleEcho = null;
            this.logToken(`${LOG_PREFIX}:maximize-toggle-echo-cleared-no-signal`);
        }
    }

    requestFullscreen(): void {
        if (!this.enabled) {
            this.logToken(`${LOG_PREFIX}:fullscreen-refused-disabled`);
            return;
        }
        if (this.r4Flight !== null) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-fullscreen`);
            return;
        }
        if (this.inFlight) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-fullscreen`);
            return;
        }
        const observed = this.freshObserved();
        if (observed === null) {
            this.logToken(`${LOG_PREFIX}:fullscreen-refused-observe`);
            return;
        }
        const target = observed.windows.find((entry) => entry.id === observed.focusedId);
        if (target === undefined) {
            this.logToken(`${LOG_PREFIX}:fullscreen-refused-observe`);
            return;
        }
        const resourceClass = isOpaqueId(target.resourceClass) ? target.resourceClass : "unknown";
        // Project-owned toggle only: KWin keeps cover-and-restore. No
        // geometry, move, float, desktop, focus, or topology command is
        // issued; the existing fullScreenChanged observation drives planner
        // isolation on enter/exit without any echo fence here.
        const wanted = !target.fullscreen;
        this.logToken(`${LOG_PREFIX}:fullscreen-toggle window=${target.id} resource_class=${resourceClass} target=${wanted ? "fullscreen" : "restored"} outcome=issued`);
        let outcome: NativeStateWriteOutcome = "threw";
        try {
            outcome = this.env.setFullscreen === undefined ? "missing" : this.env.setFullscreen(target.ref, wanted);
        } catch (error) {
            void error;
        }
        this.logToken(`${LOG_PREFIX}:fullscreen-toggle window=${target.id} resource_class=${resourceClass} target=${wanted ? "fullscreen" : "restored"} outcome=${outcome}`);
    }

    requestSticky(): void {
        if (!this.enabled) {
            this.logToken(`${LOG_PREFIX}:sticky-refused-disabled`);
            return;
        }
        if (this.r4Flight !== null) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-sticky`);
            return;
        }
        if (this.inFlight) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=toggle-sticky`);
            return;
        }
        const observed = this.freshObserved();
        if (observed === null) {
            this.logToken(`${LOG_PREFIX}:sticky-refused-observe`);
            return;
        }
        const target = observed.windows.find((entry) => entry.id === observed.focusedId);
        if (target === undefined) {
            this.logToken(`${LOG_PREFIX}:sticky-refused-observe`);
            return;
        }
        if (!this.isTiledDomain(observed.domainOutput, observed.domainWorkspace)) {
            this.logToken(`${LOG_PREFIX}:sticky-refused-workspace-floating`);
            return;
        }
        const resourceClass = isOpaqueId(target.resourceClass) ? target.resourceClass : "unknown";
        if (target.fullscreen) {
            this.logToken(`${LOG_PREFIX}:sticky-refused-fullscreen window=${target.id} resource_class=${resourceClass}`);
            return;
        }
        if (target.maximized) {
            this.logToken(`${LOG_PREFIX}:sticky-refused-maximize window=${target.id} resource_class=${resourceClass}`);
            return;
        }
        if (target.sticky === true) {
            const previousFloating = this.stickyPreviousFloating.get(target.id);
            if (previousFloating === undefined) {
                if (!this.adoptStickyUnknown(target, resourceClass)) {
                    this.logToken(`${LOG_PREFIX}:sticky-refused-untracked window=${target.id} resource_class=${resourceClass}`);
                    return;
                }
                this.issueSticky(target, false, true);
                return;
            }
            this.issueSticky(target, false, previousFloating);
            return;
        }
        const previousFloating = this.isEffectivelyFloating(observed, target.id);
        this.stickyPreviousFloating.set(target.id, previousFloating);
        if (previousFloating) {
            // Sticky-on of an automatic float adopts it as intentional
            // (D6) with a floating prior so sticky-off restores floating.
            // The mark drops only inside issueSticky on a verified native
            // write; a refused or failed setter keeps the automatic
            // identity for the next classification.
            this.issueSticky(target, true, previousFloating);
            return;
        }
        // Q2 intentional float (D6): sticky-on of a tiled window stages a
        // drop of any automatic mark, committed only when the plan is
        // applied; the sticky float stays intentional either way.
        const snapshot = this.carriedSnapshot(observed);
        this.dispatch({
            op: "toggle-float",
            snapshot,
            removed: target.id,
            body: { op: "toggle-float", window: target.id },
            floatTarget: { window: target.id, floating: true },
            stickyTarget: { window: target.id, previousFloating },
            fixedStage: { id: target.id, ref: target.ref, kind: null },
        });
    }

    // Adopted sticky-float origin for externally sticky windows: an eligible
    // normal window already native-sticky with an empty native desktop list but
    // no same-runtime origin is adopted as a sticky float with prior-float
    // semantics only. No tiled slot is guessed, no history is persisted, and no
    // planner admission is claimed. Returns false when the native membership
    // cannot prove the empty-list sticky shape (fail closed, keep the existing
    // untracked refusal and never broaden unmanaged/non-normal windows).
    private adoptStickyUnknown(target: PlanObservedWindow, resourceClass: string): boolean {
        let ids: ReadonlyArray<string> | null = null;
        try {
            const reader = this.env.readDesktopIds;
            if (typeof reader !== "function") {
                return false;
            }
            try {
                ids = reader(target.ref);
            } catch (error) {
                void error;
                ids = null;
            }
            if (ids === null || ids.length !== 0) {
                return false;
            }
        } catch (error) {
            void error;
            return false;
        }
        this.stickyPreviousFloating.set(target.id, true);
        this.adoptedSticky.add(target.id);
        this.logToken(`${LOG_PREFIX}:sticky-adopted window=${target.id} resource_class=${resourceClass} origin=unknown-float`);
        return true;
    }

    // Bounded sticky focus retention: the exact toggled window stays
    // active across the native all-desktops write. Single synchronous
    // setActive only when the fresh observation still contains the toggled
    // id; stale ids never actuate and a failed write only logs. No timer,
    // retry, desktop switch, or later-focus fighting.
    private retainStickyFocus(target: PlanObservedWindow): boolean {
        const resourceClass = isOpaqueId(target.resourceClass) ? target.resourceClass : "unknown";
        let resolvedRef: object | undefined = target.ref;
        try {
            const fresh = this.freshObserved();
            if (fresh !== null) {
                let found: object | undefined;
                for (const entry of fresh.windows) {
                    if (entry.id === target.id) {
                        found = entry.ref;
                        break;
                    }
                }
                if (found === undefined) {
                    this.logToken(`${LOG_PREFIX}:sticky-focus-stale window=${target.id} resource_class=${resourceClass}`);
                    return false;
                }
                resolvedRef = found;
            }
        } catch (error) {
            void error;
        }
        if (resolvedRef === undefined) {
            this.logToken(`${LOG_PREFIX}:sticky-focus-stale window=${target.id} resource_class=${resourceClass}`);
            return false;
        }
        let currentActive: object | null = null;
        try {
            currentActive = this.env.active();
        } catch (error) {
            void error;
            currentActive = null;
        }
        if (currentActive === resolvedRef) {
            this.logToken(`${LOG_PREFIX}:sticky-focus-retained window=${target.id} resource_class=${resourceClass}`);
            return true;
        }
        let focused = false;
        try {
            focused = this.env.setActive(resolvedRef) === true;
        } catch (error) {
            void error;
            focused = false;
        }
        if (!focused) {
            this.logToken(`${LOG_PREFIX}:sticky-focus-failed window=${target.id} resource_class=${resourceClass}`);
            return false;
        }
        this.logToken(`${LOG_PREFIX}:sticky-focus-retained window=${target.id} resource_class=${resourceClass}`);
        return true;
    }

    // Bounded float focus retention for toggle-float flights: the exact
    // toggled window stays active on entry and exit. Rust desired_focus is
    // tiled bookkeeping for survivors and must never natively activate a
    // sibling. Single synchronous setActive from the fresh observation only;
    // stale ids fail closed and failed writes fail the flight. No timer,
    // retry, desktop switch, MRU, or later-focus fighting.
    private retainFloatFocus(windowId: string, targetRef: object | undefined, resourceClass: string): boolean {
        if (targetRef === undefined) {
            this.logToken(`${LOG_PREFIX}:float-focus-stale window=${windowId} resource_class=${resourceClass}`);
            return false;
        }
        let currentActive: object | null = null;
        try {
            currentActive = this.env.active();
        } catch (error) {
            void error;
            currentActive = null;
        }
        if (currentActive === targetRef) {
            this.logToken(`${LOG_PREFIX}:float-focus-retained window=${windowId} resource_class=${resourceClass}`);
            return true;
        }
        let focused = false;
        try {
            focused = this.env.setActive(targetRef) === true;
        } catch (error) {
            void error;
            focused = false;
        }
        if (!focused) {
            this.logToken(`${LOG_PREFIX}:float-focus-failed window=${windowId} resource_class=${resourceClass}`);
            return false;
        }
        this.logToken(`${LOG_PREFIX}:float-focus-retained window=${windowId} resource_class=${resourceClass}`);
        return true;
    }

    private issueSticky(target: PlanObservedWindow, allDesktops: boolean, previousFloating: boolean): void {
        const resourceClass = isOpaqueId(target.resourceClass) ? target.resourceClass : "unknown";
        if (allDesktops && !this.ensureKeepAbove(target.ref, target.id, resourceClass)) {
            return;
        }
        // Capture adoption before the native write: a synchronous echo (test
        // fireDesktopsSync) consumes the adopted marker and persists ordinary
        // float before the tile block below runs.
        const wasAdoptedBefore = this.adoptedSticky.has(target.id);
        this.stickyEcho = { ref: target.ref, id: target.id, resourceClass, allDesktops, previousFloating };
        this.logToken(`${LOG_PREFIX}:sticky-toggle window=${target.id} resource_class=${resourceClass} target=${allDesktops ? "all-desktops" : "current-desktop"} outcome=issued`);
        this.logToken(`${LOG_PREFIX}:sticky-echo-armed`);
        let outcome: NativeStateWriteOutcome = "threw";
        try {
            outcome = this.env.setAllDesktops === undefined ? "missing" : this.env.setAllDesktops(target.ref, allDesktops);
        } catch (error) {
            void error;
        }
        this.logToken(`${LOG_PREFIX}:sticky-toggle window=${target.id} resource_class=${resourceClass} target=${allDesktops ? "all-desktops" : "current-desktop"} outcome=${outcome}`);
        if (outcome !== "invoked") {
            // A false/missing/throwing native assignment never retries,
            // replays, or fabricates success. Drop a stale
            // sticky-on claim (the window never became sticky) while keeping a
            // failed sticky-off origin for a later explicit retry.
            if (allDesktops) {
                this.stickyPreviousFloating.delete(target.id);
                this.adoptedSticky.delete(target.id);
            }
        } else {
            // A failed write owns no focus change: retain the toggled window
            // only when the native assignment actually landed.
            this.retainStickyFocus(target);
            // Q2 sticky intent (D3/D6): sticky-on adopts any automatic
            // float as intentional; sticky-off-to-tile records an
            // explicit tile win for the same live client. Sticky-off
            // with a floating prior stays floating without an override.
            // D7 durable tile moves only for fixed overrides (fixed now,
            // durable identity, or fixed-auto origin); ordinary sticky-off
            // still records the in-memory win but persists no tile.
            const priorAutoForTile =
                this.fixedClients.get(target.id) !== undefined &&
                this.fixedClients.get(target.id)?.kind === "auto" &&
                this.fixedClients.get(target.id)?.ref === target.ref;
            const hadTileForSticky = this.intentTileMembers.has(target.id);
            this.dropAutomaticFixed(target.id);
            if (!allDesktops && !previousFloating) {
                this.noteFixedTileOverride(target.id, target.ref);
                if (this.intentActive && !wasAdoptedBefore) {
                    let fixedNowForSticky = false;
                    try {
                        const hints = this.hintSizesFor(target.ref);
                        fixedNowForSticky = isFixedSize(hints.minSize, hints.maxSize, this.readFixedSizePredicate());
                    } catch (error) {
                        void error;
                    }
                    if (fixedNowForSticky || hadTileForSticky || priorAutoForTile) {
                        this.intentMembers.delete(target.id);
                        this.intentTileMembers.add(target.id);
                        this.scheduleIntentWrite();
                    }
                }
            } else if (allDesktops && this.intentActive) {
                if (this.intentTileMembers.delete(target.id)) {
                    this.scheduleIntentWrite();
                }
            }
        }
        if (this.stickyEcho !== null) {
            this.stickyEcho = null;
            this.logToken(`${LOG_PREFIX}:sticky-echo-cleared-no-signal`);
        }
    }

    // D01 native activation ownership: automatic convergence (reconcile /
    // update-gaps admission) never actuates focus while a fullscreen or
    // maximized overlay is involved. Domain fullscreen/maximized presence
    // is involvement, even when the reply target itself is an ordinary
    // sibling: the host owns activation, denial, and explicit switching.
    // Explicit op focus flights are exempt, as are CLOSE-05 survivor
    // refocuses once the overlay is gone from the fresh observation.
    // The foreground observation is re-read just before the setter (after
    // geometry writes): a sibling may have entered fullscreen mid-apply,
    // or native focus may have moved to another output. An unavailable
    // or domain-changed re-read suppresses the stale automatic write
    // fail-closed, as does a live active() that no longer matches the
    // re-read. Read-only: the existing foreground observer plus the
    // existing active() read, never a new setter or env hook.
    private automaticFocusGuard(
        flightState: PendingFlight,
    ): { readonly reason: string | null; readonly active: object | null } {
        const readActive = (): object | null => {
            try {
                return this.env.active();
            } catch (error) {
                void error;
                return null;
            }
        };
        if (flightState.op !== "reconcile" && flightState.op !== "update-gaps") {
            return { reason: null, active: readActive() };
        }
        let fresh: PlanObserved | null = null;
        try {
            fresh = this.env.observe();
        } catch (error) {
            void error;
            fresh = null;
        }
        if (fresh === null) {
            return { reason: "observe-unavailable", active: null };
        }
        if (
            fresh.domainOutput !== flightState.snapshot.domainOutput ||
            fresh.domainWorkspace !== flightState.snapshot.domainWorkspace
        ) {
            return { reason: "domain-changed", active: null };
        }
        const snapshotFlags = new Map<string, { readonly fullscreen: boolean; readonly maximized: boolean }>();
        for (const entry of flightState.snapshot.windows) {
            if (!snapshotFlags.has(entry.id)) {
                snapshotFlags.set(entry.id, { fullscreen: entry.fullscreen, maximized: entry.maximized });
            }
        }
        let maximized = false;
        for (const entry of fresh.windows) {
            const snap = snapshotFlags.get(entry.id);
            if (entry.fullscreen === true || snap?.fullscreen === true) {
                return { reason: "fullscreen-active", active: null };
            }
            if (entry.maximized === true || snap?.maximized === true) {
                maximized = true;
            }
        }
        if (maximized) {
            return { reason: "maximize-active", active: null };
        }
        const active = readActive();
        if (active !== fresh.activeRef) {
            // A live active window that is still observed is a newer
            // native decision: the re-read is already stale, so the
            // automatic write is dropped. A stale ref to a removed or
            // unknown window is benign: CLOSE-05 survivor refocus still
            // actuates below.
            const stillObserved =
                active !== null && fresh.windows.some((entry) => entry.ref === active);
            if (stillObserved) {
                return { reason: "active-changed", active };
            }
        }
        return { reason: null, active };
    }

    private ensureKeepAbove(target: object, id: string, resourceClass: string): boolean {
        let above: boolean | null = null;
        let below: boolean | null = null;
        try {
            above = this.env.readKeepAbove === undefined ? null : this.env.readKeepAbove(target);
            below = this.env.readKeepBelow === undefined ? null : this.env.readKeepBelow(target);
        } catch (error) {
            void error;
        }
        if (above === null || below === null) {
            this.logToken(`${LOG_PREFIX}:keep-above window=${id} resource_class=${resourceClass} target=above outcome=missing`);
            return false;
        }
        const prior = this.keepAbovePrevious.get(id);
        if (prior === undefined) {
            this.keepAbovePrevious.set(id, { ref: target, above, below });
        }
        if (above) {
            return true;
        }
        let outcome: KeepAboveWriteOutcome = "threw";
        try {
            outcome = this.env.setKeepAbove === undefined ? "missing" : this.env.setKeepAbove(target, true);
        } catch (error) {
            void error;
        }
        this.logToken(`${LOG_PREFIX}:keep-above window=${id} resource_class=${resourceClass} target=above outcome=${outcome}`);
        if (outcome !== "invoked") {
            if (prior === undefined) {
                this.keepAbovePrevious.delete(id);
            }
            return false;
        }
        return true;
    }

    private restoreKeepAbove(id: string, resourceClass: string): boolean {
        const prior = this.keepAbovePrevious.get(id);
        if (prior === undefined) {
            return true;
        }
        let above: boolean | null = null;
        let below: boolean | null = null;
        try {
            above = this.env.readKeepAbove === undefined ? null : this.env.readKeepAbove(prior.ref);
            below = this.env.readKeepBelow === undefined ? null : this.env.readKeepBelow(prior.ref);
        } catch (error) {
            void error;
        }
        if (above === null || below === null) {
            this.logToken(`${LOG_PREFIX}:keep-above window=${id} resource_class=${resourceClass} target=restore outcome=missing`);
            return false;
        }
        if (prior.above) {
            this.keepAbovePrevious.delete(id);
            return true;
        }
        if (prior.below) {
            if (below && !above) {
                this.keepAbovePrevious.delete(id);
                return true;
            }
            let outcome: KeepAboveWriteOutcome = "threw";
            try {
                outcome = this.env.setKeepBelow === undefined ? "missing" : this.env.setKeepBelow(prior.ref, true);
            } catch (error) {
                void error;
            }
            this.logToken(`${LOG_PREFIX}:keep-above window=${id} resource_class=${resourceClass} target=restore-below outcome=${outcome}`);
            if (outcome !== "invoked") {
                return false;
            }
            this.keepAbovePrevious.delete(id);
            return true;
        }
        if (!above) {
            this.keepAbovePrevious.delete(id);
            return true;
        }
        let outcome: KeepAboveWriteOutcome = "threw";
        try {
            outcome = this.env.setKeepAbove === undefined ? "missing" : this.env.setKeepAbove(prior.ref, false);
        } catch (error) {
            void error;
        }
        this.logToken(`${LOG_PREFIX}:keep-above window=${id} resource_class=${resourceClass} target=restore outcome=${outcome}`);
        if (outcome !== "invoked") {
            return false;
        }
        this.keepAbovePrevious.delete(id);
        return true;
    }

    private restoreAllKeepAbove(): void {
        for (const [id] of this.keepAbovePrevious) {
            this.restoreKeepAbove(id, "unknown");
        }
    }

    // Oracle route: one pointer-resize intent from the authoritative final
    // rect, single-axis or atomic corner (dual-axis). Strict decoding only;
    // fail-closed false when the window, direction(s), or boundary(ies)
    // cannot be safely bound. A corner carries both axes in exactly one
    // intent through the single pending slot (never two concurrent
    // requests); single-axis wire shape is unchanged. Defers through the
    // single pending slot when a flight is active, never bypasses it,
    // retries, or guesses.
    // Drop-intent callers pass their drag-N correlation as optional entry
    // metadata (6th arg). A drag-correlated refusal or terminal failure
    // feeds the coalesced per-domain restore marker, which converges once
    // through a single existing-route reconcile (or through any superseding
    // applied plan for the same domain). Calls without a drag correlation
    // behave exactly as before (no marker, no follow-up).
    requestPointerResize(windowId: unknown, direction: unknown, boundary: unknown, direction2?: unknown, boundary2?: unknown, dragCorrelation?: unknown): boolean {
        const drag = isDragCorrelation(dragCorrelation) ? (dragCorrelation as string) : null;
        const refuseDrag = (reason: string, output?: string, workspace?: string): false => {
            if (drag !== null) {
                this.noteDragRejected(drag, reason, typeof windowId === "string" ? windowId : null, output, workspace);
            }
            return false;
        };
        if (!this.enabled) {
            this.logToken(`${LOG_PREFIX}:pointer-refused-disabled`);
            return refuseDrag("disabled");
        }
        if (!isOpaqueId(windowId)) {
            this.logToken(`${LOG_PREFIX}:pointer-refused-identity`);
            return refuseDrag("identity");
        }
        if (!isDirection(direction)) {
            this.logToken(`${LOG_PREFIX}:pointer-refused-direction`);
            return refuseDrag("direction");
        }
        if (!isFiniteInt(boundary) || (boundary as number) < -16384 || (boundary as number) > 16384) {
            this.logToken(`${LOG_PREFIX}:pointer-refused-boundary`);
            return refuseDrag("boundary");
        }
        // Corner second axis is both-or-neither; a half-present pair, an
        // unparsable second direction, or a same-axis pair binds the exact
        // direction refusal, and an out-of-range second boundary binds the
        // exact boundary refusal. Single-axis calls never reach this block.
        const corner = direction2 !== undefined || boundary2 !== undefined;
        if (corner) {
            if (!isDirection(direction2)) {
                this.logToken(`${LOG_PREFIX}:pointer-refused-direction`);
                return refuseDrag("direction");
            }
            if (!isFiniteInt(boundary2) || (boundary2 as number) < -16384 || (boundary2 as number) > 16384) {
                this.logToken(`${LOG_PREFIX}:pointer-refused-boundary`);
                return refuseDrag("boundary");
            }
            const horizontal = (value: string): boolean => value === "left" || value === "right";
            if (horizontal(direction as string) === horizontal(direction2 as string)) {
                this.logToken(`${LOG_PREFIX}:pointer-refused-direction`);
                return refuseDrag("direction");
            }
        }
        if (this.r4Flight !== null) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=pointer-resize`);
            return refuseDrag("busy");
        }
        const observed = this.freshObserved();
        if (observed === null) {
            this.logToken(`${LOG_PREFIX}:pointer-refused-observe`);
            return refuseDrag("observe");
        }
        let found = false;
        for (const entry of observed.windows) {
            if (entry.id === (windowId as string)) {
                found = true;
                break;
            }
        }
        if (!found) {
            this.logToken(`${LOG_PREFIX}:pointer-refused-absent`);
            return refuseDrag("absent", observed.domainOutput, observed.domainWorkspace);
        }
        if (this.windowIsFullscreen(observed, windowId as string)) {
            this.logToken(`${LOG_PREFIX}:pointer-refused-fullscreen`);
            return refuseDrag("fullscreen", observed.domainOutput, observed.domainWorkspace);
        }
        if (this.windowIsMaximized(observed, windowId as string)) {
            this.logToken(`${LOG_PREFIX}:pointer-refused-maximize`);
            return refuseDrag("maximize", observed.domainOutput, observed.domainWorkspace);
        }
        const snapshot = this.carriedSnapshot(observed);
        this.noteObservation(snapshot.fingerprint);
        const intent: AutoIntent = {
            op: "pointer-resize",
            snapshot,
            removed: null,
            body: corner
                ? { op: "pointer-resize", window: windowId as string, direction, boundary, direction2, boundary2 }
                : { op: "pointer-resize", window: windowId as string, direction, boundary },
            pointerSource: windowId as string,
            ...(drag !== null ? { dragSource: drag } : {}),
        };
        // A final-geometry pointer route is selected ahead of the ordinary
        // finish resync. Do not let that resync restore the old split first.
        // A deferred drag pointer superseded here never dispatched: fold its
        // drop into the restore marker instead of losing it. The marker (not
        // the slot) owns convergence, so ordinary slot clearing below is
        // unchanged.
        this.clearDebounce();
        this.absorbDeferredDragIntent(this.deferredAuto, false);
        if (this.deferredAuto?.op === "reconcile" && this.deferredAuto.workAreaReprojection !== true) {
            this.deferredAuto = null;
        }
        this.discardInteractiveReconcile();
        if (this.inFlight) {
            this.deferredAuto = intent;
            return true;
        }
        this.dispatch(intent);
        const flight = this.pending;
        const ours =
            flight !== null &&
            flight.op === "pointer-resize" &&
            flight.pointerSource === (windowId as string) &&
            (drag === null ? flight.dragSource == null : this.dragSourceOf(flight) === drag);
        if (!ours) {
            // Dispatch never installed our pointer flight (R4 race,
            // interactive guard, or synchronous transport failure): the
            // dispatch failure paths already fed the marker when they ran
            // (deduped below), so just report refusal. A synchronous failure
            // that already dispatched the marker must not report accepted.
            // A deferred pointer would have returned true above, so this is
            // not the deferral path.
            if (drag !== null) {
                this.noteDragRejected(drag, "dispatch-failed", typeof windowId === "string" ? windowId : null, snapshot.domainOutput, snapshot.domainWorkspace);
            }
            return false;
        }
        return true;
    }

    // Pointer-domain observation for preview (read-only) and drop (dispatch):
    // domain under the pointer, not the mover's native domain. A single
    // hidden domain containing the pointer supplies the destination with the
    // mover projected there for planning only (exact mover ref, no native
    // assignment). Ambiguous/unreadable hidden evidence fails closed as
    // `outside`; zero matches fall back to the source observation (preview
    // refuses, drop dispatches from source). Started source binding is parsed
    // here; cross/output authority stays with the callers. Emits no logs.
    private buildPointerDomainObservation(
        windowId: string,
        px: number,
        py: number,
        sourceDomain: unknown,
    ):
        | {
              ok: true;
              sourceObserved: PlanObserved;
              effectiveObserved: PlanObserved;
              mover: PlanObservedWindow;
              explicitSource: { output: string; workspace: string } | null;
              pointerInSource: boolean;
              projected: boolean;
          }
        | { ok: false; reason: "observe" | "absent" | "outside"; output?: string; workspace?: string } {
        const fresh = this.freshObserved();
        if (fresh === null) {
            return { ok: false, reason: "observe" };
        }
        let sourceObserved: PlanObserved = fresh;
        let mover: PlanObservedWindow | undefined = fresh.windows.find((entry) => entry.id === windowId);
        // Native output tracks the dragged frame mid-drag, so the mover may
        // already live on the destination while the foreground still shows
        // the source while another window remains active.
        // Restore that source identity from exactly one hidden domain holding
        // the mover; zero or several stays fail-closed absent.
        let hiddenCache: ReadonlyArray<PlanObserved> | null = null;
        if (mover === undefined) {
            try {
                hiddenCache = this.env.observeHidden?.() ?? null;
            } catch (error) {
                void error;
                hiddenCache = null;
            }
            if (!Array.isArray(hiddenCache)) {
                return { ok: false, reason: "absent", output: fresh.domainOutput, workspace: fresh.domainWorkspace };
            }
            const holders = hiddenCache.filter(
                (candidate) =>
                    typeof candidate === "object" &&
                    candidate !== null &&
                    Array.isArray((candidate as PlanObserved).windows) &&
                    (candidate as PlanObserved).windows.some((entry) => entry.id === windowId),
            );
            if (holders.length !== 1) {
                return { ok: false, reason: "absent", output: fresh.domainOutput, workspace: fresh.domainWorkspace };
            }
            sourceObserved = holders[0] as PlanObserved;
            mover = sourceObserved.windows.find((entry) => entry.id === windowId);
            if (mover === undefined) {
                return { ok: false, reason: "absent", output: fresh.domainOutput, workspace: fresh.domainWorkspace };
            }
        }
        // Started binding is closed-vocabulary; parsing never throws.
        const srcRecord = isRecord(sourceDomain) ? sourceDomain : null;
        const srcOutput = srcRecord !== null ? srcRecord["output"] : undefined;
        const srcWorkspace = srcRecord !== null ? srcRecord["workspace"] : undefined;
        const explicitSource =
            isOpaqueId(srcOutput) && isOpaqueId(srcWorkspace) ? { output: srcOutput, workspace: srcWorkspace } : null;
        const outside = { ok: false as const, reason: "outside" as const, output: sourceObserved.domainOutput, workspace: sourceObserved.domainWorkspace };
        const inBounds = (bounds: PlanRect): boolean =>
            px >= bounds.x && py >= bounds.y && px < bounds.x + bounds.w && py < bounds.y + bounds.h;
        if (isTargetRect(sourceObserved.domainBounds) && inBounds(sourceObserved.domainBounds)) {
            return { ok: true, sourceObserved, effectiveObserved: sourceObserved, mover, explicitSource, pointerInSource: true, projected: false };
        }
        // One hidden read (reused when the mover was restored from hidden):
        // throw/non-array is unreadable (fail closed).
        let hidden: ReadonlyArray<PlanObserved> | null = hiddenCache;
        if (hidden === null) {
            try {
                hidden = this.env.observeHidden?.() ?? null;
            } catch (error) {
                void error;
            }
        }
        if (!Array.isArray(hidden)) {
            return outside;
        }
        const matches = hidden.filter(
            (candidate) =>
                typeof candidate === "object" &&
                candidate !== null &&
                isTargetRect(candidate.domainBounds) &&
                inBounds(candidate.domainBounds),
        );
        // The foreground is a valid pointer destination when the mover was
        // restored from hidden (source lives on hidden, pointer back over
        // the foreground domain).
        let foregroundMatch = false;
        if (sourceObserved !== fresh && isTargetRect(fresh.domainBounds) && inBounds(fresh.domainBounds)) {
            foregroundMatch = true;
        }
        const totalMatches = matches.length + (foregroundMatch ? 1 : 0);
        if (totalMatches > 1) {
            return outside;
        }
        if (totalMatches === 0) {
            return { ok: true, sourceObserved, effectiveObserved: sourceObserved, mover, explicitSource, pointerInSource: false, projected: false };
        }
        const dest = (foregroundMatch ? fresh : matches[0]) as PlanObserved;
        if (dest.windows.some((entry) => entry.id === windowId)) {
            return { ok: true, sourceObserved, effectiveObserved: dest, mover, explicitSource, pointerInSource: false, projected: false };
        }
        // Project the source-native mover for planning only (exact mover ref
        // keeps size hints; no native assignment).
        const projectedWindow: PlanObservedWindow = {
            ...mover,
            rect: { ...mover.rect },
            output: dest.domainOutput,
            workspace: dest.domainWorkspace,
        };
        const combined = [...dest.windows, projectedWindow];
        const fp = String(planFingerprint(dest.domainOutput, dest.domainWorkspace, mover.id, combined.map((entry) => entry.id).sort()));
        const effective: PlanObserved = {
            domainOutput: dest.domainOutput,
            domainWorkspace: dest.domainWorkspace,
            domainBounds: dest.domainBounds,
            domainGap: dest.domainGap,
            domainOuterGap: dest.domainOuterGap,
            focusedId: mover.id,
            windows: combined,
            activeRef: mover.ref,
            fingerprint: fp,
            revalidate: () => false,
        };
        return { ok: true, sourceObserved, effectiveObserved: effective, mover, explicitSource, pointerInSource: false, projected: true };
    }

    // Oracle route: one drag-drop intent from the finish-captured script
    // pointer (workspace.cursorPos at FINISH, carried in the finish context,
    // never read at reply) plus the verdict window identity. Strict decoding
    // only; fail-closed false when the window or pointer cannot be safely
    // bound. Defers through the single pending slot when a flight is active,
    // never bypasses it, retries, or guesses. Focus binds to the observed
    // mover (mirroring the preview) so an ordinary Meta+drag without native
    // focus agrees with the preview; refusals converge through the marker
    // like any refusal.
    // Drop-intent callers pass their drag-N correlation as optional entry
    // metadata (4th arg) plus the Started source domain binding (5th arg).
    // A drag-correlated refusal or terminal failure feeds
    // the coalesced per-domain restore marker, which converges once through
    // a single existing-route reconcile (or through any superseding applied
    // plan for the same domain). Calls without a drag correlation behave
    // exactly like pointer-resize without one (no marker, no follow-up).
    // The optional 6th arg carries the entry's local preview correlation
    // (`drag-<Started epoch>`, validated like any drag-N token): the native
    // verdict correlation is only known at Finish, so stepped previews run
    // under the local key. When present, its last validated `hover_prior`
    // rides the drop verbatim (preview key first, then drag key) and the
    // preview live slot is consumed so late preview replies fence out.
    requestDragDrop(windowId: unknown, x: unknown, y: unknown, dragCorrelation?: unknown, sourceDomain?: unknown, previewCorrelation?: unknown): boolean {
        const drag = isDragCorrelation(dragCorrelation) ? (dragCorrelation as string) : null;
        const previewKey = isDragCorrelation(previewCorrelation) ? (previewCorrelation as string) : null;
        const refuseDrag = (reason: string, output?: string, workspace?: string): false => {
            if (previewKey !== null) {
                this.dragPreview.delete(previewKey);
            }
            if (drag !== null) {
                this.noteDragRejected(drag, reason, typeof windowId === "string" ? windowId : null, output, workspace);
            }
            return false;
        };
        if (!this.enabled) {
            this.logToken(`${LOG_PREFIX}:drag-drop-refused-disabled`);
            return refuseDrag("disabled");
        }
        if (!isOpaqueId(windowId)) {
            this.logToken(`${LOG_PREFIX}:drag-drop-refused-identity`);
            return refuseDrag("identity");
        }
        if (!isFiniteInt(x) || !isFiniteInt(y) || (x as number) < -16384 || (x as number) > 16384 || (y as number) < -16384 || (y as number) > 16384) {
            this.logToken(`${LOG_PREFIX}:drag-drop-refused-coords`);
            return refuseDrag("coords");
        }
        if (this.r4Flight !== null) {
            this.logToken(`${LOG_PREFIX}:busy-refused kind=drag-drop`);
            return refuseDrag("busy");
        }
        const pointer = this.buildPointerDomainObservation(windowId as string, x as number, y as number, sourceDomain);
        if (pointer.ok === false) {
            this.logToken(`${LOG_PREFIX}:drag-drop-refused-${pointer.reason}`);
            return refuseDrag(pointer.reason, pointer.output, pointer.workspace);
        }
        const observed = pointer.sourceObserved;
        const effectiveObserved = pointer.effectiveObserved;
        const target = pointer.mover;
        const explicitSource = pointer.explicitSource;
        if (target.fullscreen) {
            this.logToken(`${LOG_PREFIX}:drag-drop-refused-fullscreen`);
            return refuseDrag("fullscreen", observed.domainOutput, observed.domainWorkspace);
        }
        if (target.maximized) {
            this.logToken(`${LOG_PREFIX}:drag-drop-refused-maximize`);
            return refuseDrag("maximize", observed.domainOutput, observed.domainWorkspace);
        }
        if (target.floating === true || target.sticky === true) {
            this.logToken(`${LOG_PREFIX}:drag-drop-refused-floating`);
            return refuseDrag("floating", observed.domainOutput, observed.domainWorkspace);
        }
        // Started binding is authoritative; retained evidence is for direct callers only.
        // Same-output native workspace drift means the window was sent mid-drag:
        // refuse and let send plus reflow win. Output moves stay cross-output.
        // The pointer destination below still routes unchanged natives.
        let crossSource: { output: string; workspace: string } | null = null;
        if (explicitSource !== null) {
            if (observed.domainOutput === explicitSource.output && observed.domainWorkspace !== explicitSource.workspace) {
                this.logToken(`${LOG_PREFIX}:drag-drop-refused-stale-workspace`);
                return refuseDrag("stale-workspace", observed.domainOutput, observed.domainWorkspace);
            }
            if (
                effectiveObserved.domainOutput !== explicitSource.output ||
                effectiveObserved.domainWorkspace !== explicitSource.workspace
            ) {
                // Destination dispatch; failures converge through the destination marker.
                crossSource = { output: explicitSource.output, workspace: explicitSource.workspace };
                if (drag !== null) {
                    this.logToken(
                        `${LOG_PREFIX}:drag-drop-cross-output correlation=${drag} source=cross-output dest=destination`,
                    );
                }
            }
        } else {
            // Direct caller without a Started binding: a pointer-domain
            // destination differing from the native source stays fail-closed
            // (no authority to join destination tiling).
            if (
                effectiveObserved.domainOutput !== observed.domainOutput ||
                effectiveObserved.domainWorkspace !== observed.domainWorkspace
            ) {
                this.logToken(`${LOG_PREFIX}:drag-drop-refused-cross-domain`);
                return refuseDrag("cross-domain", observed.domainOutput, observed.domainWorkspace);
            }
            const evidence = this.appliedById.get(windowId as string);
            if (
                evidence !== undefined &&
                (evidence.output !== observed.domainOutput || evidence.workspace !== observed.domainWorkspace)
            ) {
                this.logToken(`${LOG_PREFIX}:drag-drop-refused-cross-domain`);
                return refuseDrag("cross-domain", evidence.output, evidence.workspace);
            }
        }
        // Final drop binds focus to the observed mover, mirroring the
        // preview: the gesture owns the mover even when KWin's activeWindow
        // lags (Meta+drag without focus). The mover is present in the
        // effective observation, so this is observed mover focus, not
        // fabrication; the Engine's begin_drag requires mover==focused for
        // retained sessions. The Started source binding above stays
        // authoritative for cross-output routing.
        const dropMoverId = windowId as string;
        const dropObserved =
            effectiveObserved.focusedId === dropMoverId
                ? effectiveObserved
                : { ...effectiveObserved, focusedId: dropMoverId };
        const snapshot = this.carriedSnapshot(dropObserved);
        this.noteObservation(snapshot.fingerprint);
        // The final drop forwards the last validated preview `hover_prior`
        // verbatim (local preview key first, then the native drag-N key).
        const dropPrior =
            (previewKey !== null ? this.dragPreview.get(previewKey)?.prior : undefined) ??
            (drag !== null ? this.dragPreview.get(drag)?.prior : undefined);
        // Consume the preview entry so late replies fence out; the prior above already rides the wire.
        if (previewKey !== null) {
            this.dragPreview.delete(previewKey);
        }
        const intent: AutoIntent = {
            op: "drag-drop",
            snapshot,
            removed: null,
            body: {
                op: "drag-drop",
                window: windowId as string,
                x,
                y,
                ...(dropPrior !== undefined ? { hover_prior: dropPrior } : {}),
                ...(crossSource !== null ? { source_output: crossSource.output, source_workspace: crossSource.workspace } : {}),
            },
            pointerSource: windowId as string,
            ...(drag !== null ? { dragSource: drag } : {}),
        };
        // A final-geometry drop route is selected ahead of the ordinary
        // finish resync. Do not let that resync restore the old split first.
        // A deferred drag intent superseded here never dispatched: fold its
        // drop into the restore marker instead of losing it. The marker (not
        // the slot) owns convergence, so ordinary slot clearing below is
        // unchanged.
        this.clearDebounce();
        this.absorbDeferredDragIntent(this.deferredAuto, false);
        if (this.deferredAuto?.op === "reconcile" && this.deferredAuto.workAreaReprojection !== true) {
            this.deferredAuto = null;
        }
        this.discardInteractiveReconcile();
        if (this.inFlight) {
            this.deferredAuto = intent;
            return true;
        }
        this.dispatch(intent);
        const flight = this.pending;
        const ours =
            flight !== null &&
            flight.op === "drag-drop" &&
            flight.pointerSource === (windowId as string) &&
            (drag === null ? flight.dragSource == null : this.dragSourceOf(flight) === drag);
        if (!ours) {
            // Dispatch never installed our drop flight (interactive guard or
            // synchronous transport failure already fed the marker): report
            // refusal only. A deferred drop returned true above.
            if (drag !== null) {
                this.noteDragRejected(drag, "dispatch-failed", typeof windowId === "string" ? windowId : null, snapshot.domainOutput, snapshot.domainWorkspace);
                if (crossSource !== null) this.logToken(`${LOG_PREFIX}:drag-drop-cross-refused correlation=${drag} reason=dispatch-failed`);
            }
            return false;
        }
        return true;
    }

    // Bounded read-only drag preview: one direct DescribePlan `drag-preview`
    // request from a fresh destination observation (hints included) with
    // pointer x/y, the Started source binding, and the last validated prior
    // as `hover_prior`. Backs off when the single-flight is busy: never
    // queues, never defers, never delays the drop. Late replies are fenced by
    // sequence, scope, and owner/generation; refusal clears the prior and
    // reports null. Entry-driven clearing. No timers, no caps, no retries.
    requestDragPreview(
        windowId: unknown,
        x: unknown,
        y: unknown,
        dragCorrelation?: unknown,
        sourceDomain?: unknown,
        onPreview?: (result: PlanDragPreviewResult | null) => void,
    ): boolean {
        const callback = typeof onPreview === "function" ? onPreview : null;
        const drag = isDragCorrelation(dragCorrelation) ? (dragCorrelation as string) : null;
        const refuse = (reason: string): false => {
            const key = `${drag ?? "-"}:${sanitizeKind(reason)}`;
            if (this.lastPreviewRefuseKey !== key) {
                this.lastPreviewRefuseKey = key;
                this.logToken(
                    drag !== null
                        ? `${LOG_PREFIX}:drag-preview-refused correlation=${drag} reason=${sanitizeKind(reason)}`
                        : `${LOG_PREFIX}:drag-preview-refused reason=${sanitizeKind(reason)}`,
                );
            }
            if (drag !== null) {
                this.dragPreview.delete(drag);
            }
            try {
                callback?.(null);
            } catch (error) {
                void error;
            }
            return false;
        };
        if (!this.enabled) {
            return refuse("disabled");
        }
        if (drag === null || !isOpaqueId(windowId)) {
            return refuse("identity");
        }
        if (!isFiniteInt(x) || !isFiniteInt(y) || (x as number) < -16384 || (x as number) > 16384 || (y as number) < -16384 || (y as number) > 16384) {
            return refuse("coords");
        }
        // Single-flight busy: back off immediately, never queue anything the drop must wait for.
        // Rate-limited: one backed-off log per drag until a preview dispatches.
        if (this.r4Flight !== null || this.inFlight) {
            const busyKey = `${drag ?? "-"}:busy`;
            if (this.lastPreviewRefuseKey !== busyKey) {
                this.lastPreviewRefuseKey = busyKey;
                this.logToken(`${LOG_PREFIX}:busy-refused kind=drag-preview`);
                this.logToken(`${LOG_PREFIX}:drag-preview-backed-off correlation=${drag} reason=busy`);
            }
            return false;
        }
        const pointer = this.buildPointerDomainObservation(windowId as string, x as number, y as number, sourceDomain);
        if (pointer.ok === false) {
            return refuse(pointer.reason);
        }
        // Outside every known work area refuses with no overlay; a resolved
        // hidden destination proceeds with its own effective observation.
        if (pointer.pointerInSource === false && pointer.effectiveObserved === pointer.sourceObserved) {
            return refuse("outside");
        }
        if (pointer.mover.floating === true || pointer.mover.sticky === true || pointer.mover.fullscreen || pointer.mover.maximized) {
            return refuse("floating");
        }
        const previewSource = pointer.explicitSource;
        // Interactive-move preview binds focus to the observed mover: the
        // gesture owns the mover even when KWin's activeWindow lags (Meta+drag
        // without focus). The mover is present in the effective observation,
        // so this is observed mover focus, not fabrication; the Engine's
        // begin_drag requires mover==focused for retained sessions. Mirrors
        // the projected destination which already anchors focusedId on the
        // mover.
        const moverId = windowId as string;
        const focusedObserved =
            pointer.effectiveObserved.focusedId === moverId
                ? pointer.effectiveObserved
                : { ...pointer.effectiveObserved, focusedId: moverId };
        const snapshot = this.carriedSnapshot(focusedObserved);
        this.noteObservation(snapshot.fingerprint);
        const prior = this.dragPreview.get(drag)?.prior;
        const command: Record<string, unknown> = {
            op: "drag-preview",
            window: windowId as string,
            x,
            y,
            ...(prior !== undefined ? { hover_prior: prior } : {}),
            ...(previewSource !== null ? { source_output: previewSource.output, source_workspace: previewSource.workspace } : {}),
        };
        const seq = ++this.dragPreviewSeq;
        // Locally built from the validated generation plus "-v<seq>": always correlation-shaped.
        const correlation = `${this.generation}-v${String(seq)}`;
        const encoded = this.buildRequestPayload(snapshot, command, correlation);
        if (encoded.ok === false) {
            return refuse(encoded.reason);
        }
        const payload = encoded.payload;
        const entry = this.dragPreview.get(drag) ?? {};
        entry.live = {
            seq,
            scope: this.domainKey(snapshot),
            owner: this.owner,
            generation: this.generation,
            window: windowId as string,
            correlation,
            callback: callback ?? ((): void => {}),
        };
        this.dragPreview.set(drag, entry);
        this.lastPreviewRefuseKey = null;
        const target = isUniqueOwner(this.pinnedOwner) ? (this.pinnedOwner as string) : PLAN_SERVICE;
        try {
            this.env.callDbus(target, PLAN_OBJECT, PLAN_INTERFACE, PLAN_METHOD, payload, (reply) =>
                this.onDragPreviewReply(reply, drag, seq),
            );
        } catch (error) {
            void error;
            return refuse("dbus-failed");
        }
        return true;
    }

    // Entry-driven preview clearing: forget the carried prior and any fenced
    // late reply for one drag (or every drag when called without a valid
    // correlation). Never touches the ordinary single-flight.
    clearDragPreview(dragCorrelation?: unknown): void {
        if (isDragCorrelation(dragCorrelation)) {
            this.dragPreview.delete(dragCorrelation as string);
            return;
        }
        if (dragCorrelation === undefined) {
            this.dragPreview.clear();
        }
    }

    // Finish-edge fence: forget only the pending reply slot while preserving
    // the completed hover prior for the verdict transfer, so a late reply
    // after Finish cannot update it. Never touches the single-flight.
    invalidateDragPreviewReply(dragCorrelation?: unknown): void {
        if (isDragCorrelation(dragCorrelation)) {
            this.clearPreviewLive(dragCorrelation as string);
            return;
        }
        if (dragCorrelation === undefined) {
            for (const key of [...this.dragPreview.keys()]) {
                this.clearPreviewLive(key);
            }
        }
    }

    private clearPreviewLive(drag: string): void {
        const entry = this.dragPreview.get(drag);
        if (entry === undefined) {
            return;
        }
        delete entry.live;
        if (entry.prior === undefined) {
            this.dragPreview.delete(drag);
        }
    }

    // Destination scope fence: the destination domain must still be observed.
    // Cross-output previews dispatch from a hidden destination while the
    // mover is still native on the source, and native output follows the
    // dragged frame mid-drag: accept when that destination is still observed
    // and the mover is still on source OR already native on the destination
    // scope itself. Any other mover absence stays stale.
    private isPreviewScopeLive(scope: string, windowId: string): boolean {
        try {
            const fresh = this.freshObserved();
            const moverInFresh = fresh !== null && fresh.windows.some((entry) => entry.id === windowId);
            if (fresh !== null && this.domainKey(snapshotOf(fresh)) === scope) {
                return moverInFresh;
            }
            let destStillObserved = false;
            let moverInScope = false;
            try {
                for (const candidate of this.env.observeHidden?.() ?? []) {
                    try {
                        if (this.domainKey(snapshotOf(candidate)) === scope) {
                            destStillObserved = true;
                            if (candidate.windows.some((entry) => entry.id === windowId)) {
                                moverInScope = true;
                            }
                            break;
                        }
                    } catch (error) {
                        void error;
                    }
                }
            } catch (error) {
                void error;
            }
            if (!destStillObserved) {
                return false;
            }
            return moverInFresh || moverInScope;
        } catch (error) {
            void error;
            return false;
        }
    }

    private onDragPreviewReply(reply: unknown, drag: string, seq: number): void {
        const entry = this.dragPreview.get(drag);
        const live = entry?.live;
        if (entry === undefined || live === undefined || live.seq !== seq) {
            return;
        }
        const done = (result: PlanDragPreviewResult | null, outcome: string, reason: string): void => {
            if (result === null) {
                this.dragPreview.delete(drag);
            } else {
                this.clearPreviewLive(drag);
            }
            this.logToken(
                `${LOG_PREFIX}:drag-preview-settled correlation=${drag} outcome=${sanitizeKind(outcome)} reason=${sanitizeKind(reason)}`,
            );
            try {
                live.callback(result);
            } catch (error) {
                void error;
            }
        };
        // Fence by current owner/generation before parsing.
        if (!this.enabled || this.owner !== live.owner || this.generation !== live.generation) {
            done(null, "stale", "stale-owner");
            return;
        }
        if (typeof reply !== "string" || reply.length > PLAN_MAX_REPLY_BYTES) {
            done(null, "refused", "service-fault");
            return;
        }
        let parsed: unknown = null;
        try {
            parsed = JSON.parse(reply);
        } catch (error) {
            void error;
            done(null, "refused", "service-fault");
            return;
        }
        if (!isRecord(parsed) || parsed["v"] !== PLAN_CONTRACT_VERSION || parsed["correlation_id"] !== live.correlation) {
            done(null, "stale", "stale-dropped");
            return;
        }
        const outcome = parsed["outcome"];
        if (outcome !== "preview") {
            const kind = outcome === "rejected" || outcome === "diverged" ? sanitizeKind(parsed["kind"]) : "service-fault";
            done(null, "refused", kind);
            return;
        }
        if (!this.isPreviewScopeLive(live.scope, live.window)) {
            done(null, "stale", "stale-scope");
            return;
        }
        const rectRaw: unknown = parsed["preview_rect"];
        const priorRaw: unknown = parsed["hover_prior"];
        if (!isTargetRect(rectRaw) || !isValidHoverPrior(priorRaw)) {
            done(null, "refused", "precondition-mismatch");
            return;
        }
        const rect = rectRaw as PlanRect;
        entry.prior = priorRaw;
        done({ rect, hoverPrior: priorRaw }, "applied", "preview");
    }
    // Drop-intent correlation reader: validated drag-N only, never titles,
    // ids, or payload bytes. A pointer-resize or drag-drop flight carries
    // dragSource (the drop that dispatched it); marker reconciles carry a
    // domain key instead.
    private dragSourceOf(flightState: PendingFlight): string | null {
        const drag = flightState.dragSource;
        return typeof drag === "string" && isDragCorrelation(drag) ? drag : null;
    }

    // Validated Started source from the immutable flight body: the
    // dispatch-time cross-output binding when it names a cross domain for a
    // drag-correlated drag-drop flight. Rides deferred and stale-replan
    // dispatches unchanged, so terminals need no per-drag map.
    private crossDragSourceOf(flightState: PendingFlight): { output: string; workspace: string } | null {
        if (flightState.op !== "drag-drop" || this.dragSourceOf(flightState) === null) {
            return null;
        }
        const body = flightState.body as Record<string, unknown>;
        const output = body["source_output"];
        const workspace = body["source_workspace"];
        if (!isOpaqueId(output) || !isOpaqueId(workspace)) {
            return null;
        }
        if (output === flightState.snapshot.domainOutput && workspace === flightState.snapshot.domainWorkspace) {
            return null;
        }
        return { output: output as string, workspace: workspace as string };
    }

    // Correlated cross-output refusal: the caller owns destination-marker
    // convergence; only the generic cross diagnostic logs here.
    private logCrossDragRefused(flightState: PendingFlight, reason: string): void {
        const drag = this.dragSourceOf(flightState);
        if (drag === null || this.crossDragSourceOf(flightState) === null) {
            return;
        }
        this.logToken(`${LOG_PREFIX}:drag-drop-cross-refused correlation=${drag} reason=${sanitizeKind(reason)}`);
    }

    // Correlated cross-output success: generic applied diagnostic plus one
    // forced complete source reconcile through the existing send chain. The
    // destination plan satisfies only destination markers via the caller.
    private noteCrossDragApplied(flightState: PendingFlight): void {
        const drag = this.dragSourceOf(flightState);
        const source = this.crossDragSourceOf(flightState);
        if (drag === null || source === null) {
            return;
        }
        this.logToken(`${LOG_PREFIX}:drag-drop-cross-applied correlation=${drag} plan=${flightState.correlation}`);
        try {
            this.sendForcedDomains.add(this.dragRestoreKey(source.output, source.workspace));
        } catch (error) {
            void error;
        }
        this.dragPreview.delete(drag);
    }

    // Shared drag-pointer terminal: pointer flights feed the
    // destination-scoped marker (plus the cross refusal for cross-output
    // drops); marker reconciles bind their own terminal naming the plan.
    private settleDragTerminal(flightState: PendingFlight, reason: string, markerOutcome?: string): void {
        const drag = this.dragSourceOf(flightState);
        if (drag !== null) {
            this.noteDragRejected(drag, reason, flightState.pointerSource, flightState.snapshot.domainOutput, flightState.snapshot.domainWorkspace);
            if (flightState.op === "drag-drop") {
                this.logCrossDragRefused(flightState, reason);
                this.dragPreview.delete(drag);
            }
            return;
        }
        this.failDragRestore(flightState, markerOutcome ?? reason);
    }

    private dragRestoreKey(output: string, workspace: string): string {
        return `${output}\u0000${workspace}`;
    }

    // Resolve the domain scope for a rejected drop. Prefers the explicit
    // output/workspace (exact observation-time evidence), then the retained
    // domain holding the dragged window id (exact historical evidence, immune
    // to a foreground desktop switch between the drop and this refusal), then
    // a fresh observation only when it actually contains the dragged window.
    // An invalid (null) or unknown (unmatched) window never falls back to an
    // unrelated domain: that drop fails closed with an `unavailable` terminal
    // (bound by the caller), never an unscoped marker that any unrelated plan
    // could satisfy.
    private dragRestoreDomain(
        windowId: string | null,
        output?: string,
        workspace?: string,
    ): { output: string; workspace: string } | null {
        if (
            typeof output === "string" &&
            typeof workspace === "string" &&
            output.length > 0 &&
            workspace.length > 0
        ) {
            return { output, workspace };
        }
        if (windowId === null) {
            return null;
        }
        try {
            const evidence = this.appliedById.get(windowId);
            if (evidence !== undefined) {
                return { output: evidence.output, workspace: evidence.workspace };
            }
        } catch (error) {
            void error;
        }
        try {
            const observed = this.freshObserved();
            if (observed !== null && observed.windows.some((entry) => entry.id === windowId)) {
                return { output: observed.domainOutput, workspace: observed.domainWorkspace };
            }
        } catch (error) {
            void error;
        }
        return null;
    }

    // Record a rejected drop in its domain marker. Every rejected drag-N
    // gets exactly one `drag-rejected` line; overlapping drops share the
    // marker and each later gets its own settled terminal naming the same
    // plan correlation. When `dispatchNow` is true (no superseding intent
    // is being installed right after) a free slot dispatches the marker's
    // single reconcile immediately, otherwise the marker persists until the
    // finishFlight chain finds a free slot. Logging is best-effort and
    // never changes control flow.
    private noteDragRejected(
        drag: unknown,
        reason: string,
        windowId: string | null = null,
        output?: string,
        workspace?: string,
        dispatchNow = true,
    ): void {
        if (!isDragCorrelation(drag)) {
            return;
        }
        const id = drag as string;
        const kind = sanitizeKind(reason);
        try {
            const domain = this.dragRestoreDomain(windowId, output, workspace);
            if (domain === null) {
                // Fail closed with no marker at all: without domain evidence
                // no plan may ever claim this drop, so bind the terminal now
                // with an honest `plan=none` instead of a fabricated success.
                this.logToken(`${LOG_PREFIX}:drag-rejected correlation=${id} reason=${kind}`);
                this.logToken(`${LOG_PREFIX}:drag-reconcile-settled correlation=${id} outcome=unavailable plan=none`);
                return;
            }
            const key = this.dragRestoreKey(domain.output, domain.workspace);
            let marker = this.dragRestore.get(key);
            if (marker === undefined) {
                this.dragRestoreSeq += 1;
                marker = {
                    output: domain.output,
                    workspace: domain.workspace,
                    drags: [],
                    seq: this.dragRestoreSeq,
                    dispatched: false,
                };
                this.dragRestore.set(key, marker);
            }
            const known = marker.drags.indexOf(id) >= 0;
            if (!known) {
                this.logToken(`${LOG_PREFIX}:drag-rejected correlation=${id} reason=${kind}`);
                if (marker.drags.length >= MAX_DRAG_RESTORE_DRAGS) {
                    this.logToken(`${LOG_PREFIX}:drag-reconcile-settled correlation=${id} outcome=unavailable plan=none`);
                    return;
                }
                marker.drags.push(id);
            }
            if (marker.dispatched) {
                if (!known) {
                    this.logToken(`${LOG_PREFIX}:drag-reconcile correlation=${id} dispatch=shared`);
                }
                return;
            }
            if (!dispatchNow) {
                return;
            }
            const outcome = this.maybeDispatchDragRestore();
            if (outcome === "deferred" && !known) {
                this.logToken(`${LOG_PREFIX}:drag-reconcile correlation=${id} dispatch=deferred`);
            } else if (outcome === "failed" && !known) {
                this.logToken(`${LOG_PREFIX}:drag-reconcile correlation=${id} dispatch=failed`);
            }
        } catch (error) {
            void error;
        }
    }

    // Fold a deferred intent being superseded or cleared into the marker
    // map: a deferred drag pointer or drag-drop never dispatched, so its
    // drop joins the marker instead of vanishing. A queued marker reconcile
    // never exists (markers dispatch straight through), so nothing else
    // needs carrying. When `dispatchNow` is false the caller installs a
    // superseding intent right after, which will satisfy or fail the marker
    // on its own.
    private absorbDeferredDragIntent(intent: AutoIntent | null, dispatchNow: boolean): void {
        if (intent === null) {
            return;
        }
        try {
            if (
                (intent.op === "pointer-resize" || intent.op === "drag-drop") &&
                typeof intent.dragSource === "string" &&
                isDragCorrelation(intent.dragSource)
            ) {
                this.noteDragRejected(
                    intent.dragSource,
                    "superseded",
                    intent.pointerSource ?? null,
                    intent.snapshot.domainOutput,
                    intent.snapshot.domainWorkspace,
                    dispatchNow,
                );
            }
        } catch (error) {
            void error;
        }
    }

    // Dispatch the single reconcile for the oldest undispatched marker whose
    // domain is currently observed, through the existing dispatch route.
    // Returns the disposition for per-drag logging by the caller. Exactly one
    // dispatch per marker: the flag is set before dispatching, so a
    // never-sent attempt still consumes it (logged `dispatch=failed`, kept
    // pending for a later applied plan or recovery, never retried in a
    // loop). A marker whose domain is not currently observed persists
    // untouched until its domain is visible again; it never blocks an
    // eligible later marker for the observed domain.
    private maybeDispatchDragRestore(): "dispatched" | "deferred" | "failed" | "none" {
        try {
            if (!this.enabled) {
                return "none";
            }
            let anyPending = false;
            for (const entry of this.dragRestore.values()) {
                if (!entry.dispatched) {
                    anyPending = true;
                    break;
                }
            }
            if (!anyPending) {
                return "none";
            }
            if (
                this.inFlight ||
                this.r4Flight !== null ||
                this.interactiveResizeActive() ||
                this.deferredAuto !== null
            ) {
                return "deferred";
            }
            const observed = this.freshObserved();
            if (observed === null) {
                return "failed";
            }
            let key: string | null = null;
            let marker: DragRestoreMarker | undefined = undefined;
            for (const entry of this.dragRestore.entries()) {
                if (entry[1].dispatched) {
                    continue;
                }
                if (observed.domainOutput !== entry[1].output || observed.domainWorkspace !== entry[1].workspace) {
                    // The marked domain is not currently observed: persist
                    // untouched until its domain is visible again, never
                    // satisfy or dispatch against an unrelated domain.
                    continue;
                }
                key = entry[0];
                marker = entry[1];
                break;
            }
            if (key === null || marker === undefined) {
                return "deferred";
            }
            const snapshot = this.carriedSnapshot(observed);
            try {
                this.noteObservation(snapshot.fingerprint);
            } catch (error) {
                void error;
            }
            const intent: AutoIntent = {
                op: "reconcile",
                snapshot,
                removed: null,
                body: { op: "reconcile" },
                restoreMarker: key,
                restoreMarkerSeq: marker.seq,
            };
            marker.dispatched = true;
            this.dispatch(intent);
            if (this.inFlight) {
                for (const drag of marker.drags) {
                    try {
                        this.logToken(`${LOG_PREFIX}:drag-reconcile correlation=${drag} dispatch=dispatched`);
                    } catch (error) {
                        void error;
                    }
                }
                return "dispatched";
            }
            // Never sent: the dispatch failure paths bind their own exact
            // marker terminal (with the allocated plan correlation, or an
            // honest `plan=none` when none was allocated). Bind it here only
            // as a last-resort guard so no path strands the marker.
            if (this.dragRestore.get(key) !== undefined) {
                this.failMarkerDispatch(intent, "dispatch-failed", null);
            }
            return "failed";
        } catch (error) {
            void error;
            return "failed";
        }
    }

    // Satisfy markers through actual application only: the first subsequent
    // plan in the satisfying op set whose applied geometry covers ALL tiled
    // members of the flight domain clears that domain's marker, logging one
    // terminal per drag naming the satisfying plan correlation. Coverage is
    // the full wanted set (mirroring geometryCovers: non-floating members
    // homed to the flight domain, minus removals), so a partial geometry
    // never satisfies. Members skipped at write time (fullscreen, maximized,
    // or unrelated floating) are excluded from the restored count and never
    // produce an `applied` restoring claim: the marker's own reconcile then
    // settles a truthful `partial` terminal naming its plan with
    // covered=R/W and no retry, while another plan's partial application
    // leaves the marker pending to dispatch once the slot is free.
    // Focus-only, toggle-float, and unrelated-domain plans never satisfy.
    // Call only on the applied path, never on dispatch or reply.
    private satisfyDragRestore(flightState: PendingFlight, planned: PlannedReply, current: PlanObserved): void {
        try {
            if (!DRAG_RESTORE_SATISFYING_OPS.has(flightState.op)) {
                return;
            }
            const domainOutput = flightState.snapshot.domainOutput;
            const domainWorkspace = flightState.snapshot.domainWorkspace;
            const homed = planned.geometry.every(
                (entry) =>
                    entry.output === domainOutput &&
                    entry.workspace === domainWorkspace,
            );
            if (!homed) {
                return;
            }
            const key = this.dragRestoreKey(domainOutput, domainWorkspace);
            const marker = this.dragRestore.get(key);
            if (marker === undefined) {
                return;
            }
            if (typeof flightState.restoreMarker === "string" && typeof flightState.restoreMarkerSeq === "number" && marker.seq !== flightState.restoreMarkerSeq) {
                return;
            }
            const wanted: string[] = [];
            for (const entry of flightState.snapshot.windows) {
                if (entry.output !== domainOutput || entry.workspace !== domainWorkspace) {
                    continue;
                }
                if (flightState.removed !== null && entry.id === flightState.removed) {
                    continue;
                }
                if (
                    entry.floating === true &&
                    !(flightState.floatTarget !== null && flightState.floatTarget.window === entry.id && flightState.floatTarget.floating === false)
                ) {
                    continue;
                }
                wanted.push(entry.id);
            }
            const plannedIds = new Set<string>();
            for (const entry of planned.geometry) {
                plannedIds.add(entry.window);
            }
            const skipped = new Set<string>();
            for (const entry of current.windows) {
                if (entry.output !== domainOutput || entry.workspace !== domainWorkspace) {
                    continue;
                }
                if (entry.fullscreen) {
                    skipped.add(entry.id);
                } else if (entry.maximized) {
                    skipped.add(entry.id);
                } else if (entry.floating === true && flightState.floatTarget?.window !== entry.id) {
                    skipped.add(entry.id);
                }
            }
            let covered = 0;
            for (const id of wanted) {
                if (plannedIds.has(id) && !skipped.has(id)) {
                    covered += 1;
                }
            }
            if (covered === wanted.length) {
                this.dragRestore.delete(key);
                for (const drag of marker.drags) {
                    try {
                        this.logToken(
                            `${LOG_PREFIX}:drag-reconcile-settled correlation=${drag} outcome=applied plan=${flightState.correlation} covered=${covered}/${wanted.length}`,
                        );
                    } catch (error) {
                        void error;
                    }
                }
                return;
            }
            if (flightState.restoreMarker === key) {
                // The marker's own reconcile could not restore every tiled
                // member: bind the truthful partial terminal naming this plan
                // with the covered count, then clear with no retry.
                this.dragRestore.delete(key);
                for (const drag of marker.drags) {
                    try {
                        this.logToken(
                            `${LOG_PREFIX}:drag-reconcile-settled correlation=${drag} outcome=partial plan=${flightState.correlation} covered=${covered}/${wanted.length}`,
                        );
                    } catch (error) {
                        void error;
                    }
                }
                return;
            }
            // Another plan's partial application never satisfies: the marker
            // stays pending (re-armed, never consumed) to dispatch once the
            // slot is free.
            marker.dispatched = false;
            for (const drag of marker.drags) {
                try {
                    this.logToken(
                        `${LOG_PREFIX}:drag-reconcile correlation=${drag} dispatch=pending covered=${covered}/${wanted.length}`,
                    );
                } catch (error) {
                    void error;
                }
            }
        } catch (error) {
            void error;
        }
    }

    // Terminal for a marker reconcile that never became a flight (payload
    // build/oversize failure, timer/dispatch transport failure): one exact
    // terminal per drag naming the allocated plan correlation, or an honest
    // `plan=none` when none was allocated. The marker clears with no retry
    // and no silent loss; ordinary intents are untouched (no restore key).
    private failMarkerDispatch(intent: AutoIntent, outcome: string, plan: string | null): void {
        try {
            const key = intent.restoreMarker;
            if (typeof key !== "string") {
                return;
            }
            const marker = this.dragRestore.get(key);
            if (marker === undefined) {
                return;
            }
            if (typeof intent.restoreMarkerSeq === "number" && marker.seq !== intent.restoreMarkerSeq) {
                return;
            }
            this.dragRestore.delete(key);
            const cause = sanitizeKind(outcome);
            const planToken = typeof plan === "string" && isCorrelationId(plan) ? plan : "none";
            for (const drag of marker.drags) {
                try {
                    this.logToken(
                        `${LOG_PREFIX}:drag-reconcile-settled correlation=${drag} outcome=${cause} plan=${planToken}`,
                    );
                } catch (error) {
                    void error;
                }
            }
        } catch (error) {
            void error;
        }
    }

    // Teardown for enable/disable lifecycle resets: every pending marker gets
    // one correlated `unavailable` terminal per drag before clearing, so no
    // drop is silently lost across the reset. Best-effort logging only, never
    // changes control flow.
    private settleDragRestoreUnavailable(): void {
        try {
            for (const marker of this.dragRestore.values()) {
                for (const drag of marker.drags) {
                    try {
                        this.logToken(
                            `${LOG_PREFIX}:drag-reconcile-settled correlation=${drag} outcome=unavailable plan=none`,
                        );
                    } catch (error) {
                        void error;
                    }
                }
            }
        } catch (error) {
            void error;
        }
        this.dragRestore.clear();
    }

    // Lists are independently validated by the entry; null proves nothing.
    // Keep the incarnation fence: an output name may return after replug
    // before an old marker flight replies.
    public pruneDragRestoreForTopology(
        workspaceIds: ReadonlyArray<string> | null | undefined,
        outputNames: ReadonlyArray<string> | null | undefined,
    ): void {
        const workspaces =
            Array.isArray(workspaceIds) && workspaceIds.length > 0 ? new Set(workspaceIds) : null;
        const outputs =
            Array.isArray(outputNames) && outputNames.length > 0 ? new Set(outputNames) : null;
        if (workspaces === null && outputs === null) {
            return;
        }
        for (const [key, marker] of [...this.dragRestore.entries()]) {
            const workspaceDead = workspaces !== null && !workspaces.has(marker.workspace);
            const outputDead = outputs !== null && !outputs.has(marker.output);
            if (!workspaceDead && !outputDead) {
                continue;
            }
            if (this.dragRestore.get(key) !== marker) {
                continue;
            }
            this.dragRestore.delete(key);
            for (const drag of marker.drags) {
                this.logToken(
                    `${LOG_PREFIX}:drag-reconcile-settled correlation=${drag} outcome=unavailable plan=none`,
                );
            }
        }
    }

    // Terminal for a dispatched marker reconcile that itself failed: one
    // correlated terminal per drag naming the failed plan correlation, then
    // the marker clears with no retry. Ordinary flight failures leave
    // pending markers untouched for the finishFlight chain.
    private failDragRestore(flightState: PendingFlight, outcome: string): void {
        try {
            const key = flightState.restoreMarker;
            if (typeof key !== "string") {
                return;
            }
            const marker = this.dragRestore.get(key);
            if (marker === undefined) {
                return;
            }
            if (typeof flightState.restoreMarkerSeq === "number" && marker.seq !== flightState.restoreMarkerSeq) {
                return;
            }
            this.dragRestore.delete(key);
            const cause = sanitizeKind(outcome);
            for (const drag of marker.drags) {
                try {
                    this.logToken(
                        `${LOG_PREFIX}:drag-reconcile-settled correlation=${drag} outcome=${cause} plan=${flightState.correlation}`,
                    );
                } catch (error) {
                    void error;
                }
            }
        } catch (error) {
            void error;
        }
    }

    requestResync(): void {
        this.onSignal();
    }

    // Terminal send-settlement edge (entry-owned): force one complete
    // source AND target reconcile through the existing single-flight
    // foreground+hidden chain, bypassing the equal-applied-evidence quiet
    // path once per involved domain. Invalid keys are ignored (a generic
    // resync still runs); unreadable domains stay forced until the next
    // complete observation. Never synthesizes empty evidence and never
    // blocks normal signals or intents.
    notifySendSettled(settled: {
        readonly sourceOutput: string;
        readonly sourceWorkspace: string;
        readonly targetOutput: string;
        readonly targetWorkspace: string;
    }): void {
        if (!this.enabled) {
            return;
        }
        try {
            if (isOpaqueId(settled.sourceOutput) && isOpaqueId(settled.sourceWorkspace)) {
                this.sendForcedDomains.add(this.domainKey({
                    domainOutput: settled.sourceOutput,
                    domainWorkspace: settled.sourceWorkspace,
                }));
            }
            if (isOpaqueId(settled.targetOutput) && isOpaqueId(settled.targetWorkspace)) {
                this.sendForcedDomains.add(this.domainKey({
                    domainOutput: settled.targetOutput,
                    domainWorkspace: settled.targetWorkspace,
                }));
            }
        } catch (error) {
            void error;
        }
        this.requestResync();
    }

    // Peek without consuming: true when a send settlement forced this
    // domain's next complete refresh to reconcile even on equal applied
    // evidence. Consumed only when its reconcile actually dispatches, so
    // unreadable domains and suppressed (interactive/in-flight) rounds keep
    // the force for the next complete observation.
    private hasSendForced(key: string): boolean {
        try {
            return this.sendForcedDomains.has(key);
        } catch (error) {
            void error;
            return false;
        }
    }

    private consumeSendForced(key: string): void {
        try {
            this.sendForcedDomains.delete(key);
        } catch (error) {
            void error;
        }
    }

    // Lean R4 terminal helper: an R4-shape pending flight (move, four-direction,
    // two-domain snapshot) forces one complete source AND target reconcile
    // through the existing `notifySendSettled` chain, including equal
    // evidence. No-op for ordinary flights. Called on every R4 terminal
    // (stale/rejected/timeout before staging plus lean arrival terminals)
    // so both domains converge from native observation with no phantom.
    private forceR4SettleFromPending(flightState: PendingFlight | null): void {
        if (flightState === null) {
            return;
        }
        if (flightState.op !== "move" || flightState.background === true) {
            return;
        }
        if (!isDirection(flightState.direction)) {
            return;
        }
        const domains = flightState.snapshot.domains;
        if (domains === undefined || domains.length !== 2) {
            return;
        }
        const source = domains[0] as PlanDomain;
        const target = domains[1] as PlanDomain;
        try {
            this.notifySendSettled({
                sourceOutput: source.output,
                sourceWorkspace: source.workspace,
                targetOutput: target.output,
                targetWorkspace: target.workspace,
            });
        } catch (error) {
            void error;
        }
    }

    setInteractiveResizeActive(active: boolean): void {
        if (active) {
            this.clearDebounce();
            this.absorbDeferredDragIntent(this.deferredAuto, true);
            if (this.deferredAuto?.op === "reconcile" && this.deferredAuto.workAreaReprojection !== true) {
                this.deferredAuto = null;
            }
            this.discardInteractiveReconcile();
            return;
        }
        // Reuse the normal debounced observation path. This reasserts only the
        // retained allocation and never derives a split from native geometry.
        this.requestResync();
    }

    private clearRepeat(): void {
        this.repeatFocused = null;
        this.repeatDirection = null;
        this.repeatMode = null;
        this.repeatNext = 0;
        this.repeatFingerprint = "";
    }

    private resetReconcileState(): void {
        this.reconcileAttempts = 0;
    }

    // Accept only exact client-held rectangles after bounded reassertions.
    private acceptClientDrift(snapshot: PlanSnapshot, hidden = false): void {
        let accepted = 0;
        for (const entry of snapshot.windows) {
            if (entry.fullscreen || entry.maximized) {
                continue;
            }
            const evidence = this.appliedById.get(entry.id);
            if (
                evidence === undefined ||
                evidence.output !== entry.output ||
                evidence.workspace !== entry.workspace ||
                evidence.floating !== (entry.floating === true) ||
                evidence.sticky !== (entry.sticky === true)
            ) {
                continue;
            }
            if (hidden && entry.sticky === true) {
                continue;
            }
            if (
                evidence.rect.x === entry.rect.x &&
                evidence.rect.y === entry.rect.y &&
                evidence.rect.w === entry.rect.w &&
                evidence.rect.h === entry.rect.h
            ) {
                continue;
            }
            this.appliedById.set(entry.id, { ...evidence, rect: { ...entry.rect }, minimumPlaced: false });
            accepted += 1;
        }
        if (hidden) {
            this.clearBackgroundReconcile(snapshot);
        } else {
            this.reconcileAttempts = 0;
        }
        this.logToken(`${LOG_PREFIX}:reconcile-accepted windows=${String(accepted)} cause=stable-drift recovery=accept-client-rect`);
    }

    // Whether the snapshot observes a B6 minimum-placed member homed on its
    // domain: applied evidence behind a currently observed id carries the
    // minimum-placement marker with matching output/workspace. Converged
    // observations on such domains must not reset bounded reassertion
    // accounting (the echo of our own effective write precedes the host
    // verdict); ordinary feasible domains reset exactly as before.
    private domainHasMinimumPlaced(snapshot: PlanSnapshot): boolean {
        for (const entry of snapshot.windows) {
            const evidence = this.appliedById.get(entry.id);
            if (
                evidence !== undefined &&
                evidence.minimumPlaced === true &&
                evidence.output === entry.output &&
                evidence.workspace === entry.workspace
            ) {
                return true;
            }
        }
        return false;
    }

    private noteObservation(fingerprint: string): void {
        if (fingerprint !== this.repeatFingerprint) {
            this.repeatFingerprint = fingerprint;
            this.repeatFocused = null;
            this.repeatDirection = null;
            this.repeatMode = null;
            this.repeatNext = 0;
        }
    }

    private freshObserved(): PlanObserved | null {
        let observed: PlanObserved | null = null;
        try {
            observed = this.env.observe();
        } catch (error) {
            void error;
            observed = null;
        }
        if (!validateObserved(observed)) {
            return null;
        }
        return observed as PlanObserved;
    }

    private interactiveResizeActive(): boolean {
        try {
            return this.env.isInteractiveResizeActive?.() === true;
        } catch (error) {
            void error;
            return true;
        }
    }

    // Active-drag cancellation covers foreground and hidden reconcile
    // alike (single shared slot); work-area reprojection stays exempt.
    private discardInteractiveReconcile(): void {
        const flight = this.pending;
        if (
            flight === null ||
            flight.op !== "reconcile" ||
            flight.workAreaReprojection
        ) {
            return;
        }
        this.clearTimer();
        this.inFlight = false;
        this.pending = null;
        this.pinnedOwner = null;
        this.activationStep = 0;
        this.diag(flight.op, flight.correlation, flight.windowCount, "interactive-resize-suppressed");
        // A suppressed marker reconcile is a cancellation, never a failure:
        // re-arm the same marker pending (not dispatched) so the next
        // same-domain applied plan can satisfy it, a rejected drop can
        // trigger its single follow-up, or a free slot can dispatch it for a
        // different domain. Genuine failed reconciles still terminate through
        // failDragRestore with no retry.
        try {
            const key = flight.restoreMarker;
            if (typeof key === "string") {
                const marker = this.dragRestore.get(key);
                if (marker !== undefined) {
                    if (typeof flight.restoreMarkerSeq === "number" && marker.seq !== flight.restoreMarkerSeq) {
                        return;
                    }
                    marker.dispatched = false;
                    for (const drag of marker.drags) {
                        try {
                            this.logToken(`${LOG_PREFIX}:drag-reconcile correlation=${drag} dispatch=cancelled`);
                        } catch (error) {
                            void error;
                        }
                    }
                }
            }
        } catch (error) {
            void error;
        }
    }

    private onSignal(kind?: PlanSignal, target?: object): void {
        if (!this.enabled) {
            return;
        }
        // R-OUT-05: retain the birth ref (the entry forwards the
        // `windowAdded` payload through this target). Identity, never
        // dereferenced: the route later requires the candidate to BE this
        // ref. Consumed one-shot per foreground refresh below; a missing
        // payload refuses routing.
        if (kind === "added") {
            this.routeBirthRef = typeof target === "object" && target !== null ? target : null;
        }
        // Evict only the removed native object, never inferred absence.
        // Exact-object eviction covers marker-only ids with no applied slot,
        // so truly removed windows never leave session-local residue. No
        // native reads, no id logging, no per-domain inference: relocation
        // survivors carry a different live ref and keep their markers.
        if (kind === "removed" && typeof target === "object" && target !== null) {
            try {
                this.env.noteNativeRemoved?.(target);
            } catch (error) {
                void error;
            }
            for (const [id, ref] of this.heldInitialFullscreen) {
                if (ref === target) {
                    this.heldInitialFullscreen.delete(id);
                }
            }
            for (const [id, ref] of this.seenNonFullscreen) {
                if (ref === target) {
                    this.seenNonFullscreen.delete(id);
                }
            }
            this.evictFixedByRef(target);
        }
        // Bounded native-write exclusion only: signals delivered
        // synchronously from our own R4 setters must not advance epoch or
        // queue auto intents mid-stack; the immediate post-write observation
        // covers sync arrival. Delayed arrival stays observable through the
        // armed one-shot mover signals. No transaction-lifetime Plan block.
        if (this.r4WriteDepth > 0) {
            return;
        }
        // D06 pre-debounce anchor: the debounced refresh observes only the
        // birth domain, so a managed focus shift landing before the debounce
        // would otherwise route against the stale anchor. Capture the last
        // active eligible known-managed domain synchronously here, before the
        // coalesced-debounce early return below. Unknown refs (births),
        // ineligible flags, and unreadable guards never reanchor. Silent:
        // no diagnostics, no epoch, no dispatch.
        if (kind === "activated") {
            this.captureRouteAnchorOnActivated(target);
        }
        if (kind === "maximize" && this.maximizeAdmissionEcho !== null) {
            if (target === this.maximizeAdmissionEcho) {
                this.maximizeAdmissionEcho = null;
                this.logToken(`${LOG_PREFIX}:maximize-admission-echo-consumed`);
                return;
            }
            this.maximizeAdmissionEcho = null;
            this.logToken(`${LOG_PREFIX}:maximize-admission-echo-mismatched`);
        }
        if (kind === "maximize" && this.maximizeToggleEcho !== null) {
            if (target === this.maximizeToggleEcho.ref) {
                this.maximizeToggleEcho = null;
                this.logToken(`${LOG_PREFIX}:maximize-toggle-echo-consumed`);
                return;
            }
            this.maximizeToggleEcho = null;
            this.logToken(`${LOG_PREFIX}:maximize-toggle-echo-mismatched`);
        }
        if (kind === "desktops" && this.stickyEcho !== null) {
            const echo = this.stickyEcho;
            if (target === echo.ref) {
                this.stickyEcho = null;
                this.logToken(`${LOG_PREFIX}:sticky-echo-consumed`);
                if (!echo.allDesktops) {
                    const wasAdopted = this.adoptedSticky.delete(echo.id);
                    this.stickyPreviousFloating.delete(echo.id);
                    if (wasAdopted) {
                        // Adopted unknown origin stays a normal float on the
                        // then-current desktop: native assignment already
                        // homed it, so only mark canonical float tracking
                        // with no geometry, workspace, or planner admission.
                        // Q3: the confirmed ordinary float is also settled
                        // intent (no fixed record exists for it), recorded
                        // only when the canonical setter actually landed so a
                        // later restart keeps it. Sticky controls unchanged.
                        let floated = typeof this.env.setFloating === "function";
                        try {
                            this.env.setFloating?.(echo.id, true);
                        } catch (error) {
                            void error;
                            floated = false;
                        }
                        if (floated && this.intentActive) {
                            this.intentMembers.add(echo.id);
                            this.scheduleIntentWrite();
                        }
                    }
                    if (!echo.previousFloating) {
                        if (!this.restoreKeepAbove(echo.id, echo.resourceClass)) {
                            return;
                        }
                        this.requestFloat();
                    }
                }
                return;
            }
            this.stickyEcho = null;
            this.logToken(`${LOG_PREFIX}:sticky-echo-mismatched`);
        }
        if (this.debounceCancel !== null) {
            return;
        }
        try {
            this.debounceCancel = this.env.scheduleOnce(PLAN_DEBOUNCE_MS, () => {
                this.debounceCancel = null;
                this.refreshNow();
            });
        } catch (error) {
            void error;
            this.debounceCancel = null;
        }
    }

    // Shared debounced entry: foreground lifecycle first, then at most one
    // hidden-domain lifecycle step. Foreground behavior is unchanged and
    // always wins: a foreground intent (dispatched or deferred) suppresses the
    // background scan for this round, and the finishFlight chain converges
    // remaining hidden domains afterwards.
    private refreshNow(): void {
        if (!this.chainingHidden) {
            this.hiddenVisited.clear();
        }
        if (this.chainingHidden) {
            this.refreshForegroundNow();
            return;
        }
        this.chainingHidden = true;
        try {
            this.refreshForegroundNow();
            if (!this.enabled || this.inFlight || this.deferredAuto !== null) {
                return;
            }
            this.refreshHiddenNow();
            // Idle marker pump: when the foreground decided no ordinary
            // flight was needed and no hidden step took the slot, a pending
            // drop marker still owns convergence. This covers guard releases
            // (settle/timeout/removal resync) that land on an already-equal
            // snapshot: without a flight the finishFlight chain never runs,
            // so the marker would strand. Same single-slot guards as the
            // finishFlight pump; a dispatched ordinary or hidden flight
            // above already returned before reaching here.
            if (!this.enabled || this.inFlight || this.deferredAuto !== null) {
                return;
            }
            if (this.activeProbe !== 0) {
                return;
            }
            this.maybeDispatchDragRestore();
        } finally {
            this.chainingHidden = false;
        }
    }

    // D23 sole-close cleanup for a null foreground observation: synthesize
    // one explicit empty-domain reconcile for the first pending-close domain
    // with planner-accepted explicit emptiness. A null observation is
    // unknown (throwing/unreadable observers keep live members): without a
    // prior applied-empty proof the close stays retained for the next
    // readable signal instead of fabricating empty evidence. Remaining
    // domains stay retained for later signals.
    private dispatchCloseCleanupForNull(): boolean {
        let keys: string[] = [];
        try {
            keys = [...this.pendingClosed.keys()].sort();
        } catch (error) {
            void error;
            return false;
        }
        for (const id of keys) {
            let homing: { output: string; workspace: string } | undefined;
            try {
                homing = this.pendingClosed.get(id);
            } catch (error) {
                void error;
                continue;
            }
            if (homing === undefined) {
                continue;
            }
            const key = this.domainKey({ domainOutput: homing.output, domainWorkspace: homing.workspace });
            // Unknown is never upgraded to empty: only a planner-applied
            // empty reconcile proves no live members to churn.
            if (!this.explicitEmptyByKey.has(key)) {
                continue;
            }
            let retained: { bounds: PlanRect; gap: number; outerGap: number } | undefined;
            try {
                retained = this.lastDomainByKey.get(key);
            } catch (error) {
                void error;
                continue;
            }
            if (retained === undefined) {
                continue;
            }
            if (!this.isTiledDomain(homing.output, homing.workspace)) {
                continue;
            }
            const snapshot: PlanSnapshot = {
                domainOutput: homing.output,
                domainWorkspace: homing.workspace,
                domainBounds: { x: retained.bounds.x, y: retained.bounds.y, w: retained.bounds.w, h: retained.bounds.h },
                domainGap: retained.gap,
                domainOuterGap: retained.outerGap,
                focusedId: "",
                windows: Object.freeze([]),
                fingerprint: String(planFingerprint(homing.output, homing.workspace, "", [])),
            };
            this.epoch += 1;
            this.noteObservation(snapshot.fingerprint);
            this.logRefreshClassification("foreground", "change", "close-cleanup", "dispatch");
            this.absorbDeferredDragIntent(this.deferredAuto, false);
            this.deferredAuto = {
                op: "reconcile",
                snapshot,
                removed: null,
                body: { op: "reconcile" },
                admissionMaximizeClears: Object.freeze([]),
            };
            if (this.inFlight) {
                this.logRefreshClassification("foreground", "change", "close-cleanup", "deferred");
                return true;
            }
            const pendingReconcile = this.deferredAuto;
            this.deferredAuto = null;
            if (pendingReconcile !== null) {
                this.dispatch(pendingReconcile);
            }
            return true;
        }
        return false;
    }

    // Foreground automatic reconcile: one complete-observation reconcile per
    // debounced foreground signal. Membership converges in the Engine; KWin
    // never derives admit/remove, floating-skew, or gap/bounds choices here.
    // A correlated exact-message gap refusal triggers the existing single
    // update-gaps retry on the reply path. The carried snapshot preserves
    // per-id applied rectangles for fullscreen/transient out-of-bounds.
    // Only reassertion counters reset here; applied evidence mutates solely on
    // applied replies plus per-window client acceptance. Fresh/changed
    // membership or flags always dispatch and never accept.
    private refreshForegroundNow(): void {
        if (!this.enabled) {
            return;
        }
        // R-OUT-05 birth one-shot: consumed here so an unreadable round never
        // carries a stale birth into a later unrelated observation.
        const birthRef = this.routeBirthRef;
        this.routeBirthRef = null;
        let fresh = this.freshObserved();
        if (fresh === null) {
            // D23: a sole-minimize close leaves no live observation (unknown,
            // never empty evidence). Deliver its cleanup as one synthesized
            // explicit empty-domain reconcile on the retained dispatched
            // shape when a pending close names a known domain. Anything else
            // stays unknown exactly as before.
            if (this.dispatchCloseCleanupForNull()) {
                return;
            }
            return;
        }
        // Floating workspaces skip automatic foreground tiling.
        if (!this.isTiledDomain(fresh.domainOutput, fresh.domainWorkspace)) {
            // R-MAX-03: record floating-first sightings before the skip.
            this.noteFloatingFirstSighted(fresh);
            this.logRefreshClassification("foreground", "equal", "workspace-floating", "quiet");
            return;
        }
        // R-OUT-05 focused-output newcomer routing: an eligible newcomer the
        // host mapped onto a non-focused output moves to the anchor output's
        // shown workspace before tiling, with newcomer focus after proof.
        // Refusals fail open to native admission on the observed output. A
        // null return aborts the round (uncertain post-write state; the next
        // signal retries convergence without routing).
        fresh = this.maybeRouteNewcomerToManagedOutput(fresh, birthRef);
        if (fresh === null) {
            return;
        }
        const prepared = this.clearMaximizeAtAdmission(fresh, null);
        if (prepared === null) {
            return;
        }
        fresh = prepared.observed;
        const freshSnapshot = this.carriedSnapshot(fresh);
        const epochBeforeQuiet = this.epoch;
        this.epoch += 1;
        this.noteObservation(freshSnapshot.fingerprint);
        // D23 pending closes scoped to this domain (stale live re-sightings
        // drop inside). A non-empty set forces a reconcile below so the
        // cleanup delivers on this turn instead of staying quiet.
        const closeIds = this.closedForSnapshot(freshSnapshot);
        // One shared applied-vs-complete classification (strict foreground).
        const classification = this.classifyCompleteObservation(freshSnapshot, fresh, false);
        const appliedScope = classification.appliedScope;
        const ordinaryReason = !classification.pureDrift
            ? "membership-or-flags"
            : classification.rawRetainedOutOfBounds
              ? "raw-out-of-bounds"
              : "drift";
        // Work-area scope transition: same applied window set with changed
        // bounds and equal applied gaps. Dispatches applied reprojection even
        // while interactive, even in-flight deferred, with counter reset.
        if (
            appliedScope !== null &&
            classification.gapsEqual &&
            classification.boundsChanged &&
            this.appliedWindowSetMatches(freshSnapshot)
        ) {
            const oldBounds = appliedScope.bounds;
            const newBounds = freshSnapshot.domainBounds;
            this.logToken(
                `${LOG_PREFIX}:scope-transition old=${String(oldBounds.x)},${String(oldBounds.y)},${String(oldBounds.w)},${String(oldBounds.h)} new=${String(newBounds.x)},${String(newBounds.y)},${String(newBounds.w)},${String(newBounds.h)}`,
            );
            this.logToken(`${LOG_PREFIX}:work-area-reprojection selected=retained`);
            this.pointerEcho = null;
            this.resetReconcileState();
            this.consumeSendForced(this.domainKey(freshSnapshot));
            this.absorbDeferredDragIntent(this.deferredAuto, false);
            this.deferredAuto = {
                op: "reconcile",
                snapshot: this.reprojectionSnapshot(fresh),
                removed: null,
                body: { op: "reconcile" },
                workAreaReprojection: true,
                admissionMaximizeClears: prepared.cleared,
            };
            if (this.inFlight) {
                this.logRefreshClassification("foreground", "change", "work-area-transition", "deferred");
                return;
            }
            const nextReprojection = this.deferredAuto;
            this.deferredAuto = null;
            if (nextReprojection !== null) {
                this.dispatch(nextReprojection);
            }
            this.logRefreshClassification("foreground", "change", "work-area-transition", "dispatch");
            return;
        }
        const restoreKey = this.domainKey(freshSnapshot);
        // One-shot send-settlement force: a forced domain reconciles even
        // when the equal-applied-evidence optimization would stay quiet.
        // The force is consumed only when its reconcile dispatches below,
        // so unreadable or suppressed rounds keep it for the next complete
        // observation.
        const sendForced = this.hasSendForced(restoreKey);
        const restoreMarker = this.dragRestore.get(restoreKey);
        const hasPendingMarker =
            restoreMarker !== undefined && !restoreMarker.dispatched && restoreMarker.drags.length > 0;
        if (classification.pureDrift && classification.converged && classification.scopeEqual && !hasPendingMarker && freshSnapshot.windows.length > 0 && !classification.rawRetainedOutOfBounds && !sendForced && closeIds.length === 0) {
            // Quiet equality cannot invalidate an in-flight reply.
            this.epoch = epochBeforeQuiet;
            if (this.pointerEcho !== null) {
                this.logToken(`${LOG_PREFIX}:echo-fence-cleared-equality`);
            }
            this.pointerEcho = null;
            // Ordinary feasible domains reset exactly as before. A domain
            // observing a B6 minimum-placed member keeps its count: the
            // converged observation may echo our own just-completed
            // effective write before the host enforces its verdict, and
            // resetting here would make bounded acceptance unreachable
            // for hosts that do not retain the minimum.
            if (!this.domainHasMinimumPlaced(freshSnapshot)) {
                this.reconcileAttempts = 0;
            }
            this.absorbDeferredDragIntent(this.deferredAuto, true);
            if (this.deferredAuto !== null && this.deferredAuto.op === "reconcile") {
                this.deferredAuto = null;
            }
            this.logRefreshClassification("foreground", "equal", "applied-evidence-equal", "quiet");
            if (this.inFlight) {
                return;
            }
            const next = this.deferredAuto;
            this.deferredAuto = null;
            if (next !== null) {
                this.dispatch(next);
            }
            return;
        }
        // Pointer echo consume: a matching neighbour echo means the planned
        // geometry already landed, so consume the one-shot fence with no
        // second write and no baseline mutation. A mismatch falls through to
        // one ordinary reconcile below. A send-forced domain skips the fence
        // so its forced reconcile below still dispatches.
        const echo = this.pointerEcho;
        if (echo !== null && !sendForced && closeIds.length === 0) {
            this.pointerEcho = null;
            if (this.echoMatches(freshSnapshot, echo)) {
                this.logToken(`${LOG_PREFIX}:echo-fence-consumed`);
                this.reconcileAttempts = 0;
                this.absorbDeferredDragIntent(this.deferredAuto, true);
                if (this.deferredAuto !== null && this.deferredAuto.op === "reconcile") {
                    this.deferredAuto = null;
                }
                this.logRefreshClassification("foreground", "equal", "echo-matched", "quiet");
                if (this.inFlight) {
                    return;
                }
                const next = this.deferredAuto;
                this.deferredAuto = null;
                if (next !== null) {
                    this.dispatch(next);
                }
                return;
            }
            this.logToken(`${LOG_PREFIX}:echo-fence-mismatched`);
        }
        if (this.interactiveResizeActive()) {
            // A deferred drag pointer waits out the live gesture inside its
            // marker; ordinary queued reconciles keep the established clear.
            this.absorbDeferredDragIntent(this.deferredAuto, true);
            if (this.deferredAuto?.op === "reconcile" && this.deferredAuto.workAreaReprojection !== true) {
                this.deferredAuto = null;
            }
            this.logRefreshClassification("foreground", "uncertain", "interactive-active", "suppressed");
            return;
        }
        // Bounded per-window acceptance: pure geometry drift accepts after
        // three reassertions. Fresh/flag/scope/raw/forced always dispatch.
        if (!classification.pureDrift || classification.converged || classification.rawRetainedOutOfBounds || sendForced) {
            this.reconcileAttempts = 0;
        } else if (this.reconcileAttempts >= MAX_RECONCILE_ATTEMPTS && classification.scopeEqual) {
            this.acceptClientDrift(freshSnapshot);
            this.absorbDeferredDragIntent(this.deferredAuto, true);
            if (this.deferredAuto !== null && this.deferredAuto.op === "reconcile") {
                this.deferredAuto = null;
            }
            this.logRefreshClassification("foreground", "equal", "stable-drift-accepted", "accept");
            if (this.inFlight) {
                return;
            }
            const next = this.deferredAuto;
            this.deferredAuto = null;
            if (next !== null) {
                this.dispatch(next);
            }
            return;
        }
        // Ordinary converge: a deferred drag pointer superseded here joins
        // its domain marker (the fresh reconcile below satisfies it on
        // apply); the slot itself carries no drag binding.
        this.absorbDeferredDragIntent(this.deferredAuto, false);
        if (sendForced) {
            this.consumeSendForced(restoreKey);
        }
        this.deferredAuto = {
            op: "reconcile",
            snapshot: freshSnapshot,
            removed: null,
            body: { op: "reconcile" },
            admissionMaximizeClears: prepared.cleared,
        };
        if (this.inFlight) {
            this.logRefreshClassification("foreground", "change", sendForced ? "send-forced" : ordinaryReason, "deferred");
            return;
        }
        const pendingReconcile = this.deferredAuto;
        this.deferredAuto = null;
        if (pendingReconcile !== null) {
            this.dispatch(pendingReconcile);
        }
        this.logRefreshClassification("foreground", "change", sendForced ? "send-forced" : ordinaryReason, "dispatch");
    }

    private refreshHiddenNow(): void {
        if (!this.enabled || this.inFlight || this.deferredAuto !== null) {
            return;
        }
        // Active tiled drag holds hidden domains too: destination members
        // must not reflow mid-drag. Work is not queued; the post-drag
        // chain re-observes.
        if (this.interactiveResizeActive()) {
            return;
        }
        let hidden: ReadonlyArray<PlanObserved> = [];
        try {
            const observeHidden = this.env.observeHidden;
            if (typeof observeHidden !== "function") {
                return;
            }
            hidden = observeHidden();
        } catch (error) {
            void error;
            return;
        }
        if (!Array.isArray(hidden)) {
            return;
        }
        const valid: PlanObserved[] = [];
        for (const entry of hidden) {
            if (validateObserved(entry)) {
                valid.push(entry);
            }
        }
        // Every validated hidden domain is considered: explicit empty-source
        // cleanup for an already-retained domain converges through reconcile,
        // and new domains converge without a domain-count gate. Retained
        // domains converge below. Each complete domain is visited at most
        // once per chain (marked even when no intent is sent); absence stays
        // unknown and never synthesizes empty evidence.
        const nonEmpty = valid.filter((entry) => entry.windows.length > 0);
        const emptyExplicit = valid.filter((entry) => entry.windows.length === 0);
        for (const observed of nonEmpty) {
            const key = this.domainKey(observed);
            if (this.hiddenVisited.has(key)) {
                continue;
            }
            this.hiddenVisited.add(key);
            const decision = this.hiddenIntentFor(observed);
            if (decision === null) {
                continue;
            }
            if (decision.intent !== null) {
                this.dispatch(decision.intent);
            }
            this.logRefreshClassification("hidden", decision.outcome, decision.reason, decision.terminal);
            if (decision.intent !== null) {
                return;
            }
        }
        // Empty-source cleanup only from explicit fresh, complete
        // empty-domain evidence whose output is not tainted/unclassifiable.
        // Absence is unknown (exception transition, tainted output,
        // unreadable domain, or overall failure) and must never synthesize
        // an empty snapshot. Per-domain baselines are retired
        // only via applied reconcile convergence; attempts use the per-domain
        // background accounting. No visibility history or polling is invented.
        let foregroundKey: string | null = null;
        try {
            const foreground = this.freshObserved();
            if (foreground !== null) {
                foregroundKey = this.domainKey(snapshotOf(foreground));
            }
        } catch (error) {
            void error;
            foregroundKey = null;
        }
        for (const observed of emptyExplicit) {
            const key = this.domainKey(observed);
            if (foregroundKey !== null && key === foregroundKey) {
                continue;
            }
            if (this.hiddenVisited.has(key)) {
                continue;
            }
            this.hiddenVisited.add(key);
            // Explicit complete empty with retained applied members converges
            // through one reconcile (Engine retires the slot after its
            // fences), even when several members vanished together. Domains
            // without applied evidence/scope stay quiet inside hiddenIntentFor.
            const decision = this.hiddenIntentFor(observed);
            if (decision === null) {
                continue;
            }
            if (decision.intent !== null) {
                this.dispatch(decision.intent);
            }
            this.logRefreshClassification("hidden", decision.outcome, decision.reason, decision.terminal);
            if (decision.intent !== null) {
                return;
            }
        }
    }

    private freshHiddenFor(domain: Pick<PlanSnapshot, "domainOutput" | "domainWorkspace">): PlanObserved | null {
        let hidden: ReadonlyArray<PlanObserved> = [];
        try {
            const observeHidden = this.env.observeHidden;
            if (typeof observeHidden !== "function") {
                return null;
            }
            hidden = observeHidden();
        } catch (error) {
            void error;
            return null;
        }
        if (!Array.isArray(hidden)) {
            return null;
        }
        for (const entry of hidden) {
            if (
                validateObserved(entry) &&
                entry.domainOutput === domain.domainOutput &&
                entry.domainWorkspace === domain.domainWorkspace
            ) {
                return entry;
            }
        }
        // Absence is unknown, never empty: exception-only, tainted, or
        // unreadable domains are omitted by the observer and must remain
        // untouched. Only an explicit empty-domain observation counts as
        // empty evidence. No synthesis here.
        return null;
    }

    // Single hidden-domain step: same shared classification with sticky
    // multi-home forgiveness. Empty dispatches only with applied evidence;
    // omission stays unknown. No echo/interactive handling here. Floating
    // workspaces are unmanaged and never dispatch here.
    private hiddenIntentFor(observed: PlanObserved): HiddenDecision | null {
        if (!this.isTiledDomain(observed.domainOutput, observed.domainWorkspace)) {
            // R-MAX-03: record floating-first sightings before the skip.
            this.noteFloatingFirstSighted(observed);
            return { intent: null, outcome: "equal", reason: "workspace-floating", terminal: "quiet" };
        }
        const prepared = this.clearMaximizeAtAdmission(observed, null, () =>
            this.freshHiddenFor(observed),
        );
        if (prepared === null) {
            return null;
        }
        const freshSnapshot = this.carriedSnapshot(prepared.observed);
        const classification = this.classifyCompleteObservation(freshSnapshot, prepared.observed, true);
        const observedIds = classification.observedIds;
        const hiddenScope = classification.appliedScope;
        // D23 pending closes scoped here (stale live re-sightings drop).
        // A non-empty set forces a reconcile below so hidden minimized
        // closes deliver instead of staying quiet.
        const closeIds = this.closedForSnapshot(freshSnapshot);
        if (classification.isEmpty) {
            // Evidence scan only for explicit empty: omission stays unknown.
            // Pending closes still dispatch: the empty shape is real observer
            // evidence, never synthesis.
            if (hiddenScope === null && !this.hasAppliedEvidenceFor(freshSnapshot) && closeIds.length === 0) {
                return { intent: null, outcome: "uncertain", reason: "no-applied-evidence", terminal: "quiet" };
            }
        }
        // Scoped-omission retirement: drop per-domain applied evidence for
        // applied ids omitted from this hidden observation, exactly like
        // the applied-boundary per-domain prune below. Never a global
        // removal: a hidden-to-hidden relocation (or minimize) omits here
        // before the new domain homes the survivor, so only the
        // ref-confirmed native close path may globally evict fixed
        // provenance, intent membership, or float marks. Sticky, keep-above,
        // and maximize maps are preserved for the moving survivor, matching
        // the foreground omission path (which never cleans here).
        for (const [id, evidence] of [...this.appliedById]) {
            if (
                evidence.output === freshSnapshot.domainOutput &&
                evidence.workspace === freshSnapshot.domainWorkspace &&
                !observedIds.has(id)
            ) {
                this.appliedById.delete(id);
            }
        }
        const hiddenKey = this.domainKey(freshSnapshot);
        const hiddenForced = this.hasSendForced(hiddenKey);
        if (classification.isEmpty || !classification.pureDrift) {
            this.consumeSendForced(hiddenKey);
            return {
                intent: {
                    op: "reconcile",
                    snapshot: freshSnapshot,
                    removed: null,
                    body: { op: "reconcile" },
                    admissionMaximizeClears: prepared.cleared,
                    background: true,
                },
                outcome: "change",
                reason: classification.isEmpty ? "empty-with-evidence" : "membership-or-flags",
                terminal: "dispatch",
            };
        }
        if (classification.converged && classification.scopeEqual && !classification.rawRetainedOutOfBounds && !hiddenForced && closeIds.length === 0) {
            // Ordinary feasible domains reset exactly as before. A domain
            // observing a B6 minimum-placed member keeps its count (see the
            // foreground quiet branch): the converged observation may echo
            // our own just-completed effective write before the host
            // enforces its verdict.
            if (!this.domainHasMinimumPlaced(freshSnapshot)) {
                this.clearBackgroundReconcile(freshSnapshot);
            }
            return { intent: null, outcome: "equal", reason: "applied-evidence-equal", terminal: "quiet" };
        }
        // Work-area transition: pureDrift holds here, so same window set is
        // implied without the foreground windowSetMatches gate.
        if (hiddenScope !== null && classification.gapsEqual && classification.boundsChanged) {
            const oldBounds = hiddenScope.bounds;
            const newBounds = freshSnapshot.domainBounds;
            this.logToken(
                `${LOG_PREFIX}:scope-transition old=${String(oldBounds.x)},${String(oldBounds.y)},${String(oldBounds.w)},${String(oldBounds.h)} new=${String(newBounds.x)},${String(newBounds.y)},${String(newBounds.w)},${String(newBounds.h)}`,
            );
            this.logToken(`${LOG_PREFIX}:work-area-reprojection selected=retained`);
            this.clearBackgroundReconcile(freshSnapshot);
            this.consumeSendForced(hiddenKey);
            return {
                intent: {
                    op: "reconcile",
                    snapshot: this.reprojectionSnapshot(prepared.observed),
                    removed: null,
                    body: { op: "reconcile" },
                    workAreaReprojection: true,
                    background: true,
                },
                outcome: "change",
                reason: "work-area-transition",
                terminal: "dispatch",
            };
        }
        // Deliberate gap reload: same applied window set with a changed inner
        // and/or outer gap, converged without touching focus.
        if (hiddenScope !== null && !classification.gapsEqual) {
            this.logToken(`${LOG_PREFIX}:gap-reprojection selected=retained`);
            this.consumeSendForced(hiddenKey);
            return {
                intent: {
                    op: "update-gaps",
                    snapshot: freshSnapshot,
                    removed: null,
                    body: { op: "update-gaps" },
                    background: true,
                },
                outcome: "change",
                reason: "gap-change",
                terminal: "dispatch",
            };
        }
        // Bounded acceptance for pure geometry drift only; membership/flag
        // changes already returned above. Pending closes never accept: they
        // need a reconcile to deliver. Send-forced always converges.
        if (!hiddenForced && closeIds.length === 0 && (this.backgroundAttempts.get(this.domainKey(observed)) ?? 0) >= MAX_RECONCILE_ATTEMPTS) {
            this.acceptClientDrift(freshSnapshot, true);
            return { intent: null, outcome: "equal", reason: "stable-drift-accepted", terminal: "accept" };
        }
        if (hiddenForced) {
            this.clearBackgroundReconcile(freshSnapshot);
            this.consumeSendForced(hiddenKey);
        }
        return {
            intent: {
                op: "reconcile",
                snapshot: freshSnapshot,
                removed: null,
                body: { op: "reconcile" },
                background: true,
            },
            outcome: "change",
            reason: hiddenForced ? "send-forced" : classification.rawRetainedOutOfBounds ? "raw-out-of-bounds" : "drift",
            terminal: "dispatch",
        };
    }

    private noteBackgroundTerminal(snapshot: Pick<PlanSnapshot, "domainOutput" | "domainWorkspace">): void {
        const key = this.domainKey(snapshot);
        const attempts = (this.backgroundAttempts.get(key) ?? 0) + 1;
        this.backgroundAttempts.set(key, attempts);
    }

    private clearBackgroundReconcile(snapshot: Pick<PlanSnapshot, "domainOutput" | "domainWorkspace">): void {
        const key = this.domainKey(snapshot);
        this.backgroundAttempts.delete(key);
    }

    private clearMaximizeAtAdmission(
        observed: PlanObserved,
        previous: PlanSnapshot | null,
        refetch: () => PlanObserved | null = () => this.freshObserved(),
    ): { observed: PlanObserved; cleared: ReadonlyArray<string> } | null {
        void previous;
        const attempted: Array<{ id: string; ref: object; resourceClass: string }> = [];
        for (const entry of observed.windows) {
            // Q3 includes floating-to-tiled admission: keep maximize over a
            // reserved slot. Only a held born-fullscreen exit clears once,
            // before carriedSnapshot releases the exact-ref hold. D5: a
            // fixed exit floats untouched with no writes, so skip the
            // clear (and its refetch) using the CURRENT predicate.
            // R-MAX-06 adds one born-maximized lane below: under the
            // `tile-on-launch` default a tiled-first born maximized window
            // clears once, then admits ordinarily; `keep-maximized-overlay`
            // keeps this existing overlay behaviour (no clear here).
            if (entry.fullscreen || !entry.maximized || this.maximizeAdmissionAttempts.has(entry.id)) {
                continue;
            }
            if (this.heldInitialFullscreen.get(entry.id) !== entry.ref) {
                // R-MAX-06 born lane: fullscreen isolation above already
                // skipped fullscreen entries. Floating, sticky, R-MAX-03
                // floating-first, and already-tiled ids keep their overlay
                // under both settings; later maximizes are unchanged (a
                // prior attempt or applied evidence skips). D4 fixed births
                // share the fixed skip below and float untouched.
                if (this.readBornMaximized() !== BORN_MAXIMIZED_DEFAULT) {
                    continue;
                }
                if (this.appliedById.has(entry.id)) {
                    continue;
                }
                if (entry.floating === true || entry.sticky === true) {
                    continue;
                }
                if (this.isFloatingFirst(entry.id)) {
                    continue;
                }
            }
            const exitHints = this.hintSizesFor(entry.ref);
            if (isFixedSize(exitHints.minSize, exitHints.maxSize, this.readFixedSizePredicate())) {
                const resourceClass = isOpaqueId(entry.resourceClass) ? entry.resourceClass : "unknown";
                this.logToken(`${LOG_PREFIX}:maximize-admission-clear window=${entry.id} resource_class=${resourceClass} outcome=skipped-fixed`);
                continue;
            }
            this.maximizeAdmissionAttempts.add(entry.id);
            const resourceClass = isOpaqueId(entry.resourceClass) ? entry.resourceClass : "unknown";
            this.logToken(`${LOG_PREFIX}:maximize-admission-clear window=${entry.id} resource_class=${resourceClass} outcome=issued`);
            this.maximizeAdmissionEcho = entry.ref;
            this.logToken(`${LOG_PREFIX}:maximize-admission-echo-armed`);
            let outcome: MaximizeClearOutcome = "threw";
            try {
                outcome = this.env.clearMaximize(entry.ref);
            } catch (error) {
                void error;
            }
            this.logToken(
                `${LOG_PREFIX}:maximize-admission-clear window=${entry.id} resource_class=${resourceClass} outcome=${outcome}`,
            );
            if (this.maximizeAdmissionEcho !== null) {
                this.maximizeAdmissionEcho = null;
                this.logToken(`${LOG_PREFIX}:maximize-admission-echo-cleared-no-signal`);
            }
            attempted.push({ id: entry.id, ref: entry.ref, resourceClass });
        }
        if (attempted.length === 0) {
            return { observed, cleared: Object.freeze([]) };
        }
        const fresh = refetch();
        if (fresh === null) {
            return null;
        }
        const byId = new Map<string, PlanObservedWindow>();
        for (const entry of fresh.windows) {
            byId.set(entry.id, entry);
        }
        const cleared: string[] = [];
        for (const attempt of attempted) {
            const entry = byId.get(attempt.id);
            if (entry === undefined || entry.ref !== attempt.ref) {
                this.logToken(`${LOG_PREFIX}:maximize-admission-clear window=${attempt.id} resource_class=${attempt.resourceClass} outcome=observed-absent`);
            } else if (entry.maximized) {
                this.logToken(`${LOG_PREFIX}:maximize-admission-clear window=${attempt.id} resource_class=${attempt.resourceClass} outcome=observed-maximized`);
                cleared.push(attempt.id);
            } else {
                this.logToken(`${LOG_PREFIX}:maximize-admission-clear window=${attempt.id} resource_class=${attempt.resourceClass} outcome=observed-cleared`);
                cleared.push(attempt.id);
            }
        }
        const result = fresh;
        return { observed: result, cleared: Object.freeze(cleared) };
    }

    private echoMatches(fresh: PlanSnapshot, echo: PointerEcho): boolean {
        if (!sameScope(fresh, echo.scope)) {
            return false;
        }
        const byId = new Map<string, PlanRect>();
        for (const entry of fresh.windows) {
            byId.set(entry.id, entry.rect);
        }
        if (!byId.has(echo.source)) {
            return false;
        }
        for (const expected of echo.neighbours) {
            const actual = byId.get(expected.window);
            if (
                actual === undefined ||
                actual.x !== expected.rect.x ||
                actual.y !== expected.rect.y ||
                actual.w !== expected.rect.w ||
                actual.h !== expected.rect.h
            ) {
                return false;
            }
        }
        return true;
    }

    private noteReconcileTerminal(op: PlanOp, workAreaReprojection = false): void {
        if (
            op !== "reconcile" ||
            workAreaReprojection ||
            this.deferredAuto?.workAreaReprojection === true
        ) {
            return;
        }
        this.reconcileAttempts += 1;
    }

    // Bounded reassertion accounting for the automatic foreground route: only
    // same-membership pure-drift reconciles advance the three-strike count
    // toward per-window acceptance. A reconcile carrying tiled membership
    // or floating/sticky flag changes (the retired admit/remove shape,
    // including fresh seeds with no applied evidence yet) never advances it;
    // work-area reprojections keep their existing early return through the
    // delegate below. Reads before-apply per-id evidence only, never baseline
    // authority.
    private noteAutoReconcileTerminal(flightState: PendingFlight): void {
        if (
            flightState.op === "reconcile" &&
            flightState.background !== true &&
            flightState.workAreaReprojection !== true
        ) {
            if (this.appliedMembershipOrFlagsChanged(flightState.snapshot)) {
                return;
            }
        }
        this.noteReconcileTerminal(flightState.op, flightState.workAreaReprojection);
    }

    // Shared validated DescribePlan encoder for the ordinary single-flight
    // and the read-only drag preview (which never enters the single-flight).
    // Hints ride the snapshot, prior rides the command.
    private buildRequestPayload(
        snapshot: PlanSnapshot,
        command: Record<string, unknown>,
        correlation: string,
        closedIds?: ReadonlyArray<string>,
    ): { ok: true; payload: string } | { ok: false; reason: "request-invalid" | "request-over-cap" } {
        const sortedIds = snapshot.windows.map((entry) => entry.id).sort();
        const windows = snapshot.windows.map((entry) => ({
            window: entry.id,
            output: entry.output,
            workspace: entry.workspace,
            rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
            ...(entry.floating === true ? { floating: true } : {}),
            ...(entry.fullscreen === true ? { fullscreen: true } : {}),
            // Native maximized stays local snapshot semantics (never rides
            // the wire): the Engine admission leg fail-closes on maximized
            // entries, and the born-maximized overlay admission depends on
            // ordinary-tile admission plus local overlay isolation. The
            // shared-core G-06 focus fence therefore enforces through the
            // adapter-local fence on this route; the Engine opt-in covers
            // carriers that observe maximized directly (Windows handoff).
            ...(entry.sticky === true ? { sticky: true } : {}),
            ...(entry.fixedAuto === true ? { fixed_auto: true } : {}),
            ...(entry.fixedSuppress === true ? { fixed_suppress: true } : {}),
            ...(entry.floating === true || entry.sticky === true || entry.fullscreen || entry.maximized ? { fit_excluded: true } : {}),
            ...(entry.minSize === undefined ? {} : { min_size: { w: entry.minSize.w, h: entry.minSize.h } }),
            ...(entry.maxSize === undefined ? {} : { max_size: { w: entry.maxSize.w, h: entry.maxSize.h } }),
        }));
        const op = command["op"];
        const directionalDomains =
            (op === "focus" || op === "move") &&
            snapshot.domains !== undefined &&
            snapshot.domains.length === 2
                ? snapshot.domains.map((entry) => ({
                      output: entry.output,
                      workspace: entry.workspace,
                      bounds: { x: entry.bounds.x, y: entry.bounds.y, w: entry.bounds.w, h: entry.bounds.h },
                      gap: entry.gap,
                      outer_gap: entry.outerGap,
                      adjacent: { ...(entry.adjacent as Record<string, string>) },
                  }))
                : undefined;
        const fingerprint =
            directionalDomains === undefined
                ? planFingerprint(snapshot.domainOutput, snapshot.domainWorkspace, snapshot.focusedId, sortedIds)
                : planDirectionalFingerprint(
                      snapshot.domains as ReadonlyArray<PlanDomain>,
                      snapshot.focusedId,
                      windows.map((entry) => ({
                          window: entry.window as string,
                          output: entry.output as string,
                          workspace: entry.workspace as string,
                          rect: entry.rect as PlanRect,
                          floating: (entry as Record<string, unknown>)["floating"] === true,
                          fitExcluded: (entry as Record<string, unknown>)["fit_excluded"] === true,
                      })),
                  );
        let payload = "";
        try {
            payload = JSON.stringify({
                v: PLAN_CONTRACT_VERSION,
                correlation_id: correlation,
                owner: this.owner,
                generation: this.generation,
                revision: 0,
                fingerprint,
                domain: {
                    output: snapshot.domainOutput,
                    workspace: snapshot.domainWorkspace,
                    bounds: {
                        x: snapshot.domainBounds.x,
                        y: snapshot.domainBounds.y,
                        w: snapshot.domainBounds.w,
                        h: snapshot.domainBounds.h,
                    },
                    gap: snapshot.domainGap,
                    outer_gap: snapshot.domainOuterGap,
                },
                ...(directionalDomains === undefined ? {} : { domains: directionalDomains }),
                focused_window: snapshot.focusedId,
                windows,
                // D23 close-versus-minimize distinction, reconcile only: the
                // convergence vehicle carries explicitly closed ids so the
                // planner forgets their slots instead of minting minimize
                // slots. Omitted when empty so all other requests stay
                // byte-identical.
                ...(op === "reconcile" && closedIds !== undefined && closedIds.length > 0
                    ? { closed_windows: [...closedIds] }
                    : {}),
                command,
                // R-SPC-04 D1 predicate. The default both-axes-fixed stays
                // wire-omitted so historical requests are byte-identical;
                // either-axis-fixed carries explicitly. Invalid resolves
                // to default.
                ...(this.readFixedSizePredicate() === FIXED_SIZE_PREDICATE_EITHER
                    ? { fixed_size_predicate: FIXED_SIZE_PREDICATE_EITHER }
                    : {}),
            });
        } catch (error) {
            void error;
            return { ok: false, reason: "request-invalid" };
        }
        if (payload.length > PLAN_MAX_REQUEST_BYTES) {
            return { ok: false, reason: "request-over-cap" };
        }
        return { ok: true, payload };
    }

    private dispatch(intent: AutoIntent): void {
        if (!this.enabled || this.inFlight) {
            return;
        }
        // Q3 bootstrap fence: no plan admission (and therefore no later
        // retile-then-undo) before the single intent read settles. Dropped
        // intents are lossless: the bootstrap terminal runs one catch-up
        // resync over fresh observation. Explicit commands in the bounded
        // window land here with their kind attributed.
        if (this.intentActive && !this.intentReady) {
            this.logToken(`${LOG_PREFIX}:intent-bootstrap-deferred kind=${intent.op}`);
            return;
        }
        // Floating workspaces skip geometry-producing dispatch. Releases bypass.
        if (intent.op !== "release-domain") {
            const domains = intent.snapshot.domains;
            if (domains !== undefined && domains.length === 2) {
                for (const entry of domains) {
                    if (!this.isTiledDomain(entry.output, entry.workspace)) {
                        this.logToken(`${LOG_PREFIX}:workspace-floating-skip kind=${intent.op}`);
                        if (typeof intent.dragSource === "string" && isDragCorrelation(intent.dragSource)) {
                            this.noteDragRejected(intent.dragSource, "workspace-floating", intent.pointerSource ?? null, intent.snapshot.domainOutput, intent.snapshot.domainWorkspace);
                        }
                        this.failMarkerDispatch(intent, "workspace-floating", null);
                        return;
                    }
                }
            } else if (!this.isTiledDomain(intent.snapshot.domainOutput, intent.snapshot.domainWorkspace)) {
                this.logToken(`${LOG_PREFIX}:workspace-floating-skip kind=${intent.op}`);
                if (typeof intent.dragSource === "string" && isDragCorrelation(intent.dragSource)) {
                    this.noteDragRejected(intent.dragSource, "workspace-floating", intent.pointerSource ?? null, intent.snapshot.domainOutput, intent.snapshot.domainWorkspace);
                }
                this.failMarkerDispatch(intent, "workspace-floating", null);
                return;
            }
        }
        // A new lifecycle command supersedes an unanswered terminal probe. Its
        // callback must not make a later recovery decision for an older flight.
        this.activeProbe = 0;
        this.clearProbeTimer();
        // Active tiled drag defers reconcile actuation on both affected
        // outputs (foreground and hidden): a moving member must never reflow
        // mid-drag. Work-area reprojection stays exempt. Already-dispatched
        // drag-drop intents are not reconcile and still dispatch.
        if (
            intent.op === "reconcile" &&
            intent.workAreaReprojection !== true &&
            this.interactiveResizeActive()
        ) {
            return;
        }
        if (intent.workAreaReprojection === true) {
            if (intent.background === true) {
                this.clearBackgroundReconcile(intent.snapshot);
            } else {
                this.resetReconcileState();
            }
        }
        if (this.seq < 0 || this.seq > PLAN_MAX_SEQ) {
            const nextEpoch = this.seqEpoch + 1;
            if (!Number.isSafeInteger(nextEpoch)) {
                return;
            }
            const candidate = `${this.generation}-p${String(nextEpoch)}r0`;
            if (!isCorrelationId(candidate)) {
                return;
            }
            this.seqEpoch = nextEpoch;
            this.seq = 0;
            this.logToken(`${LOG_PREFIX}:sequence-rotated correlation=${candidate} cause=sequence-exhausted recovery=rotated`);
        }
        const correlation =
            this.seqEpoch === 0
                ? `${this.generation}-p${String(this.seq)}`
                : `${this.generation}-p${String(this.seqEpoch)}r${String(this.seq)}`;
        this.seq += 1;
        if (!isCorrelationId(correlation)) {
            return;
        }
        const snapshot = intent.snapshot;
        // D23: retain the dispatched domain shape for a later sole-close
        // cleanup (whose observation is null) and resolve closes scoped to
        // this snapshot now, so the flight carries exactly what it sends. A
        // non-empty snapshot proves live members, so it clears any accepted-
        // empty proof for the domain. Shapes persist until replaced (never
        // evicted: domains are bounded by outputs times desktops).
        try {
            const key = this.domainKey(snapshot);
            this.lastDomainByKey.set(key, {
                bounds: { x: snapshot.domainBounds.x, y: snapshot.domainBounds.y, w: snapshot.domainBounds.w, h: snapshot.domainBounds.h },
                gap: snapshot.domainGap,
                outerGap: snapshot.domainOuterGap,
            });
            if (snapshot.windows.length > 0) {
                this.explicitEmptyByKey.delete(key);
            }
        } catch (error) {
            void error;
        }
        const closedIds =
            intent.body["op"] === "reconcile" ? this.closedForSnapshot(snapshot) : Object.freeze([] as string[]);
        const encoded = this.buildRequestPayload(snapshot, intent.body, correlation, closedIds);
        if (encoded.ok === false) {
            this.logToken(`${LOG_PREFIX}:request-refused correlation=${correlation} reason=${encoded.reason}`);
            if (typeof intent.dragSource === "string" && isDragCorrelation(intent.dragSource)) {
                this.noteDragRejected(intent.dragSource, "dispatch-failed", intent.pointerSource ?? null, intent.snapshot.domainOutput, intent.snapshot.domainWorkspace);
            }
            this.failMarkerDispatch(intent, "dispatch-failed", null);
            return;
        }
        const payload = encoded.payload;
        const isRecovery = this.nextIsRecovery;
        this.nextIsRecovery = false;
        this.inFlight = true;
        this.pending = {
            correlation,
            op: intent.op,
            epoch: this.epoch,
            plannerSession: this.plannerSession,
            snapshot,
            removed: intent.removed,
            windowCount: snapshot.windows.length,
            pointerSource: intent.pointerSource ?? null,
            dragSource: intent.dragSource ?? null,
            restoreMarker: intent.restoreMarker ?? null,
            restoreMarkerSeq: intent.restoreMarkerSeq ?? null,
            workAreaReprojection: intent.workAreaReprojection === true,
            admissionMaximizeClears: intent.admissionMaximizeClears ?? Object.freeze([]),
            floatTarget: intent.floatTarget ?? null,
            stickyTarget: intent.stickyTarget ?? null,
            fixedStage: intent.fixedStage ?? null,
            background: intent.background === true,
            direction:
                intent.direction === "left" ||
                intent.direction === "right" ||
                intent.direction === "up" ||
                intent.direction === "down"
                    ? intent.direction
                    : null,
            requestPayload: payload,
            closedSent: Object.freeze([...closedIds]),
            requestRevision: 0,
            isRecovery,
            body: intent.body,
            replanned: intent.replanned === true,
        };
        this.noteFixedSizeDispatch(snapshot, intent.op, correlation);
        this.r4WriteDepth = 0;
        this.lifecycleDiag(this.pending as PendingFlight, "request", "dispatch", "started", "-");
        this.callbackSeen = false;
        this.token += 1;
        const flight = this.token;
        this.activeToken = flight;
        const session = this.plannerSession;
        this.pinnedOwner = null;
        this.activationStep = 1;
        try {
            const cancel = this.env.scheduleOnce(PLAN_TIMEOUT_MS, () => this.onTimeout(flight, session));
            this.cancelTimer = cancel;
        } catch (error) {
            void error;
            this.inFlight = false;
            this.pending = null;
            this.activationStep = 0;
            this.diag(intent.op, correlation, snapshot.windows.length, "timer-failed");
            if (intent.background === true) {
                this.noteBackgroundTerminal(intent.snapshot);
            } else {
                this.noteReconcileTerminal(intent.op, intent.workAreaReprojection === true);
            }
            // A never-sent pointer joins its marker (persisting for a later
            // free moment); a never-sent marker attempt binds its exact
            // failure naming the allocated plan correlation.
            if (typeof intent.dragSource === "string" && isDragCorrelation(intent.dragSource)) {
                this.noteDragRejected(intent.dragSource, "timer-failed", intent.pointerSource ?? null, intent.snapshot.domainOutput, intent.snapshot.domainWorkspace);
            }
            this.failMarkerDispatch(intent, "timer-failed", correlation);
            this.finishFlight();
            return;
        }
        // Phase 1: strict NameHasOwner presence. KWin does not deliver
        // GetNameOwner's absent-name error to callbacks, so presence must be
        // distinguished first. Strict boolean only; anything else is terminal
        // no-planner with no recovery and no retry.
        try {
            const presenceFlight = this.pending;
            if (presenceFlight !== null) {
                this.activateDiag(presenceFlight, "presence", "presence-requested");
            }
            this.env.callDbus(
                PLAN_DBUS_SERVICE,
                PLAN_DBUS_OBJECT,
                PLAN_DBUS_INTERFACE,
                PLAN_HAS_OWNER_METHOD,
                PLAN_SERVICE,
                (reply) => this.onNamePresence(reply, flight, session),
            );
        } catch (error) {
            void error;
            this.clearTimer();
            const lostPresence = this.pending;
            if (lostPresence !== null) {
                this.activateDiag(lostPresence, "presence", "presence-throw");
            }
            this.inFlight = false;
            this.pending = null;
            this.activationStep = 0;
            this.diag(intent.op, correlation, snapshot.windows.length, "dbus-failed");
            if (intent.background === true) {
                this.noteBackgroundTerminal(intent.snapshot);
            } else {
                this.noteReconcileTerminal(intent.op, intent.workAreaReprojection === true);
            }
            // A never-sent pointer joins its marker (persisting for a later
            // free moment); a never-sent marker attempt binds its exact
            // failure naming the allocated plan correlation.
            if (typeof intent.dragSource === "string" && isDragCorrelation(intent.dragSource)) {
                this.noteDragRejected(intent.dragSource, "dbus-failed", intent.pointerSource ?? null, intent.snapshot.domainOutput, intent.snapshot.domainWorkspace);
            }
            this.failMarkerDispatch(intent, "dbus-failed", correlation);
            this.finishFlight();
        }
    }

    private onNamePresence(reply: unknown, flight: number, session: number): void {
        if (!this.inFlight || flight !== this.activeToken || session !== this.plannerSession || this.activationStep !== 1) {
            return;
        }
        const flightState = this.pending;
        if (flightState === null || flightState.plannerSession !== session) {
            return;
        }
        if (reply === true) {
            this.activateDiag(flightState, "presence", "present");
            this.activationStep = 2;
            try {
                this.activateDiag(flightState, "resolve", "resolve-requested");
                this.env.callDbus(
                    PLAN_DBUS_SERVICE,
                    PLAN_DBUS_OBJECT,
                    PLAN_DBUS_INTERFACE,
                    PLAN_GET_OWNER_METHOD,
                    PLAN_SERVICE,
                    (ownerReply) => this.onOwnerInitial(ownerReply, flight, session),
                );
            } catch (error) {
                void error;
                this.activateDiag(flightState, "resolve", "resolve-throw");
                this.failActivation(flightState, "no-planner");
            }
            return;
        }
        if (reply !== false) {
            this.activateDiag(flightState, "presence", "presence-malformed");
            this.failActivation(flightState, "no-planner");
            return;
        }
        // Strictly absent name. With a previously pinned owner this is
        // confirmed absence: the old flight is terminal and a fresh session
        // recovery replaces it; the old command is never replayed. Without a
        // prior owner this is initial activation: proceed to one bounded start.
        if (this.knownOwner !== null) {
            const lost = flightState;
            this.activateDiag(lost, "presence", "name-loss");
            this.clearTimer();
            this.inFlight = false;
            this.pending = null;
            this.pinnedOwner = null;
            this.activationStep = 0;
            this.diag(lost.op, lost.correlation, lost.windowCount, "owner-absent");
            this.noteTerminalFor(lost);
            this.triggerRecovery("absent");
            return;
        }
        this.activateDiag(flightState, "presence", "absent");
        this.activationStep = 3;
        try {
            this.activateDiag(flightState, "start", "start-requested");
            this.env.callDbus(
                PLAN_DBUS_SERVICE,
                PLAN_DBUS_OBJECT,
                PLAN_DBUS_INTERFACE,
                PLAN_START_METHOD,
                PLAN_SERVICE,
                (startReply) => this.onStartResult(startReply, flight, session),
            );
        } catch (error) {
            void error;
            this.activateDiag(flightState, "start", "start-throw");
            this.failActivation(flightState, "no-planner");
        }
    }

    private onOwnerInitial(reply: unknown, flight: number, session: number): void {
        if (!this.inFlight || flight !== this.activeToken || session !== this.plannerSession || this.activationStep !== 2) {
            return;
        }
        const flightState = this.pending;
        if (flightState === null || flightState.plannerSession !== session) {
            return;
        }
        if (!isUniqueOwner(reply)) {
            this.activateDiag(flightState, "resolve", "owner-malformed");
            this.failActivation(flightState, "no-planner");
            return;
        }
        // A changed unique owner after a previously pinned Planner is
        // confirmed identity evidence: terminal old flight plus fresh-session
        // recovery, never replaying the old command.
        if (this.knownOwner !== null && reply !== this.knownOwner) {
            const lost = flightState;
            this.activateDiag(lost, "resolve", "owner-changed");
            this.clearTimer();
            this.inFlight = false;
            this.pending = null;
            this.pinnedOwner = null;
            this.activationStep = 0;
            this.diag(lost.op, lost.correlation, lost.windowCount, "owner-changed");
            this.noteTerminalFor(lost);
            this.triggerRecovery("changed");
            return;
        }
        this.pinnedOwner = reply;
        this.knownOwner = reply;
        this.activateDiag(flightState, "resolve", "owner-pinned");
        this.activationStep = 5;
        this.sendPlannerRequest(flight, session);
    }

    private onStartResult(reply: unknown, flight: number, session: number): void {
        if (!this.inFlight || flight !== this.activeToken || session !== this.plannerSession || this.activationStep !== 3) {
            return;
        }
        const flightState = this.pending;
        if (flightState === null || flightState.plannerSession !== session) {
            return;
        }
        if (reply !== PLAN_START_PRIMARY && reply !== PLAN_START_ALREADY) {
            this.activateDiag(
                flightState,
                "start-result",
                typeof reply === "number" ? "start-refused" : "start-malformed",
            );
            this.failActivation(flightState, "no-planner");
            return;
        }
        this.activateDiag(
            flightState,
            "start-result",
            reply === PLAN_START_PRIMARY ? "start-primary" : "start-already",
        );
        this.activationStep = 4;
        try {
            this.activateDiag(flightState, "resolve", "resolve-requested");
            this.env.callDbus(
                PLAN_DBUS_SERVICE,
                PLAN_DBUS_OBJECT,
                PLAN_DBUS_INTERFACE,
                PLAN_GET_OWNER_METHOD,
                PLAN_SERVICE,
                (ownerReply) => this.onOwnerAfterStart(ownerReply, flight, session),
            );
        } catch (error) {
            void error;
            this.activateDiag(flightState, "resolve", "resolve-throw");
            this.failActivation(flightState, "no-planner");
        }
    }

    private onOwnerAfterStart(reply: unknown, flight: number, session: number): void {
        if (!this.inFlight || flight !== this.activeToken || session !== this.plannerSession || this.activationStep !== 4) {
            return;
        }
        const flightState = this.pending;
        if (flightState === null || flightState.plannerSession !== session) {
            return;
        }
        if (!isUniqueOwner(reply)) {
            this.activateDiag(flightState, "resolve", "owner-malformed");
            this.failActivation(flightState, "no-planner");
            return;
        }
        this.pinnedOwner = reply;
        if (this.knownOwner === null) {
            this.knownOwner = reply;
        }
        this.activateDiag(flightState, "resolve", "owner-pinned");
        this.activationStep = 5;
        this.sendPlannerRequest(flight, session);
    }

    private sendPlannerRequest(flight: number, session: number): void {
        if (!this.inFlight || flight !== this.activeToken || session !== this.plannerSession || this.activationStep !== 5) {
            return;
        }
        const flightState = this.pending;
        if (flightState === null || flightState.plannerSession !== session || !isUniqueOwner(this.pinnedOwner)) {
            return;
        }
        this.callbackSeen = false;
        try {
            const target = this.pinnedOwner as string;
            this.activateDiag(flightState, "send", "send-requested");
            this.env.callDbus(
                target,
                PLAN_OBJECT,
                PLAN_INTERFACE,
                PLAN_METHOD,
                flightState.requestPayload,
                (reply) => this.onRequestReply(reply, flight, session),
            );
            this.activateDiag(flightState, "send", "request-sent");
        } catch (error) {
            void error;
            this.activateDiag(flightState, "send", "send-throw");
            this.failFlight(flightState, "owner-loss");
        }
    }

    private failActivation(flightState: PendingFlight, outcome: string): void {
        if (flightState.plannerSession !== this.plannerSession) {
            return;
        }
        if (flightState.op === "release-domain") {
            this.settleRelease(flightState.correlation, outcome);
        }
        this.clearTimer();
        this.inFlight = false;
        this.pending = null;
        this.pinnedOwner = null;
        this.activationStep = 0;
        this.diag(flightState.op, flightState.correlation, flightState.windowCount, outcome);
        this.noteTerminalFor(flightState);
        // Activation never sent the command: a pointer joins its marker
        // (persisting for a later free moment, with exactly one reconcile
        // attempt overall); a marker attempt binds its exact failure naming
        // the allocated plan correlation.
        const dragPointer = this.dragSourceOf(flightState);
        if (dragPointer !== null) {
            this.noteDragRejected(
                dragPointer,
                outcome,
                flightState.pointerSource,
                flightState.snapshot.domainOutput,
                flightState.snapshot.domainWorkspace,
            );
        } else {
            this.failDragRestore(flightState, outcome);
        }
        this.finishFlight();
    }

    private noteTerminalFor(flightState: PendingFlight): void {
        if (flightState.background === true) {
            this.noteBackgroundTerminal(flightState.snapshot);
        } else {
            this.noteReconcileTerminal(flightState.op, flightState.workAreaReprojection);
        }
    }

    private maybeProbeAfterTerminal(lost: PendingFlight): void {
        if (!this.enabled) {
            return;
        }
        if (lost.isRecovery) {
            return;
        }
        if (lost.plannerSession !== this.plannerSession) {
            return;
        }
        if (this.inFlight) {
            return;
        }
        if (this.knownOwner === null) {
            return;
        }
        // One bounded identity probe only when the flight is otherwise
        // terminal. Timeout, malformed, service fault, missing callback, and
        // correlation mismatch alone never recover; only a probe result
        // proving absence (strict false) or a changed unique owner triggers
        // recovery. One bounded deadline covers both probe callbacks: silence
        // clears only the probe and resumes the pump without recovery or
        // topology reset. No retry, polling, or systemd behavior.
        this.clearProbeTimer();
        this.probeToken += 1;
        const probe = this.probeToken;
        this.activeProbe = probe;
        const session = this.plannerSession;
        const expectedOwner = this.knownOwner;
        const correlation = lost.correlation;
        try {
            this.probeCancel = this.env.scheduleOnce(PLAN_TIMEOUT_MS, () => this.onProbeTimeout(probe, session, correlation));
        } catch (error) {
            void error;
            this.probeCancel = null;
            this.activeProbe = 0;
            return;
        }
        try {
            this.env.callDbus(
                PLAN_DBUS_SERVICE,
                PLAN_DBUS_OBJECT,
                PLAN_DBUS_INTERFACE,
                PLAN_HAS_OWNER_METHOD,
                PLAN_SERVICE,
                (reply) => this.onProbePresence(reply, probe, session, expectedOwner),
            );
        } catch (error) {
            void error;
            this.clearProbeTimer();
            this.activeProbe = 0;
        }
    }

    private onProbeTimeout(probe: number, session: number, correlation: string): void {
        if (probe !== this.activeProbe || session !== this.plannerSession) {
            return;
        }
        if (this.inFlight) {
            this.clearProbeTimer();
            this.activeProbe = 0;
            this.finishFlight();
            return;
        }
        // Silence is not confirmed loss: retain Planner/Engine topology and
        // resume deferred/hidden/marker pumping. Late callbacks stay fenced
        // by the cleared probe token below.
        this.clearProbeTimer();
        this.activeProbe = 0;
        this.logToken(`${LOG_PREFIX}:probe-timeout correlation=${correlation} cause=probe-silence outcome=pump-resumed`);
        this.finishFlight();
    }

    private onProbePresence(reply: unknown, probe: number, session: number, expectedOwner: string | null): void {
        if (probe !== this.activeProbe || session !== this.plannerSession) {
            return;
        }
        if (this.inFlight) {
            this.clearProbeTimer();
            this.activeProbe = 0;
            this.finishFlight();
            return;
        }
        if (reply === false) {
            this.triggerRecovery("absent");
            return;
        }
        if (reply !== true) {
            this.clearProbeTimer();
            this.activeProbe = 0;
            this.finishFlight();
            return;
        }
        try {
            this.env.callDbus(
                PLAN_DBUS_SERVICE,
                PLAN_DBUS_OBJECT,
                PLAN_DBUS_INTERFACE,
                PLAN_GET_OWNER_METHOD,
                PLAN_SERVICE,
                (ownerReply) => this.onProbeOwner(ownerReply, probe, session, expectedOwner),
            );
        } catch (error) {
            void error;
            this.clearProbeTimer();
            this.activeProbe = 0;
            this.finishFlight();
        }
    }

    private onProbeOwner(reply: unknown, probe: number, session: number, expectedOwner: string | null): void {
        if (probe !== this.activeProbe || session !== this.plannerSession) {
            return;
        }
        if (this.inFlight) {
            this.clearProbeTimer();
            this.activeProbe = 0;
            this.finishFlight();
            return;
        }
        if (!isUniqueOwner(reply)) {
            this.clearProbeTimer();
            this.activeProbe = 0;
            this.finishFlight();
            return;
        }
        if (expectedOwner === null) {
            this.clearProbeTimer();
            this.activeProbe = 0;
            this.finishFlight();
            return;
        }
        if (reply !== expectedOwner) {
            this.triggerRecovery("changed");
            return;
        }
        this.clearProbeTimer();
        this.activeProbe = 0;
        this.finishFlight();
    }

    private triggerRecovery(reason: string): void {
        if (!this.enabled || this.inFlight) {
            return;
        }
        // Old flight is already terminal here; late old-generation callbacks
        // are fenced by the session bump below. Clear KWin lifecycle
        // evidence so CURRENT eligible windows form a fresh session through
        // the existing fresh-observation route (Rust near-strip fitting with normal
        // tiling when no fit applies). Never replay the old command. While a workspace send is
        // active, uncertain (plan-blocked), or otherwise blocking Plan, this
        // edge stays unavailable. A failed fresh activation stays
        // bounded/terminal without loops via the isRecovery fence.
        this.plannerSession += 1;
        this.epoch += 1;
        this.pinnedOwner = null;
        this.activationStep = 0;
        this.pending = null;
        this.knownOwner = null;
        this.activeProbe = 0;
        this.clearProbeTimer();
        this.appliedById.clear();
        this.appliedScopeByDomain.clear();
        this.seenNonFullscreen.clear();
        this.heldInitialFullscreen.clear();
        // Q3: settled intent membership is deliberately NOT cleared or
        // re-read here. The store outlives planner restarts; recovery
        // replays current eligible windows through the fresh session and
        // hydrated floats keep observing floating, so no rehydration may
        // wipe local intent.
        this.fixedClients.clear();
        this.reconcileAttempts = 0;
        this.backgroundAttempts.clear();
        this.pointerEcho = null;
        this.absorbDeferredDragIntent(this.deferredAuto, false);
        this.deferredAuto = null;
        this.logToken(`${LOG_PREFIX}:recovery reason=${reason} outcome=confirmed-loss`);
        // Recovery tears down every flight without terminals: a dispatched
        // marker attempt never settled, so every marker gets one fresh
        // attempt afterwards instead of sticking consumed. Drags themselves
        // are preserved, never silently discarded.
        for (const marker of this.dragRestore.values()) {
            marker.dispatched = false;
        }
        this.dragPreview.clear();
        this.nextIsRecovery = true;
        try {
            this.refreshNow();
        } catch (error) {
            void error;
            this.nextIsRecovery = false;
        }
        if (!this.inFlight) {
            this.nextIsRecovery = false;
            this.logToken(`${LOG_PREFIX}:recovery reason=${reason} outcome=no-fresh-observation`);
        }
    }

    private onTimeout(flight: number, session: number): void {
        if (!this.inFlight || flight !== this.activeToken || session !== this.plannerSession) {
            return;
        }
        // A late arrival-timer fire while a lean R4 arrival wait holds the
        // flight belongs to the arrival deadline, not the request deadline.
        if (this.r4Flight !== null) {
            this.onR4ArrivalTimeout(flight, session);
            return;
        }
        const lost = this.pending;
        if (lost !== null && this.activationStep >= 1 && this.activationStep <= 4) {
            const phase =
                this.activationStep === 1
                    ? "presence"
                    : this.activationStep === 2
                      ? "resolve"
                      : this.activationStep === 3
                        ? "start"
                        : "start-resolve";
            this.activateDiag(lost, "timeout", "timeout", phase);
        }
        // Unanswered-request deadline: no planner reply arrived. There is no
        // ack/verify/cancel/status protocol and no retry; a late reply is
        // ignored by flight/session fencing. R4-shape flights force both
        // domains through the existing settlement chain below.
        // Ordinary reply-wait timeout only: the Planner send completed
        // (activationStep 5) and the reply never arrived. Activation-phase
        // timeouts (steps 1-4, 0) stay exclusively with the existing activate
        // diagnostics above and never emit an ordinary terminal.
        if (lost !== null && this.activationStep === 5) {
            this.ordinaryTerminal(lost, null, "timeout", "reply");
        }
        this.clearTimer();
        this.inFlight = false;
        this.pending = null;
        this.pinnedOwner = null;
        this.activationStep = 0;
        if (lost !== null) {
            this.diag(lost.op, lost.correlation, lost.windowCount, "timeout");
            if (lost.op === "release-domain") {
                this.settleRelease(lost.correlation, "timeout");
            }
            if (lost.background === true) {
                this.noteBackgroundTerminal(lost.snapshot);
            } else {
                this.noteReconcileTerminal(lost.op, lost.workAreaReprojection);
            }
            this.settleDragTerminal(lost, "timeout");
            this.maybeProbeAfterTerminal(lost);
            this.forceR4SettleFromPending(lost);
            this.finishFlight();
            return;
        }
        this.finishFlight();
    }

    private onRequestReply(reply: unknown, flight: number, session: number): void {
        if (!this.inFlight || flight !== this.activeToken || session !== this.plannerSession || this.callbackSeen) {
            return;
        }
        // A live lean R4 arrival wait owns the single-flight. A duplicate
        // `planned` must never restart native transfer or reset timers.
        if (this.r4Flight !== null) {
            return;
        }
        const flightState = this.pending;
        if (flightState === null || flightState.plannerSession !== session || this.activationStep !== 5) {
            return;
        }
        this.callbackSeen = true;
        this.clearTimer();
        this.lifecycleDiag(flightState, "reply", "reply", "received", "-");
        if (typeof reply !== "string" || reply.length > PLAN_MAX_REPLY_BYTES) {
            this.lifecycleDiag(flightState, "reply", "validate", "malformed", "service-fault");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.failFlight(flightState, "service-fault");
            return;
        }
        let parsed: unknown = null;
        try {
            parsed = JSON.parse(reply);
        } catch (error) {
            void error;
            this.lifecycleDiag(flightState, "reply", "validate", "malformed", "service-fault");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.failFlight(flightState, "service-fault");
            return;
        }
        if (!isRecord(parsed)) {
            this.lifecycleDiag(flightState, "reply", "validate", "malformed", "service-fault");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.failFlight(flightState, "service-fault");
            return;
        }
        if (parsed["v"] !== PLAN_CONTRACT_VERSION) {
            this.lifecycleDiag(flightState, "reply", "validate", "malformed", "service-fault");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.failFlight(flightState, "service-fault");
            return;
        }
        if (parsed["correlation_id"] !== flightState.correlation) {
            this.lifecycleDiag(flightState, "reply", "validate", "stale", "correlation-mismatch");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.failFlight(flightState, "correlation-mismatch");
            return;
        }
        const outcome = parsed["outcome"];
        if (outcome === "released") {
            if (flightState.op !== "release-domain") {
                this.lifecycleDiag(flightState, "reply", "validate", "malformed", "service-fault");
                this.ordinaryTerminal(flightState, null, "uncertain", "validate");
                this.failFlight(flightState, "service-fault");
                return;
            }
            if (isRecord(parsed)) {
                this.handleReleaseReply(parsed, flightState);
            } else {
                this.lifecycleDiag(flightState, "reply", "validate", "malformed", "service-fault");
                this.ordinaryTerminal(flightState, null, "uncertain", "validate");
                this.failFlight(flightState, "service-fault");
            }
            return;
        }
        if (outcome === "diverged") {
            const kind = sanitizeKind(parsed["kind"]);
            this.lifecycleDiag(flightState, "reply", "validate", "rejected", kind);
            this.ordinaryTerminal(flightState, null, "rejected", "validate");
            if (flightState.op === "release-domain") {
                this.settleRelease(flightState.correlation, sanitizeKind(parsed["kind"]));
            }
            this.failFlight(flightState, sanitizeKind(parsed["kind"]));
            return;
        }
        if (outcome === "rejected") {
            const kind = sanitizeKind(parsed["kind"]);
            const detail = sanitizeDetail(parsed["detail"]);
            this.lifecycleDiag(flightState, "reply", "validate", "rejected", kind);
            this.ordinaryTerminal(flightState, null, "rejected", "validate");
            if (isUniqueOwner(this.pinnedOwner) && this.knownOwner === null) {
                this.knownOwner = this.pinnedOwner;
            }
            this.inFlight = false;
            this.pending = null;
            this.pinnedOwner = null;
            this.activationStep = 0;
            this.diag(flightState.op, flightState.correlation, flightState.windowCount, "rejected");
            this.rejectKind(kind, detail);
            if (flightState.op === "release-domain") {
                this.settleRelease(flightState.correlation, "rejected");
            }
            // Core partial-observation diagnostics: log the retained vs
            // observed membership skew (counts only, no gate change) so a
            // floating/exception drift is attributable without guessing.
            if (kind === "partial-observation") {
                this.logCoverSkew(flightState, null, "partial-observation");
            }
            if (flightState.background === true) {
                this.noteBackgroundTerminal(flightState.snapshot);
            } else {
                this.noteAutoReconcileTerminal(flightState);
            }
            this.settleDragTerminal(flightState, kind, "rejected");
            // R4-shape rejections force both domains even on equal evidence.
            this.forceR4SettleFromPending(flightState);
            // Correlated gap-mismatch retry: an automatic reconcile refused
            // solely for inner/outer gaps re-observes the same domain fresh
            // (foreground observation, or same hidden-domain observation for
            // background flights) and dispatches at most one update-gaps
            // through the ordinary single-flight below. Never from update-gaps,
            // diverged, owner/stale/uncertain/malformed, cross-domain
            // evidence, marker/pointer flights, or a superseded slot: an
            // update-gaps rejection never retries, and a queued intent wins
            // so its own flight governs the next attempt.
            if (
                kind === "domain-mismatch" &&
                isGapMismatchMessage(parsed["message"]) &&
                flightState.op === "reconcile" &&
                flightState.workAreaReprojection !== true &&
                (flightState.restoreMarker ?? null) === null &&
                this.dragSourceOf(flightState) === null &&
                this.deferredAuto === null
            ) {
                const isHiddenRetry = flightState.background === true;
                const retryObserved = isHiddenRetry
                    ? this.freshHiddenFor(flightState.snapshot)
                    : this.freshObserved();
                if (retryObserved !== null) {
                    const retrySnapshot = this.carriedSnapshot(retryObserved);
                    if (this.domainKey(snapshotOf(retryObserved)) === this.domainKey(flightState.snapshot)) {
                        this.logToken(
                            `${LOG_PREFIX}:gap-reprojection selected=retry inner=${String(flightState.snapshot.domainGap)}->${String(retrySnapshot.domainGap)} outer=${String(flightState.snapshot.domainOuterGap)}->${String(retrySnapshot.domainOuterGap)}`,
                        );
                        this.pointerEcho = null;
                        this.deferredAuto = {
                            op: "update-gaps",
                            snapshot: retrySnapshot,
                            removed: null,
                            body: { op: "update-gaps" },
                            admissionMaximizeClears: flightState.admissionMaximizeClears,
                            ...(isHiddenRetry ? { background: true as const } : {}),
                        };
                    }
                }
            }
            this.finishFlight();
            return;
        }
        if (outcome !== "planned") {
            this.lifecycleDiag(flightState, "reply", "validate", "malformed", "service-fault");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.failFlight(flightState, "service-fault");
            return;
        }
        // D23 planner delivery: the converged request forgot the carried
        // close slots (or retired the emptied domain). Clears even ahead of
        // a stale drop below: the planner consumed the close, so any
        // redelivery would only repeat an idempotent no-op. Every other
        // outcome (rejected/diverged/malformed/timeout/transport) retains.
        if (flightState.closedSent.length > 0) {
            this.clearClosedSent(
                flightState.closedSent,
                flightState.snapshot.domainOutput,
                flightState.snapshot.domainWorkspace,
            );
        }
        // Fence stale replies: a newer observation arrived after dispatch.
        if (flightState.epoch !== this.epoch) {
            this.lifecycleDiag(flightState, "reply", "validate", "stale", "stale-dropped");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            if (isUniqueOwner(this.pinnedOwner) && this.knownOwner === null) {
                this.knownOwner = this.pinnedOwner;
            }
            this.inFlight = false;
            this.pending = null;
            this.pinnedOwner = null;
            this.activationStep = 0;
            this.diag(flightState.op, flightState.correlation, flightState.windowCount, "stale-dropped");
            if (flightState.background === true) {
                this.noteBackgroundTerminal(flightState.snapshot);
            } else {
                this.noteAutoReconcileTerminal(flightState);
            }
            this.settleDragTerminal(flightState, "stale-dropped");
            // R4-shape stale replies force both domains even on equal evidence.
            this.forceR4SettleFromPending(flightState);
            this.finishFlight();
            return;
        }
        const planned = validatePlanned(parsed, flightState.correlation);
        if (planned === null) {
            this.lifecycleDiag(flightState, "reply", "validate", "malformed", "precondition-mismatch");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.logCoverSkew(flightState, null, "malformed");
            this.failFlight(flightState, "precondition-mismatch");
            return;
        }
        if (!this.geometryCovers(planned, flightState)) {
            this.lifecycleDiag(flightState, "reply", "validate", "malformed", "precondition-mismatch", this.ordinaryRevision(planned, flightState));
            this.ordinaryTerminal(flightState, planned, "uncertain", "validate");
            this.logCoverSkew(flightState, planned, "cover-mismatch");
            this.failFlight(flightState, "precondition-mismatch");
            return;
        }
        // Toggle-orientation strict reply fence: the standard planned tiled
        // detail only (kind/capability toggle-orientation, policy_version 1),
        // full geometry with focus retained on the dispatched window, no
        // operation/preconditions and no float geometry. Wrong
        // kind/capability, malformed detail, a cross/float envelope, or
        // focus resolving to another tile never actuates.
        if (flightState.op === "toggle-orientation") {
            let focusWindow: string | null = null;
            if (planned.focus !== null) {
                for (const entry of planned.geometry) {
                    if (entry.leaf === planned.focus.leaf) {
                        focusWindow = entry.window;
                        break;
                    }
                }
            }
            if (
                !isToggleOrientationDetail(parsed["detail"]) ||
                planned.operation !== null ||
                planned.preconditions !== null ||
                planned.floatGeometry !== null ||
                focusWindow !== flightState.snapshot.focusedId
            ) {
                this.lifecycleDiag(flightState, "reply", "validate", "malformed", "precondition-mismatch", this.ordinaryRevision(planned, flightState));
                this.ordinaryTerminal(flightState, planned, "uncertain", "validate");
                this.logCoverSkew(flightState, planned, "cover-mismatch");
                this.failFlight(flightState, "precondition-mismatch");
                return;
            }
        }
        this.lifecycleDiag(flightState, "reply", "validate", "validated", "-", this.ordinaryRevision(planned, flightState));
        this.applyPlanned(planned, flightState);
    }

    // Known floating source derived from adapter observation (entry.ts:
    // floating = float-membership set OR onAllDesktops; sticky implies
    // onAllDesktops). Never a Rust derivation.
    private floatSourceOf(entry: { readonly floating: boolean; readonly sticky?: boolean }): string {
        if (entry.floating !== true) {
            return "none";
        }
        return entry.sticky === true ? "all-desktops" : "float-set";
    }

    private skewFlag(value: boolean | undefined): string {
        return value === undefined ? "unknown" : value ? "true" : "false";
    }

    // Membership-skew diagnostics (logging only, never a gate): one summary
    // line plus one bounded record per mismatched member using the existing
    // opaque window id convention, each with its observed
    // floating/sticky/fullscreen/maximized flags and known floating source.
    // Retained tiled membership comes from before-apply per-id applied
    // evidence on the same domain (never baseline authority, never core
    // state). For core partial-observation (no planned geometry) members
    // compare only against actual applied evidence; without it the log
    // reports retained=unknown and claims no precise cause. Counts only
    // otherwise; no rects, no raw native ids. Member records are bounded
    // by the built request byte cap.
    private logCoverSkew(flightState: PendingFlight, planned: PlannedReply | null, reason: string): void {
        try {
            interface SkewFlags {
                readonly floating: boolean | undefined;
                readonly sticky: boolean | undefined;
                readonly fullscreen: boolean | undefined;
                readonly maximized: boolean | undefined;
                readonly src: string;
            }
            const snapById = new Map<string, SkewFlags>();
            const wanted = new Set<string>();
            let floating = 0;
            let sticky = 0;
            let fullscreen = 0;
            let maximized = 0;
            for (const entry of flightState.snapshot.windows) {
                snapById.set(entry.id, {
                    floating: entry.floating,
                    sticky: entry.sticky,
                    fullscreen: entry.fullscreen,
                    maximized: entry.maximized,
                    src: this.floatSourceOf(entry),
                });
                if (entry.floating === true) {
                    floating += 1;
                }
                if (entry.sticky === true) {
                    sticky += 1;
                }
                if (entry.fullscreen) {
                    fullscreen += 1;
                }
                if (entry.maximized) {
                    maximized += 1;
                }
                if (entry.floating !== true || (flightState.floatTarget?.window === entry.id && flightState.floatTarget.floating === false)) {
                    wanted.add(entry.id);
                }
            }
            if (flightState.removed !== null) {
                wanted.delete(flightState.removed);
            }
            // Before-apply per-id evidence (never core state, never baseline
            // authority): retained tiled members homed to the flight domain,
            // plus sticky multi-homed members observed here with matching
            // sticky flags even when the shared slot was last written by
            // another domain.
            const hasApplied = this.hasAppliedEvidenceFor(flightState.snapshot);
            const retainedTiled = new Map<string, SkewFlags>();
            if (hasApplied) {
                for (const [id, evidence] of this.appliedById) {
                    if (
                        evidence.output === flightState.snapshot.domainOutput &&
                        evidence.workspace === flightState.snapshot.domainWorkspace &&
                        evidence.floating !== true
                    ) {
                        retainedTiled.set(id, {
                            floating: evidence.floating,
                            sticky: evidence.sticky,
                            fullscreen: evidence.fullscreen,
                            maximized: evidence.maximized,
                            src: this.floatSourceOf({ floating: evidence.floating, sticky: evidence.sticky }),
                        });
                    }
                }
                for (const entry of flightState.snapshot.windows) {
                    if (retainedTiled.has(entry.id) || entry.floating === true) {
                        continue;
                    }
                    const evidence = this.appliedById.get(entry.id);
                    if (
                        evidence !== undefined &&
                        evidence.floating === false &&
                        entry.sticky === true &&
                        evidence.sticky === true
                    ) {
                        retainedTiled.set(entry.id, {
                            floating: evidence.floating,
                            sticky: evidence.sticky,
                            fullscreen: evidence.fullscreen,
                            maximized: evidence.maximized,
                            src: this.floatSourceOf({ floating: evidence.floating, sticky: evidence.sticky }),
                        });
                    }
                }
            }
            // Missing/extra member ids with per-member flags. Cover-mismatch
            // compares wanted against the planned geometry with observed
            // (dispatch-snapshot) flags; without planned geometry the
            // comparison falls back to retained evidence when available.
            const missing: string[] = [];
            const extra: string[] = [];
            const flagOf = new Map<string, SkewFlags>();
            const unknownFlags: SkewFlags = {
                floating: undefined,
                sticky: undefined,
                fullscreen: undefined,
                maximized: undefined,
                src: "unknown",
            };
            let plannedText = "unknown";
            if (planned !== null) {
                const plannedIds = new Set<string>();
                for (const entry of planned.geometry) {
                    plannedIds.add(entry.window);
                }
                for (const id of wanted) {
                    if (!plannedIds.has(id)) {
                        missing.push(id);
                    }
                }
                for (const id of plannedIds) {
                    if (!wanted.has(id)) {
                        extra.push(id);
                    }
                }
                plannedText = String(planned.geometry.length);
                for (const id of missing) {
                    const flags = snapById.get(id);
                    if (flags !== undefined) {
                        flagOf.set(id, flags);
                    }
                }
                for (const id of extra) {
                    flagOf.set(id, snapById.get(id) ?? unknownFlags);
                }
            } else if (hasApplied) {
                for (const id of wanted) {
                    if (!retainedTiled.has(id)) {
                        missing.push(id);
                    }
                }
                for (const id of retainedTiled.keys()) {
                    if (!wanted.has(id)) {
                        extra.push(id);
                    }
                }
                for (const id of missing) {
                    const flags = snapById.get(id);
                    if (flags !== undefined) {
                        flagOf.set(id, flags);
                    }
                }
                for (const id of extra) {
                    const flags = retainedTiled.get(id);
                    if (flags !== undefined) {
                        flagOf.set(id, flags);
                    }
                }
            }
            missing.sort();
            extra.sort();
            const retainedIds = [...retainedTiled.keys()].sort();
            const hasReference = planned !== null || hasApplied;
            this.logToken(
                `${LOG_PREFIX}:membership-skew correlation=${flightState.correlation} op=${flightState.op} reason=${sanitizeKind(reason)} wanted=${String(wanted.size)} planned=${plannedText} missing=${hasReference ? String(missing.length) : "unknown"} extra=${hasReference ? String(extra.length) : "unknown"} floating=${String(floating)} sticky=${String(sticky)} fullscreen=${String(fullscreen)} maximized=${String(maximized)} retained=${hasApplied ? "known" : "unknown"} retained-wanted=${hasApplied ? String(retainedTiled.size) : "-"} retained-ids=${!hasApplied || retainedIds.length === 0 ? "-" : retainedIds.join(",")}`,
            );
            for (const id of missing) {
                const flags = flagOf.get(id);
                if (flags === undefined) {
                    continue;
                }
                this.logToken(
                    `${LOG_PREFIX}:membership-skew-member correlation=${flightState.correlation} window=${id} side=missing floating=${this.skewFlag(flags.floating)} sticky=${this.skewFlag(flags.sticky)} fullscreen=${this.skewFlag(flags.fullscreen)} maximized=${this.skewFlag(flags.maximized)} float-src=${flags.src}`,
                );
            }
            for (const id of extra) {
                const flags = flagOf.get(id);
                if (flags === undefined) {
                    continue;
                }
                this.logToken(
                    `${LOG_PREFIX}:membership-skew-member correlation=${flightState.correlation} window=${id} side=extra floating=${this.skewFlag(flags.floating)} sticky=${this.skewFlag(flags.sticky)} fullscreen=${this.skewFlag(flags.fullscreen)} maximized=${this.skewFlag(flags.maximized)} float-src=${flags.src}`,
                );
            }
        } catch (error) {
            void error;
        }
    }

    // Complete-reply binding: the reply geometry must cover exactly the
    // request window set (admit/move/focus/resize) or exactly the survivors
    // (remove). Unknown or partial windows never reach native writes. The
    // wanted set is derived from the primitive dispatch snapshot only.
    // Directional focus/move may alternatively carry a source-only local
    // plan on a two-domain snapshot: then the geometry must cover exactly
    // the source-domain wanted subset (the target is untouched).
    private geometryCovers(planned: PlannedReply, flightState: PendingFlight): boolean {
        const wanted = new Set<string>();
        const wantedById = new Map<string, PlanSnapshotWindow>();
        for (const entry of flightState.snapshot.windows) {
            if (entry.floating !== true || (flightState.floatTarget?.window === entry.id && flightState.floatTarget.floating === false)) {
                wanted.add(entry.id);
                wantedById.set(entry.id, entry);
            }
        }
        if (flightState.removed !== null) {
            wanted.delete(flightState.removed);
            wantedById.delete(flightState.removed);
        }
        if (flightState.op === "toggle-float") {
            const target = flightState.floatTarget;
            if (target === null || (target.floating && (planned.floatGeometry === null || planned.floatGeometry.window !== target.window)) || (!target.floating && planned.floatGeometry !== null)) {
                return false;
            }
        }
        // Every planned entry must be wanted. Directional two-domain flights
        // additionally require homing to one of the snapshot domains so a
        // stale target substitution cannot pass; legacy single-domain flows
        // keep the historical id-only binding unchanged.
        const directionalHoming = flightState.snapshot.domains;
        let homedPairs: Set<string> | null = null;
        if (directionalHoming !== undefined) {
            homedPairs = new Set<string>();
            for (const entry of directionalHoming) {
                homedPairs.add(`${entry.output}\u0000${entry.workspace}`);
            }
        }
        for (const entry of planned.geometry) {
            if (!wanted.has(entry.window)) {
                return false;
            }
            if (homedPairs !== null && !homedPairs.has(`${entry.output}\u0000${entry.workspace}`)) {
                return false;
            }
        }
        if (planned.geometry.length === wanted.size) {
            return true;
        }
        // Directional local plan on a two-domain snapshot: exactly the
        // source-domain wanted subset, all homed to the source domain.
        const domains = flightState.snapshot.domains;
        if (
            (flightState.op === "focus" || flightState.op === "move") &&
            domains !== undefined &&
            domains.length === 2
        ) {
            const sourceOutput = flightState.snapshot.domainOutput;
            const sourceWorkspace = flightState.snapshot.domainWorkspace;
            let sourceWanted = 0;
            for (const entry of wantedById.values()) {
                if (entry.output === sourceOutput && entry.workspace === sourceWorkspace) {
                    sourceWanted += 1;
                }
            }
            if (planned.geometry.length !== sourceWanted || sourceWanted === 0) {
                return false;
            }
            for (const entry of planned.geometry) {
                if (entry.output !== sourceOutput || entry.workspace !== sourceWorkspace) {
                    return false;
                }
            }
            // A local plan must not carry a cross focus operation.
            if (planned.operation !== null) {
                return false;
            }
            // A local plan's focus (when present) must stay in the source.
            if (
                planned.focus !== null &&
                (planned.focus.domainOutput !== sourceOutput ||
                    planned.focus.domainWorkspace !== sourceWorkspace)
            ) {
                return false;
            }
            return true;
        }
        return false;
    }

    // Called only before flight setters. Keep scope/owner fences; a second
    // stale reply follows the ordinary terminal and convergence path.
    private maybeReplanStalePrewrite(flightState: PendingFlight, freshSnapshot: PlanSnapshot): boolean {
        if (flightState.replanned) {
            return false;
        }
        if (!this.enabled || this.r4Flight !== null) {
            return false;
        }
        if (!this.inFlight || this.pending !== flightState || flightState.plannerSession !== this.plannerSession) {
            return false;
        }
        if (!isUniqueOwner(this.pinnedOwner) || !isGeneration(this.generation)) {
            return false;
        }
        if (!sameReprojectionScope(freshSnapshot, flightState.snapshot)) {
            return false;
        }
        const oldCorrelation = flightState.correlation;
        const op = flightState.op;
        this.diag(op, oldCorrelation, flightState.windowCount, "stale-scope");
        this.clearTimer();
        this.inFlight = false;
        this.pending = null;
        this.pinnedOwner = null;
        this.activationStep = 0;
        this.dispatch({
            op: flightState.op,
            snapshot: freshSnapshot,
            removed: flightState.removed,
            body: flightState.body,
            pointerSource: flightState.pointerSource,
            dragSource: flightState.dragSource ?? null,
            restoreMarker: flightState.restoreMarker ?? null,
            restoreMarkerSeq: flightState.restoreMarkerSeq ?? null,
            workAreaReprojection: flightState.workAreaReprojection,
            admissionMaximizeClears: flightState.admissionMaximizeClears,
            floatTarget: flightState.floatTarget,
            stickyTarget: flightState.stickyTarget,
            // A replanned retry carries the same explicit user intent,
            // including its staged lifecycle change; only application
            // commits it.
            fixedStage: flightState.fixedStage,
            background: flightState.background,
            direction: flightState.direction,
            replanned: true,
        });
        const next = this.pending as PendingFlight | null;
        if (next !== null && next.replanned === true && next.correlation !== oldCorrelation) {
            this.logToken(
                `${LOG_PREFIX}:stale-replan correlation=${oldCorrelation} op=${op} cause=stale-scope recovery=replan-once next=${next.correlation}`,
            );
        } else {
            this.logToken(
                `${LOG_PREFIX}:stale-replan correlation=${oldCorrelation} op=${op} cause=stale-scope recovery=dispatch-refused`,
            );
        }
        this.finishFlight();
        return true;
    }

    // Reply-boundary revalidation: never touch a possibly-destroyed Window
    // observed before dispatch. Re-observe synchronously, compare the captured
    // primitive snapshot, and resolve all geometry/focus targets only from the
    // fresh observation.
    private applyPlanned(planned: PlannedReply, flightState: PendingFlight): void {
        // A domain that turned floating after dispatch gets zero native writes.
        if (flightState.op !== "release-domain") {
            const domains = flightState.snapshot.domains;
            let floating = false;
            if (domains !== undefined && domains.length === 2) {
                for (const entry of domains) {
                    if (!this.isTiledDomain(entry.output, entry.workspace)) {
                        floating = true;
                        break;
                    }
                }
            } else if (!this.isTiledDomain(flightState.snapshot.domainOutput, flightState.snapshot.domainWorkspace)) {
                floating = true;
            }
            if (floating) {
                this.lifecycleDiag(flightState, "apply", "setters", "dropped", "workspace-floating", this.ordinaryRevision(planned, flightState));
                this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                this.failFlight(flightState, "workspace-floating");
                return;
            }
        }
        // An already-in-flight reconcile reply landing mid-drag never
        // actuates, on either output. Foreground and hidden share this fence;
        // work-area reprojection stays exempt. Cancellation re-arms restore
        // markers before releasing the shared flight slot.
        if (
            flightState.op === "reconcile" &&
            flightState.workAreaReprojection !== true &&
            this.interactiveResizeActive()
        ) {
            this.ordinaryTerminal(flightState, planned, "uncertain", "validate");
            this.discardInteractiveReconcile();
            this.finishFlight();
            return;
        }
        // Directional focus/move flights re-observe directionally (source
        // plus the same adjacent target) so cross targets resolve from fresh
        // evidence and stale substitutions fail closed before any write.
        // Reply-boundary re-observation resolves targets from the flight's own
        // domain only, so hidden-domain geometry is never applied to
        // foreground refs and vice versa.
        const directionalFlight =
            flightState.background !== true &&
            (flightState.op === "focus" || flightState.op === "move") &&
            flightState.snapshot.domains !== undefined &&
            flightState.snapshot.domains.length === 2 &&
            flightState.direction !== null;
        const fresh = directionalFlight
            ? this.freshDirectionalForFlight(flightState)
            : flightState.background === true
              ? this.freshHiddenFor(flightState.snapshot)
              : this.freshObserved();
        if (fresh === null) {
            // D23 close cleanup: a synthesized empty-domain flight (no live
            // members, empty plan, carrying closes) has no native evidence
            // to fence: the planner already converged and retired the slot,
            // and there are no refs to resolve or writes to order. Apply the
            // empty plan directly so the delivered close clears instead of
            // failing stale-scope forever. Every other null observation stays
            // fail-closed exactly as before.
            if (
                flightState.op === "reconcile" &&
                flightState.background !== true &&
                flightState.snapshot.windows.length === 0 &&
                planned.geometry.length === 0 &&
                flightState.closedSent.length > 0
            ) {
                this.lifecycleDiag(flightState, "observe", "observe", "matched", "-", this.ordinaryRevision(planned, flightState));
                this.writeGeometries(planned, flightState, {
                    domainOutput: flightState.snapshot.domainOutput,
                    domainWorkspace: flightState.snapshot.domainWorkspace,
                    domainBounds: { ...flightState.snapshot.domainBounds },
                    domainGap: flightState.snapshot.domainGap,
                    domainOuterGap: flightState.snapshot.domainOuterGap,
                    focusedId: "",
                    windows: Object.freeze([]),
                    activeRef: {},
                    fingerprint: flightState.snapshot.fingerprint,
                    revalidate: () => false,
                });
                return;
            }
            this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
            this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
            this.failFlight(flightState, "stale-scope");
            return;
        }
        if (flightState.workAreaReprojection) {
            const freshSnapshot = this.carriedSnapshot(fresh);
            // Reply-boundary revalidation is flag-exact: a floating/sticky
            // flip mid-flight must fail closed even when the blind structural
            // comparators still match. Baseline-diff skew detection runs
            // pre-dispatch and is unaffected.
            if (
                !sameReprojectionScope(freshSnapshot, flightState.snapshot) ||
                unexpectedFloatingSkewed(flightState, freshSnapshot)
            ) {
                this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
                this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
                if (this.maybeReplanStalePrewrite(flightState, freshSnapshot)) {
                    return;
                }
                this.failFlight(flightState, "stale-scope");
                return;
            }
            this.lifecycleDiag(flightState, "observe", "observe", "matched", "-", this.ordinaryRevision(planned, flightState));
            this.writeGeometries(planned, flightState, fresh);
            return;
        }
        if (flightState.op === "pointer-resize") {
            const source = flightState.pointerSource;
            if (source === null) {
                this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
                this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
                this.failFlight(flightState, "stale-scope");
                return;
            }
            const freshSnapshot = this.carriedSnapshot(fresh);
            if (
                !rectsEqualExceptSource(freshSnapshot, flightState.snapshot, source) ||
                unexpectedFloatingSkewed(flightState, freshSnapshot)
            ) {
                this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
                this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
                if (this.maybeReplanStalePrewrite(flightState, freshSnapshot)) {
                    return;
                }
                this.failFlight(flightState, "stale-scope");
                return;
            }
            this.lifecycleDiag(flightState, "observe", "observe", "matched", "-", this.ordinaryRevision(planned, flightState));
            this.writeGeometries(planned, flightState, fresh);
            return;
        }
        if (flightState.op === "drag-drop") {
            const source = flightState.pointerSource;
            if (source === null) {
                this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
                this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
                this.failFlight(flightState, "stale-scope");
                return;
            }
            // Lag: mover projected onto the destination while native is still
            // source. Reuse the R4 transfer order plus arrival readback; a
            // miss falls through so the ordinary fence below decides.
            const lagReady = this.crossDragLagPrewrite(flightState, fresh);
            if (lagReady !== undefined) {
                this.applyCrossDragLagTransfer(planned, flightState, lagReady);
                return;
            }
            // The drop flight snapshot binds focus to the observed mover
            // (mirroring dispatch), so normalize fresh the same way before
            // the comparator: native activeWindow may lag the gesture, and
            // that lag must not read as reply-boundary staleness. All other
            // identity/scope/rect evidence stays exact.
            const freshForDrop = fresh.focusedId === source ? fresh : { ...fresh, focusedId: source };
            const freshSnapshot = this.carriedSnapshot(freshForDrop);
            // The drop window's moved frame may sit out of area (native drop
            // position) while every other member must match exactly: the
            // pointer-resize exception comparator pattern. Identity, scope,
            // and floating evidence stay exact, and a scope drift still gets
            // exactly one prewrite replan below.
            if (
                !rectsEqualExceptSource(freshSnapshot, flightState.snapshot, source) ||
                unexpectedFloatingSkewed(flightState, freshSnapshot)
            ) {
                this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
                this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
                if (this.maybeReplanStalePrewrite(flightState, freshSnapshot)) {
                    return;
                }
                this.failFlight(flightState, "stale-scope");
                return;
            }
            this.lifecycleDiag(flightState, "observe", "observe", "matched", "-", this.ordinaryRevision(planned, flightState));
            this.writeGeometries(planned, flightState, fresh);
            return;
        }
        if (flightState.op === "toggle-float") {
            const freshSnapshot = this.carriedSnapshot(fresh);
            // The native flip lands at write time, so any pre-apply skew is
            // external and must fail closed.
            if (
                !snapshotsEqual(freshSnapshot, flightState.snapshot) ||
                unexpectedFloatingSkewed(flightState, freshSnapshot)
            ) {
                this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
                this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
                if (this.maybeReplanStalePrewrite(flightState, freshSnapshot)) {
                    return;
                }
                this.failFlight(flightState, "stale-scope");
                return;
            }
            this.lifecycleDiag(flightState, "observe", "observe", "matched", "-", this.ordinaryRevision(planned, flightState));
            this.writeGeometries(planned, flightState, fresh);
            return;
        }
        if (flightState.removed === null) {
            const freshSnapshot = this.carriedSnapshot(fresh);
            // Admission-time maximize tolerance rides the clears list, not
            // the op: foreground auto reconciles carry the same
            // admissionMaximizeClears as the retired admit, so a cleared
            // window that re-maximizes before the reply still applies.
            const maximizeClears = flightState.admissionMaximizeClears;
            if (
                (!snapshotsEqual(freshSnapshot, flightState.snapshot) &&
                    !(
                        maximizeClears.length > 0 &&
                        snapshotsEqualAllowingAdmissionMaximize(
                            freshSnapshot,
                            flightState.snapshot,
                            maximizeClears,
                        )
                    )) ||
                unexpectedFloatingSkewed(flightState, freshSnapshot)
            ) {
                this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
                this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
                if (this.maybeReplanStalePrewrite(flightState, freshSnapshot)) {
                    return;
                }
                this.failFlight(flightState, "stale-scope");
                return;
            }
            this.lifecycleDiag(flightState, "observe", "observe", "matched", "-", this.ordinaryRevision(planned, flightState));
            this.writeGeometries(planned, flightState, fresh);
            return;
        }
        const freshSnapshot = this.carriedSnapshot(fresh);
        // Complete-observation convergence: the remove flight already carries
        // the current complete post-removal snapshot, so the reply boundary
        // revalidates exact post-membership equality. The reply geometry must
        // cover exactly the survivors via geometryCovers. A survivor
        // floating/sticky flip mid-flight fails closed the same way.
        if (
            !snapshotsEqual(freshSnapshot, flightState.snapshot) ||
            unexpectedFloatingSkewed(flightState, freshSnapshot)
        ) {
            this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
            this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
            if (this.maybeReplanStalePrewrite(flightState, freshSnapshot)) {
                return;
            }
            this.failFlight(flightState, "stale-scope");
            return;
        }
        this.lifecycleDiag(flightState, "observe", "observe", "matched", "-", this.ordinaryRevision(planned, flightState));
        this.writeGeometries(planned, flightState, fresh);
    }

    // Cross-output focus gate: exact operation fields, preconditions, and
    // target/source binding. Returns true only when every fence passes; any
    // mismatch is a precondition failure handled by the caller.
    private isCrossFocus(planned: PlannedReply, flightState: PendingFlight): boolean {
        if (flightState.op !== "focus" || flightState.background === true) {
            return false;
        }
        const domains = flightState.snapshot.domains;
        if (domains === undefined || domains.length !== 2) {
            return false;
        }
        const operation = planned.operation;
        const focus = planned.focus;
        if (operation === null || operation.op !== "focus" || focus === null || planned.preconditions === null) {
            return false;
        }
        const source = domains[0] as PlanDomain;
        const target = domains[1] as PlanDomain;
        if (
            operation.crossSourceOutput !== source.output ||
            operation.crossSourceWorkspace !== source.workspace ||
            operation.domainOutput !== target.output ||
            operation.domainWorkspace !== target.workspace ||
            focus.domainOutput !== target.output ||
            focus.domainWorkspace !== target.workspace
        ) {
            return false;
        }
        if (operation.fromWindow !== flightState.snapshot.focusedId) {
            return false;
        }
        if (flightState.direction === null || operation.direction !== flightState.direction) {
            return false;
        }
        if (operation.direction !== "left" && operation.direction !== "right") {
            return false;
        }
        if (operation.toLeaf !== focus.leaf || operation.toWindow.length === 0) {
            return false;
        }
        if (operation.route.length !== 1 || operation.route[0] !== operation.toLeaf) {
            return false;
        }
        // Float-origin flights (explicit `float_subject` in the retained
        // dispatch body, never inferred from observation flags alone) carry
        // the leafless floating-subject precondition and a null from_leaf;
        // tile flights carry the leaf-occupancy token and a leaf string.
        // The subject itself is verified against the retained flight
        // snapshot below: id-bound to the dispatched focus, homed on the
        // source domain, and floating/sticky there.
        const floatFlight = (flightState.body as Record<string, unknown>)["float_subject"] === true;
        if (floatFlight) {
            if (operation.fromLeaf !== null) {
                return false;
            }
            let subjectEligible = false;
            for (const entry of flightState.snapshot.windows) {
                if (
                    entry.id === operation.fromWindow &&
                    entry.id === flightState.snapshot.focusedId &&
                    entry.output === source.output &&
                    entry.workspace === source.workspace &&
                    (entry.floating === true || entry.sticky === true)
                ) {
                    subjectEligible = true;
                    break;
                }
            }
            if (!subjectEligible) {
                return false;
            }
        } else if (operation.fromLeaf === null) {
            return false;
        }
        const expected = floatFlight
            ? [
                  "focused-floating-window",
                  "target-leaf-occupied",
                  "focus-targets-adjacent-output",
                  "adapter-must-verify-postconditions",
              ]
            : [
                  "focused-leaf-occupied-by-focused-window",
                  "target-leaf-occupied",
                  "focus-targets-adjacent-output",
                  "adapter-must-verify-postconditions",
              ];
        const actual = planned.preconditions;
        if (actual.length !== expected.length) {
            return false;
        }
        for (let index = 0; index < expected.length; index += 1) {
            if (actual[index] !== expected[index]) {
                return false;
            }
        }
        // The focus leaf must resolve through the planned geometry to the
        // operation's target window.
        for (const entry of planned.geometry) {
            if (entry.leaf === focus.leaf && entry.window !== operation.toWindow) {
                return false;
            }
        }
        return true;
    }

    // Cross-output focus actuation: exactly one `setActive` on the resolved
    // target window, no geometry/layout/window membership writes. Every fence
    // refuses before the actuation; the existing acknowledgement/verification
    // already committed in Rust, and the adapter's epoch/scope fences hold.
    private applyCrossFocus(
        planned: PlannedReply,
        flightState: PendingFlight,
        current: PlanObserved,
    ): void {
        const operation = planned.operation;
        const focus = planned.focus;
        if (operation === null || operation.op !== "focus" || focus === null) {
            this.ordinaryTerminal(flightState, planned, "uncertain", "apply");
            this.failFlight(flightState, "precondition-mismatch");
            return;
        }
        const domains = flightState.snapshot.domains;
        if (domains === undefined || domains.length !== 2) {
            this.ordinaryTerminal(flightState, planned, "uncertain", "apply");
            this.failFlight(flightState, "precondition-mismatch");
            return;
        }
        const target = domains[1] as PlanDomain;
        // Resolve the target from the fresh cross-domain observation only.
        let targetRef: object | undefined;
        let targetEntry: PlanObservedWindow | undefined;
        for (const entry of current.windows) {
            if (
                entry.id === operation.toWindow &&
                entry.output === target.output &&
                entry.workspace === target.workspace
            ) {
                targetRef = entry.ref;
                targetEntry = entry;
                break;
            }
        }
        if (targetRef === undefined || targetEntry === undefined) {
            this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
            this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
            this.failFlight(flightState, "stale-scope");
            return;
        }
        // Exceptional targets never actuate: refuse before any write.
        if (
            targetEntry.fullscreen ||
            targetEntry.maximized ||
            targetEntry.floating === true ||
            targetEntry.sticky === true
        ) {
            this.ordinaryTerminal(flightState, planned, "uncertain", "apply");
            this.failFlight(flightState, "precondition-mismatch");
            return;
        }
        // The planned focus leaf must match a geometry entry for the target
        // window (no layout change, target is the remembered leaf).
        let leafMatches = false;
        for (const entry of planned.geometry) {
            if (entry.window === operation.toWindow && entry.leaf === focus.leaf) {
                leafMatches = true;
                break;
            }
        }
        if (!leafMatches) {
            this.ordinaryTerminal(flightState, planned, "uncertain", "apply");
            this.failFlight(flightState, "precondition-mismatch");
            return;
        }
        let currentActive: object | null = null;
        try {
            currentActive = this.env.active();
        } catch (error) {
            void error;
            currentActive = null;
        }
        if (currentActive !== targetRef) {
            let focused = false;
            try {
                focused = this.env.setActive(targetRef) === true;
            } catch (error) {
                void error;
                focused = false;
            }
            if (!focused) {
                this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "write-failed", this.ordinaryRevision(planned, flightState));
                this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                this.failFlight(flightState, "write-failed");
                return;
            }
        }
        if (isUniqueOwner(this.pinnedOwner)) {
            this.knownOwner = this.pinnedOwner;
        }
        this.inFlight = false;
        this.pending = null;
        this.pinnedOwner = null;
        this.activationStep = 0;
        this.ordinaryTerminal(flightState, planned, "applied", "apply");
        this.diag(flightState.op, flightState.correlation, flightState.windowCount, "planned-applied");
        this.finishFlight();
    }

    // Exact production R4 cross-output move gate: the mover, the captured
    // source, the adjacent target with its current workspace, the commanded
    // direction, the R4 rule, and the transfer capability. Returns true only
    // when every fence passes; local R1/R2/R3 plans never satisfy it.
    private isCrossMove(planned: PlannedReply, flightState: PendingFlight): boolean {
        if (flightState.op !== "move" || flightState.background === true) {
            return false;
        }
        const domains = flightState.snapshot.domains;
        if (domains === undefined || domains.length !== 2) {
            return false;
        }
        const operation = planned.operation;
        if (operation === null || operation.op !== "move") {
            return false;
        }
        const move = operation as PlanMoveOperation;
        if (move.rule !== "R4" || move.capability !== "CrossOutputTransfer") {
            return false;
        }
        if (!isDirection(move.direction)) {
            return false;
        }
        if (flightState.direction === null || move.direction !== flightState.direction) {
            return false;
        }
        const source = domains[0] as PlanDomain;
        const target = domains[1] as PlanDomain;
        if (
            move.sourceOutput !== source.output ||
            move.sourceWorkspace !== source.workspace ||
            move.targetOutput !== target.output ||
            move.targetWorkspace !== target.workspace
        ) {
            return false;
        }
        if (move.window !== flightState.snapshot.focusedId) {
            return false;
        }
        if (planned.preconditions === null || planned.preconditions.length !== 3) {
            return false;
        }
        const expected = [
            "focused-leaf-occupied-by-focused-window",
            "source-root-membership-and-adjacent-same-workspace-output",
            "adapter-must-verify-postconditions",
        ];
        for (let index = 0; index < expected.length; index += 1) {
            if (planned.preconditions[index] !== expected[index]) {
                return false;
            }
        }
        if (planned.baseRevision === null) {
            return false;
        }
        // Every desired entry must home to the source/target pair and the
        // mover must be desired on the target.
        let moverDesired = false;
        for (const entry of planned.geometry) {
            const onSource = entry.output === source.output && entry.workspace === source.workspace;
            const onTarget = entry.output === target.output && entry.workspace === target.workspace;
            if (!onSource && !onTarget) {
                return false;
            }
            if (entry.window === move.window && onTarget) {
                moverDesired = true;
            }
        }
        if (!moverDesired) {
            return false;
        }
        // The desired focus must follow the mover onto the target.
        if (
            planned.focus === null ||
            planned.focus.domainOutput !== target.output ||
            planned.focus.domainWorkspace !== target.workspace
        ) {
            return false;
        }
        return true;
    }

    // R4 immediate commit, no wire ack/verify/cancel/status. Called once per
    // R4 `planned` reply with the shared single-flight held: the dispatch
    // timer is already cleared. Order is fixed: sendClientToScreen with the
    // exact target Output object, mover desktops write with the exact target
    // VirtualDesktop refs, then planned geometries in canonical order
    // (overconstrained members skipped). Fences pinned owner, correlation,
    // flight/session token, and exact source/target scope before any native
    // write and before every setter; a stale snapshot discards before writes.
    // Structural scope rechecks tolerate the intended mover relocation
    // (source, half, or target placement) while rejecting unrelated stale
    // changes. After writes, one immediate fresh exact mover-on-target proof
    // follows once (a single setActive); delayed arrival waits on one-shot
    // output/desktops signals until a bounded arrival deadline. Every
    // terminal forces complete source AND target reconciliation through
    // `notifySendSettled`, including equal evidence. No geometry echo, no
    // settlement protocol, no queue, no replay. Logs are correlated and
    // redacted (no payloads or native ids).
    private beginR4Transfer(
        planned: PlannedReply,
        flightState: PendingFlight,
        current: PlanObserved,
    ): void {
        const operation = planned.operation as PlanMoveOperation;
        const domains = flightState.snapshot.domains as ReadonlyArray<PlanDomain>;
        const target = domains[1] as PlanDomain;
        const source = domains[0] as PlanDomain;
        const correlation = flightState.correlation;
        const windowCount = flightState.windowCount;
        if (!crossOutputTransferSupported(this.env)) {
            this.failFlight(flightState, "precondition-mismatch");
            return;
        }
        if (planned.baseRevision === null) {
            this.failFlight(flightState, "precondition-mismatch");
            return;
        }
        if (!isUniqueOwner(this.pinnedOwner)) {
            this.failFlight(flightState, "owner-loss");
            return;
        }
        if (flightState.correlation !== planned.correlationId) {
            this.failFlight(flightState, "correlation-mismatch");
            return;
        }
        if (!this.inFlight || this.pending !== flightState || flightState.plannerSession !== this.plannerSession) {
            this.failFlight(flightState, "stale-scope");
            return;
        }
        const baseRevision = planned.baseRevision;
        const byRef = new Map<string, object>();
        for (const entry of current.windows) {
            byRef.set(entry.id, entry.ref);
        }
        let moverEntry: PlanObservedWindow | undefined;
        for (const entry of current.windows) {
            if (entry.id === operation.window) {
                moverEntry = entry;
                break;
            }
        }
        if (
            moverEntry === undefined ||
            moverEntry.output !== operation.sourceOutput ||
            moverEntry.workspace !== operation.sourceWorkspace ||
            moverEntry.output !== source.output ||
            moverEntry.workspace !== source.workspace ||
            moverEntry.fullscreen ||
            moverEntry.maximized ||
            moverEntry.floating === true ||
            moverEntry.sticky === true
        ) {
            this.failFlight(flightState, "precondition-mismatch");
            return;
        }
        const moverRef = moverEntry.ref;
        let targetOutputRef: object | null = null;
        let targetDesktopRef: object | null = null;
        try {
            targetOutputRef = this.env.resolveOutput?.(target.output) ?? null;
        } catch (error) {
            void error;
            targetOutputRef = null;
        }
        try {
            targetDesktopRef = this.env.resolveDesktop?.(target.workspace) ?? null;
        } catch (error) {
            void error;
            targetDesktopRef = null;
        }
        if (targetOutputRef === null || targetDesktopRef === null) {
            this.failFlight(flightState, "stale-scope");
            return;
        }
        const flight = this.activeToken;
        const session = this.plannerSession;
        const r4: R4Flight = {
            flight,
            session,
            correlation,
            baseRevision,
            epoch: this.epoch,
            op: flightState.op,
            snapshot: flightState.snapshot,
            planned,
            moverId: operation.window,
            moverRef,
            targetOutput: target.output,
            targetWorkspace: target.workspace,
            targetOutputRef,
            targetDesktopRef,
            byRef,
            direction: flightState.direction as PlanDirection,
            followed: false,
            detaches: [],
            settled: false,
        };
        this.r4Flight = r4;
        this.diag(flightState.op, correlation, windowCount, "r4-transfer-started");
        if (!this.armR4ArrivalSignals(r4, flight, session)) {
            this.r4SettleTerminal(r4, "write-failed");
            return;
        }
        try {
            const cancel = this.env.scheduleOnce(PLAN_TIMEOUT_MS, () => this.onR4ArrivalTimeout(flight, session));
            this.r4ArrivalTimer = cancel;
        } catch (error) {
            void error;
            this.r4SettleTerminal(r4, "timer-failed");
            return;
        }
        this.r4WriteDepth += 1;
        try {
            if (!this.r4FlightFencesHold(flightState, flight, session, r4)) {
                const fenced = !isUniqueOwner(this.pinnedOwner) || !isGeneration(this.generation) ? "owner-loss" : "stale-scope";
                this.r4FenceDiag(correlation, "pre-transfer-flight", fenced);
                this.r4SettleTerminal(r4, fenced);
                return;
            }
            if (!this.r4FreshScopeAllowsMover(flightState, operation.window, source, target, false, moverRef, false)) {
                this.r4FenceDiag(correlation, "pre-transfer-scope", "stale-scope");
                this.r4SettleTerminal(r4, "stale-scope");
                return;
            }
            let transferred = false;
            try {
                transferred = this.env.sendClientToScreen?.(moverRef, targetOutputRef) === true;
            } catch (error) {
                void error;
                transferred = false;
            }
            if (!transferred) {
                this.r4SettleTerminal(r4, "write-failed");
                return;
            }
            if (!this.r4FlightFencesHold(flightState, flight, session, r4)) {
                const fenced = !isUniqueOwner(this.pinnedOwner) || !isGeneration(this.generation) ? "owner-loss" : "stale-scope";
                this.r4FenceDiag(correlation, "post-transfer-flight", fenced);
                this.r4SettleTerminal(r4, fenced);
                return;
            }
            // Non-homing inter-setter fence between the back-to-back
            // transfer and membership writes: token/owner binding above
            // plus unchanged directional domains (pinned re-observation),
            // a live mover via native reads on the retained ref, frozen
            // gaps, and tiled modes on both domains. Reentrant drift,
            // close, replace, or toggle between the two setters settles
            // terminal with no further setter: a dead wrapper may still
            // accept a write, so setters never prove liveness by
            // themselves. Never homes windows (half-applied placement is
            // valid here); geometry and arrival re-observe once both
            // setters have applied.
            if (!this.r4InterSetterFencesHold(flightState, r4)) {
                this.r4FenceDiag(correlation, "inter-setter", "stale-scope");
                this.r4SettleTerminal(r4, "stale-scope");
                return;
            }
            let membershipWritten = false;
            try {
                membershipWritten = this.env.setDesktops?.(moverRef, [targetDesktopRef]) === true;
            } catch (error) {
                void error;
                membershipWritten = false;
            }
            if (!membershipWritten) {
                this.r4SettleTerminal(r4, "write-failed");
                return;
            }
            const oldById = new Map<string, PlanRect>();
            for (const entry of current.windows) {
                oldById.set(entry.id, { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h });
            }
            // B6: overconstrained members write the effective target (planned
            // origin with each violated extent raised to the freshly observed
            // declared minimum), ordered with the same canonical
            // grow-before-shrink order as every other member.
            const effectivePlanned = planned.geometry.map((entry) => {
                const effective = this.effectiveTargetFor(entry, byRef);
                return { entry, rect: effective.rect, minimumPlaced: effective.minimumPlaced };
            });
            const ordered = orderGeometryWrites(
                oldById,
                effectivePlanned.map((item) => ({ window: item.entry.window, rect: item.rect })),
            );
            const rectByWindow = new Map<string, PlanRect>();
            const placedByWindow = new Set<string>();
            for (const item of effectivePlanned) {
                rectByWindow.set(item.entry.window, item.rect);
                if (item.minimumPlaced) {
                    placedByWindow.add(item.entry.window);
                }
            }
            for (const entry of ordered) {
                if (!this.r4FlightFencesHold(flightState, flight, session, r4)) {
                    const fenced = !isUniqueOwner(this.pinnedOwner) || !isGeneration(this.generation) ? "owner-loss" : "stale-scope";
                    this.r4FenceDiag(correlation, "geometry-flight", fenced);
                    this.r4SettleTerminal(r4, fenced);
                    return;
                }
                if (!this.r4FreshScopeAllowsMover(flightState, operation.window, source, target, false, moverRef, true)) {
                    this.r4FenceDiag(correlation, "geometry-scope", "stale-scope");
                    this.r4SettleTerminal(r4, "stale-scope");
                    return;
                }
                const ref = byRef.get(entry.window);
                if (ref === undefined) {
                    this.r4SettleTerminal(r4, "write-failed");
                    return;
                }
                const writeRect = rectByWindow.get(entry.window) ?? entry.rect;
                let written = false;
                try {
                    written = this.env.setGeometry(ref, writeRect) === true;
                } catch (error) {
                    void error;
                    written = false;
                }
                const resourceClass = this.r4ResourceClass(current, entry.window);
                if (!written) {
                    this.writeDiag(entry.window, resourceClass, "write-failed", writeRect);
                    this.r4SettleTerminal(r4, "write-failed");
                    return;
                }
                this.writeDiag(entry.window, resourceClass, "written", writeRect);
                if (placedByWindow.has(entry.window)) {
                    this.logToken(`${LOG_PREFIX}:minimum-placed correlation=${correlation} window=${entry.window} resource_class=${resourceClass} op=${flightState.op} rect=${String(writeRect.x)},${String(writeRect.y)},${String(writeRect.w)},${String(writeRect.h)}`);
                }
            }
        } finally {
            this.r4WriteDepth = Math.max(0, this.r4WriteDepth - 1);
        }
        this.diag(flightState.op, correlation, windowCount, "r4-native-written");
        if (this.checkR4Arrived(r4)) {
            this.followR4Once(r4);
            if (this.r4Current() === r4 && !r4.settled) {
                this.diag(flightState.op, correlation, windowCount, "r4-arrival-waiting");
            }
            return;
        }
        const placement = this.r4PlacementDetail(r4);
        if (placement === "wrong-target" || placement === "unreadable") {
            const placed = placement === "wrong-target" ? "wrong-output" : "stale-scope";
            this.r4FenceDiag(correlation, "arrival-placement", placed);
            this.r4SettleTerminal(r4, placed);
            return;
        }
        this.diag(flightState.op, correlation, windowCount, "r4-arrival-waiting");
    }

    private writeGeometries(
        planned: PlannedReply,
        flightState: PendingFlight,
        current: PlanObserved,
    ): void {
        // Production cross-output routes: fenced before any native write.
        if (flightState.op === "focus" && this.isCrossFocus(planned, flightState)) {
            this.lifecycleDiag(flightState, "apply", "apply", "started", "-", this.ordinaryRevision(planned, flightState));
            this.applyCrossFocus(planned, flightState, current);
            return;
        }
        // Production R4 cross-output move: immediate commit on the `planned`
        // reply. Native transfer, membership, and geometry complete it
        // synchronously; the shared single-flight stays held throughout with
        // no replay. Ordinary lifecycle stays silent here: R4 owns its
        // transfer diagnostics.
        if (flightState.op === "move" && this.isCrossMove(planned, flightState)) {
            this.beginR4Transfer(planned, flightState, current);
            return;
        }
        this.lifecycleDiag(flightState, "apply", "apply", "started", "-", this.ordinaryRevision(planned, flightState));
        // A cross operation on a same-domain plan, or a cross-domain plan
        // without its exact operation, never actuates: fail closed before
        // any write. No looser alternate plan is accepted for either op:
        // on a two-domain flight a local plan keeps focus in the source
        // with no operation, and only isCrossFocus may target the adjacent
        // domain.
        if (flightState.op === "focus" || flightState.op === "move") {
            const domains = flightState.snapshot.domains;
            if (domains !== undefined && domains.length === 2) {
                // Float-origin focus flights accept only the fenced
                // cross-output target above: a local plan never actuates on
                // them, even when it homes to the source.
                if (
                    flightState.op === "focus" &&
                    (flightState.body as Record<string, unknown>)["float_subject"] === true
                ) {
                    this.ordinaryTerminal(flightState, planned, "uncertain", "apply");
                    this.failFlight(flightState, "precondition-mismatch");
                    return;
                }
                const source = domains[0] as PlanDomain;
                const focusOnSource =
                    planned.focus !== null &&
                    planned.focus.domainOutput === source.output &&
                    planned.focus.domainWorkspace === source.workspace;
                if (!(focusOnSource && planned.operation === null)) {
                    this.ordinaryTerminal(flightState, planned, "uncertain", "apply");
                    this.failFlight(flightState, "precondition-mismatch");
                    return;
                }
            } else if (planned.operation !== null) {
                this.ordinaryTerminal(flightState, planned, "uncertain", "apply");
                this.failFlight(flightState, "precondition-mismatch");
                return;
            }
        }
        const byRef = new Map<string, object>();
        const oldById = new Map<string, PlanRect>();
        const fullscreenById = new Set<string>();
        const maximizedById = new Set<string>();
        const floatingById = new Set<string>();
        const resourceClassById = new Map<string, string>();
        for (const entry of current.windows) {
            byRef.set(entry.id, entry.ref);
            oldById.set(entry.id, { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h });
            if (entry.fullscreen) {
                fullscreenById.add(entry.id);
            }
            if (entry.maximized) {
                maximizedById.add(entry.id);
            }
            if (entry.floating === true) {
                floatingById.add(entry.id);
            }
            resourceClassById.set(entry.id, isOpaqueId(entry.resourceClass) ? entry.resourceClass : "unknown");
        }
        // AR12: planner-directed skips and B6 minimum-origin placement.
        // Overconstrained members write the effective target (planned origin
        // with each violated extent raised to the freshly observed declared
        // minimum) on every op; client-clamped members are honored only on
        // reconcile/update-gaps (the only paths that set the flag) and leave
        // the observed rectangle alone. Park accounting for a fully explained
        // reconcile apply resets below; a mixed apply that also reasserts
        // genuine drift keeps the bounded increment.
        const clampedById = new Set<string>();
        for (const entry of planned.geometry) {
            // A both-flagged member keeps minimum precedence (core resolves
            // contradictory pairs minimum-first): it is minimum-placed,
            // never clamp-skipped.
            if (entry.clientClamped && !entry.overconstrained) {
                clampedById.add(entry.window);
            }
        }
        const honorClamp = flightState.op === "reconcile" || flightState.op === "update-gaps";
        // Effective write targets, resolved once from the fresh observation
        // so ordering, equality, writes, and applied bookkeeping agree. The
        // minimum sets additionally exclude overlay-skipped and honored
        // clamp-skipped members (native owns those frames or the clamp is
        // honored instead; nothing minimum-driven is written for them), so
        // the applied-evidence marker below only ever marks genuinely
        // minimum-guarded write targets.
        const effectiveById = new Map<string, PlanRect>();
        const minimumPlacedById = new Set<string>();
        const minimumGuardedById = new Set<string>();
        for (const entry of planned.geometry) {
            const effective = this.effectiveTargetFor(entry, byRef, oldById.get(entry.window));
            effectiveById.set(entry.window, effective.rect);
            const skipped =
                fullscreenById.has(entry.window) ||
                maximizedById.has(entry.window) ||
                (floatingById.has(entry.window) && flightState.floatTarget?.window !== entry.window) ||
                (honorClamp && clampedById.has(entry.window));
            if (effective.minimumPlaced && !skipped) {
                minimumPlacedById.add(entry.window);
            }
            if (effective.minimumGuarded && !skipped) {
                minimumGuardedById.add(entry.window);
            }
        }
        let honoredAr12Skips = 0;
        for (const entry of planned.geometry) {
            if (honorClamp && clampedById.has(entry.window)) {
                honoredAr12Skips += 1;
            }
        }
        const writable = planned.geometry
            .filter((entry) => !(honorClamp && clampedById.has(entry.window)))
            .map((entry) => ({ ...entry, rect: effectiveById.get(entry.window) ?? entry.rect }));
        const ordered = orderGeometryWrites(oldById, writable);
        const orderedById = new Set<string>();
        for (const entry of ordered) {
            orderedById.add(entry.window);
        }
        // Genuine reasserts pending on this apply: ordered members that are
        // not overlay-skipped. A mixed reconcile (one accepted clamp plus
        // one genuine drift) still performs a genuine write, so it must
        // still advance bounded reassertion attempts below; only a fully explained
        // apply (every drift covered by an honored AR12 skip) converges.
        let pendingWrites = 0;
        for (const entry of ordered) {
            if (
                fullscreenById.has(entry.window) ||
                maximizedById.has(entry.window) ||
                (floatingById.has(entry.window) && flightState.floatTarget?.window !== entry.window)
            ) {
                continue;
            }
            pendingWrites += 1;
        }
        // Focus is focus-only: never rewrite geometry, only move the active
        // window. Matches the standalone focus adapter single-write contract;
        // move/admit/remove/resize still apply complete geometries above.
        // KWin leaves the drag source at its raw pointer rectangle. Reassert the
        // retained projection so its inset sibling gap is restored too.
        if (flightState.op !== "focus") {
            // Bounded per-member disposition lines: every member of the applied
            // command carries its exact write outcome (skipped fullscreen,
            // skipped maximized, already equal, written, or write-failed) with
            // the stable opaque window id and target rect. Fullscreen takes
            // precedence over maximize, and either overlay state takes
            // precedence over equality. The honored client clamp takes
            // precedence over equality and carries its own default-visible
            // correlated line. B6 minimum-origin placement logs only per
            // successfully written raised member (with the actual rect) at
            // the write below, never for already-equal or failed members.
            for (const entry of planned.geometry) {
                const effectiveRect = effectiveById.get(entry.window) ?? entry.rect;
                if (fullscreenById.has(entry.window)) {
                    this.writeDiag(entry.window, resourceClassById.get(entry.window) ?? "unknown", "skip-fullscreen", effectiveRect);
                } else if (maximizedById.has(entry.window)) {
                    this.writeDiag(entry.window, resourceClassById.get(entry.window) ?? "unknown", "skip-maximized", effectiveRect);
                } else if (floatingById.has(entry.window) && flightState.floatTarget?.window !== entry.window) {
                    this.writeDiag(entry.window, resourceClassById.get(entry.window) ?? "unknown", "skip-floating", effectiveRect);
                } else if (honorClamp && clampedById.has(entry.window)) {
                    const resourceClass = resourceClassById.get(entry.window) ?? "unknown";
                    this.writeDiag(entry.window, resourceClass, "skip-clamped", effectiveRect);
                    this.logToken(`${LOG_PREFIX}:clamp-accepted correlation=${flightState.correlation} window=${entry.window} resource_class=${resourceClass} op=${flightState.op}`);
                } else if (!orderedById.has(entry.window)) {
                    this.writeDiag(entry.window, resourceClassById.get(entry.window) ?? "unknown", "skip-already-equal", effectiveRect);
                }
            }
            for (const entry of ordered) {
                if (fullscreenById.has(entry.window) || maximizedById.has(entry.window) || (floatingById.has(entry.window) && flightState.floatTarget?.window !== entry.window)) {
                    continue;
                }
                const target = byRef.get(entry.window);
                if (target === undefined) {
                    this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "precondition-mismatch", this.ordinaryRevision(planned, flightState));
                    this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                    this.failFlight(flightState, "precondition-mismatch");
                    return;
                }
                const resourceClass = resourceClassById.get(entry.window) ?? "unknown";
                const traceNativeConstraints =
                    flightState.op !== "pointer-resize" &&
                    KWIN_TRACE_ENABLED &&
                    typeof this.env.readWindowConstraints === "function";
                if (traceNativeConstraints) {
                    this.traceConstraints(
                        flightState.correlation,
                        "plan",
                        entry.window,
                        entry.output,
                        resourceClass,
                        target,
                        this.workAreaFor(flightState.snapshot, entry.output, entry.workspace),
                        entry.rect,
                        oldById.get(entry.window) ?? null,
                    );
                }
                let written = false;
                try {
                    written = this.env.setGeometry(target, entry.rect) === true;
                } catch (error) {
                    void error;
                    written = false;
                }
                if (!written) {
                    this.writeDiag(entry.window, resourceClassById.get(entry.window) ?? "unknown", "write-failed", entry.rect);
                    this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "write-failed", this.ordinaryRevision(planned, flightState));
                    this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                    this.failFlight(flightState, "write-failed");
                    return;
                }
                this.writeDiag(entry.window, resourceClass, "written", entry.rect);
                if (minimumPlacedById.has(entry.window)) {
                    this.logToken(`${LOG_PREFIX}:minimum-placed correlation=${flightState.correlation} window=${entry.window} resource_class=${resourceClass} op=${flightState.op} rect=${String(entry.rect.x)},${String(entry.rect.y)},${String(entry.rect.w)},${String(entry.rect.h)}`);
                }
                if (traceNativeConstraints) {
                    this.constraintTracePending.set(entry.window, {
                        correlation: flightState.correlation,
                        resourceClass,
                        requested: entry.rect,
                    });
                    this.traceConstraints(
                        flightState.correlation,
                        "write",
                        entry.window,
                        entry.output,
                        resourceClass,
                        target,
                        this.workAreaFor(flightState.snapshot, entry.output, entry.workspace),
                        entry.rect,
                        null,
                    );
                }
            }
            const transition = flightState.floatTarget;
            if (transition !== null && transition.floating) {
                const floatGeometry = planned.floatGeometry;
                const target = byRef.get(transition.window);
                if (floatGeometry === null || target === undefined) {
                    this.writeDiag(transition.window, resourceClassById.get(transition.window) ?? "unknown", "float-write-failed", floatGeometry?.rect ?? { x: 0, y: 0, w: 1, h: 1 });
                    this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "write-failed", this.ordinaryRevision(planned, flightState));
                    this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                    this.failFlight(flightState, "write-failed");
                    return;
                }
                if (!this.ensureKeepAbove(target, transition.window, resourceClassById.get(transition.window) ?? "unknown")) {
                    this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "write-failed", this.ordinaryRevision(planned, flightState));
                    this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                    this.failFlight(flightState, "write-failed");
                    return;
                }
                let written = false;
                try {
                    written = this.env.setGeometry(target, floatGeometry.rect) === true;
                } catch (error) {
                    void error;
                    written = false;
                }
                if (!written) {
                    this.restoreKeepAbove(transition.window, resourceClassById.get(transition.window) ?? "unknown");
                    this.writeDiag(transition.window, resourceClassById.get(transition.window) ?? "unknown", "float-write-failed", floatGeometry.rect);
                    this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "write-failed", this.ordinaryRevision(planned, flightState));
                    this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                    this.failFlight(flightState, "write-failed");
                    return;
                }
                this.writeDiag(transition.window, resourceClassById.get(transition.window) ?? "unknown", "float-written", floatGeometry.rect);
                try {
                    this.env.setFloating?.(transition.window, true);
                } catch (error) {
                    void error;
                    this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "write-failed", this.ordinaryRevision(planned, flightState));
                    this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                    this.failFlight(flightState, "write-failed");
                    return;
                }
            } else if (transition !== null) {
                if (!this.restoreKeepAbove(transition.window, resourceClassById.get(transition.window) ?? "unknown")) {
                    this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "write-failed", this.ordinaryRevision(planned, flightState));
                    this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                    this.failFlight(flightState, "write-failed");
                    return;
                }
                try {
                    this.env.setFloating?.(transition.window, false);
                } catch (error) {
                    void error;
                    this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "write-failed", this.ordinaryRevision(planned, flightState));
                    this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                    this.failFlight(flightState, "write-failed");
                    return;
                }
            }
            const sticky = flightState.stickyTarget;
            if (sticky !== null) {
                const target = current.windows.find((entry) => entry.id === sticky.window);
                if (target === undefined) {
                    this.ordinaryTerminal(flightState, planned, "uncertain", "apply");
                    this.failFlight(flightState, "precondition-mismatch");
                    return;
                }
                this.issueSticky(target, true, sticky.previousFloating);
            }
            // Toggle-float flights never actuate Rust desired_focus: it is
            // tiled survivor bookkeeping and would activate a sibling. Retain
            // the exact toggled window instead, from the fresh observation
            // only. Stale ids fail closed; a failed native write fails the
            // flight. No timer, retry, desktop switch, or later focus write.
            const floatTransition = flightState.floatTarget;
            if (floatTransition !== null) {
                const floatRef = byRef.get(floatTransition.window);
                if (floatRef === undefined) {
                    this.logToken(`${LOG_PREFIX}:float-focus-stale window=${floatTransition.window} resource_class=${resourceClassById.get(floatTransition.window) ?? "unknown"}`);
                    this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
                    this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
                    this.failFlight(flightState, "stale-scope");
                    return;
                }
                if (!this.retainFloatFocus(floatTransition.window, floatRef, resourceClassById.get(floatTransition.window) ?? "unknown")) {
                    this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "write-failed", this.ordinaryRevision(planned, flightState));
                    this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                    this.failFlight(flightState, "write-failed");
                    return;
                }
            }
        }
        const focus = planned.focus;
        // Hidden-domain flights never route focus: the anchor focusedId is a
        // structural placeholder, and applying it would steal native focus and
        // switch desktop visibility. Toggle-float flights retain the exact
        // toggled window above and never actuate survivor bookkeeping focus.
        if (focus !== null && flightState.background !== true && flightState.op !== "toggle-float") {
            let focusWindow: string | null = null;
            for (const entry of planned.geometry) {
                if (entry.leaf === focus.leaf) {
                    focusWindow = entry.window;
                    break;
                }
            }
            if (
                focusWindow !== null &&
                focus.domainOutput === current.domainOutput &&
                focus.domainWorkspace === current.domainWorkspace
            ) {
                // Automatic reconciles never actuate survivor bookkeeping
                // focus over a floating/sticky focused subject (e.g. the
                // sticky-triggered flag reconcile): the excluded subject keeps
                // native focus. Tiled reconciles and explicit focus/move
                // commands actuate unchanged.
                let focusSkipped = false;
                if (flightState.op === "reconcile" && focusWindow !== current.focusedId) {
                    const focusedEntry = current.windows.find((entry) => entry.id === current.focusedId);
                    if (focusedEntry !== undefined && (focusedEntry.floating === true || focusedEntry.sticky === true)) {
                        const reason = focusedEntry.floating === true ? "floating-active" : "sticky-active";
                        this.logToken(`${LOG_PREFIX}:focus-skipped correlation=${flightState.correlation} kind=reconcile reason=${reason} window=${current.focusedId} target=${focusWindow}`);
                        focusSkipped = true;
                    }
                }
                // D01 native activation ownership: automatic convergence
                // never actuates focus while a fullscreen/maximized overlay
                // is involved. Explicit op focus stays exempt inside the
                // guard. The guard re-reads the foreground observation
                // after geometry writes and carries the live active()
                // read forward, so the setter below never re-reads.
                if (!focusSkipped) {
                    const guard = this.automaticFocusGuard(flightState);
                    if (guard.reason !== null) {
                        this.logToken(`${LOG_PREFIX}:focus-skipped correlation=${flightState.correlation} kind=${flightState.op} reason=${guard.reason} window=${current.focusedId} target=${focusWindow}`);
                        focusSkipped = true;
                    } else {
                        const target = byRef.get(focusWindow);
                        if (target !== undefined && guard.active !== target) {
                            let focused = false;
                            try {
                                focused = this.env.setActive(target) === true;
                            } catch (error) {
                                void error;
                                focused = false;
                            }
                            if (!focused) {
                                this.lifecycleDiag(flightState, "apply", "setters", "write-failed", "write-failed", this.ordinaryRevision(planned, flightState));
                                this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
                                this.failFlight(flightState, "write-failed");
                                return;
                            }
                        }
                    }
                }
            }
        }
        // Whether a foreground auto reconcile changed tiled membership or
        // floating/sticky flags versus before-apply per-id evidence
        // (computed inside the geometry branch below, consumed by the
        // highlight edge after it).
        let autoMembershipChanged = false;
        // Before-write per-id evidence presence for the flight domain, read
        // before appliedById advances below. Fresh seeds with no evidence
        // yet converge like the retired admit/remove.
        let hasAppliedBefore = false;
        if (flightState.op !== "focus") {
            const base = this.carriedSnapshot(current);
            // A local reply to a directional two-domain request only owns the
            // source domain. Retaining the untouched target here would later
            // synthesize its removal under the source bounds.
            const localDirectional =
                flightState.op === "move" &&
                flightState.snapshot.domains !== undefined &&
                flightState.snapshot.domains.length === 2 &&
                planned.operation === null;
            const { domains: _domains, ...sourceBase } = base;
            const retainedBase = localDirectional
                ? {
                      ...sourceBase,
                      windows: Object.freeze(
                          base.windows.filter(
                              (entry) =>
                                  entry.output === flightState.snapshot.domainOutput &&
                                  entry.workspace === flightState.snapshot.domainWorkspace,
                          ),
                      ),
                  }
                : base;
            const rectById = new Map<string, PlanRect>();
            for (const entry of planned.geometry) {
                // Applied bookkeeping records the effective write target so
                // equality, readback, and settlement agree with what native
                // was asked for (B6 minimum-origin placement included).
                rectById.set(entry.window, effectiveById.get(entry.window) ?? entry.rect);
            }
            // Auto-reconcile membership edge, read before applied evidence
            // advances below: a reconcile that changed tiled membership or
            // floating/sticky flags versus before-apply per-id evidence on
            // the same domain (including fresh seeds with no evidence yet)
            // converges like the retired admit/remove. Foreground uses it
            // for the highlight refresh and counter reset below; background
            // uses it to keep membership convergence out of the bounded
            // drift count. Equal reflows stay quiet and keep existing
            // accounting. Never baseline authority.
            hasAppliedBefore = this.hasAppliedEvidenceFor(flightState.snapshot);
            if (flightState.op === "reconcile" && this.appliedMembershipOrFlagsChanged(flightState.snapshot)) {
                autoMembershipChanged = true;
            }
            const windows = retainedBase.windows.map((entry) => {
                // Every member (including a fullscreen or maximized one)
                // records the planner's retained projection from the reply: the
                // tree slot must survive enter/exit, and the carried baseline
                // never stores the compositor-owned frame rect.
                const rect = rectById.get(entry.id) ?? entry.rect;
                const transition = flightState.floatTarget;
                const floating = transition !== null && transition.window === entry.id ? transition.floating : entry.floating;
                const floatRect = transition !== null && transition.window === entry.id && transition.floating ? planned.floatGeometry?.rect : undefined;
                const next = floatRect ?? rect;
                return { id: entry.id, rect: { x: next.x, y: next.y, w: next.w, h: next.h }, output: entry.output, workspace: entry.workspace, fullscreen: entry.fullscreen, maximized: entry.maximized, floating, resourceClass: entry.resourceClass };
            });
            // A committed remove that empties the domain retires its applied
            // evidence and all background accounting at the same applied
            // boundary so the slot is released. Zero-window scope is never
            // retained.
            if (flightState.op === "remove" && windows.length === 0) {
                const emptyKey = this.domainKey(base);
                this.appliedScopeByDomain.delete(emptyKey);
                // Global initial-fullscreen markers retained: a per-domain
                // remove-empty cannot prove cross-domain absence; relocation
                // survivors keep markers. Explicit-removed below, the exact
                // native removal signal, and full resets remain true-gone
                // cleanup.
                for (const [id, evidence] of [...this.appliedById]) {
                    if (evidence.output === base.domainOutput && evidence.workspace === base.domainWorkspace) {
                        this.appliedById.delete(id);
                    }
                }
                if (flightState.background === true) {
                    this.clearBackgroundReconcile(base);
                }
                if (flightState.removed !== null) {
                    this.maximizeAdmissionAttempts.delete(flightState.removed);
                    this.keepAbovePrevious.delete(flightState.removed);
                    this.stickyPreviousFloating.delete(flightState.removed);
                    this.adoptedSticky.delete(flightState.removed);
                    this.heldInitialFullscreen.delete(flightState.removed);
                    this.seenNonFullscreen.delete(flightState.removed);
                    // No global removal here: this boundary is currently
                    // unreachable (no op:"remove" dispatch exists; removed
                    // only rides toggle-float which never takes this
                    // branch), and scoped absence must never globally evict
                    // fixed provenance, intent membership, or float marks.
                    // Genuine closes always arrive ref-confirmed through the
                    // native windowRemoved signal path, which owns every
                    // global clear.
                }
            } else {
                const scopeKey = this.domainKey(retainedBase);
                if (windows.length === 0) {
                    this.appliedScopeByDomain.delete(scopeKey);
                    // D23 accepted-empty proof for close synthesis: only a
                    // planner-applied empty reconcile proves no live members.
                    if (flightState.op === "reconcile") {
                        try {
                            this.explicitEmptyByKey.add(scopeKey);
                        } catch (error) {
                            void error;
                        }
                    }
                } else {
                    this.appliedScopeByDomain.set(scopeKey, {
                        bounds: {
                            x: retainedBase.domainBounds.x,
                            y: retainedBase.domainBounds.y,
                            w: retainedBase.domainBounds.w,
                            h: retainedBase.domainBounds.h,
                        },
                        gap: retainedBase.domainGap,
                        outerGap: retainedBase.domainOuterGap,
                    });
                }
                const live = new Set<string>();
                for (const entry of windows) {
                    live.add(entry.id);
                    const source = retainedBase.windows.find((candidate) => candidate.id === entry.id);
                    // Fresh every apply: marked exactly while the retained
                    // target is minimum-guarded (raised, floor-touching, or
                    // host-undershot). Cleared by feasible projections with
                    // slack, acceptance, and (via deletion below) member
                    // lifecycle.
                    this.appliedById.set(entry.id, {
                        rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
                        output: entry.output,
                        workspace: entry.workspace,
                        floating: entry.floating === true,
                        sticky: source?.sticky === true,
                        fullscreen: entry.fullscreen,
                        maximized: entry.maximized,
                        minimumPlaced: minimumGuardedById.has(entry.id),
                    });
                }
                for (const [id, evidence] of [...this.appliedById]) {
                    if (
                        evidence.output === retainedBase.domainOutput &&
                        evidence.workspace === retainedBase.domainWorkspace &&
                        !live.has(id)
                    ) {
                        // Per-domain prune retires applied evidence only;
                        // global markers retained for possible cross-domain
                        // survivors (see above).
                        this.appliedById.delete(id);
                    }
                }
            }
            if (flightState.op === "pointer-resize" && flightState.pointerSource !== null) {
                // The echo fence compares what native will show: minimum-
                // placed neighbours read back at the effective target, so
                // store the effective rect (already the written one above).
                const neighbours = planned.geometry
                    .filter((entry) => entry.window !== flightState.pointerSource)
                    .map((entry) => {
                        const effective = effectiveById.get(entry.window) ?? entry.rect;
                        return {
                            window: entry.window,
                            rect: { x: effective.x, y: effective.y, w: effective.w, h: effective.h },
                        };
                    });
                this.pointerEcho = {
                    correlation: flightState.correlation,
                    source: flightState.pointerSource,
                    scope: flightState.snapshot,
                    neighbours: Object.freeze(neighbours),
                };
                this.logToken(`${LOG_PREFIX}:echo-fence-armed`);
            }
            if (flightState.op === "reconcile") {
                const fullyExplained = honoredAr12Skips > 0 && pendingWrites === 0;
                const membershipConverges = !hasAppliedBefore || autoMembershipChanged;
                if (flightState.background === true) {
                    if (
                        flightState.workAreaReprojection === true ||
                        autoMembershipChanged ||
                        fullyExplained
                    ) {
                        // Membership/flag-changing, work-area, and fully
                        // explained AR12 applies converge without advancing
                        // background acceptance.
                        this.clearBackgroundReconcile(flightState.snapshot);
                    } else {
                        this.noteBackgroundTerminal(flightState.snapshot);
                    }
                } else if (fullyExplained || membershipConverges) {
                    this.reconcileAttempts = 0;
                } else {
                    this.noteReconcileTerminal(flightState.op, flightState.workAreaReprojection);
                }
            } else if (flightState.background === true) {
                this.clearBackgroundReconcile(flightState.snapshot);
            } else {
                this.reconcileAttempts = 0;
            }
        } else {
            this.reconcileAttempts = 0;
        }
        if (isUniqueOwner(this.pinnedOwner)) {
            this.knownOwner = this.pinnedOwner;
        }
        if (flightState.op !== "focus") {
            const skips = new Set<string>();
            for (const entry of planned.geometry) {
                if (fullscreenById.has(entry.window)) {
                    skips.add("skipped-fullscreen");
                } else if (maximizedById.has(entry.window)) {
                    skips.add("skipped-maximized");
                } else if (floatingById.has(entry.window) && flightState.floatTarget?.window !== entry.window) {
                    skips.add("skipped-floating");
                } else if (honorClamp && clampedById.has(entry.window)) {
                    skips.add("skipped-clamped");
                } else if (!orderedById.has(entry.window)) {
                    skips.add("skipped-equal");
                }
            }
            this.lifecycleDiag(flightState, "apply", "setters", "applied", this.skipSummary(skips), this.ordinaryRevision(planned, flightState));
            this.ordinaryTerminal(flightState, planned, "applied", "setters");
        } else {
            this.ordinaryTerminal(flightState, planned, "applied", "apply");
        }
        this.inFlight = false;
        this.pending = null;
        this.pinnedOwner = null;
        this.activationStep = 0;
        // Explicit user tile/float intent lands only on proof: a staged Q2
        // fixed-size lifecycle change commits here, after every setter
        // succeeded. Refused, stale, timed-out, or write-failed flights
        // return through failFlight or earlier exits with the stage
        // untouched, so the authoritative record never moves without
        // application. Q3 reuses this exact boundary for the durable
        // settled membership (explicit ordinary float/unfloat only).
        this.commitFixedStage(flightState.fixedStage);
        this.updateSettledIntentFromFlight(flightState);
        this.diag(flightState.op, flightState.correlation, flightState.windowCount, "planned-applied");
        this.noteCrossDragApplied(flightState);
        // Marker satisfaction through actual application only: the first
        // subsequent plan that applies this domain's full geometry clears
        // the marker with one terminal per drag naming this plan.
        this.satisfyDragRestore(flightState, planned, current);
        // Exactly one observational active-group refresh after an actual
        // successful geometry-plan boundary, even when focus is unchanged.
        // Geometry writes emit no highlight signal, so without this edge the
        // entry-owned highlight would stay stale. Admit/move/remove/resize
        // qualify, plus a foreground auto reconcile that changed tiled
        // membership or floating/sticky flags; focus, equal-reflow
        // reconcile, pointer-resize, and toggle-float never refresh here
        // (focus already re-queries via its signal). A drag-drop changes
        // tiling topology like move, so it refreshes too. Stale, rejected,
        // error, and unfinished boundaries return through failFlight or
        // earlier exits and never reach this edge. The callback is
        // best-effort and non-blocking: it must not delay the deferred
        // foreground command below.
        if (
            flightState.background !== true &&
            (flightState.op === "admit" ||
                flightState.op === "move" ||
                flightState.op === "remove" ||
                flightState.op === "resize" ||
                flightState.op === "drag-drop" ||
                (flightState.op === "reconcile" && autoMembershipChanged))
        ) {
            try {
                this.env.onPlannedApplied?.(flightState.op);
            } catch (error) {
                void error;
            }
        }
        this.finishFlight();
        this.chainPendingCloses();
    }

    // D23 drain: after an applied plan, one more pass over the existing
    // signal-driven chain while closes remain queued. Success-only (the
    // planner is healthy and progress was made): failure terminals retain
    // without chaining, so no signal-free loop can form. Rounds are
    // async-separated (a dispatch awaits its reply) and stop when pending
    // empties, when a round dispatches nothing, or when a terminal fails:
    // every close-carrying success strictly shrinks the queue. Respects the
    // identity-probe race guard exactly like finishFlight.
    private chainPendingCloses(): void {
        if (
            !this.enabled ||
            this.inFlight ||
            this.deferredAuto !== null ||
            this.activeProbe !== 0 ||
            this.pendingClosed.size === 0
        ) {
            return;
        }
        try {
            this.refreshNow();
        } catch (error) {
            void error;
        }
    }

    // Lean R4 helpers: one-shot arrival signals, fresh exact mover proof,
    // single follow, and forced source+target settlement. No geometry echo,
    // no ack/verify/cancel/status wire protocol, no settlement timers beyond
    // the bounded arrival deadline.
    private r4ResourceClass(current: PlanObserved, windowId: string): string {
        for (const entry of current.windows) {
            if (entry.id === windowId) {
                return isOpaqueId(entry.resourceClass) ? entry.resourceClass : "unknown";
            }
        }
        return "unknown";
    }

    private r4Current(): R4Flight | null {
        const r4 = this.r4Flight;
        if (r4 === null || r4.settled || !this.inFlight || r4.flight !== this.activeToken || r4.session !== this.plannerSession) {
            return null;
        }
        return r4;
    }

    // Per-setter flight fences: pinned owner/generation plus flight/session/
    // epoch/correlation identity. Called before every native setter because a
    // synchronous setter effect or an intervening signal may have released or
    // superseded the flight mid-stack.
    private r4FlightFencesHold(flightState: PendingFlight, flight: number, session: number, r4: R4Flight): boolean {
        if (!isUniqueOwner(this.pinnedOwner)) {
            return false;
        }
        if (!isGeneration(this.generation)) {
            return false;
        }
        if (!this.inFlight || this.pending !== flightState) {
            return false;
        }
        if (flight !== this.activeToken || session !== this.plannerSession) {
            return false;
        }
        if (flightState.plannerSession !== this.plannerSession) {
            return false;
        }
        if (flightState.epoch !== this.epoch) {
            return false;
        }
        if (flightState.correlation !== r4.correlation) {
            return false;
        }
        if (this.r4Current() !== r4 || r4.settled) {
            return false;
        }
        return true;
    }

    // Raw directional re-observation for R4 mid-write/follow fences: bypasses
    // dispatch-time validation (focused-in-source, homing, fingerprint,
    // revalidate) because the intended mover relocation itself breaks those
    // gates (half placement is unhomed, target placement moves focus). Only
    // structural identity is enforced by callers. Pins the flight source so
    // the still-active mover on the destination cannot flip the observation
    // source mid-flight.
    private r4RawFresh(direction: PlanDirection, snapshot: PlanSnapshot | null): PlanObserved | null {
        const hook = this.env.observeDirectional;
        if (typeof hook !== "function") {
            return null;
        }
        let raw: DirectionalObservation | PlanObserved | null = null;
        try {
            const domains = snapshot?.domains;
            const targetOutput =
                snapshot !== null && domains !== undefined && domains.length === 2
                    ? (domains[1] as PlanDomain).output
                    : undefined;
            raw = hook(
                direction,
                snapshot === null
                    ? undefined
                    : {
                          output: snapshot.domainOutput,
                          workspace: snapshot.domainWorkspace,
                          ...(targetOutput === undefined ? {} : { targetOutput }),
                      },
                true,
            );
        } catch (error) {
            void error;
            return null;
        }
        if (raw === null || raw === undefined) {
            return null;
        }
        if (typeof raw === "object" && "status" in raw) {
            const outcome = raw as DirectionalObservation;
            if (outcome.status !== "ready" || outcome.observed === null || outcome.observed === undefined) {
                return null;
            }
            return outcome.observed;
        }
        const candidate = raw as PlanObserved;
        if (candidate.windows === undefined || candidate.domains === undefined) {
            return null;
        }
        return candidate;
    }

    // Non-homing inter-setter fence between the back-to-back R4 transfer
    // and membership writes. See the call site for the full contract.
    private r4InterSetterFencesHold(flightState: PendingFlight, r4: R4Flight): boolean {
        const direction = flightState.direction;
        if (!isDirection(direction)) {
            return false;
        }
        const fresh = this.r4RawFresh(direction, flightState.snapshot);
        if (fresh === null) {
            return false;
        }
        const snapshot = flightState.snapshot;
        const domains = snapshot.domains;
        if (domains === undefined || domains.length !== 2) {
            return false;
        }
        const source = domains[0] as PlanDomain;
        const target = domains[1] as PlanDomain;
        if (!domainsEqual(fresh.domains, domains)) {
            return false;
        }
        if (fresh.domainGap !== snapshot.domainGap || fresh.domainOuterGap !== snapshot.domainOuterGap) {
            return false;
        }
        // Lifetime on the retained mover ref via native reads: a closed or
        // replaced mover fails here even when a dead wrapper would still
        // accept the membership write below.
        let output: string | null = null;
        let ids: ReadonlyArray<string> | null = null;
        try {
            output = this.env.readOutputName?.(r4.moverRef) ?? null;
            ids = this.env.readDesktopIds?.(r4.moverRef) ?? null;
        } catch (error) {
            void error;
            return false;
        }
        if (output === null || ids === null) {
            return false;
        }
        if (!this.isTiledDomain(source.output, source.workspace)) {
            return false;
        }
        if (!this.isTiledDomain(target.output, target.workspace)) {
            return false;
        }
        return true;
    }

    // Structural scope fence for R4 writes: exact domain/scope identity plus
    // window-set identity, ignoring rects, fingerprint, and focus. Tolerates
    // the intended mover relocation while rejecting unrelated stale changes:
    // - writes (`requireTarget` false): mover output/workspace may each be
    //   source or target (source, half, or target placement).
    // - follow (`requireTarget` true): mover must sit exactly on the target.
    // Non-mover windows must retain exact output/workspace plus observed
    // native flags (fullscreen/maximized/floating/sticky) against the dispatch
    // snapshot. Missing mover, extra/missing ids, changed domains, changed
    // non-mover flags, or an exceptional mover all fail, except one
    // post-setter exception: with `allowAbsentMover` true (geometry loop
    // only, after both setters applied) a fresh set that is exactly the
    // flight set minus the mover is tolerated when native reads on the
    // retained ref still place the mover in scope. Pre-transfer
    // (`allowAbsentMover` false) an absent mover always fails: no own
    // relocation has happened yet, so absence is unrelated drift.
    private r4FreshScopeAllowsMover(
        flightState: PendingFlight,
        moverId: string,
        source: PlanDomain,
        target: PlanDomain,
        requireTarget: boolean,
        moverRef: object,
        allowAbsentMover: boolean,
    ): boolean {
        const direction = flightState.direction;
        if (!isDirection(direction)) {
            return false;
        }
        const fresh = this.r4RawFresh(direction, flightState.snapshot);
        if (fresh === null) {
            this.r4ScopeSubreason(flightState.correlation, "raw-null", "");
            return false;
        }
        const flightSnapshot = flightState.snapshot;
        if (!domainsEqual(fresh.domains, flightSnapshot.domains)) {
            this.r4ScopeSubreason(flightState.correlation, "domains", "");
            return false;
        }
        if (!sameDomainScalars(fresh, flightSnapshot)) {
            this.r4ScopeSubreason(flightState.correlation, "scalars", "");
            return false;
        }
        if (!sameDomainBounds(fresh, flightSnapshot)) {
            this.r4ScopeSubreason(flightState.correlation, "bounds", "");
            return false;
        }
        if (
            fresh.windows.length !== flightSnapshot.windows.length &&
            !(allowAbsentMover && fresh.windows.length + 1 === flightSnapshot.windows.length)
        ) {
            this.r4ScopeSubreason(
                flightState.correlation,
                "window-count",
                ` fresh_len=${String(fresh.windows.length)} flight_len=${String(flightSnapshot.windows.length)}`,
            );
            return false;
        }
        const windowCountMatches = fresh.windows.length === flightSnapshot.windows.length;
        const flightById = new Map<string, PlanSnapshotWindow>();
        for (const entry of flightSnapshot.windows) {
            flightById.set(entry.id, entry);
        }
        let moverFound = false;
        for (const entry of fresh.windows) {
            const expected = flightById.get(entry.id);
            if (expected === undefined) {
                this.r4ScopeSubreason(flightState.correlation, "unknown-id", "");
                return false;
            }
            if (entry.id === moverId) {
                moverFound = true;
                if (entry.fullscreen || entry.maximized || entry.floating === true || entry.sticky === true) {
                    this.r4ScopeSubreason(
                        flightState.correlation,
                        "mover-flags",
                        ` fs=${entry.fullscreen === true ? "1" : "0"} mm=${entry.maximized === true ? "1" : "0"} fl=${entry.floating === true ? "1" : "0"} st=${entry.sticky === true ? "1" : "0"}`,
                    );
                    return false;
                }
                if (requireTarget) {
                    if (entry.output !== target.output || entry.workspace !== target.workspace) {
                        this.r4ScopeSubreason(flightState.correlation, "mover-placement", " mode=require-target");
                        return false;
                    }
                } else {
                    const outputInScope = entry.output === source.output || entry.output === target.output;
                    const workspaceInScope = entry.workspace === source.workspace || entry.workspace === target.workspace;
                    if (!outputInScope || !workspaceInScope) {
                        const outputBucket = entry.output === source.output ? "s" : entry.output === target.output ? "t" : "other";
                        const workspaceBucket =
                            entry.workspace === source.workspace ? "s" : entry.workspace === target.workspace ? "t" : "other";
                        this.r4ScopeSubreason(
                            flightState.correlation,
                            "mover-placement",
                            ` o=${outputBucket} w=${workspaceBucket}`,
                        );
                        return false;
                    }
                }
            } else {
                if (entry.output !== expected.output || entry.workspace !== expected.workspace) {
                    this.r4ScopeSubreason(flightState.correlation, "nonmover-placement", "");
                    return false;
                }
                if (
                    entry.fullscreen !== expected.fullscreen ||
                    entry.maximized !== expected.maximized ||
                    (entry.floating === true) !== (expected.floating === true) ||
                    (entry.sticky === true) !== (expected.sticky === true)
                ) {
                    this.r4ScopeSubreason(flightState.correlation, "nonmover-flags", "");
                    return false;
                }
            }
        }
        if (!moverFound) {
            // Missing-mover exception, post-setter writes only: scope
            // collection matches by output plus desktop membership, so a
            // half-placed own mover (one write visible, the other lagging;
            // precise KWin flush timing inferred, not proven) belongs to
            // neither scope and vanishes from the fresh set. Tolerate
            // exactly that gap when the fresh set is otherwise the flight
            // set minus the mover and native reads on the retained ref
            // still place the mover in scope. Before any own relocation
            // (allowAbsentMover false) an absent mover is unrelated drift
            // and always refuses, as does any other mismatch here.
            if (
                !allowAbsentMover ||
                requireTarget ||
                !this.r4AbsentMoverNativeInScope(flightSnapshot, fresh, moverId, moverRef, source, target)
            ) {
                this.r4ScopeSubreason(
                    flightState.correlation,
                    "mover-missing",
                    ` fresh_len=${String(fresh.windows.length)}`,
                );
                return false;
            }
            return true;
        }
        if (!windowCountMatches) {
            // Mover present, but a non-mover window is missing from the
            // fresh set: unrelated drift, never tolerated.
            this.r4ScopeSubreason(
                flightState.correlation,
                "nonmover-missing",
                ` fresh_len=${String(fresh.windows.length)} flight_len=${String(flightSnapshot.windows.length)}`,
            );
            return false;
        }
        return true;
    }

    // Native retained-ref proof for a half-visible own mover (see above): the fresh
    // set must be exactly the flight set minus the mover (same count minus
    // one, every flight non-mover present; unknown ids and non-mover
    // placement/flags are already refused by the caller loop), and native
    // reads on the retained ref must place the mover in scope with a single
    // desktop membership. Anything else fails.
    private r4AbsentMoverNativeInScope(
        flightSnapshot: PlanSnapshot,
        fresh: PlanObserved,
        moverId: string,
        moverRef: object,
        source: PlanDomain,
        target: PlanDomain,
    ): boolean {
        if (fresh.windows.length + 1 !== flightSnapshot.windows.length) {
            return false;
        }
        const freshIds = new Set<string>();
        for (const entry of fresh.windows) {
            freshIds.add(entry.id);
        }
        if (freshIds.has(moverId)) {
            return false;
        }
        for (const entry of flightSnapshot.windows) {
            if (entry.id === moverId) {
                continue;
            }
            if (!freshIds.has(entry.id)) {
                return false;
            }
        }
        let output: string | null = null;
        try {
            output = this.env.readOutputName?.(moverRef) ?? null;
        } catch (error) {
            void error;
            return false;
        }
        let ids: ReadonlyArray<string> | null = null;
        try {
            ids = this.env.readDesktopIds?.(moverRef) ?? null;
        } catch (error) {
            void error;
            return false;
        }
        if (output === null || ids === null) {
            return false;
        }
        if (output !== source.output && output !== target.output) {
            return false;
        }
        if (ids.length !== 1) {
            return false;
        }
        const workspace = ids[0] as string;
        if (workspace !== source.workspace && workspace !== target.workspace) {
            return false;
        }
        return true;
    }

    // Narrow subreason evidence for the R4 structural scope fence:
    // diagnostic only, never changes control flow. One bounded line per
    // rejection naming the first failing sub-comparison with structural
    // counts/buckets only (no captions, rects, or raw output names).
    private r4ScopeSubreason(correlation: string, subreason: string, detail: string): void {
        this.logToken(`${LOG_PREFIX}:r4-scope correlation=${correlation} subreason=${subreason}${detail}`);
    }

    // Lag prewrite: shared destination projection plus the pointer
    // exception comparators. Ready plus live mover ref when native is still
    // source and the projection matches the flight; undefined otherwise so
    // the ordinary drag-drop fence decides. No terminal/timer/retry here.
    private crossDragLagPrewrite(
        flightState: PendingFlight,
        fresh: PlanObserved,
    ):
        | { readonly moverId: string; readonly moverRef: object; readonly targetOutput: string; readonly targetWorkspace: string }
        | undefined {
        if (flightState.op !== "drag-drop" || flightState.background === true) {
            return undefined;
        }
        const moverId = flightState.pointerSource;
        if (moverId === null) {
            return undefined;
        }
        const source = this.crossDragSourceOf(flightState);
        if (source === null) {
            return undefined;
        }
        const targetOutput = flightState.snapshot.domainOutput;
        const targetWorkspace = flightState.snapshot.domainWorkspace;
        if (fresh.domainOutput !== source.output || fresh.domainWorkspace !== source.workspace) {
            return undefined;
        }
        const rawX: unknown = (flightState.body as Record<string, unknown>)["x"];
        const rawY: unknown = (flightState.body as Record<string, unknown>)["y"];
        if (!isFiniteInt(rawX) || !isFiniteInt(rawY)) {
            return undefined;
        }
        const freshMover = fresh.windows.find((entry) => entry.id === moverId);
        if (
            freshMover === undefined ||
            freshMover.output !== source.output ||
            freshMover.workspace !== source.workspace ||
            freshMover.fullscreen ||
            freshMover.maximized ||
            freshMover.floating === true ||
            freshMover.sticky === true
        ) {
            return undefined;
        }
        const moverRef = freshMover.ref;
        // Builder never throws (single guarded hidden read; pure predicates
        // elsewhere), so no try/catch is needed here.
        const pointer = this.buildPointerDomainObservation(moverId, rawX as number, rawY as number, { output: source.output, workspace: source.workspace });
        if (pointer.ok === false || pointer.projected !== true) {
            return undefined;
        }
        if (
            pointer.effectiveObserved.domainOutput !== targetOutput ||
            pointer.effectiveObserved.domainWorkspace !== targetWorkspace ||
            pointer.mover.ref !== moverRef
        ) {
            return undefined;
        }
        let effectiveSnapshot: PlanSnapshot;
        try {
            effectiveSnapshot = this.carriedSnapshot(pointer.effectiveObserved);
        } catch (error) {
            void error;
            return undefined;
        }
        if (
            !rectsEqualExceptSource(effectiveSnapshot, flightState.snapshot, moverId) ||
            unexpectedOverlaySkewed(flightState, effectiveSnapshot)
        ) {
            return undefined;
        }
        return { moverId, moverRef, targetOutput, targetWorkspace };
    }

    // Lag actuation in R4 order (sendClientToScreen, setDesktops, actual
    // dest observation, writeGeometries). Membership verified twice
    // (native readback, then actual observation fenced by the pointer
    // comparators plus exact mover ref). Failures use failFlight
    // (destination marker, no source fight). No timers/retries.
    private applyCrossDragLagTransfer(
        planned: PlannedReply,
        flightState: PendingFlight,
        lag: { readonly moverId: string; readonly moverRef: object; readonly targetOutput: string; readonly targetWorkspace: string },
    ): void {
        const moverRef = lag.moverRef;
        const failSetter = (cause: string, outcome: string): void => {
            this.lifecycleDiag(flightState, "apply", "setters", "write-failed", cause, this.ordinaryRevision(planned, flightState));
            this.ordinaryTerminal(flightState, planned, "uncertain", "setters");
            this.failFlight(flightState, outcome);
        };
        const failObserve = (): void => {
            this.lifecycleDiag(flightState, "observe", "observe", "mismatched", "stale-scope", this.ordinaryRevision(planned, flightState));
            this.ordinaryTerminal(flightState, planned, "uncertain", "observe");
            this.failFlight(flightState, "stale-scope");
        };
        if (!crossOutputTransferSupported(this.env)) {
            failSetter("precondition-mismatch", "precondition-mismatch");
            return;
        }
        if (!isUniqueOwner(this.pinnedOwner)) {
            failSetter("owner-loss", "owner-loss");
            return;
        }
        if (flightState.correlation !== planned.correlationId) {
            failSetter("correlation-mismatch", "correlation-mismatch");
            return;
        }
        if (!this.inFlight || this.pending !== flightState || flightState.plannerSession !== this.plannerSession || flightState.epoch !== this.epoch) {
            failSetter("stale-scope", "stale-scope");
            return;
        }
        let targetOutputRef: object | null = null;
        let targetDesktopRef: object | null = null;
        try {
            targetOutputRef = this.env.resolveOutput?.(lag.targetOutput) ?? null;
            targetDesktopRef = this.env.resolveDesktop?.(lag.targetWorkspace) ?? null;
        } catch (error) {
            void error;
            targetOutputRef = null;
            targetDesktopRef = null;
        }
        if (targetOutputRef === null || targetDesktopRef === null) {
            failSetter("stale-scope", "stale-scope");
            return;
        }
        this.r4WriteDepth += 1;
        try {
            let transferred = false;
            try {
                transferred = this.env.sendClientToScreen?.(moverRef, targetOutputRef) === true;
            } catch (error) {
                void error;
                transferred = false;
            }
            if (!transferred) {
                failSetter("write-failed", "write-failed");
                return;
            }
            if (!this.inFlight || this.pending !== flightState || flightState.plannerSession !== this.plannerSession || flightState.epoch !== this.epoch) {
                failSetter("stale-scope", "stale-scope");
                return;
            }
            let membershipWritten = false;
            try {
                membershipWritten = this.env.setDesktops?.(moverRef, [targetDesktopRef]) === true;
            } catch (error) {
                void error;
                membershipWritten = false;
            }
            if (!membershipWritten) {
                failSetter("write-failed", "write-failed");
                return;
            }
        } finally {
            this.r4WriteDepth = Math.max(0, this.r4WriteDepth - 1);
        }
        let nativeOutput: string | null = null;
        let nativeIds: ReadonlyArray<string> | null = null;
        try {
            nativeOutput = this.env.readOutputName?.(moverRef) ?? null;
            nativeIds = this.env.readDesktopIds?.(moverRef) ?? null;
        } catch (error) {
            void error;
            nativeOutput = null;
            nativeIds = null;
        }
        if (nativeOutput !== lag.targetOutput || nativeIds === null || nativeIds.length !== 1 || nativeIds[0] !== lag.targetWorkspace) {
            failObserve();
            return;
        }
        // Post-transfer proof must be an actual fresh complete destination
        // observation (hidden, or the foreground when it already shows the
        // destination). Never fabricate: absence fails without applied claim.
        let actual: PlanObserved | null = null;
        try {
            actual = this.freshHiddenFor({ domainOutput: lag.targetOutput, domainWorkspace: lag.targetWorkspace });
            if (actual === null) {
                const foreground = this.freshObserved();
                if (
                    foreground !== null &&
                    foreground.domainOutput === lag.targetOutput &&
                    foreground.domainWorkspace === lag.targetWorkspace
                ) {
                    actual = foreground;
                }
            }
        } catch (error) {
            void error;
            actual = null;
        }
        if (actual === null || actual.domainOutput !== lag.targetOutput || actual.domainWorkspace !== lag.targetWorkspace) {
            failObserve();
            return;
        }
        const movers = actual.windows.filter((entry) => entry.id === lag.moverId);
        if (movers.length !== 1) {
            failObserve();
            return;
        }
        const mover = movers[0] as PlanObservedWindow;
        if (
            mover.ref !== moverRef ||
            mover.output !== lag.targetOutput ||
            mover.workspace !== lag.targetWorkspace ||
            mover.fullscreen ||
            mover.maximized ||
            mover.floating === true ||
            mover.sticky === true
        ) {
            failObserve();
            return;
        }
        let actualSnapshot: PlanSnapshot;
        try {
            actualSnapshot = this.carriedSnapshot(actual);
        } catch (error) {
            void error;
            failObserve();
            return;
        }
        // Focus/fingerprint are arrival-transparent (the destination keeps
        // its own focus): fence survivors/membership only, like the
        // prewrite, by binding the comparison to the flight focus print.
        const comparable: PlanSnapshot = {
            ...actualSnapshot,
            focusedId: flightState.snapshot.focusedId,
            fingerprint: flightState.snapshot.fingerprint,
        };
        if (
            !rectsEqualExceptSource(comparable, flightState.snapshot, lag.moverId) ||
            unexpectedOverlaySkewed(flightState, comparable)
        ) {
            failObserve();
            return;
        }
        this.lifecycleDiag(flightState, "observe", "observe", "matched", "-", this.ordinaryRevision(planned, flightState));
        this.writeGeometries(planned, flightState, actual);
    }

    // Fresh observed mover proof for follow: exact scope identity plus mover
    // absent from source and present on target in a fresh directional
    // observation, with matching ref identity and fresh native placement on
    // the freshly resolved ref. Never trusts the retained moverRef alone.
    // Uses raw re-observation (not dispatch validation) so the intended
    // target arrival passes despite moved focus. Returns false while arrival
    // is still pending so delayed arrival stays waiting; callers must not
    // treat false as terminal.
    private r4ObservedMoverOnTarget(r4: R4Flight): boolean {
        if (!isDirection(r4.direction)) {
            return false;
        }
        const fresh = this.r4RawFresh(r4.direction, r4.snapshot);
        if (fresh === null) {
            return false;
        }
        const flightSnapshot = r4.snapshot;
        const domains = flightSnapshot.domains;
        if (domains === undefined || domains.length !== 2) {
            return false;
        }
        const target = domains[1] as PlanDomain;
        if (target.output !== r4.targetOutput || target.workspace !== r4.targetWorkspace) {
            return false;
        }
        if (
            !domainsEqual(fresh.domains, flightSnapshot.domains) ||
            !sameDomainScalars(fresh, flightSnapshot) ||
            !sameDomainBounds(fresh, flightSnapshot)
        ) {
            return false;
        }
        if (fresh.windows.length !== flightSnapshot.windows.length) {
            return false;
        }
        const flightById = new Map<string, PlanSnapshotWindow>();
        for (const entry of flightSnapshot.windows) {
            flightById.set(entry.id, entry);
        }
        let moverRef: object | null = null;
        for (const entry of fresh.windows) {
            const expected = flightById.get(entry.id);
            if (expected === undefined) {
                return false;
            }
            if (entry.id === r4.moverId) {
                if (entry.output !== target.output || entry.workspace !== target.workspace) {
                    return false;
                }
                if (entry.fullscreen || entry.maximized || entry.floating === true || entry.sticky === true) {
                    return false;
                }
                moverRef = entry.ref;
            } else {
                if (entry.output !== expected.output || entry.workspace !== expected.workspace) {
                    return false;
                }
                if (
                    entry.fullscreen !== expected.fullscreen ||
                    entry.maximized !== expected.maximized ||
                    (entry.floating === true) !== (expected.floating === true) ||
                    (entry.sticky === true) !== (expected.sticky === true)
                ) {
                    return false;
                }
            }
        }
        if (moverRef === null || moverRef !== r4.moverRef) {
            return false;
        }
        let output: string | null = null;
        try {
            output = this.env.readOutputName?.(moverRef) ?? null;
        } catch (error) {
            void error;
            return false;
        }
        if (output !== r4.targetOutput) {
            return false;
        }
        let ids: ReadonlyArray<string> | null = null;
        try {
            ids = this.env.readDesktopIds?.(moverRef) ?? null;
        } catch (error) {
            void error;
            return false;
        }
        if (ids === null || ids.length !== 1 || ids[0] !== r4.targetWorkspace) {
            return false;
        }
        // Mover absent from source is implied by the single exact target
        // entry above plus unchanged non-mover placements; explicitly guard
        // the source domain identity already pinned in domainsEqual.
        return true;
    }

    private r4MoverPlacementMatches(r4: R4Flight): boolean {
        let output: string | null = null;
        try {
            output = this.env.readOutputName?.(r4.moverRef) ?? null;
        } catch (error) {
            void error;
            return false;
        }
        if (output !== r4.targetOutput) {
            return false;
        }
        let ids: ReadonlyArray<string> | null = null;
        try {
            ids = this.env.readDesktopIds?.(r4.moverRef) ?? null;
        } catch (error) {
            void error;
            return false;
        }
        if (ids === null || ids.length !== 1 || ids[0] !== r4.targetWorkspace) {
            return false;
        }
        return true;
    }

    private checkR4Arrived(r4: R4Flight): boolean {
        return this.r4MoverPlacementMatches(r4);
    }

    private r4PlacementDetail(r4: R4Flight): "arrived" | "source" | "half" | "wrong-target" | "unreadable" {
        let output: string | null = null;
        let ids: ReadonlyArray<string> | null = null;
        try {
            output = this.env.readOutputName?.(r4.moverRef) ?? null;
            ids = this.env.readDesktopIds?.(r4.moverRef) ?? null;
        } catch (error) {
            void error;
            return "unreadable";
        }
        if (output === null || ids === null) {
            return "unreadable";
        }
        const domains = r4.snapshot.domains as ReadonlyArray<PlanDomain> | undefined;
        const source = domains !== undefined && domains.length === 2 ? (domains[0] as PlanDomain) : null;
        const outputIsTarget = output === r4.targetOutput;
        const desktopsIsTarget = ids.length === 1 && ids[0] === r4.targetWorkspace;
        if (outputIsTarget && desktopsIsTarget) {
            return "arrived";
        }
        const outputIsSource = source !== null && output === source.output;
        const desktopsIsSource = source !== null && ids.length === 1 && ids[0] === source.workspace;
        if (outputIsSource && desktopsIsSource) {
            return "source";
        }
        if ((outputIsTarget || outputIsSource) && (desktopsIsTarget || desktopsIsSource)) {
            return "half";
        }
        return "wrong-target";
    }

    private armR4ArrivalSignals(r4: R4Flight, flight: number, session: number): boolean {
        try {
            const detachOutput = this.env.subscribeMoverOutput?.(r4.moverRef, () => this.onR4ArrivalSignal(flight, session));
            if (detachOutput === null || detachOutput === undefined || typeof detachOutput !== "function") {
                return false;
            }
            r4.detaches.push(detachOutput);
        } catch (error) {
            void error;
            return false;
        }
        try {
            const detachDesktops = this.env.subscribeMoverDesktops?.(r4.moverRef, () => this.onR4ArrivalSignal(flight, session));
            if (detachDesktops === null || detachDesktops === undefined || typeof detachDesktops !== "function") {
                return false;
            }
            r4.detaches.push(detachDesktops);
        } catch (error) {
            void error;
            return false;
        }
        return true;
    }

    private onR4ArrivalSignal(flight: number, session: number): void {
        if (this.r4WriteDepth > 0) {
            return;
        }
        const r4 = this.r4Current();
        if (r4 === null || flight !== r4.flight || session !== r4.session || r4.settled) {
            return;
        }
        if (this.checkR4Arrived(r4)) {
            this.followR4Once(r4);
            return;
        }
        const placement = this.r4PlacementDetail(r4);
        if (placement === "wrong-target" || placement === "unreadable") {
            const placed = placement === "wrong-target" ? "wrong-output" : "stale-scope";
            this.r4FenceDiag(r4.correlation, "arrival-signal-placement", placed);
            this.r4SettleTerminal(r4, placed);
        }
    }

    private followR4Once(r4: R4Flight): void {
        if (r4.followed || r4.settled) {
            return;
        }
        if (!this.checkR4Arrived(r4)) {
            return;
        }
        if (!this.r4ObservedMoverOnTarget(r4)) {
            return;
        }
        let currentActive: object | null = null;
        try {
            currentActive = this.env.active();
        } catch (error) {
            void error;
            currentActive = null;
        }
        if (currentActive === r4.moverRef) {
            if (!this.checkR4Arrived(r4) || !this.r4ObservedMoverOnTarget(r4)) {
                return;
            }
            r4.followed = true;
            this.diag(r4.op, r4.correlation, r4.planned.geometry.length, "r4-followed");
            this.r4SettleTerminal(r4, "arrived");
            return;
        }
        if (!this.checkR4Arrived(r4)) {
            return;
        }
        if (!this.r4ObservedMoverOnTarget(r4)) {
            return;
        }
        if (!isUniqueOwner(this.pinnedOwner) || !isGeneration(this.generation)) {
            return;
        }
        if (this.r4Current() !== r4 || r4.flight !== this.activeToken || r4.session !== this.plannerSession || r4.epoch !== this.epoch) {
            return;
        }
        let focused = false;
        this.r4WriteDepth += 1;
        try {
            focused = this.env.setActive(r4.moverRef) === true;
        } catch (error) {
            void error;
            focused = false;
        } finally {
            this.r4WriteDepth = Math.max(0, this.r4WriteDepth - 1);
        }
        let after: object | null = null;
        try {
            after = this.env.active();
        } catch (error) {
            void error;
            after = null;
        }
        r4.followed = true;
        if (!focused || after !== r4.moverRef) {
            this.logToken(`${LOG_PREFIX}:r4-follow correlation=${r4.correlation} outcome=focus-unconfirmed`);
            this.r4SettleTerminal(r4, "focus-unconfirmed");
            return;
        }
        this.diag(r4.op, r4.correlation, r4.planned.geometry.length, "r4-followed");
        this.r4SettleTerminal(r4, "arrived");
    }

    private onR4ArrivalTimeout(flight: number, session: number): void {
        const r4 = this.r4Flight;
        if (r4 === null || r4.settled || flight !== r4.flight || session !== r4.session || !this.inFlight) {
            return;
        }
        if (this.checkR4Arrived(r4)) {
            this.followR4Once(r4);
            return;
        }
        this.r4SettleTerminal(r4, "arrival-timeout");
    }

    // Narrow fence evidence for R4 terminal diagnosis: one bounded line
    // naming the rejecting fence before settlement. Diagnostic only; never
    // changes control flow. Lets live review distinguish scope drift from
    // self-invalidation without inferring from the terminal outcome alone.
    private r4FenceDiag(correlation: string, fence: string, outcome: string): void {
        this.logToken(`${LOG_PREFIX}:r4-fence correlation=${correlation} fence=${fence} outcome=${outcome}`);
    }

    private r4SettleTerminal(r4: R4Flight, outcome: string): void {
        if (r4.settled) {
            return;
        }
        r4.settled = true;
        const op = r4.op;
        const correlation = r4.correlation;
        const count = r4.planned.geometry.length;
        const sourceTarget = (() => {
            try {
                const domains = r4.snapshot.domains as ReadonlyArray<PlanDomain> | undefined;
                if (domains === undefined || domains.length !== 2) {
                    return null;
                }
                const source = domains[0] as PlanDomain;
                const target = domains[1] as PlanDomain;
                return { source, target };
            } catch (error) {
                void error;
                return null;
            }
        })();
        this.clearR4Flight();
        this.clearR4ArrivalTimer();
        this.clearTimer();
        this.inFlight = false;
        this.pending = null;
        this.pinnedOwner = null;
        this.activationStep = 0;
        this.r4WriteDepth = 0;
        this.diag(op, correlation, count, outcome);
        if (outcome === "arrived") {
            try {
                this.env.onPlannedApplied?.(op);
            } catch (error) {
                void error;
            }
        } else {
            this.noteReconcileTerminal(op);
        }
        if (sourceTarget !== null) {
            try {
                this.notifySendSettled({
                    sourceOutput: sourceTarget.source.output,
                    sourceWorkspace: sourceTarget.source.workspace,
                    targetOutput: sourceTarget.target.output,
                    targetWorkspace: sourceTarget.target.workspace,
                });
            } catch (error) {
                void error;
            }
        } else {
            try {
                this.requestResync();
            } catch (error) {
                void error;
            }
        }
    }

    private clearR4ArrivalTimer(): void {
        const cancel = this.r4ArrivalTimer;
        this.r4ArrivalTimer = null;
        if (cancel !== null) {
            try {
                cancel();
            } catch (error) {
                void error;
            }
        }
    }

    private clearR4Flight(): void {
        const r4 = this.r4Flight;
        this.r4Flight = null;
        if (r4 !== null) {
            for (const detach of r4.detaches) {
                try {
                    detach();
                } catch (error) {
                    void error;
                }
            }
            r4.detaches.length = 0;
        }
    }

    private failFlight(flightState: PendingFlight, outcome: string): void {
        if (flightState.plannerSession !== this.plannerSession) {
            return;
        }
        this.terminateFlight(flightState, outcome);
    }

    // Shared terminal core: exactly the historical failFlight teardown. R4-shape
    // flights additionally force both domains through the settlement chain.
    private terminateFlight(flightState: PendingFlight, outcome: string): void {
        if (flightState.plannerSession !== this.plannerSession) {
            return;
        }
        if (flightState.op === "release-domain") {
            this.settleRelease(flightState.correlation, outcome);
        }
        this.clearR4Flight();
        this.clearTimer();
        this.clearR4ArrivalTimer();
        this.inFlight = false;
        this.pending = null;
        this.pinnedOwner = null;
        this.activationStep = 0;
        this.diag(flightState.op, flightState.correlation, flightState.windowCount, outcome);
        if (flightState.background === true) {
            this.noteBackgroundTerminal(flightState.snapshot);
        } else {
            this.noteReconcileTerminal(flightState.op, flightState.workAreaReprojection);
        }
        this.settleDragTerminal(flightState, outcome);
        // Ambiguous terminal failures (timeout already probed via onTimeout;
        // service-fault, correlation mismatch, precondition mismatch, stale
        // scope, write failure, owner loss) may lead to one bounded identity
        // probe. Only absence or changed owner recovers; same owner retains.
        this.maybeProbeAfterTerminal(flightState);
        // R4-shape terminals force complete source AND target reconciliation
        // through the existing single-flight chain, including equal evidence.
        this.forceR4SettleFromPending(flightState);
        this.finishFlight();
    }

    // After every flight, exactly one deferred signal-driven auto command
    // runs so admit/remove converge without queues or retries. When idle with
    // nothing deferred, one hidden-domain step chains so background domains
    // converge across successive flights without polling or new timers.
    private finishFlight(): void {
        if (!this.enabled) {
            return;
        }
        // Do not let a deferred foreground or hidden-domain command race the
        // one bounded identity check for a terminal Planner transport fault.
        if (this.activeProbe !== 0) {
            return;
        }
        const next = this.deferredAuto;
        this.deferredAuto = null;
        if (next !== null && !this.inFlight) {
            if (
                next.op === "reconcile" &&
                next.workAreaReprojection !== true &&
                this.interactiveResizeActive()
            ) {
                // Interactive guard drops the deferred intent without
                // touching markers: a deferred drag pointer folded into its
                // marker persists there, and any pending marker dispatches
                // once the slot is free again.
                this.absorbDeferredDragIntent(next, true);
                return;
            }
            this.dispatch(next);
        }
        // Converge one pending restore marker when idle: exactly one
        // existing-route reconcile per marker, unless an applied plan
        // already satisfied it. No retries, no queues, no slot bypass.
        if (!this.inFlight && this.deferredAuto === null && this.r4Flight === null && this.activeProbe === 0) {
            this.maybeDispatchDragRestore();
        }
        if (!this.inFlight && this.deferredAuto === null && !this.chainingHidden) {
            this.chainingHidden = true;
            try {
                this.refreshHiddenNow();
            } finally {
                this.chainingHidden = false;
            }
        }
        // Queued explicit releases own the next idle slot. Deferred auto,
        // drag markers, and hidden steps above take precedence within this
        // chain; the pump no-ops while any of them hold the slot.
        this.pumpReleaseQueue();
    }

    private clearTimer(): void {
        const cancel = this.cancelTimer;
        this.cancelTimer = null;
        if (cancel !== null) {
            try {
                cancel();
            } catch (error) {
                void error;
            }
        }
    }

    private clearProbeTimer(): void {
        const cancel = this.probeCancel;
        this.probeCancel = null;
        if (cancel !== null) {
            try {
                cancel();
            } catch (error) {
                void error;
            }
        }
    }

    private clearDebounce(): void {
        const cancel = this.debounceCancel;
        this.debounceCancel = null;
        if (cancel !== null) {
            try {
                cancel();
            } catch (error) {
                void error;
            }
        }
    }

    private diag(op: PlanOp, correlation: string, windows: number, outcome: string): void {
        if (outcome === "dispatch" && !KWIN_TRACE_ENABLED) {
            return;
        }
        try {
            this.env.log(`${LOG_PREFIX}:cmd=${correlation} kind=${op} windows=${String(windows)} outcome=${outcome}`);
        } catch (error) {
            void error;
        }
    }

    // Normal-level lifecycle context for cancellable R4 flights. Retains the
    // established plan:cmd prefix while making the route independently
    // attributable without payload or trace logging.
    // Normal-level correlated activation transport diagnostics (logging
    // only). One bounded `lifecycleDiag` line per accepted activation
    // boundary: presence lookup response, start request/result, unique-owner
    // resolution/pin, planner request handoff, and activation-phase timeout.
    // Reuses correlation/generation/request-revision plus the shared
    // component/route mapping. Never carries raw owners, services,
    // exceptions, payloads, or native ids. Pre-call records mark initiation;
    // callback records follow flight/token/session/step fencing. Stale, late,
    // or duplicate callbacks return before logging and never look successful.
    // Best effort: logger faults are swallowed and never change control flow.
    private activateDiag(flight: PendingFlight, event: string, outcome: string, cause = "-"): void {
        this.lifecycleDiag(flight, "activate", event, outcome, cause);
    }

    private lifecycleDiag(
        flight: PendingFlight,
        stage: string,
        event: string,
        outcome: string,
        cause: string,
        revision: number | string = flight.requestRevision,
    ): void {
        try {
            const isR4 =
                flight.op === "move" &&
                (flight.direction === "left" || flight.direction === "right") &&
                flight.snapshot.domains !== undefined &&
                flight.snapshot.domains.length === 2;
            const revisionText = typeof revision === "number" ? String(revision) : revision;
            this.env.log(
                `${LOG_PREFIX}:cmd=${flight.correlation} kind=${flight.op} windows=${String(flight.windowCount)} component=${isR4 ? "cosmic-directional" : "cosmic-plan"} route=${isR4 ? "directional-r4" : "plan"} stage=${stage} correlation=${flight.correlation} generation=${this.generation} revision=${revisionText} event=${event} outcome=${sanitizeKind(outcome)} cause=${cause === "-" ? "-" : sanitizeKind(cause)}`,
            );
        } catch (error) {
            void error;
        }
    }

    // Ordinary Plan reply/apply lifecycle (logging only, after Planner send).
    // Truthful revision: the request revision before a planned reply is known;
    // the planned base revision after validation, or the unavailable placeholder
    // when the validated reply omits it. Never carries ids, owners, payloads,
    // geometry, or exceptions. Late/stale/duplicate callbacks return before any
    // line and never look successful. The ordinary route has no ack/verify:
    // only R4 stages those, so no ack/verify lines exist here.
    private ordinaryRevision(planned: PlannedReply | null, flight: PendingFlight): number | string {
        if (planned === null) {
            return flight.requestRevision;
        }
        return planned.baseRevision === null ? "unavailable" : planned.baseRevision;
    }

    private ordinaryTerminal(
        flight: PendingFlight,
        planned: PlannedReply | null,
        outcome: string,
        lastPhase: string,
    ): void {
        this.lifecycleDiag(flight, "terminal", "settled", outcome, lastPhase, this.ordinaryRevision(planned, flight));
    }

    // Bounded aggregate skip summary for one ordinary geometry application.
    // One fixed token, never per-window geometry or ids.
    private skipSummary(kinds: ReadonlySet<string>): string {
        if (kinds.size === 0) {
            return "-";
        }
        if (kinds.size > 1) {
            return "skipped-mixed";
        }
        for (const kind of kinds) {
            return kind;
        }
        return "-";
    }

    private logToken(message: string): void {
        try {
            this.env.log(message);
        } catch (error) {
            void error;
        }
    }

    private writeDiag(window: string, resourceClass: string, disposition: string, rect: PlanRect): void {
        if (!KWIN_TRACE_ENABLED && disposition !== "write-failed" && disposition !== "float-write-failed") {
            return;
        }
        this.logToken(
            `${LOG_PREFIX}:write window=${window} resource_class=${resourceClass} disposition=${disposition} rect=${String(rect.x)},${String(rect.y)},${String(rect.w)},${String(rect.h)}`,
        );
    }

    private workAreaFor(snapshot: PlanSnapshot, output: string, workspace: string): PlanRect | null {
        if (snapshot.domainOutput === output && snapshot.domainWorkspace === workspace) {
            return snapshot.domainBounds;
        }
        return snapshot.domains?.find((domain) => domain.output === output && domain.workspace === workspace)?.bounds ?? null;
    }

    private traceConstraints(
        correlation: string,
        phase: "plan" | "write" | "post-signal",
        window: string,
        output: string,
        resourceClass: string,
        ref: object,
        workArea: PlanRect | null,
        requested: PlanRect | null,
        observed: PlanRect | null,
    ): void {
        if (!KWIN_TRACE_ENABLED || typeof this.env.readWindowConstraints !== "function") {
            return;
        }
        let constraints: PlanWindowConstraints | null = null;
        let actual = observed;
        try {
            constraints = this.env.readWindowConstraints(ref);
        } catch (error) {
            void error;
        }
        try {
            actual = this.env.readGeometry?.(ref) ?? actual;
        } catch (error) {
            void error;
        }
        const size = (value: { readonly w: number; readonly h: number } | null): string =>
            value === null ? "unknown" : `${String(value.w)},${String(value.h)}`;
        const rect = (value: PlanRect | null): string =>
            value === null ? "unknown" : `${String(value.x)},${String(value.y)},${String(value.w)},${String(value.h)}`;
        const resizeable = constraints?.resizeable === true ? "true" : constraints?.resizeable === false ? "false" : "unknown";
        this.logToken(
            `${LOG_PREFIX}:constraint-trace corr=${correlation} phase=${phase} window=${window} output=${output} resource_class=${resourceClass} resizeable=${resizeable} min=${size(constraints?.minSize ?? null)} max=${size(constraints?.maxSize ?? null)} workarea=${rect(workArea)} requested=${rect(requested)} observed=${rect(actual)}`,
        );
    }

    private rejectKind(kind: string, detail: string | null): void {
        try {
            const suffix = kind === "snapshot-invalid" && detail !== null ? ` detail=${detail}` : "";
            this.env.log(`${LOG_PREFIX}:rejected kind=${kind}${suffix}`);
        } catch (error) {
            void error;
        }
    }

    private domainKey(snapshot: Pick<PlanSnapshot, "domainOutput" | "domainWorkspace">): string {
        return `${snapshot.domainOutput}\u0000${snapshot.domainWorkspace}`;
    }

    private isTiledDomain(output: string, workspace: string): boolean {
        try {
            const gate = this.env.isDomainTiled;
            if (typeof gate !== "function") {
                return true;
            }
            return gate(output, workspace) !== false;
        } catch (error) {
            void error;
            return true;
        }
    }

    // Explicit no-write domain release. The snapshot must be a complete
    // observation; the Planner drops the domain with no geometry.
    requestDomainRelease(snapshot: PlanSnapshot, onSettled?: (outcome: string) => void): void {
        if (!this.enabled) {
            try {
                onSettled?.("disabled");
            } catch (error) {
                void error;
            }
            return;
        }
        this.releaseQueue.push({ snapshot, onSettled });
        this.pumpReleaseQueue();
    }

    private pumpReleaseQueue(): void {
        if (!this.enabled || this.inFlight || this.r4Flight !== null) {
            return;
        }
        // A deferred auto intent owns the next slot; releases wait for it.
        if (this.deferredAuto !== null) {
            return;
        }
        const next = this.releaseQueue.shift();
        if (next === undefined) {
            return;
        }
        this.dispatch({
            op: "release-domain",
            snapshot: next.snapshot,
            removed: null,
            body: { op: "release-domain" },
        });
        const flight = this.pending;
        if (flight !== null && flight.op === "release-domain" && next.onSettled !== undefined) {
            const callback = next.onSettled;
            this.releaseCallbacks.set(flight.correlation, callback);
        } else if (next.onSettled !== undefined) {
            try {
                next.onSettled("dispatch-failed");
            } catch (error) {
                void error;
            }
            this.pumpReleaseQueue();
        }
    }

    private settleRelease(correlation: string, outcome: string): void {
        const callback = this.releaseCallbacks.get(correlation);
        if (callback === undefined) {
            return;
        }
        this.releaseCallbacks.delete(correlation);
        try {
            callback(outcome);
        } catch (error) {
            void error;
        }
    }

    private dropAppliedForDomain(output: string, workspace: string): void {
        const key = this.domainKey({ domainOutput: output, domainWorkspace: workspace });
        this.appliedScopeByDomain.delete(key);
        for (const [id, evidence] of [...this.appliedById]) {
            if (evidence.output === output && evidence.workspace === workspace) {
                this.appliedById.delete(id);
            }
        }
        this.sendForcedDomains.delete(key);
        this.backgroundAttempts.delete(key);
        // D23: a released domain drops its planner slot, so pending closes
        // and empty proofs homed there are moot, never retained.
        this.explicitEmptyByKey.delete(key);
        try {
            for (const [id, homing] of [...this.pendingClosed]) {
                if (homing.output === output && homing.workspace === workspace) {
                    this.pendingClosed.delete(id);
                }
            }
        } catch (error) {
            void error;
        }
    }

    private handleReleaseReply(parsed: Record<string, unknown>, flightState: PendingFlight): boolean {
        if (parsed["outcome"] !== "released") {
            return false;
        }
        if (parsed["v"] !== PLAN_CONTRACT_VERSION) {
            this.lifecycleDiag(flightState, "reply", "validate", "malformed", "service-fault");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.failFlight(flightState, "service-fault");
            return true;
        }
        if (parsed["correlation_id"] !== flightState.correlation) {
            this.lifecycleDiag(flightState, "reply", "validate", "stale", "correlation-mismatch");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.failFlight(flightState, "correlation-mismatch");
            return true;
        }
        const kind = parsed["kind"];
        const detail = parsed["detail"];
        const detailKind = isRecord(detail) ? detail["kind"] : undefined;
        if (kind !== "release-domain" || detailKind !== "release-domain") {
            this.lifecycleDiag(flightState, "reply", "validate", "malformed", "service-fault");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.failFlight(flightState, "service-fault");
            return true;
        }
        if (flightState.epoch !== this.epoch) {
            this.lifecycleDiag(flightState, "reply", "validate", "stale", "stale-dropped");
            this.ordinaryTerminal(flightState, null, "uncertain", "validate");
            this.settleRelease(flightState.correlation, "stale-dropped");
            this.clearTimer();
            this.inFlight = false;
            this.pending = null;
            this.pinnedOwner = null;
            this.activationStep = 0;
            this.diag(flightState.op, flightState.correlation, flightState.windowCount, "stale-dropped");
            this.finishFlight();
            this.pumpReleaseQueue();
            return true;
        }
        // Confirmed release: drop the exact domain's applied evidence with
        // zero native writes. A later ordinary fresh reconcile re-adopts
        // current geometry through the existing fit/seed route.
        this.dropAppliedForDomain(flightState.snapshot.domainOutput, flightState.snapshot.domainWorkspace);
        if (isUniqueOwner(this.pinnedOwner)) {
            this.knownOwner = this.pinnedOwner;
        }
        this.lifecycleDiag(flightState, "reply", "validate", "validated", "-", flightState.requestRevision);
        this.ordinaryTerminal(flightState, null, "released", "validate");
        this.settleRelease(flightState.correlation, "released");
        this.clearTimer();
        this.inFlight = false;
        this.pending = null;
        this.pinnedOwner = null;
        this.activationStep = 0;
        this.diag(flightState.op, flightState.correlation, flightState.windowCount, "released");
        this.finishFlight();
        this.pumpReleaseQueue();
        return true;
    }
}
