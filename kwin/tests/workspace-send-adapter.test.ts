import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    WORKSPACE_SEND_CONTRACT_VERSION,
    WORKSPACE_SEND_DBUS_INTERFACE,
    WORKSPACE_SEND_DBUS_OBJECT,
    WORKSPACE_SEND_DBUS_SERVICE,
    WORKSPACE_SEND_GET_OWNER_METHOD,
    WORKSPACE_SEND_HAS_OWNER_METHOD,
    WORKSPACE_SEND_INTERFACE,
    WORKSPACE_SEND_METHOD,
    WORKSPACE_SEND_OBJECT,
    WorkspaceSendAdapter,
    WorkspaceSendAdapterEnv,
    WorkspaceSendObserved,
    WorkspaceSendSettled,
} from "../src/workspace-send-adapter";

function makeRefs(): { a: object; b: object; t: object; desktop: object } {
    return { a: {}, b: {}, t: {}, desktop: {} };
}

function rect(x: number, y: number, w: number, h: number): { x: number; y: number; w: number; h: number } {
    return { x, y, w, h };
}

interface WorldWindow {
    readonly id: string;
    readonly ref: object;
    rect: { x: number; y: number; w: number; h: number };
    workspace: string;
}

interface World {
    readonly windows: WorldWindow[];
    activeRef: object | null;
    desktopCount: number;
    targetExists: boolean;
}

function defaultWorld(refs: { a: object; b: object; t: object; desktop: object }): World {
    return {
        windows: [
            { id: "win-a", ref: refs.a, rect: rect(0, 0, 100, 100), workspace: "ws-1" },
            { id: "win-b", ref: refs.b, rect: rect(100, 0, 100, 100), workspace: "ws-1" },
            { id: "win-t", ref: refs.t, rect: rect(0, 0, 100, 100), workspace: "ws-2" },
        ],
        activeRef: refs.a,
        desktopCount: 2,
        targetExists: true,
    };
}

function worldObserved(world: World, refs: { a: object; b: object; t: object; desktop: object }): WorkspaceSendObserved {
    const sourceWindows = Object.freeze(
        world.windows
            .filter((entry) => entry.workspace === "ws-1")
            .map((entry) =>
                Object.freeze({ id: entry.id, ref: entry.ref, rect: Object.freeze({ ...entry.rect }) }),
            ),
    );
    const targetWindows = Object.freeze(
        world.windows
            .filter((entry) => entry.workspace === "ws-2")
            .map((entry) =>
                Object.freeze({ id: entry.id, ref: entry.ref, rect: Object.freeze({ ...entry.rect }) }),
            ),
    );
    const activeId = world.windows.find((entry) => entry.ref === world.activeRef)?.id ?? "";
    const focused = sourceWindows.some((entry) => entry.id === activeId) ? activeId : "";
    const moverEntry = sourceWindows.find((entry) => entry.id === activeId);
    return {
        sourceOutput: "out-1",
        sourceWorkspace: "ws-1",
        sourceBounds: Object.freeze(rect(0, 0, 1200, 800)),
        targetOutput: "out-1",
        targetWorkspace: "ws-2",
        targetBounds: Object.freeze(rect(0, 0, 1200, 800)),
        focusedId: focused,
        sourceWindows,
        targetWindows,
        activeRef: world.activeRef,
        moverRef: moverEntry === undefined ? null : moverEntry.ref,
        targetDesktopRef: refs.desktop,
        targetExists: world.targetExists,
        desktopCount: world.desktopCount,
        sourceFingerprint: "sfp-1",
        targetFingerprint: "tfp-1",
        currentWorkspace: "ws-1",
    };
}

interface DbusCall {
    readonly service: string;
    readonly path: string;
    readonly iface: string;
    readonly method: string;
    readonly payload: string;
}

interface Mocks {
    readonly dbusCalls: DbusCall[];
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }>;
    readonly logs: string[];
    readonly geometries: Array<{ target: object; rect: { x: number; y: number; w: number; h: number } }>;
    readonly desktops: Array<{ target: object; refs: ReadonlyArray<object> }>;
    readonly switches: object[];
    readonly focuses: object[];
    readonly order: string[];
    readonly world: World;
    readonly arrivalHandlers: Array<() => void>;
    settled: number;
    readonly settledInfos: WorkspaceSendSettled[];
    observeImpl: () => WorkspaceSendObserved | null;
    geometryImpl: (target: object, r: { x: number; y: number; w: number; h: number }) => boolean;
    desktopsImpl: (target: object, refs: ReadonlyArray<object>) => boolean;
    switchImpl: (desktopRef: object) => boolean;
    focusImpl: (windowRef: object) => boolean;
    holdArrival: boolean;
    env: WorkspaceSendAdapterEnv;
}

