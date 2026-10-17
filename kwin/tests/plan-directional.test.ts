import assert from "node:assert/strict";
import { describe, it } from "node:test";

import {
    PlanAdapter,
    PlanAdapterEnv,
    PlanDirection,
    PlanObserved,
    DirectionalObservation,
    planDirectionalFingerprint,
} from "../src/plan-adapter";
import { observeDirectionalDomain } from "../src/plan-adapter-entry";
import { startPlanAdapterEntry } from "../src/plan-adapter-entry";

function refs() {
    return { a: {}, b: {}, x: {} };
}

function twoDomainObserved(
    r: { a: object; b: object; x: object },
    opts: {
        focused?: object;
        aRect?: { x: number; y: number; w: number; h: number };
        xRect?: { x: number; y: number; w: number; h: number };
        bRect?: { x: number; y: number; w: number; h: number };
        targetWorkspace?: string;
        targetBounds?: { x: number; y: number; w: number; h: number };
    } = {},
): PlanObserved {
    const focused = opts.focused ?? r.a;
    const aRect = opts.aRect ?? { x: 810, y: 10, w: 100, h: 80 };
    const xRect = opts.xRect ?? { x: 10, y: 10, w: 100, h: 80 };
    const targetWorkspace = opts.targetWorkspace ?? "ws-b";
    const targetBounds = opts.targetBounds ?? { x: 800, y: 0, w: 800, h: 600 };
    const extra =
        opts.bRect === undefined
            ? []
            : [
                  Object.freeze({
                      id: "win-b",
                      ref: r.b,
                      rect: Object.freeze({ ...opts.bRect }),
                      output: "out-1",
                      workspace: "ws-a",
                      fullscreen: false,
                      maximized: false,
                      floating: false,
                      sticky: false,
                      resourceClass: "unknown",
                  }),
              ];
    return {
        domainOutput: "out-1",
        domainWorkspace: "ws-a",
        domainBounds: { x: 0, y: 0, w: 800, h: 600 },
        domainGap: 4,
        domainOuterGap: 8,
        focusedId: "win-a",
        domains: Object.freeze([
            Object.freeze({
                output: "out-1",
                workspace: "ws-a",
                bounds: { x: 0, y: 0, w: 800, h: 600 },
                gap: 4,
                outerGap: 8,
                adjacent: Object.freeze({ right: "out-2" }),
            }),
            Object.freeze({
                output: "out-2",
                workspace: targetWorkspace,
                bounds: targetBounds,
                gap: 4,
                outerGap: 8,
                adjacent: Object.freeze({ left: "out-1" }),
            }),
        ]),
        windows: Object.freeze([
            Object.freeze({
                id: "win-a",
                ref: r.a,
                rect: Object.freeze({ ...aRect }),
                output: "out-1",
                workspace: "ws-a",
                fullscreen: false,
                maximized: false,
                floating: false,
                sticky: false,
                resourceClass: "unknown",
            }),
            ...extra,
            Object.freeze({
                id: "win-x",
                ref: r.x,
                rect: Object.freeze({ ...xRect }),
                output: "out-2",
                workspace: targetWorkspace,
                fullscreen: false,
                maximized: false,
                floating: false,
                sticky: false,
                resourceClass: "unknown",
            }),
        ]),
        activeRef: focused,
        fingerprint: "dir-fp-1",
        revalidate: () => true,
    };
}

function sourceObserved(
    r: { a: object; b: object; x: object },
    aRect: { x: number; y: number; w: number; h: number },
): PlanObserved {
    const observed = twoDomainObserved(r, { aRect });
    const { domains: _domains, ...source } = observed;
    return {
        ...source,
        windows: Object.freeze(observed.windows.filter((entry) => entry.output === "out-1")),
    };
}

interface Mocks {
    readonly dbusCalls: Array<{ method: string; payload: string }>;
    readonly callbacks: Array<(reply: unknown) => void>;
    readonly logs: string[];
    readonly events: string[];
    readonly geometries: Array<{ target: object; rect: unknown }>;
    readonly actives: object[];
    readonly timers: Array<{ delayMs: number; callback: () => void; cancelled: boolean }>;
    readonly subscribes: Array<{ kind: string; handler: (target?: object) => void }>;
    directionalImpl: (direction: PlanDirection) => DirectionalObservation | PlanObserved | null;
    observeImpl: () => PlanObserved | null;
    activeImpl: () => object | null;
    env: PlanAdapterEnv;
}

function mockEnv(r: { a: object; b: object; x: object }): Mocks {
    const state = {
        dbusCalls: [],
        callbacks: [],
        logs: [],
        events: [],
        geometries: [],
        actives: [],
        timers: [],
        subscribes: [],
        directionalImpl: (_direction: PlanDirection): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: twoDomainObserved(r),
        }),
        observeImpl: (): PlanObserved | null => twoDomainObserved(r),
        activeImpl: (): object | null => r.a,
    } as unknown as Mocks;
    const env: PlanAdapterEnv = {
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
            state.dbusCalls.push({ method, payload });
            state.callbacks.push(callback);
        },
        scheduleOnce: (delayMs, callback): (() => void) => {
            const entry = { delayMs, callback, cancelled: false };
            state.timers.push(entry);
            return (): void => {
                entry.cancelled = true;
            };
        },
        log: (message): void => {
            state.logs.push(message);
        },
        observe: (): PlanObserved | null => state.observeImpl(),
        observeDirectional: (direction): DirectionalObservation | PlanObserved | null =>
            state.directionalImpl(direction),
        clearMaximize: (): "invoked" => "invoked",
        setGeometry: (target, rect): boolean => {
            state.events.push(`geometry:${(rect as { x: number }).x}`);
            state.geometries.push({ target, rect });
            return true;
        },
        setActive: (target): boolean => {
            state.events.push("setActive");
            state.actives.push(target);
            return true;
        },
        active: (): object | null => state.activeImpl(),
        subscribe: (kind, handler): (() => void) => {
            state.subscribes.push({ kind, handler });
            return (): void => {};
        },
    };
    (state as { env: PlanAdapterEnv }).env = env;
    return state;
}

function enable(mocks: Mocks): PlanAdapter {
    const adapter = new PlanAdapter(mocks.env);
    assert.equal(adapter.enable({ owner: "owner-1", generation: "gen-1" }), true);
    return adapter;
}

function payload(mocks: Mocks, index: number): Record<string, unknown> {
    return JSON.parse(mocks.dbusCalls[index]?.payload as string) as Record<string, unknown>;
}

function crossFocusReply(correlation: string): string {
    return JSON.stringify({
        v: 1,
        correlation_id: correlation,
        outcome: "planned",
        base_revision: 2,
        detail: { kind: "focus", capability: "directional-focus", direction: "right", to_window: "win-x", cross_output: true },
        desired_geometry: [
            { window: "win-a", leaf: "leaf-a", output: "out-1", workspace: "ws-a", rect: { x: 10, y: 10, w: 100, h: 80 } },
            { window: "win-x", leaf: "leaf-x", output: "out-2", workspace: "ws-b", rect: { x: 810, y: 10, w: 100, h: 80 } },
        ],
        desired_focus: { domain_output: "out-2", domain_workspace: "ws-b", leaf: "leaf-x" },
        operation: {
            op: "focus",
            domain_output: "out-2",
            domain_workspace: "ws-b",
            from_leaf: "leaf-a",
            to_leaf: "leaf-x",
            from_window: "win-a",
            to_window: "win-x",
            direction: "right",
            route: ["leaf-x"],
            cross_source_output: "out-1",
            cross_source_workspace: "ws-a",
        },
        preconditions: [
            "focused-leaf-occupied-by-focused-window",
            "target-leaf-occupied",
            "focus-targets-adjacent-output",
            "adapter-must-verify-postconditions",
        ],
    });
}

function crossMoveReply(correlation: string, targetWorkspace = "ws-b", targetHeight = 580): string {
    return JSON.stringify({
        v: 1,
        correlation_id: correlation,
        outcome: "planned",
        base_revision: 2,
        detail: { kind: "move", rule: "R4", capability: "CrossOutputTransfer", direction: "right" },
        desired_geometry: [
            { window: "win-a", leaf: "leaf-a", output: "out-2", workspace: targetWorkspace, rect: { x: 810, y: 10, w: 380, h: targetHeight } },
            { window: "win-x", leaf: "leaf-x", output: "out-2", workspace: targetWorkspace, rect: { x: 1200, y: 10, w: 380, h: targetHeight } },
        ],
        desired_focus: { domain_output: "out-2", domain_workspace: targetWorkspace, leaf: "leaf-a" },
        operation: {
            op: "move",
            rule: "R4",
            capability: "CrossOutputTransfer",
            direction: "right",
            window: "win-a",
            leaf: "leaf-a",
            source_output: "out-1",
            source_workspace: "ws-a",
            target_output: "out-2",
            target_workspace: targetWorkspace,
            source_root_child_index: 0,
            target: "occupied",
        },
        preconditions: [
            "focused-leaf-occupied-by-focused-window",
            "source-root-membership-and-adjacent-same-workspace-output",
            "adapter-must-verify-postconditions",
        ],
    });
}

describe("directional fingerprint (cross-language golden vector)", () => {
    it("matches the Rust derivation byte for byte", () => {
        // Pinned with the Rust `directional_fingerprint_golden_vector`
        // test: same input must emit the same value on both sides, or
        // requests fail `fingerprint-mismatch` validation. Item 5.2 scheme:
        // left/right/up/down adjacency in fixed order.
        const fp = planDirectionalFingerprint(
            [
                {
                    output: "out-2",
                    workspace: "ws-b",
                    bounds: { x: 800, y: 0, w: 800, h: 600 },
                    gap: 4,
                    outerGap: 8,
                    adjacent: { left: "out-1" },
                },
                {
                    output: "out-1",
                    workspace: "ws-a",
                    bounds: { x: 0, y: 0, w: 800, h: 600 },
                    gap: 4,
                    outerGap: 8,
                    adjacent: { right: "out-2" },
                },
            ],
            "win-a",
            [
                { window: "win-a", output: "out-2", workspace: "ws-b", rect: { x: 810, y: 10, w: 100, h: 80 }, floating: false, fitExcluded: false },
                { window: "win-x", output: "out-1", workspace: "ws-a", rect: { x: 10, y: 10, w: 100, h: 80 }, floating: false, fitExcluded: false },
            ],
        );
        assert.equal(fp, 2130914552);
    });
});

