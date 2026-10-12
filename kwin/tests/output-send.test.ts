import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    WORKSPACE_SEND_HAS_OWNER_METHOD,
    WorkspaceSendAdapter,
    WorkspaceSendAdapterEnv,
    WorkspaceSendObserved,
    WorkspaceSendObservedWindow,
} from "../src/workspace-send-adapter";
import {
    observeOutputSendTarget,
    planOutputSendShortcutCatalog,
    startPlanAdapterEntry,
} from "../src/plan-adapter-entry";

// Item 5 (REQ-MOV-08/OUT-04) KDE regression coverage: explicit output send
// (send-to-output) admission follow/stay, exact frozen target, arrival and
// visibility fences, sticky/float refusal, and floating boundaries with
// tiled-side reflow. Directional four-direction adjacency coverage lives in
// plan-directional.test.ts; this file covers the send route.

// ---------- adapter harness ----------

interface FakeWin {
    readonly id: string;
    readonly ref: object;
    output: string;
    desktops: string[];
    rect: { x: number; y: number; w: number; h: number };
    floating: boolean;
    sticky: boolean;
    fullscreen: boolean;
    maximized: boolean;
}

interface AdapterWorld {
    outputs: string[];
    currentByOutput: Map<string, string>;
    desktops: string[];
    wins: FakeWin[];
    activeId: string | null;
    targetDesktopRefs: Map<string, object>;
}

function makeAdapterWorld(): AdapterWorld {
    return {
        outputs: ["out-1", "out-2"],
        currentByOutput: new Map([
            ["out-1", "ws-a"],
            ["out-2", "ws-b"],
        ]),
        desktops: ["ws-a", "ws-b"],
        wins: [
            { id: "win-1", ref: {}, output: "out-1", desktops: ["ws-a"], rect: { x: 0, y: 0, w: 100, h: 80 }, floating: false, sticky: false, fullscreen: false, maximized: false },
            { id: "win-2", ref: {}, output: "out-1", desktops: ["ws-a"], rect: { x: 100, y: 0, w: 100, h: 80 }, floating: false, sticky: false, fullscreen: false, maximized: false },
            { id: "win-t", ref: {}, output: "out-2", desktops: ["ws-b"], rect: { x: 0, y: 0, w: 100, h: 80 }, floating: false, sticky: false, fullscreen: false, maximized: false },
        ],
        activeId: "win-2",
        targetDesktopRefs: new Map([
            ["ws-a", {}],
            ["ws-b", {}],
        ]),
    };
}

function toObservedWindow(win: FakeWin): WorkspaceSendObservedWindow {
    return {
        id: win.id,
        ref: win.ref,
        rect: { x: win.rect.x, y: win.rect.y, w: win.rect.w, h: win.rect.h },
        floating: win.floating,
        fitExcluded: win.floating || win.sticky || win.fullscreen || win.maximized,
        fit_excluded: win.floating || win.sticky || win.fullscreen || win.maximized,
        fullscreen: win.fullscreen,
        maximized: win.maximized,
        sticky: win.sticky,
    };
}

interface AdapterMocks {
    env: WorkspaceSendAdapterEnv;
    dbusCalls: Array<{ method: string; payload: string }>;
    callbacks: Array<(reply: unknown) => void>;
    logs: string[];
    events: string[];
    geometries: Array<{ target: object; rect: unknown }>;
    memberships: Array<{ mover: object; refs: ReadonlyArray<object> }>;
    transfers: Array<{ mover: object; output: object }>;
    switches: object[];
    focuses: object[];
    desktopsHandlers: Array<() => void>;
    outputHandlers: Array<(old: unknown) => void>;
    settled: Array<{ sourceOutput: string; targetOutput: string }>;
    world: AdapterWorld;
    tiled: boolean;
}

