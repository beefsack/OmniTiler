import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    WORKSPACE_SEND_CONTRACT_VERSION,
    WORKSPACE_SEND_DBUS_SERVICE,
    WORKSPACE_SEND_GET_OWNER_METHOD,
    WORKSPACE_SEND_HAS_OWNER_METHOD,
    WORKSPACE_SEND_METHOD,
    WORKSPACE_SEND_START_METHOD,
    WorkspaceSendAdapter,
    type WorkspaceSendAdapterEnv,
    type WorkspaceSendObserved,
} from "../src/workspace-send-adapter";
import { startWorkspaceSendAdapterEntry } from "../src/workspace-send-adapter-entry";
import { observeOutputSendTarget, observeSendTarget } from "../src/plan-adapter-entry";

// D16 approved floating+fullscreen / floating+maximized combo workspace
// transfer (option (a), no setting): explicit numbered/relative follow/stay
// send of an intentional floating window carrying exactly one native
// overlay through the shared slotless `move-deferred` route. The combo
// mover (slotless exception with retained floating size) transfers to the
// target homing with NO tree mutation and NO source/target reflow: the
// reply geometry covers tiled survivors only, the mover takes no
// allocation and no geometry write, its live frame/size is kept as-is,
// and its exact overlay flag is retained on arrival with zero overlay
// setters. Follow switches and focuses the mover native ref; stay focuses
// the source MRU survivor with no switch. Sticky combos, dual-overlay
// floats, and automatic-fixed movers keep refusing. Offline only:
// adapter-level harness with canned Engine replies plus real entry
// observation shapes.

function rect(x: number, y: number, w: number, h: number): { x: number; y: number; w: number; h: number } {
    return { x, y, w, h };
}

const COMBO_RECT = rect(100, 100, 400, 300);

type Combo = "fullscreen" | "maximized";

const COMBOS: ReadonlyArray<Combo> = ["fullscreen", "maximized"];

interface ComboWorld {
    readonly windows: Array<{
        id: string;
        ref: object;
        rect: { x: number; y: number; w: number; h: number };
        workspace: string;
        floating: boolean;
        sticky: boolean;
        maximized: boolean;
        fullscreen: boolean;
    }>;
}

function comboWorld(refs: { f: object; s: object; t: object }, combo: Combo): ComboWorld {
    return {
        windows: [
            // Slotless combo mover: intentional float still floating with
            // exactly one overlay and no tile slot.
            {
                id: "win-f",
                ref: refs.f,
                rect: { ...COMBO_RECT },
                workspace: "ws-1",
                floating: true,
                sticky: false,
                maximized: combo === "maximized",
                fullscreen: combo === "fullscreen",
            },
            { id: "win-s", ref: refs.s, rect: rect(0, 0, 1200, 800), workspace: "ws-1", floating: false, sticky: false, maximized: false, fullscreen: false },
            { id: "win-t", ref: refs.t, rect: rect(0, 0, 1200, 800), workspace: "ws-2", floating: false, sticky: false, maximized: false, fullscreen: false },
        ],
    };
}

// Source follows the mover only at dispatch; re-observations pin the
// dispatch source like the real entries (pinnedSourceWorkspace), so a
// transferred mover reads on the target while the scope stays frozen.
function comboObserved(
    world: ComboWorld,
    refs: { f: object; s: object; t: object; desktop1: object; desktop2: object },
    targetWorkspace: string,
    activeRef: object | null,
    pinnedSourceWorkspace?: string,
): WorkspaceSendObserved {
    const flagged = (entry: ComboWorld["windows"][number]): Record<string, unknown> =>
        Object.freeze({
            id: entry.id,
            ref: entry.ref,
            rect: Object.freeze({ ...entry.rect }),
            fullscreen: entry.fullscreen,
            maximized: entry.maximized,
            floating: entry.floating,
            sticky: entry.sticky,
            fitExcluded: entry.floating || entry.sticky || entry.fullscreen || entry.maximized,
            fit_excluded: entry.floating || entry.sticky || entry.fullscreen || entry.maximized,
        });
    const mover = world.windows.find((entry) => entry.id === "win-f");
    const sourceWorkspace = pinnedSourceWorkspace ?? mover?.workspace ?? "ws-1";
    const source = world.windows.filter((entry) => entry.workspace === sourceWorkspace);
    const activeId = world.windows.find((entry) => entry.ref === activeRef)?.id ?? "";
    const focused = source.some((entry) => entry.id === activeId) ? activeId : "";
    const moverEntry = source.find((entry) => entry.id === activeId);
    return {
        sourceOutput: "out-1",
        sourceWorkspace,
        sourceBounds: Object.freeze(rect(0, 0, 1200, 800)),
        targetOutput: "out-1",
        targetWorkspace,
        targetBounds: Object.freeze(rect(0, 0, 1200, 800)),
        focusedId: focused,
        sourceWindows: Object.freeze(source.map(flagged)),
        targetWindows: Object.freeze(world.windows.filter((entry) => entry.workspace === targetWorkspace).map(flagged)),
        activeRef,
        moverRef: moverEntry === undefined ? null : moverEntry.ref,
        targetDesktopRef: targetWorkspace === "ws-2" ? refs.desktop2 : refs.desktop1,
        targetExists: true,
        desktopCount: 2,
        sourceFingerprint: "sfp-1",
        targetFingerprint: "tfp-1",
        currentWorkspace: sourceWorkspace,
    } as unknown as WorkspaceSendObserved;
}