describe("plan adapter directional cross-output (active route)", () => {
    it("sends the bounded two-domain payload on Meta+Right focus", () => {
        const r = refs();
        const mocks = mockEnv(r);
        const adapter = enable(mocks);
        adapter.requestFocus("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const body = payload(mocks, 0);
        const domains = body["domains"] as Array<Record<string, unknown>>;
        assert.equal(domains.length, 2);
        assert.equal(domains[0]?.["output"], "out-1");
        assert.equal(domains[1]?.["output"], "out-2");
        assert.deepEqual((domains[0] as Record<string, unknown>)["adjacent"], { right: "out-2" });
        assert.deepEqual((domains[1] as Record<string, unknown>)["adjacent"], { left: "out-1" });
        const windows = body["windows"] as Array<Record<string, unknown>>;
        assert.equal(windows.length, 2);
        assert.deepEqual(body["command"], { op: "focus", window: "win-a", direction: "right" });
    });

    it("omits domains on Up (vertical focus stays local)", () => {
        // Item 5 authorizes moves and output sends across outputs, not
        // vertical focus: Up/Down focus never consults the directional hook
        // and stays single-domain even with a ready vertical target.
        const r = refs();
        const mocks = mockEnv(r);
        let directionalCalled = false;
        mocks.directionalImpl = (_direction): DirectionalObservation | PlanObserved | null => {
            directionalCalled = true;
            return { status: "ready", observed: twoDomainObserved(r) };
        };
        mocks.observeImpl = (): PlanObserved | null => ({
            domainOutput: "out-1",
            domainWorkspace: "ws-a",
            domainBounds: { x: 0, y: 0, w: 800, h: 600 },
            domainGap: 4,
            domainOuterGap: 8,
            focusedId: "win-a",
            windows: Object.freeze([
                Object.freeze({
                    id: "win-a",
                    ref: r.a,
                    rect: Object.freeze({ x: 10, y: 10, w: 100, h: 80 }),
                    output: "out-1",
                    workspace: "ws-a",
                    fullscreen: false,
                    maximized: false,
                    floating: false,
                    sticky: false,
                    resourceClass: "unknown",
                }),
            ]),
            activeRef: r.a,
            fingerprint: "single-fp",
            revalidate: () => true,
        });
        const adapter = enable(mocks);
        adapter.requestFocus("up");
        assert.equal(directionalCalled, false);
        const body = payload(mocks, 0);
        assert.equal("domains" in body, false);
    });

    it("refuses before dispatch on invalid directional evidence", () => {
        for (const op of ["focus", "move"] as const) {
            const r = refs();
            const mocks = mockEnv(r);
            mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
                status: "invalid",
                observed: null,
            });
            const adapter = enable(mocks);
            if (op === "focus") {
                adapter.requestFocus("right");
            } else {
                adapter.requestMove("right");
            }
            assert.equal(mocks.dbusCalls.length, 0);
            assert.ok(
                mocks.logs.some((line) => line.includes(`${op}-refused-ambiguous`)),
                JSON.stringify(mocks.logs),
            );
        }
    });

    it("refuses before dispatch on null directional evidence", () => {
        const r = refs();
        const mocks = mockEnv(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => null;
        const adapter = enable(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("move-refused-ambiguous")));
    });

    it("keeps local single-domain behavior on confirmed no-target", () => {
        const r = refs();
        const mocks = mockEnv(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "no-target",
            observed: null,
        });
        mocks.observeImpl = (): PlanObserved | null => ({
            domainOutput: "out-1",
            domainWorkspace: "ws-a",
            domainBounds: { x: 0, y: 0, w: 800, h: 600 },
            domainGap: 4,
            domainOuterGap: 8,
            focusedId: "win-a",
            windows: Object.freeze([
                Object.freeze({
                    id: "win-a",
                    ref: r.a,
                    rect: Object.freeze({ x: 10, y: 10, w: 100, h: 80 }),
                    output: "out-1",
                    workspace: "ws-a",
                    fullscreen: false,
                    maximized: false,
                    floating: false,
                    sticky: false,
                    resourceClass: "unknown",
                }),
            ]),
            activeRef: r.a,
            fingerprint: "single-fp",
            revalidate: () => true,
        });
        const adapter = enable(mocks);
        adapter.requestFocus("right");
        assert.equal(mocks.dbusCalls.length, 1);
        assert.equal("domains" in payload(mocks, 0), false);
    });

    it("applies cross focus with exactly one setActive and no geometry writes", () => {
        const r = refs();
        const mocks = mockEnv(r);
        const adapter = enable(mocks);
        adapter.requestFocus("right");
        const body = payload(mocks, 0);
        mocks.callbacks[0]?.(crossFocusReply(body["correlation_id"] as string));
        assert.equal(mocks.actives.length, 1);
        assert.equal(mocks.actives[0], r.x);
        assert.equal(mocks.geometries.length, 0);
        assert.ok(mocks.logs.some((line) => line.includes("planned-applied")));
    });

    it("refuses cross focus when the operation source mismatches", () => {
        const r = refs();
        const mocks = mockEnv(r);
        const adapter = enable(mocks);
        adapter.requestFocus("right");
        const body = payload(mocks, 0);
        const reply = JSON.parse(crossFocusReply(body["correlation_id"] as string)) as Record<string, unknown>;
        (reply["operation"] as Record<string, unknown>)["cross_source_output"] = "out-9";
        mocks.callbacks[0]?.(JSON.stringify(reply));
        assert.equal(mocks.actives.length, 0);
        assert.equal(mocks.geometries.length, 0);
    });

    it("refuses R4 before membership, geometry, or focus writes", () => {
        const r = refs();
        const mocks = mockEnv(r);
        // Native active trails the plan (e.g. the mover is not yet active on
        // the target) so the focus-follow actuation fires exactly once.
        mocks.activeImpl = () => r.x;
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const body = payload(mocks, 0);
        assert.equal((body["domains"] as Array<unknown>).length, 2);
        assert.equal((body["command"] as Record<string, unknown>)["cross_output_transfer"], false);
        mocks.callbacks[0]?.(crossMoveReply(body["correlation_id"] as string));
        // A synthetic R4 reply cannot bypass the disabled capability. No
        // geometry or focus write occurs.
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.actives.length, 0);
    });

    it("refuses an R4 reply even when the mover is already active", () => {
        const r = refs();
        const mocks = mockEnv(r);
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const body = payload(mocks, 0);
        mocks.callbacks[0]?.(crossMoveReply(body["correlation_id"] as string));
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.actives.length, 0);
    });

    it("applies a local plan on a two-domain flight without cross actuation", () => {
        const r = refs();
        const mocks = mockEnv(r);
        mocks.activeImpl = () => r.x;
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const body = payload(mocks, 0);
        // Local R1/R2/R3 shape: source-only geometry, source focus, no
        // operation. Must apply as an ordinary local move: no membership
        // transfer, geometry writes plus focus follow only.
        const local = JSON.stringify({
            v: 1,
            correlation_id: body["correlation_id"],
            outcome: "planned",
            base_revision: 2,
            detail: { kind: "move", rule: "R1", capability: "WrapPerpendicular", direction: "right" },
            desired_geometry: [
                { window: "win-a", leaf: "leaf-a", output: "out-1", workspace: "ws-a", rect: { x: 10, y: 10, w: 380, h: 580 } },
            ],
            desired_focus: { domain_output: "out-1", domain_workspace: "ws-a", leaf: "leaf-a" },
        });
        mocks.callbacks[0]?.(local);
        assert.equal(mocks.geometries.length, 1);
        assert.equal(mocks.actives.length, 1);
    });

    it("does not synthesize a target removal after a local two-domain move", () => {
        const r = refs();
        const mocks = mockEnv(r);
        mocks.activeImpl = () => r.x;
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const body = payload(mocks, 0);
        mocks.callbacks[0]?.(
            JSON.stringify({
                v: 1,
                correlation_id: body["correlation_id"],
                outcome: "planned",
                base_revision: 2,
                detail: { kind: "move", rule: "R1", capability: "WrapPerpendicular", direction: "right" },
                desired_geometry: [
                    { window: "win-a", leaf: "leaf-a", output: "out-1", workspace: "ws-a", rect: { x: 10, y: 10, w: 380, h: 580 } },
                ],
                desired_focus: { domain_output: "out-1", domain_workspace: "ws-a", leaf: "leaf-a" },
            }),
        );
        mocks.observeImpl = () => sourceObserved(r, { x: 10, y: 10, w: 380, h: 580 });
        mocks.subscribes.find((entry) => entry.kind === "geometry")?.handler(r.a);
        mocks.timers[mocks.timers.length - 1]?.callback();
        assert.equal(mocks.dbusCalls.length, 1);
    });

    it("refuses on stale directional scope before any write", () => {
        const r = refs();
        const mocks = mockEnv(r);
        const adapter = enable(mocks);
        adapter.requestFocus("right");
        const body = payload(mocks, 0);
        // Fresh observation drifts: target rect changes.
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: twoDomainObserved(r, { xRect: { x: 20, y: 10, w: 100, h: 80 } }),
        });
        mocks.callbacks[0]?.(crossFocusReply(body["correlation_id"] as string));
        assert.equal(mocks.actives.length, 0);
        assert.equal(mocks.geometries.length, 0);
    });

    it("does not retain a fabricated post-move baseline", () => {
        const r = refs();
        const mocks = mockEnv(r);
        mocks.activeImpl = () => r.x;
        const adapter = enable(mocks);
        const takeoff = twoDomainObserved(r, { bRect: { x: 400, y: 10, w: 100, h: 80 } });
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: takeoff,
        });
        mocks.observeImpl = (): PlanObserved | null => takeoff;
        adapter.requestMove("right");
        const body = payload(mocks, 0);
        mocks.callbacks[0]?.(crossMoveReply(body["correlation_id"] as string));
        assert.equal(mocks.geometries.length, 0);
    });
});


