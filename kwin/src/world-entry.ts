// World production entry (D1-D5): the single active KDE policy path over
// DiscoverWorld/DescribeWorld. Legacy plan modules remain in-tree
// for native-only callers and offline fixtures but are NOT called here and
// are never a fallback. Planner is the SOLE workspace policy: every
// selection/send/migration shortcut builds a world command; this module
// never decides trailing reuse/create, history, hotplug mapping, or
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
import {
    WORLD_MAX_REPLY_BYTES,
    WORLD_MAX_REQUEST_BYTES,
    isOpaqueId,
    utf8ByteLength,
} from "./world-seq";
import {
    buildWorldReportJson,
    cmdMigrateWorkspace,
    cmdSelectNumbered,
    cmdSelectPrevious,
    cmdSelectRelative,
    cmdSelectTrailing,
    cmdSend,
    cmdSendNumbered,
    cmdSendRelative,
    cmdSetWorkspaceTiling,
    isWorldRejection,
    parseWorldReply,
    type WorldCommand,
} from "./world-report";
import { captureWorldObservation, type WorldObservation } from "./world-observe";
import { WorldAdapterState } from "./world-state";
import { applyWorldReply } from "./world-apply";
import { decodeList } from "./qml-list";
import { normalizeNativeId } from "./native-id";
import { readNativeAlias } from "./native-alias";
import { createWorldCloseTracker } from "./world-close-tracker";
import { createWorldCommandContext } from "./world-command-context";
import { createWorldSettingsLive } from "./world-settings-live";
import { WorldBornMaximizedAdmission } from "./world-born-maximized";