function comboReply(correlation: string, follow: boolean, sourceWorkspace: string, targetWorkspace: string): string {
    const sourceSurvivor = sourceWorkspace === "ws-1" ? "win-s" : "win-t";
    const targetMember = targetWorkspace === "ws-2" ? "win-t" : "win-s";
    return JSON.stringify({
        v: WORKSPACE_SEND_CONTRACT_VERSION,
        correlation_id: correlation,
        outcome: "planned",
        kind: "send-to-workspace",
        base_revision: 0,
        detail: { kind: "send-to-workspace", policy_version: 1, capability: "move-tiled" },
        // Tiled survivors only, allocations exactly retained (never
        // reflow); the slotless combo mover takes no allocation. Follow
        // books the target MRU tile (native follow still addresses the
        // mover ref); stay books the source MRU survivor.
        desired_geometry: [
            { window: sourceSurvivor, leaf: `leaf-${sourceSurvivor}`, output: "out-1", workspace: sourceWorkspace, rect: { x: 0, y: 0, w: 1200, h: 800 } },
            { window: targetMember, leaf: `leaf-${targetMember}`, output: "out-1", workspace: targetWorkspace, rect: { x: 0, y: 0, w: 1200, h: 800 } },
        ],
        desired_focus: follow
            ? { domain_output: "out-1", domain_workspace: targetWorkspace, leaf: `leaf-${targetMember}` }
            : { domain_output: "out-1", domain_workspace: sourceWorkspace, leaf: `leaf-${sourceSurvivor}` },
        preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
        operation: {
            op: "move-deferred",
            window: "win-f",
            source_output: "out-1",
            source_workspace: sourceWorkspace,
            target_output: "out-1",
            target_workspace: targetWorkspace,
            follow,
        },
    });
}