function mockEnv(refs: { a: object; b: object; t: object; desktop: object }): Mocks {
    const state = {
        dbusCalls: [],
        callbacks: [],
        timers: [],
        logs: [],
        geometries: [],
        desktops: [],
        switches: [],
        focuses: [],
        order: [],
        world: defaultWorld(refs),
        arrivalHandlers: [],
        settled: 0,
        settledInfos: [],
        observeImpl: null as unknown as Mocks["observeImpl"],
        geometryImpl: () => true,
        desktopsImpl: () => true,
        switchImpl: () => true,
        focusImpl: () => true,
        holdArrival: false,
        env: null as unknown as WorkspaceSendAdapterEnv,
    } as Mocks;
    state.observeImpl = () => worldObserved(state.world, refs);
    state.env = {
        callDbus: (service, path, iface, method, payload, callback) => {
            if (method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                callback(true);
                return;
            }
            state.dbusCalls.push({ service, path, iface, method, payload });
            state.callbacks.push(callback);
        },
        scheduleOnce: (delayMs, callback) => {
            const timer = { delayMs, callback, cancelled: false };
            state.timers.push(timer);
            return () => {
                timer.cancelled = true;
            };
        },
        log: (message) => {
            state.logs.push(message);
        },
        observe: () => state.observeImpl(),
        onSettled: (settled) => {
            state.settled += 1;
            state.settledInfos.push(settled);
        },
        setGeometry: (target, r) => {
            state.geometries.push({ target, rect: r });
            const ok = state.geometryImpl(target, r);
            if (ok) {
                for (const entry of state.world.windows) {
                    if (entry.ref === target) {
                        entry.rect = { x: r.x, y: r.y, w: r.w, h: r.h };
                    }
                }
            }
            return ok;
        },
        readGeometry: (target) => {
            for (const entry of state.world.windows) {
                if (entry.ref === target) {
                    return { ...entry.rect };
                }
            }
            return null;
        },
        setDesktops: (target, refsArg) => {
            state.desktops.push({ target, refs: refsArg });
            const ok = state.desktopsImpl(target, refsArg);
            if (ok && refsArg.length > 0 && !state.holdArrival) {
                for (const entry of state.world.windows) {
                    if (entry.ref === target) {
                        entry.workspace = "ws-2";
                    }
                }
            }
            return ok;
        },
        switchToTarget: (desktopRef) => {
            state.order.push("switch");
            state.switches.push(desktopRef);
            return state.switchImpl(desktopRef);
        },
        focusWindow: (windowRef) => {
            state.order.push("focus");
            state.focuses.push(windowRef);
            return state.focusImpl(windowRef);
        },
        subscribeMoverDesktops: (_moverRef: object, handler: () => void) => {
            state.arrivalHandlers.push(handler);
            let detached = false;
            return () => {
                detached = true;
                void detached;
            };
        },
    };
    return state;
}

const KNOWN_PRECONDITIONS = [
    "window-observed",
    "desired-topology-valid",
    "adapter-must-verify-postconditions",
];

function plannedReply(correlation: string): string {
    return JSON.stringify({
        v: WORKSPACE_SEND_CONTRACT_VERSION,
        correlation_id: correlation,
        outcome: "planned",
        kind: "send-to-workspace",
        base_revision: 0,
        detail: { kind: "send-to-workspace", policy_version: 1, capability: "move-tiled" },
        desired_geometry: [
            { window: "win-a", leaf: "leaf-win-a", output: "out-1", workspace: "ws-2", rect: { x: 0, y: 0, w: 600, h: 800 } },
            { window: "win-b", leaf: "leaf-win-b", output: "out-1", workspace: "ws-1", rect: { x: 0, y: 0, w: 1200, h: 800 } },
            { window: "win-t", leaf: "leaf-win-t", output: "out-1", workspace: "ws-2", rect: { x: 600, y: 0, w: 600, h: 800 } },
        ],
        desired_focus: { domain_output: "out-1", domain_workspace: "ws-2", leaf: "leaf-win-a" },
        preconditions: KNOWN_PRECONDITIONS,
        operation: {
            op: "move-tiled",
            window: "win-a",
            leaf: "leaf-win-a",
            source_output: "out-1",
            source_workspace: "ws-1",
            target_output: "out-1",
            target_workspace: "ws-2",
        },
    });
}