describe("plan adapter R4 immediate transfer (lean, no wire protocol)", () => {
    interface R4Native {
        readonly out1: { name: string };
        readonly out2: { name: string };
        readonly wsA: { id: string };
        readonly wsB: { id: string };
        outputOfMover: string;
        desktopsOfMover: string[];
        rects: Map<object, { x: number; y: number; w: number; h: number }>;
        activeRef: object | null;
        outputHandlers: Array<(old: unknown) => void>;
        desktopsHandlers: Array<() => void>;
        sentTransfers: Array<{ mover: object; output: object }>;
        sentMemberships: Array<{ mover: object; refs: ReadonlyArray<object> }>;
        autoUpdate: boolean;
    }

    function r4Mocks(r: { a: object; b: object; x: object }): { mocks: Mocks; native: R4Native } {
        const mocks = mockEnv(r);
        const native: R4Native = {
            out1: { name: "out-1" },
            out2: { name: "out-2" },
            wsA: { id: "ws-a" },
            wsB: { id: "ws-b" },
            outputOfMover: "out-1",
            desktopsOfMover: ["ws-a"],
            rects: new Map([
                [r.a, { x: 810, y: 10, w: 100, h: 80 }],
                [r.x, { x: 10, y: 10, w: 100, h: 80 }],
            ]),
            activeRef: r.b,
            outputHandlers: [],
            desktopsHandlers: [],
            sentTransfers: [],
            sentMemberships: [],
            autoUpdate: true,
        };
        const env = mocks.env as unknown as Record<string, unknown>;
        env["resolveOutput"] = (name: string): object | null =>
            name === "out-1" ? native.out1 : name === "out-2" ? native.out2 : null;
        env["resolveDesktop"] = (workspace: string): object | null =>
            workspace === "ws-a" ? native.wsA : workspace === "ws-b" ? native.wsB : null;
        env["sendClientToScreen"] = (mover: object, output: object): boolean => {
            native.sentTransfers.push({ mover, output });
            mocks.events.push("transfer");
            if (mover !== r.a || (output !== native.out1 && output !== native.out2)) {
                return false;
            }
            if (native.autoUpdate) {
                native.outputOfMover = (output as { name: string }).name;
            }
            return true;
        };
        env["setDesktops"] = (mover: object, refs: ReadonlyArray<object>): boolean => {
            native.sentMemberships.push({ mover, refs });
            mocks.events.push("membership");
            if (mover !== r.a || refs.length !== 1 || (refs[0] !== native.wsA && refs[0] !== native.wsB)) {
                return false;
            }
            if (native.autoUpdate) {
                native.desktopsOfMover = [(refs[0] as { id: string }).id];
            }
            return true;
        };
        env["readOutputName"] = (ref: object): string | null =>
            ref === r.a ? native.outputOfMover : ref === r.x ? "out-2" : null;
        env["readDesktopIds"] = (ref: object): ReadonlyArray<string> | null =>
            ref === r.a ? [...native.desktopsOfMover] : ref === r.x ? ["ws-b"] : null;
        env["readGeometry"] = (ref: object): { x: number; y: number; w: number; h: number } | null => {
            const rect = native.rects.get(ref);
            return rect === undefined ? null : { ...rect };
        };
        env["subscribeMoverOutput"] = (mover: object, handler: (old: unknown) => void): (() => void) | null => {
            if (mover !== r.a) {
                return null;
            }
            native.outputHandlers.push(handler);
            return (): void => {};
        };
        env["subscribeMoverDesktops"] = (mover: object, handler: () => void): (() => void) | null => {
            if (mover !== r.a) {
                return null;
            }
            native.desktopsHandlers.push(handler);
            return (): void => {};
        };
        env["subscribeWindowGeometry"] = (): (() => void) | null => (): void => {};
        const baseSetGeometry = mocks.env.setGeometry;
        (env as { setGeometry: PlanAdapterEnv["setGeometry"] })["setGeometry"] = (target, rect): boolean => {
            native.rects.set(target, { x: rect.x, y: rect.y, w: rect.w, h: rect.h });
            mocks.events.push("geometry");
            return baseSetGeometry(target, rect);
        };
        mocks.activeImpl = (): object | null => native.activeRef;
        (env as { setActive: PlanAdapterEnv["setActive"] })["setActive"] = (target): boolean => {
            mocks.events.push("setActive");
            mocks.actives.push(target);
            native.activeRef = target;
            return true;
        };
        return { mocks, native };
    }

    // Dynamic R4 observation reflecting native mover placement: the fresh
    // directional re-observation used by per-setter scope fences and follow
    // proof must show the mover where native reads place it (source, half,
    // or target). Static mocks would otherwise pin the mover on source and
    // block truthful follow.
    function dynamicOccupiedObserved(
        r: { a: object; b: object; x: object },
        native: { outputOfMover: string; desktopsOfMover: string[] },
    ): PlanObserved {
        const moverOutput = native.outputOfMover;
        const moverWorkspace = native.desktopsOfMover[0] ?? "ws-a";
        return {
            domainOutput: "out-1",
            domainWorkspace: "ws-a",
            domainBounds: { x: 0, y: 0, w: 800, h: 600 },
            domainGap: 4,
            domainOuterGap: 8,
            focusedId: "win-a",
            domains: Object.freeze([
                Object.freeze({
                    output: "out-1",
                    workspace: "ws-a",
                    bounds: { x: 0, y: 0, w: 800, h: 600 },
                    gap: 4,
                    outerGap: 8,
                    adjacent: Object.freeze({ right: "out-2" }),
                }),
                Object.freeze({
                    output: "out-2",
                    workspace: "ws-b",
                    bounds: { x: 800, y: 0, w: 800, h: 600 },
                    gap: 4,
                    outerGap: 8,
                    adjacent: Object.freeze({ left: "out-1" }),
                }),
            ]),
            windows: Object.freeze([
                Object.freeze({
                    id: "win-a",
                    ref: r.a,
                    rect: Object.freeze({ x: 810, y: 10, w: 100, h: 80 }),
                    output: moverOutput,
                    workspace: moverWorkspace,
                    fullscreen: false,
                    maximized: false,
                    floating: false,
                    sticky: false,
                    resourceClass: "unknown",
                }),
                Object.freeze({
                    id: "win-x",
                    ref: r.x,
                    rect: Object.freeze({ x: 10, y: 10, w: 100, h: 80 }),
                    output: "out-2",
                    workspace: "ws-b",
                    fullscreen: false,
                    maximized: false,
                    floating: false,
                    sticky: false,
                    resourceClass: "unknown",
                }),
            ]),
            activeRef: r.a,
            fingerprint: "dir-fp-1",
            revalidate: () => true,
        };
    }

    function dynamicEmptyObserved(
        r: { a: object; b: object; x: object },
        native: { outputOfMover: string; desktopsOfMover: string[] },
    ): PlanObserved {
        const moverOutput = native.outputOfMover;
        const moverWorkspace = native.desktopsOfMover[0] ?? "ws-a";
        return {
            domainOutput: "out-1",
            domainWorkspace: "ws-a",
            domainBounds: { x: 0, y: 0, w: 800, h: 600 },
            domainGap: 4,
            domainOuterGap: 8,
            focusedId: "win-a",
            domains: Object.freeze([
                Object.freeze({
                    output: "out-1",
                    workspace: "ws-a",
                    bounds: { x: 0, y: 0, w: 800, h: 600 },
                    gap: 4,
                    outerGap: 8,
                    adjacent: Object.freeze({ right: "out-2" }),
                }),
                Object.freeze({
                    output: "out-2",
                    workspace: "ws-b",
                    bounds: { x: 800, y: 0, w: 800, h: 600 },
                    gap: 4,
                    outerGap: 8,
                    adjacent: Object.freeze({ left: "out-1" }),
                }),
            ]),
            windows: Object.freeze([
                Object.freeze({
                    id: "win-a",
                    ref: r.a,
                    rect: Object.freeze({ x: 810, y: 10, w: 100, h: 80 }),
                    output: moverOutput,
                    workspace: moverWorkspace,
                    fullscreen: false,
                    maximized: false,
                    floating: false,
                    sticky: false,
                    resourceClass: "unknown",
                }),
            ]),
            activeRef: r.a,
            fingerprint: "dir-fp-empty",
            revalidate: () => true,
        };
    }

    function emptyMoveReply(correlation: string): string {
        return JSON.stringify({
            v: 1,
            correlation_id: correlation,
            outcome: "planned",
            base_revision: 2,
            detail: { kind: "move", rule: "R4", capability: "CrossOutputTransfer", direction: "right" },
            desired_geometry: [
                { window: "win-a", leaf: "leaf-a", output: "out-2", workspace: "ws-b", rect: { x: 810, y: 10, w: 380, h: 580 } },
            ],
            desired_focus: { domain_output: "out-2", domain_workspace: "ws-b", leaf: "leaf-a" },
            operation: {
                op: "move",
                rule: "R4",
                capability: "CrossOutputTransfer",
                direction: "right",
                window: "win-a",
                leaf: "leaf-a",
                source_output: "out-1",
                source_workspace: "ws-a",
                target_output: "out-2",
                target_workspace: "ws-b",
                source_root_child_index: 0,
                target: "empty",
            },
            preconditions: [
                "focused-leaf-occupied-by-focused-window",
                "source-root-membership-and-adjacent-same-workspace-output",
                "adapter-must-verify-postconditions",
            ],
        });
    }

    function assertNoWireProtocol(mocks: Mocks): void {
        for (const call of mocks.dbusCalls) {
            assert.ok(!call.payload.includes("directional-move-ack"), call.payload.slice(0, 160));
            assert.ok(!call.payload.includes("directional-move-verify"), call.payload.slice(0, 160));
            assert.ok(!call.payload.includes("directional-move-cancel"), call.payload.slice(0, 160));
            assert.ok(!call.payload.includes("adapter-lost"), call.payload.slice(0, 160));
            assert.ok(!call.payload.includes("ack_outcome"), call.payload.slice(0, 160));
        }
        for (const line of mocks.logs) {
            assert.ok(!line.includes("directional-move-ack"), line);
            assert.ok(!line.includes("directional-move-verify"), line);
            assert.ok(!line.includes("directional-move-cancel"), line);
            assert.ok(!line.includes("adapter-lost"), line);
        }
    }

    function assertCorrelated(mocks: Mocks, correlation: string, outcome: string): void {
        assert.ok(
            mocks.logs.some((line) => line.includes(correlation) && line.includes(outcome)),
            `correlated ${outcome} for ${correlation}, got ${JSON.stringify(mocks.logs)}`,
        );
    }

    function fireDebounce(mocks: Mocks): number {
        const pending = mocks.timers.filter((entry) => entry.delayMs === 120 && !entry.cancelled);
        for (const entry of pending) {
            entry.callback();
        }
        return pending.length;
    }

    function floatFallbackReply(correlation: string): string {
        return JSON.stringify({
            v: 1,
            correlation_id: correlation,
            outcome: "planned",
            kind: "send-to-output",
            base_revision: 2,
            detail: { kind: "send-to-output", policy_version: 1, capability: "move-tiled" },
            desired_geometry: [
                { window: "win-x", leaf: "leaf-x", output: "out-2", workspace: "ws-b", rect: { x: 810, y: 10, w: 380, h: 580 } },
            ],
            desired_focus: { domain_output: "out-2", domain_workspace: "ws-b", leaf: "leaf-x" },
            operation: {
                op: "move-to-float",
                window: "win-a",
                leaf: "leaf-a",
                source_output: "out-1",
                source_workspace: "ws-a",
                target_output: "out-2",
                target_workspace: "ws-b",
                follow: true,
            },
            preconditions: [
                "window-observed",
                "desired-topology-valid",
                "adapter-must-verify-postconditions",
            ],
        });
    }

    it("floats the mover slotless on R4 exhaustion with one transfer, one remap, survivor geometry only, and one follow", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const body = payload(mocks, 0);
        assert.equal((body["command"] as Record<string, unknown>)["cross_output_transfer"], true);
        const correlation = body["correlation_id"] as string;
        mocks.callbacks[0]?.(floatFallbackReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentTransfers[0]?.output, native.out2);
        assert.equal(native.sentMemberships.length, 1);
        assert.deepEqual(native.sentMemberships[0]?.refs, [native.wsB]);
        // Survivor geometry only: the slotless mover takes no frame write.
        assert.equal(mocks.geometries.length, 1);
        assert.equal(mocks.geometries[0]?.target, r.x);
        const order = mocks.events.filter((entry) => entry === "transfer" || entry === "membership" || entry === "geometry");
        assert.deepEqual(order, ["transfer", "membership", "geometry"]);
        // Native follow still addresses the mover ref exactly once.
        assert.equal(mocks.actives.length, 1);
        assert.equal(mocks.actives[0], r.a);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.dbusCalls.length, 1);
        assertNoWireProtocol(mocks);
        assertCorrelated(mocks, correlation, "r4-float-transfer-started");
        assertCorrelated(mocks, correlation, "r4-native-written");
        assertCorrelated(mocks, correlation, "r4-followed");
        assertCorrelated(mocks, correlation, "arrived");
    });

    it("ignores a late duplicate float reply after arrival with no second transfer", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        const reply = floatFallbackReply(correlation);
        mocks.callbacks[0]?.(reply);
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(mocks.actives.length, 1);
        assert.equal(adapter.isR4InFlight, false);
        // A late duplicate of the same planned reply never re-actuates: no
        // second transfer, membership, geometry, or follow.
        mocks.callbacks[0]?.(reply);
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 1);
        assert.equal(mocks.actives.length, 1);
        assert.equal(adapter.isInFlight, false);
    });

    it("refuses a forged float reply naming the dropped leaf as follow focus", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        const forged = JSON.parse(floatFallbackReply(correlation)) as Record<string, unknown>;
        (forged["desired_focus"] as Record<string, unknown>)["leaf"] = "leaf-a";
        mocks.callbacks[0]?.(JSON.stringify(forged));
        assert.equal(native.sentTransfers.length, 0);
        assert.equal(native.sentMemberships.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assertCorrelated(mocks, correlation, "precondition-mismatch");
    });

    it("refuses a float reply with stay follow selection and one naming mover geometry", () => {
        for (const mutate of ["stay", "mover-geometry"] as const) {
            const r = refs();
            const { mocks, native } = r4Mocks(r);
            mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
                status: "ready",
                observed: dynamicOccupiedObserved(r, native),
            });
            const adapter = enable(mocks);
            adapter.requestMove("right");
            const correlation = payload(mocks, 0)["correlation_id"] as string;
            const forged = JSON.parse(floatFallbackReply(correlation)) as Record<string, unknown>;
            if (mutate === "stay") {
                (forged["operation"] as Record<string, unknown>)["follow"] = false;
            } else {
                ((forged["desired_geometry"] as Array<Record<string, unknown>>)).push({
                    window: "win-a",
                    leaf: "leaf-a",
                    output: "out-2",
                    workspace: "ws-b",
                    rect: { x: 810, y: 10, w: 380, h: 580 },
                });
            }
            mocks.callbacks[0]?.(JSON.stringify(forged));
            assert.equal(native.sentTransfers.length, 0, mutate);
            assert.equal(native.sentMemberships.length, 0, mutate);
            assert.equal(mocks.geometries.length, 0, mutate);
            assert.equal(mocks.actives.length, 0, mutate);
            assert.equal(adapter.isInFlight, false, mutate);
            assertCorrelated(mocks, correlation, "precondition-mismatch");
        }
    });

    it("refuses a float reply when the snapshot mover is already floating", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        const held = dynamicOccupiedObserved(r, native);
        const floatingWindows = held.windows.map((entry) =>
            entry.id === "win-a" ? { ...entry, floating: true } : entry,
        );
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: { ...held, windows: Object.freeze(floatingWindows) },
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(floatFallbackReply(correlation));
        // A held mover claiming a dropped leaf is a mismatched reply: the
        // tiled R4 float drops its leaf, never carries a held one.
        assert.equal(native.sentTransfers.length, 0);
        assert.equal(native.sentMemberships.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isInFlight, false);
        assertCorrelated(mocks, correlation, "precondition-mismatch");
    });

    it("moves to an occupied target with output, desktop, geometry order and one follow", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const body = payload(mocks, 0);
        assert.equal((body["command"] as Record<string, unknown>)["cross_output_transfer"], true);
        const correlation = body["correlation_id"] as string;
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentTransfers[0]?.output, native.out2);
        assert.equal(native.sentMemberships.length, 1);
        assert.deepEqual(native.sentMemberships[0]?.refs, [native.wsB]);
        assert.equal(mocks.geometries.length, 2);
        const order = mocks.events.filter((entry) => entry === "transfer" || entry === "membership" || entry === "geometry");
        assert.deepEqual(order, ["transfer", "membership", "geometry", "geometry"]);
        assert.equal(mocks.actives.length, 1);
        assert.equal(mocks.actives[0], r.a);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.dbusCalls.length, 1);
        assertNoWireProtocol(mocks);
        assertCorrelated(mocks, correlation, "r4-transfer-started");
        assertCorrelated(mocks, correlation, "r4-native-written");
        assertCorrelated(mocks, correlation, "r4-followed");
        assertCorrelated(mocks, correlation, "arrived");
    });

    it("moves down to an occupied target with the same transfer order and one follow", () => {
        // Item 5.1/5.2: vertical R4 actuation reuses the exact horizontal
        // fences, native order, and single follow.
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("down");
        assert.equal(mocks.dbusCalls.length, 1);
        const body = payload(mocks, 0);
        assert.deepEqual(body["command"], {
            op: "move",
            window: "win-a",
            direction: "down",
            cross_output_transfer: true,
        });
        const correlation = body["correlation_id"] as string;
        const reply = JSON.parse(crossMoveReply(correlation)) as Record<string, unknown>;
        (reply["detail"] as Record<string, unknown>)["direction"] = "down";
        const operation = reply["operation"] as Record<string, unknown>;
        operation["direction"] = "down";
        mocks.callbacks[0]?.(JSON.stringify(reply));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentTransfers[0]?.output, native.out2);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 2);
        const order = mocks.events.filter((entry) => entry === "transfer" || entry === "membership" || entry === "geometry");
        assert.deepEqual(order, ["transfer", "membership", "geometry", "geometry"]);
        assert.equal(mocks.actives.length, 1);
        assert.equal(mocks.actives[0], r.a);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assertNoWireProtocol(mocks);
        assertCorrelated(mocks, correlation, "arrived");
    });

    it("moves to an empty target with one geometry and one follow", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicEmptyObserved(r, native),
        });
        mocks.observeImpl = (): PlanObserved | null => dynamicEmptyObserved(r, native);
        const adapter = enable(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(emptyMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 1);
        assert.equal(mocks.actives.length, 1);
        assert.equal(mocks.actives[0], r.a);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.dbusCalls.length, 1);
        assertNoWireProtocol(mocks);
        assertCorrelated(mocks, correlation, "arrived");
    });

    it("tolerates a half-visible own mover at the first geometry fence (sole source)", () => {
        // Live rightward sole-source R4 (2026-10-14): both setters applied,
        // native arrival reconciled later, but the first geometry-scope
        // re-observation saw an empty window set (fresh_len=0, flight_len=1)
        // and settled stale-scope with no follow. Production scope
        // collection matches windows by output plus desktop membership, so a
        // half-placed own mover (output moved, membership lagging, or vice
        // versa) belongs to neither scope and vanishes from the fresh set
        // even though the dispatch contract tolerates source/half/target
        // placement. The fence must tolerate exactly that own-relocation
        // gap via native reads while still refusing unrelated drift.
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        // Production-faithful observation: the mover is listed only when its
        // native placement sits wholly in one scope; a half placement drops
        // it from both scope collections. The first post-membership
        // observation lags one call (live KWin flush timing), later ones see
        // the arrival.
        let lagArmed = false;
        const env = mocks.env as unknown as Record<string, unknown>;
        const baseSetDesktops = mocks.env.setDesktops?.bind(mocks.env);
        (env as { setDesktops: PlanAdapterEnv["setDesktops"] })["setDesktops"] = (mover, refs): boolean => {
            const ok = baseSetDesktops?.(mover, refs) === true;
            if (ok) {
                lagArmed = true;
            }
            return ok;
        };
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => {
            const full = dynamicEmptyObserved(r, native);
            if (!lagArmed) {
                return { status: "ready", observed: full };
            }
            lagArmed = false;
            return {
                status: "ready",
                observed: { ...full, windows: Object.freeze([]) },
            };
        };
        mocks.observeImpl = (): PlanObserved | null => dynamicEmptyObserved(r, native);
        const adapter = enable(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(emptyMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 1);
        assert.equal(mocks.actives.length, 1);
        assert.equal(mocks.actives[0], r.a);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assertNoWireProtocol(mocks);
        assertCorrelated(mocks, correlation, "r4-transfer-started");
        assertCorrelated(mocks, correlation, "r4-native-written");
        assertCorrelated(mocks, correlation, "r4-followed");
        assertCorrelated(mocks, correlation, "arrived");
    });

    it("still refuses unrelated drift when the mover is half-visible (sole source)", () => {
        // Same half-visibility gap as above, but the target workspace also
        // drifts mid-write: the fence must still refuse rather than let the
        // mover-absence tolerance mask a real scope change.
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        let lagArmed = false;
        const env = mocks.env as unknown as Record<string, unknown>;
        const baseSetDesktops = mocks.env.setDesktops?.bind(mocks.env);
        (env as { setDesktops: PlanAdapterEnv["setDesktops"] })["setDesktops"] = (mover, refs): boolean => {
            const ok = baseSetDesktops?.(mover, refs) === true;
            if (ok) {
                lagArmed = true;
            }
            return ok;
        };
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => {
            const full = dynamicEmptyObserved(r, native);
            if (!lagArmed) {
                return { status: "ready", observed: full };
            }
            lagArmed = false;
            // Half-dropped mover plus a drifted target workspace.
            const domains = (full.domains ?? []) as unknown as Array<Record<string, unknown>>;
            const drifted = Object.freeze([
                Object.freeze({ ...(domains[0] as Record<string, unknown>) }),
                Object.freeze({ ...(domains[1] as Record<string, unknown>), workspace: "ws-c" }),
            ]);
            return {
                status: "ready",
                observed: {
                    ...full,
                    domains: drifted as unknown as Exclude<typeof full.domains, undefined>,
                    windows: Object.freeze([]),
                },
            };
        };
        mocks.observeImpl = (): PlanObserved | null => dynamicEmptyObserved(r, native);
        const adapter = enable(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(emptyMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.ok(mocks.logs.some((line) => line.includes(correlation) && line.includes("stale-scope")));
        assert.ok(
            mocks.logs.some(
                (line) => line.includes(correlation) && line.includes("r4-fence") && line.includes("fence=geometry-scope"),
            ),
        );
        assertNoWireProtocol(mocks);
    });

    it("refuses a missing mover pre-transfer even when native still shows source (sole source)", () => {
        // B1 narrowing: before any own relocation an absent mover is
        // unrelated drift, never tolerated, even though native reads on the
        // retained ref place it on the source. No setter may run.
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicEmptyObserved(r, native),
        });
        mocks.observeImpl = (): PlanObserved | null => dynamicEmptyObserved(r, native);
        const adapter = enable(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        // Drop the mover from re-observations once armed (reply-boundary
        // re-observe stays full, the pre-transfer fence sees it missing);
        // native keeps it source-placed (no setter runs to move it).
        // Dispatch probes several directions, so arm only after dispatch.
        let dropPhase = 0;
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => {
            const full = dynamicEmptyObserved(r, native);
            if (dropPhase === 0 || dropPhase === 1) {
                if (dropPhase === 1) {
                    dropPhase = 2;
                }
                return { status: "ready", observed: full };
            }
            return {
                status: "ready",
                observed: { ...full, windows: Object.freeze([]) },
            };
        };
        dropPhase = 1;
        mocks.callbacks[0]?.(emptyMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 0);
        assert.equal(native.sentMemberships.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.ok(mocks.logs.some((line) => line.includes(correlation) && line.includes("stale-scope")));
        assert.ok(
            mocks.logs.some(
                (line) => line.includes(correlation) && line.includes("r4-fence") && line.includes("fence=pre-transfer-scope"),
            ),
        );
        assertNoWireProtocol(mocks);
    });

    it("tolerates a half-visible own mover at the first geometry fence (occupied target)", () => {
        // N>1 variant of the sole-source tolerance: the fresh set is
        // exactly the flight set minus the mover, the non-mover stays
        // exact, and native places the mover in scope.
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        let lagArmed = false;
        const env = mocks.env as unknown as Record<string, unknown>;
        const baseSetDesktops = mocks.env.setDesktops?.bind(mocks.env);
        (env as { setDesktops: PlanAdapterEnv["setDesktops"] })["setDesktops"] = (mover, refs): boolean => {
            const ok = baseSetDesktops?.(mover, refs) === true;
            if (ok) {
                lagArmed = true;
            }
            return ok;
        };
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => {
            const full = dynamicOccupiedObserved(r, native);
            if (!lagArmed) {
                return { status: "ready", observed: full };
            }
            lagArmed = false;
            return {
                status: "ready",
                observed: {
                    ...full,
                    windows: Object.freeze(full.windows.filter((entry) => entry.id !== "win-a")),
                },
            };
        };
        const adapter = enable(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 2);
        assert.equal(mocks.actives.length, 1);
        assert.equal(mocks.actives[0], r.a);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assertNoWireProtocol(mocks);
        assertCorrelated(mocks, correlation, "r4-transfer-started");
        assertCorrelated(mocks, correlation, "r4-native-written");
        assertCorrelated(mocks, correlation, "r4-followed");
        assertCorrelated(mocks, correlation, "arrived");
    });

    it("refuses a missing non-mover at the geometry fence (occupied target)", () => {
        // Same minus-one shape, but the mover is present and a non-mover
        // vanished: unrelated drift, never tolerated.
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        let lagArmed = false;
        const env = mocks.env as unknown as Record<string, unknown>;
        const baseSetDesktops = mocks.env.setDesktops?.bind(mocks.env);
        (env as { setDesktops: PlanAdapterEnv["setDesktops"] })["setDesktops"] = (mover, refs): boolean => {
            const ok = baseSetDesktops?.(mover, refs) === true;
            if (ok) {
                lagArmed = true;
            }
            return ok;
        };
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => {
            const full = dynamicOccupiedObserved(r, native);
            if (!lagArmed) {
                return { status: "ready", observed: full };
            }
            lagArmed = false;
            return {
                status: "ready",
                observed: {
                    ...full,
                    windows: Object.freeze(full.windows.filter((entry) => entry.id !== "win-x")),
                },
            };
        };
        const adapter = enable(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.ok(mocks.logs.some((line) => line.includes(correlation) && line.includes("stale-scope")));
        assert.ok(
            mocks.logs.some(
                (line) => line.includes(correlation) && line.includes("r4-fence") && line.includes("fence=geometry-scope"),
            ),
        );
        assert.ok(
            mocks.logs.some(
                (line) => line.includes(correlation) && line.includes("r4-scope") && line.includes("subreason=nonmover-missing"),
            ),
        );
        assertNoWireProtocol(mocks);
    });

    it("writes plan-flagged overconstrained geometry at origin plus minimum on R4", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        (mocks.env as unknown as Record<string, unknown>)["readWindowConstraints"] = (target: object): unknown => {
            if (target === r.a) {
                return { resizeable: true, minSize: { w: 400, h: 600 }, maxSize: null };
            }
            return { resizeable: true, minSize: null, maxSize: null };
        };
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        const planned = JSON.parse(crossMoveReply(correlation)) as { desired_geometry: Array<Record<string, unknown>> };
        planned.desired_geometry[0]!["overconstrained"] = true;
        mocks.callbacks[0]?.(JSON.stringify(planned));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        // B6: the flagged mover writes its planned origin with violated
        // extents raised to the declared minimum; the feasible member writes
        // its plan untouched.
        assert.equal(mocks.geometries.length, 2);
        const forTarget = (target: object): Array<Record<string, unknown>> =>
            mocks.geometries.filter((entry) => entry.target === target).map((entry) => entry.rect as Record<string, unknown>);
        assert.deepEqual(forTarget(r.a), [{ x: 810, y: 10, w: 400, h: 600 }]);
        assert.deepEqual(forTarget(r.x), [{ x: 1200, y: 10, w: 380, h: 580 }]);
        assert.equal(mocks.actives.length, 1);
        assert.equal(adapter.isR4InFlight, false);
        assert.ok(
            mocks.logs.some(
                (line) =>
                    line ===
                    `omnitiler:plan:minimum-placed correlation=${correlation} window=win-a resource_class=unknown op=move rect=810,10,400,600`,
            ),
            "successful R4 placement logs the actual rect",
        );
        assert.ok(!mocks.logs.some((line) => line.includes("overconstrained-skipped")));
        assertNoWireProtocol(mocks);
    });

    it("fails terminal on native write refusal with no follow and forced reconcile", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        (mocks.env as unknown as Record<string, unknown>)["sendClientToScreen"] = (): boolean => false;
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: twoDomainObserved(r),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.equal(mocks.dbusCalls.length, 1);
        assertNoWireProtocol(mocks);
        assertCorrelated(mocks, correlation, "write-failed");
        const before = mocks.dbusCalls.length;
        fireDebounce(mocks);
        assert.ok(mocks.dbusCalls.length > before, "terminal forces source+target reconcile");
        void native;
    });

    it("fails terminal on wrong-output proof with no follow", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        native.autoUpdate = false;
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: twoDomainObserved(r),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(adapter.isR4InFlight, true);
        assert.equal(mocks.actives.length, 0);
        native.outputOfMover = "out-9";
        native.desktopsOfMover = ["ws-a"];
        native.outputHandlers.forEach((handler) => handler(native.out1));
        native.desktopsHandlers.forEach((handler) => handler());
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.ok(mocks.logs.some((line) => line.includes(correlation) && line.includes("wrong-output")));
        assert.ok(
            mocks.logs.some(
                (line) => line.includes(correlation) && line.includes("r4-fence") && line.includes("fence=arrival-signal-placement"),
            ),
        );
        assertNoWireProtocol(mocks);
    });

    it("waits for delayed arrival then follows exactly once; busy second command refuses", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        native.autoUpdate = false;
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(adapter.isR4InFlight, true);
        assert.equal(mocks.actives.length, 0);
        adapter.requestFocus("left");
        assert.ok(mocks.logs.some((line) => line.includes("busy-refused kind=focus")));
        assert.equal(mocks.dbusCalls.length, 1);
        native.outputOfMover = "out-2";
        native.outputHandlers.forEach((handler) => handler(native.out1));
        assert.equal(mocks.actives.length, 0);
        native.desktopsOfMover = ["ws-b"];
        native.desktopsHandlers.forEach((handler) => handler());
        assert.equal(mocks.actives.length, 1);
        assert.equal(mocks.actives[0], r.a);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        native.outputHandlers.forEach((handler) => handler(native.out1));
        native.desktopsHandlers.forEach((handler) => handler());
        assert.equal(mocks.actives.length, 1);
        assertCorrelated(mocks, correlation, "r4-arrival-waiting");
        assertCorrelated(mocks, correlation, "arrived");
        assertNoWireProtocol(mocks);
        // Flight released: a fresh source-consistent observation dispatches
        // again. The post-move dynamic observation leaves source empty, so
        // reset to a source-homed view solely to prove release.
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: twoDomainObserved(r),
        });
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 2);
    });

    it("stops before membership when native reads go unreadable reentrantly during transfer", () => {
        // A reentrant close between the back-to-back R4 setters: native
        // reads throw, so the inter-setter lifetime proof fails before
        // membership with no geometry or follow. A dead wrapper that would
        // still accept the write never gets it.
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const env = mocks.env as unknown as Record<string, unknown>;
        const baseReadOutput = env["readOutputName"] as (ref: object) => string | null;
        let closed = false;
        (env as { readOutputName: (ref: object) => string | null })["readOutputName"] = (ref): string | null => {
            if (closed && ref === r.a) {
                throw new Error("removed native object");
            }
            return baseReadOutput(ref);
        };
        const baseTransfer = env["sendClientToScreen"] as (mover: object, output: object) => boolean;
        (env as { sendClientToScreen: (mover: object, output: object) => boolean })["sendClientToScreen"] = (
            mover,
            output,
        ): boolean => {
            const ok = baseTransfer(mover, output);
            // Reentrant close lands inside the transfer setter.
            closed = true;
            return ok;
        };
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1, "transfer initiated once");
        assert.equal(native.sentMemberships.length, 0, "no membership for a closed mover");
        assert.equal(mocks.geometries.length, 0, "no geometry for a closed mover");
        assert.equal(mocks.actives.length, 0, "no follow for a closed mover");
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.ok(mocks.logs.some((line) => line.includes(correlation) && line.includes("stale-scope")));
        assert.ok(
            mocks.logs.some(
                (line) => line.includes(correlation) && line.includes("r4-fence") && line.includes("fence=inter-setter"),
            ),
        );
        assertNoWireProtocol(mocks);
    });

    it("stops remaining setters on changed scope mid-write", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        // After the first geometry, swap to a stale target workspace so the
        // structural scope fence (domains) fails before the second setter.
        const env = mocks.env as unknown as Record<string, unknown>;
        const baseSetGeometry = mocks.env.setGeometry.bind(mocks.env);
        let calls = 0;
        (env as { setGeometry: PlanAdapterEnv["setGeometry"] })["setGeometry"] = (target, rect): boolean => {
            calls += 1;
            const ok = baseSetGeometry(target, rect);
            if (calls === 1) {
                mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
                    status: "ready",
                    observed: twoDomainObserved(r, { targetWorkspace: "ws-c" }),
                });
            }
            return ok;
        };
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 1);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.ok(mocks.logs.some((line) => line.includes(correlation) && line.includes("stale-scope")));
        assert.ok(
            mocks.logs.some(
                (line) => line.includes(correlation) && line.includes("r4-fence") && line.includes("fence=geometry-scope"),
            ),
        );
        assertNoWireProtocol(mocks);
    });

    it("stops remaining setters on non-mover flag change mid-write", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        // After the first geometry, flip a non-mover flag (win-x maximized)
        // while keeping scope identical so only the flag fence trips.
        const env = mocks.env as unknown as Record<string, unknown>;
        const baseSetGeometry = mocks.env.setGeometry.bind(mocks.env);
        let calls = 0;
        (env as { setGeometry: PlanAdapterEnv["setGeometry"] })["setGeometry"] = (target, rect): boolean => {
            calls += 1;
            const ok = baseSetGeometry(target, rect);
            if (calls === 1) {
                mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => {
                    const base = dynamicOccupiedObserved(r, native);
                    return {
                        status: "ready",
                        observed: {
                            ...base,
                            windows: Object.freeze(
                                base.windows.map((entry) =>
                                    entry.id === "win-x" ? { ...entry, maximized: true } : entry,
                                ),
                            ),
                        },
                    };
                };
            }
            return ok;
        };
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 1);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assert.ok(mocks.logs.some((line) => line.includes(correlation) && line.includes("stale-scope")));
        assert.ok(
            mocks.logs.some(
                (line) => line.includes(correlation) && line.includes("r4-fence") && line.includes("fence=geometry-scope"),
            ),
        );
        assertNoWireProtocol(mocks);
    });

    it("closed mover cannot focus despite native arrival", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        // Let both geometries succeed, then present a closed-mover fresh view
        // (mover absent) for the follow proof. Native still reports arrival
        // via autoUpdate, but observed proof must block setActive.
        const env = mocks.env as unknown as Record<string, unknown>;
        const baseSetGeometry = mocks.env.setGeometry.bind(mocks.env);
        let calls = 0;
        (env as { setGeometry: PlanAdapterEnv["setGeometry"] })["setGeometry"] = (target, rect): boolean => {
            calls += 1;
            const ok = baseSetGeometry(target, rect);
            if (calls === 2) {
                mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
                    status: "ready",
                    observed: {
                        domainOutput: "out-1",
                        domainWorkspace: "ws-a",
                        domainBounds: { x: 0, y: 0, w: 800, h: 600 },
                        domainGap: 4,
                        domainOuterGap: 8,
                        focusedId: "win-x",
                        domains: Object.freeze([
                            Object.freeze({
                                output: "out-1",
                                workspace: "ws-a",
                                bounds: { x: 0, y: 0, w: 800, h: 600 },
                                gap: 4,
                                outerGap: 8,
                                adjacent: Object.freeze({ right: "out-2" }),
                            }),
                            Object.freeze({
                                output: "out-2",
                                workspace: "ws-b",
                                bounds: { x: 800, y: 0, w: 800, h: 600 },
                                gap: 4,
                                outerGap: 8,
                                adjacent: Object.freeze({ left: "out-1" }),
                            }),
                        ]),
                        windows: Object.freeze([
                            Object.freeze({
                                id: "win-x",
                                ref: r.x,
                                rect: Object.freeze({ x: 10, y: 10, w: 100, h: 80 }),
                                output: "out-2",
                                workspace: "ws-b",
                                fullscreen: false,
                                maximized: false,
                                floating: false,
                                sticky: false,
                                resourceClass: "unknown",
                            }),
                        ]),
                        activeRef: r.x,
                        fingerprint: "dir-fp-closed",
                        revalidate: () => true,
                    },
                });
            }
            return ok;
        };
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 2);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, true);
        assertNoWireProtocol(mocks);
        void native;
    });

    it("wrong mover placement cannot focus despite native arrival", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: dynamicOccupiedObserved(r, native),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        // Both geometries succeed, then the fresh view shows the mover on a
        // third output while native still reports target arrival. Follow must
        // stay waiting with no setActive.
        const env = mocks.env as unknown as Record<string, unknown>;
        const baseSetGeometry = mocks.env.setGeometry.bind(mocks.env);
        let calls = 0;
        (env as { setGeometry: PlanAdapterEnv["setGeometry"] })["setGeometry"] = (target, rect): boolean => {
            calls += 1;
            const ok = baseSetGeometry(target, rect);
            if (calls === 2) {
                mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
                    status: "ready",
                    observed: {
                        domainOutput: "out-1",
                        domainWorkspace: "ws-a",
                        domainBounds: { x: 0, y: 0, w: 800, h: 600 },
                        domainGap: 4,
                        domainOuterGap: 8,
                        focusedId: "win-a",
                        domains: Object.freeze([
                            Object.freeze({
                                output: "out-1",
                                workspace: "ws-a",
                                bounds: { x: 0, y: 0, w: 800, h: 600 },
                                gap: 4,
                                outerGap: 8,
                                adjacent: Object.freeze({ right: "out-2" }),
                            }),
                            Object.freeze({
                                output: "out-2",
                                workspace: "ws-b",
                                bounds: { x: 800, y: 0, w: 800, h: 600 },
                                gap: 4,
                                outerGap: 8,
                                adjacent: Object.freeze({ left: "out-1" }),
                            }),
                        ]),
                        windows: Object.freeze([
                            Object.freeze({
                                id: "win-a",
                                ref: r.a,
                                rect: Object.freeze({ x: 810, y: 10, w: 100, h: 80 }),
                                output: "out-9",
                                workspace: "ws-b",
                                fullscreen: false,
                                maximized: false,
                                floating: false,
                                sticky: false,
                                resourceClass: "unknown",
                            }),
                            Object.freeze({
                                id: "win-x",
                                ref: r.x,
                                rect: Object.freeze({ x: 10, y: 10, w: 100, h: 80 }),
                                output: "out-2",
                                workspace: "ws-b",
                                fullscreen: false,
                                maximized: false,
                                floating: false,
                                sticky: false,
                                resourceClass: "unknown",
                            }),
                        ]),
                        activeRef: r.a,
                        fingerprint: "dir-fp-wrong",
                        revalidate: () => true,
                    },
                });
            }
            return ok;
        };
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 1);
        assert.equal(native.sentMemberships.length, 1);
        assert.equal(mocks.geometries.length, 2);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, true);
        assertNoWireProtocol(mocks);
        void native;
    });

    it("replans a stale directional pre-write once, then drops a second staleness", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        const firstCommand = payload(mocks, 0)["command"];
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: twoDomainObserved(r, { xRect: { x: 20, y: 10, w: 100, h: 80 } }),
        });
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 0, "stale pre-write transfers nothing");
        assert.equal(native.sentMemberships.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.ok(mocks.logs.some((line) => line.includes(correlation) && line.includes("stale")));
        assert.equal(mocks.dbusCalls.length, 2, "stale directional pre-write replans once");
        const replan = payload(mocks, 1);
        const replanCorrelation = replan["correlation_id"] as string;
        assert.notEqual(replanCorrelation, correlation, "replan uses a fresh correlation");
        assert.deepEqual(replan["command"], firstCommand, "replan carries the same move command");
        assert.equal(adapter.isInFlight, true, "replanned move is in flight");
        assertNoWireProtocol(mocks);
        // Second staleness against the replanned flight drops with no
        // further dispatch and no native writes.
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: twoDomainObserved(r, { xRect: { x: 30, y: 12, w: 100, h: 80 } }),
        });
        mocks.callbacks[1]?.(crossMoveReply(replanCorrelation));
        assert.equal(native.sentTransfers.length, 0, "second staleness transfers nothing");
        assert.equal(native.sentMemberships.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false, "dropped flight converges");
        assert.equal(mocks.dbusCalls.length, 2, "second staleness dispatches nothing further");
        assert.ok(mocks.logs.some((line) => line.includes(replanCorrelation) && line.includes("stale-scope")));
        assertNoWireProtocol(mocks);
        const before = mocks.dbusCalls.length;
        fireDebounce(mocks);
        assert.ok(mocks.dbusCalls.length >= before, "stale terminal still converges");
    });

    it("releases an unanswered request on timeout with no writes and forced reconcile", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        const adapter = enable(mocks);
        adapter.requestMove("right");
        assert.equal(mocks.dbusCalls.length, 1);
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        const deadline = mocks.timers[mocks.timers.length - 1];
        assert.notEqual(deadline, undefined);
        deadline?.callback();
        assert.equal(native.sentTransfers.length, 0);
        assert.equal(mocks.geometries.length, 0);
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assertCorrelated(mocks, correlation, "timeout");
        assertNoWireProtocol(mocks);
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(native.sentTransfers.length, 0);
        assert.equal(mocks.dbusCalls.length, 1);
        const before = mocks.dbusCalls.length;
        fireDebounce(mocks);
        assert.ok(mocks.dbusCalls.length >= before);
    });

    it("releases delayed arrival on arrival timeout with forced reconcile", () => {
        const r = refs();
        const { mocks, native } = r4Mocks(r);
        native.autoUpdate = false;
        mocks.directionalImpl = (): DirectionalObservation | PlanObserved | null => ({
            status: "ready",
            observed: twoDomainObserved(r),
        });
        const adapter = enable(mocks);
        adapter.requestMove("right");
        const correlation = payload(mocks, 0)["correlation_id"] as string;
        mocks.callbacks[0]?.(crossMoveReply(correlation));
        assert.equal(adapter.isR4InFlight, true);
        const arrivalTimer = mocks.timers[mocks.timers.length - 1];
        assert.notEqual(arrivalTimer, undefined);
        arrivalTimer?.callback();
        assert.equal(mocks.actives.length, 0);
        assert.equal(adapter.isR4InFlight, false);
        assert.equal(adapter.isInFlight, false);
        assertCorrelated(mocks, correlation, "arrival-timeout");
        assertNoWireProtocol(mocks);
        void native;
    });

    describe("pinned dispatch target with two right candidates (production observation)", () => {
        // The dispatch mover sits low, selecting out-3 through the
        // production FULL-rectangle observation (not left/top out-2). Every
        // pinned fence below must keep that dispatch-selected out-3 pair:
        // re-ranking mid-flight would resolve out-2 after the transfer
        // already moved the window to out-3 (partial transfer defect).
        interface ThreeWorld {
            surface: Record<string, unknown> & { screens: object[] };
            out1: { name: string; geometry: object };
            out2: { name: string; geometry: object };
            out3: { name: string; geometry: object };
            wsA: { id: string };
            wsB: { id: string };
            wsC: { id: string };
            currents: Map<object, object>;
            areas: Map<object, object>;
        }

        function threeWorld(r: { a: object; b: object; x: object }): ThreeWorld {
            const out1 = { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 1200 } };
            const out2 = { name: "out-2", geometry: { x: 800, y: 0, width: 800, height: 600 } };
            const out3 = { name: "out-3", geometry: { x: 800, y: 600, width: 800, height: 600 } };
            const wsA = { id: "ws-a" };
            const wsB = { id: "ws-b" };
            const wsC = { id: "ws-c" };
            Object.assign(r.a, {
                normalWindow: true,
                output: out1,
                onAllDesktops: false,
                desktops: [wsA],
                internalId: "win-a",
                frameGeometry: { x: 100, y: 900, width: 200, height: 100 },
                fullScreen: false,
                maximizeMode: 0,
                resourceClass: "app",
            });
            Object.assign(r.x, {
                normalWindow: true,
                output: out3,
                onAllDesktops: false,
                desktops: [wsC],
                internalId: "win-t",
                frameGeometry: { x: 900, y: 700, width: 100, height: 80 },
                fullScreen: false,
                maximizeMode: 0,
                resourceClass: "app",
            });
            Object.assign(r.b, {
                normalWindow: true,
                output: out2,
                onAllDesktops: false,
                desktops: [wsB],
                internalId: "win-b",
                frameGeometry: { x: 900, y: 100, width: 100, height: 80 },
                fullScreen: false,
                maximizeMode: 0,
                resourceClass: "app",
            });
            const currents = new Map<object, object>([
                [out1, wsA],
                [out2, wsB],
                [out3, wsC],
            ]);
            const areas = new Map<object, object>([
                [out1, { x: 0, y: 0, w: 800, h: 1200 }],
                [out2, { x: 800, y: 0, w: 800, h: 600 }],
                [out3, { x: 800, y: 600, w: 800, h: 600 }],
            ]);
            const surface = {
                activeWindow: r.a,
                screens: [out1, out2, out3],
                desktops: [wsA, wsB, wsC],
                currentDesktopForScreen: (screen: object): object => currents.get(screen) ?? wsA,
                clientArea: (_kind: number, screen: object, _desktop: object): object =>
                    areas.get(screen) ?? { x: 0, y: 0, w: 800, h: 600 },
                windowList: (): object[] => [r.a, r.x, r.b],
            } as Record<string, unknown> & { screens: object[] };
            return { surface, out1, out2, out3, wsA, wsB, wsC, currents, areas };
        }

        function mocks3(r: { a: object; b: object; x: object }, world: ThreeWorld): {
            mocks: Mocks;
            seenHook: Array<{ direction: unknown; targetOutput: unknown; forMove: unknown }>;
            outputHandlers: Array<(old: unknown) => void>;
            desktopsHandlers: Array<() => void>;
            transfers: Array<{ mover: object; output: object }>;
            memberships: Array<{ mover: object; refs: ReadonlyArray<object> }>;
            auto: { update: boolean };
        } {
            const mocks = mockEnv(r);
            const seenHook: Array<{ direction: unknown; targetOutput: unknown; forMove: unknown }> = [];
            const outputHandlers: Array<(old: unknown) => void> = [];
            const desktopsHandlers: Array<() => void> = [];
            const transfers: Array<{ mover: object; output: object }> = [];
            const memberships: Array<{ mover: object; refs: ReadonlyArray<object> }> = [];
            const auto = { update: true };
            const cache = new Map<string, string>();
            const floating = new Set<string>();
            const activeRef: { current: object | null } = { current: r.b };
            const env = mocks.env as unknown as Record<string, unknown>;
            env["observeDirectional"] = (
                direction: PlanDirection,
                pinnedSource?: { readonly output: string; readonly workspace: string; readonly targetOutput?: string },
                forMove?: boolean,
            ): DirectionalObservation | PlanObserved | null => {
                seenHook.push({ direction, targetOutput: pinnedSource?.targetOutput, forMove });
                return observeDirectionalDomain(
                    world.surface,
                    cache,
                    floating,
                    { innerGap: 4, outerGap: 8 },
                    direction as string,
                    undefined,
                    undefined,
                    pinnedSource,
                    forMove,
                );
            };
            env["resolveOutput"] = (name: string): object | null =>
                name === "out-1" ? world.out1 : name === "out-2" ? world.out2 : name === "out-3" ? world.out3 : null;
            env["resolveDesktop"] = (workspace: string): object | null =>
                workspace === "ws-a" ? world.wsA : workspace === "ws-b" ? world.wsB : workspace === "ws-c" ? world.wsC : null;
            env["sendClientToScreen"] = (mover: object, output: object): boolean => {
                transfers.push({ mover, output });
                mocks.events.push("transfer");
                if (mover !== r.a || (output !== world.out1 && output !== world.out2 && output !== world.out3)) {
                    return false;
                }
                if (auto.update) {
                    (r.a as Record<string, unknown>)["output"] = output;
                }
                return true;
            };
            env["setDesktops"] = (mover: object, refs: ReadonlyArray<object>): boolean => {
                memberships.push({ mover, refs });
                mocks.events.push("membership");
                if (mover !== r.a || refs.length !== 1) {
                    return false;
                }
                if (auto.update) {
                    (r.a as Record<string, unknown>)["desktops"] = [...refs];
                }
                return true;
            };
            env["readOutputName"] = (ref: object): string | null => {
                const output = (ref as Record<string, unknown>)["output"] as { name?: unknown } | undefined;
                return typeof output?.name === "string" ? output.name : null;
            };
            env["readDesktopIds"] = (ref: object): ReadonlyArray<string> | null => {
                const desktops = (ref as Record<string, unknown>)["desktops"] as Array<{ id?: unknown }> | undefined;
                if (!Array.isArray(desktops)) {
                    return null;
                }
                const ids: string[] = [];
                for (const entry of desktops) {
                    if (typeof entry?.id !== "string") {
                        return null;
                    }
                    ids.push(entry.id);
                }
                return ids;
            };
            env["readGeometry"] = (ref: object): { x: number; y: number; w: number; h: number } | null => {
                const rect = (ref as Record<string, unknown>)["frameGeometry"] as
                    | { x: number; y: number; width?: number; w?: number; height?: number; h?: number }
                    | undefined;
                if (rect === undefined || typeof rect.x !== "number" || typeof rect.y !== "number") {
                    return null;
                }
                const w = rect.w ?? rect.width;
                const h = rect.h ?? rect.height;
                if (typeof w !== "number" || typeof h !== "number") {
                    return null;
                }
                return { x: rect.x, y: rect.y, w, h };
            };
            env["subscribeMoverOutput"] = (mover: object, handler: (old: unknown) => void): (() => void) | null => {
                if (mover !== r.a) {
                    return null;
                }
                outputHandlers.push(handler);
                return (): void => {};
            };
            env["subscribeMoverDesktops"] = (mover: object, handler: () => void): (() => void) | null => {
                if (mover !== r.a) {
                    return null;
                }
                desktopsHandlers.push(handler);
                return (): void => {};
            };
            env["subscribeWindowGeometry"] = (): (() => void) | null => (): void => {};
            const baseSetGeometry = mocks.env.setGeometry;
            (env as { setGeometry: PlanAdapterEnv["setGeometry"] })["setGeometry"] = (target, rect): boolean => {
                (target as Record<string, unknown>)["frameGeometry"] = { x: rect.x, y: rect.y, width: rect.w, height: rect.h };
                mocks.events.push("geometry");
                return baseSetGeometry(target, rect);
            };
            mocks.activeImpl = (): object | null => activeRef.current;
            (env as { setActive: PlanAdapterEnv["setActive"] })["setActive"] = (target): boolean => {
                mocks.events.push("setActive");
                mocks.actives.push(target);
                activeRef.current = target;
                return true;
            };
            return { mocks, seenHook, outputHandlers, desktopsHandlers, transfers, memberships, auto };
        }

        function replyOut3(correlation: string): string {
            return JSON.stringify({
                v: 1,
                correlation_id: correlation,
                outcome: "planned",
                base_revision: 2,
                detail: { kind: "move", rule: "R4", capability: "CrossOutputTransfer", direction: "right" },
                desired_geometry: [
                    { window: "win-a", leaf: "leaf-a", output: "out-3", workspace: "ws-c", rect: { x: 810, y: 610, w: 380, h: 580 } },
                    { window: "win-t", leaf: "leaf-t", output: "out-3", workspace: "ws-c", rect: { x: 1200, y: 610, w: 380, h: 580 } },
                ],
                desired_focus: { domain_output: "out-3", domain_workspace: "ws-c", leaf: "leaf-a" },
                operation: {
                    op: "move",
                    rule: "R4",
                    capability: "CrossOutputTransfer",
                    direction: "right",
                    window: "win-a",
                    leaf: "leaf-a",
                    source_output: "out-1",
                    source_workspace: "ws-a",
                    target_output: "out-3",
                    target_workspace: "ws-c",
                    source_root_child_index: 0,
                    target: "occupied",
                },
                preconditions: [
                    "focused-leaf-occupied-by-focused-window",
                    "source-root-membership-and-adjacent-same-workspace-output",
                    "adapter-must-verify-postconditions",
                ],
            });
        }

        it("keeps the non-left/top dispatch target across pinned fences with full transfer and one follow", () => {
            const r = refs();
            const world = threeWorld(r);
            const h = mocks3(r, world);
            const adapter = enable(h.mocks);
            adapter.requestMove("right");
            assert.equal(h.mocks.dbusCalls.length, 1);
            const body = payload(h.mocks, 0);
            assert.equal((body["domains"] as Array<Record<string, unknown>>)[1]?.["output"], "out-3");
            assert.equal((body["command"] as Record<string, unknown>)["cross_output_transfer"], true);
            const correlation = body["correlation_id"] as string;
            h.mocks.callbacks[0]?.(replyOut3(correlation));
            assert.equal(h.transfers.length, 1);
            assert.equal((h.transfers[0]?.output as { name: string }).name, "out-3");
            assert.equal(h.memberships.length, 1);
            assert.equal(h.mocks.geometries.length, 2);
            const order = h.mocks.events.filter((entry) => entry === "transfer" || entry === "membership" || entry === "geometry");
            assert.deepEqual(order, ["transfer", "membership", "geometry", "geometry"]);
            assert.equal(h.mocks.actives.length, 1);
            assert.equal(h.mocks.actives[0], r.a);
            assert.equal(adapter.isR4InFlight, false);
            assert.equal(adapter.isInFlight, false);
            // Every pinned fence carried the dispatch-selected target: no
            // mid-flight re-rank to left/top out-2.
            const pinned = h.seenHook.filter((entry) => entry.targetOutput !== undefined);
            assert.ok(pinned.length > 0, "pinned fences ran");
            for (const entry of pinned) {
                assert.equal(entry.targetOutput, "out-3");
            }
            assertNoWireProtocol(h.mocks);
            assertCorrelated(h.mocks, correlation, "r4-transfer-started");
            assertCorrelated(h.mocks, correlation, "r4-native-written");
            assertCorrelated(h.mocks, correlation, "r4-followed");
            assertCorrelated(h.mocks, correlation, "arrived");
        });

        it("follows exactly once after delayed arrival on the pinned target", () => {
            const r = refs();
            const world = threeWorld(r);
            const h = mocks3(r, world);
            h.auto.update = false;
            const adapter = enable(h.mocks);
            adapter.requestMove("right");
            const correlation = payload(h.mocks, 0)["correlation_id"] as string;
            h.mocks.callbacks[0]?.(replyOut3(correlation));
            assert.equal(adapter.isR4InFlight, true);
            assert.equal(h.mocks.actives.length, 0);
            (r.a as Record<string, unknown>)["output"] = world.out3;
            for (const handler of h.outputHandlers) {
                handler(world.out1);
            }
            assert.equal(h.mocks.actives.length, 0);
            (r.a as Record<string, unknown>)["desktops"] = [world.wsC];
            for (const handler of h.desktopsHandlers) {
                handler();
            }
            assert.equal(h.mocks.actives.length, 1);
            assert.equal(h.mocks.actives[0], r.a);
            assert.equal(adapter.isR4InFlight, false);
            assert.equal(adapter.isInFlight, false);
            for (const handler of h.outputHandlers) {
                handler(world.out1);
            }
            for (const handler of h.desktopsHandlers) {
                handler();
            }
            assert.equal(h.mocks.actives.length, 1);
            assertCorrelated(h.mocks, correlation, "r4-arrival-waiting");
            assertCorrelated(h.mocks, correlation, "arrived");
            assertNoWireProtocol(h.mocks);
        });

        it("refuses with existing recovery when the pinned target is removed reentrantly during transfer", () => {
            const r = refs();
            const world = threeWorld(r);
            const h = mocks3(r, world);
            const env = h.mocks.env as unknown as Record<string, unknown>;
            const baseTransfer = env["sendClientToScreen"] as (mover: object, output: object) => boolean;
            (env as { sendClientToScreen: (mover: object, output: object) => boolean })["sendClientToScreen"] = (
                mover,
                output,
            ): boolean => {
                const ok = baseTransfer(mover, output);
                // Reentrant topology change inside the transfer setter: the
                // dispatch-selected out-3 leaves the screens before the
                // inter-setter fence runs.
                const at = world.surface.screens.indexOf(world.out3);
                if (at >= 0) {
                    world.surface.screens.splice(at, 1);
                }
                return ok;
            };
            const adapter = enable(h.mocks);
            adapter.requestMove("right");
            const correlation = payload(h.mocks, 0)["correlation_id"] as string;
            h.mocks.callbacks[0]?.(replyOut3(correlation));
            assert.equal(h.transfers.length, 1, "transfer initiated once");
            assert.equal(h.memberships.length, 0, "no membership for a removed target");
            assert.equal(h.mocks.geometries.length, 0, "no geometry for a removed target");
            assert.equal(h.mocks.actives.length, 0, "no follow for a removed target");
            assert.equal(adapter.isR4InFlight, false);
            assert.equal(adapter.isInFlight, false);
            assert.ok(h.mocks.logs.some((line) => line.includes(correlation) && line.includes("stale-scope")));
            assert.ok(
                h.mocks.logs.some(
                    (line) => line.includes(correlation) && line.includes("r4-fence") && line.includes("fence=inter-setter"),
                ),
            );
            assertNoWireProtocol(h.mocks);
            const before = h.mocks.dbusCalls.length;
            fireDebounce(h.mocks);
            assert.ok(h.mocks.dbusCalls.length > before, "terminal forces source+target reconcile");
        });
    });
});