function mockAdapter(world: AdapterWorld): AdapterMocks {
    const state = {
        dbusCalls: [],
        callbacks: [],
        logs: [],
        events: [],
        geometries: [],
        memberships: [],
        transfers: [],
        switches: [],
        focuses: [],
        desktopsHandlers: [],
        outputHandlers: [],
        settled: [],
        tiled: true,
    } as unknown as AdapterMocks;
    const observeOutput = (
        targetOutput: string,
        targetWorkspace: string,
        pinnedSourceWorkspace?: string,
        pinnedSourceOutput?: string,
    ): WorkspaceSendObserved | null => {
        // Defense-in-depth probe: a same-output request resolves through a
        // same-output observation so the adapter's same-output refusal fires.
        if (targetOutput === "out-1") {
            const wins = world.wins.filter((win) => win.output === "out-1" && win.desktops.includes("ws-a"));
            const mover = wins.find((win) => win.id === world.activeId) ?? null;
            // Mirrors the production observer gate: maximized/fullscreen
            // movers carry (G-D2/D28), floating/sticky and otherwise
            // unexplained fit exclusions refuse.
            const fitExcluded =
                mover !== null && (mover.floating || mover.sticky || mover.fullscreen || mover.maximized);
            const tiled =
                mover !== null &&
                !mover.floating &&
                !mover.sticky &&
                (!fitExcluded || mover.maximized || mover.fullscreen);
            return {
                sourceOutput: "out-1",
                sourceWorkspace: "ws-a",
                sourceBounds: { x: 0, y: 0, w: 800, h: 600 },
                targetOutput: "out-1",
                targetWorkspace: "ws-a",
                targetBounds: { x: 0, y: 0, w: 800, h: 600 },
                focusedId: tiled && mover !== null ? mover.id : "",
                sourceWindows: Object.freeze(wins.map(toObservedWindow)),
                targetWindows: Object.freeze([]),
                activeRef: mover !== null ? mover.ref : null,
                moverRef: tiled && mover !== null ? mover.ref : null,
                targetDesktopRef: world.targetDesktopRefs.get("ws-a") ?? null,
                targetExists: true,
                desktopCount: world.desktops.length,
                sourceFingerprint: "src-same",
                targetFingerprint: "tgt-same",
                currentWorkspace: "ws-a",
            };
        }
        if (targetOutput !== "out-2" || targetWorkspace !== world.currentByOutput.get("out-2")) {
            return null;
        }
        // Production-like source derivation: unpinned (dispatch probe) the
        // source follows the live active window; pinned (flight
        // re-observation) it is the frozen dispatch pair. Deriving from the
        // active window post-transfer would collapse target===source and
        // return null here (the production integration hazard).
        const active = world.wins.find((win) => win.id === world.activeId) ?? null;
        const sourceOutput = pinnedSourceOutput ?? active?.output ?? "out-1";
        const sourceWorkspace = pinnedSourceWorkspace ?? world.currentByOutput.get(sourceOutput) ?? "ws-a";
        if (sourceOutput === targetOutput) {
            return null;
        }
        // Live source visibility is reported separately from the frozen
        // scope, so the stay fence still sees view switches.
        const liveSourceCurrent = world.currentByOutput.get(sourceOutput) ?? sourceWorkspace;
        const sourceWindows = world.wins
            .filter((win) => win.output === sourceOutput && (win.desktops.includes(sourceWorkspace) || win.sticky))
            .map(toObservedWindow);
        const targetWindows = world.wins
            .filter((win) => win.output === targetOutput && (win.desktops.includes(targetWorkspace) || win.sticky))
            .map(toObservedWindow);
        const focused = active;
        // Mirrors the production observer gate (see the same-output probe
        // above): maximized/fullscreen movers carry, everything else
        // exceptional refuses.
        const focusedFitExcluded =
            focused !== null &&
            (focused.floating || focused.sticky || focused.fullscreen || focused.maximized);
        const moverTiled =
            focused !== null &&
            focused.output === sourceOutput &&
            focused.desktops.includes(sourceWorkspace) &&
            !focused.floating &&
            !focused.sticky &&
            (!focusedFitExcluded || focused.maximized || focused.fullscreen);
        const sourceIds = sourceWindows.map((entry) => entry.id).sort().join(",");
        const targetIds = targetWindows.map((entry) => entry.id).sort().join(",");
        return {
            sourceOutput,
            sourceWorkspace,
            sourceBounds: { x: 0, y: 0, w: 800, h: 600 },
            targetOutput,
            targetWorkspace,
            targetBounds: { x: 800, y: 0, w: 800, h: 600 },
            focusedId: moverTiled && focused !== null ? focused.id : "",
            sourceWindows: Object.freeze(sourceWindows),
            targetWindows: Object.freeze(targetWindows),
            activeRef: focused !== null ? focused.ref : null,
            moverRef: moverTiled && focused !== null ? focused.ref : null,
            targetDesktopRef: world.targetDesktopRefs.get(targetWorkspace) ?? null,
            targetExists: true,
            desktopCount: world.desktops.length,
            sourceFingerprint: `src-${sourceIds}`,
            targetFingerprint: `tgt-${targetIds}`,
            currentWorkspace: liveSourceCurrent,
        };
    };
    const env: WorkspaceSendAdapterEnv = {
        callDbus: (_service, _path, _iface, method, payload, callback): void => {
            if (method === WORKSPACE_SEND_HAS_OWNER_METHOD) {
                callback(true);
                return;
            }
            if (method === "GetNameOwner") {
                callback(":1.7");
                return;
            }
            if (method === "StartServiceByName") {
                callback(1);
                return;
            }
            state.dbusCalls.push({ method, payload });
            state.callbacks.push(callback);
        },
        scheduleOnce: (): (() => void) => (): void => {},
        log: (message): void => {
            state.logs.push(message);
        },
        observe: (): WorkspaceSendObserved | null => null,
        observeOutput,
        onSettled: (settled): void => {
            state.settled.push(settled);
        },
        setGeometry: (target, rect): boolean => {
            state.events.push("geometry");
            state.geometries.push({ target, rect });
            return true;
        },
        setDesktops: (target, refs): boolean => {
            state.events.push("membership");
            state.memberships.push({ mover: target, refs });
            const mover = world.wins.find((win) => win.ref === target) ?? null;
            if (mover === null) {
                return false;
            }
            mover.desktops = refs.map((ref) => {
                for (const [id, candidate] of world.targetDesktopRefs) {
                    if (candidate === ref) {
                        return id;
                    }
                }
                return "?";
            });
            // Synchronous delivery defers inside the adapter write stack
            // (nativeWriteDepth); the adapter consumes on real progress, so
            // the harness must not clear here.
            for (const handler of [...state.desktopsHandlers]) {
                handler();
            }
            return true;
        },
        isDomainTiled: (): boolean => state.tiled,
        switchToTarget: (desktopRef): boolean => {
            state.events.push("switch");
            state.switches.push(desktopRef);
            return true;
        },
        focusWindow: (windowRef): boolean => {
            state.events.push("focus");
            state.focuses.push(windowRef);
            const mover = world.wins.find((win) => win.ref === windowRef) ?? null;
            if (mover !== null) {
                world.activeId = mover.id;
            }
            return true;
        },
        subscribeMoverDesktops: (_moverRef, handler): (() => void) | null => {
            state.desktopsHandlers.push(handler);
            return (): void => {};
        },
        resolveOutput: (name): object | null => (name === "out-1" || name === "out-2" ? { name } : null),
        sendClientToScreen: (target, output): boolean => {
            state.events.push("transfer");
            state.transfers.push({ mover: target, output });
            const mover = world.wins.find((win) => win.ref === target) ?? null;
            const name = (output as { name: string }).name;
            if (mover === null || (name !== "out-1" && name !== "out-2")) {
                return false;
            }
            mover.output = name;
            for (const handler of [...state.outputHandlers]) {
                handler({ name: "out-1" });
            }
            return true;
        },
        readOutputName: (ref): string | null => {
            const mover = world.wins.find((win) => win.ref === ref) ?? null;
            return mover === null ? null : mover.output;
        },
        readDesktopIds: (ref): ReadonlyArray<string> | null => {
            const mover = world.wins.find((win) => win.ref === ref) ?? null;
            return mover === null ? null : [...mover.desktops];
        },
        readMoverLive: (moverRef): {
            id: string;
            floating: boolean;
            sticky: boolean;
            fullscreen: boolean;
            maximized: boolean;
        } | null => {
            // Exact retained-ref identity against the live window list:
            // removed or replaced movers resolve to null here even when a
            // dead wrapper would still answer property reads.
            const mover = world.wins.find((win) => win.ref === moverRef) ?? null;
            if (mover === null) {
                return null;
            }
            return {
                id: mover.id,
                floating: mover.floating,
                sticky: mover.sticky,
                fullscreen: mover.fullscreen,
                maximized: mover.maximized,
            };
        },
        subscribeMoverOutput: (_moverRef, handler): (() => void) | null => {
            state.outputHandlers.push(handler);
            return (): void => {};
        },
    };
    (state as { env: WorkspaceSendAdapterEnv }).env = env;
    (state as { world: AdapterWorld }).world = world;
    return state;
}

function enableAdapter(mocks: AdapterMocks): WorkspaceSendAdapter {
    const adapter = new WorkspaceSendAdapter(mocks.env);
    assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
    return adapter;
}

function sendPayload(mocks: AdapterMocks, index: number): Record<string, unknown> {
    return JSON.parse(mocks.dbusCalls[index]?.payload as string) as Record<string, unknown>;
}

function plannedOutputReply(
    correlation: string,
    opts: {
        follow: boolean;
        focusLeaf: string | null;
        focusOutput: string;
        focusWorkspace: string;
        moverLeaf?: string;
        targetWorkspace?: string;
    },
): string {
    const moverLeaf = opts.moverLeaf ?? "leaf-win-2";
    const targetWorkspace = opts.targetWorkspace ?? "ws-b";
    return JSON.stringify({
        v: 1,
        correlation_id: correlation,
        outcome: "planned",
        kind: "send-to-output",
        base_revision: 1,
        detail: { kind: "send-to-output", policy_version: 1, capability: "move-tiled" },
        desired_geometry: [
            { window: "win-1", leaf: "leaf-win-1", output: "out-1", workspace: "ws-a", rect: { x: 0, y: 0, w: 800, h: 600 } },
            { window: "win-2", leaf: moverLeaf, output: "out-2", workspace: targetWorkspace, rect: { x: 800, y: 0, w: 400, h: 600 } },
            { window: "win-t", leaf: "leaf-win-t", output: "out-2", workspace: targetWorkspace, rect: { x: 1200, y: 0, w: 400, h: 600 } },
        ],
        desired_focus:
            opts.focusLeaf === null
                ? null
                : { domain_output: opts.focusOutput, domain_workspace: opts.focusWorkspace, leaf: opts.focusLeaf },
        preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
        operation: {
            op: "move-tiled",
            window: "win-2",
            leaf: moverLeaf,
            source_output: "out-1",
            source_workspace: "ws-a",
            target_output: "out-2",
            target_workspace: targetWorkspace,
            follow: opts.follow,
        },
    });
}