function rejectedReply(correlation: string, kind: string): string {
    return JSON.stringify({
        v: WORKSPACE_SEND_CONTRACT_VERSION,
        correlation_id: correlation,
        outcome: "rejected",
        kind,
    });
}

function parsePayload(payload: string): Record<string, unknown> {
    return JSON.parse(payload) as Record<string, unknown>;
}

// Drive dispatch through owner pinning and return the request correlation.
function dispatch(mocks: Mocks, adapter: WorkspaceSendAdapter): string {
    assert.equal(adapter.requestSend("ws-2"), true);
    assert.deepEqual(adapter.pendingWorkspaces, ["ws-1", "ws-2"]);
    const ownerCall = mocks.dbusCalls[0];
    assert.equal(ownerCall?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
    assert.equal(ownerCall?.service, WORKSPACE_SEND_DBUS_SERVICE);
    assert.equal(ownerCall?.path, WORKSPACE_SEND_DBUS_OBJECT);
    assert.equal(ownerCall?.iface, WORKSPACE_SEND_DBUS_INTERFACE);
    mocks.callbacks[0]?.(":1.7");
    const requestCall = mocks.dbusCalls[1];
    assert.equal(requestCall?.service, ":1.7");
    assert.equal(requestCall?.path, WORKSPACE_SEND_OBJECT);
    assert.equal(requestCall?.iface, WORKSPACE_SEND_INTERFACE);
    assert.equal(requestCall?.method, WORKSPACE_SEND_METHOD);
    const body = parsePayload(requestCall?.payload ?? "{}");
    const command = body["command"] as Record<string, unknown>;
    assert.equal(command["op"], "send-to-workspace");
    const correlation = body["correlation_id"] as string;
    assert.ok(correlation.length > 0);
    return correlation;
}

function assertRedacted(mocks: Mocks): void {
    for (const line of mocks.logs) {
        assert.ok(line.startsWith("omnitiler:route-diag component=cosmic-send "), line);
        for (const raw of ["win-a", "win-b", "win-t", "ws-1", "ws-2", "out-1", ":1.7", "owner-1"]) {
            assert.ok(!line.includes(raw), `${raw} leaked in:\n${line}`);
        }
    }
}

describe("cosmic send-to-workspace immediate commit", () => {
    it("sends, writes planned geometry then mover membership, follows once on immediate arrival", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        // Geometry writes cover all three windows; only the mover desktop write follows.
        assert.equal(mocks.geometries.length, 3);
        assert.equal(mocks.desktops.length, 1);
        assert.equal(mocks.desktops[0]?.target, refs.a);
        assert.deepEqual(mocks.desktops[0]?.refs, [refs.desktop]);
        // Immediate exact arrival proof: switch then focus, exactly once.
        assert.deepEqual(mocks.switches, [refs.desktop]);
        assert.deepEqual(mocks.focuses, [refs.a]);
        assert.deepEqual(mocks.order, ["switch", "focus"]);
        assert.ok(mocks.logs.some((l) => l.includes("event=dispatch") && l.includes("outcome=started")), mocks.logs.join("\n"));
        assert.ok(mocks.logs.some((l) => l.includes("event=arrival") && l.includes("outcome=arrived")), mocks.logs.join("\n"));
        assert.ok(
            mocks.logs.some((l) => l.includes("stage=follow") && l.includes("event=follow") && l.includes("outcome=state-confirmed")),
            mocks.logs.join("\n"),
        );
        assert.ok(mocks.logs.some((l) => l.includes("stage=release") && l.includes("outcome=arrived")), mocks.logs.join("\n"));
        assertRedacted(mocks);
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isInFlight, false);
        assert.equal(adapter.isEnabled, true);
        // No ack/verify/status/cancel/abandon round trips exist.
        for (const call of mocks.dbusCalls.slice(1)) {
            assert.equal(call.method, WORKSPACE_SEND_METHOD);
            const body = parsePayload(call.payload);
            assert.equal((body["command"] as Record<string, unknown>)["op"], "send-to-workspace");
        }
        assert.equal("blocksPlan" in adapter, false);
    });

    it("follows once on delayed arrival via the one-shot mover signal", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.holdArrival = true;
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(mocks.geometries.length, 3);
        assert.equal(mocks.desktops.length, 1);
        // No arrival yet: no follow, flight still pinned.
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.equal(adapter.isInFlight, true);
        assert.deepEqual(adapter.pendingWorkspaces, ["ws-1", "ws-2"]);
        assert.equal(mocks.arrivalHandlers.length, 1);
        assert.ok(mocks.logs.some((l) => l.includes("event=arrival") && l.includes("outcome=waiting")), mocks.logs.join("\n"));
        // Delayed native arrival becomes visible; the signal follows once.
        mocks.holdArrival = false;
        for (const entry of mocks.world.windows) {
            if (entry.id === "win-a") {
                entry.workspace = "ws-2";
            }
        }
        mocks.arrivalHandlers[0]?.();
        assert.deepEqual(mocks.switches, [refs.desktop]);
        assert.deepEqual(mocks.focuses, [refs.a]);
        assert.deepEqual(mocks.order, ["switch", "focus"]);
        assert.equal(mocks.settled, 1);
        assert.equal(adapter.isInFlight, false);
        assertRedacted(mocks);
    });

    it("refuses a duplicate send while in flight and exposes no blocksPlan", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.holdArrival = true;
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        assert.equal(adapter.requestSend("ws-2"), false);
        assert.ok(
            mocks.logs.some((l) => l.includes("event=refuse") && l.includes("outcome=in-flight") && l.includes(`correlation=${correlation}`)),
            mocks.logs.join("\n"),
        );
        assert.equal(adapter.isInFlight, true);
        assert.equal(mocks.settled, 0);
        assert.equal("blocksPlan" in adapter, false);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(adapter.isInFlight, true, "still waiting for delayed arrival");
        mocks.holdArrival = false;
        for (const entry of mocks.world.windows) {
            if (entry.id === "win-a") {
                entry.workspace = "ws-2";
            }
        }
        mocks.arrivalHandlers[0]?.();
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled, 1);
    });

    it("settles stale-revision with no writes when the scope drifts before the reply", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.world.windows.push({ id: "win-c", ref: {}, rect: rect(200, 0, 100, 100), workspace: "ws-1" });
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.desktops.length, 0);
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=stale-revision")), mocks.logs.join("\n"));
        assertRedacted(mocks);
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isInFlight, false);
        assert.equal(adapter.requestSend("ws-2"), true, "send reusable after stale terminal");
        adapter.disable();
    });

    it("settles stale-revision with no writes when only survivor flags drift before the reply", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        // Survivor win-b is a tiled fullscreen overlay at dispatch.
        mocks.observeImpl = () => {
            const live = worldObserved(mocks.world, refs);
            return {
                ...live,
                sourceWindows: Object.freeze(
                    live.sourceWindows.map((entry) =>
                        entry.id === "win-b"
                            ? Object.freeze({
                                  ...entry,
                                  fitExcluded: true,
                                  fit_excluded: true,
                                  fullscreen: true,
                              })
                            : entry,
                    ),
                ),
            };
        };
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        const requestCall = mocks.dbusCalls[1];
        const body = parsePayload(requestCall?.payload ?? "{}");
        const sourceEntries = body["windows"] as Array<Record<string, unknown>>;
        const flagged = sourceEntries.find((entry) => entry["window"] === "win-b");
        assert.equal(flagged?.["fit_excluded"], true, "observed fit opt-out traverses the send wire");
        assert.ok(!("floating" in (flagged ?? {})), "overlay stays tiled on the wire");
        assert.ok(!("fullscreen" in (flagged ?? {})), "native overlay flags never ride the wire");
        // Only flags drift before the reply: same ids, same rects, same
        // membership. Either-domain fence still stales before any setter.
        mocks.observeImpl = () => worldObserved(mocks.world, refs);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.desktops.length, 0);
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=stale-revision")), mocks.logs.join("\n"));
        assertRedacted(mocks);
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isInFlight, false);
        assert.equal(adapter.requestSend("ws-2"), true, "send reusable after flag-stale terminal");
        adapter.disable();
    });

    it("settles stale-revision with no writes when gaps reload before the reply", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        // Deliberate configChanged reload changes the live pair while the
        // flight keeps the dispatch-frozen (8, 8) primitives.
        adapter.updateGaps({ innerGap: 12, outerGap: 8 });
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(mocks.geometries.length, 0, "gap change never reaches a geometry setter");
        assert.equal(mocks.desktops.length, 0, "gap change never reaches the mover write");
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=stale-revision")), mocks.logs.join("\n"));
        assertRedacted(mocks);
        assert.equal(mocks.settled, 1, "terminal stale carries the forced refresh hook");
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isInFlight, false);
        assert.equal(adapter.requestSend("ws-2"), true, "send reusable after gap-stale terminal");
        adapter.disable();
    });

    it("releases on the unanswered-request deadline and ignores the late reply", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        assert.equal(mocks.timers.length, 2, "separate request and arrival deadlines");
        const requestTimer = mocks.timers[0];
        assert.ok(requestTimer && !requestTimer.cancelled);
        requestTimer.callback();
        assert.ok(mocks.logs.some((l) => l.includes("stage=release") && l.includes("outcome=timeout")), mocks.logs.join("\n"));
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isInFlight, false);
        // The late planned reply is ignored: no writes, no follow, no second settle.
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.desktops.length, 0);
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.equal(mocks.settled, 1);
        assert.ok(mocks.logs.some((l) => l.includes("event=late-reply") && l.includes("outcome=ignored")), mocks.logs.join("\n"));
        assertRedacted(mocks);
    });

    it("releases on the arrival deadline when the mover never arrives", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.holdArrival = true;
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.ok(mocks.timers[0]?.cancelled, "answered request deadline is retired");
        assert.ok(!mocks.timers[1]?.cancelled, "arrival deadline stays armed");
        mocks.timers[1]?.callback();
        assert.ok(
            mocks.logs.some((l) => l.includes("stage=release") && l.includes("outcome=arrival-timeout") && l.includes(`correlation=${correlation}`)),
            mocks.logs.join("\n"),
        );
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assertRedacted(mocks);
    });

    it("settles write-failed with no follow when the mover membership write fails", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.desktopsImpl = () => false;
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.ok(mocks.logs.some((l) => l.includes("outcome=write-failed") && l.includes(`correlation=${correlation}`)), mocks.logs.join("\n"));
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isInFlight, false);
        assertRedacted(mocks);
    });

    it("settles a rejected reply with no writes and one settlement", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(rejectedReply(correlation, "target-mismatch"));
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.desktops.length, 0);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=target-mismatch")), mocks.logs.join("\n"));
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assertRedacted(mocks);
    });

    it("settles mover-closed when the mover leaves both scopes after the writes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        // The mover is closed exactly at the membership write: the write
        // reports success but the next observation finds the mover in
        // neither scope. The pre-write fence still sees the dispatch scope.
        mocks.desktopsImpl = () => {
            const at = mocks.world.windows.findIndex((entry) => entry.id === "win-a");
            if (at >= 0) {
                mocks.world.windows.splice(at, 1);
            }
            return true;
        };
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=mover-closed")), mocks.logs.join("\n"));
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assertRedacted(mocks);
    });

    it("does not switch when the switch fails and never focuses afterwards", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.switchImpl = () => false;
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.deepEqual(mocks.switches, [refs.desktop]);
        assert.deepEqual(mocks.focuses, [], "a failed switch never focuses");
        assert.ok(
            mocks.logs.some((l) => l.includes("stage=follow") && l.includes("outcome=switch-unconfirmed") && l.includes(`correlation=${correlation}`)),
            mocks.logs.join("\n"),
        );
        assert.equal(mocks.settled, 1, "arrival still settles so the entry refreshes");
        assertRedacted(mocks);
    });

    it("aborts remaining geometry writes when scope goes stale mid-write", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let calls = 0;
        const baseGeometry = mocks.geometryImpl;
        void baseGeometry;
        mocks.geometryImpl = (target, r) => {
            calls += 1;
            for (const entry of mocks.world.windows) {
                if (entry.ref === target) {
                    entry.rect = { x: r.x, y: r.y, w: r.w, h: r.h };
                }
            }
            // Scope goes stale exactly at the first native setter: the next
            // per-setter fence must abort before the mover membership write.
            if (calls === 1) {
                mocks.world.targetExists = false;
            }
            return true;
        };
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(mocks.geometries.length, 1, "second geometry setter never runs");
        assert.equal(mocks.desktops.length, 0, "stale scope never reaches the mover write");
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=stale-revision") && l.includes(`correlation=${correlation}`)), mocks.logs.join("\n"));
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isInFlight, false);
        assertRedacted(mocks);
    });

    it("aborts remaining geometry writes when survivor overlay flags flip mid-write", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let flipped = false;
        mocks.observeImpl = () => {
            const live = worldObserved(mocks.world, refs);
            if (!flipped) {
                return live;
            }
            return {
                ...live,
                sourceWindows: Object.freeze(
                    live.sourceWindows.map((entry) =>
                        entry.id === "win-b" ? Object.freeze({ ...entry, fullscreen: true }) : entry,
                    ),
                ),
            };
        };
        let calls = 0;
        mocks.geometryImpl = (target, r) => {
            calls += 1;
            for (const entry of mocks.world.windows) {
                if (entry.ref === target) {
                    entry.rect = { x: r.x, y: r.y, w: r.w, h: r.h };
                }
            }
            // Flags flip exactly at the first native setter: the next
            // per-setter flag fence must abort before the mover write,
            // without comparing rects changed by the first setter.
            if (calls === 1) {
                flipped = true;
            }
            return true;
        };
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(mocks.geometries.length, 1, "second geometry setter never runs after the flag flip");
        assert.equal(mocks.desktops.length, 0, "flag drift never reaches the mover write");
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.ok(mocks.logs.some((l) => l.includes("outcome=stale-revision") && l.includes(`correlation=${correlation}`)), mocks.logs.join("\n"));
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isInFlight, false);
        assertRedacted(mocks);
    });

    it("refuses focus when the mover disappears between switch and focus", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.switchImpl = (desktopRef) => {
            // The mover closes (or moves elsewhere) during the switch: the
            // pre-focus proof must refuse focus without a setter.
            const at = mocks.world.windows.findIndex((entry) => entry.id === "win-a");
            if (at >= 0) {
                mocks.world.windows.splice(at, 1);
            }
            void desktopRef;
            return true;
        };
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.deepEqual(mocks.switches, [refs.desktop]);
        assert.deepEqual(mocks.focuses, [], "disappearing mover never focuses");
        assert.ok(
            mocks.logs.some((l) => l.includes("stage=follow") && l.includes("outcome=arrival-unconfirmed") && l.includes(`correlation=${correlation}`)),
            mocks.logs.join("\n"),
        );
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isInFlight, false);
        assertRedacted(mocks);
    });

    it("keeps delayed arrival observable when the mover signals reentrantly during writes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let handlersAtFirstGeometry = -1;
        mocks.geometryImpl = (target, r) => {
            if (handlersAtFirstGeometry < 0) {
                handlersAtFirstGeometry = mocks.arrivalHandlers.length;
            }
            for (const entry of mocks.world.windows) {
                if (entry.ref === target) {
                    entry.rect = { x: r.x, y: r.y, w: r.w, h: r.h };
                }
            }
            return true;
        };
        mocks.desktopsImpl = (target, refsArg) => {
            for (const entry of mocks.world.windows) {
                if (entry.ref === target) {
                    entry.workspace = "ws-2";
                }
            }
            void refsArg;
            // Host signals reentrantly from inside the membership setter while
            // the write stack is live. The one-shot must stay armed so the
            // post-write observation still follows exactly once.
            for (const handler of [...mocks.arrivalHandlers]) {
                handler();
            }
            return true;
        };
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(handlersAtFirstGeometry, 1, "arrival signal armed before native writes");
        assert.deepEqual(mocks.switches, [refs.desktop]);
        assert.deepEqual(mocks.focuses, [refs.a]);
        assert.deepEqual(mocks.order, ["switch", "focus"]);
        assert.equal(mocks.settled, 1);
        assert.equal(adapter.isInFlight, false);
        // A duplicate delayed signal after settlement never follows again.
        for (const handler of [...mocks.arrivalHandlers]) {
            handler();
        }
        assert.deepEqual(mocks.switches, [refs.desktop]);
        assert.deepEqual(mocks.focuses, [refs.a]);
        assert.equal(mocks.settled, 1);
        assert.ok(mocks.logs.some((l) => l.includes("event=arrival") && l.includes("outcome=arrived")), mocks.logs.join("\n"));
        assertRedacted(mocks);
        void correlation;
    });

    it("settles once on disable with a live flight and stays silent when idle", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.holdArrival = true;
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        adapter.disable();
        assert.ok(
            mocks.logs.some((l) => l.includes("stage=release") && l.includes("outcome=disabled") && l.includes(`correlation=${correlation}`)),
            mocks.logs.join("\n"),
        );
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isEnabled, false);
        adapter.disable();
        assert.equal(mocks.settled, 1, "second disable is silent");
    });
});