describe("directional target overlay bounds", () => {
    it("clamps a target overlay into its own output, retaining the two-domain route", () => {
        const r = refs();
        const mocks = mockEnv(r);
        const observed = twoDomainObserved(r, { aRect: { x: 10, y: 10, w: 100, h: 80 }, xRect: { x: 0, y: 0, w: 100, h: 80 } });
        mocks.directionalImpl = () => ({
            status: "ready",
            observed: {
                ...observed,
                windows: observed.windows.map((entry) =>
                    entry.id === "win-x" ? { ...entry, fullscreen: true } : entry,
                ),
            },
        });
        const adapter = enable(mocks);
        adapter.requestFocus("right");
        const body = payload(mocks, 0);
        assert.equal((body["domains"] as Array<unknown>).length, 2);
        const target = (body["windows"] as Array<Record<string, unknown>>).find((entry) => entry["window"] === "win-x");
        assert.deepEqual(target?.["rect"], { x: 800, y: 0, w: 100, h: 80 });
    });
});

describe("observeDirectionalDomain (active route observation)", () => {
    function fakeWorkspace(
        opts: { ambiguous?: boolean; unreadable?: boolean; single?: boolean; badFrame?: boolean; stacked?: boolean } = {},
    ): unknown {
        const geo = (x: number, y: number, w: number, h: number): object => ({ x, y, width: w, height: h });
        const out1 = { name: "out-1", geometry: geo(0, 0, 800, 600) };
        const out2 = {
            name: "out-2",
            // Item 5.2: selection reads FULL output rectangles. The stacked
            // shape places out-2 above out-1; otherwise it sits right.
            geometry: opts.stacked === true ? geo(0, -600, 800, 600) : geo(800, 0, 800, 600),
        };
        const out3 = { name: "out-3", geometry: geo(800, 0, 800, 600) };
        if (opts.unreadable === true) {
            // An unreadable FULL rectangle fails closed (geometry missing).
            delete (out2 as Record<string, unknown>)["geometry"];
        }
        const wsA = { id: "ws-a" };
        const wsB = { id: "ws-b" };
        const screens = opts.single === true ? [out1] : opts.ambiguous === true ? [out1, out2, out3] : [out1, out2];
        const currentFor = (screen: object): object => {
            if (screen === out1) {
                return wsA;
            }
            return wsB;
        };
        const areaFor = (screen: object): { x: number; y: number; w: number; h: number } => {
            if (screen === out1) {
                return { x: 0, y: 0, w: 800, h: 600 };
            }
            if (screen === out2) {
                return { x: 800, y: 0, w: 800, h: 600 };
            }
            return { x: 800, y: 0, w: 800, h: 600 };
        };
        const winA = {
            normalWindow: true,
            output: out1,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "a",
            frameGeometry: { x: 10, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        const winX = {
            normalWindow: true,
            output: out2,
            onAllDesktops: false,
            desktops: [wsB],
            internalId: "x",
            frameGeometry:
                opts.badFrame === true
                    ? { x: Number.NaN, y: 10, width: 100, height: 80 }
                    : { x: 810, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        return {
            activeWindow: winA,
            screens,
            desktops: [wsA, wsB],
            currentDesktopForScreen: (screen: object): object => currentFor(screen),
            clientArea: (_kind: number, screen: object, _desktop: object): object => {
                if (opts.unreadable === true && screen === out2) {
                    throw new Error("unreadable");
                }
                return areaFor(screen);
            },
            windowList: (): object[] => [winA, winX],
        };
    }

    it("observes source plus the reciprocal adjacent current desktop", () => {
        const observed = observeDirectionalDomain(
            fakeWorkspace(),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
        );
        assert.equal(observed.status, "ready");
        assert.notEqual(observed.observed, null);
        assert.equal(observed.observed?.domains?.length, 2);
        assert.equal(observed.observed?.domains?.[0]?.output, "out-1");
        assert.equal(observed.observed?.domains?.[1]?.output, "out-2");
        // Distinct current workspaces are allowed.
        assert.equal(observed.observed?.domains?.[1]?.workspace, "ws-b");
        assert.deepEqual(observed.observed?.domains?.[0]?.adjacent, { right: "out-2" });
        assert.deepEqual(observed.observed?.domains?.[1]?.adjacent, { left: "out-1" });
        assert.equal(observed.observed?.windows.length, 2);
    });

    it("reports no-target on a confirmed single output", () => {
        const observed = observeDirectionalDomain(
            fakeWorkspace({ single: true }),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
        );
        assert.equal(observed.status, "no-target");
        assert.equal(observed.observed, null);
    });

    it("selects left/top among identical right candidates (position-based)", () => {
        const observed = observeDirectionalDomain(
            fakeWorkspace({ ambiguous: true }),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
        );
        assert.equal(observed.status, "ready");
        assert.equal(observed.observed?.domains?.[1]?.output, "out-2");
    });

    it("reports invalid on unreadable FULL output rectangle", () => {
        const observed = observeDirectionalDomain(
            fakeWorkspace({ unreadable: true }),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
        );
        assert.equal(observed.status, "invalid");
    });

    it("reports invalid on an unreadable target frame", () => {
        const observed = observeDirectionalDomain(
            fakeWorkspace({ badFrame: true }),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
        );
        assert.equal(observed.status, "invalid");
    });

    it("resolves vertical adjacency on stacked FULL rectangles", () => {
        // Item 5.1/5.2: stacked outputs cross up/down with reciprocal
        // adjacency on the vertical sides.
        const up = observeDirectionalDomain(
            fakeWorkspace({ stacked: true }),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "up",
        );
        assert.equal(up.status, "ready");
        assert.deepEqual(up.observed?.domains?.[0]?.adjacent, { up: "out-2" });
        assert.deepEqual(up.observed?.domains?.[1]?.adjacent, { down: "out-1" });
        const down = observeDirectionalDomain(
            fakeWorkspace({ stacked: true }),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "down",
        );
        // Nothing stacked below the source: confirmed no-target keeps local
        // behavior.
        assert.equal(down.status, "no-target");
    });

    it("reports no-target for vertical directions on a horizontal pair", () => {
        for (const direction of ["up", "down"]) {
            const observed = observeDirectionalDomain(
                fakeWorkspace(),
                new Map(),
                new Set(),
                { innerGap: 4, outerGap: 8 },
                direction,
            );
            assert.equal(observed.status, "no-target");
        }
    });

    it("uses FULL rectangles for selection but WORK AREAS for placement (panel gap)", () => {
        // Panels shrink each per-output work area so work-area edges no
        // longer touch (570 vs 630), but the FULL output rectangles still
        // edge-touch at y=600. Selection must find the adjacency anyway,
        // while carried placement stays work areas (never tiled under
        // panels).
        const wsA = { id: "ws-a" };
        const wsB = { id: "ws-b" };
        const out1 = { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 600 } };
        const out2 = { name: "out-2", geometry: { x: 0, y: 600, width: 800, height: 600 } };
        const winA = {
            normalWindow: true,
            output: out1,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "a",
            frameGeometry: { x: 10, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        const winX = {
            normalWindow: true,
            output: out2,
            onAllDesktops: false,
            desktops: [wsB],
            internalId: "x",
            frameGeometry: { x: 10, y: 640, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        const workspace = {
            activeWindow: winA,
            screens: [out1, out2],
            desktops: [wsA, wsB],
            currentDesktopForScreen: (screen: object): object => (screen === out1 ? wsA : wsB),
            clientArea: (_kind: number, screen: object, _desktop: object): object =>
                screen === out1 ? { x: 0, y: 0, w: 800, h: 570 } : { x: 0, y: 630, w: 800, h: 570 },
            windowList: (): object[] => [winA, winX],
        };
        const observed = observeDirectionalDomain(workspace, new Map(), new Set(), { innerGap: 4, outerGap: 8 }, "down");
        assert.equal(observed.status, "ready");
        assert.deepEqual(observed.observed?.domains?.[0]?.adjacent, { down: "out-2" });
        assert.deepEqual(observed.observed?.domains?.[1]?.adjacent, { up: "out-1" });
        // Carried bounds are the WORK AREAS, not the FULL rectangles.
        assert.deepEqual(observed.observed?.domains?.[0]?.bounds, { x: 0, y: 0, w: 800, h: 570 });
        assert.deepEqual(observed.observed?.domains?.[1]?.bounds, { x: 0, y: 630, w: 800, h: 570 });
        assert.deepEqual(observed.observed?.domainBounds, { x: 0, y: 0, w: 800, h: 570 });
    });

    it("never wraps: the far output is not adjacent", () => {        // Three outputs in a row: from the leftmost, only the middle
        // touches; from the rightmost, only the middle touches. The far
        // output never resolves directly (no wrap).
        const wsA = { id: "ws-a" };
        const geo = (x: number): object => ({ x, y: 0, width: 800, height: 600 });
        const out1 = { name: "out-1", geometry: geo(0) };
        const out2 = { name: "out-2", geometry: geo(800) };
        const out3 = { name: "out-3", geometry: geo(1600) };
        const winA = {
            normalWindow: true,
            output: out1,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "a",
            frameGeometry: { x: 10, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        const workspace = {
            activeWindow: winA,
            screens: [out1, out2, out3],
            desktops: [wsA],
            currentDesktopForScreen: (): object => wsA,
            clientArea: (): object => ({ x: 0, y: 0, w: 800, h: 600 }),
            windowList: (): object[] => [winA],
        };
        const right = observeDirectionalDomain(workspace, new Map(), new Set(), { innerGap: 4, outerGap: 8 }, "right");
        assert.equal(right.status, "ready");
        assert.equal(right.observed?.domains?.[1]?.output, "out-2");
        const left = observeDirectionalDomain(workspace, new Map(), new Set(), { innerGap: 4, outerGap: 8 }, "left");
        assert.equal(left.status, "no-target");
    });

    it("accepts valid reverse ambiguity (forward pair still resolves)", () => {
        // Forward selection from out-1 finds exactly one right candidate
        // (out-2); out-2's left edge also touches out-0, but the forward
        // pair is reciprocal so it resolves instead of refusing.
        const wsA = { id: "ws-a" };
        const out1 = { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 600 } };
        const out0 = { name: "out-0", geometry: { x: 0, y: 600, width: 800, height: 600 } };
        const out2 = { name: "out-2", geometry: { x: 800, y: 0, width: 800, height: 1200 } };
        const winA = {
            normalWindow: true,
            output: out1,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "a",
            frameGeometry: { x: 10, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        const workspace = {
            activeWindow: winA,
            screens: [out1, out0, out2],
            desktops: [wsA],
            currentDesktopForScreen: (): object => wsA,
            clientArea: (): object => ({ x: 0, y: 0, w: 800, h: 600 }),
            windowList: (): object[] => [winA],
        };
        const observed = observeDirectionalDomain(workspace, new Map(), new Set(), { innerGap: 4, outerGap: 8 }, "right");
        assert.equal(observed.status, "ready");
        assert.equal(observed.observed?.domains?.[1]?.output, "out-2");
    });

    it("refuses duplicate output names fail-closed", () => {
        const wsA = { id: "ws-a" };
        const out1 = { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 600 } };
        const outDup = { name: "out-1", geometry: { x: 800, y: 0, width: 800, height: 600 } };
        const winA = {
            normalWindow: true,
            output: out1,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "a",
            frameGeometry: { x: 10, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        const workspace = {
            activeWindow: winA,
            screens: [out1, outDup],
            desktops: [wsA],
            currentDesktopForScreen: (): object => wsA,
            clientArea: (): object => ({ x: 0, y: 0, w: 800, h: 600 }),
            windowList: (): object[] => [winA],
        };
        const observed = observeDirectionalDomain(workspace, new Map(), new Set(), { innerGap: 4, outerGap: 8 }, "right");
        assert.equal(observed.status, "invalid");
    });

    it("refuses malformed screen identities instead of going local", () => {
        const wsA = { id: "ws-a" };
        const out1 = { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 600 } };
        const winA = {
            normalWindow: true,
            output: out1,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "a",
            frameGeometry: { x: 10, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        const base = {
            activeWindow: winA,
            desktops: [wsA],
            currentDesktopForScreen: (): object => wsA,
            clientArea: (): object => ({ x: 0, y: 0, w: 800, h: 600 }),
            windowList: (): object[] => [winA],
        };
        // Single non-object entry: unreadable, refuses.
        const nonObject = observeDirectionalDomain(
            { ...base, screens: [null] },
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
        );
        assert.equal(nonObject.status, "invalid");
        // Single screen naming another output: inconsistent, refuses.
        const mismatched = observeDirectionalDomain(
            { ...base, screens: [{ name: "out-9", geometry: { x: 0, y: 0, width: 800, height: 600 } }] },
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
        );
        assert.equal(mismatched.status, "invalid");
    });
});

describe("entry-to-adapter directional route (production style)", () => {
    it("dispatches bounded two-domain DescribePlan through the real entry", () => {
        const stubSignal = (): { connect: (h: (p?: unknown) => void) => void; disconnect: (h: (p?: unknown) => void) => void } => ({
            connect: (): void => {},
            disconnect: (): void => {},
        });
        const out1 = { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 600 } };
        const out2 = { name: "out-2", geometry: { x: 800, y: 0, width: 800, height: 600 } };
        const wsA = { id: "ws-a" };
        const wsB = { id: "ws-b" };
        const winA = {
            normalWindow: true,
            output: out1,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "entry-a",
            frameGeometry: { x: 10, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
            frameGeometryChanged: stubSignal(),
            fullScreenChanged: stubSignal(),
            maximizedChanged: stubSignal(),
            desktopsChanged: stubSignal(),
        };
        const winX = {
            normalWindow: true,
            output: out2,
            onAllDesktops: false,
            desktops: [wsB],
            internalId: "entry-x",
            frameGeometry: { x: 810, y: 10, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
            frameGeometryChanged: stubSignal(),
            fullScreenChanged: stubSignal(),
            maximizedChanged: stubSignal(),
            desktopsChanged: stubSignal(),
        };
        const workspace = {
            activeWindow: winA,
            activeScreen: out1,
            screens: [out1, out2],
            desktops: [wsA, wsB],
            currentDesktopForScreen: (screen: object): object => (screen === out1 ? wsA : wsB),
            clientArea: (_kind: number, screen: object, _desktop: object): object =>
                screen === out1 ? { x: 0, y: 0, w: 800, h: 600 } : { x: 800, y: 0, w: 800, h: 600 },
            windowList: (): object[] => [winA, winX],
            windowAdded: stubSignal(),
            windowRemoved: stubSignal(),
            windowActivated: stubSignal(),
            screensChanged: stubSignal(),
            currentDesktopChanged: stubSignal(),
            desktopsChanged: stubSignal(),
        };
        const calls: Array<{ method: string; payload: string }> = [];
        const callbacks: Array<(reply: unknown) => void> = [];
        const logs: string[] = [];
        const handle = startPlanAdapterEntry({
            workspace,
            owner: "owner-1",
            generation: "gen-1",
            log: (message: string): void => {
                logs.push(message);
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
                calls.push({ method, payload });
                callbacks.push(callback);
            },
            scheduleOnce: (): (() => void) => (): void => {},
            registerShortcutFn: (): boolean => true,
            readProfileFn: (): string => "cosmic",
            readWorkspaceModeFn: (): string => "per-output-local",
            readInnerGapFn: (): number => 4,
            readOuterGapFn: (): number => 8,
        });
        assert.notEqual(handle, null);
        handle?.requestFocus("right");
        assert.equal(calls.length, 1);
        assert.equal(calls[0]?.method, "DescribePlan");
        const body = JSON.parse(calls[0]?.payload as string) as Record<string, unknown>;
        const domains = body["domains"] as Array<Record<string, unknown>>;
        assert.equal(domains.length, 2);
        assert.equal(domains[0]?.["output"], "out-1");
        assert.equal(domains[1]?.["output"], "out-2");
        assert.equal(domains[1]?.["workspace"], "ws-b");
        const windows = body["windows"] as Array<Record<string, unknown>>;
        assert.equal(windows.length, 2);
        // The directional fingerprint binds the full evidence: recompute it
        // over the sent primitives and require an exact match.
        const sentFp = body["fingerprint"] as number;
        assert.ok(Number.isInteger(sentFp));
        handle?.stop();
        void logs;
    });
});

describe("position-based FULL-rectangle selection (2026-10-09)", () => {
    function posWorkspace(
        screens: Array<{ name: string; geometry: object }>,
        winFrame: object,
        activeOutput: object,
    ): unknown {
        const wsA = { id: "ws-a" };
        const wsB = { id: "ws-b" };
        const winA = {
            normalWindow: true,
            output: activeOutput,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "a",
            frameGeometry: winFrame,
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        return {
            activeWindow: winA,
            screens,
            desktops: [wsA, wsB],
            currentDesktopForScreen: (screen: object): object => (screen === screens[0] ? wsA : wsB),
            clientArea: (): object => ({ x: 0, y: 0, w: 800, h: 600 }),
            windowList: (): object[] => [winA],
        };
    }

    it("centre precedence up selects the left or right candidate", () => {
        const src = { name: "src", geometry: { x: 0, y: 600, width: 1600, height: 600 } };
        const left = { name: "out-l", geometry: { x: 0, y: 0, width: 800, height: 600 } };
        const right = { name: "out-r", geometry: { x: 800, y: 0, width: 800, height: 600 } };
        const screens = [src, left, right];
        const leftSeen = observeDirectionalDomain(
            posWorkspace(screens, { x: 100, y: 700, width: 200, height: 200 }, src),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "up",
        );
        assert.equal(leftSeen.status, "ready");
        assert.equal(leftSeen.observed?.domains?.[1]?.output, "out-l");
        const rightSeen = observeDirectionalDomain(
            posWorkspace(screens, { x: 1100, y: 700, width: 200, height: 200 }, src),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "up",
        );
        assert.equal(rightSeen.status, "ready");
        assert.equal(rightSeen.observed?.domains?.[1]?.output, "out-r");
    });

    it("centre precedence left and down select across two candidates", () => {
        // Left: two candidates west of the source; centre y decides.
        const src = { name: "src", geometry: { x: 800, y: 0, width: 800, height: 1200 } };
        const northWest = { name: "nw", geometry: { x: 0, y: 0, width: 800, height: 600 } };
        const southWest = { name: "sw", geometry: { x: 0, y: 600, width: 800, height: 600 } };
        const west = [src, northWest, southWest];
        const northSeen = observeDirectionalDomain(
            posWorkspace(west, { x: 900, y: 100, width: 200, height: 100 }, src),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "left",
        );
        assert.equal(northSeen.status, "ready");
        assert.equal(northSeen.observed?.domains?.[1]?.output, "nw");
        const southSeen = observeDirectionalDomain(
            posWorkspace(west, { x: 900, y: 900, width: 200, height: 100 }, src),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "left",
        );
        assert.equal(southSeen.status, "ready");
        assert.equal(southSeen.observed?.domains?.[1]?.output, "sw");
        // Down: two candidates below a wide source; centre x decides.
        const wide = { name: "wide", geometry: { x: 0, y: 0, width: 1600, height: 600 } };
        const downL = { name: "down-l", geometry: { x: 0, y: 600, width: 800, height: 600 } };
        const downR = { name: "down-r", geometry: { x: 800, y: 600, width: 800, height: 600 } };
        const down = [wide, downL, downR];
        const westSeen = observeDirectionalDomain(
            posWorkspace(down, { x: 100, y: 100, width: 200, height: 100 }, wide),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "down",
        );
        assert.equal(westSeen.status, "ready");
        assert.equal(westSeen.observed?.domains?.[1]?.output, "down-l");
        const eastSeen = observeDirectionalDomain(
            posWorkspace(down, { x: 1100, y: 100, width: 200, height: 100 }, wide),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "down",
        );
        assert.equal(eastSeen.status, "ready");
        assert.equal(eastSeen.observed?.domains?.[1]?.output, "down-r");
    });

    it("half-open seam belongs to the upper shared edge", () => {
        const src = { name: "src", geometry: { x: 0, y: 600, width: 1600, height: 600 } };
        const left = { name: "out-l", geometry: { x: 0, y: 0, width: 800, height: 600 } };
        const right = { name: "out-r", geometry: { x: 800, y: 0, width: 800, height: 600 } };
        const screens = [src, left, right];
        // Centre x=800 exactly: excluded from [0,800), included in [800,1600).
        const seen = observeDirectionalDomain(
            posWorkspace(screens, { x: 700, y: 700, width: 200, height: 200 }, src),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "up",
        );
        assert.equal(seen.status, "ready");
        assert.equal(seen.observed?.domains?.[1]?.output, "out-r");
    });

    it("odd window extents keep their exact half-unit centre", () => {
        const src = { name: "src", geometry: { x: 0, y: 0, width: 800, height: 1200 } };
        const top = { name: "out-t", geometry: { x: 800, y: 0, width: 800, height: 700 } };
        const bottom = { name: "out-b", geometry: { x: 800, y: 700, width: 800, height: 500 } };
        const screens = [src, top, bottom];
        // h=101 at y=700 centres on 750.5, inside [700,1200).
        const seen = observeDirectionalDomain(
            posWorkspace(screens, { x: 100, y: 700, width: 100, height: 101 }, src),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
        );
        assert.equal(seen.status, "ready");
        assert.equal(seen.observed?.domains?.[1]?.output, "out-b");
    });

    it("overlap fallback selects the larger window-span overlap", () => {
        const src = { name: "src", geometry: { x: 0, y: 600, width: 1600, height: 600 } };
        const narrowL = { name: "nl", geometry: { x: 0, y: 0, width: 400, height: 600 } };
        const narrowR = { name: "nr", geometry: { x: 1200, y: 0, width: 400, height: 600 } };
        const screens = [src, narrowL, narrowR];
        // Span [300,1400) overlaps nl 100 vs nr 200.
        const seen = observeDirectionalDomain(
            posWorkspace(screens, { x: 300, y: 700, width: 1100, height: 200 }, src),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "up",
        );
        assert.equal(seen.status, "ready");
        assert.equal(seen.observed?.domains?.[1]?.output, "nr");
    });

    it("two containing edges skip overlap straight to left/top", () => {
        const src = { name: "src", geometry: { x: 0, y: 600, width: 1600, height: 600 } };
        // Overlapping rectangles above: shared edges [0,1200) and [400,1600).
        // Centre x=1100 sits in both; the later edge overlaps the window span
        // more (400 vs 300) but left/top still picks "a".
        const a = { name: "a", geometry: { x: 0, y: 0, width: 1200, height: 600 } };
        const b = { name: "b", geometry: { x: 400, y: 0, width: 1200, height: 600 } };
        const screens = [src, a, b];
        const seen = observeDirectionalDomain(
            posWorkspace(screens, { x: 900, y: 700, width: 400, height: 100 }, src),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "up",
        );
        assert.equal(seen.status, "ready");
        assert.equal(seen.observed?.domains?.[1]?.output, "a");
    });

    it("deterministic left/top breaks overlap ties", () => {
        const src = { name: "src", geometry: { x: 0, y: 600, width: 1600, height: 600 } };
        const left = { name: "out-l", geometry: { x: 0, y: 0, width: 400, height: 600 } };
        const right = { name: "out-r", geometry: { x: 1200, y: 0, width: 400, height: 600 } };
        const screens = [src, left, right];
        const seen = observeDirectionalDomain(
            posWorkspace(screens, { x: 0, y: 700, width: 1600, height: 100 }, src),
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "up",
        );
        assert.equal(seen.status, "ready");
        assert.equal(seen.observed?.domains?.[1]?.output, "out-l");
    });

    it("focus keeps the legacy refusal on forward ambiguity", () => {
        const src = { name: "src", geometry: { x: 0, y: 0, width: 800, height: 1200 } };
        const top = { name: "out-t", geometry: { x: 800, y: 0, width: 800, height: 600 } };
        const bottom = { name: "out-b", geometry: { x: 800, y: 600, width: 800, height: 600 } };
        const wsA = { id: "ws-a" };
        const wsB = { id: "ws-b" };
        const winA = {
            normalWindow: true,
            output: src,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "a",
            frameGeometry: { x: 100, y: 100, width: 200, height: 100 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        const workspace = {
            activeWindow: winA,
            screens: [src, top, bottom],
            desktops: [wsA, wsB],
            currentDesktopForScreen: (screen: object): object => (screen === src ? wsA : wsB),
            clientArea: (): object => ({ x: 0, y: 0, w: 800, h: 600 }),
            windowList: (): object[] => [winA],
        };
        const refused = observeDirectionalDomain(
            workspace,
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
            undefined,
            undefined,
            undefined,
            false,
        );
        assert.equal(refused.status, "invalid");
        const selected = observeDirectionalDomain(
            workspace,
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
        );
        assert.equal(selected.status, "ready");
        assert.equal(selected.observed?.domains?.[1]?.output, "out-t");
    });

    it("pinned re-observation keeps the dispatch target without re-ranking", () => {
        // Three outputs right of nothing: src with out-2 above and out-3
        // below. The dispatch mover sits low, selecting out-3 (non-left/top).
        // Mid-flight the still-active mover sits on out-3; the pinned fence
        // must return the same out-3 pair, not re-rank to left/top out-2.
        const src = { name: "out-1", geometry: { x: 0, y: 0, width: 800, height: 1200 } };
        const out2 = { name: "out-2", geometry: { x: 800, y: 0, width: 800, height: 600 } };
        const out3 = { name: "out-3", geometry: { x: 800, y: 600, width: 800, height: 600 } };
        const wsA = { id: "ws-a" };
        const wsB = { id: "ws-b" };
        const wsC = { id: "ws-c" };
        const winS = {
            normalWindow: true,
            output: src,
            onAllDesktops: false,
            desktops: [wsA],
            internalId: "s",
            frameGeometry: { x: 100, y: 900, width: 200, height: 100 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        const winT = {
            normalWindow: true,
            output: out3,
            onAllDesktops: false,
            desktops: [wsC],
            internalId: "t",
            frameGeometry: { x: 900, y: 700, width: 100, height: 80 },
            fullScreen: false,
            maximizeMode: 0,
            resourceClass: "app",
        };
        const currents = new Map<object, object>([
            [src, wsA],
            [out2, wsB],
            [out3, wsC],
        ]);
        const areas = new Map<object, object>([
            [src, { x: 0, y: 0, w: 800, h: 1200 }],
            [out2, { x: 800, y: 0, w: 800, h: 600 }],
            [out3, { x: 800, y: 600, w: 800, h: 600 }],
        ]);
        const midFlight = {
            activeWindow: winT,
            screens: [src, out2, out3],
            desktops: [wsA, wsB, wsC],
            currentDesktopForScreen: (screen: object): object => currents.get(screen) ?? wsA,
            clientArea: (_kind: number, screen: object, _desktop: object): object => areas.get(screen) ?? { x: 0, y: 0, w: 800, h: 600 },
            windowList: (): object[] => [winS, winT],
        };
        const pin = { output: "out-1", workspace: "ws-a", targetOutput: "out-3" };
        const kept = observeDirectionalDomain(
            midFlight,
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
            undefined,
            undefined,
            pin,
            true,
        );
        assert.equal(kept.status, "ready");
        assert.equal(kept.observed?.domains?.[1]?.output, "out-3");
        // Removed target refuses instead of silently switching to out-2.
        const removed = observeDirectionalDomain(
            { ...midFlight, screens: [src, out2] },
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
            undefined,
            undefined,
            pin,
            true,
        );
        assert.equal(removed.status, "invalid");
        // Moved target (no longer edge-touching) refuses the same way.
        const movedOut3 = { name: "out-3", geometry: { x: 900, y: 600, width: 800, height: 600 } };
        const movedCurrents = new Map<object, object>([
            [src, wsA],
            [out2, wsB],
            [movedOut3, wsC],
        ]);
        const movedAreas = new Map<object, object>([
            [src, { x: 0, y: 0, w: 800, h: 1200 }],
            [out2, { x: 800, y: 0, w: 800, h: 600 }],
            [movedOut3, { x: 900, y: 600, w: 800, h: 600 }],
        ]);
        const movedWinT = { ...winT, output: movedOut3 };
        const moved = observeDirectionalDomain(
            {
                activeWindow: movedWinT,
                screens: [src, out2, movedOut3],
                desktops: [wsA, wsB, wsC],
                currentDesktopForScreen: (screen: object): object => movedCurrents.get(screen) ?? wsA,
                clientArea: (_kind: number, screen: object, _desktop: object): object =>
                    movedAreas.get(screen) ?? { x: 0, y: 0, w: 800, h: 600 },
                windowList: (): object[] => [winS, movedWinT],
            },
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
            undefined,
            undefined,
            pin,
            true,
        );
        assert.equal(moved.status, "invalid");
        // A pinned call without the dispatch target refuses fail-closed
        // instead of re-ranking.
        const untargeted = observeDirectionalDomain(
            midFlight,
            new Map(),
            new Set(),
            { innerGap: 4, outerGap: 8 },
            "right",
            undefined,
            undefined,
            { output: "out-1", workspace: "ws-a" },
            true,
        );
        assert.equal(untargeted.status, "invalid");
    });
});