describe("output-send adapter (explicit send-to-output)", () => {
    it("follows with transfer-first order and no desktop switch", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        assert.equal(mocks.dbusCalls.length, 1);
        const body = sendPayload(mocks, 0);
        assert.equal((body["command"] as Record<string, unknown>)["op"], "send-to-output");
        assert.equal((body["command"] as Record<string, unknown>)["follow"], true);
        assert.equal((body["target_domain"] as Record<string, unknown>)["output"], "out-2");
        assert.equal((body["target_domain"] as Record<string, unknown>)["workspace"], "ws-b");
        assert.equal((body["target_windows"] as Array<unknown>).length, 1);
        const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        // R4 order: transfer, membership, geometries; then mover focus only.
        const transferAt = mocks.events.indexOf("transfer");
        const membershipAt = mocks.events.indexOf("membership");
        const firstGeometryAt = mocks.events.indexOf("geometry");
        const focusAt = mocks.events.indexOf("focus");
        assert.ok(transferAt >= 0 && membershipAt > transferAt, JSON.stringify(mocks.events));
        assert.ok(firstGeometryAt > membershipAt, JSON.stringify(mocks.events));
        assert.ok(focusAt > firstGeometryAt, JSON.stringify(mocks.events));
        assert.equal(mocks.transfers.length, 1);
        assert.equal(mover.output, "out-2");
        assert.deepEqual(mover.desktops, ["ws-b"]);
        assert.equal(mocks.switches.length, 0, "cross-output follow never switches desktops");
        assert.equal(mocks.focuses.length, 1);
        assert.equal(mocks.focuses[0], mover.ref);
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled.length, 1);
        assert.deepEqual(
            [mocks.settled[0]?.sourceOutput, mocks.settled[0]?.targetOutput],
            ["out-1", "out-2"],
        );
    });

    it("stays on the source MRU survivor with no switch", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", false), true);
        const body = sendPayload(mocks, 0);
        assert.equal((body["command"] as Record<string, unknown>)["follow"], false);
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: false,
                focusLeaf: "leaf-win-1",
                focusOutput: "out-1",
                focusWorkspace: "ws-a",
            }),
        );
        const survivor = world.wins.find((win) => win.id === "win-1") as FakeWin;
        assert.equal(mocks.switches.length, 0);
        assert.equal(mocks.focuses.length, 1);
        assert.equal(mocks.focuses[0], survivor.ref);
        assert.equal(adapter.isInFlight, false);
    });

    it("carries a maximized mover cross-output with follow and no overlay geometry write", () => {
        // G-D2 (REQ-MAX-09): the maximized mover transfers with its native
        // state untouched (no unmaximize/remaximize, no geometry write to
        // the overlay) and arrives maximized; numbered ordinals stay
        // diagnostic-only.
        const world = makeAdapterWorld();
        const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
        mover.maximized = true;
        const moverBefore = { ...mover.rect };
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true, 2), true);
        const body = sendPayload(mocks, 0);
        assert.equal((body["command"] as Record<string, unknown>)["follow"], true);
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.equal(mocks.transfers.length, 1, "output transfer runs once");
        assert.equal(mover.output, "out-2");
        assert.deepEqual(mover.desktops, ["ws-b"], "mover membership lands on the target");
        assert.equal(mover.maximized, true, "no gratuitous unmaximize/remaximize");
        assert.deepEqual(mover.rect, moverBefore, "arrival keeps the native frame: no overlay geometry write");
        assert.ok(
            !mocks.geometries.some((entry) => entry.target === mover.ref),
            "geometry writes skip the overlaid mover",
        );
        assert.equal(mocks.switches.length, 0, "cross-output follow never switches desktops");
        assert.equal(mocks.focuses.length, 1);
        assert.equal(mocks.focuses[0], mover.ref, "follow focuses the carried mover");
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled.length, 1);
    });

    it("fails a mid-flight maximize change with no membership write", () => {
        // Flag-stability fence: a mover maximized after dispatch (snapshot
        // ordinary) fails closed at the reply-boundary scope fence before
        // any setter.
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
        mover.maximized = true;
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.deepEqual(
            mover.desktops,
            ["ws-a"],
            "unstable mover never receives the membership write",
        );
        assert.deepEqual(mocks.transfers, [], "no output transfer after the flag change");
        assert.deepEqual(mocks.focuses, [], "no follow after a failed write");
        assert.ok(
            mocks.logs.some((line) => line.includes("outcome=stale-revision")),
            "scope fence settles terminal without follow",
        );
        assert.equal(adapter.isInFlight, false);
    });

    it("refuses the membership write when maximize flips mid-transfer", () => {
        // Live-mover stability fence (`crossOutputMoverLive`): a maximize
        // change landing between the output transfer and the membership
        // write refuses the membership with no geometry and no follow.
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
        const rawTransfer = mocks.env.sendClientToScreen?.bind(mocks.env);
        assert.ok(rawTransfer !== undefined, "transfer hook present");
        (mocks.env as unknown as { sendClientToScreen: unknown }).sendClientToScreen = (
            target: object,
            output: object,
        ): boolean => {
            const applied = rawTransfer === undefined ? false : (rawTransfer(target, output) ?? false);
            mover.maximized = true;
            return applied;
        };
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.deepEqual(mover.desktops, ["ws-a"], "unstable mover keeps source membership");
        assert.deepEqual(mocks.geometries, [], "no geometry after the refused membership");
        assert.deepEqual(mocks.focuses, [], "no follow after a failed write");
        assert.ok(
            mocks.logs.some((line) => line.includes("outcome=stale-revision")),
            "scope fence settles terminal without follow",
        );
        assert.equal(adapter.isInFlight, false);
    });

    it("carries a fullscreen mover cross-output with follow and no overlay geometry write", () => {
        // D28 (REQ-MAX-09 fullscreen leg): the fullscreen mover transfers
        // with its native state untouched (no drop, no geometry write to
        // the overlay) and arrives fullscreen; follow focuses the mover.
        const world = makeAdapterWorld();
        const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
        mover.fullscreen = true;
        const moverBefore = { ...mover.rect };
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true, 2), true);
        const body = sendPayload(mocks, 0);
        assert.equal((body["command"] as Record<string, unknown>)["follow"], true);
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.equal(mocks.transfers.length, 1, "output transfer runs once");
        assert.equal(mover.output, "out-2");
        assert.deepEqual(mover.desktops, ["ws-b"], "mover membership lands on the target");
        assert.equal(mover.fullscreen, true, "no gratuitous fullscreen drop");
        assert.deepEqual(mover.rect, moverBefore, "arrival keeps the native frame: no overlay geometry write");
        assert.ok(
            !mocks.geometries.some((entry) => entry.target === mover.ref),
            "geometry writes skip the overlaid mover",
        );
        assert.equal(mocks.switches.length, 0, "cross-output follow never switches desktops");
        assert.equal(mocks.focuses.length, 1);
        assert.equal(mocks.focuses[0], mover.ref, "follow focuses the carried mover");
        assert.equal(adapter.isInFlight, false);
    });

    it("fails a mid-flight fullscreen change with no membership write", () => {
        // Flag-stability fence: a mover un-fullscreened after dispatch
        // (snapshot fullscreen) fails closed at the reply-boundary scope
        // fence before any setter.
        const world = makeAdapterWorld();
        const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
        mover.fullscreen = true;
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        mover.fullscreen = false;
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.deepEqual(
            mover.desktops,
            ["ws-a"],
            "unstable mover never receives the membership write",
        );
        assert.deepEqual(mocks.transfers, [], "no output transfer after the flag change");
        assert.deepEqual(mocks.focuses, [], "no follow after a failed write");
        assert.ok(
            mocks.logs.some((line) => line.includes("outcome=stale-revision")),
            "scope fence settles terminal without follow",
        );
        assert.equal(adapter.isInFlight, false);
    });

    it("applies a null-focus stay with zero focus setters", () => {
        const world = makeAdapterWorld();
        // Emptied source: only the mover leaves.
        world.wins = world.wins.filter((win) => win.id !== "win-1");
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", false), true);
        const body = sendPayload(mocks, 0);
        mocks.callbacks[0]?.(
            JSON.stringify({
                v: 1,
                correlation_id: body["correlation_id"],
                outcome: "planned",
                kind: "send-to-output",
                base_revision: 1,
                detail: { kind: "send-to-output", policy_version: 1, capability: "move-tiled" },
                desired_geometry: [
                    { window: "win-2", leaf: "leaf-win-2", output: "out-2", workspace: "ws-b", rect: { x: 800, y: 0, w: 800, h: 600 } },
                    { window: "win-t", leaf: "leaf-win-t", output: "out-2", workspace: "ws-b", rect: { x: 800, y: 0, w: 800, h: 600 } },
                ],
                desired_focus: null,
                preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
                operation: {
                    op: "move-tiled",
                    window: "win-2",
                    leaf: "leaf-win-2",
                    source_output: "out-1",
                    source_workspace: "ws-a",
                    target_output: "out-2",
                    target_workspace: "ws-b",
                    follow: false,
                },
            }),
        );
        assert.equal(mocks.switches.length, 0);
        assert.equal(mocks.focuses.length, 0);
        assert.equal(adapter.isInFlight, false);
    });

    it("refuses a drifted destination with no dispatch", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        // The live destination workspace drifted after the entry resolved it:
        // the output hook fails closed and the dispatch refuses.
        world.currentByOutput.set("out-2", "ws-c");
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), false);
        assert.equal(mocks.dbusCalls.length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("scope-invalid")));
    });

    it("refuses a hook that resolves the wrong output as target-mismatch", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        // A misbehaving hook resolving another output than requested must
        // refuse rather than retarget.
        const env = mocks.env as unknown as {
            observeOutput: (targetOutput: string, targetWorkspace: string) => WorkspaceSendObserved | null;
        };
        const realObserve = env.observeOutput;
        env.observeOutput = (targetOutput, targetWorkspace): WorkspaceSendObserved | null => {
            const observed = realObserve(targetOutput, targetWorkspace);
            if (observed === null) {
                return null;
            }
            return { ...observed, targetOutput: "out-9" };
        };
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), false);
        assert.equal(mocks.dbusCalls.length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("target-mismatch")));
    });

    it("refuses a same-output target", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-1", "ws-a", true), false);
        assert.equal(mocks.dbusCalls.length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("same-output")));
    });

    it("refuses without transfer capabilities", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const env = mocks.env as unknown as Record<string, unknown>;
        delete env["sendClientToScreen"];
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), false);
        assert.equal(mocks.dbusCalls.length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("transfer-unavailable")));
    });

    it("refuses a sticky or intentional-float mover as non-tiled-focus", () => {
        for (const flag of ["sticky", "floating"] as const) {
            const world = makeAdapterWorld();
            const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
            if (flag === "sticky") {
                mover.sticky = true;
            } else {
                mover.floating = true;
            }
            const mocks = mockAdapter(world);
            const adapter = enableAdapter(mocks);
            assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), false);
            assert.equal(mocks.dbusCalls.length, 0);
            assert.ok(
                mocks.logs.some((line) => line.includes("non-tiled-focus")),
                `${flag}: ${JSON.stringify(mocks.logs)}`,
            );
        }
    });

    it("refuses a mismatched reply kind with zero writes", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        const correlation = body["correlation_id"] as string;
        const reply = JSON.parse(plannedOutputReply(correlation, {
            follow: true,
            focusLeaf: "leaf-win-2",
            focusOutput: "out-2",
            focusWorkspace: "ws-b",
        })) as Record<string, unknown>;
        reply["kind"] = "send-to-workspace";
        mocks.callbacks[0]?.(JSON.stringify(reply));
        assert.equal(mocks.transfers.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.memberships.length, 0);
        assert.equal(mocks.focuses.length, 0);
        assert.equal(adapter.isInFlight, false);
    });

    it("refuses a flipped follow echo with zero writes", () => {
        // A stay flight (follow=false) whose reply echoes follow=true is a
        // mismatched reply: the operation-to-snapshot binding fails before
        // any native setter.
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", false), true);
        const body = sendPayload(mocks, 0);
        assert.equal((body["command"] as Record<string, unknown>)["follow"], false);
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.equal(mocks.transfers.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.memberships.length, 0);
        assert.equal(mocks.focuses.length, 0);
        assert.equal(adapter.isInFlight, false);
        const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
        assert.equal(mover.output, "out-1");
        assert.deepEqual(mover.desktops, ["ws-a"]);
    });

    it("keeps the pinned source across transfer (no target===source collapse)", () => {
        // Production hazard regression: deriving the source from the live
        // active window collapses once the still-active mover lands on the
        // destination. The flight pin must survive transfer through arrival.
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
        // Simulate the applied end state while the mover stays active.
        mover.output = "out-2";
        mover.desktops = ["ws-b"];
        // Unpinned re-observation collapses (documents the hazard).
        const env = mocks.env as unknown as {
            observeOutput: (
                targetOutput: string,
                targetWorkspace: string,
                pinnedSourceWorkspace?: string,
                pinnedSourceOutput?: string,
            ) => WorkspaceSendObserved | null;
        };
        assert.equal(env.observeOutput("out-2", "ws-b"), null);
        // Pinned re-observation (what the flight uses) stays exact with the
        // mover on the target and no focused mover.
        const pinned = env.observeOutput("out-2", "ws-b", "ws-a", "out-1");
        assert.notEqual(pinned, null);
        assert.equal(pinned?.sourceOutput, "out-1");
        assert.equal(pinned?.sourceWorkspace, "ws-a");
        assert.equal(pinned?.targetOutput, "out-2");
        assert.equal(pinned?.focusedId, "");
        assert.ok((pinned?.targetWindows.some((entry) => entry.id === "win-2") ?? false));
        void adapter;
    });

    it("waits for the native output proof before following", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        // The native output readback lags behind the applied transfer while
        // membership applies synchronously: the scope is exact, but the
        // output proof keeps failing until the readback catches up.
        let outputLagging = true;
        const env = mocks.env as unknown as {
            readOutputName: (ref: object) => string | null;
        };
        const realReadOutput = env.readOutputName;
        env.readOutputName = (ref): string | null => {
            if (outputLagging) {
                const mover = world.wins.find((win) => win.ref === ref) ?? null;
                if (mover !== null && mover.id === "win-2") {
                    return "out-1";
                }
            }
            return realReadOutput(ref);
        };
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        // Scope exact but output unproven: no follow yet, flight waits.
        assert.equal(mocks.focuses.length, 0);
        assert.equal(adapter.isInFlight, true);
        // Delayed output readback catches up through the output fence.
        outputLagging = false;
        for (const handler of [...mocks.outputHandlers]) {
            handler({ name: "out-1" });
        }
        const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
        assert.equal(mocks.focuses.length, 1);
        assert.equal(mocks.focuses[0], mover.ref);
        assert.equal(adapter.isInFlight, false);
    });

    it("fails closed on a mid-flight floating toggle with no further writes", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        mocks.tiled = false;
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.equal(mocks.transfers.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.focuses.length, 0);
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled.length, 1);
    });

    it("fails the stay fence when the source view switches mid-flight", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", false), true);
        const body = sendPayload(mocks, 0);
        // External view switch on the source output before the reply lands.
        world.currentByOutput.set("out-1", "ws-z");
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: false,
                focusLeaf: "leaf-win-1",
                focusOutput: "out-1",
                focusWorkspace: "ws-a",
            }),
        );
        assert.equal(mocks.transfers.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.focuses.length, 0);
        assert.equal(adapter.isInFlight, false);
    });

    it("accepts a single shared desktop across distinct outputs", () => {
        // Distinct outputs may share one current workspace/desktop: no
        // last-desktop gate on the output-send route.
        const world = makeAdapterWorld();
        world.desktops = ["ws-a"];
        world.currentByOutput.set("out-1", "ws-a");
        world.currentByOutput.set("out-2", "ws-a");
        const target = world.wins.find((win) => win.id === "win-t") as FakeWin;
        target.desktops = ["ws-a"];
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-a", true), true);
        const body = sendPayload(mocks, 0);
        assert.equal((body["target_domain"] as Record<string, unknown>)["output"], "out-2");
        assert.equal((body["target_domain"] as Record<string, unknown>)["workspace"], "ws-a");
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-a",
                targetWorkspace: "ws-a",
            }),
        );
        const mover = world.wins.find((win) => win.id === "win-2") as FakeWin;
        assert.equal(mover.output, "out-2");
        assert.deepEqual(mover.desktops, ["ws-a"]);
        assert.equal(mocks.focuses.length, 1);
        assert.equal(adapter.isInFlight, false);
    });

    it("fails a follow flight when the source view switches mid-flight", () => {
        // Explicit output follow never reselects the source (no desktop
        // switch rides it), so a switched-away source refuses with zero
        // writes even for follow.
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        world.currentByOutput.set("out-1", "ws-b");
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.equal(mocks.transfers.length, 0);
        assert.equal(mocks.memberships.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.focuses.length, 0);
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled.length, 1);
    });

    it("stops before membership when the mover closes reentrantly during transfer", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        // Reentrant close during the output setter: the mover leaves the
        // live window list, so the lifetime proof fails before membership.
        mocks.outputHandlers.push(() => {
            const at = world.wins.findIndex((win) => win.id === "win-2");
            if (at >= 0) {
                world.wins.splice(at, 1);
            }
        });
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.equal(mocks.transfers.length, 1, "transfer initiated once");
        assert.equal(mocks.memberships.length, 0, "no membership for a closed mover");
        assert.equal(mocks.geometries.length, 0, "no geometry for a closed mover");
        assert.equal(mocks.focuses.length, 0, "no follow for a closed mover");
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled.length, 1, "forced reconcile still settles");
    });

    it("stops before membership when the mover is replaced reentrantly", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        // Same stable id, new live object during the output setter: the
        // retained ref no longer resolves in the window list.
        mocks.outputHandlers.push(() => {
            const at = world.wins.findIndex((win) => win.id === "win-2");
            if (at >= 0) {
                world.wins.splice(at, 1);
            }
            world.wins.push({
                id: "win-2",
                ref: {},
                output: "out-1",
                desktops: ["ws-a"],
                rect: { x: 100, y: 0, w: 100, h: 80 },
                floating: false,
                sticky: false,
                fullscreen: false,
                maximized: false,
            });
        });
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.equal(mocks.transfers.length, 1, "transfer initiated once");
        assert.equal(mocks.memberships.length, 0, "no membership for a replaced mover");
        assert.equal(mocks.geometries.length, 0, "no geometry for a replaced mover");
        assert.equal(mocks.focuses.length, 0, "no follow for a replaced mover");
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled.length, 1, "forced reconcile still settles");
    });

    it("writes no geometry when a new window arrives mid-write", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        // A newly arrived tiled window during the output setter breaks exact
        // non-mover set equality at the geometry fence.
        mocks.outputHandlers.push(() => {
            world.wins.push({
                id: "win-new",
                ref: {},
                output: "out-1",
                desktops: ["ws-a"],
                rect: { x: 200, y: 0, w: 100, h: 80 },
                floating: false,
                sticky: false,
                fullscreen: false,
                maximized: false,
            });
        });
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.equal(mocks.transfers.length, 1, "transfer initiated once");
        assert.equal(mocks.memberships.length, 1, "membership applied before the geometry fence");
        assert.equal(mocks.geometries.length, 0, "no geometry with an extra arrival");
        assert.equal(mocks.focuses.length, 0, "no follow without applied geometry");
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled.length, 1, "forced reconcile still settles");
    });

    it("writes no geometry when a survivor flips exceptional mid-write", () => {
        const world = makeAdapterWorld();
        const mocks = mockAdapter(world);
        // A survivor flipping fullscreen during the output setter breaks
        // flag-exactness at the geometry fence.
        mocks.outputHandlers.push(() => {
            const survivor = world.wins.find((win) => win.id === "win-1") as FakeWin;
            survivor.fullscreen = true;
        });
        const adapter = enableAdapter(mocks);
        assert.equal(adapter.requestSendToOutput("out-2", "ws-b", true), true);
        const body = sendPayload(mocks, 0);
        mocks.callbacks[0]?.(
            plannedOutputReply(body["correlation_id"] as string, {
                follow: true,
                focusLeaf: "leaf-win-2",
                focusOutput: "out-2",
                focusWorkspace: "ws-b",
            }),
        );
        assert.equal(mocks.transfers.length, 1, "transfer initiated once");
        assert.equal(mocks.memberships.length, 1, "membership applied before the geometry fence");
        assert.equal(mocks.geometries.length, 0, "no geometry after a survivor flag flip");
        assert.equal(mocks.focuses.length, 0, "no follow without applied geometry");
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.settled.length, 1, "forced reconcile still settles");
    });
});