interface ComboFlight {
    readonly refs: { f: object; s: object; t: object; desktop1: object; desktop2: object };
    readonly world: ComboWorld;
    readonly geometries: object[];
    readonly moverWrites: object[];
    readonly switches: object[];
    readonly focuses: object[];
    readonly logs: string[];
    readonly dbusCalls: Array<{ method: string; payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly adapter: WorkspaceSendAdapter;
    activeRef: object | null;
}

function startComboFlight(combo: Combo): ComboFlight {
    const refs = { f: {}, s: {}, t: {}, desktop1: {}, desktop2: {} };
    const world = comboWorld(refs, combo);
    const geometries: object[] = [];
    const moverWrites: object[] = [];
    const switches: object[] = [];
    const focuses: object[] = [];
    const logs: string[] = [];
    const dbusCalls: ComboFlight["dbusCalls"] = [];
    const callbacks: ComboFlight["callbacks"] = [];
    const flight: ComboFlight = {
        refs,
        world,
        geometries,
        moverWrites,
        switches,
        focuses,
        logs,
        dbusCalls,
        callbacks,
        adapter: null as unknown as WorkspaceSendAdapter,
        activeRef: refs.f,
    };
    const env: WorkspaceSendAdapterEnv = {
        callDbus: (_service, _path, _iface, method, payload, callback) => {
            if (method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                callback(true);
                return;
            }
            dbusCalls.push({ method, payload });
            callbacks.push(callback);
        },
        scheduleOnce: (): (() => void) => (): void => {},
        log: (message) => {
            logs.push(message);
        },
        observe: (targetWorkspace, pinnedSourceWorkspace) =>
            comboObserved(world, refs, targetWorkspace as string, flight.activeRef, pinnedSourceWorkspace as string | undefined),
        onSettled: () => {},
        setGeometry: (target) => {
            geometries.push(target);
            return true;
        },
        setDesktops: (target) => {
            moverWrites.push(target);
            for (const entry of world.windows) {
                if (entry.ref === target) {
                    entry.workspace = ((): string => {
                        const current = entry.workspace;
                        return current === "ws-1" ? "ws-2" : "ws-1";
                    })();
                }
            }
            return true;
        },
        switchToTarget: (desktopRef) => {
            switches.push(desktopRef);
            return true;
        },
        focusWindow: (windowRef) => {
            focuses.push(windowRef);
            flight.activeRef = windowRef;
            return true;
        },
    };
    (flight as { adapter: WorkspaceSendAdapter }).adapter = new WorkspaceSendAdapter(env);
    assert.equal(flight.adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
    return flight;
}

// Ordinal is diagnostic-only here (2 numbered, -1 absent/invalid): real
// numbered/relative ring resolution lives in the entry routes. It never
// gates request behavior; both ordinals must ride the deferred route.
function dispatchCombo(flight: ComboFlight, combo: Combo, target: string, ordinal: unknown, follow: unknown): string {
    const base = flight.dbusCalls.length;
    assert.equal(flight.adapter.requestSend(target, ordinal, follow), true, "combo carry must dispatch");
    assert.equal(flight.dbusCalls[base]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
    flight.callbacks[base]?.(":1.7");
    const body = JSON.parse(flight.dbusCalls[base + 1]?.payload ?? "{}") as Record<string, unknown>;
    // The wire carries the combo mover still floating with the fit
    // opt-out and exactly its own overlay flag.
    const windows = body["windows"] as Array<Record<string, unknown>>;
    const mover = windows.find((entry) => entry["window"] === "win-f") as Record<string, unknown>;
    assert.equal(mover["floating"], true, "combo mover rides the send wire still floating");
    assert.equal(mover["fit_excluded"], true, "combo mover rides fit-excluded");
    if (combo === "fullscreen") {
        assert.equal(mover["fullscreen"], true, "fullscreen combo rides still fullscreen");
        assert.equal(mover["maximized"], undefined, "fullscreen combo rides never maximized");
    } else {
        assert.equal(mover["maximized"], true, "maximized combo rides still maximized");
        assert.equal(mover["fullscreen"], undefined, "maximized combo rides never fullscreen");
    }
    assert.equal(body["focused_window"], "win-f", "explicit send focuses the combo mover");
    return body["correlation_id"] as string;
}

for (const combo of COMBOS) {
    describe(`D16 slotless combo carry ${combo} (adapter follow/stay)`, () => {
        it("numbered follow carries the combo with one membership write and zero geometry", () => {
            const flight = startComboFlight(combo);
            const correlation = dispatchCombo(flight, combo, "ws-2", 2, true);
            flight.callbacks[flight.callbacks.length - 1]?.(comboReply(correlation, true, "ws-1", "ws-2"));
            assert.equal(flight.moverWrites.length, 1, "mover membership written once");
            assert.deepEqual(flight.moverWrites[0], flight.refs.f);
            assert.deepEqual(flight.geometries, [], "no geometry write: allocations never reflow");
            assert.deepEqual(flight.world.windows.find((entry) => entry.id === "win-f")?.rect, COMBO_RECT, "arrival frame kept as-is");
            const mover = flight.world.windows.find((entry) => entry.id === "win-f");
            assert.equal(mover?.floating, true, "intentional float retained on arrival");
            assert.equal(combo === "fullscreen" ? mover?.fullscreen : mover?.maximized, true, "exact overlay retained on arrival");
            assert.equal(combo === "fullscreen" ? mover?.maximized : mover?.fullscreen, false, "no overlay invented on arrival");
            assert.deepEqual(flight.switches, [flight.refs.desktop2], "follow switches exactly once");
            assert.deepEqual(flight.focuses, [flight.refs.f], "follow focuses the slotless combo mover");
            assert.equal(flight.adapter.isInFlight, false);
        });

        it("numbered stay carries the combo with source MRU focus and no switch", () => {
            const flight = startComboFlight(combo);
            const correlation = dispatchCombo(flight, combo, "ws-2", 2, false);
            flight.callbacks[flight.callbacks.length - 1]?.(comboReply(correlation, false, "ws-1", "ws-2"));
            assert.equal(flight.moverWrites.length, 1, "mover membership written once");
            assert.deepEqual(flight.geometries, [], "no geometry write on stay either");
            const mover = flight.world.windows.find((entry) => entry.id === "win-f");
            assert.equal(mover?.floating, true, "float retained on stay arrival");
            assert.equal(combo === "fullscreen" ? mover?.fullscreen : mover?.maximized, true, "overlay retained on stay arrival");
            assert.deepEqual(flight.switches, [], "stay never switches desktops");
            assert.deepEqual(flight.focuses, [flight.refs.s], "stay focuses the source MRU survivor");
            assert.equal(flight.adapter.isInFlight, false);
        });

        it("diagnostic ordinal -1 follow carries the combo like the numbered route", () => {
            const flight = startComboFlight(combo);
            const correlation = dispatchCombo(flight, combo, "ws-2", -1, true);
            flight.callbacks[flight.callbacks.length - 1]?.(comboReply(correlation, true, "ws-1", "ws-2"));
            assert.equal(flight.moverWrites.length, 1, "mover membership written once");
            assert.deepEqual(flight.geometries, [], "no geometry write on the relative route");
            assert.deepEqual(flight.switches, [flight.refs.desktop2], "relative follow switches exactly once");
            assert.deepEqual(flight.focuses, [flight.refs.f], "relative follow focuses the mover");
            assert.equal(flight.adapter.isInFlight, false);
        });

        it("diagnostic ordinal -1 stay carries the combo with source MRU focus", () => {
            const flight = startComboFlight(combo);
            const correlation = dispatchCombo(flight, combo, "ws-2", -1, false);
            flight.callbacks[flight.callbacks.length - 1]?.(comboReply(correlation, false, "ws-1", "ws-2"));
            assert.equal(flight.moverWrites.length, 1, "mover membership written once");
            assert.deepEqual(flight.switches, [], "relative stay never switches desktops");
            assert.deepEqual(flight.focuses, [flight.refs.s], "relative stay focuses the source MRU survivor");
            assert.equal(flight.adapter.isInFlight, false);
        });

        it("an overlay-toggle race refuses without setters or follow", () => {
            const flight = startComboFlight(combo);
            const correlation = dispatchCombo(flight, combo, "ws-2", 2, true);
            for (const entry of flight.world.windows) {
                if (entry.ref === flight.refs.f) {
                    if (combo === "fullscreen") {
                        entry.fullscreen = false;
                    } else {
                        entry.maximized = false;
                    }
                }
            }
            flight.callbacks[flight.callbacks.length - 1]?.(comboReply(correlation, true, "ws-1", "ws-2"));
            assert.deepEqual(flight.moverWrites, [], "overlay-changed mover takes no membership write");
            assert.deepEqual(flight.geometries, [], "overlay-changed mover takes no geometry writes");
            assert.deepEqual(flight.switches, [], "overlay-changed mover never switches");
            assert.deepEqual(flight.focuses, [], "overlay-changed mover never focuses");
        });

        it("a mover float-untoggle race refuses without setters or follow", () => {
            const flight = startComboFlight(combo);
            const correlation = dispatchCombo(flight, combo, "ws-2", 2, true);
            for (const entry of flight.world.windows) {
                if (entry.ref === flight.refs.f) {
                    entry.floating = false;
                }
            }
            flight.callbacks[flight.callbacks.length - 1]?.(comboReply(correlation, true, "ws-1", "ws-2"));
            assert.deepEqual(flight.moverWrites, [], "flag-changed mover takes no membership write");
            assert.deepEqual(flight.geometries, [], "flag-changed mover takes no geometry writes");
            assert.deepEqual(flight.switches, [], "flag-changed mover never switches");
            assert.deepEqual(flight.focuses, [], "flag-changed mover never focuses");
        });

        it("empty geometry carries a sole combo mover with null focus", () => {
            const flight = startComboFlight(combo);
            flight.world.windows.length = 0;
            flight.world.windows.push(
                {
                    id: "win-f",
                    ref: flight.refs.f,
                    rect: { ...COMBO_RECT },
                    workspace: "ws-1",
                    floating: true,
                    sticky: false,
                    maximized: combo === "maximized",
                    fullscreen: combo === "fullscreen",
                },
            );
            const correlation = dispatchCombo(flight, combo, "ws-2", 2, true);
            const reply = JSON.parse(comboReply(correlation, true, "ws-1", "ws-2")) as Record<string, unknown>;
            reply["desired_geometry"] = [];
            reply["desired_focus"] = null;
            flight.callbacks[flight.callbacks.length - 1]?.(JSON.stringify(reply));
            assert.equal(flight.moverWrites.length, 1, "mover membership written once");
            assert.deepEqual(flight.moverWrites[0], flight.refs.f);
            assert.deepEqual(flight.geometries, [], "no geometry to write");
            assert.deepEqual(flight.switches, [flight.refs.desktop2], "follow switches exactly once");
            assert.deepEqual(flight.focuses, [flight.refs.f], "follow focuses the slotless combo mover");
            assert.equal(flight.adapter.isInFlight, false);
        });

        it("drifted survivor allocations still take zero geometry writes", () => {
            const flight = startComboFlight(combo);
            const correlation = dispatchCombo(flight, combo, "ws-2", 2, true);
            const reply = JSON.parse(comboReply(correlation, true, "ws-1", "ws-2")) as Record<string, unknown>;
            const geometry = reply["desired_geometry"] as Array<Record<string, unknown>>;
            for (const entry of geometry) {
                entry["rect"] = { x: 111, y: 222, w: 333, h: 444 };
            }
            flight.callbacks[flight.callbacks.length - 1]?.(JSON.stringify(reply));
            assert.equal(flight.moverWrites.length, 1, "membership still transfers");
            assert.deepEqual(flight.geometries, [], "drifted survivors take zero geometry writes");
            assert.deepEqual(flight.switches, [flight.refs.desktop2], "follow still switches");
            assert.deepEqual(flight.focuses, [flight.refs.f], "follow still focuses the mover");
            assert.equal(flight.adapter.isInFlight, false);
        });
    });
}

describe("D16 slotless combo carry (shape fences)", () => {
    it("a leafless transfer for a sticky combo mover never actuates", () => {
        const flight = startComboFlight("fullscreen");
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.sticky = true;
            }
        }
        assert.equal(flight.adapter.requestSend("ws-2", 2, true), true, "sticky combo dispatches at the harness level");
        flight.callbacks[0]?.(":1.7");
        const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
        const correlation = body["correlation_id"] as string;
        flight.callbacks[flight.callbacks.length - 1]?.(comboReply(correlation, true, "ws-1", "ws-2"));
        assert.deepEqual(flight.moverWrites, [], "sticky combo mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "sticky combo mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "sticky combo mover never switches");
        assert.deepEqual(flight.focuses, [], "sticky combo mover never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("a leafless transfer for a dual-overlay float mover never actuates", () => {
        const flight = startComboFlight("fullscreen");
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.maximized = true;
            }
        }
        assert.equal(flight.adapter.requestSend("ws-2", 2, true), true, "dual-overlay mover dispatches at the harness level");
        flight.callbacks[0]?.(":1.7");
        const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
        const correlation = body["correlation_id"] as string;
        flight.callbacks[flight.callbacks.length - 1]?.(comboReply(correlation, true, "ws-1", "ws-2"));
        assert.deepEqual(flight.moverWrites, [], "dual-overlay mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "dual-overlay mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "dual-overlay mover never switches");
        assert.deepEqual(flight.focuses, [], "dual-overlay mover never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("a leafless transfer for a tiled snapshot mover never actuates", () => {
        const flight = startComboFlight("maximized");
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.floating = false;
                entry.maximized = false;
            }
        }
        assert.equal(flight.adapter.requestSend("ws-2", 2, true), true, "tiled mover dispatches");
        flight.callbacks[0]?.(":1.7");
        const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
        const correlation = body["correlation_id"] as string;
        flight.callbacks[flight.callbacks.length - 1]?.(comboReply(correlation, true, "ws-1", "ws-2"));
        assert.deepEqual(flight.moverWrites, [], "non-combo mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "non-combo mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "non-combo mover never switches");
        assert.deepEqual(flight.focuses, [], "non-combo mover never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("a move-deferred operation carrying a leaf is malformed and never actuates", () => {
        const flight = startComboFlight("fullscreen");
        const correlation = dispatchCombo(flight, "fullscreen", "ws-2", 2, true);
        const malformed = JSON.parse(comboReply(correlation, true, "ws-1", "ws-2")) as Record<string, unknown>;
        (malformed["operation"] as Record<string, unknown>)["leaf"] = "leaf-win-f";
        flight.callbacks[flight.callbacks.length - 1]?.(JSON.stringify(malformed));
        assert.deepEqual(flight.moverWrites, [], "leaved deferred reply never actuates");
        assert.deepEqual(flight.geometries, [], "leaved deferred reply writes no geometry");
        assert.deepEqual(flight.switches, [], "leaved deferred reply never switches");
        assert.deepEqual(flight.focuses, [], "leaved deferred reply never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("a deferred follow naming an unresolvable focus never actuates", () => {
        const flight = startComboFlight("maximized");
        const correlation = dispatchCombo(flight, "maximized", "ws-2", 2, true);
        const malformed = JSON.parse(comboReply(correlation, true, "ws-1", "ws-2")) as Record<string, unknown>;
        malformed["desired_focus"] = { domain_output: "out-1", domain_workspace: "ws-2", leaf: "leaf-win-f" };
        flight.callbacks[flight.callbacks.length - 1]?.(JSON.stringify(malformed));
        assert.deepEqual(flight.moverWrites, [], "unresolvable deferred-follow focus never actuates");
        assert.deepEqual(flight.focuses, [], "unresolvable deferred-follow focus never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });
});

// ---- Real entry observation and mover gate with the floating-ids input. ----

interface EntrySignal {
    connect: (handler: () => void) => void;
    disconnect: (handler: () => void) => void;
}

function entrySignal(): EntrySignal {
    const handlers = new Set<() => void>();
    return {
        connect: (handler) => {
            handlers.add(handler);
        },
        disconnect: (handler) => {
            handlers.delete(handler);
        },
    };
}

interface EntryWindow {
    normalWindow: boolean;
    managed: boolean;
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    output: { name: string };
    desktops: Array<object>;
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number };
    desktopsChanged: EntrySignal;
}

interface EntryWrites {
    frameGeometry: number;
    desktops: number;
    fullScreen: number;
    maximizeMode: number;
}

function entryWindow(id: string, output: { name: string }, desktop: object, writes: EntryWrites, combo: Combo | null): EntryWindow {
    const backing = {
        frameGeometry: { x: 100, y: 100, width: 400, height: 300 },
        desktops: [desktop],
        fullScreen: combo === "fullscreen",
        maximizeMode: combo === "maximized" ? 3 : 0,
    };
    const win = {
        normalWindow: true,
        managed: true,
        minimized: false,
        onAllDesktops: false,
        output,
        internalId: id,
        desktopsChanged: entrySignal(),
    } as EntryWindow;
    Object.defineProperties(win, {
        frameGeometry: {
            enumerable: true,
            configurable: true,
            get: () => ({ ...backing.frameGeometry }),
            set: (rect: { x: number; y: number; width: number; height: number }) => {
                writes.frameGeometry += 1;
                backing.frameGeometry = { ...rect };
            },
        },
        desktops: {
            enumerable: true,
            configurable: true,
            get: () => [...backing.desktops],
            set: (refs: Array<object>) => {
                writes.desktops += 1;
                backing.desktops = [...refs];
            },
        },
        fullScreen: {
            enumerable: true,
            configurable: true,
            get: () => backing.fullScreen,
            set: (value: boolean) => {
                writes.fullScreen += 1;
                backing.fullScreen = value;
            },
        },
        maximizeMode: {
            enumerable: true,
            configurable: true,
            get: () => backing.maximizeMode,
            set: (value: number) => {
                writes.maximizeMode += 1;
                backing.maximizeMode = value;
            },
        },
    });
    return win;
}

interface EntryHarness {
    readonly winF: EntryWindow;
    readonly winS: EntryWindow;
    readonly winT: EntryWindow;
    readonly d1: object;
    readonly d2: object;
    readonly surface: Record<string, unknown>;
    readonly writes: EntryWrites;
    readonly switches: object[];
    readonly payloads: string[];
    readonly replies: Array<(reply: unknown) => void>;
    readonly logs: string[];
    readonly floatingIds: Set<string>;
    readonly requestSend: (target: unknown, follow?: unknown) => boolean;
    readonly stop: () => void;
    readonly isInFlight: () => boolean;
}

function entryHarness(combo: Combo): EntryHarness {
    const out = { name: "out-1" };
    const d1 = { id: "ws-1" };
    const d2 = { id: "ws-2" };
    const writes: EntryWrites = { frameGeometry: 0, desktops: 0, fullScreen: 0, maximizeMode: 0 };
    const winF = entryWindow("n-win-f", out, d1, writes, combo);
    const winS = entryWindow("n-win-s", out, d1, writes, null);
    const winT = entryWindow("n-win-t", out, d2, writes, null);
    const switches: object[] = [];
    let current: object = d1;
    const surface: Record<string, unknown> = {
        activeWindow: winF,
        currentDesktop: d1,
        screens: [out],
        desktops: [d1, d2],
        currentDesktopForScreen: (): object => current,
        setCurrentDesktopForScreen: (desktop: object): void => {
            switches.push(desktop);
            current = desktop;
        },
        clientArea: (): object => ({ x: 0, y: 0, width: 1200, height: 800 }),
        windowList: (): Array<object> => [winF, winS, winT],
    };
    const payloads: string[] = [];
    const replies: EntryHarness["replies"] = [];
    const floatingIds = new Set<string>();
    const logs: string[] = [];
    const entry = startWorkspaceSendAdapterEntry({
        workspace: surface,
        callDbus: (service, _path, _iface, method, payload, callback) => {
            if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                callback(true);
                return;
            }
            if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_GET_OWNER_METHOD) {
                callback(":1.7");
                return;
            }
            if (service === WORKSPACE_SEND_DBUS_SERVICE && method === WORKSPACE_SEND_START_METHOD) {
                callback(1);
                return;
            }
            if (method === WORKSPACE_SEND_METHOD) {
                payloads.push(payload);
                replies.push(callback);
                return;
            }
            callback(undefined);
        },
        scheduleOnce: () => (): void => {},
        log: (message) => {
            logs.push(message);
        },
        owner: "owner-1",
        generation: "gen-1",
        readInnerGapFn: () => 8,
        readOuterGapFn: () => 8,
        floatingIds,
    });
    assert.notEqual(entry, null, "send entry starts");
    if (entry === null) {
        throw new Error("send entry failed to start");
    }
    return {
        winF, winS, winT, d1, d2, surface, writes, switches,
        payloads, replies, logs, floatingIds,
        requestSend: (target, follow?) => entry.requestSend(target, follow),
        stop: () => entry.stop(),
        isInFlight: () => entry.isInFlight(),
    };
}

function entryDeferredReply(targetWorkspace: string, follow: boolean): (correlation: string) => string {
    return (correlation: string): string =>
        JSON.stringify({
            v: WORKSPACE_SEND_CONTRACT_VERSION,
            correlation_id: correlation,
            outcome: "planned",
            kind: "send-to-workspace",
            base_revision: 0,
            detail: { kind: "send-to-workspace", policy_version: 1, capability: "move-tiled" },
            desired_geometry: [
                { window: "n-win-s", leaf: "leaf-win-s", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 1200, h: 800 } },
                { window: "n-win-t", leaf: "leaf-win-t", output: "out-1", workspace: targetWorkspace, rect: { x: 0, y: 0, w: 1200, h: 800 } },
            ],
            desired_focus: follow
                ? { domain_output: "out-1", domain_workspace: targetWorkspace, leaf: "leaf-win-t" }
                : { domain_output: "out-1", domain_workspace: "ws-1", leaf: "leaf-win-s" },
            preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
            operation: {
                op: "move-deferred",
                window: "n-win-f",
                source_output: "out-1",
                source_workspace: "ws-1",
                target_output: "out-1",
                target_workspace: targetWorkspace,
                follow,
            },
        });
}