describe("cosmic send-to-workspace refusal routes", () => {
    function refusalOutcome(world: World): string {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        (mocks as { world: World }).world.windows.length = 0;
        for (const entry of world.windows) {
            (mocks as { world: World }).world.windows.push(entry);
        }
        mocks.world.activeRef = world.activeRef;
        mocks.world.desktopCount = world.desktopCount;
        mocks.world.targetExists = world.targetExists;
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        assert.equal(adapter.requestSend("ws-2"), false);
        assert.equal(adapter.isEnabled, true, "pre-flight refusal must stay enabled");
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.dbusCalls.length, 0, "pre-flight refusal must not touch D-Bus");
        assert.equal(mocks.timers.length, 0, "pre-flight refusal must not arm a timer");
        assert.equal(mocks.settled, 0, "pre-flight refusal carries no settlement hook");
        assert.deepEqual(adapter.pendingWorkspaces, []);
        const line = mocks.logs[mocks.logs.length - 1] ?? "";
        assert.ok(line.includes("event=refuse"), line);
        return (line.split("outcome=")[1] ?? "").split(" ")[0] ?? "";
    }

    it("refuses last-desktop when no distinct target can exist", () => {
        const refs = makeRefs();
        const world = defaultWorld(refs);
        world.desktopCount = 1;
        assert.equal(refusalOutcome(world), "last-desktop");
    });

    it("refuses same-workspace sends", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const live = worldObserved(mocks.world, refs);
        mocks.observeImpl = () => ({ ...live, targetWorkspace: "ws-1", targetExists: true });
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        assert.equal(adapter.requestSend("ws-1"), false);
        assert.equal(adapter.isEnabled, true, "pre-flight refusal must stay enabled");
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled, 0, "pre-flight refusal carries no settlement hook");
        const line = mocks.logs[mocks.logs.length - 1] ?? "";
        assert.ok(line.includes("event=refuse"), line);
        assert.equal((line.split("outcome=")[1] ?? "").split(" ")[0] ?? "", "same-workspace");
    });

    it("refuses an absent focused window", () => {
        const refs = makeRefs();
        const world = defaultWorld(refs);
        world.activeRef = null;
        assert.equal(refusalOutcome(world), "absent-focus");
    });

    it("refuses desktop-cap when more than 25 desktops are observed", () => {
        const refs = makeRefs();
        const world = defaultWorld(refs);
        world.desktopCount = 26;
        assert.equal(refusalOutcome(world), "desktop-cap");
    });
});

