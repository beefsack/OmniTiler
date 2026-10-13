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

// D16 ordinary intentional-float workspace transfer (option (a), no
// setting): explicit numbered/relative follow/stay send of an intentional
// floating window through the shared slotless `move-deferred` route. The
// floating mover (slotless exception with retained size) transfers to the
// target homing with NO tree mutation and NO source/target reflow: the
// reply geometry covers tiled survivors only, the mover takes no
// allocation and no geometry write, and its live frame/size is kept
// as-is. Follow switches and focuses the mover native ref; stay focuses
// the source MRU survivor with no switch. Sticky and maximized movers
// keep refusing. Offline only: adapter-level harness with canned Engine
// replies plus real entry observation shapes.

function rect(x: number, y: number, w: number, h: number): { x: number; y: number; w: number; h: number } {
    return { x, y, w, h };
}

const FLOAT_RECT = rect(100, 100, 400, 300);

interface FloatWorld {
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

function floatWorld(refs: { f: object; s: object; t: object }): FloatWorld {
    return {
        windows: [
            // Slotless float mover: ordinary intentional float, still
            // floating with no tile slot and no overlay.
            { id: "win-f", ref: refs.f, rect: { ...FLOAT_RECT }, workspace: "ws-1", floating: true, sticky: false, maximized: false, fullscreen: false },
            { id: "win-s", ref: refs.s, rect: rect(0, 0, 1200, 800), workspace: "ws-1", floating: false, sticky: false, maximized: false, fullscreen: false },
            { id: "win-t", ref: refs.t, rect: rect(0, 0, 1200, 800), workspace: "ws-2", floating: false, sticky: false, maximized: false, fullscreen: false },
        ],
    };
}

// Source follows the mover only at dispatch; re-observations pin the
// dispatch source like the real entries (pinnedSourceWorkspace), so a
// transferred mover reads on the target while the scope stays frozen.
function floatObserved(
    world: FloatWorld,
    refs: { f: object; s: object; t: object; desktop1: object; desktop2: object },
    targetWorkspace: string,
    activeRef: object | null,
    pinnedSourceWorkspace?: string,
): WorkspaceSendObserved {
    const flagged = (entry: FloatWorld["windows"][number]): Record<string, unknown> =>
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

function floatReply(correlation: string, follow: boolean, sourceWorkspace: string, targetWorkspace: string): string {
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
        // reflow); the slotless float mover takes no allocation. Follow
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

interface FloatFlight {
    readonly refs: { f: object; s: object; t: object; desktop1: object; desktop2: object };
    readonly world: FloatWorld;
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

function startFloatFlight(): FloatFlight {
    const refs = { f: {}, s: {}, t: {}, desktop1: {}, desktop2: {} };
    const world = floatWorld(refs);
    const geometries: object[] = [];
    const moverWrites: object[] = [];
    const switches: object[] = [];
    const focuses: object[] = [];
    const logs: string[] = [];
    const dbusCalls: FloatFlight["dbusCalls"] = [];
    const callbacks: FloatFlight["callbacks"] = [];
    const flight: FloatFlight = {
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
            floatObserved(world, refs, targetWorkspace as string, flight.activeRef, pinnedSourceWorkspace as string | undefined),
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
function dispatchFloat(flight: FloatFlight, target: string, ordinal: unknown, follow: unknown): string {
    const base = flight.dbusCalls.length;
    assert.equal(flight.adapter.requestSend(target, ordinal, follow), true, "float carry must dispatch");
    assert.equal(flight.dbusCalls[base]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
    flight.callbacks[base]?.(":1.7");
    const body = JSON.parse(flight.dbusCalls[base + 1]?.payload ?? "{}") as Record<string, unknown>;
    // The wire carries the float mover still floating with the fit
    // opt-out and no overlay flags.
    const windows = body["windows"] as Array<Record<string, unknown>>;
    const mover = windows.find((entry) => entry["window"] === "win-f") as Record<string, unknown>;
    assert.equal(mover["floating"], true, "float mover rides the send wire still floating");
    assert.equal(mover["fit_excluded"], true, "float mover rides fit-excluded");
    assert.equal(mover["fullscreen"], undefined, "float mover rides never fullscreen");
    assert.equal(body["focused_window"], "win-f", "explicit send focuses the float mover");
    return body["correlation_id"] as string;
}

describe("D16 slotless float carry (adapter follow/stay)", () => {
    it("numbered follow carries the float with one membership write and zero geometry", () => {
        const flight = startFloatFlight();
        const correlation = dispatchFloat(flight, "ws-2", 2, true);
        flight.callbacks[flight.callbacks.length - 1]?.(floatReply(correlation, true, "ws-1", "ws-2"));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.moverWrites[0], flight.refs.f);
        assert.deepEqual(flight.geometries, [], "no geometry write: allocations never reflow");
        assert.deepEqual(flight.world.windows.find((entry) => entry.id === "win-f")?.rect, FLOAT_RECT, "arrival frame kept as-is");
        assert.deepEqual(flight.switches, [flight.refs.desktop2], "follow switches exactly once");
        assert.deepEqual(flight.focuses, [flight.refs.f], "follow focuses the slotless float mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("numbered stay carries the float with source MRU focus and no switch", () => {
        const flight = startFloatFlight();
        const correlation = dispatchFloat(flight, "ws-2", 2, false);
        flight.callbacks[flight.callbacks.length - 1]?.(floatReply(correlation, false, "ws-1", "ws-2"));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.geometries, [], "no geometry write on stay either");
        assert.deepEqual(flight.world.windows.find((entry) => entry.id === "win-f")?.rect, FLOAT_RECT, "stay keeps the frame too");
        assert.deepEqual(flight.switches, [], "stay never switches desktops");
        assert.deepEqual(flight.focuses, [flight.refs.s], "stay focuses the source MRU survivor");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("diagnostic ordinal -1 follow carries the float like the numbered route", () => {
        const flight = startFloatFlight();
        const correlation = dispatchFloat(flight, "ws-2", -1, true);
        flight.callbacks[flight.callbacks.length - 1]?.(floatReply(correlation, true, "ws-1", "ws-2"));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.geometries, [], "no geometry write on the relative route");
        assert.deepEqual(flight.switches, [flight.refs.desktop2], "relative follow switches exactly once");
        assert.deepEqual(flight.focuses, [flight.refs.f], "relative follow focuses the mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("diagnostic ordinal -1 stay carries the float with source MRU focus", () => {
        const flight = startFloatFlight();
        const correlation = dispatchFloat(flight, "ws-2", -1, false);
        flight.callbacks[flight.callbacks.length - 1]?.(floatReply(correlation, false, "ws-1", "ws-2"));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.switches, [], "relative stay never switches desktops");
        assert.deepEqual(flight.focuses, [flight.refs.s], "relative stay focuses the source MRU survivor");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("WS1 -> WS2 -> WS1 roundtrip retains float both legs", () => {
        const flight = startFloatFlight();
        // Leg one: ws-1 -> ws-2 (follow).
        const first = dispatchFloat(flight, "ws-2", 2, true);
        flight.callbacks[flight.callbacks.length - 1]?.(floatReply(first, true, "ws-1", "ws-2"));
        assert.equal(flight.moverWrites.length, 1, "leg one transfers membership once");
        assert.deepEqual(flight.world.windows.find((entry) => entry.id === "win-f")?.workspace, "ws-2", "leg one arrives on ws-2");
        assert.equal(flight.world.windows.find((entry) => entry.id === "win-f")?.floating, true, "float retained on leg one");
        assert.equal(flight.adapter.isInFlight, false, "leg one flight released");
        // Leg two: ws-2 -> ws-1 (stay). The follow focused the mover, so it
        // stays the explicit mover on the way home.
        const writesBefore = flight.moverWrites.length;
        const second = dispatchFloat(flight, "ws-1", 2, false);
        flight.callbacks[flight.callbacks.length - 1]?.(floatReply(second, false, "ws-2", "ws-1"));
        assert.equal(flight.moverWrites.length, writesBefore + 1, "leg two transfers membership once");
        assert.deepEqual(flight.world.windows.find((entry) => entry.id === "win-f")?.workspace, "ws-1", "return leg arrives home");
        assert.equal(flight.world.windows.find((entry) => entry.id === "win-f")?.floating, true, "float retained on the return leg");
        assert.deepEqual(flight.world.windows.find((entry) => entry.id === "win-f")?.rect, FLOAT_RECT, "size kept across the roundtrip");
        assert.deepEqual(flight.geometries, [], "no geometry writes across the roundtrip");
        assert.deepEqual(flight.focuses[flight.focuses.length - 1], flight.refs.t, "return stay focuses the ws-2 MRU survivor");
        assert.equal(flight.adapter.isInFlight, false, "leg two flight released");
    });

    it("a mover float-untoggle race refuses without setters or follow", () => {
        const flight = startFloatFlight();
        const correlation = dispatchFloat(flight, "ws-2", 2, true);
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.floating = false;
            }
        }
        flight.callbacks[flight.callbacks.length - 1]?.(floatReply(correlation, true, "ws-1", "ws-2"));
        assert.deepEqual(flight.moverWrites, [], "flag-changed mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "flag-changed mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "flag-changed mover never switches");
        assert.deepEqual(flight.focuses, [], "flag-changed mover never focuses");
    });

    it("a leafless transfer for a tiled snapshot mover never actuates", () => {
        // P3 analog for floats: an ordinary tiled mover cannot sneak a
        // leafless operation and drop its own allocation.
        const flight = startFloatFlight();
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.floating = false;
            }
        }
        assert.equal(flight.adapter.requestSend("ws-2", 2, true), true, "tiled mover dispatches");
        flight.callbacks[0]?.(":1.7");
        const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
        const correlation = body["correlation_id"] as string;
        flight.callbacks[flight.callbacks.length - 1]?.(floatReply(correlation, true, "ws-1", "ws-2"));
        assert.deepEqual(flight.moverWrites, [], "non-float mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "non-float mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "non-float mover never switches");
        assert.deepEqual(flight.focuses, [], "non-float mover never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("a leafless transfer for a sticky snapshot mover never actuates", () => {
        const flight = startFloatFlight();
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.sticky = true;
            }
        }
        assert.equal(flight.adapter.requestSend("ws-2", 2, true), true, "sticky mover dispatches at the harness level");
        flight.callbacks[0]?.(":1.7");
        const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
        const correlation = body["correlation_id"] as string;
        flight.callbacks[flight.callbacks.length - 1]?.(floatReply(correlation, true, "ws-1", "ws-2"));
        assert.deepEqual(flight.moverWrites, [], "sticky mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "sticky mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "sticky mover never switches");
        assert.deepEqual(flight.focuses, [], "sticky mover never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("a leafless transfer for a maximized snapshot mover never actuates", () => {
        const flight = startFloatFlight();
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.floating = false;
                entry.maximized = true;
            }
        }
        assert.equal(flight.adapter.requestSend("ws-2", 2, true), true, "maximized mover dispatches at the harness level");
        flight.callbacks[0]?.(":1.7");
        const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
        const correlation = body["correlation_id"] as string;
        flight.callbacks[flight.callbacks.length - 1]?.(floatReply(correlation, true, "ws-1", "ws-2"));
        assert.deepEqual(flight.moverWrites, [], "maximized mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "maximized mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "maximized mover never switches");
        assert.deepEqual(flight.focuses, [], "maximized mover never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("a move-deferred operation carrying a leaf is malformed and never actuates", () => {
        const flight = startFloatFlight();
        const correlation = dispatchFloat(flight, "ws-2", 2, true);
        const malformed = JSON.parse(floatReply(correlation, true, "ws-1", "ws-2")) as Record<string, unknown>;
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
        const flight = startFloatFlight();
        const correlation = dispatchFloat(flight, "ws-2", 2, true);
        const malformed = JSON.parse(floatReply(correlation, true, "ws-1", "ws-2")) as Record<string, unknown>;
        malformed["desired_focus"] = { domain_output: "out-1", domain_workspace: "ws-2", leaf: "leaf-win-f" };
        flight.callbacks[flight.callbacks.length - 1]?.(JSON.stringify(malformed));
        assert.deepEqual(flight.moverWrites, [], "unresolvable deferred-follow focus never actuates");
        assert.deepEqual(flight.focuses, [], "unresolvable deferred-follow focus never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("empty geometry carries a sole float mover with null focus", () => {
        const flight = startFloatFlight();
        flight.world.windows.length = 0;
        flight.world.windows.push(
            { id: "win-f", ref: flight.refs.f, rect: { ...FLOAT_RECT }, workspace: "ws-1", floating: true, sticky: false, maximized: false, fullscreen: false },
        );
        const correlation = dispatchFloat(flight, "ws-2", 2, true);
        const reply = JSON.parse(floatReply(correlation, true, "ws-1", "ws-2")) as Record<string, unknown>;
        reply["desired_geometry"] = [];
        reply["desired_focus"] = null;
        flight.callbacks[flight.callbacks.length - 1]?.(JSON.stringify(reply));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.moverWrites[0], flight.refs.f);
        assert.deepEqual(flight.geometries, [], "no geometry to write");
        assert.deepEqual(flight.switches, [flight.refs.desktop2], "follow switches exactly once");
        assert.deepEqual(flight.focuses, [flight.refs.f], "follow focuses the slotless float mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("drifted survivor allocations still take zero geometry writes", () => {
        const flight = startFloatFlight();
        const correlation = dispatchFloat(flight, "ws-2", 2, true);
        const reply = JSON.parse(floatReply(correlation, true, "ws-1", "ws-2")) as Record<string, unknown>;
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

// ---- Deferred cross-output float carry (send-to-output, same op/fences). ----

interface XWorld {
    readonly windows: Array<{
        id: string;
        ref: object;
        rect: { x: number; y: number; w: number; h: number };
        output: string;
        workspace: string;
        floating: boolean;
    }>;
}

function xObserved(
    world: XWorld,
    refs: { desktop2: object },
    activeRef: object | null,
): WorkspaceSendObserved {
    const flagged = (entry: XWorld["windows"][number]): Record<string, unknown> =>
        Object.freeze({
            id: entry.id,
            ref: entry.ref,
            rect: Object.freeze({ ...entry.rect }),
            fullscreen: false,
            maximized: false,
            floating: entry.floating,
            sticky: false,
            fitExcluded: entry.floating,
            fit_excluded: entry.floating,
        });
    const source = world.windows.filter((entry) => entry.output === "out-1");
    const activeId = world.windows.find((entry) => entry.ref === activeRef)?.id ?? "";
    const focused = source.some((entry) => entry.id === activeId) ? activeId : "";
    const moverEntry = source.find((entry) => entry.id === activeId);
    return {
        sourceOutput: "out-1",
        sourceWorkspace: "ws-1",
        sourceBounds: Object.freeze(rect(0, 0, 1200, 800)),
        targetOutput: "out-2",
        targetWorkspace: "ws-2",
        targetBounds: Object.freeze(rect(0, 0, 1200, 800)),
        focusedId: focused,
        sourceWindows: Object.freeze(source.map(flagged)),
        targetWindows: Object.freeze(world.windows.filter((entry) => entry.output === "out-2").map(flagged)),
        activeRef,
        moverRef: moverEntry === undefined ? null : moverEntry.ref,
        targetDesktopRef: refs.desktop2,
        targetExists: true,
        desktopCount: 2,
        sourceFingerprint: "xfp-1",
        targetFingerprint: "xfp-2",
        currentWorkspace: "ws-1",
    } as unknown as WorkspaceSendObserved;
}

function xFloatReply(correlation: string, follow: boolean): string {
    return JSON.stringify({
        v: WORKSPACE_SEND_CONTRACT_VERSION,
        correlation_id: correlation,
        outcome: "planned",
        kind: "send-to-output",
        base_revision: 0,
        detail: { kind: "send-to-output", policy_version: 1, capability: "move-tiled" },
        desired_geometry: [
            { window: "win-s", leaf: "leaf-win-s", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 1200, h: 800 } },
            { window: "win-t", leaf: "leaf-win-t", output: "out-2", workspace: "ws-2", rect: { x: 0, y: 0, w: 1200, h: 800 } },
        ],
        desired_focus: follow
            ? { domain_output: "out-2", domain_workspace: "ws-2", leaf: "leaf-win-t" }
            : { domain_output: "out-1", domain_workspace: "ws-1", leaf: "leaf-win-s" },
        preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
        operation: {
            op: "move-deferred",
            window: "win-f",
            source_output: "out-1",
            source_workspace: "ws-1",
            target_output: "out-2",
            target_workspace: "ws-2",
            follow,
        },
    });
}

interface XFlight {
    readonly refs: { f: object; s: object; t: object; out2: object; desktop2: object };
    readonly world: XWorld;
    readonly transfers: object[];
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

function startXFloatFlight(): XFlight {
    const refs = { f: {}, s: {}, t: {}, out2: {}, desktop2: {} };
    const world: XWorld = {
        windows: [
            { id: "win-f", ref: refs.f, rect: { ...FLOAT_RECT }, output: "out-1", workspace: "ws-1", floating: true },
            { id: "win-s", ref: refs.s, rect: rect(0, 0, 1200, 800), output: "out-1", workspace: "ws-1", floating: false },
            { id: "win-t", ref: refs.t, rect: rect(0, 0, 1200, 800), output: "out-2", workspace: "ws-2", floating: false },
        ],
    };
    const transfers: object[] = [];
    const geometries: object[] = [];
    const moverWrites: object[] = [];
    const switches: object[] = [];
    const focuses: object[] = [];
    const logs: string[] = [];
    const dbusCalls: XFlight["dbusCalls"] = [];
    const callbacks: XFlight["callbacks"] = [];
    const flight: XFlight = {
        refs, world, transfers, geometries, moverWrites, switches, focuses, logs,
        dbusCalls, callbacks,
        adapter: null as unknown as WorkspaceSendAdapter,
        activeRef: refs.f,
    };
    const outputOf = (ref: object): string | null => {
        for (const entry of world.windows) {
            if (entry.ref === ref) {
                return entry.output;
            }
        }
        return null;
    };
    const workspaceOf = (ref: object): string | null => {
        for (const entry of world.windows) {
            if (entry.ref === ref) {
                return entry.workspace;
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
            dbusCalls.push({ method, payload });
            callbacks.push(callback);
        },
        scheduleOnce: (): (() => void) => (): void => {},
        log: (message) => {
            logs.push(message);
        },
        observe: () => null,
        observeOutput: () => xObserved(world, refs, flight.activeRef),
        onSettled: () => {},
        setGeometry: (target) => {
            geometries.push(target);
            return true;
        },
        setDesktops: (target) => {
            moverWrites.push(target);
            for (const entry of world.windows) {
                if (entry.ref === target) {
                    entry.workspace = "ws-2";
                }
            }
            return true;
        },
        resolveOutput: (name) => (name === "out-2" ? refs.out2 : null),
        sendClientToScreen: (mover, output) => {
            if (output !== refs.out2) {
                return false;
            }
            transfers.push(mover);
            for (const entry of world.windows) {
                if (entry.ref === mover) {
                    entry.output = "out-2";
                }
            }
            return true;
        },
        readOutputName: (ref) => outputOf(ref),
        readDesktopIds: (ref) => {
            const workspace = workspaceOf(ref);
            return workspace === null ? null : [workspace];
        },
        readMoverLive: (ref) => {
            for (const entry of world.windows) {
                if (entry.ref === ref) {
                    return { id: entry.id, floating: entry.floating, sticky: false, fullscreen: false, maximized: false };
                }
            }
            return null;
        },
        subscribeMoverOutput: () => (): void => {},
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

function dispatchXFloat(flight: XFlight, follow: unknown): string {
    assert.equal(flight.adapter.requestSendToOutput("out-2", "ws-2", follow), true, "cross-output float carry must dispatch");
    assert.equal(flight.dbusCalls[0]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
    flight.callbacks[0]?.(":1.7");
    const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
    const command = body["command"] as Record<string, unknown>;
    assert.equal(command["op"], "send-to-output");
    const windows = body["windows"] as Array<Record<string, unknown>>;
    const mover = windows.find((entry) => entry["window"] === "win-f") as Record<string, unknown>;
    assert.equal(mover["floating"], true, "float mover rides cross-output still floating");
    assert.equal(mover["fullscreen"], undefined, "float mover rides never fullscreen");
    assert.equal(body["focused_window"], "win-f");
    return body["correlation_id"] as string;
}

describe("D16 slotless float carry (cross-output follow/stay)", () => {
    it("cross-output follow transfers, writes no geometry, and focuses the mover", () => {
        const flight = startXFloatFlight();
        const correlation = dispatchXFloat(flight, true);
        flight.callbacks[flight.callbacks.length - 1]?.(xFloatReply(correlation, true));
        assert.equal(flight.transfers.length, 1, "output transfer runs once");
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.geometries, [], "no geometry writes across outputs either");
        assert.deepEqual(flight.switches, [], "cross-output follow never switches desktops");
        assert.deepEqual(flight.focuses, [flight.refs.f], "cross-output follow focuses the mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("cross-output stay transfers with source MRU focus and no switch", () => {
        const flight = startXFloatFlight();
        const correlation = dispatchXFloat(flight, false);
        flight.callbacks[flight.callbacks.length - 1]?.(xFloatReply(correlation, false));
        assert.equal(flight.transfers.length, 1, "output transfer runs once on stay");
        assert.equal(flight.moverWrites.length, 1, "mover membership written once on stay");
        assert.deepEqual(flight.geometries, [], "no geometry writes on stay either");
        assert.deepEqual(flight.switches, [], "stay never switches desktops");
        assert.deepEqual(flight.focuses, [flight.refs.s], "stay focuses the source MRU survivor");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("a cross-output mover float-untoggle race refuses without setters", () => {
        const flight = startXFloatFlight();
        const correlation = dispatchXFloat(flight, true);
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.floating = false;
            }
        }
        flight.callbacks[flight.callbacks.length - 1]?.(xFloatReply(correlation, true));
        assert.deepEqual(flight.transfers, [], "flag-changed mover never transfers outputs");
        assert.deepEqual(flight.moverWrites, [], "flag-changed mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "flag-changed mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "flag-changed mover never switches");
        assert.deepEqual(flight.focuses, [], "flag-changed mover never focuses");
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

function entryWindow(id: string, output: { name: string }, desktop: object, writes: EntryWrites): EntryWindow {
    const backing = {
        frameGeometry: { x: 100, y: 100, width: 400, height: 300 },
        desktops: [desktop],
        fullScreen: false,
        maximizeMode: 0,
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

function entryHarness(): EntryHarness {
    const out = { name: "out-1" };
    const d1 = { id: "ws-1" };
    const d2 = { id: "ws-2" };
    const writes: EntryWrites = { frameGeometry: 0, desktops: 0, fullScreen: 0, maximizeMode: 0 };
    const winF = entryWindow("n-win-f", out, d1, writes);
    const winS = entryWindow("n-win-s", out, d1, writes);
    const winT = entryWindow("n-win-t", out, d2, writes);
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

describe("D16 send entry floating-ids observation and gate", () => {
    it("an intentional float dispatches with the floating wire flag and carries deferred", () => {
        const h = entryHarness();
        try {
            h.floatingIds.add("n-win-f");
            assert.equal(h.requestSend("ws-2", true), true, "float send dispatches");
            assert.equal(h.payloads.length, 1, "exactly one planner request");
            const body = JSON.parse(h.payloads[0] ?? "{}") as Record<string, unknown>;
            const windows = body["windows"] as Array<Record<string, unknown>>;
            const mover = windows.find((entry) => entry["window"] === "n-win-f") as Record<string, unknown>;
            assert.equal(mover["floating"], true, "float mover rides still floating");
            assert.equal(mover["fit_excluded"], true, "float mover rides fit-excluded");
            assert.equal(mover["fullscreen"], undefined, "float mover rides never fullscreen");
            assert.equal(body["focused_window"], "n-win-f");
            const correlation = body["correlation_id"] as string;
            h.replies[0]?.(entryDeferredReply("ws-2", true)(correlation));
            assert.equal(h.writes.desktops, 1, "exactly one native membership write");
            assert.deepEqual(h.winF.desktops, [h.d2], "mover natively on exact target, absent source");
            assert.equal(h.writes.frameGeometry, 0, "zero native geometry writes through the real entry");
            assert.equal(h.writes.fullScreen, 0, "zero native fullscreen writes");
            assert.equal(h.writes.maximizeMode, 0, "zero native maximize writes");
            assert.deepEqual(h.switches, [h.d2], "follow switches exactly once");
            assert.equal(h.surface["activeWindow"], h.winF, "follow focuses the mover");
            assert.equal(h.isInFlight(), false);
        } finally {
            h.stop();
        }
    });

    it("a sticky mover still refuses without dispatching", () => {
        const h = entryHarness();
        try {
            h.winF.onAllDesktops = true;
            h.floatingIds.add("n-win-f");
            assert.equal(h.requestSend("ws-2", true), false, "sticky send still refused");
            assert.equal(h.payloads.length, 0, "no planner request leaves");
            assert.equal(h.isInFlight(), false, "no flight starts");
            assert.deepEqual(h.winF.desktops, [h.d1], "nothing moves");
        } finally {
            h.stop();
        }
    });

    it("an intentional maximized float now dispatches as the approved combo", () => {
        // D16 approved floating+maximized combo (2026-10-13): an
        // intentional float observed maximized dispatches like a plain
        // float and rides the shared slotless deferred transfer. Full
        // carry is pinned by workspace-send-float-combo-carry.test.ts.
        const h = entryHarness();
        try {
            h.winF.maximizeMode = 3;
            h.floatingIds.add("n-win-f");
            assert.equal(h.requestSend("ws-2", true), true, "maximized combo dispatches");
            assert.equal(h.payloads.length, 1, "planner request leaves");
            const body = JSON.parse(h.payloads[0] ?? "{}") as Record<string, unknown>;
            const windows = body["windows"] as Array<Record<string, unknown>>;
            const mover = windows.find((entry) => entry["window"] === "n-win-f") as Record<string, unknown>;
            assert.equal(mover["floating"], true, "combo mover rides still floating");
            assert.equal(mover["maximized"], true, "combo mover rides still maximized");
            assert.equal(body["focused_window"], "n-win-f");
        } finally {
            h.stop();
        }
    });

    it("a maximized tiled mover still dispatches (overlay carry preserved)", () => {
        const h = entryHarness();
        try {
            h.winF.maximizeMode = 3;
            assert.equal(h.requestSend("ws-2", true), true, "maximized tiled send still accepted");
            assert.equal(h.payloads.length, 1, "planner request leaves");
            const body = JSON.parse(h.payloads[0] ?? "{}") as Record<string, unknown>;
            const windows = body["windows"] as Array<Record<string, unknown>>;
            const mover = windows.find((entry) => entry["window"] === "n-win-f") as Record<string, unknown>;
            assert.equal(mover["floating"], undefined, "tiled mover rides never floating");
        } finally {
            h.stop();
        }
    });

    it("without the floating-ids input the legacy tiled-looking dispatch is preserved", () => {
        const out = { name: "out-1" };
        const d1 = { id: "ws-1" };
        const d2 = { id: "ws-2" };
        const winF = entryWindow("n-win-f", out, d1, { frameGeometry: 0, desktops: 0, fullScreen: 0, maximizeMode: 0 });
        const winS = entryWindow("n-win-s", out, d1, { frameGeometry: 0, desktops: 0, fullScreen: 0, maximizeMode: 0 });
        const winT = entryWindow("n-win-t", out, d2, { frameGeometry: 0, desktops: 0, fullScreen: 0, maximizeMode: 0 });
        const surface: Record<string, unknown> = {
            activeWindow: winF,
            currentDesktop: d1,
            screens: [out],
            desktops: [d1, d2],
            currentDesktopForScreen: (): object => d1,
            setCurrentDesktopForScreen: (): boolean => true,
            clientArea: (): object => ({ x: 0, y: 0, width: 1200, height: 800 }),
            windowList: (): Array<object> => [winF, winS, winT],
        };
        const payloads: string[] = [];
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
                    return;
                }
                callback(undefined);
            },
            scheduleOnce: () => (): void => {},
            log: () => {},
            owner: "owner-1",
            generation: "gen-1",
            readInnerGapFn: () => 8,
            readOuterGapFn: () => 8,
        });
        try {
            assert.notEqual(entry, null);
            assert.equal(entry?.requestSend("ws-2", true), true, "legacy dispatch preserved");
            const body = JSON.parse(payloads[0] ?? "{}") as Record<string, unknown>;
            const windows = body["windows"] as Array<Record<string, unknown>>;
            const mover = windows.find((entry) => entry["window"] === "n-win-f") as Record<string, unknown>;
            assert.equal(mover["floating"], undefined, "legacy wire omits floating without the input");
        } finally {
            entry?.stop();
        }
    });
});

// ---- Real production observers dispatch floats to the deferred route. ----
// The harnesses above prove the adapter against canned observation; these
// flights observe through the REAL production observers
// (observeSendTarget / observeOutputSendTarget with the live floating-ids
// set) and carry a canned `move-deferred` reply, proving the opened
// production gates reach the deferred route end to end.

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

function realWin(id: string, output: { name: string }, desktop: object): RealWin {
    return {
        normalWindow: true,
        managed: true,
        minimized: false,
        fullScreen: false,
        maximizeMode: 0,
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

describe("D16 real production observers dispatch floats deferred", () => {
    function startRealSameOutput(follow: boolean): {
        adapter: WorkspaceSendAdapter;
        calls: Array<{ method: string; payload: string }>;
        callbacks: Array<(reply: unknown) => void>;
        writes: { memberships: object[]; geometries: object[]; switches: object[]; focuses: object[] };
        floatingIds: Set<string>;
    } {
        const out = { name: "out-1" };
        const d1 = { id: "ws-1" };
        const d2 = { id: "ws-2" };
        const winF = realWin("n-win-f", out, d1);
        const winS = realWin("n-win-s", out, d1);
        const winT = realWin("n-win-t", out, d2);
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
        assert.equal(adapter.requestSend("ws-2", 2, follow), true, "real-observer float dispatches");
        assert.equal(calls[0]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
        callbacks[0]?.(":1.7");
        return { adapter, calls, callbacks, writes, floatingIds };
    }

    it("same-output follow carries a real-observed float deferred", () => {
        const flight = startRealSameOutput(true);
        const body = JSON.parse(flight.calls[1]?.payload ?? "{}") as Record<string, unknown>;
        const windows = body["windows"] as Array<Record<string, unknown>>;
        assert.equal(windows.find((entry) => entry["window"] === "n-win-f")?.["floating"], true);
        assert.equal(body["focused_window"], "n-win-f");
        const correlation = body["correlation_id"] as string;
        flight.callbacks[flight.callbacks.length - 1]?.(
            realReply(correlation, "send-to-workspace", true, { output: "out-1", workspace: "ws-1" }, { output: "out-1", workspace: "ws-2" }),
        );
        assert.equal(flight.writes.memberships.length, 1, "mover membership written once");
        assert.deepEqual(flight.writes.geometries, [], "no geometry writes");
        assert.equal(flight.writes.switches.length, 1, "follow switches exactly once");
        assert.equal(flight.writes.focuses.length, 1, "follow focuses the mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("same-output stay carries a real-observed float with source MRU focus", () => {
        const flight = startRealSameOutput(false);
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

    function startRealOutput(follow: boolean): {
        adapter: WorkspaceSendAdapter;
        calls: Array<{ method: string; payload: string }>;
        callbacks: Array<(reply: unknown) => void>;
        writes: { transfers: object[]; memberships: object[]; geometries: object[]; focuses: object[] };
    } {
        const out1 = { name: "out-1" };
        const out2 = { name: "out-2" };
        const wsA = { id: "ws-1" };
        const wsB = { id: "ws-2" };
        const winF = realWin("n-win-f", out1, wsA);
        const winS = realWin("n-win-s", out1, wsA);
        const winT = realWin("n-win-t", out2, wsB);
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
                        return { id: entry.internalId, floating: floatingIds.has(entry.internalId), sticky: false, fullscreen: false, maximized: false };
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
        assert.equal(adapter.requestSendToOutput("out-2", "ws-2", follow), true, "real cross-output float dispatches");
        assert.equal(calls[0]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
        callbacks[0]?.(":1.7");
        return { adapter, calls, callbacks, writes };
    }

    it("cross-output follow carries a real-observed float deferred", () => {
        const flight = startRealOutput(true);
        const body = JSON.parse(flight.calls[1]?.payload ?? "{}") as Record<string, unknown>;
        const windows = body["windows"] as Array<Record<string, unknown>>;
        assert.equal(windows.find((entry) => entry["window"] === "n-win-f")?.["floating"], true);
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

    it("cross-output stay carries a real-observed float with source MRU focus", () => {
        const flight = startRealOutput(false);
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
});