for (const combo of COMBOS) {
    describe(`D16 send entry combo observation and gate (${combo})`, () => {
        it("an intentional combo dispatches with its overlay wire flags and carries deferred", () => {
            const h = entryHarness(combo);
            try {
                h.floatingIds.add("n-win-f");
                assert.equal(h.requestSend("ws-2", true), true, "combo send dispatches");
                assert.equal(h.payloads.length, 1, "exactly one planner request");
                const body = JSON.parse(h.payloads[0] ?? "{}") as Record<string, unknown>;
                const windows = body["windows"] as Array<Record<string, unknown>>;
                const mover = windows.find((entry) => entry["window"] === "n-win-f") as Record<string, unknown>;
                assert.equal(mover["floating"], true, "combo mover rides still floating");
                assert.equal(mover["fit_excluded"], true, "combo mover rides fit-excluded");
                if (combo === "fullscreen") {
                    assert.equal(mover["fullscreen"], true, "fullscreen combo rides still fullscreen");
                } else {
                    assert.equal(mover["maximized"], true, "maximized combo rides still maximized");
                }
                assert.equal(body["focused_window"], "n-win-f");
                const correlation = body["correlation_id"] as string;
                h.replies[0]?.(entryDeferredReply("ws-2", true)(correlation));
                assert.equal(h.writes.desktops, 1, "exactly one native membership write");
                assert.deepEqual(h.winF.desktops, [h.d2], "mover natively on exact target, absent source");
                assert.equal(h.writes.frameGeometry, 0, "zero native geometry writes through the real entry");
                assert.equal(h.writes.fullScreen, 0, "zero native fullscreen writes");
                assert.equal(h.writes.maximizeMode, 0, "zero native maximize writes");
                assert.equal(h.winF.fullScreen, combo === "fullscreen", "fullscreen flag retained exactly");
                assert.equal(h.winF.maximizeMode, combo === "maximized" ? 3 : 0, "maximize flag retained exactly");
                assert.deepEqual(h.switches, [h.d2], "follow switches exactly once");
                assert.equal(h.surface["activeWindow"], h.winF, "follow focuses the mover");
                assert.equal(h.isInFlight(), false);
            } finally {
                h.stop();
            }
        });

        it("stay carries the combo with source MRU focus and no switch", () => {
            const h = entryHarness(combo);
            try {
                h.floatingIds.add("n-win-f");
                assert.equal(h.requestSend("ws-2", false), true, "combo stay dispatches");
                const body = JSON.parse(h.payloads[0] ?? "{}") as Record<string, unknown>;
                const correlation = body["correlation_id"] as string;
                h.replies[0]?.(entryDeferredReply("ws-2", false)(correlation));
                assert.equal(h.writes.desktops, 1, "exactly one native membership write");
                assert.deepEqual(h.winF.desktops, [h.d2], "mover natively on exact target");
                assert.equal(h.writes.frameGeometry, 0, "zero native geometry writes on stay");
                assert.equal(h.writes.fullScreen, 0, "zero native fullscreen writes on stay");
                assert.equal(h.writes.maximizeMode, 0, "zero native maximize writes on stay");
                assert.deepEqual(h.switches, [], "stay never switches desktops");
                assert.equal(h.surface["activeWindow"], h.winS, "stay focuses the source MRU survivor");
                assert.equal(h.isInFlight(), false);
            } finally {
                h.stop();
            }
        });

        it("a sticky combo still refuses without dispatching", () => {
            const h = entryHarness(combo);
            try {
                h.winF.onAllDesktops = true;
                h.floatingIds.add("n-win-f");
                assert.equal(h.requestSend("ws-2", true), false, "sticky combo send still refused");
                assert.equal(h.payloads.length, 0, "no planner request leaves");
                assert.equal(h.isInFlight(), false, "no flight starts");
                assert.deepEqual(h.winF.desktops, [h.d1], "nothing moves");
            } finally {
                h.stop();
            }
        });
    });
}

