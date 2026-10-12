// Bounded COSMIC send-to-workspace adapter (standalone, dev-only, disabled by
// default).
//
// Product-shaped but with no normal startup route: ordinary production startup
// never runs this module (src/entry.ts must not import it and no controller
// route may import it; the module constraints below are the explicit wiring
// contract). The only activation is the explicit exported
// WorkspaceSendAdapter class plus the separate entry helper, called by no
// production source.
//
// This adapter carries the existing portable COSMIC same-output
// distinct-workspace send operation over the ONE existing DescribePlan D-Bus
// transport. Rust owns all planning, topology, membership, focus, and rejection
// policy and commits the planned topology synchronously in the same call,
// returning both-domain geometry plus the native assignment. This module owns
// KWin observation of the focused output's source and target desktops, owner
// activation/pinning, a single request round trip, exact source+target
// re-validation, synchronous planned-geometry writes then the mover's
// Window.desktops membership write, and exactly one follow (target desktop
// switch then mover focus, no retry) on a fresh exact native membership proof:
// mover absent from source and present on target. The proof comes from an
// immediate post-write observation AND a one-shot mover desktopsChanged signal
// for delayed arrival. There is no ack/verify/cancel/abandon/status protocol,
// no per-window geometry echo, no settlement timer, and no blocksPlan: Plan is
// never blocked by a send flight.
//
// Source and target stay pinned from dispatch until arrival or a bounded
// arrival deadline; a separate unanswered-request deadline releases the flight
// and late replies are ignored by token. Every terminal path (arrival, failed
// native write, stale reply, rejected/diverged reply, missing callback,
// timeout, closed mover, disable with a live flight) clears the pin and calls
// exactly one entry-owned `onSettled` hook for a forced complete source AND
// target refresh through the single-flight Plan chain. Pre-flight refusals
// carry no hook and leave no flight.
//
// Activation first uses NameHasOwner's normal boolean reply. A present owner is
// resolved and pinned; only a confirmed absent name runs one
// StartServiceByName(service, 0) phase accepting 1 PrimaryOwner / 2
// AlreadyOwner, followed by one post-start GetNameOwner. Every planner call
// targets that pinned unique `:N.M` owner. Two one-shot timers, no retry, no
// polling. Same-UID authorization stays solely the Planner's existing single
// check.
//
// Refusal routes are exact bounded tokens: no-planner, owner-loss,
// stale-revision, cross-output, same-output, same-workspace, target-mismatch,
// transfer-unavailable, absent-focus, non-tiled-focus, desktop-cap (observed
// desktop count above the KWin cap of 25), and last-desktop (no distinct
// target desktop can exist). `cross-output` guards the same-output route;
// `same-output` guards the explicit output-send route (use workspace send);
// `target-mismatch` is a drifted output-send destination; `transfer-unavailable`
// is missing output-transfer capabilities. Pre-flight refusals
// (scope-invalid plus the requestSend validation tokens above) log one
// structured best-effort `omnitiler:route-diag` line and return false
// with the adapter still enabled, so a later valid send can proceed.
//
// All logs carry fixed fields (component, stage, correlation, generation,
// revision, event, outcome) with no sensitive, native, or payload data.

import { DOMAIN_GAP_DEFAULT, DomainGaps, normalizeGap, OUTER_DOMAIN_GAP_DEFAULT, readDomainGaps } from "./domain-gap";
import { orderGeometryWrites } from "./geometry-order";

export const WORKSPACE_SEND_SERVICE = "com.omnitiler.Planner";
export const WORKSPACE_SEND_OBJECT = "/com/omnitiler/Planner";
export const WORKSPACE_SEND_INTERFACE = "com.omnitiler.Planner1";
export const WORKSPACE_SEND_METHOD = "DescribePlan";

// Session D-Bus activation transport (one-flight, bounded, no poll/retry).
// NameHasOwner observes presence without relying on GetNameOwner's error reply,
// which KWin Script does not deliver to callbacks. A present name is resolved
// and pinned; an absent name runs exactly one StartServiceByName(service, 0)
// phase accepting only result codes 1 (PrimaryOwner) / 2 (AlreadyOwner), then
// resolves and pins one unique owner before any planner call. Flags value 0 is
// fixed; the entry appends it as the second native D-Bus argument.
export const WORKSPACE_SEND_DBUS_SERVICE = "org.freedesktop.DBus";
export const WORKSPACE_SEND_DBUS_OBJECT = "/org/freedesktop/DBus";
export const WORKSPACE_SEND_DBUS_INTERFACE = "org.freedesktop.DBus";
export const WORKSPACE_SEND_HAS_OWNER_METHOD = "NameHasOwner";
export const WORKSPACE_SEND_GET_OWNER_METHOD = "GetNameOwner";
export const WORKSPACE_SEND_START_METHOD = "StartServiceByName";
export const WORKSPACE_SEND_START_FLAGS = 0;
export const WORKSPACE_SEND_START_PRIMARY = 1;
export const WORKSPACE_SEND_START_ALREADY = 2;

export const WORKSPACE_SEND_CONTRACT_VERSION = 1;
// Planner request byte cap (mirrors the Rust codec `PLAN_MAX_REQUEST_BYTES`).
// `.length` is an exact byte count here: every string reaching request JSON
// is a fixed literal or passes isOpaqueId/isGeneration/isCorrelationId
// (charsets [A-Za-z0-9_.-] / [a-z0-9-], all <= 0x7F), and JSON numbers,
// booleans, and escapes are ASCII-only. Non-ASCII can never reach a request
// payload: validation rejects it before any build.
export const WORKSPACE_SEND_MAX_REQUEST_BYTES = 1_048_576;
export const WORKSPACE_SEND_MAX_REPLY_BYTES = 64 * 1024;
export const WORKSPACE_SEND_TIMEOUT_MS = 5000;
export const WORKSPACE_SEND_MAX_CORRELATION_LEN = 128;
export const WORKSPACE_SEND_MAX_OWNER_LEN = 128;
export const WORKSPACE_SEND_MAX_GENERATION_LEN = 64;
export const WORKSPACE_SEND_MAX_REVISION = 1000000;
export const WORKSPACE_SEND_MAX_ID_LEN = 128;
// KWin's hard desktop cap: an observation reporting over 25 desktops is
// refused fail-closed with the desktop-cap token. Window sets are not
// count-gated; an oversize built request is refused by the request byte cap.
export const WORKSPACE_SEND_MAX_DESKTOPS = 25;
export const WORKSPACE_SEND_MAX_SEQ = 1000000;

export const WORKSPACE_SEND_COMPONENT = "cosmic-send";

const LOG_PREFIX = "omnitiler:route-diag";

export interface WorkspaceSendRect {
    readonly x: number;
    readonly y: number;
    readonly w: number;
    readonly h: number;
}

export interface WorkspaceSendObservedWindow {
    readonly id: string;
    readonly ref: object;
    readonly rect: WorkspaceSendRect;
    // Complete-observation flags from the send observers (standalone and
    // production). Optional so legacy synthetic fixtures without flags keep
    // validating: absence normalizes to false, never drops an observed true.
    readonly floating?: boolean;
    readonly fitExcluded?: boolean;
    readonly fit_excluded?: boolean;
    readonly fullscreen?: boolean;
    readonly maximized?: boolean;
    readonly sticky?: boolean;
}

export interface WorkspaceSendObserved {
    readonly sourceOutput: string;
    readonly sourceWorkspace: string;
    readonly sourceBounds: WorkspaceSendRect;
    readonly targetOutput: string;
    readonly targetWorkspace: string;
    readonly targetBounds: WorkspaceSendRect;
    readonly focusedId: string;
    readonly sourceWindows: ReadonlyArray<WorkspaceSendObservedWindow>;
    readonly targetWindows: ReadonlyArray<WorkspaceSendObservedWindow>;
    readonly activeRef: object | null;
    readonly moverRef: object | null;
    readonly targetDesktopRef: object | null;
    readonly targetExists: boolean;
    readonly desktopCount: number;
    readonly sourceFingerprint: string;
    readonly targetFingerprint: string;
    // Actual current desktop stable id on the recording output at
    // observation time, or null when unreadable. Behavior input for the
    // stay source-visibility fence only: unlike the pinned source identity
    // (which survives view switches) and the diagnostic-only cur_* equality
    // flags above (which compare only the target and never gate behavior),
    // this names the live view. Null never counts as selected: visibility
    // unreadable fails the stay fence closed, never assumes selected.
    readonly currentWorkspace: string | null;
    // Session-local redacted follow diagnostics only. tgt_* is the flight
    // target native mapping (targetOrdinal: index in the live `desktops` list
    // order, targetNumber: KWin native `x11DesktopNumber`, -1 when unreadable).
    // cur_* is the live current desktop for the selected output at observation
    // time (currentOrdinal/currentNumber likewise, currentIdEq: current stable
    // id === target stable id, currentRefEq: current wrapper === target
    // wrapper, diagnostic only). outputOrdinal is the index of the selected
    // output in `screens` (-1 when unreadable). Populated by production
    // observation; absent in dev harnesses and legacy isolated tests. Never
    // validated for correctness: invalid values sanitize to -1 in diagnostics
    // and never affect commit, follow, or enablement.
    readonly targetOrdinal?: number;
    readonly targetNumber?: number;
    readonly outputOrdinal?: number;
    readonly currentOrdinal?: number;
    readonly currentNumber?: number;
    readonly currentIdEq?: number;
    readonly currentRefEq?: number;
}

// Primitive-only snapshot retained across the async D-Bus boundary. Never
// holds Window objects, refs, or revalidation closures: ids, geometry values,
// scope, fingerprints, and complete-observation flags only. Targets are
// always resolved from a fresh synchronous observation while handling
// replies. Native fullscreen/maximized/sticky stay local snapshot semantics
// for the dispatch-frozen fence; only floating and fitExcluded ride the
// portable request wire (as `floating` / `fit_excluded`).
export interface WorkspaceSendSnapshotWindow {
    readonly id: string;
    readonly rect: WorkspaceSendRect;
    readonly floating: boolean;
    readonly fitExcluded: boolean;
    readonly fullscreen: boolean;
    readonly maximized: boolean;
    readonly sticky: boolean;
}

export interface WorkspaceSendSnapshot {
    readonly sourceOutput: string;
    readonly sourceWorkspace: string;
    readonly sourceBounds: WorkspaceSendRect;
    readonly targetOutput: string;
    readonly targetWorkspace: string;
    readonly targetBounds: WorkspaceSendRect;
    readonly focusedId: string;
    readonly sourceWindows: ReadonlyArray<WorkspaceSendSnapshotWindow>;
    readonly targetWindows: ReadonlyArray<WorkspaceSendSnapshotWindow>;
    readonly sourceFingerprint: string;
    readonly targetFingerprint: string;
}

export interface WorkspaceSendAdapterEnv {
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
    readonly observe: (targetWorkspace: string, pinnedSourceWorkspace?: string) => WorkspaceSendObserved | null;
    // Cross-output observation for explicit output send (REQ-OUT-04, item
    // 5.3/5.4): source output current workspace plus the named destination
    // output's current workspace with per-output windows. Resolves the
    // destination workspace once; a target workspace that is not the live
    // current desktop of the named output fails closed (null). The flight
    // pin (source output plus workspace, both frozen at dispatch) rides the
    // trailing parameters: deriving the source from the live active window
    // would collapse target===source once the still-active mover lands on
    // the destination, and geometry/arrival could never confirm. Absent in
    // legacy harnesses, which refuse output send at dispatch.
    readonly observeOutput?: (
        targetOutput: string,
        targetWorkspace: string,
        pinnedSourceWorkspace?: string,
        pinnedSourceOutput?: string,
    ) => WorkspaceSendObserved | null;
    // Single settlement edge for entry-owned coordination: invoked exactly
    // once on every terminal path that held a flight (arrival, failed native
    // write, stale/rejected reply, missing callback, deadline, closed mover,
    // disable with a live flight), carrying the exact terminal flight's
    // source/target domain keys. The entry runs one forced complete source
    // AND target refresh through the single-flight Plan chain; the adapter
    // never resets the Plan baseline itself and never claims a native commit.
    // Never invoked for pre-dispatch refusals, which hold no pin.
    readonly onSettled?: (settled: WorkspaceSendSettled) => void;
    readonly setGeometry: (target: object, rect: WorkspaceSendRect) => boolean;
    readonly readGeometry?: (target: object) => WorkspaceSendRect | null;
    readonly setDesktops: (target: object, refs: ReadonlyArray<object>) => boolean;
    // Current tiling-mode gate: false for an (output, workspace) domain that
    // toggled floating mid-flight means unmanaged, so the flight must issue
    // zero further geometry writes there and settle normally. Absent means
    // tiled (fail-open); exceptions fail open. Read at call time so a toggle
    // between dispatch and any native setter is observed.
    readonly isDomainTiled?: (output: string, workspace: string) => boolean;
    readonly switchToTarget?: (desktopRef: object, diagnostic: WorkspaceFollowNativeDiagnostic) => boolean;
    readonly focusWindow?: (windowRef: object, diagnostic: WorkspaceFollowNativeDiagnostic) => boolean;
    // Narrow mover desktop-change subscription seam for delayed arrival.
    // Production entries bind this to the mover Window.desktopsChanged
    // public signal via the shared signal-capability helpers. One-shot: the
    // adapter detaches after the first signal and on every terminal path.
    // Absent only in legacy isolated core tests, which retain the immediate
    // post-write observation path.
    readonly subscribeMoverDesktops?: (moverRef: object, handler: () => void) => (() => void) | null;
    // Synchronous desktop-membership read for the cross-output placement
    // arrival proof (read alongside the output on the dispatch-retained
    // mover ref). Production entries bind this to the Window.desktops
    // membership stable ids.
    readonly readDesktopIds?: (ref: object) => ReadonlyArray<string> | null;
    // Read-only live mover evidence for cross-output mid-write fences. The
    // dispatch-retained ref must still resolve to the identical live object
    // in the current window list with its native identity and current
    // exception flags, homed or not: a half-applied transfer is valid
    // mid-flight but a closed or replaced mover must fail closed before any
    // later setter. Null on anything unreadable. Never invents identity
    // (exact retained ref only). Absent hooks fail closed wherever the
    // unhomed path needs them.
    readonly readMoverLive?: (moverRef: object) => {
        readonly id: string;
        readonly floating: boolean;
        readonly sticky: boolean;
        readonly fullscreen: boolean;
        readonly maximized: boolean;
    } | null;
    // Cross-output transfer capabilities for explicit output send (REQ-OUT-04,
    // item 5.3/5.4). All seven must be present for output send; any absence
    // refuses at dispatch with `transfer-unavailable` and same-output
    // behavior is unchanged. Bound to public typed surfaces only: the output
    // observation hook, exact Output object resolution by name,
    // workspace.sendClientToScreen with the exact target Output object,
    // synchronous output and desktop reads for the placement arrival proof,
    // a read-only live mover hook for mid-write lifetime proofs, and a
    // one-shot outputChanged fence for delayed arrival.
    readonly resolveOutput?: (name: string) => object | null;
    readonly sendClientToScreen?: (mover: object, output: object) => boolean;
    readonly readOutputName?: (ref: object) => string | null;
    readonly subscribeMoverOutput?: (moverRef: object, handler: (old: unknown) => void) => (() => void) | null;
    // Whole-workspace output migration observation (R-WS-12): the source
    // output's current workspace (FULL view including sticky) plus the
    // adjacent target output's current workspace (affected-view gate and
    // drift fence), resolved once via FULL-rectangle adjacency. The flight
    // pin (direction plus frozen source/target pair) rides the trailing
    // parameter: deriving the source from the live active window would lose
    // the scope once members land on the destination. Absent in legacy
    // harnesses, which refuse migration at dispatch.
    readonly observeMigrate?: (
        direction: string,
        pinned?: WorkspaceMigratePin,
    ) => WorkspaceMigrateObserved | null;
    // Session workspace-map commit for migration: moves the backing
    // workspace id from the source output scope to after the target
    // output's current workspace. Returns the source refill id (null when
    // the source scope is empty) or null when refused. Map-only: native
    // views are written by the adapter through switchMigrateView.
    readonly commitWorkspaceMap?: (
        sourceOutput: string,
        targetOutput: string,
        workspaceId: string,
    ) => { readonly refillId: string | null } | null;
    // Per-output desktop switch with an immediate readback for migration
    // views (target shows the migrated workspace, source shows the refill).
    // True only when the readback confirms the named desktop id.
    readonly switchMigrateView?: (desktopId: string, outputName: string) => boolean;
    // Read-only live active output name (the KWin activeScreen), or null
    // when unreadable. Never switches: the pre-slot expectation proof.
    readonly readActiveOutput?: () => string | null;
    // Native directional output-switch slot for empty/sticky-active
    // migrations (KWin useractions switchToOutput/setActiveOutput via the
    // directional screen slot matching the flight direction). Invoked only
    // when the target output is not already active, with the source output
    // proven active first; true means the slot was invoked (never a claim:
    // the adapter reads back the active output itself). No retry, no
    // focus setter ever rides along: KWin may choose a native client
    // itself, which is accepted and never our fabricated focus.
    readonly switchActiveOutput?: (direction: WorkspaceMigrateDirection) => boolean;
}

// Exact terminal flight domains carried on the single settlement edge so the
// entry can force a complete source AND target refresh through the
// single-flight Plan chain. Primitive ids only, captured from the flight
// snapshot before the pin clears; empty strings only on the defensive
// no-flight path, which the entry treats as a generic resync.
export interface WorkspaceSendSettled {
    readonly sourceOutput: string;
    readonly sourceWorkspace: string;
    readonly targetOutput: string;
    readonly targetWorkspace: string;
}

// Bounded per-flight sequencing is diagnostic-only. Production native hooks use
// it to place their synchronous call/readback records among adapter records.
export interface WorkspaceFollowNativeDiagnostic {
    readonly correlation: string;
    readonly revision: number;
    readonly nextSequence: () => number;
}

export interface WorkspaceSendEnableAuth {
    readonly owner: unknown;
    readonly generation: unknown;
}

// R-WS-12 whole-workspace output migration (follow-only): the active
// workspace keeps its stable backing id and moves to the adjacent output
// resolved adapter-side via full-output-rect adjacency (no wrap). Four
// directional actions, bindable and unbound by default. The Engine rekeys
// the retained session at plan time, so failures reconcile source/target
// actual observations and never imply native success.

export const WORKSPACE_MIGRATE_COMPONENT = "workspace-migrate";

export type WorkspaceMigrateDirection = "left" | "right" | "up" | "down";

export const WORKSPACE_MIGRATE_DIRECTIONS: ReadonlyArray<WorkspaceMigrateDirection> = Object.freeze([
    "left",
    "right",
    "up",
    "down",
]);

export function isMigrateDirection(value: unknown): value is WorkspaceMigrateDirection {
    return value === "left" || value === "right" || value === "up" || value === "down";
}

// Frozen flight pin for migrate re-observation: deriving the source from
// the live active window would lose the scope once members land on the
// destination, so every re-observation carries the dispatch triple. The
// optional phase-aware view expectation lets post-write verification name
// the exact views the flight itself wrote (target shows the migrated
// workspace, source shows the refill) instead of the pre-write views.
export interface WorkspaceMigratePin {
    readonly sourceOutput: string;
    readonly sourceWorkspace: string;
    readonly targetOutput: string;
    readonly expectViews?: {
        readonly source: string;
        readonly target: string;
    };
}

export interface WorkspaceMigrateObservedWindow {
    readonly id: string;
    readonly ref: object;
    readonly rect: WorkspaceSendRect;
    readonly output: string;
    readonly floating: boolean;
    readonly sticky: boolean;
    readonly fullscreen: boolean;
    readonly maximized: boolean;
    readonly fitExcluded: boolean;
    // Minimized members ride the native transfer only: no wire entry, no
    // planned geometry, no focus. Never classified as intentional float.
    readonly minimized: boolean;
    // Transient classification for implicit-arrival tracking: a transient
    // descendant of a migrating member moves with its parent and never
    // takes an explicit setter.
    readonly transient: boolean;
    // Adapter-asserted fixed-size provenance from PlanAdapter admission
    // state (read-only decoration, never classified here). Rides the wire
    // so the core keeps automatic-float and tile-override origin.
    readonly fixedAuto: boolean;
    readonly fixedSuppress: boolean;
    // Advisory client size hints, mirroring the ordinary reconcile wire.
    // Never join fences: meaningfulness is judged core-side.
    readonly minSize?: { readonly w: number; readonly h: number };
    readonly maxSize?: { readonly w: number; readonly h: number };
}

// Related native clients that ride no wire and take no explicit setter:
// transient descendants of migrating members (tracked for implicit
// arrival) plus protected (fullscreen/maximized) clients in either
// affected view even when dialog/non-normal (affected-view gate only,
// parentId null, never moved).
export interface WorkspaceMigrateRelatedWindow {
    readonly id: string;
    readonly ref: object;
    readonly output: string;
    // Stable id of the migrating member this transient follows, or null
    // for a view-local protected client that never moves.
    readonly parentId: string | null;
    readonly transient: boolean;
    readonly minimized: boolean;
    readonly floating: boolean;
    readonly sticky: boolean;
    readonly fullscreen: boolean;
    readonly maximized: boolean;
    readonly fitExcluded: boolean;
}

export interface WorkspaceMigrateObserved {
    readonly direction: WorkspaceMigrateDirection;
    // Source output current workspace, FULL view including sticky. The
    // target workspace is the same backing id on a different output.
    readonly sourceOutput: string;
    readonly sourceWorkspace: string;
    readonly sourceBounds: WorkspaceSendRect;
    readonly targetOutput: string;
    readonly targetWorkspace: string;
    readonly targetBounds: WorkspaceSendRect;
    // Live current workspace on the target output at observation time. The
    // prior workspace stays listed and hidden unchanged; a drifted view
    // fails closed. Never the migrating id itself.
    readonly targetCurrentWorkspace: string;
    // Live current workspace on the source output at observation time.
    // Pre-view it must still name the migrating workspace; the refill
    // replaces it only through the single view write.
    readonly sourceCurrentWorkspace: string;
    // Migrated active client, sticky active client, or "" when neither is
    // active. A named id outside the source view refuses downstream.
    readonly focusedId: string;
    readonly activeRef: object | null;
    readonly sourceWindows: ReadonlyArray<WorkspaceMigrateObservedWindow>;
    // FULL target current view: affected-view overlay gate plus drift
    // fence. Sticky entries here are observed, never moved.
    readonly targetViewWindows: ReadonlyArray<WorkspaceMigrateObservedWindow>;
    // Related native clients (transient descendants plus view-local
    // protected clients). Gate plus implicit-arrival tracking only: no
    // wire entry, no explicit setter, no focus, ever.
    readonly relatedWindows: ReadonlyArray<WorkspaceMigrateRelatedWindow>;
    readonly migratedDesktopRef: object | null;
    // Dispatch-frozen workspace policy: per-output-local/global-unique only,
    // with the STRICT TRUE live native perOutputVirtualDesktops flag. Any
    // other mode, a false flag, or an unreadable flag refuses.
    readonly mode: string;
    readonly perOutput: boolean | null;
    // Dispatch-frozen source tiling mode. Floating workspaces migrate
    // membership-only with zero tiler geometry writes; the mode must read
    // back unchanged before every setter or the flight stales.
    readonly sourceTiled: boolean;
    readonly desktopCount: number;
    // Overlay tripwire over carried PLUS minimized views: any
    // fullscreen/maximized (maximized floats included) in either affected
    // view refuses the whole operation before any native write.
    readonly protectedPresent: boolean;
    readonly sourceFingerprint: string;
    readonly targetViewFingerprint: string;
}

// Primitive-only snapshot retained across the async D-Bus boundary. Never
// holds refs: ids, geometry values, scope, policy, flags, and fingerprints
// only. Targets resolve from a fresh synchronous observation while
// handling replies.
export interface WorkspaceMigrateSnapshotWindow {
    readonly id: string;
    readonly rect: WorkspaceSendRect;
    readonly output: string;
    readonly floating: boolean;
    readonly sticky: boolean;
    readonly fullscreen: boolean;
    readonly maximized: boolean;
    readonly fitExcluded: boolean;
    readonly minimized: boolean;
    readonly transient: boolean;
    readonly fixedAuto: boolean;
    readonly fixedSuppress: boolean;
}

export interface WorkspaceMigrateSnapshotRelated {
    readonly id: string;
    readonly output: string;
    readonly parentId: string | null;
    readonly transient: boolean;
    readonly minimized: boolean;
    readonly floating: boolean;
    readonly sticky: boolean;
    readonly fullscreen: boolean;
    readonly maximized: boolean;
    readonly fitExcluded: boolean;
}

export interface WorkspaceMigrateSnapshot {
    readonly direction: WorkspaceMigrateDirection;
    readonly sourceOutput: string;
    readonly sourceWorkspace: string;
    readonly sourceBounds: WorkspaceSendRect;
    readonly targetOutput: string;
    readonly targetWorkspace: string;
    readonly targetBounds: WorkspaceSendRect;
    readonly targetCurrentWorkspace: string;
    readonly sourceCurrentWorkspace: string;
    readonly focusedId: string;
    readonly sourceWindows: ReadonlyArray<WorkspaceMigrateSnapshotWindow>;
    readonly targetViewWindows: ReadonlyArray<WorkspaceMigrateSnapshotWindow>;
    readonly relatedWindows: ReadonlyArray<WorkspaceMigrateSnapshotRelated>;
    readonly mode: string;
    readonly perOutput: boolean | null;
    readonly sourceTiled: boolean;
    readonly protectedPresent: boolean;
    readonly desktopCount: number;
    readonly sourceFingerprint: string;
    readonly targetViewFingerprint: string;
}

export interface WorkspaceMigrateGeometryEntry {
    readonly window: string;
    readonly leaf: string;
    readonly output: string;
    readonly workspace: string;
    readonly rect: WorkspaceSendRect;
}

export interface WorkspaceMigratePlanned {
    readonly correlationId: string;
    readonly baseRevision: number;
    readonly direction: WorkspaceMigrateDirection;
    readonly sourceOutput: string;
    readonly sourceWorkspace: string;
    readonly targetOutput: string;
    readonly targetWorkspace: string;
    // Carried non-sticky active member (tiled or float), otherwise null:
    // empty, absent, and sticky-active routes carry no focus and the
    // adapter then runs zero focus setters.
    readonly activeWindow: string | null;
    readonly geometry: ReadonlyArray<WorkspaceMigrateGeometryEntry>;
    // Migrating tiled leaf in the target domain, or null when no tiled
    // active member migrates. Never a float leaf, never fabricated.
    readonly focusLeaf: string | null;
    readonly members: number;
    readonly floats: number;
}

interface WorkspaceMigrateFlight {
    readonly correlation: string;
    readonly direction: WorkspaceMigrateDirection;
    readonly snapshot: WorkspaceMigrateSnapshot;
    readonly requestPayload: string;
    readonly innerGap: number;
    readonly outerGap: number;
    // Dispatch-captured member refs for placement proofs. Never re-derived:
    // a replaced member (same stable id, new object) fails closed.
    readonly members: ReadonlyArray<{
        readonly id: string;
        readonly ref: object;
        readonly sticky: boolean;
        readonly floating: boolean;
        readonly minimized: boolean;
        readonly output: string;
    }>;
    // Dispatch-resolved exact native objects for identity fences. A same-id
    // replacement refuses before the next setter.
    readonly targetOutputRef: object;
    readonly migratedDesktopRef: object;
    // Dispatch-captured related refs for implicit-arrival proofs. Never
    // take an explicit setter; a replaced ref fails closed.
    readonly related: ReadonlyArray<{
        readonly id: string;
        readonly ref: object;
        readonly parentId: string | null;
        readonly sticky: boolean;
        readonly output: string;
    }>;
    // Dispatch-captured target-view refs for the post-view residue proof.
    // The flight's own target switch hides the prior view, so afterwards
    // these prove out through live reads (same output, flags, desktop)
    // instead of observed lists.
    readonly watched: ReadonlyArray<{
        readonly id: string;
        readonly ref: object;
        readonly output: string;
        readonly sticky: boolean;
        readonly floating: boolean;
        readonly fullscreen: boolean;
        readonly maximized: boolean;
    }>;
    baseRevision: number;
    planned: WorkspaceMigratePlanned | null;
    // Arrival follow plus per-output view writes run at most once per
    // flight. They never advance any planner phase: the Rust topology is
    // already rekeyed at plan time.
    followed: boolean;
    followOutcome: string;
    viewsWritten: boolean;
    // Session-map refill resolved at commit time (null when the source
    // scope is left empty).
    refillId?: string | null;
    // Whether any native setter applied yet, plus the terminal outcome for
    // a mid-write abort. The first abort wins; later calls keep it.
    wroteAny: boolean;
    writeOutcome?: string;
    // Planned-geometry members whose transfer initiated while placement
    // was still pending: their target geometry applies once placement
    // verifies, before any view write. Written ids never rewrite.
    geomPending: string[];
}

// Deterministic bounded observation fingerprint (FNV-1a 32-bit over
// `output\x1fworkspace\x1fids...`, sorted ids, no focus) covering one source
// or target scope. Sent as the numeric fingerprint field and cached as the
// string identity so dispatch, validation, and repeat tracking bind one value.
export function workspaceFingerprint(
    output: string,
    workspace: string,
    sortedIds: readonly string[],
): number {
    let hash = 2166136261;
    const feed = (text: string): void => {
        for (let index = 0; index < text.length; index += 1) {
            hash ^= text.charCodeAt(index) & 0xff;
            hash = Math.imul(hash, 16777619);
        }
    };
    feed(output);
    hash ^= 0x1f;
    hash = Math.imul(hash, 16777619);
    feed(workspace);
    for (const id of sortedIds) {
        hash ^= 0x1f;
        hash = Math.imul(hash, 16777619);
        feed(id);
    }
    return hash >>> 0;
}