// ---------- observation ----------

describe("observeOutputSendTarget (cross-output send observation)", () => {
    function fakeSendWorkspace(opts: { stickyActive?: boolean; unknownTarget?: boolean; fullscreenActive?: boolean } = {}): unknown {
        const out1 = { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 600 } };
        const out2 = { name: "out-2", geometry: { x: 800, y: 0, width: 800, height: 600 } };
        const wsA = { id: "ws-a" };
        const wsB = { id: "ws-b" };
        const winA = {
            normalWindow: true,
            output: out1,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "a",
            frameGeometry: { x: 10, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
        };
        const winB = {
            normalWindow: true,
            output: out1,
            onAllDesktops: opts.stickyActive === true,
            desktops: [wsA],
            internalId: "b",
            frameGeometry: { x: 120, y: 10, width: 100, height: 80 },
            fullScreen: opts.fullscreenActive === true,
            maximizeMode: 0,
        };
        const winT = {
            normalWindow: true,
            output: out2,
            onAllDesktops: false,
            desktops: [wsB],
            internalId: "t",
            frameGeometry: { x: 810, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
        };
        return {
            activeWindow: winB,
            screens: [out1, out2],
            desktops: [wsA, wsB],
            currentDesktopForScreen: (screen: object): object => (screen === out1 ? wsA : wsB),
            clientArea: (_kind: number, screen: object, _desktop: object): object =>
                screen === out1 ? { x: 0, y: 0, w: 800, h: 600 } : { x: 800, y: 0, w: 800, h: 600 },
            windowList: (): object[] => [winA, winB, winT],
        };
    }

    it("resolves the destination current workspace with per-output windows", () => {
        const observed = observeOutputSendTarget(fakeSendWorkspace(), new Map(), "out-2", new Set());
        assert.notEqual(observed, null);
        assert.equal(observed?.sourceOutput, "out-1");
        assert.equal(observed?.sourceWorkspace, "ws-a");
        assert.equal(observed?.targetOutput, "out-2");
        assert.equal(observed?.targetWorkspace, "ws-b");
        assert.equal(observed?.sourceWindows.length, 2);
        assert.equal(observed?.targetWindows.length, 1);
        assert.equal(observed?.focusedId, "b");
        assert.notEqual(observed?.moverRef, null);
        assert.notEqual(observed?.targetDesktopRef, null);
        assert.equal(observed?.targetExists, true);
    });

    it("leaves the mover empty for a sticky active window", () => {
        const observed = observeOutputSendTarget(fakeSendWorkspace({ stickyActive: true }), new Map(), "out-2", new Set());
        assert.notEqual(observed, null);
        assert.equal(observed?.focusedId, "");
        assert.equal(observed?.moverRef, null);
    });

    it("admits a fullscreen active window as the cross-output mover", () => {
        // D28: the production cross-output observer binds a retained
        // fullscreen mover exactly like a maximized one.
        const observed = observeOutputSendTarget(fakeSendWorkspace({ fullscreenActive: true }), new Map(), "out-2", new Set());
        assert.ok(observed !== null);
        assert.equal(observed.focusedId, "b");
        assert.notEqual(observed.moverRef, null);
    });

    it("admits an intentional float as the cross-output mover", () => {
        // D16: the production cross-output observer admits an intentional
        // float mover for the slotless deferred transfer, still floating.
        const observed = observeOutputSendTarget(fakeSendWorkspace(), new Map(), "out-2", new Set(["b"]));
        assert.ok(observed !== null);
        const mover = observed.sourceWindows.find((entry) => entry.id === "b");
        assert.ok(mover !== undefined);
        assert.equal(mover.floating, true);
        assert.equal(mover.sticky, false);
        assert.equal(observed.focusedId, "b");
        assert.notEqual(observed.moverRef, null);
    });

    it("leaves the mover empty for sticky and maximized floats", () => {
        const sticky = observeOutputSendTarget(fakeSendWorkspace({ stickyActive: true }), new Map(), "out-2", new Set(["b"]));
        assert.ok(sticky !== null);
        assert.equal(sticky.focusedId, "");
        assert.equal(sticky.moverRef, null);
        const maximized = fakeSendWorkspace() as Record<string, unknown>;
        const wins = (maximized["windowList"] as () => Array<Record<string, unknown>>)();
        const mover = wins.find((entry) => entry["internalId"] === "b");
        assert.ok(mover !== undefined);
        mover["maximizeMode"] = 3;
        const refused = observeOutputSendTarget(maximized, new Map(), "out-2", new Set(["b"]));
        assert.ok(refused !== null);
        assert.equal(refused.focusedId, "");
        assert.equal(refused.moverRef, null);
    });

    it("leaves the mover empty for a floating fullscreen mover", () => {
        const observed = observeOutputSendTarget(fakeSendWorkspace({ fullscreenActive: true }), new Map(), "out-2", new Set(["b"]));
        assert.ok(observed !== null);
        assert.equal(observed.focusedId, "");
        assert.equal(observed.moverRef, null);
    });

    it("returns null for unknown or same-output targets", () => {
        assert.equal(observeOutputSendTarget(fakeSendWorkspace(), new Map(), "out-9", new Set()), null);
        assert.equal(observeOutputSendTarget(fakeSendWorkspace(), new Map(), "out-1", new Set()), null);
    });
});

// ---------- shortcut catalog ----------

describe("output-send shortcut catalog (item 5.3)", () => {
    it("binds follow on Meta+Ctrl+Alt arrows and HJKL, stay unbound", () => {
        const rows = planOutputSendShortcutCatalog();
        assert.equal(rows.length, 12);
        const follow = rows.filter((row) => row.follow);
        const stay = rows.filter((row) => !row.follow);
        assert.equal(follow.length, 8);
        assert.equal(stay.length, 4);
        const seq = (direction: string): string[] =>
            follow.filter((row) => row.direction === direction).map((row) => row.sequence).sort();
        assert.deepEqual(seq("left"), ["Meta+Ctrl+Alt+H", "Meta+Ctrl+Alt+Left"]);
        assert.deepEqual(seq("down"), ["Meta+Ctrl+Alt+Down", "Meta+Ctrl+Alt+J"]);
        assert.deepEqual(seq("up"), ["Meta+Ctrl+Alt+K", "Meta+Ctrl+Alt+Up"]);
        assert.deepEqual(seq("right"), ["Meta+Ctrl+Alt+L", "Meta+Ctrl+Alt+Right"]);
        for (const row of stay) {
            assert.equal(row.sequence, "");
        }
        const actions = rows.map((row) => row.action).sort();
        assert.deepEqual(actions, [
            "omnitiler-send-output-down",
            "omnitiler-send-output-down-arrow",
            "omnitiler-send-output-down-stay",
            "omnitiler-send-output-left",
            "omnitiler-send-output-left-arrow",
            "omnitiler-send-output-left-stay",
            "omnitiler-send-output-right",
            "omnitiler-send-output-right-arrow",
            "omnitiler-send-output-right-stay",
            "omnitiler-send-output-up",
            "omnitiler-send-output-up-arrow",
            "omnitiler-send-output-up-stay",
        ]);
    });
});

// ---------- entry route ----------

interface EntrySignal {
    readonly handlers: Array<(payload?: unknown) => void>;
    readonly signal: {
        connect: (handler: (payload?: unknown) => void) => void;
        disconnect: (handler: (payload?: unknown) => void) => void;
    };
}

function entrySignal(): EntrySignal {
    const handlers: Array<(payload?: unknown) => void> = [];
    return {
        handlers,
        signal: {
            connect: (handler: (payload?: unknown) => void): void => {
                handlers.push(handler);
            },
            disconnect: (handler: (payload?: unknown) => void): void => {
                const at = handlers.indexOf(handler);
                if (at >= 0) {
                    handlers.splice(at, 1);
                }
            },
        },
    };
}

interface EntryWorld {
    workspace: Record<string, unknown>;
    outputs: Array<{ name: string; geometry: object }>;
    desktops: Array<{ id: string; x11DesktopNumber?: number }>;
    wins: Array<Record<string, unknown>>;
    currentByOutput: Map<object, object>;
}

function makeEntryWorld(single: boolean): EntryWorld {
    const out1 = { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 600 } };
    const out2 = { name: "out-2", geometry: { x: 800, y: 0, width: 800, height: 600 } };
    const outputs = single ? [out1] : [out1, out2];
    const wsA = { id: "ws-a", x11DesktopNumber: 1 };
    const wsB = { id: "ws-b", x11DesktopNumber: 2 };
    const currentByOutput = new Map<object, object>([
        [out1, wsA],
        [out2, wsB],
    ]);
    const mkWin = (
        output: object,
        desktops: object[],
        internalId: string,
        frame: object,
        extra: Record<string, unknown> = {},
    ): Record<string, unknown> => ({
        normalWindow: true,
        output,
        onAllDesktops: false,
        desktops: [...desktops],
        internalId,
        frameGeometry: { ...(frame as Record<string, unknown>) },
        fullScreen: false,
        maximizeMode: 0,
        resourceClass: "app",
        frameGeometryChanged: entrySignal().signal,
        fullScreenChanged: entrySignal().signal,
        maximizedChanged: entrySignal().signal,
        desktopsChanged: entrySignal().signal,
        outputChanged: entrySignal().signal,
        ...extra,
    });
    const win1 = mkWin(out1, [wsA], "e-1", { x: 10, y: 10, width: 100, height: 80 });
    const win2 = mkWin(out1, [wsA], "e-2", { x: 120, y: 10, width: 100, height: 80 });
    const winT = mkWin(out2, [wsB], "e-t", { x: 810, y: 10, width: 100, height: 80 });
    const wins = single ? [win1, win2] : [win1, win2, winT];
    const workspace: Record<string, unknown> = {
        activeWindow: win2,
        activeScreen: out1,
        screens: outputs,
        desktops: [wsA, wsB],
        currentDesktopForScreen: (screen: object): object => currentByOutput.get(screen) ?? wsA,
        currentDesktop: wsA,
        setCurrentDesktopForScreen: (desktop: object, output: object): void => {
            currentByOutput.set(output, desktop);
        },
        clientArea: (_kind: number, screen: object, _desktop: object): object =>
            screen === out1 ? { x: 0, y: 0, w: 800, h: 600 } : { x: 800, y: 0, w: 800, h: 600 },
        windowList: (): object[] => [...wins],
        sendClientToScreen: (client: Record<string, unknown>, output: object): void => {
            client["output"] = output;
        },
        windowAdded: entrySignal().signal,
        windowRemoved: entrySignal().signal,
        windowActivated: entrySignal().signal,
        screensChanged: entrySignal().signal,
        currentDesktopChanged: entrySignal().signal,
        desktopsChanged: entrySignal().signal,
    };
    return { workspace, outputs, desktops: [wsA, wsB], wins, currentByOutput };
}