export const WORLD_DBUS_SERVICE = "com.omnitiler.Planner";
export const WORLD_DBUS_OBJECT = "/com/omnitiler/Planner";
export const WORLD_DBUS_INTERFACE = "com.omnitiler.Planner1";
export const WORLD_DISCOVERY_METHOD = "DiscoverWorld";
export const WORLD_DESCRIBE_METHOD = "DescribeWorld";
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
    let pendingCommand: WorldCommand | null = null;
    let reqCounter = 0;
    // Adapter-owned logical<->native mapping snapshot for report bindings,
    // converged from accepted replies (Planner-owned) plus pending tokens.
    // Only the logical id and output converge here; pin/disp/managed are
    // derived per report in pinRetained below (never stored constants).
    const logicalOfNative = new Map<string, { logical: string; out: string }>();
    const detaches: Array<() => void> = [];
    // Explicit close tracking (D5): canonical live-id cache plus
    // event-gated difference (see world-close-tracker). The removal payload
    // is never read; core treats bare absence as minimize, never close.
    const closeTracker = createWorldCloseTracker();
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
        readMode: settings.readMode,
        readGap: () => settings.readGapLive(),
        readOuterGap: () => settings.readOuterGapLive(),
        gestureActive,
        knownClosed: () => closeTracker.snapshotClosed(),
        logicalBindingOf: (nativeId) => {
            // Locally-created but not yet core-acknowledged bindings publish
            // here so core pending consumes them: a bound pending creation
            // overrides only an absent entry or a generic native->native
            // fallback echo (logical === native). A real core mapping stays
            // authoritative and the pending row is pruned on accept.
            const pending = state.pendingCreations.find((c) => c.bound === nativeId);
            const base = logicalOfNative.get(nativeId);
            if (
                pending !== undefined && pending.logical !== "" && pending.out !== "" &&
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
                // live internalId is never compared unnormalized.
                return normalizeNativeId(readProp(active as object, "internalId"));
            }
        } catch (error) {
            void error;
        }
        return null;
    };

    // Observation-derived local command context (workspace refs,
    // retention, resize signature, cross-output scope, adjacent targets)
    // lives in world-command-context; lifecycle state stays here.
    const route = createWorldCommandContext({
        observe: () => capture(),
        activeWindowId,
        isDirection,
        isCreationBound: (nativeId) => state.pendingCreations.some((c) => c.bound === nativeId),
        activeCommand: () => state.observationOnlyCatchup ? null : pendingCommand,
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

    const submitCommandInner = (command: WorldCommand, label: string): boolean => {
        if (stopped) {
            return false;
        }
        if (state.boundGen === null) {
            log(`omnitiler:world:command-refused op=${token((command as { op?: unknown }).op)} reason=undiscovered`);
            return false;
        }
        if (pendingCommand !== null || state.inFlightReq !== null) {
            // Distinct commands never silently coalesce: fail fast as busy.
            log(`omnitiler:world:command-busy op=${token((command as { op?: unknown }).op)}`);
            return false;
        }
        state.invalidate();
        pendingCommand = command;
        log(`omnitiler:world:command op=${token((command as { op?: unknown }).op)} label=${token(label)} obs=${state.currentObs}`);
        scheduleDispatch();
        return true;
    };

    // Newer explicit input supersedes a deferred B9 unfloat immediately:
    // every command routed here cancels the pending attempt (which ends
    // narrowly with no retry) before the new command submits. The B9
    // continuation itself submits through submitCommandInner so it
    // cannot cancel itself. Assigned after the native toggles exist;
    // calls before that are no-ops.
    let cancelPendingForNewerInput: (() => void) | null = null;
    const submitCommand = (command: WorldCommand, label: string): boolean => {
        try {
            cancelPendingForNewerInput?.();
        } catch (error) {
            void error;
        }
        return submitCommandInner(command, label);
    };

    // Float-origin directional focus/move lives in world-float-nav (native
    // only, same semantics): the entry owns lifecycle state and flight
    // ordering and injects them as one busy boolean plus callbacks, never
    // copied state.
    const floatNav = createFloatNav({
        surface,
        readProp,
        log,
        isBusy: () => state.inFlightReq !== null || pendingCommand !== null,
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

    // Explicit output send by direction: observation resolves the strict
    // FULL-topology target plus its current desktop; core owns the
    // membership decision. Without a resolvable target or a readable
    // target view, refuse with no writes.
    const submitOutputSend = (direction: string, follow: boolean): boolean => {
        const obs = route.observeForCommand();
        const out = activeOutputName();
        const w = activeWindowId();
        if (obs === null || out === null || w === null || !isDirection(direction)) {
            return false;
        }
        const target = route.adjacentTarget(obs, out, direction);
        if (target === null) {
            return false;
        }
        return submitCommand(cmdSend(w, target.out, target.ws, follow), `send-output-${direction}`);
    };

    // Whole-workspace migration by direction: observation resolves the
    // strict FULL-topology target and the source's current workspace; the
    // scope policy decision stays in core. Without a resolvable target,
    // refuse with no writes.
    const submitMigration = (direction: string): boolean => {
        const obs = route.observeForCommand();
        const out = activeOutputName();
        if (obs === null || out === null || !isDirection(direction)) {
            return false;
        }
        const topo = obs.topology.find((t) => t.out === out);
        const target = route.adjacentTarget(obs, out, direction);
        if (topo === undefined || topo.current === "" || target === null) {
            return false;
        }
        const cmd = cmdMigrateWorkspace(out, target.out, topo.current);
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
        const cmd = cmdSendNumbered(w, out, index, follow);
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
        submitCommand,
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

    const discover = (): void => {
        if (stopped) {
            return;
        }
        reqCounter += 1;
        const reqId = `disc-${reqCounter}`;
        const payload = `{"v":1,"adapter":${JSON.stringify(adapter)},"req":${JSON.stringify(reqId)}}`;
        let settled = false;
        const timeoutCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            if (!settled) {
                settled = true;
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
                    timeoutCancel();
                    if (typeof reply !== "string") {
                        log("omnitiler:world:discover-transport recovery=on-next-event");
                        return;
                    }
                    const parsed = parseWorldReply(reply);
                    if (parsed === null || isWorldRejection(parsed)) {
                        log("omnitiler:world:discover-rejected recovery=on-next-event");
                        return;
                    }
                    // Authenticated discovery: the reply must name this
                    // adapter namespace and the exact outstanding discovery
                    // req. Anything else binds nothing and resets nothing.
                    if (parsed.adapter !== adapter || parsed.req !== reqId) {
                        log("omnitiler:world:discover-rejected recovery=on-next-event");
                        return;
                    }
                    state.onDiscovery(parsed.gen, parsed.adapter, parsed.req, reqId);
                    log(`omnitiler:world:discovered gen=${token(parsed.gen)}`);
                    state.invalidate();
                    void dispatch(false);
                },
            );
        } catch (error) {
            void error;
            timeoutCancel();
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

    const dispatch = (isCatchup: boolean): void => {
        void isCatchup;
        if (stopped || state.boundGen === null) {
            return;
        }
        if (state.inFlightReq !== null) {
            return;
        }
        // Observation-only catchup after a stale reply drops the command.
        const command = state.observationOnlyCatchup ? null : pendingCommand;
        if (state.observationOnlyCatchup) {
            pendingCommand = null;
        }
        const observation = capture();
        if (observation === null) {
            log("omnitiler:world:observe-invalid recovery=on-next-event");
            return;
        }
        const begun = state.beginDispatch(command);
        if (begun === null) {
            return;
        }
        const json = buildWorldReportJson({
            observation, reqSeq: begun.reqSeq, cmdSeq: begun.cmdSeq, reqId: begun.reqId, command,
            owner, adapter,
            boundGen: state.boundGen,
            currentObs: state.currentObs,
            pendingFeedback: state.pendingFeedback,
            defaultTiled: settings.readDefaultTiled(),
            refill: settings.readRefillLive(),
            anchorFallback: settings.readAnchorFallbackLive(),
            noSpaceResult: settings.readNoSpaceResultLive(),
        });
        if (json === null) {
            state.noteRejectedOrTransportError();
            return;
        }
        if (utf8ByteLength(json) > WORLD_MAX_REQUEST_BYTES) {
            // Narrow overflow recovery: drop the command, keep observation
            // dirty for a smaller scope/observation on the next event. Never
            // truncate or apply partial pages.
            log("omnitiler:world:request-oversize recovery=on-next-event");
            pendingCommand = null;
            state.noteRejectedOrTransportError();
            return;
        }
        const dispatchObs = state.currentObs;
        let settled = false;
        const timeoutCancel = scheduleOnce(WORLD_TIMEOUT_MS, () => {
            if (!settled) {
                settled = true;
                log("omnitiler:world:timeout recovery=on-next-event");
                pendingCommand = null;
                state.noteRejectedOrTransportError();
                killPending("timeout");
            }
        });
        log(`omnitiler:world:dispatch req=${token(begun.reqId)} obs=${dispatchObs} cmd=${command === null ? "observe" : token((command as { op?: unknown }).op)}`);
        try {
            (callDbus as NonNullable<WorldEntryOverrides["callDbus"]>)(
                WORLD_DBUS_SERVICE, WORLD_DBUS_OBJECT, WORLD_DBUS_INTERFACE,
                WORLD_DESCRIBE_METHOD, json, (reply: unknown) => {
                    if (settled) {
                        return;
                    }
                    settled = true;
                    timeoutCancel();
                    if (typeof reply !== "string") {
                        log("omnitiler:world:transport-error recovery=on-next-event");
                        pendingCommand = null;
                        state.noteRejectedOrTransportError();
                        killPending("transport");
                        return;
                    }
                    if (utf8ByteLength(reply) > WORLD_MAX_REPLY_BYTES) {
                        log("omnitiler:world:reply-oversize recovery=on-next-event");
                        pendingCommand = null;
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        return;
                    }
                    const parsed = parseWorldReply(reply);
                    if (parsed === null) {
                        log("omnitiler:world:reply-malformed recovery=on-next-event");
                        pendingCommand = null;
                        state.noteRejectedOrTransportError();
                        killPending("rejected");
                        return;
                    }
                    if (isWorldRejection(parsed)) {
                        if (parsed.kind === "generation-mismatch") {
                            // Poison/restart: re-discover (fresh handshake),
                            // never replay the command.
                            log("omnitiler:world:generation-mismatch recovery=rediscover");
                            pendingCommand = null;
                            state.inFlightReq = null;
                            killPending("generation-mismatch");
                            discover();
                            return;
                        }
                        if (parsed.kind === "busy" || parsed.kind === "unavailable") {
                            pendingCommand = null;
                            state.noteRejectedOrTransportError();
                            killPending("busy");
                            return;
                        }
                        log(`omnitiler:world:rejected kind=${token(parsed.kind)} msg=${token(parsed.message)} recovery=on-next-event`);
                        pendingCommand = null;
                        state.inFlightReq = null;
                        state.observationOnlyCatchup = true;
                        state.dirty = true;
                        killPending("rejected");
                        return;
                    }
                    // Previous accepted snapshot for born-maximized applied
                    // evidence: the new reply contains birth rows and must
                    // not misclassify them as later maximizes.
                    const prevReply = state.lastReply;
                    const verdict = state.acceptReply(parsed, dispatchObs);
                    if (!verdict.applied) {
                        log(`omnitiler:world:stale reason=${token(verdict.reason)} recovery=catchup`);
                        pendingCommand = null;
                        // Consume exactly the flight this callback owns: the
                        // stale record otherwise wedges every later dispatch
                        // (single-flight busy forever) even though its timeout
                        // already settled. Dirty work, closes, and feedback
                        // are retained; the redispatch below is
                        // observation-only, never a command replay.
                        state.releaseFlight(begun.reqId);
                        try {
                            dragRoute.notifyStaleReply(verdict.reason);
                        } catch (error) {
                            void error;
                        }
                        if (state.dirty) {
                            scheduleDispatch();
                        }
                        return;
                    }
                    pendingCommand = null;
                    // The accepted reply settles the preview flight: the
                    // derived core rect renders (identical repeats never
                    // re-set), the hover prior chains, and the coalesced
                    // pending tick dispatches from the latest pointer.
                    try {
                        dragRoute.notifyReplyAccepted(parsed.result);
                    } catch (error) {
                        void error;
                    }
                    // Converge the adapter-owned binding snapshot from the
                    // accepted Planner mapping (Planner-owned, D2), then
                    // prune to the current native inventory so stale
                    // bindings never survive removals/disconnects.
                    for (const ws of parsed.workspaces) {
                        if (ws.bind !== "") {
                            logicalOfNative.set(ws.bind, {
                                logical: ws.logical !== "" ? ws.logical : ws.bind,
                                out: ws.out,
                            });
                            // Handoff: a real (non-fallback) core mapping
                            // consumes the matching pending creation so the
                            // stale pending out never overrides a later
                            // legitimate migration. Generic fallback echoes
                            // never consume the pending logical.
                            if (ws.logical !== "" && ws.logical !== ws.bind) {
                                state.acknowledgeCreationBinding(ws.bind, ws.logical);
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
                        const untouched = new Set<string>();
                        for (const row of parsed.windows) {
                            if (row.frame_auth === "untouched" && row.frame_hold === "") {
                                untouched.add(row.win);
                            }
                        }
                        for (const id of [...floatingIds]) {
                            if (!untouched.has(id)) {
                                floatingIds.delete(id);
                            }
                        }
                        try {
                            if (
                                parsed.result !== null && parsed.result.op === "toggle-float" &&
                                parsed.result.rect === null && parsed.result.noSpace !== true
                            ) {
                                tileIds.add(parsed.result.win);
                            } else if (
                                parsed.result !== null && parsed.result.op === "toggle-float" &&
                                parsed.result.rect === null && parsed.result.noSpace === true
                            ) {
                                // Explicit-tile no-space retry (D5): the
                                // window stays floated, so no tile override
                                // is recorded and nothing re-tiles.
                                log(`omnitiler:world:toggle-float-no-space win=${token(parsed.result.win)}`);
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
                        const liveNatives = new Set<string>();
                        for (const t of fresh.topology) {
                            for (const id of t.order) {
                                liveNatives.add(id);
                            }
                        }
                        for (const key of [...logicalOfNative.keys()]) {
                            if (!liveNatives.has(key)) {
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
                        // Prune departed lifetimes, retaining the same settled
                        // membership as the accepted rows and live inventory.
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
                    // Withdraw re-float tile overrides after capture, keeping
                    // fixed-size clients tile-classified for explicit float
                    // placement. Both sets persist through WriteFloatIntent.
                    try {
                        if (
                            parsed.result !== null && parsed.result.op === "toggle-float" &&
                            parsed.result.rect !== null
                        ) {
                            tileIds.delete(parsed.result.win);
                        }
                    } catch (error) {
                        void error;
                    }
                    // Route the read-only active-group result to the native                    // highlight renderer (no new border policy): the effect
                    // remains the renderer, this only forwards core-derived
                    // bounds with the legacy fullscreen fail-closed gate.
                    // no-group clears. Never derives topology or geometry.
                    try {
                        if (parsed.result !== null && parsed.result.op === "active-group") {
                            const members = parsed.result.members.map((m) => m.win);
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
                                const rev = Number(parsed.result.rev);
                                const payload = formatGroupHighlightPayload(
                                    parsed.req, owner, state.boundGen, rev,
                                    parsed.result.group, parsed.result.win, members,
                                    { x: parsed.result.bounds[0], y: parsed.result.bounds[1], w: parsed.result.bounds[2], h: parsed.result.bounds[3] },
                                );
                                if (payload !== null) {
                                    highlightCall(GROUP_HIGHLIGHT_SET_METHOD, payload);
                                } else {
                                    highlightCall(GROUP_HIGHLIGHT_CLEAR_METHOD);
                                }
                            } else {
                                highlightCall(GROUP_HIGHLIGHT_CLEAR_METHOD);
                            }
                        } else if (parsed.result !== null && parsed.result.op === "no-group") {
                            highlightCall(GROUP_HIGHLIGHT_CLEAR_METHOD);
                        }
                    } catch (error) {
                        void error;
                    }
                    const applied = applyWorldReply({
                        state,
                        observation: fresh,
                        reply: parsed,
                        log,
                        ...createWorldApplyHost(surface, readProp),
                    });
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
                    // Dirty work coalesces to the newest full report.
                    if (state.dirty) {
                        scheduleDispatch();
                    }
                },
            );
        } catch (error) {
            void error;
            timeoutCancel();
            pendingCommand = null;
            state.noteRejectedOrTransportError();
            killPending("transport");
        }
    };

    // Subscribe to native signals: every invalidation advances CURRENT obs
    // synchronously, before debounce. Event-driven only; no polling.
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
        const detach = connectSignal(readSignal(surface, "windowRemoved"), () => {
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
        const last = state.lastReply;
        if (last !== null) {
            for (const ws of last.workspaces) {
                if (ws.bind === scope) {
                    return { scope, tiled: ws.mode !== "floating", defaultTiled: saved };
                }
            }
            const out = activeOutputName();
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
        return topo?.current ?? "";
    };

    // Authenticated generation discovery on start; restart/poison paths
    // re-enter through the generation-mismatch branch above. The drag-oracle
    // verdict route attaches with the native surface; float membership seeds
    // from the runtime store before the first dispatch.
    dragRoute.attach();
    seedFloatIntent();
    discover();
    log(`omnitiler:world:ready ${state.describeForLog()}`);

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
        isWorldBusy: () => state.inFlightReq !== null || pendingCommand !== null,
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
        crossForDirection: route.crossForDirection,
        registerShortcutFn: overrides.registerShortcutFn,
        readSameAxisMove: () => settings.readSameAxisLive(),
        readResizeSignature: () => route.resizeSignature(),
    });

    return {
        stop: () => {
            stopped = true;
            try {
                native.cancelPendingUnfloat();
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
            const cross = (direction === "left" || direction === "right") && route.crossForDirection(direction);
            if (!submitCommand({ op: "operation", command: { op: "focus", win: w, direction, crossOutput: cross, floatSubject: false } }, "focus")) {
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
            if (w === null || !submitCommand({
                op: "operation",
                command: { op: "move", win: w, direction, crossOutput: route.crossForDirection(direction), sameAxis: settings.readSameAxisLive() },
            }, "move")) {
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
                : cmdSendRelative(w, out, delta, follow !== false);
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
