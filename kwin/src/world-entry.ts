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
import { createWorldDragRoute } from "./world-drag";
import { registerWorldShortcuts } from "./world-shortcuts";
import { createWorldApplyHost, createWorldNativeToggles } from "./world-native";
import { createFloatNav } from "./world-float-nav";
import {
    GROUP_HIGHLIGHT_CLEAR_METHOD,
    GROUP_HIGHLIGHT_INTERFACE,
    GROUP_HIGHLIGHT_OBJECT,
    GROUP_HIGHLIGHT_SERVICE,
    GROUP_HIGHLIGHT_SET_METHOD,
    formatGroupHighlightPayload,
} from "./active-group-highlight";

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
    isOpaqueId,
    utf8ByteLength,
} from "./world-seq";
import {
    buildCaptureEnvelope,
    buildDiscoveryRequest,
    buildQueueRequest,
    buildTakeRequest,
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
    cmdSetWorkspaceTiling,
    explicitSource,
    isCoreWorldReply,
    isWorldQueueReply,
    isWorldRejection,
    parseDiscoveryReply,
    parseWorldReply,
    type WorldCommand,
    type WorldReply,
    type WorldReplyResult,
    type WorldTakeReply,
} from "./world-report";
import { captureWorldObservation, type WorldObservation } from "./world-observe";
import { WorldAdapterState, type CaptureFlight, type QueuedIntent } from "./world-state";
import { applyWorldReply } from "./world-apply";
import { decodeList } from "./qml-list";
import { normalizeNativeId } from "./native-id";
import { readNativeAlias } from "./native-alias";
import { createWorldCloseTracker } from "./world-close-tracker";
import { createWorldCommandContext } from "./world-command-context";
import { createWorldSettingsLive } from "./world-settings-live";
import { normalizeScopeClassList, readBakedScopeClass, windowMatchesScopeClass } from "./scope-class";
import { WorldBornMaximizedAdmission } from "./world-born-maximized";

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
    // Native-only seams (never workspace policy): the drag-oracle D-Bus
    // caller (LastVerdict pull shape differs from the world payload shape),
    // the fixed-size predicate reader, and the fixed-origin decorator.
    readonly oracleCallDbus?: (
        service: string, path: string, iface: string, method: string,
        callback: (reply: unknown) => void,
    ) => void;
    // Renderer D-Bus caller for the native effect endpoints (drag-preview
    // overlay plus active-group highlight): the effect remains the
    // renderer, this only forwards core-derived rects/bounds. Defaults to
    // the global callDBus like the legacy plan adapter.
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
    // Keyed per (output, native): the aligned shared grid carries a
    // distinct logical identity per output cell, so one native id maps to
    // one logical per output. Only the logical id and output converge here;
    // pin/disp/managed are derived per report in pinRetained below (never
    // stored constants).
    const logicalOfNative = new Map<string, { logical: string; out: string }>();
    const bindingKey = (out: string, native: string): string => `${out}\u0000${native}`;
    const detaches: Array<() => void> = [];
    // Explicit close tracking (D5): canonical live-id cache plus
    // event-gated difference (see world-close-tracker). The removal payload
    // is never read; core treats bare absence as minimize, never close.
    const closeTracker = createWorldCloseTracker();
    // Tentative (user review pending): a persisted `global-unique`
    // workspaceMode falls back to `per-output-local` on the world route
    // with one bounded logged reason. KDE cannot present globally unique
    // numbers (desktop positions/names are global); the core keeps
    // `global-unique` for other platforms and legacy plan callers keep
    // their own mode reads untouched.
    let modeFallbackLogged = false;
    const readModeAligned = (): unknown => {
        let raw: unknown = "per-output-local";
        try {
            raw = settings.readMode();
        } catch (error) {
            void error;
        }
        if (raw === "global-unique") {
            if (!modeFallbackLogged) {
                modeFallbackLogged = true;
                log("omnitiler:world:workspace-mode-fallback from=global-unique to=per-output-local reason=kde-aligned-per-output");
            }
            return "per-output-local";
        }
        return raw;
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
    // Drag-oracle verdict route (Started/Finish capture, move-drop/resize
    // routing, per-gesture preview) lives in world-drag; created below once
    // observation and transport exist.

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
        return false;
    };

    const capture = (): WorldObservation | null => {
        try {
            closeTracker.syncLive(surface);
        } catch (error) {
            void error;
        }
        return captureWorldObservation({
        workspace: surface,
        readMode: readModeAligned,
        readGap: () => settings.readGapLive(),
        readOuterGap: () => settings.readOuterGapLive(),
        gestureActive,
        knownClosed: () => closeTracker.snapshotClosed(),
        scopeClass,
        logicalBindingOf: (nativeId, out) => {
            // Locally-created but not yet core-acknowledged bindings publish
            // here so core pending consumes them: a bound pending creation
            // overrides only an absent entry or a generic native->native
            // fallback echo (logical === native). A real core mapping stays
            // authoritative and the pending row is pruned on accept. The
            // pending lookup scopes to the requesting output; the snapshot
            // lookup keys per (output, native) for the aligned grid. An
            // output-less lookup (shared mode, never replicated) resolves
            // through the first cell bound to that native.
            const pending = state.pendingCreations.find((c) =>
                c.bound === nativeId && c.logical !== "" && c.out !== "" &&
                (out === undefined || c.out === out),
            );
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
            if (
                pending !== undefined &&
                (base === undefined || base.logical === nativeId)
            ) {
                return {
                    logical: pending.logical, out: pending.out,
                    pin: route.pinRetained(nativeId), disp: false, managed: true,
                };
            }
            if (base === undefined) {
                return null;
            }
            return {
                logical: base.logical, out: base.out,
                pin: route.pinRetained(nativeId), disp: false, managed: true,
            };
        },
        // Binding replication follows the effective workspace mode: the
        // aligned per-output-local grid binds one native once per output
        // with a distinct logical each, while shared (unchanged) keeps one
        // row per native and the nonaligned core would collapse replicates
        // through a generic native->logical map.
        replicateBindingsPerOutput: readModeAligned() === "per-output-local",
        isFloating: (win) => floatingIds.has(win),
        isTileOverride: (win) => tileIds.has(win),
        ...(overrides.readFixedPredicateFn !== undefined ? { readFixedPredicate: overrides.readFixedPredicateFn } : {}),
        ...(overrides.fixedOriginOf !== undefined ? { fixedOriginOf: overrides.fixedOriginOf } : {}),
    });
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
        dispatchTimerCancel = scheduleOnce(WORLD_DEBOUNCE_MS, () => {
            debounceArmed = false;
            dispatchTimerCancel = null;
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
        isCreationBound: (nativeId) => state.pendingCreations.some((c) => c.bound === nativeId),
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
    // `desktopsChanged` echo of our own output/membership setters must
    // not invalidate mid-apply (that would wedge the flight and can
    // resurrect stalled migrations). While a wrapped setter runs,
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

    // Float-origin directional focus/move lives in world-float-nav (native
    // only, same semantics): the entry owns lifecycle state and flight
    // ordering and injects them as one busy boolean plus callbacks, never
    // copied state.
    const floatNav = createFloatNav({
        surface,
        readProp,
        log,
        isBusy: () => state.inFlight !== null || pendingQueue.length > 0,
        activeWindowId,
        observe: route.observeForCommand,
        liveRefById,
        submitCommand,
        invalidate: () => {
            state.invalidate();
        },
        scheduleDispatch,
    });
    const handleFloatFocus = floatNav.handleFloatFocus;
    const handleFloatMove = floatNav.handleFloatMove;

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
    // 0..9 straight to core (D2/D3). Index 0 is the trailing path: core
    // reuses the live trailing empty or mints a stable pending token and
    // pends the send until the binding is reported. Resolution is all core;
    // this path never maps positions locally and never view-switches: stays
    // (follow=false) keep the origin view because sends never switch views.
    const submitCatalogMove = (action: string, index: number, follow: boolean): void => {
        const out = activeOutputName();
        const w = activeWindowId();
        if (out === null || w === null || !Number.isInteger(index) || index < 0 || index > 9) {
            shortcutFailed(action);
            return;
        }
        const cmd = cmdSendNumbered(w, explicitSource(out), index, follow);
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

    // Native effect renderer calls (never workspace policy): the drag
    // overlay and the group highlight render core-derived geometry only.
    // Fail-closed false/void when the bus is unavailable.
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
                GROUP_HIGHLIGHT_SERVICE,
                GROUP_HIGHLIGHT_OBJECT,
                GROUP_HIGHLIGHT_INTERFACE,
                method,
                ...args,
            );
            return true;
        } catch (error) {
            void error;
            return false;
        }
    };
    const DRAG_PREVIEW_SET_METHOD = "SetDragTargetPreview";
    const DRAG_PREVIEW_CLEAR_METHOD = "ClearDragTargetPreview";

    // Drag-oracle verdict route (single responsibility: Started/Finish
    // capture, move-drop/resize routing, per-gesture preview). Owned by
    // world-drag; driven here with observation and transport.
    const dragRoute = createWorldDragRoute({
        surface,
        log,
        token,
        readProp,
        observeForCommand: route.observeForCommand,
        // Drag verdicts and preview ticks keep the fail-fast single-flight
        // fence (busy refuses and releases for retry); they never queue
        // behind shortcut commands.
        submitCommand: submitCommandInner,
        isStopped: () => stopped,
        trackDetach: (detach) => {
            detaches.push(detach);
        },
        oracleCallDbus: overrides.oracleCallDbus,
        setPreview: (rect) => highlightCall(DRAG_PREVIEW_SET_METHOD, rect.x, rect.y, rect.w, rect.h),
        clearPreview: () => {
            highlightCall(DRAG_PREVIEW_CLEAR_METHOD);
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
        const timeoutCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            if (!settled) {
                settled = true;
                discoverOutstanding = false;
                log("omnitiler:world:discover-timeout recovery=on-next-event");
            }
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
                        log("omnitiler:world:discover-transport recovery=on-next-event");
                        return;
                    }
                    if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        log("omnitiler:world:discover-oversize recovery=on-next-event");
                        return;
                    }
                    const parsed = parseDiscoveryReply(reply);
                    if (parsed === null || isWorldRejection(parsed)) {
                        log("omnitiler:world:discover-rejected recovery=on-next-event");
                        return;
                    }
                    // Authenticated discovery: the bootstrap reply must be a
                    // non-ready core reply naming the broker incarnation
                    // (never this adapter) plus the exact outstanding
                    // discovery req. Anything else binds nothing and resets
                    // nothing.
                    if (parsed.ready !== false || parsed.adapter === adapter || parsed.req !== reqId) {
                        log("omnitiler:world:discover-rejected recovery=on-next-event");
                        return;
                    }
                    const changed = state.onDiscovery(parsed.gen, parsed.adapter, adapter, parsed.req, reqId);
                    if (changed) {
                        // New generation: queued intents belong to the dead
                        // generation and drop with one diagnostic; the
                        // binding snapshot and one-shot native effects
                        // reset with the backend that owned them.
                        dropDeadGenerationIntents();
                        logicalOfNative.clear();
                        try {
                            highlightCall(GROUP_HIGHLIGHT_CLEAR_METHOD);
                        } catch (error) {
                            void error;
                        }
                    }
                    log(`omnitiler:world:discovered gen=${token(parsed.gen)}`);
                    state.invalidate();
                    void dispatch(false);
                },
            );
        } catch (error) {
            void error;
            timeoutCancel();
            discoverOutstanding = false;
            log("omnitiler:world:discover-transport recovery=on-next-event");
        }
    };

    // R-MAX-06 born-maximized admission (legacy parity, adapter-owned).
    // The clear phase runs ONLY after a ready full reply (bootstrap never
    // actuates), with a current tiled/controlled scope proof from the
    // reply, before unsafe geometry application. Mode re-resolves per
    // admission (future admissions only, attempts never reset).
    const bornMaxAdmission = new WorldBornMaximizedAdmission(overrides.readBornMaximizedFn);

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
    };

    // Dead-generation intent drop: queued intents belong to the superseded
    // generation and drop with one diagnostic; the filed-but-unacked
    // identity and the undecided take clear with them, never reminted
    // into the replacement namespace. Callers rediscover (or continue,
    // for discovery itself).
    const dropDeadGenerationIntents = (): void => {
        const dropped = pendingQueue.length;
        pendingQueue.length = 0;
        undecidedClientCmd = null;
        unackedQueue = null;
        if (dropped > 0) {
            log(`omnitiler:world:queue-dropped reason=generation-mismatch depth=${dropped}`);
        }
    };

    // Drain-time subject resolution for queued tiled directional focus
    // and move. The queue retains the direction intent; the subject
    // rebinds here to the authoritative current native focus, validated
    // against the fresh observation. A vanished subject (no active
    // window, unknown to the observation, or with no live ref) or a
    // subject that has become a float/sticky exception drops narrowly
    // with one bounded line and no native writes; the next queued intent
    // then resolves in turn. Float-origin focus fallback intents
    // (keepSubject true) keep their explicit press-time window identity
    // and pass through untouched, as do non-directional commands.
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
            if (subject.floating === true || subject.sticky === true) {
                pendingQueue.shift();
                log(`omnitiler:world:command-dropped reason=${reasonPrefix}-float`);
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

    // Post-accept capture handling with the requester-only terminal result:
    // binding convergence, close/float convergence, born-maximized clear,
    // safe actuation, native toggles, and drain scheduling. Runs after the
    // take collects (chained, never raced).
    const postAcceptCapture = (
        parsed: WorldReply,
        flight: CaptureFlight,
        prevReply: WorldReply | null,
        takeResult: WorldReplyResult | null,
    ): void => {
                    // The terminal take result (requester-only, never the
                    // capture reply lane) settles the preview flight: the
                    // derived core rect renders (identical repeats never
                    // re-set), the hover prior chains, and the coalesced
                    // pending tick dispatches from the latest pointer.
                    try {
                        dragRoute.notifyReplyAccepted(takeResult);
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
                            // Handoff: a real (non-fallback) core mapping
                            // consumes the matching pending creation so the
                            // stale pending out never overrides a later
                            // legitimate migration. Generic fallback echoes
                            // never consume the pending logical.
                            if (ws.id !== "" && ws.id !== ws.bind) {
                                state.acknowledgeCreationBinding(ws.bind, ws.id);
                            }
                        }
                    }
                    // Ack the closes that rode the accepted dispatch BEFORE the
                    // post-accept fresh capture: that capture may promote new
                    // event-gated closes that have never been reported and
                    // must survive until their own accepted reply. The
                    // pre-ack snapshot also prunes born-maximized lifetimes
                    // (explicit close only, never minimized absence).
                    const closedBeforeAck = closeTracker.snapshotClosed();
                    try {
                        closeTracker.ack();
                    } catch (error) {
                        void error;
                    }
                    try {
                        bornMaxAdmission.pruneClosed(closedBeforeAck);
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
                    // R-MAX-06 post-ready clear phase: eligible births clear
                    // once natively BEFORE unsafe geometry application, with
                    // a same-canonical-ID readback proof. On any clear the
                    // apply below uses recaptured truth and the next
                    // dispatch is observation-only catchup (commands never
                    // replay).
                    if (bornMaxAdmission.runAfterReady({
                        readProp,
                        liveRefById,
                        log,
                        token,
                        invalidate: () => {
                            state.invalidate();
                        },
                    }, fresh, parsed, prevReply)) {
                        state.observationOnlyCatchup = true;
                        state.dirty = true;
                        const recaptured = capture();
                        if (recaptured === null) {
                            log("omnitiler:world:observe-invalid recovery=on-next-event");
                            return;
                        }
                        fresh = recaptured;
                    }
                    // Settle the admitted lifetime AFTER the birth decision
                    // above: ids the core provably knew stay admitted across
                    // a Planner generation rebind (null previous snapshot),
                    // so a kept, fixed, or floating maximize is never
                    // reclassified as a new birth. Never native-actuates;
                    // unknown/held scopes and still-held fullscreen births
                    // never mark.
                    try {
                        bornMaxAdmission.noteAcceptedReply(fresh, parsed);
                    } catch (error) {
                        void error;
                    }
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
                    // Route the read-only active-group result to the native                    // highlight renderer (no new border policy): the effect
                    // remains the renderer, this only forwards core-derived
                    // bounds with the legacy fullscreen fail-closed gate.
                    // no-group clears. Never derives topology or geometry.
                    try {
                        if (takeResult !== null && takeResult.op === "active-group") {
                            const members = takeResult.members.map((m) => m.win);
                            let gated = true;
                            const seen = new Set<string>();
                            for (const id of members) {
                                seen.add(id);
                            }
                            for (const domain of fresh.domains) {
                                for (const w of domain.windows) {
                                    seen.delete(w.win);
                                    if (members.includes(w.win) && w.fullscreen) {
                                        gated = false;
                                    }
                                }
                            }
                            if (gated && seen.size === 0 && state.boundGen !== null) {
                                const rev = Number(takeResult.rev);
                                const payload = formatGroupHighlightPayload(
                                    parsed.req, owner, state.boundGen, rev,
                                    takeResult.group, takeResult.win, members,
                                    { x: takeResult.bounds[0], y: takeResult.bounds[1], w: takeResult.bounds[2], h: takeResult.bounds[3] },
                                );
                                if (payload !== null) {
                                    highlightCall(GROUP_HIGHLIGHT_SET_METHOD, payload);
                                } else {
                                    highlightCall(GROUP_HIGHLIGHT_CLEAR_METHOD);
                                }
                            } else {
                                highlightCall(GROUP_HIGHLIGHT_CLEAR_METHOD);
                            }
                        } else if (takeResult !== null && takeResult.op === "no-group") {
                            highlightCall(GROUP_HIGHLIGHT_CLEAR_METHOD);
                        }
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
                    // Terminal retained-operation verdicts settle matching
                    // locally ambiguous attempts only: proven outcomes end
                    // ambiguity, continuations and abandonments stay
                    // core-owned. Unknown ops or unmatched ids change
                    // nothing and never actuate.
                    for (const settledOp of parsed.operationSettled) {
                        if (!state.attempts.has(settledOp.op)) {
                            continue;
                        }
                        if (settledOp.outcome === "float-frame-proven") {
                            state.settleAttempt(settledOp.op, "applied", "");
                        } else if (settledOp.outcome === "float-refused-proven" || settledOp.outcome === "refused-proven") {
                            state.settleAttempt(settledOp.op, "refused-proven", settledOp.cause);
                        } else if (settledOp.outcome === "superseded") {
                            state.settleAttempt(settledOp.op, "superseded-external", "");
                        }
                    }
                    const applied = applyWorldReply({
                        state,
                        observation: fresh,
                        reply: parsed,
                        dispatchWatermark: flight.watermark,
                        takeResult,
                        log,
                        ...applyHostBase,
                        setMembership: (ref, wsNativeId, wsRef) =>
                            withHomingSuppression(ref, () => applyHostBase.setMembership(ref, wsNativeId, wsRef)),
                        sendToOutput: (ref, outRef) =>
                            withHomingSuppression(ref, () => applyHostBase.sendToOutput(ref, outRef)),
                    });
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
                    // capture is fresh).
                    if (pendingQueue.length > 0) {
                        scheduleDrain();
                    } else if (state.dirty) {
                        scheduleDispatch();
                    }
    };

    // One capture flight: the inner report is ALWAYS commandless (the broker
    // stamps the drained queued command itself). Fresh-source correlation
    // and the CURRENT local watermark bind the envelope; the callback binds
    // generation/incarnation/source/watermark before any setter.
    const sendCapture = (observation: WorldObservation, headOp: string | null, clientCmd: string | null): void => {
        if (state.boundGen === null || state.brokerIncarnation === null) {
            return;
        }
        const inner = buildWorldInnerReportJson({
            observation,
            defaultTiled: settings.readDefaultTiled(),
            refill: settings.readRefillLive(),
            anchorFallback: settings.readAnchorFallbackLive(),
            noSpaceResult: settings.readNoSpaceResultLive(),
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
        let settled = false;
        const timeoutCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            if (!settled) {
                settled = true;
                log("omnitiler:world:timeout recovery=on-next-event");
                // The filed head is never replayed; queued tails drain on
                // the scheduled catchup below, never on a blind retry.
                state.noteRejectedOrTransportError();
                killPending("timeout");
                drainAfterFailure(flight);
            }
        });
        log(`omnitiler:world:dispatch src=${token(source)} nwinv=${flight.watermark} cmd=${headOp === null ? "observe" : token(headOp)}`);
        try {
            (callDbus as NonNullable<WorldEntryOverrides["callDbus"]>)(
                WORLD_DBUS_SERVICE, WORLD_DBUS_OBJECT, WORLD_DBUS_INTERFACE,
                WORLD_DESCRIBE_METHOD, envelope, (reply: unknown) => {
                    if (settled) {
                        return;
                    }
                    settled = true;
                    timeoutCancel();
                    if (typeof reply !== "string") {
                        log("omnitiler:world:transport-error recovery=on-next-event");
                        state.noteRejectedOrTransportError();
                        killPending("transport");
                        drainAfterFailure(flight);
                        return;
                    }
                    if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        log("omnitiler:world:reply-oversize recovery=on-next-event");
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure(flight);
                        return;
                    }
                    const parsed = parseWorldReply(reply);
                    if (parsed === null) {
                        log("omnitiler:world:reply-malformed recovery=on-next-event");
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure(flight);
                        return;
                    }
                    if (isWorldRejection(parsed)) {
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
                            discover();
                            return;
                        }
                        if (parsed.kind === "busy" || parsed.kind === "unavailable") {
                            state.noteRejectedOrTransportError();
                            killPending("busy");
                            drainAfterFailure(flight);
                            return;
                        }
                        log(`omnitiler:world:rejected kind=${token(parsed.kind)} msg=${token(parsed.message)} recovery=on-next-event`);
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure(flight);
                        return;
                    }
                    if (!isCoreWorldReply(parsed)) {
                        // Queue/take/presentation lanes never arrive here.
                        log("omnitiler:world:reply-malformed recovery=on-next-event");
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure(flight);
                        return;
                    }
                    // Previous accepted snapshot for born-maximized applied
                    // evidence: the new reply contains birth rows and must
                    // not misclassify them as later maximizes.
                    const prevReply = state.lastReply;
                    const verdict = state.acceptReply(parsed, flight.source, flight.watermark);
                    if (!verdict.applied) {
                        log(`omnitiler:world:stale reason=${token(verdict.reason)} recovery=catchup`);
                        // The filed head is never replayed. Consume exactly
                        // the flight this callback owns: the stale record
                        // otherwise wedges every later dispatch
                        // (single-flight busy forever) even though its
                        // timeout already settled. Dirty closes and the
                        // unsent queued remainder are retained; the
                        // redispatch below drains the queue after it.
                        state.releaseFlight(flight.source);
                        try {
                            dragRoute.notifyStaleReply(verdict.reason);
                        } catch (error) {
                            void error;
                        }
                        if (state.dirty || pendingQueue.length > 0) {
                            scheduleDispatch();
                        }
                        return;
                    }
                    if (state.currentInvalid !== flight.watermark) {
                        // Native input landed during flight: the drained
                        // command already executed server-side (collect its
                        // terminal result), but this observation is
                        // superseded, so no setters run on it. Follow-up
                        // observes the newer watermark (dirty is set).
                        log("omnitiler:world:stale reason=superseded-during-flight recovery=catchup");
                        takeResultFor(flight.clientCmd, () => {
                            if (state.dirty || pendingQueue.length > 0) {
                                scheduleDispatch();
                            }
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
                            return;
                        }
                        postAcceptCapture(parsed, flight, prevReply, takeResult);
                    });
                },
            );
        } catch (error) {
            void error;
            timeoutCancel();
            state.noteRejectedOrTransportError();
            killPending("transport");
            drainAfterFailure(flight);
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
                        return;
                    }
                    if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        log("omnitiler:world:queue-oversize recovery=on-next-event");
                        unackedQueue = { head, clientCmd, payload: queuePayload };
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure({ clientCmd });
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
                            return;
                        }
                        log("omnitiler:world:queue-malformed recovery=on-next-event");
                        unackedQueue = { head, clientCmd, payload: queuePayload };
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        drainAfterFailure({ clientCmd });
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


    // Per-window homing subscriptions (approach2/5 native drag/send
    // adoption, pin ab7df7cc): `Window::outputChanged` and window
    // `desktopsChanged` are WINDOW signals; workspace `desktopsChanged`
    // is only count/order. External native output/membership changes
    // are invisible to the workspace-only watermark, so every tracked
    // window subscribes both homing signals; each schedules a full
    // world observe (never geometry noise, no polling). Own-flight
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
        for (const signalName of ["outputChanged", "desktopsChanged"] as const) {
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
    const attach = (name: string, handler: () => void): void => {
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
        attach(name, onNativeInvalidation);
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
        // converged core mapping, including pending local creations.
        // Unmapped natives fail closed (never an invented logical).
        try {
            const key = bindingKey(out, native);
            const bound = logicalOfNative.get(key);
            if (bound !== undefined && bound.logical !== "") {
                return bound.logical;
            }
            const pending = state.pendingCreations.find((c) =>
                c.bound === native && c.logical !== "" && c.out !== "" &&
                (c.out === out),
            );
            if (pending !== undefined) {
                return pending.logical;
            }
        } catch (error) {
            void error;
        }
        return "";
    };

    // Authenticated generation discovery on start; restart/poison paths
    // re-enter through the generation-mismatch branch above. The drag-oracle
    // verdict route attaches with the native surface; float membership seeds
    // from the runtime store before the first dispatch.
    dragRoute.attach();
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
    };
    registerWorldShortcuts({
        readProp,
        isDirection,
        shortcutFailed,
        submitCommand,
        handleFloatFocus,
        handleFloatMove,
        submitCatalogMove,
        submitOutputSend,
        submitMigration,
        submitFloat: (label) => native.submitFloat(label),
        setStickyNative: (ref, id, sticky) => native.setStickyNative(ref, id, sticky),
        setMaximizedNative: (ref, maximized) => native.setMaximizedNative(ref, maximized),
        setFullscreenNative: (ref, fullscreen) => native.setFullscreenNative(ref, fullscreen),
        activeOutputName,
        activeWindowId,
        activeWindowRef,
        currentScopeId,
        getWorkspaceTilingSnapshot,
        registerShortcutFn: overrides.registerShortcutFn,
        readSameAxisMove: () => settings.readSameAxisLive(),
        readResizeSignature: () => route.resizeSignature(),
    });

    return {
        stop: () => {
            stopped = true;
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
            // Best-effort release of owned automatic keepAbove on
            // explicit stop only; the actual KWin script unload path is
            // unharnessed and makes no claim here.
            try {
                native.releaseAllAutomaticFloats("stop");
            } catch (error) {
                void error;
            }
            try {
                dragRoute.stop();
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
            const floatOutcome = handleFloatFocus(direction);
            if (floatOutcome === "handled") {
                return;
            }
            if (floatOutcome === "failed") {
                shortcutFailed("focus");
                return;
            }
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
            const floatOutcome = handleFloatMove(direction);
            if (floatOutcome === "handled") {
                return;
            }
            if (floatOutcome === "failed") {
                shortcutFailed("move");
                return;
            }
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
        // Sticky/maximize/fullscreen have no Planner command route: they
        // are synchronous native state toggles applied here with pre-setter
        // safety rechecks, then observed authoritatively on the next full
        // report (core converges the tiling consequences). Fullscreen and
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
            const stickyRaw = readProp(ref, "onAllDesktops");
            const want = (typeof stickyRaw === "boolean" ? stickyRaw : readNativeAlias(readProp, ref, "sticky")) !== true;
            if (!native.setStickyNative(ref, id, want)) {
                shortcutFailed("sticky");
            }
        },
        requestMaximize: () => {
            const ref = activeWindowRef();
            if (ref === null) {
                shortcutFailed("maximize");
                return;
            }
            try {
                native.cancelPendingUnfloat("newer-input");
            } catch (error) {
                void error;
            }
            // Maximize echo: the toggle target logs before the setter so a
            // missing finish signal still converges on the next observation.
            const mode = readProp(ref, "maximizeMode");
            const want = (typeof mode === "number" ? mode !== 0 : readNativeAlias(readProp, ref, "maximized") === true) !== true;
            log(`omnitiler:world:maximize-toggle target=${want ? "maximized" : "restored"} outcome=issued`);
            if (!native.setMaximizedNative(ref, want)) {
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
            const fsRaw = readProp(ref, "fullScreen");
            const want = (typeof fsRaw === "boolean" ? fsRaw : readNativeAlias(readProp, ref, "fullscreen") === true) !== true;
            if (!native.setFullscreenNative(ref, want)) {
                shortcutFailed("fullscreen");
            }
        },
        requestWorkspaceSelect: (index) => {
            const out = activeOutputName();
            if (out === null || !Number.isInteger(index)) {
                shortcutFailed("workspace-select");
                return;
            }
            const n = index as number;
            const cmd = n === 0 ? cmdSelectTrailing(out) : cmdSelectNumbered(out, n);
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
            const out = activeOutputName();
            if (out === null || !submitCommand(cmdSelectPrevious(out), "workspace-previous")) {
                shortcutFailed("workspace-previous");
            }
        },
        requestWorkspaceRelative: (delta) => {
            const out = activeOutputName();
            const cmd = out === null || (delta !== -1 && delta !== 1) ? null : cmdSelectRelative(out, delta);
            if (cmd === null || !submitCommand(cmd, "workspace-relative")) {
                shortcutFailed("workspace-relative");
            }
        },
        requestWorkspaceRelativeMove: (delta, follow) => {
            const out = activeOutputName();
            const w = activeWindowId();
            const cmd = out === null || w === null || (delta !== -1 && delta !== 1)
                ? null
                : cmdSendRelative(w, explicitSource(out), delta, follow !== false);
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
            const snapshot = getWorkspaceTilingSnapshot();
            if (out === null || out === "" || snapshot.scope === "") {
                shortcutFailed("toggle-workspace-tiling");
                return;
            }
            if (!submitCommand(cmdSetWorkspaceTiling(out, snapshot.scope, !snapshot.tiled), "toggle-workspace-tiling")) {
                shortcutFailed("toggle-workspace-tiling");
            }
        },
    };
}