interface EntryMocks {
    dbusCalls: Array<{ method: string; payload: string }>;
    callbacks: Array<(reply: unknown) => void>;
    logs: string[];
    shortcuts: Array<{ action: string; sequence: string; callback: () => void }>;
}

function startEntryWorld(world: EntryWorld): { handle: ReturnType<typeof startPlanAdapterEntry>; mocks: EntryMocks } {
    const mocks: EntryMocks = { dbusCalls: [], callbacks: [], logs: [], shortcuts: [] };
    const handle = startPlanAdapterEntry({
        workspace: world.workspace,
        owner: "owner-1",
        generation: "gen-1",
        log: (message: string): void => {
            mocks.logs.push(message);
        },
        callDbus: (_service, _path, _iface, method, payload, callback): void => {
            if (method === "NameHasOwner") {
                callback(true);
                return;
            }
            if (method === "GetNameOwner") {
                callback(":1.7");
                return;
            }
            if (method === "StartServiceByName") {
                callback(1);
                return;
            }
            mocks.dbusCalls.push({ method, payload });
            mocks.callbacks.push(callback);
        },
        scheduleOnce: (): (() => void) => (): void => {},
        registerShortcutFn: (action, text, sequence, callback): boolean => {
            void text;
            mocks.shortcuts.push({ action, sequence, callback });
            return true;
        },
        readProfileFn: (): string => "cosmic",
        readWorkspaceModeFn: (): unknown => "per-output-local",
        readInnerGapFn: (): number => 4,
        readOuterGapFn: (): number => 8,
    });
    return { handle, mocks };
}

