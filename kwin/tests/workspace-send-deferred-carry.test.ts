import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    WORKSPACE_SEND_CONTRACT_VERSION,
    WORKSPACE_SEND_GET_OWNER_METHOD,
    WORKSPACE_SEND_HAS_OWNER_METHOD,
    WorkspaceSendAdapter,
    type WorkspaceSendAdapterEnv,
    type WorkspaceSendObserved,
} from "../src/workspace-send-adapter";

// D28 slotless deferred transfer (option (a)): explicit numbered/relative
// follow/stay send of a NEVER-TILED born-fullscreen window. The shared
// `move-deferred` operation carries window plus source/target identities
// with NO leaf. Reply geometry covers tiled survivors only (unchanged
// allocations, never reflow); the slotless mover takes no allocation and
// no geometry write. Follow switches and focuses the mover native ref;
// stay focuses the source MRU survivor with no switch. Offline only:
// adapter-level harness with canned Engine replies plus real entry
// observation shapes.

function rect(x: number, y: number, w: number, h: number): { x: number; y: number; w: number; h: number } {
    return { x, y, w, h };
}

interface DeferredWorld {
    readonly windows: Array<{
        id: string;
        ref: object;
        rect: { x: number; y: number; w: number; h: number };
        workspace: string;
        fullscreen: boolean;
    }>;
}

function deferredWorld(refs: { f: object; s: object; t: object }): DeferredWorld {
    return {
        windows: [
            // Held slotless mover: native send-route shape (never floating
            // here; the send entry maps only sticky to floating), still
            // fullscreen with no tile slot.
            { id: "win-f", ref: refs.f, rect: rect(0, 0, 1200, 800), workspace: "ws-1", fullscreen: true },
            { id: "win-s", ref: refs.s, rect: rect(0, 0, 1200, 800), workspace: "ws-1", fullscreen: false },
            { id: "win-t", ref: refs.t, rect: rect(0, 0, 1200, 800), workspace: "ws-2", fullscreen: false },
        ],
    };
}

function deferredObserved(
    world: DeferredWorld,
    refs: { f: object; s: object; t: object; desktop: object },
    activeRef: object | null,
): WorkspaceSendObserved {
    const flagged = (entry: DeferredWorld["windows"][number]): Record<string, unknown> =>
        Object.freeze({
            id: entry.id,
            ref: entry.ref,
            rect: Object.freeze({ ...entry.rect }),
            fullscreen: entry.fullscreen,
            maximized: false,
            floating: false,
            sticky: false,
            fitExcluded: entry.fullscreen,
            fit_excluded: entry.fullscreen,
        });
    const source = world.windows.filter((entry) => entry.workspace === "ws-1");
    const activeId = world.windows.find((entry) => entry.ref === activeRef)?.id ?? "";
    const focused = source.some((entry) => entry.id === activeId) ? activeId : "";
    const moverEntry = source.find((entry) => entry.id === activeId);
    return {
        sourceOutput: "out-1",
        sourceWorkspace: "ws-1",
        sourceBounds: Object.freeze(rect(0, 0, 1200, 800)),
        targetOutput: "out-1",
        targetWorkspace: "ws-2",
        targetBounds: Object.freeze(rect(0, 0, 1200, 800)),
        focusedId: focused,
        sourceWindows: Object.freeze(source.map(flagged)),
        targetWindows: Object.freeze(world.windows.filter((entry) => entry.workspace === "ws-2").map(flagged)),
        activeRef,
        moverRef: moverEntry === undefined ? null : moverEntry.ref,
        targetDesktopRef: refs.desktop,
        targetExists: true,
        desktopCount: 2,
        sourceFingerprint: "sfp-1",
        targetFingerprint: "tfp-1",
        currentWorkspace: "ws-1",
    } as unknown as WorkspaceSendObserved;
}