describe("cosmic send-to-workspace settlement domains", () => {
    const EXPECTED_SETTLED = {
        sourceOutput: "out-1",
        sourceWorkspace: "ws-1",
        targetOutput: "out-1",
        targetWorkspace: "ws-2",
    };

    it("carries exact source/target keys on arrival", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(mocks.settled, 1);
        assert.deepEqual(mocks.settledInfos, [EXPECTED_SETTLED]);
        assertRedacted(mocks);
    });

    it("carries exact source/target keys on the missing-callback release", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        dispatch(mocks, adapter);
        mocks.timers[0]?.callback();
        assert.equal(mocks.settled, 1);
        assert.deepEqual(mocks.settledInfos, [EXPECTED_SETTLED]);
        assertRedacted(mocks);
    });

    it("carries exact source/target keys on disable with a live flight", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        mocks.holdArrival = true;
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(adapter.isInFlight, true);
        adapter.disable();
        assert.equal(mocks.settled, 1);
        assert.deepEqual(mocks.settledInfos, [EXPECTED_SETTLED]);
        assertRedacted(mocks);
    });

    it("allows a rapid repeat send immediately after settlement", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const first = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(first));
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled, 1);
        // No queue or retained block: focus the surviving source member and
        // a repeat send dispatches at once with a distinct correlation. The
        // shared dispatch helper assumes fresh mocks, so drive the second
        // flight from the current call offsets.
        mocks.world.activeRef = refs.b;
        const ownerAt = mocks.dbusCalls.length;
        const timerAt = mocks.timers.length;
        assert.equal(adapter.requestSend("ws-2"), true);
        assert.deepEqual(adapter.pendingWorkspaces, ["ws-1", "ws-2"]);
        assert.equal(mocks.dbusCalls[ownerAt]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
        mocks.callbacks[ownerAt]?.(":1.7");
        const requestCall = mocks.dbusCalls[ownerAt + 1];
        assert.equal(requestCall?.method, WORKSPACE_SEND_METHOD);
        const second = parsePayload(requestCall?.payload ?? "{}")["correlation_id"] as string;
        assert.ok(second.length > 0);
        assert.notEqual(second, first);
        assert.equal(adapter.isInFlight, true);
        assert.equal(mocks.settled, 1);
        assert.equal(mocks.timers.length, timerAt + 2);
        assertRedacted(mocks);
    });

    it("rotates the correlation namespace after 1M sends without permanent refusal", () => {        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        (adapter as unknown as { seq: number }).seq = 1000000;
        const first = dispatch(mocks, adapter);
        assert.equal(first, "gen-1-w1000000");
        mocks.callbacks[1]?.(rejectedReply(first, "stale-revision"));
        assert.equal(adapter.isInFlight, false);
        assert.equal(adapter.isEnabled, true);
        mocks.world.activeRef = refs.b;
        const ownerAt = mocks.dbusCalls.length;
        assert.equal(adapter.requestSend("ws-2"), true);
        assert.equal(mocks.dbusCalls[ownerAt]?.method, WORKSPACE_SEND_GET_OWNER_METHOD);
        mocks.callbacks[ownerAt]?.(":1.7");
        const second = parsePayload(mocks.dbusCalls[ownerAt + 1]?.payload ?? "{}")["correlation_id"] as string;
        assert.equal(second, "gen-1-w1r0");
        assert.notEqual(second, first);
        assert.ok(second.length > 0 && second.length <= 128);
        mocks.callbacks[1]?.(rejectedReply(first, "stale-revision"));
        assert.equal(adapter.isInFlight, true, "a delayed old-epoch reply cannot settle the new flight");
        assert.ok(
            mocks.logs.some((l) => l.includes("event=sequence-exhausted") && l.includes(`correlation=${second}`) && l.includes("outcome=correlation-rotated")),
            mocks.logs.join("\n"),
        );
        assertRedacted(mocks);
    });

    it("mid-flight floating toggle issues zero geometry writes and settles normally", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let tiled = true;
        const adapter = new WorkspaceSendAdapter({ ...mocks.env, isDomainTiled: () => tiled });
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        // Either flight domain toggles floating between dispatch and reply.
        tiled = false;
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.desktops.length, 0);
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.equal(mocks.settled, 1);
        assert.deepEqual(adapter.pendingWorkspaces, []);
        assert.equal(adapter.isInFlight, false);
        assert.equal(adapter.isEnabled, true);
        assert.ok(
            mocks.logs.some((l) => l.includes("stage=release") && l.includes("outcome=workspace-floating")),
            mocks.logs.join("\n"),
        );
        assert.deepEqual(mocks.settledInfos[0], {
            sourceOutput: "out-1",
            sourceWorkspace: "ws-1",
            targetOutput: "out-1",
            targetWorkspace: "ws-2",
        });
        assertRedacted(mocks);
    });

    it("mid-write floating toggle stops further geometry writes", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        let writes = 0;
        let tiled = true;
        const adapter = new WorkspaceSendAdapter({
            ...mocks.env,
            isDomainTiled: () => tiled,
            setGeometry: (target, rect) => {
                writes += 1;
                if (writes === 1) {
                    tiled = false;
                }
                return mocks.env.setGeometry(target, rect);
            },
        });
        adapter.enable({ owner: "owner-1", generation: "gen-1" });
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        // The first write applied before the toggle; the gate stops the rest
        // and the flight settles without a mover write or follow.
        assert.equal(writes, 1);
        assert.equal(mocks.desktops.length, 0);
        assert.deepEqual(mocks.switches, []);
        assert.deepEqual(mocks.focuses, []);
        assert.equal(mocks.settled, 1);
        assert.equal(adapter.isInFlight, false);
        assert.ok(
            mocks.logs.some((l) => l.includes("stage=release") && l.includes("outcome=workspace-floating")),
            mocks.logs.join("\n"),
        );
        assertRedacted(mocks);
    });
});

describe("send route labeling (workspace evidence stays send-to-workspace)", () => {
    it("labels the workspace lifecycle dispatch/plan/release as send-to-workspace", () => {
        const refs = makeRefs();
        const mocks = mockEnv(refs);
        const adapter = new WorkspaceSendAdapter(mocks.env);
        assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
        const correlation = dispatch(mocks, adapter);
        mocks.callbacks[1]?.(plannedReply(correlation));
        assert.equal(adapter.isInFlight, false);
        const routed = mocks.logs.filter(
            (line) => line.includes("route=") && line.includes(`correlation=${correlation}`),
        );
        assert.ok(routed.length > 0, mocks.logs.join("\n"));
        for (const line of routed) {
            assert.ok(line.includes("route=send-to-workspace"), line);
            assert.ok(!line.includes("route=send-to-output"), line);
        }
        assert.ok(
            routed.some((line) => line.includes("event=dispatch") && line.includes("route=send-to-workspace")),
            mocks.logs.join("\n"),
        );
        assert.ok(
            routed.some((line) => line.includes("stage=release") && line.includes("route=send-to-workspace")),
            mocks.logs.join("\n"),
        );
    });
});