function sendOpCalls(mocks: EntryMocks, op: string): Array<{ index: number; payload: Record<string, unknown> }> {
    const out: Array<{ index: number; payload: Record<string, unknown> }> = [];
    mocks.dbusCalls.forEach((call, index) => {
        if (call.method !== "DescribePlan") {
            return;
        }
        try {
            const payload = JSON.parse(call.payload) as Record<string, unknown>;
            if (((payload["command"] as Record<string, unknown> | undefined)?.["op"] as string) === op) {
                out.push({ index, payload });
            }
        } catch {
            return;
        }
    });
    return out;
}

// Settle every outstanding Plan flight with a benign rejection so later
// entry routes observe an idle single-flight. Used after workspace-mode
// toggles, which dispatch synchronously while this harness never fires
// timers or answers planner calls on its own. Settles in bounded rounds:
// answering one flight may synchronously surface the next queued intent.
function settlePlanFlights(mocks: EntryMocks): void {
    const answered = new Set<number>();
    for (let round = 0; round < 6; round += 1) {
        let progressed = false;
        mocks.dbusCalls.forEach((call, index) => {
            if (call.method !== "DescribePlan" || answered.has(index)) {
                return;
            }
            try {
                const payload = JSON.parse(call.payload) as Record<string, unknown>;
                const correlation = payload["correlation_id"] as string;
                answered.add(index);
                progressed = true;
                mocks.callbacks[index]?.(
                    JSON.stringify({ v: 1, correlation_id: correlation, outcome: "rejected", kind: "settled", message: "settled" }),
                );
            } catch {
                return;
            }
        });
        if (!progressed) {
            return;
        }
    }
}

