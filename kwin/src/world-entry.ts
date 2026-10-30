// World production entry (strict v2): the single active KDE policy path
// over the sole broker
// (DiscoverWorld/DescribeWorld/QueueWorldCommand/TakeCommandResult). The
// broker owns ALL sequencing (obs/reqSeq/cmdSeq) and its clock: this
// adapter owns only two transport fences - the per-capture source
// correlation and the local native-invalidation watermark - plus the
// queue-client command counter. Every user command files via
// QueueWorldCommand, drains into the next fresh capture, and collects its
// terminal result via TakeCommandResult (the capture reply result lane is
// always null). The legacy plan route and the v1 direct report route are
// removed; Planner is the SOLE workspace policy: every
// selection/send/migration shortcut builds a world command; this module
// never decides trailing reuse/creation, history, hotplug mapping, or
// relative/numbered resolution locally. Native code owns only observation
// plus safe actuation with no silent fallback.

import { connectSignal, readSignal } from "./signal-capability";
import { registerWorldShortcuts } from "./world-shortcuts";
import { createWorldApplyHost, createWorldNativeToggles } from "./world-native";
import {
    createWorldPresentationSubscriber,
    PRESENTATION_EFFECT_CLEAR_METHOD,
    PRESENTATION_EFFECT_HIDE_METHOD,
    PRESENTATION_EFFECT_INTERFACE,
    PRESENTATION_EFFECT_OBJECT,
    PRESENTATION_EFFECT_SERVICE,
    PRESENTATION_EFFECT_SET_METHOD,
    type WorldPresentationSubscriber,
} from "./world-presentation";

// Float-intent method names mirror the Planner1 interface (same values as
// the legacy plan adapter's intent route): one bounded read before first
// dispatch seeds membership, settled changes persist via write. Native
// membership transport only, never workspace policy.
const INTENT_READ_METHOD = "ReadFloatIntent";
const INTENT_WRITE_METHOD = "WriteFloatIntent";

// Compile-time source identity (retired source-rev module inlined here so
// the installed bundle keeps baking it): the installed KWin bundle build
// (build:installed) bakes the release archive's OMNITILER_SOURCE_REV into
// this constant through an esbuild define; ordinary npm builds resolve
// deterministically to the local-dev fallback. The value is bounded to a
// git revision (40 or 64 lowercase hex) so the constant can never carry
// arbitrary data. The world ready log carries it so the release archive's
// baked-rev grep keeps passing; no transport or behavior effect.
declare const __OMNITILER_SOURCE_REV__: string | undefined;
// Compile-time trace gate: trace builds define "1", ordinary builds define
// "0", so `typeof __OMNITILER_TRACE__ === "string"` folds per bundle and a
// statically-false gate drops the single probe construction block below.
declare const __OMNITILER_TRACE__: string | undefined;

interface WorldTraceProbe {
    noteAdded(added: object): void;
    noteDebounceStart(): void;
    noteDebounceFire(): void;
    noteSend(sourceToken: string, observation: WorldObservation | null): void;
    noteReply(sourceToken: string): void;
    setGeometryFor(
        fresh: WorldObservation,
        base: {
            setGeometry: (ref: object, rect: { x: number; y: number; w: number; h: number }) => boolean;
            readLiveRect: (ref: object) => { x: number; y: number; w: number; h: number } | null;
        },
        sourceToken: string,
    ): (ref: object, rect: { x: number; y: number; w: number; h: number }) => boolean;
    stop(): void;
}
// A weak key preserves per-adapter isolation without leaving an unused local
// initializer in ordinary bundles; all references below are compile-gated.
let worldTraceProbes: WeakMap<WorldAdapterState, WorldTraceProbe>;
if (typeof __OMNITILER_TRACE__ === "string" && __OMNITILER_TRACE__ === "1") {
    worldTraceProbes = new WeakMap();
}

const SOURCE_REV_PATTERN = /^(?:[0-9a-f]{40}|[0-9a-f]{64})$/;

function resolveSourceRev(): string {
    const candidate: unknown =
        typeof __OMNITILER_SOURCE_REV__ === "string" ? __OMNITILER_SOURCE_REV__ : undefined;
    if (typeof candidate === "string" && SOURCE_REV_PATTERN.test(candidate)) {
        return candidate;
    }
    return "local-dev";
}
import {
    WORLD_MAX_REPLY_BYTES,
    WORLD_MAX_REQUEST_BYTES,
    incrementU64,
    isCanonicalU64,
    isOpaqueId,
    utf8ByteLength,
} from "./world-seq";
import {
    buildCaptureEnvelope,
    buildDiscoveryRequest,
    buildQueueRequest,
    buildTakeRequest,
    buildWaitRequest,
    buildWorldInnerReportJson,
    cmdFocusDirectional,
    cmdMigrateDirectional,
    cmdMoveDirectional,
    cmdSelectNumbered,
    cmdSelectPrevious,
    cmdSelectRelative,
    cmdSelectTrailing,
    cmdSendDirectional,
    cmdSendNumbered,
    cmdSendRelative,
    cmdToggleMaximize,
    cmdToggleWorkspaceTiling,
    implicitSource,
    isCoreWorldReply,
    isWorldQueueReply,
    isWorldRejection,
    isWorldWaitReply,
    parseDiscoveryReply,
    parseWorldReply,
    type WorldCommand,
    type WorldReply,
    type WorldReplyResult,
    type WorldTakeReply,
    type WorldWaitReply,
} from "./world-report";
import { captureWorldObservation, type PresentationSnapshot, type WorldObservation } from "./world-observe";
import {
    GESTURE_ACK_METHOD,
    GESTURE_INTERFACE,
    GESTURE_LEASE_MS,
    GESTURE_OBJECT,
    GESTURE_SERVICE,
    GESTURE_WAIT_METHOD,
    gestureAckRequest,
    gestureWaitRequest,
    parseNativeGesture,
    type NativeGestureSnapshot,
} from "./world-gesture";
import { WorldAdapterState, type CaptureFlight, type QueuedIntent } from "./world-state";
import { applyWorldReply } from "./world-apply";
import { decodeList } from "./qml-list";
import { normalizeNativeId } from "./native-id";
import { readNativeAlias } from "./native-alias";
import { createWorldCloseTracker } from "./world-close-tracker";
import { isSeparateWorkspaceMode, readPerOutputVirtualDesktopsFlag } from "./workspace-native";
import { createWorldCommandContext } from "./world-command-context";
import { createWorldSettingsLive } from "./world-settings-live";
import { normalizeScopeClassList, readBakedScopeClass, windowMatchesScopeClass } from "./scope-class";

export const WORLD_DBUS_SERVICE = "com.omnitiler.Planner";
export const WORLD_DBUS_OBJECT = "/com/omnitiler/Planner";
export const WORLD_DBUS_INTERFACE = "com.omnitiler.Planner1";
export const WORLD_DISCOVERY_METHOD = "DiscoverWorld";
export const WORLD_DESCRIBE_METHOD = "DescribeWorld";
export const WORLD_QUEUE_METHOD = "QueueWorldCommand";
export const WORLD_TAKE_METHOD = "TakeCommandResult";
export const WORLD_READ_METHOD = "ReadWorldPresentation";
// WorldWake is a best-effort latency hint emitted by the service
// (`WorldWake(gen, rev, kind)` with `presentation-updated` after accepted
// captures and `command-queued` after new queue entries). The KWin
// scripting surface exposes no D-Bus signal subscription (callDBus is
// method calls with a one-shot callback only, never a subscribe), so the
// script cannot hear foreign wakes: our own queues dispatch a fresh capture
// directly, and foreign queued commands drain on the next native-event
// capture. Both sides re-sync on their next poll either way.
export const WORLD_WAKE_SIGNAL = "WorldWake";
export const WORLD_TIMEOUT_MS = 2000;
export const WORLD_DEBOUNCE_MS = 120;
export const WORLD_WAIT_METHOD = "WaitWorldWake";
// Native effect presentation-facts route (Stage 3 ingress, read-only):
// the `omnitiler-active-border` effect owns the well-known object and
// serves one held `WaitPresentationFacts` call per adapter generation.
// The request and reply are single JSON strings (the same callDBus shape
// as every other route): request
// {"instance":"...","rev":"...","leaseMs":20000}, reply
// {"v":1,"instance":"...","rev":"...","invalidation":"...",
//  "chord":0/1,"chordKnown":0/1,"gap":f64,"width":f64,"extension":f64,
//  "changed":0/1,"superseded":0/1,"available":0/1}. A mismatched
// identity completes immediately with the fresh snapshot; otherwise the
// effect holds the call until a fact changes or the bounded lease expires.
export const EFFECT_FACTS_SERVICE = "com.omnitiler.ActiveBorder";
export const EFFECT_FACTS_OBJECT = "/com/omnitiler/ActiveBorder";
export const EFFECT_FACTS_INTERFACE = "com.omnitiler.ActiveBorder1";
export const EFFECT_FACTS_WAIT_METHOD = "WaitPresentationFacts";
// Held-call lease, service-wake scale (the effect clamps to 1-60 s
// server-side). The host watchdog below reuses WAIT_HOST_DEADLINE_MS.
export const EFFECT_FACTS_LEASE_MS = 20000;
// Host call deadline for long-held callDBus (verified, not invented):
// KWin scripting.cpp uses sessionBus().asyncCall(message) with no timeout
// argument (default -1); Qt passes it to libdbus, which substitutes
// _DBUS_DEFAULT_TIMEOUT_VALUE (25 s); errors and timeouts only log and
// never invoke the JS callback. Every long-held callDBus (the generation
// wait below, plus host-deadline discovery recovery) therefore carries a
// watchdog at this deadline that rediscovers and re-arms even with zero
// native events. Normal 2 s capture/command timing is untouched.
export const WAIT_HOST_DEADLINE_MS = 25000;

export interface WorldEntryOverrides {
    readonly workspace?: unknown;
    readonly options?: unknown;
    readonly owner?: unknown;
    readonly adapter?: unknown;
    readonly callDbus?: (
        service: string, path: string, iface: string, method: string,
        payload: string, callback: (reply: unknown) => void,
    ) => void;
    readonly scheduleOnce?: (delayMs: number, callback: () => void) => () => void;
    readonly log?: (message: string) => void;
    readonly gestureActive?: () => boolean;
    readonly registerShortcutFn?: (name: string, text: string, sequence: string, callback: () => void) => boolean;
    readonly readWorkspaceModeFn?: () => unknown;
    // Startup-consumed shortcut profile seam (main.xml `shortcutProfile`,
    // legacy `readProfileFn` parity): validated once at startup, never
    // re-read for behavior. Offline only, no KGlobalAccel writes.
    readonly readProfileFn?: () => unknown;
    // Saved default-tiling seam (legacy `defaultTiled` parity): the raw
    // saved value for the core seed policy. Defaults to the KWin script
    // `defaultTiled` config key (backfill `true`); never per-workspace
    // policy, which rides the core reply rows.
    readonly readDefaultTiledFn?: () => unknown;
    readonly readInnerGapFn?: () => unknown;
    readonly readOuterGapFn?: () => unknown;
    readonly readSameAxisMoveFn?: () => unknown;
    // R-MAX-06 born-maximized seam (main.xml `bornMaximized`): resolved
    // live per admission so a deliberate Options reload applies to
    // subsequent admissions with no restart and no state rebuild.
    readonly readBornMaximizedFn?: () => unknown;
    // Validated R-WS-12 G-37 migration source-refill seam (main.xml
    // `migrationSourceRefill`): resolved live per report so a deliberate
    // Options reload applies to subsequent migrations with no restart.
    // Changing it never moves an already-migrated workspace.
    readonly readMigrationSourceRefillFn?: () => unknown;
    // Validated D2/D3 best-effort capacity seams (main.xml
    // `anchorFallback`, `noSpaceResult`): resolved live per report so a
    // deliberate Options reload applies to subsequent capacity decisions
    // with no restart and no retained-state reflow. Tolerant readers
    // backfill the defaults.
    readonly readAnchorFallbackFn?: () => unknown;
    readonly readNoSpaceResultFn?: () => unknown;
    // Native-only seams (never workspace policy): the fixed-size
    // predicate reader and the fixed-origin decorator.
    // Native effect presentation-facts seam (read-only, no policy): the
    // transport for the held `WaitPresentationFacts` call on the effect's
    // well-known object. Same callDBus shape as every other route
    // (service/path/iface/method plus one JSON payload string). Absent
    // skips the facts lane entirely (unknown facts hide presentation);
    // production wires the global callDBus, offline tests inject exact
    // replies or omit the seam.
    readonly effectCallDbus?: (
        service: string, path: string, iface: string, method: string,
        payload: string, callback: (reply: unknown) => void,
    ) => void;
    // Native effect gesture-facts route (read-only, no policy): the
    // `omnitiler` gesture effect owns the well-known object and serves one
    // held `WaitGesture` call per adapter generation. Absent skips the lane
    // entirely (unknown facts omit ingress truthfully); production wires
    // the global callDBus, offline tests inject exact replies or omit.
    readonly gestureCallDbus?: (
        service: string, path: string, iface: string, method: string,
        payload: string, callback: (reply: unknown) => void,
    ) => void;
    // Renderer D-Bus caller for the native effect presentation endpoint
    // (SetPresentation/ClearPresentation): the effect remains the
    // renderer of accepted core shapes only, this only forwards the
    // subscriber payload. Defaults to the global callDBus like the legacy
    // plan adapter.
    readonly highlightCallDbus?: (
        service: string, path: string, iface: string, method: string,
        ...args: ReadonlyArray<unknown>
    ) => void;
    readonly readFixedPredicateFn?: () => unknown;
    readonly fixedOriginOf?: (win: string, ref: object) => { readonly fixedAuto: boolean; readonly fixedSuppress: boolean } | null;
    // Dev/test admission scope seam (default: the baked
    // `__OMNITILER_SCOPE_CLASS__` define, else the `scopeClass` script
    // config, else whole world): comma-joined string or array of exact
    // `resourceClass` identities, validated once at startup. Offline tests
    // pass `scopeClass` directly.
    readonly scopeClass?: string | readonly string[];
    readonly readScopeClassFn?: () => unknown;
    // Compatibility seams accepted from the production entry and ignored or
    // honored without policy: generation is superseded by authenticated
    // discovery (never used as the core generation); floatIntent is accepted
    // and ignored (float membership rides the full observation); the tiling
    // callback fires after each applied reply with the live snapshot.
    readonly generation?: unknown;
    readonly floatIntent?: boolean;
    readonly onWorkspaceTilingChanged?: (snapshot: { scope: string; tiled: boolean; defaultTiled: boolean }) => void;
}

export interface WorldEntryHandle {
    readonly stop: () => void;
    readonly requestFocus: (direction: unknown) => void;
    readonly requestMove: (direction: unknown) => void;
    readonly requestResize: (direction: unknown, mode: unknown) => void;
    readonly requestToggleOrientation: () => void;
    readonly requestFloat: () => void;
    readonly requestSticky: () => void;
    readonly requestMaximize: () => void;
    readonly requestFullscreen: () => void;
    readonly requestWorkspaceSelect: (index: unknown) => void;
    readonly requestWorkspaceMove: (index: unknown, follow?: unknown) => void;
    readonly requestWorkspacePrevious: () => void;
    readonly requestWorkspaceRelative: (delta: unknown) => void;
    readonly requestWorkspaceRelativeMove: (delta: unknown, follow?: unknown) => void;
    readonly requestSendToOutput: (direction: unknown, follow?: unknown) => void;
    readonly requestWorkspaceMigrate: (direction: unknown) => void;
    readonly getWorkspaceTilingSnapshot: () => { scope: string; tiled: boolean; defaultTiled: boolean };
    readonly requestWorkspaceTilingToggle: () => void;
}

function readProp(value: object, property: string): unknown {
    try {
        return Reflect.get(value, property);
    } catch (error) {
        void error;
        return undefined;
    }
}

function mintAdapterId(provided: unknown): string | null {
    if (isOpaqueId(provided)) {
        return provided as string;
    }
    // Deterministic per-process token is the caller's job; without one we
    // refuse rather than invent an unstable namespace.
    return null;
}

// Fresh adapter incarnation per script evaluation: the stable production
// base plus a Date/random suffix (opaque, bounded). Fresh per evaluation
// with overwhelming probability, so reset request/command counters open a
// fresh sequence namespace against the retained Planner.
export function mintAdapterIncarnation(): string {
    return `kwin-world-adapter-${Date.now().toString(36)}-${Math.floor(Math.random() * 0x100000000).toString(36)}`;
}

function isDirection(value: unknown): value is "left" | "right" | "up" | "down" {
    return value === "left" || value === "right" || value === "up" || value === "down";
}