function deferredReply(correlation: string, follow: boolean): string {
    return JSON.stringify({
        v: WORKSPACE_SEND_CONTRACT_VERSION,
        correlation_id: correlation,
        outcome: "planned",
        kind: "send-to-workspace",
        base_revision: 0,
        detail: { kind: "send-to-workspace", policy_version: 1, capability: "move-tiled" },
        // Tiled survivors only, allocations exactly retained (never
        // reflow); the slotless mover takes no allocation. Follow books
        // the target MRU tile (native follow still addresses the mover
        // ref); stay books the source MRU survivor.
        desired_geometry: [
            { window: "win-s", leaf: "leaf-win-s", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 1200, h: 800 } },
            { window: "win-t", leaf: "leaf-win-t", output: "out-1", workspace: "ws-2", rect: { x: 0, y: 0, w: 1200, h: 800 } },
        ],
        desired_focus: follow
            ? { domain_output: "out-1", domain_workspace: "ws-2", leaf: "leaf-win-t" }
            : { domain_output: "out-1", domain_workspace: "ws-1", leaf: "leaf-win-s" },
        preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
        operation: {
            op: "move-deferred",
            window: "win-f",
            source_output: "out-1",
            source_workspace: "ws-1",
            target_output: "out-1",
            target_workspace: "ws-2",
            follow,
        },
    });
}