describe("output-send entry route", () => {
    it("registers the follow and stay shortcut rows", () => {
        const world = makeEntryWorld(false);
        const { mocks } = startEntryWorld(world);
        const actions = mocks.shortcuts.map((entry) => entry.action);
        assert.ok(actions.includes("omnitiler-send-output-right"));
        assert.ok(actions.includes("omnitiler-send-output-right-arrow"));
        assert.ok(actions.includes("omnitiler-send-output-right-stay"));
        const stay = mocks.shortcuts.find((entry) => entry.action === "omnitiler-send-output-up-stay");
        assert.equal(stay?.sequence, "");
    });

    it("dispatches send-to-output and follows the mover with no switch", () => {
        const world = makeEntryWorld(false);
        const { handle, mocks } = startEntryWorld(world);
        assert.notEqual(handle, null);
        handle?.requestSendToOutput("right", true);
        const sends = sendOpCalls(mocks, "send-to-output");
        assert.equal(sends.length, 1);
        const payload = sends[0]?.payload as Record<string, unknown>;
        assert.equal((payload["target_domain"] as Record<string, unknown>)["output"], "out-2");
        const correlation = payload["correlation_id"] as string;
        const mover = world.wins[1] as Record<string, unknown>;
        mocks.callbacks[sends[0]?.index as number]?.(
            JSON.stringify({
                v: 1,
                correlation_id: correlation,
                outcome: "planned",
                kind: "send-to-output",
                base_revision: 1,
                detail: { kind: "send-to-output", policy_version: 1, capability: "move-tiled" },
                desired_geometry: [
                    { window: "e-1", leaf: "leaf-e-1", output: "out-1", workspace: "ws-a", rect: { x: 0, y: 0, w: 800, h: 600 } },
                    { window: "e-2", leaf: "leaf-e-2", output: "out-2", workspace: "ws-b", rect: { x: 800, y: 0, w: 400, h: 600 } },
                    { window: "e-t", leaf: "leaf-e-t", output: "out-2", workspace: "ws-b", rect: { x: 1200, y: 0, w: 400, h: 600 } },
                ],
                desired_focus: { domain_output: "out-2", domain_workspace: "ws-b", leaf: "leaf-e-2" },
                preconditions: ["window-observed", "desired-topology-valid", "adapter-must-verify-postconditions"],
                operation: {
                    op: "move-tiled",
                    window: "e-2",
                    leaf: "leaf-e-2",
                    source_output: "out-1",
                    source_workspace: "ws-a",
                    target_output: "out-2",
                    target_workspace: "ws-b",
                    follow: true,
                },
            }),
        );
        assert.equal((mover["output"] as { name: string }).name, "out-2");
        assert.deepEqual(
            (mover["desktops"] as Array<{ id: string }>).map((entry) => entry.id),
            ["ws-b"],
        );
        assert.equal(world.workspace["activeWindow"], mover);
        // No desktop switch: the source output still shows its workspace.
        assert.equal((world.currentByOutput.get(world.outputs[0] as object) as { id: string }).id, "ws-a");
        handle?.stop();
    });

    it("no-ops on a single output with no dispatch", () => {
        const world = makeEntryWorld(true);
        const { handle, mocks } = startEntryWorld(world);
        handle?.requestSendToOutput("right", true);
        assert.equal(sendOpCalls(mocks, "send-to-output").length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("event=output-send") && line.includes("outcome=no-target")));
        handle?.stop();
    });

    it("moves membership-only across a floating boundary with the float frame stable", () => {
        const world = makeEntryWorld(false);
        const { handle, mocks } = startEntryWorld(world);
        handle?.requestWorkspaceTilingToggle();
        settlePlanFlights(mocks);
        const mover = world.wins[1] as Record<string, unknown>;
        const frameBefore = { ...(mover["frameGeometry"] as Record<string, unknown>) };
        handle?.requestSendToOutput("right", true);
        // No Rust send crosses a floating boundary.
        assert.equal(sendOpCalls(mocks, "send-to-output").length, 0);
        assert.equal((mover["output"] as { name: string }).name, "out-2");
        assert.deepEqual(
            (mover["desktops"] as Array<{ id: string }>).map((entry) => entry.id),
            ["ws-b"],
        );
        assert.deepEqual(mover["frameGeometry"], frameBefore);
        assert.equal(world.workspace["activeWindow"], mover);
        assert.ok(
            mocks.logs.some(
                (line) => line.includes("event=output-send") && line.includes("gate=floating-boundary"),
            ),
        );
        handle?.stop();
    });

    it("selects the mover-centred candidate, not left/top (position-based)", () => {
        const world = makeEntryWorld(false);
        // Split the right side: out-2 keeps the top half, out-3 takes the
        // bottom half. The mover sits low, so its centre projection selects
        // out-3 even though left/top would pick out-2.
        const out2 = world.outputs[1] as Record<string, unknown>;
        out2["geometry"] = { x: 800, y: 0, width: 800, height: 300 };
        const out3 = { name: "out-3", geometry: { x: 800, y: 300, width: 800, height: 300 } };
        (world.workspace["screens"] as object[]).push(out3);
        const mover = world.wins[1] as Record<string, unknown>;
        mover["frameGeometry"] = { x: 120, y: 400, width: 100, height: 80 };
        const { handle, mocks } = startEntryWorld(world);
        handle?.requestSendToOutput("right", true);
        const sends = sendOpCalls(mocks, "send-to-output");
        assert.equal(sends.length, 1);
        assert.equal((sends[0]?.payload["target_domain"] as Record<string, unknown>)["output"], "out-3");
        handle?.stop();
    });

    it("refuses a sticky mover on a floating boundary without setters", () => {
        const world = makeEntryWorld(false);
        const { handle, mocks } = startEntryWorld(world);
        handle?.requestWorkspaceTilingToggle();
        settlePlanFlights(mocks);
        // Sticky windows stay ineligible even on floating boundaries: the
        // probe mover gate empties and the send refuses before any setter.
        (world.wins[1] as Record<string, unknown>)["onAllDesktops"] = true;
        handle?.requestSendToOutput("right", true);
        assert.equal(sendOpCalls(mocks, "send-to-output").length, 0);
        assert.equal(((world.wins[1] as Record<string, unknown>)["output"] as { name: string }).name, "out-1");
        assert.ok(
            mocks.logs.some(
                (line) => line.includes("event=output-send") && line.includes("reason=non-tiled-focus"),
            ),
        );
        handle?.stop();
    });

    it("reports arrival-unconfirmed when the source view switches during a floating stay", () => {
        const world = makeEntryWorld(false);
        const { handle, mocks } = startEntryWorld(world);
        handle?.requestWorkspaceTilingToggle();
        settlePlanFlights(mocks);
        // A synchronous external view switch on the source output revokes
        // visibility: the stay must not claim stayed.
        const sender = world.workspace["sendClientToScreen"] as (client: object, output: object) => void;
        world.workspace["sendClientToScreen"] = (client: Record<string, unknown>, output: object): void => {
            sender(client, output);
            world.currentByOutput.set(world.outputs[0] as object, world.desktops[1] as object);
        };
        handle?.requestSendToOutput("right", false);
        const mover = world.wins[1] as Record<string, unknown>;
        // The membership transfer stands; only the stay claim is withheld.
        assert.equal((mover["output"] as { name: string }).name, "out-2");
        assert.ok(
            mocks.logs.some(
                (line) => line.includes("event=output-send") && line.includes("follow=arrival-unconfirmed"),
            ),
        );
        assert.ok(
            !mocks.logs.some(
                (line) => line.includes("event=output-send") && line.includes("follow=stayed"),
            ),
        );
        handle?.stop();
    });

    it("reports arrival-unconfirmed when the destination drifts before floating follow", () => {
        const world = makeEntryWorld(false);
        const { handle, mocks } = startEntryWorld(world);
        handle?.requestWorkspaceTilingToggle();
        settlePlanFlights(mocks);
        // The destination workspace drifts off the target output before the
        // follow proof: no focus setter runs.
        const sender = world.workspace["sendClientToScreen"] as (client: object, output: object) => void;
        world.workspace["sendClientToScreen"] = (client: Record<string, unknown>, output: object): void => {
            sender(client, output);
            world.currentByOutput.set(world.outputs[1] as object, world.desktops[0] as object);
        };
        const before = world.workspace["activeWindow"];
        handle?.requestSendToOutput("right", true);
        assert.equal(world.workspace["activeWindow"], before);
        assert.ok(
            mocks.logs.some(
                (line) => line.includes("event=output-send") && line.includes("follow=arrival-unconfirmed"),
            ),
        );
        handle?.stop();
    });
});