function isOpaqueId(value: unknown): value is string {
    if (typeof value !== "string" || value.length === 0 || value.length > WORKSPACE_SEND_MAX_ID_LEN) {
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

// Exact D-Bus unique-owner shape (`:N.M`) for the pinned planner endpoint.
function isUniqueOwner(value: unknown): value is string {
    return typeof value === "string" && /^:[0-9]+\.[0-9]+$/.test(value);
}

function isCorrelationId(value: unknown): value is string {
    return (
        typeof value === "string" &&
        value.length > 0 &&
        value.length <= WORKSPACE_SEND_MAX_CORRELATION_LEN &&
        isOpaqueId(value)
    );
}

function isOwnerId(value: unknown): value is string {
    return (
        typeof value === "string" &&
        value.length > 0 &&
        value.length <= WORKSPACE_SEND_MAX_OWNER_LEN &&
        isOpaqueId(value)
    );
}

function isGeneration(value: unknown): value is string {
    if (typeof value !== "string" || value.length === 0 || value.length > WORKSPACE_SEND_MAX_GENERATION_LEN) {
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

function isRevision(value: unknown): value is number {
    return (
        typeof value === "number" &&
        Number.isInteger(value) &&
        value >= 0 &&
        value <= WORKSPACE_SEND_MAX_REVISION
    );
}

function isRecord(value: unknown): value is Record<string, unknown> {
    return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isFiniteInt(value: unknown): value is number {
    return typeof value === "number" && Number.isFinite(value) && Number.isInteger(value);
}

function isTargetRect(value: unknown): value is WorkspaceSendRect {
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

// Bounded rejection-kind token: lowercase dashes only, otherwise redacted to
// `unknown`. Never echoes payload bytes.
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

// Bounded redacted integer for follow diagnostics: a finite integer inside
// [min, max] passes through, anything else (absent, wrong type,
// out-of-range, log/observe exception residue) sanitizes to -1. Never throws.
function toDiagInt(value: unknown, min: number, max: number): number {
    try {
        if (typeof value !== "number" || !Number.isInteger(value) || value < min || value > max) {
            return -1;
        }
        return value;
    } catch (error) {
        void error;
        return -1;
    }
}

// Requested logical ordinal from requestWorkspaceMove (0 permitted for the
// trailing target, 1..9 otherwise). Diagnostic only: anything else sanitizes
// to -1 and never gates behavior. Never throws.
function toDiagOrdinal(value: unknown): number {
    return toDiagInt(value, 0, 9);
}

// Cross-output transfer gate for explicit output send. All seven must be
// present: the output observation hook, exact Output object resolution by
// name, sendClientToScreen transfer, synchronous output and desktop reads
// for the placement arrival proof, the read-only live mover hook for
// mid-write lifetime proofs, and the one-shot outputChanged fence for
// delayed arrival. Any absence refuses output send at dispatch with
// `transfer-unavailable` and same-output behavior is unchanged.
function outputTransferSupported(env: WorkspaceSendAdapterEnv): boolean {
    return (
        typeof env.observeOutput === "function" &&
        typeof env.resolveOutput === "function" &&
        typeof env.sendClientToScreen === "function" &&
        typeof env.readOutputName === "function" &&
        typeof env.readDesktopIds === "function" &&
        typeof env.readMoverLive === "function" &&
        typeof env.subscribeMoverOutput === "function"
    );
}

// Exact lifecycle precondition vector for a same-output move-tiled plan. The
// `adapter-must-verify-postconditions` token is accepted for codec
// compatibility with the immediate-commit Rust reply; this adapter performs
// no native-verification claim and treats the vector as an opaque exact
// binding, never as proof of native state.
const KNOWN_PRECONDITIONS: readonly string[] = Object.freeze([
    "window-observed",
    "desired-topology-valid",
    "adapter-must-verify-postconditions",
]);

function isExactPreconditions(value: unknown): value is readonly string[] {
    if (!Array.isArray(value) || value.length !== KNOWN_PRECONDITIONS.length) {
        return false;
    }
    for (let index = 0; index < KNOWN_PRECONDITIONS.length; index += 1) {
        if (value[index] !== KNOWN_PRECONDITIONS[index]) {
            return false;
        }
    }
    return true;
}

interface WorkspaceGeometryEntry {
    readonly window: string;
    readonly leaf: string;
    readonly output: string;
    readonly workspace: string;
    readonly rect: WorkspaceSendRect;
}

interface WorkspaceFollowFocus {
    readonly output: string;
    readonly workspace: string;
    readonly leaf: string;
}

// Flight-pinned basis for follow diagnostics only. Carries the mover stable
// id, the flight stable target id plus request target output, the flight
// target wrapper for diagnostic equality, and the requested logical ordinal
// from the plan entry (0 permitted for the trailing target, -1 when absent).
// srcInSource/srcInTarget freeze the immutable dispatch-time source
// membership (1 present, 0 absent, -1 unknown). Never logged in raw form and
// never used for follow decisions.
interface WorkspaceFollowDiagBasis {
    readonly moverId: string;
    readonly targetWorkspace: string;
    readonly targetOutput: string;
    readonly targetDesktopRef: object | null;
    readonly requestedOrdinal: number;
    readonly srcInSource: number;
    readonly srcInTarget: number;
}

// Immutable dispatch-time mover membership from the dispatch snapshot.
// Returns 1/0 per scope, -1 when unreadable. Never throws and never logs raw
// identifiers. Called once at dispatch; results are stored on the pending
// flight and never rederived.
function snapshotMoverFlags(
    snapshot: WorkspaceSendSnapshot,
    moverId: string,
): { srcInSource: number; srcInTarget: number } {
    try {
        let inSource = 0;
        let inTarget = 0;
        for (const entry of snapshot.sourceWindows) {
            if (entry.id === moverId) {
                inSource = 1;
                break;
            }
        }
        for (const entry of snapshot.targetWindows) {
            if (entry.id === moverId) {
                inTarget = 1;
                break;
            }
        }
        return { srcInSource: inSource, srcInTarget: inTarget };
    } catch (error) {
        void error;
        return { srcInSource: -1, srcInTarget: -1 };
    }
}

interface WorkspacePlanned {
    readonly correlationId: string;
    readonly baseRevision: number;
    readonly geometry: ReadonlyArray<WorkspaceGeometryEntry>;
    readonly preconditions: readonly string[];
    readonly operation: Record<string, unknown>;
    // Explicit follow/stay selection echoed by the Rust move operation.
    // Legacy replies omit it and default to follow.
    readonly follow: boolean;
    // True for the slotless deferred transfer (`move-deferred`, D28/D16): the
    // operation names no leaf and the reply geometry covers tiled survivors
    // only (empty when neither side has tiled members). Follow carries the
    // target bookkeeping MRU or null (native follow addresses the mover
    // ref); stay resolves the source MRU exactly like tiled stays.
    readonly deferred: boolean;
    // Planned desired focus: the mover leaf in the target domain for
    // tiled follow (target bookkeeping MRU or null for deferred follow);
    // the source-MRU survivor (or null when the source is left empty or
    // MRU-less) for stay. Never the mover for stay.
    readonly followFocus: WorkspaceFollowFocus | null;
    // Stay-only source-MRU focus target as a stable window id, bound from
    // the reply geometry before any native write (leaf plus source domain
    // match, never the mover). Null for follow flights and for null-focus
    // stays, which run zero focus setters per the focused-removal
    // null-focus rule. Unbindable stay focus rejects the reply outright.
    readonly stayFocusWindow: string | null;
}

function validateGeometryEntry(value: unknown): WorkspaceGeometryEntry | null {
    if (!isRecord(value)) {
        return null;
    }
    if (!hasExactKeys(value, ["window", "leaf", "output", "workspace", "rect"])) {
        return null;
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
    };
}

function validateMoveTiledOperation(value: unknown): { record: Record<string, unknown>; follow: boolean } | null {
    if (!isRecord(value)) {
        return null;
    }
    const fields: readonly string[] = [
        "op",
        "window",
        "leaf",
        "source_output",
        "source_workspace",
        "target_output",
        "target_workspace",
    ];
    // The Rust echo carries the explicit follow selection; legacy replies
    // omit it and default to follow, preserving historical behavior.
    let follow = true;
    if (
        Object.keys(value).length === fields.length + 1 &&
        Object.prototype.hasOwnProperty.call(value, "follow")
    ) {
        if (typeof value["follow"] !== "boolean") {
            return null;
        }
        follow = value["follow"] as boolean;
        for (const field of fields) {
            if (!Object.prototype.hasOwnProperty.call(value, field)) {
                return null;
            }
        }
    } else if (!hasExactKeys(value, fields)) {
        return null;
    }
    if (value["op"] !== "move-tiled") {
        return null;
    }
    for (const field of ["window", "leaf", "source_output", "source_workspace", "target_output", "target_workspace"]) {
        if (!isOpaqueId(value[field])) {
            return null;
        }
    }
    return { record: value, follow };
}

// Slotless deferred-transfer operation (D28 born-fullscreen carry, D16
// ordinary-float carry): window plus source/target identities with NO leaf
// (never fabricated). Exact keys plus the same optional explicit follow
// echo as the tiled operation; a deferred operation carrying a leaf is
// malformed and never actuates.
function validateMoveDeferredOperation(value: unknown): { record: Record<string, unknown>; follow: boolean } | null {
    if (!isRecord(value)) {
        return null;
    }
    const fields: readonly string[] = [
        "op",
        "window",
        "source_output",
        "source_workspace",
        "target_output",
        "target_workspace",
    ];
    let follow = true;
    if (
        Object.keys(value).length === fields.length + 1 &&
        Object.prototype.hasOwnProperty.call(value, "follow")
    ) {
        if (typeof value["follow"] !== "boolean") {
            return null;
        }
        follow = value["follow"] as boolean;
        for (const field of fields) {
            if (!Object.prototype.hasOwnProperty.call(value, field)) {
                return null;
            }
        }
    } else if (!hasExactKeys(value, fields)) {
        return null;
    }
    if (value["op"] !== "move-deferred") {
        return null;
    }
    for (const field of ["window", "source_output", "source_workspace", "target_output", "target_workspace"]) {
        if (!isOpaqueId(value[field])) {
            return null;
        }
    }
    return { record: value, follow };
}

function validatePlanned(
    reply: unknown,
    correlationId: string,
    expectedKind: "send-to-workspace" | "send-to-output",
): WorkspacePlanned | null {
    if (!isRecord(reply)) {
        return null;
    }
    if (reply["v"] !== WORKSPACE_SEND_CONTRACT_VERSION) {
        return null;
    }
    if (reply["correlation_id"] !== correlationId) {
        return null;
    }
    if (reply["outcome"] !== "planned") {
        return null;
    }
    // The wire kind binds the route: same-output flights accept only
    // `send-to-workspace`, cross-output flights only `send-to-output`. A
    // mismatched kind is a mismatched reply and never actuates.
    if (reply["kind"] !== expectedKind) {
        return null;
    }
    const baseRevision = reply["base_revision"];
    if (!isRevision(baseRevision)) {
        return null;
    }
    const preconditions = reply["preconditions"];
    if (!isExactPreconditions(preconditions)) {
        return null;
    }
    const tiledValidated = validateMoveTiledOperation(reply["operation"]);
    const deferredValidated =
        tiledValidated === null ? validateMoveDeferredOperation(reply["operation"]) : null;
    const operationValidated = tiledValidated ?? deferredValidated;
    if (operationValidated === null) {
        return null;
    }
    const operation = operationValidated.record;
    const opFollow = operationValidated.follow;
    // Slotless deferred transfers name no leaf; follow carries tiled
    // bookkeeping focus or null (native follow addresses the mover ref).
    const deferred = deferredValidated !== null;
    // Follow is target-bound: the planned desired focus must name the moved
    // leaf in the operation target domain. Stay is source-bound: the
    // planned desired focus is null (or absent on the wire, which Rust
    // omits when the plan carries no focus) when the source is left empty
    // or focus-less, else it must name the source domain with a non-mover
    // survivor leaf (the source-MRU fallback, never the target). Any other
    // focus is a mismatched (stale or malicious) reply and never actuates
    // or follows.
    const focusRaw = reply["desired_focus"];
    let followFocus: WorkspaceFollowFocus | null = null;
    if (opFollow) {
        if (deferred) {
            // Slotless follow: null focus (member-less target), or a
            // target-domain tiled focus (bookkeeping MRU; native follow
            // still addresses the mover ref, never a tile). Any carried
            // focus outside the target domain is a mismatched reply and
            // never actuates or follows; resolvability against the reply
            // geometry is enforced below.
            if (focusRaw === null || focusRaw === undefined) {
                followFocus = null;
            } else {
                if (!isRecord(focusRaw) || !hasExactKeys(focusRaw, ["domain_output", "domain_workspace", "leaf"])) {
                    return null;
                }
                if (
                    !isOpaqueId(focusRaw["domain_output"]) ||
                    !isOpaqueId(focusRaw["domain_workspace"]) ||
                    !isOpaqueId(focusRaw["leaf"])
                ) {
                    return null;
                }
                if (
                    (focusRaw["domain_output"] as string) !== (operation["target_output"] as string) ||
                    (focusRaw["domain_workspace"] as string) !== (operation["target_workspace"] as string)
                ) {
                    return null;
                }
                followFocus = Object.freeze({
                    output: focusRaw["domain_output"] as string,
                    workspace: focusRaw["domain_workspace"] as string,
                    leaf: focusRaw["leaf"] as string,
                });
            }
        } else {
            if (!isRecord(focusRaw) || !hasExactKeys(focusRaw, ["domain_output", "domain_workspace", "leaf"])) {
                return null;
            }
            if (
                !isOpaqueId(focusRaw["domain_output"]) ||
                !isOpaqueId(focusRaw["domain_workspace"]) ||
                !isOpaqueId(focusRaw["leaf"])
            ) {
                return null;
            }
            if (
                (focusRaw["domain_output"] as string) !== (operation["target_output"] as string) ||
                (focusRaw["domain_workspace"] as string) !== (operation["target_workspace"] as string) ||
                (focusRaw["leaf"] as string) !== (operation["leaf"] as string)
            ) {
                return null;
            }
            followFocus = Object.freeze({
                output: focusRaw["domain_output"] as string,
                workspace: focusRaw["domain_workspace"] as string,
                leaf: focusRaw["leaf"] as string,
            });
        }
    } else {
        if (focusRaw === null || focusRaw === undefined) {
            followFocus = null;
        } else {
            if (!isRecord(focusRaw) || !hasExactKeys(focusRaw, ["domain_output", "domain_workspace", "leaf"])) {
                return null;
            }
            if (
                !isOpaqueId(focusRaw["domain_output"]) ||
                !isOpaqueId(focusRaw["domain_workspace"]) ||
                !isOpaqueId(focusRaw["leaf"])
            ) {
                return null;
            }
            if (
                (focusRaw["domain_output"] as string) !== (operation["source_output"] as string) ||
                (focusRaw["domain_workspace"] as string) !== (operation["source_workspace"] as string) ||
                (focusRaw["leaf"] as string) === (operation["leaf"] as string)
            ) {
                return null;
            }
            followFocus = Object.freeze({
                output: focusRaw["domain_output"] as string,
                workspace: focusRaw["domain_workspace"] as string,
                leaf: focusRaw["leaf"] as string,
            });
        }
    }
    const geometryRaw = reply["desired_geometry"];
    // A sole slotless source sent to an empty target legitimately carries
    // no geometry (no tiled members on either side, mover slotless). Empty
    // geometry is accepted for the deferred transfer only; tiled replies
    // keep the exact non-empty refusal.
    if (!Array.isArray(geometryRaw) || (geometryRaw.length === 0 && !deferred)) {
        return null;
    }
    const geometry: WorkspaceGeometryEntry[] = [];
    const seen = new Set<string>();
    for (const entry of geometryRaw) {
        const valid = validateGeometryEntry(entry);
        if (valid === null || seen.has(valid.window)) {
            return null;
        }
        seen.add(valid.window);
        geometry.push(valid);
    }
    let stayFocusWindow: string | null = null;
    if (!opFollow && followFocus !== null) {
        stayFocusWindow = resolveStayFocusWindow(operation, followFocus, geometry);
        if (stayFocusWindow === null) {
            // Unbindable stay focus (no source-domain geometry carries the
            // desired leaf for a non-mover window): mismatched reply, never
            // actuates or writes. Null focus stays accepted: core owns MRU
            // and null covers both emptied and MRU-less sources.
            return null;
        }
    }
    if (deferred && opFollow && followFocus !== null) {
        // Deferred-follow bookkeeping MRU must resolve through the reply
        // geometry to a non-mover window or the reply is mismatched and
        // never actuates. Native follow still addresses the mover ref.
        if (resolveStayFocusWindow(operation, followFocus, geometry) === null) {
            return null;
        }
    }
    return {
        correlationId,
        baseRevision: baseRevision as number,
        geometry: Object.freeze(geometry),
        preconditions: Object.freeze([...(preconditions as string[])]),
        operation,
        follow: opFollow,
        deferred,
        followFocus,
        stayFocusWindow,
    };
}

// Stay-only focus binding resolved before any native write: the desired
// source-MRU leaf maps through the reply geometry (same leaf binding the
// Plan adapter uses: first geometry entry with the focus leaf, here also
// pinned to the source domain) to a stable window id, never the mover.
// Null means the desired leaf carries no actuable source survivor.
function resolveStayFocusWindow(
    operation: Record<string, unknown>,
    followFocus: WorkspaceFollowFocus,
    geometry: ReadonlyArray<WorkspaceGeometryEntry>,
): string | null {
    const moverWindow = operation["window"] as string;
    for (const entry of geometry) {
        if (
            entry.leaf === followFocus.leaf &&
            entry.output === followFocus.output &&
            entry.workspace === followFocus.workspace &&
            entry.window !== moverWindow
        ) {
            return entry.window;
        }
    }
    return null;
}

function validateObserved(observed: WorkspaceSendObserved | null): observed is WorkspaceSendObserved {
    if (observed === null || typeof observed !== "object") {
        return false;
    }
    if (!isOpaqueId(observed.sourceOutput) || !isOpaqueId(observed.sourceWorkspace)) {
        return false;
    }
    if (!isOpaqueId(observed.targetOutput) || !isOpaqueId(observed.targetWorkspace)) {
        return false;
    }
    if (
        !isTargetRect({
            x: observed.sourceBounds.x,
            y: observed.sourceBounds.y,
            w: observed.sourceBounds.w,
            h: observed.sourceBounds.h,
        }) ||
        !isTargetRect({
            x: observed.targetBounds.x,
            y: observed.targetBounds.y,
            w: observed.targetBounds.w,
            h: observed.targetBounds.h,
        })
    ) {
        return false;
    }
    if (observed.focusedId !== "" && !isOpaqueId(observed.focusedId)) {
        return false;
    }
    const sourceWindows = observed.sourceWindows;
    const targetWindows = observed.targetWindows;
    if (!Array.isArray(sourceWindows) || !Array.isArray(targetWindows)) {
        return false;
    }
    const seen = new Set<string>();
    for (const entry of [...sourceWindows, ...targetWindows]) {
        if (typeof entry !== "object" || entry === null) {
            return false;
        }
        const candidate = entry as WorkspaceSendObservedWindow;
        if (!isOpaqueId(candidate.id) || typeof candidate.ref !== "object" || candidate.ref === null) {
            return false;
        }
        if (!isTargetRect({ x: candidate.rect.x, y: candidate.rect.y, w: candidate.rect.w, h: candidate.rect.h })) {
            return false;
        }
        // Complete-observation flags: absent normalizes to false downstream;
        // a present non-boolean never sanitizes, it fails closed.
        for (const flag of [
            candidate.floating,
            candidate.fitExcluded,
            candidate.fit_excluded,
            candidate.fullscreen,
            candidate.maximized,
            candidate.sticky,
        ]) {
            if (flag !== undefined && typeof flag !== "boolean") {
                return false;
            }
        }
        if (seen.has(candidate.id)) {
            return false;
        }
        seen.add(candidate.id);
    }
    if (observed.focusedId !== "" && !sourceWindows.some((w) => w.id === observed.focusedId)) {
        return false;
    }
    if (typeof observed.activeRef !== "object" && observed.activeRef !== null) {
        return false;
    }
    if (typeof observed.moverRef !== "object" && observed.moverRef !== null) {
        return false;
    }
    if (typeof observed.targetDesktopRef !== "object" && observed.targetDesktopRef !== null) {
        return false;
    }
    if (typeof observed.targetExists !== "boolean" || typeof observed.desktopCount !== "number") {
        return false;
    }
    if (typeof observed.sourceFingerprint !== "string" || observed.sourceFingerprint.length === 0) {
        return false;
    }
    if (typeof observed.targetFingerprint !== "string" || observed.targetFingerprint.length === 0) {
        return false;
    }
    // Behavior-fenced visibility input: present as a stable id or null
    // when unreadable. Absent or malformed fails closed like any other
    // shape violation; null itself validates (the stay fence, not
    // validation, treats it as not-selected).
    if (observed.currentWorkspace !== null && !isOpaqueId(observed.currentWorkspace)) {
        return false;
    }
    return true;
}

function snapshotWindowOf(entry: WorkspaceSendObservedWindow): WorkspaceSendSnapshotWindow {
    return {
        id: entry.id,
        rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
        floating: entry.floating === true,
        fitExcluded: entry.fitExcluded === true || entry.fit_excluded === true,
        fullscreen: entry.fullscreen === true,
        maximized: entry.maximized === true,
        sticky: entry.sticky === true,
    };
}

export function snapshotOf(observed: WorkspaceSendObserved): WorkspaceSendSnapshot {
    const sourceWindows = observed.sourceWindows.map(snapshotWindowOf);
    const targetWindows = observed.targetWindows.map(snapshotWindowOf);
    return {
        sourceOutput: observed.sourceOutput,
        sourceWorkspace: observed.sourceWorkspace,
        sourceBounds: {
            x: observed.sourceBounds.x,
            y: observed.sourceBounds.y,
            w: observed.sourceBounds.w,
            h: observed.sourceBounds.h,
        },
        targetOutput: observed.targetOutput,
        targetWorkspace: observed.targetWorkspace,
        targetBounds: {
            x: observed.targetBounds.x,
            y: observed.targetBounds.y,
            w: observed.targetBounds.w,
            h: observed.targetBounds.h,
        },
        focusedId: observed.focusedId,
        sourceWindows: Object.freeze(sourceWindows),
        targetWindows: Object.freeze(targetWindows),
        sourceFingerprint: observed.sourceFingerprint,
        targetFingerprint: observed.targetFingerprint,
    };
}

function snapshotsEqual(a: WorkspaceSendSnapshot, b: WorkspaceSendSnapshot): boolean {
    if (
        a.sourceOutput !== b.sourceOutput ||
        a.sourceWorkspace !== b.sourceWorkspace ||
        a.targetOutput !== b.targetOutput ||
        a.targetWorkspace !== b.targetWorkspace ||
        a.focusedId !== b.focusedId ||
        a.sourceFingerprint !== b.sourceFingerprint ||
        a.targetFingerprint !== b.targetFingerprint ||
        a.sourceBounds.x !== b.sourceBounds.x ||
        a.sourceBounds.y !== b.sourceBounds.y ||
        a.sourceBounds.w !== b.sourceBounds.w ||
        a.sourceBounds.h !== b.sourceBounds.h ||
        a.targetBounds.x !== b.targetBounds.x ||
        a.targetBounds.y !== b.targetBounds.y ||
        a.targetBounds.w !== b.targetBounds.w ||
        a.targetBounds.h !== b.targetBounds.h
    ) {
        return false;
    }
    if (a.sourceWindows.length !== b.sourceWindows.length || a.targetWindows.length !== b.targetWindows.length) {
        return false;
    }
    const sourceById = new Map<string, WorkspaceSendSnapshotWindow>();
    for (const entry of a.sourceWindows) {
        sourceById.set(entry.id, entry);
    }
    for (const entry of b.sourceWindows) {
        const other = sourceById.get(entry.id);
        if (other === undefined || !snapshotWindowsEqual(other, entry)) {
            return false;
        }
    }
    const targetById = new Map<string, WorkspaceSendSnapshotWindow>();
    for (const entry of a.targetWindows) {
        targetById.set(entry.id, entry);
    }
    for (const entry of b.targetWindows) {
        const other = targetById.get(entry.id);
        if (other === undefined || !snapshotWindowsEqual(other, entry)) {
            return false;
        }
    }
    return true;
}

// Deferred-operation snapshot eligibility: the dispatch snapshot mover
// must carry a slotless-transfer shape: the D28 held overlay (fullscreen,
// never floating/sticky/maximized, fit-excluded) or the D16 ordinary
// intentional float (floating, never fullscreen/sticky/maximized,
// fit-excluded). Sticky, maximized, and other shapes never carry, so an
// ordinary tiled mover still cannot sneak a leafless transfer and drop
// its own allocation. Test shape only proves the flags; the actual held
// topology stays shared Engine ownership, with no new state or provenance
// schema on this side.
function snapshotMoverHeld(snapshot: WorkspaceSendSnapshot, moverId: string): boolean {
    for (const entry of [...snapshot.sourceWindows, ...snapshot.targetWindows]) {
        if (entry.id !== moverId) {
            continue;
        }
        const heldFullscreen =
            entry.fullscreen === true &&
            entry.floating !== true &&
            entry.sticky !== true &&
            entry.maximized !== true &&
            entry.fitExcluded === true;
        const ordinaryFloat =
            entry.floating === true &&
            entry.fullscreen !== true &&
            entry.sticky !== true &&
            entry.maximized !== true &&
            entry.fitExcluded === true;
        return heldFullscreen || ordinaryFloat;
    }
    return false;
}

// Per-survivor equality: identical rect plus identical complete-observation
// flags. A survivor changing flags without rect/membership change still
// mismatches, on either domain, so the reply fence stales before any setter.
function snapshotWindowsEqual(a: WorkspaceSendSnapshotWindow, b: WorkspaceSendSnapshotWindow): boolean {
    return (
        a.rect.x === b.rect.x &&
        a.rect.y === b.rect.y &&
        a.rect.w === b.rect.w &&
        a.rect.h === b.rect.h &&
        a.floating === b.floating &&
        a.fitExcluded === b.fitExcluded &&
        a.fullscreen === b.fullscreen &&
        a.maximized === b.maximized &&
        a.sticky === b.sticky
    );
}

// Immutable scope fence for arrival and follow: the live source/target
// domains, bounds, and target existence still equal the dispatch snapshot.
// Membership and geometry are checked separately; wrapper identity is never
// compared because KWin may return a fresh wrapper per read.
function scopeMatchesSnapshot(observed: WorkspaceSendObserved, snapshot: WorkspaceSendSnapshot): boolean {
    return (
        observed.sourceOutput === snapshot.sourceOutput &&
        observed.sourceWorkspace === snapshot.sourceWorkspace &&
        observed.targetOutput === snapshot.targetOutput &&
        observed.targetWorkspace === snapshot.targetWorkspace &&
        observed.sourceBounds.x === snapshot.sourceBounds.x &&
        observed.sourceBounds.y === snapshot.sourceBounds.y &&
        observed.sourceBounds.w === snapshot.sourceBounds.w &&
        observed.sourceBounds.h === snapshot.sourceBounds.h &&
        observed.targetBounds.x === snapshot.targetBounds.x &&
        observed.targetBounds.y === snapshot.targetBounds.y &&
        observed.targetBounds.w === snapshot.targetBounds.w &&
        observed.targetBounds.h === snapshot.targetBounds.h &&
        observed.targetExists === true &&
        observed.targetDesktopRef !== null
    );
}

// Source-visibility fence for stay flights: the dispatch source workspace
// must still be the actual current workspace on the recording output.
// Pinned identity proves membership, never the live view: an external view
// switch elsewhere keeps every membership but revokes visibility, so a
// stay must not focus a hidden source survivor (nor confirm a null stay
// while viewing elsewhere). Unreadable (null) never counts as selected:
// fail closed, never assume.
function sourceStillSelected(observed: WorkspaceSendObserved, snapshot: WorkspaceSendSnapshot): boolean {
    try {
        return observed.currentWorkspace !== null && observed.currentWorkspace === snapshot.sourceWorkspace;
    } catch (error) {
        void error;
        return false;
    }
}

interface WorkspacePendingFlight {
    readonly correlation: string;
    readonly snapshot: WorkspaceSendSnapshot;
    readonly targetWorkspace: string;
    readonly moverId: string;
    readonly windowCount: number;
    readonly requestPayload: string;
    readonly targetDesktopRef: object | null;
    // Dispatch-frozen validated gap pair for this flight. The request payload
    // carries exactly these values even when a deliberate reload updates
    // subsequent requests.
    readonly innerGap: number;
    readonly outerGap: number;
    // Requested logical ordinal from the plan entry (0 permitted for the
    // trailing target, -1 when absent/invalid). Diagnostic only: never gates
    // request, commit, follow, or enablement.
    readonly requestedOrdinal: number;
    // Immutable dispatch-time mover membership, computed once at dispatch
    // from the dispatch snapshot and never rederived. Diagnostic only.
    readonly srcInSource: number;
    readonly srcInTarget: number;
    // Explicit follow/stay selection for this flight: true follows the mover
    // into the target (desktop switch then mover focus), false stays on the
    // source (no switch, no focus write). Bound to the echoed operation.
    readonly follow: boolean;
    // Dispatch-retained mover ref for cross-output arrival proofs. Scope-
    // homed scans cannot see a half-applied transfer (output moved,
    // membership pending, or vice versa), so cross-output arrival is proven
    // by native output plus single-desktop reads on this ref (R4 precedent),
    // never by homing. Same-output flights never consult it.
    readonly moverRef: object | null;
    // Explicit output send (REQ-OUT-04, item 5.3/5.4): the target lives on a
    // different output whose current workspace was resolved once at dispatch.
    // False preserves the historical same-output workspace-send behavior.
    // Cross-output flights transfer output first (R4 order), prove output in
    // arrival, and skip the desktop switch on follow (the destination is
    // already visible on its output; focus carries the follow).
    readonly crossOutput: boolean;
    baseRevision: number;
    planned: WorkspacePlanned | null;
    // Arrival follow runs at most once per flight. It never advances any
    // planner phase: the Rust topology is already committed.
    followed: boolean;
    followOutcome: string;
}

export interface WorkspaceSendGaps {
    readonly innerGap?: unknown;
    readonly outerGap?: unknown;
}

export class WorkspaceSendAdapter {
    private enabled = false;
    private startupEnabled = false;
    private owner = "";
    private generation = "";
    // Validated gap configuration for subsequent requests: resolved at
    // construction, then re-resolved only through updateGaps on the
    // deliberate Options configChanged reload. Each dispatched flight
    // freezes its own pair at request time so a reload during an active
    // request never alters its payload.
    private innerGap: number;
    private outerGap: number;
    private inFlight = false;
    private token = 0;
    private activeToken = 0;
    // Separate bounded deadlines: the request deadline releases an
    // unanswered request; the arrival deadline bounds the source+target pin
    // and delayed-arrival follow. Distinct epochs so a stale timer can never
    // touch a later flight.
    private deadlineToken = 0;
    private requestDeadline = 0;
    private arrivalDeadline = 0;
    private requestTimer: (() => void) | null = null;
    private arrivalTimer: (() => void) | null = null;
    private pinnedOwner: string | null = null;
    private activationStep = 0;
    private pending: WorkspacePendingFlight | null = null;
    // R-WS-12 migrate flight slot. Shares the single inFlight exclusion,
    // correlation sequencing, activation, timers, and settlement with the
    // send flight above: at most one flight of either kind is ever live, so
    // no second concurrent Engine writer exists.
    private migrate: WorkspaceMigrateFlight | null = null;
    // One-shot delayed-arrival subscription. Armed after the native writes
    // when the immediate observation shows no arrival yet; detached on the
    // first signal and on every terminal path.
    private arrivalDetach: (() => void) | null = null;
    // Cross-output companion for the mover outputChanged fence. Armed and
    // detached alongside arrivalDetach on output-send flights only; null on
    // same-output flights, which never consult it.
    private arrivalOutputDetach: (() => void) | null = null;
    // R-WS-12 per-member one-shot arrival detaches. Armed before migrate
    // native writes, consumed on the first signal, cleared on every
    // terminal path via detachMigrateArrival.
    private readonly migrateDetaches: Array<() => void> = [];
    private seq = 0;
    // Bounded correlation rotation: after WORKSPACE_SEND_MAX_SEQ correlations
    // the sequence wraps with a rotation prefix so correlations are never
    // reused within a bounded length. Same session/topology: no enable reset.
    private seqEpoch = 0;
    private diagSeq = 0;
    // KWin signals may be delivered synchronously from a setter. Do not let
    // them follow or settle while the write stack is live; the immediate
    // post-write observation after the stack covers synchronous arrival.
    private nativeWriteDepth = 0;
    private nativeFollowDepth = 0;

    constructor(
        private readonly env: WorkspaceSendAdapterEnv,
        gaps?: WorkspaceSendGaps,
    ) {
        let resolved: DomainGaps | null = null;
        try {
            if (gaps !== undefined) {
                resolved = { innerGap: normalizeGap(gaps.innerGap), outerGap: normalizeGap(gaps.outerGap) };
            } else {
                resolved = readDomainGaps();
            }
        } catch (error) {
            void error;
            resolved = null;
        }
        if (resolved === null) {
            this.innerGap = DOMAIN_GAP_DEFAULT;
            this.outerGap = OUTER_DOMAIN_GAP_DEFAULT;
        } else {
            this.innerGap = resolved.innerGap;
            this.outerGap = resolved.outerGap;
        }
    }

    // Deliberate reload edge: adopt the already-validated configChanged pair
    // for subsequent requests only. Established normalization applies;
    // invalid input falls back via normalizeGap, never refuses. Never touches
    // an active flight: its pair stays frozen in pending.
    updateGaps(gaps?: WorkspaceSendGaps): void {
        try {
            if (gaps === undefined) {
                const resolved = readDomainGaps();
                this.innerGap = resolved.innerGap;
                this.outerGap = resolved.outerGap;
                return;
            }
            this.innerGap = normalizeGap(gaps.innerGap);
            this.outerGap = normalizeGap(gaps.outerGap);
        } catch (error) {
            void error;
        }
    }

    get isEnabled(): boolean {
        return this.enabled;
    }

    get isInFlight(): boolean {
        return this.inFlight;
    }

    // Derived from the existing retained flight only. Entry diagnostics use
    // this to label a refused shortcut without retaining another history.
    get activeCorrelation(): string {
        return this.migrate?.correlation ?? this.pending?.correlation ?? "";
    }

    get activeStage(): string {
        const live = this.migrate !== null ? "migrate" : this.pending !== null ? "send" : null;
        if (!this.inFlight || live === null) {
            return "idle";
        }
        if (this.activationStep !== 5) {
            return "activation";
        }
        const planned = live === "migrate" ? this.migrate?.planned ?? null : this.pending?.planned ?? null;
        if (planned === null) {
            return "request";
        }
        return "arrival";
    }

    // Bounded source+target pin for native cleanup ordering: the exact
    // dispatch source/target workspace ids, present from dispatch until
    // arrival or the bounded arrival deadline clears them on every terminal
    // path. Empty before dispatch or after settlement, so retention is
    // strictly flight-lifetime. Primitive ids only, never refs or history.
    // A migrate flight pins its single backing id (source and target share
    // it), which keeps the moving workspace alive through the map commit.
    get pendingWorkspaces(): ReadonlyArray<string> {
        const migrating = this.migrate;
        if (this.inFlight && migrating !== null) {
            return Object.freeze([migrating.snapshot.sourceWorkspace]);
        }
        const pending = this.pending;
        if (!this.inFlight || pending === null) {
            return Object.freeze([]);
        }
        return Object.freeze([pending.snapshot.sourceWorkspace, pending.snapshot.targetWorkspace]);
    }

    enable(auth: WorkspaceSendEnableAuth): boolean {
        if (this.enabled || this.startupEnabled) {
            return false;
        }
        if (!isRecord(auth as unknown as Record<string, unknown>)) {
            return false;
        }
        if (!isOwnerId(auth.owner) || !isGeneration(auth.generation)) {
            return false;
        }
        this.owner = auth.owner as string;
        this.generation = auth.generation as string;
        this.enabled = true;
        this.startupEnabled = true;
        this.inFlight = false;
        this.pending = null;
        this.seq = 0;
        this.diagSeq = 0;
        return true;
    }

    disable(): void {
        if (!this.enabled && !this.inFlight) {
            return;
        }
        // A live flight torn down here (explicit disable or entry stop) is a
        // terminal release: clear the pin and run the single settlement hook
        // so the entry refreshes both domains from native observation. Never
        // reports to the planner and never claims anything about Rust state.
        if (this.inFlight) {
            this.settleTerminal(this.activeToken, this.activeCorrelation, "disabled", "release");
        }
        this.enabled = false;
    }

    requestSend(targetWorkspace: unknown, requestedOrdinal?: unknown, follow?: unknown): boolean {
        if (!this.enabled) {
            this.refuse("disabled");
            return false;
        }
        if (this.inFlight) {
            this.diag("request", this.activeCorrelation, this.pending?.baseRevision ?? 0, "refuse", "in-flight");
            return false;
        }
        if (!isOpaqueId(targetWorkspace)) {
            this.refuse("target-invalid");
            return false;
        }
        if (this.seq < 0 || this.seq > WORKSPACE_SEND_MAX_SEQ) {
            const nextEpoch = this.seqEpoch + 1;
            if (!Number.isSafeInteger(nextEpoch)) {
                this.refuse("sequence-invalid");
                return false;
            }
            const candidate = `${this.generation}-w${String(nextEpoch)}r0`;
            if (!isCorrelationId(candidate)) {
                this.refuse("sequence-invalid");
                return false;
            }
            this.seqEpoch = nextEpoch;
            this.seq = 0;
            this.diag("request", candidate, 0, "sequence-exhausted", "correlation-rotated");
        }
        const observed = this.freshObserved(targetWorkspace);
        if (observed === null) {
            this.refuse("scope-invalid");
            return false;
        }
        // Refusal routes are exact bounded tokens (see module doc).
        if (observed.desktopCount < 2) {
            this.refuse("last-desktop");
            return false;
        }
        if (!observed.targetExists) {
            this.refuse("target-workspace-missing");
            return false;
        }
        if (observed.targetOutput !== observed.sourceOutput) {
            this.refuse("cross-output");
            return false;
        }
        if (observed.targetWorkspace === observed.sourceWorkspace) {
            this.refuse("same-workspace");
            return false;
        }
        if (observed.activeRef === null) {
            this.refuse("absent-focus");
            return false;
        }
        if (observed.focusedId === "" || !observed.sourceWindows.some((w) => w.id === observed.focusedId)) {
            this.refuse("non-tiled-focus");
            return false;
        }
        if (observed.desktopCount > WORKSPACE_SEND_MAX_DESKTOPS) {
            this.refuse("desktop-cap");
            return false;
        }
        const correlation =
            this.seqEpoch === 0
                ? `${this.generation}-w${String(this.seq)}`
                : `${this.generation}-w${String(this.seqEpoch)}r${String(this.seq)}`;
        this.seq += 1;
        if (!isCorrelationId(correlation)) {
            this.refuse("correlation-invalid");
            return false;
        }
        const snapshot = snapshotOf(observed);
        const flightInnerGap = this.innerGap;
        const flightOuterGap = this.outerGap;
        // Explicit follow/stay selection: only an exact false stays on the
        // source; anything else (including absent) follows the mover into
        // the target, preserving the historical follow behavior.
        const wantsFollow = follow !== false;
        const payload = this.buildRequestPayload(
            observed,
            correlation,
            flightInnerGap,
            flightOuterGap,
            wantsFollow,
            "send-to-workspace",
        );
        if (payload === null) {
            this.refuse("payload-invalid", correlation);
            return false;
        }
        if (payload.length > WORKSPACE_SEND_MAX_REQUEST_BYTES) {
            this.refuse("request-over-cap", correlation);
            return false;
        }
        // Diagnostic-only handoff: an invalid ordinal sanitizes to -1 in logs
        // and never refuses or otherwise gates the send.
        this.startFlight(
            correlation,
            snapshot,
            observed,
            observed.targetWorkspace,
            observed.focusedId,
            observed.targetDesktopRef,
            payload,
            toDiagOrdinal(requestedOrdinal),
            flightInnerGap,
            flightOuterGap,
            wantsFollow,
            false,
        );
        return this.inFlight;
    }

    // Explicit output send (REQ-OUT-04, item 5.3/5.4): the mover crosses to
    // the named destination output's current workspace (resolved once by the
    // entry via FULL-rectangle adjacency selection). Distinct wire op
    // `send-to-output` with the same `target_domain`/`target_windows` shape
    // and the same ordinary admission plus explicit follow/stay as workspace
    // send. Tiled-subject eligibility mirrors workspace send (sticky and
    // intentional floats excluded via the observer's tiled-only mover gate).
    // Floating-workspace boundaries never reach here: the entry transfers
    // membership-only there. Requires all seven transfer capabilities;
    // otherwise refuses `transfer-unavailable` with same-output behavior
    // unchanged. Same-output targets refuse `same-output` (use workspace
    // send); the core stays authoritative for unchanged/unknown domains.
    requestSendToOutput(
        targetOutput: unknown,
        targetWorkspace: unknown,
        follow?: unknown,
        requestedOrdinal?: unknown,
    ): boolean {
        if (!this.enabled) {
            this.refuse("disabled");
            return false;
        }
        if (this.inFlight) {
            this.diag("request", this.activeCorrelation, this.pending?.baseRevision ?? 0, "refuse", "in-flight");
            return false;
        }
        if (!isOpaqueId(targetOutput) || !isOpaqueId(targetWorkspace)) {
            this.refuse("target-invalid");
            return false;
        }
        if (!outputTransferSupported(this.env)) {
            this.refuse("transfer-unavailable");
            return false;
        }
        if (this.seq < 0 || this.seq > WORKSPACE_SEND_MAX_SEQ) {
            const nextEpoch = this.seqEpoch + 1;
            if (!Number.isSafeInteger(nextEpoch)) {
                this.refuse("sequence-invalid");
                return false;
            }
            const candidate = `${this.generation}-w${String(nextEpoch)}r0`;
            if (!isCorrelationId(candidate)) {
                this.refuse("sequence-invalid");
                return false;
            }
            this.seqEpoch = nextEpoch;
            this.seq = 0;
            this.diag("request", candidate, 0, "sequence-exhausted", "correlation-rotated");
        }
        const observed = this.freshOutputObserved(targetOutput, targetWorkspace);
        if (observed === null) {
            this.refuse("scope-invalid");
            return false;
        }
        // Distinct outputs may share one current workspace/desktop (a single
        // global desktop shown everywhere): no last-desktop gate here, only
        // the desktop cap, target existence, and the same-output refusal.
        // Same-output workspace send keeps its own last-desktop gate.
        if (!observed.targetExists) {
            this.refuse("target-workspace-missing");
            return false;
        }
        // The output-send target must live on a different output with the
        // entry-resolved current workspace; same-output targets belong to
        // workspace send and a drifted workspace is stale scope.
        if (observed.targetOutput !== (targetOutput as string)) {
            this.refuse("target-mismatch");
            return false;
        }
        if (observed.targetOutput === observed.sourceOutput) {
            this.refuse("same-output");
            return false;
        }
        if (observed.targetWorkspace !== (targetWorkspace as string)) {
            this.refuse("target-mismatch");
            return false;
        }
        if (observed.activeRef === null) {
            this.refuse("absent-focus");
            return false;
        }
        if (observed.focusedId === "" || !observed.sourceWindows.some((w) => w.id === observed.focusedId)) {
            this.refuse("non-tiled-focus");
            return false;
        }
        if (observed.desktopCount > WORKSPACE_SEND_MAX_DESKTOPS) {
            this.refuse("desktop-cap");
            return false;
        }
        const correlation =
            this.seqEpoch === 0
                ? `${this.generation}-w${String(this.seq)}`
                : `${this.generation}-w${String(this.seqEpoch)}r${String(this.seq)}`;
        this.seq += 1;
        if (!isCorrelationId(correlation)) {
            this.refuse("correlation-invalid");
            return false;
        }
        const snapshot = snapshotOf(observed);
        const flightInnerGap = this.innerGap;
        const flightOuterGap = this.outerGap;
        const wantsFollow = follow !== false;
        const payload = this.buildRequestPayload(
            observed,
            correlation,
            flightInnerGap,
            flightOuterGap,
            wantsFollow,
            "send-to-output",
        );
        if (payload === null) {
            this.refuse("payload-invalid", correlation);
            return false;
        }
        if (payload.length > WORKSPACE_SEND_MAX_REQUEST_BYTES) {
            this.refuse("request-over-cap", correlation);
            return false;
        }
        this.startFlight(
            correlation,
            snapshot,
            observed,
            observed.targetWorkspace,
            observed.focusedId,
            observed.targetDesktopRef,
            payload,
            toDiagOrdinal(requestedOrdinal),
            flightInnerGap,
            flightOuterGap,
            wantsFollow,
            true,
        );
        return this.inFlight;
    }

    private freshObserved(targetWorkspace: string, pinnedSourceWorkspace?: string): WorkspaceSendObserved | null {
        let observed: WorkspaceSendObserved | null = null;
        try {
            observed = this.env.observe(targetWorkspace, pinnedSourceWorkspace);
        } catch (error) {
            void error;
            observed = null;
        }
        if (!validateObserved(observed)) {
            return null;
        }
        return observed as WorkspaceSendObserved;
    }

    private freshOutputObserved(
        targetOutput: string,
        targetWorkspace: string,
        pinnedSourceWorkspace?: string,
        pinnedSourceOutput?: string,
    ): WorkspaceSendObserved | null {
        const hook = this.env.observeOutput;
        if (typeof hook !== "function") {
            return null;
        }
        let observed: WorkspaceSendObserved | null = null;
        try {
            observed = hook(targetOutput, targetWorkspace, pinnedSourceWorkspace, pinnedSourceOutput);
        } catch (error) {
            void error;
            observed = null;
        }
        if (!validateObserved(observed)) {
            return null;
        }
        return observed as WorkspaceSendObserved;
    }

    // Flight-aware re-observation: cross-output flights resolve through the
    // output hook with the frozen destination pair AND the frozen source
    // pair; same-output flights keep the workspace hook. Anything unreadable
    // fails closed (null).
    private freshForPending(pending: WorkspacePendingFlight): WorkspaceSendObserved | null {
        if (pending.crossOutput) {
            return this.freshOutputObserved(
                pending.snapshot.targetOutput,
                pending.targetWorkspace,
                pending.snapshot.sourceWorkspace,
                pending.snapshot.sourceOutput,
            );
        }
        return this.freshObserved(pending.targetWorkspace, pending.snapshot.sourceWorkspace);
    }

    private buildRequestPayload(
        observed: WorkspaceSendObserved,
        correlation: string,
        innerGap: number,
        outerGap: number,
        follow: boolean,
        op: "send-to-workspace" | "send-to-output",
    ): string | null {
        // Portable wire fields: `floating` and `fit_excluded` (the
        // observer-carried fit opt-out, already ORed over floating, sticky,
        // fullscreen, and maximized at observation) plus `fullscreen` for
        // the focused mover only. The mover flag lets the shared Engine
        // preserve a held born-fullscreen exception across the combined
        // convergence instead of retiling it on the source (D28 slotless
        // carry); an intentional-float mover rides `floating` from the
        // observer (D16 slotless carry, same deferred route). Every other
        // member keeps the exact legacy encoding, so tiled fullscreen
        // survivors and all existing transfers are unchanged. Native
        // maximized/sticky never ride the wire; they stay local snapshot
        // semantics for the dispatch-frozen fence.
        const wireFlags = (entry: WorkspaceSendObservedWindow): Record<string, boolean> => ({
            ...(entry.floating === true ? { floating: true } : {}),
            ...(entry.floating === true || entry.fitExcluded === true || entry.fit_excluded === true
                ? { fit_excluded: true }
                : {}),
            ...(entry.id === observed.focusedId && entry.fullscreen === true ? { fullscreen: true } : {}),
        });
        const sourceWindows = observed.sourceWindows.map((entry) => ({
            window: entry.id,
            output: observed.sourceOutput,
            workspace: observed.sourceWorkspace,
            rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
            ...wireFlags(entry),
        }));
        const targetWindows = observed.targetWindows.map((entry) => ({
            window: entry.id,
            output: observed.targetOutput,
            workspace: observed.targetWorkspace,
            rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
            ...wireFlags(entry),
        }));
        let payload = "";
        try {
            payload = JSON.stringify({
                v: WORKSPACE_SEND_CONTRACT_VERSION,
                correlation_id: correlation,
                owner: this.owner,
                generation: this.generation,
                revision: 0,
                fingerprint: this.scopeFingerprint(observed),
                domain: {
                    output: observed.sourceOutput,
                    workspace: observed.sourceWorkspace,
                    bounds: {
                        x: observed.sourceBounds.x,
                        y: observed.sourceBounds.y,
                        w: observed.sourceBounds.w,
                        h: observed.sourceBounds.h,
                    },
                    gap: innerGap,
                    outer_gap: outerGap,
                },
                target_domain: {
                    output: observed.targetOutput,
                    workspace: observed.targetWorkspace,
                    bounds: {
                        x: observed.targetBounds.x,
                        y: observed.targetBounds.y,
                        w: observed.targetBounds.w,
                        h: observed.targetBounds.h,
                    },
                    gap: innerGap,
                    outer_gap: outerGap,
                },
                focused_window: observed.focusedId,
                windows: sourceWindows,
                target_windows: targetWindows,
                command: {
                    op,
                    window: observed.focusedId,
                    target_output: observed.targetOutput,
                    target_workspace: observed.targetWorkspace,
                    follow,
                },
            });
        } catch (error) {
            void error;
            return null;
        }
        return payload;
    }

    private scopeFingerprint(observed: WorkspaceSendObserved): number {
        const sourceIds = observed.sourceWindows.map((entry) => entry.id).sort();
        const targetIds = observed.targetWindows.map((entry) => entry.id).sort();
        const source = workspaceFingerprint(observed.sourceOutput, observed.sourceWorkspace, sourceIds);
        const target = workspaceFingerprint(observed.targetOutput, observed.targetWorkspace, targetIds);
        return (source ^ target) >>> 0;
    }

    private startFlight(
        correlation: string,
        snapshot: WorkspaceSendSnapshot,
        observed: WorkspaceSendObserved,
        targetWorkspace: string,
        moverId: string,
        targetDesktopRef: object | null,
        payload: string,
        requestedOrdinal: number,
        innerGap: number,
        outerGap: number,
        follow: boolean,
        crossOutput: boolean,
    ): void {
        this.inFlight = true;
        this.detachArrival();
        this.diagSeq = 0;
        const flags = snapshotMoverFlags(snapshot, moverId);
        this.pending = {
            correlation,
            snapshot,
            targetWorkspace,
            moverId,
            windowCount: snapshot.sourceWindows.length + snapshot.targetWindows.length,
            requestPayload: payload,
            targetDesktopRef,
            requestedOrdinal,
            innerGap,
            outerGap,
            srcInSource: flags.srcInSource,
            srcInTarget: flags.srcInTarget,
            follow,
            crossOutput,
            moverRef: observed.moverRef ?? null,
            baseRevision: 0,
            planned: null,
            followed: false,
            followOutcome: "not-reached",
        };
        this.diag("request", correlation, 0, "dispatch", "started");
        // True command-dispatch observation at the request boundary, using the
        // original dispatch observation. Best-effort only.
        this.emitFollowDiag(
            correlation,
            0,
            "send-dispatched",
            observed,
            {
                moverId,
                targetWorkspace: snapshot.targetWorkspace,
                targetOutput: snapshot.targetOutput,
                targetDesktopRef,
                requestedOrdinal,
                srcInSource: flags.srcInSource,
                srcInTarget: flags.srcInTarget,
            },
            -1,
            -1,
        );
        this.pinnedOwner = null;
        this.activationStep = 1;
        this.token += 1;
        const flight = this.token;
        this.activeToken = flight;
        // Arm both bounded deadlines from dispatch: the request deadline
        // releases an unanswered request, the arrival deadline bounds the
        // pin and delayed-arrival follow. Separate epochs; neither resets.
        this.deadlineToken += 1;
        this.requestDeadline = this.deadlineToken;
        const requestEpoch = this.requestDeadline;
        let requestCancel: (() => void) | null = null;
        try {
            requestCancel = this.env.scheduleOnce(WORKSPACE_SEND_TIMEOUT_MS, () =>
                this.onRequestTimeout(flight, requestEpoch),
            );
        } catch (error) {
            void error;
            requestCancel = null;
        }
        if (requestCancel === null) {
            this.inFlight = false;
            this.pending = null;
            this.activationStep = 0;
            this.requestDeadline = 0;
            this.refuse("timeout");
            return;
        }
        this.requestTimer = requestCancel;
        this.deadlineToken += 1;
        this.arrivalDeadline = this.deadlineToken;
        const arrivalEpoch = this.arrivalDeadline;
        let arrivalCancel: (() => void) | null = null;
        try {
            arrivalCancel = this.env.scheduleOnce(WORKSPACE_SEND_TIMEOUT_MS, () =>
                this.onArrivalTimeout(flight, arrivalEpoch),
            );
        } catch (error) {
            void error;
            arrivalCancel = null;
        }
        if (arrivalCancel === null) {
            this.clearRequestTimer();
            this.inFlight = false;
            this.pending = null;
            this.activationStep = 0;
            this.requestDeadline = 0;
            this.arrivalDeadline = 0;
            this.refuse("timeout");
            return;
        }
        this.arrivalTimer = arrivalCancel;
        // Phase 1: distinguish presence through NameHasOwner's normal boolean
        // reply. KWin does not call back for GetNameOwner's absent-name error.
        try {
            this.env.callDbus(
                WORKSPACE_SEND_DBUS_SERVICE,
                WORKSPACE_SEND_DBUS_OBJECT,
                WORKSPACE_SEND_DBUS_INTERFACE,
                WORKSPACE_SEND_HAS_OWNER_METHOD,
                WORKSPACE_SEND_SERVICE,
                (reply) => this.onNamePresence(reply, flight, correlation),
            );
        } catch (error) {
            void error;
            this.settleTerminal(flight, correlation, "no-planner", "release");
        }
    }

    private onNamePresence(reply: unknown, flight: number, correlation: string): void {
        if (!this.inFlight || flight !== this.activeToken || this.activationStep !== 1) {
            return;
        }
        if (reply === true) {
            this.activationStep = 2;
            try {
                this.env.callDbus(
                    WORKSPACE_SEND_DBUS_SERVICE,
                    WORKSPACE_SEND_DBUS_OBJECT,
                    WORKSPACE_SEND_DBUS_INTERFACE,
                    WORKSPACE_SEND_GET_OWNER_METHOD,
                    WORKSPACE_SEND_SERVICE,
                    (ownerReply) => this.onOwnerInitial(ownerReply, flight, correlation),
                );
            } catch (error) {
                void error;
                this.settleTerminal(flight, correlation, "no-planner", "release");
            }
            return;
        }
        if (reply !== false) {
            this.settleTerminal(flight, correlation, "no-planner", "release");
            return;
        }
        // Absent name: exactly one StartServiceByName(service, 0) phase.
        this.activationStep = 3;
        this.flightDiag("request", correlation, 0, "activate", "activating");
        try {
            this.env.callDbus(
                WORKSPACE_SEND_DBUS_SERVICE,
                WORKSPACE_SEND_DBUS_OBJECT,
                WORKSPACE_SEND_DBUS_INTERFACE,
                WORKSPACE_SEND_START_METHOD,
                WORKSPACE_SEND_SERVICE,
                (startReply) => this.onStartResult(startReply, flight, correlation),
            );
        } catch (error) {
            void error;
            this.settleTerminal(flight, correlation, "no-planner", "release");
        }
    }

    private onOwnerInitial(reply: unknown, flight: number, correlation: string): void {
        if (!this.inFlight || flight !== this.activeToken || this.activationStep !== 2) {
            return;
        }
        if (!isUniqueOwner(reply)) {
            this.settleTerminal(flight, correlation, "no-planner", "release");
            return;
        }
        this.pinnedOwner = reply;
        this.activationStep = 5;
        this.flightDiag("request", correlation, 0, "activate", "owner-pinned");
        this.sendPlannerRequest(flight, correlation);
    }

    private onStartResult(reply: unknown, flight: number, correlation: string): void {
        if (!this.inFlight || flight !== this.activeToken || this.activationStep !== 3) {
            return;
        }
        if (reply !== WORKSPACE_SEND_START_PRIMARY && reply !== WORKSPACE_SEND_START_ALREADY) {
            this.settleTerminal(flight, correlation, "no-planner", "release");
            return;
        }
        // Exactly one bounded post-activation owner resolution, then pin
        // before any planner call. No retry on failure.
        this.activationStep = 4;
        try {
            this.env.callDbus(
                WORKSPACE_SEND_DBUS_SERVICE,
                WORKSPACE_SEND_DBUS_OBJECT,
                WORKSPACE_SEND_DBUS_INTERFACE,
                WORKSPACE_SEND_GET_OWNER_METHOD,
                WORKSPACE_SEND_SERVICE,
                (ownerReply) => this.onOwnerAfterStart(ownerReply, flight, correlation),
            );
        } catch (error) {
            void error;
            this.settleTerminal(flight, correlation, "no-planner", "release");
        }
    }

    private onOwnerAfterStart(reply: unknown, flight: number, correlation: string): void {
        if (!this.inFlight || flight !== this.activeToken || this.activationStep !== 4) {
            return;
        }
        if (!isUniqueOwner(reply)) {
            this.settleTerminal(flight, correlation, "no-planner", "release");
            return;
        }
        this.pinnedOwner = reply;
        this.activationStep = 5;
        this.flightDiag("request", correlation, 0, "activate", "owner-pinned");
        this.sendPlannerRequest(flight, correlation);
    }

    // Activation/transport diag routed by live flight kind: migrate
    // flights log under the migrate component/route so redaction scopes
    // stay exact per kind. Send flights keep their historical lines.
    private flightDiag(stage: string, correlation: string, revision: number, event: string, outcome: string): void {
        if (this.migrate !== null && this.migrate.correlation === correlation) {
            this.mdiag(stage, correlation, revision, event, outcome);
        } else {
            this.diag(stage, correlation, revision, event, outcome);
        }
    }

    private sendPlannerRequest(flight: number, correlation: string): void {
        if (!this.inFlight || flight !== this.activeToken || this.activationStep !== 5) {
            return;
        }
        const pending = this.pending;
        const migrating = this.migrate;
        const payload =
            pending !== null && pending.correlation === correlation
                ? pending.requestPayload
                : migrating !== null && migrating.correlation === correlation
                  ? migrating.requestPayload
                  : null;
        if (payload === null || !isUniqueOwner(this.pinnedOwner)) {
            this.settleTerminal(flight, correlation, "no-planner", "release");
            return;
        }
        try {
            this.env.callDbus(
                this.pinnedOwner,
                WORKSPACE_SEND_OBJECT,
                WORKSPACE_SEND_INTERFACE,
                WORKSPACE_SEND_METHOD,
                payload,
                (reply) => this.onRequestReply(reply, flight, correlation),
            );
        } catch (error) {
            void error;
            this.settleTerminal(flight, correlation, "owner-loss", "release");
        }
    }

    private onRequestReply(reply: unknown, flight: number, correlation: string): void {
        // A late reply after the request deadline released the flight is
        // ignored by token and never actuates. One bounded redacted line.
        if (!this.inFlight || flight !== this.activeToken) {
            this.logLateReply(reply);
            return;
        }
        const pending = this.pending;
        const migrating = this.migrate;
        const isSend = pending !== null && pending.correlation === correlation;
        const isMigrate = !isSend && migrating !== null && migrating.correlation === correlation;
        if ((!isSend && !isMigrate) || !isUniqueOwner(this.pinnedOwner) || this.activationStep !== 5) {
            return;
        }
        // A duplicate reply after the plan was already bound must never
        // replay native writes.
        if (isSend && pending?.planned !== null) {
            return;
        }
        if (isMigrate && migrating?.planned !== null) {
            return;
        }
        this.clearRequestTimer();
        if (typeof reply !== "string" || reply.length > WORKSPACE_SEND_MAX_REPLY_BYTES) {
            this.settleTerminal(flight, correlation, "service-fault", "release");
            return;
        }
        let parsed: unknown = null;
        try {
            parsed = JSON.parse(reply);
        } catch (error) {
            void error;
            this.settleTerminal(flight, correlation, "service-fault", "release");
            return;
        }
        if (!isRecord(parsed)) {
            this.settleTerminal(flight, correlation, "service-fault", "release");
            return;
        }
        if (parsed["v"] !== WORKSPACE_SEND_CONTRACT_VERSION) {
            this.settleTerminal(flight, correlation, "service-fault", "release");
            return;
        }
        if (parsed["correlation_id"] !== correlation) {
            this.settleTerminal(flight, correlation, "correlation-mismatch", "release");
            return;
        }
        const outcome = parsed["outcome"];
        if (outcome === "rejected" || outcome === "diverged") {
            this.settleTerminal(flight, correlation, sanitizeKind(parsed["kind"]), "release");
            return;
        }
        if (outcome !== "planned") {
            this.settleTerminal(flight, correlation, "service-fault", "release");
            return;
        }
        if (isMigrate && migrating !== null) {
            const planned = validateMigratePlanned(parsed, correlation, migrating);
            if (planned === null) {
                this.settleTerminal(flight, correlation, "precondition-mismatch", "release");
                return;
            }
            if (!migrateBindingHolds(planned, migrating.snapshot)) {
                this.settleTerminal(flight, correlation, "precondition-mismatch", "release");
                return;
            }
            migrating.baseRevision = planned.baseRevision;
            migrating.planned = planned;
            this.mdiag("request", correlation, planned.baseRevision, "plan", "planned");
            this.actuateMigrate(flight, correlation);
            return;
        }
        if (pending === null) {
            this.settleTerminal(flight, correlation, "service-fault", "release");
            return;
        }
        const planned = validatePlanned(parsed, correlation, pending.crossOutput ? "send-to-output" : "send-to-workspace");
        if (planned === null) {
            this.settleTerminal(flight, correlation, "precondition-mismatch", "release");
            return;
        }
        if (!this.geometryCovers(planned, pending)) {
            this.settleTerminal(flight, correlation, "precondition-mismatch", "release");
            return;
        }
        if (!this.operationMatchesSnapshot(planned, pending)) {
            this.settleTerminal(flight, correlation, "precondition-mismatch", "release");
            return;
        }
        pending.baseRevision = planned.baseRevision;
        pending.planned = planned;
        this.diag("request", correlation, planned.baseRevision, "plan", "planned");
        this.actuate(flight, correlation);
    }

    // Complete-reply binding: the reply geometry must cover exactly the
    // observed tiled source+target window set (entries where floating
    // !== true). Floating exceptions stay observed but carry no planned
    // geometry. Fullscreen/maximized overlays stay tiled: fit_excluded
    // alone is NOT floating. Unknown, missing, duplicate, or floated
    // windows never reach native writes. Slotless deferred transfers
    // additionally exclude the mover (no tile slot, no allocation).
    private geometryCovers(planned: WorkspacePlanned, flightState: WorkspacePendingFlight): boolean {
        const moverId = planned.deferred ? (planned.operation["window"] as string) : null;
        const wanted = new Set<string>();
        for (const entry of flightState.snapshot.sourceWindows) {
            if (entry.floating !== true && entry.id !== moverId) {
                wanted.add(entry.id);
            }
        }
        for (const entry of flightState.snapshot.targetWindows) {
            if (entry.floating !== true && entry.id !== moverId) {
                wanted.add(entry.id);
            }
        }
        if (planned.geometry.length !== wanted.size) {
            return false;
        }
        const seen = new Set<string>();
        for (const entry of planned.geometry) {
            if (!wanted.has(entry.window) || seen.has(entry.window)) {
                return false;
            }
            seen.add(entry.window);
        }
        return true;
    }

    // Operation-to-snapshot binding: the move operation must name the
    // flight mover plus the exact captured source/target domains, and its
    // echoed follow selection must equal the flight's requested selection.
    // Any other target/object/selection is a mismatched reply and never
    // actuates. A deferred operation additionally requires the snapshot
    // mover to carry a slotless-transfer shape (D28 held fullscreen or D16
    // ordinary float, never sticky/maximized): an ordinary tiled mover can
    // never sneak a leafless transfer and drop its own allocation. Focus
    // binding is selection-aware: tiled follow
    // names the mover in the target; deferred follow is null
    // (member-less target) or a target-domain bookkeeping MRU; stay is
    // null (emptied source) or names the source domain with a non-mover
    // survivor leaf.
    private operationMatchesSnapshot(planned: WorkspacePlanned, flightState: WorkspacePendingFlight): boolean {
        const operation = planned.operation;
        const snapshot = flightState.snapshot;
        if (
            (operation["window"] as string) !== flightState.moverId ||
            (operation["source_output"] as string) !== snapshot.sourceOutput ||
            (operation["source_workspace"] as string) !== snapshot.sourceWorkspace ||
            (operation["target_output"] as string) !== snapshot.targetOutput ||
            (operation["target_workspace"] as string) !== snapshot.targetWorkspace
        ) {
            return false;
        }
        if (planned.deferred && !snapshotMoverHeld(snapshot, flightState.moverId)) {
            return false;
        }
        if (planned.follow !== flightState.follow) {
            return false;
        }
        const focus = planned.followFocus;
        if (planned.follow) {
            if (planned.deferred) {
                // Slotless follow carries null focus (member-less target)
                // or a target-domain bookkeeping MRU; native follow
                // addresses the mover ref either way.
                if (focus === null) {
                    return true;
                }
                return (
                    focus.output === snapshot.targetOutput && focus.workspace === snapshot.targetWorkspace
                );
            }
            if (focus === null) {
                return false;
            }
            if (
                focus.output !== snapshot.targetOutput ||
                focus.workspace !== snapshot.targetWorkspace ||
                focus.leaf !== (operation["leaf"] as string)
            ) {
                return false;
            }
            return true;
        }
        if (focus === null) {
            return true;
        }
        if (
            focus.output !== snapshot.sourceOutput ||
            focus.workspace !== snapshot.sourceWorkspace
        ) {
            return false;
        }
        // The tiled operation names its mover leaf; a stay focus naming it
        // is the mover, never a survivor. Deferred operations name no leaf.
        if (!planned.deferred && focus.leaf === (operation["leaf"] as string)) {
            return false;
        }
        return true;
    }

    // Identity fence held before every native setter and before switch/focus:
    // the flight token, correlation, and pinned owner/generation binding are
    // unchanged. Snapshot scope is fenced separately against a fresh
    // observation at each boundary.
    private fencesHold(flight: number, correlation: string): boolean {
        const bound = this.pending?.correlation ?? this.migrate?.correlation ?? null;
        return (
            this.inFlight &&
            flight === this.activeToken &&
            bound === correlation &&
            this.activationStep === 5 &&
            isUniqueOwner(this.pinnedOwner)
        );
    }

    // Reply-boundary actuation: re-observe synchronously, exact-revalidate
    // the source+target scope against the flight snapshot, then apply the
    // planned geometry plus the mover's desktop membership. The arrival
    // follow runs once on a fresh exact membership proof: an immediate
    // post-write observation plus, for delayed arrival, a one-shot mover
    // desktopsChanged signal. Never waits for unrelated geometry and never
    // claims a native commit.
    private actuate(flight: number, correlation: string): void {
        const pending = this.pending;
        const planned = pending?.planned ?? null;
        if (pending === null || planned === null || !this.fencesHold(flight, correlation)) {
            this.settleTerminal(flight, correlation, "stale-scope", "release");
            return;
        }
        const fresh = this.freshForPending(pending);
        if (fresh === null) {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return;
        }
        if (!snapshotsEqual(snapshotOf(fresh), pending.snapshot)) {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return;
        }
        // Stay-only source-visibility fence before ANY native setter: an
        // external view switch during the request keeps every pinned
        // membership but revokes visibility, so a stay must run zero
        // writes. Follow re-selects the view itself and is unaffected.
        if (!pending.follow && !sourceStillSelected(fresh, pending.snapshot)) {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return;
        }
        // Explicit output follow never reselects the source (no desktop
        // switch rides an output follow: the destination is already visible
        // on its output), so the actual source current workspace must still
        // match the frozen source before the first setter, for follow and
        // stay alike. A pinned observation can report a hidden old source;
        // following from a switched-away source would strand focus.
        if (pending.crossOutput && !sourceStillSelected(fresh, pending.snapshot)) {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return;
        }
        // Dispatch-frozen gap fence before ANY native setter: a configChanged
        // reload must stale the reply even though bounds/membership still
        // match. Terminal stale with forced refresh via settleTerminal.
        if (!this.flightGapsHold(pending)) {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return;
        }
        // Mid-flight floating gate: either flight domain toggled floating
        // after dispatch must receive zero native writes. Settle normally so
        // the entry refreshes both domains from native observation.
        if (!this.isFlightTiled(pending)) {
            this.settleTerminal(flight, correlation, "workspace-floating", "release");
            return;
        }
        // Arm the one-shot arrival signal BEFORE native writes so a delayed
        // arrival signalling reentrantly between setters and the post-write
        // observation stays observable. Synchronous echo during the write
        // stack stays deferred via nativeWriteDepth and is covered by the
        // immediate post-write observation.
        this.armArrivalSignal(flight, correlation);
        this.nativeWriteDepth += 1;
        // Cross-output order (R4 order): output transfer, mover desktop
        // membership, then planned geometries. Transfer and membership run
        // back-to-back on dispatch-captured refs with token/gap/tiled fences
        // only: no scope re-observation between them can mistake the
        // intended synchronous relocation for stale scope. Geometry uses the
        // same captured refs with a relocation-tolerant fence (a half-applied
        // transfer is valid mid-flight; the strict arrival proof re-observes
        // once all setters have applied). Same-output flights keep the
        // historical geometry-then-membership order with no transfer.
        // A failed transfer settles terminal with no follow.
        let transferred = true;
        let moverWritten = false;
        let geometryWritten = false;
        if (pending.crossOutput) {
            const writeRefs = new Map<string, object>();
            for (const entry of [...fresh.sourceWindows, ...fresh.targetWindows]) {
                if (!writeRefs.has(entry.id)) {
                    writeRefs.set(entry.id, entry.ref);
                }
            }
            const moverRef = writeRefs.get(pending.moverId) ?? null;
            transferred = moverRef !== null && this.writeOutputTransfer(flight, correlation, pending, moverRef);
            // Pre-write observation immediately before the mover membership
            // write. Best-effort only.
            this.emitFollowDiag(correlation, planned.baseRevision, "send-pre-mover", fresh, this.diagBasisOf(pending), -1, -1);
            moverWritten = transferred && this.writeMoverDesktopsCrossOutput(flight, correlation, pending, moverRef);
            geometryWritten = moverWritten && this.writeGeometries(flight, correlation, pending, planned, writeRefs);
        } else {
            transferred = true;
            geometryWritten = this.writeGeometries(flight, correlation, pending, planned);
            // Pre-write observation immediately before the mover membership
            // write. Best-effort only.
            this.emitFollowDiag(correlation, planned.baseRevision, "send-pre-mover", fresh, this.diagBasisOf(pending), -1, -1);
            moverWritten = geometryWritten && this.writeMoverDesktops(flight, correlation, pending);
        }
        this.nativeWriteDepth -= 1;
        if (!transferred || !geometryWritten || !moverWritten) {
            if (!this.isFlightTiled(pending)) {
                this.settleTerminal(flight, correlation, "workspace-floating", "release");
                return;
            }
            if (!this.fencesHold(flight, correlation) || !this.scopeStillMatches(pending) || !this.flightGapsHold(pending)) {
                this.settleTerminal(flight, correlation, "stale-revision", "release");
                return;
            }
            this.settleTerminal(flight, correlation, "write-failed", "release");
            return;
        }
        this.diag("arrival", correlation, planned.baseRevision, "write", "applied");
        // Immediate post-write arrival check; delayed arrival waits on the
        // one-shot mover signal (already armed above) until the bounded
        // arrival deadline.
        if (this.checkArrival(flight, correlation)) {
            return;
        }
        if (this.arrivalDetach === null) {
            this.armArrivalSignal(flight, correlation);
        }
        this.diag("arrival", correlation, planned.baseRevision, "arrival", "waiting");
    }

    private resolveMoverRef(pending: WorkspacePendingFlight, current: WorkspaceSendObserved): object | null {
        for (const entry of current.sourceWindows) {
            if (entry.id === pending.moverId) {
                return entry.ref;
            }
        }
        for (const entry of current.targetWindows) {
            if (entry.id === pending.moverId) {
                return entry.ref;
            }
        }
        return null;
    }

    // Cross-output transfer setter (R4 order, first native write): moves the
    // mover to the exact destination Output object via sendClientToScreen.
    // Fenced on token, flight identity, frozen gaps, and tiled domains only:
    // the mover ref is dispatch-captured and no scope re-observation runs
    // between the back-to-back transfer and membership setters, so the
    // intended synchronous relocation is never mistaken for stale scope. The
    // strict scope fence (geometry) and the arrival proof re-observe once
    // both setters have applied. Same-output flights never call this.
    private writeOutputTransfer(
        flight: number,
        correlation: string,
        pending: WorkspacePendingFlight,
        moverRef: object,
    ): boolean {
        if (!this.fencesHold(flight, correlation) || this.pending !== pending) {
            return false;
        }
        if (!this.isFlightTiled(pending) || !this.flightGapsHold(pending)) {
            return false;
        }
        const resolveOutput = this.env.resolveOutput;
        const sendClientToScreen = this.env.sendClientToScreen;
        if (typeof resolveOutput !== "function" || typeof sendClientToScreen !== "function") {
            return false;
        }
        let outputRef: object | null = null;
        try {
            outputRef = resolveOutput(pending.snapshot.targetOutput);
        } catch (error) {
            void error;
            outputRef = null;
        }
        if (outputRef === null) {
            return false;
        }
        let transferred = false;
        try {
            transferred = sendClientToScreen(moverRef, outputRef) === true;
        } catch (error) {
            void error;
            transferred = false;
        }
        if (transferred) {
            this.diag("arrival", correlation, pending.baseRevision, "transfer", "transferred");
        }
        return transferred;
    }

    // Cross-output membership setter (R4 order, second native write): writes
    // the mover desktop membership with the dispatch-captured target desktop
    // ref. Same minimal fences as the transfer above, plus the live-mover
    // proof (a reentrant close or replace between the two setters must stop
    // here: a dead wrapper may still accept the write), the source-visibility
    // hold, and the domain-scope hold. The strict scope fence runs at
    // geometry time once both setters have applied.
    private writeMoverDesktopsCrossOutput(
        flight: number,
        correlation: string,
        pending: WorkspacePendingFlight,
        moverRef: object | null,
    ): boolean {
        if (moverRef === null) {
            return false;
        }
        if (!this.fencesHold(flight, correlation) || this.pending !== pending) {
            return false;
        }
        if (!this.isFlightTiled(pending) || !this.flightGapsHold(pending)) {
            return false;
        }
        if (!this.crossOutputMoverLive(pending)) {
            return false;
        }
        const fresh = this.freshForPending(pending);
        if (
            fresh === null ||
            !scopeMatchesSnapshot(fresh, pending.snapshot) ||
            !sourceStillSelected(fresh, pending.snapshot)
        ) {
            return false;
        }
        if (pending.targetDesktopRef === null) {
            return false;
        }
        let written = false;
        try {
            written = this.env.setDesktops(moverRef, [pending.targetDesktopRef]) === true;
        } catch (error) {
            void error;
            written = false;
        }
        return written;
    }

    // One-shot delayed-arrival trigger: the next mover desktopsChanged signal
    // re-observes and follows once on a fresh exact membership proof. The
    // armed arrival deadline still bounds the wait; the immediate post-write
    // observation covers synchronous arrival. Armed before native writes so
    // a signal between setters and the post-write observation is not missed.
    // Cross-output flights additionally arm the mover outputChanged fence so
    // a delayed output transfer stays observable through the same one-shot.
    private armArrivalSignal(flight: number, correlation: string): void {
        const pending = this.pending;
        if (pending === null || pending.correlation !== correlation) {
            return;
        }
        if (this.arrivalDetach !== null) {
            return;
        }
        const subscribe = this.env.subscribeMoverDesktops;
        if (typeof subscribe !== "function") {
            return;
        }
        const current = this.freshForPending(pending);
        if (current === null) {
            return;
        }
        const moverRef = this.resolveMoverRef(pending, current);
        if (moverRef === null) {
            return;
        }
        let detach: (() => void) | null = null;
        try {
            detach = subscribe(moverRef, () => this.onArrivalSignal(flight, correlation));
        } catch (error) {
            void error;
            detach = null;
        }
        if (detach === null || typeof detach !== "function") {
            return;
        }
        this.arrivalDetach = detach;
        if (pending.crossOutput && this.arrivalOutputDetach === null) {
            const subscribeOutput = this.env.subscribeMoverOutput;
            if (typeof subscribeOutput === "function") {
                let outputDetach: (() => void) | null = null;
                try {
                    outputDetach = subscribeOutput(moverRef, () => this.onArrivalSignal(flight, correlation));
                } catch (error) {
                    void error;
                    outputDetach = null;
                }
                if (outputDetach !== null && typeof outputDetach === "function") {
                    this.arrivalOutputDetach = outputDetach;
                }
            }
        }
    }

    private detachArrival(): void {
        const detach = this.arrivalDetach;
        this.arrivalDetach = null;
        if (detach !== null) {
            try {
                detach();
            } catch (error) {
                void error;
            }
        }
        const outputDetach = this.arrivalOutputDetach;
        this.arrivalOutputDetach = null;
        if (outputDetach !== null) {
            try {
                outputDetach();
            } catch (error) {
                void error;
            }
        }
    }

    private onArrivalSignal(flight: number, correlation: string): void {
        // A signal arriving mid-write or mid-follow is covered by the
        // immediate post-write observation / in-progress follow instead.
        // Keep the one-shot armed so the delayed arrival stays observable;
        // do not consume it while the write/follow stack is live.
        if (this.nativeWriteDepth > 0 || this.nativeFollowDepth > 0) {
            return;
        }
        // One-shot: detach before any further progress so a duplicate signal
        // cannot produce a second follow.
        const detach = this.arrivalDetach;
        this.arrivalDetach = null;
        if (detach !== null) {
            try {
                detach();
            } catch (error) {
                void error;
            }
        }
        const outputDetach = this.arrivalOutputDetach;
        this.arrivalOutputDetach = null;
        if (outputDetach !== null) {
            try {
                outputDetach();
            } catch (error) {
                void error;
            }
        }
        this.checkArrival(flight, correlation);
    }

    // Live-mover proof for cross-output mid-write steps (membership right
    // after the output setter, geometry across a half-applied transfer):
    // the dispatch-retained ref must still resolve to the identical live
    // object in the current window list with its native identity and clean
    // exception flags. Scope homing is never consulted (half-applied
    // transfers are valid mid-flight); a closed, replaced, or excepted
    // mover fails closed before any later setter. Never invents identity.
    private crossOutputMoverLive(pending: WorkspacePendingFlight): boolean {
        const moverRef = pending.moverRef;
        if (moverRef === null) {
            return false;
        }
        const hook = this.env.readMoverLive;
        if (typeof hook !== "function") {
            return false;
        }
        let live: {
            readonly id: string;
            readonly floating: boolean;
            readonly sticky: boolean;
            readonly fullscreen: boolean;
            readonly maximized: boolean;
        } | null = null;
        try {
            live = hook(moverRef);
        } catch (error) {
            void error;
            return false;
        }
        if (live === null || typeof live !== "object") {
            return false;
        }
        if (
            live.id !== pending.moverId ||
            typeof live.floating !== "boolean" ||
            typeof live.sticky !== "boolean" ||
            typeof live.fullscreen !== "boolean" ||
            typeof live.maximized !== "boolean"
        ) {
            return false;
        }
        // D16 ordinary-float carry: a floating mover travels with its float
        // state untouched, but only when flag-stable against the dispatch
        // snapshot. A tiled mover that toggled floating mid-flight still
        // fails closed here; sticky never carries.
        let snapshotFloating: { floating: boolean } | null = null;
        for (const entry of [...pending.snapshot.sourceWindows, ...pending.snapshot.targetWindows]) {
            if (entry.id === pending.moverId) {
                snapshotFloating = { floating: entry.floating };
                break;
            }
        }
        if (snapshotFloating === null || live.floating !== snapshotFloating.floating || live.sticky) {
            return false;
        }
        // G-D2/D28 overlay carry (REQ-MAX-09): a maximized/fullscreen mover
        // travels with its native state untouched, but only when flag-stable
        // against the dispatch snapshot; a mid-flight overlay change fails
        // closed. Sticky never carries here.
        let snapshotOverlay: { maximized: boolean; fullscreen: boolean } | null = null;
        for (const entry of [...pending.snapshot.sourceWindows, ...pending.snapshot.targetWindows]) {
            if (entry.id === pending.moverId) {
                snapshotOverlay = { maximized: entry.maximized, fullscreen: entry.fullscreen };
                break;
            }
        }
        if (
            snapshotOverlay === null ||
            live.maximized !== snapshotOverlay.maximized ||
            live.fullscreen !== snapshotOverlay.fullscreen
        ) {
            return false;
        }
        return true;
    }

    // Retained-ref stability across a fresh observation: every window
    // carrying the mover id must be the identical live object as the
    // dispatch-retained ref. A replaced mover (same stable id, new object)
    // fails here even when homed with clean flags; an unhomed mover passes
    // vacuously and proves out through the live-mover hook instead.
    private moverRefStable(
        fresh: WorkspaceSendObserved,
        pending: WorkspacePendingFlight,
    ): boolean {
        for (const entry of [...fresh.sourceWindows, ...fresh.targetWindows]) {
            if (entry.id === pending.moverId && entry.ref !== pending.moverRef) {
                return false;
            }
        }
        return true;
    }

    // Synchronous native placement read for the cross-output arrival proof
    // on the dispatch-retained mover ref. True only when the mover reads
    // back exactly on the destination output with exactly the destination
    // workspace as its sole desktop. Scope-homed scans cannot see a
    // half-applied transfer, so this never homes: throwing or unreadable
    // reads are not arrival (the flight keeps waiting for the signal or the
    // bounded arrival deadline; there is no cross-output mover-closed fast
    // path). Scope divergence is handled by the caller.
    private moverPlacementMatches(pending: WorkspacePendingFlight): boolean {
        const moverRef = pending.moverRef;
        if (moverRef === null) {
            return false;
        }
        const snapshot = pending.snapshot;
        let output: string | null = null;
        let desktopIds: ReadonlyArray<string> | null = null;
        try {
            output = this.env.readOutputName?.(moverRef) ?? null;
            desktopIds = this.env.readDesktopIds?.(moverRef) ?? null;
        } catch (error) {
            void error;
            return false;
        }
        if (output === null || desktopIds === null) {
            return false;
        }
        return (
            output === snapshot.targetOutput &&
            desktopIds.length === 1 &&
            desktopIds[0] === snapshot.targetWorkspace
        );
    }

    // Fresh exact arrival proof: the mover is absent from the source and
    // present on the target with the dispatch scope unchanged. Returns true
    // when the flight reached a terminal path (arrival follow done, closed
    // mover, or stale scope); false when the mover simply has not arrived
    // yet and the flight keeps waiting for the signal or the deadline.
    // Never waits for unrelated geometry. Cross-output flights prove
    // arrival by native placement on the dispatch-retained mover ref (exact
    // destination output plus sole destination desktop): a half-applied
    // transfer is valid mid-flight and must wait, never settle as closed.
    private checkArrival(flight: number, correlation: string): boolean {
        const pending = this.pending;
        const planned = pending?.planned ?? null;
        if (pending === null || planned === null || !this.fencesHold(flight, correlation)) {
            return false;
        }
        const fresh = this.freshForPending(pending);
        if (fresh === null) {
            return false;
        }
        if (!scopeMatchesSnapshot(fresh, pending.snapshot)) {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return true;
        }
        if (pending.crossOutput) {
            // Placement plus live-mover proof on the dispatch-retained ref:
            // retained properties may stay readable on a removed or replaced
            // wrapper, so the hook must still resolve the identical live
            // object with its native identity and clean flags. A half-
            // applied transfer is valid mid-flight and waits; anything else
            // unproven waits for the signal or the bounded arrival deadline.
            if (!this.moverRefStable(fresh, pending) || !this.crossOutputMoverLive(pending) || !this.moverPlacementMatches(pending)) {
                return false;
            }
            this.diag("arrival", correlation, planned.baseRevision, "arrival", "arrived");
            // Post-write observation: verified placement plus the frozen
            // dispatch source in the diagnostic basis. Best-effort.
            this.emitFollowDiag(correlation, planned.baseRevision, "send-post-mover", fresh, this.diagBasisOf(pending), -1, -1);
            this.followOnce(flight, correlation, fresh);
            this.settleTerminal(flight, correlation, "arrived", "arrival");
            return true;
        }
        let inSource = false;
        for (const entry of fresh.sourceWindows) {
            if (entry.id === pending.moverId) {
                inSource = true;
                break;
            }
        }
        let moverRef: object | null = null;
        for (const entry of fresh.targetWindows) {
            if (entry.id === pending.moverId) {
                moverRef = entry.ref;
                break;
            }
        }
        if (!inSource && moverRef !== null) {
            this.diag("arrival", correlation, planned.baseRevision, "arrival", "arrived");
            // Post-write observation: verified read shows the mover in target
            // while the basis keeps the frozen dispatch source. Best-effort.
            this.emitFollowDiag(correlation, planned.baseRevision, "send-post-mover", fresh, this.diagBasisOf(pending), -1, -1);
            this.followOnce(flight, correlation, fresh);
            this.settleTerminal(flight, correlation, "arrived", "arrival");
            return true;
        }
        if (!inSource && moverRef === null) {
            // The mover is in neither scope: closed or moved elsewhere
            // mid-flight. The entry refresh on settlement converges both
            // domains from native observation; never a phantom, no replay.
            this.settleTerminal(flight, correlation, "mover-closed", "release");
            return true;
        }
        return false;
    }

    // Follow a confirmed native mover transfer exactly once: switch to the
    // target desktop, then focus the mover. A fresh exact arrival proof is
    // required before the switch AND before focus; ambiguous or stale proof
    // refuses the follow without setters. A failed or ambiguous switch
    // never focuses; switch/focus failures are logged without retry or
    // setter replay. Token, owner, and scope fences are rechecked before
    // each setter. Stay flights confirm instead: the same fresh arrival
    // proof is required, then the bound source-MRU survivor is focused (or
    // zero setters for null focus) with no desktop switch ever running, so
    // the source stays selected and history observes no spurious switch.
    private followOnce(
        flight: number,
        correlation: string,
        arrival: WorkspaceSendObserved,
    ): void {
        void arrival;
        const pending = this.pending;
        const planned = pending?.planned ?? null;
        if (pending === null || planned === null || pending.followed || !this.fencesHold(flight, correlation)) {
            return;
        }
        if (!pending.follow) {
            this.confirmStayOnce(flight, correlation);
            return;
        }
        // Cross-output follow skips the desktop switch: the destination
        // workspace is already the current workspace on its output, so only
        // the mover focus carries the follow. Stay reuses the shared
        // source-MRU path above unchanged.
        if (pending.crossOutput) {
            this.followOutputOnce(flight, correlation);
            return;
        }
        const switchToTarget = this.env.switchToTarget;
        const focusWindow = this.env.focusWindow;
        if (typeof switchToTarget !== "function" || typeof focusWindow !== "function") {
            pending.followed = true;
            pending.followOutcome = "hooks-unavailable";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        // Fresh arrival membership proof before the switch: the passed
        // arrival may be stale after the intervening setters.
        const preSwitch = this.freshForPending(pending);
        if (preSwitch === null || !this.fencesHold(flight, correlation) || this.pending !== pending) {
            pending.followed = true;
            pending.followOutcome = "arrival-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        let preSwitchMoverRef: object | null = null;
        let preSwitchInSource = false;
        for (const entry of preSwitch.sourceWindows) {
            if (entry.id === pending.moverId) {
                preSwitchInSource = true;
                break;
            }
        }
        for (const entry of preSwitch.targetWindows) {
            if (entry.id === pending.moverId) {
                preSwitchMoverRef = entry.ref;
                break;
            }
        }
        const preSwitchDesktopRef = preSwitch.targetDesktopRef;
        if (
            preSwitchInSource ||
            preSwitchMoverRef === null ||
            preSwitchDesktopRef === null ||
            !scopeMatchesSnapshot(preSwitch, pending.snapshot)
        ) {
            pending.followed = true;
            pending.followOutcome = "arrival-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        // Flight-pinned diagnostic basis. The fences above stay the only
        // correctness checks; wrapper equality below is diagnostic only.
        // Source membership stays frozen from the immutable dispatch snapshot.
        const basis: WorkspaceFollowDiagBasis = this.diagBasisOf(pending);
        pending.followed = true;
        this.emitFollowDiag(correlation, planned.baseRevision, "follow-pre", preSwitch, basis, -1, -1);
        let switched = false;
        this.nativeFollowDepth += 1;
        try {
            switched = switchToTarget(preSwitchDesktopRef, {
                correlation,
                revision: planned.baseRevision,
                nextSequence: () => this.nextDiagSeq(),
            }) === true;
        } catch (error) {
            void error;
            switched = false;
        }
        // Immediate after-setter observation: one best-effort synchronous
        // public re-read. A null observation logs unknown fields; the switch
        // result below is unaffected.
        let postSwitch: WorkspaceSendObserved | null = null;
        try {
            postSwitch = this.freshForPending(pending);
        } catch (error) {
            void error;
            postSwitch = null;
        }
        this.emitFollowDiag(
            correlation,
            planned.baseRevision,
            "follow-switched",
            postSwitch,
            basis,
            switched ? 1 : 0,
            -1,
        );
        if (!switched) {
            pending.followOutcome = "switch-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            this.nativeFollowDepth -= 1;
            return;
        }
        if (!this.fencesHold(flight, correlation) || this.pending !== pending) {
            this.nativeFollowDepth -= 1;
            return;
        }
        // Fresh arrival membership proof before focus: the mover may have
        // closed or moved elsewhere during the switch. Ambiguous or stale
        // proof refuses focus without a setter.
        const preFocus = this.freshForPending(pending);
        if (preFocus === null || !this.fencesHold(flight, correlation) || this.pending !== pending) {
            pending.followOutcome = "arrival-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            this.nativeFollowDepth -= 1;
            return;
        }
        let focusMoverRef: object | null = null;
        let focusInSource = false;
        for (const entry of preFocus.sourceWindows) {
            if (entry.id === pending.moverId) {
                focusInSource = true;
                break;
            }
        }
        for (const entry of preFocus.targetWindows) {
            if (entry.id === pending.moverId) {
                focusMoverRef = entry.ref;
                break;
            }
        }
        if (focusInSource || focusMoverRef === null || !scopeMatchesSnapshot(preFocus, pending.snapshot)) {
            pending.followOutcome = "arrival-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            this.nativeFollowDepth -= 1;
            return;
        }
        let focused = false;
        try {
            focused = focusWindow(focusMoverRef, {
                correlation,
                revision: planned.baseRevision,
                nextSequence: () => this.nextDiagSeq(),
            }) === true;
        } catch (error) {
            void error;
            focused = false;
        }
        // After-focus observation: one best-effort synchronous public
        // re-read. Diagnostic only; the focus result below is unaffected.
        let postFocus: WorkspaceSendObserved | null = null;
        try {
            postFocus = this.freshForPending(pending);
        } catch (error) {
            void error;
            postFocus = null;
        }
        this.emitFollowDiag(
            correlation,
            planned.baseRevision,
            "follow-focused",
            postFocus,
            basis,
            1,
            focused ? 1 : 0,
        );
        pending.followOutcome = focused ? "state-confirmed" : "focus-unconfirmed";
        this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
        this.nativeFollowDepth -= 1;
    }

    // Cross-output follow for a verified transfer: re-prove the exact
    // arrival by native placement on the dispatch-retained mover ref (exact
    // destination output plus sole destination desktop, dispatch scope
    // unchanged), then focus the mover with no desktop switch ever running.
    // The destination is already visible on its output; the focus carries
    // the follow. The source pin persists through focus even though the
    // active window changed outputs. A failed or ambiguous proof refuses
    // without setters.
    private followOutputOnce(flight: number, correlation: string): void {
        const pending = this.pending;
        const planned = pending?.planned ?? null;
        if (pending === null || planned === null || pending.followed || !this.fencesHold(flight, correlation)) {
            return;
        }
        const focusWindow = this.env.focusWindow;
        if (typeof focusWindow !== "function") {
            pending.followed = true;
            pending.followOutcome = "hooks-unavailable";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        const proof = this.freshForPending(pending);
        if (proof === null || !this.fencesHold(flight, correlation) || this.pending !== pending) {
            pending.followed = true;
            pending.followOutcome = "arrival-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        // Scope-homed scans cannot see a half-applied transfer; the mover
        // must additionally be absent from the live source scope here (a
        // mover still fully home pre-arrival waits instead).
        let focusInSource = false;
        for (const entry of proof.sourceWindows) {
            if (entry.id === pending.moverId) {
                focusInSource = true;
                break;
            }
        }
        const focusMoverRef = pending.moverRef;
        if (
            focusInSource ||
            focusMoverRef === null ||
            !scopeMatchesSnapshot(proof, pending.snapshot) ||
            !this.moverRefStable(proof, pending) ||
            !this.crossOutputMoverLive(pending) ||
            !this.moverPlacementMatches(pending)
        ) {
            pending.followed = true;
            pending.followOutcome = "arrival-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        const basis: WorkspaceFollowDiagBasis = this.diagBasisOf(pending);
        pending.followed = true;
        this.emitFollowDiag(correlation, planned.baseRevision, "follow-pre", proof, basis, -1, -1);
        let focused = false;
        this.nativeFollowDepth += 1;
        try {
            if (!this.fencesHold(flight, correlation) || this.pending !== pending) {
                this.nativeFollowDepth -= 1;
                return;
            }
            focused = focusWindow(focusMoverRef, {
                correlation,
                revision: planned.baseRevision,
                nextSequence: () => this.nextDiagSeq(),
            }) === true;
        } catch (error) {
            void error;
            focused = false;
        }
        let postFocus: WorkspaceSendObserved | null = null;
        try {
            postFocus = this.freshForPending(pending);
        } catch (error) {
            void error;
            postFocus = null;
        }
        this.emitFollowDiag(
            correlation,
            planned.baseRevision,
            "follow-focused",
            postFocus,
            basis,
            1,
            focused ? 1 : 0,
        );
        pending.followOutcome = focused ? "state-confirmed" : "focus-unconfirmed";
        this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
        this.nativeFollowDepth -= 1;
    }

    // Stay focus for a verified transfer: re-prove the exact arrival
    // membership (mover absent from source, present on target, dispatch
    // scope unchanged) on a fresh observation, then focus the exact
    // source-MRU survivor bound from the reply geometry at validation time.
    // No desktop switch ever runs on this path, so the source stays
    // selected and history observes no switch. Null desired focus (emptied
    // or MRU-less source) applies the focused-removal null-focus rule: zero
    // setters, native removal focus stands. Ambiguous or stale proof, a
    // missing survivor, or a failed focus write refuses without further
    // setters. A stay needing focus without a focus hook reports
    // hooks-unavailable like follow; null-focus stays never need hooks.
    private confirmStayOnce(flight: number, correlation: string): void {
        const pending = this.pending;
        const planned = pending?.planned ?? null;
        if (pending === null || planned === null || pending.followed || !this.fencesHold(flight, correlation)) {
            return;
        }
        const proof = this.freshForPending(pending);
        if (proof === null || !this.fencesHold(flight, correlation) || this.pending !== pending) {
            pending.followed = true;
            pending.followOutcome = "arrival-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        // Mover arrival: cross-output flights prove native placement on the
        // dispatch-retained ref (a half-applied transfer is valid mid-flight
        // and must not read as unarrived); same-output flights use
        // scope-homed membership. Either way the mover must additionally be
        // absent from the live source scope and the dispatch scope must
        // still match.
        let proofInSource = false;
        for (const entry of proof.sourceWindows) {
            if (entry.id === pending.moverId) {
                proofInSource = true;
                break;
            }
        }
        let moverArrived = false;
        if (pending.crossOutput) {
            moverArrived = !proofInSource && this.moverRefStable(proof, pending) && this.moverPlacementMatches(pending);
        } else {
            for (const entry of proof.targetWindows) {
                if (entry.id === pending.moverId) {
                    moverArrived = !proofInSource;
                    break;
                }
            }
        }
        if (!moverArrived || !scopeMatchesSnapshot(proof, pending.snapshot)) {
            pending.followed = true;
            pending.followOutcome = "arrival-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        // Stay-only source-visibility fence on the same fresh proof: an
        // external view switch after the membership write keeps membership
        // and pinned scope but revokes visibility. Zero focus setters on a
        // stale source - including the null-focus path, which must also see
        // the source still selected before stay-confirmed. A source-empty
        // stay therefore never selects the target.
        if (!sourceStillSelected(proof, pending.snapshot)) {
            pending.followed = true;
            pending.followOutcome = "arrival-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        if (planned.stayFocusWindow === null) {
            pending.followed = true;
            pending.followOutcome = "stay-confirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        const focusWindow = this.env.focusWindow;
        if (typeof focusWindow !== "function") {
            pending.followed = true;
            pending.followOutcome = "hooks-unavailable";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        // The survivor resolves from the same fresh proof by the
        // validation-bound stable id: a survivor that left the source under
        // us is stale scope, never a guessed fallback.
        let survivorRef: object | null = null;
        for (const entry of proof.sourceWindows) {
            if (entry.id === planned.stayFocusWindow) {
                survivorRef = entry.ref;
                break;
            }
        }
        if (survivorRef === null) {
            pending.followed = true;
            pending.followOutcome = "arrival-unconfirmed";
            this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
            return;
        }
        const basis: WorkspaceFollowDiagBasis = this.diagBasisOf(pending);
        pending.followed = true;
        this.emitFollowDiag(correlation, planned.baseRevision, "stay-pre", proof, basis, -1, -1);
        let focused = false;
        this.nativeFollowDepth += 1;
        try {
            if (!this.fencesHold(flight, correlation) || this.pending !== pending) {
                this.nativeFollowDepth -= 1;
                return;
            }
            focused = focusWindow(survivorRef, {
                correlation,
                revision: planned.baseRevision,
                nextSequence: () => this.nextDiagSeq(),
            }) === true;
        } catch (error) {
            void error;
            focused = false;
        }
        // After-focus observation: one best-effort synchronous public
        // re-read. Diagnostic only; the focus result below is unaffected.
        let postFocus: WorkspaceSendObserved | null = null;
        try {
            postFocus = this.freshForPending(pending);
        } catch (error) {
            void error;
            postFocus = null;
        }
        this.emitFollowDiag(
            correlation,
            planned.baseRevision,
            "stay-focused",
            postFocus,
            basis,
            -1,
            focused ? 1 : 0,
        );
        pending.followOutcome = focused ? "stay-confirmed" : "focus-unconfirmed";
        this.diag("follow", correlation, planned.baseRevision, "follow", pending.followOutcome);
        this.nativeFollowDepth -= 1;
    }

    // Flight-pinned diagnostic basis using the dispatch-frozen source flags
    // stored on the pending flight. Never rederives from live reads.
    private diagBasisOf(pending: WorkspacePendingFlight): WorkspaceFollowDiagBasis {
        return {
            moverId: pending.moverId,
            targetWorkspace: pending.snapshot.targetWorkspace,
            targetOutput: pending.snapshot.targetOutput,
            targetDesktopRef: pending.targetDesktopRef,
            requestedOrdinal: pending.requestedOrdinal,
            srcInSource: pending.srcInSource,
            srcInTarget: pending.srcInTarget,
        };
    }

    // Best-effort redacted visible-follow observation. Emits one fixed
    // `route-diag component=cosmic-send stage=follow` line carrying only
    // session-local ordinals/counts plus 0/1/-1 equality flags (-1 unknown):
    // req_ord (requested logical ordinal from the plan entry, 0 permitted for
    // the trailing target), tgt_ord/tgt_num (flight target live list order and
    // KWin native x11DesktopNumber), cur_ord/cur_num (live current desktop for
    // the selected output), cur_id_eq (current stable id === flight target
    // stable id), cur_ref_eq (current wrapper === flight target wrapper,
    // diagnostic only), out_ord (selected output live screens index), out_eq
    // (selected output === request target output), desktops (live desktop
    // count), mover_in_target (mover by stable id present in live target
    // windows), active_is_mover (live active wrapper === live mover wrapper),
    // src_in_src/src_in_tgt (mover present in the immutable dispatch snapshot
    // source/target scopes, frozen at dispatch so later dynamic reads never
    // overwrite the source view), switched/focused (native hook results, -1
    // when not yet invoked).
    // Raw desktop ids, output identifiers, window native ids, object refs,
    // captions, app data, payload, and environment never enter logs. Wrapper
    // equality here is diagnostic only; existing follow gates stay unchanged.
    // Never throws or affects follow result, arrival, focus behavior,
    // enablement, or flight state.
    private emitFollowDiag(
        correlation: string,
        revision: number,
        event: string,
        current: WorkspaceSendObserved | null,
        basis: WorkspaceFollowDiagBasis | null,
        switched: number,
        focused: number,
    ): void {
        try {
            const outcome = current === null ? "unknown" : "observed";
            const reqOrd = basis === null ? -1 : toDiagInt(basis.requestedOrdinal, 0, 9);
            const tgtOrd = current === null ? -1 : toDiagInt(current.targetOrdinal, 0, WORKSPACE_SEND_MAX_DESKTOPS);
            const tgtNum = current === null ? -1 : toDiagInt(current.targetNumber, 1, WORKSPACE_SEND_MAX_REVISION);
            const curOrd = current === null ? -1 : toDiagInt(current.currentOrdinal, 0, WORKSPACE_SEND_MAX_DESKTOPS);
            const curNum = current === null ? -1 : toDiagInt(current.currentNumber, 1, WORKSPACE_SEND_MAX_REVISION);
            const curIdEq =
                current === null ? -1 : current.currentIdEq === 0 || current.currentIdEq === 1 ? current.currentIdEq : -1;
            const curRefEq =
                current === null ? -1 : current.currentRefEq === 0 || current.currentRefEq === 1 ? current.currentRefEq : -1;
            const outOrd = current === null ? -1 : toDiagInt(current.outputOrdinal, 0, 1024);
            const desktops = current === null ? -1 : toDiagInt(current.desktopCount, 0, WORKSPACE_SEND_MAX_REVISION);
            let outEq = -1;
            try {
                if (current !== null && basis !== null) {
                    outEq =
                        current.sourceOutput === basis.targetOutput && current.targetOutput === basis.targetOutput ? 1 : 0;
                }
            } catch (error) {
                void error;
                outEq = -1;
            }
            let moverInTarget = -1;
            let moverWrapper: object | null = null;
            try {
                if (current !== null && basis !== null) {
                    moverInTarget = 0;
                    for (const entry of current.targetWindows) {
                        if (entry.id === basis.moverId) {
                            moverInTarget = 1;
                            moverWrapper = entry.ref;
                            break;
                        }
                    }
                    if (moverWrapper === null) {
                        for (const entry of current.sourceWindows) {
                            if (entry.id === basis.moverId) {
                                moverWrapper = entry.ref;
                                break;
                            }
                        }
                    }
                }
            } catch (error) {
                void error;
                moverInTarget = -1;
                moverWrapper = null;
            }
            let activeIsMover = -1;
            try {
                if (current === null || moverWrapper === null) {
                    activeIsMover = -1;
                } else {
                    activeIsMover = current.activeRef === moverWrapper ? 1 : 0;
                }
            } catch (error) {
                void error;
                activeIsMover = -1;
            }
            const switchedFlag = switched === 0 || switched === 1 ? switched : -1;
            const focusedFlag = focused === 0 || focused === 1 ? focused : -1;
            let srcInSrc = -1;
            let srcInTgt = -1;
            try {
                if (basis !== null) {
                    srcInSrc = basis.srcInSource === 0 || basis.srcInSource === 1 ? basis.srcInSource : -1;
                    srcInTgt = basis.srcInTarget === 0 || basis.srcInTarget === 1 ? basis.srcInTarget : -1;
                }
            } catch (error) {
                void error;
                srcInSrc = -1;
                srcInTgt = -1;
            }
            this.env.log(
                `${LOG_PREFIX} component=${WORKSPACE_SEND_COMPONENT} stage=follow correlation=${correlation} generation=${this.generation} revision=${String(revision)} diag_seq=${String(this.nextDiagSeq())} event=${event} outcome=${outcome} req_ord=${String(reqOrd)} tgt_ord=${String(tgtOrd)} tgt_num=${String(tgtNum)} cur_ord=${String(curOrd)} cur_num=${String(curNum)} cur_id_eq=${String(curIdEq)} cur_ref_eq=${String(curRefEq)} out_ord=${String(outOrd)} out_eq=${String(outEq)} desktops=${String(desktops)} mover_in_target=${String(moverInTarget)} active_is_mover=${String(activeIsMover)} src_in_src=${String(srcInSrc)} src_in_tgt=${String(srcInTgt)} switched=${String(switchedFlag)} focused=${String(focusedFlag)}`,
            );
        } catch (error) {
            void error;
        }
    }

    // Dispatch-frozen gap fence: the live configured pair must still equal
    // the flight-frozen primitives before any reply geometry or membership
    // write. Never throws. Structural over both flight kinds.
    private flightGapsHold(pending: { readonly innerGap: number; readonly outerGap: number }): boolean {
        try {
            return pending.innerGap === this.innerGap && pending.outerGap === this.outerGap;
        } catch (error) {
            void error;
            return false;
        }
    }

    // Bounded scope fence for mid-write checks: a fresh observation still
    // carries the exact dispatch source+target scope. Never throws.
    private scopeStillMatches(pending: WorkspacePendingFlight): boolean {
        try {
            const fresh = this.freshForPending(pending);
            return (
                fresh !== null &&
                scopeMatchesSnapshot(fresh, pending.snapshot) &&
                this.flagsStillMatch(fresh, pending.snapshot)
            );
        } catch (error) {
            void error;
            return false;
        }
    }

    // Earlier setters may change rects; fence flags without comparing rects.
    private flagsStillMatch(current: WorkspaceSendObserved, snapshot: WorkspaceSendSnapshot): boolean {
        try {
            const freshById = new Map([...current.sourceWindows, ...current.targetWindows].map((entry) => [entry.id, entry]));
            return [...snapshot.sourceWindows, ...snapshot.targetWindows].every((entry) => {
                const fresh = freshById.get(entry.id);
                return fresh !== undefined &&
                    (fresh.fullscreen === true) === entry.fullscreen &&
                    (fresh.maximized === true) === entry.maximized &&
                    (fresh.floating === true) === entry.floating &&
                    (fresh.sticky === true) === entry.sticky &&
                    (fresh.fitExcluded === true || fresh.fit_excluded === true) === entry.fitExcluded;
            });
        } catch (error) {
            void error;
            return false;
        }
    }

    // Current-mode gate for a live flight: both flight domains must still
    // be tiled. Either side toggling floating mid-flight forbids further
    // geometry writes. Absent gate or exceptions fail open (tiled).
    // Structural over both flight kinds: migrate source and target share
    // one backing id, so both gates read the same workspace.
    private isFlightTiled(pending: {
        readonly snapshot: {
            readonly sourceOutput: string;
            readonly sourceWorkspace: string;
            readonly targetOutput: string;
            readonly targetWorkspace: string;
        };
    }): boolean {
        try {
            const gate = this.env.isDomainTiled;
            if (typeof gate !== "function") {
                return true;
            }
            const snapshot = pending.snapshot;
            if (gate(snapshot.sourceOutput, snapshot.sourceWorkspace) === false) {
                return false;
            }
            if (gate(snapshot.targetOutput, snapshot.targetWorkspace) === false) {
                return false;
            }
            return true;
        } catch (error) {
            void error;
            return true;
        }
    }

    // Relocation-tolerant scope fence for cross-output geometry writes
    // (R4 parity): the dispatch domains, gaps, tiled modes, and live source
    // selection must hold. Observed windows carry no output/workspace
    // fields: scope comes from list membership (sourceWindows vs
    // targetWindows). The non-mover set must equal exactly: every snapshot
    // survivor homed in its dispatch list with identical flags, with no
    // missing survivors and no newly arrived windows. The mover must sit in
    // at most one list, homed and non-exceptional, or in neither mid-
    // transfer with the live-mover proof (identical live ref, native
    // identity, clean flags); a closed, replaced, or excepted mover fails
    // closed here with zero further setters. Dispatch-captured refs still
    // target an unhomed mover safely; global coordinates need no output.
    private crossOutputGeometryScopeAllows(
        flight: number,
        correlation: string,
        pending: WorkspacePendingFlight,
    ): boolean {
        if (!this.fencesHold(flight, correlation) || this.pending !== pending) {
            return false;
        }
        if (!this.flightGapsHold(pending)) {
            return false;
        }
        const fresh = this.freshForPending(pending);
        if (fresh === null || !scopeMatchesSnapshot(fresh, pending.snapshot)) {
            return false;
        }
        if (!sourceStillSelected(fresh, pending.snapshot)) {
            return false;
        }
        const snapshot = pending.snapshot;
        const freshById = new Map<string, WorkspaceSendObservedWindow>();
        const freshSourceIds = new Set<string>();
        const freshTargetIds = new Set<string>();
        for (const entry of fresh.sourceWindows) {
            if (!freshById.has(entry.id)) {
                freshById.set(entry.id, entry);
            }
            freshSourceIds.add(entry.id);
        }
        for (const entry of fresh.targetWindows) {
            if (!freshById.has(entry.id)) {
                freshById.set(entry.id, entry);
            }
            freshTargetIds.add(entry.id);
        }
        const flagsEqual = (
            seen: WorkspaceSendObservedWindow,
            expected: WorkspaceSendSnapshotWindow,
        ): boolean =>
            (seen.fullscreen === true) === expected.fullscreen &&
            (seen.maximized === true) === expected.maximized &&
            (seen.floating === true) === expected.floating &&
            (seen.sticky === true) === expected.sticky &&
            (seen.fitExcluded === true || seen.fit_excluded === true) === expected.fitExcluded;
        const snapshotNonMovers = new Set<string>();
        for (const entry of [...snapshot.sourceWindows, ...snapshot.targetWindows]) {
            if (entry.id !== pending.moverId) {
                snapshotNonMovers.add(entry.id);
            }
        }
        const freshNonMovers = new Set<string>();
        for (const id of [...freshSourceIds, ...freshTargetIds]) {
            if (id !== pending.moverId) {
                freshNonMovers.add(id);
            }
        }
        // Exact non-mover set equality: no missing survivors, no newly
        // arrived windows mid-write.
        if (freshNonMovers.size !== snapshotNonMovers.size) {
            return false;
        }
        for (const id of snapshotNonMovers) {
            if (!freshNonMovers.has(id)) {
                return false;
            }
        }
        for (const entry of snapshot.sourceWindows) {
            if (entry.id === pending.moverId) {
                continue;
            }
            const seen = freshById.get(entry.id);
            if (seen === undefined || !freshSourceIds.has(entry.id)) {
                return false;
            }
            if (!flagsEqual(seen, entry)) {
                return false;
            }
        }
        for (const entry of snapshot.targetWindows) {
            if (entry.id === pending.moverId) {
                continue;
            }
            const seen = freshById.get(entry.id);
            if (seen === undefined || !freshTargetIds.has(entry.id)) {
                return false;
            }
            if (!flagsEqual(seen, entry)) {
                return false;
            }
        }
        // The mover may sit in either dispatch list (source, half, or
        // target placement by list homing) or in neither mid-transfer.
        // Homed movers must stay non-exceptional; unhomed movers prove
        // identical live ref, native identity, and clean flags, and prove
        // out at the placement arrival proof.
        const moverInSource = freshSourceIds.has(pending.moverId);
        const moverInTarget = freshTargetIds.has(pending.moverId);
        if (moverInSource && moverInTarget) {
            return false;
        }
        if (moverInSource || moverInTarget) {
            const moverSeen = freshById.get(pending.moverId);
            if (moverSeen === undefined) {
                return false;
            }
            // G-D2/D28 overlay carry (REQ-MAX-09): a homed mover must
            // flag-match its dispatch snapshot entry; floating and sticky
            // never carry, while a stable maximized/fullscreen flag travels
            // with the window (geometry writes already skip overlays).
            let snapshotMover: WorkspaceSendSnapshotWindow | null = null;
            for (const entry of [...snapshot.sourceWindows, ...snapshot.targetWindows]) {
                if (entry.id === pending.moverId) {
                    snapshotMover = entry;
                    break;
                }
            }
            if (snapshotMover === null || !flagsEqual(moverSeen, snapshotMover)) {
                return false;
            }
            if (moverSeen.floating === true || moverSeen.sticky === true) {
                return false;
            }
            // A homed mover must be the dispatch-retained live object: a
            // same-id replacement fails here even with clean flags.
            if (moverSeen.ref !== pending.moverRef) {
                return false;
            }
            return true;
        }
        return this.crossOutputMoverLive(pending);
    }

    private writeGeometries(
        flight: number,
        correlation: string,
        pending: WorkspacePendingFlight,
        planned: WorkspacePlanned,
        writeRefs?: ReadonlyMap<string, object> | null,
    ): boolean {
        // Slotless deferred transfers never actuate geometry: shared trees
        // are untouched by construction, so neither the mover nor any
        // survivor takes a native write even when observed frames drift
        // from retained allocations (the next ordinary reconcile reflows).
        // The reply still passed the operation, coverage, flag, and scope
        // fences before this call; tiled paths below are unchanged.
        if (planned.deferred) {
            return true;
        }
        // Stable ordering baseline from the dispatch snapshot; per-setter
        // targets and scope fences below always resolve from a fresh
        // observation unless dispatch-captured refs ride along (cross-output
        // only, tolerating the intended relocation mid-write).
        const baseline = new Map<string, WorkspaceSendRect>();
        for (const entry of pending.snapshot.sourceWindows) {
            baseline.set(entry.id, { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h });
        }
        for (const entry of pending.snapshot.targetWindows) {
            baseline.set(entry.id, { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h });
        }
        const overlays = new Set(
            [...pending.snapshot.sourceWindows, ...pending.snapshot.targetWindows]
                .filter((entry) => entry.fullscreen || entry.maximized)
                .map((entry) => entry.id),
        );
        const ordered = orderGeometryWrites(baseline, planned.geometry);
        for (let writeOrdinal = 0; writeOrdinal < ordered.length; writeOrdinal += 1) {
            // Retain the exact source+target scope and token before every
            // native setter: a window may resize, move, or close mid-write.
            // A mid-write floating toggle stops further geometry writes.
            if (!this.fencesHold(flight, correlation) || this.pending !== pending) {
                return false;
            }
            if (!this.isFlightTiled(pending)) {
                return false;
            }
            let target: object | null = null;
            const entry = ordered[writeOrdinal];
            if (entry === undefined) {
                return false;
            }
            // Retain an overlaid tile's allocation without writing its native frame.
            if (overlays.has(entry.window)) {
                continue;
            }
            if (writeRefs !== undefined && writeRefs !== null) {
                if (!this.crossOutputGeometryScopeAllows(flight, correlation, pending)) {
                    return false;
                }
                target = writeRefs.get(entry.window) ?? null;
            } else {
                const fresh = this.freshForPending(pending);
                if (fresh === null || !scopeMatchesSnapshot(fresh, pending.snapshot) || !this.flightGapsHold(pending)) {
                    return false;
                }
                if (!this.flagsStillMatch(fresh, pending.snapshot)) {
                    return false;
                }
                target = this.resolveWindowRef(fresh, entry.window);
            }
            if (target === null) {
                return false;
            }
            let written = false;
            try {
                written = this.env.setGeometry(target, entry.rect) === true;
            } catch (error) {
                void error;
                written = false;
            }
            if (!written) {
                return false;
            }
        }
        return true;
    }

    private resolveWindowRef(current: WorkspaceSendObserved, id: string): object | null {
        for (const entry of current.sourceWindows) {
            if (entry.id === id) {
                return entry.ref;
            }
        }
        for (const entry of current.targetWindows) {
            if (entry.id === id) {
                return entry.ref;
            }
        }
        return null;
    }

    private writeMoverDesktops(
        flight: number,
        correlation: string,
        pending: WorkspacePendingFlight,
    ): boolean {
        // Fresh scope fence immediately before the mover membership setter:
        // never reuse the pre-geometry observation after geometry writes.
        // A floating toggle before this setter forbids the membership write.
        if (!this.fencesHold(flight, correlation) || this.pending !== pending) {
            return false;
        }
        if (!this.isFlightTiled(pending)) {
            return false;
        }
        const fresh = this.freshForPending(pending);
        if (fresh === null || !scopeMatchesSnapshot(fresh, pending.snapshot) || !this.flightGapsHold(pending)) {
            return false;
        }
        if (fresh.targetDesktopRef === null) {
            return false;
        }
        const mover = this.resolveWindowRef(fresh, pending.moverId);
        if (mover === null) {
            return false;
        }
        let written = false;
        try {
            written = this.env.setDesktops(mover, [fresh.targetDesktopRef]) === true;
        } catch (error) {
            void error;
            written = false;
        }
        return written;
    }

    // Single terminal exit for every flight path: release both deadline
    // timers, detach the arrival signal, clear the source+target pin, log one
    // bounded redacted release line, and invoke the entry-owned settlement
    // hook exactly once for a forced complete source AND target refresh.
    // Never reports to the planner, never replays setters, never claims a
    // native commit. A stale flight token never settles.
    private settleTerminal(flight: number, correlation: string, outcome: string, event: string): void {
        if (flight !== this.activeToken) {
            return;
        }
        const pending = this.pending;
        const migrating = this.migrate;
        const route = migrating !== null ? "migrate-workspace" : "send-to-workspace";
        const revision = migrating?.baseRevision ?? pending?.baseRevision ?? 0;
        const settledCorrelation = migrating?.correlation ?? pending?.correlation ?? correlation;
        const snapshot = migrating?.snapshot ?? pending?.snapshot ?? null;
        this.clearRequestTimer();
        this.clearArrivalTimer();
        this.detachArrival();
        this.detachMigrateArrival();
        this.inFlight = false;
        this.pending = null;
        this.migrate = null;
        this.activationStep = 0;
        this.pinnedOwner = null;
        this.requestDeadline = 0;
        this.arrivalDeadline = 0;
        try {
            this.env.log(
                `${LOG_PREFIX} component=${migrating !== null ? WORKSPACE_MIGRATE_COMPONENT : WORKSPACE_SEND_COMPONENT} route=${route} stage=release correlation=${settledCorrelation} generation=${this.generation} revision=${String(revision)} diag_seq=${String(this.nextDiagSeq())} event=${sanitizeKind(event)} outcome=${sanitizeKind(outcome)}`,
            );
        } catch (error) {
            void error;
        }
        try {
            this.env.onSettled?.({
                sourceOutput: snapshot?.sourceOutput ?? "",
                sourceWorkspace: snapshot?.sourceWorkspace ?? "",
                targetOutput: snapshot?.targetOutput ?? "",
                targetWorkspace: snapshot?.targetWorkspace ?? "",
            });
        } catch (error) {
            void error;
        }
    }

    // Unanswered-request deadline: the planner callback never arrived. The
    // flight releases with no native write; a late reply arriving afterwards
    // is ignored by token. The entry refresh on settlement converges both
    // domains from native observation.
    private onRequestTimeout(flight: number, deadline: number): void {
        if (!this.inFlight || flight !== this.activeToken || deadline !== this.requestDeadline) {
            return;
        }
        // A bound plan is already actuating synchronously past the request
        // boundary; the arrival deadline owns the flight from there.
        if ((this.pending?.planned ?? this.migrate?.planned ?? null) !== null) {
            return;
        }
        const correlation = this.activeCorrelation;
        this.settleTerminal(flight, correlation, "timeout", "release");
    }

    // Bounded arrival deadline: the pin and delayed-arrival wait expire with
    // no exact membership proof. No phantom is retained; the entry refresh
    // on settlement converges both domains from native observation.
    private onArrivalTimeout(flight: number, deadline: number): void {
        if (!this.inFlight || flight !== this.activeToken || deadline !== this.arrivalDeadline) {
            return;
        }
        const correlation = this.activeCorrelation;
        this.settleTerminal(flight, correlation, "arrival-timeout", "release");
    }

    private clearRequestTimer(): void {
        const cancel = this.requestTimer;
        this.requestTimer = null;
        if (cancel !== null) {
            try {
                cancel();
            } catch (error) {
                void error;
            }
        }
    }

    private clearArrivalTimer(): void {
        const cancel = this.arrivalTimer;
        this.arrivalTimer = null;
        if (cancel !== null) {
            try {
                cancel();
            } catch (error) {
                void error;
            }
        }
    }

    // Bounded redacted late-reply line: a reply arriving after the flight
    // released is ignored and never actuates. The correlation echoes only
    // when it passes the opaque-id shape; anything else stays empty.
    private logLateReply(reply: unknown): void {
        try {
            let correlation = "";
            if (typeof reply === "string" && reply.length <= WORKSPACE_SEND_MAX_REPLY_BYTES) {
                try {
                    const parsed: unknown = JSON.parse(reply);
                    if (isRecord(parsed) && isCorrelationId(parsed["correlation_id"])) {
                        correlation = parsed["correlation_id"] as string;
                    }
                } catch (error) {
                    void error;
                }
            }
            this.env.log(
                `${LOG_PREFIX} component=${WORKSPACE_SEND_COMPONENT} route=send-to-workspace stage=request correlation=${correlation} generation=${this.generation} revision=0 diag_seq=${String(this.nextDiagSeq())} event=late-reply outcome=ignored`,
            );
        } catch (error) {
            void error;
        }
    }

    // Refusal during pre-flight (no pin, no hook): one structured route
    // diagnostic with an exact bounded token. Pre-flight refusals stay
    // enabled with no flight, timer, D-Bus, native, or follow so a
    // subsequent valid send can proceed.
    private refuse(outcome: string, correlation = ""): void {
        this.diag("request", correlation, 0, "refuse", outcome);
    }

    private diag(
        stage: string,
        correlation: string,
        revision: number,
        event: string,
        outcome: string,
    ): void {
        try {
            const followGate =
                event === "refuse"
                    ? ` follow=not-reached gate=pre-commit phase=request reason=${outcome}`
                    : event === "activate" && outcome === "no-planner"
                      ? " follow=not-reached gate=pre-commit phase=activation reason=no-planner"
                      : "";
            this.env.log(
                `${LOG_PREFIX} component=${WORKSPACE_SEND_COMPONENT} route=send-to-workspace stage=${stage} correlation=${correlation} generation=${this.generation} revision=${String(revision)} diag_seq=${String(this.nextDiagSeq())} event=${event} outcome=${outcome}${followGate}`,
            );
        } catch (error) {
            void error;
        }
    }

    // ============ R-WS-12 whole-workspace output migration ============
    //
    // Follow-only whole-workspace migration sharing this adapter's single
    // flight, correlation sequencing, activation, timers, and settlement
    // with the send paths above: the shared inFlight flag means no second
    // concurrent Engine writer ever exists. The Engine rekeys the retained
    // session at plan time, so every failure below reconciles source/target
    // actual observations through onSettled and never implies native
    // success. No native atomic promise: all setters carry readbacks, there
    // is no replay and no focus after uncertainty, and recovery is logged.

    requestMigrateWorkspace(direction: unknown): boolean {
        if (!this.enabled) {
            this.mrefuse("disabled");
            return false;
        }
        if (this.inFlight) {
            this.mdiag(
                "request",
                this.activeCorrelation,
                this.migrate?.baseRevision ?? this.pending?.baseRevision ?? 0,
                "refuse",
                "in-flight",
            );
            return false;
        }
        if (!isMigrateDirection(direction)) {
            this.mrefuse("direction-invalid");
            return false;
        }
        if (typeof this.env.observeMigrate !== "function") {
            this.mrefuse("transfer-unavailable");
            return false;
        }
        // Bounded correlation rotation, duplicated from the send paths so
        // those payloads stay byte-identical. Same session/topology: no
        // enable reset.
        if (this.seq < 0 || this.seq > WORKSPACE_SEND_MAX_SEQ) {
            const nextEpoch = this.seqEpoch + 1;
            if (!Number.isSafeInteger(nextEpoch)) {
                this.mrefuse("sequence-invalid");
                return false;
            }
            const candidate = `${this.generation}-w${String(nextEpoch)}r0`;
            if (!isCorrelationId(candidate)) {
                this.mrefuse("sequence-invalid");
                return false;
            }
            this.seqEpoch = nextEpoch;
            this.seq = 0;
            this.mdiag("request", candidate, 0, "sequence-exhausted", "correlation-rotated");
        }
        const observed = this.freshMigrate(direction);
        if (observed === null) {
            this.mrefuse("scope-invalid");
            return false;
        }
        // Workspace policy gates with exact reasons. Shared mode, a false
        // native flag, or an unreadable flag refuses; native option/config
        // writes never happen here.
        if (observed.mode !== "per-output-local" && observed.mode !== "global-unique") {
            this.mrefuse(observed.mode === "shared" ? "mode-shared" : "mode-invalid");
            return false;
        }
        if (observed.perOutput !== true) {
            this.mrefuse(observed.perOutput === false ? "per-output-disabled" : "per-output-unreadable");
            return false;
        }
        if (observed.sourceOutput === observed.targetOutput) {
            this.mrefuse("same-output");
            return false;
        }
        if (observed.sourceWorkspace !== observed.targetWorkspace) {
            this.mrefuse("workspace-mismatch");
            return false;
        }
        if (observed.targetCurrentWorkspace === observed.sourceWorkspace) {
            this.mrefuse("target-visible");
            return false;
        }
        if (observed.desktopCount > WORKSPACE_SEND_MAX_DESKTOPS) {
            this.mrefuse("desktop-cap");
            return false;
        }
        // Only an unexplained fit-excluded flag without
        // floating/sticky/fullscreen/maximized origin refuses here.
        if (migrateOverlayPresent(observed)) {
            this.mrefuse("overlay-present");
            return false;
        }
        if (
            observed.focusedId !== "" &&
            !observed.sourceWindows.some((entry) => entry.id === observed.focusedId)
        ) {
            this.mrefuse("focus-mismatch");
            return false;
        }
        // Commit-path transfer capabilities. Same-output behavior is
        // untouched; any absence refuses migration at dispatch. View reads
        // ride the phase-aware pinned observation (expectViews), so no
        // separate view-read hook exists.
        if (
            typeof this.env.resolveOutput !== "function" ||
            typeof this.env.sendClientToScreen !== "function" ||
            typeof this.env.readOutputName !== "function" ||
            typeof this.env.readDesktopIds !== "function" ||
            typeof this.env.readMoverLive !== "function" ||
            typeof this.env.commitWorkspaceMap !== "function" ||
            typeof this.env.switchMigrateView !== "function" ||
            typeof this.env.isDomainTiled !== "function"
        ) {
            this.mrefuse("transfer-unavailable");
            return false;
        }
        // Source tiling mode is frozen at dispatch and reproven before
        // every setter. Floating workspaces migrate membership-only with
        // zero tiler geometry writes; a mode change mid-flight stales.
        let sourceTiled: boolean;
        try {
            sourceTiled = this.env.isDomainTiled(observed.sourceOutput, observed.sourceWorkspace) !== false;
        } catch (error) {
            void error;
            this.mrefuse("mode-unreadable");
            return false;
        }
        // Dispatch-resolved exact target output object for the per-transfer
        // identity fence: a same-id replacement refuses mid-write.
        let targetOutputRef: object | null = null;
        try {
            targetOutputRef = this.env.resolveOutput(observed.targetOutput);
        } catch (error) {
            void error;
            targetOutputRef = null;
        }
        if (targetOutputRef === null) {
            this.mrefuse("transfer-unavailable");
            return false;
        }
        // Source/project tiling flags must agree: the observed dispatch
        // mode must equal the live adapter read, or the flight refuses
        // before any transport. Mid-flight the live read must keep
        // matching the frozen snapshot or the flight stales.
        if (sourceTiled !== observed.sourceTiled) {
            this.mrefuse("mode-mismatch");
            return false;
        }
        const correlation =
            this.seqEpoch === 0
                ? `${this.generation}-w${String(this.seq)}`
                : `${this.generation}-w${String(this.seqEpoch)}r${String(this.seq)}`;
        this.seq += 1;
        if (!isCorrelationId(correlation)) {
            this.mrefuse("correlation-invalid");
            return false;
        }
        const snapshot = snapshotOfMigrate(observed);
        const payload = this.buildMigratePayload(observed, correlation, this.innerGap, this.outerGap);
        if (payload === null) {
            this.mrefuse("payload-invalid", correlation);
            return false;
        }
        if (payload.length > WORKSPACE_SEND_MAX_REQUEST_BYTES) {
            this.mrefuse("request-over-cap", correlation);
            return false;
        }
        this.startMigrateFlight(correlation, direction, snapshot, observed, payload, targetOutputRef);
        return this.inFlight;
    }

    private freshMigrate(
        direction: WorkspaceMigrateDirection,
        pinned?: WorkspaceMigratePin,
    ): WorkspaceMigrateObserved | null {
        const hook = this.env.observeMigrate;
        if (typeof hook !== "function") {
            return null;
        }
        let observed: WorkspaceMigrateObserved | null = null;
        try {
            observed = hook(direction, pinned);
        } catch (error) {
            void error;
            observed = null;
        }
        if (!validateMigrateObserved(observed)) {
            return null;
        }
        return observed as WorkspaceMigrateObserved;
    }

    private buildMigratePayload(
        observed: WorkspaceMigrateObserved,
        correlation: string,
        innerGap: number,
        outerGap: number,
    ): string | null {
        // Stable wire request: standard v1 envelope over the SOURCE domain,
        // FULL visible source view INCLUDING sticky (sticky:true plus
        // fit_excluded:true), floats as floating:true plus fit_excluded:true,
        // the native maximized bool on every entry (especially floats,
        // where the maximized flag alone trips the overlay gate), plus the
        // adapter-asserted fixed-size provenance (fixed_auto for automatic
        // floats, fixed_suppress for tile overrides) and advisory client
        // size hints, mirroring the ordinary reconcile wire. Minimized
        // members ride native-only and are filtered from the request
        // windows; related transient/protected clients never ride the wire.
        // The target domain carries the same backing workspace on the
        // different output with its target bounds and gaps; target_windows
        // stays empty. Revision stays 0 like the normal adapters; the
        // Engine derives accepted bases internally.
        const wireWindows = observed.sourceWindows
            .filter((entry) => !entry.minimized)
            .map((entry) => ({
                window: entry.id,
                output: observed.sourceOutput,
                workspace: observed.sourceWorkspace,
                rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
                ...(entry.floating === true ? { floating: true } : {}),
                ...(entry.sticky === true ? { sticky: true } : {}),
                fullscreen: entry.fullscreen === true,
                maximized: entry.maximized === true,
                ...(entry.fitExcluded === true ? { fit_excluded: true } : {}),
                ...(entry.fixedAuto === true ? { fixed_auto: true } : {}),
                ...(entry.fixedSuppress === true ? { fixed_suppress: true } : {}),
                ...(entry.minSize === undefined || entry.minSize === null
                    ? {}
                    : { min_size: { w: entry.minSize.w, h: entry.minSize.h } }),
                ...(entry.maxSize === undefined || entry.maxSize === null
                    ? {}
                    : { max_size: { w: entry.maxSize.w, h: entry.maxSize.h } }),
            }));
        let payload = "";
        try {
            payload = JSON.stringify({
                v: WORKSPACE_SEND_CONTRACT_VERSION,
                correlation_id: correlation,
                owner: this.owner,
                generation: this.generation,
                revision: 0,
                fingerprint: this.migrateScopeFingerprint(observed),
                domain: {
                    output: observed.sourceOutput,
                    workspace: observed.sourceWorkspace,
                    bounds: {
                        x: observed.sourceBounds.x,
                        y: observed.sourceBounds.y,
                        w: observed.sourceBounds.w,
                        h: observed.sourceBounds.h,
                    },
                    gap: innerGap,
                    outer_gap: outerGap,
                },
                target_domain: {
                    output: observed.targetOutput,
                    workspace: observed.targetWorkspace,
                    bounds: {
                        x: observed.targetBounds.x,
                        y: observed.targetBounds.y,
                        w: observed.targetBounds.w,
                        h: observed.targetBounds.h,
                    },
                    gap: innerGap,
                    outer_gap: outerGap,
                },
                focused_window: observed.focusedId,
                windows: wireWindows,
                target_windows: [],
                command: { op: "migrate-workspace", direction: observed.direction },
            });
        } catch (error) {
            void error;
            return null;
        }
        return payload;
    }

    private migrateScopeFingerprint(observed: WorkspaceMigrateObserved): number {
        const sourceIds = observed.sourceWindows.map((entry) => entry.id).sort();
        const source = workspaceFingerprint(observed.sourceOutput, observed.sourceWorkspace, sourceIds);
        const target = workspaceFingerprint(observed.targetOutput, observed.targetWorkspace, []);
        return (source ^ target) >>> 0;
    }

    private startMigrateFlight(
        correlation: string,
        direction: WorkspaceMigrateDirection,
        snapshot: WorkspaceMigrateSnapshot,
        observed: WorkspaceMigrateObserved,
        payload: string,
        targetOutputRef: object,
    ): void {
        this.inFlight = true;
        this.detachArrival();
        this.detachMigrateArrival();
        this.diagSeq = 0;
        const members = observed.sourceWindows.map((entry) => ({
            id: entry.id,
            ref: entry.ref,
            sticky: entry.sticky,
            floating: entry.floating,
            minimized: entry.minimized,
            output: entry.output,
        }));
        const related = observed.relatedWindows.map((entry) => ({
            id: entry.id,
            ref: entry.ref,
            parentId: entry.parentId,
            sticky: entry.sticky,
            output: entry.output,
        }));
        const watched = observed.targetViewWindows.map((entry) => ({
            id: entry.id,
            ref: entry.ref,
            output: entry.output,
            sticky: entry.sticky,
            floating: entry.floating,
            fullscreen: entry.fullscreen,
            maximized: entry.maximized,
        }));
        const migratedDesktopRef = observed.migratedDesktopRef as object | null;
        if (migratedDesktopRef === null) {
            this.inFlight = false;
            this.mrefuse("scope-invalid");
            return;
        }
        this.migrate = {
            correlation,
            direction,
            snapshot,
            requestPayload: payload,
            innerGap: this.innerGap,
            outerGap: this.outerGap,
            members,
            related,
            watched,
            targetOutputRef,
            migratedDesktopRef,
            baseRevision: 0,
            planned: null,
            followed: false,
            followOutcome: "not-reached",
            viewsWritten: false,
            wroteAny: false,
            geomPending: [],
        };
        this.mdiag("request", correlation, 0, "dispatch", "started");
        try {
            // Dispatch-time carried-overlay counts, no raw ids.
            let fullscreen = 0;
            let maximized = 0;
            for (const entry of observed.sourceWindows) {
                if (entry.sticky) {
                    continue;
                }
                if (entry.fullscreen) {
                    fullscreen += 1;
                }
                if (entry.maximized) {
                    maximized += 1;
                }
            }
            this.env.log(
                `${LOG_PREFIX} component=${WORKSPACE_MIGRATE_COMPONENT} route=migrate-workspace stage=request correlation=${correlation} generation=${this.generation} revision=0 diag_seq=${String(this.nextDiagSeq())} event=overlays-carried outcome=started fullscreen=${String(fullscreen)} maximized=${String(maximized)}`,
            );
        } catch (error) {
            void error;
        }
        this.pinnedOwner = null;
        this.activationStep = 1;
        this.token += 1;
        const flight = this.token;
        this.activeToken = flight;
        // Arm both bounded deadlines from dispatch: the request deadline
        // releases an unanswered request, the arrival deadline bounds the
        // pin and delayed-arrival follow. Separate epochs; neither resets.
        this.deadlineToken += 1;
        this.requestDeadline = this.deadlineToken;
        const requestEpoch = this.requestDeadline;
        let requestCancel: (() => void) | null = null;
        try {
            requestCancel = this.env.scheduleOnce(WORKSPACE_SEND_TIMEOUT_MS, () =>
                this.onRequestTimeout(flight, requestEpoch),
            );
        } catch (error) {
            void error;
            requestCancel = null;
        }
        if (requestCancel === null) {
            this.inFlight = false;
            this.migrate = null;
            this.activationStep = 0;
            this.requestDeadline = 0;
            this.mrefuse("timeout");
            return;
        }
        this.requestTimer = requestCancel;
        this.deadlineToken += 1;
        this.arrivalDeadline = this.deadlineToken;
        const arrivalEpoch = this.arrivalDeadline;
        let arrivalCancel: (() => void) | null = null;
        try {
            arrivalCancel = this.env.scheduleOnce(WORKSPACE_SEND_TIMEOUT_MS, () =>
                this.onArrivalTimeout(flight, arrivalEpoch),
            );
        } catch (error) {
            void error;
            arrivalCancel = null;
        }
        if (arrivalCancel === null) {
            this.clearRequestTimer();
            this.inFlight = false;
            this.migrate = null;
            this.activationStep = 0;
            this.requestDeadline = 0;
            this.arrivalDeadline = 0;
            this.mrefuse("timeout");
            return;
        }
        this.arrivalTimer = arrivalCancel;
        // Phase 1: distinguish presence through NameHasOwner's normal boolean
        // reply. KWin does not call back for GetNameOwner's absent-name error.
        try {
            this.env.callDbus(
                WORKSPACE_SEND_DBUS_SERVICE,
                WORKSPACE_SEND_DBUS_OBJECT,
                WORKSPACE_SEND_DBUS_INTERFACE,
                WORKSPACE_SEND_HAS_OWNER_METHOD,
                WORKSPACE_SEND_SERVICE,
                (reply) => this.onNamePresence(reply, flight, correlation),
            );
        } catch (error) {
            void error;
            this.settleTerminal(flight, correlation, "no-planner", "release");
        }
    }

    // Reply-boundary actuation: re-observe the pinned triple, exact-match
    // the dispatch snapshot (frozen identity and membership), bind the
    // plan, commit the session map, then transfer members with per-setter
    // fences. Arrival, views, and the single active retain run through
    // progressMigrate only on verified placement. Any failure settles
    // terminal with normal recovery through onSettled; nothing is replayed
    // and nothing implies native success.
    private actuateMigrate(flight: number, correlation: string): void {        const migrating = this.migrate;
        const planned = migrating?.planned ?? null;
        if (migrating === null || planned === null || !this.fencesHold(flight, correlation)) {
            this.settleTerminal(flight, correlation, "stale-scope", "release");
            return;
        }
        const snapshot = migrating.snapshot;
        const fresh = this.freshMigrate(migrating.direction, migratePinOf(snapshot));
        if (fresh === null || !migrateSnapshotsEqual(snapshotOfMigrate(fresh), snapshot)) {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return;
        }
        if (!this.flightGapsHold(migrating)) {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return;
        }
        if (!this.migrateModeHolds(migrating)) {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return;
        }
        if (!migrateBindingHolds(planned, snapshot)) {
            this.settleTerminal(flight, correlation, "precondition-mismatch", "release");
            return;
        }
        // Session map commit before any native window write: insert the
        // backing id after the target current workspace. The target prior
        // workspace stays listed and hidden unchanged.
        const commitWorkspaceMap = this.env.commitWorkspaceMap;
        if (typeof commitWorkspaceMap !== "function") {
            this.settleTerminal(flight, correlation, "transfer-unavailable", "release");
            return;
        }
        let refillId: string | null = null;
        try {
            const committed = commitWorkspaceMap(snapshot.sourceOutput, snapshot.targetOutput, snapshot.sourceWorkspace);
            if (committed === null || typeof committed !== "object") {
                this.settleTerminal(flight, correlation, "map-commit-failed", "release");
                return;
            }
            refillId = committed.refillId;
        } catch (error) {
            void error;
            this.settleTerminal(flight, correlation, "map-commit-failed", "release");
            return;
        }
        migrating.refillId = refillId;
        // Arm the one-shot per-member arrival signals BEFORE native writes
        // so delayed output/membership signals between setters and the
        // post-write verification stay observable. Synchronous echo during
        // the write stack stays deferred via nativeWriteDepth and is covered
        // by the immediate post-write verification.
        this.armMigrateArrival(flight, correlation);
        this.nativeWriteDepth += 1;
        let written = false;
        try {
            written = this.writeMigrateMembers(flight, correlation, migrating, planned, fresh);
        } catch (error) {
            void error;
            written = false;
        }
        this.nativeWriteDepth -= 1;
        if (!written) {
            if (!this.fencesHold(flight, correlation) || !this.flightGapsHold(migrating)) {
                this.settleTerminal(flight, correlation, "stale-revision", "release");
                return;
            }
            this.settleTerminal(flight, correlation, migrating.writeOutcome ?? "write-failed", "release");
            return;
        }
        this.mdiag("arrival", correlation, planned.baseRevision, "write", "applied");
        if (this.progressMigrate(flight, correlation) === "waiting") {
            this.mdiag("arrival", correlation, planned.baseRevision, "arrival", "waiting");
        }
    }

    // Current tiling-mode read for a migrate flight: the live source mode
    // must still equal the frozen dispatch mode. Floating workspaces are
    // carried membership-only, never refused for their mode; a mode change
    // mid-flight stales instead. A throw reads as drift. Never throws.
    private migrateModeHolds(migrating: WorkspaceMigrateFlight): boolean {
        try {
            const gate = this.env.isDomainTiled;
            if (typeof gate !== "function") {
                return false;
            }
            return (
                gate(migrating.snapshot.sourceOutput, migrating.snapshot.sourceWorkspace) ===
                migrating.snapshot.sourceTiled
            );
        } catch (error) {
            void error;
            return false;
        }
    }

    // Frozen member + related ref liveness: every dispatch-captured ref
    // must still resolve to the identical live object with its stable
    // identity. A closed, replaced, or unreadable ref fails the next
    // setter. Never throws.
    private migrateRefsLive(migrating: WorkspaceMigrateFlight): boolean {
        const hook = this.env.readMoverLive;
        if (typeof hook !== "function") {
            return false;
        }
        for (const tracked of [...migrating.members, ...migrating.related]) {
            let live: { readonly id: string } | null = null;
            try {
                live = hook(tracked.ref);
            } catch (error) {
                void error;
                return false;
            }
            if (live === null || typeof live !== "object" || live.id !== tracked.id) {
                return false;
            }
        }
        return true;
    }

    // Full pre-setter hold for one native write: flight identity, frozen
    // gaps, frozen tiling mode, then a pinned re-observation that must keep
    // the frozen scope (member refs/windows/desktops/outputs, views,
    // policy, native flags, related identity). The optional phase-aware
    // view expectation names the exact views the flight itself already
    // wrote (post-target-switch, post-source-switch); without it the
    // pre-write views must still hold. Post-view (residueViews true), the
    // prior target view is hidden by the flight's own switch: target and
    // related members prove out through ref-based residue instead of
    // observed lists, and no new related id may appear. Desktop wrapper
    // identity is reproven too: a same-id replacement fails closed.
    // Returns the fresh observation for the setter, or null when the next
    // setter must not run. Never throws.
    private migratePreSetterHold(
        flight: number,
        correlation: string,
        migrating: WorkspaceMigrateFlight,
        expectViews?: { readonly source: string; readonly target: string },
        residueViews?: boolean,
    ): WorkspaceMigrateObserved | null {
        if (!this.fencesHold(flight, correlation) || this.migrate !== migrating) {
            return null;
        }
        if (!this.flightGapsHold(migrating) || !this.migrateModeHolds(migrating)) {
            return null;
        }
        const pin =
            expectViews === undefined
                ? migratePinOf(migrating.snapshot)
                : { ...migratePinOf(migrating.snapshot), expectViews };
        const fresh = this.freshMigrate(migrating.direction, pin);
        if (fresh === null) {
            return null;
        }
        const scopeOk =
            expectViews === undefined
                ? migrateCommitScopeHolds(fresh, migrating.snapshot)
                : migrateScopedHolds(fresh, migrating.snapshot, expectViews.source, expectViews.target, !residueViews);
        if (!scopeOk) {
            return null;
        }
        if (residueViews === true) {
            if (!this.migrateTargetResidueHolds(migrating, fresh)) {
                    return null;
            }
            if (!this.migrateRelatedPostViewHolds(migrating, fresh)) {
                    return null;
            }
        }
        if (fresh.migratedDesktopRef !== migrating.migratedDesktopRef) {
            return null;
        }
        if (!this.migrateRefsLive(migrating)) {
            return null;
        }
        return fresh;
    }

    // Post-view target residue: every dispatch target-view member still
    // resolves live with its dispatch output, flags, and (non-sticky)
    // desktop, and no window outside the dispatch target set plus the
    // migrating members may appear in the live target view. A closed,
    // moved, replaced, overlaid, or newly arrived survivor fails the next
    // setter. Never throws.
    private migrateTargetResidueHolds(
        migrating: WorkspaceMigrateFlight,
        fresh: WorkspaceMigrateObserved,
    ): boolean {
        const hook = this.env.readMoverLive;
        if (typeof hook !== "function") {
            return false;
        }
        for (const watched of migrating.watched) {
            let live: {
                readonly id: string;
                readonly floating: boolean;
                readonly sticky: boolean;
                readonly fullscreen: boolean;
                readonly maximized: boolean;
            } | null = null;
            try {
                live = hook(watched.ref);
            } catch (error) {
                void error;
                return false;
            }
            if (live === null || typeof live !== "object" || live.id !== watched.id) {
                return false;
            }
            if (
                live.floating !== watched.floating ||
                live.sticky !== watched.sticky ||
                live.fullscreen !== watched.fullscreen ||
                live.maximized !== watched.maximized
            ) {
                return false;
            }
            let output: string | null = null;
            let desktopIds: ReadonlyArray<string> | null = null;
            try {
                output = this.env.readOutputName?.(watched.ref) ?? null;
                desktopIds = this.env.readDesktopIds?.(watched.ref) ?? null;
            } catch (error) {
                void error;
                return false;
            }
            if (output === null || desktopIds === null) {
                return false;
            }
            if (output !== watched.output) {
                return false;
            }
            if (!watched.sticky) {
                if (desktopIds.length !== 1 || desktopIds[0] !== migrating.snapshot.targetCurrentWorkspace) {
                    return false;
                }
            }
        }
        // No additions: every live target-view window must belong to the
        // dispatch target set or the migrating set (which legitimately
        // joins the view through the flight's own switch).
        const allowed = new Set<string>();
        for (const entry of migrating.snapshot.targetViewWindows) {
            allowed.add(entry.id);
        }
        for (const entry of migrating.snapshot.sourceWindows) {
            allowed.add(entry.id);
        }
        for (const entry of fresh.targetViewWindows) {
            if (!allowed.has(entry.id)) {
                return false;
            }
        }
        return true;
    }

    // Post-view related proof: every dispatch related client proves out
    // through live reads (implicit-arrival output rule plus overlay
    // flags), and no related id outside the dispatch set may appear in
    // the fresh observation. Never throws.
    private migrateRelatedPostViewHolds(
        migrating: WorkspaceMigrateFlight,
        fresh: WorkspaceMigrateObserved,
    ): boolean {
        const hook = this.env.readMoverLive;
        if (typeof hook !== "function") {
            return false;
        }
        const parentOutput = new Map<string, string>();
        for (const member of migrating.members) {
            if (member.sticky) {
                continue;
            }
            let output: string | null = null;
            try {
                output = this.env.readOutputName?.(member.ref) ?? null;
            } catch (error) {
                void error;
                return false;
            }
            if (output === null) {
                return false;
            }
            parentOutput.set(member.id, output);
        }
        const dispatchIds = new Set(migrating.related.map((entry) => entry.id));
        for (const related of migrating.related) {
            let live: {
                readonly id: string;
                readonly floating: boolean;
                readonly sticky: boolean;
                readonly fullscreen: boolean;
                readonly maximized: boolean;
            } | null = null;
            try {
                live = hook(related.ref);
            } catch (error) {
                void error;
                return false;
            }
            if (live === null || typeof live !== "object" || live.id !== related.id) {
                return false;
            }
            let output: string | null = null;
            try {
                output = this.env.readOutputName?.(related.ref) ?? null;
            } catch (error) {
                void error;
                return false;
            }
            if (output === null) {
                return false;
            }
            if (related.parentId === null || related.sticky) {
                if (output !== related.output) {
                    return false;
                }
            } else {
                const parent = parentOutput.get(related.parentId);
                if (parent === undefined || output !== parent) {
                    return false;
                }
            }
        }
        for (const entry of fresh.relatedWindows) {
            if (!dispatchIds.has(entry.id)) {
                return false;
            }
        }
        return true;
    }

    // Mid-write abort outcome: the first abort wins; later calls keep it.
    private failMigrateWrite(migrating: WorkspaceMigrateFlight, outcome?: string): false {
        if (migrating.writeOutcome === undefined) {
            migrating.writeOutcome = outcome ?? (migrating.wroteAny ? "write-failed" : "stale-revision");
        }
        return false;
    }

    // Phase A: per-member output transfer plus planned target geometry.
    // Sticky windows are never touched. Tiled members move in planned
    // geometry order, then floats, then minimized members (transfer only).
    // Overlay members carry via native output remap only: no size/position
    // writes, and no focus writes while fullscreen. A transfer that
    // initiates while placement is still pending defers that member's
    // geometry until placement verifies; only a refused transfer aborts.
    //
    // Before EACH transfer the exact target object is re-resolved and
    // compared to the dispatch identity, and the full pre-setter hold
    // reproves member refs, scopes, views, policy, flags, gaps, and tiling
    // mode. Floating workspaces carry membership-only.
    private writeMigrateMembers(
        flight: number,
        correlation: string,
        migrating: WorkspaceMigrateFlight,
        planned: WorkspaceMigratePlanned,
        fresh: WorkspaceMigrateObserved,
    ): boolean {
        void fresh;
        const resolveOutput = this.env.resolveOutput;
        const sendClientToScreen = this.env.sendClientToScreen;
        if (typeof resolveOutput !== "function" || typeof sendClientToScreen !== "function") {
            return this.failMigrateWrite(migrating);
        }
        const memberById = new Map<string, { readonly ref: object }>();
        for (const member of migrating.members) {
            if (!memberById.has(member.id)) {
                memberById.set(member.id, { ref: member.ref });
            }
        }
        const geometryOrder = planned.geometry.map((entry) => entry.window);
        const floatIds = migrating.members
            .filter((member) => !member.sticky && !member.minimized && !geometryOrder.includes(member.id))
            .map((member) => member.id)
            .sort();
        const minimizedIds = migrating.members
            .filter((member) => !member.sticky && member.minimized)
            .map((member) => member.id)
            .sort();
        const ordered = [...geometryOrder, ...floatIds, ...minimizedIds];
        const geometryByWindow = new Map<string, WorkspaceSendRect>();
        for (const entry of planned.geometry) {
            geometryByWindow.set(entry.window, { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h });
        }
        // Exact migrating set: every non-sticky dispatch member moves, no
        // skipping windows and no outside-window effect.
        const expected = migrating.members.filter((member) => !member.sticky).map((member) => member.id).sort();
        const wanted = [...ordered].sort();
        if (expected.length !== wanted.length || expected.some((id, index) => id !== wanted[index])) {
            return this.failMigrateWrite(migrating);
        }
        const geometryAllowed = migrating.snapshot.sourceTiled;
        for (const id of ordered) {
            const hold = this.migratePreSetterHold(flight, correlation, migrating);
            if (hold === null) {
                return this.failMigrateWrite(migrating);
            }
            void hold;
            // Exact target identity per transfer: re-resolve and compare to
            // the dispatch object before touching native state.
            let targetOutputRef: object | null = null;
            try {
                targetOutputRef = resolveOutput(migrating.snapshot.targetOutput);
            } catch (error) {
                void error;
                targetOutputRef = null;
            }
            if (targetOutputRef === null || targetOutputRef !== migrating.targetOutputRef) {
                return this.failMigrateWrite(migrating);
            }
            const member = memberById.get(id) ?? null;
            if (member === null) {
                return this.failMigrateWrite(migrating);
            }
            const ref = member.ref;
            // Liveness guard before the first setter: the ref must still
            // resolve to the same live id. Overlays never refuse here.
            if (!this.migrateMemberLive(ref, id)) {
                return this.failMigrateWrite(migrating, migrating.wroteAny ? "write-failed" : "stale-revision");
            }
            let transferred = false;
            try {
                transferred = sendClientToScreen(ref, targetOutputRef) === true;
            } catch (error) {
                void error;
                transferred = false;
            }
            if (!transferred) {
                return this.failMigrateWrite(migrating);
            }
            migrating.wroteAny = true;
            // A member that died or was replaced by its own transfer aborts
            // the whole operation at once: no further writes, no views, no
            // focus.
            if (!this.migrateMemberLive(ref, id)) {
                return this.failMigrateWrite(migrating, "write-failed");
            }
            // Initially overlaid members stay native-only even if flags
            // later read normal.
            const snapshotOverlay = migrating.snapshot.sourceWindows.some(
                (entry) => entry.id === id && (entry.fullscreen || entry.maximized),
            );
            if (snapshotOverlay) {
                continue;
            }
            const rect = geometryByWindow.get(id);
            if (rect === undefined || !geometryAllowed) {
                // Floats, minimized members, and every member of a floating
                // workspace carry membership-only: never deferred, never
                // written.
                continue;
            }
            // Delayed output application defers this member's geometry
            // until placement verifies; a placed member writes at once.
            if (!this.migrateMemberPlaced(ref, migrating.snapshot.targetOutput, migrating.snapshot.sourceWorkspace)) {
                if (!migrating.geomPending.includes(id)) {
                    migrating.geomPending.push(id);
                }
                continue;
            }
            // Immediate live overlay read before the write: a member that
            // flipped since the hold skips geometry instead of refusing.
            const writeLive = this.migrateLiveOverlay(ref, id);
            if (writeLive === null) {
                return this.failMigrateWrite(migrating, "write-failed");
            }
            if (writeLive.fullscreen || writeLive.maximized) {
                continue;
            }
            let written = false;
            try {
                written = this.env.setGeometry(ref, rect) === true;
            } catch (error) {
                void error;
                written = false;
            }
            if (!written) {
                return this.failMigrateWrite(migrating);
            }
        }
        return true;
    }

    // Read-only live member evidence for mid-write fences: the retained ref
    // must still resolve to the identical live object with its readable
    // native identity. Null on anything unreadable, unlisted, or replaced.
    private migrateMemberLive(ref: object, id: string): boolean {
        return this.migrateLiveOverlay(ref, id) !== null;
    }

    // Immediate live overlay flags for one member: null on unreadable or
    // replaced identity (the caller fails the write); otherwise the live
    // flags. Read immediately before each geometry/focus setter so a member
    // that flipped since the hold skips the write instead of refusing.
    private migrateLiveOverlay(ref: object, id: string): { fullscreen: boolean; maximized: boolean } | null {
        const hook = this.env.readMoverLive;
        if (typeof hook !== "function") {
            return null;
        }
        let live: {
            readonly id: string;
            readonly floating: boolean;
            readonly sticky: boolean;
            readonly fullscreen: boolean;
            readonly maximized: boolean;
        } | null = null;
        try {
            live = hook(ref);
        } catch (error) {
            void error;
            return null;
        }
        if (live === null || typeof live !== "object" || live.id !== id) {
            return null;
        }
        return { fullscreen: live.fullscreen === true, maximized: live.maximized === true };
    }

    // Exact native placement read for one member ref: destination output
    // plus the migrated workspace as the sole desktop.
    private migrateMemberPlaced(ref: object, targetOutput: string, workspaceId: string): boolean {
        let output: string | null = null;
        let desktopIds: ReadonlyArray<string> | null = null;
        try {
            output = this.env.readOutputName?.(ref) ?? null;
            desktopIds = this.env.readDesktopIds?.(ref) ?? null;
        } catch (error) {
            void error;
            return false;
        }
        return output === targetOutput && desktopIds !== null && desktopIds.length === 1 && desktopIds[0] === workspaceId;
    }

    // Deferred planned geometry for transfers that initiated while
    // placement was still pending. Runs once placement verifies, before
    // any view write, with a full pre-setter hold plus a live re-guard per
    // member. Overlays never defer; a member that flipped since the hold
    // skips its write. Any failure aborts with no further writes and no
    // focus. Never runs on floating workspaces.
    private writeMigratePendingGeometries(
        flight: number,
        correlation: string,
        migrating: WorkspaceMigrateFlight,
        planned: WorkspaceMigratePlanned,
    ): boolean {
        if (!migrating.snapshot.sourceTiled) {
            return true;
        }
        const refById = new Map<string, object>();
        for (const member of migrating.members) {
            if (!refById.has(member.id)) {
                refById.set(member.id, member.ref);
            }
        }
        const geometryByWindow = new Map<string, WorkspaceSendRect>();
        for (const entry of planned.geometry) {
            geometryByWindow.set(entry.window, { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h });
        }
        const pending = migrating.geomPending.splice(0, migrating.geomPending.length);
        for (const id of pending) {
            if (this.migratePreSetterHold(flight, correlation, migrating) === null) {
                return this.failMigrateWrite(migrating);
            }
            const ref = refById.get(id) ?? null;
            const rect = geometryByWindow.get(id);
            if (ref === null || rect === undefined) {
                return this.failMigrateWrite(migrating);
            }
            if (!this.migrateMemberPlaced(ref, migrating.snapshot.targetOutput, migrating.snapshot.sourceWorkspace)) {
                return this.failMigrateWrite(migrating);
            }
            // Immediate live overlay read before the write: a member that
            // flipped since the hold skips geometry instead of refusing.
            // Initially overlaid members never reach this queue.
            const writeLive = this.migrateLiveOverlay(ref, id);
            if (writeLive === null) {
                return this.failMigrateWrite(migrating, "write-failed");
            }
            if (writeLive.fullscreen || writeLive.maximized) {
                continue;
            }
            let written = false;
            try {
                written = this.env.setGeometry(ref, rect) === true;
            } catch (error) {
                void error;
                written = false;
            }
            if (!written) {
                return this.failMigrateWrite(migrating);
            }
        }
        return true;
    }

    // Phases B..F: verified placement, deferred planned geometry,
    // per-output view writes, output activation for null-active routes, and
    // the single active retain. Returns "waiting" only when every setter
    // applied but some members have not arrived yet with none closed: the
    // armed one-shot signals or the bounded arrival deadline own the flight
    // from there. Every other path settles terminal exactly once.
    private progressMigrate(flight: number, correlation: string): "settled" | "waiting" {
        const migrating = this.migrate;
        const planned = migrating?.planned ?? null;
        if (migrating === null || planned === null || !this.fencesHold(flight, correlation) || this.migrate !== migrating) {
            this.settleTerminal(flight, correlation, "stale-scope", "release");
            return "settled";
        }
        // Pre-view full hold: frozen scope, policy, tiling mode, gaps,
        // desktop identity, and every frozen ref reproven.
        if (this.migratePreSetterHold(flight, correlation, migrating) === null) {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return "settled";
        }
        // Verified arrival: every migrating window reads back the exact
        // native output AND the migrated workspace as its sole desktop
        // (minimized members included); sticky windows read back unchanged;
        // related transients follow their live parent output. A member in
        // neither scope is closed: settle at once, never wait for it.
        const placement = this.readMigratePlacement(migrating);
        if (placement === "unreadable") {
            this.settleTerminal(flight, correlation, "stale-revision", "release");
            return "settled";
        }
        if (placement === "closed") {
            this.settleTerminal(flight, correlation, "member-closed", "release");
            return "settled";
        }
        if (placement === "pending") {
            return "waiting";
        }
        // Deferred planned geometry lands now that placement verifies,
        // before any view write. Any failure aborts with no views and no
        // focus; the settled refresh recovers both domains.
        if (migrating.geomPending.length > 0) {
            if (!this.writeMigratePendingGeometries(flight, correlation, migrating, planned)) {
                this.settleTerminal(flight, correlation, migrating.writeOutcome ?? "write-failed", "release");
                return "settled";
            }
        }
        // Views run at most once. A sole-scoped source (null refill)
        // settles source-empty without focus: never claim arrived while
        // the source still shows the moved id.
        const views = this.writeMigrateViews(flight, correlation, migrating);
        if (views === false) {
            if (!this.fencesHold(flight, correlation) || this.migrate !== migrating) {
                this.settleTerminal(flight, correlation, "stale-revision", "release");
                return "settled";
            }
            this.settleTerminal(flight, correlation, "switch-unconfirmed", "release");
            return "settled";
        }
        if (views === "partial") {
            this.settleTerminal(flight, correlation, "source-empty", "release");
            return "settled";
        }
        if (planned.activeWindow === null) {
            // Empty/sticky-active routes activate the target output
            // natively, then settle with zero focus setters ever.
            if (!this.switchMigrateActiveOutput(flight, correlation, migrating)) {
                this.settleTerminal(flight, correlation, "output-unconfirmed", "release");
                return "settled";
            }
            migrating.followed = true;
            migrating.followOutcome = "state-confirmed";
            this.mdiag("follow", migrating.correlation, planned.baseRevision, "follow", migrating.followOutcome);
            this.settleTerminal(flight, correlation, "arrived", "arrival");
            return "settled";
        }
        this.finishMigrateFocus(flight, correlation, migrating, planned);
        const outcome = migrating.followOutcome;
        // Member/view arrival is verified; native-only follow (no tiler
        // focus write) still counts as arrived.
        this.settleTerminal(
            flight,
            correlation,
            outcome === "state-confirmed" || outcome === "native-only" ? "arrived" : outcome,
            "arrival",
        );
        return "settled";
    }

    // Per-member placement rollup over dispatch-captured refs. "complete"
    // only when every non-sticky member (minimized included) proves
    // destination output plus sole migrated desktop, every sticky member
    // proves unchanged, and every related transient proves the live output
    // of its migrating parent (implicit arrival, never an explicit
    // setter). "pending" when some members still read back on the source
    // with none closed. "closed" when any member is gone, replaced, or
    // unreadable. "unreadable" when a scope read itself fails.
    private readMigratePlacement(migrating: WorkspaceMigrateFlight): "complete" | "pending" | "closed" | "unreadable" {
        const snapshot = migrating.snapshot;
        const outputOf = (ref: object): string | null => {
            try {
                return this.env.readOutputName?.(ref) ?? null;
            } catch (error) {
                void error;
                return null;
            }
        };
        let pending = false;
        for (const member of migrating.members) {
            let live: {
                readonly id: string;
                readonly floating: boolean;
                readonly sticky: boolean;
                readonly fullscreen: boolean;
                readonly maximized: boolean;
            } | null = null;
            try {
                live = this.env.readMoverLive?.(member.ref) ?? null;
            } catch (error) {
                void error;
                return "closed";
            }
            if (live === null || typeof live !== "object" || live.id !== member.id) {
                return "closed";
            }
            if (member.sticky) {
                if (live.sticky !== true) {
                    return "closed";
                }
                const output = outputOf(member.ref);
                if (output === null) {
                    return "unreadable";
                }
                if (output !== member.output) {
                    return "closed";
                }
                continue;
            }
            let output: string | null = null;
            let desktopIds: ReadonlyArray<string> | null = null;
            try {
                output = this.env.readOutputName?.(member.ref) ?? null;
                desktopIds = this.env.readDesktopIds?.(member.ref) ?? null;
            } catch (error) {
                void error;
                return "closed";
            }
            if (output === null || desktopIds === null) {
                return "unreadable";
            }
            const homed =
                desktopIds.length === 1 && desktopIds[0] === snapshot.sourceWorkspace;
            if (!homed) {
                return "closed";
            }
            if (output === snapshot.targetOutput) {
                continue;
            }
            if (output === snapshot.sourceOutput) {
                pending = true;
                continue;
            }
            return "closed";
        }
        const parentOutput = new Map<string, string>();
        for (const member of migrating.members) {
            if (member.sticky) {
                continue;
            }
            const output = outputOf(member.ref);
            if (output === null) {
                return "unreadable";
            }
            parentOutput.set(member.id, output);
        }
        for (const related of migrating.related) {
            let live: { readonly id: string } | null = null;
            try {
                live = this.env.readMoverLive?.(related.ref) ?? null;
            } catch (error) {
                void error;
                return "closed";
            }
            if (live === null || typeof live !== "object" || live.id !== related.id) {
                return "closed";
            }
            const output = outputOf(related.ref);
            if (output === null) {
                return "unreadable";
            }
            if (related.parentId === null) {
                // View-local protected client: never moves.
                if (output !== related.output) {
                    return "closed";
                }
                continue;
            }
            if (related.sticky) {
                // Sticky transients stay even when their parent moves.
                if (output !== related.output) {
                    return "closed";
                }
                continue;
            }
            const parent = parentOutput.get(related.parentId);
            if (parent === undefined) {
                return "closed";
            }
            if (output === parent) {
                continue;
            }
            pending = true;
        }
        return pending ? "pending" : "complete";
    }

    // Phase C: target output shows the migrated workspace, source output
    // shows the frozen refill (last remaining scoped workspace). Each
    // switch carries an immediate readback inside the hook plus a
    // phase-aware hold: after the target switch the source must still show
    // the source workspace; after the source switch both views must read
    // back exactly. A failed, ambiguous, or drifted switch never focuses
    // and never replays. Runs at most once. A sole-scoped source (null
    // refill) reports "partial": the target view stands as written, but
    // the flight settles source-empty without focus and never claims
    // arrived while the source still shows the moved id.
    private writeMigrateViews(
        flight: number,
        correlation: string,
        migrating: WorkspaceMigrateFlight,
    ): "done" | "partial" | false {
        if (migrating.viewsWritten) {
            return "done";
        }
        const switchView = this.env.switchMigrateView;
        if (typeof switchView !== "function") {
            return false;
        }
        const snapshot = migrating.snapshot;
        if (this.migratePreSetterHold(flight, correlation, migrating) === null) {
            return false;
        }
        let targetOk = false;
        try {
            targetOk = switchView(snapshot.sourceWorkspace, snapshot.targetOutput) === true;
        } catch (error) {
            void error;
            targetOk = false;
        }
        if (!targetOk) {
            return false;
        }
        // Prove the first own write plus the remaining view unchanged
        // before the second write: target shows migrated, source still
        // shows the source workspace.
        if (
            this.migratePreSetterHold(
                flight,
                correlation,
                migrating,
                {
                    source: snapshot.sourceWorkspace,
                    target: snapshot.sourceWorkspace,
                },
                true,
            ) === null
        ) {
            return false;
        }
        const refillId = migrating.refillId ?? null;
        if (refillId === null) {
            // Sole-scoped source: no refill exists. The target view stands
            // as written, but the flight reports partial: no source write,
            // no focus, never arrived while the source still shows the
            // moved id. Recovery reconciles both domains.
            migrating.viewsWritten = true;
            this.mdiag("arrival", migrating.correlation, migrating.baseRevision, "view", "source-empty");
            return "partial";
        }
        if (
            this.migratePreSetterHold(
                flight,
                correlation,
                migrating,
                {
                    source: snapshot.sourceWorkspace,
                    target: snapshot.sourceWorkspace,
                },
                true,
            ) === null
        ) {
            return false;
        }
        let sourceOk = false;
        try {
            sourceOk = switchView(refillId, snapshot.sourceOutput) === true;
        } catch (error) {
            void error;
            sourceOk = false;
        }
        if (!sourceOk) {
            return false;
        }
        // After the source write reprove both views exactly: target shows
        // migrated, source shows the frozen refill.
        if (
            this.migratePreSetterHold(
                flight,
                correlation,
                migrating,
                {
                    source: refillId,
                    target: snapshot.sourceWorkspace,
                },
                true,
            ) === null
        ) {
            return false;
        }
        migrating.viewsWritten = true;
        return "done";
    }

    // Phase D2 (null-active routes only): native output activation for
    // empty/sticky-active migrations. The desktop view setter only takes
    // visible effect on the active output, so when the target is not
    // already active the flight invokes the native directional screen
    // slot matching the migration direction (KWin useractions
    // switchToOutput/setActiveOutput). Pre-slot the source must read back
    // active (never switch from a wrong source); post-slot the target
    // must read back active. A missing slot, a wrong source, or a wrong
    // readback settles output-unconfirmed with no retry. Zero focus
    // setters ever ride along: KWin may choose a native client itself,
    // which is accepted and never our fabricated focus. Runs at most once
    // per flight (guarded by followed).
    private switchMigrateActiveOutput(
        flight: number,
        correlation: string,
        migrating: WorkspaceMigrateFlight,
    ): boolean {
        const readActive = this.env.readActiveOutput;
        const switchSlot = this.env.switchActiveOutput;
        if (typeof readActive !== "function" || typeof switchSlot !== "function") {
            return false;
        }
        const snapshot = migrating.snapshot;
        let before: string | null = null;
        try {
            before = readActive();
        } catch (error) {
            void error;
            return false;
        }
        if (before === null) {
            return false;
        }
        if (before === snapshot.targetOutput) {
            return true;
        }
        if (before !== snapshot.sourceOutput) {
            return false;
        }
        if (!this.fencesHold(flight, correlation) || this.migrate !== migrating) {
            return false;
        }
        let invoked = false;
        try {
            invoked = switchSlot(migrating.direction) === true;
        } catch (error) {
            void error;
            invoked = false;
        }
        if (!invoked) {
            return false;
        }
        let after: string | null = null;
        try {
            after = readActive();
        } catch (error) {
            void error;
            return false;
        }
        return after === snapshot.targetOutput;
    }

    // Phase D: retain the migrated active client (tiled or float) after
    // VERIFIED arrival and views. Empty, absent, and sticky-active routes
    // carry a null active window and never reach here (output activation
    // runs instead). A fullscreen active takes no tiler focus write; follow
    // records native-only. Runs at most once.
    private finishMigrateFocus(
        flight: number,
        correlation: string,
        migrating: WorkspaceMigrateFlight,
        planned: WorkspaceMigratePlanned,
    ): void {
        if (migrating.followed) {
            return;
        }
        migrating.followed = true;
        if (planned.activeWindow === null) {
            migrating.followOutcome = "state-confirmed";
            this.mdiag("follow", migrating.correlation, planned.baseRevision, "follow", migrating.followOutcome);
            return;
        }
        const focusWindow = this.env.focusWindow;
        if (typeof focusWindow !== "function") {
            migrating.followOutcome = "hooks-unavailable";
            this.mdiag("follow", migrating.correlation, planned.baseRevision, "follow", migrating.followOutcome);
            return;
        }
        // Phase-aware post-view proof immediately before focus: both views
        // read back the flight's own writes, every frozen ref reproves,
        // and the active member still reads back placed.
        const refillId = migrating.refillId ?? null;
        if (refillId === null) {
            migrating.followOutcome = "arrival-unconfirmed";
            this.mdiag("follow", migrating.correlation, planned.baseRevision, "follow", migrating.followOutcome);
            return;
        }
        if (
            this.migratePreSetterHold(
                flight,
                correlation,
                migrating,
                {
                    source: refillId,
                    target: migrating.snapshot.sourceWorkspace,
                },
                true,
            ) === null
        ) {
            migrating.followOutcome = "arrival-unconfirmed";
            this.mdiag("follow", migrating.correlation, planned.baseRevision, "follow", migrating.followOutcome);
            return;
        }
        // Fresh arrival proof immediately before focus: the active member
        // must still read back placed, with fences held.
        const active = migrating.members.find((member) => member.id === planned.activeWindow) ?? null;
        if (
            active === null ||
            active.sticky ||
            active.minimized ||
            !this.fencesHold(flight, correlation) ||
            this.migrate !== migrating ||
            !this.migrateMemberPlaced(active.ref, migrating.snapshot.targetOutput, migrating.snapshot.sourceWorkspace)
        ) {
            migrating.followOutcome = "arrival-unconfirmed";
            this.mdiag("follow", migrating.correlation, planned.baseRevision, "follow", migrating.followOutcome);
            return;
        }
        const snapshotFullscreen = migrating.snapshot.sourceWindows.some(
            (entry) => entry.id === planned.activeWindow && entry.fullscreen,
        );
        // Immediate live read before focus: a member that turned fullscreen
        // since the hold takes no tiler focus write. Initially fullscreen
        // actives never reach focus either.
        const focusLive = this.migrateLiveOverlay(active.ref, active.id);
        if (focusLive === null) {
            migrating.followOutcome = "arrival-unconfirmed";
            this.mdiag("follow", migrating.correlation, planned.baseRevision, "follow", migrating.followOutcome);
            return;
        }
        if (snapshotFullscreen || focusLive.fullscreen) {
            migrating.followOutcome = "native-only";
            this.mdiag("follow", migrating.correlation, planned.baseRevision, "follow", migrating.followOutcome);
            return;
        }
        let focused = false;
        this.nativeFollowDepth += 1;
        try {
            if (this.fencesHold(flight, correlation) && this.migrate === migrating) {
                focused =
                    focusWindow(active.ref, {
                        correlation: migrating.correlation,
                        revision: planned.baseRevision,
                        nextSequence: () => this.nextDiagSeq(),
                    }) === true;
            }
        } catch (error) {
            void error;
            focused = false;
        }
        this.nativeFollowDepth -= 1;
        migrating.followOutcome = focused ? "state-confirmed" : "focus-unconfirmed";
        this.mdiag("follow", migrating.correlation, planned.baseRevision, "follow", migrating.followOutcome);
    }

    // One-shot delayed-arrival triggers: the next per-member (or related
    // transient) desktopsChanged or outputChanged signal re-verifies and
    // finishes once on a fresh exact proof. Armed before native writes so
    // a signal between setters and the post-write verification is not
    // missed. Synchronous echo during the write stack stays deferred via
    // nativeWriteDepth. Sticky entries never move and take no signal.
    private armMigrateArrival(flight: number, correlation: string): void {
        const migrating = this.migrate;
        if (migrating === null || migrating.correlation !== correlation || this.migrateDetaches.length > 0) {
            return;
        }
        const subscribeDesktops = this.env.subscribeMoverDesktops;
        const subscribeOutput = this.env.subscribeMoverOutput;
        if (typeof subscribeDesktops !== "function" && typeof subscribeOutput !== "function") {
            return;
        }
        const tracked = [
            ...migrating.members.filter((member) => !member.sticky).map((member) => member.ref),
            ...migrating.related.filter((related) => !related.sticky).map((related) => related.ref),
        ];
        const seen = new Set<object>();
        for (const ref of tracked) {
            if (seen.has(ref)) {
                continue;
            }
            seen.add(ref);
            if (typeof subscribeDesktops === "function") {
                try {
                    const detach = subscribeDesktops(ref, () => this.onMigrateArrivalSignal(flight, correlation));
                    if (typeof detach === "function") {
                        this.migrateDetaches.push(detach);
                    }
                } catch (error) {
                    void error;
                }
            }
            if (typeof subscribeOutput === "function") {
                try {
                    const detach = subscribeOutput(ref, () => this.onMigrateArrivalSignal(flight, correlation));
                    if (typeof detach === "function") {
                        this.migrateDetaches.push(detach);
                    }
                } catch (error) {
                    void error;
                }
            }
        }
    }

    private detachMigrateArrival(): void {
        const detaches = this.migrateDetaches.splice(0, this.migrateDetaches.length);
        for (const detach of detaches) {
            try {
                detach();
            } catch (error) {
                void error;
            }
        }
    }

    private onMigrateArrivalSignal(flight: number, correlation: string): void {
        // A signal arriving mid-write or mid-follow is covered by the
        // immediate post-write verification / in-progress follow instead.
        // Keep the one-shots armed so the delayed arrival stays observable;
        // do not consume them while the write/follow stack is live.
        if (this.nativeWriteDepth > 0 || this.nativeFollowDepth > 0) {
            return;
        }
        if (!this.inFlight || flight !== this.activeToken || this.migrate === null) {
            return;
        }
        // One-shot: detach before any further progress so a duplicate signal
        // cannot produce a second follow.
        this.detachMigrateArrival();
        if (this.progressMigrate(flight, correlation) === "waiting" && this.migrateDetaches.length === 0) {
            this.armMigrateArrival(flight, correlation);
        }
    }

    // Refusal during pre-flight (no pin, no hook): one structured migrate
    // diagnostic with an exact bounded token. Pre-flight refusals stay
    // enabled with no flight, timer, D-Bus, native, or follow so a
    // subsequent valid migration can proceed.
    private mrefuse(outcome: string, correlation = ""): void {
        this.mdiag("request", correlation, 0, "refuse", outcome);
    }

    private mdiag(stage: string, correlation: string, revision: number, event: string, outcome: string): void {
        try {
            const followGate =
                event === "refuse"
                    ? ` follow=not-reached gate=pre-commit phase=request reason=${outcome}`
                    : "";
            this.env.log(
                `${LOG_PREFIX} component=${WORKSPACE_MIGRATE_COMPONENT} route=migrate-workspace stage=${stage} correlation=${correlation} generation=${this.generation} revision=${String(revision)} diag_seq=${String(this.nextDiagSeq())} event=${event} outcome=${outcome}${followGate}`,
            );
        } catch (error) {
            void error;
        }
    }

    private nextDiagSeq(): number {
        this.diagSeq += 1;
        return this.diagSeq;
    }
}

// ============ R-WS-12 migrate module helpers (no native access) ============

function migratePinOf(snapshot: WorkspaceMigrateSnapshot): WorkspaceMigratePin {
    return {
        sourceOutput: snapshot.sourceOutput,
        sourceWorkspace: snapshot.sourceWorkspace,
        targetOutput: snapshot.targetOutput,
    };
}

function isMigrateSize(value: unknown): value is { readonly w: number; readonly h: number } {
    if (typeof value !== "object" || value === null) {
        return false;
    }
    const record = value as Record<string, unknown>;
    const wRaw = record["w"] !== undefined ? record["w"] : record["width"];
    const hRaw = record["h"] !== undefined ? record["h"] : record["height"];
    return (
        typeof wRaw === "number" &&
        Number.isInteger(wRaw) &&
        typeof hRaw === "number" &&
        Number.isInteger(hRaw) &&
        wRaw >= 0 &&
        hRaw >= 0 &&
        wRaw <= 16384 &&
        hRaw <= 16384
    );
}

function validateMigrateObservedWindow(value: unknown): value is WorkspaceMigrateObservedWindow {
    if (typeof value !== "object" || value === null) {
        return false;
    }
    const candidate = value as Record<string, unknown>;
    if (!isOpaqueId(candidate["id"]) || typeof candidate["ref"] !== "object" || candidate["ref"] === null) {
        return false;
    }
    const rect = candidate["rect"];
    if (typeof rect !== "object" || rect === null) {
        return false;
    }
    const record = rect as Record<string, unknown>;
    if (
        !isTargetRect({ x: record["x"], y: record["y"], w: record["w"], h: record["h"] }) ||
        !isOpaqueId(candidate["output"])
    ) {
        return false;
    }
    for (const flag of ["floating", "sticky", "fullscreen", "maximized", "fitExcluded", "minimized", "transient", "fixedAuto", "fixedSuppress"]) {
        if (typeof candidate[flag] !== "boolean") {
            return false;
        }
    }
    for (const hint of ["minSize", "maxSize"]) {
        const raw = candidate[hint];
        if (raw !== undefined && raw !== null && !isMigrateSize(raw)) {
            return false;
        }
    }
    return true;
}

function validateMigrateRelatedWindow(value: unknown): value is WorkspaceMigrateRelatedWindow {
    if (typeof value !== "object" || value === null) {
        return false;
    }
    const candidate = value as Record<string, unknown>;
    if (!isOpaqueId(candidate["id"]) || typeof candidate["ref"] !== "object" || candidate["ref"] === null) {
        return false;
    }
    if (!isOpaqueId(candidate["output"])) {
        return false;
    }
    const parentId = candidate["parentId"];
    if (parentId !== null && !isOpaqueId(parentId)) {
        return false;
    }
    for (const flag of ["transient", "minimized", "floating", "sticky", "fullscreen", "maximized", "fitExcluded"]) {
        if (typeof candidate[flag] !== "boolean") {
            return false;
        }
    }
    return true;
}

function validateMigrateObserved(value: WorkspaceMigrateObserved | null): value is WorkspaceMigrateObserved {
    if (value === null || typeof value !== "object") {
        return false;
    }
    if (!isMigrateDirection(value.direction)) {
        return false;
    }
    if (!isOpaqueId(value.sourceOutput) || !isOpaqueId(value.sourceWorkspace)) {
        return false;
    }
    if (!isOpaqueId(value.targetOutput) || !isOpaqueId(value.targetWorkspace)) {
        return false;
    }
    if (!isOpaqueId(value.targetCurrentWorkspace) || !isOpaqueId(value.sourceCurrentWorkspace)) {
        return false;
    }
    if (
        !isTargetRect({ x: value.sourceBounds.x, y: value.sourceBounds.y, w: value.sourceBounds.w, h: value.sourceBounds.h }) ||
        !isTargetRect({ x: value.targetBounds.x, y: value.targetBounds.y, w: value.targetBounds.w, h: value.targetBounds.h })
    ) {
        return false;
    }
    if (value.focusedId !== "" && !isOpaqueId(value.focusedId)) {
        return false;
    }
    // A named focus outside the source view is a dispatch-time
    // focus-mismatch, not a shape violation: it survives validation so the
    // request path refuses with the exact reason.
    if (!Array.isArray(value.sourceWindows) || !Array.isArray(value.targetViewWindows)) {
        return false;
    }
    if (!Array.isArray(value.relatedWindows)) {
        return false;
    }
    const seen = new Set<string>();
    for (const entry of [...value.sourceWindows, ...value.targetViewWindows]) {
        if (!validateMigrateObservedWindow(entry)) {
            return false;
        }
        const candidate = entry as WorkspaceMigrateObservedWindow;
        if (candidate.output !== value.sourceOutput && candidate.output !== value.targetOutput) {
            return false;
        }
        if (seen.has(candidate.id)) {
            return false;
        }
        seen.add(candidate.id);
    }
    // Related clients ride no wire but join the frozen identity: no id may
    // repeat across any list, and a transient parent must name a carried
    // migrating member.
    const memberIds = new Set(value.sourceWindows.map((entry) => (entry as WorkspaceMigrateObservedWindow).id));
    for (const entry of value.relatedWindows) {
        if (!validateMigrateRelatedWindow(entry)) {
            return false;
        }
        const candidate = entry as WorkspaceMigrateRelatedWindow;
        if (seen.has(candidate.id)) {
            return false;
        }
        seen.add(candidate.id);
        if (candidate.parentId !== null && !memberIds.has(candidate.parentId)) {
            return false;
        }
    }
    if (typeof value.migratedDesktopRef !== "object" || value.migratedDesktopRef === null) {
        return false;
    }
    if (typeof value.mode !== "string" || value.mode.length === 0) {
        return false;
    }
    if (value.perOutput !== true && value.perOutput !== false && value.perOutput !== null) {
        return false;
    }
    if (typeof value.sourceTiled !== "boolean") {
        return false;
    }
    if (typeof value.protectedPresent !== "boolean") {
        return false;
    }
    if (typeof value.desktopCount !== "number" || !Number.isInteger(value.desktopCount) || value.desktopCount < 0) {
        return false;
    }
    if (typeof value.sourceFingerprint !== "string" || typeof value.targetViewFingerprint !== "string") {
        return false;
    }
    return true;

}

function snapshotWindowOfMigrate(entry: WorkspaceMigrateObservedWindow): WorkspaceMigrateSnapshotWindow {
    return {
        id: entry.id,
        rect: { x: entry.rect.x, y: entry.rect.y, w: entry.rect.w, h: entry.rect.h },
        output: entry.output,
        floating: entry.floating,
        sticky: entry.sticky,
        fullscreen: entry.fullscreen,
        maximized: entry.maximized,
        fitExcluded: entry.fitExcluded,
        minimized: entry.minimized,
        transient: entry.transient,
        fixedAuto: entry.fixedAuto,
        fixedSuppress: entry.fixedSuppress,
    };
}

function snapshotRelatedOfMigrate(entry: WorkspaceMigrateRelatedWindow): WorkspaceMigrateSnapshotRelated {
    return {
        id: entry.id,
        output: entry.output,
        parentId: entry.parentId,
        transient: entry.transient,
        minimized: entry.minimized,
        floating: entry.floating,
        sticky: entry.sticky,
        fullscreen: entry.fullscreen,
        maximized: entry.maximized,
        fitExcluded: entry.fitExcluded,
    };
}

export function snapshotOfMigrate(observed: WorkspaceMigrateObserved): WorkspaceMigrateSnapshot {
    return {
        direction: observed.direction,
        sourceOutput: observed.sourceOutput,
        sourceWorkspace: observed.sourceWorkspace,
        sourceBounds: {
            x: observed.sourceBounds.x,
            y: observed.sourceBounds.y,
            w: observed.sourceBounds.w,
            h: observed.sourceBounds.h,
        },
        targetOutput: observed.targetOutput,
        targetWorkspace: observed.targetWorkspace,
        targetBounds: {
            x: observed.targetBounds.x,
            y: observed.targetBounds.y,
            w: observed.targetBounds.w,
            h: observed.targetBounds.h,
        },
        targetCurrentWorkspace: observed.targetCurrentWorkspace,
        sourceCurrentWorkspace: observed.sourceCurrentWorkspace,
        focusedId: observed.focusedId,
        sourceWindows: Object.freeze(observed.sourceWindows.map(snapshotWindowOfMigrate)),
        targetViewWindows: Object.freeze(observed.targetViewWindows.map(snapshotWindowOfMigrate)),
        relatedWindows: Object.freeze(observed.relatedWindows.map(snapshotRelatedOfMigrate)),
        mode: observed.mode,
        perOutput: observed.perOutput,
        sourceTiled: observed.sourceTiled,
        protectedPresent: observed.protectedPresent,
        desktopCount: observed.desktopCount,
        sourceFingerprint: observed.sourceFingerprint,
        targetViewFingerprint: observed.targetViewFingerprint,
    };
}

function migrateWindowListsEqual(
    a: ReadonlyArray<WorkspaceMigrateSnapshotWindow>,
    b: ReadonlyArray<WorkspaceMigrateSnapshotWindow>,
    compareRects: boolean,
    compareOutput: boolean,
): boolean {
    if (a.length !== b.length) {
        return false;
    }
    const byId = new Map<string, WorkspaceMigrateSnapshotWindow>();
    for (const entry of a) {
        byId.set(entry.id, entry);
    }
    for (const entry of b) {
        const other = byId.get(entry.id);
        if (other === undefined) {
            return false;
        }
        if (
            other.floating !== entry.floating ||
            other.sticky !== entry.sticky ||
            other.fullscreen !== entry.fullscreen ||
            other.maximized !== entry.maximized ||
            other.fitExcluded !== entry.fitExcluded ||
            other.minimized !== entry.minimized ||
            other.transient !== entry.transient ||
            other.fixedAuto !== entry.fixedAuto ||
            other.fixedSuppress !== entry.fixedSuppress
        ) {
            return false;
        }
        if (compareOutput && other.output !== entry.output) {
            return false;
        }
        // Native send-to-output re-fits overlays: overlay arrival verifies
        // identity/membership/output, never the old rectangle.
        const eitherOverlay =
            other.fullscreen || other.maximized || entry.fullscreen || entry.maximized;
        if (
            compareRects &&
            !eitherOverlay &&
            (other.rect.x !== entry.rect.x ||
                other.rect.y !== entry.rect.y ||
                other.rect.w !== entry.rect.w ||
                other.rect.h !== entry.rect.h)
        ) {
            return false;
        }
    }
    return true;
}

// Related identity: same id set with equal gate flags. Entries following a
// migrating parent (parentId set) move implicitly, so their output is
// proven separately and never compared here; view-local protected
// clients never move, so their output compares.
function migrateRelatedListsEqual(
    a: ReadonlyArray<WorkspaceMigrateSnapshotRelated>,
    b: ReadonlyArray<WorkspaceMigrateSnapshotRelated>,
): boolean {
    if (a.length !== b.length) {
        return false;
    }
    const byId = new Map<string, WorkspaceMigrateSnapshotRelated>();
    for (const entry of a) {
        byId.set(entry.id, entry);
    }
    for (const entry of b) {
        const other = byId.get(entry.id);
        if (other === undefined) {
            return false;
        }
        if (
            other.parentId !== entry.parentId ||
            other.transient !== entry.transient ||
            other.minimized !== entry.minimized ||
            other.floating !== entry.floating ||
            other.sticky !== entry.sticky ||
            other.fullscreen !== entry.fullscreen ||
            other.maximized !== entry.maximized ||
            other.fitExcluded !== entry.fitExcluded
        ) {
            return false;
        }
        if (entry.parentId === null && other.output !== entry.output) {
            return false;
        }
    }
    return true;
}

function migrateBoundsEqual(
    a: WorkspaceSendRect,
    b: WorkspaceSendRect,
): boolean {
    return a.x === b.x && a.y === b.y && a.w === b.w && a.h === b.h;
}

// Strict frozen identity and membership validation before writes: the
// pinned re-observation must equal the dispatch snapshot in scope, views,
// policy, members, flags, and geometry. Used only at the reply boundary
// while nothing has moved yet.
function migrateSnapshotsEqual(a: WorkspaceMigrateSnapshot, b: WorkspaceMigrateSnapshot): boolean {
    return (
        a.direction === b.direction &&
        a.sourceOutput === b.sourceOutput &&
        a.sourceWorkspace === b.sourceWorkspace &&
        a.targetOutput === b.targetOutput &&
        a.targetWorkspace === b.targetWorkspace &&
        a.targetCurrentWorkspace === b.targetCurrentWorkspace &&
        a.sourceCurrentWorkspace === b.sourceCurrentWorkspace &&
        a.focusedId === b.focusedId &&
        a.mode === b.mode &&
        a.perOutput === b.perOutput &&
        a.sourceTiled === b.sourceTiled &&
        a.protectedPresent === b.protectedPresent &&
        a.desktopCount === b.desktopCount &&
        a.sourceFingerprint === b.sourceFingerprint &&
        a.targetViewFingerprint === b.targetViewFingerprint &&
        migrateBoundsEqual(a.sourceBounds, b.sourceBounds) &&
        migrateBoundsEqual(a.targetBounds, b.targetBounds) &&
        migrateWindowListsEqual(a.sourceWindows, b.sourceWindows, true, true) &&
        migrateWindowListsEqual(a.targetViewWindows, b.targetViewWindows, true, true) &&
        migrateRelatedListsEqual(a.relatedWindows, b.relatedWindows)
    );
}

// Post-write commit scope: same frozen checks minus volatile fields. Member
// rects change through planned geometry writes and member outputs change
// through transfer, so those never compare here; id sets plus exception
// flags must still match exactly (no missing survivors, no newly arrived
// windows, no flag flips). The focused/active identity is excluded: KWin
// may refocus as a side effect of transfers and view switches, and the
// active retain resolves by stable id only. Views must still show the
// pre-view state (no external switch under us).
function migrateCommitScopeHolds(
    fresh: WorkspaceMigrateObserved,
    snapshot: WorkspaceMigrateSnapshot,
): boolean {
    return migrateScopedHolds(fresh, snapshot, snapshot.sourceWorkspace, snapshot.targetCurrentWorkspace, true);
}

// Shared frozen-scope comparison with explicit expected views: the
// pre-write commit state plus the flight's own intermediate and terminal
// view states. Expected views are the only view inputs; weakening any
// other check is never a recovery. Source members and related clients
// always compare by observed list (they stay desktop-homed throughout).
// The target current view compares by observed list only while no own
// view write has landed (checkTargetView); afterwards the prior target
// members are hidden by the flight's own switch and prove out through
// the ref-based residue check instead.
function migrateScopedHolds(
    fresh: WorkspaceMigrateObserved,
    snapshot: WorkspaceMigrateSnapshot,
    expectSourceView: string,
    expectTargetView: string,
    checkTargetView: boolean,
): boolean {
    if (
        fresh.direction !== snapshot.direction ||
        fresh.sourceOutput !== snapshot.sourceOutput ||
        fresh.sourceWorkspace !== snapshot.sourceWorkspace ||
        fresh.targetOutput !== snapshot.targetOutput ||
        fresh.targetWorkspace !== snapshot.targetWorkspace ||
        fresh.targetCurrentWorkspace !== expectTargetView ||
        fresh.sourceCurrentWorkspace !== expectSourceView ||
        fresh.mode !== snapshot.mode ||
        fresh.perOutput !== snapshot.perOutput ||
        fresh.sourceTiled !== snapshot.sourceTiled ||
        fresh.protectedPresent !== snapshot.protectedPresent ||
        fresh.desktopCount !== snapshot.desktopCount ||
        fresh.sourceFingerprint !== snapshot.sourceFingerprint ||
        (checkTargetView && fresh.targetViewFingerprint !== snapshot.targetViewFingerprint) ||
        !migrateBoundsEqual(fresh.sourceBounds, snapshot.sourceBounds) ||
        !migrateBoundsEqual(fresh.targetBounds, snapshot.targetBounds)
    ) {
        return false;
    }
    if (
        !migrateWindowListsEqual(snapshotOfMigrate(fresh).sourceWindows, snapshot.sourceWindows, false, false)
    ) {
        return false;
    }
    if (!migrateRelatedListsEqual(snapshotOfMigrate(fresh).relatedWindows, snapshot.relatedWindows)) {
        return false;
    }
    if (
        checkTargetView &&
        !migrateWindowListsEqual(snapshotOfMigrate(fresh).targetViewWindows, snapshot.targetViewWindows, false, true)
    ) {
        return false;
    }
    return true;
}

// Narrow pre-dispatch guard: only an unexplained fit-excluded flag without
// floating/sticky/fullscreen/maximized origin refuses. Fullscreen and
// maximized members carry and never refuse here.
function migrateOverlayPresent(observed: WorkspaceMigrateObserved): boolean {
    for (const entry of [...observed.sourceWindows, ...observed.targetViewWindows]) {
        if (entry.fitExcluded && !entry.floating && !entry.sticky && !entry.fullscreen && !entry.maximized) {
            return true;
        }
    }
    for (const entry of observed.relatedWindows) {
        if (entry.fitExcluded && !entry.floating && !entry.sticky && !entry.fullscreen && !entry.maximized) {
            return true;
        }
    }
    return false;
}

function validateMigrateGeometryEntry(value: unknown): WorkspaceMigrateGeometryEntry | null {
    if (!isRecord(value)) {
        return null;
    }
    if (!hasExactKeys(value, ["window", "leaf", "output", "workspace", "rect"])) {
        return null;
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
    };
}

function validateMigrateOperation(
    value: unknown,
    correlation: string,
    flight: { readonly direction: WorkspaceMigrateDirection; readonly snapshot: WorkspaceMigrateSnapshot },
): { record: Record<string, unknown>; activeWindow: string | null } | null {
    void correlation;
    if (!isRecord(value)) {
        return null;
    }
    if (
        !hasExactKeys(value, [
            "op",
            "direction",
            "source_output",
            "source_workspace",
            "target_output",
            "target_workspace",
            "active_window",
        ])
    ) {
        return null;
    }
    if (value["op"] !== "migrate-workspace") {
        return null;
    }
    if (!isMigrateDirection(value["direction"]) || value["direction"] !== flight.direction) {
        return null;
    }
    for (const field of ["source_output", "source_workspace", "target_output", "target_workspace"]) {
        if (!isOpaqueId(value[field])) {
            return null;
        }
    }
    const snapshot = flight.snapshot;
    if (
        (value["source_output"] as string) !== snapshot.sourceOutput ||
        (value["source_workspace"] as string) !== snapshot.sourceWorkspace ||
        (value["target_output"] as string) !== snapshot.targetOutput ||
        (value["target_workspace"] as string) !== snapshot.targetWorkspace
    ) {
        return null;
    }
    const activeRaw = value["active_window"];
    if (activeRaw !== null && !isOpaqueId(activeRaw)) {
        return null;
    }
    const activeWindow = activeRaw === null ? null : (activeRaw as string);
    if (activeWindow !== null && !snapshot.sourceWindows.some((entry) => entry.id === activeWindow)) {
        return null;
    }
    return { record: value, activeWindow };
}

function validateMigratePlanned(
    reply: unknown,
    correlation: string,
    flight: { readonly direction: WorkspaceMigrateDirection; readonly snapshot: WorkspaceMigrateSnapshot },
): WorkspaceMigratePlanned | null {
    if (!isRecord(reply)) {
        return null;
    }
    if (reply["v"] !== WORKSPACE_SEND_CONTRACT_VERSION) {
        return null;
    }
    if (reply["correlation_id"] !== correlation) {
        return null;
    }
    if (reply["outcome"] !== "planned") {
        return null;
    }
    if (reply["kind"] !== "migrate-workspace") {
        return null;
    }
    const baseRevision = reply["base_revision"];
    if (!isRevision(baseRevision)) {
        return null;
    }
    const preconditions = reply["preconditions"];
    if (!isExactPreconditions(preconditions)) {
        return null;
    }
    const operationValidated = validateMigrateOperation(reply["operation"], correlation, flight);
    if (operationValidated === null) {
        return null;
    }
    const operation = operationValidated.record;
    // Strict detail binding: kind, policy version 1, capability, the
    // retained-rekey commit marker, direction, both scopes, the active
    // window echo, and member/float counts. Exact key set, frozen shape.
    const detailRaw = reply["detail"];
    if (!isRecord(detailRaw)) {
        return null;
    }
    if (
        !hasExactKeys(detailRaw, [
            "kind",
            "policy_version",
            "capability",
            "commit",
            "direction",
            "source_output",
            "source_workspace",
            "target_output",
            "target_workspace",
            "active_window",
            "members",
            "floats",
        ])
    ) {
        return null;
    }
    if (
        detailRaw["kind"] !== "migrate-workspace" ||
        detailRaw["policy_version"] !== 1 ||
        detailRaw["capability"] !== "migrate-workspace" ||
        detailRaw["commit"] !== "retained-rekey-planned" ||
        detailRaw["direction"] !== operation["direction"] ||
        detailRaw["source_output"] !== operation["source_output"] ||
        detailRaw["source_workspace"] !== operation["source_workspace"] ||
        detailRaw["target_output"] !== operation["target_output"] ||
        detailRaw["target_workspace"] !== operation["target_workspace"]
    ) {
        return null;
    }
    const detailActive = detailRaw["active_window"];
    const operationActive = operation["active_window"];
    if (
        (detailActive === null) !== (operationActive === null) ||
        (detailActive !== null && detailActive !== operationActive)
    ) {
        return null;
    }
    for (const count of ["members", "floats"]) {
        const raw = detailRaw[count];
        if (typeof raw !== "number" || !Number.isInteger(raw) || raw < 0 || raw > WORKSPACE_SEND_MAX_DESKTOPS * 64) {
            return null;
        }
    }
    const geometryRaw = reply["desired_geometry"];
    if (!Array.isArray(geometryRaw)) {
        return null;
    }
    const geometry: WorkspaceMigrateGeometryEntry[] = [];
    const seen = new Set<string>();
    for (const entry of geometryRaw) {
        const valid = validateMigrateGeometryEntry(entry);
        if (valid === null || seen.has(valid.window)) {
            return null;
        }
        seen.add(valid.window);
        geometry.push(valid);
    }
    // Desired focus binds the follow: null when no tiled active member
    // migrates, else the migrating tiled leaf in the target domain. Never
    // a float leaf, never fabricated.
    const focusRaw = reply["desired_focus"];
    let focusLeaf: string | null = null;
    if (focusRaw !== null && focusRaw !== undefined) {
        if (!isRecord(focusRaw) || !hasExactKeys(focusRaw, ["domain_output", "domain_workspace", "leaf"])) {
            return null;
        }
        if (
            !isOpaqueId(focusRaw["domain_output"]) ||
            !isOpaqueId(focusRaw["domain_workspace"]) ||
            !isOpaqueId(focusRaw["leaf"])
        ) {
            return null;
        }
        if (
            (focusRaw["domain_output"] as string) !== flight.snapshot.targetOutput ||
            (focusRaw["domain_workspace"] as string) !== flight.snapshot.targetWorkspace
        ) {
            return null;
        }
        focusLeaf = focusRaw["leaf"] as string;
    }
    return {
        correlationId: correlation,
        baseRevision: baseRevision as number,
        direction: flight.direction,
        sourceOutput: flight.snapshot.sourceOutput,
        sourceWorkspace: flight.snapshot.sourceWorkspace,
        targetOutput: flight.snapshot.targetOutput,
        targetWorkspace: flight.snapshot.targetWorkspace,
        activeWindow: operationValidated.activeWindow,
        geometry: Object.freeze(geometry),
        focusLeaf,
        members: detailRaw["members"] as number,
        floats: detailRaw["floats"] as number,
    };
}

// Frozen plan-to-snapshot binding held before any native write: the
// operation scopes match, geometry covers exactly the wire members
// (non-sticky, non-minimized, non-floating) in the target domain, and
// member/float counts match the wire partition. Minimized members ride
// native-only and never join the wire counts; sticky members stay homed.
// Focus/active are consistent (active names a carried visible member only;
// focus names that member's tiled leaf, or both are null for
// empty/absent/sticky-active/float-active routes).
function migrateBindingHolds(planned: WorkspaceMigratePlanned, snapshot: WorkspaceMigrateSnapshot): boolean {
    if (
        planned.direction !== snapshot.direction ||
        planned.sourceOutput !== snapshot.sourceOutput ||
        planned.sourceWorkspace !== snapshot.sourceWorkspace ||
        planned.targetOutput !== snapshot.targetOutput ||
        planned.targetWorkspace !== snapshot.targetWorkspace
    ) {
        return false;
    }
    // Wire partition: minimized members are native-only (filtered from the
    // request windows), sticky members stay homed.
    const wireMembers = snapshot.sourceWindows.filter((entry) => !entry.sticky && !entry.minimized);
    const tiled = wireMembers.filter((entry) => !entry.floating);
    const floats = wireMembers.filter((entry) => entry.floating);
    if (planned.members !== wireMembers.length || planned.floats !== floats.length) {
        return false;
    }
    const tiledIds = new Set(tiled.map((entry) => entry.id));
    if (planned.geometry.length !== tiled.length) {
        return false;
    }
    const seenWindows = new Set<string>();
    const leafToWindow = new Map<string, string>();
    for (const entry of planned.geometry) {
        if (!tiledIds.has(entry.window) || seenWindows.has(entry.window)) {
            return false;
        }
        seenWindows.add(entry.window);
        if (entry.output !== snapshot.targetOutput || entry.workspace !== snapshot.targetWorkspace) {
            return false;
        }
        if (leafToWindow.has(entry.leaf)) {
            return false;
        }
        leafToWindow.set(entry.leaf, entry.window);
    }
    const active = planned.activeWindow;
    const focusedEntry = snapshot.sourceWindows.find((entry) => entry.id === snapshot.focusedId) ?? null;
    if (snapshot.focusedId === "") {
        if (active !== null) {
            return false;
        }
    } else if (focusedEntry === null) {
        return false;
    } else if (focusedEntry.sticky || focusedEntry.minimized) {
        // Sticky active stays homed on the source and minimized clients
        // never take focus: the reply echoes null like an absent client
        // and the adapter runs zero focus setters.
        if (active !== null) {
            return false;
        }
    } else if (active !== snapshot.focusedId) {
        return false;
    }
    if (planned.focusLeaf === null) {
        // Null focus covers empty/absent/sticky-active routes plus a
        // carried visible float active (no tiled leaf to name).
        if (active === null) {
            return true;
        }
        const activeEntry = snapshot.sourceWindows.find((entry) => entry.id === active) ?? null;
        return (
            activeEntry !== null && activeEntry.floating && !activeEntry.sticky && !activeEntry.minimized
        );
    }
    // Set focus must name the active member's own tiled leaf.
    if (active === null) {
        return false;
    }
    const activeEntry = snapshot.sourceWindows.find((entry) => entry.id === active) ?? null;
    if (activeEntry === null || activeEntry.floating || activeEntry.sticky || activeEntry.minimized) {
        return false;
    }
    return leafToWindow.get(planned.focusLeaf) === active;
}