describe("D16 send entry combo refusal shapes", () => {
    it("a dual-overlay float refuses without dispatching", () => {
        const h = entryHarness("fullscreen");
        try {
            h.winF.maximizeMode = 3;
            h.floatingIds.add("n-win-f");
            assert.equal(h.requestSend("ws-2", true), false, "dual-overlay float refused");
            assert.equal(h.payloads.length, 0, "no planner request leaves");
            assert.equal(h.isInFlight(), false, "no flight starts");
            assert.deepEqual(h.winF.desktops, [h.d1], "nothing moves");
        } finally {
            h.stop();
        }
    });

    it("a fullscreen tiled mover still dispatches (overlay carry preserved)", () => {
        const h = entryHarness("fullscreen");
        try {
            assert.equal(h.requestSend("ws-2", true), true, "fullscreen tiled send still accepted");
            assert.equal(h.payloads.length, 1, "planner request leaves");
            const body = JSON.parse(h.payloads[0] ?? "{}") as Record<string, unknown>;
            const windows = body["windows"] as Array<Record<string, unknown>>;
            const mover = windows.find((entry) => entry["window"] === "n-win-f") as Record<string, unknown>;
            assert.equal(mover["floating"], undefined, "tiled mover rides never floating");
            assert.equal(mover["fullscreen"], true, "tiled mover rides still fullscreen");
        } finally {
            h.stop();
        }
    });
});