export function startWorldAdapterEntry(overrides: WorldEntryOverrides = {}): WorldEntryHandle | null {
    const liveWorkspace: unknown = overrides.workspace !== undefined
        ? overrides.workspace
        : (() => {
            try {
                const candidate: unknown = workspace;
                return typeof candidate === "object" && candidate !== null ? candidate : null;
            } catch (error) {
                void error;
                return null;
            }
        })();
    if (typeof liveWorkspace !== "object" || liveWorkspace === null) {
        return null;
    }
    const surface = liveWorkspace as object;
    if (typeof readProp(surface, "windowList") !== "function") {
        return null;
    }
    if (!isOpaqueId(overrides.owner)) {
        return null;
    }
    const owner = overrides.owner as string;
    const adapter = mintAdapterId(overrides.adapter ?? "kwin-world-adapter");
    if (adapter === null) {
        return null;
    }
    let callDbus = overrides.callDbus;
    if (callDbus === undefined) {
        try {
            const native: unknown = callDBus;
            if (typeof native !== "function") {
                return null;
            }
            callDbus = (service, path, iface, method, payload, callback) => {
                try {
                    (native as (...a: ReadonlyArray<unknown>) => void)(service, path, iface, method, payload, callback);
                } catch (error) {
                    void error;
                }
            };
        } catch (error) {
            void error;
            return null;
        }
    }
    const scheduleOnce = overrides.scheduleOnce ?? ((delayMs, callback) => {
        try {
            const timer = new QTimer();
            timer.interval = delayMs;
            timer.singleShot = true;
            timer.timeout.connect(() => {
                try {
                    callback();
                } catch (error) {
                    void error;
                }
            });
            timer.start();
            return () => {
                try {
                    timer.stop();
                } catch (error) {
                    void error;
                }
            };
        } catch (error) {
            void error;
            return () => undefined;
        }
    });
    const log = overrides.log ?? ((message: string): void => {
        try {
            console.log(message);
        } catch (error) {
            void error;
        }
    });
    const state = new WorldAdapterState(adapter, owner);
    let stopped = false;
    let debounceArmed = false;
    let dispatchTimerCancel: (() => void) | null = null;
    // No DescribeWorld round may start while a DiscoverWorld round is
    // outstanding: the generation it would carry is known-stale (rebind
    // window) or absent (startup, already gated by boundGen).
    let discoverOutstanding = false;
    // FIFO of user intents not yet filed, in press order. The head is
    // shifted at QueueWorldCommand send (never replayed, even when the
    // transport reply is lost: re-filing under a new clientCmd would risk a
    // second execution). Drag-verdict and B9-continuation traffic bypasses
    // this queue through submitCommandInner (fail-fast single-flight).
    // keepSubject is LOCAL routing only (float-origin fallback keeps its
    // press-time subject); it never rides the wire.
    const pendingQueue: QueuedIntent[] = [];
    let discCounter = 0;
    let sourceCounter = 0;
    // Per-producer-lifetime monotonic capture sequence (canonical decimal
    // u64, 1-based) minted alongside the source: transport only, never core
    // sequencing. Same-lifetime rediscovery keeps it monotonic (the broker
    // floor refuses rewound sequences); only a fresh producer lifetime
    // (fresh entry) restarts it.
    let sourceSeq = "0";
    // Last queue-client command whose terminal result is still
    // uncollected (transport-failed takes retry after the next accept).
    // Keyed by clientCmd; the server settles exactly once and re-serves
    // while remembered, so retries never re-execute.
    let undecidedClientCmd: string | null = null;
    // Adapter-owned logical<->native mapping snapshot for report bindings,
    // converged from accepted replies (Planner-owned) plus pending tokens.
    // Keyed per (output, native): the separate grid carries a
    // distinct logical identity per output cell, so one native id maps to
    // one logical per output. Only the logical id and output converge here;
    // pin/disp/managed are derived per report in pinRetained below (never
    // stored constants).
    const logicalOfNative = new Map<string, { logical: string; out: string }>();
    const bindingKey = (out: string, native: string): string => `${out}\u0000${native}`;
    const detaches: Array<() => void> = [];
    // Inline compile gates let ordinary builds strip all probe work; an
    // imported/shared boolean does not fold at these use sites in esbuild.
    // Each adapter owns bounded, fail-open trace state. The only added
    // subscription is a trace-only frameGeometryChanged one-shot attached
    // before a requested write, detached on fire/supersede/eviction/stop.
    // Timestamps are wall-clock milliseconds, not native completion proof.
    if (typeof __OMNITILER_TRACE__ === "string" && __OMNITILER_TRACE__ === "1") {
        const tracePending: string[] = [];
        const traceSentBySrc = new Map<string, string[]>();
        const traceRequested = new Map<string, { req: string; ref: object; gen: number; src: string; detach: (() => void) | null }>();
        let traceGeneration = 0;
        const traceEmitPending = (ev: string, extra: string): void => {
            for (const win of [...tracePending]) {
                log(`omnitiler:world:trace win=${win} ev=${ev} t=${Date.now()}${extra}`);
            }
        };
        const traceDetachRequested = (win: string): void => {
            const want = traceRequested.get(win) ?? null;
            if (want === null) {
                return;
            }
            traceRequested.delete(win);
            const done = want.detach;
            want.detach = null;
            if (done !== null) {
                try {
                    done();
                } catch (error) {
                    void error;
                }
            }
        };
        const traceRememberSent = (src: string, wins: string[]): void => {
            traceSentBySrc.delete(src);
            while (traceSentBySrc.size >= 16) {
                const oldest = traceSentBySrc.keys().next();
                if (oldest.done) {
                    break;
                }
                traceSentBySrc.delete(oldest.value as string);
            }
            traceSentBySrc.set(src, wins);
        };
        worldTraceProbes.set(state, {
            noteAdded: (added) => {
                try {
                    const id = normalizeNativeId(readProp(added, "internalId"));
                    if (id === null) {
                        return;
                    }
                    if (tracePending.indexOf(id) < 0) {
                        tracePending.push(id);
                        while (tracePending.length > 16) {
                            const evicted = tracePending.shift();
                            if (evicted !== undefined) {
                                traceDetachRequested(evicted);
                            }
                        }
                    }
                    log(`omnitiler:world:trace win=${id} ev=added t=${Date.now()}`);
                } catch (error) {
                    void error;
                }
            },
            noteDebounceStart: () => {
                try {
                    traceEmitPending("debounce-start", "");
                } catch (error) {
                    void error;
                }
            },
            noteDebounceFire: () => {
                try {
                    traceEmitPending("debounce-fire", "");
                } catch (error) {
                    void error;
                }
            },
            noteSend: (sourceToken, observation) => {
                try {
                    let wins: string[];
                    if (observation !== null) {
                        // Actual flight truth: only the pending ids carried
                        // by the transmitted report snapshot correlate to
                        // this source. The retry path reuses the stored
                        // snapshot for its source and never falls back to
                        // live pending.
                        const inReport = new Set<string>();
                        for (const fact of observation.windowFacts) {
                            inReport.add(fact.win);
                        }
                        wins = tracePending.filter((id) => inReport.has(id));
                        traceRememberSent(sourceToken, [...wins]);
                    } else {
                        wins = [...(traceSentBySrc.get(sourceToken) ?? [])];
                    }
                    for (const win of wins) {
                        log(`omnitiler:world:trace win=${win} ev=send t=${Date.now()} src=${sourceToken}`);
                    }
                } catch (error) {
                    void error;
                }
            },
            noteReply: (sourceToken) => {
                try {
                    // One-shot per flight: only windows in the matching sent
                    // snapshot are attributed, never a window added while
                    // the flight was outstanding.
                    const wins = traceSentBySrc.get(sourceToken) ?? [];
                    for (const win of [...wins]) {
                        log(`omnitiler:world:trace win=${win} ev=reply t=${Date.now()} src=${sourceToken}`);
                    }
                } catch (error) {
                    void error;
                }
            },
            setGeometryFor: (fresh, base, sourceToken) => {
                try {
                    // Write truth rides the send snapshot: a window added
                    // after the flight sent is not yet correlated to this
                    // source, so its writes wait for its own flight.
                    const snap = traceSentBySrc.get(sourceToken) ?? [];
                    const winByRef = new Map<object, string>();
                    for (const domain of fresh.domains) {
                        for (const w of domain.windows) {
                            if (!winByRef.has(w.ref)) {
                                winByRef.set(w.ref, w.win);
                            }
                        }
                    }
                    for (const m of fresh.minimized) {
                        if (!winByRef.has(m.ref)) {
                            winByRef.set(m.ref, m.win);
                        }
                    }
                    return (ref: object, rect: { x: number; y: number; w: number; h: number }): boolean => {
                        // Armed before the production write so a synchronous
                        // native signal during the setter still finds its
                        // generation. Cleaned below on explicit false or
                        // throw so a refused write never misattributes a
                        // later unrelated frame change.
                        let probeWin: string | null = null;
                        try {
                            const win = winByRef.get(ref) ?? null;
                            if (win !== null && snap.indexOf(win) >= 0) {
                                const req = `${rect.x},${rect.y},${rect.w},${rect.h}`;
                                log(`omnitiler:world:trace win=${win} ev=write t=${Date.now()} src=${sourceToken} req=${req}`);
                                traceGeneration += 1;
                                const gen = traceGeneration;
                                traceDetachRequested(win);
                                while (traceRequested.size >= 16) {
                                    const oldest = traceRequested.keys().next();
                                    if (oldest.done) {
                                        break;
                                    }
                                    traceDetachRequested(oldest.value as string);
                                }
                                const entry = { req, ref, gen, src: sourceToken, detach: null as (() => void) | null };
                                entry.detach = connectSignal(readSignal(ref, "frameGeometryChanged"), () => {
                                    try {
                                        const want = traceRequested.get(win) ?? null;
                                        if (want === null || want.gen !== gen) {
                                            return;
                                        }
                                        traceRequested.delete(win);
                                        const done = want.detach;
                                        want.detach = null;
                                        if (done !== null) {
                                            done();
                                        }
                                        let seen = "null";
                                        let match = 0;
                                        const live = base.readLiveRect(ref);
                                        if (live !== null) {
                                            seen = `${live.x},${live.y},${live.w},${live.h}`;
                                            match = seen === want.req ? 1 : 0;
                                        }
                                        log(`omnitiler:world:trace win=${win} ev=observed t=${Date.now()} src=${want.src} req=${want.req} seen=${seen} match=${match}`);
                                        const at = tracePending.indexOf(win);
                                        if (at >= 0) {
                                            tracePending.splice(at, 1);
                                        }
                                    } catch (error) {
                                        void error;
                                    }
                                });
                                // A window without a connectable signal
                                // keeps its write line but holds no pending
                                // expectation: nothing could ever observe it.
                                if (entry.detach !== null) {
                                    traceRequested.set(win, entry);
                                    probeWin = win;
                                }
                            }
                        } catch (error) {
                            void error;
                        }
                        let outcome: boolean;
                        try {
                            outcome = base.setGeometry(ref, rect);
                        } catch (error) {
                            if (probeWin !== null) {
                                try {
                                    traceDetachRequested(probeWin);
                                } catch (cleanup) {
                                    void cleanup;
                                }
                            }
                            throw error;
                        }
                        if (!outcome && probeWin !== null) {
                            try {
                                traceDetachRequested(probeWin);
                            } catch (cleanup) {
                                void cleanup;
                            }
                        }
                        return outcome;
                    };
                } catch (error) {
                    void error;
                    return base.setGeometry;
                }
            },
            stop: () => {
                try {
                    for (const win of [...traceRequested.keys()]) {
                        traceDetachRequested(win);
                    }
                    tracePending.length = 0;
                    traceSentBySrc.clear();
                } catch (error) {
                    void error;
                }
            },
        });
    }
    // Explicit close tracking (D5): canonical live-id cache plus
    // event-gated difference (see world-close-tracker). The removal payload
    // is never read; core treats bare absence as minimize, never close.
    const closeTracker = createWorldCloseTracker();
    // Startup-consumed workspace mode passes through verbatim: all three
    // catalog tokens (per-output-local, global-unique, shared) ride the
    // world route; the observer validates, and 6.6 hosts degrade
    // per-output modes to shared predictably.
    const readModePassthrough = (): unknown => {
        try {
            return settings.readMode();
        } catch (error) {
            void error;
            return "per-output-local";
        }
    };
    // Change-gated separate-mode host warning (no polling): when a
    // separate mode is requested but KWin's per-output desktops are off,
    // log once per transition. Unknown (unreadable flag, e.g. 6.6) and
    // shared stay silent.
    let lastPerOutputWarnKey = "";
    const maybeLogPerOutputWarning = (): void => {
        let requested: unknown = "per-output-local";
        try {
            requested = readModePassthrough();
        } catch (error) {
            void error;
        }
        if (!isSeparateWorkspaceMode(requested)) {
            lastPerOutputWarnKey = `shared|${String(requested)}`;
            return;
        }
        let flag: boolean | null = null;
        try {
            const source: unknown = overrides.options !== undefined ? overrides.options : (() => {
                try {
                    const candidate: unknown = options;
                    return typeof candidate === "object" && candidate !== null ? candidate : null;
                } catch (error) {
                    void error;
                    return null;
                }
            })();
            flag = readPerOutputVirtualDesktopsFlag(source);
        } catch (error) {
            void error;
            flag = null;
        }
        const key = `${String(requested)}|${flag === null ? "unknown" : flag ? "on" : "off"}`;
        if (flag === false && key !== lastPerOutputWarnKey) {
            log(`omnitiler:world:per-output-unavailable mode=${token(String(requested))} reason=kwin-per-output-off`);
        }
        lastPerOutputWarnKey = key;
    };
    // Adapter-owned intentional-float membership (observation input only):
    // converged from accepted Planner exception rows, pruned to live ids.
    // Seeded once from the Rust-owned runtime store (ReadFloatIntent) when
    // floatIntent is enabled, persisted on settled change (WriteFloatIntent).
    const floatingIds = new Set<string>();
    let floatSeeded = false;
    // Tile-override membership (native only): fixed-window tile intent from
    // the Rust-owned store (ReadFloatIntent `tile`), disjoint from floats.
    const tileIds = new Set<string>();
    // Authoritative native gesture lifecycle (effect-owned, read-only):
    // the latest versioned snapshot the held gesture waiter delivered for
    // the bound generation, or null when unknown (effect absent, stale,
    // malformed, or explicitly unavailable). The script carries it raw;
    // the broker owns ALL mapping to the core ingress lane, so no
    // script-side identity map exists here. Read-only identity exposure
    // for presentation/preview rides `gestureIdentityForPresentation`
    // below (broker-settled truth only); no freshness enforcement.
    let gestureSnapshot: NativeGestureSnapshot | null = null;
    // Content-gated changed-line diagnostics (never timers): the gesture
    // lane logs only identity/phase transitions and the facts lane only
    // regime transitions, so continuous pointer/mover motion and
    // per-frame invalidation bumps stay silent while opens, phase
    // changes, new serials, chord/style regimes, and generation rotations
    // remain diagnosable.
    let lastGestureChangedLog = "";
    let lastEffectChangedLog = "";
    let gestureWaiterOutstanding = false;
    let gestureWaiterNonce = 0;
    let gestureWaiterGen: string | null = null;
    let gestureWatchdogCancel: (() => void) | null = null;
    let gestureRetryCancel: (() => void) | null = null;
    // Unacked terminal fence: while a terminal snapshot is incorporated
    // but not yet acked, the waiter stays down (no re-arm) so the same
    // terminal never hot-loops. Set at waiter adoption; cleared on ack
    // completion, rotation, or stop. Never abandoned while core-settled:
    // the exact pending ack persists and retries at the host-deadline
    // cadence until the native FIFO retires it.
    let gestureTerminalPending: { instance: string; serial: string } | null = null;
    // Single outstanding ack (exact pending ack): the native identity plus
    // the bound generation and nonce fencing the callback. At most one ack
    // flies at a time; transport loss retries on a bounded fast cadence
    // then at the host-deadline cadence, never a second settlement (the
    // core never re-emits one) and never an arbitrary accepted capture.
    let gestureAckPending: {
        instance: string; serial: string; gen: string; nonce: number; attempts: number;
    } | null = null;
    let gestureAckNonce = 0;
    let gestureAckTimeoutCancel: (() => void) | null = null;
    // Proven bound actuator association (broker truth, refreshed on every
    // accepted capture carrying one): the exact core id the broker mapped
    // for one native identity. Read-only presentation seam only; generation
    // fenced, snapshot matched. Set from accepted replies (open or
    // terminal), never terminal-only, so the NEXT package can expose the
    // current native identity with its proven core id while open.
    let gestureBoundAssoc: { instance: string; serial: string; coreId: string; gen: string } | null = null;
    // Two pending post-actuation follow-up observations (single slot,
    // overwritten by newer settlement, pruned on generation rotation and
    // stop). Two captures cover the pre-apply and next-reply boundaries,
    // NOT a completion/settling guarantee. No new captures or timing.
    let dropFollowup: { serial: string; instance: string; win: string; remaining: number } | null = null;
    // Exact pending ack retries: initial send plus this many fast attempts
    // total, 2 s apart, then the exact pending ack persists and retries at
    // the existing 25 s host-deadline cadence (no hot loop, no new
    // capture, no new core terminal). The native ack is idempotent
    // (exact-identity retire), so retries are side-effect free; the FIFO
    // head is never abandoned while core-settled.
    const GESTURE_ACK_MAX_ATTEMPTS = 3;
    const gestureIdentityForPresentation = (): { instance: string; serial: string; coreId: string } | null => {
        if (gestureSnapshot === null || !gestureSnapshot.available || gestureSnapshot.phase === "idle") {
            return null;
        }
        const assoc = gestureBoundAssoc;
        if (assoc === null || assoc.gen !== state.boundGen) {
            return null;
        }
        if (assoc.instance !== gestureSnapshot.instance || assoc.serial !== gestureSnapshot.serial) {
            return null;
        }
        return { instance: assoc.instance, serial: assoc.serial, coreId: assoc.coreId };
    };
    // Authoritative native gesture lifecycle (effect-owned): preview,
    // drop, resize, and hover routing all run through the core
    // GestureIngress lane. No script-side verdict route remains.

    // Live settings seams (startup snapshots plus per-dispatch
    // re-reads) live in world-settings-live; the entry only consumes
    // them here. Behavior identical to the previous inline closures.
    const settings = createWorldSettingsLive(overrides);

    // Dev/test admission scope, resolved once at startup: explicit
    // override, test seam, baked define, `scopeClass` script config, else
    // whole world. Invalid input or a throwing reader fails closed
    // (inert script). Only the absent script-config global in offline
    // tests falls back to whole world.
    const scopeClass: readonly string[] | null = ((): readonly string[] | null => {
        if (overrides.scopeClass !== undefined) {
            return normalizeScopeClassList(overrides.scopeClass);
        }
        if (overrides.readScopeClassFn !== undefined) {
            try {
                return normalizeScopeClassList(overrides.readScopeClassFn());
            } catch (error) {
                void error;
                return null;
            }
        }
        const baked = readBakedScopeClass();
        if (baked !== undefined) {
            return normalizeScopeClassList(baked);
        }
        if (typeof readConfig !== "function") {
            return [];
        }
        try {
            return normalizeScopeClassList(readConfig("scopeClass", ""));
        } catch (error) {
            void error;
            return null;
        }
    })();
    if (scopeClass === null) {
        log("omnitiler:world:scope-invalid recovery=refuse-start");
        return null;
    }
    log(`omnitiler:world:scope class=${scopeClass.length === 0 ? "-" : scopeClass.join(",")}`);

    const gestureActive = (): boolean => {
        if (overrides.gestureActive !== undefined) {
            try {
                return overrides.gestureActive();
            } catch (error) {
                void error;
                return false;
            }
        }
        // Authoritative barrier follows the native open only: script
        // interactive signals never mint a lifecycle.
        return gestureSnapshot !== null && gestureSnapshot.available &&
            (gestureSnapshot.phase === "started" || gestureSnapshot.phase === "updated");
    };

    // The raw native snapshot rides the capture verbatim: the broker maps
    // it (plus the same-capture script cursor/windowAt facts the observer
    // reads live) to the core ingress lane. Script signals never mint
    // here; terminal snapshots freeze the native finish cursor upstream.
    const capture = (): WorldObservation | null => {
        try {
            maybeLogPerOutputWarning();
        } catch (error) {
            void error;
        }
        try {
            closeTracker.syncLive(surface);
        } catch (error) {
            void error;
        }
        const observation = captureWorldObservation({
        workspace: surface,
        readMode: readModePassthrough,
        readGap: () => settings.readGapLive(),
        readOuterGap: () => settings.readOuterGapLive(),
        gestureActive,
        knownClosed: () => closeTracker.snapshotClosed(),
        scopeClass,
        logicalBindingOf: (nativeId, out) => {
            // Core-owned bindings only: the converged core mapping published
            // through accepted replies is authoritative. The adapter keeps
            // live native-id/handle lookups, never a native-to-logical
            // table of its own. Outstanding local creation receipts prove
            // nothing until the core publishes the bind.
            let base: { logical: string; out: string } | undefined;
            if (out !== undefined) {
                base = logicalOfNative.get(bindingKey(out, nativeId));
            } else {
                for (const [key, value] of logicalOfNative) {
                    if (key.endsWith(`\u0000${nativeId}`)) {
                        base = value;
                        break;
                    }
                }
            }
            if (base === undefined) {
                return null;
            }
            return {
                logical: base.logical, out: base.out,
                pin: route.pinRetained(nativeId), disp: false, managed: true,
            };
        },
        // Binding replication follows the requested workspace mode: both
        // separate modes (per-output-local, global-unique) bind one
        // native once per output with a distinct logical each, while
        // shared (unchanged) keeps one row per native. Any other value
        // (unreadable or invalid) stays single-row; the observer validates
        // the effective mode separately (6.6 degrades to shared).
        replicateBindingsPerOutput: (() => {
            try {
                return isSeparateWorkspaceMode(readModePassthrough());
            } catch (error) {
                void error;
                return false;
            }
        })(),
        isFloating: (win) => floatingIds.has(win),
        isTileOverride: (win) => tileIds.has(win),
        // Latest native-effect facts snapshot for this generation (a
        // snapshot or null when unknown): the held waiter keeps it fresh
        // and invalidates on change, so every capture carries facts that
        // precede their observation.
        presentationFacts: effectSnapshot,
        ...(overrides.readFixedPredicateFn !== undefined ? { readFixedPredicate: overrides.readFixedPredicateFn } : {}),
        ...(overrides.fixedOriginOf !== undefined ? { fixedOriginOf: overrides.fixedOriginOf } : {}),
        nativeGesture: gestureSnapshot,
    });
        if (observation === null) {
            return null;
        }
        return observation;
    };

    const token = (value: unknown): string => {
        if (typeof value === "string" && value.length > 0 && value.length <= 64 && /^[a-z0-9-]+$/.test(value)) {
            return value;
        }
        return "unknown";
    };

    // Event-driven scheduling only: synchronous invalidation first, then one
    // debounced dispatch. No idle polling anywhere.
    const scheduleDispatch = (): void => {
        if (stopped || debounceArmed) {
            return;
        }
        debounceArmed = true;
        if (typeof __OMNITILER_TRACE__ === "string" && __OMNITILER_TRACE__ === "1") {
            worldTraceProbes.get(state)?.noteDebounceStart();
        }
        dispatchTimerCancel = scheduleOnce(WORLD_DEBOUNCE_MS, () => {
            debounceArmed = false;
            dispatchTimerCancel = null;
            if (typeof __OMNITILER_TRACE__ === "string" && __OMNITILER_TRACE__ === "1") {
                worldTraceProbes.get(state)?.noteDebounceFire();
            }
            void dispatch(false);
        });
    };

    // Zero-delay queue drain, armed only after an accepted reply with
    // queued directionals still waiting. Sync native effects already ran
    // inside apply, so capturing on the next tick is fresh without
    // waiting out the observation debounce. Never preempts an armed
    // native debounce: that timer already guarantees a dispatch.
    let drainTimerCancel: (() => void) | null = null;
    const scheduleDrain = (): void => {
        if (stopped || drainTimerCancel !== null || debounceArmed) {
            return;
        }
        drainTimerCancel = scheduleOnce(0, () => {
            drainTimerCancel = null;
            void dispatch(false);
        });
    };
    // One bounded catchup drain after a terminal failure that consumed a
    // command-carrying flight with queued tails still waiting. The next
    // dispatch files the next head (or observes commandless when the queue
    // emptied), so tails drain only after the failure settles. A failed
    // commandless flight schedules nothing: no retry loop.
    const drainAfterFailure = (flight: { clientCmd: string | null }): void => {
        if (flight.clientCmd !== null && pendingQueue.length > 0) {
            scheduleDispatch();
        }
    };
    const onNativeInvalidation = (): void => {
        if (stopped) {
            return;
        }
        // Advance CURRENT obs synchronously, including during flight and
        // gestures, BEFORE any debounce.
        state.invalidate();
        scheduleDispatch();
    };

    // Shortcut/command failure line: one bounded log, native state untouched.
    const shortcutFailed = (action: string): void => {
        log(`omnitiler:world:shortcut-failed action=${token(action)}`);
    };


    const activeOutputName = (): string | null => {
        try {
            const screen = readProp(surface, "activeScreen");
            if (typeof screen === "object" && screen !== null) {
                const name = readProp(screen as object, "name");
                return isOpaqueId(name) ? (name as string) : null;
            }
        } catch (error) {
            void error;
        }
        return null;
    };

    const activeWindowId = (): string | null => {
        try {
            const active = readProp(surface, "activeWindow");
            if (typeof active === "object" && active !== null) {
                // Canonical native-id form (braced UUIDs unwrap): the raw
                // live internalId is never compared unnormalized. A foreign
                // focus reads as no active window under a scope, so no
                // command or effect path can target it.
                const id = normalizeNativeId(readProp(active as object, "internalId"));
                if (id === null) {
                    return null;
                }
                if (scopeClass.length !== 0 && !windowMatchesScopeClass(readProp, active as object, scopeClass)) {
                    return null;
                }
                return id;
            }
        } catch (error) {
            void error;
        }
        return null;
    };

    // Observation-derived local command context (workspace refs,
    // retention, resize signature) lives in world-command-context; the
    // core owns all topology selection, so no target derivation lives
    // here. Lifecycle state stays here.
    const route = createWorldCommandContext({
        observe: () => capture(),
        isDirection,
        isCreationBound: (nativeId) => {
            // A fresh native id named by an outstanding local creation
            // receipt (causal created id, a same-turn after-image delta,
            // or a deferred receipt completed at the next capture)
            // carries adapter-retained intent against core cleanup until the
            // core incorporates the binding. Never a logical mapping.
            for (const attempt of state.creationAttempts.values()) {
                if (attempt.created !== null && attempt.created === nativeId) {
                    return true;
                }
                if (attempt.created === null && !attempt.before.includes(nativeId) && attempt.after.includes(nativeId)) {
                    return true;
                }
            }
            return false;
        },
        // Retention sees every unsent queued intent (union, press order
        // irrelevant): direction-only commands name no workspace, sends
        // and migrations name their workspace refs.
        activeCommands: () => state.observationOnlyCatchup ? [] : pendingQueue.map((q) => q.command),
    });

    const liveRefById = (id: string): object | null => {
        // Window-list cap mirrors the observer bound.
        const MAX_LIVE_WINDOWS = 10000;
        try {
            const fn = readProp(surface, "windowList");
            if (typeof fn !== "function") {
                return null;
            }
            // QML array-like on the live host, never a JS array: decode
            // before iterating. Identity is matched by stable internalId
            // (fresh wrappers per enumeration), as before.
            const list = decodeList(Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []), MAX_LIVE_WINDOWS);
            if (list === null) {
                return null;
            }
            for (const item of list) {
                if (typeof item === "object" && item !== null) {
                    let native: string | null = null;
                    try {
                        // Canonical form (braced UUIDs unwrap) so live
                        // wrappers join the normalized observation ids.
                        native = normalizeNativeId(readProp(item as object, "internalId"));
                    } catch (error) {
                        void error;
                        continue;
                    }
                    if (native === id) {
                        return item as object;
                    }
                }
            }
            return null;
        } catch (error) {
            void error;
            return null;
        }
    };

    // Per-window homing echo suppression (approach2/5 native
    // drag/send adoption): the synchronous `outputChanged` /
    // `desktopsChanged` / `frameGeometryChanged` echo of our own
    // output/membership/geometry setters must not invalidate mid-apply
    // (that would wedge the flight and can resurrect stalled
    // migrations). While a wrapped setter runs,
    // `homingSuppressDepth` is nonzero with the actuated ref; that
    // window's own echo defers to one post-apply observation, while any
    // other window's signal, and closure/unsafe/disappearance of the
    // actuated window itself, still invalidates immediately.
    let homingSuppressDepth = 0;
    let homingSuppressRef: object | null = null;
    let deferredOwnHoming = false;
    const applyHostBase = createWorldApplyHost(surface, readProp);
    const withHomingSuppression = (ref: object, run: () => boolean): boolean => {
        homingSuppressDepth += 1;
        homingSuppressRef = ref;
        try {
            return run();
        } finally {
            homingSuppressDepth -= 1;
            if (homingSuppressDepth <= 0) {
                homingSuppressDepth = 0;
                homingSuppressRef = null;
            }
        }
    };

    // Scoped command fence: a window-carrying command whose live subject
    // is provably foreign never rides the wire (observation filtering
    // already drops foreign subjects at dispatch, but the command itself
    // must never name them). An unresolvable subject passes through:
    // dispatch drops it narrowly as vanished and apply holds unknown
    // targets without writes.
    const commandWindowTarget = (command: WorldCommand): string | null => {
        const rec = command as unknown as Record<string, unknown>;
        if (typeof rec["win"] === "string") {
            return rec["win"] as string;
        }
        if (rec["op"] === "operation") {
            const inner = rec["command"];
            if (typeof inner === "object" && inner !== null &&
                typeof (inner as Record<string, unknown>)["win"] === "string") {
                return (inner as Record<string, unknown>)["win"] as string;
            }
        }
        return null;
    };
    const windowInScope = (win: string): boolean => {
        if (scopeClass.length === 0) {
            return true;
        }
        let ref: object | null = null;
        try {
            ref = liveRefById(win);
        } catch (error) {
            void error;
            ref = null;
        }
        if (ref === null) {
            return true;
        }
        try {
            return windowMatchesScopeClass(readProp, ref, scopeClass);
        } catch (error) {
            void error;
            return false;
        }
    };
    // Fail-fast single-flight submit: a queued intent or a capture flight
    // in progress refuses as busy with one bounded line (the caller fails
    // closed and retries after the flight settles). All non-directional
    // commands route here, plus drag-verdict and B9-continuation traffic.
    // Filing (QueueWorldCommand) plus the draining capture happen on the
    // dispatch path; the head shifts at file-send, never replays.
    const submitCommandInner = (command: WorldCommand, label: string, keepSubject = false): boolean => {
        if (stopped) {
            return false;
        }
        if (state.boundGen === null || state.brokerIncarnation === null) {
            log(`omnitiler:world:command-refused op=${token((command as { op?: unknown }).op)} reason=undiscovered`);
            return false;
        }
        const target = commandWindowTarget(command);
        if (target !== null && !windowInScope(target)) {
            log(`omnitiler:world:command-refused op=${token((command as { op?: unknown }).op)} reason=scope-class`);
            return false;
        }
        if (pendingQueue.length > 0 || state.inFlight !== null) {
            // Distinct commands never silently coalesce: fail fast as busy.
            log(`omnitiler:world:command-busy op=${token((command as { op?: unknown }).op)}`);
            return false;
        }
        pendingQueue.push({ command, keepSubject });
        log(`omnitiler:world:command op=${token((command as { op?: unknown }).op)} label=${token(label)} nwinv=${state.currentInvalid}`);
        scheduleDispatch();
        return true;
    };

    // Narrow FIFO scope: direction-only focus/move intents queue in press
    // order behind any debounce or flight instead of refusing as busy.
    // Tiled focus and move queue the direction intent only: the subject
    // rebinds at dispatch to the authoritative current native focus
    // (below), so an earlier queued intent cannot stale a later one. The
    // float-origin cross-output fallback (same focus op shape, keepSubject
    // true) keeps its press-time subject. Everything else revalidates
    // through the fail-fast path.
    const isQueuedDirectional = (command: WorldCommand): boolean => {
        const op = (command as { op?: unknown }).op;
        return op === "focus-directional" || op === "move-directional";
    };
    // Newer explicit input supersedes a deferred B9 unfloat immediately:
    // every command routed here cancels the pending attempt (which ends
    // narrowly with no retry) before the new command routes. The B9
    // continuation itself submits through submitCommandInner so it
    // cannot cancel itself. Assigned after the native toggles exist;
    // calls before that are no-ops.
    let cancelPendingForNewerInput: (() => void) | null = null;
    // FIFO submit for directional tiled focus/move: every accepted press
    // queues in press order behind any debounce or flight instead of
    // refusing as busy. Enqueueing never advances the native-invalidation
    // watermark (only native changes invalidate), so a queued press cannot
    // stale the in-flight head; each dispatch captures fresh and rebinds a
    // tiled focus/move head to the current native focus. Returns true when
    // the press is queued or dispatched, false only on stop/undiscovered
    // refusal.
    const submitCommand = (command: WorldCommand, label: string, keepSubject = false): boolean => {
        try {
            cancelPendingForNewerInput?.();
        } catch (error) {
            void error;
        }
        if (stopped) {
            return false;
        }
        if (state.boundGen === null || state.brokerIncarnation === null) {
            log(`omnitiler:world:command-refused op=${token((command as { op?: unknown }).op)} reason=undiscovered`);
            return false;
        }
        const target = commandWindowTarget(command);
        if (target !== null && !windowInScope(target)) {
            log(`omnitiler:world:command-refused op=${token((command as { op?: unknown }).op)} reason=scope-class`);
            return false;
        }
        if (!isQueuedDirectional(command)) {
            return submitCommandInner(command, label, keepSubject);
        }
        pendingQueue.push({ command, keepSubject });
        log(`omnitiler:world:command op=${token((command as { op?: unknown }).op)} label=${token(label)} nwinv=${state.currentInvalid} depth=${pendingQueue.length}`);
        // Explicit directionals skip the observation debounce only when
        // nothing is unsettled; otherwise the debounced/event-driven path
        // drains the queue in order.
        if (state.inFlight === null && !debounceArmed && !state.observationOnlyCatchup) {
            void dispatch(false);
        } else {
            scheduleDispatch();
        }
        return true;
    };

    // Explicit output send by direction: the adapter names the window and
    // direction only (no endpoint, no cross flag); the core resolves the
    // strict target plus its current desktop and owns the membership
    // decision. No adapter topology check happens here.
    const submitOutputSend = (direction: string, follow: boolean): boolean => {
        const w = activeWindowId();
        if (w === null || !isDirection(direction)) {
            return false;
        }
        const cmd = cmdSendDirectional(w, direction, follow);
        if (cmd === null) {
            return false;
        }
        return submitCommand(cmd, `send-output-${direction}`);
    };

    // Whole-workspace migration by direction: the adapter names the source
    // output plus its current workspace and the direction only (never the
    // target, even with no active window); the core resolves the strict
    // target and owns the scope decision. No adapter topology check.
    const submitMigration = (direction: string): boolean => {
        const obs = route.observeForCommand();
        const out = activeOutputName();
        if (obs === null || out === null || !isDirection(direction)) {
            return false;
        }
        const topo = obs.topology.find((t) => t.out === out);
        if (topo === undefined || topo.current === "") {
            return false;
        }
        const cmd = cmdMigrateDirectional(direction);
        if (cmd === null) {
            return false;
        }
        return submitCommand(cmd, `migrate-${direction}`);
    };

    // Numbered workspace moves (follow/stay): positional send-numbered
    // 0..9 straight to core (D2/D3) with the keyboard implicit source
    // (retained home, else cached-live, else first live). Index 0 is the
    // trailing path: core reuses the live trailing empty or mints a stable
    // pending token and pends the send until the binding is reported.
    // Resolution is all core; this path never maps positions locally and
    // never view-switches: stays (follow=false) keep the origin view
    // because sends never switch views.
    const submitCatalogMove = (action: string, index: number, follow: boolean): void => {
        const w = activeWindowId();
        if (w === null || !Number.isInteger(index) || index < 0 || index > 9) {
            shortcutFailed(action);
            return;
        }
        const cmd = cmdSendNumbered(w, implicitSource("home-then-cached-live"), index, follow);
        if (cmd === null || !submitCommand(cmd, action)) {
            shortcutFailed(action);
        }
    };

    // Float/tile intent persistence (native membership only, never policy):
    // one bounded ReadFloatIntent before first dispatch seeds
    // intentional-float membership plus fixed-window tile overrides
    // (`tile`, disjoint from floats); settled changes persist both via
    // WriteFloatIntent. Omitted unless floatIntent is enabled. Same Planner
    // service, intent methods.
    const seedFloatIntent = (): void => {
        if (overrides.floatIntent !== true || floatSeeded) {
            return;
        }
        floatSeeded = true;
        try {
            (callDbus as NonNullable<WorldEntryOverrides["callDbus"]>)(
                WORLD_DBUS_SERVICE, WORLD_DBUS_OBJECT, WORLD_DBUS_INTERFACE,
                INTENT_READ_METHOD, `{"v":1,"correlation_id":"float-seed"}`, (reply: unknown) => {
                    try {
                        if (typeof reply !== "string") {
                            return;
                        }
                        const parsed = JSON.parse(reply) as { members?: unknown; floating?: unknown; tile?: unknown };
                        const members = Array.isArray(parsed.members)
                            ? parsed.members
                            : Array.isArray(parsed.floating)
                                ? parsed.floating
                                : null;
                        if (members === null) {
                            return;
                        }
                        for (const id of members) {
                            if (isOpaqueId(id)) {
                                floatingIds.add(id as string);
                            }
                        }
                        // Fixed-window tile overrides ride the same store,
                        // disjoint from floats; absent `tile` reads as empty.
                        if (Array.isArray(parsed.tile)) {
                            for (const id of parsed.tile) {
                                if (isOpaqueId(id) && !floatingIds.has(id as string)) {
                                    tileIds.add(id as string);
                                }
                            }
                        }
                        state.invalidate();
                        scheduleDispatch();
                    } catch (error) {
                        void error;
                    }
                },
            );
        } catch (error) {
            void error;
        }
    };

    const persistFloatIntent = (): void => {
        if (overrides.floatIntent !== true || !floatSeeded) {
            return;
        }
        try {
            const members = [...floatingIds].filter((id) => isOpaqueId(id)).slice(0, 1024);
            const tile = [...tileIds].filter((id) => isOpaqueId(id) && !floatingIds.has(id)).slice(0, 1024);
            const payload = `{"v":1,"correlation_id":"float-settled","members":${JSON.stringify(members)},"tile":${JSON.stringify(tile)}}`;
            (callDbus as NonNullable<WorldEntryOverrides["callDbus"]>)(
                WORLD_DBUS_SERVICE, WORLD_DBUS_OBJECT, WORLD_DBUS_INTERFACE,
                INTENT_WRITE_METHOD, payload, () => undefined,
            );
        } catch (error) {
            void error;
        }
    };

    // Native effect renderer calls (never workspace policy): the presentation
    // subscriber paints accepted core shapes only (border, underlay, drop
    // preview); the effect remains the renderer with native anchoring plus
    // immediate hide-only vetoes. Fail-closed false/void when the bus is
    // unavailable.
    const highlightCall = (
        method: string, ...args: ReadonlyArray<unknown>
    ): boolean => {
        try {
            const call = overrides.highlightCallDbus !== undefined
                ? overrides.highlightCallDbus
                : (() => {
                    try {
                        const native: unknown = callDBus;
                        return typeof native === "function"
                            ? (native as (...callArgs: ReadonlyArray<unknown>) => void)
                            : null;
                    } catch (error) {
                        void error;
                        return null;
                    }
                })();
            if (call === null) {
                return false;
            }
            (call as (service: string, path: string, iface: string, method: string, ...rest: ReadonlyArray<unknown>) => void)(
                PRESENTATION_EFFECT_SERVICE,
                PRESENTATION_EFFECT_OBJECT,
                PRESENTATION_EFFECT_INTERFACE,
                method,
                ...args,
            );
            return true;
        } catch (error) {
            void error;
            return false;
        }
    };

    // Read-only presentation subscriber (Stage 5): event-driven consumer of
    // the broker ReadWorldPresentation projection (same-UID read-only,
    // separate from capture/actuation). Paints accepted core shapes only
    // (border, underlay, drop preview) through the native effect renderer;
    // never derives geometry or eligibility, never touches one-shot lanes.
    // Reads trigger on generation bind/rotation (reconnect seed), on
    // accepted captures, on wait wakes (a foreign accepted capture may
    // have advanced the projection), and on effect resets; a lost reply
    // retries on the shared host deadline. No polling, no self re-arm.
    // Preview gating is causal, not epoch-based: the producing native
    // identity must match the live open gesture identity, and every paint
    // must cover the latest native-invalidation witness. Read-only
    // observation for the presentation subscriber.
    const presentationSubscriber: WorldPresentationSubscriber = createWorldPresentationSubscriber({
        callDbus: (service, path, iface, method, payload, callback) => {
            try {
                (callDbus as NonNullable<WorldEntryOverrides["callDbus"]>)(
                    service, path, iface, method, payload, callback,
                );
            } catch (error) {
                void error;
                throw error;
            }
        },
        getGeneration: () => state.boundGen,
        // Latest known causal witness for the subscriber's coverage gate:
        // the native-invalidation instance plus counter from the latest
        // fetched effect facts, or null when unknown (fail closed: no
        // drawable coverage without proof).
        getInvalidation: () => effectSnapshot === null
            ? null
            : { instance: effectSnapshot.instance, counter: effectSnapshot.invalidation },
        // Live open native gesture identity for the subscriber's preview
        // gate: the mapped native lifecycle identity behind the current
        // snapshot, or null when no gesture is open (a terminal is not
        // open). Exact identity match, never epoch or geometry inference.
        getGestureIdentity: () => gestureIdentityForPresentation(),
        isStopped: () => stopped,
        log,
        token,
        scheduleOnce,
        renderer: {
            renderPresentation: (payload) => highlightCall(PRESENTATION_EFFECT_SET_METHOD, payload),
            hidePresentation: () => {
                highlightCall(PRESENTATION_EFFECT_HIDE_METHOD);
            },
            clearPresentation: () => {
                highlightCall(PRESENTATION_EFFECT_CLEAR_METHOD);
            },
        },
    });

    // Authenticated discovery (strict v2): binds the serving generation
    // plus the broker incarnation (the reply adapter). A changed generation
    // cancels flights, ordering, the queue-client namespace, and desired
    // caches (never native wins); the adapter re-observes and command
    // issuers re-establish with fresh per-client command ids.
    const discover = (): void => {
        if (stopped) {
            return;
        }
        discCounter += 1;
        const reqId = `disc-${discCounter}`;
        const payload = buildDiscoveryRequest(adapter, reqId);
        let settled = false;
        discoverOutstanding = true;
        discoverChainActive = true;
        const timeoutCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            if (!settled) {
                settled = true;
                discoverOutstanding = false;
                log("omnitiler:world:discover-timeout recovery=on-next-event");
            }
        });
        // Host-deadline discovery recovery: when the host kills the call
        // without a callback (errors never call back), the 2 s timer above
        // already gave up waiting on a native event that may never come.
        // Retry once per host deadline until an answer lands: host-chained,
        // never idle polling. A bound generation is not proof of liveness
        // (the wait watchdog proved silence), so the chain persists while
        // no answer arrived. Sending cancels any previous host timer so
        // exactly one chain stays live across overlapping discovers.
        try {
            discoverHostCancel?.();
        } catch (error) {
            void error;
        }
        discoverHostCancel = scheduleOnce(WAIT_HOST_DEADLINE_MS, () => {
            discoverHostCancel = null;
            if (stopped || !discoverChainActive) {
                return;
            }
            log("omnitiler:world:discover-host-timeout recovery=rediscover");
            discover();
        });
        try {
            (callDbus as NonNullable<WorldEntryOverrides["callDbus"]>)(
                WORLD_DBUS_SERVICE, WORLD_DBUS_OBJECT, WORLD_DBUS_INTERFACE,
                WORLD_DISCOVERY_METHOD, payload, (reply: unknown) => {
                    if (settled) {
                        return;
                    }
                    settled = true;
                    discoverOutstanding = false;
                    timeoutCancel();
                    if (typeof reply !== "string") {
                        log("omnitiler:world:discover-transport recovery=rearm");
                        cancelDiscoverChain();
                        armWait();
                        return;
                    }
                    if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        log("omnitiler:world:discover-oversize recovery=rearm");
                        cancelDiscoverChain();
                        armWait();
                        return;
                    }
                    const parsed = parseDiscoveryReply(reply);
                    if (parsed === null || isWorldRejection(parsed)) {
                        log("omnitiler:world:discover-rejected recovery=rearm");
                        cancelDiscoverChain();
                        armWait();
                        return;
                    }
                    // Authenticated discovery: the bootstrap reply must be a
                    // non-ready core reply naming the broker incarnation
                    // (never this adapter) plus the exact outstanding
                    // discovery req. Anything else binds nothing and resets
                    // nothing.
                    if (parsed.ready !== false || parsed.adapter === adapter || parsed.req !== reqId) {
                        log("omnitiler:world:discover-rejected recovery=rearm");
                        cancelDiscoverChain();
                        armWait();
                        return;
                    }
                    const changed = state.onDiscovery(parsed.gen, parsed.adapter, adapter, parsed.req, reqId);
                    if (changed) {
                        // New generation: queued intents belong to the dead
                        // generation and drop with one diagnostic; the
                        // binding snapshot and one-shot native effects
                        // reset with the backend that owned them. The
                        // presentation subscriber clears its paint for the
                        // dead generation and seeds one read for the new one
                        // (reconnect seed, never a stale repaint).
                        dropDeadGenerationIntents();
                        logicalOfNative.clear();
                        try {
                            presentationSubscriber.notifyGeneration(parsed.gen);
                        } catch (error) {
                            void error;
                        }
                    } else {
                        // Same-generation rediscovery (service owner change or
                        // rebind): force one fresh presentation read so a
                        // restarted service with an empty cache hides instead
                        // of holding a pre-restart paint.
                        try {
                            presentationSubscriber.notifyGeneration(parsed.gen);
                        } catch (error) {
                            void error;
                        }
                    }
                    log(`omnitiler:world:discovered gen=${token(parsed.gen)}`);
                    cancelDiscoverChain();
                    waitHoldForDiscovery = false;
                    if (changed) {
                        waitCursor = "0";
                        wakePendingCapture = false;
                        // A fresh generation re-observes from a new
                        // watermark: the post-discovery dispatch below
                        // re-proves every native fact. A same-generation
                        // rebind is transport-only (the broker map persists),
                        // so it must not fabricate a native invalidation: the
                        // bump would supersede an in-flight terminal envelope
                        // and suppress its exact same-envelope recovery while
                        // the terminal is factually unchanged.
                        state.invalidate();
                    }
                    // A fresh generation drops the old effect identity
                    // before the post-discovery observation: stale snapshots
                    // and replaced waiters never cross the rotation. The
                    // proven new instance arrives with fresh facts; the
                    // unknown gap itself never resets the stream.
                    lastKnownEffectInstance = null;
                    setEffectSnapshot(null);
                    effectWaiterOutstanding = false;
                    effectWaiterGen = null;
                    try {
                        effectWatchdogCancel?.();
                    } catch (error) {
                        void error;
                    }
                    effectWatchdogCancel = null;
                    cancelEffectRecovery();
                    // A fresh generation also drops the old gesture identity:
                    // the broker map reset with the engine, so stale core
                    // ids and replaced waiters never cross the rotation. An
                    // unavailable interval inside a generation recovers
                    // without reset (broker mapping persists there). A
                    // same-generation rebind keeps the pending terminal, its
                    // proven association, and its exact same-envelope retry:
                    // the broker remembers the settlement under this generation.
                    if (changed) {
                        gestureSnapshot = null;
                        gestureTerminalPending = null;
                        gestureBoundAssoc = null;
                        dropFollowup = null;
                        clearGestureAck();
                        clearTerminalCaptureRetry();
                        lastResolvedCaptureSource = null;
                        gestureWaiterOutstanding = false;
                        gestureWaiterGen = null;
                        try {
                            gestureWatchdogCancel?.();
                        } catch (error) {
                            void error;
                        }
                        gestureWatchdogCancel = null;
                        cancelGestureRecovery();
                    }
                    void dispatch(false);
                    armEffectWaiter();
                    armGestureWaiter();
                    armWait();
                },
            );
        } catch (error) {
            void error;
            timeoutCancel();
            cancelDiscoverChain();
            discoverOutstanding = false;
            log("omnitiler:world:discover-transport recovery=on-next-event");
        }
    };

    // Deferred B9 attempt lifetime: any terminal report failure ends the
    // attempt narrowly (no retry, no replay); a later explicit press may
    // retry. Stale replies are excluded (no report arrived; the boundary
    // report is still ahead and its guards decide). `native` is declared
    // below; every call below runs after initialization.
    const killPending = (cause: string): void => {
        try {
            native.cancelPendingUnfloat(cause);
        } catch (error) {
            void error;
        }
        try {
            native.cancelPendingNativeState(cause);
        } catch (error) {
            void error;
        }
    };

    // Dead-generation intent drop: queued intents belong to the superseded
    // generation and drop with one diagnostic; the filed-but-unacked
    // identity and the undecided take clear with them, never reminted
    // into the replacement namespace. The presentation paint belongs to
    // the dead generation too: unbind the subscriber now (immediate hide,
    // in-flight reads fenced off) so no stale paint survives into the
    // replacement; the successful rediscovery re-seeds it. Callers
    // rediscover (or continue, for discovery itself).
    const dropDeadGenerationIntents = (): void => {
        const dropped = pendingQueue.length;
        pendingQueue.length = 0;
        undecidedClientCmd = null;
        unackedQueue = null;
        if (dropped > 0) {
            log(`omnitiler:world:queue-dropped reason=generation-mismatch depth=${dropped}`);
        }
        try {
            presentationSubscriber.notifyGeneration(null);
        } catch (error) {
            void error;
        }
    };

    // Drain-time subject resolution for queued directional focus and
    // move. The queue retains the direction intent; the subject rebinds
    // here to the authoritative current native focus, validated against
    // the fresh observation. A vanished subject (no active window,
    // unknown to the observation, or with no live ref) drops narrowly
    // with one bounded line and no native writes; the next queued intent
    // then resolves in turn. Floating/sticky subjects pass through like
    // tiled ones: the core owns the float-focus lattice and half-snap
    // geometry. Non-directional commands pass through untouched.
    const resolveHeadCommand = (observation: WorldObservation): QueuedIntent | null => {
        if (state.observationOnlyCatchup) {
            return null;
        }
        while (pendingQueue.length > 0) {
            const head = pendingQueue[0] as QueuedIntent;
            const rec = head.command as unknown as Record<string, unknown>;
            const op = rec["op"];
            if (op === "focus-directional" && head.keepSubject) {
                // Explicit float-origin fallback: the core resolves the
                // strict target from the press-time subject.
                return head;
            }
            if (op !== "focus-directional" && op !== "move-directional") {
                return head;
            }
            const isMove = op === "move-directional";
            const reasonPrefix = isMove ? "move" : "focus";
            const direction = rec["direction"];
            if (direction !== "left" && direction !== "right" && direction !== "up" && direction !== "down") {
                pendingQueue.shift();
                log(`omnitiler:world:command-dropped reason=${reasonPrefix}-invalid`);
                continue;
            }
            let sameAxis: string | null = null;
            if (isMove) {
                const rawAxis = rec["sameAxis"];
                if (rawAxis !== "group-with-neighbor" && rawAxis !== "swap-with-neighbor") {
                    pendingQueue.shift();
                    log(`omnitiler:world:command-dropped reason=${reasonPrefix}-invalid`);
                    continue;
                }
                sameAxis = rawAxis as string;
            }
            let current: string | null = null;
            try {
                current = activeWindowId();
            } catch (error) {
                void error;
                current = null;
            }
            if (current === null) {
                pendingQueue.shift();
                log(`omnitiler:world:command-dropped reason=${reasonPrefix}-vanished`);
                continue;
            }
            let subject: WorldObservation["domains"][number]["windows"][number] | null = null;
            for (const domain of observation.domains) {
                for (const entry of domain.windows) {
                    if (entry.win === current) {
                        subject = entry;
                        break;
                    }
                }
                if (subject !== null) {
                    break;
                }
            }
            if (subject === null || liveRefById(current) === null) {
                pendingQueue.shift();
                log(`omnitiler:world:command-dropped reason=${reasonPrefix}-vanished`);
                continue;
            }
            if (current === (rec["win"] as string)) {
                return head;
            }
            const rebuilt = isMove
                ? cmdMoveDirectional(current, direction as string, sameAxis as string)
                : cmdFocusDirectional(current, direction as string);
            if (rebuilt === null) {
                pendingQueue.shift();
                log(`omnitiler:world:command-dropped reason=${reasonPrefix}-invalid`);
                continue;
            }
            pendingQueue[0] = { command: rebuilt, keepSubject: head.keepSubject };
            return pendingQueue[0] as QueuedIntent;
        }
        return null;
    };

    // Fresh per-capture source correlation (opaque, bounded): every
    // capture mints a new source; byte-identical redelivery replays exactly
    // server-side while a changed same-source body refuses. Never reused,
    // never empty, never a sequence. The per-lifetime capture sequence
    // mints alongside: 1-based, monotonic, never reset on rediscovery under
    // the same producer lifetime.
    const mintSource = (): string => {
        sourceCounter += 1;
        let rand = "0";
        try {
            rand = Math.floor(Math.random() * 0x100000000).toString(36);
        } catch (error) {
            void error;
            rand = "0";
        }
        return `src-${sourceCounter.toString(36)}-${rand}`;
    };
    const mintSourceSeq = (): string | null => {
        const next = incrementU64(sourceSeq);
        if (next === null) {
            return null;
        }
        sourceSeq = next;
        return next;
    };
    // Filed-but-unacked queue payload: the head shifts at QueueWorldCommand
    // send; when the transport reply is lost (timeout/transport/malformed/
    // oversize) the command may already be filed server-side. The broker
    // per-client high-water dedups the same clientCmd (never re-executes),
    // so the next dispatch redelivers THE SAME queue bytes/id to resolve
    // the ambiguity, never a new numbered duplicate. The terminal take
    // collects exactly once through the normal accept path. Cleared on a
    // filed reply, on in-band refusal (never filed, head requeues at
    // front), on generation rotation, and on stop. Oversize payloads never
    // retain: identical bytes could never file.
    let unackedQueue: { head: QueuedIntent; clientCmd: string; payload: string } | null = null;
    // External-command wait lifecycle (B1): one outstanding long-held
    // WaitWorldWake per generation. waitCursor is the last delivered wake
    // cursor ("0" before the first wake): it advances only on a delivered
    // wake, persists across same-generation rediscovery, and resets on a
    // new generation, so a lost reply re-armed with the old cursor can
    // never skip unseen work. waitHoldForDiscovery blocks re-arming while
    // a generation-mismatch recovery is outstanding. wakePendingCapture
    // marks a wake owed a capture when it arrived mid-flight; sendCapture
    // consumes it on start and wakeFollowup drains it at flight end, so no
    // wake is lost to single-flight contention.
    let waitCursor = "0";
    let waitNonceCounter = 0;
    let waitNonce = "";
    let waitGen: string | null = null;
    let waitOutstanding = false;
    let waitWatchdogCancel: (() => void) | null = null;
    let waitRetryCancel: (() => void) | null = null;
    let discoverHostCancel: (() => void) | null = null;
    // Host-deadline discovery chain: set on every discover send, cleared
    // only when an answer lands (any callback) or on stop. While set, the
    // host-deadline timer keeps retrying even with a bound (but silent)
    // generation, so recovery is always host-deadline causal and never
    // waits on a native event.
    let discoverChainActive = false;
    const cancelDiscoverChain = (): void => {
        discoverChainActive = false;
        try {
            discoverHostCancel?.();
        } catch (error) {
            void error;
        }
        discoverHostCancel = null;
    };
    let waitHoldForDiscovery = false;
    let wakePendingCapture = false;
    // Native effect presentation-facts snapshot (Stage 3 ingress): the
    // latest versioned snapshot the held effect waiter delivered for the
    // bound generation, or null when unknown (effect absent, stale, or
    // malformed). Every capture carries it verbatim; the observer hides
    // presentation on null instead of guessing defaults. One waiter holds
    // per generation; fact changes invalidate and wake observation without
    // polling, and generation rotation drops stale waiters (spend/no replay
    // on connection changes: no snapshot or waiter ever crosses a
    // generation).
    let effectSnapshot: PresentationSnapshot | null = null;
    let effectWaiterOutstanding = false;
    let effectWaiterNonce = 0;
    let effectWaiterGen: string | null = null;
    let effectWatchdogCancel: (() => void) | null = null;
    let effectRetryCancel: (() => void) | null = null;

    // Presentation renderer reset/reseed event (finding H, Stage 5): the
    // raw presentation-facts instance/availability is already read every
    // generation; an instance change (effect restart) or loss (teardown)
    // means the native paint is gone, so the subscriber forgets its painted
    // projection (clear first, then reseed read on the ordered channel) and
    // re-reads. The fire-and-forget renderer call succeeds vacuously while
    // the endpoint is down and the same-projection dedupe would otherwise
    // never resend: without this explicit event a restarted effect stays
    // blank. Silence is handled by the subscriber watchdog deadline. No
    // polling; the identity folds instance only (rev bumps already flow
    // through capture invalidation).
    let lastEffectIdentityForPresentation = "unknown";
    // Last proven non-null facts instance. Unknown intervals never clear
    // this: only a proven different instance is a stream replacement.
    // Reset on generation rotation and stop.
    let lastKnownEffectInstance: string | null = null;
    const setEffectSnapshot = (next: PresentationSnapshot | null): void => {
        effectSnapshot = next;
        if (next === null) {
            // Unknown facts (unavailable/malformed/timeout/teardown, or a
            // generation rotation whose reset rides the generation lane):
            // hide display-only. Unknown is not a proven stream
            // replacement, so the painted point and the last known
            // instance are retained.
            const identity = "unknown";
            if (identity !== lastEffectIdentityForPresentation) {
                lastEffectIdentityForPresentation = identity;
                try {
                    presentationSubscriber.notifyEffectUnknown();
                } catch (error) {
                    void error;
                }
            }
            return;
        }
        const identity = `known:${next.instance}`;
        if (identity !== lastEffectIdentityForPresentation) {
            lastEffectIdentityForPresentation = identity;
        }
        // Only a proven different instance resets the stream (effect
        // restart with a new instance). A same-instance return after an
        // unknown interval recovers without reset.
        if (lastKnownEffectInstance !== null && next.instance !== lastKnownEffectInstance) {
            try {
                presentationSubscriber.notifyEffectReset();
            } catch (error) {
                void error;
            }
        }
        lastKnownEffectInstance = next.instance;
    };

    // Strict versioned parse of one effect facts reply: v:1 with an
    // opaque instance/rev, 0/1 chord lanes, finite style doubles, an
    // explicit changed/superseded lane pair and an availability lane.
    // Oversize payloads refuse like malformed ones (bounded ingress: the
    // genuine reply is tiny). A contradictory chord (held without known)
    // refuses too, and the chord folds to false whenever unknown: the
    // script never reports a level it cannot prove. Any other shape is
    // unknown (never a partial snapshot: a half-parsed chord with guessed
    // style would mix sources).
    const parseEffectFacts = (reply: unknown): (PresentationSnapshot & { changed: boolean; superseded: boolean; available: boolean }) | null => {
        if (typeof reply !== "string") {
            return null;
        }
        if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
            return null;
        }
        let value: unknown;
        try {
            value = JSON.parse(reply);
        } catch (error) {
            void error;
            return null;
        }
        if (typeof value !== "object" || value === null || Array.isArray(value)) {
            return null;
        }
        const r = value as Record<string, unknown>;
        if (r["v"] !== 1) {
            return null;
        }
        const instance = r["instance"];
        const rev = r["rev"];
        const invalidation = r["invalidation"];
        const chord = r["chord"];
        const chordKnown = r["chordKnown"];
        const gap = r["gap"];
        const width = r["width"];
        const extension = r["extension"];
        const changed = r["changed"];
        const superseded = r["superseded"];
        const available = r["available"];
        if (
            typeof instance !== "string" || instance === "" || instance.length > 128 ||
            typeof rev !== "string" || rev === "" || rev.length > 128 ||
            typeof invalidation !== "string" || !isCanonicalU64(invalidation) ||
            (chord !== 0 && chord !== 1) || (chordKnown !== 0 && chordKnown !== 1) ||
            (chord === 1 && chordKnown !== 1) ||
            typeof gap !== "number" || !Number.isFinite(gap) ||
            typeof width !== "number" || !Number.isFinite(width) ||
            typeof extension !== "number" || !Number.isFinite(extension) ||
            (changed !== 0 && changed !== 1) ||
            (superseded !== 0 && superseded !== 1) ||
            (available !== 0 && available !== 1)
        ) {
            return null;
        }
        return {
            instance, rev, invalidation, chord: chord === 1 && chordKnown === 1,
            borderGap: gap, borderWidth: width, underlayExtension: extension,
            changed: changed === 1, superseded: superseded === 1, available: available === 1,
        };
    };

    // Single held effect-facts waiter for the bound generation. Re-arm
    // FIRST on every terminal waiter reply (the effect parks the new
    // waiter immediately), then invalidate on changed snapshots so the
    // fresh facts precede their observation. The 25 s host watchdog drops
    // a killed call to unknown (hide) and re-arms with the same identity,
    // so an enabled-later effect is adopted without polling and a
    // removed effect hides without wedging the capture lane.
    const armEffectWaiter = (): void => {
        if (stopped || effectWaiterOutstanding) {
            return;
        }
        if (state.boundGen === null || state.brokerIncarnation === null || discoverOutstanding) {
            return;
        }
        // The facts lane is opt-in transport: without the effect seam the
        // lane stays unknown (hide) and nothing is armed, so offline
        // doubles without an effect service observe zero lane impact.
        const effectCall = overrides.effectCallDbus;
        if (effectCall === undefined) {
            return;
        }
        const gen = state.boundGen;
        effectWaiterNonce += 1;
        const nonce = effectWaiterNonce;
        effectWaiterGen = gen;
        effectWaiterOutstanding = true;
        const request = JSON.stringify({
            instance: effectSnapshot?.instance ?? "",
            rev: effectSnapshot?.rev ?? "",
            leaseMs: EFFECT_FACTS_LEASE_MS,
        });
        let settled = false;
        effectWatchdogCancel = scheduleOnce(WAIT_HOST_DEADLINE_MS, () => {
            if (settled || stopped || nonce !== effectWaiterNonce) {
                return;
            }
            settled = true;
            effectWaiterOutstanding = false;
            effectWaiterGen = null;
            effectWatchdogCancel = null;
            log("omnitiler:world:effect-facts-host-timeout recovery=rearm-unknown");
            setEffectSnapshot(null);
            state.invalidate();
            scheduleDispatch();
            armEffectWaiter();
        });
        try {
            (effectCall as NonNullable<WorldEntryOverrides["effectCallDbus"]>)(
                EFFECT_FACTS_SERVICE, EFFECT_FACTS_OBJECT, EFFECT_FACTS_INTERFACE,
                EFFECT_FACTS_WAIT_METHOD, request, (reply: unknown) => {
                    if (settled) {
                        return;
                    }
                    if (nonce !== effectWaiterNonce || gen !== effectWaiterGen) {
                        // Stale completion (superseded arm or rotated
                        // generation): never clears the current slot and
                        // never cancels its watchdog. The orphaned waiter
                        // is terminal here; the live arm keeps its timeout.
                        settled = true;
                        log("omnitiler:world:effect-facts-stale recovery=ignore");
                        return;
                    }
                    settled = true;
                    const cancel = effectWatchdogCancel;
                    effectWatchdogCancel = null;
                    try {
                        cancel?.();
                    } catch (error) {
                        void error;
                    }
                    onEffectFactsReply(nonce, gen, reply);
                },
            );
        } catch (error) {
            void error;
            settled = true;
            effectWaiterOutstanding = false;
            effectWaiterGen = null;
            try {
                effectWatchdogCancel?.();
            } catch (discardError) {
                void discardError;
            }
            effectWatchdogCancel = null;
            // Synchronous transport throw: drop any retained snapshot (a
            // stale chord or style must never ride another capture) with one
            // follow-up capture only when something was known, then stand
            // down to the single host-deadline recovery chain: no hot loop
            // on a synchronously throwing transport.
            if (effectSnapshot !== null) {
                setEffectSnapshot(null);
                state.invalidate();
                scheduleDispatch();
            }
            log("omnitiler:world:effect-facts-transport recovery=host-deadline-unknown");
            scheduleEffectRecovery();
        }
    };
    // Terminal reply of one held effect-facts call. The nonce plus
    // generation fence stale completions first (a retried waiter owns the
    // slot; a late callback neither clears it nor re-arms beside it).
    // Every valid reply re-arms for the current generation; only changed
    // snapshots invalidate (a lease expiry is not work and never dispatches
    // by itself). A superseded waiter stands down to the host deadline
    // instead of ping-ponging an immediate re-arm against its replacement.
    // A malformed reply hides presentation (unknown snapshot plus one
    // follow-up capture) and then stands down the same way: a persistently
    // malformed effect yields one call per host deadline, never a hot
    // re-arm loop over a stale snapshot.
    const onEffectFactsReply = (nonce: number, gen: string, reply: unknown): void => {
        if (stopped) {
            return;
        }
        if (nonce !== effectWaiterNonce || gen !== effectWaiterGen) {
            log("omnitiler:world:effect-facts-stale recovery=ignore");
            return;
        }
        effectWaiterOutstanding = false;
        effectWaiterGen = null;
        if (gen !== state.boundGen) {
            log("omnitiler:world:effect-facts-stale recovery=ignore");
            return;
        }
        const parsed = parseEffectFacts(reply);
        if (parsed === null) {
            // Malformed/oversize/invalid: unknown supersedes any retained
            // snapshot (a stale chord or style must never ride another
            // capture), one follow-up capture publishes the hide, and the
            // lane stands down to the single host-deadline recovery chain.
            log("omnitiler:world:effect-facts-malformed recovery=host-deadline-unknown");
            setEffectSnapshot(null);
            state.invalidate();
            scheduleDispatch();
            scheduleEffectRecovery();
            return;
        }
        if (!parsed.available) {
            // Explicit endpoint teardown: same unknown treatment as
            // malformed, with its own reason. The next waiter (after a
            // restart) re-binds a fresh instance; a stale one never applies
            // here because generation fencing already dropped it.
            log("omnitiler:world:effect-facts-unavailable recovery=host-deadline-unknown");
            setEffectSnapshot(null);
            state.invalidate();
            scheduleDispatch();
            scheduleEffectRecovery();
            return;
        }
        if (parsed.superseded) {
            // Replaced by a newer waiter: stand down to the host deadline
            // instead of ping-ponging an immediate re-arm. The snapshot is
            // kept: supersession is transport, never newer facts.
            log("omnitiler:world:effect-facts-superseded recovery=host-deadline");
            scheduleEffectRecovery();
            return;
        }
        setEffectSnapshot({
            instance: parsed.instance, rev: parsed.rev, invalidation: parsed.invalidation,
            chord: parsed.chord,
            borderGap: parsed.borderGap, borderWidth: parsed.borderWidth,
            underlayExtension: parsed.underlayExtension,
        });
        armEffectWaiter();
        if (parsed.changed) {
            // Content-gated like the gesture lane: per-frame invalidation
            // bumps stay silent while regime transitions (effect restart,
            // chord toggle, style change) stay diagnosable. The facts
            // revision advances on every bump, so it must not key the
            // gate; chord plus style numbers identify the regime.
            const effectChangeKey =
                `${state.boundGen ?? ""}\u0000${parsed.instance}\u0000${parsed.chord ? 1 : 0}\u0000${parsed.borderGap}\u0000${parsed.borderWidth}\u0000${parsed.underlayExtension}`;
            if (effectChangeKey !== lastEffectChangedLog) {
                lastEffectChangedLog = effectChangeKey;
                log(`omnitiler:world:effect-facts-changed instance=${token(parsed.instance)} rev=${token(parsed.rev)}`);
            }
            state.invalidate();
            scheduleDispatch();
        }
    };

    // Single host-bound effect-facts recovery chain: at most one live 25 s
    // timer; arming while one lives keeps the original deadline (never
    // overwritten). On fire the waiter re-arms if the generation still
    // holds it; an unbound generation waits for discovery, which arms
    // directly. Superseded and malformed lanes converge here, so neither
    // can hot-loop: at most one waiter flight per host deadline.
    const cancelEffectRecovery = (): void => {
        try {
            effectRetryCancel?.();
        } catch (error) {
            void error;
        }
        effectRetryCancel = null;
    };
    const scheduleEffectRecovery = (): void => {
        if (stopped || effectRetryCancel !== null) {
            return;
        }
        effectRetryCancel = scheduleOnce(WAIT_HOST_DEADLINE_MS, () => {
            effectRetryCancel = null;
            if (stopped || effectWaiterOutstanding) {
                return;
            }
            if (state.boundGen === null || discoverOutstanding) {
                return;
            }
            armEffectWaiter();
        });
    };

    // Gesture-facts transport: the explicit override first, else the
    // production default wrapper around the global callDBus (same
    // service/path/iface/method plus one JSON payload string shape as every
    // other route). Null when neither exists: the lane stays unknown and
    // nothing is armed, so offline doubles without a gesture service
    // observe zero lane impact.
    const resolveGestureCallDbus = (): ((
        service: string, path: string, iface: string, method: string,
        payload: string, callback: (reply: unknown) => void,
    ) => void) | null => {
        if (overrides.gestureCallDbus !== undefined) {
            return overrides.gestureCallDbus;
        }
        try {
            if (typeof callDBus !== "function") {
                return null;
            }
            const native = callDBus;
            return (service, path, iface, method, payload, callback) => {
                try {
                    (native as (...a: ReadonlyArray<unknown>) => void)(
                        service, path, iface, method, payload, callback,
                    );
                } catch (error) {
                    void error;
                }
            };
        } catch (error) {
            void error;
            return null;
        }
    };

    // Single held gesture-facts waiter for the bound generation. Terminal
    // snapshots stay down (no re-arm) until the exact core settlement acks
    // them: a terminal completes every waiter immediately, so re-arming
    // would hot-loop the same unacked terminal. Open/idle snapshots re-arm
    // immediately; only changed snapshots invalidate. The 25 s host
    // watchdog drops a killed call to unknown (omit) and re-arms, so an
    // unavailable interval recovers without reset (mapping persists, the
    // broker owns it).
    const armGestureWaiter = (): void => {
        if (stopped || gestureWaiterOutstanding) {
            return;
        }
        if (gestureTerminalPending !== null) {
            return;
        }
        if (state.boundGen === null || state.brokerIncarnation === null || discoverOutstanding) {
            return;
        }
        const gestureCall = resolveGestureCallDbus();
        if (gestureCall === null) {
            return;
        }
        const gen = state.boundGen;
        gestureWaiterNonce += 1;
        const nonce = gestureWaiterNonce;
        gestureWaiterGen = gen;
        gestureWaiterOutstanding = true;
        const request = gestureWaitRequest(
            gestureSnapshot?.instance ?? "",
            gestureSnapshot === null || gestureSnapshot.phase === "idle" ? "0" : gestureSnapshot.serial,
            gestureSnapshot?.revision ?? "0",
            GESTURE_LEASE_MS,
        );
        let settled = false;
        gestureWatchdogCancel = scheduleOnce(WAIT_HOST_DEADLINE_MS, () => {
            if (settled || stopped || nonce !== gestureWaiterNonce) {
                return;
            }
            settled = true;
            gestureWaiterOutstanding = false;
            gestureWaiterGen = null;
            gestureWatchdogCancel = null;
            log("omnitiler:world:gesture-facts-host-timeout recovery=rearm-unknown");
            gestureSnapshot = null;
            state.invalidate();
            scheduleDispatch();
            armGestureWaiter();
        });
        try {
            (gestureCall as NonNullable<WorldEntryOverrides["gestureCallDbus"]>)(
                GESTURE_SERVICE, GESTURE_OBJECT, GESTURE_INTERFACE,
                GESTURE_WAIT_METHOD, request, (reply: unknown) => {
                    if (settled) {
                        return;
                    }
                    if (nonce !== gestureWaiterNonce || gen !== gestureWaiterGen) {
                        settled = true;
                        log("omnitiler:world:gesture-facts-stale recovery=ignore");
                        return;
                    }
                    settled = true;
                    const cancel = gestureWatchdogCancel;
                    gestureWatchdogCancel = null;
                    try {
                        cancel?.();
                    } catch (error) {
                        void error;
                    }
                    onGestureFactsReply(nonce, gen, reply);
                },
            );
        } catch (error) {
            void error;
            settled = true;
            gestureWaiterOutstanding = false;
            gestureWaiterGen = null;
            try {
                gestureWatchdogCancel?.();
            } catch (discardError) {
                void discardError;
            }
            gestureWatchdogCancel = null;
            if (gestureSnapshot !== null) {
                gestureSnapshot = null;
                state.invalidate();
                scheduleDispatch();
            }
            log("omnitiler:world:gesture-facts-transport recovery=host-deadline-unknown");
            scheduleGestureRecovery();
        }
    };
    const onGestureFactsReply = (nonce: number, gen: string, reply: unknown): void => {
        if (stopped) {
            return;
        }
        if (nonce !== gestureWaiterNonce || gen !== gestureWaiterGen) {
            log("omnitiler:world:gesture-facts-stale recovery=ignore");
            return;
        }
        gestureWaiterOutstanding = false;
        gestureWaiterGen = null;
        if (gen !== state.boundGen) {
            log("omnitiler:world:gesture-facts-stale recovery=ignore");
            return;
        }
        const parsed = parseNativeGesture(reply);
        if (parsed === null) {
            log("omnitiler:world:gesture-facts-malformed recovery=host-deadline-unknown");
            gestureSnapshot = null;
            state.invalidate();
            scheduleDispatch();
            scheduleGestureRecovery();
            return;
        }
        if (!parsed.available) {
            log("omnitiler:world:gesture-facts-unavailable recovery=host-deadline-unknown");
            gestureSnapshot = null;
            gestureTerminalPending = null;
            clearTerminalCaptureRetry();
            state.invalidate();
            scheduleDispatch();
            scheduleGestureRecovery();
            return;
        }
        if (parsed.superseded) {
            log("omnitiler:world:gesture-facts-superseded recovery=host-deadline");
            scheduleGestureRecovery();
            return;
        }
        // Every gesture update advances the watermark and dispatches a
        // capture with the fresh cursor so the core owns the next preview.
        // No entry-side motion classification: the native witness
        // distinguishes motion from structure (the facts lane carries each
        // bump), and a superseded in-flight reply may still drive a
        // preview read while the native per-surface gates decide.
        gestureSnapshot = parsed;
        const terminal = parsed.phase === "released" || parsed.phase === "cancelled";
        if (terminal) {
            gestureTerminalPending = { instance: parsed.instance, serial: parsed.serial };
            // Terminals always dispatch (even with changed:0): the waiter
            // never holds past an unacked terminal, so no hot loop is
            // possible and no terminal is ever lost to a quiet flag. The
            // exact pending ack is never abandoned while core-settled: fast
            // retries yield to the host-deadline cadence instead.
            log(`omnitiler:world:gesture-facts-terminal phase=${parsed.phase} serial=${parsed.serial} cursor=${parsed.cursor === null ? "-" : `${parsed.cursor[0]},${parsed.cursor[1]}`}`);
            state.invalidate();
            scheduleDispatch();
            return;
        }
        gestureTerminalPending = null;
        armGestureWaiter();
        if (parsed.changed) {
            // Per-motion changed lines would flood the log on every
            // pointer move: log only identity/phase transitions (open,
            // phase change, new serial), never pure cursor progress. The
            // generation prefix keeps rotations diagnosable.
            const gestureChangeKey =
                `${state.boundGen ?? ""}\u0000${parsed.instance}\u0000${parsed.serial}\u0000${parsed.phase}`;
            if (gestureChangeKey !== lastGestureChangedLog) {
                lastGestureChangedLog = gestureChangeKey;
                log(`omnitiler:world:gesture-facts-changed phase=${parsed.phase} serial=${parsed.serial}`);
            }
            state.invalidate();
            scheduleDispatch();
        }
    };
    const cancelGestureRecovery = (): void => {
        try {
            gestureRetryCancel?.();
        } catch (error) {
            void error;
        }
        gestureRetryCancel = null;
    };
    const scheduleGestureRecovery = (): void => {
        if (stopped || gestureRetryCancel !== null) {
            return;
        }
        gestureRetryCancel = scheduleOnce(WAIT_HOST_DEADLINE_MS, () => {
            gestureRetryCancel = null;
            if (stopped || gestureWaiterOutstanding || gestureTerminalPending !== null) {
                return;
            }
            if (state.boundGen === null || discoverOutstanding) {
                return;
            }
            armGestureWaiter();
        });
    };
    // Exact pending ack plumbing: one outstanding ack, generation- plus
    // nonce-fenced. Retries are idempotent (the native side retires the
    // head terminal on exact {instance, serial} match only; a mismatch is a
    // no-op returning the current snapshot), so a retried ack can never
    // double-retire.
    const cancelGestureAckTimer = (): void => {
        try {
            gestureAckTimeoutCancel?.();
        } catch (error) {
            void error;
        }
        gestureAckTimeoutCancel = null;
    };
    const clearGestureAck = (): void => {
        cancelGestureAckTimer();
        gestureAckPending = null;
    };
    const sendGestureAck = (): void => {
        const ack = gestureAckPending;
        if (ack === null || stopped) {
            return;
        }
        if (state.boundGen === null || state.boundGen !== ack.gen) {
            return;
        }
        // Every send owns a fresh nonce: a superseded attempt's late
        // callback can never adopt beside the live attempt.
        gestureAckNonce += 1;
        ack.nonce = gestureAckNonce;
        const nonce = ack.nonce;
        const gestureCall = resolveGestureCallDbus();
        if (gestureCall === null) {
            noteGestureAckFailed();
            return;
        }
        cancelGestureAckTimer();
        gestureAckTimeoutCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            gestureAckTimeoutCancel = null;
            onGestureAckTimeout(nonce);
        });
        try {
            gestureCall(
                GESTURE_SERVICE, GESTURE_OBJECT, GESTURE_INTERFACE,
                GESTURE_ACK_METHOD, gestureAckRequest(ack.instance, ack.serial), (reply: unknown) => {
                    onGestureAckReply(nonce, reply);
                },
            );
        } catch (error) {
            void error;
            // Synchronous transport throw counts as a failed attempt (never
            // a silent wedge): retry on the timeout cadence while attempts
            // remain, else abandon as unknown.
            cancelGestureAckTimer();
            noteGestureAckFailed();
        }
    };
    // One failed ack attempt (timeout or synchronous throw): retry the
    // exact identity on the fast cadence while attempts remain, then retain
    // the exact pending ack and retry at the host-deadline cadence (never
    // abandoned while core-settled, never a hot loop).
    const noteGestureAckFailed = (): void => {
        const ack = gestureAckPending;
        if (ack === null || stopped) {
            return;
        }
        if (state.boundGen === null || state.boundGen !== ack.gen) {
            return;
        }
        ack.attempts += 1;
        if (ack.attempts > GESTURE_ACK_MAX_ATTEMPTS) {
            scheduleGestureAckHostRetry();
            return;
        }
        cancelGestureAckTimer();
        gestureAckTimeoutCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            gestureAckTimeoutCancel = null;
            onGestureAckTimeout(ack.nonce);
        });
    };
    const onGestureAckTimeout = (nonce: number): void => {
        const ack = gestureAckPending;
        if (ack === null || stopped || nonce !== ack.nonce) {
            return;
        }
        if (state.boundGen === null || state.boundGen !== ack.gen) {
            return;
        }
        log("omnitiler:world:gesture-ack-timeout recovery=retry-bounded");
        ack.attempts += 1;
        if (ack.attempts > GESTURE_ACK_MAX_ATTEMPTS) {
            scheduleGestureAckHostRetry();
            return;
        }
        sendGestureAck();
    };
    // Fast attempts exhausted with no native answer: the native FIFO head
    // still blocks future gestures, so the exact pending ack persists and
    // retries at the existing 25 s host-deadline cadence until the native
    // side retires it. No hot loop, no new capture, no new core terminal.
    const scheduleGestureAckHostRetry = (): void => {
        const ack = gestureAckPending;
        if (ack === null || stopped) {
            return;
        }
        if (state.boundGen === null || state.boundGen !== ack.gen) {
            return;
        }
        log("omnitiler:world:gesture-ack-exhausted recovery=host-deadline-retry");
        cancelGestureAckTimer();
        gestureAckTimeoutCancel = scheduleOnce(WAIT_HOST_DEADLINE_MS, () => {
            gestureAckTimeoutCancel = null;
            const live = gestureAckPending;
            if (live === null || stopped) {
                return;
            }
            if (state.boundGen === null || state.boundGen !== live.gen) {
                return;
            }
            // Host-cadence retry keeps the exact pending identity: attempts
            // stay past the fast bound so every later failure returns here.
            sendGestureAck();
        });
    };
    // Terminal ack reply: adopt the returned next snapshot regardless of
    // the changed flag (the native ack return always carries changed:0
    // even when exposing the next terminal/open/idle) and never lose a
    // native terminal to a quiet flag.
    const onGestureAckReply = (nonce: number, reply: unknown): void => {
        if (stopped) {
            return;
        }
        const ack = gestureAckPending;
        if (ack === null || nonce !== ack.nonce) {
            log("omnitiler:world:gesture-ack-stale recovery=ignore");
            return;
        }
        if (state.boundGen === null || state.boundGen !== ack.gen) {
            log("omnitiler:world:gesture-ack-stale recovery=ignore");
            return;
        }
        clearGestureAck();
        const parsed = parseNativeGesture(reply);
        if (parsed === null) {
            log("omnitiler:world:gesture-ack-malformed recovery=host-deadline-unknown");
            gestureSnapshot = null;
            gestureTerminalPending = null;
            state.invalidate();
            scheduleDispatch();
            scheduleGestureRecovery();
            return;
        }
        if (!parsed.available) {
            log("omnitiler:world:gesture-ack-unavailable recovery=host-deadline-unknown");
            gestureSnapshot = null;
            gestureTerminalPending = null;
            state.invalidate();
            scheduleDispatch();
            scheduleGestureRecovery();
            return;
        }
        if (parsed.superseded) {
            log("omnitiler:world:gesture-ack-superseded recovery=host-deadline");
            gestureTerminalPending = null;
            scheduleGestureRecovery();
            return;
        }
        gestureSnapshot = parsed;
        gestureTerminalPending = null;
        const terminal = parsed.phase === "released" || parsed.phase === "cancelled";
        if (terminal) {
            gestureTerminalPending = { instance: parsed.instance, serial: parsed.serial };
            log(`omnitiler:world:gesture-ack-terminal phase=${parsed.phase} serial=${parsed.serial}`);
            state.invalidate();
            scheduleDispatch();
            return;
        }
        armGestureWaiter();
        log(`omnitiler:world:gesture-ack-next phase=${parsed.phase} serial=${parsed.serial}`);
        state.invalidate();
        scheduleDispatch();
    };
    // Exact-identity ack after the core incorporated the terminal: only the
    // broker-bound association proves the current ingress. The script
    // requires GestureSettled.id === association.coreId AND the flight
    // native pair === association native pair before any ack fires, so a
    // core-retained close settling an unrelated id while the new ingress
    // omitted can never alias the current terminal. At-most-once is
    // core-owned (the core never re-emits a settlement for a consumed
    // identity; the native ack itself retires exactly once on identity
    // match). The proven association is broker-only (never a shared
    // core/protocol field) and rides the exact response replay.
    const ackGestureTerminal = (
        flightGesture: { instance: string; serial: string } | null,
        settledId: unknown,
        assoc: { instance: string; serial: string; coreId: string } | null,
    ): void => {
        if (stopped) {
            return;
        }
        if (typeof settledId !== "string" || settledId === "") {
            return;
        }
        if (assoc === null || settledId !== assoc.coreId) {
            return;
        }
        const pending = gestureTerminalPending;
        if (pending === null || gestureAckPending !== null) {
            return;
        }
        if (
            flightGesture === null ||
            flightGesture.instance !== pending.instance ||
            flightGesture.serial !== pending.serial
        ) {
            return;
        }
        // The flight identity must be the association's proven native pair:
        // an unrelated settled id for another ingress never matches here.
        if (assoc.instance !== pending.instance || assoc.serial !== pending.serial) {
            return;
        }
        const gen = state.boundGen;
        if (gen === null) {
            return;
        }
        gestureAckNonce += 1;
        const nonce = gestureAckNonce;
        gestureAckPending = { instance: pending.instance, serial: pending.serial, gen, nonce, attempts: 1 };
        log(`omnitiler:world:gesture-ack id=${token(settledId)} serial=${pending.serial}`);
        sendGestureAck();
    };

    // Whether the exact settlement for this terminal identity is still
    // owed: the terminal is pending and no ack for it is already underway.
    // An armed ack retires the native FIFO exactly once on identity match,
    // so the capture-lane obligation is discharged while it is in flight.
    const isTerminalSettlementOwedFor = (
        gesture: { instance: string; serial: string } | null,
    ): boolean => {
        if (gesture === null) {
            return false;
        }
        const pending = gestureTerminalPending;
        if (
            pending === null ||
            pending.instance !== gesture.instance ||
            pending.serial !== gesture.serial
        ) {
            return false;
        }
        const ack = gestureAckPending;
        if (ack !== null && ack.instance === pending.instance && ack.serial === pending.serial) {
            return false;
        }
        return true;
    };
    const isTerminalCaptureOwed = (flight: CaptureFlight): boolean =>
        isTerminalSettlementOwedFor(flight.gesture);

    // Ack-only settlement discharge, split from snapshot freshness: proves
    // the broker's bound response settles THIS pending terminal (current
    // generation, current broker incarnation, response source/nativeInvalid
    // equal the flight's original envelope bindings, settled id equals the
    // bound association core id, association and flight native pairs equal
    // the pending terminal) and arms the exact native ack. Applies no
    // setters, consumes no desires, paints nothing: stale snapshots,
    // superseded watermarks, and stale revisions may still discharge the
    // terminal while their observations stay unapplied. Returns true when
    // the settlement is proven (ack armed or already underway).
    const tryDischargeTerminalSettlement = (parsed: WorldReply, flight: CaptureFlight): boolean => {
        if (stopped) {
            return false;
        }
        if (state.boundGen === null || parsed.gen !== state.boundGen) {
            return false;
        }
        if (state.brokerIncarnation === null || parsed.adapter !== state.brokerIncarnation) {
            return false;
        }
        if (parsed.req !== flight.source || parsed.source !== flight.source) {
            return false;
        }
        if (parsed.nativeInvalid !== flight.watermark) {
            return false;
        }
        const settled = parsed.gestureSettled;
        const assoc = parsed.gestureAssoc;
        if (settled === null || settled === undefined) {
            return false;
        }
        if (assoc === null || assoc === undefined) {
            return false;
        }
        if (typeof settled.id !== "string" || settled.id === "" || settled.id !== assoc.coreId) {
            return false;
        }
        const pending = gestureTerminalPending;
        if (pending === null) {
            return false;
        }
        if (assoc.instance !== pending.instance || assoc.serial !== pending.serial) {
            return false;
        }
        if (
            flight.gesture === null ||
            flight.gesture.instance !== pending.instance ||
            flight.gesture.serial !== pending.serial
        ) {
            return false;
        }
        const ack = gestureAckPending;
        if (ack !== null) {
            return ack.instance === pending.instance && ack.serial === pending.serial;
        }
        ackGestureTerminal(
            flight.gesture,
            settled.id,
            { instance: assoc.instance, serial: assoc.serial, coreId: assoc.coreId },
        );
        const armed = gestureAckPending;
        return armed !== null && armed.instance === pending.instance && armed.serial === pending.serial;
    };

    // Terminal take collection for one filed queue-client command. The
    // capture reply never carries the result (requester-only even when the
    // actuator is the issuer): ready carries the core result (possibly null
    // when the snapshot alone is the outcome), failed carries the explicit
    // rejection kind/message. Pending keeps the clientCmd undecided and
    // retries after the next accept; duplicate means bounded eviction (never
    // re-executes). Transport loss keeps the clientCmd undecided: the
    // settlement is re-readable server-side while remembered, so a lost take
    // reply never loses the outcome and retries never re-execute.
    const takeResultFor = (clientCmd: string | null, done: (result: WorldReplyResult | null) => void): void => {
        if (clientCmd === null) {
            done(null);
            return;
        }
        // The take binds the serving generation captured at send: a delayed
        // reply from a superseded generation can never settle (or clear) a
        // newer flight's outcome.
        const takeGen = state.boundGen;
        if (takeGen === null) {
            done(null);
            return;
        }
        const payload = buildTakeRequest(state.client, takeGen, clientCmd);
        if (utf8ByteLength(payload) > WORLD_MAX_REQUEST_BYTES) {
            log("omnitiler:world:take-oversize recovery=on-next-event");
            done(null);
            return;
        }
        let settled = false;
        const timeoutCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            if (!settled) {
                settled = true;
                log("omnitiler:world:take-timeout recovery=on-next-event");
                done(null);
            }
        });
        try {
            (callDbus as NonNullable<WorldEntryOverrides["callDbus"]>)(
                WORLD_DBUS_SERVICE, WORLD_DBUS_OBJECT, WORLD_DBUS_INTERFACE,
                WORLD_TAKE_METHOD, payload, (reply: unknown) => {
                    if (settled) {
                        return;
                    }
                    settled = true;
                    timeoutCancel();
                    if (typeof reply !== "string") {
                        log("omnitiler:world:take-transport recovery=on-next-event");
                        done(null);
                        return;
                    }
                    if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        log("omnitiler:world:take-oversize recovery=on-next-event");
                        done(null);
                        return;
                    }
                    const parsed = parseWorldReply(reply);
                    if (parsed === null) {
                        log("omnitiler:world:take-malformed recovery=on-next-event");
                        done(null);
                        return;
                    }
                    if (isWorldRejection(parsed)) {
                        log(`omnitiler:world:take-rejected kind=${token(parsed.kind)} msg=${token(parsed.message)} recovery=on-next-event`);
                        if (parsed.kind === "generation-mismatch") {
                            // Our take raced a lifetime rotation. When we
                            // have not rotated yet the dead generation
                            // drops (never reminted) and we re-discover;
                            // a delayed callback after our own reset is a
                            // no-op that never clears newer undecided state.
                            if (state.boundGen === takeGen) {
                                dropDeadGenerationIntents();
                                killPending("generation-mismatch");
                                waitHoldForDiscovery = true;
                                wakePendingCapture = false;
                                log("omnitiler:world:generation-mismatch recovery=rediscover");
                                discover();
                            } else {
                                log("omnitiler:world:take-stale recovery=on-next-event");
                            }
                            done(null);
                            return;
                        }
                        if (parsed.gen !== takeGen || parsed.gen !== state.boundGen) {
                            log("omnitiler:world:take-stale recovery=on-next-event");
                            done(null);
                            return;
                        }
                        if (parsed.kind === "unknown-command") {
                            undecidedClientCmd = null;
                        }
                        done(null);
                        return;
                    }
                    if (isWorldQueueReply(parsed)) {
                        log("omnitiler:world:take-malformed recovery=on-next-event");
                        done(null);
                        return;
                    }
                    // Generation gate before accepting any terminal result:
                    // a stale reply from a superseded generation is a no-op
                    // that never clears newer undecided state.
                    const takeReplyGen = (parsed as { gen?: unknown }).gen;
                    if (takeReplyGen !== takeGen || takeReplyGen !== state.boundGen) {
                        log("omnitiler:world:take-stale recovery=on-next-event");
                        done(null);
                        return;
                    }
                    const outcome = (parsed as { outcome?: unknown }).outcome;
                    if (outcome === "pending") {
                        done(null);
                        return;
                    }
                    if (outcome === "ready") {
                        const take = parsed as Extract<WorldTakeReply, { outcome: "ready" }>;
                        if (undecidedClientCmd === clientCmd) {
                            undecidedClientCmd = null;
                        }
                        state.lastTakeResult = take.result;
                        done(take.result);
                        return;
                    }
                    if (outcome === "failed") {
                        const take = parsed as Extract<WorldTakeReply, { outcome: "failed" }>;
                        log(`omnitiler:world:command-failed kind=${token(take.kind)} msg=${token(take.message)} recovery=on-next-event`);
                        if (undecidedClientCmd === clientCmd) {
                            undecidedClientCmd = null;
                        }
                        state.lastTakeResult = null;
                        done(null);
                        return;
                    }
                    if (outcome === "duplicate") {
                        log("omnitiler:world:take-duplicate recovery=on-next-event");
                        if (undecidedClientCmd === clientCmd) {
                            undecidedClientCmd = null;
                        }
                        done(null);
                        return;
                    }
                    log("omnitiler:world:take-malformed recovery=on-next-event");
                    done(null);
                },
            );
        } catch (error) {
            void error;
            timeoutCancel();
            log("omnitiler:world:take-transport recovery=on-next-event");
            done(null);
        }
    };

    // Terminal drop diagnostic: `drop-settled` per proven settlement,
    // `drop-observed` on the next two accepted captures (diagnostic
    // coverage of the two report boundaries, not a settling guarantee).
    // Native-window keyed; core numeric ids never match native windows.
    const logDropDiagnostics = (parsed: WorldReply, flight: CaptureFlight, fresh: WorldObservation): void => {
        const findWin = (win: string): { rect: { x: number; y: number; w: number; h: number }; out: string; ws: string } | null => {
            for (const domain of fresh.domains) {
                for (const w of domain.windows) {
                    if (w.win === win) {
                        return { rect: w.rect, out: w.out, ws: w.ws };
                    }
                }
            }
            return null;
        };
        const pending = dropFollowup;
        if (pending !== null) {
            pending.remaining -= 1;
            if (pending.remaining <= 0) {
                dropFollowup = null;
            }
            const seen = findWin(pending.win);
            // Reply authority for the tracked window beside the fresh
            // pre-apply frame. rev/obs name the reply; the fresh frame is
            // a newer read, not proof it was the staged frame behind a
            // hold. Reported authority only, never a client-commit claim.
            const row = parsed.windows.find((r) => r.win === pending.win) ?? null;
            const transfer = parsed.transfers.find((t) => t.kind === "window" && t.win === pending.win) ?? null;
            log(`omnitiler:world:drop-observed ${JSON.stringify({ serial: pending.serial, instance: pending.instance, win: pending.win, rev: parsed.rev, obs: parsed.obs, desired: row?.alloc ?? null, desiredFrame: row?.frame ?? null, frame_auth: row?.frame_auth ?? null, frame_hold: row?.frame_hold ?? null, member_auth: row?.member_auth ?? null, member_hold: row?.member_hold ?? null, transfer, frame: seen === null ? null : seen.rect, at: seen === null ? null : { out: seen.out, ws: seen.ws } })}`);
        }
        const g = flight.gesture;
        if (g === null || g.win === "") {
            return;
        }
        const settled = parsed.gestureSettled;
        const assoc = parsed.gestureAssoc;
        if (settled === null || settled === undefined || assoc === null || assoc === undefined) {
            return;
        }
        if (typeof settled.id !== "string" || settled.id === "" || settled.id !== assoc.coreId) {
            return;
        }
        if (assoc.instance !== g.instance || assoc.serial !== g.serial) {
            return;
        }
        // Destination desired allocation and frame from the accepted reply
        // row, keyed by the native gesture window.
        let dest: { out: string; ws: string } | null = null;
        let desired: readonly [number, number, number, number] | null = null;
        let desiredFrame: readonly [number, number, number, number] | null = null;
        for (const row of parsed.windows) {
            if (row.win === g.win) {
                dest = { out: row.out, ws: row.ws };
                desired = row.alloc;
                desiredFrame = row.frame;
                break;
            }
        }
        // Pre-actuation observed frame from the fresh capture (taken before
        // apply), keyed the same way.
        const seen = findWin(g.win);
        // Destination work area when cheap.
        let work: { x: number; y: number; w: number; h: number } | null = null;
        if (dest !== null) {
            for (const screen of fresh.screens) {
                if (screen.out === dest.out) {
                    work = screen.work;
                    break;
                }
            }
        }
        log(`omnitiler:world:drop-settled ${JSON.stringify({
            rev: parsed.rev, serial: g.serial, instance: g.instance, win: g.win,
            settled: settled.id, committed: settled.committed,
            dest, desired, desiredFrame,
            observedPre: seen === null ? null : seen.rect,
            observedAt: seen === null ? null : { out: seen.out, ws: seen.ws },
            work,
        })}`);
        dropFollowup = { serial: g.serial, instance: g.instance, win: g.win, remaining: 2 };
    };

    // Post-accept capture handling with the requester-only terminal result:
    // binding convergence, close/float convergence, safe actuation,
    // native toggles, and drain scheduling. Runs after the
    // take collects (chained, never raced).
    const postAcceptCapture = (
        parsed: WorldReply,
        flight: CaptureFlight,
        takeResult: WorldReplyResult | null,
    ): void => {
                    // Proven bound association (broker-only): every accepted
                    // reply carrying one refreshes the read-only identity,
                    // open or terminal, so the NEXT package exposes the
                    // current native identity with its proven core id while
                    // open. Omitted lanes carry null and change nothing.
                    try {
                        const assoc = (parsed as unknown as {
                            gestureAssoc?: { instance?: unknown; serial?: unknown; coreId?: unknown } | null;
                        }).gestureAssoc ?? null;
                        if (
                            assoc !== null && assoc !== undefined &&
                            typeof assoc.instance === "string" &&
                            typeof assoc.serial === "string" &&
                            typeof assoc.coreId === "string" &&
                            state.boundGen !== null
                        ) {
                            gestureBoundAssoc = {
                                instance: assoc.instance,
                                serial: assoc.serial,
                                coreId: assoc.coreId,
                                gen: state.boundGen,
                            };
                        }
                    } catch (error) {
                        void error;
                    }
                    // Exact-identity gesture ack: only the broker-bound
                    // association proves the current ingress (settled id
                    // must equal the association core id and the flight
                    // identity must equal the association native pair).
                    // The terminal carries frozen start facts; the ack
                    // exposes the next snapshot.
                    try {
                        const settled = (parsed as unknown as {
                            gestureSettled?: { id?: unknown; committed?: unknown } | null;
                        }).gestureSettled;
                        const assoc = (parsed as unknown as {
                            gestureAssoc?: { instance?: unknown; serial?: unknown; coreId?: unknown } | null;
                        }).gestureAssoc ?? null;
                        if (settled !== null && settled !== undefined) {
                            ackGestureTerminal(
                                flight.gesture,
                                settled.id,
                                assoc !== null && assoc !== undefined &&
                                typeof assoc.instance === "string" &&
                                typeof assoc.serial === "string" &&
                                typeof assoc.coreId === "string"
                                    ? { instance: assoc.instance, serial: assoc.serial, coreId: assoc.coreId }
                                    : null,
                            );
                        }
                    } catch (error) {
                        void error;
                    }
                    // Converge the adapter-owned binding snapshot from the
                    // accepted Planner mapping (Planner-owned, D2), then
                    // prune to the current native inventory so stale
                    // bindings never survive removals/disconnects. Keyed
                    // per (output, native): the aligned grid binds the same
                    // native once per output with a distinct logical each.
                    for (const ws of parsed.workspaces) {
                        if (ws.bind !== "") {
                            logicalOfNative.set(bindingKey(ws.out, ws.bind), {
                                logical: ws.id !== "" ? ws.id : ws.bind,
                                out: ws.out,
                            });
                            // No adapter-owned pending table remains:
                            // executed creations retire apply-side when the
                            // core publishes the bind
                            // (incorporateCreationBindings); nothing is
                            // consumed here.
                        }
                    }
                    // Ack the closes that rode the accepted dispatch BEFORE the
                    // post-accept fresh capture: that capture may promote new
                    // event-gated closes that have never been reported and
                    // must survive until their own accepted reply.
                    try {
                        closeTracker.ack();
                    } catch (error) {
                        void error;
                    }
                    // Clear settled floats and record unfloat tile overrides
                    // before capture: otherwise stale floating/fitExcluded
                    // flags hold the tile frame until another native event.
                    // Float arrivals stay after capture because explicit
                    // placement needs the pre-command tile classification.
                    // Native overlay reads and safety gates stay unchanged.
                    // Explicit classification only: a floating row with
                    // untouched frame authority keeps intent; unclassified
                    // held/overlay/minimized rows never read as floats
                    // merely for missing allocation.
                    {
                        // Held/unreadable scopes never clear settled intent:
                        // only a positive tile proof for the same live
                        // native scope removes it (floating false with
                        // controlled frame AND membership authority and no
                        // hold), or an explicit successful unfloat below.
                        // Missing rows, held rows, untouched rows (the
                        // fullscreen-overlay tiled shape rides untouched),
                        // overlay lifetimes, minimized lifetimes, and
                        // cross-scope rows all retain.
                        const rowByWin = new Map<string, (typeof parsed.windows)[number]>();
                        for (const row of parsed.windows) {
                            if (!rowByWin.has(row.win)) {
                                rowByWin.set(row.win, row);
                            }
                        }
                        // Reply ws may be logical: resolve (out, ws) to the
                        // native scope through the reply workspace rows,
                        // then the adapter logical snapshot. Unbound rows
                        // resolve to nothing (fail closed); with no mapping
                        // the reply ws reads as the native id.
                        const replyNativeOf = (out: string, ws: string): string | null => {
                            for (const wsRow of parsed.workspaces) {
                                if (wsRow.out === out && wsRow.id === ws) {
                                    return wsRow.bind !== "" ? wsRow.bind : null;
                                }
                            }
                            for (const [key, value] of logicalOfNative) {
                                const split = key.indexOf("\u0000");
                                if (split >= 0 && key.slice(0, split) === out && value.logical === ws) {
                                    return key.slice(split + 1);
                                }
                            }
                            return ws;
                        };
                        for (const id of [...floatingIds]) {
                            const row = rowByWin.get(id);
                            if (row === undefined) {
                                continue;
                            }
                            if (row.floating !== false) {
                                continue;
                            }
                            if (row.frame_auth !== "controlled" || row.member_auth !== "controlled") {
                                continue;
                            }
                            if (row.frame_hold !== "" || row.member_hold !== "") {
                                continue;
                            }
                            // Live overlay/minimized lifetimes retain, as
                            // does any unreadable live read (fail closed).
                            let live: object | null = null;
                            try {
                                live = liveRefById(id);
                            } catch (error) {
                                void error;
                                live = null;
                            }
                            if (live === null) {
                                continue;
                            }
                            let safety: { minimized: boolean | null; fullscreen: boolean | null; maximized: boolean | null; sticky: boolean | null } | null = null;
                            try {
                                safety = applyHostBase.readLiveSafety(live);
                            } catch (error) {
                                void error;
                                safety = null;
                            }
                            if (
                                safety === null || safety.minimized !== false ||
                                safety.fullscreen !== false || safety.maximized !== false ||
                                safety.sticky !== false
                            ) {
                                continue;
                            }
                            let home: { out: string; ws: string } | null = null;
                            try {
                                home = applyHostBase.readLiveMembership(live);
                            } catch (error) {
                                void error;
                                home = null;
                            }
                            if (home === null) {
                                continue;
                            }
                            const nativeWs = replyNativeOf(row.out, row.ws);
                            if (nativeWs === null || nativeWs !== home.ws || row.out !== home.out) {
                                continue;
                            }
                            floatingIds.delete(id);
                        }
                        try {
                            if (
                                takeResult !== null && takeResult.op === "toggle-float" &&
                                takeResult.rect === null && takeResult.noSpace !== true
                            ) {
                                tileIds.add(takeResult.win);
                                floatingIds.delete(takeResult.win);
                                // Explicit unfloat of an automatic float
                                // releases owned keepAbove once (the
                                // suppress pin above holds the tile; never
                                // a keepBelow write). Never throws into
                                // the accept path.
                                try {
                                    native.releaseAutomaticFloat(takeResult.win, "explicit-unfloat");
                                } catch (error) {
                                    void error;
                                }
                            } else if (
                                takeResult !== null && takeResult.op === "toggle-float" &&
                                takeResult.rect === null && takeResult.noSpace === true
                            ) {
                                // Explicit-tile no-space retry (D5): the
                                // window stays floated, so no tile override
                                // is recorded and nothing re-tiles.
                                log(`omnitiler:world:toggle-float-no-space win=${token(takeResult.win)}`);
                            }
                        } catch (error) {
                            void error;
                        }
                    }
                    let fresh = capture();
                    if (fresh === null) {
                        log("omnitiler:world:observe-invalid recovery=on-next-event");
                        return;
                    }
                    // Terminal drop diagnostic: pre-apply, so the observed
                    // frame and source homing are pre-actuation evidence.
                    logDropDiagnostics(parsed, flight, fresh);
                    // Born admission and the one-shot clear are core-owned
                    // (N6): the reply's `clear_operation` desire rides the
                    // normal apply lane below with fresh pre/post proof.
                    // No adapter-side birth decision runs here.
                    {
                        const liveCells = new Set<string>();
                        for (const t of fresh.topology) {
                            for (const id of t.order) {
                                liveCells.add(bindingKey(t.out, id));
                            }
                        }
                        for (const key of [...logicalOfNative.keys()]) {
                            if (!liveCells.has(key)) {
                                logicalOfNative.delete(key);
                            }
                        }
                    }
                    // Add float arrivals after capture so explicit placement
                    // keeps its pre-command classification. Only rows the
                    // core explicitly classifies as ordinary floats
                    // (floating with untouched frame authority, user or
                    // capacity alike) converge intent; minimized lifetimes
                    // count as observed so their membership survives.
                    // Unclassified protected/held/overlay/minimized rows
                    // never relabel merely for missing allocation.
                    {
                        const obsWins = new Set<string>();
                        for (const domain of fresh.domains) {
                            for (const w of domain.windows) {
                                obsWins.add(w.win);
                            }
                        }
                        for (const m of fresh.minimized) {
                            obsWins.add(m.win);
                        }
                        for (const row of parsed.windows) {
                            if (row.floating === true && row.frame_auth === "untouched" && row.frame_hold === "" && obsWins.has(row.win)) {
                                floatingIds.add(row.win);
                            }
                        }
                        // Prune departed lifetimes only on authoritative
                        // complete inventory: any held/unreadable scope, an
                        // incomplete report, or an incomplete topology holds
                        // ALL intent (held omission never proves departure,
                        // so this over-retains departed ids on readable
                        // scopes rather than risk uncommanded loss).
                        // Positively acked closes are not excepted here:
                        // absence under hold cannot distinguish departure
                        // from omission, so they prune on the next complete
                        // inventory instead. Clean close/departure prunes
                        // when the same complete inventory omits the id.
                        const freshHeld = !fresh.complete ||
                            fresh.domains.some((d) => d.hold !== "" || !d.readable || !d.complete) ||
                            fresh.topology.some((t) => !t.complete);
                        if (!freshHeld) {
                            for (const id of [...floatingIds]) {
                                if (!obsWins.has(id)) {
                                    floatingIds.delete(id);
                                }
                            }
                            // Fixed-window tile overrides persist locally: they
                            // are disjoint from floats and pruned only when the
                            // lifetime leaves the inventory.
                            for (const id of [...tileIds]) {
                                if (!obsWins.has(id)) {
                                    tileIds.delete(id);
                                }
                            }
                        }
                    }
                    // Withdraw re-float tile overrides after capture, keeping
                    // fixed-size clients tile-classified for explicit float
                    // placement. Both sets persist through WriteFloatIntent.
                    try {
                        if (
                            takeResult !== null && takeResult.op === "toggle-float" &&
                            takeResult.rect !== null
                        ) {
                            tileIds.delete(takeResult.win);
                            floatingIds.add(takeResult.win);
                        }
                    } catch (error) {
                        void error;
                    }
                    // Presentation invalidation (Stage 5): this accepted capture
                    // advanced the cached projection (or a foreign command
                    // did, drained through this capture). The read-only
                    // subscriber re-reads the projection and paints accepted
                    // core shapes only. Take-lane results never paint: they
                    // are requester-only terminal outcomes, not renderer
                    // authority. The subscriber fences generation, request,
                    // and transport; malformed reads hide.
                    try {
                        presentationSubscriber.notifyInvalidated();
                    } catch (error) {
                        void error;
                    }
                    // Unsupported warp truth: the core may desire a cursor
                    // warp point (cross-output focus), but this adapter
                    // declares cursorWarp false and never actuates the
                    // cursor. Never a write; one bounded truth line.
                    if (parsed.warpCursor !== null) {
                        log(`omnitiler:world:warp-desired ignored x=${parsed.warpCursor[0]} y=${parsed.warpCursor[1]}`);
                    }
                    // Core-published retained-operation settlement rides the
                    // reply for core-side consumers only: the adapter never
                    // mirrors it into local dispatch state. Local dispatch
                    // records clear only on generation rotation; transfers
                    // retire through subsequent replies, and raw attempt
                    // rows plus fresh native facts are the only evidence
                    // the adapter reports. Unknown ops change nothing and
                    // never actuate.
                    // Trace-preserving geometry base: the trace probe wraps
                    // the host setter when armed (one-shot observation), so
                    // the homing suppression below wraps the traced setter
                    // and synchronous own frame echoes still defer.
                    const setGeometryBase = typeof __OMNITILER_TRACE__ === "string" && __OMNITILER_TRACE__ === "1"
                        ? (worldTraceProbes.get(state)?.setGeometryFor(fresh, applyHostBase, token(flight.source)) ?? applyHostBase.setGeometry)
                        : applyHostBase.setGeometry;
                    const applied = applyWorldReply({
                        state,
                        observation: fresh,
                        reply: parsed,
                        dispatchWatermark: flight.watermark,
                        takeResult,
                        log,
                        ...applyHostBase,
                        setGeometry: (ref, rect) =>
                            withHomingSuppression(ref, () => setGeometryBase(ref, rect)),
                        setMembership: (ref, wsNativeId, wsRef) =>
                            withHomingSuppression(ref, () => applyHostBase.setMembership(ref, wsNativeId, wsRef)),
                        sendToOutput: (ref, outRef) =>
                            withHomingSuppression(ref, () => applyHostBase.sendToOutput(ref, outRef)),
                    });
                    // Authorized maximize actuation: the take result for this
                    // exact flight's clientCmd is the settlement of the
                    // command this flight filed (the head was consumed at
                    // file-send, so no later reply can replay it; the take
                    // lane already fenced generation/revision/input staleness).
                    // Only a maximize result on a command-carrying flight
                    // actuates, against the exact result window under fresh
                    // native fences inside. Pending native settlement polls
                    // BEFORE issuance so the same accepted capture never
                    // confirms the attempt it just armed: only the next
                    // fresh accepted boundary settles it.
                    try {
                        native.pollPendingNativeState(fresh, gestureActive());
                    } catch (error) {
                        void error;
                    }
                    try {
                        if (flight.clientCmd !== null && takeResult !== null && takeResult.op === "maximize") {
                            if (!native.applyAuthorizedMaximize(takeResult)) {
                                shortcutFailed("maximize");
                            }
                        }
                    } catch (error) {
                        void error;
                    }
                    // Terminal drop-attempt diagnostic (tracked window only):
                    // pending-report rows for the NEXT report. The accepted
                    // ack retires only exactly-equal delivered rows, so no
                    // origin beyond pending is claimed. Silent when empty.
                    try {
                        const tracked = dropFollowup;
                        if (tracked !== null) {
                            const frames = state.framesForReport().filter((a) => a.win === tracked.win);
                            const members = state.memberAttemptsForReport().filter((a) => a.win === tracked.win);
                            if (frames.length > 0 || members.length > 0) {
                                log(`omnitiler:world:drop-attempt ${JSON.stringify({ rev: parsed.rev, serial: tracked.serial, instance: tracked.instance, win: tracked.win, evidence: "pending-report", frames, members })}`);
                            }
                        }
                    } catch (error) {
                        void error;
                    }
                    // Deferred own-echo observation: synchronous homing
                    // echoes of the setters above coalesced here into one
                    // post-apply invalidation (never mid-apply).
                    if (deferredOwnHoming && !stopped) {
                        deferredOwnHoming = false;
                        onNativeInvalidation();
                    }
                    // Automatic fixed-size float stacking (option A):
                    // project-owned keepAbove above tiles from the fresh
                    // capture, with no geometry, focus, or keepBelow
                    // writes. Never throws into the accept path.
                    try {
                        native.syncAutomaticFloatStacking(fresh);
                    } catch (error) {
                        void error;
                    }
                    // Persist settled intentional-float membership (native
                    // only): the accepted exception rows already converged
                    // floatingIds above; a change writes through.
                    persistFloatIntent();
                    log(`omnitiler:world:applied rev=${token(parsed.rev)} geom=${applied.geometryWrites} member=${applied.membershipWrites} views=${applied.viewWrites} mk=${applied.creations} rm=${applied.removals} fx=${applied.effects} held=${applied.held}`);
                    // B9 single-observation boundary: the first accepted
                    // full-state report after the issued clear decides the
                    // deferred unfloat exactly once (complete or narrow
                    // cancel). Full-state remains the policy path: this
                    // only submits a core command, never tiles. Scope and
                    // gesture come from the actual current capture.
                    try {
                        native.pollPendingUnfloat(fresh, gestureActive());
                    } catch (error) {
                        void error;
                    }
                    if (overrides.onWorkspaceTilingChanged !== undefined) {
                        try {
                            overrides.onWorkspaceTilingChanged(getWorkspaceTilingSnapshot());
                        } catch (error) {
                            void error;
                        }
                    }
                    // Dirty work coalesces to the newest full report on the
                    // native debounce; queued directionals drain zero-delay
                    // instead (sync effects already ran, so the next-tick
                    // capture is fresh). An owed wake drains even from a
                    // clean accept with an empty local queue: the work is
                    // foreign (another sender queued it).
                    if (pendingQueue.length > 0) {
                        scheduleDrain();
                    } else if (state.dirty) {
                        scheduleDispatch();
                    }
                    wakeFollowup();
    };

    // Bounded same-envelope terminal capture retry (finding A): a
    // commandless terminal flight whose reply is lost (timeout, transport,
    // malformed, oversize) keeps its immutable terminal/source identity and
    // redelivers the BYTE-IDENTICAL envelope on a bounded cadence. The
    // broker replays the remembered bound response (with its exact settled
    // outcome) when the first delivery already processed, or processes the
    // envelope fresh when it was lost before processing. Fresh-source
    // re-execution alone could never recover the settlement (consumed core
    // ids never re-emit), so the retry never mints a new source and never
    // infers a new terminal: it fires only while the same terminal is
    // still pending and no newer native input arrived. No double commit
    // (server-side dedup by source), no unrelated window events needed.
    //
    // Timer justification (genuine recovery bounds, never causality
    // inference): the native terminal is already frozen upstream and the
    // envelope bytes are immutable; the timers below bound only the loss of
    // the transport callback itself (a killed callDBus never invokes its
    // callback, mirroring the 25 s host kill documented at
    // WAIT_HOST_DEADLINE_MS). Elapsed time never proves the native
    // finished, cancelled, went fresh, or settled: settlement is proven
    // only by the broker's bound response (exact gestureSettled plus the
    // broker-bound gestureAssoc) and retirement only by the native ack.
    // The 2 s fast cadence matches the existing capture timeout
    // (WORLD_TIMEOUT_MS) and fires at most TERMINAL_CAPTURE_MAX_RETRIES
    // times; the 25 s cadence reuses the existing host deadline
    // (WAIT_HOST_DEADLINE_MS) and persists the exact pending envelope
    // until the same terminal settles, rotates, or stops. No busy loop:
    // at most one retry timer lives at a time.
    let terminalCaptureRetry: {
        envelope: string; source: string; watermark: string;
        gesture: { instance: string; serial: string; win: string }; attempts: number;
        timerCancel: (() => void) | null;
    } | null = null;
    const TERMINAL_CAPTURE_MAX_RETRIES = 2;
    // Last envelope source resolved by a VALID definitive broker reply
    // (accepted, stale-but-parsed, or in-band rejection). Transport loss
    // and invalid data (non-string, oversize, malformed) never resolve
    // the replay obligation: the same terminal stays pending and the
    // bounded cadence above still recovers it. Reset on generation
    // rotation and stop.
    let lastResolvedCaptureSource: string | null = null;
    const clearTerminalCaptureRetry = (): void => {
        try {
            terminalCaptureRetry?.timerCancel?.();
        } catch (error) {
            void error;
        }
        terminalCaptureRetry = null;
    };

    // One capture flight: the inner report is ALWAYS commandless (the broker
    // stamps the drained queued command itself). Fresh-source correlation
    // and the CURRENT local watermark bind the envelope; the callback binds
    // generation/incarnation/source/watermark before any setter.
    const sendCapture = (observation: WorldObservation, headOp: string | null, clientCmd: string | null): void => {
        if (state.boundGen === null || state.brokerIncarnation === null) {
            return;
        }
        // Flush deferred creation evidence first: a spent desire whose
        // same-turn post-call readback was unavailable must not wait for
        // another apply. The first successful complete readback completes
        // the receipt once, uncorrelated (created null); an unavailable
        // read preserves the evidence silently for a later capture.
        if (state.creationPending.size > 0) {
            let order: Array<{ readonly id: string; readonly name: string }> | null = null;
            try {
                order = applyHostBase.listDesktops?.() ?? null;
            } catch (error) {
                void error;
                order = null;
            }
            if (order !== null) {
                state.completeDeferredCreations(order.map((d) => d.id));
            }
        }
        const inner = buildWorldInnerReportJson({
            observation,
            defaultTiled: settings.readDefaultTiled(),
            refill: settings.readRefillLive(),
            bornMaximized: settings.readBornMaximizedLive(),
            anchorFallback: settings.readAnchorFallbackLive(),
            noSpaceResult: settings.readNoSpaceResultLive(),
            creationAttempts: state.creationAttemptsForReport(),
            frameAttempts: state.framesForReport(),
            memberAttempts: state.memberAttemptsForReport(),
            viewAttempts: state.viewAttemptsForReport(),
            clearAttempts: state.clearAttemptsForReport(),
        });
        const source = mintSource();
        const seq = mintSourceSeq();
        if (seq === null) {
            log("omnitiler:world:command-refused op=observe reason=seq-exhausted");
            return;
        }
        const boundGen = state.boundGen;
        if (boundGen === null) {
            return;
        }
        const envelope = buildCaptureEnvelope(adapter, boundGen, source, seq, state.currentInvalid, inner);
        if (utf8ByteLength(envelope) > WORLD_MAX_REQUEST_BYTES) {
            // Narrow overflow recovery: the filed head is already consumed
            // (never replayed); keep the queued remainder and the
            // observation dirty for a smaller scope on the next event.
            log("omnitiler:world:request-oversize recovery=on-next-event");
            state.noteRejectedOrTransportError();
            killPending("rejected");
            drainAfterFailure({ clientCmd });
            return;
        }
        const flight = state.beginCapture(source, clientCmd);
        if (flight === null) {
            return;
        }
        // Stamp the exact raw terminal identity this capture carries (if
        // any): the later accept binds its GestureSettled to this flight
        // identity before any ack may fire. The native window freezes here
        // too: the settled id is numeric, never a native row key.
        try {
            const raw = observation.nativeGesture ?? null;
            flight.gesture = (
                raw !== null && raw.available &&
                (raw.phase === "released" || raw.phase === "cancelled")
            )
                ? { instance: raw.instance, serial: raw.serial, win: raw.win }
                : null;
        } catch (error) {
            void error;
        }
        // A fresh terminal send supersedes any pending same-envelope retry
        // only for a NEW terminal: a same-identity re-carry keeps the
        // oldest unrecovered envelope owning recovery (only the original
        // source can retrieve the broker's cached settlement; a fresh
        // source never re-settles a consumed terminal).
        if (flight.gesture !== null) {
            const pending = gestureTerminalPending;
            if (
                pending === null ||
                pending.instance !== flight.gesture.instance ||
                pending.serial !== flight.gesture.serial
            ) {
                clearTerminalCaptureRetry();
            }
        }
        transmitCapture(envelope, flight, headOp, observation);
    };

    // Bounded same-envelope retry arming for a failed commandless terminal
    // send. Fires only while the same terminal settlement is still owed
    // and the wire is free. Fast failures redeliver the byte-identical
    // envelope at most TERMINAL_CAPTURE_MAX_RETRIES times (2 s cadence);
    // further failures persist the exact pending envelope at the existing
    // 25 s host-deadline cadence until the same terminal settles, rotates,
    // or stops (never a new source, never a new terminal, never a silent
    // give-up). Oldest-envelope ownership: an entry armed for the same
    // terminal identity keeps governing (a newer re-carry proves nothing
    // new and can never retrieve the cached settlement); only an unowned
    // identity arms a fresh schedule. Attempt accounting rides the owned
    // envelope: the timer holds the count as an in-flight marker
    // (timerCancel null) so the redelivery's own failure increments
    // instead of restarting at 1.
    const fireTerminalRetry = (
        saved: { envelope: string; source: string; watermark: string; gesture: { instance: string; serial: string; win: string }; attempts: number },
        cadenceMs: number,
    ): void => {
        if (stopped) {
            return;
        }
        if (lastResolvedCaptureSource === saved.source) {
            terminalCaptureRetry = null;
            return;
        }
        if (state.boundGen === null) {
            return;
        }
        // The terminal moved on (settled, acked, or superseded): the owned
        // envelope is dead. Otherwise the replay fires regardless of the
        // current watermark: the bytes name the ORIGINAL fence and the
        // replay flight below rebinds it exactly, so a bound old reply can
        // still discharge the settlement while the snapshot stays stale.
        if (!isTerminalSettlementOwedFor(saved.gesture)) {
            terminalCaptureRetry = null;
            return;
        }
        if (state.inFlight !== null) {
            // Wire busy: re-arm the same cadence (count unchanged, at most
            // one timer) so the exact envelope is never silently dropped.
            // A newer fresh reply resolving this source still cancels below.
            clearTerminalCaptureRetry();
            terminalCaptureRetry = {
                envelope: saved.envelope, source: saved.source, watermark: saved.watermark,
                gesture: saved.gesture, attempts: saved.attempts,
                timerCancel: scheduleOnce(cadenceMs, () => {
                    const pending = terminalCaptureRetry;
                    if (pending === null || stopped) {
                        return;
                    }
                    fireTerminalRetry(
                        {
                            envelope: pending.envelope, source: pending.source,
                            watermark: pending.watermark, gesture: pending.gesture,
                            attempts: pending.attempts,
                        },
                        cadenceMs,
                    );
                }),
            };
            return;
        }
        // The count crosses the flight: the marker below (timerCancel null)
        // preserves it so the redelivery's own failure increments.
        terminalCaptureRetry = {
            envelope: saved.envelope, source: saved.source, watermark: saved.watermark,
            gesture: saved.gesture, attempts: saved.attempts, timerCancel: null,
        };
        // The replay flight rebinds the ORIGINAL envelope watermark (the
        // bytes carry the old fence): beginCapture would stamp the current
        // fence instead, which would weaken the source/gen binding proof.
        const retryFlight = state.beginReplayCapture(saved.source, saved.watermark, null);
        if (retryFlight === null) {
            // Wire raced busy after the guard: re-arm the same cadence so
            // the exact envelope is not silently dropped. The count is
            // unchanged (no new attempt fired). At most one timer lives.
            terminalCaptureRetry.timerCancel = scheduleOnce(cadenceMs, () => {
                const pending = terminalCaptureRetry;
                if (pending === null || stopped) {
                    return;
                }
                fireTerminalRetry(
                    {
                        envelope: pending.envelope, source: pending.source,
                        watermark: pending.watermark, gesture: pending.gesture,
                        attempts: pending.attempts,
                    },
                    cadenceMs,
                );
            });
            return;
        }
        retryFlight.gesture = {
            instance: saved.gesture.instance, serial: saved.gesture.serial, win: saved.gesture.win,
        };
        transmitCapture(saved.envelope, retryFlight, null, null);
    };
    const maybeRetryTerminalCapture = (envelope: string, flight: CaptureFlight): void => {
        if (stopped || flight.clientCmd !== null || flight.gesture === null) {
            return;
        }
        if (lastResolvedCaptureSource === flight.source) {
            return;
        }
        const pending = gestureTerminalPending;
        if (
            pending === null ||
            pending.instance !== flight.gesture.instance ||
            pending.serial !== flight.gesture.serial
        ) {
            return;
        }
        if (state.boundGen === null || state.inFlight !== null) {
            return;
        }
        const prev = terminalCaptureRetry;
        let attempts: number;
        if (prev !== null && prev.source === flight.source) {
            // The owned envelope failed again: its own count increments
            // toward the host-deadline escalation below.
            attempts = prev.attempts + 1;
        } else if (
            prev !== null &&
            prev.gesture.instance === flight.gesture.instance &&
            prev.gesture.serial === flight.gesture.serial
        ) {
            // A newer re-carry failed while the oldest envelope is owned:
            // the oldest keeps governing (only it can retrieve the cached
            // settlement), so this failure schedules nothing new.
            return;
        } else {
            attempts = 1;
        }
        const gesture = {
            instance: flight.gesture.instance, serial: flight.gesture.serial, win: flight.gesture.win,
        };
        if (attempts > TERMINAL_CAPTURE_MAX_RETRIES) {
            // Fast bound reached: the exact pending envelope persists and
            // retries at the existing host-deadline cadence (never a hot
            // loop, never abandoned while core-settled). Each host failure
            // re-arms exactly one host timer below via the same path.
            clearTerminalCaptureRetry();
            log(`omnitiler:world:terminal-retry attempt=${attempts} recovery=host-deadline`);
            const saved = { envelope, source: flight.source, watermark: flight.watermark, gesture, attempts };
            const timerCancel = scheduleOnce(WAIT_HOST_DEADLINE_MS, () => {
                fireTerminalRetry(saved, WAIT_HOST_DEADLINE_MS);
            });
            terminalCaptureRetry = {
                envelope, source: flight.source, watermark: flight.watermark,
                gesture, attempts, timerCancel,
            };
            return;
        }
        clearTerminalCaptureRetry();
        log(`omnitiler:world:terminal-retry attempt=${attempts} recovery=same-envelope`);
        const saved = { envelope, source: flight.source, watermark: flight.watermark, gesture, attempts };
        const timerCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            fireTerminalRetry(saved, WORLD_TIMEOUT_MS);
        });
        terminalCaptureRetry = {
            envelope, source: flight.source, watermark: flight.watermark,
            gesture, attempts, timerCancel,
        };
    };

    // Transmit one capture envelope with its bound flight: timeout plus the
    // full DescribeWorld reply handling. Shared by fresh sends and by the
    // bounded same-envelope terminal retry above.
    const transmitCapture = (envelope: string, flight: CaptureFlight, headOp: string | null, observation: WorldObservation | null): void => {
        if (stopped || state.boundGen === null || state.brokerIncarnation === null) {
            return;
        }
        // A fresh capture consumes any owed wake: it drains at most one
        // head server-side (or observes when the queue is empty).
        wakePendingCapture = false;
        let settled = false;
        const timeoutCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            if (!settled) {
                settled = true;
                log("omnitiler:world:timeout recovery=on-next-event");
                // The filed head is never replayed; queued tails drain on
                // the scheduled catchup below, never on a blind retry. A
                // lost commandless terminal redelivers its identical
                // envelope on a bounded cadence instead.
                state.noteRejectedOrTransportError();
                killPending("timeout");
                drainAfterFailure(flight);
                maybeRetryTerminalCapture(envelope, flight);
                wakeFollowup();
            }
        });
        log(`omnitiler:world:dispatch src=${token(flight.source)} nwinv=${flight.watermark} cmd=${headOp === null ? "observe" : token(headOp)}`);
        if (typeof __OMNITILER_TRACE__ === "string" && __OMNITILER_TRACE__ === "1") {
            worldTraceProbes.get(state)?.noteSend(token(flight.source), observation);
        }
        try {
            (callDbus as NonNullable<WorldEntryOverrides["callDbus"]>)(
                WORLD_DBUS_SERVICE, WORLD_DBUS_OBJECT, WORLD_DBUS_INTERFACE,
                WORLD_DESCRIBE_METHOD, envelope, (reply: unknown) => {
                    if (settled) {
                        return;
                    }
                    settled = true;
                    timeoutCancel();
                    if (typeof __OMNITILER_TRACE__ === "string" && __OMNITILER_TRACE__ === "1") {
                        worldTraceProbes.get(state)?.noteReply(token(flight.source));
                    }
                    if (typeof reply !== "string") {
                        log("omnitiler:world:transport-error recovery=on-next-event");
                        state.noteRejectedOrTransportError();
                        killPending("transport");
                        drainAfterFailure(flight);
                        maybeRetryTerminalCapture(envelope, flight);
                        wakeFollowup();
                        return;
                    }
                    if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        log("omnitiler:world:reply-oversize recovery=on-next-event");
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure(flight);
                        maybeRetryTerminalCapture(envelope, flight);
                        wakeFollowup();
                        return;
                    }
                    const parsed = parseWorldReply(reply);
                    if (parsed === null) {
                        log("omnitiler:world:reply-malformed recovery=on-next-event");
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure(flight);
                        maybeRetryTerminalCapture(envelope, flight);
                        wakeFollowup();
                        return;
                    }
                    // Only a valid definitive broker reply resolves the
                    // replay obligation: transport loss and invalid data
                    // above retain it (the same terminal stays pending and
                    // the bounded cadence still recovers it). A late
                    // timeout for a resolved source never re-arms. Wrong-lane
                    // and binding-mismatch replies below resolve nothing.
                    if (isWorldRejection(parsed)) {
                        // An in-band rejection is a definitive broker verdict
                        // for this source: the capture was refused (never
                        // processed), so a fresh re-carry can still settle
                        // the terminal event-driven. The replay obligation
                        // for this source resolves with the verdict.
                        lastResolvedCaptureSource = flight.source;
                        if (parsed.kind === "generation-mismatch") {
                            // Poison/restart: queued intents belong to the
                            // dead generation and drop with one diagnostic;
                            // the filed head is never replayed. Re-discover
                            // (fresh handshake); recovery then waits for the
                            // next native event or press, never a retry.
                            dropDeadGenerationIntents();
                            log("omnitiler:world:generation-mismatch recovery=rediscover");
                            state.inFlight = null;
                            killPending("generation-mismatch");
                            waitHoldForDiscovery = true;
                            wakePendingCapture = false;
                            discover();
                            return;
                        }
                        if (parsed.kind === "busy" || parsed.kind === "unavailable") {
                            state.noteRejectedOrTransportError();
                            killPending("busy");
                            drainAfterFailure(flight);
                            wakeFollowup();
                            return;
                        }
                        log(`omnitiler:world:rejected kind=${token(parsed.kind)} msg=${token(parsed.message)} recovery=on-next-event`);
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure(flight);
                        wakeFollowup();
                        return;
                    }
                    if (!isCoreWorldReply(parsed)) {
                        // Queue/take/presentation lanes never arrive on the
                        // capture callback: a wrong-lane payload is
                        // malformed-class (never a resolution, never an
                        // actuation). The same terminal stays owed and the
                        // bounded same-envelope cadence still recovers it.
                        log("omnitiler:world:reply-malformed recovery=on-next-event");
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure(flight);
                        maybeRetryTerminalCapture(envelope, flight);
                        wakeFollowup();
                        return;
                    }
                    // Previous accepted snapshot is the accepted-reply cache
                    // in adapter state; the core owns admitted-lifetime
                    // tracking across generation rebinds.
                    const verdict = state.acceptReply(parsed, flight.source, flight.watermark);
                    if (!verdict.applied) {                        log(`omnitiler:world:stale reason=${token(verdict.reason)} recovery=catchup`);
                        // Ack-only discharge first: a bound old reply can
                        // still carry the exact settlement for the pending
                        // terminal even when the snapshot is stale. Only a
                        // discharged (or unowed) terminal resolves the
                        // replay obligation; otherwise the oldest exact
                        // envelope persists on its bounded cadence.
                        const discharged = tryDischargeTerminalSettlement(parsed, flight);
                        // The filed head is never replayed. Consume exactly
                        // the flight this callback owns: the stale record
                        // otherwise wedges every later dispatch
                        // (single-flight busy forever) even though its
                        // timeout already settled. Dirty closes and the
                        // unsent queued remainder are retained; the
                        // redispatch below drains the queue after it.
                        state.releaseFlight(flight.source);
                        if (discharged || !isTerminalCaptureOwed(flight)) {
                            lastResolvedCaptureSource = flight.source;
                            clearTerminalCaptureRetry();
                        } else {
                            maybeRetryTerminalCapture(envelope, flight);
                        }
                        if (state.dirty || pendingQueue.length > 0) {
                            scheduleDispatch();
                        }
                        wakeFollowup();
                        return;
                    }
                    // Ack-only settlement discharge before the freshness
                    // split: a bound reply carrying the exact settlement for
                    // the pending terminal retires the native FIFO even when
                    // the snapshot is superseded. Only a discharged (or
                    // unowed) terminal resolves the replay obligation; an
                    // accepted-but-unsettled terminal keeps the oldest exact
                    // envelope on its bounded cadence (fresh sources never
                    // re-settle a consumed terminal).
                    const discharged = tryDischargeTerminalSettlement(parsed, flight);
                    if (discharged || !isTerminalCaptureOwed(flight)) {
                        lastResolvedCaptureSource = flight.source;
                        clearTerminalCaptureRetry();
                    } else {
                        maybeRetryTerminalCapture(envelope, flight);
                    }
                    if (state.currentInvalid !== flight.watermark) {
                        // Native input landed during flight: the drained
                        // command already executed server-side (collect its
                        // terminal result), but this observation is
                        // superseded, so no setters run on it. Follow-up
                        // observes the newer watermark (dirty is set).
                        log("omnitiler:world:stale reason=superseded-during-flight recovery=catchup");
                        // Active-drag preview continuity: a superseded
                        // accepted reply still drives a preview-only
                        // presentation read. The broker already cached this
                        // accepted reply, so the read serves its latest
                        // completed preview; the native per-surface gates
                        // (exact monotonic cover for borders/underlay, the
                        // structural floor plus live producer for the
                        // same-gesture preview) decide. Structurally-stale
                        // replies are rejected natively with no display
                        // touch. Actuation stays strict: no setters run on
                        // this path either way.
                        try {
                            presentationSubscriber.notifyInvalidated();
                        } catch (error) {
                            void error;
                        }
                        takeResultFor(flight.clientCmd, () => {
                            if (state.dirty || pendingQueue.length > 0) {
                                scheduleDispatch();
                            }
                            wakeFollowup();
                        });
                        return;
                    }
                    takeResultFor(flight.clientCmd, (takeResult) => {
                        if (parsed.gen !== state.boundGen) {
                            // The take outlived its generation (a delayed
                            // reply after rotation): never apply stale
                            // actuation or terminal results. The undecided
                            // and result updates already no-op inside the
                            // take lane; the stream continues from the
                            // current generation.
                            log("omnitiler:world:take-stale recovery=catchup");
                            if (state.dirty || pendingQueue.length > 0) {
                                scheduleDispatch();
                            }
                            wakeFollowup();
                            return;
                        }
                        postAcceptCapture(parsed, flight, takeResult);
                    });
                },
            );
        } catch (error) {
            void error;
            timeoutCancel();
            state.noteRejectedOrTransportError();
            killPending("transport");
            drainAfterFailure(flight);
            maybeRetryTerminalCapture(envelope, flight);
            wakeFollowup();
        }
    };

    // One queue filing: ships one QueueWorldCommand payload, then drains it
    // with a fresh commandless capture. Shared by fresh heads and by
    // filed-but-unacked redelivery (identical bytes/id). Transport loss
    // retains the exact unresolved identity in unackedQueue for the next
    // event-driven dispatch; in-band refusal proves the command never
    // filed and requeues the head at front instead. filedGen is the serving
    // generation captured at send: a delayed reply from a superseded
    // generation is a no-op that never touches newer queue state.
    const fileQueue = (
        head: QueuedIntent,
        clientCmd: string,
        queuePayload: string,
        observation: WorldObservation,
        mark: string,
        filedGen: string,
    ): void => {
        const headOp = (head.command as { op?: unknown }).op;
        let qsettled = false;
        const qtimeoutCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            if (!qsettled) {
                qsettled = true;
                log("omnitiler:world:queue-timeout recovery=on-next-event");
                unackedQueue = { head, clientCmd, payload: queuePayload };
                state.noteRejectedOrTransportError();
                killPending("timeout");
                drainAfterFailure({ clientCmd });
                wakeFollowup();
            }
        });
        log(`omnitiler:world:queue clientCmd=${token(clientCmd)} op=${token(headOp)}`);
        try {
            (callDbus as NonNullable<WorldEntryOverrides["callDbus"]>)(
                WORLD_DBUS_SERVICE, WORLD_DBUS_OBJECT, WORLD_DBUS_INTERFACE,
                WORLD_QUEUE_METHOD, queuePayload, (reply: unknown) => {
                    if (qsettled) {
                        return;
                    }
                    qsettled = true;
                    qtimeoutCancel();
                    if (typeof reply !== "string") {
                        log("omnitiler:world:queue-transport recovery=on-next-event");
                        unackedQueue = { head, clientCmd, payload: queuePayload };
                        state.noteRejectedOrTransportError();
                        killPending("transport");
                        drainAfterFailure({ clientCmd });
                        wakeFollowup();
                        return;
                    }
                    if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        log("omnitiler:world:queue-oversize recovery=on-next-event");
                        unackedQueue = { head, clientCmd, payload: queuePayload };
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure({ clientCmd });
                        wakeFollowup();
                        return;
                    }
                    const parsed = parseWorldReply(reply);
                    if (parsed === null || !isWorldQueueReply(parsed)) {
                        if (parsed !== null && isWorldRejection(parsed)) {
                            if (parsed.kind === "generation-mismatch") {
                                // Our filing raced a lifetime rotation: the
                                // command never filed. When we have not
                                // rotated yet the dead generation drops
                                // (never reminted) and we re-discover; a
                                // delayed callback after our own reset is a
                                // no-op that never touches newer queue state.
                                if (state.boundGen === filedGen) {
                                    dropDeadGenerationIntents();
                                    killPending("generation-mismatch");
                                    waitHoldForDiscovery = true;
                                    wakePendingCapture = false;
                                    log("omnitiler:world:generation-mismatch recovery=rediscover");
                                    discover();
                                } else {
                                    log("omnitiler:world:queue-stale recovery=on-next-event");
                                }
                                return;
                            }
                            if (parsed.gen !== filedGen || parsed.gen !== state.boundGen) {
                                log("omnitiler:world:queue-stale recovery=on-next-event");
                                return;
                            }
                            // In-band refusal proves the command never
                            // filed: requeue the head at front (never a
                            // replay, the server holds nothing) and wait
                            // for the next native event. Never a retry
                            // loop: no schedule here.
                            log(`omnitiler:world:queue-rejected kind=${token(parsed.kind)} msg=${token(parsed.message)} recovery=on-next-event`);
                            unackedQueue = null;
                            pendingQueue.unshift(head);
                            state.noteRejectedOrTransportError();
                            killPending("rejected");
                            wakeFollowup();
                            return;
                        }
                        log("omnitiler:world:queue-malformed recovery=on-next-event");
                        unackedQueue = { head, clientCmd, payload: queuePayload };
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure({ clientCmd });
                        wakeFollowup();
                        return;
                    }
                    // Filed (or duplicate of an already-filed command, which
                    // never re-executes): a fresh capture drains exactly one
                    // FIFO command. Our own queue wakes our own capture
                    // directly (no D-Bus signal subscription exists on the
                    // scripting surface); foreign wakes drain on the next
                    // native-event capture. The ambiguity is resolved: the
                    // filed identity is acked, never retained. The reply
                    // must bind the filed generation and identity: a stale
                    // reply from a superseded generation (or for another
                    // command) is a no-op that never clears newer undecided
                    // state.
                    if (
                        parsed.gen !== filedGen || parsed.gen !== state.boundGen ||
                        parsed.client !== state.client || parsed.clientCmd !== clientCmd
                    ) {
                        log("omnitiler:world:queue-stale recovery=on-next-event");
                        return;
                    }
                    unackedQueue = null;
                    undecidedClientCmd = clientCmd;
                    if (state.currentInvalid !== mark) {
                        const fresh = capture();
                        if (fresh === null) {
                            log("omnitiler:world:observe-invalid recovery=on-next-event");
                            state.noteRejectedOrTransportError();
                            killPending("rejected");
                            drainAfterFailure({ clientCmd });
                            return;
                        }
                        sendCapture(fresh, typeof headOp === "string" ? headOp : null, clientCmd);
                        return;
                    }
                    sendCapture(observation, typeof headOp === "string" ? headOp : null, clientCmd);
                },
            );
        } catch (error) {
            void error;
            qtimeoutCancel();
            log("omnitiler:world:queue-transport recovery=on-next-event");
            unackedQueue = { head, clientCmd, payload: queuePayload };
            state.noteRejectedOrTransportError();
            killPending("transport");
            drainAfterFailure({ clientCmd });
            wakeFollowup();
        }
    };

    const dispatch = (isCatchup: boolean): void => {
        void isCatchup;
        if (stopped || state.boundGen === null || state.brokerIncarnation === null || discoverOutstanding) {
            return;
        }
        if (state.inFlight !== null) {
            return;
        }
        const observation = capture();
        if (observation === null) {
            log("omnitiler:world:observe-invalid recovery=on-next-event");
            return;
        }
        const mark = state.currentInvalid;
        // Filed-but-unacked redelivery first: the identical queue bytes/id
        // resolve the ambiguity (the broker high-water dedups, never
        // re-executes). Fresh heads wait behind it in order.
        const filedGen = state.boundGen;
        if (filedGen === null) {
            return;
        }
        if (unackedQueue !== null) {
            const redeliver = unackedQueue;
            log(`omnitiler:world:queue-redeliver clientCmd=${token(redeliver.clientCmd)}`);
            fileQueue(redeliver.head, redeliver.clientCmd, redeliver.payload, observation, mark, filedGen);
            return;
        }
        // Observation-only catchup withholds the queue without dropping it:
        // unsent intents stay queued and file after the catchup report is
        // accepted. A tiled focus or move head rebinds its subject at
        // dispatch to the current native focus (invalid subjects drop
        // narrowly with no native writes); float-origin focus fallbacks keep
        // press-time identity. The head shifts at file-send, so binding
        // retention still sees the riding intent during capture, exactly
        // like the old single pending slot.
        const head = resolveHeadCommand(observation);
        if (head === null) {
            sendCapture(observation, null, null);
            return;
        }
        const clientCmd = state.mintClientCmd();
        if (clientCmd === null) {
            log(`omnitiler:world:command-refused op=${token((head.command as { op?: unknown }).op)} reason=cmd-exhausted`);
            pendingQueue.shift();
            state.noteRejectedOrTransportError();
            return;
        }
        // The report is built: consume the head now, so a later reply
        // (accepted, stale, rejected, timed out) can never replay it. The
        // unsent remainder stays queued in order.
        pendingQueue.shift();
        const queuePayload = buildQueueRequest(state.client, filedGen, clientCmd, head.command);
        if (utf8ByteLength(queuePayload) > WORLD_MAX_REQUEST_BYTES) {
            log("omnitiler:world:request-oversize recovery=on-next-event");
            state.noteRejectedOrTransportError();
            killPending("rejected");
            drainAfterFailure({ clientCmd });
            return;
        }
        fileQueue(head, clientCmd, queuePayload, observation, mark, filedGen);
    };

    // A wake owed a capture drains at flight end: every sendCapture tail
    // calls this after its existing recovery. It fires only when no
    // flight owns the wire anymore; sendCapture consumes the flag on start
    // (exactly one follow-up capture per owed wake, never a retry storm).
    const wakeFollowup = (): void => {
        if (wakePendingCapture && !stopped && state.inFlight === null) {
            wakePendingCapture = false;
            scheduleDispatch();
        }
    };

    // One outstanding long-held WaitWorldWake for the current generation.
    // Re-arm FIRST on every terminal wait reply (before dispatching): the
    // server parks the new waiter (no epoch advance yet), then the capture
    // drains and the epoch wakes it again if work remains. The 25 s host
    // watchdog rediscovers and re-arms with the SAME cursor when the host
    // kills the call without a callback (errors never call back), so an
    // external command wakes the adapter with zero native events.
    const armWait = (): void => {
        if (stopped || waitOutstanding || waitHoldForDiscovery) {
            return;
        }
        if (state.boundGen === null || state.brokerIncarnation === null || discoverOutstanding) {
            return;
        }
        const gen = state.boundGen;
        waitNonceCounter += 1;
        let rand = "0";
        try {
            rand = Math.floor(Math.random() * 0x100000000).toString(36);
        } catch (error) {
            void error;
            rand = "0";
        }
        const nonce = `wait-${waitNonceCounter.toString(36)}-${rand}`;
        waitNonce = nonce;
        waitGen = gen;
        waitOutstanding = true;
        const payload = buildWaitRequest(adapter, gen, nonce, waitCursor);
        if (utf8ByteLength(payload) > WORLD_MAX_REQUEST_BYTES) {
            // Unreachable with bounded fields; fail closed without a loop.
            waitOutstanding = false;
            return;
        }
        let settled = false;
        waitWatchdogCancel = scheduleOnce(WAIT_HOST_DEADLINE_MS, () => {
            if (settled || stopped || nonce !== waitNonce) {
                return;
            }
            settled = true;
            waitOutstanding = false;
            waitWatchdogCancel = null;
            log("omnitiler:world:wait-host-timeout recovery=rediscover");
            if (!discoverOutstanding) {
                discover();
            }
            armWait();
        });
        log(`omnitiler:world:wait nonce=${token(nonce)} nwinv=${state.currentInvalid} cursor=${token(waitCursor)}`);
        try {
            (callDbus as NonNullable<WorldEntryOverrides["callDbus"]>)(
                WORLD_DBUS_SERVICE, WORLD_DBUS_OBJECT, WORLD_DBUS_INTERFACE,
                WORLD_WAIT_METHOD, payload, (reply: unknown) => {
                    if (settled) {
                        return;
                    }
                    settled = true;
                    const cancel = waitWatchdogCancel;
                    waitWatchdogCancel = null;
                    try {
                        cancel?.();
                    } catch (error) {
                        void error;
                    }
                    onWaitReply(nonce, gen, reply);
                },
            );
        } catch (error) {
            void error;
            settled = true;
            waitOutstanding = false;
            try {
                waitWatchdogCancel?.();
            } catch (discardError) {
                void discardError;
            }
            waitWatchdogCancel = null;
            log("omnitiler:world:wait-transport recovery=host-deadline");
            scheduleWaitRecovery();
        }
    };

    // Single host-bound wait recovery chain: at most one live 25 s
    // timer; scheduling while one lives keeps the original deadline (never
    // overwritten). On fire, an unbound or held generation rediscovers
    // (guarded against a duplicate in-flight discover); otherwise the wait
    // re-arms. Discovery success clears the hold and arms the wait, so the
    // chain always terminates host-deadline causally.
    const cancelWaitRecovery = (): void => {
        try {
            waitRetryCancel?.();
        } catch (error) {
            void error;
        }
        waitRetryCancel = null;
    };
    const scheduleWaitRecovery = (): void => {
        if (stopped || waitRetryCancel !== null) {
            return;
        }
        waitRetryCancel = scheduleOnce(WAIT_HOST_DEADLINE_MS, () => {
            waitRetryCancel = null;
            if (stopped || waitOutstanding) {
                return;
            }
            if (state.boundGen === null || waitHoldForDiscovery) {
                if (!discoverOutstanding) {
                    discover();
                }
                return;
            }
            armWait();
        });
    };
    // Terminal reply of one wait call. The nonce plus generation fence
    // stale completions FIRST (a host-timeout recovery already re-armed
    // under a fresh nonce owns the slot; a late callback must neither
    // clear it nor re-arm beside it). Every later path re-arms for the
    // current generation unless a mismatch holds for rediscovery; misses
    // never dispatch (a lease is not work).
    const onWaitReply = (nonce: string, gen: string, reply: unknown): void => {
        if (stopped) {
            return;
        }
        if (nonce !== waitNonce || gen !== waitGen) {
            log("omnitiler:world:wait-stale recovery=ignore");
            return;
        }
        waitOutstanding = false;
        if (typeof reply !== "string") {
            log("omnitiler:world:wait-transport recovery=host-deadline");
            scheduleWaitRecovery();
            return;
        }
        if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
            log("omnitiler:world:wait-oversize recovery=host-deadline");
            scheduleWaitRecovery();
            return;
        }
        const parsed = parseWorldReply(reply);
        if (parsed === null) {
            log("omnitiler:world:wait-malformed recovery=host-deadline");
            scheduleWaitRecovery();
            return;
        }
        if (isWorldRejection(parsed)) {
            if (parsed.kind === "generation-mismatch") {
                if (state.boundGen === gen) {
                    dropDeadGenerationIntents();
                    killPending("generation-mismatch");
                    waitHoldForDiscovery = true;
                    wakePendingCapture = false;
                    log("omnitiler:world:generation-mismatch recovery=rediscover");
                    discover();
                } else {
                    log("omnitiler:world:wait-stale recovery=rearm");
                    armWait();
                }
                return;
            }
            if (parsed.gen !== gen || parsed.gen !== state.boundGen || nonce !== waitNonce) {
                log("omnitiler:world:wait-stale recovery=host-deadline");
                scheduleWaitRecovery();
                return;
            }
            if (parsed.kind === "unknown-adapter") {
                // Our producer lifetime is not bound server-side: hold the
                // wait for a host-deadline rediscovery. Never touch the
                // active producer binding or re-arm against the stale one.
                waitHoldForDiscovery = true;
                log(`omnitiler:world:wait-rejected kind=${token(parsed.kind)} msg=${token(parsed.message)} recovery=host-deadline`);
                scheduleWaitRecovery();
                return;
            }
            // Any other in-band wait refusal carries no work: stand down to
            // the host chain (never an immediate re-arm of this call).
            log(`omnitiler:world:wait-rejected kind=${token(parsed.kind)} msg=${token(parsed.message)} recovery=host-deadline`);
            scheduleWaitRecovery();
            return;
        }
        if (!isWorldWaitReply(parsed)) {
            log("omnitiler:world:wait-malformed recovery=rearm");
            armWait();
            return;
        }
        const waitReply = parsed as WorldWaitReply;
        if (waitReply.gen !== state.boundGen || waitReply.gen !== gen || nonce !== waitNonce) {
            log("omnitiler:world:wait-stale recovery=rearm");
            armWait();
            return;
        }
        if (waitReply.outcome === "superseded") {
            // A newer registration replaced this waiter (possibly an
            // overlapping script): stand down to the host chain instead of
            // re-superseding the live owner. The deadline retry re-arms
            // exactly one wait.
            log("omnitiler:world:wait-superseded recovery=host-deadline");
            scheduleWaitRecovery();
            return;
        }
        if (waitReply.outcome === "lease") {
            // Idle server lease: the cursor is unchanged by construction.
            // A cursor that advanced anyway is a wake in disguise: adopt
            // it and drain.
            if (waitReply.cursor !== waitCursor) {
                waitCursor = waitReply.cursor;
                armWait();
                wakePendingCapture = true;
                void dispatch(false);
                return;
            }
            armWait();
            return;
        }
        // Wake: queued work can drain. Adopt the advanced cursor, re-arm
        // first for continuous coverage, then capture (owed when a flight
        // owns the wire). Reply cursors only advance on delivered wakes,
        // and the re-armed waiter parks (no epoch advance yet), so burst
        // work drains capture-by-capture with no hot loop.
        waitCursor = waitReply.cursor;
        armWait();
        wakePendingCapture = true;
        void dispatch(false);
    };


    // Per-window homing subscriptions (approach2/5 native drag/send
    // adoption, pin ab7df7cc): `Window::outputChanged`, window
    // `desktopsChanged`, `Window::hiddenChanged` (src/window.h:590 at
    // pinned v6.7.4), and `Window::frameGeometryChanged` are WINDOW
    // signals; workspace `desktopsChanged`
    // is only count/order. External native output/membership/hidden/frame
    // changes are invisible to the workspace-only watermark, so every
    // tracked window subscribes all four homing signals; each schedules a
    // full world observe (no polling). Own-flight
    // echoes (synchronous emissions inside the wrapped setters above)
    // defer to the single post-apply observation instead; the actuated
    // window's closure/unsafe/disappearance still invalidates
    // immediately. Legacy workspace routes untouched.
    const windowHomingDetaches = new Map<object, Array<() => void>>();
    const onWindowHoming = (ref: object): (() => void) => {
        return (): void => {
            if (stopped) {
                return;
            }
            if (homingSuppressDepth > 0 && homingSuppressRef === ref) {
                // Same-flight own echo: closure/disappearance/unsafe
                // still invalidates immediately and stops apply; only
                // the live expected echo defers.
                let listed = true;
                try {
                    listed = applyHostBase.isStillListed(ref);
                } catch (error) {
                    void error;
                    listed = false;
                }
                if (!listed) {
                    onNativeInvalidation();
                    return;
                }
                try {
                    if (applyHostBase.readLiveSafety(ref) === null) {
                        onNativeInvalidation();
                        return;
                    }
                } catch (error) {
                    void error;
                    onNativeInvalidation();
                    return;
                }
                deferredOwnHoming = true;
                return;
            }
            onNativeInvalidation();
        };
    };
    const attachWindowHoming = (ref: object): void => {
        if (windowHomingDetaches.has(ref)) {
            return;
        }
        const detachesForWindow: Array<() => void> = [];
        for (const signalName of ["outputChanged", "desktopsChanged", "hiddenChanged", "frameGeometryChanged"] as const) {
            try {
                const detach = connectSignal(readSignal(ref, signalName), onWindowHoming(ref));
                if (detach !== null) {
                    detachesForWindow.push(detach);
                }
            } catch (error) {
                void error;
            }
        }
        if (detachesForWindow.length > 0) {
            windowHomingDetaches.set(ref, detachesForWindow);
        }
    };
    const detachWindowHoming = (ref: object): void => {
        const owned = windowHomingDetaches.get(ref);
        if (owned === undefined) {
            return;
        }
        windowHomingDetaches.delete(ref);
        for (const detach of owned) {
            try {
                detach();
            } catch (error) {
                void error;
            }
        }
    };
    // Safe inventory reconciliation: enumerate the live inventory for
    // attach only (existing windows plus later additions). Removal
    // detaches by payload identity only; the removed payload's
    // properties are never read (its QObject may be dead).
    const reconcileWindowHomingAttach = (): void => {
        try {
            const fn = readProp(surface, "windowList");
            if (typeof fn !== "function") {
                return;
            }
            const list = decodeList(Reflect.apply(fn as (...a: ReadonlyArray<never>) => unknown, surface, []), 10000);
            if (list === null) {
                return;
            }
            for (const item of list) {
                if (typeof item === "object" && item !== null) {
                    attachWindowHoming(item as object);
                }
            }
        } catch (error) {
            void error;
        }
    };
    // Subscribe to workspace native signals: every invalidation
    // advances CURRENT obs synchronously, before debounce.
    // Event-driven only; no polling.
    const attach = (name: string, handler: (payload?: unknown) => void): void => {
        try {
            const detach = connectSignal(readSignal(surface, name), handler);
            if (detach !== null) {
                detaches.push(detach);
            }
        } catch (error) {
            void error;
        }
    };
    for (const name of ["windowAdded", "windowActivated", "desktopsChanged", "screensChanged", "currentDesktopChanged", "activeScreenChanged"] as const) {
        if (typeof __OMNITILER_TRACE__ === "string" && __OMNITILER_TRACE__ === "1" && name === "windowAdded") {
            // Admission truth lives on the invalidation handler: the probe
            // records the added window BEFORE the synchronous invalidation
            // (and its real debounce arm) runs. No synthetic start is
            // emitted here; only actual scheduleDispatch arms log start.
            attach(name, (added) => {
                if (typeof added === "object" && added !== null) {
                    worldTraceProbes.get(state)?.noteAdded(added);
                }
                onNativeInvalidation();
            });
        } else {
            attach(name, onNativeInvalidation);
        }
    }
    // Explicit close retention (D5): windowRemoved only arms the close
    // tracker; the payload is never read (its QObject may be dead). The
    // next provably complete full inventory promotes cached ids absent
    // there to the explicit closed list until an accepted reply acks them
    // (core treats bare absence as minimize, never close). Unreadable
    // inventories still invalidate; they never fabricate ids.
    try {
        const detach = connectSignal(readSignal(surface, "windowRemoved"), (removed) => {
            // Detach by object identity only; never read the payload's
            // properties (its QObject may be dead).
            if (typeof removed === "object" && removed !== null) {
                detachWindowHoming(removed as object);
            }
            try {
                closeTracker.noteRemovedEvent();
            } catch (error) {
                void error;
            }
            onNativeInvalidation();
        });
        if (detach !== null) {
            detaches.push(detach);
        }
    } catch (error) {
        void error;
    }
    // New windows attach homing on arrival (idempotent per object
    // identity); the workspace windowAdded invalidation above still
    // schedules the observe. Late inventory additions reconcile here
    // when the payload is absent.
    try {
        const detach = connectSignal(readSignal(surface, "windowAdded"), (added) => {
            if (typeof added === "object" && added !== null) {
                attachWindowHoming(added as object);
            } else {
                reconcileWindowHomingAttach();
            }
        });
        if (detach !== null) {
            detaches.push(detach);
        }
    } catch (error) {
        void error;
    }
    reconcileWindowHomingAttach();
    try {
        const optionsGlobal: unknown = overrides.options !== undefined ? overrides.options : (() => {
            try {
                const candidate: unknown = options;
                return typeof candidate === "object" && candidate !== null ? candidate : null;
            } catch (error) {
                void error;
                return null;
            }
        })();
        if (typeof optionsGlobal === "object" && optionsGlobal !== null) {
            // Startup-consumed drift note: workspaceMode and
            // shortcutProfile never re-read for behavior here. Drift is
            // restart-required, never adopted; live settings (gaps,
            // same-axis, refill, born-maximized, default-tiling) re-resolve
            // per dispatch independently.
            const onConfigChanged = (): void => {
                try {
                    const drifted = settings.checkStartupDrift();
                    if (drifted.length > 0) {
                        log(`omnitiler:world:config-reloaded stage=restart-required keys=${drifted.join(",")}`);
                    }
                } catch (error) {
                    void error;
                }
                onNativeInvalidation();
            };
            const detach = connectSignal(readSignal(optionsGlobal as object, "configChanged"), onConfigChanged);
            if (detach !== null) {
                detaches.push(detach);
            }
        }
    } catch (error) {
        void error;
    }





    // Planner-owned tiling snapshot (D2): the last ready reply's tiling mode
    // for the current scope plus the live saved default. No TS policy:
    // without an accepted reply the scope reports the saved default as the
    // tiled seed.
    const getWorkspaceTilingSnapshot = (): { scope: string; tiled: boolean; defaultTiled: boolean } => {
        const scope = currentScopeId();
        const saved = settings.readDefaultTiled();
        if (scope === "") {
            return { scope: "", tiled: saved, defaultTiled: saved };
        }
        const out = activeOutputName();
        const last = state.lastReply;
        if (last !== null) {
            for (const ws of last.workspaces) {
                // The aligned grid repeats one native per output: scope
                // the match to the active output, never a same-native row
                // from another output. Rows are keyed by stable core
                // logical id, never the native binding.
                if (ws.id === scope && (out === null || ws.out === out)) {
                    return { scope, tiled: ws.mode !== "floating", defaultTiled: saved };
                }
            }
            if (out !== null) {
                for (const o of last.outputs) {
                    if (o.out === out && (o.view === scope || o.view === "")) {
                        return { scope, tiled: o.mode !== "floating", defaultTiled: saved };
                    }
                }
            }
        }
        return { scope, tiled: saved, defaultTiled: saved };
    };

    const currentScopeId = (): string => {
        const out = activeOutputName();
        if (out === null) {
            return "";
        }
        const obs = route.observeForCommand();
        const topo = obs?.topology.find((t) => t.out === out);
        const native = topo?.current ?? "";
        if (native === "") {
            return "";
        }
        // Stable core logical id for commands (v2 commands name logical
        // workspaces, never native desktop ids): resolve through the
        // converged core mapping only. Unmapped natives fail closed (never
        // an invented logical); outstanding local receipts prove nothing
        // until the core publishes the bind.
        try {
            const key = bindingKey(out, native);
            const bound = logicalOfNative.get(key);
            if (bound !== undefined && bound.logical !== "") {
                return bound.logical;
            }
        } catch (error) {
            void error;
        }
        return "";
    };

    // Authenticated generation discovery on start; restart/poison paths
    // re-enter through the generation-mismatch branch above. Float
    // membership seeds from the runtime store before the first dispatch.
    seedFloatIntent();
    discover();
    log(`omnitiler:world:ready ${state.describeForLog()} source=${resolveSourceRev()}`);

    void WORLD_MAX_REPLY_BYTES;

    const activeWindowRef = (): object | null => {
        const id = activeWindowId();
        return id === null ? null : liveRefById(id);
    };


    // Synchronous native state toggles (float/sticky/maximize/fullscreen)
    // live in world-native; created here once every accessor they close
    // over is defined. Shortcut registration follows: shortcut names to
    // world commands, callbacks resolving at press time.
    const native = createWorldNativeToggles({
        surface,
        log,
        token,
        readProp,
        shortcutFailed,
        submitCommand,
        submitSettledCommand: submitCommandInner,
        invalidate: () => {
            state.invalidate();
        },
        scheduleDispatch,
        persistFloatIntent,
        isWorldBusy: () => state.inFlight !== null || pendingQueue.length > 0,
        isFloating: (win) => floatingIds.has(win),
        noteFloating: (win) => {
            floatingIds.add(win);
        },
        clearFloating: (win) => {
            floatingIds.delete(win);
        },
        activeWindowId,
        activeWindowRef,
        liveRefById,
        readLiveRect: (ref) => {
            try {
                const g = readProp(ref, "frameGeometry") as Record<string, unknown> | undefined;
                if (typeof g !== "object" || g === null) {
                    return null;
                }
                const x = g["x"];
                const y = g["y"];
                const w = g["width"] !== undefined ? g["width"] : g["w"];
                const h = g["height"] !== undefined ? g["height"] : g["h"];
                if (typeof x !== "number" || typeof y !== "number" || typeof w !== "number" || typeof h !== "number") {
                    return null;
                }
                return { x: Math.round(x), y: Math.round(y), w: Math.round(w), h: Math.round(h) };
            } catch (error) {
                void error;
                return null;
            }
        },
    });
    // Newer explicit input supersedes the deferred B9 attempt from here
    // on (all command traffic flows through the hooked submitCommand
    // above; the native toggles below cancel explicitly).
    cancelPendingForNewerInput = (): void => {
        try {
            native.cancelPendingUnfloat("newer-input");
        } catch (error) {
            void error;
        }
        try {
            native.cancelPendingNativeState("newer-input");
        } catch (error) {
            void error;
        }
    };
    registerWorldShortcuts({
        readProp,
        isDirection,
        shortcutFailed,
        submitCommand,
        submitCatalogMove,
        submitOutputSend,
        submitMigration,
        submitFloat: (label) => native.submitFloat(label),
        setStickyNative: (ref, id, sticky) => native.setStickyNative(ref, id, sticky),
        setFullscreenNative: (ref, fullscreen, label) => native.setFullscreenNative(ref, fullscreen, label ?? "toggle-fullscreen"),
        activeOutputName,
        activeWindowId,
        activeWindowRef,
        currentScopeId,
        registerShortcutFn: overrides.registerShortcutFn,
        readSameAxisMove: () => settings.readSameAxisLive(),
        readResizeSignature: () => route.resizeSignature(),
    });

    return {
        stop: () => {
            stopped = true;
            // Wait teardown: cancel the host watchdog and any
            // host-deadline retry, drop the owed wake, and release the
            // outstanding slot so late callbacks no-op on the fence.
            waitOutstanding = false;
            waitHoldForDiscovery = false;
            wakePendingCapture = false;
            try {
                waitWatchdogCancel?.();
            } catch (error) {
                void error;
            }
            waitWatchdogCancel = null;
            cancelWaitRecovery();
            cancelDiscoverChain();
            cancelEffectRecovery();
            // Presentation subscriber teardown: drop in-flight reads and
            // clear the native paint so no stale shapes survive the entry.
            try {
                presentationSubscriber.stop();
            } catch (error) {
                void error;
            }
            // Effect-facts teardown: drop the held waiter slot so a late
            // facts callback no-ops on the fence; the pending native call
            // (if any) completes into a cleared slot.
            effectWaiterOutstanding = false;
            effectWaiterGen = null;
            lastKnownEffectInstance = null;
            setEffectSnapshot(null);
            try {
                effectWatchdogCancel?.();
            } catch (error) {
                void error;
            }
            effectWatchdogCancel = null;
            // Gesture-facts teardown: drop the held waiter slot so a late
            // facts callback no-ops on the fence; unacked terminals and
            // outstanding acks drop with the entry (no ack after stop).
            gestureWaiterOutstanding = false;
            gestureWaiterGen = null;
            gestureTerminalPending = null;
            gestureSnapshot = null;
            gestureBoundAssoc = null;
            dropFollowup = null;
            clearGestureAck();
            clearTerminalCaptureRetry();
            lastResolvedCaptureSource = null;
            try {
                gestureWatchdogCancel?.();
            } catch (error) {
                void error;
            }
            gestureWatchdogCancel = null;
            cancelGestureRecovery();
            // Per-window homing teardown first so no stale window
            // callback can fire after stop; workspace detaches below
            // are idempotent and also release the per-window set.
            for (const owned of windowHomingDetaches.values()) {
                for (const detach of owned) {
                    try {
                        detach();
                    } catch (error) {
                        void error;
                    }
                }
            }
            windowHomingDetaches.clear();
            deferredOwnHoming = false;
            if (typeof __OMNITILER_TRACE__ === "string" && __OMNITILER_TRACE__ === "1") {
                worldTraceProbes.get(state)?.stop();
                worldTraceProbes.delete(state);
            }
            // Teardown drops unsent queued intents: no post-stop dispatch
            // may carry them, and later presses refuse on `stopped`.
            pendingQueue.length = 0;
            unackedQueue = null;
            if (drainTimerCancel !== null) {
                try {
                    drainTimerCancel();
                } catch (error) {
                    void error;
                }
                drainTimerCancel = null;
            }
            try {
                native.cancelPendingUnfloat();
            } catch (error) {
                void error;
            }
            try {
                native.cancelPendingNativeState();
            } catch (error) {
                void error;
            }
            // Best-effort release of owned automatic keepAbove on
            // explicit stop only; the actual KWin script unload path is
            // unharnessed and makes no claim here.
            try {
                native.releaseAllAutomaticFloats("stop");
            } catch (error) {
                void error;
            }
            if (dispatchTimerCancel !== null) {
                try {
                    dispatchTimerCancel();
                } catch (error) {
                    void error;
                }
                dispatchTimerCancel = null;
            }
            for (const detach of detaches.splice(0)) {
                try {
                    detach();
                } catch (error) {
                    void error;
                }
            }
        },
        requestFocus: (direction) => {
            if (!isDirection(direction)) {
                shortcutFailed("focus");
                return;
            }
            // Command-only: floating/sticky subjects route the same
            // direction-only command; the core owns the float lattice.
            const w = activeWindowId();
            if (w === null) {
                shortcutFailed("focus");
                return;
            }
            const cmd = cmdFocusDirectional(w, direction);
            if (cmd === null || !submitCommand(cmd, "focus")) {
                shortcutFailed("focus");
            }
        },
        requestMove: (direction) => {
            if (!isDirection(direction)) {
                shortcutFailed("move");
                return;
            }
            // Command-only: floating/sticky subjects route the same
            // direction-only command; the core owns half-snap geometry.
            const w = activeWindowId();
            if (w === null) {
                shortcutFailed("move");
                return;
            }
            const cmd = cmdMoveDirectional(w, direction, settings.readSameAxisLive());
            if (cmd === null || !submitCommand(cmd, "move")) {
                shortcutFailed("move");
            }
        },
        requestResize: (direction, mode) => {
            const w = activeWindowId();
            if (w === null || !isDirection(direction) || (mode !== "inwards" && mode !== "outwards")) {
                shortcutFailed("resize");
                return;
            }
            if (!submitCommand({ op: "operation", command: { op: "resize", win: w, direction, mode, press: route.nextResizePress(w, direction, mode) } }, "resize")) {
                shortcutFailed("resize");
            }
        },
        requestToggleOrientation: () => {
            const w = activeWindowId();
            if (w === null || !submitCommand({ op: "operation", command: { op: "toggle-orientation", win: w } }, "toggle-orientation")) {
                shortcutFailed("toggle-orientation");
            }
        },
        requestFloat: () => {
            native.submitFloat("float");
        },
        // Sticky/fullscreen have no Planner command route: they are
        // synchronous native state toggles applied here with pre-setter
        // safety rechecks, then observed authoritatively on the next full
        // report (core converges the tiling consequences). Maximize routes
        // a Planner toggle-maximize command instead. Fullscreen and
        // maximized overlays refuse exactly like the legacy guards; game
        // safety wins over parity. Sticky restores floating origin plus
        // keepAbove/keepBelow on off.
        requestSticky: () => {
            const id = activeWindowId();
            const ref = activeWindowRef();
            if (id === null || ref === null) {
                shortcutFailed("sticky");
                return;
            }
            try {
                native.cancelPendingUnfloat("newer-input");
            } catch (error) {
                void error;
            }
            try {
                native.cancelPendingNativeState("newer-input");
            } catch (error) {
                void error;
            }
            const stickyRaw = readProp(ref, "onAllDesktops");
            const want = (typeof stickyRaw === "boolean" ? stickyRaw : readNativeAlias(readProp, ref, "sticky")) !== true;
            if (!native.setStickyNative(ref, id, want)) {
                shortcutFailed("sticky");
            }
        },
        requestMaximize: () => {
            const id = activeWindowId();
            if (id === null) {
                shortcutFailed("maximize");
                return;
            }
            try {
                native.cancelPendingUnfloat("newer-input");
            } catch (error) {
                void error;
            }
            try {
                native.cancelPendingNativeState("newer-input");
            } catch (error) {
                void error;
            }
            if (!submitCommand(cmdToggleMaximize(id), "maximize")) {
                shortcutFailed("maximize");
            }
        },
        requestFullscreen: () => {
            const ref = activeWindowRef();
            if (ref === null) {
                shortcutFailed("fullscreen");
                return;
            }
            try {
                native.cancelPendingUnfloat("newer-input");
            } catch (error) {
                void error;
            }
            try {
                native.cancelPendingNativeState("newer-input");
            } catch (error) {
                void error;
            }
            const fsRaw = readProp(ref, "fullScreen");
            const want = (typeof fsRaw === "boolean" ? fsRaw : readNativeAlias(readProp, ref, "fullscreen") === true) !== true;
            if (!native.setFullscreenNative(ref, want, "fullscreen")) {
                shortcutFailed("fullscreen");
            }
        },
        requestWorkspaceSelect: (index) => {
            // Neutral selection: core resolves the output cursor-first.
            if (!Number.isInteger(index)) {
                shortcutFailed("workspace-select");
                return;
            }
            const n = index as number;
            const cmd = n === 0 ? cmdSelectTrailing(null) : cmdSelectNumbered(null, n);
            if (cmd === null || !submitCommand(cmd, "workspace-select")) {
                shortcutFailed("workspace-select");
            }
        },
        requestWorkspaceMove: (index, follow) => {
            if (!Number.isInteger(index)) {
                shortcutFailed("workspace-move");
                return;
            }
            submitCatalogMove("workspace-move", index as number, follow !== false);
        },
        requestWorkspacePrevious: () => {
            if (!submitCommand(cmdSelectPrevious(null), "workspace-previous")) {
                shortcutFailed("workspace-previous");
            }
        },
        requestWorkspaceRelative: (delta) => {
            const cmd = delta !== -1 && delta !== 1 ? null : cmdSelectRelative(null, delta);
            if (cmd === null || !submitCommand(cmd, "workspace-relative")) {
                shortcutFailed("workspace-relative");
            }
        },
        requestWorkspaceRelativeMove: (delta, follow) => {
            const w = activeWindowId();
            const cmd = w === null || (delta !== -1 && delta !== 1)
                ? null
                : cmdSendRelative(w, implicitSource("home-then-cached-live"), delta, follow !== false);
            if (cmd === null || !submitCommand(cmd, "workspace-relative-move")) {
                shortcutFailed("workspace-relative-move");
            }
        },
        requestSendToOutput: (direction, follow) => {
            if (!isDirection(direction) || !submitOutputSend(direction, follow !== false)) {
                shortcutFailed("send-to-output");
            }
        },
        requestWorkspaceMigrate: (direction) => {
            if (!isDirection(direction) || !submitMigration(direction)) {
                shortcutFailed("migrate");
            }
        },
        getWorkspaceTilingSnapshot: () => getWorkspaceTilingSnapshot(),
        requestWorkspaceTilingToggle: () => {
            const out = activeOutputName();
            const scope = currentScopeId();
            if (out === null || out === "" || scope === "") {
                shortcutFailed("toggle-workspace-tiling");
                return;
            }
            if (!submitCommand(cmdToggleWorkspaceTiling(out, scope), "toggle-workspace-tiling")) {
                shortcutFailed("toggle-workspace-tiling");
            }
        },
    };
}