interface DeferredFlight {
    readonly refs: { f: object; s: object; t: object; desktop: object };
    readonly world: DeferredWorld;
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

function startDeferredFlight(): DeferredFlight {
    const refs = { f: {}, s: {}, t: {}, desktop: {} };
    const world = deferredWorld(refs);
    const geometries: object[] = [];
    const moverWrites: object[] = [];
    const switches: object[] = [];
    const focuses: object[] = [];
    const logs: string[] = [];
    const dbusCalls: DeferredFlight["dbusCalls"] = [];
    const callbacks: DeferredFlight["callbacks"] = [];
    const flight: DeferredFlight = {
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
        observe: () => deferredObserved(world, refs, flight.activeRef),
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
// numbered/relative ring resolution lives in the entry routes, covered by
// the existing entry tests and the production entry drive below. It never
// gates request behavior.
function dispatchDeferred(flight: DeferredFlight, ordinal: unknown, follow: unknown): string {
    assert.equal(flight.adapter.requestSend("ws-2", ordinal, follow), true, "slotless carry must dispatch");
    assert.equal(flight.dbusCalls[0]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
    flight.callbacks[0]?.(":1.7");
    const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
    // The wire carries the held mover with native flags: never floating
    // here, still fullscreen with the fit opt-out.
    const windows = body["windows"] as Array<Record<string, unknown>>;
    const mover = windows.find((entry) => entry["window"] === "win-f") as Record<string, unknown>;
    assert.equal(mover["fit_excluded"], true, "held mover rides fit-excluded");
    assert.equal(body["focused_window"], "win-f", "explicit send focuses the held mover");
    return body["correlation_id"] as string;
}

function assertDeferredWireFullscreen(flight: DeferredFlight): void {
    const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
    const windows = body["windows"] as Array<Record<string, unknown>>;
    const mover = windows.find((entry) => entry["window"] === "win-f") as Record<string, unknown>;
    assert.equal(mover["floating"], undefined, "held mover rides the send wire never-floating");
    assert.equal(mover["fullscreen"], true, "held mover rides still fullscreen");
}

describe("D28 slotless deferred carry (adapter follow/stay)", () => {
    it("numbered follow carries slotless with one membership write and zero geometry", () => {
        const flight = startDeferredFlight();
        const correlation = dispatchDeferred(flight, 2, true);
        assertDeferredWireFullscreen(flight);
        flight.callbacks[1]?.(deferredReply(correlation, true));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.moverWrites[0], flight.refs.f);
        assert.deepEqual(flight.geometries, [], "no geometry write: allocations never reflow");
        assert.deepEqual(flight.switches, [flight.refs.desktop], "follow switches exactly once");
        assert.deepEqual(flight.focuses, [flight.refs.f], "follow focuses the slotless mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("numbered stay carries slotless with source MRU focus and no switch", () => {
        const flight = startDeferredFlight();
        const correlation = dispatchDeferred(flight, 2, false);
        assertDeferredWireFullscreen(flight);
        flight.callbacks[1]?.(deferredReply(correlation, false));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.geometries, [], "no geometry write on stay either");
        assert.deepEqual(flight.switches, [], "stay never switches desktops");
        assert.deepEqual(flight.focuses, [flight.refs.s], "stay focuses the source MRU survivor");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("diagnostic ordinal -1 follow carries slotless like the numbered route", () => {
        const flight = startDeferredFlight();
        const correlation = dispatchDeferred(flight, -1, true);
        assertDeferredWireFullscreen(flight);
        flight.callbacks[1]?.(deferredReply(correlation, true));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.geometries, [], "no geometry write on the relative route");
        assert.deepEqual(flight.switches, [flight.refs.desktop], "relative follow switches exactly once");
        assert.deepEqual(flight.focuses, [flight.refs.f], "relative follow focuses the mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("diagnostic ordinal -1 stay carries slotless with source MRU focus", () => {
        const flight = startDeferredFlight();
        const correlation = dispatchDeferred(flight, -1, false);
        assertDeferredWireFullscreen(flight);
        flight.callbacks[1]?.(deferredReply(correlation, false));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.switches, [], "relative stay never switches desktops");
        assert.deepEqual(flight.focuses, [flight.refs.s], "relative stay focuses the source MRU survivor");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("a mover fullscreen flag race refuses without setters or follow", () => {
        const flight = startDeferredFlight();
        const correlation = dispatchDeferred(flight, 2, true);
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.fullscreen = false;
            }
        }
        flight.callbacks[1]?.(deferredReply(correlation, true));
        assert.deepEqual(flight.moverWrites, [], "flag-changed mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "flag-changed mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "flag-changed mover never switches");
        assert.deepEqual(flight.focuses, [], "flag-changed mover never focuses");
    });

    it("a move-deferred operation carrying a leaf is malformed and never actuates", () => {
        const flight = startDeferredFlight();
        const correlation = dispatchDeferred(flight, 2, true);
        const malformed = JSON.parse(deferredReply(correlation, true)) as Record<string, unknown>;
        (malformed["operation"] as Record<string, unknown>)["leaf"] = "leaf-win-f";
        flight.callbacks[1]?.(JSON.stringify(malformed));
        assert.deepEqual(flight.moverWrites, [], "leaved deferred reply never actuates");
        assert.deepEqual(flight.geometries, [], "leaved deferred reply writes no geometry");
        assert.deepEqual(flight.switches, [], "leaved deferred reply never switches");
        assert.deepEqual(flight.focuses, [], "leaved deferred reply never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("a deferred follow naming tiled focus is malformed and never actuates", () => {
        const flight = startDeferredFlight();
        const correlation = dispatchDeferred(flight, 2, true);
        const malformed = JSON.parse(deferredReply(correlation, true)) as Record<string, unknown>;
        malformed["desired_focus"] = { domain_output: "out-1", domain_workspace: "ws-2", leaf: "leaf-win-f" };
        flight.callbacks[1]?.(JSON.stringify(malformed));
        assert.deepEqual(flight.moverWrites, [], "unresolvable deferred-follow focus never actuates");
        assert.deepEqual(flight.focuses, [], "unresolvable deferred-follow focus never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("a deferred follow naming source-domain focus is malformed and never actuates", () => {
        const flight = startDeferredFlight();
        const correlation = dispatchDeferred(flight, 2, true);
        const malformed = JSON.parse(deferredReply(correlation, true)) as Record<string, unknown>;
        malformed["desired_focus"] = { domain_output: "out-1", domain_workspace: "ws-1", leaf: "leaf-win-s" };
        flight.callbacks[1]?.(JSON.stringify(malformed));
        assert.deepEqual(flight.moverWrites, [], "source-domain deferred-follow focus never actuates");
        assert.deepEqual(flight.focuses, [], "source-domain deferred-follow focus never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("empty geometry carries a sole slotless mover with null focus", () => {
        // P1: sole slotless source sent to an empty target legitimately
        // carries no geometry. Tiled replies still refuse [] below.
        const flight = startDeferredFlight();
        flight.world.windows.length = 0;
        flight.world.windows.push(
            { id: "win-f", ref: flight.refs.f, rect: rect(0, 0, 1200, 800), workspace: "ws-1", fullscreen: true },
        );
        const correlation = dispatchDeferred(flight, 2, true);
        const reply = JSON.parse(deferredReply(correlation, true)) as Record<string, unknown>;
        reply["desired_geometry"] = [];
        reply["desired_focus"] = null;
        flight.callbacks[1]?.(JSON.stringify(reply));
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.moverWrites[0], flight.refs.f);
        assert.deepEqual(flight.geometries, [], "no geometry to write");
        assert.deepEqual(flight.switches, [flight.refs.desktop], "follow switches exactly once");
        assert.deepEqual(flight.focuses, [flight.refs.f], "follow focuses the slotless mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("tiled replies still refuse empty geometry", () => {
        // P1 guard: [] acceptance is deferred-only.
        const flight = startDeferredFlight();
        const correlation = dispatchDeferred(flight, 2, true);
        const reply = JSON.parse(deferredReply(correlation, true)) as Record<string, unknown>;
        reply["operation"] = {
            op: "move-tiled",
            window: "win-f",
            leaf: "leaf-win-f",
            source_output: "out-1",
            source_workspace: "ws-1",
            target_output: "out-1",
            target_workspace: "ws-2",
            follow: true,
        };
        reply["desired_geometry"] = [];
        reply["desired_focus"] = { domain_output: "out-1", domain_workspace: "ws-2", leaf: "leaf-win-f" };
        flight.callbacks[1]?.(JSON.stringify(reply));
        assert.deepEqual(flight.moverWrites, [], "empty tiled reply never actuates");
        assert.deepEqual(flight.geometries, [], "empty tiled reply writes no geometry");
        assert.deepEqual(flight.switches, [], "empty tiled reply never switches");
        assert.deepEqual(flight.focuses, [], "empty tiled reply never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });

    it("drifted survivor allocations still take zero geometry writes", () => {
        // P2: deferred transfers never actuate mover OR survivor geometry,
        // even when retained allocations differ from observed frames (the
        // next ordinary reconcile reflows; the send does not).
        const flight = startDeferredFlight();
        const correlation = dispatchDeferred(flight, 2, true);
        const reply = JSON.parse(deferredReply(correlation, true)) as Record<string, unknown>;
        const geometry = reply["desired_geometry"] as Array<Record<string, unknown>>;
        for (const entry of geometry) {
            entry["rect"] = { x: 111, y: 222, w: 333, h: 444 };
        }
        flight.callbacks[1]?.(JSON.stringify(reply));
        assert.equal(flight.moverWrites.length, 1, "membership still transfers");
        assert.deepEqual(flight.geometries, [], "drifted survivors take zero geometry writes");
        assert.deepEqual(flight.switches, [flight.refs.desktop], "follow still switches");
        assert.deepEqual(flight.focuses, [flight.refs.f], "follow still focuses the mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("a leafless transfer for a non-held snapshot mover never actuates", () => {
        // P3: an ordinary tiled/non-fullscreen mover cannot sneak a
        // leafless operation and drop its own allocation.
        const flight = startDeferredFlight();
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.fullscreen = false;
            }
        }
        assert.equal(flight.adapter.requestSend("ws-2", 2, true), true, "tiled mover dispatches");
        flight.callbacks[0]?.(":1.7");
        const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
        const correlation = body["correlation_id"] as string;
        flight.callbacks[1]?.(deferredReply(correlation, true));
        assert.deepEqual(flight.moverWrites, [], "non-held mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "non-held mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "non-held mover never switches");
        assert.deepEqual(flight.focuses, [], "non-held mover never focuses");
        assert.ok(
            flight.logs.some((line) => line.includes("precondition-mismatch")),
            flight.logs.join("\n"),
        );
    });
});

// ---- Deferred cross-output carry (send-to-output, same op/fences). ----

interface XWorld {
    readonly windows: Array<{
        id: string;
        ref: object;
        rect: { x: number; y: number; w: number; h: number };
        output: string;
        workspace: string;
        fullscreen: boolean;
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
            fullscreen: entry.fullscreen,
            maximized: false,
            floating: false,
            sticky: false,
            fitExcluded: entry.fullscreen,
            fit_excluded: entry.fullscreen,
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

function xDeferredReply(correlation: string, follow: boolean): string {
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

function startXFlight(): XFlight {
    const refs = { f: {}, s: {}, t: {}, out2: {}, desktop2: {} };
    const world: XWorld = {
        windows: [
            { id: "win-f", ref: refs.f, rect: rect(0, 0, 1200, 800), output: "out-1", workspace: "ws-1", fullscreen: true },
            { id: "win-s", ref: refs.s, rect: rect(0, 0, 1200, 800), output: "out-1", workspace: "ws-1", fullscreen: false },
            { id: "win-t", ref: refs.t, rect: rect(0, 0, 1200, 800), output: "out-2", workspace: "ws-2", fullscreen: false },
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
                    return { id: entry.id, floating: false, sticky: false, fullscreen: entry.fullscreen, maximized: false };
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

function dispatchX(flight: XFlight, follow: unknown): string {
    assert.equal(flight.adapter.requestSendToOutput("out-2", "ws-2", follow), true, "cross-output carry must dispatch");
    assert.equal(flight.dbusCalls[0]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
    flight.callbacks[0]?.(":1.7");
    const body = JSON.parse(flight.dbusCalls[1]?.payload ?? "{}") as Record<string, unknown>;
    const command = body["command"] as Record<string, unknown>;
    assert.equal(command["op"], "send-to-output");
    const windows = body["windows"] as Array<Record<string, unknown>>;
    const mover = windows.find((entry) => entry["window"] === "win-f") as Record<string, unknown>;
    assert.equal(mover["floating"], undefined, "held mover rides never-floating");
    assert.equal(mover["fullscreen"], true, "held mover rides still fullscreen");
    assert.equal(body["focused_window"], "win-f");
    return body["correlation_id"] as string;
}

describe("D28 slotless deferred carry (cross-output follow/stay)", () => {
    it("cross-output follow transfers, writes no geometry, and focuses the mover", () => {
        const flight = startXFlight();
        const correlation = dispatchX(flight, true);
        flight.callbacks[1]?.(xDeferredReply(correlation, true));
        assert.equal(flight.transfers.length, 1, "output transfer runs once");
        assert.equal(flight.moverWrites.length, 1, "mover membership written once");
        assert.deepEqual(flight.geometries, [], "no geometry writes across outputs either");
        assert.deepEqual(flight.switches, [], "cross-output follow never switches desktops");
        assert.deepEqual(flight.focuses, [flight.refs.f], "cross-output follow focuses the mover");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("cross-output stay transfers with source MRU focus and no switch", () => {
        const flight = startXFlight();
        const correlation = dispatchX(flight, false);
        flight.callbacks[1]?.(xDeferredReply(correlation, false));
        assert.equal(flight.transfers.length, 1, "output transfer runs once on stay");
        assert.equal(flight.moverWrites.length, 1, "mover membership written once on stay");
        assert.deepEqual(flight.geometries, [], "no geometry writes on stay either");
        assert.deepEqual(flight.switches, [], "stay never switches desktops");
        assert.deepEqual(flight.focuses, [flight.refs.s], "stay focuses the source MRU survivor");
        assert.equal(flight.adapter.isInFlight, false);
    });

    it("a cross-output mover fullscreen race refuses without setters", () => {
        const flight = startXFlight();
        const correlation = dispatchX(flight, true);
        for (const entry of flight.world.windows) {
            if (entry.ref === flight.refs.f) {
                entry.fullscreen = false;
            }
        }
        flight.callbacks[1]?.(xDeferredReply(correlation, true));
        assert.deepEqual(flight.transfers, [], "flag-changed mover never transfers outputs");
        assert.deepEqual(flight.moverWrites, [], "flag-changed mover takes no membership write");
        assert.deepEqual(flight.geometries, [], "flag-changed mover takes no geometry writes");
        assert.deepEqual(flight.switches, [], "flag-changed mover never switches");
        assert.deepEqual(flight.focuses, [], "flag-changed mover never focuses");
    });
});