// ---- Real production observers dispatch combos to the deferred route. ----

interface RealWin {
    normalWindow: boolean;
    managed: boolean;
    minimized: boolean;
    fullScreen: boolean;
    maximizeMode: number;
    onAllDesktops: boolean;
    output: { name: string };
    desktops: Array<object>;
    internalId: string;
    frameGeometry: { x: number; y: number; width: number; height: number };
}

function realWin(id: string, output: { name: string }, desktop: object, combo: Combo | null): RealWin {
    return {
        normalWindow: true,
        managed: true,
        minimized: false,
        fullScreen: combo === "fullscreen",
        maximizeMode: combo === "maximized" ? 3 : 0,
        onAllDesktops: false,
        output,
        desktops: [desktop],
        internalId: id,
        frameGeometry: { x: 100, y: 100, width: 400, height: 300 },
    };
}

function realReply(
    correlation: string,
    kind: "send-to-workspace" | "send-to-output",
    follow: boolean,
    source: { output: string; workspace: string },
    target: { output: string; workspace: string },
): string {
    const sourceSurvivor = source.workspace === "ws-1" ? "n-win-s" : "n-win-t";
    const targetMember = target.workspace === "ws-2" ? "n-win-t" : "n-win-s";
    return JSON.stringify({
        v: WORKSPACE_SEND_CONTRACT_VERSION,
        correlation_id: correlation,
        outcome: "planned",
        kind,
        base_revision: 0,
        detail: { kind, policy_version: 1, capability: "move-tiled" },
        desired_geometry: [
            { window: sourceSurvivor, leaf: `leaf-${sourceSurvivor}`, output: source.output, workspace: source.workspace, rect: { x: 0, y: 0, w: 1200, h: 800 } },
            { window: targetMember, leaf: `leaf-${targetMember}`, output: target.output, workspace: target.workspace, rect: { x: 0, y: 0, w: 1200, h: 800 } },
        ],
        desired_focus: follow
            ? { domain_output: target.output, domain_workspace: target.workspace, leaf: `leaf-${targetMember}` }
            : { domain_output: source.output, domain_workspace: source.workspace, leaf: `leaf-${sourceSurvivor}` },
        preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
        operation: {
            op: "move-deferred",
            window: "n-win-f",
            source_output: source.output,
            source_workspace: source.workspace,
            target_output: target.output,
            target_workspace: target.workspace,
            follow,
        },
    });
}

describe("D16 real production observers dispatch combos deferred", () => {
    function startRealSameOutput(combo: Combo, follow: boolean): {
        adapter: WorkspaceSendAdapter;
        calls: Array<{ method: string; payload: string }>;
        callbacks: Array<(reply: unknown) => void>;
        writes: { memberships: object[]; geometries: object[]; switches: object[]; focuses: object[] };
        wins: { winF: RealWin; winS: RealWin; winT: RealWin };
    } {
        const out = { name: "out-1" };
        const d1 = { id: "ws-1" };
        const d2 = { id: "ws-2" };
        const winF = realWin("n-win-f", out, d1, combo);
        const winS = realWin("n-win-s", out, d1, null);
        const winT = realWin("n-win-t", out, d2, null);
        let current: object = d1;
        const surface: Record<string, unknown> = {
            activeWindow: winF,
            screens: [out],
            desktops: [d1, d2],
            currentDesktopForScreen: (): object => current,
            clientArea: (): object => ({ x: 0, y: 0, width: 1200, height: 800 }),
            windowList: (): Array<object> => [winF, winS, winT],
        };
        const cache = new Map<string, string>();
        const floatingIds = new Set<string>(["n-win-f"]);
        const writes = { memberships: [] as object[], geometries: [] as object[], switches: [] as object[], focuses: [] as object[] };
        const calls: Array<{ method: string; payload: string }> = [];
        const callbacks: Array<(reply: unknown) => void> = [];
        const env: WorkspaceSendAdapterEnv = {
            callDbus: (_service, _path, _iface, method, payload, callback) => {
                if (method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                    callback(true);
                    return;
                }
                calls.push({ method, payload });
                callbacks.push(callback);
            },
            scheduleOnce: (): (() => void) => (): void => {},
            log: () => {},
            observe: (targetWorkspace, pinnedSourceWorkspace?) =>
                observeSendTarget(
                    surface,
                    cache,
                    targetWorkspace as string,
                    floatingIds,
                    pinnedSourceWorkspace as string | undefined,
                ),
            onSettled: () => {},
            setGeometry: (target) => {
                writes.geometries.push(target);
                return true;
            },
            setDesktops: (target, refs) => {
                writes.memberships.push(target);
                (target as RealWin).desktops = [...(refs as Array<object>)];
                return true;
            },
            subscribeMoverDesktops: () => (): void => {},
            switchToTarget: (desktopRef) => {
                writes.switches.push(desktopRef);
                current = desktopRef;
                return true;
            },
            focusWindow: (windowRef) => {
                writes.focuses.push(windowRef);
                surface["activeWindow"] = windowRef;
                return true;
            },
        };
        const adapter = new WorkspaceSendAdapter(env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        assert.equal(adapter.requestSend("ws-2", 2, follow), true, "real-observer combo dispatches");
        assert.equal(calls[0]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
        callbacks[0]?.(":1.7");
        return { adapter, calls, callbacks, writes, wins: { winF, winS, winT } };
    }

    for (const combo of COMBOS) {
        it(`same-output follow carries a real-observed ${combo} combo deferred`, () => {
            const flight = startRealSameOutput(combo, true);
            const body = JSON.parse(flight.calls[1]?.payload ?? "{}") as Record<string, unknown>;
            const windows = body["windows"] as Array<Record<string, unknown>>;
            const mover = windows.find((entry) => entry["window"] === "n-win-f") as Record<string, unknown>;
            assert.equal(mover["floating"], true);
            assert.equal(combo === "fullscreen" ? mover["fullscreen"] : mover["maximized"], true);
            assert.equal(body["focused_window"], "n-win-f");
            const correlation = body["correlation_id"] as string;
            flight.callbacks[flight.callbacks.length - 1]?.(
                realReply(correlation, "send-to-workspace", true, { output: "out-1", workspace: "ws-1" }, { output: "out-1", workspace: "ws-2" }),
            );
            assert.equal(flight.writes.memberships.length, 1, "mover membership written once");
            assert.deepEqual(flight.writes.geometries, [], "no geometry writes");
            assert.equal(flight.writes.switches.length, 1, "follow switches exactly once");
            assert.equal(flight.writes.focuses.length, 1, "follow focuses the mover");
            assert.equal(flight.wins.winF.fullScreen, combo === "fullscreen", "fullscreen retained");
            assert.equal(flight.wins.winF.maximizeMode, combo === "maximized" ? 3 : 0, "maximize retained");
            assert.equal(flight.adapter.isInFlight, false);
        });

        it(`same-output stay carries a real-observed ${combo} combo with source MRU focus`, () => {
            const flight = startRealSameOutput(combo, false);
            const body = JSON.parse(flight.calls[1]?.payload ?? "{}") as Record<string, unknown>;
            const correlation = body["correlation_id"] as string;
            flight.callbacks[flight.callbacks.length - 1]?.(
                realReply(correlation, "send-to-workspace", false, { output: "out-1", workspace: "ws-1" }, { output: "out-1", workspace: "ws-2" }),
            );
            assert.equal(flight.writes.memberships.length, 1, "mover membership written once");
            assert.deepEqual(flight.writes.geometries, [], "no geometry writes on stay");
            assert.deepEqual(flight.writes.switches, [], "stay never switches");
            assert.equal(flight.writes.focuses.length, 1, "stay focuses the source survivor");
            assert.equal(flight.adapter.isInFlight, false);
        });
    }

    function startRealOutput(combo: Combo, follow: boolean): {
        adapter: WorkspaceSendAdapter;
        calls: Array<{ method: string; payload: string }>;
        callbacks: Array<(reply: unknown) => void>;
        writes: { transfers: object[]; memberships: object[]; geometries: object[]; focuses: object[] };
        wins: { winF: RealWin; winS: RealWin; winT: RealWin };
    } {
        const out1 = { name: "out-1" };
        const out2 = { name: "out-2" };
        const wsA = { id: "ws-1" };
        const wsB = { id: "ws-2" };
        const winF = realWin("n-win-f", out1, wsA, combo);
        const winS = realWin("n-win-s", out1, wsA, null);
        const winT = realWin("n-win-t", out2, wsB, null);
        const surface: Record<string, unknown> = {
            activeWindow: winF,
            screens: [out1, out2],
            desktops: [wsA, wsB],
            currentDesktopForScreen: (screen: object): object => (screen === out1 ? wsA : wsB),
            clientArea: (): object => ({ x: 0, y: 0, width: 1200, height: 800 }),
            windowList: (): Array<object> => [winF, winS, winT],
        };
        const cache = new Map<string, string>();
        const floatingIds = new Set<string>(["n-win-f"]);
        const writes = { transfers: [] as object[], memberships: [] as object[], geometries: [] as object[], focuses: [] as object[] };
        const calls: Array<{ method: string; payload: string }> = [];
        const callbacks: Array<(reply: unknown) => void> = [];
        const outputOf = (ref: object): string | null => {
            for (const entry of [winF, winS, winT]) {
                if (entry === ref) {
                    return (entry.output as { name: string }).name;
                }
            }
            return null;
        };
        const workspaceOf = (ref: object): string | null => {
            for (const entry of [winF, winS, winT]) {
                if (entry === ref) {
                    return ((entry.desktops[0] ?? {}) as { id?: string }).id ?? null;
                }
            }
            return null;
        };
        const env: WorkspaceSendAdapterEnv = {
            callDbus: (_service, _path, _iface, method, payload, callback) => {
                if (method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                    callback(true);
                    return;
                }
                calls.push({ method, payload });
                callbacks.push(callback);
            },
            scheduleOnce: (): (() => void) => (): void => {},
            log: () => {},
            observe: () => null,
            observeOutput: (targetOutput, _targetWorkspace, pinnedSourceWorkspace?, pinnedSourceOutput?) =>
                observeOutputSendTarget(
                    surface,
                    cache,
                    targetOutput as string,
                    floatingIds,
                    pinnedSourceWorkspace as string | undefined,
                    pinnedSourceOutput as string | undefined,
                ),
            onSettled: () => {},
            setGeometry: (target) => {
                writes.geometries.push(target);
                return true;
            },
            setDesktops: (target, refs) => {
                writes.memberships.push(target);
                (target as RealWin).desktops = [...(refs as Array<object>)];
                for (const entry of [winF, winS, winT]) {
                    if ((entry as unknown as object) === target) {
                        entry.desktops = [...(refs as Array<object>)];
                    }
                }
                return true;
            },
            resolveOutput: (name) => (name === "out-2" ? out2 : null),
            sendClientToScreen: (mover, output) => {
                if (output !== out2) {
                    return false;
                }
                writes.transfers.push(mover);
                (mover as RealWin).output = out2;
                return true;
            },
            readOutputName: (ref) => outputOf(ref),
            readDesktopIds: (ref) => {
                const workspace = workspaceOf(ref);
                return workspace === null ? null : [workspace];
            },
            readMoverLive: (ref) => {
                for (const entry of [winF, winS, winT]) {
                    if ((entry as unknown as object) === ref) {
                        return {
                            id: entry.internalId,
                            floating: floatingIds.has(entry.internalId),
                            sticky: false,
                            fullscreen: entry.fullScreen,
                            maximized: entry.maximizeMode !== 0,
                        };
                    }
                }
                return null;
            },
            subscribeMoverOutput: () => (): void => {},
            switchToTarget: () => true,
            focusWindow: (windowRef) => {
                writes.focuses.push(windowRef);
                surface["activeWindow"] = windowRef;
                return true;
            },
        };
        const adapter = new WorkspaceSendAdapter(env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-2", follow), true, "real cross-output combo dispatches");
        assert.equal(calls[0]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
        callbacks[0]?.(":1.7");
        return { adapter, calls, callbacks, writes, wins: { winF, winS, winT } };
    }

    for (const combo of COMBOS) {
        it(`cross-output follow carries a real-observed ${combo} combo deferred`, () => {
            const flight = startRealOutput(combo, true);
            const body = JSON.parse(flight.calls[1]?.payload ?? "{}") as Record<string, unknown>;
            const windows = body["windows"] as Array<Record<string, unknown>>;
            const mover = windows.find((entry) => entry["window"] === "n-win-f") as Record<string, unknown>;
            assert.equal(mover["floating"], true);
            assert.equal(combo === "fullscreen" ? mover["fullscreen"] : mover["maximized"], true);
            const correlation = body["correlation_id"] as string;
            flight.callbacks[flight.callbacks.length - 1]?.(
                realReply(correlation, "send-to-output", true, { output: "out-1", workspace: "ws-1" }, { output: "out-2", workspace: "ws-2" }),
            );
            assert.equal(flight.writes.transfers.length, 1, "output transfer runs once");
            assert.equal(flight.writes.memberships.length, 1, "mover membership written once");
            assert.deepEqual(flight.writes.geometries, [], "no geometry writes");
            assert.equal(flight.writes.focuses.length, 1, "follow focuses the mover");
            assert.equal(flight.adapter.isInFlight, false);
        });
    }

    it("cross-output stay carries a real-observed fullscreen combo with source MRU focus", () => {
        const flight = startRealOutput("fullscreen", false);
        const body = JSON.parse(flight.calls[1]?.payload ?? "{}") as Record<string, unknown>;
        const correlation = body["correlation_id"] as string;
        flight.callbacks[flight.callbacks.length - 1]?.(
            realReply(correlation, "send-to-output", false, { output: "out-1", workspace: "ws-1" }, { output: "out-2", workspace: "ws-2" }),
        );
        assert.equal(flight.writes.transfers.length, 1, "output transfer runs once on stay");
        assert.equal(flight.writes.memberships.length, 1, "mover membership written once on stay");
        assert.deepEqual(flight.writes.geometries, [], "no geometry writes on stay");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("a cross-output combo overlay race refuses without setters", () => {
        const flight = startRealOutput("maximized", true);
        const body = JSON.parse(flight.calls[1]?.payload ?? "{}") as Record<string, unknown>;
        const correlation = body["correlation_id"] as string;
        // Native maximize cleared mid-flight: the flag fence stales before
        // any setter.
        flight.wins.winF.maximizeMode = 0;
        flight.callbacks[flight.callbacks.length - 1]?.(
            realReply(correlation, "send-to-output", true, { output: "out-1", workspace: "ws-1" }, { output: "out-2", workspace: "ws-2" }),
        );
        assert.deepEqual(flight.writes.transfers, [], "overlay-changed mover never transfers outputs");
        assert.deepEqual(flight.writes.memberships, [], "overlay-changed mover takes no membership write");
        assert.deepEqual(flight.writes.geometries, [], "overlay-changed mover takes no geometry writes");
        assert.deepEqual(flight.writes.focuses, [], "overlay-changed mover never focuses");
    });
});
